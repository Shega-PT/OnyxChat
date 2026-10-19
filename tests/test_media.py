# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_media.py — o módulo de media do lado do Python.

Confere :mod:`messenger.media` contra os mesmos vectores que o Rust
congela em ``tests/vectors/media.json``, gerados por
``cargo run -p onyxchatd --example gerar_vetores``.

Os vectors cobrem duas famílias:

* **chaves** — a ``K1`` derivada por tipo de conteúdo, verificada
  também por ``tests/test_amizade.py``;
* **molduras** — o preâmbulo ``ONX1``, incluindo o vector ``104``, que
  fixa o limite de ``i32::MIN``, e o ``103``, que é a moldura sem
  corpo — o ``HOLD`` de ``docs/media.md`` §7.4.

O vector ``102`` é o que sustenta o formato inteiro: um ``clock_ms``
negativo é ``FF FF FF 06`` em complemento de dois, e ``FF`` não é
UTF-8 válido. Uma versão anterior da moldura punha campos binários no
plaintext, e o ``ENCODE`` passava a recusá-lo com ``0x08
TextoInvalidoUtf8`` — uma falha que não dizia nada sobre a sua causa.

Os testes de propriedades vêm a seguir, e verificam o que os vectores
não conseguem: que a ida e volta vale para *qualquer* entrada, e não
só para os oito casos que os vectores fixam.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from messenger import amizade, media
from messenger.amizade import ErroDerivacao, derivar_k1_por_tipo

RAIZ = Path(__file__).resolve().parent.parent
VECTORES = json.loads((RAIZ / "tests/vectors/media.json").read_text(encoding="utf-8"))

#: As três chaves de teste, iguais às de `crypto/rust/src/amizade.rs`.
KA = bytes([0x11] * 32)
KB = bytes([0x22] * 32)
PUB_A = bytes([0xAA] * 32)
PUB_B = bytes([0xBB] * 32)
NONCE = bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 0x0A, 0x0B, 0x0C])


def _por_familia(nome: str) -> list[dict]:
    return [v for v in VECTORES["vetores"] if v["familia"] == nome]


# ---------------------------------------------------------------------
# As chaves por tipo
# ---------------------------------------------------------------------


def test_as_chaves_reproduzem_os_vetores():
    """Cada chave derivada bate com o valor que o Rust congelou."""
    for v in _por_familia("chaves"):
        derivada = derivar_k1_por_tipo(
            bytes.fromhex(v["k_emissor_hex"]),
            bytes.fromhex(v["k_recetor_hex"]),
            bytes.fromhex(v["publica_emissor_hex"]),
            v["tipo"],
        )
        assert derivada.hex() == v["saida_hex"], (
            f'vector {v["id"]}: a K1 derivada mudou — '
            "é uma quebra de formato, não um bug"
        )


def test_o_texto_nao_e_derivado():
    """A chave de texto sai igual à que entrou, byte a byte.

    A propriedade de ``docs/media.md`` §1.2 que garante que o texto
    continua byte-a-byte igual ao que produzia antes de existir media.

    ## E porque o texto **entra** no dicionário

    A primeira versão deste teste afirmava ``"texto" not in derivadas``.
    A mudança é deliberada: com os tipos de controlo (`call`,
    `ficheiro`), quem sonda um envelope precisa de ter **todas** as
    chaves num sítio só, e um dicionário sem o texto obrigaria o
    chamador a lembrar de o juntar à mão — o que é exactamente o
    caminho para um envelope de texto deixar de abrir.

    O que a invariante exige não é que o texto esteja fora do
    dicionário; é que a **sua chave não seja derivada**. E continua
    não a ser: o valor é a ``k_texto`` que entrou, byte a byte.
    """
    k_texto = bytes([0x55] * 32)
    derivadas = media.chaves_media(k_texto, KA, KB, PUB_A)

    assert derivadas["texto"] == k_texto, "o texto tem de ser a chave que entrou, sem derivar"

    # E os quatro tipos derivados têm de ser distintos uns dos outros e
    # do texto — uma colisão daria duas mensagens de tipos diferentes a
    # abrir com a mesma chave.
    derivadas_de_media = {
        nome: v for nome, v in derivadas.items() if nome != "texto"
    }
    assert len(set(derivadas_de_media.values())) == len(derivadas_de_media), (
        f"dois tipos partilham chave: {derivadas_de_media}"
    )
    for nome, chave in derivadas_de_media.items():
        assert chave != k_texto, f"«{nome}» devolveu a chave do texto"


