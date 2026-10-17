# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
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
import re
import socket
import threading
import urllib.error
import urllib.request
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest

from messenger import conta as conta_mod
from messenger import ipc_client
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


class _DaemonFalso:
    """Um daemon que responde sem estar a correr.

    Existe porque uma rota de media tem de poder ser testada **sem**
    daemon, e um teste que só pudesse dizer «deu 503» não provaria nada
    sobre o que a rota mandou. Este regista os comandos e devolve o que
    o teste quiser.
    """

    def __init__(
        self,
        *,
        leitor: int = 0,
        loja: bool = True,
        erro: Exception | None = None,
        quadro_erro: Exception | None = None,
        tem_audio: bool = True,
    ):
        self.leitor = leitor
        self.loja_apontada = loja
        self.erro = erro
        self.iniciados: list[str] = []
        self.reproduzidos: list[tuple[str, int]] = []
        #: Os argumentos de cada `media_capturar`, para o teste fixar a
        #: geometria que a rota pediu.
        self.capturas: list[tuple[int, ...]] = []
        #: O erro que o `media_ler` dá. `None` devolve um quadro.
        self.quadro_erro = quadro_erro
        #: O erro que o `media_audio` dá.
        self.audio_erro: Exception | None = None
        #: Se o `media_audio` tem pacotes para dar.
        self.tem_audio = tem_audio

    def media_iniciar(self, caminho) -> None:
        if self.erro is not None:
            raise self.erro
        self.iniciados.append(str(caminho))

    def media_estado(self):
        if self.erro is not None:
            raise self.erro
        return ipc_client.EstadoMedia(
            loja_apontada=self.loja_apontada, ffmpeg_embebido=True, leitor=self.leitor
        )

    def media_reproduzir(self, transferencia_id: str, leitor: int) -> int:
        if self.erro is not None:
            raise self.erro
        self.reproduzidos.append((transferencia_id, leitor))
        return 4321

    def media_capturar(
        self,
        largura: int,
        altura: int,
        fps: int,
        audio_kbps: int,
        video_kbps: int,
    ) -> None:
        if self.erro is not None:
            raise self.erro
        self.capturas.append((largura, altura, fps, audio_kbps, video_kbps))

    def media_ler(self):
        if self.quadro_erro is not None:
            raise self.quadro_erro
        return ipc_client.QuadroVideo(
            sequencia=7, clock_ms=233, dados=bytes(range(256))
        )

    def media_audio(self):
        if self.quadro_erro is not None:
            raise self.quadro_erro
        if self.audio_erro is not None:
            raise self.audio_erro
        if not self.tem_audio:
            return None
        # 80 bytes a 32 kbit/s são 20 ms (§5.3).
        return bytes(range(80))


class _Aberto:
    """Um sidecar a correr, com o ``finally`` que o desliga."""

    def __init__(
        self,
        loja: loja_mod.Loja,
        conta: Any,
        raiz_ui: Path | None = None,
        caminho_conta: Path | None = None,
        daemon: Any | None = None,
    ) -> None:
        self.loja = loja
        self.daemon = daemon
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
            daemon=daemon,
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
def daemon() -> _DaemonFalso:
    """Um daemon que responde bem, com um leitor disponível."""
    return _DaemonFalso()


@pytest.fixture
def aberto_sem_leitor(
    tmp_path: Path, conta_de_teste: conta_mod.Conta
) -> Iterator[_Aberto]:
    """Um sidecar com daemon, mas sem ``mpv`` nem ``ffplay`` instalados.

    Falta a terceira das três coisas independentes: o daemon está lá, a
    loja está apontada, e não há leitor. É o caso em que a interface
    tem de dizer **porque** em vez de oferecer um botão que não funciona.
    """
    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        servidor = _Aberto(loja, conta_de_teste, daemon=_DaemonFalso(leitor=-1))
        try:
            yield servidor
        finally:
            servidor.parar()


@pytest.fixture
def aberto_com_daemon(
    tmp_path: Path, conta_de_teste: conta_mod.Conta, daemon: _DaemonFalso
) -> Iterator[_Aberto]:
    """Um sidecar **com** daemon, que é o caso que importa para a media."""
    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        servidor = _Aberto(loja, conta_de_teste, daemon=daemon)
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


