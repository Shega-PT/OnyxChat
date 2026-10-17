# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_media_receptor.py — o caminho de recepção de media.

F4 deixou o emissor, a loja e as rotas, e **não** deixou isto: nada
escrevia uma transferência recebida na loja. Sem este ficheiro, o
`MEDIA_REPRODUZIR` tinha um esquema, rotas e um daemon — e nenhum
conteúdo para reproduzir.

O que estes testes tentam estragar, por ordem:

1. **Perder uma transferência** por causa de ordem: um relay reordena,
   e um manifesto que chega em último não pode deitar fora as 42
   fatias que chegaram primeiro.
2. **Escrever uma órfã**: uma linha que aponte para uma transferência
   que não existe fica na conversa para sempre, e nem a contagem nem a
   eliminação a alcançam.
3. **Marcar completa antes do resumo**: `completo` sem conferência dá
   um ficheiro truncado a tocar.
4. **Confundir texto com media**: uma conversa normal não tem media
   nenhuma, e o receptor tem de devolver `None` sem tocar em nada.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from messenger import media
from server.loja import Loja

CONTENTO = bytes((i * 37 + 11) % 256 for i in range(3 * media.TAM_FATIA + 137))


@pytest.fixture
def loja(tmp_path: Path):
    with Loja(tmp_path / "loja.db") as aberta:
        pedido = aberta.registar_pedido("ONYX-AAAAAA-!A!A#", "Ana Silva", "Olá")
        contacto = aberta.decidir_pedido(pedido["id"], "aceite")["contactoId"]
        yield aberta, aberta.abrir_conversa(contacto)["id"]


def _quadros(loja, conversa: str, conteudo: bytes = CONTENTO, **mudar):
    """As molduras de uma transferência, como o receptor as receberia."""
    manifesto = media.Manifesto.novo(
        "video", "matroska", "matroska", 42, "praia.mkv", conteudo, **mudar
    )
    fatias = manifesto.fatias(conteudo)
    stream = 12_345
    textos = [media.moldura_de_manifesto(manifesto, stream, 0)]
    for seq, pedaco in enumerate(fatias, start=1):
        textos.append(media.moldura_de_pedaco(manifesto, stream, seq, pedaco))
    return manifesto, textos


def test_uma_fatia_antes_do_manifesto_nao_se_perde(loja) -> None:
    """O relay reordena, e o receptor tem de reter o que chegou antes.

    É a propriedade que faz «a chegar, 42 de 204» funcionar: as 42
    fatias estão na loja **antes** de a 43ª chegar, e não quando a
    transferência fica completa.
    """
    aberta, conversa = loja
    manifesto, textos = _quadros(aberta, conversa)
    receptor = media.Receptor(aberta, conversa)

    # O manifesto é o **último** a chegar. É o caso que a implementação
    # tem de suportar, e não o lucky one.
    for texto in textos[1:]:
        assert receptor.receber(texto) is None, "uma fatia sem manifesto não tem id"

    assert len(aberta.transferencias(conversa)) == 0, "não há transferência ainda"
    assert receptor.em_curso() == {}, "e não há nada a meio"

    id = receptor.receber(textos[0])
    assert id == manifesto.id

    # E agora estão lá todas — inclusive as que chegaram primeiro.
    assert aberta.contar_pedacos(id) == len(textos) - 1
    assert aberta.transferencia(id)["completo"] is True


def test_a_ordem_normal_tambem_monta(loja) -> None:
    """O caminho fácil, que é o que se vai usar quase sempre."""
    aberta, conversa = loja
    manifesto, textos = _quadros(aberta, conversa)
    receptor = media.Receptor(aberta, conversa)

    for texto in textos:
        assert receptor.receber(texto) == manifesto.id

    assert aberta.contar_pedacos(manifesto.id) == len(textos) - 1
    assert aberta.transferencia(manifesto.id)["completo"] is True