def test_um_tipo_desconhecido_nao_deriva():
    """Um tipo fora da lista recusa, em vez de produzir uma chave inútil.

    A alternativa — devolver 32 bytes para qualquer ``tipo`` — geraria
    chaves que ninguém sabe distinguir umas das outras, e o erro
    apareceria como «a imagem não abre», muito longe da sua causa.

    E os valores que **foram** ocupados por `call` e `ficheiro` antes de
    os tipos existirem (0x03, 0x04) agora são **conhecidos**, porque os
    valores de derivação são congelados: reatribuí-los a outro tipo
    abriria envelopes antigos com a chave do tipo novo, e o sintoma é
    texto ilegível em vez de um erro.
    """
    for tipo in (0x00, 0x05, 0x10, 0xFF):
        with pytest.raises(ErroDerivacao, match="tipo de media desconhecido"):
            derivar_k1_por_tipo(KA, KB, PUB_A, tipo)


def test_a_ordem_de_sondagem_comeca_pelo_texto():
    """O texto é a primeira tentativa, porque é o caso comum.

    Uma conversa normal custa **uma** verificação. Se o texto fosse o
    último, todas as mensagens do sistema pagariam três verificações
    Poly1305 para nada.

    ## E os tipos de controlo vêm no fim, por uma razão de custo

    A ordem é `texto`, os dois de media, e os dois de controlo. Um
    convite de chamada é raro comparado com uma conversa, e pagar
    quatro verificações por convite em vez de duas não é nada — mas
    pagar quatro verificações por **cada mensagem de texto** seria.
    """
    ordem = media.order_de_sondagem()
    assert ordem[0] == "texto"
    # E a ordem completa, com os tipos de controlo no fim.
    #
    # A ordem é a mesma que a do Rust (`media/teclas.rs::ORDEM`), e
    # divergirem daria um tipo que um dos lados abre e o outro não.
    assert ordem[1:] == ("audio", "video", "ficheiro", "call"), ordem[1:]


def test_a_ordem_de_sondagem_cobre_exatamente_os_tipos_com_chave():
    """A ordem de sondagem e :data:`TIPOS_DE_CHAVE` não podem divergir.

    São duas fontes de verdade sobre os mesmos tipos, e divergirem daria
    um envelope de `call` que ninguém sonda — que se traduz num erro de
    «nenhuma das chaves abriu», sem dizer qual tipo falta.
    """
    ordem = media.order_de_sondagem()
    assert set(ordem) == {"texto", *media.TIPOS_DE_CHAVE}, (
        f"a ordem {ordem} e os tipos {sorted(media.TIPOS_DE_CHAVE)} não são o mesmo conjunto"
    )
    # E a ordem é a mesma que o Rust declara, tipo a tipo. Ver a nota em
    # `order_de_sondagem`.
    assert ordem == ("texto", *media.TIPOS_DE_CHAVE)


def test_a_sondagem_devolve_o_tipo_que_abriu():
    """Cada chave é a que abre, e o texto ganha ao resto."""
    k_texto = bytes([0x55] * 32)
    derivadas = media.chaves_media(k_texto, KA, KB, PUB_A)

    def com_chave_que_abre(chave: bytes) -> object:
        return (chave == k_texto, b"sou texto")

    # A closure devolve `True` (abriu) para o texto e `False` para o
    # resto, e a sondagem tem de parar no primeiro.
    nome, texto = media.sondar(
        {"texto": k_texto, **derivadas}, lambda c: b"sou texto" if c == k_texto else None
    )
    assert nome == "texto"
    assert texto == b"sou texto"

    for esperado in ("audio", "video"):
        alvo = derivadas[esperado]
        nome, _ = media.sondar(
            {"texto": k_texto, **derivadas}, lambda c, a=alvo: b"media" if c == a else None
        )
        assert nome == esperado


