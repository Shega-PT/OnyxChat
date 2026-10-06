# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Derivação da chave de mensagem a partir do segredo de uma amizade.

Este módulo é a metade Python de ``crypto/rust/src/amizade.rs``. As duas
implementações têm de produzir **os mesmos bytes**: o cliente deriva a
chave, o daemon recebe-a no campo ``k1`` de ``ENCODE``/``DECODE`` e
nunca vê o segredo de longa duração.

O vector conhecido em ``tests/test_amizade.py`` e
``crypto/rust/src/amizade.rs`` é o mesmo valor hexadecimal congelado nas
duas pontas. Se as implementações divergirem, uma das duas falha — e é
por isso que o vector está congelado em vez de recalculado com a mesma
fórmula do código.

## Fórmulas

.. code-block:: text

    semente   = SHA-256(b"ONYX/AMIZADE/v1" ‖ menor(k_a, k_b) ‖ maior(k_a, k_b))
    k1_msg    = HKDF-SHA256(semente, salt=nonce1,
                            info=b"ONYX/K1/v1" ‖ publica_do_emissor,
                            comprimento=32)

A ordenação canónica das chaves é o que torna a semente igual para os
dois lados: cada um tem «a minha» e «a do par», e sem ordenação cada um
calcularia um valor diferente.

## Sem forward secrecy — e isso é uma declaração, não uma omissão

A derivação é determinística em função de duas chaves de longa duração e
de um nonce público. Se ``k5_proprio`` ou ``k5_par`` for comprometida
depois da conversa, **todas** as mensagens — passadas e futuras — se
abrem, porque a semente é reconstruível a partir das chaves.

Só há forward secrecy com um segredo efémero por sessão que nunca seja
transmitido. Isso é um handshake diferente (tipo Noise IK com
efémeros), não uma optimização. Ver ``docs/threat_model.md`` §I.

