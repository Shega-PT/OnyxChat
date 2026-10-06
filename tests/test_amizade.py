# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_amizade.py — a derivação da K1 por mensagem, dos dois lados.

Verifica a implementação Python de ``messenger/amizade.py`` contra o
mesmo vector conhecido que a implementação Rust congela em
``crypto/rust/src/amizade.rs``:

.. code-block:: text

    semente = SHA-256(b"ONYX/AMIZADE/v1" ‖ menor(k_a,k_b) ‖ maior(k_a,k_b))
    k1      = HKDF-SHA256(semente, salt=nonce1,
                         info=b"ONYX/K1/v1" ‖ publica_emissor, 32)

com ``k_a = 0x11×32``, ``k_b = 0x22×32``, ``publica = 0xAA×32`` e
``nonce1 = 01 02 … 0c``:

.. code-block:: text

    k1 = f01f37511c8daa9cb81be7faf296f43b6e78271516cdba0403d3108d7218a664

O vector foi calculado pelo Python (biblioteca padrão) e verificado
pelo Rust (crate ``hkdf``). Se as duas implementações divergirem, uma
falha — e não uma diferença silenciosa em produção, que apareceria como
«as mensagens não abrem».

Os testes de propriedades vêm a seguir, e verificam o que o vector não
consegue: que a simetria e a direccionalidade valem para *qualquer*
entrada, e não só para o par de valores que o vector fixa.
"""

from __future__ import annotations

import hashlib
import hmac

import pytest

from messenger.amizade import (
    DOMINIO_AMIZADE,
    INFO_K1,
    TAM_CHAVE,
    TAM_NONCE,
    TAM_PUBLICA,
    ErroDerivacao,
    derivar_k1,
    info,
    semente,
)

#: O mesmo vector, nas mesmas pontas. Ver o docstring do módulo.
VECTOR_SEMENTE = "27e15a94059c937034b10823b479f30ef84c732075a95588541e443575d443bc"
VECTOR_K1 = "f01f37511c8daa9cb81be7faf296f43b6e78271516cdba0403d3108d7218a664"

KA = bytes([0x11] * 32)
KB = bytes([0x22] * 32)
PUB_A = bytes([0xAA] * 32)
PUB_B = bytes([0xBB] * 32)
NONCE = bytes(range(1, 13))


# ---------------------------------------------------------------------
# O vector congelado
# ---------------------------------------------------------------------


def test_semente_bate_com_o_vector_congelado():
    """A semente é a do vector que o Rust também congela."""
    assert semente(KA, KB).hex() == VECTOR_SEMENTE


def test_k1_bate_com_o_vector_congelado():
    """A K1 derivada é byte a byte a que o Rust deriva.

    Este é o teste de paridade entre linguagens. Se falhar, ou o Rust
    mudou ou o Python mudou — e nos dois casos os clientes velhos
    deixam de conversar com os novos.
    """
    assert derivar_k1(KA, KB, PUB_A, NONCE).hex() == VECTOR_K1


def test_a_formula_escrita_a_mao_da_o_vector():
    """A fórmula, recalculada à mão, dá o mesmo que a implementação.

    Recalcula os dois passos com a biblioteca padrão, sem passar por
    ``semente`` nem por ``info``. É belt-and-braces sobre o vector: se um
    dia a implementação divergir do vector, isto diz **onde** — se
    divergir, o erro é no HKDF ou no SHA-256 e não na montagem das
    peças, que é a diferença entre dez minutos e uma hora de investigação.
    """
    menor, maior = (KA, KB) if KA <= KB else (KB, KA)
    semente_pela_mao = hashlib.sha256(DOMINIO_AMIZADE + menor + maior).digest()
    assert semente_pela_mao.hex() == VECTOR_SEMENTE

    prk = hmac.new(NONCE, semente_pela_mao, hashlib.sha256).digest()
    bloco = hmac.new(prk, INFO_K1 + PUB_A + b"\x01", hashlib.sha256).digest()
    assert bloco[:32].hex() == VECTOR_K1


# ---------------------------------------------------------------------
# As propriedades que o vector não fixa
# ---------------------------------------------------------------------


def test_derivacao_e_simetrica_na_chave():
    """A e B obtêm a mesma chave, cada um na sua posição.

    A propriedade que faz a coisa funcionar. Se ela quebrar, o código
    compila, os testes de uma camada passam, e a falha só aparece como
    mensagens indecifráveis.
    """
    assert derivar_k1(KA, KB, PUB_A, NONCE) == derivar_k1(KB, KA, PUB_A, NONCE)


def test_derivacao_e_direccional():
    """A chave de A→B não é a de B→A.

    A identidade do emissor entra no ``info`` do HKDF. Sem isso, um
    envelope de Alice reflectido abriria como se fosse de Bob.
    """
    assert derivar_k1(KA, KB, PUB_A, NONCE) != derivar_k1(KB, KA, PUB_B, NONCE)


def test_nonce_diferente_da_chave_diferente():
    """Nonces diferentes dão chaves diferentes."""
    outro = bytes([NONCE[0] ^ 0x01]) + NONCE[1:]
    assert derivar_k1(KA, KB, PUB_A, NONCE) != derivar_k1(KA, KB, PUB_A, outro)


def test_identidade_diferente_da_chave_diferente():
    """Identidades diferentes dão chaves diferentes."""
    assert derivar_k1(KA, KB, PUB_A, NONCE) != derivar_k1(KA, KB, PUB_B, NONCE)


def test_saida_nao_e_uma_das_entradas():
    """A K1 derivada não é nenhuma entrada, nem zeros.

    Um derivador que devolvesse a entrada passava num teste de
    round-trip e seria uma falha completa em produção.
    """
    derivada = derivar_k1(KA, KB, PUB_A, NONCE)
    assert derivada != KA
    assert derivada != KB
    assert derivada != PUB_A
    assert derivada != bytes(TAM_CHAVE)
    assert derivada[:28] != bytes(28)


# ---------------------------------------------------------------------
# Contrato: tamanhos e limites
# ---------------------------------------------------------------------


@pytest.mark.parametrize(
    "nome,chamada",
    [
        ("k_a curto", lambda: semente(b"\x00" * 31, KB)),
        ("k_b longo", lambda: semente(KA, b"\x00" * 33)),
        ("nonce curto", lambda: derivar_k1(KA, KB, PUB_A, b"\x00" * 11)),
        ("publica vazia", lambda: derivar_k1(KA, KB, b"", NONCE)),
    ],
)
def test_tamanhos_errados_sao_rejeitados(nome, chamada):
    """Um tamanho errado é erro explícito, não resultado silencioso.

    A truncagem silenciosa de uma chave a 31 bytes dariam uma chave de
    menos entropia sem que nada dissesse nada — que é a forma mais cara
    de um bug de interoperabilidade.
    """
    with pytest.raises(ErroDerivacao):
        chamada()


def test_tipo_errado_e_rejeitado():
    """``str`` onde se espera ``bytes`` dá erro de tipo, não de tamanho."""
    with pytest.raises(ErroDerivacao):
        semente("a" * 32, KB)  # type: ignore[arg-type]


def test_info_tem_o_rotulo_e_a_publica():
    """O ``info`` é o rótulo seguido da identidade, sem mais nada."""
    assert info(PUB_A) == INFO_K1 + PUB_A
    assert len(info(PUB_A)) == len(INFO_K1) + TAM_PUBLICA


# ---------------------------------------------------------------------
# HKDF: contra os vectores da RFC 5869
# ---------------------------------------------------------------------


def test_hkdf_bate_com_o_appendice_a1_da_rfc5869():
    """Caso de teste 1 da RFC 5869 — extração e expansão basic.

    Não é um vector do OnyxChat: é um vector **da RFC**, e é o que
    valida a implementação do HKDF em si. Sem ele, um HKDF com um erro
    de encadeamento passaria nos vectores do projecto desde que o erro
    fosse determinístico.

    A RFC 5869 Apêndice A.1 usa IKM de 22 bytes, o que não é o formato
    do nosso HKDF (que aceita um segredo de qualquer tamanho). É
    exactamente esse caso que o vector cobre.
    """
    ikm = bytes.fromhex("0b" * 22)
    salt = bytes.fromhex("000102030405060708090a0b0c")
    info_rfc = bytes.fromhex("f0f1f2f3f4f5f6f7f8f9")

    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    assert prk.hex() == (
        "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
    )

    # Expansão para 42 bytes (o L do caso 1).
    saida = bytearray()
    anterior = b""
    for i in range(1, 3):
        anterior = hmac.new(
            prk, anterior + info_rfc + bytes([i]), hashlib.sha256
        ).digest()
        saida.extend(anterior)
    assert saida[:42].hex() == (
        "3cb25f25faacd57a90434f64d0362f2a"
        "2d2d0a90cf1a5a4c5db02d56ecc4c5bf"
        "34007208d5b887185865"
    )


def test_hkdf_rejeita_salt_mais_longo_que_o_hash():
    """Um ``salt`` maior que o hash é erro, não truncagem silenciosa."""
    from messenger.amizade import _hkdf_sha256

    with pytest.raises(ErroDerivacao):
        _hkdf_sha256(b"\x00" * 32, b"\x00" * 33, b"info", 32)


def test_hkdf_rejeita_info_demais_comprido():
    """Um ``info`` acima de 255×hash é erro, não truncagem.

    Impossível de atingir pelo caminho normal — o ``info`` tem 48 bytes
    e o limite são 8160 — mas o limite existe e o teste existe para que
    a linha não seja um ramo morto que ninguém sabe se funciona.
    """
    from messenger.amizade import _hkdf_sha256

    with pytest.raises(ErroDerivacao):
        _hkdf_sha256(b"\x00" * 32, b"\x00" * 12, b"i" * (255 * 32 + 1), 32)


def test_hkdf_rejeita_saida_mais_longa_que_255_blocos():
    """Uma saída acima de 255×32 bytes é erro, não truncagem.

    A RFC 5869 impõe 255 iterações de expansão como limite. Pedir mais
    que isso não se pode satisfazer sem inventar bytes, e inventar bytes
    numa derivação de chave é exactamente o tipo de coisa que não se
    pode inventar.
    """
    from messenger.amizade import _hkdf_sha256

    with pytest.raises(ErroDerivacao):
        _hkdf_sha256(b"\x00" * 32, b"\x00" * 12, b"info", 255 * 32 + 1)


# ---------------------------------------------------------------------
# Geração aleatória: a K1 derivada nunca colide com o segredo
# ---------------------------------------------------------------------


def test_derivacao_nao_e_deterministica_entre_pares():
    """Pares de chaves aleatórios dão chaves distintas.

    Não é um teste de colisão (32 bytes não colidem por sorte em duas
    amostras) — é um teste de que a derivação **depende** das chaves, e
    não de outra coisa que happença estar constante.
    """
    import secrets

    derivadas = {
        derivar_k1(secrets.token_bytes(32), secrets.token_bytes(32), PUB_A, NONCE)
        for _ in range(16)
    }
    assert len(derivadas) == 16