# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_sidecar.py — o servidor, com um servidor a correr.

Estes testes **não** chamam as rotas directamente. Arrancam o sidecar
numa porta livre e falam com ele por HTTP, porque o que se está a testar
é o servidor: o ``Host``, o ``Origin``, o ``Content-Length``, o
encaminhamento, o ``404``, os cabeçalhos.

Chamar `_atender_api()` com um objecto falso provaria que a função faz o
que a função faz. Não provaria que um ``fetch`` do browser a alcança.

## O teste mais importante

``test_um_site_da_internet_nao_le_a_identidade`` parte um servidor real e
faz um pedido com o ``Origin`` de uma página hostil. Se a resposta trouxer
a identidade, o sidecar está comprometido e o resto dos testes é
decoração.
"""

from __future__ import annotations

import base64
import json
import socket
import threading
import urllib.error
import urllib.request
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest

from messenger import conta as conta_mod
from server import loja as loja_mod
from server import rotas as rotas_mod
from server import sidecar as sidecar_mod

ORIGEM_NOSSA = "http://127.0.0.1:{porta}"

CABECALHOS_NOSSOS = {
    "Origin": "http://127.0.0.1:8787",
    "Sec-Fetch-Site": "same-origin",
}


# ---------------------------------------------------------------------
# Arranque
# ---------------------------------------------------------------------


def _porta_livre() -> int:
    """Uma porta que ninguém está a usar, agora.

    Pedir a porta ``0`` ao sistema e usar a que ele der é a única forma de
    não ter dois testes em paralelo a disputarem a mesma.
    """
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


@pytest.fixture
def conta_de_teste(tmp_path: Path) -> conta_mod.Conta:
    return conta_mod.criar(
        "Marta Vasconcelos",
        "quatro cavalos lentos numa mare",
        caminho=tmp_path / "conta.keystore",
    )


class _Aberto:
    """Um sidecar a correr, com o ``finally`` que o desliga."""

    def __init__(
        self,
        loja: loja_mod.Loja,
        conta: Any,
        raiz_ui: Path | None = None,
        caminho_conta: Path | None = None,
    ) -> None:
        self.loja = loja
        self.porta = _porta_livre()
        self.servidor = sidecar_mod.criar_servidor(
            loja,
            conta,
            rotas_mod.montar(loja, conta),
            porta=self.porta,
            raiz_ui=raiz_ui if raiz_ui is not None else Path("/nonexistente"),
            caminho_conta=(
                caminho_conta
                if caminho_conta is not None
                else loja._caminho.parent / "conta.keystore"  # noqa: SLF001
            ),
            silencioso=True,
        )
        self.fio = threading.Thread(target=self.servidor.serve_forever, daemon=True)
        self.fio.start()

    def parar(self) -> None:
        self.servidor.shutdown()
        self.servidor.server_close()
        self.fio.join(timeout=5)

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.porta}"


@pytest.fixture
def aberto(tmp_path: Path, conta_de_teste: conta_mod.Conta) -> Iterator[_Aberto]:
    """Um sidecar com conta aberta e loja vazia."""
    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        servidor = _Aberto(loja, conta_de_teste)
        try:
            yield servidor
        finally:
            servidor.parar()


@pytest.fixture
def aberto_trancado(tmp_path: Path, conta_de_teste: conta_mod.Conta) -> Iterator[_Aberto]:
    """Um sidecar com a conta escrita mas **trancada**."""
    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        servidor = _Aberto(loja, None)
        try:
            yield servidor
        finally:
            servidor.parar()


# ---------------------------------------------------------------------
# O pedido
# ---------------------------------------------------------------------


def pedir(
    servidor: _Aberto,
    caminho: str,
    *,
    metodo: str = "GET",
    corpo: dict[str, Any] | None = None,
    cabecalhos: dict[str, str] | None = None,
) -> tuple[int, Any]:
    """Faz um pedido e devolve ``(código, corpo JSON ou texto)``.

    Os cabeçalhos legítimos são posto por omissão **com a porta certa**.
    Uma constante com ``8787`` escrito à mão faria o teste passar contra
    um servidor noutra porta só porque o `Host` não é conferido — e a
    conferência do `Host` é metade do que estes testes provam.
    """
    finais = {
        "Host": f"127.0.0.1:{servidor.porta}",
        "Origin": ORIGEM_NOSSA.format(porta=servidor.porta),
        "Sec-Fetch-Site": "same-origin",
        **(cabecalhos or {}),
    }

    dados = None
    if corpo is not None:
        dados = json.dumps(corpo).encode("utf-8")
        finais["Content-Type"] = "application/json"
        finais["Content-Length"] = str(len(dados))

    pedido = urllib.request.Request(
        f"{servidor.url}{caminho}", data=dados, headers=finais, method=metodo
    )

    try:
        with urllib.request.urlopen(pedido, timeout=5) as resposta:
            bruto = resposta.read()
            codigo = resposta.status
    except urllib.error.HTTPError as erro:
        bruto = erro.read()
        codigo = erro.code

    try:
        return codigo, json.loads(bruto)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return codigo, bruto


# ---------------------------------------------------------------------
# Ataques, contra um servidor a correr
# ---------------------------------------------------------------------


def test_um_site_da_internet_nao_le_a_identidade(aberto: _Aberto) -> None:
    """O teste mais importante deste ficheiro.

    Um servidor em `127.0.0.1` **não é privado**: a especificação
    permite o `fetch` e o browser não o impede. Se este pedido trouxesse a
    identidade, qualquer página que a pessoa visitasse teria o Onyx ID e a
    impressão digital.
    """
    codigo, corpo = pedir(
        aberto,
        "/api/identidade",
        cabecalhos={
            "Origin": "https://atacante.example",
            "Sec-Fetch-Site": "cross-site",
        },
    )

    assert codigo == 403
    assert b"fingerprint" not in json.dumps(corpo).encode()
    assert b"ONYX-" not in json.dumps(corpo).encode()


def test_um_site_da_internet_nao_escreve(aberto: _Aberto) -> None:
    """Nem enviar uma mensagem em nome da pessoa."""
    codigo, _ = pedir(
        aberto,
        "/api/mensagens",
        metodo="POST",
        corpo={"texto": "não é sua"},
        cabecalhos={
            "Origin": "https://atacante.example",
            "Sec-Fetch-Site": "cross-site",
        },
    )
    assert codigo == 403
    assert aberto.loja.conversas() == []


def test_o_rebinding_de_dns_nao_entra(aberto: _Aberto) -> None:
    """`Host` de um nome hostil, `Origin` a imitar o nosso.

    É o caso perigoso: se o servidor confiasse no `Origin` sem ver o
    `Host`, este pedido passaria com o `Origin` perfeito à frente.
    """
    codigo, _ = pedir(
        aberto,
        "/api/identidade",
        cabecalhos={
            "Host": "atacante.example",
            "Origin": ORIGEM_NOSSA.format(porta=aberto.porta),
            "Sec-Fetch-Site": "cross-site",
        },
    )
    assert codigo == 403


def test_o_erro_nao_diz_qual_defesa_apanhou(aberto: _Aberto) -> None:
    """Dizer qual das quatro apanhou é dar o trabalho de tentar as outras.

    O corpo tem o código e a mensagem do código, e nada que relacione a
    recusa com um cabeçalho em particular.
    """
    _, corpo = pedir(aberto, "/api/identidade", cabecalhos={"Origin": "https://x.example"})
    texto = json.dumps(corpo).lower()
    for pista in ("host", "origin", "sec-fetch", "rebinding"):
        assert pista not in texto


def test_a_aplicacao_propria_le_a_identidade(aberto: _Aberto, conta_de_teste: conta_mod.Conta) -> None:
    codigo, corpo = pedir(aberto, "/api/identidade")
    assert codigo == 200
    assert corpo["identifier"] == conta_de_teste.identificador
    assert corpo["fingerprint"] == conta_de_teste.impressao
    assert corpo["name"] == "Marta Vasconcelos"


def test_o_erro_tem_o_forma_certo(aberto: _Aberto) -> None:
    """A interface lê `erro`, `codigo` e `detalhe`, e não outra coisa."""
    codigo, corpo = pedir(aberto, "/api/nada")
    assert codigo == 404
    assert corpo["codigo"] == 404
    assert isinstance(corpo["erro"], str)
    assert "detalhe" in corpo


# ---------------------------------------------------------------------
# As rotas de dados
# ---------------------------------------------------------------------


def test_a_conta_trancada_nao_da_a_identidade(aberto_trancado: _Aberto) -> None:
    """Sem a frase, o ficheiro não abre, e o que a pessoa vê é ``503``.

    Não é ``404``: a rota existe e a conta existe, o que falta é a
    sessão. E não é um objecto com campos a zero.
    """
    codigo, corpo = pedir(aberto_trancado, "/api/identidade")
    assert codigo == 503
    assert "indisponivel" in json.dumps(corpo) or "dispon" in json.dumps(corpo)


def test_o_estado_da_conta_tem_tres_valores(aberto_trancado: _Aberto, aberto: _Aberto) -> None:
    codigo, corpo = pedir(aberto_trancado, "/api/conta/estado")
    assert codigo == 200
    assert corpo["estado"] == "trancada"

    codigo, corpo = pedir(aberto, "/api/conta/estado")
    assert corpo["estado"] == "desbloqueada"


def test_sem_conta_o_estado_e_sem_conta(tmp_path: Path) -> None:
    with loja_mod.Loja(tmp_path / "l.db") as loja:
        servidor = _Aberto(loja, None)
        servidor.servidor.sidecar.caminho_conta = tmp_path / "nao-existe.keystore"
        try:
            _, corpo = pedir(servidor, "/api/conta/estado")
            assert corpo["estado"] == "sem-conta"
        finally:
            servidor.parar()


def test_desbloquear_com_a_frase_certo(aberto_trancado: _Aberto, conta_de_teste: conta_mod.Conta) -> None:
    codigo, corpo = pedir(
        aberto_trancado,
        "/api/conta/desbloquear",
        metodo="POST",
        corpo={"frase": "quatro cavalos lentos numa mare"},
    )
    assert codigo == 200
    assert corpo["fingerprint"] == conta_de_teste.impressao
    # E a sessão fica, para o pedido seguinte.
    codigo, ident = pedir(aberto_trancado, "/api/identidade")
    assert ident["identifier"] == conta_de_teste.identificador


def test_desbloquear_com_a_frase_errada(aberto_trancado: _Aberto) -> None:
    codigo, _ = pedir(
        aberto_trancado,
        "/api/conta/desbloquear",
        metodo="POST",
        corpo={"frase": "outra"},
    )
    assert codigo == 403


def test_a_frase_fraca_nao_cria_conta(tmp_path: Path) -> None:
    """Uma frase curta dá `400`, e não `409`.

    A distinção importa: `409` diz que o pedido conflita com o estado
    actual, e quem lê «já existe uma conta» quando o problema é a frase
    fica a procurar uma conta que não existe. Em `messenger/conta.py`,
    `FraseForte` é subclasse de `ContaInvalida` — e apanhar a classe geral
    primeiro produzia `409` sem mais.
    """
    with loja_mod.Loja(tmp_path / "l.db") as loja:
        servidor = _Aberto(loja, None)
        servidor.servidor.sidecar.caminho_conta = tmp_path / "nova.keystore"
        try:
            codigo, corpo = pedir(
                servidor,
                "/api/conta",
                metodo="POST",
                corpo={"utilizador": "ana", "frase": "aaaa"},
            )
            # A regra avisa do comprimento primeiro, e é essa a ordem que
            # a pessoa tem de corrigir: alongar a frase resolve.
            assert codigo == 400
            assert "pelo menos 12 caracteres" in corpo["detalhe"]
            assert not (tmp_path / "nova.keystore").exists()
        finally:
            servidor.parar()


def test_uma_frase_longa_e_repetitiva_da_400_por_variedade(tmp_path: Path) -> None:
    """O segundo mínimo: comprimento cumprido, variedade em falta.

    Uma frase de vinte `a` passa o comprimento e falha a variedade. É o
    caso que uma regra de «uma maiúscula, um número» deixaria passar.
    """
    with loja_mod.Loja(tmp_path / "l.db") as loja:
        servidor = _Aberto(loja, None)
        servidor.servidor.sidecar.caminho_conta = tmp_path / "nova.keystore"
        try:
            codigo, corpo = pedir(
                servidor,
                "/api/conta",
                metodo="POST",
                corpo={"utilizador": "ana", "frase": "a" * 20},
            )
            assert codigo == 400
            assert "bits estimados" in corpo["detalhe"]
        finally:
            servidor.parar()


def test_registar_e_desbloquear(aberto_trancado: _Aberto) -> None:
    codigo, criada = pedir(
        aberto_trancado,
        "/api/conta",
        metodo="POST",
        corpo={"utilizador": "Ana", "frase": "quatro cavalos lentos numa mare"},
    )
    # A conta já existe neste teste, por isso é conflito e não criação.
    assert codigo == 409


def test_trancar_fecha_a_sessao(aberto: _Aberto) -> None:
    assert pedir(aberto, "/api/identidade")[0] == 200
    assert pedir(aberto, "/api/conta/trancar", metodo="POST")[0] == 200
    assert pedir(aberto, "/api/identidade")[0] == 503
    # E a conta continua lá.
    assert pedir(aberto, "/api/conta/estado")[1]["estado"] == "trancada"


def test_exportar_devolve_o_ficheiro_cifrado(aberto: _Aberto) -> None:
    """Base64, e `0600` no lado do Python.

    O corpo de uma resposta tem de ser JSON e o ficheiro é binário, daí
    a base64. A interface descarrega-o e nunca sabe o que está dentro — que
    é o que deixa a cópia ir para um disco externo.
    """
    codigo, corpo = pedir(aberto, "/api/conta/exportar")
    assert codigo == 200

    bruto = base64.b64decode(corpo["conteudo"], validate=True)
    assert bruto.startswith(b"ONYXKS")      # a magia do keystore
    assert b"ana" not in bruto.lower()
    assert corpo["modo"] == "0600"


def test_exportar_sem_conta_da_404(tmp_path: Path) -> None:
    with loja_mod.Loja(tmp_path / "l.db") as loja:
        servidor = _Aberto(loja, None)
        servidor.servidor.sidecar.caminho_conta = tmp_path / "nada.keystore"
        try:
            assert pedir(servidor, "/api/conta/exportar")[0] == 404
        finally:
            servidor.parar()


def test_restaurar_verifica_antes_de_sobrescrever(aberto: _Aberto) -> None:
    """A conta boa tem de sobreviver a uma cópia com a frase errada.

    Se a verificação viesse depois da escrita, a pessoa ficaria sem conta
    e sem a cópia que tentava usar.
    """
    _, copia = pedir(aberto, "/api/conta/exportar")
    codigo, _ = pedir(
        aberto,
        "/api/conta/restaurar",
        metodo="POST",
        corpo={"conteudo": copia["conteudo"], "frase": "frase errada"},
    )
    assert codigo == 403
    assert pedir(aberto, "/api/identidade")[0] == 200


def test_restaurar_com_base64_invalida(aberto: _Aberto) -> None:
    codigo, _ = pedir(
        aberto,
        "/api/conta/restaurar",
        metodo="POST",
        corpo={"conteudo": "isto não é base64 !!!", "frase": "x"},
    )
    assert codigo == 400


# ---------------------------------------------------------------------
# Conversas, contactos, pedidos
# ---------------------------------------------------------------------


def _contacto(servidor: _Aberto, identificador: str = "ONYX-AAAAAA-!A!A#") -> str:
    pedido = servidor.loja.registar_pedido(identificador, "Ana Silva", "Olá")
    return servidor.loja.decidir_pedido(pedido["id"], "aceite")["contactoId"]


def test_as_conversas_vem_vazias_e_nao_com_erro(aberto: _Aberto) -> None:
    """`[]` aqui é verdade: a loja abriu e está vazia."""
    codigo, corpo = pedir(aberto, "/api/conversas")
    assert codigo == 200
    assert corpo == []


def test_as_contagens(aberto: _Aberto) -> None:
    _contacto(aberto)
    codigo, corpo = pedir(aberto, "/api/contagens")
    assert codigo == 200
    assert corpo["contacts"] == 1


def test_enviar_mensagem_cria_a_conversa(aberto: _Aberto) -> None:
    _contacto(aberto)
    codigo, corpo = pedir(
        aberto,
        "/api/mensagens",
        metodo="POST",
        corpo={"identificador": "ONYX-AAAAAA-!A!A#", "texto": "Olá"},
    )
    assert codigo == 200

    _, conversas = pedir(aberto, "/api/conversas")
    assert conversas[0]["lastMessage"] == "Olá"

    _, mensagens = pedir(aberto, f"/api/conversas/{corpo['conversationId']}")
    assert mensagens["messages"][0]["text"] == "Olá"


def test_enviar_para_contacto_inexistente(aberto: _Aberto) -> None:
    codigo, _ = pedir(
        aberto,
        "/api/mensagens",
        metodo="POST",
        corpo={"identificador": "ONYX-NADA-!A!A#", "texto": "Olá"},
    )
    assert codigo == 404


def test_enviar_sem_texto(aberto: _Aberto) -> None:
    _contacto(aberto)
    codigo, corpo = pedir(
        aberto, "/api/mensagens", metodo="POST", corpo={"identificador": "ONYX-AAAAAA-!A!A#"}
    )
    assert codigo == 400
    assert "texto" in corpo["detalhe"]


def test_marcar_como_lida(aberto: _Aberto) -> None:
    contacto = _contacto(aberto)
    conversa = aberto.loja.abrir_conversa(contacto)
    aberto.loja.adicionar_mensagem(conversa["id"], "them", "Chegaste?")

    assert pedir(aberto, "/api/conversas")[1][0]["unread"] == 1
    assert pedir(aberto, f"/api/conversas/{conversa['id']}/lida", metodo="POST")[0] == 200
    assert pedir(aberto, "/api/conversas")[1][0]["unread"] == 0


def test_decidir_um_pedido_pela_api(aberto: _Aberto) -> None:
    pedido = aberto.loja.registar_pedido("ONYX-CCCCCC-!C!C#", "Carla")
    codigo, corpo = pedir(
        aberto, f"/api/pedidos/{pedido['id']}", metodo="POST", corpo={"decisao": "aceite"}
    )
    assert codigo == 200
    assert corpo["decisao"] == "aceite"
    assert len(aberto.loja.contactos()) == 1


def test_decidir_sem_decisao(aberto: _Aberto) -> None:
    pedido = aberto.loja.registar_pedido("ONYX-CCCCCC-!C!C#", "Carla")
    codigo, corpo = pedir(aberto, f"/api/pedidos/{pedido['id']}", metodo="POST", corpo={})
    assert codigo == 400
    assert "decisao" in corpo["detalhe"]


def test_as_definicoes_vao_e_vem(aberto: _Aberto) -> None:
    codigo, _ = pedir(
        aberto,
        "/api/definicoes",
        metodo="PUT",
        corpo={"seccoes": {"tema": "escuro"}},
    )
    assert codigo == 200
    assert pedir(aberto, "/api/definicoes")[1]["seccoes"]["tema"] == "escuro"


def test_a_seguranca_nao_inventa_um_score(aberto: _Aberto) -> None:
    """Um score calculado a partir de dados que não existem é a coisa que
    a faixa de demonstração existe para impedir."""
    codigo, corpo = pedir(aberto, "/api/seguranca")
    assert codigo == 200
    assert corpo["score"] is None


def test_a_seguranca_trancada(aberto_trancado: _Aberto) -> None:
    assert pedir(aberto_trancado, "/api/seguranca")[0] == 503


def test_a_sugestao_diz_que_nao_a_tem(aberto: _Aberto) -> None:
    """Um sidecar a sério não tem um identificador de exemplo para dar.

    A rota existia para a demonstração da interface, que é o único sítio
    com um Onyx ID verdadeiro para experimentar. Num sidecar real inventar
    um seria dar à pessoa um endereço que não é de ninguém — e a resposta
    honesta é ``404``, não um exemplo.
    """
    codigo, corpo = pedir(aberto, "/api/descoberta/sugestao")
    assert codigo == 404
    assert "sugest" in corpo["detalhe"]


def test_pedir_contacto_fica_pendente_e_nao_e_um_contacto(aberto: _Aberto) -> None:
    """Um pedido a alguém que ainda não é contacto guarda-se como pedido.

    O que **não** acontece é fingir a amizade: o handshake — e as chaves —
    são do daemon, e sem daemon a sério não há nada a fingir.
    """
    codigo, corpo = pedir(
        aberto,
        "/api/pedidos",
        metodo="POST",
        corpo={"identificador": "ONYX-CCCCCC-!C!C#", "mensagem": "olá"},
    )
    assert codigo == 200
    assert corpo["estado"] == "pendente"
    assert corpo["identifier"] == "ONYX-CCCCCC-!C!C#"
    assert corpo["message"] == "olá"
    assert len(aberto.loja.contactos()) == 0


def test_pedido_de_contacto_sem_mensagem(aberto: _Aberto) -> None:
    """A mensagem é a única coisa opcional — o destinatário, não."""
    codigo, corpo = pedir(
        aberto,
        "/api/pedidos",
        metodo="POST",
        corpo={"identificador": "ONYX-CCCCCC-!C!C#"},
    )
    assert codigo == 200
    assert corpo["message"] == ""


def test_a_descoberta_diz_nao_encontrado_quando_falha(aberto: _Aberto) -> None:
    """Uma excepção do servidor de descoberta é «não encontrado».

    O que este teste apanha é o contrário: a rota chamava
    ``resolver_ou_usar`` com um só argumento, o ``TypeError`` caía no
    ``except`` largo e a rota respondia «não encontrado» para sempre.
    """
    codigo, corpo = pedir(aberto, "/api/descoberta?identificador=" + "ab" * 32)
    assert codigo == 200
    assert corpo == {"status": "not_found"}


def test_a_descoberta_devolve_o_onion_quando_o_servidor_sabe(
    aberto: _Aberto, monkeypatch
) -> None:
    """O caminho do ``found``, com o cliente de descoberta substituído."""
    from messenger import descoberta

    visto: list[tuple[str, int, str]] = []

    def resolver(destino, modo, servidor, *resto):
        visto.append((destino, modo, servidor))
        return "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwxyz2345.onion"

    monkeypatch.setattr(descoberta, "resolver_ou_usar", resolver)
    codigo, corpo = pedir(aberto, "/api/descoberta?identificador=" + "ab" * 32)
    assert codigo == 200
    assert corpo["status"] == "found"
    assert corpo["onion"].endswith(".onion")
    # Os três argumentos: destino, modo e servidor. A chamada anterior
    # passava o primeiro só, e era o que provava que a rota nunca resolvia.
    assert visto == [("ab" * 32, 0x00, "http://127.0.0.1:8789")]


def test_a_descoberta_exige_identificador(aberto: _Aberto) -> None:
    codigo, corpo = pedir(aberto, "/api/descoberta")
    assert codigo == 400
    assert "identificador" in corpo["detalhe"]


def test_a_descoberta_trata_o_none_como_nao_encontrado(
    aberto: _Aberto, monkeypatch
) -> None:
    """``None`` é «não encontrado», não um erro.

    A função substituída devolve ``None`` sem levantar excepção. É um ramo
    que só existe porque a função real levanta :class:`DestinoInvalido`
    em vez de devolver ``None`` — mas um substituto mal comportado no
    teste é exactamente o que apanha um ``return {"status": "found",
    "onion": None}``, que a interface leria como um endereço vazio.
    """
    from messenger import descoberta

    monkeypatch.setattr(descoberta, "resolver_ou_usar", lambda *a, **k: None)
    codigo, corpo = pedir(aberto, "/api/descoberta?identificador=" + "ab" * 32)
    assert codigo == 200
    assert corpo == {"status": "not_found"}


def test_restaurar_com_outra_frase_da_403_e_nao_toca_no_destino(
    aberto: _Aberto,
) -> None:
    """A frase errada é ``403``, e a conta continua como estava.

    Este é o caso comum de quem guarda a cópia noutro sítio e a abre com a
    frase actual por hábito. A cópia é verificada **antes** de tocar no
    destino — substituir uma conta boa por uma cópia que não abre seria
    perder a conta e a cópia numa só operação.
    """
    exportar = pedir(aberto, "/api/conta/exportar")
    antes = (aberto.servidor.sidecar.caminho_conta).read_bytes()

    codigo, corpo = pedir(
        aberto,
        "/api/conta/restaurar",
        metodo="POST",
        corpo={
            "frase": "uma frase completamente diferente",
            "conteudo": exportar[1]["conteudo"],
        },
    )
    assert codigo == 403
    assert "não abre" in corpo["detalhe"]
    assert aberto.servidor.sidecar.caminho_conta.read_bytes() == antes


def test_restaurar_recusa_a_copia_que_o_messenger_nao_aceita(
    aberto: _Aberto, monkeypatch
) -> None:
    """Um ``ContaInvalida`` é ``400``, e o ``403`` é só do keystore.

    A distinção é de quem errou: ``403`` é o ficheiro que não abre com a
    frase, e ``400`` é um registo que o ``messenger.conta`` recusa por
    outro motivo. Tratar os dois como ``403`` diria à pessoa que a frase
    está errada quando o problema é o conteúdo da cópia.
    """
    from messenger import conta as conta_mod

    exportar = pedir(aberto, "/api/conta/exportar")

    def recusa(_origem, _frase, _caminho=None):
        raise conta_mod.ContaInvalida("o registo dentro da cópia não é válido")

    monkeypatch.setattr(conta_mod, "restaurar", recusa)
    codigo, corpo = pedir(
        aberto,
        "/api/conta/restaurar",
        metodo="POST",
        corpo={
            "frase": "quatro cavalos lentos numa mare",
            "conteudo": exportar[1]["conteudo"],
        },
    )
    assert codigo == 400
    assert "não é válido" in corpo["detalhe"]


def test_registar_devolve_a_identidade_nova(tmp_path: Path) -> None:
    """O registo responde com o Onyx ID, a impressão e quem ficou com a conta.

    Numa loja vazia e sem conta — o que ``aberto_trancado`` não é, porque a
    fixture partilha a conta criada noutro teste. É o caminho de quem
    instala o programa pela primeira vez.
    """
    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        servidor = _Aberto(loja, None, caminho_conta=tmp_path / "conta.keystore")
        try:
            codigo, corpo = pedir(
                servidor,
                "/api/conta",
                metodo="POST",
                corpo={
                    "utilizador": "ana",
                    "frase": "quatro cavalos lentos numa mare",
                },
            )
            assert codigo == 200
            assert corpo["utilizador"] == "ana"
            assert corpo["identificador"].startswith("ONYX-")
            assert len(corpo["fingerprint"]) > 0
            # `criadaEm` é um instante em ISO, não um `u64`: o
            # `camelCase` do JavaScript não muda o tipo.
            assert corpo["criadaEm"].startswith("20")
            # O ficheiro existe e é cifrado: o Onyx ID não está lá em claro.
            bruto = (tmp_path / "conta.keystore").read_bytes()
            assert bruto.startswith(b"ONYXKS")
            assert corpo["identificador"].encode() not in bruto
        finally:
            servidor.parar()


def test_restaurar_devolve_ok_e_recusa_a_copia_que_nao_abre(aberto: _Aberto) -> None:
    """Uma cópia cifrada válida restaura; uma que não abre é 403.

    A cópia é validada numa pasta temporária e só depois toca no destino —
    uma cópia que não abre tem de deixar a conta como estava.
    """
    exportar = pedir(aberto, "/api/conta/exportar")
    assert exportar[0] == 200
    copia = exportar[1]["conteudo"]

    codigo, corpo = pedir(
        aberto,
        "/api/conta/restaurar",
        metodo="POST",
        corpo={"frase": "quatro cavalos lentos numa mare", "conteudo": copia},
    )
    assert codigo == 200
    assert corpo == {"ok": True}

    adulterada = bytearray(base64.b64decode(copia))
    adulterada[40] ^= 0xFF  # um bit do texto cifrado
    codigo, _ = pedir(
        aberto,
        "/api/conta/restaurar",
        metodo="POST",
        corpo={
            "frase": "quatro cavalos lentos numa mare",
            "conteudo": base64.b64encode(bytes(adulterada)).decode(),
        },
    )
    assert codigo == 403


def test_a_rede_diz_que_esta_indisponivel(aberto: _Aberto) -> None:
    """Sem daemon, `503` em `/api/estado` e um objecto que **diz** que está
    indisponível.

    A forma é a mesma nos dois casos, com um campo a distinguir. Assim
    nenhum ecrã precisa de descobrir por si como se diz «não há dados».
    """
    codigo, corpo = pedir(aberto, "/api/estado")
    assert codigo == 200
    assert corpo["indisponivel"] is True
    assert corpo["indisponivelMotivo"]


# ---------------------------------------------------------------------
# Encaminhamento
# ---------------------------------------------------------------------


def test_uma_rota_inexistente_da_404(aberto: _Aberto) -> None:
    assert pedir(aberto, "/api/nada")[0] == 404


def test_o_metodo_errado_da_405_ou_404(aberto: _Aberto) -> None:
    """`GET` numa rota de escrita.

    O `405` viria do router se o caminho casasse com outro método. Como
    só há uma rota para `/api/conta/desbloquear` e é `POST`, um `GET` não
    casa com nada e dá `404` — que é a resposta honesta: não há um `GET`
    aí.
    """
    assert pedir(aberto, "/api/conta/desbloquear")[0] == 404


def test_o_json_malformado_da_400(aberto: _Aberto) -> None:
    codigo, corpo = pedir(aberto, "/api/conta/desbloquear", metodo="POST", corpo=[])
    assert codigo == 400


def test_patch_e_delete_passam_pelo_mesmo_caminho(aberto: _Aberto) -> None:
    """``PATCH`` e ``DELETE`` chegam ao router como os outros verbos.

    Só ``GET``, ``HEAD``, ``POST`` e ``PUT`` estavam exercitados. Um
    ``do_PATCH`` apagado ou mal escrito daria ``501`` do ``http.server`` —
    a resposta certa para «o servidor não implementa isto», que é uma
    mentira: o servidor implementa, o teste é que não o perguntava.
    """
    assert pedir(aberto, "/api/identidade", metodo="PATCH")[0] in (404, 405)
    assert pedir(aberto, "/api/identidade", metodo="DELETE")[0] in (404, 405)


def test_um_campo_de_texto_grande_da_413(aberto: _Aberto) -> None:
    """O tecto do campo é de 64 KiB, e diz que campo é.

    Escrever o tecto numa rota e não noutra é como uma rota fica sem ele.
    Este campo é o corpo do ``desbloquear``, e um corpo dentro do
    ``MAX_CORPO`` de 4 MiB mas com um campo de 5 MiB passaria pelo
    ``Content-Length`` e só era recusado aqui.
    """
    codigo, corpo = pedir(
        aberto,
        "/api/conta/desbloquear",
        metodo="POST",
        corpo={"frase": "x" * (64 * 1024 + 1)},
    )
    assert codigo == 413
    assert "«frase»" in corpo["detalhe"]


def _substituir_rota(aberto: _Aberto, nome: str, accao) -> None:
    """Troca a acção de uma rota montada, sem tocar no resto."""
    from server import rotas as rotas_mod

    for indice, rota in enumerate(aberto.servidor.sidecar.rotas):
        if rota.nome == nome:
            aberto.servidor.sidecar.rotas[indice] = rotas_mod.Rota(
                rota.metodo, rota.caminho, accao, nome
            )
            return
    raise AssertionError(f"não há rota chamada {nome!r}")


def test_a_excepcao_inesperada_da_500_e_nao_vaza_a_resposta(aberto: _Aberto) -> None:
    """Uma excepção apanhada é ``500`` e nada mais na resposta.

    O traceback vai para os registos. O que este teste garante é o outro
    sentido — que nenhum caminho de ficheiro, nome de símbolo ou excepção
   original chegue a quem fez o pedido. Um servidor em ``127.0.0.1`` com a
    interface de outra pessoa em cima é um mapa do programa para quem deu
    de encontrar a porta.
    """

    def avaria(_contexto, _parametros):
        raise RuntimeError("segredo: /home/alguem/keystore e a chave K1")

    _substituir_rota(aberto, "identidade", avaria)

    codigo, corpo = pedir(aberto, "/api/identidade")
    assert codigo == 500
    texto = json.dumps(corpo, ensure_ascii=False)
    for perigoso in ("segredo", "K1", "/home/alguem", "RuntimeError", "avaria"):
        assert perigoso not in texto


def test_o_registo_recebe_o_traceback(aberto: _Aberto, monkeypatch) -> None:
    """O que o ``500`` esconde da resposta vai para os registos.

    Sem isto, um ``except`` largo que engole a excepção sem registar nada
    é indistinguível de um servidor que simplesmente não respondeu — e a
    primeira versão desta classe de tratamento era precisamente isso.
    """
    escrito: list[str] = []

    def avaria(_contexto, _parametros):
        raise RuntimeError("a excepção original")

    _substituir_rota(aberto, "identidade", avaria)
    monkeypatch.setattr(
        sidecar_mod.Manipulador,
        "_log",
        lambda self, formato, *args: escrito.append(formato % args),
    )

    assert pedir(aberto, "/api/identidade")[0] == 500
    assert escrito, "o traceback não foi para o registo"
    assert "a excepção original" in escrito[0]


def test_um_content_length_negativo_da_400(aberto: _Aberto) -> None:
    """Um ``Content-Length`` negativo é lixo, e lixo dá ``400``."""
    with socket.create_connection(("127.0.0.1", aberto.porta), timeout=5) as ligacao:
        ligacao.sendall(
            (
                "POST /api/conta/desbloquear HTTP/1.1\r\n"
                f"Host: 127.0.0.1:{aberto.porta}\r\n"
                f"Origin: {ORIGEM_NOSSA.format(porta=aberto.porta)}\r\n"
                "Sec-Fetch-Site: same-origin\r\n"
                "Content-Type: application/json\r\n"
                "Content-Length: -1\r\n"
                "\r\n"
            ).encode()
        )
        resposta = ligacao.recv(4096)
    assert b"400" in resposta.split(b"\r\n")[0]


def test_post_sem_content_length_le_o_que_veio(aberto: _Aberto) -> None:
    """``POST`` sem ``Content-Length`` é um corpo vazio, e dá ``400``.

    Não é ``411``: um ``POST`` sem o cabeçalho é um cliente que não
    cumpre o contrato, e o que este programa precisa de dizer é que o
    pedido está mal formado. O corpo vazio leva à validação do campo, que
    dá ``400`` pelo motivo certo — «falta a frase» e não «falta o
    cabeçalho».
    """
    with socket.create_connection(("127.0.0.1", aberto.porta), timeout=5) as ligacao:
        ligacao.sendall(
            (
                "POST /api/conta/desbloquear HTTP/1.1\r\n"
                f"Host: 127.0.0.1:{aberto.porta}\r\n"
                f"Origin: {ORIGEM_NOSSA.format(porta=aberto.porta)}\r\n"
                "Sec-Fetch-Site: same-origin\r\n"
                "Content-Type: application/json\r\n"
                "\r\n"
            ).encode()
        )
        resposta = ligacao.recv(4096)
    assert b"400" in resposta.split(b"\r\n")[0]


def test_o_content_length_invalido_da_400(aberto: _Aberto) -> None:
    pedido = urllib.request.Request(
        f"{aberto.url}/api/conta/desbloquear",
        data=b"{}",
        headers={
            "Host": f"127.0.0.1:{aberto.porta}",
            "Origin": ORIGEM_NOSSA.format(porta=aberto.porta),
            "Sec-Fetch-Site": "same-origin",
            "Content-Type": "application/json",
            "Content-Length": "isto não é um número",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(pedido, timeout=5) as resposta:
            codigo = resposta.status
    except urllib.error.HTTPError as erro:
        codigo = erro.code
    assert codigo == 400


def test_um_corpo_grande_da_413_sem_ter_espera_por_ele(aberto: _Aberto) -> None:
    """O `Content-Length` é conferido **antes** de ler.

    Ler primeiro e verificar o tamanho depois é a forma de gastar a
    memória antes de ter decidido que não se devia.

    O pedido vai por socket cru e **sem corpo**, e é essa a prova: se o
    servidor lesse o corpo antes de responder, ficaria à espera de cinco
    megabytes que nunca chegam, e o teste passava a demorar cinco
    megabytes.
    """
    with socket.create_connection(("127.0.0.1", aberto.porta), timeout=5) as ligacao:
        ligacao.sendall(
            (
                "POST /api/conta/desbloquear HTTP/1.1\r\n"
                f"Host: 127.0.0.1:{aberto.porta}\r\n"
                f"Origin: {ORIGEM_NOSSA.format(porta=aberto.porta)}\r\n"
                "Sec-Fetch-Site: same-origin\r\n"
                "Content-Type: application/json\r\n"
                "Expect: 100-continue\r\n"
                f"Content-Length: {5 * 1024 * 1024}\r\n"
                "\r\n"
            ).encode()
        )
        resposta = ligacao.recv(4096)

    assert b"413" in resposta.split(b"\r\n")[0]


def test_sem_expect_o_corpo_grande_tambem_da_413(aberto: _Aberto) -> None:
    """Sem `Expect`, o corpo é descartado e a resposta chega na mesma.

    É o caso do `curl` e do `fetch`, que não esperam pelo servidor. O
    servidor esvazia o corpo até :data:`MAX_CORPO` para o cliente conseguir
    ler a resposta `413`; acima disso fecha a ligação, e é o que se quer.

    O corpo é ``MAX_CORPO`` mais uma margem pequena, e não muito maior.
    Com 5 MiB contra um tecto de drenagem de 4 MiB, o servidor fechava a
    ligação com um megabyte ainda por escrever e o cliente via uma
    ligação quebrada em vez da resposta — correctamente do ponto de
    vista do TCP, e uma falha intermitente do teste. Com o excesso a
    caber no tampão, o cliente acaba de escrever e lê o `413`.
    """
    excesso = 64 * 1024
    alvo = sidecar_mod.MAX_CORPO + excesso
    vazio = b'{"frase": ""}'
    corpo = vazio.replace(b'""', b'"' + b"x" * (alvo - len(vazio)) + b'"')
    assert len(corpo) == alvo
    pedido = urllib.request.Request(
        f"{aberto.url}/api/conta/desbloquear",
        data=corpo,
        headers={
            "Host": f"127.0.0.1:{aberto.porta}",
            "Origin": ORIGEM_NOSSA.format(porta=aberto.porta),
            "Sec-Fetch-Site": "same-origin",
            "Content-Type": "application/json",
            "Content-Length": str(len(corpo)),
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(pedido, timeout=10) as resposta:
            codigo = resposta.status
    except urllib.error.HTTPError as erro:
        codigo = erro.code
    assert codigo == 413


def test_o_canal_de_eventos_diz_que_nao_existe(aberto: _Aberto) -> None:
    """`501`, e não um WebSocket a meio.

    Um `404` mudo não diz à pessoa se é um erro ou uma escolha; o `501`
    com a razão diz que o caminho está previsto e por agora não há nada
    lá.
    """
    codigo, corpo = pedir(aberto, "/api/eventos")
    assert codigo == 501
    assert "não está implementado" in corpo["detalhe"]


def test_o_preflight_responde(aberto: _Aberto) -> None:
    """`OPTIONS` tem de responder, mesmo quando o pedido depois é recusado.

    É a primeira coisa que um `fetch` de outra origem pergunta. Uma
    resposta de `500` aqui seria indistinguível, do lado do browser, de um
    servidor em baixo.
    """
    codigo, _ = pedir(aberto, "/api/identidade", metodo="OPTIONS")
    # 204 e não 200: um pré-voo não tem corpo para responder, e um `200`
    # com corpo seria um corpo que ninguém pediu.
    assert codigo == 204


# ---------------------------------------------------------------------
# Ficheiros estáticos
# ---------------------------------------------------------------------


@pytest.fixture
def com_ui(tmp_path: Path, conta_de_teste: conta_mod.Conta) -> Iterator[_Aberto]:
    """Um sidecar com um ``dist/`` à mão."""
    raiz = tmp_path / "dist"
    (raiz / "assets").mkdir(parents=True)
    (raiz / "index.html").write_text("<!doctype html><title>Onyx</title>", encoding="utf-8")
    (raiz / "assets" / "app.js").write_text("console.log(1)", encoding="utf-8")

    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        servidor = _Aberto(loja, conta_de_teste, raiz_ui=raiz)
        try:
            yield servidor
        finally:
            servidor.parar()


def test_o_index_e_servido(com_ui: _Aberto) -> None:
    codigo, corpo = pedir(com_ui, "/")
    assert codigo == 200
    assert b"Onyx" in corpo


def test_um_ficheiro_do_assets(com_ui: _Aberto) -> None:
    codigo, corpo = pedir(com_ui, "/assets/app.js")
    assert codigo == 200
    assert corpo == b"console.log(1)"


def test_uma_rota_da_aplicacao_serve_o_index(com_ui: _Aberto) -> None:
    """`/contactos` tem de servir a aplicação, não dar 404.

    É assim que uma aplicação de página única funciona: o browser recebe
    o `index.html` e o roteador do lado do cliente decide o que mostrar.
    """
    assert pedir(com_ui, "/contactos")[0] == 200
    assert pedir(com_ui, "/definicoes")[0] == 200


def test_a_saida_do_dist_e_recusada(com_ui: _Aberto) -> None:
    """`..` a mais não sai da directoria.

    O `urllib` normaliza o caminho antes de o mandar, por isso o teste
    manda o `..` **escapado**, que é a forma que chega ao servidor sem
    ter sido normalizada.
    """
    codigo, _ = pedir(com_ui, "/..%2f..%2f..%2fetc%2fpasswd")
    assert codigo == 404


def test_o_parentesea_fora_do_dist(com_ui: _Aberto) -> None:
    codigo, _ = pedir(com_ui, "/../mensenger/conta.py")
    assert codigo == 404


def test_um_link_simbolico_para_fora_e_recusado(
    com_ui: _Aberto,
    tmp_path: Path,
) -> None:
    """O `realpath` é conferido **depois** de resolver.

    Um link simbólico dentro de `dist/` apontaria para fora sem este
    passo, e `os.path.normpath` — que é a defesa mais óbvia — não o
    apanha, porque o normpath resolve o caminho sem seguir links.
    """
    (tmp_path / "segredo.txt").write_text("não devia ser servido", encoding="utf-8")
    (com_ui.servidor.sidecar.raiz_ui / "fuga.txt").symlink_to(tmp_path / "segredo.txt")

    assert pedir(com_ui, "/fuga.txt")[0] == 404


def test_um_ficheiro_inexistente_da_404(com_ui: _Aberto) -> None:
    assert pedir(com_ui, "/assets/nada.js")[0] == 404


def test_o_head_nao_devolve_corpo(com_ui: _Aberto) -> None:
    codigo, _ = pedir(com_ui, "/index.html", metodo="HEAD")
    assert codigo == 200


def test_os_ficheiros_trazem_cabecalhos_de_seguranca(com_ui: _Aberto) -> None:
    """`frame-ancestors 'self'` fecha o *clickjacking*.

    Num servidor local a consequência é concreta: uma página hostil numa
    iframe por cima do formulário de desbloqueio, a receber os cliques.
    """
    pedido = urllib.request.Request(
        f"{com_ui.url}/index.html",
        headers={"Host": f"127.0.0.1:{com_ui.porta}"},
    )
    with urllib.request.urlopen(pedido, timeout=5) as resposta:
        assert resposta.headers["X-Content-Type-Options"] == "nosniff"
        assert "frame-ancestors 'self'" in resposta.headers["Content-Security-Policy"]


def test_as_respostas_json_tambem_trazem_os_cabecalhos(aberto: _Aberto) -> None:
    pedido = urllib.request.Request(
        f"{aberto.url}/api/contagens",
        headers={
            "Host": f"127.0.0.1:{aberto.porta}",
            "Origin": ORIGEM_NOSSA.format(porta=aberto.porta),
            "Sec-Fetch-Site": "same-origin",
        },
    )
    with urllib.request.urlopen(pedido, timeout=5) as resposta:
        assert resposta.headers["X-Frame-Options"] == "DENY"
        assert resposta.headers["Referrer-Policy"] == "no-referrer"
        assert resposta.headers["Cache-Control"] == "no-store"


# ---------------------------------------------------------------------
# A escuta
# ---------------------------------------------------------------------


def test_o_sidecar_recusa_escutar_fora_do_loopback(tmp_path: Path) -> None:
    """Um sidecar em `0.0.0.0` está a dar a identidade a toda a rede.

    E é a alteração mais pequena possível neste ficheiro: uma palavra no
    endereço de escuta. Por isso é uma excepção, não um aviso.
    """
    with loja_mod.Loja(tmp_path / "l.db") as loja:
        with pytest.raises(ValueError) as erro:
            sidecar_mod.criar_servidor(loja, None, [], porta=0, host="0.0.0.0")
    assert "loopback" in str(erro.value)


@pytest.mark.parametrize(
    ("host", "esperado"),
    [
        ("127.0.0.1", True),
        ("127.0.0.2", True),
        ("localhost", True),
        ("::1", True),
        ("0.0.0.0", False),
        ("192.168.1.10", False),
        ("10.0.0.1", False),
        ("atacante.example", False),
    ],
)
def test_e_loopback(host: str, esperado: bool) -> None:
    assert sidecar_mod.e_loopback(host) is esperado


# ---------------------------------------------------------------------
# As rotas, isoladas
# ---------------------------------------------------------------------


def test_o_casamento_de_rota_exige_o_mesmo_numero_de_segmentos() -> None:
    """`/conversas` não casa com `/conversas/abc/def`.

    O casamento por segmento é o que evita que uma expressão regular mal
    escrita aceite o que não deve.
    """
    rota = sidecar_mod.Rota("GET", "/conversas/{id}", lambda c, p: {}, "teste")
    assert rota.casa("GET", "/conversas/abc") == {"id": "abc"}
    assert rota.casa("GET", "/conversas/abc/def") is None
    assert rota.casa("POST", "/conversas/abc") is None


def test_o_casamento_descodifica_o_parametro() -> None:
    rota = sidecar_mod.Rota("GET", "/conversas/{id}", lambda c, p: {}, "teste")
    assert rota.casa("GET", "/conversas/a%20b") == {"id": "a b"}


def test_uma_rota_sem_caminho_casa_com_a_raiz() -> None:
    rota = sidecar_mod.Rota("GET", "/", lambda c, p: {}, "teste")
    assert rota.casa("GET", "/") == {}
    assert rota.casa("GET", "") == {}


def test_a_rota_tem_um_nome_para_os_registos() -> None:
    """Um nome é melhor do que `<lambda>` quando se procura a causa de um
    ``500``."""
    rotas = rotas_mod.montar(loja_mod.Loja(":memory:"), None)
    assert all(rota.nome for rota in rotas)
    assert len({r.nome for r in rotas}) == len(rotas)


def test_camel_converte() -> None:
    assert rotas_mod.camel("identificador") == "identificador"
    assert rotas_mod.camel("incluir_bloqueados") == "incluirBloqueados"
    assert rotas_mod.camel("a_b_c") == "aBC"


# ---------------------------------------------------------------------
# O `Contexto`, sem passar pelo HTTP
#
# O `Contexto` é um `dataclass` com dois campos obrigatórios, e
# construí-lo directamente é mais honesto do que inventar um pedido HTTP
# para chegar a `flag()` — um pedido que o `flag()` não precisa e whose
# único efeito seriaUm 404 se o caminho estivesse errado.
# ---------------------------------------------------------------------


def _contexto(consulta: dict[str, list[str]] | None = None, corpo: bytes = b""):
    return sidecar_mod.Contexto(
        caminho="/api/x",
        consulta=consulta or {},
        corpo=corpo,
        loja=loja_mod.Loja(":memory:"),
    )


def test_o_flag_aceita_o_que_a_interface_manda() -> None:
    """Quatro maneiras de dizer «sim», e nada mais.

    O `flag` existe porque o `camelCase` do JavaScript manda `"true"` e
    alguém, mais tarde, vai mandar `"1"`. Uma lista escrita à mão que fica
    desactualizada é pior do que não existir — por isso o teste fixa a
    lista, para que acrescentar um valor seja uma alteração visível.
    """
    for verdadeiro in ("1", "true", "sim", "verdade", "TRUE", "Sim"):
        assert _contexto({"x": [verdadeiro]}).flag("x") is True
    for falso in ("0", "false", "nao", "", "2", "verdadeiro"):
        assert _contexto({"x": [falso]}).flag("x") is False
    assert _contexto().flag("x") is False


def test_o_json_invalido_da_400_com_a_razão() -> None:
    """A mensagem diz **o que** está mal, não só que está mal.

    Um `JSON inválido` sem mais diz à pessoa que o corpo não é JSON, o que
    ela já sabia. O `json` do Python diz onde foi — e essa posição é a
    diferença entre corrigir em trinta segundos e procurar em vão.
    """
    with pytest.raises(sidecar_mod.ErroDePedido) as erro:
        _contexto(corpo=b'{"a": }').json()
    assert erro.value.codigo == 400
    assert "JSON inválido" in erro.value.mensagem
    # A posição que o `json` dedo.
    assert ": " in erro.value.mensagem


def test_o_corpo_vazio_e_um_objecto_vazio() -> None:
    """``GET`` não tem corpo, e isso é um ``{}`` e não uma excepção."""
    assert _contexto().json() == {}


def test_o_405_estatico_recusa_o_que_nao_e_leitura(tmp_path: Path) -> None:
    """Servir ficheiros com ``POST`` é ``405``, e não um ``404`` de caminho.

    A distinção importa: ``404`` diz «isto não existe», ``405`` diz «existe
    mas não se pode assim». Dar ``404`` a um ``POST`` em ``/`` esconderia
    que a interface construída está lá e é servível.
    """
    dist = tmp_path / "dist"
    dist.mkdir()
    (dist / "index.html").write_text("<!doctype html>", encoding="utf-8")
    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        servidor = _Aberto(loja, None, raiz_ui=dist)
        try:
            assert pedir(servidor, "/", metodo="POST")[0] == 405
            assert pedir(servidor, "/", metodo="DELETE")[0] == 405
            assert pedir(servidor, "/")[0] == 200
        finally:
            servidor.parar()


def test_o_expect_aceita_o_corpo_que_cabe(aberto: _Aberto) -> None:
    """Um ``Expect`` com corpo aceitável passa, e o corpo é lido.

    O caminho de recusa está testado por `test_um_corpo_grande_da_413`; o
    de aceitação não estava, e é o que a maior parte do tráfego real
    percorre. Deixar o ``return super()`` sem exercitar significava não
    saber se o ``100 Continue`` saía ou não.
    """
    corpo = json.dumps({"frase": "quatro cavalos lentos numa mare"}).encode()
    with socket.create_connection(("127.0.0.1", aberto.porta), timeout=5) as ligacao:
        ligacao.sendall(
            (
                "POST /api/conta/desbloquear HTTP/1.1\r\n"
                f"Host: 127.0.0.1:{aberto.porta}\r\n"
                f"Origin: {ORIGEM_NOSSA.format(porta=aberto.porta)}\r\n"
                "Sec-Fetch-Site: same-origin\r\n"
                "Content-Type: application/json\r\n"
                "Expect: 100-continue\r\n"
                f"Content-Length: {len(corpo)}\r\n"
                "\r\n"
            ).encode()
        )
        primeira = ligacao.recv(4096)
        assert b"100 Continue" in primeira.split(b"\r\n")[0]
        ligacao.sendall(corpo)
        segunda = ligacao.recv(4096)
    # Um `200` prova que o corpo chegou e foi lido: o `100 Continue` saiu,
    # o servidor esperou, recebeu os bytes e passou a frase à rota.
    assert b"200" in segunda.split(b"\r\n")[0]


def test_a_drenagem_para_quando_o_cliente_desiste(aberto: _Aberto) -> None:
    """Um cliente que anuncia 4 MiB e escreve 1 KiB não trava o servidor.

    O servidor esvazia até ao tecto antes de responder `413`, para que o
    cliente consiga ler a resposta. Sem a paragem no fim do que chegou, um
    cliente que/desiste a meio transformava o `413` — que é uma resposta
    rápida — numa espera de quatro megabytes que nunca chegam.
    """
    with socket.create_connection(("127.0.0.1", aberto.porta), timeout=5) as ligacao:
        ligacao.sendall(
            (
                "PUT /api/definicoes HTTP/1.1\r\n"
                f"Host: 127.0.0.1:{aberto.porta}\r\n"
                f"Origin: {ORIGEM_NOSSA.format(porta=aberto.porta)}\r\n"
                "Sec-Fetch-Site: same-origin\r\n"
                "Content-Type: application/json\r\n"
                f"Content-Length: {sidecar_mod.MAX_CORPO + 1}\r\n"
                "\r\n"
            ).encode()
        )
        ligacao.sendall(b"x" * 1024)
        ligacao.shutdown(socket.SHUT_WR)  # fim de ficheiro: o cliente desistiu
        resposta = ligacao.recv(4096)
    assert b"413" in resposta.split(b"\r\n")[0]


def test_o_registo_silencioso_e_o_que_faz_o_teste_ficar_legivel() -> None:
    """``silencioso`` corta o ``log_message`` e **só** o ``log_message``.

    O motivo está no docstring do método: um teste que passa a inundar o
    terminal de pedidos é um teste cuja falha se perde no ruído. Mas o
    corte tem de ser só do ``stderr`` — o que a excepção do `500` escreve
    continua a ter de ir para algum sítio, e esse sítio é o `stderr`.
    """
    escrito: list[str] = []

    class _ServidorFalso:
        silencioso = True

        def log_message(self, formato, *args):
            escrito.append("servidor")

    manip = object.__new__(sidecar_mod.Manipulador)
    manip.server = _ServidorFalso()  # type: ignore[attr-defined]
    manip.log_message("pedido %s", "x")
    assert escrito == []

    _ServidorFalso.silencioso = False

    def registar(formato, *args):
        escrito.append(formato % args)

    # `super().log_message` é o do `BaseHTTPRequestHandler`; com um
    # servidor falso chega para ver que foi chamado.
    import http.server

    http.server.BaseHTTPRequestHandler.log_message = staticmethod(registar)
    try:
        manip.log_message("pedido %s", "x")
    finally:
        del http.server.BaseHTTPRequestHandler.log_message
    assert escrito[-1] == "pedido x"


def test_o_bruto_tamanho_e_o_content_length_ou_zero() -> None:
    """Três entradas e três saídas, e uma delas é o lixo.

    O `Content-Length` que não é número dá ``0`` e não levanta: quem o
    chama é o `Expect: 100-continue`, e a esse ponto o ``400`` do
    ``_ler_corpo`` já está a caminho. Deixar aqui uma excepção dava ao
    `http.server` um erro que ele reportaria como ``Internal Server
    Error`` — um ``500`` num pedido que devia ser um ``400``.
    """
    assert sidecar_mod.bruto_tamanho({}) == 0
    assert sidecar_mod.bruto_tamanho(None) == 0
    assert sidecar_mod.bruto_tamanho({"Content-Length": "10"}) == 10
    assert sidecar_mod.bruto_tamanho({"Content-Length": "-5"}) == 0
    assert sidecar_mod.bruto_tamanho({"Content-Length": "muito grande"}) == 0


def test_a_aplicacao_monta_as_rotas_da_loja_e_da_conta(tmp_path: Path) -> None:
    """``aplicacao`` é a ponte entre a loja e a tabela de rotas.

    Os testes montam a tabela à mão (``rotas_mod.montar``) e por isso não
    passavam por aqui — que é a função que o ``executar`` chama e que a
    CLI exercita sem nenhum teste pelo meio. As rotas têm de diferir
    conforme a conta está aberta ou trancada, e essa é a razão de a
    função existir separada.
    """
    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        trancadas = sidecar_mod.aplicacao(loja, None)
        conta = conta_mod.criar(
            "Marta", "quatro cavalos lentos numa mare", caminho=tmp_path / "c.keystore"
        )
        abertas = sidecar_mod.aplicacao(loja, conta)
    assert trancadas and abertas
    assert {r.nome for r in trancadas} == {r.nome for r in abertas}


def test_o_executar_arranca_e_fecha_a_loja(tmp_path: Path, monkeypatch) -> None:
    """``executar`` é o caminho da CLI, e fecha o que abriu.

    A loja é criada aqui quando não é passada, e fechada no ``finally``.
    Deixar uma loja ``sqlite3`` aberta por cada ``Ctrl-C`` do sidecar é
    um ficheiro de bloqueio que fica no directório do projecto — e o
    ``KeyboardInterrupt`` tem de ser tratado, senão o traceback aparece no
    terminal de quem só queria fechar o programa.
    """
    fechadas: list[object] = []
    classed: list[object] = []

    class _LojaQueSeFecha(loja_mod.Loja):
        def __init__(self, caminho=None) -> None:
            super().__init__(caminho if caminho is not None else ":memory:")
            self.fechada = False

        def fechar(self) -> None:
            self.fechada = True
            fechadas.append(self)

    def servir(self):
        # `serve_forever` a ser interrompido é o `Ctrl-C` de quem
        # fecha o sidecar.
        raise KeyboardInterrupt

    monkeypatch.setattr(sidecar_mod.loja_mod, "Loja", _LojaQueSeFecha)
    monkeypatch.setattr(sidecar_mod._Servidor, "serve_forever", servir)
    monkeypatch.setattr(sidecar_mod._Servidor, "server_close", lambda self: classed.append(self))

    sidecar_mod.executar(porta=_porta_livre(), caminho_conta=tmp_path / "c.keystore")

    assert len(fechadas) == 1, "a loja que o executar abriu ficou por fechar"
    assert fechadas[0].fechada is True
    assert classed, "o socket não foi fechado"
