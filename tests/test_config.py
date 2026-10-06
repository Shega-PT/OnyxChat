# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_config.py — configuração local (user.config).

Cobre os dois formatos: v1 (legado, seed em hex no JSON) e v2
(keystore cifrado com PBKDF2 + ChaCha20-Poly1305 ao lado do JSON).

Cobre o caminho por omissão/env, a criação com protecção contra
substituição silenciosa, permissões 0600 e todas as validações
tipadas da leitura (`ConfigInvalida`).
"""

from __future__ import annotations

import json
import os
import stat
from pathlib import Path

import pytest

from messenger import storage
from messenger.keys import TAMANHO_CHAVE, Identidade
from messenger.storage import PassphraseErrada
from user import config


def test_caminho_config_env_e_omissao(monkeypatch, tmp_path: Path) -> None:
    """``$ONYXCHAT_CONFIG`` tem prioridade; sem ela, ``~/.config/…``."""
    alvo = tmp_path / "conf.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(alvo))
    assert config.caminho_config() == alvo
    monkeypatch.delenv("ONYXCHAT_CONFIG", raising=False)
    assert config.caminho_config() == (
        Path.home() / ".config" / "onyxchat" / "config.json"
    )


def test_criar_guardar_carregar_roundtrip(monkeypatch, tmp_path: Path) -> None:
    """``criar`` grava 0600 e ``carregar`` recupera tudo (caminho default)."""
    alvo = tmp_path / "nova" / "conf.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(alvo))
    criada = config.criar()
    assert alvo.is_file()
    assert stat.S_IMODE(alvo.stat().st_mode) == 0o600
    volta = config.carregar()
    assert volta.identidade.seed == criada.identidade.seed
    assert volta.k5 == criada.k5 and volta.k9 == criada.k9
    assert volta.para_json() == criada.para_json()
    assert volta.pub == criada.identidade.pub


def test_criar_existente_sem_forcar_levanta(tmp_path: Path) -> None:
    """Nunca substitui uma identidade existente sem ``forcar``."""
    caminho = tmp_path / "conf.json"
    primeira = config.criar(caminho)
    with pytest.raises(config.ConfigInvalida, match="já existe"):
        config.criar(caminho)
    segunda = config.criar(caminho, forcar=True)
    assert segunda.identidade.seed != primeira.identidade.seed  # substituiu


def test_criar_parametros_expostos(tmp_path: Path) -> None:
    """Socket, pasta de dados e TTL escolhidos ficam guardados."""
    pasta = tmp_path / "dados"
    cfg = config.criar(
        tmp_path / "c.json",
        socket="/tmp/meu.sock",
        pasta_dados=pasta,
        ttl_descoberta=42,
    )
    volta = config.carregar(tmp_path / "c.json")
    assert cfg.socket == "/tmp/meu.sock"
    assert cfg.pasta_dados == pasta and cfg.ttl_descoberta == 42
    assert volta.socket == "/tmp/meu.sock"
    assert volta.pasta_dados == pasta and volta.ttl_descoberta == 42


def test_criar_defaults_e_gravar_default(monkeypatch, tmp_path: Path) -> None:
    """Sem socket nem pasta explícitos, usa as predefinições; ``guardar`` sem caminho."""
    cfg = config.criar(tmp_path / "c.json")
    assert cfg.socket is None
    assert cfg.pasta_dados == Path.home() / ".local" / "share" / "onyxchat"
    assert cfg.ttl_descoberta == config.TTL_DESCOBERTA
    # ``guardar`` sem caminho usa ``caminho_config`` (via env).
    alvo = tmp_path / "via-guardar.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(alvo))
    destino = config.guardar(cfg)
    assert destino == alvo and alvo.is_file()


def test_config_dataclass_valida_tudo() -> None:
    """K5/K9 errados e TTL ≤ 0 → ``ConfigInvalida``; ``pub`` é derivada."""
    identidade = Identidade.gerar()
    valido = config.Config(identidade=identidade, k5=b"\x01" * 32, k9=b"\x02" * 32)
    assert valido.pub == identidade.pub
    with pytest.raises(config.ConfigInvalida, match="K5/K9"):
        config.Config(identidade=identidade, k5=b"curto", k9=b"\x02" * 32)
    with pytest.raises(config.ConfigInvalida, match="K5/K9"):
        config.Config(
            identidade=identidade, k5=b"\x01" * 32, k9=b"\x02" * TAMANHO_CHAVE + b"x"
        )
    with pytest.raises(config.ConfigInvalida, match="TTL"):
        config.Config(
            identidade=identidade,
            k5=b"\x01" * 32,
            k9=b"\x02" * 32,
            ttl_descoberta=0,
        )


def test_de_json_rejeita_versao_e_nao_dicionario() -> None:
    """Lista, versão errada ou versão cifrada sem passphrase → ``ConfigInvalida``.

    O formato v2 (cifrado) sem passphrase dá ``PassphraseNecessaria``,
    que é uma subclasse de ``ConfigInvalida`` e por isso ainda entra
    neste ``pytest.raises``. A distinção importa para a CLI, que
    transforma uma em prompt e a outra em mensagem de erro.
    """
    with pytest.raises(config.ConfigInvalida, match="objeto JSON"):
        config._de_json([1, 2, 3])
    with pytest.raises(config.ConfigInvalida, match="vers"):
        config._de_json({"versao": 99})
    with pytest.raises(config.PassphraseNecessaria):
        config._de_json({"versao": config.VERSAO_CONFIG_CIFRADA, "keystore": "x"})


def test_de_json_rejeita_campos_invalidos() -> None:
    """Hex inválido, campo em falta e tipo errado → ``ConfigInvalida``."""
    base = {
        "versao": 1,
        "seed": "ab" * 32,
        "k5": "cd" * 32,
        "k9": "ef" * 32,
        "socket": None,
        "pasta_dados": "/tmp/d",
        "ttl_descoberta": 300,
    }
    com_hex_mau = dict(base, seed="zz")
    with pytest.raises(config.ConfigInvalida, match="campos"):
        config._de_json(com_hex_mau)
    sem_campo = {k: v for k, v in base.items() if k != "k5"}
    with pytest.raises(config.ConfigInvalida, match="campos"):
        config._de_json(sem_campo)
    tipo_mau = dict(base, ttl_descoberta=None)  # int(None) → TypeError
    with pytest.raises(config.ConfigInvalida, match="campos"):
        config._de_json(tipo_mau)


def test_de_json_rejeita_socket_nao_string() -> None:
    """``socket`` não-str e não-nulo → ``ConfigInvalida``."""
    dados = {
        "versao": 1,
        "seed": "ab" * 32,
        "k5": "cd" * 32,
        "k9": "ef" * 32,
        "socket": 123,
        "pasta_dados": "/tmp/d",
        "ttl_descoberta": 300,
    }
    with pytest.raises(config.ConfigInvalida, match="socket"):
        config._de_json(dados)


def test_de_json_rejeita_seed_tamanho_errado() -> None:
    """Hex válido mas ≠ 32 bytes → ``ConfigInvalida`` na identidade."""
    dados = {
        "versao": 1,
        "seed": "ab" * 16,  # 16 bytes
        "k5": "cd" * 32,
        "k9": "ef" * 32,
        "socket": None,
        "pasta_dados": "/tmp/d",
        "ttl_descoberta": 300,
    }
    with pytest.raises(config.ConfigInvalida, match="seed"):
        config._de_json(dados)


def test_de_json_ttl_zero_rejeitado() -> None:
    """TTL 0 passa o parse mas falha na validação do dataclass."""
    dados = {
        "versao": 1,
        "seed": "ab" * 32,
        "k5": "cd" * 32,
        "k9": "ef" * 32,
        "socket": None,
        "pasta_dados": "/tmp/d",
        "ttl_descoberta": 0,
    }
    with pytest.raises(config.ConfigInvalida, match="TTL"):
        config._de_json(dados)


def test_de_json_happy_path() -> None:
    """Conversão completa devolve ``Config`` coerente."""
    identidade = Identidade.gerar()
    dados = {
        "versao": config.VERSAO_CONFIG,
        "seed": identidade.seed.hex(),
        "k5": bytes(range(32)).hex(),
        "k9": bytes(range(32, 64)).hex(),
        "socket": "/tmp/x.sock",
        "pasta_dados": "/tmp/pasta",
        "ttl_descoberta": "60",  # string numérica também é aceite
    }
    cfg = config._de_json(dados)
    assert cfg.identidade.seed == identidade.seed
    assert cfg.k5 == bytes(range(32))
    assert cfg.socket == "/tmp/x.sock"
    assert cfg.ttl_descoberta == 60


def test_carregar_ficheiro_invalido(tmp_path: Path) -> None:
    """Ficheiro ausente e JSON inválido → ``ConfigInvalida``."""
    with pytest.raises(config.ConfigInvalida, match="ilegível"):
        config.carregar(tmp_path / "nao-existe.json")
    mau = tmp_path / "mau.json"
    mau.write_text("{nao é json", encoding="utf-8")
    with pytest.raises(config.ConfigInvalida, match="JSON válido"):
        config.carregar(mau)
    json_errado = tmp_path / "tipo.json"
    json_errado.write_text(json.dumps(["lista"]), encoding="utf-8")
    with pytest.raises(config.ConfigInvalida, match="objeto JSON"):
        config.carregar(json_errado)
    versao_97 = tmp_path / "v97.json"
    versao_97.write_text(json.dumps({"versao": 97}), encoding="utf-8")
    with pytest.raises(config.ConfigInvalida, match="vers"):
        config.carregar(versao_97)


# ---------------------------------------------------------------------
# Formato cifrado (v2)
# ---------------------------------------------------------------------


def test_criar_v2_nao_mete_segredos_no_json(tmp_path: Path) -> None:
    """``criar(passphrase=…)`` escreve a seed fora do JSON.

    É a propriedade que justifica o formato: o ``config.json`` de um
    utilizador que escolheu cifrar não pode conter a seed, nem em hex.
    """
    destino = tmp_path / "config.json"
    cfg = config.criar(destino, passphrase="uma passphrase qualquer")

    lido = json.loads(destino.read_text(encoding="utf-8"))
    assert lido["versao"] == config.VERSAO_CONFIG_CIFRADA
    assert "seed" not in lido
    assert "k5" not in lido
    assert "k9" not in lido
    assert lido[config.CHAVE_KEYSTORE] == config.caminho_keystore(destino).name

    assert cfg.keystore == config.caminho_keystore(destino)
    assert cfg.keystore.exists()
    assert cfg.keystore.stat().st_mode & 0o777 == 0o600
    assert destino.stat().st_mode & 0o777 == 0o600

    # E a identidade abre com a passphrase.
    aberta = config.carregar(destino, "uma passphrase qualquer")
    assert aberta.pub == cfg.pub
    assert aberta.k5 == cfg.k5
    assert aberta.k9 == cfg.k9


def test_migrar_v1_para_v2_mantem_a_identidade(tmp_path: Path) -> None:
    """Migrar reescreve o JSON mas não toca na identidade."""
    destino = tmp_path / "config.json"
    original = config.criar(destino)
    assert "seed" in json.loads(destino.read_text(encoding="utf-8"))

    keystore = config.migrar_para_cifrado("pass", destino)

    lido = json.loads(destino.read_text(encoding="utf-8"))
    assert lido["versao"] == config.VERSAO_CONFIG_CIFRADA
    assert "seed" not in lido

    migrada = config.carregar(destino, "pass")
    assert migrada.pub == original.pub, "migrar não pode recriar a identidade"
    assert migrada.k5 == original.k5
    assert migrada.k9 == original.k9
    assert migrada.socket == original.socket
    assert migrada.pasta_dados == original.pasta_dados
    assert migrada.ttl_descoberta == original.ttl_descoberta
    assert migrada.keystore == keystore


def test_migrar_config_ja_cifrada_e_recusado(tmp_path: Path) -> None:
    """Migrar duas vezes não pode sobrescrever um keystore bom."""
    destino = tmp_path / "config.json"
    config.criar(destino, passphrase="a")
    with pytest.raises(config.ConfigInvalida, match="já está cifrada"):
        config.migrar_para_cifrado("b", destino)


def test_migrar_config_inexistente(tmp_path: Path) -> None:
    """Migrar sem configuração legível → ``ConfigInvalida``."""
    with pytest.raises(config.ConfigInvalida, match="inexistente"):
        config.migrar_para_cifrado("pass", tmp_path / "nao-existe.json")


def test_carregar_v2_sem_passphrase_pede_passphrase(tmp_path: Path) -> None:
    """Sem passphrase, o erro diz **que** ficheiro precisa dela.

    A CLI transforma esta excepção num prompt; se a mensagem não
    dissesse qual o keystore, o utilizador teria de adivinhar.
    """
    destino = tmp_path / "config.json"
    config.criar(destino, passphrase="secreta")
    with pytest.raises(config.PassphraseNecessaria) as erro:
        config.carregar(destino)
    assert str(config.caminho_keystore(destino)) in str(erro.value)
    assert erro.value.keystore == str(config.caminho_keystore(destino))


def test_passphrase_errada_nao_e_config_invalida(tmp_path: Path) -> None:
    """Passphrase errada é erro de passphrase, não de configuração.

    A distinção é o que permite à CLI dizer «passphrase errada» em vez
    de «o ficheiro está partido» — e o utilizador que se enganou a
    digitar precisa de saber qual das duas foi.
    """
    destino = tmp_path / "config.json"
    config.criar(destino, passphrase="a certa")
    with pytest.raises(PassphraseErrada):
        config.carregar(destino, "a errada")


def test_keystore_ilegivel_durante_a_migracao_preserva_o_v1(
    tmp_path: Path,
) -> None:
    """Se o keystore não pode ser escrito, o v1 continua intacto.

    A migração grava o keystore **antes** de reescrever o JSON. Se a
    escrita do keystore falhar, o JSON continua em v1 e a identidade
    continua a abrir — o que importa, porque o utilizador já estava a
    usar aquele formato e uma migração falhada não pode deixá-lo sem
    identidade.
    """
    destino = tmp_path / "config.json"
    original = config.criar(destino)

    # Um directório no lugar do keystore faz o `os.open` falhar.
    destino.parent.joinpath("config.keystore").mkdir()
    with pytest.raises(OSError):
        config.migrar_para_cifrado("pass", destino)

    # O JSON continua em v1, legível, com a mesma identidade.
    assert json.loads(destino.read_text(encoding="utf-8"))["versao"] == 1
    assert config.carregar(destino).pub == original.pub


def test_keystore_ausente_da_erro(tmp_path: Path) -> None:
    """JSON v2 sem o ficheiro cifrado → erro, não configuração vazia."""
    destino = tmp_path / "config.json"
    config.criar(destino, passphrase="p")
    config.caminho_keystore(destino).unlink()
    with pytest.raises(config.ConfigInvalida, match="keystore não encontrado"):
        config.carregar(destino, "p")


def test_keystore_com_caminho_malicioso_e_recusado(tmp_path: Path) -> None:
    """Um ``keystore`` com ``..`` ou caminho absoluto é recusado.

    O ``config.json`` é um ficheiro que o utilizador pode editar à
    mão. Aceitar um caminho que sai do directório da configuração seria
    permitir que um ficheiro JSON apontasse para qualquer keystore do
    disco — e, pior, para um que não é keystore nenhum.
    """
    for nome in ["../outro.keystore", "/etc/passwd"]:
        destino = tmp_path / "config.json"
        destino.write_text(
            json.dumps(
                {
                    "versao": config.VERSAO_CONFIG_CIFRADA,
                    "socket": None,
                    "pasta_dados": str(tmp_path),
                    "ttl_descoberta": 300,
                    config.CHAVE_KEYSTORE: nome,
                }
            ),
            encoding="utf-8",
        )
        with pytest.raises(config.ConfigInvalida, match="inválido"):
            config.carregar(destino, "p")


def test_keystore_com_nome_vazio_e_recusado(tmp_path: Path) -> None:
    """``keystore: ""`` ou de tipo errado → ``ConfigInvalida``."""
    destino = tmp_path / "config.json"
    base = {
        "versao": config.VERSAO_CONFIG_CIFRADA,
        "socket": None,
        "pasta_dados": str(tmp_path),
        "ttl_descoberta": 300,
    }
    for valor in ["", 42, None]:
        destino.write_text(
            json.dumps({**base, config.CHAVE_KEYSTORE: valor}), encoding="utf-8"
        )
        with pytest.raises(config.ConfigInvalida, match="sem chave de keystore"):
            config.carregar(destino, "p")


def test_keystore_malformado_da_erro(tmp_path: Path) -> None:
    """Keystore que abre mas não tem os campos esperados → erro."""
    destino = tmp_path / "config.json"
    config.criar(destino, passphrase="p")
    # Substitui por um keystore cifrado válido mas sem os segredos.
    storage.guardar_keystore({"outro": "campo"}, config.caminho_keystore(destino), "p")
    with pytest.raises(config.ConfigInvalida, match="keystore malformado"):
        config.carregar(destino, "p")


def test_keystore_com_seed_invalida_da_erro(tmp_path: Path) -> None:
    """Seed de tamanho errado dentro do keystore → ``ConfigInvalida``."""
    destino = tmp_path / "config.json"
    cfg = config.criar(destino, passphrase="p")
    storage.guardar_keystore(
        {"seed": "aabb", "k5": cfg.k5.hex(), "k9": cfg.k9.hex()},
        config.caminho_keystore(destino),
        "p",
    )
    with pytest.raises(config.ConfigInvalida, match="seed inválida"):
        config.carregar(destino, "p")


def test_guardar_escreve_atomico_e_nao_deixa_temporarios(
    tmp_path: Path,
) -> None:
    """Nem um ficheiro temporário sobrevive a um ``guardar`` bem-sucedido.

    O temporário em v1 contém a seed em claro; num caso de disco seria um
    ficheiro 0600 esquecido com a identidade dentro. O nome é fixo aqui
    para o teste poder afirmar a ausência.
    """
    destino = tmp_path / "config.json"
    config.criar(destino)
    assert not list(tmp_path.glob(".config-*")), "sobrou um temporário"
    assert [p.name for p in tmp_path.iterdir()] == ["config.json"]


def test_guardar_limpa_temporario_se_a_escrita_falhar(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Se escrever o temporário falha, o destino original fica intacto.

    O ponto coberto aqui é o `os.replace`: a escrita num ``mkstemp``
    que já não é substituível porque o `os.replace` foi injectado com
    falha. É a hipótese real de «o ficheiro temporário existe mas o
    destino nunca é tocado» — e o que o teste garante é que o
    ``config.json`` do utilizador continua a ser o de antes, com a
    identidade certa.

    A falha da serialização (``json.dumps``) acontece **antes** do
    ``mkstemp`` e portanto nunca deixa um temporário; injectar aí
    testaria uma linha que não existe.
    """
    destino = tmp_path / "config.json"
    cfg = config.criar(destino)

    def falha_replace(*args: object, **kwargs: object) -> None:
        raise OSError("disco cheio")

    monkeypatch.setattr(os, "replace", falha_replace)
    with pytest.raises(OSError, match="disco cheio"):
        config.guardar(cfg, destino)
    monkeypatch.undo()

    # O temporário foi limpo e o destino original continua a abrir com a
    # identidade intacta — uma falha ao gravar não pode custar a
    # identidade.
    assert not list(tmp_path.glob(".config-*")), "o temporário ficou em disco"
    assert config.carregar(destino).pub == cfg.pub