O que esta derivação **compra**, e que é real: a chave de longa
duração não atravessa a IPC nem entra na memória do daemon. Um dump do
processo do daemon dá chaves de mensagens isoladas, não o segredo da
amizade. E a chave derivada é direccional — a de A→B não é a de B→A —
porque a identidade do emissor entra no ``info`` do HKDF.
"""

from __future__ import annotations

import hashlib
import hmac

__all__ = [
    "DOMINIO_AMIZADE",
    "INFO_K1",
    "TAM_CHAVE",
    "TAM_NONCE",
    "TAM_PUBLICA",
    "semente",
    "info",
    "derivar_k1",
]

#: Rótulo de derivação do segredo de amizade. Igual ao
#: `DOMINIO_AMIZADE` de `crypto/rust/src/amizade.rs`.
DOMINIO_AMIZADE = b"ONYX/AMIZADE/v1"

#: Rótulo do HKDF para a K1 derivada. Igual ao `INFO_K1` de Rust.
INFO_K1 = b"ONYX/K1/v1"

TAM_CHAVE = 32
TAM_NONCE = 12
TAM_PUBLICA = 32


class ErroDerivacao(ValueError):
    """Argumento com tamanho errado, ou resultado imposible de obter."""


def _exigir(valor: bytes, tamanho: int, nome: str) -> bytes:
    """Devolve `valor` se tiver `tamanho` bytes; levanta erro se não.

    A verificação é explícita e não um `assert`: os `assert` desaparecem
    com ``python -O``, e um erro de tamanho que só existe em debug é um
    erro de tamanho que existe em produção.

    A excepção é `ValueError` e não um tipo novo — quem chama está a
    passar dados de um envelope ou de um keystore, e um erro de
    programação não justifica uma hierarquia de excepções.
    """
    if not isinstance(valor, (bytes, bytearray, memoryview)):
        raise ErroDerivacao(f"{nome} tem de ser bytes, não {type(valor).__name__}")
    if len(valor) != tamanho:
        raise ErroDerivacao(
            f"{nome} tem {len(valor)} bytes, esperado {tamanho}"
        )
    return bytes(valor)


def _hkdf_sha256(semente: bytes, salt: bytes, contexto: bytes, tamanho: int) -> bytes:
    """HKDF-SHA256 (RFC 5869), só o suficiente para uma saída.

    Implementação própria, e não ``cryptography``: o projecto não tem
    dependências de runtime (``pyproject.toml`` diz ``dependencies =
    []``), e acrescentar uma biblioteca para doze linhas de extracção
    seria trocar dependência nenhuma por uma que ninguém lê. ``hmac`` e
    ``hashlib`` são de biblioteca padrão e as suas implementações estão
    correctas — e o vector conhecido desta função existe precisamente
    para que ninguém tenha de confiar nisto.

    Os limites da RFC 5869 verificados aqui — ``salt`` não mais longo
    que o tamanho do hash, ``contexto`` não mais longo que 255 vezes o
    tamanho do hash, ``tamanho`` não mais longo que 255 vezes — são
    launchados como `ErroDerivacao` e não truncados em silêncio. Um
    truncamento silencioso daria uma chave de menos entropia sem que
    nada dissesse nada.
    """
    tamanho_hash = hashlib.sha256().digest_size  # 32
    if len(salt) > tamanho_hash:
        # A RFC 5869 manda considerar o salt vazio quando é longo
        # demais. Isto nunca acontece com um nonce de 12 bytes, e
        # obedecer à letra da RFC tornaria o comportamento dependente
        # de um caso que não existe — melhor dizer que não.
        raise ErroDerivacao(f"salt de {len(salt)} bytes excede {tamanho_hash}")
    if len(contexto) > 255 * tamanho_hash:
        raise ErroDerivacao("info demasiado longo para HKDF")

    # Extract: PRK = HMAC(salt, semente).
    prk = hmac.new(salt or b"\x00" * tamanho_hash, semente, hashlib.sha256).digest()

    # Expand: T = HMAC(PRK, T(i-1) ‖ info ‖ i), parando no tamanho pedido.
    saida = bytearray()
    bloco_anterior = b""
    contador = 1
    while len(saida) < tamanho:
        if contador > 255:
            raise ErroDerivacao("saída demasiado longa para HKDF")
        bloco_anterior = hmac.new(
            prk, bloco_anterior + contexto + bytes([contador]), hashlib.sha256
        ).digest()
        saida.extend(bloco_anterior)
        contador += 1

    return bytes(saida[:tamanho])


def semente(k_a: bytes, k_b: bytes) -> bytes:
    """Segredo de longo prazo da amizade, independente da ordem.

    ``k_a`` e ``k_b`` são as chaves de envio dos dois lados (o ``k5``
    de cada um, tal como guardado em :class:`ChavesAmizade`). A
    ordenação é lexicográfica sobre os 32 bytes — a mesma comparação
    que o Rust faz com ``[u8; 32] <= [u8; 32]``, que é lexicográfica.
    """
    a = _exigir(k_a, TAM_CHAVE, "k_a")
    b = _exigir(k_b, TAM_CHAVE, "k_b")
    menor, maior = (a, b) if a <= b else (b, a)

    return hashlib.sha256(DOMINIO_AMIZADE + menor + maior).digest()


def info(publica_emissor: bytes) -> bytes:
    """Contexto do HKDF: rótulo seguido da identidade do emissor."""
    publica = _exigir(publica_emissor, TAM_PUBLICA, "publica_emissor")
    return INFO_K1 + publica


def derivar_k1(
    k_emissor: bytes,
    k_recetor: bytes,
    publica_emissor: bytes,
    nonce1: bytes,
) -> bytes:
    """K1 derivada para uma mensagem: `HKDF(semente, nonce1, info)`.

    ``nonce1`` é o nonce do envelope, que é o mesmo que o AEAD vai
    usar — daí ser o `salt` do HKDF (a RFC 5869 reserva o `salt` para um
    valor não secreto que o derivador use para aleatorizar a
    extracção).

    :returns: 32 bytes.
    """
    nonce = _exigir(nonce1, TAM_NONCE, "nonce1")
    segredo = semente(k_emissor, k_recetor)
    return _hkdf_sha256(segredo, nonce, info(publica_emissor), TAM_CHAVE)