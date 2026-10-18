# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
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
    "DOMINIO_MEDIA",
    "TAM_CHAVE",
    "TAM_NONCE",
    "TAM_PUBLICA",
    "TIPO_AUDIO",
    "TIPO_VIDEO",
    "TIPO_CALL",
    "TIPO_FICHEIRO",
    "TIPOS_CONHECIDOS",
    "semente",
    "info",
    "derivar_k1",
    "derivar_k1_por_tipo",
]

#: Rótulo de derivação do segredo de amizade. Igual ao
#: `DOMINIO_AMIZADE` de `crypto/rust/src/amizade.rs`.
DOMINIO_AMIZADE = b"ONYX/AMIZADE/v1"

#: Rótulo do HKDF para a K1 derivada. Igual ao `INFO_K1` de Rust.
INFO_K1 = b"ONYX/K1/v1"

#: Rótulo que separa as chaves de media das chaves de texto. Igual ao
#: `DOMINIO_MEDIA` de `crypto/rust/src/amizade.rs`.
#:
#: Entra no `info` do HKDF e não noutro lado, porque o `info` é a
#: única parte do contexto que não é preciso manter secreta — e por isso
#: a única que pode viajar para o outro lado sem custo. Nada disto é
#: transmitido: o `TIPO` está no `info`, o `info` está **dentro** da
#: chave, e a chave não sai do host de quem cifra.
DOMINIO_MEDIA = b"ONYX/MEDIA/"

#: Tipos de conteúdo que têm chave derivada. O texto **não** aparece:
#: continua a usar a K1 de longa duração, e por isso o texto produzido
#: é byte-a-byte igual ao que produzia antes de existir media.
TIPO_AUDIO = 0x01
TIPO_VIDEO = 0x02

#: Os tipos que a derivação aceita. Um tipo fora daqui não deriva —
#: falha, em vez de produzir uma chave que ninguém sabe desambiguar.
#:
#: ## E os tipos de controlo, que também derivam
#:
#: `TIPO_CALL` e `TIPO_FICHEIRO` entram aqui pelo mesmo motivo que o
#: áudio e o vídeo: precisam de uma chave que **só** o par conhece, e
#: que a rede não pode distinguir de nenhuma outra.
#:
#: Não têm carácter na moldura — `call` e `ficheiro` não são molduras
#: `ONX1`, são envelopes cujo corpo é ASCII versionado (ver
#: `controle.py`). O que entra é o byte de derivação, que é o `info` do
#: HKDF, e esse é independente do formato do payload.
#:
#: Os valores 0x03 e 0x04 são os primeiros livres depois do áudio e do
#: vídeo, e ficam **congelados**: mudar um valor de derivação é
#: indistinguível de um envelope antigo a abrir com a chave nova, e o
#: resultado é texto ilegível em vez de um erro.
TIPO_CALL = 0x03
TIPO_FICHEIRO = 0x04

TIPOS_CONHECIDOS = (TIPO_AUDIO, TIPO_VIDEO, TIPO_CALL, TIPO_FICHEIRO)

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


def derivar_k1_por_tipo(
    k_emissor: bytes,
    k_recetor: bytes,
    publica_emissor: bytes,
    tipo: int,
) -> bytes:
    """K1 derivada de uma mensagem de media, por tipo de conteúdo.

    É :func:`derivar_k1` com o tipo no ``info`` do HKDF:

    .. code-block:: text

        info = INFO_K1 ‖ publica_emissor ‖ DOMINIO_MEDIA ‖ tipo

    A mesma semente e o mesmo ``info`` — mais um byte. É a razão pela
    qual a rede não consegue distinguir uma mensagem de audio de uma de
    texto: a distinção está **dentro** da chave, e a chave não é
    observável do lado de fora.

    O ``salt`` é o rótulo de domínio e o tipo, e **não** um nonce —
    ver a nota longa no fim do docstring.

    Só a K1 é derivada. K5 e K9 entram no ``ENCODE`` inalteradas, porque
    a K1 é a **primeira** camada e é AEAD: uma etiqueta errada morre
    ali. Logo, sondar o tipo custa uma verificação Poly1305 — não uma
    descifra completa.

    ## O ``salt`` é o rótulo e o tipo, e não um nonce

    A primeira versão punha ``salt = nonce1``, herdado de
    :func:`derivar_k1`. E isso **não funciona**, por uma razão que só
    aparece em execução.

    O emissor teria de cifrar **duas vezes** — uma para obter um
    envelope com um ``nonce1`` conhecido, e outra com a chave derivada
    desse nonce. Mas a segunda ``ENCODE`` gera um ``nonce1`` **novo**,
    e o envelope que vai para a rede tem esse nonce novo, não o que a
    chave usou.

    O receptor deriva a chave a partir do envelope que **recebeu**, logo
    deriva com ``Q`` e não com ``P`` — e a cifra falha.

    E a falha é do tipo mais caro que existe: **cifra bem e não abre**.
    ``0x05 DecifragemFalhou`` não aponta para a chave, que é a pista
    mais errada que se pode dar.

    Com um ``salt`` constante, os dois lados derivam a mesma chave a
    partir do que **já sabem ambos** — o par de chaves e a identidade do
    remetente — e uma passagem única resolve. O ``nonce1`` continua a
    ser o nonce do AEAD, que é onde tem de estar, e é o que garante que
    duas mensagens com a mesma chave usam nonces diferentes.

    A consequência de segurança é a mesma que o texto já tem: a K1 de
    media é de longa duração dentro do par. Ver ``docs/media.md`` §1.4
    — que é por isso que está escrita como limitação declarada e não
    como detalhe omitido.

    :param tipo: ``TIPO_AUDIO`` ou ``TIPO_VIDEO``.
    :raises ErroDerivacao: se ``tipo`` não for um tipo conhecido.
    :returns: 32 bytes.
    """
    if tipo not in TIPOS_CONHECIDOS:
        raise ErroDerivacao(
            f"tipo de media desconhecido: 0x{tipo:02x} "
            f"(conhecidos: {', '.join(f'0x{t:02x}' for t in TIPOS_CONHECIDOS)})"
        )

    publica = _exigir(publica_emissor, TAM_PUBLICA, "publica_emissor")
    segredo = semente(k_emissor, k_recetor)
    contexto = INFO_K1 + publica + DOMINIO_MEDIA + bytes([tipo])
    sal = DOMINIO_MEDIA + bytes([tipo])
    return _hkdf_sha256(segredo, sal, contexto, TAM_CHAVE)
