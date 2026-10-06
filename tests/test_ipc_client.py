# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_ipc_client.py — cliente UDS do daemon (messenger.ipc_client).

Usa um servidor IPC falso (`tests/apoio.py`) com resposta programável
para cobrir o protocolo completo de ``docs/ipc_spec.md``: framing,
ENCODE/DECODE, estados, códigos de erro e falhas de transporte — sem
depender do daemon compilado.
"""

from __future__ import annotations

import os
import socket
import struct
import threading
from contextlib import contextmanager

import pytest

from messenger import ipc_client
from messenger.ipc_client import (
    CMD_ACEITAR_AMIZADE,
    CMD_CONFIRMAR_AMIZADE,
    CMD_DECODE,
    CMD_ENCODE,
    CMD_ENVIAR,
    CMD_ESTADO,
    CMD_FECHAR,
    CMD_HELLO,
    CMD_LIGAR,
    CMD_OUVIR,
    CMD_PEDIR_AMIZADE,
    CMD_RECEBER,
    CMD_RECUSAR_AMIZADE,
    ERR_VERSAO_INCOMPATIVEL,
    FRAME_CHAT,
    MODO_DIRETO,
    MODO_RELAY,
    TAM_CORPO_ACEITE,
    TAM_CORPO_PEDIDO,
    TAM_CORPO_RECUSA,
    TOR_ATIVO,
    VERSAO_IPC,
    AceiteAmizade,
    ChavesAmizade,
    ClienteIpc,
    DaemonIndisponivel,
    ErroDaemon,
    EstadoDaemon,
    Frame,
    PedidoAmizade,
    PedidoInvalido,
    RespostaInvalida,
)
from tests.apoio import servidor_falso


def test_caminho_socket_padrao_e_por_env(monkeypatch, tmp_path) -> None:
    """``$ONYXCHAT_SOCKET`` tem prioridade; sem ela, o padrão por UID."""
    monkeypatch.delenv("ONYXCHAT_SOCKET", raising=False)
    assert ipc_client.caminho_socket() == f"/tmp/onyxchat-{os.getuid()}.sock"
    monkeypatch.setenv("ONYXCHAT_SOCKET", str(tmp_path / "x.sock"))
    assert ipc_client.caminho_socket() == str(tmp_path / "x.sock")


def test_cliente_usa_env_por_omissao(monkeypatch) -> None:
    """Construído sem caminho, o cliente herda ``$ONYXCHAT_SOCKET``."""
    monkeypatch.setenv("ONYXCHAT_SOCKET", "/tmp/qualquer.sock")
    assert ClienteIpc().caminho == "/tmp/qualquer.sock"


def test_encode_formato_do_pedido_e_resposta(tmp_path) -> None:
    """O pedido ENCODE tem ``[comando][k1][k5][k9][seed][texto]``."""
    recebido: list[bytes] = []
    caminho = tmp_path / "s.sock"
    k1, k5, k9, seed = (bytes([i]) * 32 for i in range(4))
    envelope = b"\x01" + b"e" * 116
    with servidor_falso(
        caminho, lambda pedido: recebido.append(pedido) or b"\x00" + envelope
    ) as socket:
        cliente = ClienteIpc(socket)
        assert cliente.cifrar(k1, k5, k9, seed, "Olá mundo!") == envelope
        assert cliente.cifrar(k1, k5, k9, seed, b"raw") == envelope
    pedido = recebido[0]
    assert pedido[0] == CMD_ENCODE
    assert pedido[1:33] == k1
    assert pedido[33:65] == k5
    assert pedido[65:97] == k9
    assert pedido[97:129] == seed
    assert pedido[129:] == "Olá mundo!".encode("utf-8")
    assert recebido[1][129:] == b"raw"


def test_decode_devolve_utf8(tmp_path) -> None:
    """DECODE devolve o corpo como texto UTF-8."""
    k1, k5, k9, pub = (bytes([i + 9]) * 32 for i in range(4))
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00" + "Olá!".encode("utf-8")) as socket:
        cliente = ClienteIpc(socket)
        texto = cliente.decifrar(k1, k5, k9, pub, b"env")
    assert texto == "Olá!"


def test_decode_nao_utf8_vira_resposta_invalida(tmp_path) -> None:
    """Corpo OK que não é UTF-8 → ``RespostaInvalida`` tipada."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00\xff\xfe") as socket:
        with pytest.raises(RespostaInvalida, match="UTF-8"):
            ClienteIpc(socket).decifrar(bytes(32), bytes(32), bytes(32), bytes(32), b"e")