def test_a_tabela_de_rotas_da_documentacao_esta_certa() -> None:
    """A contagem de `docs/sidecar.md` §A é conferida contra o código.

    A tabela diz que a contagem é conferida, e sem esta conferência a
    frase é uma promessa. O número é o que separa «acrescentei três
    rotas de media e documente-as» de «acrescentei três rotas e a
    documentação ficou a mentir, que é o estado em que ninguém repara
    porque tudo continua a funcionar».
    """
    rotas = rotas_mod.montar(loja_mod.Loja(":memory:"), None)
    documentacao = (Path(__file__).resolve().parents[1] / "docs" / "sidecar.md").read_text(
        encoding="utf-8"
    )

    na_tabela = {
        (metodo, caminho)
        for metodo, caminho in re.findall(
            r"\|\s*`(GET|POST|PUT|DELETE)`\s*\|\s*`([^`]+)`\s*\|", documentacao
        )
    }
    no_codigo = {(rota.metodo, rota.caminho) for rota in rotas}

    assert na_tabela == no_codigo, (
        f"rotas no código e na tabela não coincidem: "
        f"só no código {sorted(no_codigo - na_tabela)}, "
        f"só na tabela {sorted(na_tabela - no_codigo)}"
    )
    assert re.search(r"\*\*" + str(len(rotas)) + r" rotas\*\*", documentacao), (
        f"a contagem em prosa não é {len(rotas)}"
    )


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


# ---------------------------------------------------------------------
# As acções do menu de contexto
# ---------------------------------------------------------------------
#
# Estas rotas existem desde 2026-10-13. Antes disso o menu tinha seis
# botões que não escreviam em lado nenhum, e a rota `/conversas/{id}/lida`
# já existia no servidor desde a Etapa 5 sem que nada a chamasse. O teste
# que prova isto é o de baixo: conta as rotas declaradas e confirma que
# cada uma tem pelo menos um teste que a exercita.


def _conversa(servidor: _Aberto) -> tuple[str, str]:
    """Devolve ``(conversa_id, contacto_id)``, com a conversa já criada."""
    contacto = _contacto(servidor)
    pedir(
        servidor,
        "/api/mensagens",
        metodo="POST",
        corpo={"identificador": "ONYX-AAAAAA-!A!A#", "texto": "Olá"},
    )
    return servidor.loja.abrir_conversa(contacto)["id"], contacto


def test_marcar_como_lida_pela_rota(aberto: _Aberto) -> None:
    """A rota existia e não era chamada por nada."""
    conversa, _ = _conversa(aberto)
    aberto.loja.adicionar_mensagem(conversa, "them", "Chegaste?")

    codigo, _ = pedir(aberto, f"/api/conversas/{conversa}/lida", metodo="POST")
    assert codigo == 200

    _, conversas = pedir(aberto, "/api/conversas")
    assert conversas[0]["unread"] == 0


def test_fixar_e_desfixar_pela_rota(aberto: _Aberto) -> None:
    conversa, _ = _conversa(aberto)

    codigo, _ = pedir(
        aberto,
        f"/api/conversas/{conversa}/fixada",
        metodo="POST",
        corpo={"valor": True},
    )
    assert codigo == 200
    assert pedido_a_conversa(aberto, conversa)["pinned"] is True

    pedir(
        aberto,
        f"/api/conversas/{conversa}/fixada",
        metodo="POST",
        corpo={"valor": False},
    )
    assert pedido_a_conversa(aberto, conversa)["pinned"] is False


def test_silenciar_pela_rota(aberto: _Aberto) -> None:
    conversa, _ = _conversa(aberto)
    codigo, _ = pedir(
        aberto,
        f"/api/conversas/{conversa}/silenciada",
        metodo="POST",
        corpo={"valor": True},
    )
    assert codigo == 200
    assert pedido_a_conversa(aberto, conversa)["muted"] is True


def test_limpar_historico_pela_rota(aberto: _Aberto) -> None:
    conversa, _ = _conversa(aberto)
    codigo, corpo = pedir(aberto, f"/api/conversas/{conversa}/limpar", metodo="POST")
    assert codigo == 200
    assert corpo == {"ok": True}
    assert pedido_a_conversa(aberto, conversa)["messages"] == []


