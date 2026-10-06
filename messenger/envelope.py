# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""envelope.py — montagem e parsing do envelope binário do OnyxChat.

Implementa o formato especificado em ``docs/message_format.md``:

```
 offset  campo                       tamanho
    0    versão (0x01)               1 byte
    1    nonce1 (K1)                 12 bytes
   13    nonce5 (K5)                 12 bytes
   25    nonce9 (K9)                 12 bytes
   37    ciphertext final (K9)       N bytes (N ≥ 16)
  37+N   assinatura Ed25519          64 bytes  (sobre [0 .. 37+N))
  ----   total                       101 + N (mínimo 117)
```

Parsing sempre por comprimento — nunca por delimitadores — e qualquer
desvio (curto, demasiado grande, versão desconhecida) rejeita o envelope
inteiro com :class:`EnvelopeInvalido`, antes de qualquer decifragem. O
módulo não tem dependências: opera sobre ``bytes`` puros e é usado tanto
pelo pipeline (verificação local da assinatura) como pelos testes.
"""

from __future__ import annotations

from dataclasses import dataclass

VERSAO = 0x01
TAM_NONCE = 12
TAM_ASSINATURA = 64
#: 1 + 3·12 + 64 fixos + 16 da menor tag AEAD (Poly1305/ChaCha20).
TAMANHO_MINIMO = 117
#: Tamanho fixo da estrutura sem o ciphertext (101 bytes).
TAM_ESTRUTURA = 101
#: Tamanho máximo de um envelope (``docs/message_format.md`` §Limites):
#: ``MAX_PLAINTEXT`` (65 536) + sobretotal máximo de todas as camadas
#: (K1+16, K4+5, K5+16, K7+2, K9+16, cabeçalho+assinatura+101 = 156).
#: Envelopes acima disto são impossíveis de produzir pela implementação
#: — rejeitá-los é *fail-closed* e evita alocações proporcionais a um
#: comprimento não validado.
TAMANHO_MAXIMO = 65_692

# Offsets dos campos dentro do envelope.
_OFF_NONCE1 = 1
_OFF_NONCE5 = _OFF_NONCE1 + TAM_NONCE
_OFF_NONCE9 = _OFF_NONCE5 + TAM_NONCE
_OFF_CT = _OFF_NONCE9 + TAM_NONCE


class EnvelopeInvalido(ValueError):
    """Envelope malformado — rejeitado sem chegar ao pipeline de decifragem."""


@dataclass(frozen=True)
class Envelope:
    """Envelope parseado (ou pronto a ser serializado)."""

    versao: int
    nonce1: bytes
    nonce5: bytes
    nonce9: bytes
    ciphertext: bytes
    assinatura: bytes

    @property
    def regiao_assinada(self) -> bytes:
        """Bytes cobertos pela assinatura: ``versão ‖ nonces ‖ ciphertext``."""
        return (
            bytes([self.versao])
            + self.nonce1
            + self.nonce5
            + self.nonce9
            + self.ciphertext
        )

    def para_bytes(self) -> bytes:
        """Serializa o envelope completo (inclui a assinatura final)."""
        return self.regiao_assinada + self.assinatura


def montar(
    nonce1: bytes,
    nonce5: bytes,
    nonce9: bytes,
    ciphertext: bytes,
    assinatura: bytes,
    versao: int = VERSAO,
) -> Envelope:
    """Constrói um :class:`Envelope` validando tamanhos e versão."""
    if versao != VERSAO:
        raise EnvelopeInvalido(f"versão desconhecida: {versao}")
    for nome, valor, tamanho in (
        ("nonce1", nonce1, TAM_NONCE),
        ("nonce5", nonce5, TAM_NONCE),
        ("nonce9", nonce9, TAM_NONCE),
        ("assinatura", assinatura, TAM_ASSINATURA),
    ):
        if len(valor) != tamanho:
            raise EnvelopeInvalido(f"{nome} tem de ter {tamanho} bytes")
    if len(ciphertext) < TAMANHO_MINIMO - TAM_ESTRUTURA:
        raise EnvelopeInvalido("ciphertext curto demais (tag AEAD em falta)")
    return Envelope(versao, nonce1, nonce5, nonce9, ciphertext, assinatura)


def parsear(dados: bytes) -> Envelope:
    """Parseia ``dados`` num :class:`Envelope` ou levanta :class:`EnvelopeInvalido`.

    Regras (message_format.md), por ordem normativa: comprimento ≥ 117,
    comprimento ≤ ``TAMANHO_MAXIMO`` (65 692), versão == 0x01, e a
    assinatura é o bloco final de 64 bytes — a região assinada são
    todos os bytes anteriores.

    Nenhum *slice* do ``dados`` é sequer calculado antes de as três
    verificações de forma passarem.
    """
    if len(dados) < TAMANHO_MINIMO:
        raise EnvelopeInvalido(
            f"envelope curto: {len(dados)} < {TAMANHO_MINIMO} bytes"
        )
    if len(dados) > TAMANHO_MAXIMO:
        raise EnvelopeInvalido(
            f"envelope demasiado grande: {len(dados)} > {TAMANHO_MAXIMO} bytes"
        )
    if dados[0] != VERSAO:
        raise EnvelopeInvalido(f"versão desconhecida: {dados[0]}")
    return Envelope(
        versao=dados[0],
        nonce1=dados[_OFF_NONCE1:_OFF_NONCE5],
        nonce5=dados[_OFF_NONCE5:_OFF_NONCE9],
        nonce9=dados[_OFF_NONCE9:_OFF_CT],
        ciphertext=dados[_OFF_CT:-TAM_ASSINATURA],
        assinatura=dados[-TAM_ASSINATURA:],
    )
