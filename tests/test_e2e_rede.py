# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_e2e_rede.py — Etapa 7: dois daemons reais ligados pelo relé.

Cobre o caminho completo **rede + handshake** sem tocar na rede real:

* relé TURN a correr em TCP de loopback (em processo, event loop próprio);
* dois processos ``onyxchatd --tor nenhum`` (backend ``TorFalso``) ligados
  em ``MODO_RELAY`` um ao outro;
* ``OUVIR`` (hidden service falso), ``ESTADO``, ``LIGAR``, ``ENVIAR`` e
  ``RECEBER`` com ``CHAT`` nas duas direções;
* handshake ``PEDIR → RECEBER → ACEITAR → RECEBER → CONFIRMAR`` e a
  recusa ``PEDIR → RECEBER → RECUSAR → RECEBER(REJECT)``.

Tudo é *saltado* se o binário não estiver compilado (mesmo padrão de
``test_e2e_daemon.py``).
"""

from __future__ import annotations

import asyncio
import socket
import subprocess
import threading
import time
from collections.abc import Iterator
from pathlib import Path

import pytest

from messenger.ipc_client import (
    FRAME_CHAT,
    FRAME_FRIEND_ACCEPT,
    FRAME_FRIEND_REJECT,
    FRAME_FRIEND_REQUEST,
    MODO_RELAY,
    ClienteIpc,
    ErroDaemon,
)
from messenger.keys import Chaves, Identidade
from messenger.pipeline import Pipeline
from server import relay_server

RAIZ = Path(__file__).resolve().parent.parent
BINARIO = RAIZ / "target" / "debug" / "onyxchatd"

pytestmark = pytest.mark.skipif(
    not BINARIO.is_file(),
    reason="daemon não compilado — correr `cargo build` na raiz do workspace",
)

CHAVES = Chaves(k1=b"\x11" * 32, k5=b"\x22" * 32, k9=b"\x33" * 32)
#: Nonce de handshake dentro do corpo de ``FRIEND_REQUEST`` (129..145):
#: o byte 0 é a versão do protocolo (``handshake.md`` §Mensagens).
FATIA_NONCE = slice(129, 145)


class RelayEmThread:
    """Relay TCP num *thread* com event loop próprio (nunca o do teste)."""

    def __init__(self) -> None:
        self.porta = 0
        self._pronto = threading.Event()
        self._loop: asyncio.AbstractEventLoop | None = None
        self._thread = threading.Thread(target=self._correr, daemon=True)

    def _correr(self) -> None:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        servidor, _estado = loop.run_until_complete(
            relay_server.servir("127.0.0.1", 0)
        )
        self.porta = servidor.sockets[0].getsockname()[1]
        self._loop = loop
        self._pronto.set()
        loop.run_forever()
        servidor.close()
        loop.run_until_complete(servidor.wait_closed())
        loop.close()

    def iniciar(self) -> int:
        """Arranca o relé e devolve a porta TCP."""
        self._thread.start()
        if not self._pronto.wait(timeout=10):
            raise AssertionError("relé não arrancou em 10 s")
        return self.porta

    def parar(self) -> None:
        """Para o loop e junta o *thread* (limpo, sem avisos)."""
        if self._loop is not None:
            self._loop.call_soon_threadsafe(self._loop.stop)
        self._thread.join(timeout=10)


@pytest.fixture(scope="session")
def porta_relay() -> Iterator[int]:
    """Relé partilhado por toda a sessão (uma porta TCP por sessão)."""
    reler = RelayEmThread()
    porta = reler.iniciar()
    yield porta
    reler.parar()


def _arrancar_daemon(caminho: Path) -> tuple[ClienteIpc, subprocess.Popen]:
    """Sobe ``onyxchatd --tor nenhum`` num socket e espera ficar pronto."""
    processo = subprocess.Popen(
        [str(BINARIO), "--socket", str(caminho), "--tor", "nenhum"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    limite = time.monotonic() + 10.0
    while time.monotonic() < limite:
        if caminho.exists():
            try:
                prova = socket.socket(socket.AF_UNIX)
                prova.settimeout(1.0)
                prova.connect(str(caminho))
                prova.close()
                return ClienteIpc(str(caminho)), processo
            except OSError:
                pass
        if processo.poll() is not None:
            raise AssertionError(f"daemon terminou cedo ({processo.returncode})")
        time.sleep(0.05)
    raise AssertionError("daemon não abriu o socket em 10 s")


def _parar(processos: list[subprocess.Popen]) -> None:
    """Termina os daemons arrancados (nunca deixa processos órfãos)."""
    for processo in processos:
        processo.terminate()
    for processo in processos:
        try:
            processo.wait(timeout=5)
        except subprocess.TimeoutExpired:
            processo.kill()
            processo.wait(timeout=5)


class Cenario:
    """Os dois daemons, as identidades e o endpoint do relé."""

    def __init__(
        self,
        a: ClienteIpc,
        b: ClienteIpc,
        identidade_a: Identidade,
        identidade_b: Identidade,
        endpoint: str,
    ) -> None:
        self.a = a
        self.b = b
        self.identidade_a = identidade_a
        self.identidade_b = identidade_b
        self.endpoint = endpoint


@pytest.fixture()
def par(
    tmp_path: Path, porta_relay: int
) -> Iterator[Cenario]:
    """Dois daemons reais já ``OUVIR``ados e ligados em modo relay.

    B subscreve a caixa ANTES de A ligar, para que a primeira mensagem
    de A seja *push* (e não adiada) — o caminho normal da ligação.
    """
    processos: list[subprocess.Popen] = []
    try:
        cliente_a, processo_a = _arrancar_daemon(tmp_path / "a.sock")
        cliente_b, processo_b = _arrancar_daemon(tmp_path / "b.sock")
        processos = [processo_a, processo_b]

        identidade_a = Identidade.gerar()
        identidade_b = Identidade.gerar()

        onion_a = cliente_a.ouvir()
        onion_b = cliente_b.ouvir()
        assert onion_a.endswith(".onion")
        assert onion_a == cliente_a.ouvir()  # OUVIR é idempotente
        assert onion_b != onion_a

        endpoint = f"127.0.0.1:{porta_relay}"
        cliente_b.ligar(
            MODO_RELAY, endpoint, "", identidade_b.pub, identidade_a.pub
        )
        cliente_a.ligar(
            MODO_RELAY, endpoint, "", identidade_a.pub, identidade_b.pub
        )
        yield Cenario(cliente_a, cliente_b, identidade_a, identidade_b, endpoint)
    finally:
        _parar(processos)


def test_ouvir_e_estado_dos_dois_daemons(par: Cenario) -> None:
    """``OUVIR`` publica ``.onion``; ``ESTADO`` vê tor ativo e sem amigos."""
    estado_a = par.a.estado()
    estado_b = par.b.estado()
    assert estado_a.tor == 0x02 and estado_a.onion.endswith(".onion")
    assert estado_b.tor == 0x02 and estado_b.onion != estado_a.onion
    assert estado_a.ligado and estado_b.ligado
    assert estado_a.amigos == 0 and estado_b.amigos == 0


def test_chat_pelo_relay_nas_duas_direcoes(par: Cenario) -> None:
    """ENCODE → ENVIAR → relé → RECEBER → DECODE, nos dois sentidos."""
    fachada_a = Pipeline(par.a)
    fachada_b = Pipeline(par.b)

    envelope = fachada_a.cifrar("Olá pelo relé!", CHAVES, par.identidade_a)
    par.a.enviar(envelope)
    frame = par.b.receber(5000)
    assert frame.tipo == FRAME_CHAT and frame.corpo == envelope
    texto = fachada_b.decifrar(frame.corpo, CHAVES, par.identidade_a.pub)
    assert texto == "Olá pelo relé!"

    # Sentido inverso: B responde e A decifra com a pública de B.
    resposta = fachada_b.cifrar("recebido!", CHAVES, par.identidade_b)
    par.b.enviar(resposta)
    frame = par.a.receber(5000)
    assert frame.tipo == FRAME_CHAT
    assert (
        fachada_a.decifrar(frame.corpo, CHAVES, par.identidade_b.pub)
        == "recebido!"
    )


def test_receber_sem_frames_devolve_sem_mensagem(par: Cenario) -> None:
    """``RECEBER`` com prazo curto e sem tráfego → ``SemMensagem`` (0x0E)."""
    with pytest.raises(ErroDaemon) as capturado:
        par.a.receber(200)
    assert capturado.value.codigo == 0x0E


def test_handshake_completo_pelo_relay(par: Cenario) -> None:
    """PEDIR → RECEBER → ACEITAR → RECEBER → CONFIRMAR com chaves iguais."""
    pedido = par.a.pedir_amizade(par.identidade_a.seed, par.identidade_b.pub)
    assert len(pedido.corpo) == 209
    assert pedido.corpo[0] == 0x01, "versão no primeiro byte do corpo"

    recebido = par.b.receber(5000)
    assert recebido.tipo == FRAME_FRIEND_REQUEST
    assert recebido.corpo == pedido.corpo

    aceite = par.b.aceitar_amizade(par.identidade_b.seed, recebido.corpo)
    assert len(aceite.corpo) == 177
    assert aceite.corpo[0] == 0x01, "versão no primeiro byte do corpo"

    recebido_a = par.a.receber(5000)
    assert recebido_a.tipo == FRAME_FRIEND_ACCEPT
    assert recebido_a.corpo == aceite.corpo

    confirmadas = par.a.confirmar_amizade(
        par.identidade_a.seed, recebido_a.corpo
    )
    assert confirmadas.publica == par.identidade_b.pub
    assert aceite.chaves.publica == par.identidade_a.pub
    # Ambos calculam o MESMO k1 e veem as chaves do outro trocadas.
    assert confirmadas.k1 == aceite.chaves.k1
    assert confirmadas.k5_par == aceite.chaves.k5_proprio
    assert confirmadas.k5_proprio == aceite.chaves.k5_par

    assert par.a.estado().amigos == 1
    assert par.b.estado().amigos == 1


def test_recusa_de_handshake_rejeitada_pelo_par(par: Cenario) -> None:
    """PEDIR → RECEBER → RECUSAR → RECEBER(FRIEND_REJECT) em A."""
    par.a.pedir_amizade(par.identidade_a.seed, par.identidade_b.pub)
    recebido = par.b.receber(5000)
    assert recebido.tipo == FRAME_FRIEND_REQUEST

    nonce = recebido.corpo[FATIA_NONCE]
    recusa = par.b.recusar_amizade(par.identidade_b.seed, nonce)
    assert len(recusa) == 81

    recebido_a = par.a.receber(5000)
    assert recebido_a.tipo == FRAME_FRIEND_REJECT
    assert recebido_a.corpo == recusa
    # ``FRIEND_REJECT`` = versao(1) ‖ nonce(16) ‖ sig(64).
    assert recebido_a.corpo[1:17] == nonce
    assert recebido_a.corpo[0] == 0x01

    # A recusa nunca cria amizade.
    assert par.a.estado().amigos == 0
    assert par.b.estado().amigos == 0


def test_fechar_e_religar_pelo_relay(par: Cenario) -> None:
    """``FECHAR`` limpa a ligação; sem ligação ``ENVIAR`` falha; ligar de novo."""
    par.a.fechar()
    assert par.a.estado().ligado is False

    with pytest.raises(ErroDaemon) as capturado:
        par.a.enviar(b"sem ligacao")
    assert capturado.value.codigo == 0x0A  # NaoLigado

    par.a.ligar(
        MODO_RELAY,
        par.endpoint,
        "",
        par.identidade_a.pub,
        par.identidade_b.pub,
    )
    assert par.a.estado().ligado is True
    envelope = Pipeline(par.a).cifrar("de novo", CHAVES, par.identidade_a)
    par.a.enviar(envelope)
    frame = par.b.receber(5000)
    assert frame.tipo == FRAME_CHAT
    assert (
        Pipeline(par.b).decifrar(frame.corpo, CHAVES, par.identidade_a.pub)
        == "de novo"
    )