def test_erro_do_daemon_nomeado(tmp_path) -> None:
    """``ERRO`` vira ``ErroDaemon`` com nome de código e mensagem."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, b"\x01\x05" + "tag inválida".encode("utf-8")
    ) as socket:
        with pytest.raises(ErroDaemon, match="DecifragemFalhou") as capturado:
            ClienteIpc(socket).cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")
    assert capturado.value.codigo == 0x05
    assert "tag inválida" in str(capturado.value)


def test_erro_do_daemon_codigo_desconhecido(tmp_path) -> None:
    """Código fora da tabela → nome genérico ``Erro0xNN``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x01\x42") as socket:
        with pytest.raises(ErroDaemon, match="Erro0x42") as capturado:
            ClienteIpc(socket).cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")
    assert capturado.value.mensagem == ""


def test_erro_do_daemon_sem_mensagem_utf8(tmp_path) -> None:
    """Mensagem de erro não-UTF-8 é ignorada (só o código conta)."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x01\x03\xff\xfe") as socket:
        with pytest.raises(ErroDaemon, match="ChaveInvalida") as capturado:
            ClienteIpc(socket).cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")
    assert capturado.value.mensagem == ""


def test_erro_do_daemon_sem_corpo(tmp_path) -> None:
    """``ERRO`` sem byte de código → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x01") as socket:
        with pytest.raises(RespostaInvalida, match="sem código"):
            ClienteIpc(socket).cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")


def test_estado_desconhecido(tmp_path) -> None:
    """Estado fora de {0x00, 0x01} → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x07qualquer") as socket:
        with pytest.raises(RespostaInvalida, match="desconhecido"):
            ClienteIpc(socket).cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")


def test_enquadramento_comprimento_invalido(tmp_path) -> None:
    """Header de resposta a anunciar 0 bytes → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    # O servidor falso envia sempre [u32][corpo]: devolvemos um corpo que,
    # ao ser interpretado, nunca chega — em vez disso, construímos a
    # resposta "só header" ligando directamente:
    with servidor_falso(caminho, b"") as socket:
        # corpo vazio → o falso envia header com comprimento 0.
        with pytest.raises(RespostaInvalida, match="enquadramento"):
            ClienteIpc(socket).cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")


def test_enquadramento_comprimento_grande(tmp_path) -> None:
    """Header a anunciar payload acima de 16 MiB → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    resposta = b"\x00" * (ipc_client.MAX_PAYLOAD + 1)
    with servidor_falso(caminho, resposta) as socket:
        with pytest.raises(RespostaInvalida, match="enquadramento"):
            ClienteIpc(socket).cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")


def test_resposta_truncada(tmp_path) -> None:
    """O daemon morre no meio da resposta → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, None) as socket:  # type: ignore[arg-type]
        with pytest.raises(RespostaInvalida, match="truncada"):
            ClienteIpc(socket).cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")


def test_daemon_indisponivel(tmp_path) -> None:
    """Sem servidor no caminho → ``DaemonIndisponivel``."""
    cliente = ClienteIpc(str(tmp_path / "nao-existe.sock"))
    with pytest.raises(DaemonIndisponivel, match="inacessível"):
        cliente.cifrar(bytes(32), bytes(32), bytes(32), bytes(32), "x")


def test_interpretar_resposta_vazia() -> None:
    """``_interpretar(b'')`` → ``RespostaInvalida`` (caminho unitário)."""
    with pytest.raises(RespostaInvalida, match="vazia"):
        ClienteIpc._interpretar(b"")


