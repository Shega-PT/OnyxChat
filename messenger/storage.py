# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""storage.py — keystore cifrado, anti-replay e cache do OnyxChat.

Três utilitários de persistência/estado, sem dependências externas:

* **ChaCha20-Poly1305** (RFC 8439) em Python puro + keystore JSON
  cifrado (:func:`guardar_keystore` / :func:`carregar_keystore`) — o
  segredo deriva-se da *passphrase* com PBKDF2-HMAC-SHA256 e um sal
  aleatório por gravação; qualquer adulteração ou passphrase errada
  falha na etiqueta AEAD (:class:`PassphraseErrada`). O formato é
  versionado pela própria magia (``ONYXKS<n>``) e a **leitura aceita
  todas as versões conhecidas** — ver :data:`FORMATOS_KEYSTORE`.
* :class:`RegistoAntiReplay` — rejeita identificadores repetidos com
  TTL (nonces de handshake, mensagens reenviadas).
* :class:`Cache` — cache LRU limitada para valores derivados.

Nota de desempenho: a implementação ChaCha20 é em *software* puro —
adequada a keystores pequenos (KB), não a streams grandes. O caminho
crítico de mensagens de chat vive no daemon Rust (libsodium).
"""

from __future__ import annotations

import hashlib
import hmac
import json
import os
import secrets
import struct
import time
from collections import OrderedDict
from collections.abc import Callable
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------
# ChaCha20-Poly1305 (RFC 8439) — constantes e primitivas
# ---------------------------------------------------------------------

_M32 = 0xFFFFFFFF
TAM_BLOCO = 64
TAM_NONCE = 12
TAM_ETIQUETA = 16
TAM_SAL = 16

#: Prefixo comum a todas as magics de formato.
PREFIXO_KEYSTORE = b"ONYXKS"

#: Magia do formato **v1** (legado, 7 bytes).
#:
#: Não pode levar um terminador: já há ficheiros v1 em disco, e mudar a
#: sua magic seria torná-los ilegíveis — o oposto do que se quer.
MAGIA_V1 = PREFIXO_KEYSTORE + b"1"

#: Magia de um formato **v2 ou superior**: `ONYXKS` ‖ `\x00` ‖ versão.
#:
#: O `\x00` existe por uma razão concreta. Sem ele, `ONYXKS12` (versão
#: 12) seria um superconjunto de `ONYXKS1` (versão 1), e a leitura
#: tentaria decifrar com o AAD errado — dando «passphrase errada» em vez
#: de «formato desconhecido», o que manda o utilizador rever a
#: passphrase quando o problema é o software.
#:
#: Um byte NUL não pode ser um dígito de versão, logo a ambiguidade fica
#: resolvida: v1 tem um dígito no índice 6, v2+ têm um NUL.
def _magica_versao(versao: int) -> bytes:
    """Magia (8 bytes) do formato com o número de versão indicado."""
    if not 2 <= versao <= 255:
        raise ValueError(f"versão de keystore fora do intervalo: {versao}")
    return PREFIXO_KEYSTORE + b"\x00" + bytes([versao])


#: Formatos reconhecidos na leitura, indexados pela versão.
#:
#: | versão | AAD do AEAD            | estado                  |
#: | ------ | ---------------------- | ----------------------- |
#: | 1      | `MAGIA_V1` (7 bytes)   | legada, só leitura      |
#: | 2      | `MAGIA ‖ sal` (24 B)   | actual, leitura+escrita |
FORMATOS_KEYSTORE: dict[int, bytes] = {
    1: MAGIA_V1,
    2: _magica_versao(2),
}

#: Versão escrita por :func:`guardar_keystore`. Só aumenta, e só quando a
#: leitura tolerante da versão anterior estiver implementada e testada.
VERSAO_KEYSTORE_ATUAL = 2

#: Prefixo do formato de escrita actual.
MAGIA = FORMATOS_KEYSTORE[VERSAO_KEYSTORE_ATUAL]

#: PBKDF2 por omissão — ajustável nos testes para não os atrasar.
ITERACOES_PADRAO = 200_000


def _rot32(valor: int, n: int) -> int:
    """Rotação circular de 32 bits à esquerda."""
    return ((valor << n) | (valor >> (32 - n))) & _M32


def _quarto(
    estado: list[int], a: int, b: int, c: int, d: int
) -> None:
    """Aplica uma *quarter round* do ChaCha20 ao ``estado`` (in-place)."""
    estado[a] = (estado[a] + estado[b]) & _M32
    estado[d] = _rot32(estado[d] ^ estado[a], 16)
    estado[c] = (estado[c] + estado[d]) & _M32
    estado[b] = _rot32(estado[b] ^ estado[c], 12)
    estado[a] = (estado[a] + estado[b]) & _M32
    estado[d] = _rot32(estado[d] ^ estado[a], 8)
    estado[c] = (estado[c] + estado[d]) & _M32
    estado[b] = _rot32(estado[b] ^ estado[c], 7)


def _bloco(chave: bytes, contador: int, nonce: bytes) -> bytes:
    """Função de permutação ChaCha20 de um bloco (64 bytes)."""
    estado = list(struct.unpack("<4I", b"expand 32-byte k"))
    estado += list(struct.unpack("<8I", chave))
    estado.append(contador & _M32)
    estado += list(struct.unpack("<3I", nonce))
    original = estado.copy()
    for _ in range(10):  # 20 rodadas = 10 duplas coluna/diagonal
        _quarto(estado, 0, 4, 8, 12)
        _quarto(estado, 1, 5, 9, 13)
        _quarto(estado, 2, 6, 10, 14)
        _quarto(estado, 3, 7, 11, 15)
        _quarto(estado, 0, 5, 10, 15)
        _quarto(estado, 1, 6, 11, 12)
        _quarto(estado, 2, 7, 8, 13)
        _quarto(estado, 3, 4, 9, 14)
    return struct.pack(
        "<16I",
        *((x + y) & _M32 for x, y in zip(estado, original)),
    )


def _chacha20_xor(chave: bytes, nonce: bytes, texto: bytes) -> bytes:
    """Cifra/descifra ``texto`` com ChaCha20 (contador parte em 1, RFC §2.4)."""
    saida = bytearray()
    for inicio in range(0, len(texto), TAM_BLOCO):
        indice = inicio // TAM_BLOCO
        keystream = _bloco(chave, 1 + indice, nonce)
        pedaco = texto[inicio : inicio + TAM_BLOCO]
        saida.extend(x ^ y for x, y in zip(pedaco, keystream))
    return bytes(saida)


def _poly1305(chave: bytes, mensagens: bytes) -> bytes:
    """MAC Poly1305 (RFC 8439 §2.5.2) sobre ``mensagens`` concatenadas."""
    r = int.from_bytes(chave[:16], "little") & 0x0FFFFFFC0FFFFFFC0FFFFFFC0FFFFFFF
    s = int.from_bytes(chave[16:32], "little")
    primo = (1 << 130) - 5
    acumulador = 0
    for inicio in range(0, len(mensagens), 16):
        bloco = mensagens[inicio : inicio + 16]
        # Cada bloco termina com byte 0x01 (terminador) em little-endian.
        acumulador = (acumulador + int.from_bytes(bloco + b"\x01", "little")) * r % primo
    return ((acumulador + s) & ((1 << 128) - 1)).to_bytes(16, "little")


def _alinhamento(dados: bytes) -> bytes:
    """Padding de 16 bytes para os campos do MAC (RFC §2.8.1)."""
    resto = len(dados) % 16
    return b"" if resto == 0 else b"\x00" * (16 - resto)


def _etiqueta(poly_key: bytes, aad: bytes, cifrado: bytes) -> bytes:
    """Calcula a etiqueta Poly1305 sobre o construto AEAD completo."""
    campos = (
        aad
        + _alinhamento(aad)
        + cifrado
        + _alinhamento(cifrado)
        + struct.pack("<QQ", len(aad), len(cifrado))
    )
    return _poly1305(poly_key, campos)


class EtiquetaInvalida(Exception):
    """Falha da etiqueta AEAD — dados adulterados ou segredo errado."""


def _cifrar_aead(chave: bytes, nonce: bytes, texto: bytes, aad: bytes) -> bytes:
    """AEAD encrypt: devolve ``ciphertext ‖ etiqueta`` (RFC §2.8.2)."""
    poly_key = _bloco(chave, 0, nonce)[:32]
    cifrado = _chacha20_xor(chave, nonce, texto)
    return cifrado + _etiqueta(poly_key, aad, cifrado)


def _decifrar_aead(
    chave: bytes, nonce: bytes, cifrado_etiqueta: bytes, aad: bytes
) -> bytes:
    """AEAD decrypt; levanta :class:`EtiquetaInvalida` se a tag não bater."""
    if len(cifrado_etiqueta) < TAM_ETIQUETA:
        raise EtiquetaInvalida("ciphertext curto demais para tag AEAD")
    cifrado = cifrado_etiqueta[:-TAM_ETIQUETA]
    etiqueta = cifrado_etiqueta[-TAM_ETIQUETA:]
    poly_key = _bloco(chave, 0, nonce)[:32]
    esperada = _etiqueta(poly_key, aad, cifrado)
    if not hmac.compare_digest(etiqueta, esperada):
        raise EtiquetaInvalida("etiqueta AEAD inválida")
    # ChaCha20 é cifra de fluxo: descifrar = XOR com o mesmo keystream.
    return _chacha20_xor(chave, nonce, cifrado)


# ---------------------------------------------------------------------
# Keystore cifrado
# ---------------------------------------------------------------------


class KeystoreInvalido(ValueError):
    """Ficheiro keystore malformado, ilegível ou sem conteúdo válido."""


class PassphraseErrada(KeystoreInvalido):
    """A etiqueta AEAD não confirma — passphrase errada ou adulteração."""


def derivar_chave(
    passphrase: str, sal: bytes, iteracoes: int = ITERACOES_PADRAO
) -> bytes:
    """Deriva a chave de 32 bytes da passphrase (PBKDF2-HMAC-SHA256)."""
    return hashlib.pbkdf2_hmac(
        "sha256", passphrase.encode("utf-8"), sal, iteracoes, dklen=32
    )


def guardar_keystore(
    dados: dict[str, Any],
    caminho: Path | str,
    passphrase: str,
    iteracoes: int = ITERACOES_PADRAO,
) -> None:
    """Cifra ``dados`` (JSON) com a ``passphrase`` e grava em ``caminho`` (0600).

    Sal e nonce aleatórios por gravação — duas gravações iguais nunca
    produzem o mesmo ficheiro.

    Grava sempre no formato actual (v2, com o sal no AAD). A leitura de
    formatos antigos é feita por :func:`carregar_keystore`, que é
    tolerante; a escrita não é retrocompatível porque não há motivo para
    producir ficheiros antigos.
    """
    sal = secrets.token_bytes(TAM_SAL)
    nonce = secrets.token_bytes(TAM_NONCE)
    # `bytearray` (best-effort) para poder limpar a chave derivada — a
    # garantia forte de zeroização é do Rust/C (`docs/key_management.md`).
    chave = bytearray(derivar_chave(passphrase, sal, iteracoes))
    texto = json.dumps(dados, ensure_ascii=False, sort_keys=True).encode("utf-8")
    magia = FORMATOS_KEYSTORE[VERSAO_KEYSTORE_ATUAL]
    try:
        # AAD v2: a magia **e** o sal. Incluir o sal impede que este
        # seja trocado por outro mantendo a etiqueta válida.
        conteudo = _cifrar_aead(chave, nonce, texto, magia + sal)
    finally:
        chave[:] = b"\x00" * len(chave)
    blob = magia + sal + nonce + conteudo
    fd = os.open(caminho, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "wb") as ficheiro:
        ficheiro.write(blob)
    # O modo do O_CREAT respeita o umask — garante 0600 sempre.
    os.chmod(caminho, 0o600)


def _reconhecer_versao(blob: bytes) -> int:
    """Devolve a versão do ficheiro a partir da magia, sem decifrar.

    Testa as magics da mais longa para a mais curta.

    # Limitação declarada: a v1 legada tem 7 bytes, e ``ONYXKS12`` tem
    ``ONYXKS1`` nos seus primeiros 7 bytes. A ambiguidade é **irredutível**
    — a informação não está no ficheiro, e nenhuma ordem de teste a
    remove.

    É por isso que a v2 adoptou o terminador NUL: a partir de v2 o esquema
    de nomes é ``ONYXKS\x00<n>``, e um NUL nunca é um dígito, logo nenhuma
    versão futura pode colidir com a v1 nem com outra v2+. O único ficheiro
    que ainda colide é um ``ONYXKS12…`` escrito segundo o **esquema
    abandonado** — que ninguém escreve a partir de v2.

    A consequência, se acontecer, é de **diagnosticabilidade**, não de
    segurança: o ficheiro é tratado como v1 e falha com
    :class:`PassphraseErrada` em vez de «formato desconhecido». Não pode
    ser aceite indevidamente, porque a etiqueta AEAD não verifica contra
    um AAD diferente.
    """
    for versao, _ in sorted(FORMATOS_KEYSTORE.items(), reverse=True):
        magia = FORMATOS_KEYSTORE[versao]
        if blob[: len(magia)] == magia:
            return versao
    conhecidas = ", ".join(m.decode("latin-1") for m in FORMATOS_KEYSTORE.values())
    raise KeystoreInvalido(
        f"keystore de formato desconhecido (mágica {blob[:8]!r}; "
        f"conhecidas: {conhecidas})"
    )


def carregar_keystore(
    caminho: Path | str, passphrase: str, iteracoes: int = ITERACOES_PADRAO
) -> dict[str, Any]:
    """Decifra e devolve ``dados``; erros tipados: :class:`KeystoreInvalido`,
    :class:`PassphraseErrada`.

    **Leitura tolerante de versões.** Qualquer formato em
    :data:`FORMATOS_KEYSTORE` é aceite: a versão vem da magia e decide o
    AAD a usar. Um ficheiro v1 escrito por uma versão antiga do programa
    continua a abrir numa versão nova — que é o requisito de poder
    conviver com utilizadores em versões diferentes do software.
    """
    try:
        blob = Path(caminho).read_bytes()
    except OSError as erro:
        raise KeystoreInvalido(f"keystore ilegível: {caminho}") from erro

    versao = _reconhecer_versao(blob)
    magia = FORMATOS_KEYSTORE[versao]
    cabecalho = len(magia) + TAM_SAL + TAM_NONCE
    if len(blob) < cabecalho + TAM_ETIQUETA:
        raise KeystoreInvalido("keystore malformado (mágica/comprimento)")
    sal = blob[len(magia) : len(magia) + TAM_SAL]
    nonce = blob[len(magia) + TAM_SAL : cabecalho]
    conteudo = blob[cabecalho:]
    chave = bytearray(derivar_chave(passphrase, sal, iteracoes))
    # AAD dependente da versão: v1 cobre só a magia, v2 cobre também o sal.
    aad = magia if versao == 1 else magia + sal
    try:
        texto = _decifrar_aead(chave, nonce, conteudo, aad)
    except EtiquetaInvalida as erro:
        raise PassphraseErrada("passphrase errada ou keystore adulterado") from erro
    finally:
        # Mesmo no erro: a chave derivada nunca fica em RAM depois.
        chave[:] = b"\x00" * len(chave)
    # A tag AEAD autentica o JSON — decodificação não pode falhar aqui.
    return json.loads(texto.decode("utf-8"))


# ---------------------------------------------------------------------
# Anti-replay
# ---------------------------------------------------------------------


class RegistoAntiReplay:
    """Rejeita identificadores repetidos, com expiração por TTL.

    Usado para nonces de handshake e reenvios: um identificador é
    aceite uma única vez dentro da janela ``ttl`` segundos; após
    expirar, pode voltar a ser aceite (memória limitada por design).
    """

    def __init__(
        self,
        ttl: float = 300.0,
        relogio: Callable[[], float] = time.monotonic,
    ) -> None:
        if ttl <= 0:
            raise ValueError("o TTL tem de ser positivo")
        self._ttl = ttl
        self._relogio = relogio
        self._vistos: dict[Any, float] = {}

    @property
    def total(self) -> int:
        """Número de identificadores atualmente retidos."""
        return len(self._vistos)

    def aceitar(self, identificador: Any) -> bool:
        """``True`` se novo (registado); ``False`` se for replay."""
        agora = self._relogio()
        expirados = [
            chave
            for chave, limite in self._vistos.items()
            if limite <= agora
        ]
        for chave in expirados:
            del self._vistos[chave]
        if identificador in self._vistos:
            return False
        self._vistos[identificador] = agora + self._ttl
        return True


# ---------------------------------------------------------------------
# Cache LRU
# ---------------------------------------------------------------------


class Cache:
    """Cache LRU limitada — descarta o menos usado quando atinge o limite."""

    def __init__(self, capacidade: int = 128) -> None:
        if capacidade <= 0:
            raise ValueError("a capacidade tem de ser positiva")
        self._capacidade = capacidade
        self._dados: OrderedDict[Any, Any] = OrderedDict()

    @property
    def total(self) -> int:
        """Número de entradas presentes na cache."""
        return len(self._dados)

    def obter(self, chave: Any) -> Any:
        """Devolve o valor (marcando-o como usado) ou ``None`` se ausente."""
        if chave not in self._dados:
            return None
        self._dados.move_to_end(chave)
        return self._dados[chave]

    def colocar(self, chave: Any, valor: Any) -> None:
        """Insere/atualiza ``chave``; se cheia, descarta a menos usada."""
        if chave in self._dados:
            self._dados.move_to_end(chave)
        self._dados[chave] = valor
        if len(self._dados) > self._capacidade:
            self._dados.popitem(last=False)
