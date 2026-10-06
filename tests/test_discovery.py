# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_discovery.py — servidor de descoberta (server.discovery_server).

Cobre o núcleo puro (``Descoberta`` com relógio injetado), o transport
HTTP completo (200/400/404) e o cliente ``consultar_descoberta``
(incluindo 500 e servidor em baixo). Nenhum teste dorme: TTL com
relógio falso e portas efémeras.
"""

from __future__ import annotations

import json
import socket
import threading
from collections.abc import Iterator
from contextlib import contextmanager

import pytest
import urllib.error
import urllib.request

from server.discovery_server import (
    Descoberta,
    DescobertaInvalida,
    consultar_descoberta,
    criar_servidor,
)

#: Endereço .onion v3 válido (56 caracteres base32 + ".onion").
ONION = "a" * 56 + ".onion"
ONION2 = "b" * 56 + ".onion"
ID = "amigo.local"


def test_ttl_padrao_invalido() -> None:
    """TTL padrão ≤ 0 → ``DescobertaInvalida``."""
    with pytest.raises(DescobertaInvalida, match="positivo"):
        Descoberta(ttl_padrao=0)
    with pytest.raises(DescobertaInvalida, match="positivo"):
        Descoberta(ttl_padrao=-1)


def test_registar_consultar_e_substituir() -> None:
    """Registo devolve o .onion; re-registo substitui a entrada."""
    d = Descoberta(ttl_padrao=60.0)
    assert d.total == 0
    d.registar(ID, ONION)
    assert d.consultar(ID) == ONION
    assert d.total == 1
    d.registar(ID, ONION2)  # substitui
    assert d.consultar(ID) == ONION2
    assert d.total == 1


def test_registar_rejeita_identificador() -> None:
    """ID vazio, com espaços, demasiado longo ou não-str → ``DescobertaInvalida``."""
    d = Descoberta()
    with pytest.raises(DescobertaInvalida, match="identificador"):
        d.registar("espaco mau", ONION)
    with pytest.raises(DescobertaInvalida, match="identificador"):
        d.registar("", ONION)
    with pytest.raises(DescobertaInvalida, match="identificador"):
        d.registar("x" * 65, ONION)
    with pytest.raises(DescobertaInvalida, match="identificador"):
        d.registar(123, ONION)  # type: ignore[arg-type]


def test_registar_rejeita_onion_invalido() -> None:
    """Onion curto, com maiúsculas ou não-str → ``DescobertaInvalida``."""
    d = Descoberta()
    with pytest.raises(DescobertaInvalida, match="onion"):
        d.registar(ID, "a" * 55 + ".onion")
    with pytest.raises(DescobertaInvalida, match="onion"):
        d.registar(ID, "A" * 56 + ".onion")  # base32 minúsculas só
    with pytest.raises(DescobertaInvalida, match="onion"):
        d.registar(ID, "a" * 56)  # sem sufixo .onion
    with pytest.raises(DescobertaInvalida, match="onion"):
        d.registar(ID, 42)  # type: ignore[arg-type]


def test_registar_ttl_invalido() -> None:
    """TTL por pedido ≤ 0 → ``DescobertaInvalida``."""
    d = Descoberta()
    with pytest.raises(DescobertaInvalida, match="TTL"):
        d.registar(ID, ONION, ttl=0)
    with pytest.raises(DescobertaInvalida, match="TTL"):
        d.registar(ID, ONION, ttl=-5)


def test_consultar_desconhecido_e_expirado() -> None:
    """Desconhecido → ``None``; expirado → ``None`` e remoção imediata."""
    agora = [0.0]
    d = Descoberta(ttl_padrao=10.0, relogio=lambda: agora[0])
    assert d.consultar("nunca-visto") is None
    d.registar(ID, ONION)
    agora[0] = 11.0  # passou o TTL
    assert d.consultar(ID) is None
    assert d.total == 0  # a entrada expirada foi apagada


def test_consultar_ainda_vivo() -> None:
    """Dentro do TTL o .onion continua disponível."""
    agora = [0.0]
    d = Descoberta(ttl_padrao=10.0, relogio=lambda: agora[0])
    d.registar(ID, ONION, ttl=5.0)
    agora[0] = 4.9
    assert d.consultar(ID) == ONION


def test_limpar_remove_só_expiradas() -> None:
    """``limpar`` devolve o nº removido e preserva as vivas."""
    agora = [0.0]
    d = Descoberta(ttl_padrao=100.0, relogio=lambda: agora[0])
    d.registar("velho", ONION, ttl=5.0)
    d.registar("velho2", ONION2, ttl=5.0)
    d.registar("vivo", ONION, ttl=500.0)
    agora[0] = 6.0
    assert d.limpar() == 2
    assert d.total == 1
    assert d.consultar("vivo") == ONION
    assert d.limpar() == 0  # nada mais a limpar


# ---------------------------------------------------------------------
# Transport HTTP
# ---------------------------------------------------------------------


@pytest.fixture()
def servidor() -> Iterator[tuple[Descoberta, str]]:
    """Servidor HTTP real numa porta efémera, parado ao fim do teste."""
    httpd = criar_servidor(porta=0)
    fio = threading.Thread(target=httpd.serve_forever, daemon=True)
    fio.start()
    host, porta = httpd.server_address
    try:
        yield httpd.descoberta, f"http://{host}:{porta}"  # type: ignore[attr-defined]
    finally:
        httpd.shutdown()
        httpd.server_close()
        fio.join(timeout=3)


@contextmanager
def _http_falso(resposta: bytes) -> Iterator[str]:
    """TCP server que devolve uma resposta HTTP crua uma única vez."""
    ligacao = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    ligacao.bind(("127.0.0.1", 0))
    ligacao.listen(1)
    porta = ligacao.getsockname()[1]

    def atender() -> None:
        try:
            cliente, _ = ligacao.accept()
            with cliente:
                cliente.recv(65536)
                cliente.sendall(resposta)
        except OSError:
            pass

    fio = threading.Thread(target=atender, daemon=True)
    fio.start()
    try:
        yield f"http://127.0.0.1:{porta}"
    finally:
        ligacao.close()
        fio.join(timeout=3)


def _post(base: str, corpo: bytes) -> tuple[int, dict]:
    """POST crua ao ``/registrar``; devolve (estado, corpo JSON)."""
    pedido = urllib.request.Request(
        base + "/registrar", data=corpo, method="POST"
    )
    try:
        with urllib.request.urlopen(pedido, timeout=5) as resposta:
            return resposta.status, json.loads(resposta.read().decode("utf-8"))
    except urllib.error.HTTPError as erro:
        return erro.code, json.loads(erro.read().decode("utf-8"))


def test_http_registrar_e_consultar(servidor) -> None:
    """POST /registrar 200 seguido de GET /onion/<id> 200."""
    descoberta, base = servidor
    estado, corpo = _post(
        base, json.dumps({"id": ID, "onion": ONION, "ttl": 30}).encode()
    )
    assert (estado, corpo) == (200, {"ok": True})
    assert descoberta.total == 1
    assert consultar_descoberta(base, ID) == ONION
    assert consultar_descoberta(base + "/", ID) == ONION  # barra extra ok


def test_http_get_desconhecido_404(servidor) -> None:
    """ID desconhecido → 404 → ``consultar_descoberta`` devolve ``None``."""
    _descoberta, base = servidor
    assert consultar_descoberta(base, "fantasma") is None


def test_http_rota_get_errada(servidor) -> None:
    """Caminho que não é ``/onion/<id>`` → 404 «rota desconhecida»."""
    _descoberta, base = servidor
    # O cliente acrescenta /onion/<id> ao base — um base com caminho
    # próprio produz um path com ≠ 2 segmentos, ramo `else` do do_GET.
    assert consultar_descoberta(base + "/outra/rota", ID) is None


def test_http_post_rota_errada_404(servidor) -> None:
    """POST fora de ``/registrar`` → 404 «rota desconhecida»."""
    _descoberta, base = servidor
    estado, corpo = _post(base + "/nada", b"{}")
    assert estado == 404
    assert "rota" in corpo["erro"]


def test_http_post_sem_content_length_400(servidor) -> None:
    """POST sem ``Content-Length`` → 400 «comprimento em falta»."""
    _descoberta, base = servidor
    host, porta = base.split("//")[1].split(":")
    crua = socket.create_connection((host, int(porta)), timeout=5)
    with crua:
        crua.sendall(
            b"POST /registrar HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        )
        # O servidor fecha após responder (HTTP/1.0) → lê até EOF.
        crua.settimeout(3)
        pedacos: list[bytes] = []
        while True:
            try:
                pedaco = crua.recv(4096)
            except socket.timeout:
                break
            if not pedaco:
                break
            pedacos.append(pedaco)
        resposta = b"".join(pedacos)
    assert b" 400 " in resposta
    assert "comprimento" in resposta.decode("utf-8", "replace")


def test_http_post_corpos_invalidos_400(servidor) -> None:
    """JSON inválido, UTF-8 inválido, campos em falta, tipos e TTL errados."""
    _descoberta, base = servidor
    casos: list[bytes] = [
        b"nao-json",  # JSONDecodeError
        b"\xff\xfe",  # UnicodeDecodeError
        json.dumps({"onion": ONION}).encode(),  # KeyError ("id")
        json.dumps([1, 2]).encode(),  # TypeError (lista sem [...])
        json.dumps({"id": "mau id", "onion": ONION}).encode(),  # DescobertaInvalida
        json.dumps({"id": ID, "onion": "curto"}).encode(),  # onion inválido
        json.dumps({"id": ID, "onion": ONION, "ttl": 0}).encode(),  # TTL
        json.dumps({"id": ID, "onion": ONION, "ttl": "abc"}).encode(),  # TypeError
    ]
    for corpo in casos:
        estado, resposta = _post(base, corpo)
        assert estado == 400, corpo
        assert "erro" in resposta


def test_http_log_silenciado(servidor) -> None:
    """Pedidos não imprimem o log do ``http.server`` (linhas em ``pass``)."""
    _descoberta, base = servidor
    # Qualquer pedido activa ``log_message``; saída nenhuma esperada.
    assert consultar_descoberta(base, "inexistente") is None


def test_criar_servidor_com_e_sem_descoberta() -> None:
    """``criar_servidor`` instancia um ``Descoberta`` ou aceita o dado."""
    httpd = criar_servidor(porta=0)
    try:
        assert isinstance(httpd.descoberta, Descoberta)  # type: ignore[attr-defined]
    finally:
        httpd.server_close()
    proprio = Descoberta(ttl_padrao=9.0)
    httpd2 = criar_servidor(porta=0, descoberta=proprio)
    try:
        assert httpd2.descoberta is proprio  # type: ignore[attr-defined]
    finally:
        httpd2.server_close()


def test_consultar_devolve_onion_do_json(servidor) -> None:
    """A resposta 200 é lida e o campo ``onion`` extraído (linha final)."""
    _descoberta, base = servidor
    _post(base, json.dumps({"id": ID, "onion": ONION}).encode())
    assert consultar_descoberta(base, ID) == ONION


def test_consultar_500_vira_descoberta_invalida() -> None:
    """HTTP ≠ 404 (aqui 500) → ``DescobertaInvalida`` com o estado."""
    with _http_falso(
        b"HTTP/1.1 500 Erro Interno\r\nContent-Length: 0\r\n\r\n"
    ) as base:
        with pytest.raises(DescobertaInvalida, match="HTTP 500"):
            consultar_descoberta(base, ID)


def test_consultar_servidor_em_baixo() -> None:
    """Sem serviço à escuta → ``DescobertaInvalida`` «inacessível»."""
    # Porta efémera já fechada: reserva e liberta de imediato.
    reserva = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    reserva.bind(("127.0.0.1", 0))
    porta = reserva.getsockname()[1]
    reserva.close()
    with pytest.raises(DescobertaInvalida, match="inacessível"):
        consultar_descoberta(f"http://127.0.0.1:{porta}", ID)