def test_nome_de_erro_desconhecido() -> None:
    """Tabela de nomes cobre os 19 códigos documentados (0x01..0x13)."""
    assert set(ipc_client.NOMES_ERRO) == set(range(0x01, 0x14))
    assert ipc_client.NOMES_ERRO[0x09] == "PayloadGrandeDemais"
    assert ipc_client.NOMES_ERRO[0x11] == "Relay"
    assert ipc_client.NOMES_ERRO[0x13] == "NonceRepetido"
    # O limite deixou de ser 16 MiB escritos à mão: deriva de um
    # orçamento de 1 GB, com o mínimo do protocolo como piso. Este
    # teste fixa a INVARIANTE (servir o maior DECODE) e não o número —
    # um literal aqui voltaria a fixar o valor errado em vez de dizer o
    # que tem de ser verdade.
    assert ipc_client.MAX_PAYLOAD >= ipc_client.MIN_CORPO, (
        f"o limite do cliente tem de servir um DECODE de {ipc_client.MIN_CORPO} B"
    )


def test_tabela_de_estados_tor() -> None:
    """Os três estados do backend Tor têm nome legível."""
    assert set(ipc_client.NOMES_TOR) == set(range(0x00, 0x03))
    assert ipc_client.NOMES_TOR[TOR_ATIVO] == "ativo"


# ---------------------------------------------------------------------
# Rede (Etapa 7) — comandos 0x03..0x08
# ---------------------------------------------------------------------


def test_estado_parseia_resposta(tmp_path) -> None:
    """``ESTADO`` → ``tor ‖ ligado ‖ amigos:u16 ‖ onion`` tipados."""
    pedidos: list[bytes] = []
    corpo = b"\x02\x01" + (513).to_bytes(2, "little") + b"abcd.onion"
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, lambda pedido: pedidos.append(pedido) or b"\x00" + corpo
    ) as socket:
        estado = ClienteIpc(socket).estado()
    assert pedidos == [bytes([CMD_ESTADO])]
    assert estado == EstadoDaemon(
        tor=TOR_ATIVO, ligado=True, amigos=513, onion="abcd.onion"
    )
    assert estado.ligado is True


def test_estado_sem_onion_e_desligado(tmp_path) -> None:
    """Caso fronteira: sem onion (campo vazio) e sem ligação."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00\x00\x00\x00\x00") as socket:
        estado = ClienteIpc(socket).estado()
    assert estado == EstadoDaemon(tor=0x00, ligado=False, amigos=0, onion="")


def test_estado_resposta_curta(tmp_path) -> None:
    """Resposta com menos de 4 bytes → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00\x00\x01") as socket:
        with pytest.raises(RespostaInvalida, match="curta"):
            ClienteIpc(socket).estado()


def test_estado_onion_nao_utf8(tmp_path) -> None:
    """Onion ilegível nunca derruba o cliente — ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00\x00\x00\x00\x00\xff\xfe") as socket:
        with pytest.raises(RespostaInvalida, match="UTF-8"):
            ClienteIpc(socket).estado()


def test_ouvir_devolve_onion(tmp_path) -> None:
    """``OUVIR`` devolve o endereço ``.onion`` como texto."""
    pedidos: list[bytes] = []
    caminho = tmp_path / "s.sock"
    resposta = b"\x00" + b"onyxexample.onion"
    with servidor_falso(
        caminho, lambda pedido: pedidos.append(pedido) or resposta
    ) as socket:
        onion = ClienteIpc(socket).ouvir()
    assert pedidos == [bytes([CMD_OUVIR])]
    assert onion == "onyxexample.onion"


def test_ouvir_nao_utf8(tmp_path) -> None:
    """Endereço não-UTF-8 → ``RespostaInvalida`` (caminho de ``_utf8``)."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00\xff") as socket:
        with pytest.raises(RespostaInvalida, match="UTF-8"):
            ClienteIpc(socket).ouvir()


def test_ligar_formato_do_pedido(tmp_path) -> None:
    """``LIGAR`` = ``modo ‖ u16+endpoint ‖ u16+destino ‖ pub_p ‖ pub_par``."""
    recebido: list[bytes] = []
    pub_a, pub_b = bytes([1]) * 32, bytes([2]) * 32
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, lambda pedido: recebido.append(pedido) or b"\x00"
    ) as socket:
        ClienteIpc(socket).ligar(MODO_RELAY, "127.0.0.1:9001", "par.onion", pub_a, pub_b)
    pedido = recebido[0]
    assert pedido[0] == CMD_LIGAR
    assert pedido[1] == MODO_RELAY
    resto = pedido[2:]
    tam_endpoint = int.from_bytes(resto[:2], "little")
    assert resto[2 : 2 + tam_endpoint] == b"127.0.0.1:9001"
    pos = 2 + tam_endpoint
    tam_destino = int.from_bytes(resto[pos : pos + 2], "little")
    pos += 2
    assert resto[pos : pos + tam_destino] == b"par.onion"
    pos += tam_destino
    assert resto[pos:] == pub_a + pub_b


