# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Proíbe que a documentação e os comentários falem da máquina.

## A regra

Um pico de 931 MB é uma propriedade do **build** — do grafo de crates e
dos perfis de `Cargo.toml` — e não do computador onde o build corre. A
máquina decide é se cabe. Documentar «nesta máquina o pico foi 931 MB»
prende o número a um ambiente que quem lê não tem, e sugere que o número
pode ser outro, o que é falso.

O mesmo vale para o tempo: «era recusada de manhã» é uma afirmação que
deixa de ser verdade à meia-noite, e que ninguém pode verificar.

A regra que este programa impõe, então, é:

```text
diz ONDE a medição foi feita e o que dela é generalizável
nunca «nesta máquina», «minha máquina», «aqui», «de manhã»
```

## O que fica de fora, e porquê

**O texto dirigido a quem usa a aplicação.** `UI/src/pages/Registar.jsx`
diz «Tudo fica neste computador», e está certo: é o computador da
pessoa que está a criar a conta, não o de quem escreveu a frase. Por isso
este programa varre **linhas de comentário** nos ficheiros de código, e
não o ficheiro inteiro — as cadeias de literais nunca são lidas.

## A isenção

Uma linha com `verificar_ambiente.py: ignorar` é saltada.
`scripts/verificar_gitignore.sh` usa-o numa linha que **cita** a frase
proibida para explicar qual é o problema; reescrevê-la destruiria o
sentido. A isenção é explícita e pesquisável, o que é o que impede um
portão de ser desligado por irritação — um portão que obriga a mutilar a
prosa acaba desactivado, e desactivado não protege nada.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent

#: Documentos lidos por inteiro: a prosa é o ficheiro.
DOCUMENTOS = {".md", ".txt", ".rst"}

#: Extensões de código. Aqui só as **linhas de comentário** são lidas.
CODIGO = {
    ".rs", ".py", ".sh", ".c", ".cpp", ".h", ".lua",
    ".js", ".jsx", ".mjs", ".cjs", ".lua",
}

#: `UI/tools/node` é um Node.js completo descompactado por
#: `UI/tools/onyxchat instalar`. Não é nosso, e varrer 2 369 cabeçalhos e
#: ficheiros de um pacote de terceiro para procurar português é tempo
#: gasto a proteger a prosa de outra pessoa. `scripts/verificar_gitignore.sh`
#: já o declara como artefacto não versionado; aqui é a mesma decisão.
#: Nomes de directório, em qualquer sítio da árvore.
IGNORADOS = {
    ".git", "target", "node_modules", "dist", "__pycache__",
    ".pytest_cache", ".hypothesis", ".venv", "build", "vendor",
}

#: Caminhos deprefixo. Têm de ser comparados como tuplas de segmentos, e
#: não como uma cadeia: `{"UI/tools/node"} & {"UI", "tools", "node"}` é
#: vazio, e foi exactamente isso que fez a primeira versão deste
#: programa varrer 2 369 ficheiros do Node sem os excluir.
CAMINHOS_IGNORADOS = {("UI", "tools", "node")}

#: As frases que apanha. Um `\b` à volta de «máquina» não chega:
#: o que interessa é a locução inteira, não a palavra solta.
REGRAS: list[tuple[str, str]] = [
    # `mesma` **não** está aqui, e é deliberado: «a mesma máquina» é um
    # termo técnico — é o que define a ameaça A8, «outro utilizador do
    # mesmo computador» — e existe em `threat_model.md`,
    # `key_management.md`, `ipc_spec.md` e `server/origem.py`. A regra
    # proíbe a referência ao ambiente de quem escreve, não a palavra.
    (r"\b(?:esta|estaque|nesta|nestaque|minha)\s+m[áa]quina\b",
     "referência à máquina de quem escreve"),
    (r"\bnest[ae]\s+(?:port[áa]til|computador|pc|equipamento|host)\b"
     r"(?!.{0,40}(?:do utilizador|da pessoa|do cliente))",
     "referência ao computador de quem executa"),
    (r"\b(?:aqui|nesta\s+m[áa]quina)\s+(?:morreu|morre|estava|funciona|"
     r"correu|falhou|estourou|bateu|demorou)\b",
     "narração dependente do momento ou do sítio"),
    (r"\bde\s+manh[ãa]\b|\bontem\b|\b(?:nesta|na)\s+(?:sess[ãa]o|"
     r"semana)\s+passada\b",
     "referência a um momento que já passou"),
    (r"\b[àa]s\s+\d{1,2}[:h]\d{2}\b(?![^\n]{0,30}(?:registado|registada|"
     r"no log|na tabela|\|))",
     "hora de relógio como referência"),
    (r"\bmemória\s+desta\s+m[áa]quina\b|\bnesta\s+m[áa]quina\b",
     "referência à máquina de quem escreve"),
]

#: A isenção, e a linha que a documenta.
ISENCAO = "verificar_ambiente.py: ignorar"


