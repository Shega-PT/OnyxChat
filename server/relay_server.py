# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""relay_server.py — relay/TURN ciego (``docs/relay.md``).

O relay é um ponto de rendezvous para quando a ligação **direta** entre
dois hidden services falha: só vê bytes já cifrados pelo pipeline
K1→K9, não tem chaves e não participa no handshake de amizade.

Protocolo (TCP, enquadramento ``[u32 LE][tipo][corpo]``, idêntico ao
IPC local):

* ``0x01 SUBSCREVER`` ``mailbox(32)`` → ``0x83 OK`` + ``0x84 CAIXA``
  (entrega dos envelopes pendentes);
* ``0x02 ENVIAR`` ``mailbox(32)‖envelope(N)`` → ``0x83 OK`` e, se houver
  subscritor, ``0x84 CAIXA`` **push** para cada um; sem subscritor o
  envelope fica pendente até ao TTL (300 s);
* ``0x03 FECHO`` vazio → ``0x83 OK`` e a ligação fecha;
* ``0x81 ERRO`` ``código‖mensagem`` para qualquer violação.

Regras: máx. 64 envelopes pendentes por mailbox, 32 ``ENVIAR``/s por
ligação, TTL de 300 s. Tudo é biblioteca padrão (``asyncio``).

A lógica de estado está em :class:`Relay` (relógio injetável, testável
sem dormir nem abrir sockets); o transporte é o ``asyncio`` por baixo.
"""

from __future__ import annotations

import asyncio
import hashlib
import struct
import time
from typing import Any

#: Comprimento máximo do corpo de uma mensagem.
#:
#: Igual ao IPC, e lido da mesma forma: o valor vem de
#: ``ONYXCHAT_MAX_PAYLOAD`` quando definido, e cai no mínimo do
#: protocolo quando não está. Ver ``messenger/ipc_client.py`` para a
#: justificativa de não ser um literal — a divergência entre o cliente e
#: o daemon num valor escrito à mão é a classe de bug que ``ipc.rs``
#: documenta.
import os as _os

_MAX = int(_os.environ.get("ONYXCHAT_MAX_PAYLOAD", 0)) or 512 * 1024 * 1024
MAX_PAYLOAD = max(_MAX, 65_692 + 4 * 32)
#: Tamanho de uma mailbox (``SHA-256`` → 32 bytes).
TAM_MAILBOX = 32
#: Tamanho da pública Ed25519 usada para derivar a mailbox.
TAM_PUB = 32

#: Segundos que um envelope fica pendente sem subscritor.
TTL_ENVELOPE = 300.0
#: Máximo de envelopes pendentes por mailbox.
MAX_PENDENTES = 64
#: ``ENVIAR`` permitidos por janela de cada ligação.
MAX_ENVIOS = 32
#: Duração da janela de rate-limit (segundos).
JANELA_ENVIOS = 1.0

# ---------------------------------------------------------------------
# Tipos de mensagem (docs/relay.md §Mensagens)
# ---------------------------------------------------------------------
TIPO_SUBSCREVER = 0x01
TIPO_ENVIAR = 0x02
TIPO_FECHO = 0x03
TIPO_ERRO = 0x81
TIPO_OK = 0x83
TIPO_CAIXA = 0x84

# ---------------------------------------------------------------------
# Códigos de erro (docs/relay.md §Códigos de erro)
# ---------------------------------------------------------------------
ERR_PAYLOAD_MALFORMADO = 0x01
ERR_MAILBOX_INVALIDA = 0x02
ERR_PAYLOAD_GRANDE_DEMAIS = 0x03
ERR_SEM_ESPACO = 0x04
ERR_RATE_LIMIT = 0x05
ERR_COMANDO_DESCONHECIDO = 0x06

#: Nomes legíveis dos códigos de erro.
ERROS = {
    ERR_PAYLOAD_MALFORMADO: "PayloadMalformado",
    ERR_MAILBOX_INVALIDA: "MailboxInvalida",
    ERR_PAYLOAD_GRANDE_DEMAIS: "PayloadGrandeDemais",
    ERR_SEM_ESPACO: "SemEspaco",
    ERR_RATE_LIMIT: "RateLimit",
    ERR_COMANDO_DESCONHECIDO: "ComandoDesconhecido",
}

#: Prefixo da derivação da mailbox (``docs/relay.md`` §Identidade).
DOMINIO_MAILBOX = b"ONYX/RELAY/v1"


class RelayErro(Exception):
    """Violação do protocolo com código legível (``0x81 ERRO``)."""

    def __init__(self, codigo: int, mensagem: str = "") -> None:
        self.codigo = codigo
        self.mensagem = mensagem
        super().__init__(ERROS.get(codigo, f"Erro{codigo:#04x}"))

    def corpo(self) -> bytes:
        """``código: 1B ‖ «Nome: mensagem»`` — corpo de ``0x81 ERRO``."""
        return _erro(self.codigo, self.mensagem)


def _erro(codigo: int, mensagem: str) -> bytes:
    """Monta o corpo de uma mensagem ``0x81 ERRO``."""
    nome = ERROS.get(codigo, f"Erro{codigo:#04x}")
    texto = f"{nome}: {mensagem}" if mensagem else nome
    return bytes([codigo]) + texto.encode("utf-8")


def mailbox_de(pub: bytes) -> bytes:
    """Mailbox do destinatário: ``SHA-256("ONYX/RELAY/v1" ‖ pub)``.

    O hash é estável e não-reversível — quem não conhece a pública do
    destinatário não consegue depositar mensagens na caixa dele.
    """
    if len(pub) != TAM_PUB:
        raise RelayErro(ERR_MAILBOX_INVALIDA, "a pública tem de ter 32 bytes")
    return hashlib.sha256(DOMINIO_MAILBOX + pub).digest()


def enquadrar(tipo: int, corpo: bytes) -> bytes:
    """``[u32 LE do comprimento incl. tipo][tipo][corpo]``."""
    return struct.pack("<I", 1 + len(corpo)) + bytes([tipo]) + corpo


# ---------------------------------------------------------------------
# Núcleo — estado das caixas, subscritores e rate-limit
# ---------------------------------------------------------------------


class Relay:
    """Estado do relay: caixas pendentes, subscritores e janelas de taxa.

    ``relogio`` é injetável — os testes avançam o tempo sem dormir.
    As *ligações* são tokens opacos (hashable): o núcleo não conhece o
    ``asyncio``, só o transporte é que os escreve.
    """

    def __init__(self, relogio=time.monotonic) -> None:
        self._relogio = relogio
        # mailbox → [(prazo, envelope)] pendentes
        self._pendentes: dict[bytes, list[tuple[float, bytes]]] = {}
        # mailbox → ligações subscritas
        self._subscritores: dict[bytes, set[Any]] = {}
        # ligação → instantes dos ``ENVIAR`` da janela corrente
        self._envios: dict[Any, list[float]] = {}

    def subscrever(self, ligacao: Any, mailbox: bytes) -> list[tuple[Any, bytes]]:
        """Liga ``ligacao`` à ``mailbox`` e devolve o que estava pendente.

        Uma ligação só pode estar numa caixa de cada vez: subscrever a
        outra move-a (re-subscrição, ``docs/relay.md`` §Mensagens).
        """
        for conjunto in self._subscritores.values():
            conjunto.discard(ligacao)
        self._subscritores.setdefault(mailbox, set()).add(ligacao)
        caixa = self._pendentes.pop(mailbox, None)
        if caixa is None:
            return []
        return [(ligacao, envelope) for envelope in self._vivos(caixa)]

    def entregar(self, ligacao: Any, mailbox: bytes, envelope: bytes) -> list[tuple[Any, bytes]]:
        """Encaminha ``envelope`` aos subscritores ou guarda-o na caixa.

        Devolve os pares ``(destinatário, envelope)`` a escrever. Sem
        subscritor o envelope fica pendente até ao TTL; caixa cheia ou
        rate-limit excedido levantam :class:`RelayErro`.

        Invariante: só existe pendente em caixas **sem** subscritor
        (``subscrever`` drena a caixa), por isso o caminho com alvos não
        precisa de a ler.
        """
        self._consumir_taxa(ligacao)
        alvos = self._subscritores.get(mailbox)
        if alvos:
            return [(alvo, envelope) for alvo in alvos]
        caixa = self._pendentes.setdefault(mailbox, [])
        self._vivos(caixa)
        if len(caixa) >= MAX_PENDENTES:
            raise RelayErro(ERR_SEM_ESPACO, "caixa cheia")
        caixa.append((self._relogio() + TTL_ENVELOPE, envelope))
        return []

    def desligar(self, ligacao: Any) -> None:
        """Remove a ligação de todas as caixas e da janela de taxa."""
        for conjunto in self._subscritores.values():
            conjunto.discard(ligacao)
        self._envios.pop(ligacao, None)

    def _vivos(self, caixa: list[tuple[float, bytes]]) -> list[bytes]:
        """Descarta os caducados e devolve os envelopes ainda vivos."""
        agora = self._relogio()
        vivos = [(prazo, envelope) for prazo, envelope in caixa if prazo > agora]
        caixa[:] = vivos
        return [envelope for _, envelope in vivos]

    def _consumir_taxa(self, ligacao: Any) -> None:
        """Conta um ``ENVIAR``; acima do limite levanta ``RateLimit``."""
        agora = self._relogio()
        janela = [
            instante
            for instante in self._envios.get(ligacao, [])
            if agora - instante < JANELA_ENVIOS
        ]
        if len(janela) >= MAX_ENVIOS:
            raise RelayErro(ERR_RATE_LIMIT, "demasiados ENVIAR")
        janela.append(agora)
        self._envios[ligacao] = janela


# ---------------------------------------------------------------------
# Transporte asyncio
# ---------------------------------------------------------------------


async def _ler_mensagem(leitor: asyncio.StreamReader) -> tuple[int, bytes] | None:
    """Lê uma mensagem completa; ``None`` = EOF no início de um enquadramento."""
    try:
        cabecalho = await leitor.readexactly(4)
    except asyncio.IncompleteReadError:
        return None
    (comprimento,) = struct.unpack("<I", cabecalho)
    if comprimento == 0:
        raise RelayErro(ERR_PAYLOAD_MALFORMADO, "comprimento zero")
    if comprimento > 1 + MAX_PAYLOAD:
        raise RelayErro(ERR_PAYLOAD_GRANDE_DEMAIS, "mensagem acima do limite")
    corpo = await leitor.readexactly(comprimento)
    return corpo[0], corpo[1:]


async def _enviar(
    escritor: asyncio.StreamWriter, tipo: int, corpo: bytes
) -> None:
    """Escreve uma mensagem enquadada e espera que o buffer esvazie."""
    escritor.write(enquadrar(tipo, corpo))
    await escritor.drain()


async def _entregar(pares: list[tuple[Any, bytes]], relay: Relay) -> None:
    """Escreve ``0x84 CAIXA`` a cada destinatário; falha → desliga-o."""
    for alvo, envelope in pares:
        try:
            await _enviar(alvo, TIPO_CAIXA, envelope)
        except OSError:
            relay.desligar(alvo)


async def _processar(
    mensagem: tuple[int, bytes],
    escritor: asyncio.StreamWriter,
    relay: Relay,
) -> bool:
    """Executa um comando; devolve ``False`` quando a ligação deve fechar."""
    tipo, corpo = mensagem

    if tipo == TIPO_SUBSCREVER:
        if len(corpo) != TAM_MAILBOX:
            await _enviar(
                escritor,
                TIPO_ERRO,
                RelayErro(ERR_MAILBOX_INVALIDA, "mailbox tem de ter 32 bytes").corpo(),
            )
            return True
        pendentes = relay.subscrever(escritor, corpo)
        await _enviar(escritor, TIPO_OK, b"")
        await _entregar(pendentes, relay)
        return True

    if tipo == TIPO_ENVIAR:
        if len(corpo) <= TAM_MAILBOX:
            await _enviar(
                escritor,
                TIPO_ERRO,
                RelayErro(ERR_PAYLOAD_MALFORMADO, "ENVIAR: mailbox ‖ envelope").corpo(),
            )
            return True
        try:
            pares = relay.entregar(escritor, corpo[:TAM_MAILBOX], corpo[TAM_MAILBOX:])
        except RelayErro as erro:
            await _enviar(escritor, TIPO_ERRO, erro.corpo())
            return True
        await _enviar(escritor, TIPO_OK, b"")
        await _entregar(pares, relay)
        return True

    if tipo == TIPO_FECHO:
        if corpo:
            await _enviar(
                escritor,
                TIPO_ERRO,
                RelayErro(ERR_PAYLOAD_MALFORMADO, "FECHO sem corpo").corpo(),
            )
            return True
        await _enviar(escritor, TIPO_OK, b"")
        return False

    await _enviar(
        escritor,
        TIPO_ERRO,
        RelayErro(ERR_COMANDO_DESCONHECIDO, f"tipo {tipo:#04x}").corpo(),
    )
    return True


async def _atender(
    leitor: asyncio.StreamReader,
    escritor: asyncio.StreamWriter,
    relay: Relay,
) -> None:
    """Atende uma ligação até ``FECHO``, EOF ou erro de stream."""
    try:
        while True:
            try:
                mensagem = await _ler_mensagem(leitor)
            except RelayErro as erro:
                await _enviar(escritor, TIPO_ERRO, erro.corpo())
                break
            if mensagem is None:
                break
            if not await _processar(mensagem, escritor, relay):
                break
    except (asyncio.IncompleteReadError, OSError):
        # Ligação cortada a meio de um frame — só se desliga, sem resposta.
        pass
    finally:
        relay.desligar(escritor)
        escritor.close()


async def servir(host: str, porta: int) -> tuple[asyncio.AbstractServer, Relay]:
    """Abre o relay em ``host:porta`` (``0`` = porta livre) e devolve-o."""
    relay = Relay()
    servidor = await asyncio.start_server(
        lambda leitor, escritor: _atender(leitor, escritor, relay), host, porta
    )
    return servidor, relay


async def _para_sempre(host: str, porta: int) -> None:
    """Arranca o relay e serve até a tarefa ser cancelada."""
    servidor, _ = await servir(host, porta)
    async with servidor:
        await servidor.serve_forever()


def executar(host: str, porta: int) -> None:
    """Corre o relay indefinidamente (usado por ``onyxchat relay``)."""
    asyncio.run(_para_sempre(host, porta))
