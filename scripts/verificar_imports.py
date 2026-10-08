# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Confere que cada import nomeado existe mesmo no módulo importado.

## Porque isto existe a mais

O `tsc` confirma os **tipos**. Um import de um símbolo que o módulo não
exporta dá erro de tipo — mas `shadcn.js` envolve os primitivos em
`semTipos()`, que os degrada para `any`, e aí um símbolo com o nome errado
passa a verificação e só falha quando alguém carrega no botão.

O mesmo vale para `export default`: um componente importado sem o nome
correcto dá `undefined` em tempo de execução, e `undefined` como elemento
React dá um erro que não diz onde o ficheiro foi importado.

Um build Vite não apanha nenhum dos dois casos, porque resolve o módulo
(correcto) e não inspeciona o que foi importado dele.

## O que é verificado

Para cada `import` relativo ou com o alias `@/`:

- o módulo resolve;
- se foi importado `default`, existe um `export default`;
- cada símbolo de `import { … }` é exportado pelo módulo.

## O que NÃO é verificado

Se o símbolo exportado tem a forma que o consumidor espera. Isso é o
trabalho do `tsc`, e esta verificação não finge fazer mais do que faz.

Uso: `.venv/bin/python scripts/verificar_imports.py`
Sai com 1 se encontrar algum problema.
"""

from __future__ import annotations

import pathlib
import re
import sys

RAIZ = pathlib.Path(__file__).resolve().parent.parent
FONTE = RAIZ / "UI" / "src"

#: `import a, { b, c } from '…'` e `import a from '…'`.
RE_IMPORT = re.compile(
    r"^import\s+(?:(?P<por_omissao>\w+)\s*,\s*)?"
    r"(?:\{(?P<nomeados>[^}]*)\}\s*)?"
    r"(?P<unico>\w+)?\s*"
    r"from\s+'(?P<modulo>[^']+)'",
    re.M,
)

#: `export function a`, `export const a`, `export class a`, `export let a`.
RE_EXPORTADO = re.compile(
    r"^export\s+(?:default\s+)?(?:async\s+)?"
    r"(?:function|const|class|let)\s+(\w+)",
    re.M,
)

#: `export { a, b as c }` — o bloco pode ter várias linhas.
RE_EXPORT_BLOCO = re.compile(r"export\s*\{([^}]*)\}", re.S)

#: `export * from './x'` — não se pode resolver sem carrega o módulo.
RE_EXPORT_TUDO = re.compile(r"^export\s+\*", re.M)

#: As extensões que o Vite tenta, por ordem.
EXTENSOES = (".js", ".jsx", ".json")


def resolver(origem: pathlib.Path, modulo: str) -> pathlib.Path | None:
    """Resolve um especificador para um caminho de ficheiro existente.

    Mesma ordem que o Vite: caminho tal como está, depois com cada
    extensão, depois como directório com `index`.
    """
    if modulo.startswith("@/"):
        base = FONTE / modulo[2:]
    elif modulo.startswith("."):
        base = (origem.parent / modulo).resolve()
    else:
        # Pacote externo: o `tsc` trata, e não temos o que ver aqui.
        return None

    for candidato in (
        base,
        *(pathlib.Path(str(base) + ext) for ext in EXTENSOES),
        *(base / f"index{ext}" for ext in EXTENSOES),
    ):
        if candidato.is_file():
            return candidato
    return None


def simbolos_exportados(corpo: str) -> tuple[set[str], bool]:
    """Devolve os nomes exportados, e se há `export *` por resolver."""
    nomes = set(RE_EXPORTADO.findall(corpo))

    for bloco in RE_EXPORT_BLOCO.findall(corpo):
        for parte in bloco.split(","):
            parte = parte.strip()
            if not parte:
                continue
            # `b as c` exporta `c`.
            nomes.add(parte.split(" as ")[-1].strip())

    tem_tudo = bool(RE_EXPORT_TUDO.search(corpo))
    return nomes, tem_tudo


def main() -> int:
    ficheiros = sorted([*FONTE.rglob("*.js"), *FONTE.rglob("*.jsx")])
    problemas: list[str] = []
    modulos_com_tudo = 0

    for ficheiro in ficheiros:
        corpo = ficheiro.read_text(encoding="utf-8")

        for achado in RE_IMPORT.finditer(corpo):
            por_omissao = achado.group("por_omissao")
            nomeados = achado.group("nomeados")
            modulo = achado.group("modulo")

            destino = resolver(ficheiro, modulo)
            if destino is None:
                # Só é um problema se o módulo for nosso.
                if modulo.startswith((".", "@/")):
                    problemas.append(
                        f"{ficheiro.relative_to(RAIZ)}: não resolve {modulo}"
                    )
                continue

            destino_corpo = destino.read_text(encoding="utf-8")
            nomes, tem_tudo = simbolos_exportados(destino_corpo)
            if tem_tudo:
                modulos_com_tudo += 1

            linha = corpo[: achado.start()].count("\n") + 1

            if por_omissao and "export default" not in destino_corpo:
                problemas.append(
                    f"{ficheiro.relative_to(RAIZ)}:{linha}: {modulo} não tem `export default` "
                    f"(importado como `{por_omissao}`)"
                )

            for simbolo in (n.strip() for n in (nomeados or "").split(",")):
                simbolo = simbolo.split(" as ")[0].strip()
                if not simbolo:
                    continue
                # Com `export *` não se sabe o que o módulo reexporta
                # sem o carregar; assume-se o pior e não se acusa.
                if tem_tudo or simbolo in nomes or simbolo.startswith("type "):
                    continue
                problemas.append(
                    f"{ficheiro.relative_to(RAIZ)}:{linha}: {modulo} não exporta `{simbolo}`"
                )

    for problema in problemas:
        print(f"  {problema}")

    print(
        f"\n  {len(ficheiros)} ficheiros verificados, {len(problemas)} problema(s)"
        f"{f', {modulos_com_tudo} módulo(s) com `export *` não verificados' if modulos_com_tudo else ''}"
    )
    return 1 if problemas else 0


if __name__ == "__main__":
    sys.exit(main())
