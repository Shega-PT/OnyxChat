# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Confere que o mapa `Estrutura.txt` ainda corresponde à árvore.

## Porque isto existe a mais

`Estrutura.txt` fica **fora** do repositório, um nível acima da raiz, e
é o único documento do projecto que ninguém abre como parte do
trabalho. Por isso era o mais desatualizado de todos: listava sete
ficheiros que não existiam — referências Python que nunca chegaram a
escrever-se, dois módulos Lua absorvidos por `deslocamento.lua`, e um
`messenger/plugins_loader.py` que nunca existiu — e omitia `scripts/`,
`fuzz/`, a interface inteira e `messenger/conta.py`.

Um mapa desatualizado não é um documento inútil: é um documento que
responde com confiança a perguntas cuja resposta já não é verdade. Alguém
que leia «o carregador de plugins está em `messenger/plugins_loader.py`»
vai à procura de um ficheiro que não está lá, e a conclusão natural é
que o ficheiro se apagou.

Um verificador de texto não teria apanhado nada disto — a divergência é
de **presença**, não de grafia. Só uma leitura da árvore o apanha.

## As duas afirmações, e porque são de tipos diferentes

O mapa faz dois tipos de afirmação, e verificá-las igual seria perder
a distinção que as torna úteis:

* **`caminho`** — «este ficheiro existe». É a afirmação normal. Se o
  ficheiro desaparecer, o mapa está errado e isso é **erro**.
* **`~caminho`** — «este ficheiro **não** existe», com um til, como
  uma lápide. Só aparece na secção que reconstitui os erros da versão
  anterior, e é a única forma de esses nomes serem legíveis sem
  parecerem uma promessa. Se um dia alguém criar
  `messenger/plugins_loader.py`, o til mente, e isso também é **erro** —
  porque a nota histórica passou a ser falsa.

Sem o til, a secção histórica seria indistinguível de uma lista de
ficheiros reais, e o verificador reportaria como «em falta» um ficheiro
que o próprio mapa diz que nunca existiu. Com ele, a nota deixa de ser
anecdótica e passa a ser conferida.

## A árvore é recontruída, não lida

O mapa é uma árvore com indentação de quatro colunas, e um ficheiro
aparece pelo seu nome — `index.md`, não `docs/index.md`. Um leitor de
caminhos que faça `split('.')` e compare com a árvore não encontra
`docs/index.md` em lado nenhum, e reporta 117 ficheiros em falta que
existem todos.

Daí a pilha: cadadirectório encontrado empurra o seu nome, e cada
ficheiro fecha o caminho com o que a pilha tem por baixo. As secções
que nomeiam um directório no cabeçalho — `docs/ — 19 documentos` — são
o pai dos seus próprios filhos, porque é assim que o mapa está escrito.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent

#: Onde o mapa vive **agora**: dentro do repositório.
#:
#: Until 2026-10-07 it was one level above, in `../Estrutura.txt`. Being
#: there was the reason it rotted: a file that is not in the repository
#: is not tracked, is not cloned, and cannot be checked by a continuous
#: integration run — the three things that keep a map honest. It listed
#: seven files that did not exist for nobody to notice.
MAPA = RAIZ / "Estrutura.txt"

# O que nunca é versionado nem escrito à mão.
IGNORADOS = {
    ".git", "target", "node_modules", "dist", "__pycache__",
    ".pytest_cache", ".hypothesis", ".venv", "build",
}

# Nome de ficheiro ou directório, com ponto de extensão ou barra final.
_NOME = re.compile(r"[\w.][\w./+-]*")
_TIL = "~"

# O nome que o próprio mapa dá ao projecto, e que não é um directório
# dentro do repositório.
_RAIZ_DO_PROJECTO = "onyxchat"