def enumerar() -> list[Path]:
    """Os ficheiros que este programa lê, ordenados."""
    ficheiros = []
    for p in RAIZ.rglob("*"):
        if not p.is_file() or p.suffix.lower() not in DOCUMENTOS | CODIGO:
            continue
        partes = p.relative_to(RAIZ).parts
        if set(partes) & IGNORADOS:
            continue
        if any(partes[: len(caminho)] == caminho for caminho in CAMINHOS_IGNORADOS):
            continue
        ficheiros.append(p)
    return sorted(ficheiros)


def sem_marca(linha: str) -> str:
    """A linha sem a marca de comentário nem os espaços.

    O marcador de isenção escreve-se como ``# verificar_ambiente.py:
    ignorar``, e comparar o seu início contra o texto da linha não
    bateria nunca: a linha começa por ``#``. Tirá-lo primeiro é o que
    faz o marcador funcionar dentro de um comentário de shell, que é
    onde está a única cita da frase proibida.
    """
    limpa = re.sub(r"^\s*(?:#+|//+|\*+\s?)+", "", linha).strip()
    # A marcação de integral é outra decoração: o marcador escreve-se
    # `verificar_ambiente.py: ignorar` entre crases, como tudo o que é
    # nome de programa neste projecto.
    return limpa.strip("`").strip()


def e_comentario(ficheiro: Path, linha: str) -> bool:
    """A linha é comentário, e não código nem texto para o utilizador?

    O teste é por marca de linguagem, e é exacto para este projecto:
    `#` em shell e Python, `//` e `/*` em Rust, JavaScript, C e Lua.

    Uma cadeia que comece por `//` — um URL — conta como comentário, e
    é uma limitação declarada: `verificar_comentarios.py` tem a mesma.
    O custo é um falso positivo ocasional; o benefício é não escrever um
    analisador de cada linguagem.
    """
    limpa = linha.strip()
    if limpa.startswith(("#", "//", "/*", "*", "///", "//!")):
        return True
    # Comentário no fim de uma linha de código, em Rust/C/JS.
    return bool(re.search(r"(?:^|\s)(?://|#)\s", linha)) and not _e_cadeia(linha, linha)


def _e_cadeia(_ficheiro: Path, linha: str) -> bool:
    """A linha abre uma cadeia de caracteres antes do(s) marca(s)."""
    marca = linha.find("//")
    if marca < 0:
        marca = linha.find("#")
    if marca < 0:
        return False
    antes = linha[:marca]
    return antes.count('"') % 2 == 1 or antes.count("'") % 2 == 1


def procurar(ficheiro: Path) -> list[tuple[int, str, str]]:
    """As ocorrências de um ficheiro: ``(linha, regra, texto)``."""
    achados = []
    try:
        texto = ficheiro.read_text(encoding="utf-8", errors="replace").split("\n")
    except OSError:
        return []
    eh_documento = ficheiro.suffix.lower() in DOCUMENTOS
    isento = False

    for n, linha in enumerate(texto, 1):
        # A isenção em bloco: uma linha que seja só o marcador vale até
        # ao fim do trecho. Sem isto, citar a frase proibida obrigaria a
        # escrever o marcador no meio da frase — e um portão que obriga a
        # mutilar a prosa acaba desactivado.
        #
        # O fim do bloco **depende do ficheiro**, e a distinção não é
        # caprice: num documento não há marcas de comentário, e o fim
        # natural é a linha em branco que separa o trecho isento do
        # resto. Num ficheiro de código, todas as linhas sem marca são
        # código e nunca são lidas — por isso o fim é a primeira linha
        # que não seja comentário, e nada mais.
        if sem_marca(linha).startswith(ISENCAO):
            isento = True
            continue
        if isento:
            if eh_documento and not linha.strip():
                isento = False
            elif not eh_documento and not e_comentario(ficheiro, linha):
                isento = False
            continue
        if ISENCAO in linha:
            continue

        if not eh_documento and not e_comentario(ficheiro, linha):
            continue

        for padrao, motivo in REGRAS:
            if re.search(padrao, linha, re.IGNORECASE):
                achados.append((n, motivo, linha.strip()[:70]))
                break
    return achados


def main() -> int:
    ficheiros = enumerar()
    if not ficheiros:
        print("nenhum ficheiro lido — a enumeração mudou?",
              file=sys.stderr)
        return 2

    total = 0
    for f in ficheiros:
        achados = procurar(f)
        if not achados:
            continue
        rel = f.relative_to(RAIZ)
        for n, motivo, texto in achados:
            total += 1
            print(f"  {rel}:{n}  {motivo}")
            print(f"      {texto}")

    print(f"\nficheiros lidos: {len(ficheiros)}")
    if total:
        print(f"{total} ocorrência(s) ligada(s) a uma máquina ou a um momento")
        print("  a regra é `docs/testing.md` §Os portões de entrega")
        return 1
    print("nenhuma referência à máquina de quem escreve")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())