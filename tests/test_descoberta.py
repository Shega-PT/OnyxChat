# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""testes de ``messenger/descoberta.py`` — resolução de destino P2P.

Cobre a matriz de ``docs/ipc_spec.md`` §``destino``:

* classificação estrita de ``.onion`` e de ID hex;
* ``directo`` resolve o ID via discovery; ``relay`` não consulta;
* revalidação do ``.onion`` devolvido por um servidor substituto;
* mensagens de erro accionáveis.
"""

from __future__ import annotations

import pytest

from messenger import descoberta
from messenger.descoberta import DestinoInvalido, classificar_destino, resolver_ou_usar
from messenger.ipc_client import MODO_DIRETO, MODO_RELAY

# Endereço `.onion` v3 válido: 56 caracteres de base32.
ONION = "a" * 56 + ".onion"
OUTRO_ONION = "b" * 56 + ".onion"
# ID hex de 64 caracteres (32 bytes).
ID = "ab" * 32
OUTRO_ID = "cd" * 32


class ConsultorFalso:
    """Consulta injectável que regista as chamadas.

    Existe para que os testes não precisem de um servidor HTTP e para
    que se possa afirmar *quantas* vezes houve consulta — que é a
    diferença entre «resolver» e «consultar sempre».
    """

    def __init__(self, resposta: str | None = ONION) -> None:
        self.resposta = resposta
        self.chamadas: list[tuple[str, str]] = []

    def __call__(self, servidor: str, identificador: str) -> str | None:
        self.chamadas.append((servidor, identificador))
        return self.resposta

    @property
    def consultou(self) -> bool:
        return bool(self.chamadas)


# ---------------------------------------------------------------------
# classificar_destino
# ---------------------------------------------------------------------


def test_classifica_onion() -> None:
    assert classificar_destino(ONION) == "onion"


def test_classifica_id_hex() -> None:
    assert classificar_destino(ID) == "id"


def test_classifica_vazio() -> None:
    assert classificar_destino("") == "vazio"


def test_id_hex_maiusculas_tambem_e_id() -> None:
    """Um ID colado em maiúsculas é o mesmo ID."""
    assert classificar_destino(ID.upper()) == "id"


@pytest.mark.parametrize(
    "mau",
    [
        "a" * 55 + ".onion",  # etiqueta curta
        "a" * 57 + ".onion",  # etiqueta longa
        "abc.onion",  # curta e com ponto interior
        ".onion",  # sem etiqueta
        "a" * 56,  # sem sufixo
        "a" * 56 + ".ONION",  # sufixo em maiúsculas
    ],
)
def test_onion_malformado_e_recusado(mau: str) -> None:
    with pytest.raises(DestinoInvalido):
        classificar_destino(mau)


@pytest.mark.parametrize(
    "mau",
    [
        "0" * 56 + ".onion",  # 0 e 1 não são base32
        "1" * 56 + ".onion",
        "8" * 56 + ".onion",  # 8 e 9 não são base32
        "9" * 56 + ".onion",
        "+" * 56 + ".onion",  # fora do alfabeto
        "a" * 55 + "b" * 56 + ".onion",  # etiqueta com tamanho errado
    ],
)
def test_onion_com_alfabeto_errado_e_recusado(mau: str) -> None:
    """Base32 v3 é ``a-z`` e ``2-7``; mais nada."""
    with pytest.raises(DestinoInvalido):
        classificar_destino(mau)


@pytest.mark.parametrize(
    "mau",
    [
        "ab" * 31,  # 62 caracteres
        "ab" * 33,  # 66 caracteres
        "g" * 64,  # 'g' não é hex
        "",
    ],
)
def test_id_hex_malformado_ou_vazio_e_recusado(mau: str) -> None:
    # O vazio é tratado à parte (é legal em relay) — o teste acima cobre
    # a classificação; aqui cobrimos o resto.
    if not mau:
        assert classificar_destino(mau) == "vazio"
        return
    with pytest.raises(DestinoInvalido):
        classificar_destino(mau)


def test_erro_de_classificacao_diz_o_que_esperava() -> None:
    """A mensagem tem de dizer as duas formas aceites."""
    with pytest.raises(DestinoInvalido) as info:
        classificar_destino("nada")
    mensagem = str(info.value)
    assert ".onion" in mensagem
    assert "64" in mensagem


# ---------------------------------------------------------------------
# resolver_ou_usar — modo directo
# ---------------------------------------------------------------------


def test_directo_com_onion_passa_tal_e_qual() -> None:
    consultor = ConsultorFalso()
    assert (
        resolver_ou_usar(ONION, MODO_DIRETO, "http://d", consultor) == ONION
    )
    assert not consultor.consultou, "com um .onion não há nada a consultar"


def test_directo_com_id_consulta_e_resolve() -> None:
    """O ID hex é resolvido em ``.onion`` — é para isto que o módulo existe."""
    consultor = ConsultorFalso(OUTRO_ONION)
    resolvido = resolver_ou_usar(ID, MODO_DIRETO, "http://d", consultor)
    assert resolvido == OUTRO_ONION
    assert consultor.chamadas == [("http://d", ID)]


def test_directo_com_id_desconhecido_da_erro_accaoavel() -> None:
    consultor = ConsultorFalso(None)
    with pytest.raises(DestinoInvalido) as info:
        resolver_ou_usar(ID, MODO_DIRETO, "http://d", consultor)
    mensagem = str(info.value)
    assert ID in mensagem, "tem de dizer que ID falhou"
    assert "TTL" in mensagem, "tem de mencionar o TTL"
    assert "relay" in mensagem, "tem de sugerir a alternativa (modo relay)"


def test_directo_revalida_o_onion_do_servidor() -> None:
    """Um discovery substituto pode devolver lixo; o cliente não o aceita.

    Sem esta revalidação, um `.onion` malformado chegaria ao daemon e
    voltaria como um ``0x0D`` indistinguível de uma falha de rede.
    """
    consultor = ConsultorFalso("isto-nao-e-um-onion")
    with pytest.raises(DestinoInvalido) as info:
        resolver_ou_usar(ID, MODO_DIRETO, "http://d", consultor)
    assert "inválido" in str(info.value)


# ---------------------------------------------------------------------
# resolver_ou_usar — modo relay
# ---------------------------------------------------------------------


def test_relay_com_id_passa_tal_e_qual_sem_consultar() -> None:
    """Em relay a mailbox vem da pública no corpo — não há o que resolver.

    E é por isso que a frequência de consultas ao discovery não aumenta
    neste modo (ver ``docs/privacy_model.md`` §4).
    """
    consultor = ConsultorFalso()
    assert resolver_ou_usar(ID, MODO_RELAY, "http://d", consultor) == ID
    assert not consultor.consultou, "modo relay não consulta o discovery"


def test_relay_com_onion_passa_tal_e_qual() -> None:
    consultor = ConsultorFalso()
    assert (
        resolver_ou_usar(ONION, MODO_RELAY, "http://d", consultor) == ONION
    )
    assert not consultor.consultou


def test_relay_aceita_id_maiusculas_sem_consultar() -> None:
    consultor = ConsultorFalso()
    assert (
        resolver_ou_usar(ID.upper(), MODO_RELAY, "http://d", consultor)
        == ID.upper()
    )
    assert not consultor.consultou


# ---------------------------------------------------------------------
# casos partilhados
# ---------------------------------------------------------------------


@pytest.mark.parametrize("modo", [MODO_DIRETO, MODO_RELAY])
def test_destino_vazio_e_erro_em_ambos_os_modos(modo: int) -> None:
    """Vazio não é destino: em directo é obrigatório; em relay também."""
    consultor = ConsultorFalso()
    with pytest.raises(DestinoInvalido):
        resolver_ou_usar("", modo, "http://d", consultor)
    assert not consultor.consultou


@pytest.mark.parametrize("modo", [MODO_DIRETO, MODO_RELAY])
@pytest.mark.parametrize("mau", ["abc.onion", "ab" * 31, "nao-existe"])
def test_destino_malformado_e_erro_sem_consultar(modo: int, mau: str) -> None:
    """Um destino malformado é recusado **antes** de qualquer consulta.

    Consultar o discovery para um destino que já se sabe inválido seria
    uma fuga de metadados inútil: o servidor de discovery ficaria a saber
    um identificador que o cliente sabe que não existe.
    """
    consultor = ConsultorFalso()
    with pytest.raises(DestinoInvalido):
        resolver_ou_usar(mau, modo, "http://d", consultor)
    assert not consultor.consultou, "destino localmente inválido não se consulta"


def test_modo_desconhecido_e_tratado_como_nao_directo() -> None:
    """Um modo fora do conjunto não deve dar origem a uma consulta.

    A CLI valida o modo antes; aqui verificamos que a função não faz
    suposições perigosas se for chamada com um valor inesperado.
    """
    consultor = ConsultorFalso()
    assert resolver_ou_usar(ID, 0x7F, "http://d", consultor) == ID
    assert not consultor.consultou


def test_a_consulta_por_omissao_e_a_do_discovery() -> None:
    """Sem injectar, a função usa a consulta real do discovery."""
    # Só se verifica a ligação ao padrão, sem rede: a assinatura tem de
    # bater com a de `consultar_descoberta`.
    import inspect

    from server.discovery_server import consultar_descoberta

    alvo = descoberta.consultar_descoberta
    assert alvo is consultar_descoberta
    assert list(inspect.signature(alvo).parameters) == ["servidor", "identificador"]


def test_erro_e_valueerror_para_a_cli() -> None:
    """A CLI apanha ``ValueError``; ``DestinoInvalido`` tem de ser um.

    Se não fosse, `onyxchat ligar <destino mau>` levantaria em vez de
    imprimir `onyxchat: …` e sair com código 1.
    """
    assert issubclass(DestinoInvalido, ValueError)
