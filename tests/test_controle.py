# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes do envelope de controlo: o que uma chamada diz.

O módulo é pequeno e o formato é novo, o que faz destes testes mais
importantes do que a contagem de linhas sugere: um parser malformado
aceita uma linha que devia recusar, e o sintoma — um convite que toca
sozinho, ou um ficheiro que se lê mal — não aponta para o parser.
"""

from __future__ import annotations

import pytest

from messenger import controle
from messenger.controle import ErroControlo

#: A sessão de todos os testes. Fixa os bytes porque um teste que
#: invoca a sorte só falha uma vez em duzentas e cinquenta.
SESSAO = controle.sessao_nova(bytes(range(16)))

#: A sessão invertida, para os testes que precisam de duas.
OUTRA = controle.sessao_nova(bytes(reversed(range(16))))


# ---------------------------------------------------------------------
# A linha
# ---------------------------------------------------------------------


def test_a_linha_de_ring_tem_os_tres_campos():
    linha = controle.linha_call("RING", SESSAO, video="vp8", audio="opus")
    assert linha == f"ONYX-CALL/1 RING {SESSAO} vp8 opus"


def test_a_linha_de_accept_so_tem_a_sessao():
    linha = controle.linha_call("ACCEPT", SESSAO)
    assert linha == f"ONYX-CALL/1 ACCEPT {SESSAO}"


def test_a_linha_abre_de_volta():
    """O que sai de :func:`linha_call` volta igual de :func:`abrir`."""
    for accao, campos in (
        ("RING", {"video": "vp8", "audio": "opus"}),
        ("ACCEPT", {}),
        ("REJECT", {}),
        ("CANCEL", {}),
        ("BYE", {}),
    ):
        linha = controle.linha_call(accao, SESSAO, **campos)
        comando = controle.abrir(linha)
        assert comando is not None, linha
        assert comando.tipo == "call"
        assert comando.accao == accao
        assert comando.sessao == SESSAO
        assert comando.campos == campos


def test_o_ficheiro_abre_de_volta():
    linha = controle.linha_ficheiro(
        "MANIFESTO", "ab" * 16, nome="relatorio.pdf", total="12345"
    )
    comando = controle.abrir(linha)
    assert comando is not None
    assert comando.tipo == "ficheiro"
    assert comando.accao == "MANIFESTO"
    assert comando.campos == {"nome": "relatorio.pdf", "total": "12345"}


# ---------------------------------------------------------------------
# O que NÃO é controlo
# ---------------------------------------------------------------------


def test_uma_linha_de_texto_normal_da_none():
    """O caso comum, e o que a sondagem depende.

    Uma linha de conversa tem de dar ``None`` e não um erro: o receptor
    sonda o texto primeiro e, se ``abrir`` levantasse em tudo, nenhuma
    conversa abriria.
    """
    for linha in (
        "olá, como vais?",
        "",
        "ONYX-CALL",
        "ONYX-CALL/",
        "onyx-call/1 RING",
        " ONYX-CALL/1 RING abc",
    ):
        assert controle.abrir(linha) is None, linha


def test_uma_outra_versao_da_none_e_nao_erro():
    """Uma versão diferente **não** é um erro de parse.

    O que se decide aqui é uma coisa de compatibilidade: uma linha
    escrita por uma versão futura tem de ser tratada como o que é — uma
    mensagem que este sistema não percebe — e não como uma linha de
    controlo corrompida. Levantar `ErroControlo` faria uma conversa
    normal com um `#ONYX-CALL` no meio parar de abrir.

    E o preço é explícito: uma linha de controlo de uma versão
    desconhecida **não abre**. Quem a mandou vai ver uma chamada que
    nunca toca, e não uma mensagem de erro. É a escolha certa — é o
    mesmo princípio do handshake (`docs/handshake.md` §Tolerância de
    versões), onde uma versão desconhecida recusa em vez de tentar.
    """
    assert controle.abrir(f"ONYX-CALL/99 RING {SESSAO} vp8 opus") is None
    assert controle.abrir(f"ONYX-FICHEIRO/99 MANIFESTO {'ab'*16} a=1") is None


def test_os_filtros_dizem_o_que_e_sem_desdobrar():
    """``e_de_call`` e ``e_de_ficheiro`` são o filtro, sem o custo.

    O receptor tem de decidir o que é **antes** de tentar abrir, e
    ``abrir`` levanta em linha malformada — que num filtro seria um
    custo por mensagem de texto.
    """
    linha_call = controle.linha_call("ACCEPT", SESSAO)
    linha_fich = controle.linha_ficheiro("MANIFESTO", "ab" * 16, total="1")

    assert controle.e_de_call(linha_call)
    assert not controle.e_de_call(linha_fich)
    assert not controle.e_de_call("olá")

    assert controle.e_de_ficheiro(linha_fich)
    assert not controle.e_de_ficheiro(linha_call)
    assert not controle.e_de_ficheiro("olá")


# ---------------------------------------------------------------------
# O que tem de recusar
# ---------------------------------------------------------------------


@pytest.mark.parametrize(
    "linha",
    [
        pytest.param("ONYX-CALL/1 TOCAR", id="comando-que-nao-existe"),
        pytest.param("ONYX-CALL/1 RING", id="sem-nada"),
    ],
)
def test_um_comando_desconhecido_recusa(linha):
    with pytest.raises(ErroControlo):
        controle.abrir(linha)


def test_um_comando_de_ficheiro_desconhecido_recusa():
    """Um `BAIXAR` que ninguém reconhece não é um ficheiro a chegar.

    Sem esta verificação o receptor trataria a linha como um manifesto
    com um campo, e guardaria metadados que não sabe o que são.
    """
    with pytest.raises(ErroControlo):
        controle.abrir(f"ONYX-FICHEIRO/1 DESCONHECIDO {'ab'*16}")


def test_a_sessao_tem_de_ser_hex_minusculo_com_32():
    """A sessão tem de ter a forma exacta.

    Maiúsculas são recusadas de propósito: a sessão de Alice e a de Bob
    têm de ser a **mesma** string dos dois lados, e aceitar as duas
    formas daria duas sessões para uma chamada.
    """
    for sessao in ("abc", SESSAO[:-1], SESSAO + "a", SESSAO.upper(), "", "z" * 32):
        with pytest.raises(ErroControlo, match="sessão mal formada"):
            controle.linha_call("ACCEPT", sessao)


def test_a_sessao_nova_tem_o_comprimento_certo():
    assert len(controle.sessao_nova()) == controle.TAM_HEX_SESSAO
    assert len(controle.sessao_nova(bytes(16))) == 32


def test_a_sessao_nova_recusa_o_que_nao_da_16_bytes():
    for n in (0, 8, 15, 17, 32):
        with pytest.raises(ErroControlo, match="sessão são 16 bytes"):
            controle.sessao_nova(bytes(n))


def test_a_sessao_nova_e_diferente_de_chamada_para_chamada():
    """Duas sessões não podem ser iguais, ou uma chamada pode ser injectada.

    Este teste não prova aleatoriedade — não pode, e seria falso dizer
    que prova. Prova que a fonte não é um contador: mil chamadas dão
    mil sessões distintas.
    """
    sessoes = {controle.sessao_nova() for _ in range(1000)}
    assert len(sessoes) == 1000


@pytest.mark.parametrize(
    "campos, mensagem",
    [
        pytest.param({"video": "vp8"}, "faltam audio", id="sem-audio"),
        pytest.param({"audio": "opus"}, "faltam video", id="sem-video"),
        pytest.param({}, "faltam", id="sem-ambos"),
        pytest.param({"video": "vp8", "audio": "opus", "extra": "x"}, "a mais", id="a-mais"),
    ],
)
def test_o_ring_exige_o_seis_campos(campos, mensagem):
    with pytest.raises(ErroControlo, match=mensagem):
        controle.linha_call("RING", SESSAO, **campos)


def test_o_accept_nao_leva_campos():
    with pytest.raises(ErroControlo, match="a mais"):
        controle.linha_call("ACCEPT", SESSAO, video="vp8")


def test_um_codec_que_ninguem_sabe_descodificar_recusa():
    """`h265` recusa na escrita, e não na leitura.

    Um codec que o receptor não tem produz uma chamada que toca e não
    mostra. Recusar no emissor dá o erro a quem está a chamar, que é a
    quem pode escolher outro.
    """
    for nome, valor in (("video", "h265"), ("audio", "mp3"), ("video", "av1")):
        with pytest.raises(ErroControlo, match=f"não sabe {nome}"):
            controle.linha_call("RING", SESSAO, **{"video": "vp8", "audio": "opus", nome: valor})


def test_um_valor_com_espaco_recusa():
    """O espaço é o separador, e um valor com espaço parte a linha.

    Sem esta verificação um `video` com espaço produziria uma linha com
    um campo a mais, e o receptor contaria mal os campos.
    """
    with pytest.raises(ErroControlo, match="não sabe video"):
        controle.linha_call("RING", SESSAO, video="v p8", audio="opus")


def test_a_linha_truncada_recusa():
    """Faltar um campo é erro, e o erro diz quantos faltam.

    Uma linha truncada que fosse aceite teria um campo `None`, e o
    receptor escolheria um descodificador com `None`.
    """
    with pytest.raises(ErroControlo, match=r"RING.*2 campo\(s\).*traz 1"):
        controle.abrir(f"ONYX-CALL/1 RING {SESSAO} vp8")


def test_a_linha_a_mais_recusa():
    """O mesmo pela outra ponta: um campo que ninguém pediu.

    Uma linha com um campo a mais é a mesma informação com uma
    ambiguidade, e aceitar daria dois leitores para o mesmo comando.
    """
    # `ONYX-CALL/1 ACCEPT <sessao> a mais` são dois campos depois da
    # sessão, e `ACCEPT` não quer nenhum.
    with pytest.raises(ErroControlo, match=r"ACCEPT.*0 campo\(s\).*traz 2"):
        controle.abrir(f"ONYX-CALL/1 ACCEPT {SESSAO} a mais")


def test_um_campo_de_ficheiro_sem_igual_recusa():
    with pytest.raises(ErroControlo, match="mal formado"):
        controle.abrir(f"ONYX-FICHEIRO/1 MANIFESTO {'ab'*16} semigual")


def test_um_campo_de_ficheiro_repetido_recusa():
    """A segunda ocorrência seria silenciosamente ignorada.

    E o emissor pensaria que a mandou. Um `size=1 size=2` tem de ser
    erro, não um `size=2`.
    """
    with pytest.raises(ErroControlo, match="vem duas vezes"):
        controle.abrir(f"ONYX-FICHEIRO/1 MANIFESTO {'ab'*16} tamanho=1 tamanho=2")


def test_um_comando_de_ficheiro_desconhecido_recusa():
    with pytest.raises(ErroControlo, match="comando de ficheiro desconhecido"):
        controle.linha_ficheiro("BAIXAR", "ab" * 16)


def test_o_identificador_de_ficheiro_tem_de_ser_hex():
    for ident in ("abc", "zz" * 16, "AB" * 16):
        with pytest.raises(ErroControlo, match="identificador mal formado"):
            controle.linha_ficheiro("MANIFESTO", ident)


# ---------------------------------------------------------------------
# A forma do comando
# ---------------------------------------------------------------------


def test_o_comando_e_imutavel():
    """Um comando que muda depois de cifrado não é o que foi assinado.

    O `frozen=True` é o que garante isto em tempo de execução, e não é
    uma questão de estilo: um `dataclass` normal permitiria alterar a
    sessão de um convite entre a cifra e o envio.
    """
    comando = controle.abrir(controle.linha_call("ACCEPT", SESSAO))
    assert comando is not None
    with pytest.raises(Exception):  # FrozenInstanceError
        comando.sessao = OUTRA  # type: ignore[misc]


def test_o_comando_confere_a_forma():
    with pytest.raises(ErroControlo, match="tipo de controlo desconhecido"):
        controle.Comando(tipo="texto", accao="RING")
    with pytest.raises(ErroControlo, match="sessão mal formada"):
        controle.Comando(tipo="call", accao="RING", sessao="curta")


def test_as_sessoes_das_duas_partes_sao_distintas_e_iguais_a_si_proprias():
    """A sessão é a âncora, e tem de sobreviver à ida e à volta.

    A mesma sessão dos dois lados é o que liga um `ACCEPT` ao `RING` que
    o provocou. Duas sessões diferentes são duas chamadas.
    """
    linha = controle.linha_call("RING", SESSAO, video="vp8", audio="opus")
    a = controle.abrir(linha)
    b = controle.abrir(linha)
    assert a is not None and b is not None
    assert a.sessao == b.sessao == SESSAO
    assert a.sessao != OUTRA


# ---------------------------------------------------------------------
# A lista de comandos é fechada
# ---------------------------------------------------------------------


def test_a_lista_de_comandos_cobre_o_ciclo_de_uma_chamada():
    """Um `RING` sem `ACCEPT` nem `REJECT` deixaria quem chama à espera.

    A lista é fechada e tem de cobrir os cinco momentos: chamar, aceitar,
    recusar, desistir e terminar. Um comando que falta aqui é um estado
    em que a chamada fica pendurada.
    """
    assert set(controle.COMANDOS_CALL) == {"RING", "ACCEPT", "REJECT", "CANCEL", "BYE"}
    assert set(controle.CAMPOS_ESPERADOS) == set(controle.COMANDOS_CALL)


def test_cada_comando_tem_a_descricao():
    """A descrição é o que a UI mostra, e um comando sem ela é invisível.

    Não é decorativo: `COMANDOS_CALL` é a fonte de verdade da
    interface, e
    uma entrada sem descrição apareceria como um botão sem rótulo.
    """
    for accao, texto in controle.COMANDOS_CALL.items():
        assert texto, f"«{accao}» não tem descrição"
        assert texto[0].islower() or accao in ("RING", "ACCEPT"), texto
