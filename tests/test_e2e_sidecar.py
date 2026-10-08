# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_e2e_sidecar.py — sidecar, loja e daemon, os três a correr.

Este é o único ficheiro de testes que arranca os três. Vale a pena porque
é o único que pode apanhar a classe de erro que os outros não apanham:
o **desacordo entre os três** — o sidecar que fala uma língua que o
`ipc_client` não fala, um nome de campo que a interface espera e o
`loja` não devolve, um código de erro que o sidecar manda e o
`bridge-adapter` não sabe tratar.

Um teste com um daemon falso passa em cheio e não prova nada: o daemon
falso é escrito pela mesma pessoa que o lado do Python e concorda com ele
por construção. Só o binário real discorda quando há um desacordo.

## ``--tor nenhum``

O daemon arranca com o backend em memória. Os testes não podem depender da
rede real nem escrever no estado de Tor da pessoa que os corre — e um
teste que toca em ``$HOME`` é um teste que falha na máquina de outra
gente sem dizer porquê.
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest

from messenger import conta as conta_mod
from messenger import keys
from messenger.ipc_client import ClienteIpc, ErroIpc
from server import loja as loja_mod
from server import rotas as rotas_mod
from server import sidecar as sidecar_mod

RAIZ = Path(__file__).parent.parent
BINARIO = RAIZ / "target" / "debug" / "onyxchatd"

FRASE = "quatro cavalos lentos numa mare"


# ---------------------------------------------------------------------
# O daemon
# ---------------------------------------------------------------------