def test_guardar_mantem_o_temporario_quando_o_unlink_falha(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A limpeza é best-effort: um `unlink` impossível não come o erro real.

    Num sistema onde o temporário não pode ser removido (permissões
    alteradas entre o `mkstemp` e o `replace`), o `OSError` do `unlink`
    não pode substituir a excepção original — o utilizador tem de ver o
    que correu mal, não «Permission denied» de uma limpeza.
    """
    destino = tmp_path / "config.json"
    cfg = config.criar(destino)

    def falha_replace(*args: object, **kwargs: object) -> None:
        raise OSError("disco cheio")

    def falha_unlink(caminho: object) -> None:
        raise PermissionError("não posso apagar")

    monkeypatch.setattr(os, "replace", falha_replace)
    monkeypatch.setattr(os, "unlink", falha_unlink)
    with pytest.raises(OSError, match="disco cheio"):
        config.guardar(cfg, destino)
    monkeypatch.undo()


def test_config_em_v1_continua_a_ser_v1_ao_gravar(tmp_path: Path) -> None:
    """Uma config lida de um v1 volta a v1 quando gravada.

    Abrir uma configuração legada e gravá-la não pode introduzir
    segredos num formato que não os tem — nem referenciar um keystore
    que não existe.
    """
    destino = tmp_path / "config.json"
    original = config.criar(destino)
    relida = config.carregar(destino)
    assert relida.keystore is None

    config.guardar(relida, destino)
    lido = json.loads(destino.read_text(encoding="utf-8"))
    assert lido["versao"] == 1
    assert "keystore" not in lido
    assert lido["seed"] == original.identidade.seed.hex()


def test_migrar_json_que_nao_e_objeto(tmp_path: Path) -> None:
    """Um v1 que nao é um objeto JSON não pode ser migrado."""
    destino = tmp_path / "config.json"
    destino.write_text(json.dumps(["lista"]), encoding="utf-8")
    with pytest.raises(config.ConfigInvalida, match="objeto JSON"):
        config.migrar_para_cifrado("pass", destino)