def test_nenhuma_chave_abre_da_erro():
    """Sem chave que abra, o erro é único e não sugere o próximo passo."""
    derivadas = media.chaves_media(bytes([0x55] * 32), KA, KB, PUB_A)
    with pytest.raises(media.ErroMedia, match="nenhuma das chaves"):
        media.sondar({"texto": bytes([0x55] * 32), **derivadas}, lambda _c: None)


# ---------------------------------------------------------------------
# A moldura
# ---------------------------------------------------------------------


def test_as_molduras_reproduzem_os_vectores():
    """Cada moldura bate com o texto que o Rust congelou."""
    for v in _por_familia("molduras"):
        # O vector traz o que vai no fio — o carácter — e a `Moldura`
        # do Python usa o nome. A tradução é a de `media.py`, não uma
        # feita aqui à mão.
        m = media.Moldura(
            tipo=media.TIPOS_POR_CARACTERE[v["tipo"]],
            modo=media.MODOS[v["modo"].encode("ascii")],
            stream=v["stream"],
            seq=v["seq"],
            clock_ms=v["clock_ms"],
            flags=v["flags"],
            corpo=bytes.fromhex(v["corpo_hex"]),
        )
        assert m.texto == v["texto"], f'vector {v["id"]}: o texto mudou'
        assert len(m.para_bytes()) == v["comprimento"], f'vector {v["id"]}: o comprimento mudou'


def test_o_plaintext_e_ascii_para_valores_extremos():
    """ASCII para tudo, incluindo ``i32::MIN``.

    É o teste que cobre o bug de formato: campos binários punham ``FF``
    no plaintext, e ``FF`` não é UTF-8 válido. O ``ENCODE`` recusaria com
    ``0x08`` e a falha apareceria como «a media não envia».
    """
    for clock in (0, 1, -1, 2**31 - 1, -(2**31), -250):
        for seq in (0, 1, 2**32 - 1):
            m = media.Moldura(
                tipo="video", modo="streaming", stream=0xFFFF, seq=seq, clock_ms=clock
            )
            bruto = m.para_bytes()
            assert bruto.isascii(), f"clock {clock} / seq {seq} produziu bytes não ASCII"
            assert media.abrir(bruto.decode("ascii")).clock_ms == clock


def test_as_molduras_fazem_ida_e_volta():
    """Todos os campos sobrevivem à volta, e o corpo também."""
    corpos = [b"", b"a", b"abc", b"payload", bytes(range(256))]
    for corpo in corpos:
        for flags in (0, 1, 2, 4, 7):
            m = media.Moldura(
                tipo="audio",
                modo="nao-streaming",
                stream=4660,
                seq=7,
                clock_ms=-250,
                flags=flags,
                corpo=corpo,
            )
            a = media.abrir(m.texto)
            assert a.tipo == "audio"
            assert a.modo == "nao-streaming"
            assert a.stream == 4660
            assert a.seq == 7
            assert a.clock_ms == -250
            assert a.flags == flags
            assert a.corpo == corpo


def test_a_moldura_e_legivel():
    """O preâmbulo lê-se sem descodificador — a razão dos algarismos."""
    m = media.Moldura(
        tipo="video",
        modo="streaming",
        stream=4660,
        seq=7,
        clock_ms=-250,
        flags=media.FLAGS["keyframe"],
        corpo=b"",
    )
    cabecalho = m.texto[: media.INICIO_CORPO]
    assert cabecalho == "ONX1V1S2046600000000007-00000000250"

    # E cada campo está no offset que a spec diz — o que a comparação
    # acima não prova, porque as duas funções partilham a mesma
    # constante errada.
    assert cabecalho[media.INICIO_TIPO] == "V"
    assert cabecalho[media.INICIO_VERSAO] == "1"
    assert cabecalho[media.INICIO_MODO] == "S"
    assert cabecalho[media.INICIO_FLAGS] == "2"  # só a keyframe
    assert cabecalho[media.INICIO_STREAM : media.INICIO_STREAM + media.LARGURA_STREAM] == "04660"
    assert cabecalho[media.INICIO_SEQ : media.INICIO_SEQ + media.LARGURA_SEQ] == "0000000007"
    assert cabecalho[media.INICIO_CLOCK : media.INICIO_CLOCK + media.LARGURA_CLOCK] == "-00000000250"


