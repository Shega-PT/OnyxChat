# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_ui.py — renderizador TUI (user.ui).

Funções puras: cores ANSI, conversa com prefixos e linha de estado.
"""

from __future__ import annotations

from user import ui


def test_pintar_cor_conhecida() -> None:
    """Cores do mapa envolvem o texto e fecham com «normal»."""
    assert ui.pintar("oi", "verde") == "\033[32moi\033[0m"
    assert ui.pintar("erro", "vermelho") == "\033[31merro\033[0m"
    assert ui.pintar("a") == "\033[0ma\033[0m"


def test_pintar_cor_desconhecida_e_normal() -> None:
    """Cor desconhecida devolve o texto intacto (sem ANSI)."""
    assert ui.pintar("texto", "inexistente") == "texto"
    assert ui.pintar("texto", "normal") == "\033[0mtexto\033[0m"


def test_renderizar_conversa_vazia() -> None:
    """Sem mensagens → string vazia."""
    assert ui.renderizar_conversa([]) == ""


def test_renderizar_conversa_prefixos_e_customizados() -> None:
    """Enviado e recebido usam prefixos distintos, por omissão e à escolha."""
    mensagens = [(True, "eu", "olá"), (False, "ana", "tudo bem?")]
    assert ui.renderizar_conversa(mensagens) == "→ eu: olá\n← ana: tudo bem?"
    assert (
        ui.renderizar_conversa(mensagens, prefixo_enviado="E", prefixo_recebido="R")
        == "E eu: olá\nR ana: tudo bem?"
    )


def test_renderizar_conversa_varias_mensagens() -> None:
    """Uma linha por mensagem, pela ordem dada."""
    mensagens = [(True, "a", "1"), (True, "b", "2"), (False, "c", "3")]
    linhas = ui.renderizar_conversa(mensagens).split("\n")
    assert len(linhas) == 3
    assert linhas[2] == "← c: 3"


def test_renderizar_estado() -> None:
    """A linha de estado mostra id truncado, socket e contador."""
    pub = bytes(range(32))
    linha = ui.renderizar_estado(pub, "/tmp/s.sock", 7)
    assert pub.hex()[:16] in linha
    assert "…" in linha
    assert "/tmp/s.sock" in linha
    assert "mensagens 7" in linha
