# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes da extracção de prosa do auditor de comentários.

## O buraco que estes testes fecham

`scripts/verificar_comentarios.py` escolhia o ficheiro pelo sufixo e,
para um sufixo que não conhecesse, caía no `else` final e devolvia lista
vazia. Um verificador que devolve vazio porque **não sabe** é
indistinguível, na saída, de um verificador que passa — e a interface
inteira, 79 ficheiros versionados, estava nesse `else` desde sempre.

Um teste que só confirmasse «não há achados» não apanharia nada disto.
O que tem de ser provado é a **extracção**: que um americanismo dentro
de cada forma de comentário do JavaScript chega à prosa. Um verde com a
extracção a falhar é o pior resultado possível, porque parece trabalho
feito.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

RAIZ = Path(__file__).resolve().parent.parent


def _modulo():
    spec = importlib.util.spec_from_file_location(
        "verificar_comentarios", RAIZ / "scripts" / "verificar_comentarios.py"
    )
    assert spec and spec.loader
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


vc = _modulo()

# As quatro formas de comentário que o JavaScript tem, mais o caso que
# **não** é comentário e nunca pode ser contado como tal.
CASOS = [
    pytest.param("// o usuario leu a senha\n", "linha", id="linha"),
    pytest.param("/*\n * o usuario leu a senha\n */\n", "bloco", id="bloco"),
    pytest.param("      {/* o usuario leu a senha */}\n", "bloco", id="jsx"),
    pytest.param(
        "      {/*\n       * o usuario leu a senha\n       */}\n",
        "bloco",
        id="jsx-multilinha",
    ),
    pytest.param("const x = 'o usuario leu a senha';\n", None, id="codigo"),
    pytest.param("const url = 'http://exemplo.pt/a';\n", None, id="url-em-cadeia"),
]


@pytest.mark.parametrize("conteudo,quer_prosa", CASOS)
def test_a_extraccao_deixa_ver_a_prosa(
    tmp_path: Path, conteudo: str, quer_prosa: bool | None
) -> None:
    """A prosa de um ficheiro JavaScript é vista, e o código não."""
    f = tmp_path / "exemplo.jsx"
    f.write_text(conteudo, encoding="utf-8")
    prosa = [texto for _, texto in vc.prosa_de(f)]

    if quer_prosa is None:
        assert prosa == [], f"código contou como prosa: {prosa!r}"
    else:
        assert prosa, "não extraiu prosa nenhuma de um comentário"
        assert any("senha" in texto for texto in prosa), prosa


def test_a_interface_tem_prosa_por_extrair() -> None:
    """A extracção tem de produzir conteúdo real nos ficheiros do projecto.

    Este é o teste que diz «a extensão foi acrescentada de facto». Sem
    ele, um regresso que apague o ramo de JavaScript passa todos os
    testes de forma e falha só quando um americanismo entra no código —
    que é o mesmo que nunca vir.
    """
    js = [
        p
        for p in (RAIZ / "UI").rglob("*")
        if p.is_file()
        and p.suffix in {".js", ".jsx", ".mjs", ".cjs"}
        and "node_modules" not in p.parts
        and "dist" not in p.parts
    ]
    assert js, "não há ficheiros JavaScript na interface"

    linhas = sum(len(vc.prosa_de(p)) for p in js)
    assert linhas > 500, f"apenas {linhas} linhas de prosa extraídas"


@pytest.mark.parametrize(
    "conteudo",
    [
        "# o usuario leu a senha\n",
        "jobs:\n  x:\n    # o usuario leu a senha\n",
    ],
)
def test_a_prosa_de_um_workflow_sao_os_comentarios(tmp_path: Path, conteudo: str) -> None:
    """Num `.yml`, a prosa são os comentários — e só eles.

    Sem este ramo, `.yml` caía no `else` final de `prosa_de()` e devolvia
    lista vazia: o auditor passava em silêncio sobre a única
    configuração que decide se o projecto é verificado. Foi o mesmo
    buraco que o JavaScript tinha, e a mesma correcção.
    """
    f = tmp_path / "exemplo.yml"
    f.write_text(conteudo, encoding="utf-8")
    prosa = [texto for _, texto in vc.prosa_de(f)]
    assert prosa, "não extraiu o comentário"
    assert any("senha" in texto for texto in prosa), prosa


def test_o_comentario_dentro_de_run_e_prosa(tmp_path: Path) -> None:
    """Um `#` dentro de `run:` é um comentário de shell — e é prosa.

    O comando em si (`rustup default stable`) é código e não aparece. O
    comentário é texto que alguém escreveu para explicar o passo, e é
    a prosa que mais apodrece num ficheiro de configuração.

    A distinção entre os dois é a mesma do `verificar_portugues.py`:
    `IDENTIFICADORES` remove `default` porque é o nome de um subcomando.
    Aqui não há remoção nenhuma — a prosa é a linha que começa por
    `#`, e só ela.
    """
    f = tmp_path / "exemplo.yaml"
    f.write_text(
        "jobs:\n"
        "  x:\n"
        "    steps:\n"
        "      - name: compilar\n"
        "        run: |\n"
        "          # o usuario leu a senha\n"
        "          rustup default stable\n",
        encoding="utf-8",
    )
    prosa = [texto for _, texto in vc.prosa_de(f)]
    assert prosa == ["o usuario leu a senha"], prosa


def test_um_ficheiro_ilegivel_nao_rebenta(tmp_path: Path) -> None:
    """Um ficheiro apagado devolve vazio, não excepção.

    `git ls-files` lista o que está **indexado**. Um ficheiro apagado
    na árvore de trabalho continua indexado, e um auditor que rebenta
    deixa de auditar — o pior resultado, porque não avisa ninguém.
    """
    ausente = tmp_path / "inexistente.js"
    assert not ausente.exists()
    assert vc.prosa_de(ausente) == []


def test_o_auditor_passa_no_repositorio() -> None:
    """A varredura real, com o JavaScript incluído, tem de dar verde."""
    import subprocess
    import sys

    r = subprocess.run(
        [sys.executable, str(RAIZ / "scripts" / "verificar_comentarios.py")],
        capture_output=True,
        text=True,
        cwd=RAIZ,
    )
    assert r.returncode == 0, f"falhou:\n{r.stdout}\n{r.stderr}"