def test_limpar_uma_conversa_que_nao_existe_da_404(aberto: _Aberto) -> None:
    codigo, _ = pedir(aberto, "/api/conversas/c-nao-existe/limpar", metodo="POST")
    assert codigo == 404


def test_bloquear_e_desbloquear_pela_rota(aberto: _Aberto) -> None:
    _, contacto = _conversa(aberto)

    codigo, _ = pedir(
        aberto,
        f"/api/contactos/{contacto}/bloqueado",
        metodo="POST",
        corpo={"valor": True},
    )
    assert codigo == 200
    _, contactos = pedir(aberto, "/api/contactos")
    assert contactos[0]["blocked"] is True

    pedir(
        aberto,
        f"/api/contactos/{contacto}/bloqueado",
        metodo="POST",
        corpo={"valor": False},
    )
    _, contactos = pedir(aberto, "/api/contactos")
    assert contactos[0]["blocked"] is False


def test_bloquear_um_contacto_que_nao_existe_da_404(aberto: _Aberto) -> None:
    codigo, _ = pedir(
        aberto,
        "/api/contactos/x-nao-existe/bloqueado",
        metodo="POST",
        corpo={"valor": True},
    )
    assert codigo == 404


def test_o_valor_tem_de_ser_um_booleano(aberto: _Aberto) -> None:
    """«false» é uma string verdadeira em Python.

    Sem a conferência de tipo, um cliente que mandasse texto em vez de
    JSON desligaria a conversa em vez de a recusar — e a pessoa ficaria
    com uma conversa silenciada que não pediu.
    """
    conversa, contacto = _conversa(aberto)
    for caminho in (
        f"/api/conversas/{conversa}/fixada",
        f"/api/conversas/{conversa}/silenciada",
        f"/api/contactos/{contacto}/bloqueado",
    ):
        codigo, _ = pedir(aberto, caminho, metodo="POST", corpo={"valor": "false"})
        assert codigo == 400, caminho


def test_o_valor_que_falta_da_400_e_nao_desliga_nada(aberto: _Aberto) -> None:
    """Um campo ausente não pode valer por «falso».

    `bool(corpo.get(...))` trataria a ausência como `False` — que é
    «desligar». Um pedido malformado que desliga algo é pior do que um
    pedido malformado que não faz nada.
    """
    conversa, _ = _conversa(aberto)
    pedir(
        aberto,
        f"/api/conversas/{conversa}/fixada",
        metodo="POST",
        corpo={"valor": True},
    )

    codigo, _ = pedir(aberto, f"/api/conversas/{conversa}/fixada", metodo="POST", corpo={})
    assert codigo == 400
    assert pedido_a_conversa(aberto, conversa)["pinned"] is True


def pedido_a_conversa(servidor: _Aberto, conversa_id: str) -> dict[str, Any]:
    """Uma conversa pela rota, e não pela loja.

    Ler pela rota é o que interessa: o teste passa a provar o mesmo
    caminho que a interface usa, em vez de confirmar que a escrita e a
    leiturahappened na mesma estrutura.
    """
    codigo, corpo = pedir(servidor, f"/api/conversas/{conversa_id}")
    assert codigo == 200
    return corpo


def test_as_rotas_do_menu_tem_todas_accao_e_todas_sao_escritas(aberto: _Aberto) -> None:
    """As seis acções existem, e nenhuma das seis é uma leitura.

    Uma rota de acção que respondesse `200` a um `GET` seria uma porta
    que se pode atravessar sem querer — e o `origem.py` isenta `GET` da
    exigência de `Origin` precisamente porque não tem efeito. O inverso
    também é verdade e é mais grave: uma acção registada como leitura
    ficaria de fora da verificação de `Origin`.
    """
    esperados = {
        "marcar-lida": "conversas/{id}/lida",
        "fixar-conversa": "conversas/{id}/fixada",
        "silenciar-conversa": "conversas/{id}/silenciada",
        "limpar-historico": "conversas/{id}/limpar",
        "bloquear-contacto": "contactos/{id}/bloqueado",
    }
    # `caminho` traz a barra inicial; `lstrip("/")` alinha com o que o
    # `docs/sidecar.md` escreve.
    por_nome = {rota.nome: rota for rota in rotas_mod.montar(aberto.loja, None)}
    for nome, caminho in esperados.items():
        assert nome in por_nome, f"a rota {nome} desapareceu"
        rota = por_nome[nome]
        assert rota.caminho.lstrip("/") == caminho
        assert rota.metodo == "POST", f"{nome} é {rota.metodo}"


