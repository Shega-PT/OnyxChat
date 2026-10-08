# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_origem.py — quem tem o direito de falar com o sidecar.

Estes testes estão escritos como **ataques**, não como chamadas. A
diferença importa: um teste que verifica «a função devolve ``True`` para um
pedido legítimo» passa mesmo que a função recuse tudo, e uma defesa que
recusa tudo também é uma defesa — até alguém precisar de abrir a aplicação.

Por isso cada caso de negação vem com o nome do ataque que representa, e
cada caso de aceitação vem com o nome do cliente que tem de funcionar.
"""

from __future__ import annotations

import pytest

from server.origem import (
    METODOS_COM_EFFECTO,
    Decisao,
    avaliar,
)

#: Cabeçalhos de uma chamada da própria aplicação, servida pelo sidecar.
LEGITIMO = {
    "Host": "127.0.0.1:8787",
    "Origin": "http://127.0.0.1:8787",
    "Sec-Fetch-Site": "same-origin",
}

PORTA = 8787


def com(**substituicoes: str) -> dict[str, str]:
    """Os cabeçalhos legítimos com alguns trocados."""
    return {**LEGITIMO, **substituicoes}


# ---------------------------------------------------------------------
# O que tem de funcionar
# ---------------------------------------------------------------------


def test_a_aplicacao_propria_passa() -> None:
    assert avaliar("GET", LEGITIMO, PORTA)


def test_a_aplicacao_pode_mudar_o_estado() -> None:
    """`POST` da mesma origem é o caminho normal de enviar uma mensagem."""
    assert avaliar("POST", LEGITIMO, PORTA)


def test_escrever_por_localhost_tambem_e_a_aplicacao() -> None:
    """`localhost` e `127.0.0.1` são origens distintas para o browser.

    Quem abre `http://localhost:8787` à mão tem de funcionar. Uma defesa
    que obrigasse a escrever o endereço de uma maneira só seria um
    problema a resolver desligando a defesa.
    """
    assert avaliar("GET", com(Host="localhost:8787", Origin="http://localhost:8787"), PORTA)


def test_a_aplicacao_via_ipv6_tambem_e_a_aplicacao() -> None:
    assert avaliar("GET", com(Host="[::1]:8787", Origin="http://[::1]:8787"), PORTA)


def test_o_origin_pode_vir_com_barra_final() -> None:
    """Uma barra final não muda a origem, e não deve mudar a decisão."""
    assert avaliar("GET", com(Origin="http://127.0.0.1:8787/"), PORTA)


def test_o_origin_e_comparado_sem_respeitar_as_mausculas_do_host() -> None:
    """`Origin` não é sensível às maiúsculas no esquema e no anfitrião."""
    assert avaliar("GET", com(Origin="HTTP://127.0.0.1:8787"), PORTA)


def test_navegacao_sem_origin_passa() -> None:
    """Abrir o endereço na barra do browser não leva `Origin`.

    Recusar isto destruiria a aplicação: a primeira visita é sempre uma
    navegação, e ninguém conseguia entrar.
    """
    decisao = avaliar("GET", {"Host": "127.0.0.1:8787"}, PORTA)
    assert decisao, decisao.motivo


def test_recurso_de_outra_origem_sem_origin_passa() -> None:
    """`<img>` e `<script>` entre origens não levam `Origin` nem effecto."""
    assert avaliar("GET", {"Host": "127.0.0.1:8787"}, PORTA)


def test_um_cliente_local_com_sec_fetch_none_pode_ler() -> None:
    """`Sec-Fetch-Site: none` é navegação directa, e navegação é ler.

    Recusar leitura a quem abriu a aplicação é recusar a aplicação.
    """
    assert avaliar("GET", com(**{"Sec-Fetch-Site": "none"}), PORTA)


def test_um_outro_servidor_no_mesmo_computador_pode_ler() -> None:
    """`same-site` inclui outra porta no mesmo anfitrião.

    O caso real é um `localhost:3000` de outra pessoa durante o
    desenvolvimento. Recusar a leitura a partir de lá é o que torna a
    excepção mais brittle do que o problema, e o risco é o oposto do que
    se quer proteger: nenhum atacante ganha com a *leitura* porque a
    resposta é o que fica bloqueado pela política de origem do browser.
    """
    assert avaliar("GET", com(**{"Sec-Fetch-Site": "same-site"}), PORTA)


# ---------------------------------------------------------------------
# Ataques
# ---------------------------------------------------------------------


def test_ataque_le_a_identidade_de_uma_pagina_web() -> None:
    """Ataque principal: uma página da Internet lê o sidecar.

    É o ataque que a subordinação de origem do browser **não** impede, e
    é a razão de este módulo existir.
    """
    decisao = avaliar(
        "GET",
        com(Origin="https://atacante.example", **{"Sec-Fetch-Site": "cross-site"}),
        PORTA,
    )
    assert not decisao
    assert decisao.codigo == 403


def test_ataque_com_origem_de_qualquer_pagina() -> None:
    """Qualquer `Origin` que não seja o nosso é recusada."""
    for origem in (
        "https://atacante.example",
        "http://atacante.example",
        "http://127.0.0.1:9999",      # outra porta
        "http://127.0.0.2:8787",      # outro endereço de loopback
        "http://localhost:9999",
        "https://127.0.0.1:8787",     # esquema diferente, mesma máquina
        "null",                        # `file://`, sandbox de iframe
        "",
    ):
        decisao = avaliar("GET", com(Origin=origem), PORTA)
        assert not decisao, f"aceitou Origin={origem!r}"


def test_ataque_de_rebinding_de_dns() -> None:
    """*Rebinding*: o `Host` é o do atacante, o pedido vai para o loopback.

    O atacante serve ``atacante.example`` com TTL zero; ao fim de alguns
    segundos o nome resolve para ``127.0.0.1``. O browser continua a
    acreditar que está na origem hostil, e o pedido chega ao sidecar com
    ``Host: atacante.example``.

    Um servidor que não olhe para o `Host` não tem como distinguir este
    pedido de um legítimo — e o `Origin` do atacante pode ser o que ele
    quiser, porque o browser o preenche sozinho.
    """
    decisao = avaliar(
        "GET",
        {
            "Host": "atacante.example",
            "Origin": "http://atacante.example",
            "Sec-Fetch-Site": "cross-site",
        },
        PORTA,
    )
    assert not decisao
    assert decisao.codigo == 403


def test_ataque_de_rebinding_com_origem_forjada() -> None:
    """O mesmo, com um `Origin` que parece o nosso.

    É o caso perigoso: se o servidor confiasse no `Origin` sem ver o
    `Host`, este pedido passaria. A ordem das verificações é o que fecha
    isto.
    """
    decisao = avaliar(
        "GET",
        {"Host": "atacante.example", "Origin": "http://127.0.0.1:8787"},
        PORTA,
    )
    assert not decisao, decisao.motivo


def test_ataque_de_rebinding_a_escrita() -> None:
    """O mesmo rebinding, a tentar enviar uma mensagem."""
    decisao = avaliar(
        "POST",
        {
            "Host": "atacante.example",
            "Origin": "http://127.0.0.1:8787",
            "Sec-Fetch-Site": "cross-site",
        },
        PORTA,
    )
    assert not decisao


def test_ataque_de_csrf_com_formulario_cross_site() -> None:
    """Um `<form>` apontado ao sidecar, numa página atacante.

    O browser **manda** `Origin` num `POST` de formulário, precisamente
    porque é isso que o distingue de uma navegação. E é por isso que
    recusar também depende do `Origin` e não só do `Sec-Fetch-Site`.
    """
    decisao = avaliar(
        "POST",
        com(Origin="https://atacante.example", **{"Sec-Fetch-Site": "cross-site"}),
        PORTA,
    )
    assert not decisao


def test_ataque_de_csrf_sem_origin() -> None:
    """`curl` a escrever directamente no sidecar.

    Não há browser que faça isto. Um cliente HTTP bruto omite o `Origin`
    porque não tem de o mandar — e o lado do servidor tem de o exigir
    assim que o pedido tem efeito.
    """
    for metodo in sorted(METODOS_COM_EFFECTO):
        decisao = avaliar(metodo, {"Host": "127.0.0.1:8787"}, PORTA)
        assert not decisao, f"{metodo} sem Origin passou"
        assert decisao.codigo == 403


def test_ataque_a_formulario_navegado() -> None:
    """`Sec-Fetch-Site: none` com efeito é navegação com `<form>`.

    A aplicação não submete formulários ao próprio servidor — fala JSON.
    Um pedido com efeito e navegação é sempre um ataque ou um cliente
    exótico, e recusar não custa nada a quem usa a aplicação.
    """
    decisao = avaliar(
        "POST",
        {"Host": "127.0.0.1:8787", "Sec-Fetch-Site": "none"},
        PORTA,
    )
    assert not decisao


def test_ataque_sem_host() -> None:
    """Sem `Host` não há como saber a que endereço o pedido se destina."""
    assert not avaliar("GET", {"Origin": "http://127.0.0.1:8787"}, PORTA)


def test_ataque_com_host_de_outro_servico_no_loopback() -> None:
    """Outro programa no mesmo computador, noutra porta.

    Não é uma ameaça — é outra pessoa a usar a máquina. Mas aceitar um
    `Host` que não é o nosso transformaria o sidecar num ponto de entrada
    de qualquer serviço que se aperceba do endereço, e o custo de o
    recusar é zero para a aplicação.
    """
    assert not avaliar("GET", com(Host="127.0.0.1:5000"), PORTA)


def test_sec_fetch_site_ausente_nao_dispensa_o_origin() -> None:
    """Um cliente que não manda `Sec-Fetch-Site` tem de mandar `Origin`.

    É o que fecha o caso do `curl`: pode escrever `Sec-Fetch-Site` à
    vontade, porque não há nada que o obrigue a dizer a verdade, mas
    então tem de passar no `Origin`, e um `curl` de ataque não tem um
    `Origin` legítimo para mandar.
    """
    # Com `Origin` bom e sem `Sec-Fetch-Site`, passa.
    assert avaliar("GET", {"Host": "127.0.0.1:8787", "Origin": "http://127.0.0.1:8787"}, PORTA)
    # Sem nenhum dos dois, e a escrever, não passa.
    assert not avaliar("POST", {"Host": "127.0.0.1:8787"}, PORTA)


def test_a_origem_no_todo_pode_ser_minuscula() -> None:
    """O `Origin` é case-insensitive no esquema e no anfitrião."""
    assert avaliar("GET", com(Origin="http://LOCALHOST:8787"), PORTA)


def test_o_host_pode_vir_com_espacos() -> None:
    """Espaço em volta é ruído de um cliente mal-educado, não um ataque."""
    assert avaliar("GET", com(Host="  127.0.0.1:8787  "), PORTA)


# ---------------------------------------------------------------------
# A forma do objecto
# ---------------------------------------------------------------------


def test_a_decisao_e_verdadeira_quando_permite() -> None:
    decisao = avaliar("GET", LEGITIMO, PORTA)
    assert isinstance(decisao, Decisao)
    assert bool(decisao) is True
    assert decisao.codigo == 200
    assert decisao.motivo == ""


def test_a_decisao_guarda_a_causa_nos_registos() -> None:
    """A causa vai para o registo local e **não** para a resposta.

    Dizer a um atacante qual das quatro defesas o apanhou é dar-lhe o
    trabalho de tentar as outras três.
    """
    decisao = avaliar("GET", com(Origin="https://atacante.example"), PORTA)
    assert "Origin" in decisao.motivo


def test_a_decisao_tem_representacao_util() -> None:
    """Só para depuração; o `pragma` no módulo marca-o como não testado."""
    assert "permitido=True" in repr(avaliar("GET", LEGITIMO, PORTA))


def test_o_metodo_e_normalizado() -> None:
    """`post` minúsculo é `POST`, e não um método desconhecido."""
    assert avaliar("post", LEGITIMO, PORTA)
    assert not avaliar("post", {"Host": "127.0.0.1:8787"}, PORTA)


def test_os_cabecalhos_sao_normalizados() -> None:
    """Chaves em minúsculas ou em maiúsculas dão a mesma decisão."""
    for chave in ("host", "HOST", "Host"):
        assert not avaliar("POST", {chave: "atacante.example"}, PORTA)


def test_a_porta_da_o_ponto_de_entrada() -> None:
    """A porta entra na comparação do `Host` e da origem.

    Um sidecar noutra porta tem a sua própria lista — e é por isso que a
    porta é um argumento e não uma constante: mudá-la sem mudar a defesa
    deixaria um sidecar recusado na totalidade.
    """
    assert avaliar("GET", {"Host": "127.0.0.1:9999"}, porta=9999)
    assert not avaliar("GET", {"Host": "127.0.0.1:8787"}, porta=9999)


# ---------------------------------------------------------------------
# A excepção de desenvolvimento
# ---------------------------------------------------------------------


def test_sem_variavel_de_ambiente_nao_ha_origem_extra() -> None:
    """A predefinição é a lista fechada. Não é uma lista que cresce sozinha."""
    assert avaliar(
        "POST",
        {"Host": "127.0.0.1:8787", "Origin": "http://localhost:3000"},
        PORTA,
    ).permitido is False


def test_a_variavel_de_ambiente_acrescenta_uma_origem(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Desenvolver a interface noutra porta tem de ser possível.

    `npm run dev` põe a página em `localhost:3000`, e essa página tem de
    poder falar com o sidecar em `127.0.0.1:8787`. Sem esta excepção a
    resposta de quem está a desenvolver seria desligar a defesa, e aí a
    excepção deixaria de ser de desenvolvimento.
    """
    monkeypatch.setenv("ONYX_ORIGENS", "http://localhost:3000")
    pedido = {
        "Host": "127.0.0.1:8787",
        "Origin": "http://localhost:3000",
        "Sec-Fetch-Site": "cross-site",
    }
    assert avaliar("POST", pedido, PORTA)


