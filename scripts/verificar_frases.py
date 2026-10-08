# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Caça às frases que não são português.

O `verificar_portugues.py` apanha americanismos — palavras que existem
em português mas dizem a coisa errada. Este apanha o outro caso, e mais
grave: **frases que não são português de nenhum sítio** — quase sempre
resultado de uma edição que colou texto no sítio errado.

O método é o das palavras-vizinhas: uma palavra isolada é difícil de
julgar, mas uma **palestra de cinco palavras** que não existe em
português é quase sempre lixo. Por isso a lista é de sequências, não de
palavras, e o relatório mostra o contexto para se decidir.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent

# Sequências que não fazem sentido em português. Cada uma é lixo que
# entrou por descuido.
LIXO = [
    # «uma a o», «de de», «não não» — colagens
    r"\ba\s+o\s+(o|a|os|as)\b",
    r"\bo\s+a\s+(o|a|os|as)\b",
    r"\bde\s+de\s+(o|a|os|as)\b",
    r"\bnão\s+não\b",
    r"\bem\s+em\s+a\b",
    r"\bque\s+que\b",
    r"\bum\s+um\b",
    # Palestras de caracteres colados. Os intervalos CJK e Hangul estão
    # escritos como escapes unicode e não como caracteres literais: o
    # padrão precisa de os cobrir, mas escrevê-los por extenso punha
    # justamente os caracteres que este projecto proíbe no código-fonte.
    # É a diferença entre uma regra e uma violação dela.
    r"[a-zà-ú][\u4e00-\u9fff]",
    r"[\u4e00-\u9fff][a-zà-ú]",
    r"[a-zà-ú][\uac00-\ud7af]",
    r"[\uac00-\ud7af][a-zà-ú]",
    # Palavras que aparecem em código inglês colado em comentários
    r"\bthe\s+code\b",
    r"\bof\s+the\s+",
    r"\bis\s+used\s+",
    r"\bmust\s+be\b",
    r"\bshould\s+be\b",
    r"\bthis\s+is\s+",
    r"\bfor\s+example\b",
    r"\bnote\s+that\b",
    r"\bin\s+order\s+to\b",
    # Número seguido de palavra que não faz sentido
    r"\bcom o\s+qual\s+é\s+é\b",
    # Fragmentos de instrução de shell colados
    r"\bsi ou não\b",
    r"\bque é que\b",
]

IGNORADOS = {".git", "target", ".venv", "build", "__pycache__",
             ".hypothesis", ".pytest_cache", "vendor", "node_modules",
             "UI/dist"}

# Este ficheiro contém os padrões de busca de todos os outros, incluindo
# as sequências que ele próprio caça. Reportá-lo a si mesmo seria ruído
# garantido.
FORA_DA_BUSCA = {
    "scripts/verificar_frases.py",
    # Os ficheiros de testes dos auditores guardam americanismos e
    # sequências-que-não-existem como dados de entrada.
    "tests/test_verificar_portugues.py",
    "tests/test_verificar_comentarios.py",
}

# Documentos de terceiro vendorizado: o inglês é a língua original.
DOCUMENTOS_ESTRANGEIROS = {"README", "LICENSE", "LICENSE.txt", "CHANGELOG",
                           "NOTICE", "AUTHORS"}


def ficheiros() -> list[Path]:
    r = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        cwd=RAIZ, capture_output=True, text=True)
    saida = []
    for linha in r.stdout.splitlines():
        p = RAIZ / linha
        if any(parte in IGNORADOS for parte in p.parts):
            continue
        if str(p.relative_to(RAIZ)) in FORA_DA_BUSCA:
            continue
        # `.js`, `.jsx`, `.mjs` e `.cjs` desde 2026-10-07: sem eles, a
        # interface — 79 ficheiros versionados — ficava por verificar, e
        # era o mesmo buraco que `verificar_comentarios.py` tinha.
        if p.suffix in {".rs", ".py", ".md", ".c", ".cpp", ".h", ".lua",
                        ".toml", ".sh", ".json", ".js", ".jsx", ".mjs",
                        ".cjs", ".yml", ".yaml"}:
            saida.append(p)
    return saida


def main() -> int:
    padroes = [(re.compile(p, re.IGNORECASE), p) for p in LIXO]
    achados = []
    for caminho in ficheiros():
        rel = caminho.relative_to(RAIZ)
        # README/LICENSE na raiz são os do projecto; dentro de vendor são
        # de terceiros e ficam de fora pelo filtro de directório.
        try:
            texto = caminho.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for n, linha in enumerate(texto.splitlines(), 1):
            if linha.lstrip().startswith("|") and "|" in linha[2:]:
                # Tabela markdown: o formato distorce a leitura
                continue
            for rx, bruto in padroes:
                for m in rx.finditer(linha):
                    achados.append((str(rel), n, m.group(0), bruto))

    print(f"ficheiros varridos: {len(ficheiros())}")
    if not achados:
        print("nenhuma frase não-portuguesa detectada")
        return 0

    print(f"\n{len(achados)} ocorrência(s):\n")
    vistos: set[tuple[str, int, str]] = set()
    for f, n, achado, bruto in achados:
        chave = (f, n, achado)
        if chave in vistos:
            continue
        vistos.add(chave)
        marca = f"  {f}:{n}  {achado!r}  (padrão {bruto!r})"
        try:
            linha = (RAIZ / f).read_text(encoding="utf-8").splitlines()[n - 1]
        except (UnicodeDecodeError, OSError, IndexError):
            linha = ""
        print(marca)
        print(f"      {linha.strip()[:120]}")
    return 1


if __name__ == "__main__":
    sys.exit(main())