def test_nenhum_erro_de_abertura_aponta_para_o_conteudo():
    """Nenhuma mensagem de erro leva o que a produziu.

    ``docs/security_model.md`` exige que nenhum erro carregue plaintext.
    Um erro que diga «esperava A e veio B» com caracteres do corpo seria
    uma fuga disfarçada de diagnóstico — e é por isso que o teste
    verifica o **conteúdo** e não a excepção.
    """
    segredo = "ONYX-CONTIDO-NAO-PODE-VAZAR-1234567890"
    m = media.Moldura(
        tipo="video", modo="nao-streaming", stream=0, seq=0, clock_ms=0, corpo=segredo.encode()
    )
    tentativas = [
        m.texto[:10],  # truncada
        "X" + m.texto[1:],  # magic trocada
        m.texto[:5] + "9" + m.texto[6:],  # versão desconhecida
        m.texto[: media.INICIO_TIPO] + "Z" + m.texto[media.INICIO_TIPO + 1 :],  # tipo
        m.texto[: media.INICIO_FLAGS] + "9" + m.texto[media.INICIO_FLAGS + 1 :],  # flags
        m.texto.replace("\n", "\r"),  # separador
        "ONX1V1S00000000000000000+00000000000\n!!!!",  # base-64 inválido
    ]
    for tentativa in tentativas:
        with pytest.raises(media.ErroMedia) as info:
            media.abrir(tentativa)
        mensagem = str(info.value)
        assert "CONTIDO" not in mensagem, f"o erro deixou ver o conteúdo: {mensagem}"
        assert "NAO-PODE" not in mensagem, f"o erro deixou ver o conteúdo: {mensagem}"


def test_a_geometria_esta_em_acordo_com_a_spec():
    """Os números do código, do vector e da spec são o mesmo número."""
    assert media.TAM_PRE_AMBULO == 36
    assert media.INICIO_CORPO == 35
    assert VECTORES["pre_ambulo"] == media.TAM_PRE_AMBULO
    assert VECTORES["limite_media"] == media.TAM_MEDIA_MAX
    # E o limite tem de caber mesmo: é um limite, não uma promessa.
    folha = media.Moldura(
        tipo="video",
        modo="nao-streaming",
        stream=0,
        seq=0,
        clock_ms=0,
        corpo=bytes(media.TAM_MEDIA_MAX),
    )
    assert len(folha.para_bytes()) <= media.MAX_PLAINTEXT


# ---------------------------------------------------------------------
# Propriedades
# ---------------------------------------------------------------------


@settings(max_examples=200, deadline=None)
@given(
    tipo=st.sampled_from(sorted(media.TIPOS)),
    modo=st.sampled_from(sorted(media.MODOS.values())),
    stream=st.integers(min_value=0, max_value=0xFFFF),
    seq=st.integers(min_value=0, max_value=0xFFFFFFFF),
    clock_ms=st.integers(min_value=-(2**31), max_value=2**31 - 1),
    flags=st.integers(min_value=0, max_value=7),
    corpo=st.binary(max_size=512),
)
def test_propriedade_ida_e_volta(tipo, modo, stream, seq, clock_ms, flags, corpo):
    """Qualquer moldura bem formada volta com os mesmos campos.

    Os vectores fixam oito casos. Esta propriedade cobre o espaço
    inteiro — e é ela que apanha um ``clock_ms`` no limite, uma flag
   combination que ninguém pensou, e um corpo de comprimento ímpar que
    obriga ao preenchimento.
    """
    m = media.Moldura(
        tipo=tipo, modo=modo, stream=stream, seq=seq, clock_ms=clock_ms, flags=flags, corpo=corpo
    )
    a = media.abrir(m.texto)
    assert a.tipo == tipo
    assert a.modo == modo
    assert a.stream == stream
    assert a.seq == seq
    assert a.clock_ms == clock_ms
    assert a.flags == flags
    assert a.corpo == corpo


