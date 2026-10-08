# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Confere que os rótulos do roadmap estão certos.

## Porque é que isto existe

O `docs/roadmap.md` distingue três estados, e a definição de `PLANEADO`
é — e é citável — «decidido como próximo passo, **especificado**, ainda
sem código». Uma vez written, a tabela do `PLANEADO` tinha uma linha com
a coluna «Especificado em» a `—`, que é exactamente a negação da
definição.

Não apanhou ninguém porque **um rótulo errado não dá sintaxe inválida**.
O Markdown continua bem formado, o ficheiro continua legível, e o
leitor vê uma linha numa tabela que parece exactamente igual às outras.
É a divergência mais silenciosa que um repositório tem: não está no
código, não está na sintaxe, está numa palavra.

E a linha estava errada desde que o roadmap foi escrito — o que prova
que «temos uma regra editorial» não é o mesmo que «a regra é aplicada».

## A regra, tal e qual

A tabela do `PLANEADO` tem uma coluna «Especificado em». Aponta para um
documento normativo? Então é `PLANEADO`. Está vazia? Então é
`CONCEITO`, porque **nada foi especificado** — e o que falta não é
código, é a especificação.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
ROADMAP = RAIZ / "docs" / "roadmap.md"

#: Valores da coluna que significam «não especificado».
VAZIO = {"—", "-", "", "n/a", "N/A", "nenhum", "nenhuma"}


def secoes(texto: str) -> dict[str, list[str]]:
    """As linhas de cada secção `## ...`, por título."""
    saida: dict[str, list[str]] = {}
    titulo = None
    for linha in texto.split("\n"):
        if linha.startswith("## "):
            titulo = linha[3:].strip()
            saida.setdefault(titulo, [])
        elif titulo is not None:
            saida[titulo].append(linha)
    return saida


def linhas_da_tabela(linhas: list[str]) -> list[list[str]]:
    """As células de cada linha de tabela, sem cabeçalho nem separador."""
    saida = []
    for linha in linhas:
        if not linha.startswith("| ") or set(linha) <= set("| -"):
            continue
        celulas = [c.strip() for c in linha.split("|")[1:-1]]
        if celulas and celulas[0] in ("Item", "---"):
            continue
        saida.append(celulas)
    return saida


def main() -> int:
    if not ROADMAP.exists():
        print(f"roadmap não encontrado: {ROADMAP}", file=sys.stderr)
        return 2

    sec = secoes(ROADMAP.read_text(encoding="utf-8"))
    planeado = linhas_da_tabela(sec.get("PLANEADO", []))
    conceito = linhas_da_tabela(sec.get("CONCEITO", []))

    if not planeado or not conceito:
        print("uma das tabelas não tem linhas — o formato mudou?", file=sys.stderr)
        return 2

    print(f"PLANEADO: {len(planeado)} item(ns)")
    print(f"CONCEITO: {len(conceito)} item(ns)")

    # A regra de uma linha, aplicada.
    erros = []
    for celulas in planeado:
        if len(celulas) < 2:
            continue
        item, especificado = celulas[0], celulas[1]
        if especificado in VAZIO:
            erros.append(
                f"PLANEADO sem especificação: {item} — "
                "com a coluna «Especificado em» vazia, a definição "
                "diz CONCEITO (§Como se decide)"
            )

    # E o simétrico: um CONCEITO que aponte para uma secção normativa
    # está especificado, e portanto é PLANEADO. Não se pode ter um item
    # nos dois sítios.
    normalizados = {re.sub(r"[^a-z0-9]", "", c[0].lower()) for c in conceito}
    for celulas in planeado:
        chave = re.sub(r"[^a-z0-9]", "", celulas[0].lower())
        if chave in normalizados:
            erros.append(f"o mesmo item está nas duas tabelas: {celulas[0]}")

    if erros:
        print(f"\n{len(erros)} erro(s):")
        for e in erros:
            print(f"  {e}")
        return 1

    print("\nos rótulos estão de acordo com a definição")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
