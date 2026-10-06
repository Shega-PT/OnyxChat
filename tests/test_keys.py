# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_keys.py — chaves do pipeline e identidade Ed25519 (messenger.keys).

Cobre os vectores oficiais do RFC 8032, o roundtrip aleatório, todos os
caminhos de rejeição de ``verificar`` e as validações dos dataclasses.
"""

from __future__ import annotations

import pytest

from messenger import keys

# Vectores do RFC 8032 §7.1 ------------------------------------------------
SEED1 = bytes.fromhex(
    "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"
)
PUB1 = bytes.fromhex(
    "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
)
SIG1 = bytes.fromhex(
    "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
    "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
)
SEED2 = bytes.fromhex(
    "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb"
)
PUB2 = bytes.fromhex(
    "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"
)
SIG2 = bytes.fromhex(
    "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
    "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"
)
SEED3 = bytes.fromhex(
    "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7"
)
PUB3 = bytes.fromhex(
    "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025"
)
SIG3 = bytes.fromhex(
    "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac"
    "18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a"
)


def test_vector_rfc8032_test1_vazio() -> None:
    """TEST 1: mensagem vazia — derivação, assinatura e verificação."""
    assert keys.chave_publica(SEED1) == PUB1
    assert keys.assinar(b"", SEED1) == SIG1
    assert keys.verificar(b"", SIG1, PUB1)


def test_vector_rfc8032_test2() -> None:
    """TEST 2: mensagem ``b'r'`` — e rejeição com mensagem trocada."""
    assert keys.chave_publica(SEED2) == PUB2
    assert keys.verificar(b"r", SIG2, PUB2)
    assert not keys.verificar(b"x", SIG2, PUB2)


def test_vector_rfc8032_test3() -> None:
    """TEST 3: mensagem ``af82``."""
    assert keys.chave_publica(SEED3) == PUB3
    assert keys.verificar(bytes.fromhex("af82"), SIG3, PUB3)


def test_roundtrip_identidade_aleatoria() -> None:
    """Identidade gerada aleatoriamente assina e verifica o roundtrip."""
    identidade = keys.Identidade.gerar()
    mensagem = b"Ola mundo!"
    assinatura = identidade.assinar(mensagem)
    assert identidade.verifica(mensagem, assinatura)
    assert not identidade.verifica(mensagem + b"!", assinatura)


def test_tamper_na_assinatura_rejeita() -> None:
    """Um bit à flípe na assinatura rejeita a mensagem."""
    identidade = keys.Identidade.gerar()
    mensagem = b"msg"
    assinatura = identidade.assinar(mensagem)
    adulterada = assinatura[:-1] + bytes([assinatura[-1] ^ 1])
    assert not keys.verificar(mensagem, adulterada, identidade.pub)


def test_rejeicoes_de_verificar() -> None:
    """Todos os caminhos de ``False``: comprimentos, curva, S ≥ L, chave."""
    assert not keys.verificar(b"m", SIG1[:63], PUB1)  # assinatura curta
    assert not keys.verificar(b"m", SIG1, PUB1[:31])  # pública curta
    # y=0 → ponto (sqrt(-1), 0) de ordem pequena: decodifica, mas a
    # equação de verificação falha.
    assert not keys.verificar(b"m", SIG1, bytes(32))
    # y=2 não tem x que satisfaça a curva → ValueError → False.
    assert not keys.verificar(b"m", SIG1, (2).to_bytes(32, "little"))
    # S = L (fora do intervalo) — R válido de SIG1 mantido.
    sig_s_grande = SIG1[:32] + keys._L.to_bytes(32, "little")
    assert not keys.verificar(b"", sig_s_grande, PUB1)
    # Pública com bit de sinal 1 (x ímpar) — outra chave, falha a equação.
    pub_impar = bytes(list(PUB1[:31]) + [PUB1[31] | 0x80])
    assert not keys.verificar(b"", SIG1, pub_impar)


def test_decodificar_ponto_comprimento_invalido() -> None:
    """Ponto com tamanho ≠ 32 bytes levanta ``ValueError``."""
    with pytest.raises(ValueError, match="32 bytes"):
        keys._decodificar_ponto(b"curto")
    with pytest.raises(ValueError, match="32 bytes"):
        keys._decodificar_ponto(bytes(33))


def test_recuperar_x_ramos_de_raiz() -> None:
    """Cobre a correção com sqrt(-1) (y=0) e a normalização par (y=10)."""
    assert keys._recuperar_x(0) == keys._I  # ramo (x² ≠ xx) → × sqrt(-1)
    x_10 = keys._recuperar_x(10)
    assert x_10 % 2 == 0  # normalização para x par executada


def test_validacoes_de_chave() -> None:
    """Tamanhos errados levantam ``ValueError`` em toda a API."""
    with pytest.raises(ValueError, match="32 bytes"):
        keys.chave_publica(b"curta")
    with pytest.raises(ValueError, match="32 bytes"):
        keys.assinar(b"m", b"curta")
    with pytest.raises(ValueError, match="k1"):
        keys.Chaves(k1=b"xx", k5=bytes(32), k9=bytes(32))
    with pytest.raises(ValueError, match="k9"):
        keys.Chaves(k1=bytes(32), k5=bytes(32), k9="nao-bytes")  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="seed"):
        keys.Identidade(seed=bytes(16))


def test_chaves_gerar_produz_trio_valido() -> None:
    """``Chaves.gerar`` devolve 32 bytes por campo e campos distintos."""
    chaves = keys.Chaves.gerar()
    assert len(chaves.k1) == len(chaves.k5) == len(chaves.k9) == 32
    assert chaves.k1 != chaves.k5 != chaves.k9


def test_identidade_gerar_e_propriedades() -> None:
    """``Identidade.gerar`` tem pub derivada estável e coerente."""
    identidade = keys.Identidade.gerar()
    assert len(identidade.seed) == 32
    assert identidade.pub == keys.chave_publica(identidade.seed)
    # A pública é atribuída uma vez na criação (não é propriedade computada).
    assert identidade.pub == keys.Identidade(seed=identidade.seed).pub
