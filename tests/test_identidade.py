# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes de `messenger/identidade.py`.

Duas coisas são testadas, e a segunda é a importante: que a interface em
JavaScript produz os mesmos valores. O vector está congelado em
`tests/vectors/identidade.json` e **não** é recalculado a partir da
fórmula do código — se fosse, um erro na fórmula passaria o teste por
definição.
"""

from __future__ import annotations

import json
import pathlib

import pytest

from messenger import identidade as mod
from messenger.identidade import (
    ALFABETO,
    DOMINIO_ID,
    DOMINIO_IMPRESSAO,
    SIMBOLOS,
    TAM_BLOCO,
    TAM_CAUDA,
    TAM_IMPRESSAO,
    TAM_SAL,
    IdentidadeInvalida,
    gerar_sal,
    identificador_de,
    impressao_de,
    normalizar_utilizador,
)

VECTOR = pathlib.Path(__file__).parent / "vectors" / "identidade.json"


def _vetores() -> list[dict]:
    return json.loads(VECTOR.read_text(encoding="utf-8"))["vetores"]


# ---------------------------------------------------------------------
# Normalização
# ---------------------------------------------------------------------


def test_normalizar_reduz_maiusculas() -> None:
    assert normalizar_utilizador("ANA") == normalizar_utilizador("ana")


def test_normalizar_remove_espacosNas_extremidades() -> None:
    assert normalizar_utilizador("  ana  ") == "ana"


def test_normalizar_reduz_formas_unicode_equivalentes() -> None:
    """O A com anel (U+00C5) tem duas formas de composição.

    As duas têm de dar o mesmo identificador. Escritas como escapes
    porque o A com anel não pertence ao repertório português do
    repositório — que é o motivo de ter de ser um escape, e não uma
    limitação do teste.
    """
    decomposto = "A\u030Angstro\u0308m"
    composto = "\u00c5ngstr\u00f6m"
    assert normalizar_utilizador(decomposto) == normalizar_utilizador(composto)


def test_normalizar_recusa_texto_vazio() -> None:
    with pytest.raises(IdentidadeInvalida):
        normalizar_utilizador("   ")


def test_normalizar_recusa_texto_nao_string() -> None:
    with pytest.raises(IdentidadeInvalida):
        normalizar_utilizador(123)  # type: ignore[arg-type]


# ---------------------------------------------------------------------
# Sal
# ---------------------------------------------------------------------


def test_gerar_sal_tem_o_tamanho_certo() -> None:
    assert len(gerar_sal()) == TAM_SAL


def test_gerar_sal_e_aleatorio() -> None:
    """Dois salts têm de diferir: um salt fixo seria o mesmo que não ter salt."""
    assert gerar_sal() != gerar_sal()


# ---------------------------------------------------------------------
# Formato
# ---------------------------------------------------------------------


def _forma(identificador: str) -> None:
    """Verifica a forma de um identificador, seja ele qual for."""
    assert identificador.startswith("ONYX-")
    assert identificador.endswith("#")
    corpo = identificador[len("ONYX-") : -1]
    partes = corpo.split("-")
    assert len(partes) == 2
    bloco, cauda = partes
    assert len(bloco) == TAM_BLOCO
    assert len(cauda) == TAM_CAUDA
    assert all(c in ALFABETO for c in bloco)
    for indice, caractere in enumerate(cauda):
        if indice % 2 == 0:
            assert caractere in SIMBOLOS
        else:
            assert caractere in ALFABETO


def test_identificador_tem_a_forma_prometida() -> None:
    _forma(identificador_de("ana", bytes(TAM_SAL)))


def test_identificador_nao_contem_o_terminador_na_cauda() -> None:
    """A cauda não pode levar ``#``: o identificador deixaria de se desdobrar."""
    for i in range(2000):
        cauda = identificador_de(f"u{i}", bytes([i % 256] * 16)).split("-")[2][:-1]
        assert "#" not in cauda


def test_identificador_nao_contem_ampersando() -> None:
    """A cauda não pode levar ``&``: separado de parâmetros num endereço."""
    for i in range(2000):
        cauda = identificador_de(f"u{i}", bytes([(i * 7) % 256] * 16)).split("-")[2][:-1]
        assert "&" not in cauda


def test_alfabeto_nao_tem_letras_ambiguas() -> None:
    for caractere in "IO01":
        assert caractere not in ALFABETO


def test_alfabetos_tampos_compativeis() -> None:
    """Cinco bits por caractere, três bits por símbolo: potências de dois."""
    assert len(ALFABETO) == 32
    assert len(SIMBOLOS) == 8


