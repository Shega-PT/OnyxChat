# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_envelope.py — parsing/monstagem do envelope (messenger.envelope).

Cobre as regras de ``docs/message_format.md``: comprimento mínimo,
versão, offsets dos nonces, região assinada e roundtrip.
"""

from __future__ import annotations

import pytest

from messenger import envelope

NONCE = b"\x11" * 12
CT = b"\x22" * 40
SIG = b"\x33" * 64


def test_parsear_offsets_e_roundtrip() -> None:
    """O parse recupera exatamente os campos e ``para_bytes`` inverte-o."""
    dados = b"\x01" + b"a" * 12 + b"b" * 12 + b"c" * 12 + CT + SIG
    env = envelope.parsear(dados)
    assert env.versao == 0x01
    assert env.nonce1 == b"a" * 12
    assert env.nonce5 == b"b" * 12
    assert env.nonce9 == b"c" * 12
    assert env.ciphertext == CT
    assert env.assinatura == SIG
    assert env.para_bytes() == dados


def test_regiao_assinada_e_prefixo() -> None:
    """A região assinada cobre versão + nonces + ciphertext, sem assinatura."""
    dados = b"\x01" + b"a" * 12 + b"b" * 12 + b"c" * 12 + CT + SIG
    env = envelope.parsear(dados)
    assert env.regiao_assinada == dados[:-64]
    assert dados.startswith(env.regiao_assinada)
    assert len(env.regiao_assinada) == 37 + len(CT)


def test_parsear_rejeita_curto() -> None:
    """Menos de 117 bytes → ``EnvelopeInvalido`` (EnvelopeCurto)."""
    with pytest.raises(envelope.EnvelopeInvalido, match="curto"):
        envelope.parsear(bytes(116))
    with pytest.raises(envelope.EnvelopeInvalido, match="curto"):
        envelope.parsear(b"")


def test_parsear_rejeita_versao() -> None:
    """Versão ≠ 0x01 → ``EnvelopeInvalido`` (VersaoDesconhecida)."""
    dados = bytearray(bytes(117))
    dados[0] = 0x09
    with pytest.raises(envelope.EnvelopeInvalido, match="vers"):
        envelope.parsear(bytes(dados))


def test_parsear_rejeita_demasiado_grande() -> None:
    """Acima de 65 692 bytes → ``EnvelopeInvalido`` (antes da versão)."""
    with pytest.raises(envelope.EnvelopeInvalido, match="demasiado grande"):
        envelope.parsear(b"\x01" * (envelope.TAMANHO_MAXIMO + 1))
    # Ordem normativa (mín → máx → versão): um envelope grande com
    # versão inválida reporta o tamanho, nunca a versão.
    with pytest.raises(envelope.EnvelopeInvalido, match="demasiado grande"):
        envelope.parsear(b"\x09" * (envelope.TAMANHO_MAXIMO + 1))
    # Exatamente no limite continua a ser parseável.
    assert len(envelope.parsear(b"\x01" * envelope.TAMANHO_MAXIMO).para_bytes()) == (
        envelope.TAMANHO_MAXIMO
    )


def test_limites_documentados() -> None:
    """Os limites coincidem com ``docs/message_format.md`` (65 536 + 156)."""
    assert envelope.TAMANHO_MINIMO == 117
    assert envelope.TAMANHO_MAXIMO == 65_692
    assert envelope.TAMANHO_MAXIMO == 65_536 + 156


def test_montar_valida_campos() -> None:
    """``montar`` valida versão e tamanhos; roundtrip byte-perfeito."""
    env = envelope.montar(NONCE, NONCE, NONCE, CT, SIG)
    dados = env.para_bytes()
    assert len(dados) == 101 + len(CT)
    volta = envelope.parsear(dados)
    assert volta == env


def test_montar_rejeita_versao_e_tamanhos() -> None:
    """Versão errada, nonces/assinatura com tamanho errado, tag em falta."""
    with pytest.raises(envelope.EnvelopeInvalido, match="vers"):
        envelope.montar(NONCE, NONCE, NONCE, CT, SIG, versao=0x02)
    with pytest.raises(envelope.EnvelopeInvalido, match="nonce1"):
        envelope.montar(b"curto", NONCE, NONCE, CT, SIG)
    with pytest.raises(envelope.EnvelopeInvalido, match="nonce5"):
        envelope.montar(NONCE, b"curto", NONCE, CT, SIG)
    with pytest.raises(envelope.EnvelopeInvalido, match="nonce9"):
        envelope.montar(NONCE, NONCE, b"curto", CT, SIG)
    with pytest.raises(envelope.EnvelopeInvalido, match="assinatura"):
        envelope.montar(NONCE, NONCE, NONCE, CT, b"curta")
    with pytest.raises(envelope.EnvelopeInvalido, match="ciphertext"):
        envelope.montar(NONCE, NONCE, NONCE, bytes(15), SIG)


def test_tamanhos_documentados() -> None:
    """Constantes batem certo com a spec (mínimo 117, estrutura 101)."""
    assert envelope.TAMANHO_MINIMO == 117
    assert envelope.TAM_ESTRUTURA == 101
    assert envelope.TAM_NONCE == 12
    assert envelope.TAM_ASSINATURA == 64
    assert envelope.VERSAO == 0x01