def test_ligar_modo_invalido(tmp_path) -> None:
    """Modo fora de {0x00, 0x01} é rejeitado ANTES de ir para o daemon."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00") as socket:
        with pytest.raises(PedidoInvalido, match="0x00 ou 0x01"):
            ClienteIpc(socket).ligar(0x07, "", "x.onion", bytes(32), bytes(32))


def test_ligar_pub_curta(tmp_path) -> None:
    """Chave pública fora dos 32 bytes → ``PedidoInvalido``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00") as socket:
        with pytest.raises(PedidoInvalido, match="pub_par"):
            ClienteIpc(socket).ligar(MODO_DIRETO, "", "x.onion", bytes(32), b"curta")


def test_ligar_cadeia_demasiado_longa(tmp_path) -> None:
    """Endpoint acima de 65535 bytes não cabe no prefixo ``u16``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00") as socket:
        with pytest.raises(PedidoInvalido, match="máx. 65535"):
            ClienteIpc(socket).ligar(MODO_DIRETO, "x" * 65536, "", bytes(32), bytes(32))


def test_ligar_resposta_inesperada(tmp_path) -> None:
    """``LIGAR`` devolve corpo → ``RespostaInvalida`` (resposta vazia exigida)."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00\x01\x02") as socket:
        with pytest.raises(RespostaInvalida, match="LIGAR"):
            ClienteIpc(socket).ligar(MODO_DIRETO, "", "x.onion", bytes(32), bytes(32))


def test_enviar_formato_e_resposta(tmp_path) -> None:
    """``ENVIAR`` repassa o envelope intacto e exige resposta vazia."""
    recebido: list[bytes] = []
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, lambda pedido: recebido.append(pedido) or b"\x00"
    ) as socket:
        ClienteIpc(socket).enviar(b"\x01" + b"e" * 20)
    assert recebido == [bytes([CMD_ENVIAR]) + b"\x01" + b"e" * 20]


def test_enviar_sem_ligacao(tmp_path) -> None:
    """O daemon responde ``0x0A NaoLigado`` — vira ``ErroDaemon`` tipado."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x01\x0a" + b"sem ligacao") as socket:
        with pytest.raises(ErroDaemon, match="NaoLigado") as capturado:
            ClienteIpc(socket).enviar(b"envelope")
    assert capturado.value.codigo == 0x0A


def test_receber_devolve_frame(tmp_path) -> None:
    """``RECEBER`` envia ``timeout_ms:u32 LE`` e devolve ``(tipo, corpo)``."""
    recebido: list[bytes] = []
    caminho = tmp_path / "s.sock"
    resposta = b"\x00" + bytes([FRAME_CHAT]) + b"envelope"
    with servidor_falso(
        caminho, lambda pedido: recebido.append(pedido) or resposta
    ) as socket:
        frame = ClienteIpc(socket).receber(250)
    assert recebido == [bytes([CMD_RECEBER]) + (250).to_bytes(4, "little")]
    assert frame == Frame(tipo=FRAME_CHAT, corpo=b"envelope")


def test_receber_timeout_fora_do_intervalo(tmp_path) -> None:
    """``timeout_ms`` tem de caber em ``u32`` — validado antes do IPC."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00\x20") as socket:
        with pytest.raises(PedidoInvalido, match="u32"):
            ClienteIpc(socket).receber(-1)