@settings(max_examples=200, deadline=None)
@given(
    texto=st.binary(max_size=128).map(
        lambda b: b.decode("utf-8", "ignore") if all(c < 0x80 for c in b) else "ONX1V1S0"
    ),
)
def test_abrir_nunca_entra_em_panico(texto):
    """Nada faz ``abrir`` levantar nada que não seja ``ErroMedia``.

    O alvo de fuzzing ``fuzz/fuzz_targets/media_parser.rs`` vai repetir
    isto com milhões de combinações; aqui são duzentas e legíveis.
    """
    try:
        media.abrir(texto)
    except media.ErroMedia:
        pass


@settings(max_examples=100, deadline=None)
@given(corpo=st.binary(max_size=256))
def test_o_corpo_e_ascii_qualquer_que_seja(corpo):
    """Corpo com qualquer byte continua a dar um plaintext ASCII.

    A propriedade que o ``ENCODE`` exige: um corpo de ``0x00`` a
    ``0xFF`` tem de sair como base-64 ASCII, e não como os bytes que
    entrou. É o que garante que nenhum payload produz um plaintext que
    o daemon recuse com ``0x08``.
    """
    m = media.Moldura(tipo="video", modo="streaming", stream=0, seq=0, clock_ms=0, corpo=corpo)
    assert m.para_bytes().isascii()
    assert media.abrir(m.texto).corpo == corpo


# ---------------------------------------------------------------------
# O manifesto de transferência
# ---------------------------------------------------------------------


def test_os_manifestos_reproduzem_os_vectores():
    """Cada manifesto bate com o que o Rust congelou.

    O vector ``204`` é o que importa: ``tipo`` vídeo com ``codec`` Opus.
    Sem o ``tipo`` como campo independente, o Python derivá-lo-ia do
    ``codec`` e leria o manifesto como áudio — com a **chave errada**,
    e sem qualquer erro pelo caminho.
    """
    for v in _por_familia("manifestos"):
        total = v["total_bytes"]
        conteudo = bytes(((i * 17) + 3) % 256 for i in range(total))

        m = media.Manifesto.novo(
            tipo=media.TIPOS_POR_CARACTERE[v["tipo"]],
            codec=_nome_de(media.CODECS, v["codec"]),
            formato=_nome_de(media.FORMATOS, v["formato"]),
            duracao_s=v["duracao_s"],
            nome=v["nome"],
            conteudo=conteudo,
        )

        assert m.sha256.hex() == v["sha256_hex"], f'vector {v["id"]}: o resumo mudou'
        assert m.id == v["id_hex"], f'vector {v["id"]}: o id mudou'
        assert m.total_pedacos == v["total_pedacos"], f'vector {v["id"]}: as fatias mudaram'
        assert m.bytes_por_pedaco == v["bytes_por_pedaco"], f'vector {v["id"]}: a fatia mudou'
        assert media.moldura_de_manifesto(m, 1, 0) == v["texto"], f'vector {v["id"]}: o texto mudou'

        # E a ida e volta, que é o que prova que o `tipo` sobrevive.
        lido = media.Manifesto.de_corpo(m.corpo())
        assert lido.tipo == media.TIPOS_POR_CARACTERE[v["tipo"]]
        assert lido.nome == v["nome"]
        assert lido.sha256 == m.sha256


def _nome_de(tabela: dict, valor: int) -> str:
    for v, nome in tabela.items():
        if v == valor:
            return nome
    raise AssertionError(f"{valor:#06x} não está na tabela")


def test_o_tipo_sobrevive_a_um_codec_que_daria_outro_tipo():
    """Vídeo com Opus dentro é vídeo, e não áudio.

    O caso que forçou o ``tipo`` a ser um campo do manifesto: o codec
    Opus é o conteúdo de **áudio**, e um vídeo com uma faixa de áudio
    Opus tem-no. Derivado do codec, o receptor escolheria a chave de
    áudio para um conteúdo que é vídeo — e a mensagem não abriria.
    """
    m = media.Manifesto.novo("video", "opus", "matroska", 90, "v.mkv", b"" * 100)
    lido = media.Manifesto.de_corpo(m.corpo())

    assert lido.tipo == "video"
    assert lido.codec == "opus"
    # E os dois são independentes: trocar o tipo não muda o codec.
    outro = media.Manifesto.de_corpo(
        media.Manifesto.novo("audio", "opus", "matroska", 90, "a.mkv", b"\x01" * 100).corpo()
    )
    assert outro.tipo == "audio"
    assert outro.codec == lido.codec == "opus"