# ---------------------------------------------------------------------
# Transferências de media (`docs/media.md` §4)
# ---------------------------------------------------------------------


def _transferencia(aberto: _Aberto, conversa: str, **mudar) -> dict[str, Any]:
    """Regista uma transferência na loja e devolve o manifesto."""
    campos: dict[str, Any] = {
        "id": "d" * 32,
        "tipo": "video",
        "codec": "matroska",
        "formato": "matroska",
        "nome": "praia.mkv",
        "total_bytes": 204 * 49125,
        "total_pedacos": 204,
        "bytes_por_pedaco": 49125,
        "duracao_s": 42,
        "sha256": bytes(range(32)),
        "completo": False,
    }
    campos.update(mudar)
    return aberto.loja.registar_transferencia(conversa, "them", **campos)


def test_as_transferencias_da_conversa_pela_rota(aberto: _Aberto) -> None:
    """A rota que a interface chama para listar media de uma conversa."""
    conversa, _ = _conversa(aberto)
    _transferencia(aberto, conversa)

    codigo, corpo = pedir(aberto, f"/api/conversas/{conversa}/transferencias")
    assert codigo == 200
    assert [t["id"] for t in corpo["transferencias"]] == ["d" * 32]
    assert corpo["transferencias"][0]["totalPedacos"] == 204


def test_o_filtro_completas_tem_os_tres_casos(aberto: _Aberto) -> None:
    """`completas=0` não é a mesma coisa que não filtrar.

    São três estados — só as completas, só as incompletas, todas — e
    confundir os dois primeiros com a omissão era esconder a
    transferência que está a chegar, que é informação e não ruído.
    """
    conversa, _ = _conversa(aberto)
    _transferencia(aberto, conversa)
    _transferencia(aberto, conversa, id="e" * 32, nome="pronto.mkv")
    aberto.loja.marcar_transferencia_completa("e" * 32)

    _, todas = pedir(aberto, f"/api/conversas/{conversa}/transferencias")
    _, prontas = pedir(
        aberto, f"/api/conversas/{conversa}/transferencias?completas=1"
    )
    _, a_meio = pedir(
        aberto, f"/api/conversas/{conversa}/transferencias?completas=0"
    )
    _, todas_explicitamente = pedir(
        aberto, f"/api/conversas/{conversa}/transferencias?completas=todas"
    )

    assert len(todas["transferencias"]) == 2
    assert [t["id"] for t in prontas["transferencias"]] == ["e" * 32]
    assert [t["id"] for t in a_meio["transferencias"]] == ["d" * 32]
    assert len(todas_explicitamente["transferencias"]) == 2


def test_a_rota_da_transferencia_traz_o_progresso(aberto: _Aberto) -> None:
    """`recebidasPedacos` é o que permite «a chegar, 42 de 204».

    Sem este número no servidor, a interface teria de contar envelopes
    — e contaria os da conversa inteira, que não é o mesmo.
    """
    conversa, _ = _conversa(aberto)
    _transferencia(aberto, conversa)
    for i in range(5):
        aberto.loja.adicionar_mensagem(
            conversa,
            "them",
            f"ONX1 {i}",
            envelope=b"x",
            transferencia_id="d" * 32,
        )

    codigo, corpo = pedir(aberto, f"/api/transferencias/{'d' * 32}")
    assert codigo == 200
    assert corpo["transferencia"]["recebidasPedacos"] == 5
    assert corpo["transferencia"]["totalPedacos"] == 204


def test_uma_transferencia_que_nao_existe_da_404(aberto: _Aberto) -> None:
    codigo, _ = pedir(aberto, f"/api/transferencias/{'f' * 32}")
    assert codigo == 404
    codigo, _ = pedir(aberto, f"/api/transferencias/{'f' * 32}", metodo="DELETE")
    assert codigo == 404


