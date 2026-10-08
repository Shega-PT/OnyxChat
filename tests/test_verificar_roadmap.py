# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes do verificador de rótulos do roadmap.

## A pergunta

`docs/roadmap.md` distingue `IMPLEMENTADO`, `PLANEADO` e `CONCEITO`, e
a definição de `PLANEADO` é «decidido, **especificado**, ainda sem
código». Houve uma linha com a coluna «Especificado em» a `—`.

Um verificador que só corre contra o estado actual não prova nada: o
roadmap já esteve errado e passou. Estes testes escrevem o erro de volta
e exigem que seja apanhado.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

RAIZ = Path(__file__).resolve().parent.parent
ROADMAP = RAIZ / "docs" / "roadmap.md"


def _modulo():
    spec = importlib.util.spec_from_file_location(
        "verificar_roadmap", RAIZ / "scripts" / "verificar_roadmap.py"
    )
    assert spec and spec.loader
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


vr = _modulo()


@pytest.fixture
def com_roadmap(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    def _escrever(texto: str) -> Path:
        destino = tmp_path / "roadmap.md"
        destino.write_text(texto, encoding="utf-8")
        monkeypatch.setattr(vr, "ROADMAP", destino)
        return destino

    return _escrever


#: Um roadmap mínimo, com as duas tabelas presentes.
BASE = (
    "## PLANEADO\n"
    "\n"
    "| Item | Especificado em | Nota |\n"
    "| --- | --- | --- |\n"
    "{planeado}"
    "\n"
    "## CONCEITO\n"
    "\n"
    "| Item | Nota |\n"
    "| --- | --- |\n"
    "{conceito}"
    "\n"
)


def _erros(com_roadmap, texto: str) -> int:
    """O código de saída do verificador sobre um roadmap escrito à mão.

    `main()` **devolve** o código, não levanta `SystemExit` — quem chama
    o verificador como biblioteca recebe o número, e é assim que o CI o
    consome.

    E o roadmap é um ficheiro temporário. Uma versão anterior deste
    teste escrebia no `docs/roadmap.md` a sério e restaurava-o no fim —
    o que, num teste que falha a meio, deixa o roadmap do projecto
    substituído pelo texto do teste.
    """
    com_roadmap(texto)
    return vr.main()


# ---------------------------------------------------------------------
# A regra de uma linha
# ---------------------------------------------------------------------

def test_um_item_especificado_e_planeado() -> None:
    assert vr.linhas_da_tabela([
        "| Item | Especificado em | Nota |",
        "| --- | --- | --- |",
        "| `mlockall` | [`key_management.md`](key_management.md) §X | fecha a lacuna |",
    ]) == [["`mlockall`", "[`key_management.md`](key_management.md) §X",
             "fecha a lacuna"]]


@pytest.mark.parametrize("vazio", ["—", "-", "", "n/a", "nenhum"])
def test_a_coluna_vazia_e_o_sinal(com_roadmap, vazio: str) -> None:
    """Qualquer valor vazio na coluna é o sinal de `CONCEITO`.

    A lista de valores não é arbitrária: `—` é o que lá estava, e `n/a`
    é o que alguém escreveria ao ser confrontado com a pergunta. Todos
    significam a mesma coisa — nada especificado.
    """
    codigo = _erros(com_roadmap, BASE.format(
        planeado=f"| item | {vazio} | nota |\n" if vazio
                 else "| item | [`pipeline.md`](pipeline.md) | nota |\n",
        conceito="| outro | nota |\n",
    ))
    if vazio:
        assert codigo == 1, f"não apanhou a coluna {vazio!r}"
    else:
        assert codigo == 0


def test_o_roadmap_real_nao_tem_linha_vazia() -> None:
    """Nenhum item do `PLANEADO` fica sem especificação."""
    texto = ROADMAP.read_text(encoding="utf-8")
    sec = vr.secoes(texto)
    planeado = vr.linhas_da_tabela(sec["PLANEADO"])
    assert planeado, "a tabela do PLANEADO ficou vazia"
    for celulas in planeado:
        assert celulas[1] not in vr.VAZIO, celulas[0]


def test_o_item_dos_algoritmos_esta_no_conceito() -> None:
    """A reclassificação de 2026-10-07 está no sítio certo.

    A linha vivia no `PLANEADO` com «Especificado em: —» desde que o
    roadmap foi escrito. Com a coluna vazia, a definição do projecto
    já a colocava no `CONCEITO` — e é lá que está.
    """
    texto = ROADMAP.read_text(encoding="utf-8")
    sec = vr.secoes(texto)
    assert "PLANEADO" in sec and "CONCEITO" in sec
    assert not any(
        "IDs de algoritmo" in c[0] or "algoritmo por camada" in c[0]
        for c in vr.linhas_da_tabela(sec["PLANEADO"])
    ), "a linha voltou para o PLANEADO"
    assert any(
        "algoritmo por camada" in c[0] for c in vr.linhas_da_tabela(sec["CONCEITO"])
    ), "a linha não está no CONCEITO"


# ---------------------------------------------------------------------
# As duas direcções
# ---------------------------------------------------------------------

def test_o_mesmo_item_nas_duas_tabelas_e_erro(com_roadmap) -> None:
    """Um item não pode estar nos dois sítios.

    Uma cópia acidental é a forma mais provável de o rótulo voltar a
    ficar errado depois de uma correcção — e a menos visível, porque as
    duas linhas ficam parecidas e nenhuma delas parece errada.
    """
    codigo = _erros(com_roadmap, BASE.format(
        planeado="| routing nodes | [`SYS_GUIDE.md`](SYS_GUIDE.md) §10.4 | nota |\n",
        conceito="| Routing Nodes | nota |\n",
    ))
    assert codigo == 1


def test_uma_tabela_vazia_e_erro_de_formato(com_roadmap) -> None:
    """Se uma das tabelas deixar de existir, o verificador diz.

    Sem isto, uma tabela apagada por engano dava um verde — o mesmo
    silêncio de sempre, agora dentro do verificador dos rótulos.
    """
    codigo = _erros(
        com_roadmap,
        "## PLANEADO\n\n| Item | Especificado em | Nota |\n| --- | --- | --- |\n"
        "| a | [`x.md`](x.md) | n |\n\n"
    )
    assert codigo == 2