def test_receber_sem_tipo(tmp_path) -> None:
    """Resposta ``OK`` vazia → ``RespostaInvalida`` (falta o tipo)."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00") as socket:
        with pytest.raises(RespostaInvalida, match="sem tipo"):
            ClienteIpc(socket).receber(0)


def test_fechar_idempotente(tmp_path) -> None:
    """``FECHAR`` é sempre resposta vazia (mesmo sem ligação)."""
    recebido: list[bytes] = []
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, lambda pedido: recebido.append(pedido) or b"\x00"
    ) as socket:
        ClienteIpc(socket).fechar()
    assert recebido == [bytes([CMD_FECHAR])]


def test_fechar_resposta_inesperada(tmp_path) -> None:
    """Corpo não vazio em ``FECHAR`` → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00\x99") as socket:
        with pytest.raises(RespostaInvalida, match="FECHAR"):
            ClienteIpc(socket).fechar()


# ---------------------------------------------------------------------
# Handshake (Etapa 7) — comandos 0x09..0x0C
# ---------------------------------------------------------------------


def test_pedir_amizade_formato_e_resposta(tmp_path) -> None:
    """``PEDIR_AMIZADE``: ``seed‖pub_dest`` → ``k1‖k5‖k9‖pedido(209)``."""
    recebido: list[bytes] = []
    seed, pub = bytes([7]) * 32, bytes([8]) * 32
    chaves = b"K" * 96
    pedido_corpo = b"R" * TAM_CORPO_PEDIDO
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, lambda p: recebido.append(p) or b"\x00" + chaves + pedido_corpo
    ) as socket:
        resultado = ClienteIpc(socket).pedir_amizade(seed, pub)
    assert recebido == [bytes([CMD_PEDIR_AMIZADE]) + seed + pub]
    assert resultado == PedidoAmizade(
        k1=b"K" * 32, k5=b"K" * 32, k9=b"K" * 32, corpo=pedido_corpo
    )


def test_pedir_amizade_seed_curta(tmp_path) -> None:
    """Seed fora dos 32 bytes → ``PedidoInvalido`` antes do IPC."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00") as socket:
        with pytest.raises(PedidoInvalido, match="seed"):
            ClienteIpc(socket).pedir_amizade(b"curta", bytes(32))


def test_pedir_amizade_resposta_curta(tmp_path) -> None:
    """Resposta fora dos 304 bytes → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00" + b"K" * 95) as socket:
        with pytest.raises(RespostaInvalida, match="PEDIR_AMIZADE"):
            ClienteIpc(socket).pedir_amizade(bytes(32), bytes(32))


def test_aceitar_amizade_formato_e_resposta(tmp_path) -> None:
    """``ACEITAR_AMIZADE``: ``seed‖pedido(209)`` → ``chaves(192)‖aceite(177)``."""
    recebido: list[bytes] = []
    seed = bytes([3]) * 32
    corpo_pedido = b"P" * TAM_CORPO_PEDIDO
    chaves = bytes(range(256))[:192]
    aceite = b"A" * TAM_CORPO_ACEITE
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, lambda p: recebido.append(p) or b"\x00" + chaves + aceite
    ) as socket:
        resultado = ClienteIpc(socket).aceitar_amizade(seed, corpo_pedido)
    assert recebido == [bytes([CMD_ACEITAR_AMIZADE]) + seed + corpo_pedido]
    assert isinstance(resultado, AceiteAmizade)
    assert resultado.corpo == aceite
    assert resultado.chaves == ChavesAmizade(
        k1=chaves[0:32],
        k5_proprio=chaves[32:64],
        k9_proprio=chaves[64:96],
        k5_par=chaves[96:128],
        k9_par=chaves[128:160],
        publica=chaves[160:192],
    )


def test_aceitar_amizade_pedido_curto(tmp_path) -> None:
    """Corpo de pedido fora dos 209 bytes → ``PedidoInvalido``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00") as socket:
        with pytest.raises(PedidoInvalido, match="corpo_pedido"):
            ClienteIpc(socket).aceitar_amizade(bytes(32), b"curto")


def test_recusar_amizade_formato_e_resposta(tmp_path) -> None:
    """``RECUSAR_AMIZADE``: ``seed‖nonce(16)`` → ``reject(80)``."""
    recebido: list[bytes] = []
    seed, nonce = bytes([1]) * 32, bytes([2]) * 16
    recusa = b"X" * TAM_CORPO_RECUSA
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, lambda p: recebido.append(p) or b"\x00" + recusa
    ) as socket:
        corpo = ClienteIpc(socket).recusar_amizade(seed, nonce)
    assert recebido == [bytes([CMD_RECUSAR_AMIZADE]) + seed + nonce]
    assert corpo == recusa


def test_recusar_amizade_nonce_errado(tmp_path) -> None:
    """Nonce fora dos 16 bytes → ``PedidoInvalido``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00" + b"r" * TAM_CORPO_RECUSA) as socket:
        with pytest.raises(PedidoInvalido, match="nonce"):
            ClienteIpc(socket).recusar_amizade(bytes(32), b"curto")


