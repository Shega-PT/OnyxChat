# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""pipeline.py — coordena o pipeline K1→K9 através do daemon (IPC).

O Python **não** reimplementa as nove camadas: o daemon Rust orquestra
Rust (K1, K5, K7), C/C++ (K4, K9) e Lua (K2, K3, K6, K8). Este módulo
coordena as duas operações do lado do cliente:

* :meth:`Pipeline.cifrar` — ``ENCODE``: texto → envelope assinado.
* :meth:`Pipeline.decifrar` — valida **localmente** a assinatura
  Ed25519 do envelope (antes de qualquer IPC, como exigem
  ``message_format.md`` e ``handshake.md``) e só depois pede ao daemon
  a decifragem do pipeline inverso.

Rejeições da assinatura levantam :class:`AssinaturaInvalida` sem
nenhum dado chegar ao daemon; malformações levantam
``EnvelopeInvalido``; o resto são exceções ``ErroIpc`` do cliente.
"""

from __future__ import annotations

from .envelope import parsear
from .ipc_client import ClienteIpc
from .keys import Chaves, Identidade, verificar


class AssinaturaInvalida(Exception):
    """Assinatura Ed25519 do envelope rejeitada — mensagem nunca aceite."""


class Pipeline:
    """Fachada síncrona do pipeline K1→K9 (uma operação = um IPC)."""

    def __init__(self, cliente: ClienteIpc | None = None) -> None:
        self._cliente = cliente if cliente is not None else ClienteIpc()

    @property
    def caminho_socket(self) -> str:
        """Caminho do socket do daemon (útil para estado/debug da UI)."""
        return self._cliente.caminho

    def cifrar(
        self, texto: str, chaves: Chaves, remetente: Identidade
    ) -> bytes:
        """Cifra ``texto`` (K1→K9) e devolve o envelope assinado completo."""
        return self._cliente.cifrar(
            chaves.k1,
            chaves.k5,
            chaves.k9,
            remetente.seed,
            texto,
        )

    def decifrar(
        self, envelope: bytes, chaves: Chaves, pub_remetente: bytes
    ) -> str:
        """Valida a assinatura localmente e decifra (K9⁻¹→K1⁻¹) no daemon.

        Ordem obrigatória: parse → assinatura → IPC. Mensagens
        adulteradas morrem na primeira etapa, sem tocar no pipeline.
        """
        env = parsear(envelope)
        if not verificar(env.regiao_assinada, env.assinatura, pub_remetente):
            raise AssinaturaInvalida("assinatura do envelope rejeitada")
        return self._cliente.decifrar(
            chaves.k1,
            chaves.k5,
            chaves.k9,
            pub_remetente,
            envelope,
        )
