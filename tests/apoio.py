# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""apoio.py — utilitários partilhados dos testes (não é código de produto).

Fornece um **servidor IPC falso** (protocolo do ``docs/ipc_spec.md`` com
resposta programável) e construtores de envelopes válidos, para que os
testes da camada Python não dependam do daemon compilado (o E2E com o
daemon real vive em ``test_e2e_daemon.py``).
"""

from __future__ import annotations

import socket
import struct
import threading
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from pathlib import Path

from messenger.envelope import VERSAO
from messenger.keys import Identidade


def _ler_exato(ligacao: socket.socket, tamanho: int) -> bytes:
    """Lê até ``tamanho`` bytes (ou até o fecho da ligação)."""
    dados = b""
    while len(dados) < tamanho:
        pedaco = ligacao.recv(tamanho - len(dados))
        if not pedaco:
            break
        dados += pedaco
    return dados


#: ``[0x00][versao]`` do cliente — o ``HELLO`` obrigatório da sessão.
_HELLO = bytes([0x00, 0x01])
#: ``[OK][versao][build]`` — resposta fixa ao ``HELLO`` (spec §HELLO).
_RESPOSTA_HELLO = bytes([0x00, 0x01]) + b"0.0-falso"
#: Portão da sessão: primeiro pedido que não seja ``HELLO`` → ``0x02``.
_SEM_HELLO = b"\x01\x02" + "HELLO obrigatório antes de qualquer comando".encode()


def _enviar(ligacao: socket.socket, corpo: bytes) -> bool:
    """Enquadra e envia ``corpo``; ``False`` se a ligação morreu."""
    try:
        ligacao.sendall(struct.pack("<I", len(corpo)) + corpo)
        return True
    except OSError:
        return False


def _ler_pedido(ligacao: socket.socket) -> bytes | None:
    """Lê um pedido enquadrado; ``None`` em EOF/truncado."""
    cabecalho = _ler_exato(ligacao, 4)
    if len(cabecalho) < 4:
        return None
    (tamanho,) = struct.unpack("<I", cabecalho)
    if tamanho == 0:
        return b""
    pedido = _ler_exato(ligacao, tamanho)
    return pedido if len(pedido) == tamanho else None


@contextmanager
def servidor_falso(
    caminho: Path, resposta: bytes | Callable[[bytes], bytes | None]
) -> Iterator[str]:
    """Servidor UDS num *thread* com resposta fixa ou por pedido.

    ``resposta`` é o **corpo completo** da resposta (inclui o byte de
    estado) ou um *callback* ``(pedido) → corpo | None`` — ``None``
    simula o daemon a morrer sem responder.

    Como o daemon real, o servidor **exige o ``HELLO``** como primeiro
    pedido (``docs/ipc_spec.md`` §Protocolo de versão): responde-o
    internamente e só depois aplica ``resposta`` ao comando do teste.
    """
    ligacao_servidor = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    ligacao_servidor.bind(str(caminho))
    ligacao_servidor.listen(4)
    ligacao_servidor.settimeout(0.2)
    parado = threading.Event()

    def atuar(ligacao: socket.socket, pedido: bytes) -> None:
        corpo = resposta(pedido) if callable(resposta) else resposta
        if corpo is not None:
            _enviar(ligacao, corpo)

    def ciclo() -> None:
        while not parado.is_set():
            try:
                ligacao, _ = ligacao_servidor.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            with ligacao:
                pedido = _ler_pedido(ligacao)
                if pedido is None:
                    continue
                if pedido != _HELLO:
                    # Portão da sessão: sem HELLO, responde 0x02 e nada
                    # mais (mesmo comportamento do daemon).
                    _enviar(ligacao, _SEM_HELLO)
                    continue
                if not _enviar(ligacao, _RESPOSTA_HELLO):
                    continue
                pedido = _ler_pedido(ligacao)
                if pedido is not None:
                    atuar(ligacao, pedido)

    thread = threading.Thread(target=ciclo, daemon=True)
    thread.start()
    try:
        yield str(caminho)
    finally:
        parado.set()
        ligacao_servidor.close()
        thread.join(timeout=3)


def envelope_assinado(
    identidade: Identidade,
    ciphertext: bytes = b"c" * 32,
    nonce1: bytes | None = None,
) -> bytes:
    """Envelope ``101+N`` válido, assinado localmente por ``identidade``."""
    nonce1 = nonce1 if nonce1 is not None else bytes([1]) * 12
    nonce5 = bytes([2]) * 12
    nonce9 = bytes([3]) * 12
    regiao = bytes([VERSAO]) + nonce1 + nonce5 + nonce9 + ciphertext
    return regiao + identidade.assinar(regiao)
