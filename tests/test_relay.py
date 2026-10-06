# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_relay.py — relay/TURN (server.relay_server).

Cobre o núcleo puro (:class:`Relay`, relógio injetável: TTL, caixa cheia,
rate-limit, re-subscrição) e o transporte asyncio real por TCP: push,
entrega adiada, enquadramento, todos os códigos de erro e o fecho de
ligação. Sem dependências externas — ``asyncio`` da biblioteca padrão.
"""

from __future__ import annotations

import asyncio
import hashlib
import struct

import pytest

from server import relay_server
from server.relay_server import (
    ERR_COMANDO_DESCONHECIDO,
    ERR_MAILBOX_INVALIDA,
    ERR_PAYLOAD_GRANDE_DEMAIS,
    ERR_PAYLOAD_MALFORMADO,
    ERR_RATE_LIMIT,
    ERR_SEM_ESPACO,
    MAX_PAYLOAD,
    Relay,
    RelayErro,
    TIPO_CAIXA,
    TIPO_ERRO,
    TIPO_FECHO,
    TIPO_OK,
    mailbox_de,
)

MB_A = bytes([0xAA]) * 32
MB_B = bytes([0xBB]) * 32


# ---------------------------------------------------------------------
# Identidade e enquadramento
# ---------------------------------------------------------------------


def test_mailbox_de_deriva_com_o_dominio() -> None:
    """``SHA-256("ONYX/RELAY/v1" ‖ pub)`` — estável e de 32 bytes."""
    pub = bytes([7]) * 32
    esperado = hashlib.sha256(b"ONYX/RELAY/v1" + pub).digest()
    assert mailbox_de(pub) == esperado
    assert len(mailbox_de(pub)) == 32
    assert mailbox_de(pub) != mailbox_de(bytes([8]) * 32)


def test_mailbox_de_rejeita_publica_curta() -> None:
    """Pública fora dos 32 bytes → ``MailboxInvalida``."""
    with pytest.raises(RelayErro) as capturado:
        mailbox_de(b"curta")
    assert capturado.value.codigo == ERR_MAILBOX_INVALIDA


def test_enquadrar_formata_comprimento_incluindo_tipo() -> None:
    """``[u32 LE = 1+len][tipo][corpo]`` — mesmo desenho do IPC local."""
    pacote = relay_server.enquadrar(0x84, b"abc")
    assert pacote[:4] == struct.pack("<I", 4)
    assert pacote[4] == 0x84
    assert pacote[5:] == b"abc"


def test_corpo_de_erro_formatado_e_sem_mensagem() -> None:
    """``RelayErro.corpo()`` = ``código‖«Nome: mensagem»`` (ou só o nome)."""
    assert RelayErro(ERR_SEM_ESPACO, "caixa cheia").corpo() == (
        bytes([ERR_SEM_ESPACO]) + b"SemEspaco: caixa cheia"
    )
    assert RelayErro(ERR_RATE_LIMIT).corpo() == bytes([ERR_RATE_LIMIT]) + b"RateLimit"


# ---------------------------------------------------------------------
# Núcleo: Relay (relógio injetável)
# ---------------------------------------------------------------------


def test_subscrever_sem_caixa_devolve_vazio() -> None:
    """Caixa inexistente → sem pendentes (``caixa is None``)."""
    relay = Relay()
    assert relay.subscrever("cliente", MB_A) == []


def test_subscricao_drena_a_caixa_pendente() -> None:
    """O que estava pendente é devolvido ao subscritor novo."""
    relay = Relay()
    assert relay.entregar("emissor", MB_A, b"atrasado") == []
    assert relay.subscrever("cliente", MB_A) == [("cliente", b"atrasado")]
    # Caixa vazia: nova subscrição não devolve nada.
    assert relay.subscrever("cliente", MB_A) == []


def test_entrega_direta_para_subscritores() -> None:
    """Com subscritores devolve pares (destinatário, envelope)."""
    relay = Relay()
    relay.subscrever("um", MB_A)
    relay.subscrever("dois", MB_A)
    pares = relay.entregar("emissor", MB_A, b"conteudo")
    assert sorted(pares) == [("dois", b"conteudo"), ("um", b"conteudo")]


def test_resubscrever_move_a_ligacao_de_caixa() -> None:
    """Uma ligação só fica numa caixa: subscrever a outra move-a."""
    relay = Relay()
    relay.subscrever("cliente", MB_A)
    relay.subscrever("cliente", MB_B)
    assert relay.entregar("outro", MB_A, b"para-a") == []
    assert relay.entregar("outro", MB_B, b"para-b") == [("cliente", b"para-b")]


def test_desligar_limpa_subscricoes_e_taxa() -> None:
    """Desligar remove a subscrição e a janela de rate-limit."""
    relogio = [0.0]
    relay = Relay(relogio=lambda: relogio[0])
    relay.subscrever("cliente", MB_A)
    relay.entregar("cliente", MB_A, b"x")  # conta na janela
    relay.desligar("cliente")
    assert relay.entregar("outro", MB_A, b"y") == []
    # Janela limpa: os 32 ENVIAR voltam a estar disponíveis.
    relogio[0] = 10.0
    for indice in range(32):
        relay.entregar("cliente", MB_A, bytes([indice]))


def test_caixa_cheia_levanta_sem_espaco() -> None:
    """64 pendentes é o limite; o 65.º não é descartado em silêncio."""
    relogio = [0.0]

    def avancar() -> float:
        relogio[0] += 1.0
        return relogio[0]

    relay = Relay(relogio=avancar)
    for indice in range(64):
        relay.entregar("emissor", MB_A, bytes([indice]))
    with pytest.raises(RelayErro) as capturado:
        relay.entregar("emissor", MB_A, b"65")
    assert capturado.value.codigo == ERR_SEM_ESPACO


def test_ttl_expurga_os_envelopes_velhos() -> None:
    """Passado o TTL o envelope é descartado (silenciosamente)."""
    relogio = [0.0]
    relay = Relay(relogio=lambda: relogio[0])
    relay.entregar("emissor", MB_A, b"velho")
    relogio[0] = 301.0
    # Expirou: a caixa é drenada e removida, sem nunca entregar o velho.
    assert relay.subscrever("cliente", MB_A) == []
    relay.desligar("cliente")
    assert relay.entregar("emissor", MB_A, b"novo") == []
    assert relay.subscrever("cliente", MB_A) == [("cliente", b"novo")]


def test_rate_limit_por_ligacao() -> None:
    """32 ENVIAR por janela; o 33.º dá ``RateLimit`` (relógio parado)."""
    relay = Relay(relogio=lambda: 0.0)
    for indice in range(32):
        relay.entregar("emissor", MB_A, bytes([indice]))
    with pytest.raises(RelayErro) as capturado:
        relay.entregar("emissor", MB_A, b"demais")
    assert capturado.value.codigo == ERR_RATE_LIMIT
    # Outra ligação tem a sua própria janela.
    assert relay.entregar("outro", MB_A, b"ok") == []


# ---------------------------------------------------------------------
# Transporte: entrega a subscritores mortos
# ---------------------------------------------------------------------


class EscritorMorto:
    """Transporte fechado — ``write`` levanta logo ``ConnectionResetError``."""

    def write(self, _dados: bytes) -> None:
        raise ConnectionResetError("transporte fechado")


def test_entrega_a_morto_desliga_o_alvo() -> None:
    """Falha ao escrever a um subscritor → desliga-o sem derrubar o relé."""
    relay = Relay()
    relay.subscrever(EscritorMorto(), MB_A)
    pares = relay.entregar("emissor", MB_A, b"env")
    assert len(pares) == 1
    asyncio.run(relay_server._entregar(pares, relay))
    # Desligado: a caixa volta a ficar sem destinatários.
    assert relay.entregar("emissor", MB_A, b"outro") == []


# ---------------------------------------------------------------------
# Transporte: servidor real em TCP
# ---------------------------------------------------------------------


class Cliente:
    """Cliente mínimo do protocolo do relé (enquadramento por comprimento)."""

    def __init__(self, leitor: asyncio.StreamReader, escritor: asyncio.StreamWriter) -> None:
        self.leitor = leitor
        self.escritor = escritor

    async def enviar(self, tipo: int, corpo: bytes = b"") -> None:
        self.escritor.write(relay_server.enquadrar(tipo, corpo))
        await self.escritor.drain()

    async def ler(self) -> tuple[int, bytes]:
        cabecalho = await self.leitor.readexactly(4)
        (comprimento,) = struct.unpack("<I", cabecalho)
        corpo = await self.leitor.readexactly(comprimento)
        return corpo[0], corpo[1:]

    def fechar(self) -> None:
        self.escritor.close()


async def _arrancar() -> tuple[asyncio.AbstractServer, int]:
    """Sobe o relay numa porta livre e devolve ``(servidor, porta)``."""
    servidor, _ = await relay_server.servir("127.0.0.1", 0)
    return servidor, servidor.sockets[0].getsockname()[1]


async def _cliente(porta: int) -> Cliente:
    leitor, escritor = await asyncio.open_connection("127.0.0.1", porta)
    return Cliente(leitor, escritor)


async def _parar(servidor: asyncio.AbstractServer, *clientes: Cliente) -> None:
    """Fecha clientes e servidor sem deixar tarefas penduradas."""
    for cliente in clientes:
        cliente.fechar()
    servidor.close()
    await asyncio.sleep(0.05)
    await servidor.wait_closed()


def test_push_para_subscritor() -> None:
    """SUBSCREVER + ENVIAR → ``OK`` ao emissor e ``CAIXA`` ao receptor."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        recebedor = await _cliente(porta)
        emissor = await _cliente(porta)
        try:
            await recebedor.enviar(relay_server.TIPO_SUBSCREVER, MB_A)
            assert await recebedor.ler() == (TIPO_OK, b"")

            await emissor.enviar(relay_server.TIPO_ENVIAR, MB_A + b"envelope")
            assert await emissor.ler() == (TIPO_OK, b"")
            assert await recebedor.ler() == (TIPO_CAIXA, b"envelope")
        finally:
            await _parar(servidor, recebedor, emissor)

    asyncio.run(cenario())