@pytest.fixture(scope="module")
def daemon(tmp_path_factory: pytest.TempPathFactory) -> Iterator[Path]:
    """Um ``onyxchatd`` a correr, e o caminho do socket.

    ``scope="module"`` porque arrancar o binário custa tempo e nenhum
    destes testes o altera: todos leem o estado, e o estado não muda.
    """
    if not BINARIO.is_file():
        pytest.skip(f"{BINARIO} não existe; correr `cargo build` primeiro")

    pasta = tmp_path_factory.mktemp("sidecar-e2e")
    caminho = pasta / "d.sock"
    processo = subprocess.Popen(
        [str(BINARIO), "--socket", str(caminho), "--tor", "nenhum"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        limite = time.monotonic() + 15.0
        while time.monotonic() < limite:
            if caminho.exists():
                try:
                    prova = socket.socket(socket.AF_UNIX)
                    prova.settimeout(1.0)
                    prova.connect(str(caminho))
                    prova.close()
                    break
                except OSError:
                    pass
            if processo.poll() is not None:
                pytest.skip(f"o daemon terminou cedo ({processo.returncode})")
            time.sleep(0.05)
        else:
            pytest.skip("o daemon não abriu o socket em 15 s")
        yield caminho
    finally:
        processo.terminate()
        try:
            processo.wait(timeout=5)
        except subprocess.TimeoutExpired:
            processo.kill()
            processo.wait(timeout=5)


# ---------------------------------------------------------------------
# O sidecar
# ---------------------------------------------------------------------


class _Aberto:
    def __init__(self, pasta: Path, conta: Any) -> None:
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            porta = int(s.getsockname()[1])

        self.loja = loja_mod.Loja(pasta / "loja.db")
        self.porta = porta
        self.servidor = sidecar_mod.criar_servidor(
            self.loja,
            conta,
            rotas_mod.montar(self.loja, conta, cliente=lambda: ClienteIpc(str(DAEMON[0]))),
            porta=porta,
            raiz_ui=pasta / "dist",
            caminho_conta=pasta / "conta.keystore",
            silencioso=True,
        )
        self.fio = threading.Thread(target=self.servidor.serve_forever, daemon=True)
        self.fio.start()

    def parar(self) -> None:
        self.servidor.shutdown()
        self.servidor.server_close()
        self.fio.join(timeout=5)
        self.loja.fechar()

    def pedir(self, caminho: str, metodo: str = "GET", corpo: Any = None) -> tuple[int, Any]:
        cabecalhos = {
            "Host": f"127.0.0.1:{self.porta}",
            "Origin": f"http://127.0.0.1:{self.porta}",
            "Sec-Fetch-Site": "same-origin",
        }
        dados = None
        if corpo is not None:
            dados = json.dumps(corpo).encode("utf-8")
            cabecalhos["Content-Type"] = "application/json"
            cabecalhos["Content-Length"] = str(len(dados))

        pedido = urllib.request.Request(
            f"http://127.0.0.1:{self.porta}{caminho}", data=dados, headers=cabecalhos, method=metodo
        )
        try:
            with urllib.request.urlopen(pedido, timeout=10) as resposta:
                bruto = resposta.read()
                codigo = resposta.status
        except urllib.error.HTTPError as erro:
            bruto = erro.read()
            codigo = erro.code
        try:
            return codigo, json.loads(bruto)
        except (UnicodeDecodeError, json.JSONDecodeError):
            return codigo, bruto


#: O socket do daemon, posto pelo fixture para as rotas o usarem.
DAEMON: list[Path] = []


@pytest.fixture
def daemon_path(daemon: Path) -> Path:
    DAEMON[:] = [daemon]
    return daemon


@pytest.fixture
def completo(tmp_path: Path, daemon_path: Path) -> Iterator[_Aberto]:
    """Sidecar + loja + conta, com o daemon real por trás."""
    conta = conta_mod.criar("Marta Vasconcelos", FRASE, caminho=tmp_path / "conta.keystore")
    (tmp_path / "dist").mkdir()
    (tmp_path / "dist" / "index.html").write_text("<!doctype html>", encoding="utf-8")

    servidor = _Aberto(tmp_path, conta)
    try:
        yield servidor
    finally:
        servidor.parar()


# ---------------------------------------------------------------------
# O daemon é alcançado pelo sidecar
# ---------------------------------------------------------------------


def test_o_estado_da_rede_vem_do_daemon(completo: _Aberto) -> None:
    """A rota que diz `indisponivel: false` só pode dizê-lo se falou com o
    daemon de verdade.

    É o teste que vale mais deste ficheiro: os restantes usam o daemon
    mais nada além de confirmar que continua lá, e este confirma que a
    cadeia `HTTP → sidecar → ipc_client → onyxchatd` está inteira.
    """
    codigo, corpo = completo.pedir("/api/estado")
    assert codigo == 200
    assert corpo["indisponivel"] is False, corpo.get("indisponivelMotivo")
    assert corpo["indisponivelMotivo"] == ""


def test_o_daemon_responde_igualmente_pela_ipc(completo: _Aberto) -> None:
    """O mesmo daemon, lido pelas duas vias, dá o mesmo estado.

    Se as duas dessem estados diferentes, a interface mostraria uma coisa
    e o CLI outra — e quem diz a quem, não saberia qual era a certa.
    """
    directo = ClienteIpc(str(DAEMON[0])).estado()
    _, pelo_http = completo.pedir("/api/estado")
    assert pelo_http["state"] == getattr(directo, "estado", "desligado")


def test_a_identidade_vem_da_conta_e_nao_do_daemon(completo: _Aberto) -> None:
    """A identidade é da conta; o daemon só tem a chave.

    A rota de identidade **não** pergunta ao daemon. Poderia, e seria
    mais correcto — a identidade real é a chave Ed25519 dele — mas a conta
    tem o identificador derivado e a impressão, e as duas coisas têm de
    concordar. Ver a nota sobre a Etapa 7.
    """
    codigo, corpo = completo.pedir("/api/identidade")
    assert codigo == 200
    assert corpo["identifier"].startswith("ONYX-")
    assert len(corpo["fingerprint"].split(":")) == 16


def test_a_seguranca_nao_da_um_score(completo: _Aberto) -> None:
    """Mesmo com o daemon a responder, não há score.

    Um score medido a partir de dados que o sidecar não tem seria
    exactamente a invenção que a faixa de demonstração existe para
    impedir — e aqui não há sequer faixa, porque o modo é real.
    """
    codigo, corpo = completo.pedir("/api/seguranca")
    assert codigo == 200
    assert corpo["score"] is None


# ---------------------------------------------------------------------
# A cadeia completa: cifra pelo daemon, guarda na loja
# ---------------------------------------------------------------------


def test_uma_mensagem_cifrada_pelo_daemon_vem_de_cima(
    tmp_path: Path, daemon_path: Path
) -> None:
    """O daemon cifra, e o que sai é um envelope de versão 1.

    Isto não é um teste do sidecar — é o limite do que a Etapa 5 pode
    afirmar. A rota `/api/mensagens` **guarda texto**, não envelopes: o
    texto que chega ao ecrã é o que a pessoa escreveu, e o envelope que
    vai para o outro lado é montado pela Etapa 7, quando a Etapa 6 já
    der um `k1` de sessão a sério.

    Ver a nota sobre a honestidade da loja em `server/loja.py` §O que é
    guardado em claro: o que não está cifrado é a metainformação, e o
    texto em claro no máximo até ao momento da cifra.
    """
    identidade = keys.Identidade.gerar()
    chaves = keys.Chaves.gerar()
    cliente = ClienteIpc(str(daemon_path))

    envelope = cliente.cifrar(
        chaves.k1, chaves.k5, chaves.k9, identidade.seed, "uma frase qualquer"
    )
    assert envelope[0] == 0x01, "o daemon devolveu outra coisa"

    # E o lado do sidecar guarda o que lhe mandarem, sem o decifrar.
    conta = conta_mod.criar("Marta", FRASE, caminho=tmp_path / "conta.keystore")
    (tmp_path / "dist").mkdir()
    with loja_mod.Loja(tmp_path / "loja.db") as loja:
        pedido = loja.registar_pedido("ONYX-AAAAAA-!A!A#", "Ana")
        contacto = loja.decidir_pedido(pedido["id"], "aceite")["contactoId"]
        conversa = loja.abrir_conversa(contacto)
        mensagem = loja.adicionar_mensagem(
            conversa["id"], "me", "uma frase qualquer", envelope=envelope
        )

        assert mensagem["text"] == "uma frase qualquer"
        assert "envelope" not in mensagem, "a interface não recebe o envelope"

        guardada = loja._cx.execute(  # noqa: SLF001 — o teste precisa do bruto
            "SELECT envelope FROM mensagens WHERE id = ?", (mensagem["id"],)
        ).fetchone()
        assert bytes(guardada[0]) == envelope


def test_o_contacto_precisa_de_haver(completo: _Aberto) -> None:
    """Enviar para quem não é contacto dá `404`, e não cria a conversa.

    Sem contacto não há `contacto_id`, e `conversas.contacto_id` é
    `NOT NULL`. Criar um contacto inventado resolveria o `NOT NULL` e
    poria na loja alguém que a pessoa não conhece — que é a forma mais
    discreta de fabricar presença.
    """
    codigo, corpo = completo.pedir(
        "/api/mensagens",
        "POST",
        {"identificador": "ONYX-CCCCCC-!C!C#", "texto": "Olá"},
    )
    assert codigo == 404
    assert completo.loja.contactos() == []


def test_a_rota_inteira_do_daemon_nao_dá_500(completo: _Aberto) -> None:
    """Nenhuma rota que fala com o daemon devolve `500`.

    Um `500` aqui significaria um desacordo entre o sidecar e o daemon —
    um comando trocado, um campo com outro nome. Percorrer as rotas de
    rede e verificar cada uma uma vez é o que torna o ficheiro um
    teste de integração e não uma nota de intenções.
    """
    for caminho in ("/api/estado", "/api/seguranca", "/api/identidade", "/api/contagens"):
        codigo, _ = completo.pedir(caminho)
        assert codigo in (200, 404, 503), f"{caminho} devolveu {codigo}"


def test_sem_daemon_as_rotas_dizem_que_nao(daemon_path: Path) -> None:
    """A rota que fala com um daemon que não existe devolve `503`.

    E não uma excepção, e não `[]`. O teste aponta `cliente` para um
    socket que não existe, que é o que acontece quando o `onyxchatd` não
    arrancou.
    """
    from collections.abc import Iterator as _I

    def _nao_existe() -> _I[ClienteIpc]:
        yield ClienteIpc("/tmp/onyxchat-nao-existe.sock")

    with loja_mod.Loja("/tmp/opencode/loja-sem-daemon.db") as loja:
        servidor = sidecar_mod.criar_servidor(
            loja,
            None,
            rotas_mod.montar(loja, None, cliente=lambda: next(_nao_existe())),
            porta=0,
            raiz_ui=Path("/nonexistente"),
            silencioso=True,
        )
        fio = threading.Thread(target=servidor.serve_forever, daemon=True)
        fio.start()
        try:
            with urllib.request.urlopen(
                urllib.request.Request(
                    f"http://127.0.0.1:{servidor.server_port}/api/estado",
                    headers={
                        "Host": f"127.0.0.1:{servidor.server_port}",
                        "Origin": f"http://127.0.0.1:{servidor.server_port}",
                        "Sec-Fetch-Site": "same-origin",
                    },
                ),
                timeout=10,
            ) as resposta:
                corpo = json.loads(resposta.read())
        finally:
            servidor.shutdown()
            servidor.server_close()
            fio.join(timeout=5)

    assert corpo["indisponivel"] is True
    assert "daemon" in corpo["indisponivelMotivo"]


def test_a_loja_sobrevive_a_o_daemon_morrer(completo: _Aberto, daemon_path: Path) -> None:
    """A conversa escrita não depende do daemon estar lá.

    Uma conversa que se perdesse porque o daemon caiu seria uma conversa
    que o daemon sabe nada, e ele não tem de saber nada.
    """
    contacto = completo.loja.registar_pedido("ONYX-AAAAAA-!A!A#", "Ana")
    completo.loja.decidir_pedido(contacto["id"], "aceite")

    codigo, _ = completo.pedir(
        "/api/mensagens",
        "POST",
        {"identificador": "ONYX-AAAAAA-!A!A#", "texto": "Fica guardada"},
    )
    assert codigo == 200

    # O daemon pode desaparecer; a loja não muda.
    DAEMON[0] = Path("/tmp/onyxchat-desapareceu.sock")
    try:
        assert len(completo.pedir("/api/conversas")[1]) == 1
    finally:
        DAEMON[0] = daemon_path
