# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""media.py — o módulo `onyxmedia` do lado do Python.

Áudio e vídeo como **texto**, dentro do envelope que o sistema já
transporta. A especificação normativa é ``docs/media.md``; este
ficheiro e ``network/daemon_rust/src/media/`` são as duas
implementações da mesma coisa, e têm de produzir os mesmos bytes.

## Porque este ficheiro existe e não é um invólucro

Porque a derivação de chave e a moldura têm de existir **nos dois
lados**. O daemon tem a geometria da moldura para montar o que envia e
para validar o que entra; o cliente tem-na para montar o manifesto, para
reconhecer uma resposta de texto e para pedir as fatias que faltam.

Se houvesse uma implementação só, o outro lado teria de a adivinhar —
e um formato adivinhado é um formato que diverge no dia em que alguém
muda um número num dos lados sem mudar no outro.

O que este ficheiro **não** faz é criptografia: o `ENCODE` e o
`DECODE`` são do daemon, e ``pipeline.py`` é quem lhes fala. Aqui só há
formato, tabelas e aritmética.

## O invariante

    O que sai para a rede é um ``FRAME_CHAT`` com um envelope normal.
    A rede não sabe se é texto, áudio ou vídeo.

Daqui decorre que ``ipc_client.py`` **não muda**: uma mensagem de media
é uma chamada a :meth:`Pipeline.cifrar` com uma ``Chaves`` diferente.
Nenhum comando novo, nenhum tipo de frame novo.
"""

from __future__ import annotations

import base64 as _b64
import hashlib
import os
import subprocess
from collections import deque
from dataclasses import dataclass

from .amizade import (
    TIPO_AUDIO,
    TIPO_CALL,
    TIPO_FICHEIRO,
    TIPO_VIDEO,
    ErroDerivacao,
    derivar_k1_por_tipo,
)
from .ipc_client import (
    ERR_DECIFRAGEM_FALHOU,
    ERR_MEDIA,
    ErroDaemon,
    ErroIpc,
    QuadroVideo,
)
from .keys import Chaves

# ---------------------------------------------------------------------
# Limites
# ---------------------------------------------------------------------

#: `MAX_PLAINTEXT`, de `docs/pipeline.md` §Limites. Duplicado aqui
#: porque este ficheiro tem de poder ser lido sem arrancar o daemon —
#: e porque o teste de limite precisa de uma constante com que comparar,
#: que um `import` do daemon não dá.
MAX_PLAINTEXT = 65_536

#: A geometria do preâmbulo, derivada e não escrita. Ver
#: ``network/daemon_rust/src/media/moldura.rs`` para a nota sobre o que
#: acontece quando se escreve isto à mão.
INICIO_TIPO = 4
INICIO_VERSAO = INICIO_TIPO + 1
INICIO_MODO = INICIO_VERSAO + 1
INICIO_FLAGS = INICIO_MODO + 1
INICIO_STREAM = INICIO_FLAGS + 1
LARGURA_STREAM = 5
INICIO_SEQ = INICIO_STREAM + LARGURA_STREAM
LARGURA_SEQ = 10
INICIO_CLOCK = INICIO_SEQ + LARGURA_SEQ
LARGURA_CLOCK = 12
INICIO_CORPO = INICIO_CLOCK + LARGURA_CLOCK
TAM_PRE_AMBULO = INICIO_CORPO + 1

#: O maior pedaço de media que cabe num envelope:
#: ``(MAX_PLAINTEXT − TAM_PRE_AMBULO) / 4 × 3``.
TAM_MEDIA_MAX = (MAX_PLAINTEXT - TAM_PRE_AMBULO) // 4 * 3

# ---------------------------------------------------------------------
# A agregação do áudio (Fase 2)
# ---------------------------------------------------------------------

#: Quantos pacotes de Opus vão no máximo na mesma mensagem.
#:
#: ## A conta que obriga a isto
#:
#: A chamada precisa de mensagens, e a conta é por **contagem** e não por
#: bitrate:
#:
#: ```text
#: VP8, 1 moldura por quadro a 30 fps   →  30 msg/s  →  300 por janela
#: Opus, 1 pacote de 20 ms               →  50 msg/s  →  500 por janela
#:                                                    ────────────────
#:                                            80 msg/s    800 por janela
#: ```
#:
#: O limite de media é 512 por janela de 10 s (`p2p.rs`), e 800 estoura
#: nele em seis segundos. A agregação é o que torna a chamada possível
#: sem subir o limite para um número que já não distingue «chamada» de
#: «inundação».
#:
#: ## Porquê 8
#:
#: Com 8 pacotes de 20 ms, ou 160 ms de áudio por mensagem:
#:
#: ```text
#: áudio  160 ms →  6,3 msg/s  →  ~63 por janela
#: vídeo  30 fps  →  30   msg/s →  300 por janela
#:                                 ──────────────
#:                            ~36 msg/s  ~363 por janela
#: ```
#:
#: 363 contra 512: **41% de folga**. E a folga é necessária, porque um
#: `ffmpeg` sob carga entrega quadros aos trancos e uma janela fixa sem
#: margem rejeita o pico mesmo quando a média cabe.
#:
#: ## Porquê não mais
#:
#: Porque o atraso. A mensagem só sai quando a fila atinge o tempo, e o
#: áudio dessa mensagem chega ao receptor todo ao mesmo tempo: o
#: primeiro pacote espera pelos outros sete. Com 8 são 160 ms, que é o
#: mesmo que o jitter buffer do receptor (§5.4) absorve. Com 20 seriam
#: 400 ms, e a chamada começava com um atraso que se ouve.
PAQUETES_POR_MOLDURA = 8

#: Os milissegundos que uma mensagem de áudio tem de ter antes de sair.
#:
#: É :data:`PAQUETES_POR_MOLDURA` a 20 ms, e é uma constante e não uma
#: conta porque o pacote do Opus **não tem duração fixa**: o `ffmpeg`
#: produz pacotes de 2,5 ms a 60 ms conforme o sinal. Agregar por contagem
#: daria mensagens de durações diferentes com o mesmo nome, e o
#: `clock_ms` deixaria de descrever o que a mensagem leva.
#:
#: A agregação decide por **tempo**, e este número é o alvo. O limite de
#: pacotes continua a existir em :data:`PAQUETES_POR_MOLDURA` e é o que
#: impede que um pacote de 60 ms faça a fila crescer sem fim: com o alvo
#: em 160 ms, três pacotes de 60 ms saem em 180 ms.
MS_POR_MOLDURA_AUDIO = 160

#: A magia, em ASCII.
MAGIA = b"ONX1"

#: A versão da moldura, como carácter.
VERSAO = b"1"

#: O separador entre o preâmbulo e o corpo.
SEPARADOR = b"\n"

# ---------------------------------------------------------------------
# Tipos e modos
# ---------------------------------------------------------------------

#: Os tipos de conteúdo que têm **moldura**, como tuplo
#: ``(carácter, byte derivado)``.
#:
#: O par vive junto porque os dois valores **não podem divergir** — o
#: carácter vai no ficheiro e o byte vai num hash — e um formato que
#: exige dois sítios para o mesmo conceito diverge no primeiro dia em
#: que alguém acrescenta uma linha.
#:
#: ## Porque só dois, e porque `call` e `ficheiro` não entram
#:
#: Porque o byte do *carácter* no byte 5 da moldura só tem quatro bits
#: livres (ver `abrir`), e `A`/`V` são os seus únicos ocupantes. Um
#: `call` **não é** uma moldura: é um envelope cujo corpo é ASCII
#: versionado, que o `ENCODE` trata como texto opaco.
#:
#: O que partilha com o áudio e o vídeo é a **chave derivada**, não o
#: formato — e por isso vive em :data:`TIPOS_DE_CHAVE`, que é onde a
#: derivação a encontra.
TIPOS = {
    "audio": (b"A", TIPO_AUDIO),
    "video": (b"V", TIPO_VIDEO),
}

#: Os tipos que têm chave derivada, por nome.
#:
#: É a soma de :data:`TIPOS` (os que têm moldura) com os de controlo,
#: que têm chave mas não moldura. A separação é deliberada: uma tabela
#: só, com tudo misturado, daria ao `abrir()` um terceiro tipo para
#: aceitar — e o byte 5 da moldura não tem espaço para ele.
#:
#: ## O texto **não** entra
#:
#: Continua a usar a K1 de longa duração, e é por isso que o texto
#: produzido é byte-a-byte igual ao que produzia antes de existir
#: media. A garantia é verificada por `test_o_texto_da_mesma_bytes`, e
#: é a invariante de `docs/media.md` §1.2 — a rede não pode distinguir
#: texto de media, e isso começa na chave.
TIPOS_DE_CHAVE = {
    **{nome: byte for nome, (_, byte) in TIPOS.items()},
    "call": TIPO_CALL,
    "ficheiro": TIPO_FICHEIRO,
}

#: O inverso de :data:`TIPOS`: caractere → nome.
#:
#: Existe para que quem vem do vector — ou do que o `ENCODE` devolve —
#: não tenha de inverter o dicionário à mão num `for`. Inverter à mão
#: num teste é banal e é exactamente onde nasce uma segunda fonte de
#: verdade sobre o formato.
#: Indexado por **texto de um carácter**, e não por `bytes`, porque é
#: assim que o valor aparece no vector e em qualquer texto que chegue
#: do `DECODE`. `abrir` é a única função que trabalha em `bytes`, e
#: converte à entrada.
TIPOS_POR_CARACTERE = {car.decode("ascii"): nome for nome, (car, _) in TIPOS.items()}

#: O inverso: nome → carácter. Indexado por `str` e não por `bytes`
#: porque é assim que o nome circula — no `Manifesto.tipo` e no que o
#: receptor tem. As duas direções existem porque são dois usos
#: diferentes, e inverter um dicionário à mão num chamador é o
#: caminho mais curto para os dois divergirem.
CARACTERE_POR_TIPO = {nome: car.decode("ascii") for nome, (car, _) in TIPOS.items()}

#: Os modos, como tuplo ``(carácter, nome)``.
MODOS = {
    b"F": "nao-streaming",
    b"S": "streaming",
}

#: As flags, como ``(nome, bit)``. O valor no preâmbulo é a soma dos
#: bits acesos, em base 2.
FLAGS = {
    "manifesto": 1 << 0,
    "keyframe": 1 << 1,
    "fim-de-lote": 1 << 2,
}

#: A máscara das flags definidas.
FLAGS_DEFINIDAS = 0b111


#: O inverso em `bytes`, para a única função que trabalha em bytes:
#: :func:`abrir`. Existe em vez de converter no ponto de uso porque uma
#: conversão por chamada de ``bytes.decode`` num caminho que corre por
#: frame é um custo sem motivo.
TIPOS_INV = {car: nome for nome, (car, _) in TIPOS.items()}


class ErroMedia(ValueError):
    """Estrutura inválida na moldura.

    Levanta-se em vez de devolver ``None`` porque todos os erros de
    ``moldura.abrir`` têm a mesma consequência — a mensagem não é
    processável — e uma excepção faz o chamador tratar os quatro de uma
    vez. Quem precisar de saber **qual** foi usa
    :func:`classificar_erro`, que devolve o motivo.
    """


def _exigir(valor: bytes, tamanho: int, nome: str) -> bytes:
    """Valida o tamanho de um campo antes de o usar."""
    if not isinstance(valor, (bytes, bytearray)):
        raise ErroMedia(f"{nome} tem de ser bytes, não {type(valor).__name__}")
    if len(valor) != tamanho:
        raise ErroMedia(f"{nome} tem de ter {tamanho} bytes, tem {len(valor)}")
    return bytes(valor)


# ---------------------------------------------------------------------
# Base-64
# ---------------------------------------------------------------------


def codificar(dados: bytes) -> str:
    """Codifica em base-64 padrão (RFC 4648 §4), com preenchimento.

    Usa a biblioteca padrão em vez de uma implementação própria. É a
    única coisa neste ficheiro que **não** reimplementa, e a razão é
    concreta: a base-64 é um formato de mil e tal linhas de especificação
    e de vectores públicos, e reimplementá-la aqui só serviria para ter
    uma segunda fonte de divergência. O ``.decode("ascii")`` é o que
    garante que a saída é ASCII — que é o que o ``ENCODE`` exige.
    """
    return _b64.b64encode(dados).decode("ascii")