# `├─ docs/index.md` — a coluna onde aparece o marcador é a profundidade.
#
# O conteúdo é `.*` e não `\S+` porque uma linha pode nomear vários
# ficheiros: `├─ main.jsx  App.jsx  index.css`. Com `\S+` só o primeiro
# entrava na lista, e a verificação passava por cima de dois ficheiros
# que o mapa afirma existirem — o mesmo silêncio do buraco original,
# noutra forma.
#
# A profundidade **não** é a coluna dividida por uma constante: ver
# `unidade_de_indentacao`.
_MARCADOR = re.compile(r"^([│ ]*)(\d*)[├└]─\s+(.*)$")


def unidade_de_indentacao(linhas: list[str]) -> int:
    """Quantas colunas tem um nível de indentação, neste documento.

    ## Porque é que isto é medido e não constante

    Os dois documentos com árvore não usam a mesma unidade, e ninguém
    disse que devessem:

    * `Estrutura.txt` indenta com **4** colunas: `│   ├─ …`;
    * `UI/README.md` indenta com **3**: `   │  ├─ …`.

    Uma fórmula com a constante embutida dava 3 // 4 = 0, e `src/`,
    `lib/`, `components/` e `pages/` ficavam todos ao mesmo nível. O
    resultado era `components/onyx` em vez de `src/components/onyx` — que
    não existe — e a contagem devolvia zero **sem dar erro nenhum**, o
    que é a pior forma de um verificador falhar.

    A unidade mede-se pelo menor prefixo não vazio do documento. É a
    única definição que não pressupõe nada sobre quem escreveu a árvore.
    """
    varios = []
    for linha in linhas:
        m = _MARCADOR.match(linha)
        if m:
            n = len(m.group(1)) + len(m.group(2))
            if n:
                varios.append(n)
    return min(varios) if varios else 1
# `docs/ — 19 documentos`: o cabeçalho de secção que é pai dos filhos.
_SECAO = re.compile(r"^#\s+([\w.-]+/)")
# `# 1. Criptografia`: as secções numeradas não levam sublinhado, e sem
# esta regra o contexto da secção anterior (`scripts/`) sobreviveria a
# elas — cada `lib.rs` do daemon seria lido como `scripts/lib.rs`.
_NUMERO = re.compile(r"^#\s+\d+\.")
# `crypto/` sozinho numa linha: o directório da secção, escrito sem
# marcador porque é o primeiro. Sem esta regra ele nunca entraria na
# pilha e `├─ rust/` leria-se `rust/` em vez de `crypto/rust`.
_SECCAO_BARE = re.compile(r"^([\w.-]+(?:/[\w.-]+)*/)\s*(?:#.*)?$")
# `docs/index.md` sozinho na linha, sem marcador. Não é a convenção do
# mapa, mas um leitor que a ignorasse em silêncio aceitaria que alguém
# acrescentasse um ficheiro ao mapa sem ele ser alguma vez verificado.
_FICHEIRO_BARE = re.compile(r"^~?([\w./+-]+\.[a-z]+)\s*$")
# `| K1 | crypto/rust/aead_chacha.rs | …` — as tabelas citam caminhos
# completos, e não precisam de pilha.
# O til e o caminho aparecem dentro de crases na secção histórica
# (`| `~messenger/plugins_loader.py` | … |`), e sem crases no mapa das
# camadas — por isso as crases são opcionais, e o til é um grupo
# próprio para que a afirmação continue a ser lida como ausência.
_TABELA = re.compile(r"\|\s*`?~?([\w./+-]+\.[a-z]+)`?\s*\|")


#: «20 documentos», «45 componentes», «Os 11 ecrãs».
#: As palavras compostas vêm **primeiro** na alternativa. Numa alternância
#: o Python devolve a primeira que casa, e `ficheiros` casa antes de
#: `ficheiros JavaScript versionados` — o que faria o número ser
#: contado por todos os ficheiros em vez dos de JavaScript, sem erro
#: nenhum. A ordem não é um detalhe de estilo: é o que decide o que a
#: frase quer dizer.
_CONTAGEM = re.compile(
    r"\b(?:Os\s+|As\s+|os\s+|as\s+)?(\d+)\s+"
    r"(ficheiros JavaScript versionados|sementes"
    r"|documentos|componentes|ecrãs|vistas|páginas|ficheiros|testes"
    r"|crates|alvos|corpora)\b"
)

