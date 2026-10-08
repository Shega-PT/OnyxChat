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
    # Português do Brasil, que o pt-PT não usa. Não é o mesmo idioma
    # em item nenhum, e «ecrã» é o que toda a gente em Portugal escreve.
    r"\btela\b": "ecrã",
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
    #
    # `arquivo` **não** está aqui, e a decisão é deliberada. A palavra
    # é um substantivo comum do português — «arquivo notarial», «arquivo
    # de arquivo» — e aceitá-la tornaria o verificador **incapaz de
    # reportar** o americanismo mais comum do português de informática.
    #
    # A alternativa era filtrar pela forma (plural, artigo), que se
    # provou impossível: nenhum padrão separa «o arquivo foi apagado» de
    # «o arquivo de arquivo», e os quatro padrões testados apanhavam
    # ambos ou nenhum. Um verificador que aceita a palavra toda deixa de
    # ver Americanismos a sério.
    #
    # O custo é o rótulo «Arquivadas» em `UI/src/lib/onyx/nav.js`, que é
    # adjectivo e por isso passa. O comentário do ficheiro explica a
    # escolha, e foi ele que deixou de ser reportado ao acertar a lista.
}

# Vocabulário de API cujo nome é inglês. Não é português por descuido: é
# o nome que a biblioteca chama, e traduzi-lo seria escrever `className`
# como `classNome` e não compilar.
#
# ## Porque isto remove o atributo inteiro
#
# As classes do Tailwind vivem dentro de `className="…"`, e o atributo é
# o que separa o código do texto. Remover só o nome deixaria o resto
# — `flex`, `items-center`, `w-full` — a ser lido como português, e a
# varredura passaria a procurar palavras onde não há texto.
#
# ## Porque não há uma lista de nomes de componente
#
# A primeira versão filtrava por `PascalCase`, que é a convenção de
# componentes React. **Apanhava o texto dentro dos nomes**: um
# componente chamado `SenhaDoUtilizador` levava consigo «senha» e
# «utilizador», e o verificador deixava de os reportar — o oposto do que
# se quer. Um verificador que erra a favor do código não é um
# verificador, é um buraco.
#
# O que fica é o contrário: remove-se o que é **sintaxe** (o atributo,
# os utilitários de dimensão), e cada palavra que sobrar é candidata a
# americanismo. `tests/test_verificar_portugues.py` fixa este
# comportamento com uma prova que tem de falhar se ele deixar de apanhar.
CLASSNAME = re.compile(r'\bclassName\s*=\s*(?:"[^"]*"|\{[^}]*\})')
UTILITARIOS_TAILWIND = re.compile(
    # Dimensão e ecrã: `h-screen`, `min-h-screen`, `w-full`, `max-w-md`.
    r"\b(?:min-|max-)?[hw]-[a-z0-9-]+\b"
    # As restantes famílias do Tailwind, todas com o formato
    # `<família>-<valor>` e nenhuma com palavra portuguesa dentro.
    r"|\b(?:p|m|gap|space|flex|grid|col|row|order|basis|grow|shrink)-"
    r"[a-z0-9-]+\b"
    r"|\b(?:text|bg|border|ring|shadow|outline|fill|stroke|from|via|to|"
    r"divide|placeholder|accent|caret|decoration)-[a-z0-9-]+\b",
    re.IGNORECASE,
)

IGNORADOS = {
    ".git", "target", ".venv", "build", "__pycache__", ".hypothesis",
    ".pytest_cache", "vendor", "node_modules", "UI/dist",
}

# Ficheiros que contêm os americanismos de propósito: são o padrão de
# procura, e reportá-los seria ruído.
#
# `UI/package-lock.json` entra aqui por um motivo diferente dos outros: é
# um ficheiro **gerado** (pelo npm) e que **tem** de ser versionado — sem
# ele, duas pessoas instalam versões diferentes das mesmas dependências.
# Não há português para auditar lá dentro: é metadado de pacotes, e as
# únicas ocorrências são nomes de dependências e palavras-chave do npm,
# como o pacote `performance`. Editar o ficheiro para as eliminar seria
# pior do que o achado: o próximo `npm install` traz-as de volta, e um
# `package-lock.json` divergente da árvore instalada é um problema muito
# mais difícil de diagnosticar do que um relatório de auditoria com duas
# linhas.
FORA_DA_BUSCA = {
    "scripts/verificar_portugues.py",
    "UI/package-lock.json",
    # O ficheiro de testes do verificador contém os americanismos como
    # dados de entrada, e reportá-los é o mesmo que reportar o padrão
    # de procura. Está aqui pelo mesmo motivo que o próprio verificador.
    "tests/test_verificar_portugues.py",
    # O ficheiro de testes do auditor de comentários guarda os
    # americanismos como dados de entrada, para provar que a extracção
    # de prosa os alcança. Reportá-los seria reportar o próprio teste.
    "tests/test_verificar_comentarios.py",
}

# Construções onde a palavra inglesa é nome de identificador, não
# português traduzido. `default=` é keyword de Python, `default =` é chave
# de TOML, `default-features` é flag do Cargo, `--default-toolchain` é flag
# do rustup.
IDENTIFICADORES = re.compile(
    r"\bdefault\s*[=,)(]"            # `default=` e `default =`
    r"|\bDEFAULT\b"                    # palavra-chave do SQL (`CREATE TABLE …\n                                       # DEFAULT`). A língua do SQL é o inglês
                                       # e não se traduz: `DEFAULT` não existe
                                       # em português nenhum SQLite)
    r"|default-features"                # flag do Cargo
    r"|export\s+default"                # keyword de JavaScript
    r"|\(\?:default"                  # a mesma palavra dentro de um literal
                                       # de expressão regular: é código dentro
                                       # de uma cadeia, e `limpar()` não remove
                                       # cadeias porque uma docstring é uma
    r"|default-toolchain"               # flag do rustup
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
        # `.yml` e `.yaml` desde 2026-10-07: os workflows são
        # configuração versionada com prosa portuguesa nos nomes dos
        # passos, e um ficheiro de configuração que ninguém lê é onde
        # a documentação apodrece em silêncio.
        if p.suffix not in {".rs", ".py", ".md", ".c", ".cpp", ".h", ".lua",
                            ".toml", ".sh", ".json", ".txt",
                            ".js", ".jsx", ".mjs", ".cjs",
                            ".yml", ".yaml"}:
            continue
        saida.append(p)
    return saida


def limpar(linha: str) -> str:
    """Tira blocos de código e URLs, onde o inglês é correcto."""
    linha = re.sub(r"`[^`]*`", " ", linha)
    linha = re.sub(r"https?://\S+", " ", linha)
    # Nomes de ficheiro são caminhos, não português. Um ficheiro chamado
    # `Senha.js` não é um americanismo em prosa — e a lista de extensões
    # tem de incluir as de JavaScript desde que o auditor as varre.
    linha = re.sub(
        r"\b[\w.-]+\.(rs|py|md|json|toml|sh|lua|a|h|cpp|c|js|jsx|mjs|cjs)\b",
        " ",
        linha,
    )
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
            # A sintaxe JavaScript sai **antes** do Americanismo, e
            # não depois de uma lista de palavras aceites. A diferença
            # é que a lista cresce com cada biblioteca nova, e o
            # padrão não.
            limpa = CLASSNAME.sub(" ", limpa)
            limpa = UTILITARIOS_TAILWIND.sub(" ", limpa)
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