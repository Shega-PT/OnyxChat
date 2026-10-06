# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Auditoria de português europeu — anglicismos e americanismos.

Varre comentários e documentos à procura de construções que são
inglesas ou brasileiras e que deveriam ser europeias. Não corrige nada:
reporta com ficheiro, linha e o trecho, para se decidir caso a caso.

O que se procura:

* **americanismos**: "arquivo", "usuário", "senha", "tela", "celular",
  "fato" no sentido de "facto", "time-out", "endereço de e-mail" com
  "e-mail" usado como "correio".
* **anglicismos de código**: "handler", "worker", "payload", "buffer"
  (que é aceite), "commit" (que é aceite), "issue" (que é aceite).
  Estes últimos ficam fora porque são vocabulário técnico consagrado
  em português e traduzi-los tornaria o código menos legível para quem
  lê a community.
* **frases bittenas** que sobraram de edições anteriores — palavras que
  não fazem sentido em português, o sintoma mais fiável de um ficheiro
  que foi mexido sem revisão.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent

# (palavra, o que escrever) — só o que é inequivocamente errado.
AMERICANISMOS = {
    r"\barquivo(s)?\b": "ficheiro",
    r"\busuário(s)?\b": "utilizador",
    r"\busuarios\b": "utilizadores",
    r"\bsenha(s)?\b": "palavra-passe",
    r"\btela(s)?\b": "ecrã",
    r"\bcelular(es)?\b": "telemóvel",
    r"\bcelulares\b": "telemóveis",
    r"\bfaturar\b": "emitir factura",
    r"\bdefaults?\b": "predefinição",
    r"\bdefault\b": "predefinição",
    r"\bperformance\b": "desempenho",
    r"\bdeploy\b": "publicação",
    r"\bdeployar\b": "publicar",
    r"\bcommit(s)?\b": "commit (aceite — vocabulário consagrado)",
    r"\bscreen(s)?\b": "ecrã",
}

# Construções que são erro de português, não escolha de variante.
GRAMATICA = [
    # Duplicação dearticles
    (r"\ba o\b(?=\s+(guarda|resto|lista|mesmo|conjunto|campo|ponto)\b)", "a+o"),
    (r"\bo a\b(?=\s+(guarda|resto|lista|mesmo|conjunto)\b)", "o+a"),
    # "há" onde se quer "a" (há + tempo)
    (r"\bhá (decisões|decisão|anos|meses|dias|horas)\b", "há — correcto"),
    # Duplicação de preposições herdada de traduções
    (r"\bnão\s+não\b", "não"),
    (r"\bde\s+de\b", "de"),
    # Concordância: "os" antes de feminino
    (r"\bos (chaves|amizades|mensagens|ligações|vagas|entradas)\b", "as \\1"),
    (r"\bas (ficheiros|ficheiros)\b", "os ficheiros"),
    # Article + substantivo feminino plural
    (r"\boas (chaves|entradas|vagas)\b", "as \\1"),
]

# Vocabulário técnico consagrado: NÃO deve ser traduzido.
ACEITOS = {
    "handler", "worker", "payload", "commit", "commit(s)", "issue",
    "branch", "merge", "push", "pull", "request", "diff", "staging",
    "toolchain", "runtime", "backend", "frontend", "build", "release",
    "link", "scope", "target", "input", "output", "debug", "check",
    "socket", "buffer", "mutex", "thread", "shutdown", "setup",
}

IGNORADOS = {
    ".git", "target", ".venv", "build", "__pycache__", ".hypothesis",
    ".pytest_cache", "vendor",
}

# Ficheiros que contêm os americanismos de propósito: são o padrão de
# procura, e reportá-los seria ruído.
FORA_DA_BUSCA = {"scripts/verificar_portugues.py"}

# Construções onde a palavra inglesa é nome de identificador, não
# português traduzido. `default=` é keyword de Python, `default =` é chave
# de TOML, `default-features` é flag do Cargo.
IDENTIFICADORES = re.compile(
    r"\bdefault\s*[=,)(]"            # `default=` e `default =`
    r"|default-features"                # flag do Cargo
    r"|impl\s+Default\s+for"            # trait da std
    r"|#\[default\]"                   # atributo de enum
    r"|::default\(\)"                  # `Self::default()`
    r"|::new\(\)"                      # `X::new()`
    r"|\bSelf::",
    re.IGNORECASE,
)


def ficheiros() -> list[Path]:
    r = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        cwd=RAIZ, capture_output=True, text=True,
    )
    saida = []
    for linha in r.stdout.splitlines():
        p = RAIZ / linha
        if any(parte in IGNORADOS for parte in p.parts):
            continue
        if p.suffix not in {".rs", ".py", ".md", ".c", ".cpp", ".h", ".lua",
                            ".toml", ".sh", ".json", ".txt"}:
            continue
        saida.append(p)
    return saida


def limpar(linha: str) -> str:
    """Tira blocos de código e URLs, onde o inglês é correcto."""
    linha = re.sub(r"`[^`]*`", " ", linha)
    linha = re.sub(r"https?://\S+", " ", linha)
    linha = re.sub(r"\b[\w.-]+\.(rs|py|md|json|toml|sh|lua|a|h|cpp|c)\b", " ", linha)
    return linha


def main() -> int:
    padroes = [(re.compile(p, re.IGNORECASE), o) for p, o in AMERICANISMOS.items()]
    achados: list[tuple[str, int, str, str]] = []
    total_linhas = 0

    for caminho in ficheiros():
        try:
            texto = caminho.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        rel = caminho.relative_to(RAIZ)
        if str(rel) in FORA_DA_BUSCA:
            continue
        for n, linha in enumerate(texto.splitlines(), 1):
            total_linhas += 1
            limpa = limpar(linha)
            limpa = IDENTIFICADORES.sub(" ", limpa)
            for rx, sugerido in padroes:
                for m in rx.finditer(limpa):
                    palavra = m.group(0)
                    if palavra.lower() in ACEITOS:
                        continue
                    achados.append((str(rel), n, palavra, sugerido))

    print(f"ficheiros varridos, {total_linhas} linhas\n")
    if not achados:
        print("nenhum americanismo encontrado")
        return 0

    for f, n, palavra, sugerido in achados:
        print(f"  {f}:{n}  {palavra!r} -> {sugerido}")
    print(f"\ntotal: {len(achados)}")
    return 1


if __name__ == "__main__":
    sys.exit(main())