def test_enviar_antes_de_subscrever_fica_pendente() -> None:
    """Sem subscritor o envelope espera; a subscrição entrega-o logo."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        emissor = await _cliente(porta)
        recebedor = await _cliente(porta)
        try:
            await emissor.enviar(relay_server.TIPO_ENVIAR, MB_B + b"adiado")
            assert await emissor.ler() == (TIPO_OK, b"")

            await recebedor.enviar(relay_server.TIPO_SUBSCREVER, MB_B)
            assert await recebedor.ler() == (TIPO_OK, b"")
            assert await recebedor.ler() == (TIPO_CAIXA, b"adiado")
        finally:
            await _parar(servidor, emissor, recebedor)

    asyncio.run(cenario())


def test_fecho_responde_ok_e_fecha() -> None:
    """``FECHO`` vazio → ``OK`` e a ligação termina (EOF no cliente)."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            await cliente.enviar(relay_server.TIPO_FECHO)
            assert await cliente.ler() == (TIPO_OK, b"")
            with pytest.raises(asyncio.IncompleteReadError):
                await cliente.leitor.readexactly(1)
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_subscricao_mailbox_invalida() -> None:
    """Subscrição com corpo ≠ 32 bytes → ``MailboxInvalida``."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            await cliente.enviar(relay_server.TIPO_SUBSCREVER, b"curto")
            tipo, corpo = await cliente.ler()
            assert tipo == TIPO_ERRO
            assert corpo[0] == ERR_MAILBOX_INVALIDA
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_enviar_sem_envelope() -> None:
    """``ENVIAR`` só com a mailbox (envelope vazio) → ``PayloadMalformado``."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            await cliente.enviar(relay_server.TIPO_ENVIAR, MB_A)
            tipo, corpo = await cliente.ler()
            assert (tipo, corpo[0]) == (TIPO_ERRO, ERR_PAYLOAD_MALFORMADO)
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_fecho_com_corpo() -> None:
    """``FECHO`` com corpo → ``PayloadMalformado`` e a ligação continua."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            await cliente.enviar(relay_server.TIPO_FECHO, b"extra")
            tipo, corpo = await cliente.ler()
            assert (tipo, corpo[0]) == (TIPO_ERRO, ERR_PAYLOAD_MALFORMADO)
            await cliente.enviar(relay_server.TIPO_FECHO)
            assert await cliente.ler() == (TIPO_OK, b"")
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_tipo_desconhecido() -> None:
    """Tipo fora de {0x01, 0x02, 0x03} → ``ComandoDesconhecido``."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            await cliente.enviar(0x7F, b"lixo")
            tipo, corpo = await cliente.ler()
            assert (tipo, corpo[0]) == (TIPO_ERRO, ERR_COMANDO_DESCONHECIDO)
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_comprimento_zero_fecha_a_ligacao() -> None:
    """``comprimento = 0`` viola o protocolo → ``PayloadMalformado`` + fim."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            cliente.escritor.write(struct.pack("<I", 0))
            await cliente.escritor.drain()
            tipo, corpo = await cliente.ler()
            assert (tipo, corpo[0]) == (TIPO_ERRO, ERR_PAYLOAD_MALFORMADO)
            with pytest.raises(asyncio.IncompleteReadError):
                await cliente.leitor.readexactly(1)
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_comprimento_acima_do_limite() -> None:
    """Anúncio acima de ``1 + 16 MiB`` → ``PayloadGrandeDemais`` + fim."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            cliente.escritor.write(struct.pack("<I", 2 + MAX_PAYLOAD))
            await cliente.escritor.drain()
            tipo, corpo = await cliente.ler()
            assert (tipo, corpo[0]) == (TIPO_ERRO, ERR_PAYLOAD_GRANDE_DEMAIS)
            with pytest.raises(asyncio.IncompleteReadError):
                await cliente.leitor.readexactly(1)
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_frame_truncado_só_desliga() -> None:
    """EOF a meio do enquadramento → a ligação morre sem mensagem de erro."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            # Anuncia 4 bytes de corpo e envia só 1 → EOF a meio.
            cliente.escritor.write(struct.pack("<I", 4) + b"\x01")
            await cliente.escritor.drain()
            cliente.escritor.write_eof()
            with pytest.raises(asyncio.IncompleteReadError):
                await cliente.leitor.readexactly(1)
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_rate_limit_no_servidor() -> None:
    """A 33.ª ``ENVIAR`` da mesma ligação o relé responde ``RateLimit``."""

    async def cenario() -> None:
        servidor, porta = await _arrancar()
        cliente = await _cliente(porta)
        try:
            for indice in range(32):
                await cliente.enviar(relay_server.TIPO_ENVIAR, MB_A + bytes([indice]))
                assert await cliente.ler() == (TIPO_OK, b"")
            await cliente.enviar(relay_server.TIPO_ENVIAR, MB_A + b"33")
            tipo, corpo = await cliente.ler()
            assert (tipo, corpo[0]) == (TIPO_ERRO, ERR_RATE_LIMIT)
        finally:
            await _parar(servidor, cliente)

    asyncio.run(cenario())


def test_ligacao_que_sai_e_limpa_o_estado() -> None:
    """O ``finally`` do atendente desliga a ligação do relé (EOF limpo)."""

    async def cenario() -> None:
        servidor, relay = await relay_server.servir("127.0.0.1", 0)
        porta = servidor.sockets[0].getsockname()[1]
        cliente = await _cliente(porta)
        await cliente.enviar(relay_server.TIPO_SUBSCREVER, MB_A)
        assert await cliente.ler() == (TIPO_OK, b"")
        cliente.fechar()
        await asyncio.sleep(0.05)
        # Subscritor desligado: o relé volta a entregar em modo pendente.
        assert relay.entregar("emissor", MB_A, b"depois") == []
        servidor.close()
        await servidor.wait_closed()

    asyncio.run(cenario())


# ---------------------------------------------------------------------
# Arranque
# ---------------------------------------------------------------------


def test_para_sempre_e_cancelado() -> None:
    """``_para_sempre`` serve até a tarefa ser cancelada (limpa sem avisos)."""

    async def cenario() -> None:
        tarefa = asyncio.create_task(relay_server._para_sempre("127.0.0.1", 0))
        await asyncio.sleep(0.1)
        assert not tarefa.done()
        tarefa.cancel()
        with pytest.raises(asyncio.CancelledError):
            await tarefa

    asyncio.run(cenario())


def test_executar_usa_asyncio_run(monkeypatch) -> None:
    """``executar`` delega em ``asyncio.run`` (o CLI só passa host/porta)."""
    corrotinas = []
    monkeypatch.setattr(
        relay_server.asyncio, "run", lambda corotina: corrotinas.append(corotina)
    )
    relay_server.executar("127.0.0.1", 0)
    assert len(corrotinas) == 1
    corrotinas[0].close()  # nunca executada — evita o aviso de corrotina