def test_eliminar_pela_rota_apaga_so_a_transferencia(aberto: _Aberto) -> None:
    """O `DELETE` não pode levar o texto da conversa.

    É a propriedade que a implementação anterior violava, e ela só é
    visível por uma rota: o `DELETE` filtrava por conversa e apaga o que
    a interface não pediu.
    """
    conversa, _ = _conversa(aberto)
    _transferencia(aberto, conversa)
    _transferencia(aberto, conversa, id="e" * 32, nome="outro.mkv")
    for i in range(3):
        aberto.loja.adicionar_mensagem(
            conversa, "them", f"ONX1 {i}", envelope=b"x", transferencia_id="d" * 32
        )
        aberto.loja.adicionar_mensagem(
            conversa, "them", f"ONX1 {i}", envelope=b"y", transferencia_id="e" * 32
        )
    aberto.loja.adicionar_mensagem(conversa, "them", "olá", envelope=b"z")

    codigo, corpo = pedir(aberto, f"/api/transferencias/{'d' * 32}", metodo="DELETE")
    assert codigo == 200
    assert corpo == {"ok": True}

    _, restantes = pedir(aberto, f"/api/conversas/{conversa}/transferencias")
    assert [t["id"] for t in restantes["transferencias"]] == ["e" * 32]

    mensagens = pedido_a_conversa(aberto, conversa)["messages"]
    assert not [m for m in mensagens if m["mediaId"] == "d" * 32]
    # O texto que já lá estava — a mensagem que criou a conversa e a que
    # o teste escreveu — sobrevive intacto.
    assert "olá" in [m["text"] for m in mensagens]
    assert [m for m in mensagens if m["mediaId"] == "e" * 32] != []


# ---------------------------------------------------------------------
# Reprodução (`docs/media.md` §4.6)
# ---------------------------------------------------------------------


def _transferencia_completa(aberto: _Aberto, conversa: str) -> str:
    """Regista uma transferência de duas fatias, marcada como completa."""
    aberto.loja.registar_transferencia(
        conversa,
        "them",
        id="d" * 32,
        tipo="video",
        codec="matroska",
        formato="matroska",
        nome="praia.mkv",
        total_bytes=100_000,
        total_pedacos=2,
        bytes_por_pedaco=49125,
        duracao_s=42,
        sha256=bytes(range(32)),
        completo=True,
    )
    for i in range(2):
        aberto.loja.adicionar_mensagem(
            conversa, "them", f"ONX1 {i}", transferencia_id="d" * 32
        )
    return "d" * 32


def test_media_iniciar_vai_no_arranque_e_com_o_caminho_da_loja(
    tmp_path: Path,
) -> None:
    """O daemon tem de ficar a apontar para **esta** loja.

    O caminho vem do sidecar e nunca do cliente, e é o caminho da loja
    que este sidecar tem aberta. Um `MEDIA_INICIAR` com o caminho
    errado daria um daemon a ler a base de outra pessoa — ou de um
    ficheiro que não existe, e o sintoma seria um vídeo que não abre.
    """
    daemon = _DaemonFalso()
    caminho = tmp_path / "loja.db"
    with loja_mod.Loja(caminho) as aberta:
        pedido = aberta.registar_pedido("ONYX-AAAAAA-!A!A#", "Ana", "Olá")
        aberta.decidir_pedido(pedido["id"], "aceite")

        servidor = sidecar_mod.criar_servidor(
            aberta,
            None,
            rotas_mod.montar(aberta, None),
            porta=_porta_livre(),
            raiz_ui=Path("/nonexistente"),
            daemon=daemon,
            silencioso=True,
        )

    assert daemon.iniciados == [str(caminho)]


def test_um_daemon_indisponivel_nao_impede_o_sidecar_de_arrancar(
    tmp_path: Path,
) -> None:
    """O modo degradado do outro lado: sem daemon, o texto fica inteiro.

    Se ``apontar_media`` levantasse, uma pessoa sem daemon em execução
    perdia a conversa toda por causa de um botão de áudio.
    """
    daemon = _DaemonFalso(erro=ipc_client.DaemonIndisponivel("sem daemon"))
    with loja_mod.Loja(tmp_path / "loja.db") as aberta:
        servidor = sidecar_mod.criar_servidor(
            aberta,
            None,
            rotas_mod.montar(aberta, None),
            porta=_porta_livre(),
            raiz_ui=Path("/nonexistente"),
            daemon=daemon,
            silencioso=True,
        )
        app = servidor.sidecar  # type: ignore[attr-defined]
        assert app.media_erro is not None, "e o motivo fica escrito"