def test_a_variavel_aceita_varias_origens(monkeypatch: pytest.MonkeyPatch) -> None:
    """Separadas por vírgula, e espaços à volta são ignorados."""
    monkeypatch.setenv("ONYX_ORIGENS", " http://localhost:3000 , https://preview.example ")
    for origem in ("http://localhost:3000", "https://preview.example"):
        assert avaliar("GET", {"Host": "127.0.0.1:8787", "Origin": origem}, PORTA)


def test_a_variavel_nao_traspassa_o_host(monkeypatch: pytest.MonkeyPatch) -> None:
    """Acrescentar uma origem não desliga a verificação do `Host`.

    A excepção é para a **origem**, e o `Host` continua a ser o que decide
    a que endereço o pedido se destinava. Sem esta separação, Bastava
    `ONYX_ORIGENS` para abrir a porta a qualquer coisa.
    """
    monkeypatch.setenv("ONYX_ORIGENS", "http://localhost:3000")
    assert not avaliar("GET", {"Host": "atacante.example", "Origin": "http://localhost:3000"}, PORTA)


def test_origem_invalida_na_variavel_e_ignorada(monkeypatch: pytest.MonkeyPatch) -> None:
    """Um erro de escrita não pode inutilizar o sidecar.

    Uma variável com ``http://localhost:3000,,`` ou só com espaços não
    pode fazer o programa recusar tudo. Ignorar a entrada estragada e
    seguir com as boas é o comportamento que deixa o erro visível sem
    tornar o programa inutilizável.
    """
    monkeypatch.setenv("ONYX_ORIGENS", "  ,  ,http://localhost:3000,  ")
    assert avaliar("GET", {"Host": "127.0.0.1:8787", "Origin": "http://localhost:3000"}, PORTA)
    assert avaliar("GET", {"Host": "127.0.0.1:8787", "Origin": "http://127.0.0.1:8787"}, PORTA)


