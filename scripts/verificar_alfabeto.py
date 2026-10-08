# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Varredura de caracteres fora do alfabeto latino-português.

Varre todos os ficheiros versionados (o que o `.gitignore` exclui fica de
fora) e reporta qualquer carácter que não pertença ao repertório
português — acentos incluídos, que são parte da língua.

Interessa o que *não* é português:
  * caracteres de outros sistemas de escrita (CJK, cirílico, grego,
    árabe, hebraico, devanagari) — entram por descuido de edição;
  * aspas alternativas que não são as do português europeu;
  * espaços inválidos (NBSP, espaço fino) que quebram a comparação de
    texto;
  * controlo Unicode invisíveis, que são o pior tipo: não se vêem.

Cada ocorrência é reportada com ficheiro e linha, para não adivinhar.
"""

from __future__ import annotations

import subprocess
import sys
import unicodedata
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent

# Alfabeto latino-português: ASCII mais o que a língua usa.
PERMITIDOS_EXTRA = set(
    "áàâãä" "ÁÀÂÃÄ"
    "éèêë" "ÉÈÊË"
    "íìîï" "ÍÌÎÏ"
    "óòôõö" "ÓÒÔÕÖ"
    "úùûü" "ÚÙÛÜ"
    "çÇ"
    "ñÑ"          # menos comum, aparece em nomes próprios
    "«»“”„‟′″"  # aspas e marcas tipográficas usadas em PT
    "–—―"        # travessões
    "…·•"        # pontuação
    "€§¶"        # símbolos técnicos
    "←↑→↓↔"      # setas em diagramas
    "°ºª"        # graus e indicadores — presentes em texto técnico
)

# ## Tipografia deliberada
#
# Estes caracteres **não** pertencem ao repertório português, mas são
# escolha editorial consciente e não contaminação. Têm de estar
# listados um a um, com o motivo, porque a regra por defeito é o
# contrário: um carácter que entra aqui sem justificação passa a ser
# invisível para a auditoria.
#
# Tirá-los não é uma questão de conforming: sem `─` e `│` os diagramas
# de arquitectura em `docs/architecture.md` desmoronam; sem `‖` a
# notação de concatenação do protocolo em `docs/ipc_spec.md` fica
# ilegível; sem `≡` e `∈` as tabelas de equivalence criptográfica perdem
# o sentido.
# As chaves são **um carácter de cada vez**: `DELIBERADOS` é consultado
# com o carácter isolado, e uma cadeia de caracteres como chave nunca
# casaria. A primeira versão tinha as cadeias e reportava 1767
# violações que eram exactamente a tipografia que queria isentar —
# o que torna a lição óbvia: um mecanismo de excepção que não excepciona
# é pior do que não existir, porque dá confiança falsa.
# Cada entrada é `caracteres -> motivo`, escrita como se lê.
_MOTIVOS: dict[str, str] = {
    "─│┌┐└┘├┬┴┼"
    "═║╔╗╚╝╠╣╦╩╬": "desenho de caixas (ASCII-art dos diagramas)",
    "►◄▶◀▲▼◊○●": "setas e marcadores nos diagramas de fluxo",
    "✓✔✗✘×⚠": "marcas de resultado em tabelas e testes",
    "‖": "operador de concatenação da notação do protocolo",
    "≡≈≠≤≥±÷×∑": "operadores matemáticos nas tabelas de equivalência",
    "ᵢ": "índice em subscrito, na fórmula da entropia de `messenger/conta.py`",
    "∈∉∌∅∀∃": "conjuntos, na especificação criptográfica",
    "√∫∂": "matemática elementar nas fórmulas de custo",
    "⌊⌋⌈⌉": "tectos e Festas em medidas de memória",
    "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎"
    "ₐₑₒₓₔₕₖₗₘₙₚₛₜ": "índices em subscritos",
    "⁰¹²³⁴⁵⁶⁷⁸⁹ⁿ⁺⁻": "índices em sobrescritos e expoentes",
    "–—―": "travessões",
    "−": "sinal menos tipográfico, em fórmulas e intervalos",
    "⇒": "seta de implicação, nas tabelas de derivação",
    "∘": "composição de matrizes (multiplicação na K7)",
    "⌘": "símbolo da tecla de comando nos atalhos da interface (⌘K, ⌘N)",
    "Åß": (
        "nomes de utilizador de teste em `tests/vectors/identidade.json`, "
        "renderizados por `docs/test_vectors.md`. São os vectores que "
        "provam que NFKC e casefold fazem o que devem: o A com anel "
        "escrito de duas maneiras tem de dar o mesmo identificador, e o "
        "sharp-s tem de virar «ss». Escrever «Angra», «strasse» ou "
        "«strasse» com outra letra provaria menos e não provaria nada. "
        "O repositório é português; estes dois caracteres aparecem aqui "
        "porque o que se está a testar é precisamente o caso em que o "
        "português não dá"
    ),
}

# Expande para uma chave por carácter: `classificar` consulta o
# carácter **isolado**, e uma chave de vários caracteres nunca casaria.
# A primeira versão tentou isto com as variáveis trocadas e produziu
# um dicionário com letras da palavra "desenho" como chaves.
DELIBERADOS: dict[str, str] = {
    caractere: motivo
    for caracteres, motivo in _MOTIVOS.items()
    for caractere in caracteres
}

# Recipientes de outros sistemas de escrita, nomeados para o relatório.
OUTROS_SISTEMAS = {
    "CJK Unified Ideographs": range(0x4E00, 0xA000),
    "CJK Symbols": range(0x3000, 0x3040),
    "Hiragana": range(0x3040, 0x30A0),
    "Katakana": range(0x30A0, 0x3100),
    "Hangul": range(0xAC00, 0xD800),
    "Cyrillic": range(0x0400, 0x0500),
    "Greek": range(0x0370, 0x0400),
    "Arabic": range(0x0600, 0x0700),
    "Hebrew": range(0x0590, 0x0600),
    "Devanagari": range(0x0900, 0x0980),
    "Thai": range(0x0E00, 0x0E80),
    "CJK Extension A": range(0x3400, 0x4DBF),
}

CONTROLOS_UV = {
    0x00A0: "NBSP (espaço não separável)",
    0x2007: "espaço fino",
    0x202F: "narrow no-break space",
    0x200B: "zero-width space",
    0x200E: "LEFT-TO-RIGHT MARK",
    0x200F: "RIGHT-TO-LEFT MARK",
    0xFEFF: "BOM no meio do ficheiro",
    0x2060: "word joiner",
}


def ficheiros_versionados() -> list[Path]:
    """Ficheiros que o Git publicaria, pelos critérios do `.gitignore`."""
    r = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        cwd=RAIZ, capture_output=True, text=True,
    )
    if r.returncode != 0:
        # Sem repositório: cai para uma caminhada simples, ignorando
        # os directórios de artefactos à mão.
        ignorados = {".git", "target", ".venv", "build", "__pycache__",
                     ".hypothesis", ".pytest_cache"}
        return [
            p for p in RAIZ.rglob("*")
            if p.is_file()
            and not any(parte in ignorados or parte.endswith("_testes.rs")
                        for parte in p.parts)
        ]
    return [RAIZ / linha for linha in r.stdout.splitlines() if linha.strip()]


def classificar(c: str) -> tuple[str, str] | None:
    """Devolve (classe, razão) se `c` for indesejável, ou None se está bem.

    A classe distingue **violação** — um carácter que entrou por
    descuido — de **deliberado** — escolha editorial com motivo. Só a
    primeira faz o script sair com erro. Confundir as duas é o que faria
    este script ser ignorado à primeira utilização, porque ninguém
    remove 1867 linhas de diagramas para agradar a uma auditoria.
    """
    cp = ord(c)
    if cp < 0x80 or c in PERMITIDOS_EXTRA:
        return None
    if cp in CONTROLOS_UV:
        return ("violacao", CONTROLOS_UV[cp])
    if c in DELIBERADOS:
        return ("deliberado", DELIBERADOS[c])
    for nome, faixa in OUTROS_SISTEMAS.items():
        if cp in faixa:
            return ("violacao", nome)
    return ("violacao", f"U+{cp:04X} ({unicodedata.name(c, 'desconhecido')})")


def main() -> int:
    ficheiros = ficheiros_versionados()
    violacoes: list[tuple[str, int, str, str]] = []
    deliberados: dict[str, int] = {}
    lidos = 0

    for caminho in ficheiros:
        try:
            texto = caminho.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        lidos += 1
        for n, linha in enumerate(texto.splitlines(), 1):
            for c in linha:
                achado = classificar(c)
                if not achado:
                    continue
                classe, razao = achado
                if classe == "deliberado":
                    deliberados[razao] = deliberados.get(razao, 0) + 1
                else:
                    violacoes.append(
                        (str(caminho.relative_to(RAIZ)), n, repr(c), razao)
                    )

    print(f"ficheiros lidos: {lidos}")

    if deliberados:
        total = sum(deliberados.values())
        print(f"\ntipografia deliberada: {total} ocorrência(s), "
              f"{len(deliberados)} categoria(s)")
        for razao, n in sorted(deliberados.items(), key=lambda kv: -kv[1]):
            print(f"  {n:5d}  {razao}")

    if not violacoes:
        print("\nNenhuma violação: zero caracteres de outros sistemas "
              "de escrita.")
        return 0

    # Agrupa por carácter: 200 linhas da mesma confusão diz menos do que
    # dizer que ela existe em 200 sítios.
    por_c: dict[str, list[tuple[str, int]]] = {}
    for f, n, c, razao in violacoes:
        por_c.setdefault(f"{c} — {razao}", []).append((f, n))

    print(f"\nVIOLAÇÕES: {len(violacoes)} ocorrência(s) em "
          f"{len(por_c)} carácter(s) distinto(s):\n")
    for chave, sitios in sorted(por_c.items(), key=lambda kv: -len(kv[1])):
        print(f"  {chave}  ×{len(sitios)}")
        for f, n in sitios[:6]:
            print(f"      {f}:{n}")
        if len(sitios) > 6:
            print(f"      ... mais {len(sitios) - 6}")
    return 1


if __name__ == "__main__":
    sys.exit(main())