def test_confirmar_amizade_devolve_chaves(tmp_path) -> None:
    """``CONFIRMAR_AMIZADE`` → as seis chaves da amizade fechada."""
    recebido: list[bytes] = []
    seed = bytes([5]) * 32
    aceite = b"C" * TAM_CORPO_ACEITE
    chaves = bytes(range(256))[:192]
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, lambda p: recebido.append(p) or b"\x00" + chaves
    ) as socket:
        resultado = ClienteIpc(socket).confirmar_amizade(seed, aceite)
    assert recebido == [bytes([CMD_CONFIRMAR_AMIZADE]) + seed + aceite]
    assert resultado.publica == chaves[160:192]
    assert resultado.k1 == chaves[0:32]


def test_confirmar_amizade_aceite_curto(tmp_path) -> None:
    """Aceite fora dos 177 bytes → ``PedidoInvalido``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00" + b"c" * 192) as socket:
        with pytest.raises(PedidoInvalido, match="corpo_aceite"):
            ClienteIpc(socket).confirmar_amizade(bytes(32), b"curto")


def test_confirmar_amizade_resposta_errada(tmp_path) -> None:
    """Resposta fora dos 192 bytes → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00" + b"c" * 191) as socket:
        with pytest.raises(RespostaInvalida, match="CONFIRMAR_AMIZADE"):
            ClienteIpc(socket).confirmar_amizade(bytes(32), b"A" * TAM_CORPO_ACEITE)


# ---------------------------------------------------------------------
# HELLO — sessão com versão explícita (spec §Protocolo de versão)
# ---------------------------------------------------------------------


def _ler_frame(ligacao: socket.socket) -> bytes | None:
    """Lê um frame `[u32 LE][carga]`; ``None`` em EOF/truncado."""
    cabecalho = b""
    while len(cabecalho) < 4:
        pedaco = ligacao.recv(4 - len(cabecalho))
        if not pedaco:
            return None
        cabecalho += pedaco
    (tamanho,) = struct.unpack("<I", cabecalho)
    dados = b""
    while len(dados) < tamanho:
        pedaco = ligacao.recv(tamanho - len(dados))
        if not pedaco:
            return None
        dados += pedaco
    return dados


@contextmanager
def _sessao_crua(caminho, respostas: list[bytes]):
    """Servidor mínimo que responde ``respostas`` por ordem e grava pedidos.

    Ao contrário de ``servidor_falso`` (resposta fixa ao HELLO), este
    servidor controla **também** a resposta ao ``HELLO`` — para testar
    versões incompatíveis, erros 0x12 e EOF à cabeça de sessão.
    """
    servidor = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    servidor.bind(str(caminho))
    servidor.listen(1)
    servidor.settimeout(3.0)
    pedidos: list[bytes] = []

    def correr() -> None:
        try:
            ligacao, _ = servidor.accept()
        except (OSError, socket.timeout):
            return
        with ligacao:
            while True:
                pedido = _ler_frame(ligacao)
                if pedido is None:
                    return  # cliente fechou
                pedidos.append(pedido)
                if not respostas:
                    return  # "daemon morre" sem responder a este pedido
                corpo = respostas.pop(0)
                try:
                    ligacao.sendall(struct.pack("<I", len(corpo)) + corpo)
                except OSError:
                    return

    thread = threading.Thread(target=correr, daemon=True)
    thread.start()
    try:
        yield pedidos
    finally:
        servidor.close()
        thread.join(timeout=3)


