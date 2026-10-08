# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""descoberta.py — resolução de destino P2P do lado do cliente.

Porquê este módulo existe
-------------------------

O comando ``LIGAR`` aceita dois tipos de destino, e a validação é
**dependente do modo** (norma em ``docs/ipc_spec.md`` §``destino``):

======================  ==========================================
Modo                    ``destino`` aceite
======================  ==========================================
``directo``             ``xxxx.onion`` **apenas**
``relay``               ID hex de 64 caracteres da pública do par
======================  ==========================================

O daemon Rust **não tem cliente de discovery** — falaria em vez de
resolver, e a falha seria ``0x0D`` indistinguível de um endereço
inválido. A resolução ``ID → .onion`` é, portanto, responsabilidade do
cliente Python, e acontece **antes** de emitir ``LIGAR``.

Esta separação é deliberada: manter a resolução no cliente evita meter um
cliente HTTP (e as suas dependências) dentro do daemon, que só fala Tor.

Privacidade — leia antes de usar
--------------------------------

Automatizar a resolução significa que o servidor de discovery passa a ver
**uma consulta por tentativa de ligação**, sem intervenção humana. O
conteúdo da consulta não muda (quem procura que ``id``); a frequência e a
correlação com instantes de actividade **aumentam**.

Isto está declarado em ``docs/discovery.md`` §Metadados observáveis e em
``docs/privacy_model.md`` §4. Se a frequência for relevante para a sua
ameaça, use :func:`resolver_ou_usar` com um destino já resolvido, ou
corra a sua própria instância de discovery.

Ver também ``docs/SYS_GUIDE.md`` §6.
"""

from __future__ import annotations

import re
from typing import Callable

from server.discovery_server import consultar_descoberta

__all__ = [
    "DestinoInvalido",
    "SERVIDOR_DESCOBERTA",
    "classificar_destino",
    "resolver_ou_usar",
]

#: O servidor de descoberta por omissão.
#:
#: Vive aqui e não na CLI porque quem precisa dele é
#: :func:`resolver_ou_usar` — a CLI e a rota do sidecar precisam do mesmo
#: endereço, e duas constantes com o mesmo valor em dois ficheiros é uma
#: delas a divergir sem ninguém dar por isso.
SERVIDOR_DESCOBERTA = "http://127.0.0.1:8789"

#: Comprimento da etiqueta base32 de um endereço `.onion` v3.
_ETIQUETA_ONION = 56

#: `.onion` v3: 56 caracteres de base32 (a-z e 2-7) + ".onion".
#: `\w` foi evitado de propósito porque incluiria maiúsculas, vírgulas e
#: o próprio underscore — a classe de caracteres tem de ser explícita para
#: que o limite inferior do formato seja respeitado.
_RE_ONION = re.compile(rf"\A[a-z2-7]{{{_ETIQUETA_ONION}}}\.onion\Z")

#: ID de utilizador: pública Ed25519 de 32 bytes em hex (64 caracteres).
_RE_ID_HEX = re.compile(r"\A[0-9a-fA-F]{64}\Z")


class DestinoInvalido(ValueError):
    """``destino`` não é um endereço ``.onion`` nem um ID hex válido.

    Herda de :class:`ValueError` para que a CLI a trate sem lista
    explícita (``user.cli.main`` apanha ``ValueError``).
    """


def classificar_destino(destino: str) -> str:
    """Diz o que ``destino`` é, sem o resolver.

    :param destino: endereço ``.onion``, ID hex de 64 caracteres, ou vazio.
    :returns: ``"onion"``, ``"id"`` ou ``"vazio"``.
    :raises DestinoInvalido: se não for nenhuma das formas conhecidas.

    A validação é estrita nas duas direções. Um ``.onion`` curto ou com
    caracteres fora do base32, e um hex de comprimento errado, são
    recusados aqui — no cliente — em vez de chegarem ao daemon e regressarem
    como um ``0x0D`` genérico.
    """
    if not destino:
        return "vazio"
    if _RE_ONION.match(destino):
        return "onion"
    if _RE_ID_HEX.match(destino):
        return "id"
    raise DestinoInvalido(
        f"destino inválido: esperava um endereço .onion (v3) ou um ID hex de "
        f"64 caracteres; veio {destino!r}"
    )


def resolver_ou_usar(
    destino: str,
    modo: int,
    servidor_descoberta: str,
    consultar: Callable[[str, str], str | None] = consultar_descoberta,
) -> str:
    """Devolve o ``destino`` pronto para ``LIGAR``, resolvendo o ID se preciso.

    A semântica é a da tabela no topo deste módulo:

    * ``directo`` + ``.onion``  → devolvido tal e qual;
    * ``directo`` + ID hex       → **consultado** ao discovery e devolvido
      o ``.onion``; se o ID for desconhecido, :class:`DestinoInvalido`;
    * ``relay`` + ID hex         → devolvido tal e qual (a mailbox é
      derivada da pública no corpo, e o daemon valida a coerência);
    * ``relay`` + ``.onion``     → devolvido tal e qual;
    * qualquer modo + vazio      → erro: o modo directo exige destino.

    :param consultar: função de consulta, injectável para os testes não
        precisarem de um servidor HTTP. A assinatura é a de
        :func:`server.discovery_server.consultar_descoberta`.
    :raises DestinoInvalido: destino malformado ou ID não registado.
    """
    forma = classificar_destino(destino)

    if forma == "vazio":
        raise DestinoInvalido(
            "destino vazio: em modo directo é preciso um .onion ou um ID; "
            "em modo relay, um ID hex da pública do par"
        )

    # Modo relay: o destino é apenas informativo — a mailbox vem da
    # pública no corpo. Não há nada para resolver.
    if modo != 0x00:  # MODO_DIRETO
        return destino

    # Modo directo com um .onion: já está pronto.
    if forma == "onion":
        return destino

    # Modo directo com ID hex: é aqui que o discovery entra.
    onion = consultar(servidor_descoberta, destino)
    if onion is None:
        raise DestinoInvalido(
            f"discovery: {destino} não está registado (ou expirou — o TTL "
            f"é de 300 s). Verifique se o par está com `onyxchat ouvir`, ou "
            f"ligue em modo relay, que não precisa do .onion."
        )
    # O servidor de discovery valida o formato do seu lado, mas um
    # servidor substituto pode não o fazer: revalidar antes de enviar.
    if _RE_ONION.match(onion) is None:
        raise DestinoInvalido(
            f"discovery devolveu um endereço inválido para {destino}: {onion!r}"
        )
    return onion