#: O que conta, por substantivo. Um substantivo sem lista conta tudo o que
#: não é directório, o que é a leitura que uma pessoa faz e a que um
#: verificador tem de fazer igual.
#: Extensões que o projecto chama «ficheiros JavaScript». A lista é
#: escrita uma vez e usada pelas contagens, para que acrescentar
#: ``.mjs`` ao comando do interface não obrigue a mudar também a prosa.
EXTENSOES_JAVASCRIPT = (".js", ".jsx", ".mjs", ".cjs")


def indexados(directorio: Path) -> set[str]:
    """Os caminhos **versionados** sob um directório.

    ## Porque é que isto é uma função, e não `git ls-files | wc -l`

    Porque a diferença entre o disco e o índice é a lição inteira de
    B6. As sementes de fuzzing estavam todas no disco e **nenhuma** no
    índice: 173 ficheiros, 50 versionados. Um portão que conta o disco
    diz que o corpus tem 173 sementes, e quem lê acredita — porque o
    ficheiro está ali, à vista.

    E `git ls-files` inclui o que está indexado **e** o que foi apagado
    na árvore de trabalho. Para esta contagem isso é o certo: o que
    interessa é o que um `clone` recebe, e um ficheiro indexado que foi
    apagado localmente ainda lá vai para quem clonar. Sem o filtro
    `is_file()`, um `git rm` sem `--cached` contaria como versionado.
    """
    try:
        r = subprocess.run(
            ["git", "ls-files", "--", str(directorio)],
            capture_output=True, text=True, cwd=RAIZ, check=False,
        )
    except OSError:
        return set()
    if r.returncode != 0:
        return set()
    return {linha for linha in r.stdout.split("\n") if linha}


def _conta(directorio: Path | None, substantivo: str) -> int | None:
    """Quantos ficheiros há, contados à maneira de quem escreve.

    ``documentos`` conta ``.md`` no directório, que é o que uma pessoa
    quer dizer. ``componentes``, ``ecrãs``, ``vistas``, ``páginas`` e
    ``testes`` contam ficheiros a qualquer profundidade — que é o que se
    vê numa árvore com sub-directórios.

    ``sementes`` e ``ficheiros JavaScript versionados`` contam no
    **índice**, e não no disco, porque é isso que a palavra «versionado»
    quer dizer. Ver `indexados`.

    Devolve ``None`` quando o substantivo não é contável — ``crates``,
    ``alvos`` e ``corpora`` só um `cargo metadata` ou um `ls` de um
    directório crate os dá. Deixar por medir é honesto; medir mal não é.
    """
    if directorio is None or not directorio.is_dir():
        return None
    if substantivo == "documentos":
        return len(list(directorio.glob("*.md")))
    if substantivo in ("componentes", "ecrãs", "vistas", "páginas", "testes"):
        return len([p for p in directorio.rglob("*") if p.is_file()])
    if substantivo == "ficheiros":
        return len([p for p in directorio.rglob("*") if p.is_file()])
    if substantivo == "sementes":
        return len(indexados(directorio))
    if substantivo == "ficheiros JavaScript versionados":
        return len({
            c for c in indexados(directorio)
            if c.endswith(EXTENSOES_JAVASCRIPT)
        })
    return None


class Afirmacao:
    """Um caminho que o mapa afirma existir, ou afirmar que não existe."""

    __slots__ = ("caminho", "existe", "linha")

    def __init__(self, caminho: str, existe: bool, linha: int) -> None:
        self.caminho = caminho
        self.existe = existe
        self.linha = linha

    def __repr__(self) -> str:  # pragma: no cover — só para diagnóstico
        sinal = "" if self.existe else _TIL
        return f"<{sinal}{self.caminho}@{self.linha}>"


