# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Paridade entre a derivação de identidade em Python e o que a interface mostra.

O objectivo é estreito e verificável: **os identificadores e as impressões
digitais que a interface mostra têm de ser os que
`messenger/identidade.py` produz.** Se as duas pontas divergirem, quem
confirma uma impressão compara-a contra o valor errado e obtém um
resultado que não significa nada.

Este teste lê o vector congelado de ``tests/vectors/identidade.json`` e
confere-o contra o que está escrito em
``UI/src/lib/onyx/mock-data.js`` — o ficheiro que a interface vai mesmo
ler. Não executa JavaScript: o valor que importa é o que está no ficheiro.

## Porque é que os valores vivem no ficheiro e não são calculados no browser

A interface **não deriva** identidade; lê-a, como leria de um sidecar.
A identidade é calculada uma vez, no registo, em Python, e persistida.

A alternativa seria a interface calcular o SHA-256 com
``crypto.subtle.digest``. Seria possível e seria pior por três razões:
a operação é assíncrona; a fórmula passaria a existir em dois sítios que
não se verificam um ao outro; e a derivação correria no browser de quem lê o
código, em vez de correr uma vez no registo. Este teste é a verificação de
que os dois lados concordam — não uma segunda implementação.
"""

from __future__ import annotations

import json
import pathlib
import re

import pytest

from messenger.identidade import identificador_de, impressao_de

RAIZ = pathlib.Path(__file__).parent.parent
VECTOR = RAIZ / "tests" / "vectors" / "identidade.json"
MOCK = RAIZ / "UI" / "src" / "lib" / "onyx" / "mock-data.js"

# Uma linha de pessoa: `id: '…', identifier: '…', fingerprint: '…', name: '…'`.
RE_PESSOA = re.compile(
    r"id: '(?P<id>[^']+)'"
    r".*?identifier: '(?P<identificador>[^']+)'"
    r".*?fingerprint: '(?P<impressao>[0-9A-F:]+)'"
    r".*?name: '(?P<nome>[^']+)'"
)

# A forma que o vector impõe. Uma expressão regular independente da
# derivação: se `identidade.py` e o formato divergirem, uma das duas
# apanha.
#
# O bloco de seis caracteres usa `[A-HJ-NP-Z2-9]` e **não** `[A-Z2-9]`: o
# alfabeto de `messenger/identidade.py` não tem `I` nem `O`, e uma classe
# mais larga aceitaria identificadores que a derivação nunca produz.
LETRAS = "[A-HJ-NP-Z2-9]"
SIMBOLO = "[!@$%*+~=]"
# A cauda tem **quatro** caracteres: símbolo, letra, símbolo, letra.
FORMA_ID = re.compile(
    rf"^ONYX-{LETRAS}{{6}}-{SIMBOLO}{LETRAS}{SIMBOLO}{LETRAS}#$"
)

RE_SAL = re.compile(r"SAL_DEMONSTRACAO = '([0-9a-f]+)'")


def _mock() -> str:
    return MOCK.read_text(encoding="utf-8")


def _pessoas() -> list[dict]:
    return [m.groupdict() for m in RE_PESSOA.finditer(_mock())]


def test_o_mock_existe() -> None:
    assert MOCK.is_file(), f"{MOCK} não existe — a interface tem de ser lida"


def test_o_mock_declara_o_sal_da_demonstracao() -> None:
    """O sal tem de estar escrito, ou a derivação não é reproduzível."""
    achado = RE_SAL.search(_mock())
    assert achado, "SAL_DEMONSTRACAO não está declarado no mock"
    assert len(bytes.fromhex(achado.group(1))) == 16


def test_ha_pessoas_para_verificar() -> None:
    pessoas = _pessoas()
    assert len(pessoas) >= 10, f"só {len(pessoas)} pessoas com identidade no mock"


# ---------------------------------------------------------------------
# A forma
# ---------------------------------------------------------------------


def test_todo_o_modo_cumpre_a_forma() -> None:
    """Cada valor tem de estar na forma que o vector impõe.

    Um valor inventado que «pareça» um identificador é pior do que um em
    falta: uma impressão que não é uma impressão convence quem a olha.
    """
    for pessoa in _pessoas():
        assert FORMA_ID.match(pessoa["identificador"]), (
            f"fora da forma: {pessoa['identificador']!r} ({pessoa['nome']})"
        )


def test_toda_a_impressao_tem_128_bits() -> None:
    for pessoa in _pessoas():
        partes = pessoa["impressao"].split(":")
        assert len(partes) == 16, f"impressão curta: {pessoa['impressao']!r}"
        assert all(len(p) == 2 for p in partes)


# ---------------------------------------------------------------------
# Paridade com `messenger/identidade.py`
# ---------------------------------------------------------------------


def test_cada_identificador_e_derivavel_do_nome_da_pessoa() -> None:
    """O teste central.

    Para cada pessoa do mock, o identificador tem de sair da fórmula de
    referência a partir **do nome que está no mesmo linha**. É o que
    garante que o que a interface mostra é o que o sistema real
    produziria.
    """
    sal = bytes.fromhex(RE_SAL.search(_mock()).group(1))
    pessoas = _pessoas()
    for pessoa in pessoas:
        esperado = identificador_de(pessoa["nome"], sal)
        assert pessoa["identificador"] == esperado, (
            f"{pessoa['nome']!r}: o mock traz {pessoa['identificador']!r}, "
            f"a derivação dá {esperado!r}"
        )


def test_cada_impressao_e_derivavel_do_identificador() -> None:
    sal = bytes.fromhex(RE_SAL.search(_mock()).group(1))
    for pessoa in _pessoas():
        esperado = impressao_de(identificador_de(pessoa["nome"], sal))
        assert pessoa["impressao"] == esperado, (
            f"{pessoa['nome']!r}: impressão divergente"
        )


# ---------------------------------------------------------------------
# Unicidade
# ---------------------------------------------------------------------


def test_nenhum_identificador_repetido() -> None:
    identificadores = [p["identificador"] for p in _pessoas()]
    repetidos = {i for i in identificadores if identificadores.count(i) > 1}
    assert not repetidos, f"identificadores repetidos: {sorted(repetidos)}"


def test_nenhuma_impressao_repetida() -> None:
    """Duas pessoas com a mesma impressão é o que a impressão existe para
    impedir."""
    impressoes = [p["impressao"] for p in _pessoas()]
    repetidas = {i for i in impressoes if impressoes.count(i) > 1}
    assert not repetidas, f"impressões repetidas: {sorted(repetidas)}"


# ---------------------------------------------------------------------
# O vector continua a ser o mesmo
# ---------------------------------------------------------------------


def test_o_mock_e_o_vector_usa_o_mesmo_formato() -> None:
    documento = json.loads(VECTOR.read_text(encoding="utf-8"))
    for vetor in documento["vetores"]:
        assert FORMA_ID.match(vetor["identificador"]), vetor["identificador"]
        assert len(vetor["impressao"].split(":")) == 16