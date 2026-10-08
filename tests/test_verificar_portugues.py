# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes do verificador de português.

## Porque é que um verificador precisa de testes

Um verificador que **não reporta nada** é indistinguível de um verificador
correcto. Em 2026-10-07, alargar as extensões de
`scripts/verificar_portugues.py` aos ficheiros JavaScript deu «nenhum
americanismo encontrado» — e a razão era que o filtro que lá pusera
removia o português correcto junto com as classes do Tailwind. Um verificador
que engole texto é um buraco com aspecto de portão.

Estes testes existem para que «verde» signifique «verde» e para que
qualquer alteração ao filtro quebre alguma coisa. São a prova de que o
verificador ainda **apanha**.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

import pytest

RAIZ = Path(__file__).resolve().parent.parent
VERIFICADOR = RAIZ / "scripts" / "verificar_portugues.py"


def _modulo():
    """Importa o verificador como módulo, sem o executar."""
    import importlib.util

    spec = importlib.util.spec_from_file_location("verificar_portugues", VERIFICADOR)
    assert spec and spec.loader
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


vp = _modulo()


def _apanha(texto: str) -> bool:
    """O Americanismo é encontrado depois de todo o filtro?

    Reimplementa a cadeia de filtros do verificador, e **inclui o
    `ACEITOS`** — que é onde vive a excepção de `arquivo`. Uma cópia
    que seesqueça dele dá resultados diferentes das do verificador, e um
    teste que mede outra coisa do que julga medir é pior do que nenhum.
    """
    limpa = vp.limpar(texto)
    limpa = vp.CLASSNAME.sub(" ", limpa)
    limpa = vp.UTILITARIOS_TAILWIND.sub(" ", limpa)
    limpa = vp.IDENTIFICADORES.sub(" ", limpa)
    for padrao in vp.AMERICANISMOS:
        for m in re.finditer(padrao, limpa, re.IGNORECASE):
            if m.group(0).lower() in vp.ACEITOS:
                continue
            return True
    return False


# ---------------------------------------------------------------------
# O verificador tem de apanhar
# ---------------------------------------------------------------------

@pytest.mark.parametrize(
    "texto",
    [
        "os arquivos do projecto",
        "arquivos corrompidos",
        "o arquivo foi apagado",
        "a senha do utilizador",
        "as senhas guardadas",
        "o usuário autenticou-se",
        "ver a tela inteira",
        "no celular",
    ],
)
def test_apanha_os_americanismos(texto: str) -> None:
    """Os casos que o verificador existe para apanhar são apanhados.

    Sem esta lista, o filtro pode passar a engolir tudo e o verificador
    continua a dar verde.
    """
    assert _apanha(texto), f"não apanhou {texto!r}"


# ---------------------------------------------------------------------
# O verificador não pode engolir o português
# ---------------------------------------------------------------------

@pytest.mark.parametrize(
    "texto",
    [
        # `arquivar` e `arquivado` são portugueses e passam: o padrão
        # é `\barquivo(s)?\b` com fronteira de palavra, e o sufixo
        # impede a distinção. O que **não** passa é o adjectivo
        # `Arquivadas`, pelo mesmo motivo.
        #
        # `arquivo` como substantivo — «arquivo notarial» — é um falso
        # positivo aceite de propósito, e está registado como tal no
        # verificador. Ver `test_falsos_positivos_conhecidos`.
        "arquivar a mensagem",
        "arquivado",
        # O resto do vocabulário correcto, que um filtro demasiado
        # largo apagaria.
        "palavra-passe",
        "ficheiro temporário",
        "ecrã inteiro",
        "telemóvel",
        "utilizador",
    ],
)
def test_nao_engole_portugues_correcto(texto: str) -> None:
    """O filtro tem de ser largo no que apanha e estreito no que apaga.

    Este é o teste que teria apanhado o erro de 2026-10-07: o filtro de
    nomes de componente React removeva o texto **dentro** de
    `SenhaDoUtilizador`, e o verificador deixava de reportar «senha».
    """
    assert not _apanha(texto), f"engoliu {texto!r}"


# ---------------------------------------------------------------------
# Falsos positivos que se aceitam, e porque
# ---------------------------------------------------------------------

@pytest.mark.parametrize(
    "texto",
    [
        "arquivo notarial",
        "o arquivo de arquivo",
        "«Arquivadas» e não «Arquivo»",
    ],
)
def test_falsos_positivos_conhecidos(texto: str) -> None:
    """Os casos que o verificador **erra** de propósito.

    `arquivo` é um substantivo comum do português e o padrão reporta-o
    como americanismo. Silenciá-lo exigiria filtrar pela forma — plural,
    artigo — e quatro padrões foram testados: apanhavam o americanismo
    («o arquivo foi apagado») ou o português correcto («o arquivo de
    arquivo»), nunca só um.

    A escolha é feita a favor de **ver o americanismo**. Um verificador
    que aceita a palavra toda deixa de reportar o calão mais comum do
    português de informática, e um verificador que não reporta é
    indistinguível de um que funciona.

    Este teste existe para que a falha seja **deliberada e visível**: se
    um dia alguém resolver o problema com um filtro melhor, este teste
    falha e obriga a decidir o que fazer com a lista de excepções.
    """
    assert _apanha(texto), (
        f"{texto!r} deixou de ser falso positivo — o filtro melhorou. "
        "Decidir: aceitar em ACEITOS, ou actualizar esta lista."
    )


