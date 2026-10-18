# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_media_transferencia.py — a orquestração de uma transferência.

``tests/test_media.py`` prova que o **formato** é o mesmo nos dois
lados. Este prova que o **percurso** funciona: enviar um ficheiro
produz a sequência de envelopes certa, e o receptor monta a partir
dela exactamente o que foi enviado.

## Porque estes testes precisam do daemon verdadeiro

Porque o caminho tem o ``nonce1`` no meio, e o ``nonce1`` é a parte
delicada da coisa. A chave de media é derivada com
``HKDF(semente, salt=nonce1, info)`` e o ``nonce1`` **só existe depois
do ``ENCODE``**.

Daí o emissor cifrar **duas vezes** por mensagem: uma para obter um
envelope com um nonce válido, e outra com a chave derivada — que é a
que vai para a rede.

Um daemon de mentira que «cifra» a sério seria uma **segunda
implementação de K1→K9** em Python, e uma segunda implementação é
exactamente o que os vectors existem para evitar: ela podia divergir
e o teste passava na mesma.

E o custo de errar aqui não é um erro: é uma mensagem que **cifra bem
e não abre**, com ``0x05 DecifragemFalhou`` — que não aponta para a
chave, que é a pista mais errada que existe.
"""

from __future__ import annotations

import subprocess
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

import pytest

from messenger import media
from messenger.ipc_client import ChavesAmizade, ClienteIpc, ErroDaemon
from messenger.keys import Chaves, Identidade
from messenger.ipc_client import (
    ERR_MEDIA,
    ErroDaemon,
    ErroIpc,
    Frame,
    QuadroVideo,
)
from messenger.pipeline import Pipeline

RAIZ = Path(__file__).resolve().parent.parent
BINARIO = RAIZ / "target" / "debug" / "onyxchatd"

#: A ida e volta precisa do daemon verdadeiro, pelo motivo do cabeçalho.
pytestmark = pytest.mark.skipif(
    not BINARIO.is_file(),
    reason="daemon não compilado — correr `cargo build` na raiz do workspace",
)


# ---------------------------------------------------------------------
# A infrastructure
# ---------------------------------------------------------------------


@pytest.fixture()
def daemon() -> Iterator[ClienteIpc]:
    """Arranca ``onyxchatd`` num socket temporário e espera ficar pronto.

    ``--tor nenhum`` escolhe o backend em memória (``TorFalso``): os
    testes não podem depender da rede real nem escrever estado da arti
    no ``$HOME``.
    """
    caminho = Path(str(BINARIO)).parent / f"media-test-{id(object())}.sock"
    processo = subprocess.Popen(
        [str(BINARIO), "--socket", str(caminho), "--tor", "nenhum"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    for _ in range(200):
        if caminho.exists():
            break
        time.sleep(0.05)
    try:
        yield ClienteIpc(str(caminho))
    finally:
        processo.terminate()
        processo.wait(timeout=5)
        caminho.unlink(missing_ok=True)


@pytest.fixture()
def par() -> dict:
    """Um par de amizades com chaves e identidades dos dois lados.

    K5 e K9 são **distintos** entre A e B em todos os campos que
    importam, para que um erro de direcção não possa passar por
    coincidência: um teste que usasse a mesma chave nos dois sentidos
    abriria com a chave errada sem dar erro.
    """
    ident_a = Identidade.gerar()
    return {
        "chaves": ChavesAmizade(
            k1=bytes([0x55] * 32),
            k5_proprio=bytes([0x11] * 32),
            k9_proprio=bytes([0x33] * 32),
            k5_par=bytes([0x22] * 32),
            k9_par=bytes([0x44] * 32),
            publica=ident_a.pub,
        ),
        "texto": Chaves(bytes([0x55] * 32), bytes([0x11] * 32), bytes([0x33] * 32)),
        "identidade": ident_a,
        "pub": ident_a.pub,
    }


@contextmanager
def a_registar(cliente: ClienteIpc) -> Iterator[list[tuple[bytes, bytes]]]:
    """Regista cada ``(plaintext, envelope)`` que passa pelo ``ENCODE``.

    ## Porque o ``ENCODE`` e não o ``ENVIAR``

    O ``ENVIAR`` do daemon exige um ``LIGAR`` primeiro, e um ``LIGAR``
    exige um destino — que estes testes não têm, porque **não é a rede
    que estão a testar**. Montar uma ligação P2P para depois affirms
    que um envelope existe seria estar a testar o transporte e não a cifra.

    E o ``ENCODE`` é onde a cifra acontece: é o comando que recebe o
    plaintext e devolve o envelope. Registo-o, e o ``ENVIAR`` continua a
    ser chamado — para o daemon ver exactamente o que veria em
    produção, e falhar se algo estiver errado.

    O par é ``(plaintext, envelope)`` porque é a **dupla passagem** que
    precisa de ser visível: cada mensagem passa pelo ``ENCODE`` duas
    vezes, uma com a chave de texto e outra com a chave derivada, e o
    que vai para a rede é a segunda. Um registo só do envelope não
    diria qual dos dois é.
    """
    registados: list[tuple[bytes, bytes]] = []
    cifrar_original = cliente.cifrar
    enviar_original = cliente.enviar

    def cifrar(k1, k5, k9, seed, mensagem):  # type: ignore[no-untyped-def]
        envelope = cifrar_original(k1, k5, k9, seed, mensagem)
        bruto = mensagem.encode("utf-8") if isinstance(mensagem, str) else mensagem
        registados.append((bruto, envelope))
        return envelope

    def enviar(envelope: bytes, media: bool = False) -> None:
        """Não envia. A rede não é o que estes testes medem."""
        registados.append((b"", envelope))

    cliente.cifrar = cifrar  # type: ignore[method-assign]
    cliente.enviar = enviar  # type: ignore[method-assign]
    try:
        yield registados
    finally:
        cliente.cifrar = cifrar_original  # type: ignore[method-assign]
        cliente.enviar = enviar_original  # type: ignore[method-assign]


def _chave_de(par: dict, tipo: str, envelope: bytes) -> media.Chaves:
    """A :class:`Chaves` de um tipo, derivada do nonce **deste** envelope.

    É a mesma conta que o receptor faz ao sondar
    (``docs/media.md`` §1.3), e usa o nonce do envelope que vai ser
    decifrado — não outro. É o ponto inteiro do teste.
    """
    derivadas = media.chaves_media(
        par["chaves"].k1,
        par["chaves"].k5_proprio,
        par["chaves"].k5_par,
        par["pub"],
    )
    # O texto usa a `k1` de longa duração, e não uma derivada por
    # tipo: é o que mantém uma conversa byte-a-byte igual ao que era
    # antes de existir media (§1.2).
    k1 = par["chaves"].k1 if tipo == "texto" else derivadas[tipo]
    return media.Chaves(k1, par["chaves"].k5_proprio, par["chaves"].k9_proprio)


# ---------------------------------------------------------------------
# Os testes
# ---------------------------------------------------------------------


def test_o_receptor_monta_o_que_o_emissor_enviou(daemon: ClienteIpc, par: dict) -> None:
    """A ida e volta completa: enviar, sondar, montar.

    O que o emissor produz, cifrado com a chave derivada **do nonce do
    seu próprio envelope**, tem de abrir no receptor com a chave
    derivada do **mesmo** nonce — e o resumo tem de bater.
    """
    pipeline = Pipeline(daemon)
    conteudo = bytes((i * 17 + 3) % 256 for i in range(media.TAM_FATIA + 77))

    with a_registar(daemon) as registos:
        enviado = media.enviar_transferencia(
            pipeline, par["chaves"], par["identidade"], daemon, conteudo, "video.webm"
        )
    assert enviado.total_pedacos == 2, "48 KB + 77 bytes são duas fatias"

    # **Uma** passagem de ``ENCODE`` por mensagem. A primeira versão
    # fazia duas — uma para obter um nonce e outra para cifrar com a
    # chave derivada desse nonce — e a segunda gerava um nonce novo,
    # que não era o que a chave usava. Ver `messenger/amizade.py`.
    encodes = [r for r in registos if r[0]]
    assert len(encodes) == 3, f"{len(encodes)} ENCODE para 3 mensagens"

    # E o mesmo número de envelopes enviados.
    # O registo do `ENVIAR` tem o plaintext a `b""`, e o do `ENCODE`
    # não. É essa distinção que separa os dois sem precisar de um
    # campo extra.
    envelopes = [env for plaintext, env in registos if not plaintext]
    assert len(envelopes) == 3, "o manifesto mais duas fatias"

    montador = None
    for envelope in envelopes:
        texto = pipeline.decifrar(envelope, _chave_de(par, enviado.tipo, envelope), par["pub"])
        aberta = media.abrir(texto)
        assert aberta.modo == "nao-streaming"

        if aberta.flags & media.FLAGS["manifesto"]:
            montador = media.Montador(media.Manifesto.de_corpo(aberta.corpo))
            continue
        assert montador is not None, "uma fatia chegou antes do manifesto"
        montador.receber(aberta.seq, aberta.corpo)

    assert montador is not None, "nenhum manifesto chegou"
    assert montador.faltam() == 0, f"faltam {montador.indices_em_falta()}"
    assert montador.montar() == conteudo, "o conteúdo montado não é o enviado"


def test_a_media_nao_abre_com_a_chave_de_texto(daemon: ClienteIpc, par: dict) -> None:
    """A separação de chaves é o que faz a sondagem funcionar.

    Se um envelope de áudio abrisse com a chave de texto, a sondagem
    seria uma formalidade: o receptor não saberia que é áudio, e a
    distinção de tipo seria impossível.
    """
    pipeline = Pipeline(daemon)
    with a_registar(daemon) as registos:
        media.enviar_transferencia(
            pipeline, par["chaves"], par["identidade"], daemon, b"x" * 500, "som.opus"
        )
    envelope = registos[0][1]

    with pytest.raises(ErroDaemon) as info:
        pipeline.decifrar(envelope, par["texto"], par["pub"])
    # O código IPC é `0x05` e o `Display` mostra o **nome** dele. É o
    # nome que se verifica: o número vive num enumerado Rust e muda de
    # forma quando o `docs/ipc.md` o renumera, o nome não.
    assert info.value.codigo == 0x05, f"a falha não foi de decifra: {info.value}"
    assert "DecifragemFalhou" in str(info.value)

    # E com a chave certa abre — o que confirma que a falha é de chave
    # e não de envelope. Sem esta segunda metade, o teste passaria
    # com um envelope corrompido.
    texto = pipeline.decifrar(envelope, _chave_de(par, "audio", envelope), par["pub"])
    assert media.abrir(texto).tipo == "audio"


def test_a_sondagem_escolhe_o_tipo_certo(daemon: ClienteIpc, par: dict) -> None:
    """A ordem de sondagem devolve o tipo, e para no primeiro que abre.

    É a propriedade de ``docs/media.md`` §1.3 com o pipeline real: o
    texto é a primeira tentativa, o áudio é a segunda, e o vídeo nunca
    chega a ser tentado porque a anterior já abriu.
    """
    pipeline = Pipeline(daemon)
    with a_registar(daemon) as registos:
        media.enviar_transferencia(
            pipeline, par["chaves"], par["identidade"], daemon, b"y" * 300, "som.opus"
        )
    envelope = registos[0][1]

    # O dicionário completo na ordem da sondagem: a chave de texto é a
    # de longa duração, e **não** sai de `chaves_media` — é por isso que
    # continua byte-a-byte igual à de antes de existir media.
    derivadas = {
        "texto": par["chaves"].k1,
        **media.chaves_media(
            par["chaves"].k1, par["chaves"].k5_proprio, par["chaves"].k5_par, par["pub"]
        ),
    }
    tentativas: list[str] = []

    def decifrar(chave: bytes) -> bytes | None:
        """Um ``DECODE`` a sério, que regista por qual chave vai."""
        for nome, k in derivadas.items():
            if k == chave:
                tentativas.append(nome)
        try:
            return pipeline.decifrar(
                envelope,
                media.Chaves(chave, par["chaves"].k5_proprio, par["chaves"].k9_proprio),
                par["pub"],
            ).encode("utf-8")
        except ErroDaemon:
            return None

    nome, texto = media.sondar(derivadas, decifrar)

    assert nome == "audio"
    assert tentativas == ["texto", "audio"], f"a ordem foi {tentativas}"
    # E o que abriu é mesmo a moldura de áudio do emissor.
    assert media.abrir(texto.decode("utf-8")).tipo == "audio"



def test_uma_extensao_desconhecida_e_recusada(daemon: ClienteIpc, par: dict) -> None:
    """Uma extensão fora da tabela não é adivinhada.

    Adivinhar o formato pelo nome é a conveniência que manda um vídeo
    para o receptor a ser lido como áudio — e o receptor não tem como
    saber que errou.
    """
    with pytest.raises(media.ErroMedia, match="não sei o formato"):
        media.enviar_transferencia(
            Pipeline(daemon), par["chaves"], par["identidade"], daemon,
            b"x" * 100, "ficheiro.desconhecido",
        )


def test_matroska_exige_que_se_diga_o_codec(daemon: ClienteIpc, par: dict) -> None:
    """O Matroska não decide o codec — e o emissor recusa por isso.

    É o caso de ``docs/media.md`` §2.5: um vídeo Matroska com Opus
    dentro é ``Codec::Opus``, e escolher por omissão mandaria um vídeo
    ser rotulado de áudio.
    """
    with pytest.raises(media.ErroMedia, match="não decide o codec"):
        media.enviar_transferencia(
            Pipeline(daemon), par["chaves"], par["identidade"], daemon,
            b"x" * 100, "video.mkv",
        )


def test_um_ficheiro_vazio_e_so_o_manifesto(daemon: ClienteIpc, par: dict) -> None:
    """Zero bytes é uma mensagem, não duas."""
    with a_registar(daemon) as registos:
        enviado = media.enviar_transferencia(
            Pipeline(daemon), par["chaves"], par["identidade"], daemon, b"", "s.wav"
        )
    assert enviado.total_pedacos == 0
    # Uma mensagem, um ``ENCODE`` e um ``ENVIAR``: dois registos. E o
    # ficheiro vazio **não** é um caso especial no caminho da cifra —
    # a chave deriva do par e do tipo, e não do conteúdo.
    encodes = [r for r in registos if r[0]]
    enviados = [r for r in registos if not r[0]]
    assert len(encodes) == 1, f"{len(encodes)} ENCODE para um ficheiro vazio"
    assert len(enviados) == 1, "só o manifesto vai para a rede"


def test_a_resposta_de_retoma_e_uma_mensagem_que_nao_se_confunde(
    daemon: ClienteIpc, par: dict
) -> None:
    """«9f3a…: faltam 7 42 118» é texto, e texto comum não se confunde.

    A retoma é uma mensagem de texto porque é a **única** forma de
    pedir uma fatia sem um tipo de frame novo — e a rede não pode ver
    que é media (``docs/media.md`` §4.3).
    """
    texto = media.texto_de_retoma("ab" * 16, [7, 42, 118])
    assert media.interpretar_retoma(texto) == ("ab" * 16, [7, 42, 118])

    # E o que não é de retoma dá ``None`` em vez de levantar: o texto
    # chega pelo mesmo caminho de todas as mensagens da conversa, e
    # levantar por causa de uma frase comum seria trocar uma mensagem
    # por um erro.
    for comum in [
        "olá",
        "já vi, obrigado",
        "ab" * 16 + ": chegou bem",
        "curto: faltam 1",
        "zz" * 16 + ": faltam 1",
        "ab" * 16 + ": faltam x",
    ]:
        assert media.interpretar_retoma(comum) is None, f"aceitou {comum!r}"

    # E a mensagem de retoma é cifrável como texto normal — o que é o
    # ponto: ela **é** um texto normal.
    envelope = daemon.cifrar(
        par["chaves"].k1, par["chaves"].k5_proprio, par["chaves"].k9_proprio,
        par["identidade"].seed, texto,
    )
    lido = Pipeline(daemon).decifrar(envelope, par["texto"], par["pub"])
    assert lido == texto


# ---------------------------------------------------------------------
# O streaming: do `ffmpeg` para a rede
# ---------------------------------------------------------------------


class _ClienteQueDevolveQuadros:
    """Substitui os dois comandos de media de um cliente ligado.

    Não há ``ffmpeg`` nem ecrã nesta máquina, e o caminho que estes
    testes medem **não é** o ``ffmpeg`` — é a moldura, a cifra e a
    janela de rate-limit. O ``ffmpeg`` tem os seus testes, e um teste
    que dependesse dele falharia em qualquer máquina sem ecrã.

    É um **involucro** e não uma subclasse: o cliente da fixture já
    está ligado ao daemon, e construir outro exigiria um socket
    próprio sem ganhar nada.
    """

    def __init__(
        self, cliente: ClienteIpc, quadros: int = 3, pacotes: int = 0
    ) -> None:
        self._cliente = cliente
        self._quadros_restantes = quadros
        self.pacotes_restantes = pacotes
        #: Os tamanhos dos pacotes de áudio, se o teste os impuser.
        self.tamanhos: list[int] = []
        self.capturar_args: tuple[int, ...] | None = None
        #: A marca de origem de cada envio, na ordem.
        self.origens: list[bool] = []
        cliente.media_capturar = self.media_capturar  # type: ignore[method-assign]
        cliente.media_ler = self.media_ler  # type: ignore[method-assign]
        cliente.media_audio = self.media_audio  # type: ignore[method-assign]

    def devolver(self) -> ClienteIpc:
        """O cliente de verdade, com os comandos de media trocados."""
        return self._cliente

    def media_capturar(self, *args: int) -> None:
        self.capturar_args = tuple(args)

    def media_audio(self) -> bytes | None:
        if self.pacotes_restantes <= 0:
            return None
        self.pacotes_restantes -= 1
        # Os tamanhos por pacote. Por omissão 80 bytes = 20 ms a
        # 32 kbit/s (§5.3). Um teste que passe uma lista obtém pacotes
        # de durações diferentes, que é o caso que o relógio tem de
        # aguentar sem derivar.
        tamanho = self.tamanhos.pop(0) if self.tamanhos else 80
        return bytes(tamanho)

    def media_ler(self) -> QuadroVideo:
        if self._quadros_restantes <= 0:
            raise ErroDaemon(ERR_MEDIA, "MEDIA_LER: a captura terminou")
        self._quadros_restantes -= 1
        return QuadroVideo(
            sequencia=self._quadros_restantes,
            clock_ms=self._quadros_restantes * 33,
            dados=bytes((i * 7 + 1) % 256 for i in range(400)),
        )

    def enviar(self, envelope: bytes, media: bool = False) -> None:
        """Regista a origem e deixa passar.

        O cliente real continua a receber o envelope: a rede não é o que
        estes testes medem, e um `enviar` que não enviasse mediria outra
        coisa.
        """
        self.origens.append(media)
        self._cliente.enviar(envelope, media=media)



def test_o_streaming_manda_molduras_de_vp8(
    daemon: ClienteIpc, par: dict
) -> None:
    """Um quadro do `ffmpeg` vira uma moldura `streaming` de VP8.

    E o que o teste fixa é o **modo**: `streaming`, não
    `nao-streaming`. Uma moldura de streaming com modo errado é
    aceite pelo receptor e interleada como se fosse um ficheiro, e o
    sintoma é o vídeo a aparecer aos bocados entre as fatias do áudio.
    """
    pipeline = Pipeline(daemon)
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=3)
    cliente = involucro.devolver()

    with a_registar(cliente) as registos:
        # O emissor recebe o **wrapper**, que reencaminha `enviar` para
        # o cliente — e é assim que a troca de `a_registar` chega até
        # ele. Passar o cliente directo faria o emissor guardar o
        # `enviar` antigo e a rede receber de facto o envelope.
        emissor = media.EmissorStreaming(
            pipeline, par["chaves"], par["identidade"], involucro
        )
        emissor.ligar_captura(emissor.LARGURA, emissor.ALTURA)
        enviados = emissor.correr()

    assert enviados == 3, "três quadros pedidos, três enviados"
    assert involucro.capturar_args == (854, 480, 30, 32, 300), (
        "a captura pede 480p a 30 fps com 300 kbit/s de vídeo"
    )

    # Cada envio é um par (bruto, envelope).
    brutos = [b for b, _ in registos if b]
    assert len(brutos) == 3
    # O `codec` de VP8 (0x0004) é o que o `CODECS` mapeia, e o teste
    # fecha-se noutro: um quadro de streaming **não** tem manifesto, e
    # por isso `Aberta` não tem `codec`. O que ele tem é o tipo, e é o
    # tipo que escolhe a chave da cifra.
    for bruto in brutos:
        aberta = media.abrir(bruto.decode())
        assert aberta.modo == "streaming", f"modo errado: {aberta.modo}"
        assert aberta.tipo == "video", f"tipo errado: {aberta.tipo}"
        assert aberta.corpo, "o corpo não pode estar vazio"
    assert [media.abrir(b.decode()).seq for b in brutos] == [0, 1, 2]
    assert media.CODECS[0x0004] == "vp8", "VP8 tem de estar nos codecs"


def test_o_streaming_marca_a_origem_como_media(
    daemon: ClienteIpc, par: dict
) -> None:
    """A janela de rate-limit é a de media, não a de texto.

    Sem isto, 30 fps de 480p contariam como conversa e batiam no tecto
    de 128 por janela sem nunca chegar a 30 fps.
    """
    pipeline = Pipeline(daemon)
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=2)
    cliente = involucro.devolver()

    # O wrapper regista a origem de cada envio. Não se pode interceptar
    # o `enviar` por fora: `a_registar` substitui-o no cliente, e quem
    # chama é o wrapper — que é o que vê a marca.
    with a_registar(cliente):
        emissor = media.EmissorStreaming(
            pipeline, par["chaves"], par["identidade"], involucro
        )
        emissor.correr()
    origens = involucro.origens


    assert origens == [True, True], f"todas as marcas de origem: {origens}"


def test_a_captura_que_acaba_nao_e_um_erro(daemon: ClienteIpc, par: dict) -> None:
    """O `ffmpeg` morreu: a chamada acaba, e não é uma excepção.

    O `MEDIA_LER` devolve ``0x14 Media``, e o emissor traduz isso em
    «acabou». Qualquer outro erro propaga — porque um ``0x0A NaoLigado``
    a meio de uma chamada é uma falha, não o fim.
    """
    pipeline = Pipeline(daemon)
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=2)
    cliente = involucro.devolver()
    emissor = media.EmissorStreaming(
        pipeline, par["chaves"], par["identidade"], involucro
    )

    # `a_registar` intercepta o `enviar`: sem isto o daemon responde
    # `NaoLigado` e o teste mediria a falta de ligação, não o fim da
    # captura.
    with a_registar(cliente):
        assert emissor.um_quadro() is True
        assert emissor.um_quadro() is True
        assert emissor.um_quadro() is False, "o terceiro não há"
    assert emissor.enviados == 2


# ---------------------------------------------------------------------
# O receptor de streaming: texto, áudio e vídeo no mesmo fluxo
# ---------------------------------------------------------------------


def _receptor(par: dict, daemon: ClienteIpc):
    """Um :class:`ReceptorStreaming` com as chaves do par."""
    # `chaves_media` **não** devolve a chave de texto: essa é a
    # `k1_texto` de longa duração, e é por isso que uma conversa
    # continua byte-a-byte igual ao que era antes de existir media
    # (§1.2).
    derivadas = media.chaves_media(
        par["chaves"].k1,
        par["chaves"].k5_proprio,
        par["chaves"].k5_par,
        par["pub"],
    )
    derivadas = {"texto": par["chaves"].k1, **derivadas}
    longas = media.Chaves(
        par["chaves"].k1, par["chaves"].k5_proprio, par["chaves"].k9_proprio
    )
    return media.ReceptorStreaming(
        derivadas, Pipeline(daemon), par["pub"], longas
    )


def _envelope_de_video(pipeline, par: dict, seq: int, clock_ms: int, dados: bytes) -> bytes:
    """Um envelope com uma moldura `streaming` de VP8."""
    texto = media.Moldura(
        tipo="video",
        modo="streaming",
        stream=1,
        seq=seq,
        clock_ms=clock_ms,
        corpo=dados,
    ).texto
    return pipeline.cifrar(texto, _chave_de(par, "video", b""), par["identidade"])


def test_o_receptor_separa_texto_audio_e_video(
    daemon: ClienteIpc, par: dict
) -> None:
    """Numa chamada o `stream` leva os três ao mesmo tempo.

    E o que o teste fixa é a **separação**: um envelope de texto não
    pode ser tomado por um quadro de vídeo, nem o contrário. Uma
    sondagem que devolvesse o texto como se fosse vídeo daria ao
    `VideoDecoder` bytes de uma frase, e ele mostraria um ecrã de lixo
    sem dizer nada.
    """
    pipeline = Pipeline(daemon)
    receptor = _receptor(par, daemon)

    textos: list[str] = []
    audios: list[bytes] = []

    # 1. Texto.
    envelope_texto = pipeline.cifrar(
        "olá, estás a ouvir-me?", _chave_de(par, "texto", b""), par["identidade"]
    )
    assert receptor.um_quadro_de_video(envelope_texto, textos.append, audios.append) is None
    assert textos == ["olá, estás a ouvir-me?"], textos
    assert audios == []

    # 2. Áudio.
    moldura_audio = media.Moldura(
        tipo="audio",
        modo="streaming",
        stream=1,
        seq=0,
        clock_ms=40,
        corpo=bytes((i * 3) % 256 for i in range(120)),
    ).texto
    envelope_audio = pipeline.cifrar(
        moldura_audio, _chave_de(par, "audio", b""), par["identidade"]
    )
    assert receptor.um_quadro_de_video(envelope_audio, textos.append, audios.append) is None
    assert len(audios) == 1, "um pacote de áudio"
    assert audios[0] == bytes((i * 3) % 256 for i in range(120))

    # 3. Vídeo.
    vp8 = bytes((i * 11 + 5) % 256 for i in range(400))
    envelope_video = _envelope_de_video(pipeline, par, 42, 233, vp8)
    quadro = receptor.um_quadro_de_video(envelope_video, textos.append, audios.append)
    assert quadro is not None, "isto era vídeo e tem de ser reconhecido"
    seq, clock_ms, dados = quadro
    assert (seq, clock_ms) == (42, 233), f"{seq}, {clock_ms}"
    assert dados == vp8, "os bytes de VP8 chegam intactos"
    assert len(audios) == 1, "o vídeo não acorda o áudio"


def test_a_sondagem_abre_video_com_a_chave_de_video(
    daemon: ClienteIpc, par: dict
) -> None:
    """Um envelope de vídeo abre com `video` e **só** com `video`.

    A K1 de cada tipo é derivada com o tipo no `info` do HKDF, e por isso
    uma etiqueta errada morre na Poly1305. É isso que a sondagem prova:
    não que a cifra falhou, mas que a etiqueta não bateu.
    """
    pipeline = Pipeline(daemon)
    receptor = _receptor(par, daemon)
    envelope = _envelope_de_video(pipeline, par, 1, 33, b"\x01\x02\x03")

    tipo, texto = receptor.classificar(envelope)
    assert tipo == "video"
    aberta = media.abrir(texto)
    assert aberta.tipo == "video"
    assert aberta.modo == "streaming"


def test_um_envelope_que_nenhuma_chave_abre_e_erro(
    daemon: ClienteIpc, par: dict
) -> None:
    """Lixo não é texto, áudio nem vídeo: é um erro.

    E o erro é a resposta certa. Um envelope que nenhuma chave abre é ou
    lixo ou um ataque, e nenhum dos dois se continua a processar.
    """
    pipeline = Pipeline(daemon)
    receptor = _receptor(par, daemon)
    # Um envelope de texto, decifrado com as chaves de **media**: a
    # Poly1305 não bate e nenhuma das chaves de media o abre.
    envelope = pipeline.cifrar(
        "olá", _chave_de(par, "texto", b""), par["identidade"]
    )
    derivadas = media.chaves_media(
        par["chaves"].k1, par["chaves"].k5_proprio, par["chaves"].k5_par, par["pub"]
    )
    sem_texto = {k: v for k, v in derivadas.items() if k != "texto"}
    receptor_sem_texto = media.ReceptorStreaming(
        sem_texto,
        Pipeline(daemon),
        par["pub"],
        media.Chaves(
            derivadas["video"], par["chaves"].k5_proprio, par["chaves"].k9_proprio
        ),
    )
    _ = receptor
    try:
        receptor_sem_texto.classificar(envelope)
    except media.ErroMedia as erro:
        assert "nenhuma" in str(erro), erro
    else:
        raise AssertionError("um envelope de texto devia ser recusado sem a chave de texto")

    # E o receptor completo, que **tem** a chave de texto, abre-o.
    assert receptor.classificar(envelope)[0] == "texto"


def test_a_ordem_de_sondagem_e_texto_primeiro() -> None:
    """Texto primeiro: uma conversa sem chamada custa **uma** verificação.

    E a asserção é a **ordem**, não a contagem. Uma ordem que testasse o
    áudio antes do texto custaria três verificações a cada mensagem de
    uma conversa normal — que é o caso comum, e a sondagem é o que
    impede esse custo.

    A ordem é a mesma no Rust e aqui, e é exportada por isso: duas
    listas divergentes dariam um sistema que abre texto num lado e
    áudio no outro.
    """
    ordem = media.order_de_sondagem()
    assert ordem[0] == "texto", f"a primeira tentativa é {ordem[0]}"
    assert ordem == ("texto", "audio", "video", "call", "ficheiro"), ordem
    # Os dois primeiros depois do texto têm moldura; os dois últimos
    # são de controlo e não têm. A distinção é o que impede alguém de
    # tentar abrir `call` como se fosse uma moldura `ONX1`.
    for nome in ("audio", "video"):
        assert nome in media.TIPOS, f"{nome!r} não é um tipo de media com moldura"
    for nome in ("call", "ficheiro"):
        assert nome not in media.TIPOS, (
            f"{nome!r} tem chave mas não tem moldura — se estivesse em TIPOS, "
            "o byte 5 da moldura aceitaria um terceiro tipo que não cabe"
        )
        assert nome in media.TIPOS_DE_CHAVE, f"{nome!r} não deriva chave"


# ---------------------------------------------------------------------
# O áudio: o limitador da §8.0 em acção
# ---------------------------------------------------------------------


def test_o_limitador_nao_deixa_o_audio_passar_a_frente(
    daemon: ClienteIpc, par: dict
) -> None:
    """Uma rajada de áudio não pode encher o buffer do receptor.

    Este é o teste da tua correcção. Sem o limitador, o áudio de uma
    rajada entra todo antes do vídeo da rajada correspondente e o
    receptor tem de descartar — o que **se ouve**.
    """
    limitador = media.LimitadorAudio()
    # O vídeo não avançou: ainda não há nenhum quadro. A janela de
    # 200 ms deixa passar **dez** pacotes e depois segura — e é
    # propositado: uma rajada de 200 ms é normal numa chamada, e cortar
    # áudio que ia chegar é pior do que o atraso de 200 ms.
    #
    # A primeira versão deste teste esperava zero e falhou, e a falha
    # estava certa: uma janela de zero não é um limitador, é um
    # cortador de áudio.
    entraram = 0
    for _ in range(50):
        if limitador.registar_audio(20):
            entraram += 1
    assert entraram == 10, f"{entraram} entraram, e a janela são 200 ms"
    assert limitador.desvio_ms() == 200, limitador.desvio_ms()


def test_o_audio_passa_quando_o_video_avanca(daemon: ClienteIpc, par: dict) -> None:
    """Com vídeo a avançar, o áudio entra — e a janela segura o jitter.

    A 30 fps o vídeo oferece 1000 ms por segundo e o áudio também: os
    dois duram o mesmo tempo. O que a janela segura é a rajada, e por
    isso 50 pacotes de 20 ms depois de 30 quadros de 33 ms entram
    todos com um desvio pequeno.
    """
    limitador = media.LimitadorAudio()
    for _ in range(30):
        limitador.registar_video(33)
    entraram = sum(1 for _ in range(50) if limitador.registar_audio(20))
    assert entraram == 50, f"{entraram} de 50: o limitador está a apertar demais"
    assert limitador.desvio_ms() <= limitador.janela_ms, (
        f"desvio de {limitador.desvio_ms()} ms acima da janela de "
        f"{limitador.janela_ms} ms"
    )


def test_o_limitador_preserva_o_equilibrio(daemon: ClienteIpc, par: dict) -> None:
    """O equilíbrio tem de emitir, e não é um detalhe.

    A comparação é `>` e não `>=`. Com `>=` a unidade que levaria o
    áudio exactamente à janela seria recusada, a seguinte também, e o
    áudio ficaria preso enquanto o vídeo não avançasse — uma chamada
    que cala a voz.
    """
    limitador = media.LimitadorAudio(janela_ms=200)
    limitador.registar_video(100)
    # A janela são 100 + 200 = 300 ms, e cada unidade são 20 ms.
    for _ in range(15):
        assert limitador.registar_audio(20) is True
    # 300 ms de áudio contra 100 de vídeo: desvio de 200.
    assert limitador.desvio_ms() == 200, limitador.desvio_ms()
    # A dezasseisª levaria a 320 ms: aí espera.
    assert limitador.registar_audio(20) is False


def test_repor_e_necessario_ou_o_audio_fica_mudo(
    daemon: ClienteIpc, par: dict
) -> None:
    """Um limitador com o áudio adiantado recusa tudo até o tempo apanhar.

    E quem reinicia o stream chama `repor`. Sem isso a segunda chamada
    fica muda, porque a primeira deixou o relógio do áudio muito à
    frente.
    """
    limitador = media.LimitadorAudio()
    for _ in range(40):
        limitador.registar_audio(20)
    assert limitador.desvio_ms() == 200, "o áudio ficou na janela"
    # O vídeo apanha: um pacote de 33 ms de vídeo por 20 ms de áudio é
    # o caso normal, e o limitador tem de voltar a deixar passar.
    limitador.registar_video(33 * 40)
    assert limitador.registar_audio(20) is True, "com vídeo, o áudio volta"

    limitador.repor()
    assert limitador.desvio_ms() == 0
    assert limitador.registar_audio(20) is True, "depois de repor, volta a sair"


def test_o_audio_e_o_video_saos_no_ramo_da_moldura_certa(
    daemon: ClienteIpc, par: dict
) -> None:
    """Vídeo sai com `video` e áudio com `audio` — e a chave difere.

    É a [sondagem] que depende disto: se os dois saíssem com a mesma
    chave, o receptor não teria como separá-los, e um pacote de Opus
    seria entregue ao `VideoDecoder`.
    """
    pipeline = Pipeline(daemon)
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=1, pacotes=3)
    cliente = involucro.devolver()

    with a_registar(cliente) as registos:
        emissor = media.EmissorStreaming(
            pipeline, par["chaves"], par["identidade"], involucro
        )
        emissor.um_quadro()
        # Oito pacotes de 20 ms são os 160 ms de uma mensagem de áudio
        # (a agregação da Fase 2), e não três mensagens de uma.
        for _ in range(8):
            emissor.um_audio_distinto()

    tipos = []
    for bruto, _ in registos:
        if not bruto:
            continue
        tipos.append(media.abrir(bruto.decode()).tipo)
    assert tipos == ["video", "audio"], tipos
    # E o emissor conta **mensagens**, não pacotes: uma mensagem, um
    # incremento. O `audios` é o `seq` do preâmbulo.
    assert emissor.audios == 1, emissor.audios


def test_o_clock_ms_do_audio_sai_dos_bytes(daemon: ClienteIpc, par: dict) -> None:
    """O relógio do áudio sai dos próprios bytes, não de um contador.

    É §5.3 ao contrário: 80 bytes a 32 kbit/s são 20 ms. E um contador
    seria um segundo relógio, que deriva do primeiro ao longo de uma
    chamada — que é o desvio que §5.3 existe para evitar.

    E a agregação (Fase 2) não muda isto: o relógio que sai é o
    **acumulado** de todos os pacotes da mensagem, não o do primeiro.
    """
    pipeline = Pipeline(daemon)
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=4, pacotes=16)
    cliente = involucro.devolver()

    with a_registar(cliente) as registos:
        emissor = media.EmissorStreaming(
            pipeline, par["chaves"], par["identidade"], involucro
        )
        # O limitador de §8.0 tem uma janela de 200 ms e o áudio não
        # passa o vídeo à frente: sem vídeo nenhum registado, os
        # primeiros 200 ms de áudio **esperam** — que é o limitador a
        # funcionar, e não uma falha da agregação.
        #
        # Por isso o teste regista vídeo antes de cada bloco de áudio,
        # para que o limitador os deixe sair.
        emissor.um_quadro()
        for _ in range(8):
            emissor.um_audio_distinto()

    molduras = [media.abrir(b.decode()) for b, _ in registos if b]
    audios = [m for m in molduras if m.tipo == "audio"]
    # Uma mensagem, com 640 bytes de corpo: são 160 ms a 32 kbit/s
    # (4 bytes por milissegundo). A conta sai dos **bytes**, não de um
    # contador — que é o ponto do teste.
    assert len(audios) == 1, f"{len(audios)} mensagens de áudio"
    assert len(audios[0].corpo) == 640, f"corpo de {len(audios[0].corpo)} bytes, não 640"
    # E o relógio da mensagem de áudio é o tempo **antes** dela: 0.
    assert audios[0].clock_ms == 0, audios[0].clock_ms


def test_a_audio_agrega_oitenta_pacotes_em_mensagens_de_cento_e_seisenta(
    daemon: ClienteIpc, par: dict
) -> None:
    """A agregação: 8 pacotes de 20 ms numa mensagem, e o resto aritmética.

    Este é o teste que sustenta o limite de taxa. Sem a agregação, uma
    chamada de 30 fps com Opus de 20 ms manda 80 mensagens por segundo —
    800 por janela de 10 s — e o limite de media (`p2p.rs`, 512) estoura
    em seis segundos de chamada.

    O que se prova aqui é a **forma** da agregação: oito pacotes num
    corpo só, o relógio a saltar de 160 em 160, e nada perdido pelo
    caminho.
    """
    pipeline = Pipeline(daemon)
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=4, pacotes=16)
    cliente = involucro.devolver()

    with a_registar(cliente) as registos:
        emissor = media.EmissorStreaming(
            pipeline, par["chaves"], par["identidade"], involucro
        )
        # O limitador de §8.0 tem janela de 200 ms, e é ele que decide
        # — não a agregação. Duas mensagens de 160 ms são 320 ms de
        # áudio, e o limitador só as deixa passar se houver vídeo que as
        # justifique: 33 ms por quadro, e `320 − 200 = 120` ms de
        # vídeo bastam, ou seja quatro quadros.
        #
        # Um teste que não der vídeo suficiente não está a testar a
        # agregação: está a testar o limitador, que já tem testes seus.
        for _ in range(4):
            emissor.um_quadro()
        for _ in range(16):
            emissor.um_audio_distinto()

    molduras = [media.abrir(b.decode()) for b, _ in registos if b]
    audios = [m for m in molduras if m.tipo == "audio"]
    assert len(audios) == 2, f"{len(audios)} mensagens de áudio para 16 pacotes"

    # O relógio avança de 160 em 160: 0 e 160.
    assert [m.clock_ms for m in audios] == [0, 160], [m.clock_ms for m in audios]

    # E cada corpo tem os oito pacotes de 80 bytes, uns a seguir aos
    # outros. A concatenação é o que faz o `ffmpeg` do receptor ler um
    # fluxo Opus válido de um corpo só.
    for m in audios:
        assert len(m.corpo) == 8 * 80, f"corpo de {len(m.corpo)} bytes, não 640"

    # E nada se perdeu: 16 pacotes de entrada, 2 mensagens de saída, e o
    # total de bytes no corpo é exactamente o que entrou.
    assert sum(len(m.corpo) for m in audios) == 16 * 80


def test_a_fila_de_audio_esvazia_ao_mandar(daemon: ClienteIpc, par: dict) -> None:
    """A fila não acumula entre mensagens.

    Uma fila que não esvaziasse faria a segunda mensagem levar os
    pacotes da primeira duas vezes — e o áudio dobrado, que se ouve
    como um eco robótico.
    """
    pipeline = Pipeline(daemon)
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=4, pacotes=16)
    cliente = involucro.devolver()

    with a_registar(cliente):
        emissor = media.EmissorStreaming(
            pipeline, par["chaves"], par["identidade"], involucro
        )
        # O limitador precisa de vídeo para deixar o áudio passar:
        # 160 ms de áudio numa janela de 200 ms passam com um quadro
        # (33 ms), e o segundo bloco precisa de mais dois.
        for _ in range(4):
            emissor.um_quadro()
        for _ in range(8):
            emissor.um_audio_distinto()
        assert len(emissor._audio_a_espera) == 0, "a fila não esvaziou ao mandar"
        emissor.um_quadro()
        for _ in range(8):
            emissor.um_audio_distinto()
        assert len(emissor._audio_a_espera) == 0, "a segunda mensagem não esvaziou"


def test_o_audio_que_acaba_devolve_none_e_nao_erro(
    daemon: ClienteIpc, par: dict
) -> None:
    """A captura que acaba é ``None``, e `None`` não é o mesmo que espera.

    A diferença é a que o `correr` precisa: ``False`` é o limitador a
    segurar e ``None`` é o fim. Confundir as duas coisas faria uma
    chamada parar a cada rajada de áudio.

    E com a agregação, a fila ainda pode ter pacotes quando o `ffmpeg`
    morre — o que obriga a **despejar** o que ficou, ou os últimos
    160 ms de áudio nunca chegariam ao par.
    """
    pipeline = Pipeline(daemon)
    # **Seis** pacotes: chegam a 120 ms, que não chegam aos 160 do alvo
    # — e é esse o caso que interessa. Uma captura que morre com a fila
    # meio cheia tem de mandar o que ficou (a Fase 2, `_despejar_audio`)
    # e só depois dizer que acabou. Se a fila estivesse cheia, o fim
    # seria indistinguível do despejo.
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=4, pacotes=6)
    cliente = involucro.devolver()
    emissor = media.EmissorStreaming(
        pipeline, par["chaves"], par["identidade"], involucro
    )

    with a_registar(cliente) as registos:
        # Os seis pacotes não chegam aos 160 ms: cada um guarda e
        # devolve «espere», que **não** é fim.
        for _ in range(6):
            assert emissor.um_audio_distinto() is False

        # Agora não há mais pacotes. E a fila tem 120 ms por enviar: o
        # `ffmpeg` morreu a meio, e esses 120 ms têm de sair na mesma —
        # uma chamada que termina corta a pessoa a meio de uma frase se
        # o áudio fica na memória.
        assert emissor.um_audio_distinto() is None, "o que não há é o fim"

    tipos = [media.abrir(b.decode()).tipo for b, _ in registos if b]
    # Uma mensagem: a do despejo, com os seis pacotes.
    assert tipos == ["audio"], tipos
    # E tem os 120 ms: 480 bytes a 4 bytes por milissegundo.
    despejado = media.abrir([b for b, _ in registos if b][0].decode())
    assert len(despejado.corpo) == 6 * 80, (
        f"o despejo saiu com {len(despejado.corpo)} bytes, não 480"
    )


# ---------------------------------------------------------------------
# O laço de rede
# ---------------------------------------------------------------------


class _LigacaoFalsa:
    """Uma ligação que entrega frames e depois acaba.

    É o que o `RECEBER` devolve, e o `ChamadaRede.correr` consome. Não
    há `p2p` aqui: o que se testa é a **separação**, e ela é a mesma
    com ou sem rede — a rede só troca quem traz o frame.
    """

    def __init__(self, envelopes: list[bytes], tipo: int = 0x01) -> None:
        self.pendentes = [(tipo, e) for e in envelopes]
        self.timeouts: list[int] = []

    def receber(self, timeout_ms: int) -> Frame:
        self.timeouts.append(timeout_ms)
        if not self.pendentes:
            raise ErroIpc("a ligação fechou")
        tipo, envelope = self.pendentes.pop(0)
        return Frame(tipo=tipo, corpo=envelope)


def _envelope_de_audio(pipeline, par: dict, seq: int, dados: bytes) -> bytes:
    texto = media.Moldura(
        tipo="audio",
        modo="streaming",
        stream=1,
        seq=seq,
        clock_ms=seq * 20,
        corpo=dados,
    ).texto
    return pipeline.cifrar(texto, _chave_de(par, "audio", b""), par["identidade"])


def test_o_laco_separa_texto_audio_e_video_da_ligacao(
    daemon: ClienteIpc, par: dict
) -> None:
    """Uma ligação, três tipos, e o `ChamadaRede` separa os três.

    É este o objecto que faltava entre a rede e a chamada: recebe um
    `Frame`, devolve o que é vídeo, entrega o resto pelos callbacks, e
    conta para a UI dizer que a chamada está viva.
    """
    pipeline = Pipeline(daemon)
    receptor = _receptor(par, daemon)
    textos: list[str] = []
    audios: list[bytes] = []
    chamada = media.ChamadaRede(
        pipeline, receptor, ao_texto=textos.append, ao_audio=audios.append
    )

    ligacao = _LigacaoFalsa(
        [
            pipeline.cifrar(
                "e aí?", _chave_de(par, "texto", b""), par["identidade"]
            ),
            _envelope_de_audio(pipeline, par, 0, b"\x10" * 80),
            _envelope_de_video(pipeline, par, 5, 233, b"\x01\x02\x03" * 40),
            _envelope_de_audio(pipeline, par, 1, b"\x11" * 80),
        ]
    )

    chamada.correr(ligacao)

    assert textos == ["e aí?"], textos
    assert len(audios) == 2, f"{len(audios)} pacotes de áudio"
    assert chamada.quadros == 1, "um quadro de vídeo"
    assert chamada.audios == 0, "o áudio não é contado como quadro"
    assert (chamada.ultimo_seq, chamada.ultimo_clock_ms) == (5, 233)


def test_o_laco_ignora_frames_que_nao_sao_chat(
    daemon: ClienteIpc, par: dict
) -> None:
    """Um `PING` ou um handshake não é media nem texto.

    Quem os trata é o daemon. Devolvê-los à `Chamada.jsx` seria dar à
    UI trabalho de rede — e um `PING` é a coisa mais frequente que
    passa por uma ligação.
    """
    pipeline = Pipeline(daemon)
    receptor = _receptor(par, daemon)
    chamada = media.ChamadaRede(pipeline, receptor)

    # `0x20` é o `PING` de keepalive (`p2p.rs`).
    ligacao = _LigacaoFalsa([b"ping"], tipo=0x20)
    chamada.correr(ligacao)

    assert chamada.quadros == 0



def test_a_ligacao_que_fecha_acaba_o_laco_sem_excepcao(
    daemon: ClienteIpc, par: dict
) -> None:
    """Uma ligação que fecha é o fim de uma chamada, e não um erro.

    A excepção para isto obrigaria a distinguir «a chamada acabou» de
    «a chamada avariou» — e o `Chamada.jsx` trata as duas do mesmo
    modo. Uma `ErroIpc` do `RECEBER` é a ligação a fechar.
    """
    pipeline = Pipeline(daemon)
    receptor = _receptor(par, daemon)
    chamada = media.ChamadaRede(pipeline, receptor)

    ligacao = _LigacaoFalsa([])  # nada para entregar
    chamada.correr(ligacao)  # não levanta

    assert chamada.quadros == 0


def test_o_timeout_zero_espera_indefinidamente(
    daemon: ClienteIpc, par: dict
) -> None:
    """O `RECEBER` recebe ``0`` por omissão — espera indefinida.

    E é isso que distingue uma chamada de um instante de conversa: não
    há temporizador, e o ritmo é o da ligação.
    """
    pipeline = Pipeline(daemon)
    receptor = _receptor(par, daemon)
    chamada = media.ChamadaRede(pipeline, receptor)

    ligacao = _LigacaoFalsa([])
    chamada.correr(ligacao)

    assert ligacao.timeouts == [0], ligacao.timeouts


# ---------------------------------------------------------------------
# A chamada inteira: emissor → ligação → receptor
# ---------------------------------------------------------------------


def test_a_chamada_va_do_emissor_ao_receptor(
    daemon: ClienteIpc, par: dict
) -> None:
    """O teste que fecha a chamada: um lado emite, o outro recebe.

    E é o primeiro teste que põe os dois lados no mesmo teste. Todos
    os anteriores fixavam uma peça — a cifra, a moldura, a sondagem, o
    limitador — e um sistema feito de peças que passam pode não fechar:
    o `base_seq` pode não bater, ou a chave pode ser a errada, ou a
    ordem pode estar trocada.

    Este teste não pode dar certeza sobre a rede real — não há Tor,
    não há `ffmpeg`, não há ecrã. Pode dar certeza sobre **o caminho**:
    que o que sai do `EmissorStreaming` é o que o `ChamadaRede` separa.
    """
    pipeline = Pipeline(daemon)
    receptor = _receptor(par, daemon)

    # --- quem fala ----------------------------------------------------
    # O wrapper sabe os comandos de media e tem um `enviar` que deixa
    # passar; e o `daemon` da fixture tem de estar ligado para o
    # `enviar` não responder `NaoLigado`, por isso o `enviar` e'
    # substituido por um que **guarda**.
    envolvidos: list[tuple[bytes, bool]] = []
    envolver = _ClienteQueDevolveQuadros(daemon, quadros=1, pacotes=8)
    cliente_do_emissor = envolver.devolver()

    def guardar(envelope: bytes, media: bool = False) -> None:
        """Guarda a unidade e o seu `media`, e não envia.

        E a `media=True` é o que o teste fixa: sem ela, a chamada
        contaria como conversa e batia no tecto de 128 sem chegar a
        30 fps.
        """
        envolvidos.append((envelope, media))

    cliente_do_emissor.enviar = guardar  # type: ignore[method-assign]

    emissor = media.EmissorStreaming(
        pipeline, par["chaves"], par["identidade"], envolver
    )

    # Um quadro de vídeo e uma mensagem de áudio — que são **oito**
    # pacotes de 20 ms agregados (a Fase 2), e não oito mensagens.
    emissor.enviados = 0
    emissor.um_quadro()
    emissor.enviados = 0
    for _ in range(8):
        emissor.um_audio_distinto()

    assert len(envolvidos) == 2, f"{len(envolvidos)} unidades"
    assert all(m for _, m in envolvidos), "todas são media"

    # --- a ligação ---------------------------------------------------
    ligacao = _LigacaoFalsa([e for e, _ in envolvidos])

    # --- quem ouve ----------------------------------------------------
    quadros: list[tuple[int, int, bytes]] = []
    audios_recebidos: list[bytes] = []
    textos: list[str] = []
    chamada = media.ChamadaRede(
        pipeline,
        receptor,
        ao_texto=textos.append,
        ao_audio=audios_recebidos.append,
    )

    # O `ChamadaRede` entrega o quadro como devolve o receptor.
    original = receptor.um_quadro_de_video

    def a_capturar(envelope, ao_texto=None, ao_audio=None):
        r = original(envelope, ao_texto, ao_audio)
        if r is not None:
            quadros.append(r)
        return r

    receptor.um_quadro_de_video = a_capturar
    chamada.correr(ligacao)

    assert len(quadros) == 1, f"{len(quadros)} quadros"
    seq, clock_ms, dados = quadros[0]
    assert seq == 0, f"o seq do primeiro quadro: {seq}"
    assert len(dados) == 400, f"o VP8 chegou com {len(dados)} bytes"
    # A agregação (Fase 2) chega aqui como **uma** mensagem de áudio
    # com os pacotes concatenados — 640 bytes, que são 160 ms a
    # 32 kbit/s (4 bytes por milissegundo).
    #
    # O `ChamadaRede` entrega o corpo como veio, e é quem tem o Opus que
    # o desmultiplexa: concatenar pacotes Opus num corpo é exactamente o
    # que o `ffmpeg -f opus` lê de um ficheiro.
    assert len(audios_recebidos) == 1, f"{len(audios_recebidos)} unidades de áudio"
    assert len(audios_recebidos[0]) == 640, (
        f"o áudio chegou com {len(audios_recebidos[0])} bytes, não 640"
    )
    assert textos == [], "uma chamada não fala"


def test_o_emissor_para_quando_a_captura_acaba(
    daemon: ClienteIpc, par: dict
) -> None:
    """O `ffmpeg` morreu: o laço para e não levanta.

    E os dois laços têm de parar **juntos**. Com dois booleanos
    independentes, o `True` do outro desfazia a paragem do primeiro e
    a chamada continuava a ler de um `ffmpeg` que já não existe.
    """
    pipeline = Pipeline(daemon)
    envolvidos = []

    class _EnvolvidoQueGuarda:
        def enviar(self, envelope: bytes, media: bool = False) -> None:
            envolvidos.append(envelope)

    # Um quadro e nenhum pacote de áudio: o vídeo acaba primeiro.
    involucro = _ClienteQueDevolveQuadros(daemon, quadros=1, pacotes=0)
    cliente = involucro.devolver()
    enviados: list[bytes] = []
    cliente.enviar = lambda e, media=False: enviados.append(e)  # type: ignore[method-assign]

    emissor = media.EmissorStreaming(
        pipeline, par["chaves"], par["identidade"], involucro
    )
    rede = media.EmissorChamadaRede(emissor, involucro)

    assert rede.correr() is True, "a captura liga"
    assert len(enviados) == 1, f"{len(enviados)} unidades"


def test_o_emissor_diz_false_quando_o_ffmpeg_da_recusa(
    daemon: ClienteIpc, par: dict
) -> None:
    """Uma máquina sem ecrã devolve ``False``, e não uma excepção.

    E ``False`` **não** é um erro: é o `ffmpeg` a dizer que a máquina
    não tem ecrã ou que a geometria não serve, e quem chama decide se
    isso é uma falha ou um motivo para não ligar. Uma excepção
    obrigaria cada chamador a apanhar uma coisa que não é excepcional.
    """
    pipeline = Pipeline(daemon)

    class _QueRecusa:
        def enviar(self, envelope: bytes, media: bool = False) -> None:
            pass

        def media_capturar(self, *args: int) -> None:
            raise ErroDaemon(ERR_MEDIA, "Unknown input format: 'x11grab'")

        def media_ler(self):  # type: ignore[no-untyped-def]
            raise AssertionError("não devia ler: a captura não ligou")

    emissor = media.EmissorStreaming(
        pipeline, par["chaves"], par["identidade"], _QueRecusa()  # type: ignore[arg-type]
    )
    rede = media.EmissorChamadaRede(emissor, _QueRecusa())  # type: ignore[arg-type]

    assert rede.correr() is False, "a captura não ligou"
    assert rede.ligada is False


def test_o_clock_ms_do_audio_e_o_tempo_acumulado(
    daemon: ClienteIpc, par: dict
) -> None:
    """O relógio do áudio conta **tempo**, e não pacotes.

    E o teste é sobre pacotes de **tamanhos diferentes**, que é onde a
    diferença aparece: o `ffmpeg` não garante que todos os pacotes
    tenham os mesmos 20 ms, e uma rajada produz um maior. Com
    `self.audios * ms` — índice em vez de tempo — o relógio saltava
    para o fim da chamada sem que nada désse conta, que é exactamente
    o «desvio de relógio» que §5.3 existe para detectar.

    ## E a agregação (Fase 2) não estraga isto

    A agregação junta pacotes antes de mandar, e a soma dos tempos de um
    corpo pode ser lida do próprio corpo: a 32 kbit/s são 4 bytes por
    milissegundo, e o corpo é a concatenação de pacotes válidos. É essa
    soma que tem de aparecer no relógio da mensagem **seguinte** — se o
    relógio usasse o índice, saltaria a cada rajada.
    """
    pipeline = Pipeline(daemon)
    # 80 bytes = 20 ms, 160 = 40 ms, 80 = 20 ms, repetido. Uma rajada
    # produz um pacote maior, e o Opus tem pacotes de duração variável.
    #
    # Dezasseis pacotes chegam a 480 ms, o que dá três mensagens de
    # 160 ms — e o limitador de §8.0 precisa de vídeo que as justifique,
    # porque a janela é 200 ms.
    envolvido = _ClienteQueDevolveQuadros(daemon, quadros=5, pacotes=16)
    envolvido.tamanhos = [80, 160, 80] * 6
    cliente = envolvido.devolver()

    with a_registar(cliente) as registos:
        emissor = media.EmissorStreaming(
            pipeline, par["chaves"], par["identidade"], envolvido
        )
        for _ in range(5):
            emissor.um_quadro()
        for _ in range(16):
            emissor.um_audio_distinto()

    audios = [
        media.abrir(b.decode())
        for b, _ in registos
        if b and media.abrir(b.decode()).tipo == "audio"
    ]
    assert len(audios) >= 2, f"só {len(audios)} mensagens de áudio nos 16 pacotes"

    # O tempo de uma mensagem lê-se do seu próprio corpo: a 32 kbit/s
    # são 4 bytes por milissegundo, e o corpo é a concatenação.
    def ms_do_corpo(corpo: bytes) -> int:
        return len(corpo) // 4

    # E o relógio de cada mensagem é a soma das anteriores. É esta
    # asserção que prova que o relógio conta **tempo**: a segunda
    # mensagem começa onde a primeira acabou, e não num índice.
    acumulado = 0
    for i, m in enumerate(audios):
        assert m.clock_ms == acumulado, (
            f"a mensagem {i} tem clock_ms {m.clock_ms} e o acumulado é "
            f"{acumulado}: o relógio está a contar pacotes, não tempo"
        )
        acumulado += ms_do_corpo(m.corpo)

    # E o tempo total dos corpos bate com o que o relógio acumulou, que
    # é a propriedade de «não deriva» de §5.3.
    total_ms = sum(ms_do_corpo(m.corpo) for m in audios)
    assert total_ms == acumulado, f"{total_ms} ms nos corpos, {acumulado} no relógio"