def test_a_variavel_vazia_nao_muda_nada(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("ONYX_ORIGENS", "")
    assert avaliar("GET", LEGITIMO, PORTA)
    assert not avaliar("GET", {"Host": "127.0.0.1:8787", "Origin": "https://x.example"}, PORTA)


def test_a_variavel_tem_o_nome_certo() -> None:
    """O nome é parte da interface com quem develope.

    Um nome mudado sem mudar a documentação de `docs/sidecar.md` faria a
    excepção desaparecer em silêncio — que é o modo mais comum de uma
    protecção deixar de existir.
    """
    from server.origem import AMBIENTE_EXTRAS

    assert AMBIENTE_EXTRAS == "ONYX_ORIGENS"


def test_head_e_aceite_sem_origin() -> None:
    """`HEAD` é leitura, e é o que faz uma verificação de saúde."""
    assert avaliar("HEAD", {"Host": "127.0.0.1:8787"}, PORTA)


def test_metodos_desconhecidos_com_efeito_sao_recusados() -> None:
    """Um método que o sidecar não implementa não passa a checagem só
    porque não está na lista.

    A lista é fechada de propósito: acrescentar um método novo ao servidor
    sem acrescentar aqui daria uma operação com efeito que ninguém
    classificou.
    """
    assert not avaliar("PROPFIND", {"Host": "127.0.0.1:8787"}, PORTA)


def test_cross_site_de_outra_origem_apenas_le() -> None:
    """`<img>` e `<script>` de outra origem passam, e a escrita não.

    A leitura é inofensiva porque a política de origem do browser não
    deixa oscript ver a resposta — e é a leitura que não pode ser
    bloqueada, ou a interface não carregava nada.
    """
    pedido = {"Host": "127.0.0.1:8787", "Sec-Fetch-Site": "cross-site"}
    assert avaliar("GET", pedido, PORTA)
    assert not avaliar("POST", pedido, PORTA)


def test_same_origin_com_efeito_e_a_aplicacao() -> None:
    """O caminho normal: a aplicação a escrever, sem `Origin` no caminho.

    `fetch` da mesma origem **manda** `Origin`, mas um `<form>` com
    `Sec-Fetch-Site: same-origin` não manda, e há quem o use. A
    aplicação fala JSON e não submete formulários, mas recusar isto seria
    recusar a aplicação a si própria.
    """
    assert avaliar("POST", {"Host": "127.0.0.1:8787", "Sec-Fetch-Site": "same-origin"}, PORTA)


def test_options_e_uma_leitura() -> None:
    """`OPTIONS` é o pedido que o browser faz em `fetch` com método
    estranho. Trata-lo como escrita impediria a detecção de CORS."""
    assert avaliar("OPTIONS", {"Host": "127.0.0.1:8787"}, PORTA)


def test_a_lista_de_leitura_nao_cresce_sozinha() -> None:
    """Nenhum método HTTP pode entrar na lista sem que este teste falhe.

    É a verificação do *fail-open*: a lista é curta de propósito, e
    acrescentar a um lado sem o outro é o modo mais barato de deixar uma
    protecção de fora.
    """
    from server.origem import METODOS_DE_LEITURA

    assert METODOS_DE_LEITURA == frozenset({"GET", "HEAD", "OPTIONS"})


def test_e_tambem_inverso_da_de_efeito() -> None:
    """As duas listas descrevem o mesmo conjunto, por lados opostos."""
    from server.origem import METODOS_COM_EFFECTO, METODOS_DE_LEITURA

    assert not (METODOS_COM_EFFECTO & METODOS_DE_LEITURA)


def test_sec_fetch_site_sem_origin_e_leitura_valida() -> None:
    """O caminho que os outros testes não atingem: `Sec-Fetch-Site` sem `Origin`.

    É o caso de um recurso pedido por CSS ou por um `fetch` que o browser
    considera mesmo da mesma origem e por isso dispensa o `Origin`. Sem
    este teste a linha ficaria por cobrir, e uma linha por cobrir é uma
    linha em que ninguém pensou.
    """
    for sitio in ("same-origin", "same-site", "none"):
        assert avaliar("GET", {"Host": "127.0.0.1:8787", "Sec-Fetch-Site": sitio}, PORTA)