# ---------------------------------------------------------------------
# O filtro é sintaxe, não vocabulário
# ---------------------------------------------------------------------

@pytest.mark.parametrize(
    "texto",
    [
        'className="flex min-h-screen h-screen w-full max-w-md"',
        'className={cn("p-4 bg-surface text-foreground")}',
        'className={cond ? "hidden" : "block"}',
    ],
)
def test_as_classes_do_tailwind_nao_sao_portugues(texto: str) -> None:
    """`h-screen` é o nome de uma utilitária, não uma palavra traduzida.

    Um `screen` aqui é o que a biblioteca chama. Traduzi-lo daria um
    componente que não compila.
    """
    assert not _apanha(texto), f"apanhou dentro de um className: {texto!r}"


def test_o_texto_fora_do_classname_e_apanhado() -> None:
    """O filtro apaga o atributo, não a linha inteira.

    Um americanismo numa cadeia normal, ao lado de um `className`, tem de
    continuar a ser reportado.
    """
    assert _apanha('const senha = "min-h-screen flex";')
    assert not _apanha('const largura = "min-h-screen flex";')


def test_um_token_entre_crases_e_literal() -> None:
    """Entre crases não há prosa, e portanto não há americanismo.

    A regra vem do próprio `limpar()`: um token entre crases é o nome de
    uma coisa, não uma palavra escolhida por alguém. É o que permite
    documentar um americanismo — `UI/README.md` e este ficheiro reportam
    as ocorrências que corrigiram, e não podem pagar por isso.
    """
    assert not _apanha("um «gerador de `senhas`» de sempre")
    assert _apanha("um «gerador de senhas» de sempre")


@pytest.mark.parametrize(
    "texto",
    [
        "o ficheiro Senha.js guarda a palavra-passe",
        "ver `pages/Registar.jsx` e `lib/onyx/nav.js`",
    ],
)
def test_um_nome_de_ficheiro_nao_e_prosa(texto: str) -> None:
    """Um caminho não é português, mesmo quando parece.

    `Senha.js` é o nome de um ficheiro. Sem a excepção, o auditor
    reportaria o nome de um ficheiro como se fosse uma palavra escrita
    por alguém — e o ficheiro com o pior nome seria impossível de
    corrigir.
    """
    assert not _apanha(texto), f"apanhou um nome de ficheiro: {texto!r}"


# ---------------------------------------------------------------------
# As extensões
# ---------------------------------------------------------------------

def test_o_yaml_entra_na_busca() -> None:
    """`.yml` e `.yaml` têm de estar na lista de extensões.

    Os workflows são configuração versionada, com prosa portuguesa nos
    nomes dos passos. Um ficheiro que decide se o projecto é verificado
    e que ninguém lê é onde a documentação apodrece em silêncio.
    """
    codigo = VERIFICADOR.read_text(encoding="utf-8")
    for extensao in (".yml", ".yaml"):
        assert f'"{extensao}"' in codigo, f"{extensao} não está na lista"


def test_um_comando_nao_e_prosa() -> None:
    """`rustup default stable` é um comando, não uma frase mal traduzida.

    `default` é o nome de um subcomando do `rustup`. Traduzi-lo daria um
    comando que não existe — o mesmo caso de `className`, e a mesma
    razão. O que o remove é `IDENTIFICADORES`, o filtro que já existia
    para identificadores; não há uma regra específica para `run:`.

    E o nome de um passo continua a ser auditado, porque é prosa.
    """
    assert not _apanha("run: |\n  rustup default stable\n")
    assert _apanha("- name: a senha do utilizador")


def test_o_javascript_entra_na_busca() -> None:
    """`.js` e `.jsx` têm de estar na lista de extensões.

    A lista tem de ser lida do código e não repetida aqui: um teste que
    copiasse a lista passaria depois de alguém acrescentar uma extensão
    ao verificador sem actualizar o teste — que é exactamente o que o
    projecto promete não fazer.
    """
    codigo = VERIFICADOR.read_text(encoding="utf-8")
    for extensao in (".js", ".jsx"):
        assert f'"{extensao}"' in codigo, f"{extensao} não está na lista"


def test_o_verificador_passa_no_repositorio() -> None:
    """A varredura real tem de dar verde.

    Sem este teste, uma alteração que faça o verificador engolir texto
    passa em todos os testes acima e falha só no CI — se houver CI.
    """
    r = subprocess.run(
        [sys.executable, str(VERIFICADOR)],
        capture_output=True,
        text=True,
        cwd=RAIZ,
    )
    assert r.returncode == 0, f"verificador falhou:\n{r.stdout}\n{r.stderr}"