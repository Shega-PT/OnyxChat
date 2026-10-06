# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""ui.py — renderizador TUI minimalista do OnyxChat (opcional, sem dependências).

Funções puras que transformam estado em texto com ANSI — o CLI pode
usá-las para colorir a saída e uma futura TUI pode compô-las. Não há
importações de ``curses`` nem código que só corra num terminal
interativo: tudo é testável como texto corrido.
"""

from __future__ import annotations

from collections.abc import Sequence

#: Cores suportadas → código ANSI (par «normal» desliga a cor).
_CORES = {
    "normal": "\033[0m",
    "negrito": "\033[1m",
    "vermelho": "\033[31m",
    "verde": "\033[32m",
    "amarelo": "\033[33m",
    "azul": "\033[34m",
}


def pintar(texto: str, cor: str = "normal") -> str:
    """Envolve ``texto`` no código ANSI de ``cor`` (desconhecida → sem cor)."""
    codigo = _CORES.get(cor)
    if codigo is None:
        return texto
    return f"{codigo}{texto}{_CORES['normal']}"


def renderizar_conversa(
    mensagens: Sequence[tuple[bool, str, str]],
    prefixo_enviado: str = "→",
    prefixo_recebido: str = "←",
) -> str:
    """Renderiza ``[(enviado?, autor, texto), …]`` — uma linha por mensagem."""
    linhas = []
    for enviado, autor, texto in mensagens:
        prefixo = prefixo_enviado if enviado else prefixo_recebido
        linhas.append(f"{prefixo} {autor}: {texto}")
    return "\n".join(linhas)


def renderizar_estado(pub: bytes, socket: str, total_mensagens: int) -> str:
    """Linha de estado: identidade (pública), socket IPC e contador."""
    return (
        f"ID {pub.hex()[:16]}… · socket {socket} · "
        f"mensagens {total_mensagens}"
    )
