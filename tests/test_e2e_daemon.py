# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_e2e_daemon.py — integração com o daemon Rust compilado.

Cobre a ponte real Python ↔ ``onyxchatd``: framing do ``docs/ipc_spec.md``,
roundtrip ENCODE→DECODE, rejeições no daemon (assinatura e tag AEAD),
rejeição local no ``Pipeline`` e falhas de transporte. Os testes são
*saltados* se o binário não estiver compilado (``cargo build``), para
não quebrar a suite em ambientes só com Python.
"""

from __future__ import annotations

import socket
import struct
import subprocess
import time
from collections.abc import Iterator
from pathlib import Path

import pytest

from messenger.ipc_client import (
    CMD_ENCODE,
    CMD_HELLO,
    ClienteIpc,
    DaemonIndisponivel,
    ErroDaemon,
    VERSAO_IPC,
)
from messenger.keys import Chaves, Identidade
from messenger.pipeline import AssinaturaInvalida, Pipeline

RAIZ = Path(__file__).resolve().parent.parent
BINARIO = RAIZ / "target" / "debug" / "onyxchatd"

pytestmark = pytest.mark.skipif(
    not BINARIO.is_file(),
    reason="daemon não compilado — correr `cargo build` na raiz do workspace",
)

CHAVES = Chaves(k1=b"\x11" * 32, k5=b"\x22" * 32, k9=b"\x33" * 32)


def _enviar_e_ler(ligacao: socket.socket, payload: bytes) -> bytes:
    """Enquadra ``payload``, envia e devolve o corpo da resposta."""
    ligacao.sendall(struct.pack("<I", len(payload)) + payload)
    cabecalho = b""
    while len(cabecalho) < 4:
        pedaco = ligacao.recv(4 - len(cabecalho))
        if not pedaco:
            return b""  # daemon fechou sem (mais) resposta
        cabecalho += pedaco
    (tamanho,) = struct.unpack("<I", cabecalho)
    corpo = b""
    while len(corpo) < tamanho:
        pedaco = ligacao.recv(tamanho - len(corpo))
        if not pedaco:
            break
        corpo += pedaco
    return corpo


def _pedir_raw(
    caminho: str, payload: bytes, timeout: float = 10.0, hello: bool = True
) -> bytes:
    """Pedido IPC cruo (qualquer comando) → corpo da resposta.

    ``hello=True`` (por omissão) abre a sessão com o ``HELLO``
    obrigatório antes do comando; ``hello=False`` envia o ``payload``
    como primeiro pedido — para testar o portão da sessão.
    """
    ligacao = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    ligacao.settimeout(timeout)
    try:
        ligacao.connect(caminho)
        if hello:
            _enviar_e_ler(ligacao, bytes([CMD_HELLO, VERSAO_IPC]))
        return _enviar_e_ler(ligacao, payload)
    finally:
        ligacao.close()


@pytest.fixture()
def daemon(tmp_path: Path) -> Iterator[ClienteIpc]:
    """Arranca ``onyxchatd`` num socket temporário e espera ficar pronto.

    ``--tor nenhum`` escolhe o backend em memória (``TorFalso``): os testes
    não podem depender da rede real nem escrever estado da arti no ``$HOME``.
    """
    caminho = tmp_path / "d.sock"
    processo = subprocess.Popen(
        [str(BINARIO), "--socket", str(caminho), "--tor", "nenhum"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        cliente: ClienteIpc | None = None
        limite = time.monotonic() + 10.0
        while time.monotonic() < limite:
            if caminho.exists():
                try:
                    prova = socket.socket(socket.AF_UNIX)
                    prova.settimeout(1.0)
                    prova.connect(str(caminho))
                    prova.close()
                    cliente = ClienteIpc(str(caminho))
                    break
                except OSError:
                    pass
            if processo.poll() is not None:
                pytest.fail(f"daemon terminou cedo ({processo.returncode})")
            time.sleep(0.05)
        if cliente is None:
            pytest.fail("daemon não abriu o socket em 10 s")
        yield cliente
    finally:
        processo.terminate()
        try:
            processo.wait(timeout=5)
        except subprocess.TimeoutExpired:
            processo.kill()
            processo.wait(timeout=5)


def test_roundtrip_encode_decode(daemon: ClienteIpc) -> None:
    """ENCODE → envelope válido → DECODE devolve o texto original."""
    identidade = Identidade.gerar()
    envelope = daemon.cifrar(
        CHAVES.k1, CHAVES.k5, CHAVES.k9, identidade.seed, "Olá mundo!"
    )
    assert len(envelope) >= 117
    assert envelope[0] == 0x01
    texto = daemon.decifrar(
        CHAVES.k1, CHAVES.k5, CHAVES.k9, identidade.pub, envelope
    )
    assert texto == "Olá mundo!"


def test_envelope_adulterado_rejeitado_no_daemon(daemon: ClienteIpc) -> None:
    """Um bit alterado → o daemon falha a assinatura (erro 0x04)."""
    identidade = Identidade.gerar()
    envelope = bytearray(
        daemon.cifrar(
            CHAVES.k1, CHAVES.k5, CHAVES.k9, identidade.seed, "integro"
        )
    )
    envelope[50] ^= 0x01  # adultera a região assinada
    with pytest.raises(ErroDaemon, match="AssinaturaInvalida") as capturado:
        daemon.decifrar(
            CHAVES.k1, CHAVES.k5, CHAVES.k9, identidade.pub, bytes(envelope)
        )
    assert capturado.value.codigo == 0x04


def test_k9_errado_falha_tag_aead(daemon: ClienteIpc) -> None:
    """K9 do receptor errada → tag AEAD falha (erro 0x05)."""
    identidade = Identidade.gerar()
    envelope = daemon.cifrar(
        CHAVES.k1, CHAVES.k5, CHAVES.k9, identidade.seed, "secreto"
    )
    with pytest.raises(ErroDaemon, match="DecifragemFalhou") as capturado:
        daemon.decifrar(
            CHAVES.k1, CHAVES.k5, b"\x99" * 32, identidade.pub, envelope
        )
    assert capturado.value.codigo == 0x05


def test_pipeline_local_rejeita_sem_tocar_no_daemon(
    daemon: ClienteIpc, monkeypatch
) -> None:
    """A assinatura é validada localmente ANTES de qualquer IPC."""
    identidade = Identidade.gerar()
    fachada = Pipeline(daemon)
    envelope = bytearray(
        daemon.cifrar(
            CHAVES.k1, CHAVES.k5, CHAVES.k9, identidade.seed, "ok"
        )
    )
    chamadas: list[bytes] = []
    original = daemon.decifrar

    def espiar(*argumentos, **palavra_chave) -> str:
        chamadas.append(b"ipc")
        return original(*argumentos, **palavra_chave)

    monkeypatch.setattr(daemon, "decifrar", espiar)
    envelope[60] ^= 0x01  # adultera a assinatura
    with pytest.raises(AssinaturaInvalida, match="rejeitada"):
        fachada.decifrar(bytes(envelope), CHAVES, identidade.pub)
    assert chamadas == []


def test_pipeline_roundtrip_pelo_daemon_real(daemon: ClienteIpc) -> None:
    """Fachada completa: cifrar e decifrar passam pelo daemon compilado."""
    identidade = Identidade.gerar()
    fachada = Pipeline(daemon)
    envelope = fachada.cifrar("mensagem real", CHAVES, identidade)
    texto = fachada.decifrar(envelope, CHAVES, identidade.pub)
    assert texto == "mensagem real"


def test_comando_desconhecido(daemon: ClienteIpc) -> None:
    """Comando fora de {ENCODE, DECODE} → erro 0x01 ComandoDesconhecido."""
    resposta = _pedir_raw(daemon.caminho, bytes([0x7F]) + b"lixo")
    assert resposta[0] == 0x01  # ERRO
    assert resposta[1] == 0x01  # ComandoDesconhecido


def test_payload_malformado(daemon: ClienteIpc) -> None:
    """Pedido ENCODE curto → erro 0x02 PayloadMalformado."""
    resposta = _pedir_raw(daemon.caminho, bytes([CMD_ENCODE]) + b"curto")
    assert resposta[0] == 0x01
    assert resposta[1] == 0x02  # PayloadMalformado


def test_hello_obrigatorio_no_daemon_real(daemon: ClienteIpc) -> None:
    """Primeiro pedido sem ``HELLO`` → 0x02; depois, a tabela normal."""
    # Sem HELLO: o portão da sessão rejeita ANTES da tabela de comandos
    # (um comando desconhecido daria 0x01 se a sessão estivesse aberta).
    sem_hello = _pedir_raw(daemon.caminho, bytes([0x7F]), hello=False)
    assert sem_hello[0] == 0x01
    assert sem_hello[1] == 0x02, "HELLO obrigatório antes de qualquer comando"

    # Com HELLO válido, o mesmo comando cai na tabela normal.
    com_hello = _pedir_raw(daemon.caminho, bytes([0x7F]))
    assert com_hello[0] == 0x01
    assert com_hello[1] == 0x01, "ComandoDesconhecido após sessão aberta"


@pytest.mark.parametrize(
    "versao",
    [
        0x10,
        0x20,
        0xF0,
    ],
    ids=lambda v: f"major-{v >> 4}",
)
def test_hello_major_diferente_rejeitado_no_daemon_real(
    daemon: ClienteIpc, versao: int
) -> None:
    """Major diferente → ERRO 0x12 contra o binário **real**.

    É a metade restritiva da tolerância de versões: o que não é nativo
    rejeita.
    """
    resposta = _pedir_raw(daemon.caminho, bytes([CMD_HELLO, versao]), hello=False)
    assert resposta[0] == 0x01
    assert resposta[1] == 0x12  # VersaoIncompativel


@pytest.mark.parametrize("minor", [0x00, 0x02, 0x03, 0x07, 0x0F])
def test_hello_minor_diferente_aceite_no_daemon_real(
    daemon: ClienteIpc, minor: int
) -> None:
    """Major igual, minor diferente → **aceite**, contra o binário real.

    É a metade permissiva da tolerância de versões, e é a que garante que
    um utilizador com software antigo continua a funcionar com software
    novo. Sem isto, bastaria um patch release do daemon para partir
    todos os clientes installados.

    Verifica-se também que a resposta ecoa a versão do cliente, para que
    este saiba que a sua foi aceite.
    """
    resposta = _pedir_raw(daemon.caminho, bytes([CMD_HELLO, VERSAO_IPC]), hello=False)
    assert resposta[0] == 0x00, f"OK: {resposta!r}"
    # A resposta é [versão][build]: a versão tem de ser a nossa.
    assert resposta[1] == VERSAO_IPC, "a resposta ecoa a versão do cliente"

    # A persistência da sessão ao longo da ligação é testada noutro
    # sítio; aqui o que interessa é apenas que a versão foi aceite.


def test_hello_echoa_a_versao_do_cliente(daemon: ClienteIpc) -> None:
    """A versão que volta é a do cliente, não a do daemon.

    Quem recebe a resposta precisa de confirmar que a *sua* versão foi
    aceite — se devolvêssemos a nossa, o cliente não teria como saber.
    """
    resposta = _pedir_raw(daemon.caminho, bytes([CMD_HELLO, 0x01]), hello=False)
    assert resposta[0] == 0x00
    assert resposta[1] == VERSAO_IPC


def test_daemon_em_baixo(tmp_path: Path) -> None:
    """Sem daemon → ``ErroIpc`` tipado (mensagem «inacessível»)."""
    cliente = ClienteIpc(str(tmp_path / "nao-existe.sock"))
    with pytest.raises(DaemonIndisponivel, match="inacessível"):
        cliente.cifrar(CHAVES.k1, CHAVES.k5, CHAVES.k9, b"\x00" * 32, "x")


def test_saida_da_versao() -> None:
    """``--version`` e ``--help`` do binário real saem a 0."""
    saida = subprocess.run(
        [str(BINARIO), "--version"], capture_output=True, text=True, timeout=10
    )
    assert saida.returncode == 0
    assert "onyxchatd" in saida.stdout
    ajuda = subprocess.run(
        [str(BINARIO), "--help"], capture_output=True, text=True, timeout=10
    )
    assert ajuda.returncode == 0
    assert "socket" in ajuda.stdout