def ler(documento: str | None = None) -> list[str]:
    """As linhas do mapa, ou de outro documento com árvore.

    O mapa é `Estrutura.txt`. `UI/README.md` tem uma segunda árvore, e as
    contagens dela são verificadas do mesmo modo — foram três números
    errados numa só secção, e nenhum deles era verificável a olho.
    """
    caminho = MAPA if documento is None else RAIZ / documento
    if not caminho.exists():
        if documento is None:
            print(f"mapa não encontrado: {caminho}", file=sys.stderr)
            print("  este ficheiro passou a viver dentro do repositório, em",
                  file=sys.stderr)
            print("  `Estrutura.txt`, para ser versionado e conferido no CI.",
                  file=sys.stderr)
        else:
            print(f"documento não encontrado: {caminho}", file=sys.stderr)
        raise SystemExit(2)
    return caminho.read_text(encoding="utf-8").split("\n")


def afirmacoes() -> list[Afirmacao]:
    """Lê o mapa e devolve tudo o que ele afirma sobre ficheiros.

    Um **título** é uma linha começada por `#` cujo ponto seguinte é uma
    linha de traços. A distinção importa porque o mapa usa `#` também
    para comentários de prosa — `# o binário é o único processo que fala
    Tor` — e um leitor que tratasse qualquer `#` como título limparia o
    contexto no meio de uma secção e leria `AppShell.jsx` como se
    estivesse na raiz.
    """
    linhas = ler()
    unidade = unidade_de_indentacao(linhas)
    saida: list[Afirmacao] = []
    pilha: list[str] = []          # directórios, do mais externo ao mais interno
    contexto: str | None = None    # directório nomeado no título da secção

    def fechar(nome: str, profundidade: int) -> str:
        """Fecha um nome com o directório que lhe corresponde.

        Numa árvore, o pai de um nó de profundidade `d` é o nó de
        profundidade `d - 1` — e é por isso que a profundidade zero usa
        o `contexto` da secção, e não a pilha. Um nó que já traga
        barras no nome é um caminho **relativo** como outro qualquer —
        `components/onyx/` dentro de `UI/src/` dá
        `UI/src/components/onyx`, e `app/desktop/` dentro de `UI/` dá
        `UI/app/desktop`. Só as tabelas do mapa citam caminhos
        completos, e essas não passam por aqui.
        """
        if profundidade == 0:
            base = contexto
        else:
            base = pilha[profundidade - 1] if profundidade - 1 < len(pilha) else None
        return f"{base}/{nome}" if base else nome

    for n, linha in enumerate(linhas, 1):
        for m in _TABELA.finditer(linha):
            til = "~" in m.group(0)[:m.group(0).index(m.group(1))]
            saida.append(Afirmacao(m.group(1), not til, n))

        seguinte = linhas[n] if n < len(linhas) else ""
        sublinhado = set(seguinte) == {"-"} and len(seguinte) >= 3
        if linha.startswith("#") and (
            sublinhado or _NUMERO.match(linha) or _SECAO.match(linha)
        ):
            s = _SECAO.match(linha)
            contexto = s.group(1).rstrip("/") if s else None
            pilha = []
            continue

        if m := _MARCADOR.match(linha):
            profundidade = (len(m.group(1)) + len(m.group(2))) // unidade
        elif b := _SECCAO_BARE.match(linha):
            # Directório de secção, sem marcador: é o contexto dos
            # filhos seguintes, e a pilha passa a ser só ele.
            contexto = b.group(1).rstrip("/")
            if contexto == _RAIZ_DO_PROJECTO:
                # A primeira linha do mapa pode ser `onyxchat/`, o nome
                # do projecto — que não é um directório dentro do
                # repositório. Tratá-lo como um directório prefixava todos
                # os caminhos com `onyxchat/` e nenhum era encontrado.
                contexto = None
            pilha = []
            if contexto is not None and contexto.startswith(_TIL):
                saida.append(Afirmacao(contexto.lstrip(_TIL), False, n))
            continue
        else:
            # `docs/index.md` sozinho na linha, sem marcador: não é a
            # convenção do mapa, mas um leitor que o ignorasse em
            # silêncio aceitaria que alguém acrescentasse um ficheiro ao
            # mapa sem ele ser alguma vez verificado — que é a falha
            # que este programa existe para apanhar.
            f = _FICHEIRO_BARE.match(linha.split("#", 1)[0].strip())
            if f:
                saida.append(Afirmacao(f.group(1), not linha.startswith(_TIL), n))
            continue

        # O comentário à direita da entrada não é uma afirmação sobre
        # ficheiros — e citam-se lá nomes de ficheiro a propósito
        # (`docs/test_vectors.md` é gerado), que passariam a ser
        # verificados como se fossem entradas do mapa.
        corpo = m.group(3).split("#", 1)[0]

        # `main.jsx  App.jsx  index.css` — três ficheiros na mesma linha.
        for token in _NOME.findall(corpo):
            existe = not token.startswith(_TIL)
            nome = token.lstrip(_TIL).rstrip("/")

            if token.endswith("/"):
                caminho = fechar(nome, profundidade)
                if not existe:
                    saida.append(Afirmacao(caminho, False, n))
                del pilha[profundidade:]
                pilha.append(caminho)
                continue

            caminho = fechar(nome, profundidade)
            saida.append(Afirmacao(caminho, existe, n))

    return saida


