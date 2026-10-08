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
    # erro: passavam a parecer erro a quem viesse estender a lista.
    #
    # `codigo` com lookahead, e não `codigo`: sem ele, `codigo:`
    # — que é um dicionário, não um erro de ortografia — era reportado
    # como se fosse.
    (r"\bproprio\b", "próprio"),
    (r"\bproprietario\b", "proprietário"),
]

IGNORADOS = {".git", "target", ".venv", "build", "__pycache__",
             ".hypothesis", ".pytest_cache", "vendor", "node_modules",
             "UI/dist"}
FORA = {"scripts/verificar_portugues.py", "scripts/verificar_frases.py",
        "scripts/verificar_comentarios.py",
        # Os testes do verificador de português guardam os americanismos
        # como dados de entrada. Reportá-los aqui seria reportar o
        # próprio teste — e o mesmo ficheiro já está fora da busca do
        # `verificar_portugues.py`, pelo mesmo motivo.
        "tests/test_verificar_portugues.py",
        # O ficheiro de testes do auditor de comentários guarda os
        # americanismos como dados de entrada, para provar que a extracção
        # de prosa os alcança. Reportá-los seria reportar o próprio teste.
        "tests/test_verificar_comentarios.py"}


def prosa_de(f: Path) -> list[tuple[int, str]]:
    """Devolve [(linha, texto)] com o que é comentários e docstrings.

    Um ficheiro ilegível devolve lista vazia em vez de rebentar. Os
    outros dois auditores já faziam isto, e a assimetria custou uma
    excepção em 2026-10-07 ao varrer a interface: `git ls-files` lista
    ficheiros **indexados**, e um ficheiro apagado na árvore de trabalho
    continua indexado durante o `git rm` — ou de um `git add` antigo. Um
    auditor que rebenta não audita.
    """
    try:
        texto = f.read_text(encoding="utf-8", errors="replace").split("\n")
    except OSError:
        return []
    ext = f.suffix
    saida: list[tuple[int, str]] = []

    if ext in {".rs", ".c", ".cpp", ".h", ".js", ".jsx", ".mjs", ".cjs"}:
        # ## A limitação declarada
        #
        # Isto lê a linha **começada** pela marca de comentário. Uma
        # cadeia de caracteres que comece por `//` — um URL inteiro, por
        # exemplo — conta como comentário e é reportada. É a mesma
        # limitação que os ramos de Rust e C já tinham, e é o preço de
        # não escrever um analisador de JavaScript.
        #
        # O custo é um falso positivo ocasional num URL; o benefício é
        # que o verificador não fica em silêncio sobre a interface. Um
        # verificador que devolve vazio porque não sabe é pior do que um
        # que se engana uma vez — porque o silêncio não se corrige.
        #
        # ## Porque JSX precisa de tratamento à parte
        #
        # Em JSX o comentário é `{/* … */}`: o `{` não é comentário, e
        # sem o desembrulho a linha não começaria por nenhuma marca.
        dentro = None
        jsx = ext in {".js", ".jsx", ".mjs", ".cjs"}
        for n, linha in enumerate(texto, 1):
            limpa = linha.strip()
            if jsx and limpa.startswith("{"):
                limpa = limpa[1:].strip()
                if limpa.endswith("}"):
                    limpa = limpa[:-1].strip()
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

    elif ext in {".yml", ".yaml"}:
        # ## A prosa de um workflow são os comentários
        #
        # Não há mais texto a ler: os nomes dos passos e dos jobs são
        # identificadores. O conteúdo de `run:` é shell, e os comandos
        # não são prosa — mas o **comentário** dentro do bloco é, e é
        # auditado, porque é onde se explica porque é que o passo
        # existe.
        #
        # Este ramo **tem de existir**. Sem ele, `.yml` caía no `else`
        # final, devolvia lista vazia, e o auditor passava em silêncio
        # sobre a única configuração que decide se o projecto é
        # verificado. É o mesmo buraco que o JavaScript tinha.
        for n, linha in enumerate(texto, 1):
            limpa = linha.strip()
            if limpa.startswith("#"):
                saida.append((n, limpa.lstrip("#").strip()))

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
        # `.js`, `.jsx`, `.mjs` e `.cjs` desde 2026-10-07.
        if p.suffix in {".rs", ".py", ".md", ".c", ".cpp", ".h", ".lua",
                        ".js", ".jsx", ".mjs", ".cjs",
                        ".yml", ".yaml",
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