def test_um_ficheiro_vazio_tem_zero_fatias():
    """Zero, e a coerência é o ponto.

    Um manifesto que dissesse «1 fatia» para um ficheiro vazio faria o
    receptor esperar uma fatia que nunca chega — e a transferência
    ficaria pendurada para sempre, sem erro em lado nenhum.
    """
    assert media.numero_de_fatias(0) == 0
    assert media.fatiar(b"") == []

    m = media.Manifesto.novo("video", "matroska", "matroska", 0, "vazio.mkv", b"")
    assert m.total_pedacos == 0
    assert m.total_bytes == 0
    m.validar()  # coerente

    montador = media.Montador(m)
    assert montador.faltam() == 0
    assert montador.montar() == b"", "um ficheiro vazio monta de imediato"


def test_zero_fatias_com_conteudo_e_recusado():
    """A condição invertida é tão incoerente como a outra."""
    base = media.Manifesto.novo("video", "matroska", "matroska", 1, "v.mkv", b"x" * 100).corpo()

    # Um manifesto de conteúdo com zero fatias.
    corpo = bytearray(base)
    corpo[media.INICIO_PEDACOS : media.INICIO_BYTES_PEDACO] = (0).to_bytes(4, "big")
    with pytest.raises(media.ErroMedia, match="incoerente"):
        media.Manifesto.de_corpo(bytes(corpo))

    # E um manifesto de zero bytes com uma fatia.
    corpo = bytearray(base)
    corpo[media.INICIO_TOTAL : media.INICIO_PEDACOS] = (0).to_bytes(4, "big")
    with pytest.raises(media.ErroMedia, match="incoerente"):
        media.Manifesto.de_corpo(bytes(corpo))


def test_a_fatia_trocada_e_apanhada_pelo_resumo():
    """Uma fatia do mesmo tamanho mas com outros bytes não passa.

    É o caso que a verificação de tamanho não apanha — e que é o que o
    resumo apanha. Sem ele, o receptor tocaria um ficheiro que não é o
    que foi enviado, e não haveria nenhum sinal.
    """
    original = bytes((i * 31 + 7) % 256 for i in range(media.TAM_FATIA + 100))
    m = media.Manifesto.novo("video", "matroska", "matroska", 5, "v.mkv", original)
    partes = m.fatias(original)

    montador = media.Montador(m)
    # O `seq` é o da rede: o manifesto é o 0, as fatias começam no 1.
    for seq, parte in enumerate(partes, start=1):
        montador.receber(seq, parte)
    assert montador.montar() == original

    montador = media.Montador(m)
    montador.receber(1, partes[0])
    montador.receber(2, b"\x00" * media.TAM_FATIA)
    with pytest.raises(media.ErroMedia, match="SHA-256"):
        montador.montar()


def test_as_fatias_podem_chegar_desordenadas():
    """O ``seq`` ordena, e a ordem de chegada não entra."""
    original = bytes((i * 13 + 5) % 256 for i in range(3 * media.TAM_FATIA + 5))
    m = media.Manifesto.novo("video", "matroska", "matroska", 5, "v.mkv", original)
    partes = m.fatias(original)
    assert len(partes) == 4, "3 fatias cheias e uma com 5 bytes"

    montador = media.Montador(m)
    montador.receber(4, partes[3])
    montador.receber(2, partes[1])
    assert montador.indices_em_falta() == [0, 2]
    montador.receber(1, partes[0])
    montador.receber(3, partes[2])

    assert montador.montar() == original, "a montagem seguiu a ordem de chegada"