def ficheiros_reais() -> set[str]:
    """Todos os ficheiros da árvore, relativos à raiz do repositório."""
    achados: set[str] = set()
    for p in RAIZ.rglob("*"):
        if not p.is_file():
            continue
        if set(p.relative_to(RAIZ).parts) & IGNORADOS:
            continue
        achados.add(str(p.relative_to(RAIZ)))
    return achados


#: Onde cada documento com árvore tem a raiz da sua árvore. `UI/README.md`
#: desenha a árvore a partir de `src/`, e não da raiz do repositório —
#: por isso cada documento declara a sua.
# Onde cada documento com árvore tem a sua raiz.
#
# Ambas as árvores começam pelo nome do projecto ou do directório de topo
# (`onyxchat/` na `Estrutura.txt`, `UI/` no `UI/README.md`), por isso os
# caminhos que saem da pilha **já trazem o prefixo** e a raiz é a mesma
# nos dois: a raiz do repositório.
#
# Declarar `UI/` como raiz aqui era o erro natural a cometer — e o
# resultado era `UI/UI/src/components/onyx`, que não existe, e uma
# contagem de zero **sem qualquer erro**, que é a forma mais cara de um
# verificador falhar.
RAIZES_DAS_ARVORES = {
    "Estrutura.txt": None,
    "UI/README.md": None,
}