def test_hello_e_o_primeiro_frame_de_toda_ligacao(tmp_path) -> None:
    """O cliente abre **toda** a ligação com ``[0x00][0x01]`` (spec)."""
    caminho = tmp_path / "s.sock"
    with _sessao_crua(
        caminho, [b"\x00\x01" + b"9.9-falso", b"\x00"]
    ) as pedidos:
        assert ClienteIpc(str(caminho)).fechar() is None
    assert pedidos[0] == bytes([CMD_HELLO, VERSAO_IPC]), "HELLO primeiro"
    assert pedidos[1] == bytes([CMD_FECHAR]), "comando só após sessão aberta"


def test_hello_com_erro_0x12_do_daemon(tmp_path) -> None:
    """``ERRO 0x12`` à resposta do HELLO propaga ``ErroDaemon``."""
    caminho = tmp_path / "s.sock"
    resposta = b"\x01\x12" + "versão do protocolo incompatível".encode()
    with _sessao_crua(caminho, [resposta]):
        with pytest.raises(ErroDaemon) as capturado:
            ClienteIpc(str(caminho)).fechar()
    assert capturado.value.codigo == ERR_VERSAO_INCOMPATIVEL


@pytest.mark.parametrize(
    "resposta",
    [
        # Major diferente da nossa (0x01) → incompatível.
        b"\x00\x10" + b"build",
        b"\x00",  # OK com corpo vazio (sem versão nenhuma)
    ],
    ids=["major-diferente", "corpo-vazio"],
)
def test_hello_com_versao_inesperada(tmp_path, resposta: bytes) -> None:
    """Major diferente, ou corpo sem versão → ``0x12 VersaoIncompativel``."""
    caminho = tmp_path / "s.sock"
    with _sessao_crua(caminho, [resposta]):
        with pytest.raises(ErroDaemon) as capturado:
            ClienteIpc(str(caminho)).fechar()
    assert capturado.value.codigo == ERR_VERSAO_INCOMPATIVEL


@pytest.mark.parametrize(
    # Só major 0: `0x1A` tem major 1 e é incompatível, coberto por outro
    # teste. Incluí-lo aqui seria um caso negativo com o nome de positivo.
    "minor_do_daemon",
    [0x00, 0x01, 0x02, 0x07, 0x0F],
    ids=lambda v: f"minor-{v:#04x}",
)
def test_hello_aceita_minor_diferente(tmp_path, minor_do_daemon: int) -> None:
    """Um daemon com outro ``minor`` é aceite — é a tolerância de versões.

    O daemon devolve a versão do *cliente*, mas um daemon com software
    anterior devolve a dele. Aceitar qualquer ``minor`` com o mesmo
    ``major`` é o que permite a dois utilizadores em versões diferentes
    do software usar o mesmo sistema.

    A resposta tem de incluir a resposta ao comando seguinte, por isso a
    sessão tem duas respostas.
    """
    caminho = tmp_path / "s.sock"
    hello = bytes([0x00, minor_do_daemon]) + b"9.9-falso"
    with _sessao_crua(caminho, [hello, b"\x00"]) as pedidos:
        assert ClienteIpc(str(caminho)).fechar() is None
    assert pedidos[0] == bytes([CMD_HELLO, VERSAO_IPC]), "HELLO primeiro"


def test_versao_compativel_separa_major_de_minor() -> None:
    """A função é a norma executável do princípio de tolerância."""
    from messenger.ipc_client import versao_compativel

    assert versao_compativel(0x01)  # idêntica
    assert versao_compativel(0x00)  # mesmo major, minor diferente
    assert versao_compativel(0x0F)  # mesmo major, minor extremo
    assert not versao_compativel(0x10)  # major diferente
    assert not versao_compativel(0x00, 0x20)  # major 0 vs major 2


def test_hello_com_enquadramento_invalido(tmp_path) -> None:
    """Header do HELLO a anunciar 0 bytes → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with _sessao_crua(caminho, [b""]):
        with pytest.raises(RespostaInvalida, match="HELLO"):
            ClienteIpc(str(caminho)).fechar()


def test_hello_sem_resposta_truncada(tmp_path) -> None:
    """O daemon morre antes de responder ao HELLO → ``RespostaInvalida``."""
    caminho = tmp_path / "s.sock"
    with _sessao_crua(caminho, []):
        with pytest.raises(RespostaInvalida, match="truncada"):
            ClienteIpc(str(caminho)).fechar()
