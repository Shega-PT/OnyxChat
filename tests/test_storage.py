# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_storage.py — keystore AEAD, anti-replay e cache (messenger.storage).

Valida a ChaCha20-Poly1305 contra o vector oficial RFC 8439 §2.8.2 e
cobre os erros tipados do keystore, o registo anti-replay com relógio
injetado e a semântica LRU da cache.
"""

from __future__ import annotations

import json
import stat
import tempfile
from pathlib import Path

import pytest

from messenger import storage

# --- Vector RFC 8439 §2.8.2 (paridade com a referência) ---------------
_CHAVE = bytes(range(0x80, 0xA0))
_NONCE = bytes.fromhex("070000004041424344454647")
_AAD = bytes.fromhex("50515253c0c1c2c3c4c5c6c7")
_PT = bytes.fromhex(
    "4c616469657320616e642047656e746c656d656e206f662074686520636c6173"
    "73206f66202739393a204966204920636f756c64206f6666657220796f75206f"
    "6e6c79206f6e652074697020666f7220746865206675747572652c2073756e73"
    "637265656e20776f756c642062652069742e"
)
_CT_TAG = bytes.fromhex(
    "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d6"
    "3dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b36"
    "92ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc"
    "3ff4def08e4b7a9de576d26586cec64b6116"
    "1ae10b594f09e26a7e902ecbd0600691"
)


def test_vetor_rfc_8439_seccao_2_8_2() -> None:
    """AEAD bate certo no vector oficial e decifra em verso."""
    saida = storage._cifrar_aead(_CHAVE, _NONCE, _PT, _AAD)
    assert saida == _CT_TAG
    assert storage._decifrar_aead(_CHAVE, _NONCE, saida, _AAD) == _PT


def test_chacha20_multibloco_e_contra() -> None:
    """O keystream cobre >64 bytes e o XOR é involutivo."""
    texto = bytes(range(256)) * 3
    cifrado = storage._chacha20_xor(_CHAVE, _NONCE, texto)
    assert cifrado != texto
    assert storage._chacha20_xor(_CHAVE, _NONCE, cifrado) == texto
    assert len(cifrado) == len(texto)


def test_aead_rejeita_tag_errada_e_curta() -> None:
    """Tag adulterada → ``EtiquetaInvalida``; payload curto → idem."""
    adulterado = bytearray(_CT_TAG)
    adulterado[-1] ^= 0x01
    with pytest.raises(storage.EtiquetaInvalida, match="etiqueta AEAD"):
        storage._decifrar_aead(_CHAVE, _NONCE, bytes(adulterado), _AAD)
    with pytest.raises(storage.EtiquetaInvalida, match="curto"):
        storage._decifrar_aead(_CHAVE, _NONCE, bytes(15), _AAD)
    with pytest.raises(storage.EtiquetaInvalida, match="curto"):
        storage._decifrar_aead(_CHAVE, _NONCE, b"", _AAD)


def test_poly1305_e_alinhamento() -> None:
    """MAC com blocos de 16 bytes exactos (resto == 0) e AAD multi-bloco."""
    poly_key = bytes(range(32))
    tag = storage._etiqueta(poly_key, bytes(16), bytes(16) * 2)
    assert len(tag) == 16
    assert storage._alinhamento(b"") == b""
    assert storage._alinhamento(bytes(16)) == b""
    assert storage._alinhamento(bytes(15)) == b"\x00"
    assert storage._etiqueta(poly_key, b"aad", b"cifrado") != tag


def test_derivar_chave_deterministica() -> None:
    """PBKDF2 é determinístico e sensível a sal, passphrase e iterações."""
    k1 = storage.derivar_chave("segredo", b"s" * 16, 1000)
    k2 = storage.derivar_chave("segredo", b"s" * 16, 1000)
    assert k1 == k2 and len(k1) == 32
    assert k1 != storage.derivar_chave("outro", b"s" * 16, 1000)
    assert k1 != storage.derivar_chave("segredo", b"t" * 16, 1000)
    assert k1 != storage.derivar_chave("segredo", b"s" * 16, 2000)


def test_keystore_roundtrip_e_permissoes(tmp_path: Path) -> None:
    """Guardar→carregar devolve os dados e o ficheiro fica 0600."""
    caminho = tmp_path / "keystore.bin"
    dados = {"identidade": "abc", "n": 42, "lista": [1, 2]}
    storage.guardar_keystore(dados, caminho, "passphrase certa", iteracoes=1000)
    modo = stat.S_IMODE(caminho.stat().st_mode)
    assert modo == 0o600
    assert storage.carregar_keystore(
        caminho, "passphrase certa", iteracoes=1000
    ) == dados


def test_keystore_bytes_nao_repetem(tmp_path: Path) -> None:
    """Sal e nonce aleatórios: duas gravações iguais diferem no ficheiro."""
    caminho_a, caminho_b = tmp_path / "a.bin", tmp_path / "b.bin"
    storage.guardar_keystore({"x": 1}, caminho_a, "p", iteracoes=1000)
    storage.guardar_keystore({"x": 1}, caminho_b, "p", iteracoes=1000)
    assert caminho_a.read_bytes() != caminho_b.read_bytes()


def test_keystore_passphrase_errada(tmp_path: Path) -> None:
    """Passphrase errada → ``PassphraseErrada`` (subclasse de Invalido)."""
    caminho = tmp_path / "k.bin"
    storage.guardar_keystore({"x": 1}, caminho, "certa", iteracoes=1000)
    with pytest.raises(storage.PassphraseErrada, match="passphrase"):
        storage.carregar_keystore(caminho, "errada", iteracoes=1000)
    with pytest.raises(storage.KeystoreInvalido):
        storage.carregar_keystore(caminho, "errada", iteracoes=1000)


def test_keystore_adulterado(tmp_path: Path) -> None:
    """Um byte alterado no corpo → ``PassphraseErrada`` (tag falha)."""
    caminho = tmp_path / "k.bin"
    storage.guardar_keystore({"x": 1}, caminho, "p", iteracoes=1000)
    blob = bytearray(caminho.read_bytes())
    blob[-1] ^= 0x01
    caminho.write_bytes(bytes(blob))
    with pytest.raises(storage.PassphraseErrada):
        storage.carregar_keystore(caminho, "p", iteracoes=1000)


def test_keystore_malformado_e_illegivel(tmp_path: Path) -> None:
    """Sem magia, corpo curto ou ficheiro ausente → ``KeystoreInvalido``."""
    curto = tmp_path / "curto.bin"
    curto.write_bytes(storage.MAGIA + bytes(10))
    with pytest.raises(storage.KeystoreInvalido, match="malformado"):
        storage.carregar_keystore(curto, "p", iteracoes=1000)
    sem_magia = tmp_path / "sem.bin"
    sem_magia.write_bytes(b"OUTROMAG" + bytes(64))
    # Uma magic desconhecida é um problema diferente de um ficheiro
    # truncado, e a mensagem tem de dizer quais sao as validas — e sao
    # as duas: quem tem um ficheiro v1 nao deve ser mandado ao lixo
    # por causa de uma actualizacao de formato.
    with pytest.raises(storage.KeystoreInvalido, match="formato desconhecido"):
        storage.carregar_keystore(sem_magia, "p", iteracoes=1000)
    apenas_magia = tmp_path / "apenas-magia.bin"
    apenas_magia.write_bytes(storage.MAGIA)
    with pytest.raises(storage.KeystoreInvalido, match="malformado"):
        storage.carregar_keystore(apenas_magia, "p", iteracoes=1000)
    with pytest.raises(storage.KeystoreInvalido, match="ilegível"):
        storage.carregar_keystore(tmp_path / "nao-existe.bin", "p")


def test_anti_replay_novo_replay_e_expiracao() -> None:
    """Replay rejeitado, TTL expira e limpa entradas, relógio injetado."""
    agora = [1000.0]
    registo = storage.RegistoAntiReplay(ttl=60.0, relogio=lambda: agora[0])
    assert registo.aceitar("nonce-a") is True
    assert registo.total == 1
    assert registo.aceitar("nonce-a") is False  # replay na janela
    agora[0] += 61.0  # janela expirou → pode ser aceite outra vez
    assert registo.aceitar("nonce-a") is True
    assert registo.total == 1  # a entrada expirada foi limpa


def test_anti_replay_limpa_expirados_a_cada_aceite() -> None:
    """Entradas expiradas desaparecem mesmo sem voltar a usá-las."""
    agora = [0.0]
    registo = storage.RegistoAntiReplay(ttl=10.0, relogio=lambda: agora[0])
    registo.aceitar("velho")
    registo.aceitar("tambem-velho")
    assert registo.total == 2
    agora[0] = 20.0
    registo.aceitar("novo")
    assert registo.total == 1
    assert registo.aceitar("velho") is True  # já não estava retido


def test_anti_replay_ttl_invalido() -> None:
    """TTL ≤ 0 → ``ValueError`` (configuração impossível)."""
    with pytest.raises(ValueError, match="positivo"):
        storage.RegistoAntiReplay(ttl=0)
    with pytest.raises(ValueError, match="positivo"):
        storage.RegistoAntiReplay(ttl=-5)


def test_cache_lru_completa() -> None:
    """Faltas/hits, actualização e descarte do menos usado no limite."""
    with pytest.raises(ValueError, match="positiva"):
        storage.Cache(capacidade=0)
    cache = storage.Cache(capacidade=2)
    assert cache.total == 0
    assert cache.obter("a") is None  # miss
    cache.colocar("a", 1)
    cache.colocar("b", 2)
    assert cache.obter("a") == 1 and cache.total == 2  # hit marca "a" como usado
    cache.colocar("a", 10)  # actualizar não aumenta o total
    assert cache.total == 2 and cache.obter("a") == 10
    cache.colocar("c", 3)  # cheia → descarta "b" (menos usada)
    assert cache.total == 2
    assert cache.obter("b") is None
    assert cache.obter("c") == 3


def test_cache_obter_devolve_valor_ou_none() -> None:
    """``obter`` devolve o valor presente e ``None`` apenas em miss."""
    cache = storage.Cache()
    cache.colocar("chave", "valor")
    assert cache.obter("chave") == "valor"
    assert cache.obter("outra") is None


# =====================================================================
# Versionamento tolerante do keystore (F2.3)
# =====================================================================
#
# Princípio: o sistema tem de funcionar com utilizadores em versões
# diferentes do software. Um keystore escrito por uma versão antiga tem de
# continuar a abrir; a escrita faz-se sempre no formato mais recente.
#
# Isto é testado com um **produtor v1 independente** — um fixture escrito
# à mão com a magia e o AAD do formato v1 — e não relendo o que o próprio
# código actual produz. Se o teste usasse `guardar_keystore` para criar o
# v1, estaria a testar a implementação contra ela própria.


def _escrever_keystore_v1(
    caminho: Path, dados: dict, passphrase: str, iteracoes: int
) -> None:
    """Escreve um keystore no formato **v1** (AAD = só a magia).

    Reproduz o que uma versão antiga do programa teria escrito:
    ``ONYXKS1 ‖ sal ‖ nonce ‖ AEAD(aad=ONYXKS1)``.
    """
    sal = b"\x11" * storage.TAM_SAL
    nonce = b"\x22" * storage.TAM_NONCE
    chave = storage.derivar_chave(passphrase, sal, iteracoes)
    texto = json.dumps(dados, ensure_ascii=False, sort_keys=True).encode()
    conteudo = storage._cifrar_aead(chave, nonce, texto, storage.MAGIA_V1)
    caminho.write_bytes(storage.MAGIA_V1 + sal + nonce + conteudo)
    caminho.chmod(0o600)


def test_keystore_v1_ainda_abre() -> None:
    """Um ficheiro v1 escrito por uma versão antiga continua a abrir.

    Este é o teste que justifica a leitura tolerante. Sem ele, um bump de
    formato perderia os dados de quem actualizou o software.
    """
    caminho = Path(tempfile.mkdtemp()) / "antigo.ks"
    dados = {"k1": "aa" * 32, "amigos": 3}
    _escrever_keystore_v1(caminho, dados, "pass", 1000)

    assert storage.carregar_keystore(caminho, "pass", iteracoes=1000) == dados


def test_keystore_v2_e_o_formato_de_escrita() -> None:
    """A escrita usa sempre a versão mais recente."""
    caminho = Path(tempfile.mkdtemp()) / "novo.ks"
    storage.guardar_keystore({"a": 1}, caminho, "pass", iteracoes=1000)
    blob = caminho.read_bytes()

    assert blob.startswith(storage.MAGIA)
    # A v2+ usa `ONYXKS\x00<n>`: o NUL é o que a separa da v1 legada.
    assert storage.MAGIA == b"ONYXKS\x00\x02"
    assert storage.VERSAO_KEYSTORE_ATUAL == 2
    # E o NUL garante que nenhuma magic futura é prefixo desta.
    assert not b"ONYXKS12".startswith(storage.MAGIA)


def test_leitura_tolerante_aceita_todas_as_versoes_conhecidas() -> None:
    """Toda a versão em ``FORMATOS_KEYSTORE`` tem de ser legível."""
    caminho = Path(tempfile.mkdtemp()) / "qualquer.ks"
    dados = {"x": "y"}
    for versao, magia in storage.FORMATOS_KEYSTORE.items():
        sal = b"\x33" * storage.TAM_SAL
        nonce = b"\x44" * storage.TAM_NONCE
        chave = storage.derivar_chave("p", sal, 1000)
        texto = json.dumps(dados, ensure_ascii=False, sort_keys=True).encode()
        # v1 cobre só a magia; v2 e seguintes cobrem também o sal.
        aad = magia if versao == 1 else magia + sal
        conteudo = storage._cifrar_aead(chave, nonce, texto, aad)
        caminho.write_bytes(magia + sal + nonce + conteudo)
        assert (
            storage.carregar_keystore(caminho, "p", iteracoes=1000) == dados
        ), f"versão {versao} tem de ser legível"


def test_deslocar_o_cabecalho_invalida_o_ficheiro() -> None:
    """Deslocar o cabeçalho tem de invalidar o ficheiro — nunca aceitá-lo.

    Inserir um byte à frente do sal desloca todo o cabeçalho. A magic v1
    continua a bater (são os mesmos 7 bytes), mas o sal lido já não é o
    sal usado para derivar a chave, pelo que a etiqueta AEAD falha.

    O teste afirma a propriedade que importa — **recusa** — sem fixar qual
    dos dois erros é o diagnóstico, que é um detalhe da implementação.
    """
    caminho = Path(tempfile.mkdtemp()) / "v1.ks"
    _escrever_keystore_v1(caminho, {"a": 1}, "pass", 1000)
    blob = bytearray(caminho.read_bytes())
    blob[7:7] = b"\x00"  # insere um byte à frente do sal
    caminho.write_bytes(bytes(blob))

    with pytest.raises(storage.KeystoreInvalido):
        storage.carregar_keystore(caminho, "pass", iteracoes=1000)


def test_magics_v2_nao_tem_prefixo_uns_das_outras() -> None:
    """De v2 em diante, nenhuma magic é prefixo de outra.

    O terminador NUL garante isto: ``ONYXKS\x00<n>`` tem sempre um NUL no
    índice 6, e um NUL não pode ser o primeiro byte de outra magic. É a
    razão de ser do terminador — sem ele, a v10 colidiria com a v1.
    """
    for _, a in storage.FORMATOS_KEYSTORE.items():
        for _, b in storage.FORMATOS_KEYSTORE.items():
            if a != b:
                assert not b.startswith(a), f"{b!r} tem {a!r} como prefixo"


def test_versao_12_do_esquema_antigo_e_tratada_como_v1() -> None:
    """Limitação declarada: ``ONYXKS12`` é irremediavelmente ambíguo.

    A magic v1 tem 7 bytes e ``ONYXKS12`` começa por ``ONYXKS1`` — a
    informação não está no ficheiro, e nenhuma implementação o resolve.
    É por isso que v2 adoptou o esquema ``ONYXKS\x00<n>``: ninguém escreve
    ``ONYXKS12`` a partir de v2.

    Este teste fixa o **comportamento real** e a sua natureza: um erro de
    diagnosticabilidade (``PassphraseErrada`` em vez de «formato
    desconhecido»), **não** de segurança. Um ficheiro assim não pode ser
    aceite, porque a etiqueta AEAD não verifica contra um AAD diferente.
    """
    caminho = Path(tempfile.mkdtemp()) / "esquema_antigo.ks"
    caminho.write_bytes(storage.PREFIXO_KEYSTORE + b"12" + bytes(64))

    # Não é aceite — que é o que importa.
    with pytest.raises(storage.PassphraseErrada):
        storage.carregar_keystore(caminho, "p", iteracoes=1000)


def test_ficheiro_com_o_esquema_novo_e_sempre_desambiguado() -> None:
    """``ONYXKS\x00<n>`` para ``n`` desconhecido ⇒ formato desconhecido.

    Ao contrário da v1, aqui não há ambiguidade: o NUL no índice 6
    impossibilita qualquer colisão. Este é o caminho que uma versão futura
    tem, e é o que tem de dar o diagnóstico certo.
    """
    caminho = Path(tempfile.mkdtemp()) / "futuro.ks"
    caminho.write_bytes(storage._magica_versao(99) + bytes(64))
    with pytest.raises(storage.KeystoreInvalido, match="formato desconhecido"):
        storage.carregar_keystore(caminho, "p", iteracoes=1000)


def test_trocar_o_sal_num_v2_falha() -> None:
    """O v2 inclui o sal no AAD: trocar o sal tem de invalidar a etiqueta.

    É a melhoria de integridade que justifica o bump — sem ela, o sal
    ficava coberto apenas por via indirecta (a chave derivada).
    """
    caminho = Path(tempfile.mkdtemp()) / "v2.ks"
    storage.guardar_keystore({"a": 1}, caminho, "pass", iteracoes=1000)
    blob = bytearray(caminho.read_bytes())
    off = len(storage.MAGIA)
    blob[off] ^= 0xFF  # adulterar o primeiro byte do sal
    caminho.write_bytes(bytes(blob))

    with pytest.raises(storage.PassphraseErrada):
        storage.carregar_keystore(caminho, "pass", iteracoes=1000)


def test_versao_desconhecida_diz_quais_sao_validas() -> None:
    """A mensagem de erro tem de listar as magics aceites."""
    caminho = Path(tempfile.mkdtemp()) / "x.ks"
    caminho.write_bytes(b"NAOTEMMAG" + bytes(64))
    with pytest.raises(storage.KeystoreInvalido) as info:
        storage.carregar_keystore(caminho, "p", iteracoes=1000)
    mensagem = str(info.value)
    for _, magia in storage.FORMATOS_KEYSTORE.items():
        assert magia.decode() in mensagem, f"tem de listar {magia}: {mensagem}"


def test_migrar_v1_para_v2_e_transparente(tmp_path: Path) -> None:
    """O caminho de migração é carregar e gravar — sem passos manuais.

    É o que o utilizador faz uma vez: abrir com a versão nova (que lê v1)
    e gravar de volta (que escreve v2). Nenhum passo intermediário.
    """
    caminho = tmp_path / "migra.ks"
    _escrever_keystore_v1(caminho, {"segredo": "valor"}, "pass", 1000)
    assert caminho.read_bytes().startswith(b"ONYXKS1")

    dados = storage.carregar_keystore(caminho, "pass", iteracoes=1000)
    storage.guardar_keystore(dados, caminho, "pass", iteracoes=1000)

    assert caminho.read_bytes().startswith(storage.MAGIA)
    assert storage.carregar_keystore(caminho, "pass", iteracoes=1000) == {
        "segredo": "valor"
    }


def test_magica_de_versao_fora_do_intervalo_e_recusada() -> None:
    """`_magica_versao` só aceita 2..255.

    A v1 tem magic própria e não passa por aqui; abaixo de 2 não há
    formato; acima de 255 o byte da versão não cabe. Falhar aqui, na
    construção da tabela, é melhor do que gerar uma magic que nunca vai
    ser reconhecida.
    """
    for impossivel in (0, 1, 256, -1):
        with pytest.raises(ValueError, match="fora do intervalo"):
            storage._magica_versao(impossivel)


def test_magica_de_versao_valida_tem_o_terminador() -> None:
    """Toda a magic v2+ tem ``PREFIXO`` ‖ NUL ‖ versão — 8 bytes exactos."""
    for versao in (2, 3, 10, 99, 255):
        magia = storage._magica_versao(versao)
        assert len(magia) == 8, f"versão {versao}: {magia!r}"
        assert magia.startswith(storage.PREFIXO_KEYSTORE)
        assert magia[6] == 0, "o índice 6 tem de ser o terminador NUL"
        assert magia[7] == versao