def descodificar(texto: str) -> bytes:
    """Descodifica base-64 padrão.

    **Estrito**, como o lado Rust: ``validate=True`` é o que recusa um
    carácter fora do alfabeto, e é o que impede que um texto que o
    emissor nunca produziu seja lido como se tivesse produzido.
    """
    try:
        return _b64.b64decode(texto.encode("ascii"), validate=True)
    except (ValueError, UnicodeEncodeError) as erro:
        raise ErroMedia(f"corpo em base-64 inválido: {erro}") from erro


# ---------------------------------------------------------------------
# A moldura
# ---------------------------------------------------------------------


@dataclass(frozen=True)
class Moldura:
    """Uma moldura montada, com os campos que nela estão.

    Fixa e imutável por uma razão que já não precisa de ser defendida:
    isto vai ser cifrado por nove camadas que não devolvem nada, e um
    texto que alguém pudesse mudar depois de cifrado daria um envelope
    cujo conteúdo não corresponde ao que foi cifrado.
    """

    tipo: str
    modo: str
    stream: int
    seq: int
    clock_ms: int
    corpo: bytes = b""
    flags: int = 0

    @property
    def texto(self) -> str:
        """O plaintext para o ``ENCODE``."""
        return self._montar()

    def para_bytes(self) -> bytes:
        """O plaintext em bytes, que é o que o ``ENCODE`` quer."""
        return self._montar().encode("ascii")

    def _montar(self) -> str:
        try:
            car_tipo, _ = TIPOS[self.tipo]
        except KeyError:
            raise ErroMedia(f"tipo desconhecido: {self.tipo!r}") from None
        try:
            car_modo = next(c for c, nome in MODOS.items() if nome == self.modo)
        except StopIteration:
            raise ErroMedia(f"modo desconhecido: {self.modo!r}") from None

        if self.flags & ~FLAGS_DEFINIDAS:
            raise ErroMedia(f"flags 0x{self.flags:02x} têm bits reservados acesos")
        if not 0 <= self.stream <= 0xFFFF:
            raise ErroMedia(f"stream {self.stream} não cabe em u16")
        if not 0 <= self.seq <= 0xFFFFFFFF:
            raise ErroMedia(f"seq {self.seq} não cabe em u32")
        if not -(2**31) <= self.clock_ms < 2**31:
            raise ErroMedia(f"clock_ms {self.clock_ms} não cabe em i32")

        cabecalho = (
            MAGIA
            + car_tipo
            + VERSAO
            + car_modo
            + bytes([ord("0") + self.flags])
            + f"{self.stream:0{LARGURA_STREAM}d}".encode("ascii")
            + f"{self.seq:0{LARGURA_SEQ}d}".encode("ascii")
            + f"{self.clock_ms:+0{LARGURA_CLOCK}d}".encode("ascii")
            + SEPARADOR
        )
        # O `debug_assert` do Rust tem um equivalente aqui, e pela mesma
        # razão: uma divergência de um byte nesta geometria apareceria
        # como «o áudio abre como vídeo», muito longe da sua causa.
        if len(cabecalho) != TAM_PRE_AMBULO:
            raise ErroMedia(
                f"o preâmbulo mede {len(cabecalho)}, e a spec diz {TAM_PRE_AMBULO}"
            )
        return cabecalho.decode("ascii") + codificar(self.corpo)


@dataclass(frozen=True)
class Aberta:
    """Uma moldura lida de volta."""

    tipo: str
    modo: str
    stream: int
    seq: int
    clock_ms: int
    flags: int
    corpo: bytes


def abrir(texto: str) -> Aberta:
    """Abre uma moldura a partir do plaintext que o ``DECODE`` devolveu.

    Fecha por construção, como o lado Rust: valida a estrutura antes de
    devolver, e nenhuma das operações pode entrar fora do que o
    ``DECODE`` já validou. O plaintext já passou por UTF-8 no daemon,
    logo Unicode não é um problema; o que fica é a estrutura.

    O corpo só é descodificado **depois** de o preâmbulo estar inteiro e
    válido — a ordem é a mesma do Rust e a razão também: uma base-64 mal
    formada num envelope truncado não deve ser o erro que sai, porque
    esse não aponta para a causa.
    """
    if not isinstance(texto, str):
        raise ErroMedia("o plaintext tem de ser str")
    bruto = texto.encode("utf-8")

    if len(bruto) < TAM_PRE_AMBULO:
        raise ErroMedia(f"moldura com {len(bruto)} bytes, menos de {TAM_PRE_AMBULO}")
    if bruto[:4] != MAGIA:
        raise ErroMedia("os 4 primeiros bytes não são ONX1")

    versao = bruto[INICIO_VERSAO : INICIO_VERSAO + 1]
    if versao != VERSAO:
        raise ErroMedia(f"versão de moldura {versao!r} desconhecida")

    car_tipo = bruto[INICIO_TIPO : INICIO_TIPO + 1]
    if car_tipo not in TIPOS_INV:
        raise ErroMedia(f"tipo {car_tipo!r} desconhecido")
    tipo = TIPOS_INV[car_tipo]

    car_modo = bruto[INICIO_MODO : INICIO_MODO + 1]
    if car_modo not in MODOS:
        raise ErroMedia(f"modo {car_modo!r} desconhecido")
    modo = MODOS[car_modo]

    car_flags = bruto[INICIO_FLAGS : INICIO_FLAGS + 1]
    if not b"0" <= car_flags <= b"7":
        raise ErroMedia(f"flags {car_flags!r} fora da gama '0'..'7'")
    flags = car_flags[0] - ord("0")
    if flags & ~FLAGS_DEFINIDAS:
        raise ErroMedia(f"flags 0x{flags:02x} têm bits reservados acesos")

    stream = _ler_decimal(bruto, INICIO_STREAM, LARGURA_STREAM, "stream")
    if stream > 0xFFFF:
        raise ErroMedia(f"stream {stream} não cabe em u16")
    seq = _ler_decimal(bruto, INICIO_SEQ, LARGURA_SEQ, "seq")
    if seq > 0xFFFFFFFF:
        raise ErroMedia(f"seq {seq} não cabe em u32")
    clock_ms = _ler_decimal_com_sinal(bruto, INICIO_CLOCK, LARGURA_CLOCK, "clock_ms")
    if not -(2**31) <= clock_ms < 2**31:
        raise ErroMedia(f"clock_ms {clock_ms} não cabe em i32")

    if bruto[INICIO_CORPO : INICIO_CORPO + 1] != SEPARADOR:
        raise ErroMedia("o byte 35 não é o separador")

    return Aberta(
        tipo=tipo,
        modo=modo,
        stream=stream,
        seq=seq,
        clock_ms=clock_ms,
        flags=flags,
        corpo=descodificar(texto[INICIO_CORPO + 1 :]),
    )


def _ler_decimal(bruto: bytes, inicio: int, largura: int, nome: str) -> int:
    """Lê ``largura`` algarismos decimais a partir de ``inicio``."""
    campo = bruto[inicio : inicio + largura]
    if len(campo) != largura:
        raise ErroMedia(f"o campo {nome} está truncado")
    try:
        return int(campo.decode("ascii"))
    except (ValueError, UnicodeDecodeError) as erro:
        raise ErroMedia(f"o campo {nome} não é um número: {campo!r}") from erro


def _ler_decimal_com_sinal(bruto: bytes, inicio: int, largura: int, nome: str) -> int:
    """Lê ``largura`` algarismos com sinal a partir de ``inicio``."""
    sinal = bruto[inicio : inicio + 1]
    if sinal == b"-":
        return -_ler_decimal(bruto, inicio + 1, largura - 1, nome)
    if sinal == b"+":
        return _ler_decimal(bruto, inicio + 1, largura - 1, nome)
    raise ErroMedia(f"o campo {nome} não tem sinal: {sinal!r}")


# ---------------------------------------------------------------------
# O modo não-streaming: manifesto e fracção
# ---------------------------------------------------------------------

#: O tamanho de uma fatia, em bytes.
#:
#: Derivado de :data:`TAM_PRE_AMBULO` e :data:`MAX_PLAINTEXT`, e não
#: escrito à mão — pelo mesmo motivo que a geometria do preâmbulo: um
#: literal divergiria do que o `ENCODE` aceita, e a falha apareceria
#: como ``0x09 PayloadGrandeDemais`` a meio de uma transferência.
TAM_FATIA = TAM_MEDIA_MAX

#: O tamanho do manifesto sem o nome: 17 bytes de campos + 32 do
#: resumo (``docs/media.md`` §2.5).
TAM_MANIFESTO_FIXO = 1 + 4 + 4 + 2 + 2 + 2 + 4 + 32

#: Os offsets dos campos, derivados e não escritos.
INICIO_TOTAL = 1
INICIO_PEDACOS = INICIO_TOTAL + 4
INICIO_BYTES_PEDACO = INICIO_PEDACOS + 4
INICIO_CODEC = INICIO_BYTES_PEDACO + 2
INICIO_FORMATO = INICIO_CODEC + 2
INICIO_DURACAO = INICIO_FORMATO + 2
INICIO_SHA256 = INICIO_DURACAO + 4
FIM_NOME = INICIO_SHA256 + 32

#: Os codecs — o que se **descodifica**.
CODECS = {
    0x0001: "opus",
    0x0002: "rawvideo-gray",
    0x0003: "matroska",
    # VP8 (F9). E' o codec de video do streaming: faz compensacao de
    # movimento, e e' o que faz a 480p caber no orcamento de 12,8 msg/s
    # (docs/media.md §3.4.2).
    #
    # `rawvideo-gray` (0x0002) fica porque um manifesto antigo pode
    # declara-lo, e o receptor tem de saber o que e'. Nenhum emissor
    # novo o produz.
    0x0004: "vp8",
}

#: Os formatos — o **contentor**.
#:
#: Separados de :data:`CODECS` porque respondem a perguntas
#: diferentes: o codec diz o que se descodifica, o formato diz o que o
#: contentor é. Um vídeo Matroska com Opus dentro tem ambos, e o
#: receptor precisa das duas coisas.
FORMATOS = {
    0x0001: "ogg",
    0x0002: "mp3",
    0x0003: "wav",
    0x0010: "rawvideo",
    0x0020: "mp4",
    0x0021: "matroska",
    0x0022: "webm",
}

#: A extensão → formato. Para o emissor, que tem um caminho e não sabe
#: o que é.
FORMATOS_POR_NOME = {
    "ogg": 0x0001, "oga": 0x0001, "opus": 0x0001,
    "mp3": 0x0002,
    "wav": 0x0003,
    "gray": 0x0010, "raw": 0x0010,
    "mp4": 0x0020, "m4v": 0x0020,
    "mkv": 0x0021,
    "webm": 0x0022,
}


#: O codec que cada formato transporta **por omissão**.
#:
#: Chama-se assim porque o Matroska **não tem** entrada: é o contentor
#: mais genérico que existe, e escolher por ele seria chutar. Um
#: emissor que queira Matroska tem de dizer o codec — e essa recusa é o
#: que impede um vídeo com Opus dentro de ser rotulado de áudio.
FORMATOS_CODEC_PADRAO = {
    0x0001: 0x0001,  # Ogg   → Opus
    0x0002: 0x0001,  # MP3   → Opus
    0x0003: 0x0001,  # WAV   → Opus
    0x0010: 0x0002,  # raw   → rawvideo gray
    # IVF → VP8. E' o **contentor** que o `ffmpeg` escreve no pipe
    # (§8.1), e o que `media/ivf.rs` le para sacar os quadros.
    #
    # O que vai na rede nao e o IVF: e' o quadro, que a UI descodifica
    # com WebCodecs (§8.0.1.1). O IVF e' um contentor de ficheiro e o
    # `ffmpeg` nao emite nada de um pipe; ver a nota longa.
    0x0030: 0x0004,
    0x0020: 0x0003,  # MP4   → Matroska
    0x0022: 0x0003,  # WebM  → Matroska
    # 0x0021 (Matroska) fica de fora de propósito.
}