def test_o_manifesto_fic_na_loja(loja) -> None:
    """O manifesto é uma linha de `mensagens`, e não só uma tabela.

    É o que o `MEDIA_REPRODUZIR` lê para saber o resumo e os números.
    Tê-lo só em `transferencias` obrigaria o daemon a duplicar o
    formato do manifesto noutro sítio.
    """
    aberta, conversa = loja
    _, textos = _quadros(aberta, conversa)
    receptor = media.Receptor(aberta, conversa)

    id = receptor.receber(textos[0])

    linhas = [m for m in aberta.conversa(conversa)["messages"] if m["mediaId"] == id]
    assert len(linhas) == 1
    assert linhas[0]["text"].startswith("ONX1")
    # E a linha do manifesto é a que tem a flag de manifesto.
    assert media.FLAGS["manifesto"] & media.abrir(linhas[0]["text"]).flags


def test_texto_normal_nao_e_media(loja) -> None:
    """O caso comum: uma conversa sem media não toca em nada."""
    aberta, conversa = loja
    receptor = media.Receptor(aberta, conversa)

    assert receptor.receber("olá") is None
    assert receptor.receber("") is None
    assert receptor.receber("ONX") is None
    assert aberta.transferencias(conversa) == []


def test_moldura_de_streaming_e_ignorada(loja) -> None:
    """O streaming é F7, e não pode estragar o que já está guardado."""
    aberta, conversa = loja
    receptor = media.Receptor(aberta, conversa)
    streaming = media.Moldura(
        tipo="video", modo="streaming", stream=7, seq=1, clock_ms=40, corpo=b"\x00\x01"
    ).texto

    assert receptor.receber(streaming) is None
    assert aberta.transferencias(conversa) == []


def test_uma_transferencia_incompleta_nao_e_completa(loja) -> None:
    """`completo` só depois do resumo bater — nunca antes."""
    aberta, conversa = loja
    manifesto, textos = _quadros(aberta, conversa)
    receptor = media.Receptor(aberta, conversa)

    receptor.receber(textos[0])
    for texto in textos[1:-1]:  # falta a última
        receptor.receber(texto)

    t = aberta.transferencia(manifesto.id)
    assert t["completo"] is False, "marcou completa sem todas as fatias"
    assert t["totalPedacos"] == len(textos) - 1
    assert receptor.em_curso() == {manifesto.id: 1}


def test_um_resumo_que_nao_bate_nao_marca_completa(loja) -> None:
    """Uma fatia trocada tem de dar erro de resumo, não um ficheiro.

    É o teste que impede «o vídeo toca mas é o vídeo errado»: o
    receptor apanha a troca na montagem, e a loja continua a dizer que
    está incompleta.
    """
    aberta, conversa = loja
    manifesto, textos = _quadros(aberta, conversa)
    receptor = media.Receptor(aberta, conversa)

    receptor.receber(textos[0])
    trocada = media.abrir(textos[1])
    adulterada = media.moldura_de_pedaco(
        manifesto, trocada.stream, trocada.seq, bytes(len(trocada.corpo))
    )

    for texto in textos[2:]:
        receptor.receber(texto)
    with pytest.raises(media.ErroMedia):
        receptor.receber(adulterada)

    assert aberta.transferencia(manifesto.id)["completo"] is False


def test_um_ficheiro_vazio_nao_tem_fatias(loja) -> None:
    """O caso que o manifesto inteiro é a transferência.

    Sem ele, um ficheiro de zero bytes ficaria «a meio» para sempre,
    com zero de 204 — e a interface mostraria um spinner que nunca
    acaba.
    """
    aberta, conversa = loja
    manifesto, textos = _quadros(aberta, conversa, conteudo=b"")
    receptor = media.Receptor(aberta, conversa)

    assert len(textos) == 1, "só o manifesto"
    assert receptor.receber(textos[0]) == manifesto.id

    t = aberta.transferencia(manifesto.id)
    assert t["totalPedacos"] == 0
    assert t["completo"] is True, "zero de zero está completo"
    assert receptor.em_curso() == {}
