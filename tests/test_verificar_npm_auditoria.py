# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes da comparação entre o relatório do `npm audit` e as excepções.

## A pergunta que estes testes respondem

`scripts/verificar_npm_auditoria.py` é o que impede que uma auditoria
de JavaScript seja desligada na primeira semana. Em 2026-10-07, à
primeira execução, ela encontrou **quatro** advisories reais num
projecto com 32 pacotes. Sem onde os registar, a resposta seria
`--omit=dev` — e o `vite`, que escreve o código distribuído, ficaria
fora da vista.

Estes testes não correm o `npm` — isso exige `node_modules` e rede. Testam
a **lógica**, com relatórios que reproduzem a forma real da saída do
registry, que é onde os erros acontecem: o `via` de um pacote transitivo
é outro pacote, e não um advisory, e contá-los como furos distintos
diria que há dez furos onde há um.
"""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

RAIZ = Path(__file__).resolve().parent.parent


def _modulo():
    spec = importlib.util.spec_from_file_location(
        "verificar_npm_auditoria", RAIZ / "scripts" / "verificar_npm_auditoria.py"
    )
    assert spec and spec.loader
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


vn = _modulo()


#: A forma real: `via` é uma lista misturada de advisories (dicionários
#: com `url`) e de pacotes por onde se chega lá (strings). `braces` é
#: alcançado por `chokidar`, que é alcançado por `tailwindcss` — e só o
#: primeiro é um advisory.
RELATORIO = json.dumps({
    "metadata": {"vulnerabilities": {
        "info": 0, "low": 0, "moderate": 2, "high": 2, "critical": 0, "total": 4,
    }},
    "vulnerabilities": {
        "braces": {
            "severity": "high", "isDirect": False, "fixAvailable": False,
            "via": [{
                "url": "https://github.com/advisories/GHSA-vfj7-8cjw-p6xm",
                "severity": "high", "title": "braces vulnerable to stack exhaustion",
            }],
        },
        "chokidar": {
            "severity": "high", "isDirect": False, "fixAvailable": False,
            "via": ["braces"],          # rota de dependência, não advisory
        },
        "react-router": {
            "severity": "moderate", "isDirect": False,
            "fixAvailable": {"name": "react-router-dom", "version": "7.18.4"},
            "via": [
                {"url": "https://github.com/advisories/GHSA-wrjc-x8rr-h8h6",
                 "severity": "moderate", "title": "Open redirect via backslash"},
                {"url": "https://github.com/advisories/GHSA-337j-9hxr-rhxg",
                 "severity": "moderate", "title": "Constructor injection in SSR"},
            ],
        },
        "react-router-dom": {
            "severity": "moderate", "isDirect": True,
            "via": ["react-router"],    # rota, não advisory
        },
    },
})


@pytest.fixture
def com_aceites(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    """Um `seguranca-excepcoes.toml` escrito à mão, num directório só seu."""

    def _escrever(aceites: list[dict]) -> Path:
        destino = tmp_path / "seguranca-excepcoes.toml"
        corpo = "\n".join(
            "[[npm.aceites]]\n"
            f'advisory = "{e["advisory"]}"\n'
            f'pacote   = "{e.get("pacote", "?")}"\n'
            f'revisao  = "{e.get("revisao", "2027-01-01")}"\n'
            f'razao    = "{e.get("razao", "razão de teste")}"\n'
            for e in aceites
        )
        destino.write_text(corpo, encoding="utf-8")
        monkeypatch.setattr(vn, "EXCECCOES", destino)
        return destino

    return _escrever


# ---------------------------------------------------------------------
# A extracção
# ---------------------------------------------------------------------

def test_as_rotas_de_dependencia_nao_sao_advisories() -> None:
    """`via` mistura advisories com pacotes. Só os primeiros contam.

    O registry diz «chokidar é vulnerável» e, no `via`, escreve
    `"braces"` — o nome de outro pacote. Tratar isso como um segundo
    advisory daria sete furos onde há um, e uma lista de excepções com
    entradas que não existem.
    """
    achados = vn.advisories(json.loads(RELATORIO))
    assert set(achados) == {
        "GHSA-vfj7-8cjw-p6xm",
        "GHSA-wrjc-x8rr-h8h6",
        "GHSA-337j-9hxr-rhxg",
    }


def test_o_identificador_e_o_do_advisory_nao_o_do_pacote() -> None:
    """A chave é o `GHSA-…`, e o pacote é metadata.

    Comparar pelo nome do pacote seria comparar por o síntoma: quando o
    `react-router` gain dois advisories, o segundo esconderia o primeiro.
    """
    achados = vn.advisories(json.loads(RELATORIO))
    assert achados["GHSA-wrjc-x8rr-h8h6"]["pacote"] == "react-router"


def test_o_limiar_moderado_inclui_a_superficie_de_execucao() -> None:
    """Os dois advisories do `react-router` são `moderate`, e contam.

    Um limiar `high` deixaria de fora os dois únicos furos desta lista que
    chegam ao browser — o resto da cadeia é só de construção.
    """
    todos = vn.advisories(json.loads(RELATORIO))
    assert len(todos) == 3
    assert len(vn.acima_do_minimo(todos)) == 3


def test_um_nivel_abaixo_do_limiar_fica_de_fora() -> None:
    """`low` não conta — e hoje não há nenhum.

    Corrigir um advisory `low` todos os dias é a forma mais rápida de
    desligar um portão. O teste fixa o limite para que subir de nível
    seja uma decisão e não um efeito colateral.
    """
    d = {"vulnerabilities": {"x": {"severity": "low", "via": [{
        "url": "https://github.com/advisories/GHSA-baixo", "severity": "low",
    }]}}}
    assert vn.acima_do_minimo(vn.advisories(d)) == {}


# ---------------------------------------------------------------------
# As duas direcções
# ---------------------------------------------------------------------

def test_um_advisory_novo_falha(com_aceites, monkeypatch) -> None:
    """Um advisory sem excepção declarada é **erro**, não aviso.

    É o comportamento que mantém a lista honesta: aceitar é uma decisão
    escrita, com razão e data, e não o que acontece quando alguém não
    teve tempo.
    """
    com_aceites([{"advisory": "GHSA-vfj7-8cjw-p6xm"}])  # só o braces
    monkeypatch.setattr(vn, "relatorio", lambda: json.loads(RELATORIO))
    assert vn.main() == 1


def test_todos_aceites_passam(com_aceites, monkeypatch) -> None:
    com_aceites([
        {"advisory": "GHSA-vfj7-8cjw-p6xm", "revisao": "2027-04-07"},
        {"advisory": "GHSA-wrjc-x8rr-h8h6", "revisao": "2027-01-07"},
        {"advisory": "GHSA-337j-9hxr-rhxg", "revisao": "2027-01-07"},
    ])
    monkeypatch.setattr(vn, "relatorio", lambda: json.loads(RELATORIO))
    assert vn.main() == 0


def test_um_relatorio_sem_advisories_passa(com_aceites, monkeypatch) -> None:
    """O caso em que não há nada a reportar é o que tem de passar."""
    com_aceites([])
    monkeypatch.setattr(vn, "relatorio",
                        lambda: {"metadata": {"vulnerabilities": {"total": 0}},
                                 "vulnerabilities": {}})
    assert vn.main() == 0


def test_uma_excepcao_que_nao_se_ja_aplica_avisa(com_aceites, monkeypatch,
                                                 capsys) -> None:
    """Uma excepção órfã avisa e não falha.

    Pode acontecer porque a dependência foi actualizada e o advisory
    deixou de existir — e nesse caso a excepção ficou obsoleta. Falhar
    por isso seria transformar a limpeza da lista em trabalho.
    """
    com_aceites([{"advisory": "GHSA-inexistente-0000"}])
    monkeypatch.setattr(vn, "relatorio", lambda: {"vulnerabilities": {}})
    assert vn.main() == 0
    assert "já não correspondem" in capsys.readouterr().err


# ---------------------------------------------------------------------
# O ficheiro de excepções do projecto
# ---------------------------------------------------------------------

def test_as_excepcoes_reais_tem_razão_e_data() -> None:
    """Cada excepção versionada tem os dois campos obrigatórios.

    Uma excepção sem razão é opinião; uma sem data é um ficheiro de
    excepções que ninguém reevalua.
    """
    aceites = vn.aceites()
    assert aceites, "o projecto tem advisories registados"
    for id, e in aceites.items():
        assert id.startswith("GHSA-"), id
        assert len(e.get("razao", "")) > 20, f"{id}: razão curta demais"
        assert len(e.get("revisao", "")) == 10, f"{id}: sem data ISO"
        assert e.get("pacote"), f"{id}: sem pacote"


def test_as_excepcoes_reais_sao_sobre_este_projecto() -> None:
    """As quatro excepções são as que a auditoria encontrou a 2026-10-07.

    Lista feita à mão a partir da memória é lista que ninguém actualiza;
    esta é conferida contra o ficheiro, e o ficheiro é conferido contra o
    que o registry devolve.
    """
    assert set(vn.aceites()) == {
        "GHSA-vfj7-8cjw-p6xm",
        "GHSA-rj75-hqrm-r3gf",
        "GHSA-wrjc-x8rr-h8h6",
        "GHSA-337j-9hxr-rhxg",
    }