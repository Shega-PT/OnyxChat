# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""ipc_client.py — cliente Unix Domain Socket para o daemon ``onyxchatd``.

Implementa o contrato de ``docs/ipc.md``:

* transporte **UDS** local, caminho ``$ONYXCHAT_SOCKET`` ou
  ``/tmp/onyxchat-{uid}.sock``;
* enquadramento ``[comprimento: u32 LE][carga útil]``, com limite de
  16 MiB por lado;
* pedidos síncronos, em três famílias:
  - pipeline — ``ENCODE (0x01)`` e ``DECODE (0x02)``, ver ``docs/pipeline.md``;
  - rede     — ``ESTADO..FECHAR (0x03..0x08)``;
  - handshake— ``PEDIR..CONFIRMAR_AMIZADE (0x09..0x0C)``;
* respostas ``[estado: 1B][corpo]`` — ``OK (0x00)`` devolve o corpo;
  ``ERRO (0x01)`` devolve ``[código: 1B][mensagem UTF-8 opcional]`` e
  vira :class:`ErroDaemon` com o código tipado.

Falhas de transporte (daemon em baixo, ligação cortada) viram
:class:`DaemonIndisponivel`/ :class:`RespostaInvalida`; o daemon nunca
aborta por dados do cliente, mas o cliente também nunca levanta
exceções genéricas — tudo é :class:`ErroIpc` ou subclasse: argumentos
malformados do chamador vêm :class:`PedidoInvalido`.
"""

from __future__ import annotations

import os
import socket
import struct
from dataclasses import dataclass

#: Enquadramento: 4 bytes little-endian; o daemon fecha acima disto.
#:
#: O valor deixou de ser um literal. O daemon deriva-o de um orçamento de
#: memória (``network/daemon_rust/src/orcamento.rs``), e este cliente
#: tem de aceitar o mesmo valor — por isso lê a variável de ambiente que
#: o daemon também lê, e cai no mesmo mínimo quando não está definida.
#:
#: A divergência entre os dois lados é o tipo de bug que já aconteceu
#: neste projecto: um literal 240 desactualizado num dos lados levava o
#: utilizador a acrescentar bytes ao pedido para «resolver» o problema
#: errado (``ipc.rs``, comentário sobre ``ERR_CORPO_INVALIDO``).
MAX_PAYLOAD = int(os.environ.get("ONYXCHAT_MAX_PAYLOAD", 0)) or 512 * 1024 * 1024

#: Maior corpo que o protocolo produz (4 chaves de 32 B + envelope de
#: 65 692 B). Serve de floor: um ``ONYXCHAT_MAX_PAYLOAD`` inválido no
#: ambiente não pode deixar o cliente abaixo do que o daemon consegue
#: descifrar.
MIN_CORPO = 65_692 + 4 * 32
MAX_PAYLOAD = max(MAX_PAYLOAD, MIN_CORPO)

# ---------------------------------------------------------------------
# Comandos (docs/ipc.md §Pedidos)
# ---------------------------------------------------------------------
CMD_HELLO = 0x00
CMD_ENCODE = 0x01
CMD_DECODE = 0x02
CMD_ESTADO = 0x03
CMD_OUVIR = 0x04
CMD_LIGAR = 0x05
CMD_ENVIAR = 0x06

# A origem do envio, que escolhe a janela de rate-limit no daemon.
ORIGEM_TEXTO = 0x00
ORIGEM_MEDIA = 0x01
CMD_RECEBER = 0x07
CMD_FECHAR = 0x08
CMD_PEDIR_AMIZADE = 0x09
CMD_ACEITAR_AMIZADE = 0x0A
CMD_RECUSAR_AMIZADE = 0x0B
CMD_CONFIRMAR_AMIZADE = 0x0C
# Media (`docs/media.md` §4.6). `0x0F` é `MEDIA_CAPTURAR`, de F7, e
# responde `ComandoDesconhecido` até lá.
CMD_MEDIA_INICIAR = 0x0D
CMD_MEDIA_ESTADO = 0x0E
CMD_MEDIA_CAPTURAR = 0x0F

# `MEDIA_LER` (F9): o próximo quadro de VP8 da captura viva. É o
# comando que faz os quadros saírem do `ffmpeg`: sem ele o `ffmpeg`
# bloqueava assim que o pipe enchesse, e a captura «funcionava»
# durante dois segundos.
CMD_MEDIA_LER = 0x11

# `MEDIA_AUDIO` (F9): o próximo pacote de Opus. Sem cabeçalho nenhum,
# porque o áudio **é** o relógio — o receptor conta os bytes e sabe o
# tempo (docs/media.md §5.3).
CMD_MEDIA_AUDIO = 0x12
# `MEDIA_IMAGEM` — descodifica um quadro de VP8 para JPEG.
#
# E o caminho de volta da imagem. O renderizador da interface é um
# WebKitGTK 6, e o WebKitGTK **não garante** `VideoDecoder` — a API de
# WebCodecs. Sem ela, quem tem o `ffmpeg` com o decoder de VP8 é o
# daemon, e quem mostra a imagem é a interface, num `<img>`.
CMD_MEDIA_IMAGEM = 0x13

#: Erro de media do daemon (``ERR_MEDIA`` em ``ipc.rs``).
#:
#: Vive aqui e não só no mapa de nomes porque o emissor de streaming
#: **decide** com ele: um `0x14` é «o ffmpeg morreu e a captura
#: acabou», que é o fim de uma chamada, e qualquer outro erro é uma
#: falha a propagar. Distinguir pelo código e não pelo texto é o que
#: impede uma palavra na spec de mudar o comportamento.
ERR_MEDIA = 0x14

#: O daemon não conseguiu decifrar com a chave que lhe deram
#: (``ERR_DECIFRAGEM_FALHOU`` em ``ipc.rs``).
#:
#: Vive aqui porque a **sondagem** precisa dele: um `MEDIA_LER` ou um
#: `DECODE` com a chave errada dá este código, e a sondagem trata-o
#: como «esta chave não bate» em vez de como uma falha. Qualquer outro
#: código — uma rede que caiu, um daemon que não respondeu — sobe, porque
#: não é a etiqueta que está errada.
ERR_DECIFRAGEM_FALHOU = 0x05
CMD_MEDIA_REPRODUZIR = 0x10

ESTADO_OK = 0x00
ESTADO_ERRO = 0x01

#: Versão do protocolo IPC (nibbles: high = major, low = minor) —
#: enviada no ``HELLO`` obrigatório (§Protocolo de versão).
VERSAO_IPC = 0x01
#: Versão do daemon incompatível → ``ERRO 0x12``.
ERR_VERSAO_INCOMPATIVEL = 0x12


def versao_compativel(versao: int, suporte: int = VERSAO_IPC) -> bool:
    """``True`` se duas versões ``major.minor`` são compatíveis.

    Compatível = **o ``major`` é igual**; o ``minor`` pode diferir.

    É a norma executável do princípio de tolerância de versões
    (``docs/index.md`` §Princípio da tolerância de versões), e é a
    espelhar de ``versao_compativel`` em ``ipc.rs``.

    Uma alteração de ``minor`` **tem de continuar compatível** — só se
    acrescenta, nunca muda a semântica do que já existe. Uma alteração de
    ``major`` é incompatível por definição, e é o que justifica o
    ``0x12``.

    Que isto importa: sem tolerância de ``minor``, um cliente com a
    versão local mais recente deixaria de falar com um daemon com a
    anterior — e dois utilizadores em versões diferentes do software não
    poderiam usar o mesmo sistema.
    """
    return (versao >> 4) == (suporte >> 4)

# ---------------------------------------------------------------------
# Tipos de frame P2P (corpo de RECEBER) — p2p.rs `FRAME_*`
# ---------------------------------------------------------------------
FRAME_CHAT = 0x01
FRAME_FRIEND_REQUEST = 0x10
FRAME_FRIEND_ACCEPT = 0x11
FRAME_FRIEND_REJECT = 0x12
FRAME_PING = 0x20
FRAME_PONG = 0x21

# Modo de ligação pedido a LIGAR — p2p.rs `ModoLigacao::de_ipc`.
MODO_DIRETO = 0x00
MODO_RELAY = 0x01

# Estado do backend Tor (byte de ESTADO) — tor.rs `EstadoTor::para_ipc`.
TOR_PARADO = 0x00
TOR_ARRANCANDO = 0x01
TOR_ATIVO = 0x02

# ---------------------------------------------------------------------
# Tamanhos fixos do protocolo (ipc.rs / handshake.rs)
# ---------------------------------------------------------------------
#: Chave privada Ed25519 (``seed``) de todos os comandos.
TAM_SEED = 32
#: Chave pública Ed25519.
TAM_PUB = 32
#: Nonce de handshake (anti-replay).
TAM_NONCE = 16
#: ``k1‖k5_p‖k9_p‖k5_par‖k9_par‖pub`` — resposta de ACEITAR/CONFIRMAR.
TAM_AMIZADE = 192
#: ``pub‖k1‖k5‖k9‖nonce‖sig`` — corpo de ``FRIEND_REQUEST``
#: (``versao(1)‖`` prefixado; 209 bytes no total).
TAM_CORPO_PEDIDO = 209
#: ``pub‖k5‖k9‖nonce‖sig`` — corpo de ``FRIEND_ACCEPT``
#: (``versao(1)‖`` prefixado; 177 bytes no total).
TAM_CORPO_ACEITE = 177
#: ``nonce‖sig`` — corpo de ``FRIEND_REJECT``
#: (``versao(1)‖`` prefixado; 81 bytes no total).
TAM_CORPO_RECUSA = 81

#: Nomes dos códigos de erro do daemon (ipc.md §Códigos de erro).
NOMES_ERRO = {
    0x01: "ComandoDesconhecido",
    0x02: "PayloadMalformado",
    0x03: "ChaveInvalida",
    0x04: "AssinaturaInvalida",
    0x05: "DecifragemFalhou",
    0x06: "EnvelopeInvalido",
    0x07: "CifragemFalhou",
    0x08: "TextoInvalidoUtf8",
    0x09: "PayloadGrandeDemais",
    0x0A: "NaoLigado",
    0x0B: "TorIndisponivel",
    0x0C: "HandshakeInvalido",
    0x0D: "DestinoInvalido",
    0x0E: "SemMensagem",
    0x0F: "EstadoInvalido",
    0x10: "RateLimit",
    0x11: "Relay",
    0x12: "VersaoIncompativel",
    0x13: "NonceRepetido",
    0x14: "Media",
}

#: Os leitores que o daemon tenta, por ordem (``media/reproduzir.rs``).
#:
#: A ordem é a mesma que a do Rust, e é uma **tabela partilhada por
#: índice**: o índice é o que vai em ``MEDIA_REPRODUZIR``, e um cliente
#: que mandasse o nome em vez do índice transformaria a ordem numa
#: interface. Mudar a lista dos dois lados ao mesmo tempo é a condição
#: para isto continuar a funcionar.
LEITORES_MEDIA = ("mpv", "ffplay", "cvlc")

#: O valor de «não há leitor» em ``MEDIA_ESTADO``. Não é um índice, e
#: passá-lo a ``MEDIA_REPRODUZIR`` dá erro — que é o que se quer, porque
#: o cliente que o mandou não viu a resposta do ``ESTADO``.
SEM_LEITOR = 0xFF


#: Nomes dos estados do backend Tor reportados por ``ESTADO``.
NOMES_TOR = {
    TOR_PARADO: "parado",
    TOR_ARRANCANDO: "arrancando",
    TOR_ATIVO: "ativo",
}


def caminho_socket() -> str:
    """Caminho do socket IPC: ``$ONYXCHAT_SOCKET`` ou padrão por UID."""
    caminho = os.environ.get("ONYXCHAT_SOCKET")
    if caminho:
        return caminho
    return f"/tmp/onyxchat-{os.getuid()}.sock"


# ---------------------------------------------------------------------
# Estruturas devolvidas pelos comandos da etapa de rede
# ---------------------------------------------------------------------


@dataclass(frozen=True)
class EstadoDaemon:
    """Resposta de ``ESTADO``: backend Tor, ligação P2P e amigos."""

    #: Código do backend Tor (ver :data:`NOMES_TOR`).
    tor: int
    #: ``True`` se existir ligação P2P ativa.
    ligado: bool
    #: Número de amizades registadas (``u16`` do daemon).
    amigos: int
    #: Endereço ``.onion`` publicado (``""`` se não estiver a escuta).
    onion: str


@dataclass(frozen=True)
class EstadoMedia:
    """Resposta de ``MEDIA_ESTADO``: o que o daemon pode fazer de media."""

    #: ``True`` depois de um ``MEDIA_INICIAR`` com sucesso.
    loja_apontada: bool
    #: ``True`` quando o ``ffmpeg`` está embebido nesta compilação.
    ffmpeg_embebido: bool
    #: O índice em :data:`LEITORES_MEDIA` do primeiro leitor disponível,
    #: ou ``-1`` se não houver nenhum.
    leitor: int


@dataclass(frozen=True)
class Frame:
    """Frame P2P recebido: tipo (ver ``FRAME_*``) e corpo."""

    tipo: int
    corpo: bytes


@dataclass(frozen=True)
class QuadroVideo:
    """Um quadro de vídeo da captura viva (``MEDIA_LER``).

    Attributes:
        sequencia: a ordem de **transporte**. Vem do daemon e não do
            IVF: o ``ffmpeg`` não escreve uma sequência, e o receptor
            precisa de distinguir um quadro perdido de um quadro
            atrasado — e o tempo não distingue, porque dois quadros
            podem ter o mesmo instante.
        clock_ms: o tempo de **media** do quadro, em milissegundos.
            É o §5.3: o receptor compara-o com o relógio contado e
            sabe se o stream se desincronizou.
        dados: os bytes de **VP8**, não pixéis.

            O daemon não descodifica: liga o ``libvpx`` para codificar
            e ligar também para descodificar duplicaria o executável
            por uma coisa que o emissor nunca precisa. Quem descodifica
            é o receptor (``media.VideoDescodificador``).
    """

    sequencia: int
    clock_ms: int
    dados: bytes


@dataclass(frozen=True)
class PacoteAudio:
    """Um pacote de Opus da captura viva (``MEDIA_AUDIO``).

    Attributes:
        sequencia: a ordem de **transporte**, com a mesma semântica do
            :class:`QuadroVideo` — a ordem em que o pacote viajou, e
            não a ordem em que foi capturado.
        clock_ms: o tempo de **media** do pacote, em milissegundos.

            E o que permite o *lip sync* (§5.4): o receptor compara-o
            com o relógio contado e sabe a que momento do áudio este
            pacote pertence, o que é o que alinha o áudio com o vídeo.

            E é o **mestre**: §5.3 diz que o áudio é o relógio, porque
            é o canal que não se perde um pacote sem se notar.
        dados: os bytes de **Opus**, não PCM.

            E o daemon não descodifica no caminho da captura — a
            descodificação é de quem **ouve**, e quem ouve é o receptor.
    """

    sequencia: int
    clock_ms: int
    dados: bytes


@dataclass(frozen=True)
class ChavesAmizade:
    """Tuplo de chaves de uma amizade confirmada (``TAM_AMIZADE`` bytes)."""

    k1: bytes
    k5_proprio: bytes
    k9_proprio: bytes
    k5_par: bytes
    k9_par: bytes
    publica: bytes


@dataclass(frozen=True)
class PedidoAmizade:
    """Resposta de ``PEDIR_AMIZADE``: chaves provisórias + pedido enviado."""

    k1: bytes
    k5: bytes
    k9: bytes
    #: Corpo de ``FRIEND_REQUEST`` (``TAM_CORPO_PEDIDO``) — entregar ao par.
    corpo: bytes


@dataclass(frozen=True)
class AceiteAmizade:
    """Resposta de ``ACEITAR_AMIZADE``: chaves + aceite a entregar ao par."""

    chaves: ChavesAmizade
    #: Corpo de ``FRIEND_ACCEPT`` (``TAM_CORPO_ACEITE``) — entregar ao par.
    corpo: bytes


class ErroIpc(Exception):
    """Base de todas as falhas de comunicação com o daemon."""


class ErroRespostaCurta(ErroIpc):
    """A resposta do daemon é mais curta do que o comando promete.

    Leva o comprimento **recebido** e o **esperado**, porque «resposta
    curta» sem os dois números obriga a ir ao log do daemon para
    descobrir se o comando mudou ou se o corpo é que veio mal.
    """

    def __init__(self, comando: int, recebido: int, esperado: int) -> None:
        super().__init__(
            f"comando {comando:#04x}: resposta com {recebido} bytes, "
            f"e esperavam-se {esperado}"
        )
        self.comando = comando
        self.recebido = recebido
        self.esperado = esperado


class DaemonIndisponivel(ErroIpc):
    """O daemon não está acessível (ligação recusada/ligação cortada)."""


class RespostaInvalida(ErroIpc):
    """Resposta malformada, truncada ou fora do contrato do protocolo."""


class PedidoInvalido(ErroIpc):
    """Argumento do chamador viola o contrato do protocolo IPC."""


class ErroDaemon(ErroIpc):
    """Erro reportado pelo daemon (``estado = ERRO``) com código tipado."""

    def __init__(self, codigo: int, mensagem: str = "") -> None:
        self.codigo = codigo
        self.mensagem = mensagem
        nome = NOMES_ERRO.get(codigo, f"Erro{codigo:#04x}")
        super().__init__(f"{nome}: {mensagem}" if mensagem else nome)


# ---------------------------------------------------------------------
# Utilitários de (de)serialização
# ---------------------------------------------------------------------


def _ler_exato(ligacao: socket.socket, tamanho: int) -> bytes:
    """Lê exatamente ``tamanho`` bytes; EOF prematuro → :class:`RespostaInvalida`."""
    fragmentos: list[bytes] = []
    restante = tamanho
    while restante > 0:
        pedaco = ligacao.recv(restante)
        if not pedaco:
            raise RespostaInvalida("resposta truncada pelo daemon")
        fragmentos.append(pedaco)
        restante -= len(pedaco)
    return b"".join(fragmentos)


def _utf8(dados: bytes, operacao: str) -> str:
    """Decodifica um campo UTF-8 da resposta ou levanta ``RespostaInvalida``."""
    try:
        return dados.decode("utf-8")
    except UnicodeDecodeError as erro:
        raise RespostaInvalida(f"{operacao}: corpo não é UTF-8") from erro


def _cadeia(texto: str) -> bytes:
    """String UTF-8 com prefixo ``u16 LE`` — o inverso de ``tirar_cadeia``."""
    dados = texto.encode("utf-8")
    if len(dados) > 0xFFFF:
        raise PedidoInvalido(f"cadeia com {len(dados)} bytes (máx. 65535)")
    return struct.pack("<H", len(dados)) + dados


def _exigir_bytes(dados: bytes, esperado: int, nome: str) -> None:
    """Valida o tamanho de um campo fixo do pedido ANTES de o enviar."""
    if len(dados) != esperado:
        raise PedidoInvalido(
            f"{nome}: esperados {esperado} bytes (veio {len(dados)})"
        )


def _exigir_tamanho(corpo: bytes, esperado: int, operacao: str) -> None:
    """Valida o tamanho exato da resposta de um comando."""
    if len(corpo) != esperado:
        raise RespostaInvalida(
            f"{operacao}: resposta com {len(corpo)} bytes (esperados {esperado})"
        )


def _exigir_vazio(corpo: bytes, operacao: str) -> None:
    """Comandos cuja resposta ``OK`` é vazia (LIGAR/ENVIAR/FECHAR)."""
    if corpo:
        raise RespostaInvalida(
            f"{operacao}: resposta inesperada com {len(corpo)} bytes"
        )


def _chaves_de(corpo: bytes) -> ChavesAmizade:
    """Divide os ``TAM_AMIZADE`` bytes de uma amizade nos seis campos."""
    return ChavesAmizade(
        k1=corpo[0:32],
        k5_proprio=corpo[32:64],
        k9_proprio=corpo[64:96],
        k5_par=corpo[96:128],
        k9_par=corpo[128:160],
        publica=corpo[160:192],
    )


class ClienteIpc:
    """Cliente síncrono: uma ligação por pedido (o protocolo é local e curto)."""

    def __init__(self, caminho: str | None = None) -> None:
        self.caminho = caminho if caminho is not None else caminho_socket()

    def _pedir(self, comando: int, corpo: bytes) -> bytes:
        """Envia ``[comando][corpo]`` e devolve o corpo de uma resposta ``OK``."""
        pedido = bytes([comando]) + corpo
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as ligacao:
                ligacao.connect(self.caminho)
                self._hello(ligacao)
                ligacao.sendall(struct.pack("<I", len(pedido)) + pedido)
                cabecalho = _ler_exato(ligacao, 4)
                (tamanho,) = struct.unpack("<I", cabecalho)
                if tamanho == 0 or tamanho > MAX_PAYLOAD:
                    raise RespostaInvalida(
                        f"enquadramento inválido: {tamanho} bytes"
                    )
                resposta = _ler_exato(ligacao, tamanho)
        except OSError as erro:
            raise DaemonIndisponivel(
                f"daemon inacessível em {self.caminho}"
            ) from erro
        return self._interpretar(resposta)

    def _hello(self, ligacao: socket.socket) -> None:
        """Executa o ``HELLO`` obrigatório e valida a versão do daemon.

        ``docs/ipc.md`` §Protocolo de versão: toda a ligação abre
        com ``[0x00][versao]`` e o daemon responde ``OK ‖ versao ‖
        build`` ou ``ERRO 0x12``.

        Aceita-se qualquer ``minor`` com o mesmo ``major``
        (:func:`versao_compativel`); só uma diferença de ``major`` é
        incompatível. É o que permite a um cliente com a versão local
        mais recente falar com um daemon com a anterior.
        """
        pedido = bytes([CMD_HELLO, VERSAO_IPC])
        ligacao.sendall(struct.pack("<I", len(pedido)) + pedido)
        cabecalho = _ler_exato(ligacao, 4)
        (tamanho,) = struct.unpack("<I", cabecalho)
        if tamanho == 0 or tamanho > MAX_PAYLOAD:
            raise RespostaInvalida(
                f"enquadramento inválido no HELLO: {tamanho} bytes"
            )
        corpo = self._interpretar(_ler_exato(ligacao, tamanho))
        # O daemon devolve o *major* dele. Aceita-se se for compatível com
        # o nosso — não se exige igualdade, que quebraria a tolerância.
        if not corpo or not versao_compativel(corpo[0]):
            recebida = corpo[0] if corpo else None
            raise ErroDaemon(
                ERR_VERSAO_INCOMPATIVEL,
                f"versão do daemon {recebida!r} incompativível com {VERSAO_IPC:#04x}",
            )

    @staticmethod
    def _interpretar(resposta: bytes) -> bytes:
        """Converte ``[estado][corpo]`` em corpo (``OK``) ou exceção tipada."""
        if not resposta:
            raise RespostaInvalida("resposta vazia")
        estado, corpo = resposta[0], resposta[1:]
        if estado == ESTADO_OK:
            return corpo
        if estado != ESTADO_ERRO:
            raise RespostaInvalida(f"estado desconhecido: {estado:#04x}")
        if not corpo:
            raise RespostaInvalida("resposta de erro sem código")
        try:
            mensagem = corpo[1:].decode("utf-8")
        except UnicodeDecodeError:
            # Mensagem ilegível nunca derruba o cliente — segue sem ela.
            mensagem = ""
        raise ErroDaemon(corpo[0], mensagem)

    # -------------------------------------------------------------
    # Pipeline (etapa anterior)
    # -------------------------------------------------------------

    def cifrar(
        self,
        k1: bytes,
        k5: bytes,
        k9: bytes,
        seed: bytes,
        mensagem: str | bytes,
    ) -> bytes:
        """``ENCODE``: devolve o envelope binário completo (assinatura incluída)."""
        texto = mensagem.encode("utf-8") if isinstance(mensagem, str) else mensagem
        return self._pedir(CMD_ENCODE, k1 + k5 + k9 + seed + texto)

    def decifrar(
        self,
        k1: bytes,
        k5: bytes,
        k9: bytes,
        pub: bytes,
        envelope: bytes,
    ) -> str:
        """``DECODE``: devolve o plaintext UTF-8 ou levanta :class:`ErroDaemon`."""
        corpo = self._pedir(CMD_DECODE, k1 + k5 + k9 + pub + envelope)
        try:
            return corpo.decode("utf-8")
        except UnicodeDecodeError as erro:
            raise RespostaInvalida("plaintext do daemon não é UTF-8") from erro

    # -------------------------------------------------------------
    # Rede (etapa de rede)
    # -------------------------------------------------------------

    # -----------------------------------------------------------------
    # Media (`docs/media.md` §4.6) — o non-streaming
    # -----------------------------------------------------------------
    #
    # Três métodos, e nenhum leva uma chave. A §4.6.4 de `media.md` diz
    # porquê: desencriptar os envelopes guardados rebatia no
    # anti-replay, porque o `nonce1` de cada fatia já foi consumido
    # quando a mensagem chegou.

    def media_iniciar(self, caminho_loja) -> None:
        """``MEDIA_INICIAR``: aponta o daemon para a loja do ``sidecar``.

        Chamar **uma vez**, no arranque. O caminho tem de ser
        absoluto — o daemon recusa um relativo em vez de o resolver,
        porque um caminho relativo parece funcionar e falha depois, no
        ``MEDIA_REPRODUZIR``, onde já não há forma de saber que a loja
        errada é que estava aberta.

        Chamar outra vez com outro caminho substitui. A loja pode
        mudar de sítio se o utilizador a mover, e o daemon tem de
        acompanhar.
        """
        caminho = os.fspath(caminho_loja)
        if isinstance(caminho, bytes):
            caminho = caminho.decode("utf-8")
        if not caminho:
            raise ErroIpc("MEDIA_INICIAR: caminho vazio")
        dados = caminho.encode("utf-8")
        if len(dados) > 0xFFFF:
            raise ErroIpc(f"MEDIA_INICIAR: caminho de {len(dados)} bytes")

        self._pedir(CMD_MEDIA_INICIAR, struct.pack("<H", len(dados)) + dados)

    def media_capturar(
        self,
        largura: int,
        altura: int,
        fps: int,
        audio_kbps: int,
        video_kbps: int,
    ) -> None:
        """``MEDIA_CAPTURAR``: liga a captura e a codificação VP8.

        O corpo tem **10** bytes, todos ``u16`` little-endian::

            largura(2) altura(2) fps(2) audio_kbps(2) video_kbps(2)

        Args:
            largura: largura em pixels. Não precisa de ser múltiplo de
                16 — a grelha de telhas foi com o ``rawvideo`` (F9) e o
                VP8 trata do quadro inteiro.
            altura: altura em pixels.
            fps: quadros por segundo da fonte.
            audio_kbps: taxa do Opus. Com ``-vbr off`` o valor é exacto,
                e é por isso que o relógio por contagem funciona.
            video_kbps: taxa do VP8. É o **contrato** com o receptor:
                o daemon dimensiona o anel por este número, e se os
                dois lados divergirem a fila cresce sem sinal.

        854x480 a 30 fps com 300 kbit/s de vídeo e 32 de áudio são 344
        kbit/s, ou seja 55 % do orçamento de 12,8 msg/s
        (``docs/media.md`` §3.4.2).
        """
        for nome, valor, minimo in (
            ("largura", largura, 16),
            ("altura", altura, 16),
            ("fps", fps, 1),
            ("audio_kbps", audio_kbps, 1),
            ("video_kbps", video_kbps, 1),
        ):
            if not isinstance(valor, int) or isinstance(valor, bool):
                raise PedidoInvalido(f"{nome} tem de ser inteiro: {valor!r}")
            if valor < minimo or valor > 0xFFFF:
                raise PedidoInvalido(
                    f"{nome} tem de estar entre {minimo} e 65535: {valor}"
                )
        corpo = b"".join(
            v.to_bytes(2, "little") for v in (largura, altura, fps, audio_kbps, video_kbps)
        )
        _exigir_vazio(self._pedir(CMD_MEDIA_CAPTURAR, corpo), "MEDIA_CAPTURAR")

    def media_ler(self) -> QuadroVideo:
        """``MEDIA_LER``: o próximo quadro de VP8 da captura viva.

        Não tem corpo de pedido: o ``ffmpeg`` já tem o que há para dar,
        e pedir um quadro específico por ``stream`` ou ``seq`` seria o
        receptor a controlar o que o ecrã de quem emite mostra.

        Returns:
            O quadro: sequência, tempo de media e **bytes de VP8**.

        Raises:
            ErroDaemon: se não há captura, ou se a captura terminou. São
                duas coisas distintas e o receptor precisa de as
                distinguir: a primeira pede ``MEDIA_CAPTURAR``, a
                segunda manda parar de tentar.
        """
        resposta = self._pedir(CMD_MEDIA_LER, b"")
        if len(resposta) < 12:
            raise ErroRespostaCurta(CMD_MEDIA_LER, len(resposta), 12)
        seq = int.from_bytes(resposta[0:4], "little")
        clock_ms = int.from_bytes(resposta[4:12], "little", signed=True)
        return QuadroVideo(
            sequencia=seq,
            clock_ms=clock_ms,
            dados=bytes(resposta[12:]),
        )

    def media_imagem(self, vp8: bytes, largura: int, altura: int) -> dict[str, object]:
        """``MEDIA_IMAGEM``: descodifica um VP8 para JPEG.

        E o caminho de volta da imagem.

        ## E porque a geometria vai no pedido

        Porque o JPEG descodificado **também** tem a dimensão, e
        quem escreve o ``<img>`` precisa de saber a altura antes de o
        navegador ter descodificado seja o que for.

        E porque o VP8 **cru** não sabe a sua própria dimensão: a
        geometria está no cabeçalho do IVF, que é do emissor, e quem
        recebe o VP8 pela rede não o viu.

        :param vp8: os bytes crus do quadro.
        :param largura: a largura em pixéis.
        :param altura: a altura em pixéis.
        :returns: um dicionário com ``largura``, ``altura`` e ``jpeg``.
        """
        corpo = (
            largura.to_bytes(4, "little")
            + altura.to_bytes(4, "little")
            + vp8
        )
        resposta = self._pedir(CMD_MEDIA_IMAGEM, corpo)
        if len(resposta) < 4:
            raise ErroRespostaCurta(CMD_MEDIA_IMAGEM, len(resposta), 4)
        largura_v = int.from_bytes(resposta[0:2], "little")
        altura_v = int.from_bytes(resposta[2:4], "little")
        return {
            "largura": largura_v,
            "altura": altura_v,
            "jpeg": bytes(resposta[4:]),
        }

    def media_audio(self, timeout_ms: int | None = None) -> PacoteAudio | None:
        """``MEDIA_AUDIO``: o próximo pacote de Opus, com o seu tempo.

        Bloqueia até haver pacote, ou devolve ``None`` se a captura
        acabou.

        ## E porque tem ``clock_ms``, e antes não tinha

        Porque o *lip sync* (§5.4) precisa de saber **quando** o áudio
        que chegou foi produzido, e não só **quanto** áudio é.

        A versão anterior não tinha cabeçalho, e a razão era que «o áudio
        **é** o relógio»: o receptor contava os bytes e dividia por
        ``bytes_por_ms``. E isso dava o tempo **recebido** — o tempo em
        que o pacote chegou — e não o tempo em que foi capturado.

        São coisas diferentes. Um pacote que esperou 200 ms na rede tem o
        mesmo tamanho, e o receptor sem cabeçalho não distingue «a voz
        chegou agora» de «a voz foi capturada há 200 ms». E essa
        diferença é exactamente o que faz o áudio e o vídeo de uma
        chamada deixarem de estar alinhados.

        ## E o formato é o mesmo do :meth:`media_ler`

        ``seq: u32 LE ‖ clock_ms: i64 LE ‖ opus``. E não por simetria
        estética: o receptor tem **um** caminho de código para os dois, e
        um caminho só é um caminho que não pode ter metade das regras de
        um e metade as do outro.

        Args:
            timeout_ms: Não usado — o ``MEDIA_AUDIO`` bloqueia até haver
                pacote. Está aqui para a assinatura bater com os outros
                métodos de media e não obrigar quem chama a saber qual
                dos dois bloqueia.

        Returns:
            O pacote de Opus com o seu tempo, ou ``None`` se a captura
            acabou.
        """
        try:
            resposta = self._pedir(CMD_MEDIA_AUDIO, b"")
        except ErroDaemon as e:
            # `ERR_MEDIA` no `MEDIA_AUDIO` é o `ffmpeg` a morrer, e é o
            # fim da captura. Qualquer outro código é uma falha e sobe:
            # uma rede que caiu não é «a chamada acabou».
            if e.codigo == ERR_MEDIA:
                return None
            raise

        if len(resposta) < 12:
            raise ErroIpc(
                f"o MEDIA_AUDIO devolveu {len(resposta)} bytes, "
                "e o cabeçalho são 12"
            )

        seq = int.from_bytes(resposta[0:4], "little")
        clock_ms = int.from_bytes(resposta[4:12], "little", signed=True)
        return PacoteAudio(
            sequencia=seq,
            clock_ms=clock_ms,
            dados=bytes(resposta[12:]),
        )

    def media_estado(self) -> EstadoMedia:
        """``MEDIA_ESTADO``: o que o daemon pode fazer de media.

        As três coisas falham **independentemente**: ter loja não
        implica ter ``ffmpeg`` (que só interessa ao streaming), e
        nenhum dos dois implica ter o ``mpv`` instalado. Por isso são
        três bytes e não um estado único.
        """
        corpo = self._pedir(CMD_MEDIA_ESTADO, b"")
        if len(corpo) != 3:
            raise RespostaInvalida(f"MEDIA_ESTADO: resposta curta ({len(corpo)} bytes)")
        return EstadoMedia(
            loja_apontada=bool(corpo[0]),
            ffmpeg_embebido=bool(corpo[1]),
            leitor=-1 if corpo[2] == SEM_LEITOR else int(corpo[2]),
        )

    def media_reproduzir(self, transferencia_id: str, leitor: int) -> int:
        """``MEDIA_REPRODUZIR``: monta em RAM e devolve o ``pid``.

        :param transferencia_id: os 32 caracteres hex do ``id``.
        :param leitor: o índice em :data:`LEITORES_MEDIA` que veio de
            :meth:`media_estado`.
        :returns: o ``pid`` do leitor lançado.
        :raises ErroIpc: se o ``id`` não tiver a forma, se o índice não
            for de um leitor conhecido, ou se o daemon não conseguir.

        O ficheiro **nunca toca o disco**: o daemon escreve num
        ``memfd`` selado e lança o leitor sobre ``/proc/self/fd/N``.
        """
        if len(transferencia_id) != 32:
            raise ErroIpc(f"MEDIA_REPRODUZIR: o id tem {len(transferencia_id)} caracteres")
        # Hex em minúsculas, e validado. A loja guarda minúsculas, e
        # aceitar as duas formas daria dois ``id`` para a mesma
        # transferência — um que existe e um que não, e a diferença
        # só apareceria como «o vídeo não abre».
        if not all(c in "0123456789abcdef" for c in transferencia_id):
            raise ErroIpc("MEDIA_REPRODUZIR: o id não é hex minúsculo")
        if not 0 <= leitor < len(LEITORES_MEDIA):
            raise ErroIpc(f"MEDIA_REPRODUZIR: leitor {leitor} não existe")

        corpo = bytes([leitor]) + transferencia_id.encode("ascii")
        resposta = self._pedir(CMD_MEDIA_REPRODUZIR, corpo)
        if len(resposta) != 4:
            raise RespostaInvalida(f"MEDIA_REPRODUZIR: resposta curta ({len(resposta)} bytes)")
        return int.from_bytes(resposta, "little")

    def estado(self) -> EstadoDaemon:
        """``ESTADO``: estado do backend Tor, ligação P2P e nº de amizades."""
        corpo = self._pedir(CMD_ESTADO, b"")
        if len(corpo) < 4:
            raise RespostaInvalida(f"ESTADO: resposta curta ({len(corpo)} bytes)")
        return EstadoDaemon(
            tor=corpo[0],
            ligado=bool(corpo[1]),
            amigos=int.from_bytes(corpo[2:4], "little"),
            onion=_utf8(corpo[4:], "ESTADO"),
        )

    def ouvir(self) -> str:
        """``OUVIR``: publica o hidden service e devolve o endereço ``.onion``."""
        return _utf8(self._pedir(CMD_OUVIR, b""), "OUVIR")

    def ligar(
        self,
        modo: int,
        endpoint: str,
        destino: str,
        pub_propria: bytes,
        pub_par: bytes,
    ) -> None:
        """``LIGAR``: abre a ligação P2P (``MODO_DIRETO``/``MODO_RELAY``).

        ``endpoint`` vazio usa o endpoint por omissão do daemon; ``destino``
        aceita ``xxxx.onion`` ou o ID hex da pub Ed25519 do par.
        """
        if modo not in (MODO_DIRETO, MODO_RELAY):
            raise PedidoInvalido(f"modo deve ser 0x00 ou 0x01 (veio {modo:#04x})")
        _exigir_bytes(pub_propria, TAM_PUB, "pub_propria")
        _exigir_bytes(pub_par, TAM_PUB, "pub_par")
        corpo = (
            bytes([modo])
            + _cadeia(endpoint)
            + _cadeia(destino)
            + pub_propria
            + pub_par
        )
        _exigir_vazio(self._pedir(CMD_LIGAR, corpo), "LIGAR")

    def enviar(self, envelope: bytes, media: bool = False) -> None:
        """``ENVIAR``: frame ``CHAT 0x01`` com o envelope por corpo.

        O primeiro byte do comando é a **origem**, que escolhe a janela
        de rate-limit no daemon (``docs/p2p.rs``, excepção do streaming):

        ```text
        texto   0x00   128 mensagens por janela de 10 s
        media   0x01   64 mensagens por janela de 10 s
        ```

        São janelas separadas e não um teto maior porque `CHAT` é o
        mesmo tipo para os dois: o daemon não decifra, e quem sabe o
        que é é quem envia. ``media=True`` gasta a janela de media e
        nunca a de texto — uma chamada de vídeo a 480p usa 0,9 msg/s e
        pode conviver com uma conversa a 12,8 msg/s.

        Args:
            envelope: o envelope já cifrado, como antes.
            media: se o envio é streaming de áudio ou vídeo.
        """
        origem = bytes([ORIGEM_MEDIA if media else ORIGEM_TEXTO])
        _exigir_vazio(self._pedir(CMD_ENVIAR, origem + envelope), "ENVIAR")

    def receber(self, timeout_ms: int) -> Frame:
        """``RECEBER``: bloqueia até haver frame (``0`` = espera infinita)."""
        if not 0 <= timeout_ms <= 0xFFFFFFFF:
            raise PedidoInvalido(f"timeout_ms fora do intervalo u32: {timeout_ms}")
        corpo = self._pedir(CMD_RECEBER, struct.pack("<I", timeout_ms))
        if not corpo:
            raise RespostaInvalida("RECEBER: resposta sem tipo de frame")
        return Frame(tipo=corpo[0], corpo=corpo[1:])

    def fechar(self) -> None:
        """``FECHAR``: fecha a ligação P2P (idempotente)."""
        _exigir_vazio(self._pedir(CMD_FECHAR, b""), "FECHAR")

    # -------------------------------------------------------------
    # Handshake (etapa de rede)
    # -------------------------------------------------------------

    def pedir_amizade(self, seed: bytes, pub_dest: bytes) -> PedidoAmizade:
        """``PEDIR_AMIZADE``: envia ``FRIEND_REQUEST`` ao par indicado."""
        _exigir_bytes(seed, TAM_SEED, "seed")
        _exigir_bytes(pub_dest, TAM_PUB, "pub_dest")
        corpo = self._pedir(CMD_PEDIR_AMIZADE, seed + pub_dest)
        _exigir_tamanho(corpo, 3 * TAM_PUB + TAM_CORPO_PEDIDO, "PEDIR_AMIZADE")
        return PedidoAmizade(
            k1=corpo[0:32],
            k5=corpo[32:64],
            k9=corpo[64:96],
            corpo=corpo[96:],
        )

    def aceitar_amizade(self, seed: bytes, corpo_pedido: bytes) -> AceiteAmizade:
        """``ACEITAR_AMIZADE``: valida o pedido recebido e envia o aceite."""
        _exigir_bytes(seed, TAM_SEED, "seed")
        _exigir_bytes(corpo_pedido, TAM_CORPO_PEDIDO, "corpo_pedido")
        corpo = self._pedir(CMD_ACEITAR_AMIZADE, seed + corpo_pedido)
        _exigir_tamanho(corpo, TAM_AMIZADE + TAM_CORPO_ACEITE, "ACEITAR_AMIZADE")
        return AceiteAmizade(chaves=_chaves_de(corpo[:TAM_AMIZADE]), corpo=corpo[TAM_AMIZADE:])

    def recusar_amizade(self, seed: bytes, nonce: bytes) -> bytes:
        """``RECUSAR_AMIZADE``: devolve o ``FRIEND_REJECT`` assinado (81 B)."""
        _exigir_bytes(seed, TAM_SEED, "seed")
        _exigir_bytes(nonce, TAM_NONCE, "nonce")
        corpo = self._pedir(CMD_RECUSAR_AMIZADE, seed + nonce)
        _exigir_tamanho(corpo, TAM_CORPO_RECUSA, "RECUSAR_AMIZADE")
        return corpo

    def confirmar_amizade(self, seed: bytes, corpo_aceite: bytes) -> ChavesAmizade:
        """``CONFIRMAR_AMIZADE``: valida o aceite e fecha a amizade."""
        _exigir_bytes(seed, TAM_SEED, "seed")
        _exigir_bytes(corpo_aceite, TAM_CORPO_ACEITE, "corpo_aceite")
        corpo = self._pedir(CMD_CONFIRMAR_AMIZADE, seed + corpo_aceite)
        _exigir_tamanho(corpo, TAM_AMIZADE, "CONFIRMAR_AMIZADE")
        return _chaves_de(corpo)