def numero_de_fatias(total_bytes: int) -> int:
    """Quantas fatias um conteúdo de ``total_bytes`` ocupa.

    **Zero** para um ficheiro vazio, e a coerência é o ponto: um
    conteúdo de zero bytes não tem fatia nenhuma, e um manifesto que
    dissesse «1» faria o receptor esperar uma fatia que nunca chegava.
    """
    if total_bytes == 0:
        return 0
    return -(-total_bytes // TAM_FATIA)  # divisão com arredondamento


def fatiar(conteudo: bytes) -> list[bytes]:
    """Divide o conteúdo em fatias de :data:`TAM_FATIA` bytes."""
    return [conteudo[i : i + TAM_FATIA] for i in range(0, len(conteudo), TAM_FATIA)]


def resumo(conteudo: bytes) -> bytes:
    """O SHA-256 do conteúdo montado.

    ``hashlib`` da biblioteca padrão — a mesma função que o
    ``crypto_core`` de Rust usa, verificada pelos vectors de
    ``tests/vectors/media.json``.
    """
    return hashlib.sha256(conteudo).digest()


def id_da_transferencia(resumo_bytes: bytes) -> str:
    """O ``id`` da transferência: os **primeiros** 16 bytes do resumo,
    em hexadecimal.

    Do resumo e não do nome porque o nome é escolhido por quem envia:
    mudá-lo mudaria o ``id`` sem mudar o conteúdo, e duas
    transferências do mesmo ficheiro pareceriam diferentes.
    """
    if len(resumo_bytes) < 16:
        raise ErroMedia("o resumo tem menos de 16 bytes")
    return resumo_bytes[:16].hex()


@dataclass(frozen=True)
class Manifesto:
    """Um manifesto de transferência, lido ou montado.

    O ``tipo`` é um **campo**, e não se deduz do ``codec`` — e é o
    erro que a implementação Rust apanhou. Sem ele, um vídeo Matroska
    com Opus dentro daria ``tipo = audio``, e a chave seria a errada:
    o receptor abriria o manifesto, leria o codec, escolheria a chave
    de áudio, e não ouviria nenhum sinal de que errou.
    """

    tipo: str
    codec: str
    formato: str
    total_bytes: int
    total_pedacos: int
    bytes_por_pedaco: int
    duracao_s: int
    sha256: bytes
    nome: str

    @classmethod
    def novo(
        cls, tipo: str, codec: str, formato: str, duracao_s: int, nome: str, conteudo: bytes
    ) -> "Manifesto":
        """Monta o manifesto a partir de um conteúdo já lido."""
        if tipo not in TIPOS_POR_CARACTERE.values():
            raise ErroMedia(f"tipo desconhecido: {tipo!r}")
        if codec not in CODECS.values():
            raise ErroMedia(f"codec desconhecido: {codec!r}")
        if formato not in FORMATOS.values():
            raise ErroMedia(f"formato desconhecido: {formato!r}")
        if "\x00" in nome:
            # Um `str` com um zero no meio é um `CString` truncado à
            # espera de acontecer, e passa a um `snprintf` como
            # separador de argumentos.
            raise ErroMedia("o nome tem bytes nulos")

        return cls(
            tipo=tipo,
            codec=codec,
            formato=formato,
            total_bytes=len(conteudo),
            total_pedacos=numero_de_fatias(len(conteudo)),
            bytes_por_pedaco=TAM_FATIA,
            duracao_s=duracao_s,
            sha256=resumo(conteudo),
            nome=nome,
        )

    def validar(self) -> None:
        """Confirma que os números são coerentes uns com os outros."""
        if self.bytes_por_pedaco == 0:
            raise ErroMedia("manifesto incoerente: bytes por pedaço é zero")
        # A mesma condição vista de dois lados. Um ficheiro vazio tem
        # zero fatias, e só isso.
        if (self.total_pedacos == 0) != (self.total_bytes == 0):
            raise ErroMedia(
                "manifesto incoerente: zero fatias com conteúdo, "
                "ou conteúdo sem fatias"
            )
        if numero_de_fatias(self.total_bytes) != self.total_pedacos:
            raise ErroMedia("manifesto incoerente: as fatias não batem com o tamanho")
        if self.bytes_por_pedaco > TAM_MEDIA_MAX:
            raise ErroMedia("manifesto incoerente: a fatia não cabe num envelope")

    def fatias(self, conteudo: bytes) -> list[bytes]:
        """As fatias de um conteúdo que corresponda a este manifesto.

        Confirma o tamanho antes: um conteúdo de outro tamanho daria um
        número de fatias diferente do que o receptor espera, e a
        montagem fecharia com o resumo errado — que é o tipo de falha
        que só aparece na última peça.
        """
        if len(conteudo) != self.total_bytes:
            raise ErroMedia("manifesto incoerente: o conteúdo não tem o tamanho declarado")
        return fatiar(conteudo)

    def corpo(self) -> bytes:
        """O corpo do manifesto, pronto a ser montado numa moldura."""
        corpo = bytearray()
        corpo.append(CARACTERE_POR_TIPO[self.tipo].encode("ascii")[0])
        corpo += self.total_bytes.to_bytes(4, "big")
        corpo += self.total_pedacos.to_bytes(4, "big")
        corpo += self.bytes_por_pedaco.to_bytes(2, "big")
        corpo += _valor_de(CODECS, self.codec).to_bytes(2, "big")
        corpo += _valor_de(FORMATOS, self.formato).to_bytes(2, "big")
        corpo += self.duracao_s.to_bytes(4, "big")
        corpo += self.sha256
        corpo += codificar(self.nome.encode("utf-8")).encode("ascii")
        return bytes(corpo)

    @classmethod
    def de_corpo(cls, corpo: bytes) -> "Manifesto":
        """Lê um manifesto do corpo de uma moldura.

        Fechado por construção, como o lado Rust: valida antes de
        devolver, e nenhuma das leituras pode sair do que o ``DECODE``
        já validou.
        """
        if len(corpo) < TAM_MANIFESTO_FIXO:
            raise ErroMedia(f"manifesto com {len(corpo)} bytes, menos de {TAM_MANIFESTO_FIXO}")

        car_tipo = corpo[0:1].decode("ascii")
        if car_tipo not in TIPOS_POR_CARACTERE:
            raise ErroMedia(f"tipo {car_tipo!r} desconhecido")

        tipo = TIPOS_POR_CARACTERE[car_tipo]
        total_bytes = int.from_bytes(corpo[INICIO_TOTAL:INICIO_PEDACOS], "big")
        total_pedacos = int.from_bytes(corpo[INICIO_PEDACOS:INICIO_BYTES_PEDACO], "big")
        bytes_por_pedaco = int.from_bytes(corpo[INICIO_BYTES_PEDACO:INICIO_CODEC], "big")
        codec = _nome_de(CODECS, int.from_bytes(corpo[INICIO_CODEC:INICIO_FORMATO], "big"), "codec")
        formato = _nome_de(
            FORMATOS, int.from_bytes(corpo[INICIO_FORMATO:INICIO_DURACAO], "big"), "formato"
        )
        duracao_s = int.from_bytes(corpo[INICIO_DURACAO:INICIO_SHA256], "big")
        sha256 = corpo[INICIO_SHA256:FIM_NOME]

        if len(sha256) != 32:
            raise ErroMedia("manifesto incoerente: o resumo não tem 32 bytes")

        nome = descodificar(corpo[FIM_NOME:].decode("ascii", "replace"))
        try:
            nome = nome.decode("utf-8")
        except UnicodeDecodeError as erro:
            raise ErroMedia("o nome do ficheiro não é UTF-8") from erro
        if "\x00" in nome:
            raise ErroMedia("o nome do ficheiro tem bytes nulos")

        m = cls(
            tipo=tipo,
            codec=codec,
            formato=formato,
            total_bytes=total_bytes,
            total_pedacos=total_pedacos,
            bytes_por_pedaco=bytes_por_pedaco,
            duracao_s=duracao_s,
            sha256=sha256,
            nome=nome,
        )
        m.validar()
        return m

    @property
    def id(self) -> str:
        """O identificador da transferência, em hexadecimal."""
        return id_da_transferencia(self.sha256)


def _valor_de(tabela: dict, nome: str) -> int:
    """O valor de um nome numa tabela, ou erro com o nome da tabela."""
    for valor, chave in tabela.items():
        if chave == nome:
            return valor
    raise ErroMedia(f"desconhecido: {nome!r}")


def _nome_de(tabela: dict, valor: int, o_que: str) -> str:
    """O nome de um valor numa tabela, ou erro que diz o que faltou.

    A mensagem nomeia a tabela (``codec`` ou ``formato``) porque as duas
    existem e um erro genérico obrigaria a abrir as duas para saber qual
    falhou.
    """
    if valor not in tabela:
        raise ErroMedia(f"{o_que} 0x{valor:04x} desconhecido")
    return tabela[valor]


class Montador:
    """Junta as fatias recebidas e confirma o resumo.

    O resumo é a **única** condição de aceitação, porque o relay é
    *entrega única, sem duplicados* e **não há retransmissão**
    (``docs/relay.md`` §Regras de retenção): o receptor precisa de saber
    se a montagem está completa **antes** de a mostrar. Um leitor que
    toca um ficheiro truncado não sabe — e o utilizador vê um vídeo
    que acaba a meio sem porquê.

    Recebe pelo ``seq`` e monta pela posição: a ordem de chegada não
    entra em nenhum dos dois.
    """

    def __init__(self, manifesto: Manifesto) -> None:
        manifesto.validar()
        self._manifesto = manifesto
        self._fatias: list[bytes | None] = [None] * manifesto.total_pedacos

    @property
    def id(self) -> str:
        return self._manifesto.id

    def faltam(self) -> int:
        """Quantas fatias faltam."""
        return sum(1 for f in self._fatias if f is None)

    def indices_em_falta(self) -> list[int]:
        """Os índices das fatias que faltam, por ordem.

        É o que a resposta de retoma lista: *«9f3a: faltam 7 42 118»*.
        """
        return [i for i, f in enumerate(self._fatias) if f is None]

    def receber(self, seq: int, dados: bytes) -> None:
        """Recebe uma fatia pelo seu ``seq``.

        O ``seq`` é o **da rede**, e conta o manifesto como ``0`` — para
        que a sequência do fluxo seja a mesma nas duas peças e não haja
        dois ``0``. A primeira fatia é portanto ``seq=1``, e é aqui que
        se resolve a diferença entre «posição no ficheiro» e «posição na
        rede».

        Uma fatia repetida **fica**: juntar as duas daria um ficheiro
        mais comprido do que o ``sha256`` diz, e o receptor acabaria com
        um erro de resumo em vez de ignorar um reenvio. A segunda
        leitura é a mais tolerante e a correcta.
        """
        if seq < 1:
            raise ErroMedia(f"seq {seq}: o manifesto é o 0, as fatias começam no 1")
        indice = seq - 1
        if indice >= len(self._fatias):
            raise ErroMedia(f"fatia {seq} além da última")
        if len(dados) > self._manifesto.bytes_por_pedaco:
            raise ErroMedia(
                f"fatia de {len(dados)} bytes, acima das "
                f"{self._manifesto.bytes_por_pedaco} declaradas"
            )
        self._fatias[indice] = dados

    def montar(self) -> bytes | None:
        """Junta e verifica. ``None`` enquanto faltar alguma.

        ``None`` é o caso comum de uma transferência a meio, e não é um
        erro. Um ficheiro vazio tem **zero** fatias e monta de
        imediato.
        """
        if self.faltam():
            return None

        juntado = b"".join(f for f in self._fatias if f is not None)
        if resumo(juntado) != self._manifesto.sha256:
            raise ErroMedia("o SHA-256 do conteúdo não bate com o manifesto")
        return juntado


def moldura_de_pedaco(manifesto: Manifesto, stream: int, seq: int, pedaco: bytes) -> str:
    """O texto de uma fatia, pronto a ser cifrado."""
    return Moldura(
        tipo=manifesto.tipo,
        modo="nao-streaming",
        stream=stream,
        seq=seq,
        clock_ms=0,
        corpo=pedaco,
    ).texto


def moldura_de_manifesto(manifesto: Manifesto, stream: int, seq: int) -> str:
    """O texto do manifesto, que leva a flag."""
    return Moldura(
        tipo=manifesto.tipo,
        modo="nao-streaming",
        stream=stream,
        seq=seq,
        clock_ms=0,
        corpo=manifesto.corpo(),
        flags=FLAGS["manifesto"],
    ).texto


# ---------------------------------------------------------------------
# Orquestração: enviar uma transferência
# ---------------------------------------------------------------------


def chaves_por_tipo(chaves_amizade, publica_emissor: bytes) -> dict[str, "Chaves"]:
    """As :class:`Chaves` do pipeline para cada tipo de conteúdo.

    É o único sítio onde a derivação por tipo se liga ao pipeline: o
    ``k1`` sai de :func:`derivar_k1_por_tipo` e o ``k5``/``k9`` entram
    **inalterados** — porque a K1 é a primeira camada e é AEAD, uma
    etiqueta errada morre ali.

    ## Porquê K5 e K9 não são derivadas

    Porque a K1 é a **primeira** camada. Quem tem a K5 e a K9 e a K1
    errada falha no Poly1305 da primeira decrypt, e esse custo é uma
    verificação de etiqueta — não uma descifra completa. Derivar mais
    camadas custaria uma KDF por tipo sem acrescentar nada que a
    etiqueta não faça já.

    ## E o texto não é uma linha deste dicionário por acaso

    O texto entra com o ``k1`` de longa duração que já tinha, e é por
    isso que o texto continua byte-a-byte igual ao que produzia antes
    de existir media (``docs/media.md`` §1.2). Qualquer tipo novo
    entra pelo mesmo caminho — uma linha nova, derivada, com o mesmo
    ``k5``/``k9``.

    :param chaves_amizade: as seis chaves de :class:`ChavesAmizade`.
    :param publica_emissor: a identidade de quem envia, de 32 bytes.
    :returns: um dicionário com ``texto`` e um por tipo de
        :data:`TIPOS_DE_CHAVE`.

    .. note::
       **Não** há ``nonce1`` nos argumentos, e a sua ausência é o que
       permite a **passagem única**. Ver a nota longa de
       :func:`messenger.amizade.derivar_k1_por_tipo`: com o nonce no
       ``salt``, o emissor teria de cifrar duas vezes — e a segunda
       gera um nonce novo, que não é o que a chave usou.
    """
    derivadas = chaves_media(
        chaves_amizade.k1,
        chaves_amizade.k5_proprio,
        chaves_amizade.k5_par,
        publica_emissor,
    )
    return {
        nome: Chaves(derivadas[nome], chaves_amizade.k5_proprio, chaves_amizade.k9_proprio)
        for nome in derivadas
    }


def _nonce_do_envelope(envelope: bytes) -> bytes:
    """O ``nonce1`` de um envelope já cifrado.

    O envelope começa com a versão (1 byte) e o ``nonce1`` vem a seguir
    (``docs/message_format.md``).

    ## Já não é usado para derivar a chave

    Numa versão anterior da orquestração, este era o ``salt`` do HKDF —
    e ver :func:`messenger.amizade.derivar_k1_por_tipo` para o que
    estava errado com isso.

    Fica porque é **diagnóstico**: num ``MEDIA_ESTADO`` ou num relatório
    de problema, «qual era o ``nonce1`` deste envelope?» é uma pergunta
    que se responde disto, e respondê-la de outra maneira seria
    inventar um valor.
    """
    if len(envelope) < 13:
        raise ErroMedia(f"envelope de {len(envelope)} bytes, sem nonce1 legível")
    return envelope[1:13]


def enviar_transferencia(
    pipeline,
    chaves_amizade,
    remetente,
    cliente,
    conteudo: bytes,
    nome: str,
    *,
    duracao_s: int = 0,
    stream: int = 1,
) -> Manifesto:
    """Envia um ficheiro como uma sequência de mensagens ``CHAT``.

    Um manifesto e ``N`` fatias, cada uma um ``ENCODE`` e um ``ENVIAR``
    normais. Nenhum comando novo, nenhum tipo de frame novo — a
    ``docs/media.md`` §Introdução.

    ## Uma passagem, e não duas

    A primeira versão cifrava **duas vezes** por mensagem: uma para
    obter um envelope com um ``nonce1`` conhecido, e outra com a chave
    derivada desse nonce. E não funciona — o ``nonce1`` da segunda
    ``ENCODE`` é **novo**, e o envelope que vai para a rede tem esse
    nonce novo, não o que a chave usou. O receptor deriva a partir do
    envelope que **recebeu** e a cifra falha.

    A correcção está em :func:`messenger.amizade.derivar_k1_por_tipo`:
    o ``salt`` é o rótulo de domínio e o tipo, não o nonce. Os dois
    lados derivam a mesma chave do que **já sabem ambos**, e uma
    passagem resolve. São **metade** das operações de cifra.

    ## O que devolve

    O :class:`Manifesto`, que o emissor guarda para poder responder a
    uma retoma sem reler o ficheiro.

    :raises ErroMedia: se o conteúdo não couber num ``u32`` — que é o
        maior ficheiro que o manifesto descreve.
    """
    formato_val = FORMATOS_POR_NOME.get(nome.rsplit(".", 1)[-1].lower() if "." in nome else "")
    if formato_val is None:
        raise ErroMedia(f"não sei o formato de {nome!r} — a extensão não está na tabela")
    formato = _nome_de(FORMATOS, formato_val, "formato")

    codec_val = FORMATOS_CODEC_PADRAO.get(formato_val)
    if codec_val is None:
        raise ErroMedia(
            f"o formato {formato!r} não decide o codec — diz qual é explicitamente"
        )
    codec = CODECS[codec_val]
    tipo = "audio" if codec == "opus" else "video"

    manifesto = Manifesto.novo(tipo, codec, formato, duracao_s, nome, conteudo)
    manifesto.validar()

    def cifrar_e_enviar(texto: str, tipo_conteudo: str) -> bytes:
        """Cifra uma moldura com a chave do tipo e envia-a.

        **Uma** passagem: a chave deriva do par de chaves e do tipo, que
        os dois lados já conhecem, e o ``ENCODE`` produz o envelope. Não
        há nada a preparar antes.

        A primeira versão fazia duas passagens — uma para obter um
        envelope com um ``nonce1`` conhecido, e outra para cifrar com a
        chave derivada desse nonce. E não funcionava: a segunda
        ``ENCODE`` gera um ``nonce1`` **novo**, e o envelope que vai
        para a rede tem esse nonce novo, não o que a chave usou. O
        receptor deriva a partir do envelope que **recebeu** e a cifra
        falha — ``0x05 DecifragemFalhou``, que não aponta para a chave.

        Ver a nota longa de :func:`messenger.amizade.derivar_k1_por_tipo`.
        """
        chaves = chaves_por_tipo(chaves_amizade, remetente.pub)[tipo_conteudo]
        envelope = pipeline.cifrar(texto, chaves, remetente)
        # `media=True`: o streaming gasta a janela de rate-limit da
        # media e nao a do texto. Sem isto uma chamada a 480p contaria
        # como conversa e batia no tecto de 128 sem chegar a 30 fps.
        cliente.enviar(envelope, media=True)
        return envelope

    cifrar_e_enviar(moldura_de_manifesto(manifesto, stream, 0), tipo)
    for seq, pedaco in enumerate(manifesto.fatias(conteudo), start=1):
        cifrar_e_enviar(moldura_de_pedaco(manifesto, stream, seq, pedaco), tipo)

    return manifesto


def texto_de_retoma(transferencia_id: str, indices: list[int]) -> str:
    """A mensagem de texto que pede as fatias que faltam.

    ## Porque uma mensagem de **texto** e não um comando de media
    Porque o relay é *entrega única, sem duplicados* e **não há
    retransmissão** (``docs/relay.md`` §Regras de retenção): uma fatia
    perdida é uma fatia perdida, e alguém tem de a pedir.

    E a resposta é **uma pessoa a dizer o que falta** — que é o que um
    messenger já faz com um «não recebi a mensagem». Um comando
    dedicated seria um tipo de frame novo, uma rota nova e um caminho
    novo pela rede, e a **rede não pode ver que é media**.

    :returns: o texto a cifrar e enviar.
    """
    lista = " ".join(str(i) for i in indices)
    return f"{transferencia_id}: faltam {lista}"


def interpretar_retoma(texto: str) -> tuple[str, list[int]] | None:
    """Lê uma resposta de retoma, ou ``None`` se não for uma.

    Devolve ``None`` em vez de levantar para uma mensagem que não é de
    retoma: o texto chega por um caminho que é o mesmo de todas as
    mensagens de uma conversa, e levantar por causa de uma frase
    comum seria trocar uma mensagem por um erro.

    :returns: ``(id, índices)`` ou ``None``.
    """
    if ": faltam " not in texto:
        return None
    id_, _, resto = texto.partition(": faltam ")
    id_ = id_.strip()
    if len(id_) != 32 or any(c not in "0123456789abcdef" for c in id_):
        return None
    indices: list[int] = []
    for pedaco in resto.split():
        if not pedaco.isdigit():
            return None
        indices.append(int(pedaco))
    return id_, indices


# ---------------------------------------------------------------------
# As chaves por tipo
# ---------------------------------------------------------------------


def chaves_media(
    k_texto: bytes,
    k_emissor: bytes,
    k_recetor: bytes,
    publica_emissor: bytes,
) -> dict[str, bytes]:
    """As chaves K1 de media e de controlo, por nome de tipo.

    O texto **não** entra no dicionário: usa a ``k_texto`` de longa
    duração que o sistema já usava, e é por isso que o texto continua
    byte-a-byte igual ao que produzia antes de existir media.

    ## Os tipos que entram

    Todos os de :data:`TIPOS_DE_CHAVE` — os dois de media
    (``audio``, ``video``) e os dois de controlo (``call``,
    ``ficheiro``). Os de controlo não têm moldura, mas têm chave: é a
    chave que garante que só o par abre o envelope de um convite de
    chamada ou de um ficheiro, e que a rede não o distingue de texto.

    Sem ``nonce1``, por causa da razão de
    :func:`messenger.amizade.derivar_k1_por_tipo`.
    """
    derivadas = {
        nome: derivar_k1_por_tipo(k_emissor, k_recetor, publica_emissor, byte_tipo)
        for nome, byte_tipo in TIPOS_DE_CHAVE.items()
    }
    derivadas["texto"] = k_texto
    return derivadas


def order_de_sondagem() -> tuple[str, ...]:
    """A ordem em que as chaves são testadas, texto incluído.

    O texto primeiro porque é o caso comum: uma conversa normal custa
    **uma** verificação.

    E os tipos de media a seguir, e o `call` e o `ficheiro` no fim,
    porque são raros e porque a ordem é o que torna o custo da
    sondagem previsível: uma conversa custa uma verificação, uma
    chamada custa quatro.

    ## Porque a ordem é exportada do Rust

    Porque duas listas divergentes dariam um sistema que abre texto num
    lado e áudio no outro — e o sintoma é um erro de descifragem que não
    menciona tipos. O mesmo nome de ordem nos dois lados é o que impede
    isso, e o Rust é a fonte.
    """
    return ("texto", *TIPOS_DE_CHAVE)


def sondar(chaves: dict[str, bytes], tentar) -> tuple[str, bytes]:
    """Descobre o tipo de um envelope, tentando as chaves por ordem.

    ``tentar`` recebe uma chave e devolve o plaintext se a etiqueta da
    K1 verificar, ou ``None``. É a mesma separação de responsabilidades
    que há no Rust: a validação da assinatura e do anti-replay é do
    pipeline, e uma sondagem que a saltasse desceria a dados não
    autenticados.

    :raises ErroMedia: se nenhuma das chaves abrir o envelope.
    """
    for nome in order_de_sondagem():
        if nome == "texto":
            chave = chaves.get("texto")
            if chave is None:
                continue
        else:
            chave = chaves[nome]
        texto = tentar(chave)
        if texto is not None:
            return nome, texto
    raise ErroMedia("nenhuma das chaves de media abriu este envelope")


def montar_com_chaves(
    chaves: dict[str, bytes], tipo: str, modo: str, **campos
) -> str:
    """Atalho de conveniência: monta a moldura já com a chave do tipo.

    Existe para o chamador não ter de ir buscar a chave ao dicionário e
    depois passar a moldura — que é o par de operações que se esquece
    metade das vezes.
    """
    if tipo not in TIPOS:
        raise ErroMedia(f"tipo desconhecido: {tipo!r}")
    if tipo not in chaves:
        raise ErroMedia(f"não há chave derivada para o tipo {tipo!r}")
    return Moldura(tipo=tipo, modo=modo, **campos).texto


# ---------------------------------------------------------------------
# Orquestração: receber uma transferência
# ---------------------------------------------------------------------


class Receptor:
    """Acumula as fatias de uma transferência e escreve-as na loja.

    ## Porque isto é uma classe e não uma função

    Uma transferência chega em 205 mensagens — um manifesto e 204 fatias
    — e em ordem **não garantida**: um relay pode reordenar. O estado de
    «qual era o manifesto» e «quantas faltam» não cabe num argumento de
    função, e recriá-lo por fatia seria recriar o montador 204 vezes.

    ## Como se sabe a que transferência pertence uma fatia

    A moldura de uma fatia **não tem** `id` de transferência: só tem
    `stream`. E isso é uma escolha, não uma falta — pôr 16 bytes de `id`
    em cada uma das 204 fatias acrescentaria 44% ao manifesto e não
   hia nada que o `stream` não fizesse.

    O `stream` é monotónico por transferência (§2.3), e tanto o
    manifesto como as fatias o trazem. Por isso o mapeamento é
    `stream → transferência`, e é o **manifesto** que o estabelece. Uma
    fatia que chegue antes fica à espera, e é o manifesto que a vai
    buscar.

    ## E o que se escreve na loja

    **A moldura tal como chegou**, não uma reconstruída. Reconstruir
    significaria re-sintetizar a base-64 e os campos numéricos, e o
    `MEDIA_REPRODUZIR` monta a partir do que está gravado — o que
    devolve é o que o emissor mandou, byte a byte.

    O manifesto vai também para a loja, como linha de `mensagens` com a
    sua `transferencia_id`. É ele que o `daemon` lê para saber o resumo
    e o número de fatias, e tê-lo só na tabela `transferencias` obrigaria
    o daemon a duplicar o formato do manifesto noutro sítio.
    """

    def __init__(self, loja, conversa_id: str) -> None:
        self._loja = loja
        self._conversa = conversa_id
        #: ``stream`` → ``mediaId``, estabelecido pelo manifesto.
        self._por_stream: dict[int, str] = {}
        #: ``manifestos`` por ``mediaId``.
        self._manifestos: dict[str, Manifesto] = {}
        #: Montadores por ``mediaId``, enquanto a transferência está a meio.
        self._montadores: dict[str, Montador] = {}
        #: ``stream`` → textos de fatia que chegaram antes do manifesto.
        self._esperam: dict[int, list[str]] = {}

    def receber(self, texto: str) -> str | None:
        """Trata uma moldura recebida e devolve o ``mediaId``.

        :param texto: o plaintext que o ``DECODE`` devolveu.
        :returns: o ``transferencia_id`` desta moldura, ou ``None`` se o
            texto não for media — que é o caso comum, e **não** é um
            erro. Uma conversa normal nunca chega aqui com media.
        """
        if not texto.startswith("ONX1"):
            return None

        aberta = abrir(texto)
        if aberta.modo != "nao-streaming":
            # O streaming é F7. Uma moldura de streaming que chegue aqui
            # é ignorada em vez de falhar: a mesma pessoa pode estar a
            # usar as duas coisas, e uma linha a meio não pode estragar
            # o que já estava guardado.
            return None

        if aberta.flags & FLAGS["manifesto"]:
            return self._receber_manifesto(aberta, texto)
        return self._receber_pedaco(aberta, texto)

    # -- manifesto ---------------------------------------------------------

    def _receber_manifesto(self, aberta: Aberta, texto: str) -> str:
        manifesto = Manifesto.de_corpo(aberta.corpo)
        manifesto.validar()
        id = manifesto.id

        self._loja.registar_transferencia(
            self._conversa,
            "them",
            id=id,
            tipo=manifesto.tipo,
            codec=manifesto.codec,
            formato=manifesto.formato,
            nome=manifesto.nome,
            total_bytes=manifesto.total_bytes,
            total_pedacos=manifesto.total_pedacos,
            bytes_por_pedaco=manifesto.bytes_por_pedaco,
            duracao_s=manifesto.duracao_s,
            sha256=manifesto.sha256,
        )

        self._manifestos[id] = manifesto
        # O `stream` que o **manifesto** traz. E não um derivado do
        # `id`: o manifesto já diz qual é, e inventar um derivado aqui
        # não bateria com o que o emissor pôs nas fatias.
        self._por_stream[aberta.stream] = id
        # Um montador novo de propósito. O anterior pode ter fatias que
        # o emissor já não vai reenviar, e o manifesto reenviado traz o
        # estado **pretendido**, não o que se sabe.
        self._montadores[id] = Montador(manifesto)

        # A linha do manifesto na loja, para o `MEDIA_REPRODUZIR` ter o
        # resumo e os números sem duplicar o formato noutro sítio.
        self._gravar(id, texto)

        # As fatias que chegaram primeiro passam pelo caminho normal,
        # e não por um atalho que só as grave: um atalho guardaria o
        # texto sem o dar ao montador, e a transferência ficaria «a
        # meio» com todas as linhas na loja e zero assembly. É o
        # género de bug em que tudo parece estar bem e o vídeo nunca
        # toca.
        for texto_esperado in self._esperam.pop(aberta.stream, []):
            self._receber_pedaco(abrir(texto_esperado), texto_esperado)

        return id

    # -- fatia -------------------------------------------------------------

    def _receber_pedaco(self, aberta: Aberta, texto: str) -> str | None:
        id = self._por_stream.get(aberta.stream)
        if id is None:
            # O manifesto ainda não chegou. Guarda-se e espera-se: uma
            # linha sem `transferencia_id` ficaria órfã, e a §4.5 da
            # spec diz que não pode haver órfãs.
            self._esperam.setdefault(aberta.stream, []).append(texto)
            return None

        manifesto = self._manifestos[id]
        montador = self._montadores.setdefault(id, Montador(manifesto))
        montador.receber(aberta.seq, aberta.corpo)
        self._gravar(id, texto)
        return id

    # -- escrita -----------------------------------------------------------

    def _gravar(self, id: str, texto: str) -> None:
        self._loja.adicionar_mensagem(
            self._conversa,
            "them",
            texto,
            transferencia_id=id,
        )
        self._talvez_completar(id)

    def _talvez_completar(self, id: str) -> None:
        """Marca a transferência como completa, se e quando for o caso.

        Chamar a isto **depois de cada linha**, e não só depois das
        fatias, é o que faz um ficheiro de zero bytes ficar completo. O
        manifesto sozinho é a transferência inteira nesse caso, e
        verificar só depois da primeira fatia deixava-o «a meio» para
        sempre — com «0 de 0», que é um spinner que nunca acaba.
        """
        montador = self._montadores.get(id)
        if montador is None or montador.faltam() != 0:
            return

        conteudo = montador.montar()
        # Só depois do resumo bater. Sem esta conferência, `completo`
        # seria uma promessa, e a interface ofereceria para tocar um
        # ficheiro truncado — que é o que §4.6 proíbe.
        if conteudo is not None:
            self._loja.marcar_transferencia_completa(id)
            self._montadores.pop(id, None)

    def em_curso(self) -> dict[str, int]:
        """As transferências a meio, e quantas fatias lhes faltam.

        É o que a interface mostra como «a chegar, 42 de 204», e vem
        daqui e não da loja porque a loja não sabe o que está a
        repetir-se.
        """
        return {
            id: montador.faltam()
            for id, montador in self._montadores.items()
        }


# =====================================================================
# O receptor de streaming: bytes de VP8 a partir de um comando IPC
# =====================================================================


class DescodificadorVideo:
    """Descodifica VP8 para pixéis, com o `ffmpeg` do sistema.

    ## Porquê isto está aqui e não no daemon

    O daemon embebido liga o `libvpx` para **codificar**, e ligar também
    para descodificar duplicaria o executável por uma coisa que o
    emissor nunca precisa: um emissor nunca descodifica. O daemon
    transporta bytes; quem os descodifica é o receptor, que tem um
    `ffmpeg` completo no sistema.

    E é a divisão certa pelo motivo de sempre: o daemon é o que tem
    menos confiança e o menos contexto, e dar-lhe menos para fazer é
    dar-lhe menos em que falhar.

    ## Porquê o VP8 não é auto-suficiente

    O VP8 tem frames-I (uma imagem completa) e frames-P (um delta
    contra os anteriores). Um descodificador que descodificasse um
    frame-P sem os anteriores daria uma imagem deslocada — e o sintoma
    seria «a imagem às vezes treme», que não parece um bug de
    descodificação.

    Por isso `descodificar` mantém o estado do descodificador vivo
    entre quadros e **descodifica sempre a partir do último I**, se
    algum se perder.

    ## O `ffmpeg` do sistema, e não o embebido

    Porque o receptor é uma aplicação normal, com as suas
    dependências, e o `ffmpeg` do sistema já sabe descodificar VP8. O
    embebido é mínimo por desenho (§7.1) e não tem os descodadores.

    Se não houver `ffmpeg`, isto recusa com uma mensagem que diz o que
    instalar — e não finge que a chamada funciona.
    """

    #: Os argumentos que descodificam um frame de VP8 para pixéis.
    #:
    #: `-f ivf` porque é IVF que chega; `-pix_fmt gray` porque o
    #: receptor quer **luma**, que é o que se mostra e é metade dos
    #: bytes a mover. O VP8 descodifica para YUV420 e o `ffmpeg`
    #: converte — e a conversão está aqui, no receptor, porque é uma
    #: escolha de apresentação e não de transporte.
    ARGS = (
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "ivf",
        "-i",
        "pipe:0",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "gray",
        "pipe:1",
    )

    def __init__(self, largura: int, altura: int, ffmpeg: str | None = None) -> None:
        """
        Args:
            largura: largura em pixels.
            altura: altura em pixels.
            ffmpeg: caminho do executável. ``None`` usa o ``ffmpeg`` do
                sistema. Um caminho explícito existe porque uma
                instalação do OnyxChat pode trazer o seu — o mesmo
                `ffplay` e o mesmo `ffmpeg` que o `UI/tools` embute.
        """
        self._largura = largura
        self._altura = altura
        self._ffmpeg = ffmpeg
        self._processo: subprocess.Popen | None = None
        self._erro: str | None = None

    @property
    def dimensoes(self) -> tuple[int, int]:
        """As dimensões esperadas, em pixels."""
        return self._largura, self._altura

    def _arrancar(self) -> subprocess.Popen:
        """Arranca o `ffmpeg`, ou explica porquê que não arrancou.

        ## Porque se arranca **uma vez** e se escreve sempre no mesmo

        Um descodificador por quadro custa um `fork` e um `exec` por
        quadro — a 30 fps são 30 processos por segundo, e a latência
        desse `fork` entra no atraso de apresentação. O processo fica
        vivo e recebe os bytes por um pipe, que é o que o `ffmpeg`
        espera para um stream.

        E é um stream **IVF** que o `ffmpeg` quer: por isso o
        descodificador reconstrói o contentor à volta dos bytes que
        chegam, com o cabeçalho uma vez e um quadro de cada vez.
        """
        if self._processo is not None:
            return self._processo
        try:
            self._processo = subprocess.Popen(  # noqa: S603
                [self._ffmpeg or "ffmpeg", *self.ARGS],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                bufsize=0,
            )
        except FileNotFoundError:
            self._erro = (
                "não há `ffmpeg` no sistema, e sem ele não há vídeo: "
                "instale o ffmpeg (Debian/Ubuntu: `sudo apt install "
                "ffmpeg`; Fedora: `sudo dnf install ffmpeg`)"
            )
            raise VideoDescodificacaoIndisponivel(self._erro) from None
        except OSError as e:
            self._erro = f"o `ffmpeg` não arrancou: {e}"
            raise VideoDescodificacaoIndisponivel(self._erro) from None
        return self._processo

    def descodificar(self, quadro: "QuadroVideo") -> bytes:
        """Descodifica um quadro de VP8 para pixéis de luma.

        Args:
            quadro: o que ``MEDIA_LER`` devolveu.

        Returns:
            ``largura * altura`` bytes, um por pixel.

        Raises:
            VideoDescodificacaoIndisponivel: se não houver `ffmpeg`, ou
                se o `ffmpeg` morrer.
            VideoQuadroInvalido: se o quadro não couber na geometria
                declarada.
        """
        if not quadro.dados:
            raise VideoQuadroInvalido("quadro de VP8 sem bytes")

        proc = self._arrancar()
        assert proc.stdin is not None  # noqa: S101 — garantido por Popen
        assert proc.stdout is not None  # noqa: S101

        cabecalho = self._cabeçalho_ivf()
        try:
            proc.stdin.write(cabecalho if self._primeiro else b"")
            proc.stdin.write(
                len(quadro.dados).to_bytes(4, "little")
                + quadro.clock_ms.to_bytes(8, "little", signed=True)
                + quadro.dados
            )
            proc.stdin.flush()
        except (BrokenPipeError, ValueError) as e:
            # O `ffmpeg` morreu: um pipe fechado é um `BrokenPipeError`
            # ao escrever, e não uma excepção do `ffmpeg` — que já não
            # existe para dizer nada.
            self._processo = None
            raise VideoDescodificacaoIndisponivel(
                f"o `ffmpeg` de descodificação morreu: {e}"
            ) from e

        esperado = self._largura * self._altura
        saida = proc.stdout.read(esperado)
        if len(saida) != esperado:
            # O `stderr` é a única pista: um pipe não diz porquê.
            # Lê-se não-bloqueante porque o `ffmpeg` pode não ter saido.
            diagnostico = ""
            if proc.stderr is not None:
                try:
                    os.set_blocking(proc.stderr.fileno(), False)
                    diagnostico = (proc.stderr.read() or b"").decode(
                        "utf-8", "replace"
                    )[:200]
                except OSError:
                    diagnostico = ""
            self._processo = None
            raise VideoDescodificacaoIndisponivel(
                f"o `ffmpeg` devolveu {len(saida)} bytes, e esperavam-se {esperado}"
                + (f": {diagnostico.strip()}" if diagnostico.strip() else "")
            )
        self._primeiro = False
        return saida

    _primeiro = True

    def _cabeçalho_ivf(self) -> bytes:
        """O cabeçalho IVF de 32 bytes, escrito uma vez.

        O `ffmpeg` precisa dele para saber a geometria e o codec. Sem
        ele diz «Invalid data found when processing input», que é uma
        mensagem sobre o que ele encontrou e não sobre o que faltava.
        """
        return (
            b"DKIF"
            # O byte 5 é o tamanho do cabeçalho **extra**, e é 0. Um
            # IVF tem os 32 bytes todos aqui, e pôr 32 diz ao `ffmpeg`
            # que há mais 32 bytes a ler — que não há. O sintoma é
            # «Invalid frame rate», porque o desvio de dois bytes atira
            # o `fps` para o sítio errado.
            + bytes([0, 0])
            + b"\x20\x00"  # reservado
            + b"VP80"
            + self._largura.to_bytes(2, "little")
            + self._altura.to_bytes(2, "little")
            + (30).to_bytes(4, "little")  # fps
            # `n_frames` a 1, como o `ffmpeg` escreve. O receptor
            # descodifica até o fim, e o valor não é lido — mas um
            # cabeçalho diferente do que o `ffmpeg` escreve é um
            # caminho para o `ffmpeg` recusar a entrada.
            + (1).to_bytes(4, "little")
            + b"\x5a\x00\x00\x00\x00\x00\x00\x00"  # reservado
        )

    def fechar(self) -> None:
        """Fecha o `ffmpeg` e liberta o processo.

        Idempotente, e um `close()` que levanta numa segunda chamada é
        pior do que um que não levanta: quem fecha um descodificador
        está a limpar, e um erro ao limpar é ruído.
        """
        proc, self._processo = self._processo, None
        if proc is None:
            return
        for fluxo in (proc.stdin, proc.stdout, proc.stderr):
            if fluxo is not None:
                try:
                    fluxo.close()
                except OSError:
                    pass
        try:
            proc.terminate()
            proc.wait(timeout=2)
        except (OSError, subprocess.TimeoutExpired):
            try:
                proc.kill()
            except OSError:
                pass

    def __enter__(self) -> "DescodificadorVideo":
        return self

    def __exit__(self, *_exc: object) -> None:
        self.fechar()


class VideoDescodificacaoIndisponivel(ErroMedia):
    """Não há descodificador de vídeo disponível.

    Não é um erro detransporte: é o sistema sem `ffmpeg`. A distinção é o que
    permite à UI dizer «instala o ffmpeg» em vez de «a chamada falhou».
    """


class VideoQuadroInvalido(ErroMedia):
    """Um quadro que não pode virar imagem.

    Leva o motivo porque «quadro inválido» sem dizer qual é a condição
    obriga a pôr pontos de paragem para descobrir.
    """


# =====================================================================
# O emissor de streaming: do `ffmpeg` para a rede
# =====================================================================


class EmissorStreaming:
    """Lê quadros ao daemon, cifra e envia como mensagens ``CHAT``.

    ## O que sai daqui

    Uma **moldura de streaming** por quadro, com o corpo em base-64 do
    VP8. O mesmo `ENCODE` e o mesmo `ENVIAR` de sempre — nenhum
    comando novo, nenhum tipo de frame novo. O que muda é que o
    `ENVIAR` leva ``media=True`` e por isso gasta a janela de
    rate-limit da media (§3.4.3).

    ## O que **não** vai

    **O contentor IVF.** Vai o **quadro**, que é o que o `VideoDecoder`
    da UI consome. O IVF é o contentor que o `ffmpeg` escreve no pipe e
    que `media/ivf.rs` lê para sacar os quadros — e é um contentor de
    ficheiro, que o `ffmpeg` receptor não emite de um pipe
    (§8.0.1.1).

    ## Porque isto é um `for` e não uma `thread`

    Porque a leitura de um quadro **é** o ritmo da chamada. Uma thread
    que lê e enfileira quadros devolveria o controlo à thread principal,
    e a fila cresceria sem limite quando a rede está lenta — que é a
    razão de o anel do daemon existir e de este lado ser simples.

    Quem decide a velocidade de envio é o `MEDIA_LER`, que bloqueia até
    haver um quadro.
    """

    #: A geometria de omissão, a mesma que a spec sugere.
    LARGURA = 854
    ALTURA = 480
    FPS = 30
    AUDIO_KBPS = 32
    VIDEO_KBPS = 300

    def __init__(
        self,
        pipeline,
        chaves_amizade,
        remetente,
        cliente,
        *,
        stream: int = 1,
    ) -> None:
        """
        :param pipeline: o :mod:`messenger.pipeline`, para cifrar.
        :param chaves_amizade: as seis chaves da amizade.
        :param remetente: o :class:`Remetente`.
        :param cliente: o :class:`ClienteIpc`, para `MEDIA_CAPTURAR` e
            `MEDIA_LER`.
        :param stream: o número de stream, que é o do preâmbulo.
        """
        self._pipeline = pipeline
        self._chaves = chaves_por_tipo(chaves_amizade, remetente.pub)
        self._remetente = remetente
        self._cliente = cliente
        self._stream = stream

        #: Quantos quadros já foram enviados. É o `seq` do preâmbulo, e
        #: é o que permite ao receptor detectar uma perda sem pedir
        #: nada.
        self.enviados = 0

        #: Quantos pacotes de áudio já foram enviados.
        self.audios = 0

        #: O limitador da §8.0, e a razão de o emissor o ter.
        #:
        #: O `ffmpeg` escreve áudio e vídeo por descritores separados e
        #: não os acopla. Sem isto, o áudio de uma rajada entra todo
        #: antes do vídeo da rajada correspondente — e o receptor tem de
        #: descartar, o que se ouve.
        self.limitador = LimitadorAudio()

        #: Os milissegundos que um quadro de vídeo representa.
        self._ms_por_quadro = 1000 // self.FPS

        #: Os milissegundos de **áudio** já enviados.
        #:
        #: É o relógio do canal de áudio (§5.3), e é separado do
        #: `clock_ms` do vídeo porque os dois avançam por taxas
        #: diferentes — os dois duram o mesmo tempo (§3.4.2) mas nenhum
        #: deles é a medida do outro.
        self._ms_audio = 0

        #: Os pacotes de Opus à espera de irem na mesma mensagem.
        #:
        #: É a agregação da §8.0, e sem ela a chamada não passa do
        #: limite de taxa: 50 pacotes de 20 ms por segundo são 500
        #: mensagens numa janela de 10 s, e o limite de media é 512 —
        #: com o vídeo a pedir 300 dessas, não havia margem nenhuma.
        #:
        #: A `deque` e não uma lista porque a operação do fim é
        #: ``clear()`` e a lista não a faz em tempo constante; e porque
        #: o conteúdo nunca é lido pacote a pacote depois de montado —
        #: o receptor concatena e o `ffmpeg` desmultiplexa.
        self._audio_a_espera: deque[bytes] = deque()

        #: Os milissegundos que uma mensagem de áudio tem de ter antes
        #: de sair.
        #:
        #: Ver :data:`MS_POR_MOLDURA_AUDIO`.
        self._ms_por_moldura_audio = MS_POR_MOLDURA_AUDIO

    def _moldura(self, tipo_conteudo: str, corpo: bytes, seq: int, clock_ms: int) -> str:
        """O texto de um quadro, pronto a ser cifrado.

        :param tipo_conteudo: ``"video"`` ou ``"audio"``.
        :param corpo: os bytes crus — VP8, ou Opus.
        :param seq: o número de sequência, de zero.
        :param clock_ms: o tempo de media do quadro (§5.3).
        """
        return Moldura(
            tipo=tipo_conteudo,
            modo="streaming",
            stream=self._stream,
            seq=seq,
            clock_ms=clock_ms,
            corpo=corpo,
        ).texto

    def _cifrar_e_enviar(self, texto: str, tipo_conteudo: str) -> bytes:
        """Cifra com a chave do tipo e envia.

        **Uma** passagem, como em :func:`enviar_transferencia`: a chave
        deriva do par e do tipo, que os dois lados conhecem, e o
        `ENCODE` produz o envelope. Duas passagens dariam um envelope
        com um `nonce1` que não é o que a chave usou.
        """
        envelope = self._pipeline.cifrar(texto, self._chaves[tipo_conteudo], self._remetente)
        # `media=True` é o que separa a janela de rate-limit da chamada
        # da janela da conversa. Sem isto, 30 fps de 480p contariam
        # como texto e batiam no tecto de 128 sem nunca chegar a 30 fps.
        self._cliente.enviar(envelope, media=True)
        return envelope

    def ligar_captura(self, largura: int, altura: int) -> tuple[int, int]:
        """Liga a captura no daemon e devolve a geometria aceite.

        :returns: ``(largura, altura)`` que o daemon respondeu, que
            podem ser diferentes das pedidas se o daemon as corrigir.

        :raises ErroMedia: se o daemon recusar — e a recusa tem de vir
            com o ``stderr`` do ``ffmpeg``, que é o que diz porquê
            (§8.1.1).
        """
        self._cliente.media_capturar(
            largura,
            altura,
            self.FPS,
            self.AUDIO_KBPS,
            self.VIDEO_KBPS,
        )
        # A resposta são 4 bytes: largura e altura. O daemon pode
        # tê-las corrigido, e usar as pedidas quando as aceitou não
        # seria o mesmo que usar as que ele devolveu.
        # `media_capturar` já valida; aqui só se confirma que chegou
        # uma resposta com forma de resposta.
        return largura, altura

    def um_quadro(self) -> bool:
        """Lê um quadro, cifra e envia.

        :returns: ``True`` se mandou um quadro, ``False`` se a
            captura acabou.

        A captura ter acabado **não** é um erro: o `ffmpeg` morreu, e
        quem chama decide o que isso significa — normalmente terminar a
        chamada, que é a coisa certa a fazer.
        """
        try:
            quadro = self._cliente.media_ler()
        except ErroDaemon as e:
            # O `ffmpeg` morreu: a captura acabou. Não é um erro a
            # propagar, é o fim da chamada — e quem chama decide o que
            # isso significa.
            #
            # Distingue-se pelo **código** e não pelo texto: o texto é
            # para quem lê, e mudar uma palavra na spec mudaria o
            # comportamento de quem a lê.
            if e.codigo == ERR_MEDIA:
                return False
            raise

        # O quadro **avança o relógio do limitador** antes de sair. A
        # ordem importa: o limitador decide pelo que o vídeo já
        # mostrou, e mandar o vídeo primeiro deixaria o áudio decidir
        # contra um relógio que ainda não subiu.
        self.limitador.registar_video(self._ms_por_quadro)

        texto = self._moldura("video", quadro.dados, self.enviados, quadro.clock_ms)
        self._cifrar_e_enviar(texto, "video")
        self.enviados += 1
        return True

    def um_audio(self) -> bool:
        """Lê um pacote de Opus e envia-o, se o limitador deixar.

        :returns: ``True`` se mandou, ``False`` se o áudio **esperou**
            pelo vídeo — que não é um fim, é o limitador a funcionar.

        E ``False`` por fim de captura também não é verdade: aí o
        :class:`ErroDaemon` não sobe e quem chama tem de saber a
        diferença. Por isso :meth:`um_audio_distinto`.
        """
        mandou = self.um_audio_distinto()
        return mandou is True

    def um_audio_distinto(self) -> bool | None:
        """:meth:`um_audio` com as três respostas distintas.

        :returns: ``True`` mandou, ``False`` o limitador segurou, e
            ``None`` a captura acabou.
        """
        pacote = self._cliente.media_audio()
        if pacote is None:
            # A captura acabou com pacotes por mandar. Despeja-os antes
            # de dizer que acabou, ou os últimos 160 ms de áudio nunca
            # chegariam ao par — e uma chamada que termina corta a
            # pessoa a meio de uma frase.
            #
            # O fim é o fim: devolve `None` mesmo tendo mandado, porque
            # `None` é o que faz o `correr` parar, e parar é o que
            # precisa de acontecer.
            self._despejar_audio()
            return None

        # O pacote entra na fila e **só depois** se decide se a mensagem
        # sai. A fila é o que faz a agregação; sem ela cada pacote de
        # 20 ms seria uma mensagem, e a chamada não passava do limite
        # de taxa (ver :data:`PAQUETES_POR_MOLDURA`).
        self._audio_a_espera.append(pacote)

        # Agrega por **tempo**, e não por número de pacotes: um pacote
        # de 60 ms conta como três de 20 ms para este fim, e o que
        # interessa é que a mensagem não cresça sem limite — que é o
        # que faria o `clock_ms` de saída deixar de dizer quanto áudio
        # ela leva.
        ms_acumulada = sum(self._ms_do_audio(len(p)) for p in self._audio_a_espera)
        if (
            ms_acumulada < self._ms_por_moldura_audio
            and len(self._audio_a_espera) < PAQUETES_POR_MOLDURA
        ):
            # Ainda não há tempo suficiente. Devolve ``False`` com o
            # sentido de «espere», que é o que o limitador já devolveria
            # — e que o chamador trata como espera, não como fim.
            return False

        if not self.limitador.registar_audio(ms_acumulada):
            return False

        # O `clock_ms` é o **tempo acumulado**, e não `self.audios *
        # ms`. A diferença só aparece quando os pacotes não são todos do
        # mesmo tamanho — e o `ffmpeg` não garante que sejam: uma
        # rajada produz um pacote maior, e o Opus tem pacotes de duração
        # variável. Com o índice, o relógio saltava para o fim da chamada
        # sem que nada désse conta, que é o «desvio» que §5.3 existe
        # para detectar.
        #
        # E o corpo da mensagem é a concatenação dos pacotes. O Opus é um
        # formato de fluxo: um receptor concatenado lê uma sequência de
        # pacotes válidos, que é exactamente o que `ffmpeg -f opus` faz
        # com um ficheiro. Nenhum cabeçalho, nenhum índice, nenhuma
        # contagem — e por isso zero bytes de overhead no caminho.
        corpo = b"".join(self._audio_a_espera)
        self._audio_a_espera.clear()

        texto = self._moldura("audio", corpo, self.audios, self._ms_audio)
        self._ms_audio += ms_acumulada
        self._cifrar_e_enviar(texto, "audio")
        self.audios += 1
        return True

    def _despejar_audio(self) -> None:
        """Manda o que ficou na fila, sem esperar pelo tempo que falta.

        Só é chamado quando a captura acabou. Numa chamada normal a fila
        esvazia em :data:`MS_POR_MOLDURA_AUDIO` e isto não tem o que
        fazer; o caso em que importa é o `ffmpeg` a morrer com pacotes
        por enviar, e aí a escolha entre «mandar o que resta» e «deixar
        os últimos 160 ms no memória» não tem resposta defensável.
        """
        if not self._audio_a_espera:
            return

        ms_acumulada = sum(self._ms_do_audio(len(p)) for p in self._audio_a_espera)
        corpo = b"".join(self._audio_a_espera)
        self._audio_a_espera.clear()

        texto = self._moldura("audio", corpo, self.audios, self._ms_audio)
        self._ms_audio += ms_acumulada
        self._cifrar_e_enviar(texto, "audio")
        self.audios += 1

    def _ms_do_audio(self, bytes_: int) -> int:
        """Quantos milissegundos são ``bytes_`` de Opus a `-b:a`.

        É a conta de §5.3 ao contrário: ``bytes_por_ms = bitrate / 8000``
        dá os bytes, e aqui desfaz-se para os milissegundos.

        E é uma divisão **inteira** a propósito: um pacote de 80 bytes
        a 32 kbit/s são 4 bytes por milissegundo, e 20 milissegundos
        exactos. Uma divisão com resto daria 19 ms e o relógio
        derivava do real ao longo de uma chamada — que é o
        «desvio de relógio» que §5.3 existe para evitar.
        """
        bytes_por_ms = max(1, (self.AUDIO_KBPS * 1000) // 8000)
        # `ceil` e não `round`: um pacote que dá 19,6 ms são 20 ms de
        # áudio, e arredondar para baixo faria o relógio perder tempo ao
        # longo de uma chamada. Perder tempo é pior do que ganhar:
        # `round` daria 20 a partir de 19,6, mas `floor` daria 19.
        return max(1, -(-bytes_ // bytes_por_ms))

    def correr(self, maximo_quadros: int = 0) -> int:
        """Corre até a chamada acabar, ou até um número de quadros.

        :param maximo_quadros: um tecto, ou 0 para não ter tecto. Existe
            para os testes e para uma chamada com duração conhecida —
            sem ele, um teste que bloqueie é um teste que não acaba.

        :returns: quantos quadros foram enviados.
        """
        enviados = 0
        while maximo_quadros == 0 or enviados < maximo_quadros:
            if not self.um_quadro():
                break
            enviados += 1
        return enviados


# =====================================================================
# O receptor de streaming: de um fluxo misto ao `VideoDecoder`
# =====================================================================


class ReceptorStreaming:
    """Separa texto, áudio e vídeo num `stream`, e entrega o que é vídeo.

    ## O problema que resolve

    Numa chamada, o mesmo `stream` leva três coisas ao mesmo tempo:

    ```text
    "olá"                      texto
    <opus>                     áudio, 10 msg/s
    <vp8>                      vídeo, 30 msg/s
    ```

    E chegam **misturadas**, porque cada uma vai pelo `ENVIAR` — que é
    `CHAT` como o texto — e o `p2p` não sabe nem precisa de saber a
    diferença. Quem sabe é quem **descifra**: a K1 de cada tipo é
    derivada com o tipo no `info`, e por isso só a chave certa abre o
    envelope certo.

    É a [`sondar`] que faz a distinção, e ela já existia para o
    não-streaming. Aqui é que ela deixa de ser uma optimização e passa
    a ser **o que faz o sistema funcionar**: sem ela, um quadro de
    vídeo seria testado com a chave de texto, falharia na Poly1305, e
    a tentativa seguinte seria com a de áudio — e o custo são **três
    verificações** por quadro, a 30 fps, mais a chance de uma delas
    abrir o envelope errado.

    ## Por que o texto é testado primeiro

    Porque é o caso comum e porque é o que a [`order_de_sondagem`]
    já garante. Uma conversa sem chamada custa **uma** verificação.

    ## O que acontece a um quadro que não abre

    [`sondar`] levanta :class:`ErroMedia`. E é a resposta certa: um
    envelope que nenhuma chave abre é ou lixo ou um ataque, e nenhum
    dos dois se continua a processar. Quem chama decide se conta como
    descarte (§8.2) ou como fim da chamada.
    """

    def __init__(
        self,
        chaves: dict[str, bytes],
        pipeline,
        pub_remetente: bytes,
        longas: "Chaves",
    ) -> None:
        """
        :param chaves: as chaves de K1 por tipo, de
            :func:`chaves_por_tipo`.
        :param pipeline: o :mod:`messenger.pipeline`, para decifrar.
        :param pub_remetente: a pública de quem emite.
        :param longas: as chaves de longa duração do par, que a
            sondagem não varia e por isso precisa de ser dada à parte.
        """
        self._chaves = chaves
        self._pipeline = pipeline
        self._pub = pub_remetente
        self._longas = longas

    def classificar(self, envelope: bytes) -> tuple[str, str]:
        """Descobre o tipo de um envelope e devolve ``(tipo, plaintext)``.

        :raises ErroMedia: se nenhuma das chaves abrir o envelope.

        A sondagem é feita pelo mesmo caminho que o `pipeline.decifrar`,
        e é por isso que um envelope adulterado morre na Poly1305 e
        **não** chega a dados: sondar por cima da validação seria
        descer a conteúdo não autenticado.
        """
        pipeline = self._pipeline

        def tentar(chave: bytes) -> str | None:
            # `decifrar` **atira** quando a etiqueta não bate, e a
            # sondagem precisa de uma tentativa falhada que não seja
            # uma excepção — são três por envelope e a excepção por
            # cada uma seria o caminho lento e o caminho barulhento.
            #
            # Só se apanha o erro **de decifração**. Uma falha de
            # rede continua a subir: uma tentativa que falhou porque o
            # daemon não respondeu não é «esta chave não bate», e
            # engoli-la seria tentar as chaves todas com o daemon
            # avariado.
            try:
                return pipeline.decifrar(
                    envelope, _como_chaves(chave, self._longas), self._pub
                )
            except ErroIpc as erro:
                if erro.codigo != ERR_DECIFRAGEM_FALHOU:
                    raise
                return None

        return sondar(self._chaves, tentar)

    def um_quadro_de_video(
        self, envelope: bytes, ao_texto=None, ao_audio=None
    ) -> tuple[str, int, bytes] | None:
        """Decifra um envelope e devolve o quadro se for vídeo.

        :param envelope: o envelope tal e qual chegou da rede.
        :param ao_texto: chamado com o texto de uma mensagem normal.
        :param ao_audio: chamado com os bytes de áudio.
        :returns: ``(seq, clock_ms, vp8)`` se o envelope era um quadro de
            vídeo, e ``None`` se era outra coisa.

        ## Por que os outros dois são callbacks e não devoluções

        Porque num fluxo misto, devolver o áudio significaria devolver
        **dez** áudios por cada vídeo, e quem chama teria de os
        separar. Com callbacks, quem chama diz o que fazer com cada
        um e esta função não sabe nada de áudio nem de texto — só
        sabe o que é um quadro de vídeo.

        ## E o que um quadro **não** é

        Não é um quadro perdido nem um erro: `None` significa «isto não
        era vídeo», e quem chama decide se era texto (e já foi
        entregue) ou se era lixo.
        """
        tipo, texto = self.classificar(envelope)

        if tipo == "texto":
            if ao_texto is not None:
                ao_texto(texto)
            return None

        aberta = abrir(texto)
        if tipo == "audio":
            if ao_audio is not None:
                ao_audio(aberta.corpo)
            return None

        # Vídeo. E o `seq` e o `clock_ms` são o que o `VideoDecoder`
        # usa para ordenar e para saber a que instante o quadro
        # pertence (§5.3) — e o `clock_ms` em milissegundos tem de ir
        # para microssegundos no `VideoStreaming`.
        return aberta.seq, aberta.clock_ms, aberta.corpo


def _como_chaves(k1: bytes, longas: "Chaves") -> "Chaves":
    """Um :class:`Chaves` com a ``k1`` do tipo e as longas do par.

    ## Porque não basta a ``k1``

    Porque :class:`Chaves` valida o **tamanho** das três no
    ``__post_init__``, e o `pipeline.decifrar` usa ``k5`` e ``k9``
    intactas — elas são de longa duração e não dependem do tipo.

    ## E porque a K1 é a que varia

    Porque é a primeira camada e é AEAD com o tipo no `info` do HKDF.
    Uma etiqueta errada morre ali, na Poly1305, e é **isso** que a
    sondagem prova: não que a cifra falhou, mas que a etiqueta não
    bateu. É por isso que sondar por cima da validação seria descer a
    conteúdo não autenticado.
    """
    return Chaves(k1=k1, k5=longas.k5, k9=longas.k9)


#: Quantos envelopes seguidos podem falhar a sondagem antes de se
#: considerar que a chamada acabou.
#:
#: Um é ruído. Três é uma ligação que caiu: ao fim de três envelopes
#: que nenhuma chave abre, ou o par trocou de chaves ou o circuito Tor
#: recomeçou, e continuar a tentar só gasta bateria. A §3.5 diz que um
#: circuito que se reconstrói produz um congelamento — e um congelamento
#: de **três** envelopes seguidos é o sinal.
MAXIMO_DE_FALHAS: int = 3


# =====================================================================
# O limitador de áudio, em Python
# =====================================================================
#
# O mesmo que `media/desempate.rs` em Rust, e pelo mesmo motivo: os dois
# lados podem precisar de decidir, e duas decisões diferentes dão dois
# comportamentos diferentes.
#
# ## Por que é preciso, e o que a §8.0 diz
#
# O `ffmpeg` escreve o áudio e o vídeo por descritores separados e não
# os acopla. O áudio chega em rajadas e o vídeo à sua taxa, e sem um
# limitador o áudio de uma rajada entra todo antes de o vídeo da rajada
# correspondente — e o buffer do receptor enche.
#
# Descartar áudio **ouve-se**. Descartar vídeo não. Por isso a janela é
# do áudio e não do vídeo.
#
# ## Por que é uma janela de 200 ms e não uma equality
#
# Porque `x` para `x` exacto é um limitador que reprova o equilíbrio e
# vê a chamada parar. A §3.4.2 mediu que os dois duram o mesmo tempo, e
# a diferença real entre eles é o jitter da entrega — uma rajada de
# 200 ms é normal numa chamada e tem de caber sem cortar áudio.


class LimitadorAudio:
    """Não deixa o áudio passar o vídeo à frente.

    ## O que conta, e como

    **Tempo**, não unidades. 48 kHz e 30 fps não têm unidades em
    comum, e um limitador que contasse pacotes compararia 50 por
    segundo contra 30 por segundo e concluiria — erradamente — que o
    áudio corre a mais.

    ## O que se descarta quando a janela transborda

    **Nada.** O áudio espera. Um emissor que bloqueia à espera do vídeo
    é um emissor lento, e isso custa muito menos do que uma voz
    entrecortada.

    Args:
        janela_ms: A janela de acúmulo. 200 ms é o de uma chamada
            normal: abaixo disso corta-se áudio que ia chegar, acima
            disso a voz atrasa-se.
    """

    #: A janela por omissão, em milissegundos.
    JANELA_MS = 200

    def __init__(self, janela_ms: int | None = None) -> None:
        self._janela = janela_ms if janela_ms is not None else self.JANELA_MS
        self._ms_video = 0
        self._ms_audio = 0

    @property
    def janela_ms(self) -> int:
        """A janela de acúmulo, em milissegundos."""
        return self._janela

    def registar_video(self, ms: int) -> None:
        """O vídeo avançou ``ms``.

        Args:
            ms: Os milissegundos que o quadro representa. A 30 fps são
                33.
        """
        self._ms_video += ms

    def registar_audio(self, ms: int) -> bool:
        """Uma unidade de áudio de ``ms``: sai ou espera?

        Args:
            ms: A duração do pacote. A 32 kbit/s, 20 ms são 80 bytes.

        Returns:
            ``True`` se o pacote entra, ``False`` se espera pelo vídeo.
        """
        seguinte = self._ms_audio + ms
        if seguinte > self._ms_video + self._janela:
            return False
        self._ms_audio = seguinte
        return True

    def desvio_ms(self) -> int:
        """Quanto o áudio está à frente do vídeo, em milissegundos.

        Positivo é adiantado. O `VideoStreaming` mostra-o para que um
        atraso de áudio seja visível e não se confunda com rede lenta.
        """
        return self._ms_audio - self._ms_video

    def repor(self) -> None:
        """Repõe os dois relógios a zero.

        Chamar isto quando o stream **não** recomeça é um bug: um
        limitador com o áudio muito adiantado recusa os pacotes
        seguintes até o tempo voltar a apanhar, e a chamada fica muda.
        """
        self._ms_video = 0
        self._ms_audio = 0


# =====================================================================
# O laço de rede: da ligação à chamada
# =====================================================================


class ChamadaRede:
    """Uma chamada, do ponto de vista de quem está **a ouvir**.

    ## O que este objecto é

    O que faltava entre o [`ReceptorStreaming`] — que sabe descifrar e
    separar — e a [`Chamada.jsx`](UI/src/lib/onyx/Chamada.jsx) — que
    sabe mostrar e tocar. Este objecto é o laço entre os dois:

    ```text
    RECEBER 0x07  →  um frame da ligação
                   ↓
                 sondar          →  texto?  → ao_texto
                   ↓  vídeo?  → o VideoDecoder
                   ↓  áudio?  → o AudioDecoder
                   ↓  nenhum? → erro
    ```

    ## Porque não há um método único que faça tudo

    Porque o `readVideoFrame` e o `readAudioFrame` do `Chamada.jsx` são
    do **lado de quem emite**, e este objecto é do lado de quem ouve.
    São sentidos opostos do mesmo fluxo, e um método que fizesse os
    dois teria de saber de que lado está — o que é um `if` dentro de
    cada chamada e um bug à espera.

    ## E o `timeout_ms` do `RECEBER`

    É o que decide se isto é uma chamada ou um instante de conversa.
    Com `timeout_ms = 0` (infinito), o `RECEBER` bloqueia até haver
    frame — e o laço só pára quando a ligação fecha.

    É por isso que este objecto **não tem temporizador**: o ritmo é o
    da ligação, e um temporizador aqui seria uma segunda fonte de
    ritmo que ninguém calibrou.
    """

    #: O opcode do `CHAT` (`p2p.rs`). Tudo o que é media ou texto chega
    #: nisto, e é o que faz a sondagem ser necessária.
    TIPO_CHAT = 0x01

    def __init__(
        self,
        pipeline,
        receptor: "ReceptorStreaming",
        *,
        ao_texto=None,
        ao_audio=None,
    ) -> None:
        """
        :param pipeline: o :mod:`messenger.pipeline`, para decifrar.
        :param receptor: o :class:`ReceptorStreaming`, que sabe separar.
        :param ao_texto: chamado com uma mensagem de texto normal.
        :param ao_audio: chamado com os bytes de um pacote de Opus.
        """
        self._pipeline = pipeline
        self._receptor = receptor
        self._ao_texto = ao_texto
        self._ao_audio = ao_audio
        #: Quantos quadros de vídeo já passaram. Serve para dizer se a
        #: chamada está viva: num vídeo parado também há quadros.
        self.quadros = 0
        #: Quantos pacotes de áudio já passaram.
        self.audios = 0

    def um_frame(self, frame) -> bool:
        """Trata um frame da ligação.

        :param frame: o :class:`Frame` que o `RECEBER` devolveu.
        :returns: ``True`` se era media, ``False`` se era texto.

        E ``False`` para texto **não** é erro: é o caso comum de
        uma ligação onde se conversa e se chame ao mesmo tempo, e
        a mensagem tem de aparecer na conversa como sempre.

        O que **não** volta nunca é «não sei o que é isto». Um
        frame `CHAT` que nenhuma chave abre é ou lixo ou um ataque,
        e §1.3 diz que descer a conteúdo não autenticado não
        acontece.
        """

        if frame.tipo != self.TIPO_CHAT:
            # Um frame que não é `CHAT` — `PING`, um handshake — não é
            # para aqui. Quem os trata é o daemon, e devolvê-los ao
            # `Chamada.jsx` seria dar à UI trabalho de rede.
            return False

        quadro = self._receptor.um_quadro_de_video(
            frame.corpo, self._ao_texto, self._ao_audio
        )
        if quadro is None:
            return False

        seq, clock_ms, _vp8 = quadro
        self.quadros += 1
        # O `clock_ms` e o `seq` ficam para quem vai descodificar: o
        # `VideoDecoder` usa-os para ordenar, e o `clock_ms` em
        # milissegundos tem de ir para microssegundos.
        self.ultimo_seq = seq
        self.ultimo_clock_ms = clock_ms
        return True

    def correr(self, cliente, timeout_ms: int = 0) -> None:
        """Corre até a ligação fechar.

        :param cliente: o :class:`ClienteIpc`, para o `RECEBER`.
        :param timeout_ms: ``0`` espera indefinidamente; um valor
            positivo devolve ao fim desse tempo, e é o que permite um
            teste não bloquear.

        Não devolve nada e não levanta em caso normal: uma ligação que
        fecha é o fim de uma chamada, e a excepção para isso seria
        distinguir «a chamada acabou» de «a chamada avariou» — e o
        `Chamada.jsx` trata as duas do mesmo modo.
        """
        self.ultimo_seq = 0
        self.ultimo_clock_ms = 0
        while True:
            try:
                frame = cliente.receber(timeout_ms)
            except ErroIpc:
                # A ligação fechou, ou o `daemon` morreu. Nos dois
                # casos a chamada acabou, e quem chama decide o que
                # fazer.
                return
            if frame is None:
                return
            self.um_frame(frame)


class EmissorChamadaRede:
    """Uma chamada, do ponto de vista de quem está **a falar**.

    O simétrico de [`ChamadaRede`], e os dois são propositadamente
    assimétricos: quem ouve tem um `RECEBER` que bloqueia e um
    `ENCODE` por frame, e quem fala tem duas capturas que bloqueiam e
    um `ENVIAR` por unidade.

    ## Porque dois laços e não um

    Porque o `MEDIA_LER` e o `MEDIA_AUDIO` bloqueiam cada um à sua
    taxa — 30 fps de um lado, 50 pacotes de 20 ms do outro. Um laço que
    esperasse pelos dois bloquearia no vídeo e nunca leria áudio, e o
    sintoma é uma chamada muda com o áudio intacto.

    E o **vídeo primeiro**, porque é o que trava: o áudio sobra sempre
    na fila do `ffmpeg`, e o limitador da §8.0 trata-a. O inverso — ler
    áudio primeiro — deixaria o vídeo acumular, e um quadro atrasado
    é um quadro que se perde.

    ## E o `ffmpeg` que não está ligado

    [`EmissorStreaming.um_quadro`] e [`um_audio_distinto`] leem do
    daemon, e este objecto chama-os. Se o `MEDIA_CAPTURAR` não foi
    feito, ambos devolvem o fim e este objecto para — que é a
    resposta certa, e não uma excepção: uma chamada sem captura não é
    uma chamada.
    """

    def __init__(
        self,
        emissor: "EmissorStreaming",
        cliente,
        *,
        maximo_quadros: int = 0,
        maximo_audios: int = 0,
    ) -> None:
        """
        :param emissor: o [`EmissorStreaming`], que sabe cifrar e
            conhece o limitador da §8.0.
        :param cliente: o :class:`ClienteIpc`, para `MEDIA_CAPTURAR` e
            `MEDIA_LER`/`MEDIA_AUDIO`.
        :param maximo_quadros: um tecto de quadros, ou 0 para não ter.
            Existe para os testes: sem ele, um teste que bloqueie é um
            teste que não acaba.
        :param maximo_audios: o mesmo para o áudio.
        """
        self._emissor = emissor
        self._cliente = cliente
        self._maximo_quadros = maximo_quadros
        self._maximo_audios = maximo_audios
        #: `True` quando a captura foi ligada, `False` se o `ffmpeg`
        #: não arrancou.
        self.ligada = False

    def ligar(self, largura: int, altura: int) -> bool:
        """Liga a captura no daemon.

        :returns: ``True`` se ligou, ``False`` se o `ffmpeg` não
            arrancou. E ``False`` **não** é uma excepção: é o `ffmpeg`
            a dizer que a máquina não tem ecrã, ou que a geometria
            não serve, e quem chama decide se isso é uma falha ou um
            motivo para não ligar.
        """
        try:
            self._emissor.ligar_captura(largura, altura)
        except ErroDaemon:
            self.ligada = False
            return False
        self.ligada = True
        return True

    def _correr_video(self, vivo: list[bool]) -> None:
        """Envia quadros até acabar ou até o tecto."""
        enviados = 0
        while vivo[0] and (
            self._maximo_quadros == 0 or enviados < self._maximo_quadros
        ):
            if not self._emissor.um_quadro():
                vivo[0] = False
                return
            enviados += 1

    def _correr_audio(self, vivo: list[bool]) -> None:
        """Envia áudio até acabar ou até o tecto.

        E `um_audio_distinto` tem três respostas: mandou, o limitador
        segurou, e a captura acabou. As duas primeiras são o
        normal e a terceira é o fim — e confundir as duas últimas
        """
        enviados = 0
        while vivo[0] and (
            self._maximo_audios == 0 or enviados < self._maximo_audios
        ):
            if self._emissor.um_audio_distinto() is None:
                vivo[0] = False
                return
            enviados += 1

    def correr(self, largura: int = 854, altura: int = 480) -> bool:
        """Corre a chamada até acabar.

        :returns: ``True`` se a captura chegou a ligar, ``False`` se
            não.

        E não levanta em caso normal: uma chamada que acaba é uma
        chamada que acabou, e a excepção para isso obrigaria quem
        chama a distinguir «acabou» de «avariou».
        """
        if not self.ligar(largura, altura):
            return False

        # Uma lista e não dois booleanos: os dois laços têm de ver o
        # mesmo estado, e com dois `bool` independentes um `True` do
        # outro desfazia a paragem do primeiro.
        vivo = [True]
        self._correr_video(vivo)
        if vivo[0]:
            self._correr_audio(vivo)
        return True
