# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes do verificador de ambiente.

## A pergunta

Um verificador que passa porque não viu nada é indistinguível de um que
funciona. Estes testes escrevem cada caso proibido e exigem que seja
apanhado — e escrevem os casos que **não** podem ser accusados, que são
os que um verificador demasiado zeloso transformaria em portas fechadas
por quem escreve a documentação.

## O caso que mais importa

`UI/src/pages/Registar.jsx` diz «Tudo fica neste computador», e está
certo: é o computador de quem cria a conta. Se o verificador varresse o
ficheiro inteiro, obrigaria a reescrever texto dirigido ao utilizador —
e o primeiro a desligar o portão seria esse.

Por isso o programa lê **só linhas de comentário** nos ficheiros de
código, e o teste abaixo fixa essa decisão. Sem ele, alguém «simplifica»
a implementação e quebra a interface sem o reparar.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

RAIZ = Path(__file__).resolve().parent.parent
VERIFICADOR = RAIZ / "scripts" / "verificar_ambiente.py"


def _modulo():
    spec = importlib.util.spec_from_file_location("verificar_ambiente", VERIFICADOR)
    assert spec and spec.loader
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


va = _modulo()


def _achados(caminho: Path) -> list[tuple[int, str, str]]:
    return va.procurar(caminho)


# ---------------------------------------------------------------------
# Tem de apanhar
# ---------------------------------------------------------------------

@pytest.mark.parametrize(
    "linha",
    [
        "Nesta máquina o pico foi de 931 MB.",
        "o que morreu três vezes nesta máquina foi o editor",
        "Medido em 2026-10-07 nesta máquina (i3-5005U, 3,8 GB).",
        "# Verificado nesta máquina: cgroup v2 e systemd 255.",
        "// a degradação fica por verificar nesta máquina",
        "A última linha é a que era recusada de manhã.",
        "de manhã a verificação era recusada, à noite passava",
        "498 fingerprints escritos às 16:43 e o OOM às 16:44",
    ],
)
def test_apanha_as_frases_ligadas_ao_ambiente(tmp_path: Path, linha: str) -> None:
    """Cada caso é uma linha que esteve na documentação ou no código.

    Todas saíram de uma auditoria real. Um teste com frases inventadas
    passaria mesmo que os padrões tivessem mudado para pior.
    """
    f = tmp_path / "doc.md"
    f.write_text(linha + "\n", encoding="utf-8")
    assert _achados(f), f"não apanhou {linha!r}"


# ---------------------------------------------------------------------
# Tem de NÃO apanhar
# ---------------------------------------------------------------------

@pytest.mark.parametrize(
    "linha",
    [
        # `mesma máquina` é o termo técnico da ameaça A8. Tirá-lo da lista
        # faria o verificador deixar de ver a frase que interessa sem
        # motivo — e apagaria a definition de uma ameaça real.
        "### A8 — Atacante local (mesma máquina, outro utilizador)",
        "um atacante **não-root** da mesma máquina",
        "O IPC é usado exclusivamente **dentro da mesma máquina**",
        # Texto dirigido a quem usa a aplicação: é o computador da
        # pessoa que o lê, não o de quem escreveu a frase.
        '      subtitle="Tudo fica neste computador. Não há servidor."',
        "antiga <strong>não é apagada</strong> — passa a estar órfã neste computador",
    ],
)
def test_nao_apanha_o_texto_para_o_utilizador(tmp_path: Path, linha: str) -> None:
    """«Neste computador» numa caixa de registo é exacto, e é do outro.

    Num ficheiro de código só as linhas de comentário são lidas, e por
    isso uma cadeia de literais nunca é accusada. É a razão de o
    programa não varrer o ficheiro inteiro.
    """
    f = tmp_path / "exemplo.jsx"
    f.write_text(linha + "\n", encoding="utf-8")
    assert _achados(f) == [], f"acusou texto do utilizador: {linha!r}"


def test_uma_linha_de_comentario_javascript_e_lida(tmp_path: Path) -> None:
    """O filtro de comentário tem de distinguir código de prosa.

    Sem esta prova, a exclusão acima «passa» por ser que o
    verificador simplesmente não lê ficheiros de código — e ninguém
    notava até um americanismo aparecer lá.
    """
    f = tmp_path / "exemplo.jsx"
    f.write_text("// nesta máquina o build morre\nconst x = 1;\n", encoding="utf-8")
    assert _achados(f)


def test_a_isencao_por_bloco_cita_o_marcador(tmp_path: Path) -> None:
    """`# verificar_ambiente.py: ignorar` isenta o resto do comentário.

    A única cita da frase proibida está em `verificar_gitignore.sh`, e
    reescrevê-la destruiria o sentido. O teste garante que a isenção
    existe e funciona — um portão sem saída acaba desactivado.
    """
    f = tmp_path / "comentario.sh"
    f.write_text(
        "# verificar_ambiente.py: ignorar\n"
        "#\n"
        '# o "funciona na minha máquina" volta a aparecer\n'
        'UI/package.json|nao\n',
        encoding="utf-8",
    )
    assert _achados(f) == []


def test_a_isencao_terna_fim_no_documento(tmp_path: Path) -> None:
    """O bloco isento acaba, e a linha seguinte é apanhada.

    Uma isenção sem fim seria uma brecha para desligar o verificador — e
    é por isso que o fim é testado e não apenas a isenção.

    Num documento todas as linhas são prosa, o que torna o limite do
    bloco visível: **é a linha em branco que o encerra**, e não a
    próxima frase. A citação tem de vir imediatamente a seguir ao
    marcador — um parágrafo em branco entre os dois já está fora da
    isenção, que é o comportamento pretendido. Num ficheiro de código, uma linha que não seja
    comentário nunca é lida — e isso é uma decisão à parte, testada em
    `test_nao_apanha_o_texto_para_o_utilizador`.
    """
    f = tmp_path / "doc.md"
    f.write_text(
        "verificar_ambiente.py: ignorar\n"
        "Isto é uma citação isenta: a minha máquina.\n"
        "\n"
        "Isto já não é isento: nesta máquina o pico foi de 931 MB.\n",
        encoding="utf-8",
    )
    achados = _achados(f)
    assert len(achados) == 1, achados
    assert achados[0][0] == 4, achados


# ---------------------------------------------------------------------
# O repositório
# ---------------------------------------------------------------------

def test_o_repositorio_nao_tem_ocorrencias() -> None:
    """A auditoria real tem de estar limpa.

    Um teste que só exercita frases inventadas passa com o repositório
    inteiro em desacordo — que é a situação que o auditor tem de apanhar.
    """
    problemas = []
    for f in va.enumerar():
        problemas += [(f, *a) for a in _achados(f)]
    assert not problemas, problemas


def test_o_node_terceiro_nao_e_lido() -> None:
    """`UI/tools/node` são 2 369 ficheiros de um pacote de outro.

    Varrê-los para procurar a nossa prosa é tempo gasto a proteger o
    texto de outra pessoa — e foi um erro real: a primeira versão
    comparava `UI/tools/node` como uma cadeia contra um conjunto de
    nomes de directório, e o resultado era vazio.
    """
    lidos = va.enumerar()
    assert lidos, "a enumeração devolveu nada"
    assert not [f for f in lidos if "tools/node" in str(f)]
    assert len(lidos) < 500, f"{len(lidos)} ficheiros — a exclusão voltou a falhar"