# ---------------------------------------------------------------------
# Determinismo e sal
# ---------------------------------------------------------------------


def test_identificador_e_deterministico() -> None:
    sal = bytes(range(16))
    assert identificador_de("ana", sal) == identificador_de("ana", sal)


def test_utilizador_diferente_da_identificadores_diferentes() -> None:
    sal = bytes(range(16))
    assert identificador_de("ana", sal) != identificador_de("ana2", sal)


def test_sal_diferente_da_identificadores_diferentes() -> None:
    """É isto que impede o dicionário de nomes de utilizador."""
    assert identificador_de("ana", bytes(16)) != identificador_de("ana", bytes(range(1, 17)))


def test_mesmo_utilizador_normalizado_da_o_mesmo_identificador() -> None:
    sal = bytes(range(16))
    alvo = identificador_de("ana", sal)
    assert identificador_de("ANA", sal) == alvo
    assert identificador_de("  Ana  ", sal) == alvo


def test_identificador_recusa_sal_de_tamanho_errado() -> None:
    with pytest.raises(IdentidadeInvalida):
        identificador_de("ana", bytes(TAM_SAL - 1))


# ---------------------------------------------------------------------
# Impressão digital
# ---------------------------------------------------------------------


def test_impressao_tem_o_tamanho_certo() -> None:
    partes = impressao_de("ONYX-AAAAAA-!X!A#").split(":")
    assert len(partes) == TAM_IMPRESSAO
    assert all(len(p) == 2 for p in partes)
    assert all(p == p.upper() for p in partes)


def test_impressao_depende_so_do_identificador() -> None:
    """O ponto do desenho: quem tem o ID pode verificar a impressão."""
    identificador = identificador_de("ana", bytes(range(16)))
    assert impressao_de(identificador) == impressao_de(identificador)


def test_impressao_difere_entre_identificadores() -> None:
    a = identificador_de("ana", bytes(16))
    b = identificador_de("ana", bytes(range(1, 17)))
    assert impressao_de(a) != impressao_de(b)


def test_impressao_recusa_identificador_mal_formado() -> None:
    for mau in ("", "ONYX-AAAAAA", "XXXXXX", "onyx-aaaaaa-!x0%x0#"):
        with pytest.raises(IdentidadeInvalida):
            impressao_de(mau)


def test_rotulos_de_dominio_sao_distintos() -> None:
    """O mesmo resumo não pode servir para as duas derivações."""
    assert DOMINIO_ID != DOMINIO_IMPRESSAO
    assert mod.DOMINIO_ID == b"ONYX/ID/v1"
    assert mod.DOMINIO_IMPRESSAO == b"ONYX/FP/v1"


# ---------------------------------------------------------------------
# Paridade com o vector congelado
# ---------------------------------------------------------------------


@pytest.mark.parametrize("vetor", _vetores(), ids=lambda v: v["id"])
def test_vector_congelado(vetor: dict) -> None:
    """O vector foi gerado uma vez e não se regenera.

    Se a fórmula mudar, este teste **tem de falhar** — é para isso que o
    valor está escrito à mão no ficheiro em vez de recalculado com a
    mesma fórmula do código. Actualizar o vector é uma decisão, não um
    efeito colateral de correr os testes.
    """
    sal = bytes.fromhex(vetor["sal_hex"])
    assert identificador_de(vetor["utilizador"], sal) == vetor["identificador"]
    assert impressao_de(vetor["identificador"]) == vetor["impressao"]


def test_vector_cobre_normalizacao() -> None:
    """Três vectores com o mesmo sal têm de dar o mesmo identificador.

    Se a normalização deixasse de reduzir maiúsculas ou espaços, estes
    três divergiam entre si — e o teste seguinte de cada um apanhava.
    """
    com_o_mesmo_sal = [v for v in _vetores() if v["sal_hex"] == "000102030405060708090a0b0c0d0e0f"]
    identificadores = {v["identificador"] for v in com_o_mesmo_sal}
    # «ana», «ANA» e «  ana  » são o mesmo nome; «Marta Vasconcelos» não é.
    assert len(identificadores) == 2


def test_metadados_do_vector_batem_com_o_codigo() -> None:
    documento = json.loads(VECTOR.read_text(encoding="utf-8"))
    assert documento["alfabeto"] == ALFABETO
    assert documento["simbolos"] == SIMBOLOS
    assert documento["dominio_id"] == DOMINIO_ID.decode("ascii")
    assert documento["dominio_impressao"] == DOMINIO_IMPRESSAO.decode("ascii")
    assert documento["tam_sal"] == TAM_SAL