def contagens(
    documento: str = "Estrutura.txt", raiz: Path | None = None
) -> list[tuple[str, int, int]]:
    """As contagens que o mapa afirma, com as que o disco dá.

    O mapa é um índice seleccionado, por isso **não** se exige que nomeie
    cada ficheiro. Mas um índice que nomeia directorios e diz «20
    documentos» está a afirmar uma aritmética, e uma aritmética errada
    é pior do que nenhuma: quem a usa para saber o tamanho de uma coisa
    recebe um número e não um dúvida.

    Devolve ``(descrição, afirmado, medido)``.
    """
    if raiz is None:
        raiz = RAIZ
    linhas = ler(documento)
    unidade = unidade_de_indentacao(linhas)
    achados: list[tuple[str, int, int]] = []
    pilha: list[str] = []          # directórios, do mais externo ao mais interno
    contexto: str | None = None    # directório nomeado no cabeçalho da secção

    def fechar(nome: str, profundidade: int) -> str:
        """Fecha um nome com os directórios que lhe ficam por baixo.

        A mesma regra de `afirmacoes()`: o pai de um nó de profundidade
        `d` é o de profundidade `d - 1`, e um nome com barras é relativo
        — `components/onyx/` dentro de `src/` dá
        `src/components/onyx`.
        """
        if "/" in nome:
            return nome if profundidade == 0 else f"{_pai(profundidade)}/{nome}"
        if profundidade == 0:
            return nome if contexto is None else f"{contexto}/{nome}"
        return f"{_pai(profundidade)}/{nome}"

    def _pai(profundidade: int) -> str:
        indice = profundidade - 1
        return pilha[indice] if 0 <= indice < len(pilha) else ""

    def registar(caminho: str, comentario: str, linha: int) -> None:
        cont = _CONTAGEM.search(comentario)
        if not cont:
            return
        medido = _conta(raiz / caminho, cont.group(2))
        if medido is not None:
            achados.append(
                (f"{caminho}/ — «{cont.group(2)}» (linha {linha})",
                 int(cont.group(1)), medido)
            )

    for n, linha in enumerate(linhas, 1):
        # Um cabeçalho de secção define o contexto — se o trouxer.
        s = _SECAO.match(linha)
        seguinte = linhas[n] if n < len(linhas) else ""
        e_titulo = linha.startswith("#") and (
            s or _NUMERO.match(linha) or (set(seguinte) == {"-"} and len(seguinte) >= 3)
        )
        if e_titulo:
            contexto = s.group(1).rstrip("/") if s else None
            if contexto == _RAIZ_DO_PROJECTO:
                contexto = None
            pilha = []
            if contexto:
                registar(contexto, linha, n)
            continue

        m = _MARCADOR.match(linha)
        if m is None:
            b = _SECCAO_BARE.match(linha)
            if b:
                contexto = b.group(1).rstrip("/")
                if contexto == _RAIZ_DO_PROJECTO:
                    contexto = None
                pilha = []
                if contexto:
                    registar(contexto, linha, n)
            continue

        profundidade = (len(m.group(1)) + len(m.group(2))) // unidade
        corpo = m.group(3).split("#", 1)
        nome = corpo[0].strip().rstrip("/")
        if not nome:
            continue
        caminho = fechar(nome, profundidade)
        if len(corpo) > 1:
            registar(caminho, corpo[1], n)
        if m.group(3).split("#", 1)[0].strip().endswith("/"):
            del pilha[profundidade:]
            pilha.append(caminho)
    return achados


def main() -> int:
    global _PAI

    af = afirmacoes()
    if not af:
        print("nenhuma afirmação lida do mapa — o formato mudou?",
              file=sys.stderr)
        return 2

    reais = ficheiros_reais()
    erros: list[str] = []

    for a in af:
        existe = a.caminho in reais
        if a.existe and not existe:
            erros.append(f"em falta   {a.caminho}  ({MAPA.name}:{a.linha})")
        elif not a.existe and existe:
            erros.append(
                f"afirma que falta, mas existe  {a.caminho}  "
                f"({MAPA.name}:{a.linha})"
            )

    afirmadas = sum(1 for a in af if a.existe)
    print(f"afirmações no mapa: {len(af)}  ({afirmadas} de existência, "
          f"{len(af) - afirmadas} de ausência)")
    print(f"ficheiros na árvore: {len(reais)}")

    contagens_divergentes = []
    for documento, raiz in RAIZES_DAS_ARVORES.items():
        for descricao, afirmado, medido in contagens(documento, raiz):
            onde = f"{documento} {descricao}"
            if afirmado != medido:
                contagens_divergentes.append(
                    f"contagem errada  {onde}: o documento diz {afirmado}, "
                    f"o disco tem {medido}"
                )
            else:
                print(f"  contagem  {onde}: {medido}")

    erros.extend(contagens_divergentes)
    if erros:
        print(f"\n{len(erros)} divergência(s):")
        for e in sorted(set(erros)):
            print(f"  {e}")
        return 1

    print("\no mapa corresponde à árvore")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())