def test_o_estado_da_media_diz_que_sim(aberto_com_daemon: _Aberto, daemon: _DaemonFalso) -> None:
    codigo, corpo = pedir(aberto_com_daemon, "/api/media/estado")
    assert codigo == 200
    assert corpo["disponivel"] is True
    # O `leitor` é um **nome**, e não o índice que vai no IPC.
    assert corpo["leitor"] == ipc_client.LEITORES_MEDIA[0]


def test_o_estado_da_media_explica_a_falta(aberto_sem_leitor: _Aberto) -> None:
    codigo, corpo = pedir(aberto_sem_leitor, "/api/media/estado")
    assert codigo == 200
    assert corpo["disponivel"] is False
    assert "leitor" in corpo["motivo"]

    # E a interface tem de poder dizer **porque**, e não só «não».
    assert corpo["motivo"] != ""


def test_reproduzir_devolve_o_pid_e_manda_o_id(aberto_com_daemon: _Aberto, daemon: _DaemonFalso) -> None:
    conversa, _ = _conversa(aberto_com_daemon)
    id = _transferencia_completa(aberto_com_daemon, conversa)

    codigo, corpo = pedir(aberto_com_daemon, f"/api/transferencias/{id}/reproduzir", metodo="POST")
    assert codigo == 200
    assert corpo["pid"] == 4321
    assert daemon.reproduzidos == [(id, 0)], "o id e o índice do leitor"


def test_uma_transferencia_incompleta_da_409_e_nao_404(
    aberto_com_daemon: _Aberto, daemon: _DaemonFalso
) -> None:
    """Ela existe e está a meio. `404` faria a interface escondê-la."""
    conversa, _ = _conversa(aberto_com_daemon)
    aberto_com_daemon.loja.registar_transferencia(
        conversa,
        "them",
        id="e" * 32,
        tipo="audio",
        codec="opus",
        formato="ogg",
        nome="a.opus",
        total_bytes=100_000,
        total_pedacos=2,
        bytes_por_pedaco=49125,
        duracao_s=9,
        sha256=bytes(range(32)),
    )
    aberto_com_daemon.loja.adicionar_mensagem(conversa, "them", "ONX1 0", transferencia_id="e" * 32)

    codigo, corpo = pedir(aberto_com_daemon, f"/api/transferencias/{'e' * 32}/reproduzir", metodo="POST")
    assert codigo == 409
    assert corpo["codigo"] == 409
    # O detalhe diz **quantas** faltam, que é o que a interface escreve
    # ao lado do «a chegar».
    assert "1 de 2" in corpo["detalhe"], corpo
    assert daemon.reproduzidos == [], "e não pediu nada ao daemon"


def test_reproduzir_uma_transferencia_que_nao_existe_da_404(
    aberto_com_daemon: _Aberto, daemon: _DaemonFalso
) -> None:
    codigo, _ = pedir(aberto_com_daemon, f"/api/transferencias/{'f' * 32}/reproduzir", metodo="POST")
    assert codigo == 404
    assert daemon.reproduzidos == []


def test_reproduzir_sem_daemon_da_503(aberto: _Aberto) -> None:
    """O sidecar a correr sem o outro processo.

    É o caso comum depois de fechar a aplicação, e tem de dar `503` e
    não `500`: `503` diz «tenta outra vez», `500` diz «está partido».
    A transferência existe e está completa, para que a resposta seja
    sobre o daemon e não sobre a loja.
    """
    conversa, _ = _conversa(aberto)
    id = _transferencia_completa(aberto, conversa)

    codigo, corpo = pedir(aberto, f"/api/transferencias/{id}/reproduzir", metodo="POST")
    assert codigo == 503
    # A mensagem vem do sidecar e não diz «daemon» por acaso: ela é o
    # que quem lê o erro precisa de saber.
    # `ServicoIndisponivel` escreve a mensagem em `erro`; um
    # `ErroDePedido` escreve-a em `detalhe`. São duas formas
    # diferentes para o mesmo corpo, e o teste fixa cada uma onde
    # aparece.
    assert "não está disponível" in corpo["erro"], corpo


