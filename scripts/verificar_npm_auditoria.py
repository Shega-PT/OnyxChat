# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Compara o relatório do `npm audit` com as excepções declaradas.

## Porque é que isto existe

`npm audit` não tem ficheiro de excepções. As duas saídas que a
ferramenta oferece são as duas que não servem:

* **`--omit=dev`** esconde advisories das ferramentas de construção — e
  um `vite` com advisory escreve o código que o projecto distribui.
  Esconder o advisory da ferramenta que escreve o bundle é esconder a
  vulnerabilidade.
* **`--audit-level=critical`** afina o barulho trocando por cobertura, e
  o `braces` com CVSS 7.5 passava — e continua a ser um DoS.

A terceira saída — a que este programa implementa — é comparar o
relatório com `seguranca-excepcoes.toml`: falha no que não estiver
aceite, e **imprime** o que estiver, com a razão e a data de revisão.

O imprimir não é um detalhe. Uma excepção que não aparece em cada
execução é uma excepção que ninguém reevalua, e a data de revisão é o
que a torna uma decisão e não um esquecimento.

## As razões são verificáveis, e isso é o que se exige

Cada excepção aponta para um ficheiro do projecto que prova que o
caminho vulnerável não é alcançável. «Não é explorável» não é uma
razão. «A função não é importada em lado nenhum, e a aplicação não faz
SSR» é — e pode-se confirmar com `grep`.

Um advisory que chegue sem excepção declarada é **erro**, e não aviso:
o objectivo de um portão que falha é lembrar que alguém tem de olhar.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
UI = RAIZ / "UI"
EXCECCOES = RAIZ / "seguranca-excepcoes.toml"

#: Nível mínimo que conta.
#:
#: É `moderate`, e não `high`, por uma razão concreta: a superfície de
#: dependências de **execução** da interface é de 14 pacotes, e dois dos
#: advisories que o registry reporta para eles são `moderate`. Um limiar
#: que os deixa de fora é um limiar que esconde os dois únicos furos
#: desta lista que chegam ao browser — o resto da cadeia é só de
#: construção.
#:
#: `low` fica de fora porque não há nenhum, e porque corrigir um advisory
#: `low` todos os dias é a forma mais rápida de desligar um portão.
NIVEL_MINIMO = "moderate"

#: Ordem de gravidade do registry, para ordenar o relatório. A ordem é
#: do registry e não nossa: reordenar por severidade é decidir que o
#: nosso juízo vale mais do que o de quem publica o advisory.
SEVERIDADES = ("critical", "high", "moderate", "low", "info")


def relatorio() -> dict:
    """Corre `npm audit --json` em `UI/` e devolve o relatório."""
    if not (UI / "package-lock.json").exists():
        print("UI/package-lock.json não encontrado — nada que auditar",
              file=sys.stderr)
        raise SystemExit(2)

    r = subprocess.run(
        ["npm", "audit", "--json"],
        cwd=UI, capture_output=True, text=True, check=False,
    )
    # O `npm audit` sai != 0 **quando encontra advisories**. Um código
    # diferente de zero com relatório legível não é erro de execução, e
    # tratar como se fosse era a forma de nunca mostrar um relatório
    # quando há algo para mostrar.
    if not r.stdout.strip():
        print("npm audit não devolveu relatório", file=sys.stderr)
        print(r.stderr.strip()[:400], file=sys.stderr)
        raise SystemExit(2)
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError as e:
        print(f"relatório do npm audit ilegível: {e}", file=sys.stderr)
        raise SystemExit(2) from e


def aceites() -> dict[str, dict]:
    """As excepções declaradas, por advisory."""
    if not EXCECCOES.exists():
        print(f"excepções não encontradas: {EXCECCOES}", file=sys.stderr)
        raise SystemExit(2)
    dados = tomllib.loads(EXCECCOES.read_text(encoding="utf-8"))
    return {e["advisory"]: e for e in dados.get("npm", {}).get("aceites", [])}


def advisories(d: dict) -> dict[str, dict]:
    """Os advisories directos, indexados pelo seu identificador.

    O registry lista cada pacote afectado e, em `via`, os advisories que
    o alcançam — os transitivos vêm como pacote com `via` feito de
    outros pacotes. Só os que são **dicionários com `url`** são
    advisories; o resto são rotas de dependência, e contá-los como
    vulnerabilidades distintas seria dizer que há dez furos onde há um.
    """
    achados: dict[str, dict] = {}
    for pacote, v in (d.get("vulnerabilities") or {}).items():
        for via in v.get("via", []):
            if isinstance(via, dict) and via.get("url"):
                achados.setdefault(via["url"].rstrip("/").split("/")[-1],
                                   dict(via, pacote=pacote))
    return achados


def acima_do_minimo(achados: dict[str, dict]) -> dict[str, dict]:
    """Só os que estão no nível mínimo ou acima."""
    corte = SEVERIDADES.index(NIVEL_MINIMO)
    return {
        id: a for id, a in achados.items()
        if SEVERIDADES.index(a.get("severity", "info")) <= corte
    }


def main() -> int:
    d = relatorio()
    total = (d.get("metadata") or {}).get("vulnerabilities", {})
    print(f"npm audit: {total.get('total', 0)} avisos no grafo "
          f"({total.get('critical', 0)} críticos, {total.get('high', 0)} altos)")

    achados = acima_do_minimo(advisories(d))
    permitidas = aceites()

    # ## Porque é que esta verificação vem **antes** do return
    #
    # Porque uma excepção fica órfã quando o advisory deixa de existir —
    # e isso acontece quando a dependência é actualizada, que é
    # precisamente o caso em que `achados` está vazio. Com a verificação
    # depois do `return`, a lista nunca emcendia do que nela sobrou.
    # Testado em `test_uma_excepcao_que_nao_se_ja_aplica_avisa`.
    orfas = [e for e in permitidas.values() if e["advisory"] not in achados]
    if orfas:
        print("\nexcepções que já não correspondem a nenhum aviso: "
              + ", ".join(e["advisory"] for e in orfas), file=sys.stderr)
        print("  removê-las mantém a lista honesta; o portão não falha por isto.",
              file=sys.stderr)

    if not achados:
        print(f"nenhum advisory de nível {NIVEL_MINIMO} ou acima")
        return 0

    novos = []
    print(f"\nadvisories de nível {NIVEL_MINIMO} ou acima:")
    for id in sorted(achados, key=lambda i: SEVERIDADES.index(achados[i].get("severity", "info"))):
        a = achados[id]
        excepcao = permitidas.get(id)
        if excepcao is None:
            novos.append((id, a))
            print(f"  ✗ {id}  {a.get('pacote')}  ({a.get('severity')})")
            print(f"      {a.get('title')}")
        else:
            print(f"  ✓ {id}  {a.get('pacote')}  — aceite, revisão {excepcao['revisao']}")
            print(f"      razão: {excepcao['razao']}")

    if novos:
        print(f"\n{len(novos)} advisory(s) sem excepção declarada.", file=sys.stderr)
        print("Aceitar é uma decisão: acrescenta a `seguranca-excepcoes.toml` "
              "com uma razão verificável no código e uma data de revisão — "
              "ou corrige a dependência.", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())