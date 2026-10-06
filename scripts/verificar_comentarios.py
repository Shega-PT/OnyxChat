# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Americanismos **apenas em comentários e docstrings**.

`verificar_portugues.py` varre o ficheiro inteiro e por isso apanha
identificadores — `versao=`, `VERSAO`, `MEMORIA`. Renomear esses não é
correcção de português: são nomes internos, e trocar `versao` por
`versão` num identificador seria pior, não melhor.

Este ficheiro isola o que interessa: **prosa**. Em Rust, C e C++ são
as linhas de comentário; em Python, os comentários `#` e o texto das
docstrings. É aí que se lê português — é para aí que vai a correcção.
"""

from __future__ import annotations

import io
import re
import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent

# (regex, forma portuguesa). Só o que é inequivocamente errado.
REGRAS = [
    (r"\busuário(s)?\b", "utilizador"),
    (r"\barquivo(s)?\b", "ficheiro"),
    (r"\bsenha(s)?\b", "palavra-passe"),
    (r"\btela(s)?\b", "ecrã"),
    (r"\bcelular(es)?\b", "telemóvel"),
    (r"\bconexão|conexões\b", "ligação / ligações"),
    (r"\bfaturar\b", "emitir factura"),
    (r"\bperformance\b", "desempenho"),
    (r"\bdeployar\b", "publicar"),
    (r"\bvoc\^?ê\b", "tu / o utilizador"),
    (r"\bgerando\b", "a gerar"),
    (r"\bfazendo\b", "a fazer"),
    (r"\bcriando\b", "a criar"),
    (r"\benfileiramento\b", "colocação em fila"),
    (r"\bdeliciar\b", "eliminar"),
    (r"\baplicativo(s)?\b", "aplicação"),
    (r"\bcodigo\b(?=[^:])", "código"),
    # NOTA: "computacional", "comportamento" e "baixar o limite" são
    # português correcto e foram removidos da lista depois de a regra dar
    # falso positivo. Deixá-los levar a correcção seria pior do que o
    # erro: passavam a parecer erro a quem viesse estender a lista.        # não `codigo:` (identificador)
    (r"\bproprio\b", "próprio"),
    (r"\bproprietario\b", "proprietário"),
]

IGNORADOS = {".git", "target", ".venv", "build", "__pycache__",
             ".hypothesis", ".pytest_cache", "vendor"}
FORA = {"scripts/verificar_portugues.py", "scripts/verificar_frases.py",
        "scripts/verificar_comentarios.py"}


def prosa_de(f: Path) -> list[tuple[int, str]]:
    """Devolve [(linha, texto)] com o que é comentários e docstrings."""
    texto = f.read_text(encoding="utf-8", errors="replace").split("\n")
    ext = f.suffix
    saida: list[tuple[int, str]] = []

    if ext in {".rs", ".c", ".cpp", ".h"}:
        dentro = None
        for n, linha in enumerate(texto, 1):
            limpa = linha.strip()
            if dentro is not None:
                saida.append((n, limpa))
                if dentro in limpa:
                    dentro = None
                continue
            if limpa.startswith("/*"):
                saida.append((n, limpa))
                if "*/" not in limpa[2:]:
                    dentro = "*/"
                continue
            for marca in ("///", "//", "*"):
                if limpa.startswith(marca):
                    saida.append((n, limpa[len(marca):].strip()))
                    break

    elif ext in {".py", ".sh"}:
        dentro_doc = False
        # Docstrings abrem com `"""` e fecham com `"""` na mesma linha
        # ou mais abaixo. Um contador simples basta para texto nosso.
        for n, linha in enumerate(texto, 1):
            limpa = linha.strip()
            if dentro_doc:
                saida.append((n, limpa))
                if '"""' in limpa:
                    dentro_doc = False
                continue
            if limpa.startswith("#"):
                saida.append((n, limpa.lstrip("#").strip()))
                continue
            if limpa.startswith(('"""', "'''")) and not (
                limpa.startswith('"""') and limpa.count('"""') >= 2
                and len(limpa) > 3
            ):
                saida.append((n, limpa))
                dentro_doc = True
                continue

    elif ext in {".md", ".lua", ".toml"}:
        for n, linha in enumerate(texto, 1):
            saida.append((n, linha.strip()))

    return saida


def ficheiros() -> list[Path]:
    r = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        cwd=RAIZ, capture_output=True, text=True)
    saida = []
    for linha in r.stdout.splitlines():
        p = RAIZ / linha
        if any(x in p.parts for x in IGNORADOS):
            continue
        if str(p.relative_to(RAIZ)) in FORA:
            continue
        if p.suffix in {".rs", ".py", ".md", ".c", ".cpp", ".h", ".lua",
                        ".sh", ".toml"}:
            saida.append(p)
    return saida


def main() -> int:
    padroes = [(re.compile(p, re.IGNORECASE), certo) for p, certo in REGRAS
               if certo is not None]
    achados = []
    for f in ficheiros():
        rel = f.relative_to(RAIZ)
        for n, linha in prosa_de(f):
            limpa = re.sub(r"`[^`]*`", " ", linha)      # código inline
            limpa = re.sub(r"https?://\S+", " ", limpa)   # URL
            limpa = re.sub(r"\b\w+\.(rs|py|md|json|toml|sh|lua|a|h)\b", " ",
                           limpa)                        # nomes de ficheiro
            for rx, certo in padroes:
                for m in rx.finditer(limpa):
                    achados.append((str(rel), n, m.group(0), certo))

    print(f"ficheiros em prosa: {len(ficheiros())}")
    if not achados:
        print("nenhum americanismo na prosa")
        return 0
    vistos = set()
    for f, n, achado, certo in achados:
        if (f, n, achado) in vistos:
            continue
        vistos.add((f, n, achado))
        print(f"  {f}:{n}  {achado!r} -> {certo}")
    print(f"\ntotal: {len(vistos)}")
    return 1


if __name__ == "__main__":
    sys.exit(main())