# ---------------------------------------------------------------------
# O streaming de vídeo
# ---------------------------------------------------------------------


def test_iniciar_streaming_pede_480p_a_30_fps(
    aberto_com_daemon: _Aberto, daemon: _DaemonFalso
) -> None:
    """A rota pede 854×480 a 30 fps com 300 kbit/s de vídeo.

    São os números de §3.4.2, medidos: 6,3× de folga no orçamento de
    12,8 msg/s. E a geometria vem do **cliente**, não de uma constante
    aqui — por isso o teste fixa o que foi pedido e não o que a rota
    devolve, que é o mesmo número e não provaria nada.
    """
    codigo, corpo = pedir(
        aberto_com_daemon,
        "/api/media/streaming/iniciar",
        metodo="POST",
        corpo={"largura": 854, "altura": 480, "fps": 30, "videoKbps": 300},
    )
    assert codigo == 200, corpo
    assert corpo["largura"] == 854
    assert daemon.capturas == [(854, 480, 30, 32, 300)], daemon.capturas


def test_iniciar_streaming_usa_480p_quando_o_cliente_nao_diz(
    aberto_com_daemon: _Aberto, daemon: _DaemonFalso
) -> None:
    """Sem geometria no pedido, a rota pede 480p — que é o mínimo.

    E «o mínimo» é literal: 480p é o que a spec exige e o que o codec
    medido aguenta. Uma rota que aceitasse um `320` do cliente estaria
    a aceitar uma imagem que não traz definição nenhuma.
    """
    codigo, corpo = pedir(
        aberto_com_daemon, "/api/media/streaming/iniciar", metodo="POST", corpo={}
    )
    assert codigo == 200, corpo
    assert daemon.capturas == [(854, 480, 30, 32, 300)], daemon.capturas


def test_iniciar_streaming_recusa_um_valor_que_nao_e_inteiro(
    aberto_com_daemon: _Aberto,
) -> None:
    """`fps` a texto é ``400``, e o `ffmpeg` nunca chega a arrancar.

    O `JSON` aceita uma string onde se quer um número, e o `IPC` não.
    Passar isso adiante dava um `ffmpeg` com `-framerate texto`, que
    é um erro muito mais tarde e muito mais difícil de ler.
    """
    codigo, corpo = pedir(
        aberto_com_daemon,
        "/api/media/streaming/iniciar",
        metodo="POST",
        corpo={"fps": "trinta"},
    )
    assert codigo == 400, corpo
    assert "fps" in corpo["detalhe"]


def test_iniciar_streaming_da_503_quando_o_ffmpeg_da_recusa(
    aberto_com_daemon: _Aberto, daemon: _DaemonFalso
) -> None:
    """O `ffmpeg` não arrancou é ``503``, e a mensagem dele diz porquê.

    Não é ``400``: o pedido estava certo e o servidor é que não pôde.
    E a mensagem traz o `stderr` do `ffmpeg` (§8.1.1), que é a única
    informação que permite resolver — «a máquina não tem ecrã» e «a
    largura não é múltipla» dão o mesmo `503`.
    """
    daemon.erro = ipc_client.ErroDaemon(0x14, "Unknown input format: 'x11grab'")
    codigo, corpo = pedir(
        aberto_com_daemon, "/api/media/streaming/iniciar", metodo="POST", corpo={}
    )
    assert codigo == 503, corpo
    assert "x11grab" in corpo["detalhe"]


def test_o_quadro_vem_em_base64_e_com_a_geometria_certo(
    aberto_com_daemon: _Aberto,
) -> None:
    """O quadro volta em base-64, com a sequência e o tempo.

    E a base-64 é o mesmo custo que o `ONX1` paga desde o início: é o
    preço de a media ser indistinguível de texto.
    """
    codigo, corpo = pedir(aberto_com_daemon, "/api/media/streaming/quadro")
    assert codigo == 200, corpo
    assert corpo["sequencia"] == 7
    assert corpo["clockMs"] == 233
    bruto = base64.b64decode(corpo["vp8"])
    assert bruto == bytes(range(256)), "o base-64 tem de devolver os bytes intactos"


