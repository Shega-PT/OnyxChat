# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""keys.py — chaves do pipeline (K1/K5/K9) e identidade Ed25519 local.

Dois objetos de segurança são definidos aqui:

* :class:`Chaves` — o trio ``K1 ‖ K5 ‖ K9`` (32 bytes cada) exigido pelas
  camadas digitais do pipeline: ``K1`` é partilhada pelo par, ``K5`` é do
  remetente e ``K9`` é do receptor (ver ``docs/handshake.md``).
* :class:`Identidade` — o par Ed25519 do utilizador: a *seed* privada de
  32 bytes (nunca sai desta máquina nem é impressa em logs) e a chave
  pública derivada, usada para «estabilizar a identidade» em cada
  mensagem (assinatura de envelopes e de handshake).

O Ed25519 é uma implementação pura em Python do RFC 8032, sem
dependências externas — o projeto não tem *runtime deps*
(``pyproject.toml``). Serve para operações locais ocasionais (derivação
da pública e verificação local de envelopes); a assinatura de cada
envelope de chat é produzida pelo daemon Rust via IPC, que recebe a seed
apenas em memória e nunca a persiste.

Desempenho: a multiplicação escalar usa coordenadas homogéneas
estendidas com a lei completa de adição para ``a = −1``
(Hisil–Wong–Carter–Dawson 2008) — nenhuma inversão modular dentro do
laço, apenas no fim; inversões usam ``pow(z, −1, p)`` (Euclides), ~100×
mais rápido que o modular exponentiation.
"""

from __future__ import annotations

import hashlib
import secrets
from dataclasses import dataclass, field

TAMANHO_CHAVE = 32
TAMANHO_ASSINATURA = 64

# ---------------------------------------------------------------------
# Ed25519 (RFC 8032) — parâmetros da curva twisted-Edwards
# ---------------------------------------------------------------------

_P = 2**255 - 19
_L = 2**252 + 27742317777372353535851937790883648493
_D = (-121665 * pow(121666, _P - 2, _P)) % _P
_2D = (2 * _D) % _P
# sqrt(-1) modulo _P — usado em ``_recuperar_x`` quando a raiz direta
# não satisfaz x² = xx (caso de pontos inválidos ou xx não solúvel).
_I = pow(2, (_P - 1) // 4, _P)


def _h(*partes: bytes) -> bytes:
    """SHA-512 de ``partes`` concatenadas (função H do RFC 8032)."""
    return hashlib.sha512(b"".join(partes)).digest()


def _ponto_na_curva(x: int, y: int) -> bool:
    """Confirma a equação ``-x² + y² = 1 + d·x²·y²`` da curva."""
    return (-x * x + y * y - 1 - _D * x * x * y * y) % _P == 0


def _recuperar_x(y: int) -> int:
    """Devolve o ``x`` canónico (par, ``x ≥ 0``) da curva para o ``y``.

    Segue ``xrecover`` do RFC 8032 (§5.1.3): tenta a raiz quadrática
    direta de ``xx = (y²−1)/(d·y²+1)`` e, se ``x² ≠ xx``, corrige com a
    raiz de ``−1``. O chamador valida a curva em seguida — esta função
    nunca levanta exceção por si só.
    """
    xx = (y * y - 1) * pow(_D * y * y + 1, -1, _P) % _P
    x = pow(xx, (_P + 3) // 8, _P)
    if (x * x - xx) % _P != 0:
        x = (x * _I) % _P
    if x % 2 != 0:
        x = _P - x
    return x


# ---------------------------------------------------------------------
# Pontos em coordenadas homogéneas estendidas (X : Y : Z : T),
# com x = X/Z, y = Y/Z e T = XY/Z. Lei de adição completa para a = −1
# (add-2008-hwcd-3) — usada também como dobramento (unificada).
# ---------------------------------------------------------------------

_Ponto = tuple[int, int, int, int]


def _de_afim(x: int, y: int) -> _Ponto:
    """Converte ponto afim (x, y) → coordenadas estendidas."""
    x %= _P
    y %= _P
    return (x, y, 1, x * y % _P)


def _para_afim(ponto: _Ponto) -> tuple[int, int]:
    """Converte estendidas → afim (uma única inversão modular)."""
    x, y, z, _t = ponto
    inv_z = pow(z, -1, _P)
    return (x * inv_z % _P, y * inv_z % _P)


def _iguais(p1: _Ponto, p2: _Ponto) -> bool:
    """Igualdade de pontos em projeção (sem normalizar, sem inversão)."""
    x1, y1, z1, _ = p1
    x2, y2, z2, _ = p2
    return (x1 * z2 - x2 * z1) % _P == 0 and (y1 * z2 - y2 * z1) % _P == 0


def _soma(p1: _Ponto, p2: _Ponto) -> _Ponto:
    """Soma (unificada, inclui dobramento) — add-2008-hwcd-3, a = −1."""
    x1, y1, z1, t1 = p1
    x2, y2, z2, t2 = p2
    a = (y1 - x1) * (y2 - x2) % _P
    b = (y1 + x1) * (y2 + x2) % _P
    c = _2D * t1 * t2 % _P
    d = 2 * z1 * z2 % _P
    e = b - a
    f = d - c
    g = d + c
    h = b + a
    return (e * f % _P, g * h % _P, f * g % _P, e * h % _P)


def _escalar(ponto: _Ponto, e: int) -> _Ponto:
    """Multiplicação escalar por *double-and-add* recursivo.

    A recursão tem profundidade ``log2(e) + 1 ≤ 255`` — dentro do limite
    de recursão do Python e usada apenas em operações locais ocasionais.
    """
    if e == 0:
        return (0, 1, 1, 0)
    meio = _escalar(ponto, e // 2)
    meio = _soma(meio, meio)
    if e & 1:
        meio = _soma(meio, ponto)
    return meio


def _codificar_ponto(ponto: tuple[int, int]) -> bytes:
    """Codifica ponto afim → 32 bytes (y little-endian + bit de sinal de x)."""
    x, y = ponto
    bits = [(y >> i) & 1 for i in range(255)] + [x & 1]
    return bytes(
        sum(bits[i * 8 + j] << j for j in range(8)) for i in range(32)
    )


def _decodificar_ponto(dados: bytes) -> tuple[int, int]:
    """Decodifica 32 bytes → ponto afim; levanta :class:`ValueError` se inválido.

    Layout: bits 0–254 são ``y`` (little-endian) e o bit 255 é o sinal
    de ``x`` — o mesmo que ``_codificar_ponto`` produz.
    """
    if len(dados) != TAMANHO_CHAVE:
        raise ValueError("ponto Ed25519 tem de ter 32 bytes")
    valor = int.from_bytes(dados, "little")
    y = valor & ((1 << 255) - 1)
    x = _recuperar_x(y)
    if (x & 1) != (valor >> 255):
        x = _P - x
    if not _ponto_na_curva(x, y):
        raise ValueError("ponto fora da curva Ed25519")
    return (x, y)


def _escalar_ajustado(h: bytes) -> int:
    """Expande a seed e aplica o *clamp* do RFC 8032 (§5.1.5).

    Limpa os bits 0–2 e 255, e força o bit 254 — garante que o escalar
    está no intervalo pretendido e é par (evita pequenos subgrupos).
    """
    bruto = int.from_bytes(h[:TAMANHO_CHAVE], "little")
    return (bruto & ((1 << 255) - 8)) | (1 << 254)


# Ponto base B do Ed25519 (y = 4/5).
_BY = 4 * pow(5, -1, _P) % _P
_B: _Ponto = _de_afim(_recuperar_x(_BY), _BY)


def chave_publica(seed: bytes) -> bytes:
    """Deriva a chave pública (32 bytes) da seed privada."""
    if not isinstance(seed, bytes) or len(seed) != TAMANHO_CHAVE:
        raise ValueError("a seed Ed25519 tem de ter exatamente 32 bytes")
    a = _escalar_ajustado(_h(seed))
    return _codificar_ponto(_para_afim(_escalar(_B, a)))


def _assinar(mensagem: bytes, seed: bytes, pub: bytes) -> bytes:
    """Núcleo da assinatura (RFC 8032 §5.1.6) com a pública já derivada."""
    h = _h(seed)
    a = _escalar_ajustado(h)
    r = int.from_bytes(_h(h[TAMANHO_CHAVE:], mensagem), "little") % _L
    grande_r = _codificar_ponto(_para_afim(_escalar(_B, r)))
    k = int.from_bytes(_h(grande_r, pub, mensagem), "little") % _L
    s = (r + k * a) % _L
    return grande_r + s.to_bytes(TAMANHO_CHAVE, "little")


def assinar(mensagem: bytes, seed: bytes) -> bytes:
    """Assina ``mensagem`` com a seed (RFC 8032 §5.1.6) → 64 bytes."""
    if not isinstance(seed, bytes) or len(seed) != TAMANHO_CHAVE:
        raise ValueError("a seed Ed25519 tem de ter exatamente 32 bytes")
    return _assinar(mensagem, seed, chave_publica(seed))


def verificar(mensagem: bytes, assinatura: bytes, pub: bytes) -> bool:
    """Verifica ``assinatura`` de ``mensagem`` sob ``pub`` (RFC 8032 §5.1.7).

    Devolve ``False`` para qualquer entrada malformada (comprimentos,
    pontos fora da curva, ``S ≥ L``) ou assinatura adulterada — nunca
    levanta exceção, para que o chamador possa tratar rejeição como
    caminho normal de controle.
    """
    if len(assinatura) != TAMANHO_ASSINATURA or len(pub) != TAMANHO_CHAVE:
        return False
    try:
        ponto_pub = _de_afim(*_decodificar_ponto(pub))
        ponto_r = _de_afim(*_decodificar_ponto(assinatura[:TAMANHO_CHAVE]))
    except ValueError:
        return False
    s = int.from_bytes(assinatura[TAMANHO_CHAVE:], "little")
    if s >= _L:
        return False
    k = (
        int.from_bytes(
            _h(assinatura[:TAMANHO_CHAVE], pub, mensagem), "little"
        )
        % _L
    )
    # Equação de verificação: [s]·B == R + [k]·A (tudo em projeção).
    return _iguais(_escalar(_B, s), _soma(ponto_r, _escalar(ponto_pub, k)))


# ---------------------------------------------------------------------
# Objetos de segurança locais
# ---------------------------------------------------------------------


def _validar_tamanho(nome: str, valor: object) -> None:
    """Garante ``bytes`` de 32 bytes; levanta :class:`ValueError` caso contrário."""
    if not isinstance(valor, bytes) or len(valor) != TAMANHO_CHAVE:
        raise ValueError(f"{nome} tem de ser bytes de {TAMANHO_CHAVE} bytes")


@dataclass(frozen=True)
class Chaves:
    """Trio de chaves do pipeline: ``K1`` (par), ``K5`` (remetente), ``K9`` (receptor)."""

    k1: bytes
    k5: bytes
    k9: bytes

    def __post_init__(self) -> None:
        for nome, valor in (("k1", self.k1), ("k5", self.k5), ("k9", self.k9)):
            _validar_tamanho(nome, valor)

    @classmethod
    def gerar(cls) -> Chaves:
        """Gera um trio novo com o CSPRNG do sistema (``secrets``)."""
        return cls(
            k1=secrets.token_bytes(TAMANHO_CHAVE),
            k5=secrets.token_bytes(TAMANHO_CHAVE),
            k9=secrets.token_bytes(TAMANHO_CHAVE),
        )


@dataclass(frozen=True)
class Identidade:
    """Identidade local: seed privada Ed25519 + pública derivada."""

    seed: bytes
    #: Chave pública derivada da seed — calculada uma vez na criação.
    pub: bytes = field(init=False)

    def __post_init__(self) -> None:
        _validar_tamanho("a seed", self.seed)
        object.__setattr__(self, "pub", chave_publica(self.seed))

    @classmethod
    def gerar(cls) -> Identidade:
        """Cria uma identidade nova a partir do CSPRNG do sistema."""
        return cls(seed=secrets.token_bytes(TAMANHO_CHAVE))

    def assinar(self, mensagem: bytes) -> bytes:
        """Assina ``mensagem`` com a identidade local (64 bytes).

        Reutiliza a pública já derivada em ``__post_init__`` — evita uma
        multiplicação escalar por assinatura.
        """
        return _assinar(mensagem, self.seed, self.pub)

    def verifica(self, mensagem: bytes, assinatura: bytes) -> bool:
        """Verifica uma assinatura feita *por esta* identidade."""
        return verificar(mensagem, assinatura, self.pub)