def test_o_seq_da_reserva_o_manifesto():
    """O manifesto é o ``seq`` 0, e por isso as fatias começam no 1.

    A sequência do fluxo tem de ser a mesma para as duas peças. Se as
    fatias começassem no 0, o manifesto e a primeira fatia teriam o
    mesmo ``seq`` — e um receptor que ordenasse por ele não saberia
    qual dos dois veio primeiro.
    """
    original = b"z" * 200
    m = media.Manifesto.novo("audio", "opus", "ogg", 9, "a.opus", original)
    montador = media.Montador(m)

    with pytest.raises(media.ErroMedia, match="manifesto é o 0"):
        montador.receber(0, original)

    montador.receber(1, original)
    assert montador.montar() == original


def test_a_meio_da_transferencia_da_none_e_nao_erro():
    """Um ficheiro a meio da transferência não é um erro."""
    original = bytes((i * 3) % 256 for i in range(3 * media.TAM_FATIA))
    m = media.Manifesto.novo("video", "matroska", "matroska", 5, "v.mkv", original)
    montador = media.Montador(m)

    assert montador.montar() is None
    montador.receber(1, original[: media.TAM_FATIA])
    assert montador.montar() is None
    assert montador.faltam() == 2


def test_o_nome_com_nulos_e_recusado():
    """Um zero no meio do nome é recusado nas duas portas."""
    with pytest.raises(media.ErroMedia, match="nulos"):
        media.Manifesto.novo("video", "matroska", "matroska", 1, "a\x00b.mkv", b"x")

    # E do lado da leitura, com um nome injectado no corpo.
    m = media.Manifesto.novo("video", "matroska", "matroska", 1, "ok.mkv", b"x" * 10)
    corpo = m.corpo()[: media.TAM_MANIFESTO_FIXO] + media.codificar(b"a\x00b").encode("ascii")
    with pytest.raises(media.ErroMedia, match="nulos"):
        media.Manifesto.de_corpo(corpo)


def test_a_maior_fatia_cabe_e_a_seguinte_nao():
    """A propriedade que liga este módulo ao ``MAX_PLAINTEXT``."""
    m = media.Manifesto.novo("video", "matroska", "matroska", 1, "v.mkv", b"x" * media.TAM_FATIA)

    cheia = media.moldura_de_pedaco(m, 1, 0, b"\x00" * media.TAM_FATIA)
    assert len(cheia.encode("ascii")) <= media.MAX_PLAINTEXT

    excesso = media.moldura_de_pedaco(m, 1, 0, b"\x00" * (media.TAM_FATIA + 1))
    assert len(excesso.encode("ascii")) > media.MAX_PLAINTEXT


def test_a_geometria_do_manifesto_esta_em_acordo():
    """Os números do código, do vector e da spec são o mesmo número."""
    assert media.TAM_MANIFESTO_FIXO == 51
    assert media.TAM_FATIA == media.TAM_MEDIA_MAX
    assert media.INICIO_TOTAL == 1
    assert media.FIM_NOME == 51
    # E o resumo bate com a implementação de fora.
    assert media.resumo(b"abc").hex() == (
        "ba7816bf8f01cfea414140de5dae2223"
        "b00361a396177a9cb410ff61f20015ad"
    )


# ---------------------------------------------------------------------
# Os tipos de controlo
# ---------------------------------------------------------------------


def test_os_tipos_de_controle_derivam_e_sao_distintos():
    """``call`` e ``ficheiro`` têm chave própria, e ela é diferente.

    É a propriedade de que depende tudo o resto: um convite de
    chamada e um ficheiro têm de abrir com chaves que **só o par**
    conhece, e que são diferentes entre si.

    Sem a distinção, um convite seria um envelope de texto e a rede
    veria uma conversa — que é o que o projecto recusa em
    ``docs/media.md`` §1.2.
    """
    k_texto = bytes([0x55] * 32)
    derivadas = media.chaves_media(k_texto, KA, KB, PUB_A)

    for nome in ("call", "ficheiro"):
        assert nome in derivadas, f"«{nome}» não deriva chave"
        assert derivadas[nome] != k_texto, f"«{nome}» devolveu a chave do texto"

    assert derivadas["call"] != derivadas["ficheiro"], "convite e ficheiro partilham chave"
    assert derivadas["call"] != derivadas["audio"], "convite e áudio partilham chave"
    assert derivadas["call"] != derivadas["video"], "convite e vídeo partilham chave"