def test_a_captura_que_acaba_da_409_e_nao_404(
    aberto_com_daemon: _Aberto, daemon: _DaemonFalso
) -> None:
    """A captura que morreu é ``409``, e o `VideoStreaming` traduz em fim.

    ``409`` e não ``404``: «não há captura» é uma falha de arranque e
    «a captura acabou» é uma chamada que terminou, e quem lê o registo
    precisa de saber qual das duas foi. A UI trata as duas igual — as
    duas são o fim — mas o registo não.
    """
    daemon.quadro_erro = ipc_client.ErroDaemon(ipc_client.ERR_MEDIA, "a captura terminou")
    codigo, corpo = pedir(aberto_com_daemon, "/api/media/streaming/quadro")
    assert codigo == 409, corpo


def test_sem_daemon_as_tres_respondem_503(aberto: _Aberto) -> None:
    """Sem daemon, as três dão ``503`` — e nenhuma entra em pânico."""
    for caminho, metodo in (
        ("/api/media/streaming/iniciar", "POST"),
        ("/api/media/streaming/quadro", "GET"),
        ("/api/media/streaming/parar", "POST"),
    ):
        codigo, _corpo = pedir(aberto, caminho, metodo=metodo)
        assert codigo == 503, f"{caminho} respondeu {codigo}"


def test_parar_streaming_nao_parte_o_ffmpeg(aberto_com_daemon: _Aberto) -> None:
    """Parar é um ``204`` que não toca no daemon.

    `MEDIA_PARAR` não existe (§8.1.2): parar a captura é o `Drop` do
    `Stream`. Esta rota não manda o `ffmpeg` parar — diz à UI que pode
    deixar de pedir quadros, e o `409` seguinte faz o resto.
    """
    codigo, corpo = pedir(aberto_com_daemon, "/api/media/streaming/parar", metodo="POST")
    assert codigo == 200, corpo


def test_o_audio_vem_em_base64_numa_bolsa(
    aberto_com_daemon: _Aberto,
) -> None:
    """O áudio volta em `{opus: base64}`, e não em base-64 pelado.

    Porque uma `string` vazia não distingue «não há pacote» de «um
    pacote de tamanho zero» — e um pacote de Opus de tamanho zero é um
    pacote corrompido, não um silêncio. Devolver `""` faria o
    `Chamada.jsx` tocar silêncio em vez de dizer que não há nada.
    """
    codigo, corpo = pedir(aberto_com_daemon, "/api/media/streaming/audio")
    assert codigo == 200, corpo
    assert set(corpo) == {"opus"}, corpo
    bruto = base64.b64decode(corpo["opus"])
    assert bruto == bytes(range(80)), "o pacote tem de chegar intacto"


def test_o_audio_que_acaba_da_null_e_nao_uma_bolsa_vazia(
    aberto_com_daemon: _Aberto, daemon: _DaemonFalso
) -> None:
    """Sem pacotes, `{"opus": null}` — e o `null` é o fim da chamada.

    E a rota devolve `200` e não `409`, ao contrário do quadro. A
    diferença é deliberada: o `409` serve o `readVideoFrame`, que é um
    `fetch` por quadro e quer um erro HTTP; o áudio vem por um laço que
    só precisa de saber que acabou, e `null` é mais barato e não
    passa por um caminho de excepção que acordaria o browser.
    """
    daemon.tem_audio = False
    codigo, corpo = pedir(aberto_com_daemon, "/api/media/streaming/audio")
    assert codigo == 200, corpo
    assert corpo == {"opus": None}, corpo


def test_a_captura_morta_da_409_no_audio(
    aberto_com_daemon: _Aberto, daemon: _DaemonFalso
) -> None:
    """O `ffmpeg` que morreu é `409` — e o browser traduz em `null`."""
    daemon.audio_erro = ipc_client.ErroDaemon(
        ipc_client.ERR_MEDIA, "a captura terminou"
    )
    codigo, _corpo = pedir(aberto_com_daemon, "/api/media/streaming/audio")
    assert codigo == 409


def test_o_audio_sem_daemon_da_503(aberto: _Aberto) -> None:
    codigo, _corpo = pedir(aberto, "/api/media/streaming/audio")
    assert codigo == 503