def test_o_tipo_de_um_envelope_de_controlo_diz_o_que_e():
    """A sondagem devolve o nome do tipo que abriu.

    Sem isto, um receptor de chamada teria de adivinhar pelo conteúdo, e
    um ficheiro que principiasse por `ONYX-CALL` seria lido como um
    convite — o que é a mesma classe de bug que ler um pacote de áudio
    como se fosse um vídeo.
    """
    derivadas = media.chaves_media(bytes([0x55] * 32), KA, KB, PUB_A)

    for nome in ("call", "ficheiro"):
        chave = derivadas[nome]

        # A sondagem recebe uma chave e devolve o plaintext se a
        # etiqueta verificar, ou `None`. Aqui o «plaintext» é o nome do
        # tipo que abriu, o que torna a falha legível: se a sondagem
        # parasse no tipo errado, a comparação abaixo diz qual.
        def tentar(c: bytes, esperado: str = nome) -> str | None:
            return f"corpo de {esperado}" if c == chave else None

        tipo_aberto, corpo = media.sondar(derivadas, tentar)
        assert tipo_aberto == nome, f"«{nome}» foi lido como «{tipo_aberto}»"
        assert corpo == f"corpo de {nome}"


def test_o_valor_do_tipo_de_derivacao_esta_congelado():
    """Os bytes de tipo não mudam, e o teste diz porquê.

    Um valor de derivação é um **contrato**: mudar ``TIPO_CALL`` de
    ``0x03`` para outra coisa faria um convite antigo abrir com a chave
    nova — sem erro, sem aviso, e com texto ilegível. O sintoma está
    longe da causa, que é a pior propriedade possível num protocolo.
    """
    assert amizade.TIPO_AUDIO == 0x01
    assert amizade.TIPO_VIDEO == 0x02
    assert amizade.TIPO_CALL == 0x03
    assert amizade.TIPO_FICHEIRO == 0x04

    # E o Rust tem de ter os mesmos quatro. É o teste que impede as
    # duas implementações de divergirem: o cliente deriva a chave e o
    # daemon valida a etiqueta, e um valor diferente num dos lados
    # daria um erro de descifragem que não menciona tipos.
    rust = Path(__file__).resolve().parent.parent / "crypto" / "rust" / "src" / "amizade.rs"
    texto_rust = rust.read_text(encoding="utf-8")
    for nome in ("TIPO_AUDIO", "TIPO_VIDEO", "TIPO_CALL", "TIPO_FICHEIRO"):
        valor = getattr(amizade, nome)
        # O Rust escreve os bytes em minúsculas e o Python não precisa de
        # os declarar; a comparação é feita sobre ambos para aceitar os dois.
        declarado = f"pub const {nome}: u8 = 0x{valor:02x};"
        assert declarado in texto_rust, (
            f"o Rust não declara «{declarado}» e o Python diz 0x{valor:02X}: "
            "as duas implementações derivam chaves diferentes"
        )


def test_a_ordem_e_a_tabela_dos_tipos_nao_podem_divergir():
    """Duas fontes de verdade sobre os mesmos tipos, e um teste que as liga.

    A ordem de sondagem e :data:`TIPOS_DE_CHAVE` dizem a mesma coisa de
    duas maneiras. Divergirem daria um tipo de que ninguém tem chave —
    e o erro seria «nenhuma das chaves abriu este envelope», que não
    menciona tipos e por isso não aponta para a causa.
    """
    assert media.order_de_sondagem() == ("texto", *media.TIPOS_DE_CHAVE)
    assert set(media.TIPOS_DE_CHAVE) == {"audio", "video", "call", "ficheiro"}


def test_o_texto_da_chave_continua_byte_a_byte():
    """A invariante de ``docs/media.md`` §1.2, verificada depois de tudo.

    Derivar quatro chaves novas não pode mudar o que o texto produz.
    A chave de texto é a ``k_texto`` que entrou, e o ``ENCODE`` com ela
    dá exactamente o envelope de antes de existir media.
    """
    k_texto = bytes([0x55] * 32)
    derivadas = media.chaves_media(k_texto, KA, KB, PUB_A)
    assert derivadas["texto"] is not None
    assert derivadas["texto"] == k_texto, "a chave de texto foi derivada e não devia"
