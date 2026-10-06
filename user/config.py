# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""config.py — gestão da identidade local e das preferências do OnyxChat.

A configuração fica num ficheiro JSON **0600** (só o utilizador do
daemon o lê), com:

* a *seed* Ed25519 da identidade local (a pública deriva-se da seed —
  nunca se guarda a seed em claro em memória partilhada nem em logs);
* as chaves próprias ``K5`` (remetente) e ``K9`` (receptor), geradas no
  ``iniciar`` e trocadas depois no handshake de amizade;
* preferências: socket IPC, pasta de dados e TTL do discovery.

Caminho por omissão: ``$ONYXCHAT_CONFIG`` ou
``~/.config/onyxchat/config.json``.

Formato
-------

O ficheiro tem **dois formatos**, e a escolha é do utilizador:

``v1`` (legado)
    JSON simples com a seed e as chaves em hex. É o que as versões
    anteriores escreviam, e continua a ser lido sem alteração.

``v2`` (actual, opcional)
    JSON simples **sem segredos**, mais um keystore cifrado ao lado
    (``config.keystore``) com PBKDF2-HMAC-SHA256 + ChaCha20-Poly1305.
    A seed e as chaves só existem lá dentro, cifradas.

Porquê a mudança
    O modo 0600 cobre outro utilizador da máquina, que é a ameaça que a
    ``threat_model.md`` chama A8. **Não** cobre um backup do ``$HOME``,
    um ``.tar`` do utilizador, ou sincronização de cloud — e a seed
    Ed25519 *é* a identidade: quem a tem lê tudo o que essa identidade
    enviou. ``key_management.md`` já prometia «o cliente guarda-as no
    keystore cifrado» desde a primeira versão; o código promises estava
    escrito e testado, e nenhum módulo o usava.

A passphrase não é guardada em lado nenhum, e não há como a recuperar:
perdê-la significa perder a identidade. É o preço de um ficheiro que
não pode ser lido por quem o copiou.
"""

from __future__ import annotations

import json
import os
import secrets
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from messenger.keys import TAMANHO_CHAVE, Identidade

#: Versão do formato do ficheiro — mudanças incompatíveis bumpam isto.
#:
#: A v1 é o JSON simples com a seed em hex. A v2 é o mesmo JSON **sem
#: segredos**, com o keystore cifrado num ficheiro ao lado.
VERSAO_CONFIG = 1

#: Versão do formato **cifrado**. Não é a mesma coisa que
#: :data:`VERSAO_CONFIG`: aquele versiona o JSON de preferências, este
#: versiona o keystore, que tem a sua própria mágica (``ONYXKS\0<n>``).
VERSAO_CONFIG_CIFRADA = 2

#: Chave de ``config.json`` que aponta para o keystore cifrado.
#:
#: Nome curto porque é uma chave de um ficheiro que o utilizador lê
#: quando algo corre mal; o valor é um caminho **relativo** ao
#: directório da configuração, para que mover ``.config/onyxchat`` para
#: um disco externo não quebre o par.
CHAVE_KEYSTORE = "keystore"


#: TTL por omissão das entradas do discovery server (segundos).
TTL_DESCOBERTA = 300


class ConfigInvalida(ValueError):
    """Ficheiro de configuração ausente, malformado ou incoerente."""


class PassphraseNecessaria(ConfigInvalida):
    """A configuração está cifrada e nenhuma passphrase foi dada.

    Separada de :class:`ConfigInvalida` porque o que se deve fazer a
    seguir é diferente: pedir a passphrase, não dizer ao utilizador que
    o ficheiro está partido. A CLI distingue-as e presents um prompt.

    Transporta o caminho do keystore, para o erro dizer **que** ficheiro
    precisa de passphrase e não apenas que «falta uma».
    """

    def __init__(self, keystore: str) -> None:
        super().__init__(f"configuração cifrada; passphrase necessária para {keystore}")
        self.keystore = keystore


def caminho_config() -> Path:
    """Caminho do ficheiro: ``$ONYXCHAT_CONFIG`` ou o padrão do utilizador."""
    caminho = os.environ.get("ONYXCHAT_CONFIG")
    if caminho:
        return Path(caminho)
    return Path.home() / ".config" / "onyxchat" / "config.json"


@dataclass(frozen=True)
class Config:
    """Estado local completo: identidade, chaves próprias e preferências."""

    identidade: Identidade
    k5: bytes
    k9: bytes
    socket: str | None = None
    pasta_dados: Path = field(
        default_factory=lambda: Path.home() / ".local" / "share" / "onyxchat"
    )
    ttl_descoberta: int = TTL_DESCOBERTA
    #: Keystore cifrado, ou ``None`` se a configuração ainda está em
    #: formato v1 (seed em hex claro no próprio JSON).
    #:
    #: ``None`` não é um estado temporário — é um formato legado que
    #: qualquer versão do programa sabe ler. Quem tiver uma v1 continua
    #: a funcionar; o que mantém é a promessa de que nada se perde.
    keystore: Path | None = None

    def __post_init__(self) -> None:
        if len(self.k5) != TAMANHO_CHAVE or len(self.k9) != TAMANHO_CHAVE:
            raise ConfigInvalida(
                f"K5/K9 têm de ter {TAMANHO_CHAVE} bytes"
            )
        if self.ttl_descoberta <= 0:
            raise ConfigInvalida("o TTL do discovery tem de ser positivo")

    @property
    def pub(self) -> bytes:
        """Chave pública da identidade local (derivada da seed)."""
        return self.identidade.pub

    def para_json(self) -> dict[str, object]:
        """Representação serializável.

        Em formato v2 (com :attr:`keystore`), **não** inclui a seed nem
        as chaves: elas vivem no keystore cifrado. A função decide
        sozinha, a partir da presença do campo — não há modo de escrever
        segredos em claro numa configuração que diz estar cifrada.
        """
        comum: dict[str, object] = {
            "versao": VERSAO_CONFIG,
            "socket": self.socket,
            "pasta_dados": str(self.pasta_dados),
            "ttl_descoberta": self.ttl_descoberta,
        }
        if self.keystore is None:
            # Formato v1: hex plano, tal como as versões anteriores.
            # Um literal aqui, e não uma constante, porque o valor é
            # exactamente o que a v1 escrevia.
            comum["versao"] = 1
            comum["seed"] = self.identidade.seed.hex()
            comum["k5"] = self.k5.hex()
            comum["k9"] = self.k9.hex()
            return comum

        comum["versao"] = VERSAO_CONFIG_CIFRADA
        # Relativo ao directório da configuração, para que mover a pasta
        # para outro disco não quebre o par.
        comum[CHAVE_KEYSTORE] = self.keystore.name
        return comum


def guardar(config: Config, caminho: Path | None = None) -> Path:
    """Grava ``config`` em JSON (modo 0600, com criação das diretórias).

    Quando :attr:`Config.keystore` está definido, o JSON **não** recebe a
    seed nem as chaves — o texto vai para o keystore cifrado e o JSON fica
    só com preferências e a referência ao ficheiro irmão.

    A escrita do JSON é atómica: escreve-se num ficheiro temporário no
    mesmo directório e faz-se ``os.replace``. Sem isso, um corte de
    energia a meio deixa um ``config.json`` truncado, e a identidade
    fica perdida — o keystore cifrado está intacto, mas o JSON que o
    aponta também não pode.
    """
    destino = Path(caminho) if caminho is not None else caminho_config()
    destino.parent.mkdir(parents=True, exist_ok=True)
    texto = json.dumps(config.para_json(), ensure_ascii=False, indent=2)

    # Temporário no mesmo directório: `os.replace` só é atómico dentro
    # do mesmo sistema de ficheiros, e um temporário em `/tmp` pode estar
    # noutro.
    fd, temporario = tempfile.mkstemp(dir=destino.parent, prefix=".config-", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as ficheiro:
            ficheiro.write(texto)
            # Antes do `replace`: o ficheiro final nunca existe com
            # permissões abertas, nem que seja por um instante.
            os.fchmod(ficheiro.fileno(), 0o600)
        os.replace(temporario, destino)
    except BaseException:
        # Um `KeyError` ao meio deixaria o temporário para sempre, com a
        # seed em claro dentro se o config for v1.
        try:
            os.unlink(temporario)
        except OSError:
            pass
        raise

    # O `os.replace` substitui o conteúdo mas mantém as permissões do
    # ficheiro **de destino** quando este já existia. Se um utilizador
    # tivesse um `config.json` com outro modo, e o próprio utilizador o
    # tivesse posto assim, reaffirmar 0600 é o comportamento pretendido:
    # o ficheiro guarda a identidade.
    os.chmod(destino, 0o600)
    return destino


def caminho_keystore(caminho_config: Path | str) -> Path:
    """O keystore cifrado que acompanha ``caminho_config``.

    Fica ao lado do JSON, com o mesmo nome e sufixo diferente: o par é
    óbvio numa listagem de directório, e não há um caminho gravado em
    lado nenhum que possa divergir do ficheiro real.

    Aceita `str` como o resto da API, que usa `Path | None`.
    """
    return Path(caminho_config).with_suffix(".keystore")


def criar(
    caminho: Path | None = None,
    *,
    forcar: bool = False,
    socket: str | None = None,
    pasta_dados: Path | None = None,
    ttl_descoberta: int = TTL_DESCOBERTA,
    passphrase: str | None = None,
) -> Config:
    """Cria uma configuração nova (identidade + K5/K9 próprias).

    Se ``caminho`` já existir e ``forcar`` seja ``False``, levanta
    :class:`ConfigInvalida` — nunca substitui identidades silenciosamente.

    Com ``passphrase``, a seed e as chaves são escritas num keystore
    cifrado ao lado do JSON, e o JSON fica sem segredos. Sem
    ``passphrase``, mantém-se o formato legado (v1), com a seed em hex
    claro no próprio ficheiro.

    A escolha é do utilizador e é irreversível sem recriar a identidade:
    migrar de v1 para v2 é possível (ver :func:`migrar_para_cifrado`),
    o contrário exigiria recuar a versão do ficheiro.
    """
    destino = Path(caminho) if caminho is not None else caminho_config()
    if destino.exists() and not forcar:
        raise ConfigInvalida(f"configuração já existe: {destino}")

    identidade = Identidade.gerar()
    k5 = secrets.token_bytes(TAMANHO_CHAVE)
    k9 = secrets.token_bytes(TAMANHO_CHAVE)

    config = Config(
        identidade=identidade,
        k5=k5,
        k9=k9,
        socket=socket,
        **({"pasta_dados": pasta_dados} if pasta_dados is not None else {}),
        ttl_descoberta=ttl_descoberta,
        keystore=caminho_keystore(destino) if passphrase else None,
    )

    if passphrase is not None:
        _escrever_keystore(config, destino, passphrase)
    guardar(config, destino)
    return config


def _escrever_keystore(config: Config, destino: Path, passphrase: str) -> None:
    """Cifra os segredos de ``config`` para o keystore ao lado dele.

    Função interna: os chamadores externos usam :func:`criar` ou
    :func:`migrar_para_cifrado`, que tratam da ordem das escritas.
    """
    from messenger import storage

    destino.parent.mkdir(parents=True, exist_ok=True)
    storage.guardar_keystore(
        {
            "seed": config.identidade.seed.hex(),
            "k5": config.k5.hex(),
            "k9": config.k9.hex(),
        },
        caminho_keystore(destino),
        passphrase,
    )


def _de_json(dados: object) -> Config:
    """Despacha um JSON já lido para o formato que ele declara.

    A assinatura é ``object`` e não ``dict`` porque este é o ponto de
    entrada do JSON da configuração, e um ficheiro de configuração é um
    ficheiro que o utilizador pode editar à mão: validar a forma aqui
    evita que cada chamador tenha de a repetir.

    O despacho pela ``versao`` é o que permite que uma v1 continue a
    abrir depois de a v2 existir. Um ``versao`` desconhecida é recusada —
    ler um formato que não se conhece é ler o que não se entende.
    """
    if not isinstance(dados, dict):
        raise ConfigInvalida("configuração não é um objeto JSON")
    versao = dados.get("versao")
    if versao == 1:
        return _de_json_v1(dados)
    if versao == VERSAO_CONFIG_CIFRADA:
        # Só a identidade e as chaves estão cifradas; as preferências
        # estão no JSON. Mas o keystore precisa de uma passphrase, que
        # esta função não tem — por isso devolve a `Config` com as
        # preferências e **sem** segredos, e o chamador tem de usar
        # `carregar` para ter a identidade completa.
        #
        # Devolver meia configuração seria um erro silencioso: um
        # `criar` ou `guardar` a seguir escreveria a seed de volta em
        # claro. Falhar é mais seguro que devolver incompleto.
        raise PassphraseNecessaria("configuração cifrada")
    raise ConfigInvalida(f"versão de configuração desconhecida: {versao!r}")


def _preferencias(dados: dict[str, object]) -> tuple[str | None, Path, int]:
    """Extrai socket, pasta de dados e TTL de um JSON de configuração.

    Separado do parsing dos segredos porque é a única parte que os dois
    formatos têm em comum, e duplicar essa extracção era o caminho mais
    curto para os dois divergirem.
    """
    socket = dados["socket"]
    pasta = Path(dados["pasta_dados"])
    ttl = int(dados["ttl_descoberta"])
    if socket is not None and not isinstance(socket, str):
        raise ConfigInvalida("o socket tem de ser caminho ou null")
    return socket, pasta, ttl


def _de_json_v1(dados: dict[str, object]) -> Config:
    """Formato legado: seed e chaves em hex no próprio JSON."""
    try:
        seed = bytes.fromhex(dados["seed"])
        k5 = bytes.fromhex(dados["k5"])
        k9 = bytes.fromhex(dados["k9"])
        socket, pasta, ttl = _preferencias(dados)
    except (KeyError, ValueError, TypeError) as erro:
        raise ConfigInvalida(f"campos em falta ou inválidos: {erro}") from erro
    try:
        identidade = Identidade(seed)
    except ValueError as erro:
        # Seed com tamanho errado (hex válido mas não-32-bytes).
        raise ConfigInvalida(f"seed inválida: {erro}") from erro
    return Config(
        identidade=identidade,
        k5=k5,
        k9=k9,
        socket=socket,
        pasta_dados=pasta,
        ttl_descoberta=ttl,
    )


def _localizar_keystore(dados: dict[str, object], origem: Path) -> Path:
    """O caminho do keystore a partir do JSON v2, validado.

    O nome é relativo ao directório da configuração, e resolve-se
    **contra** `origem.parent` e não contra o directório de trabalho:
    um JSON deve abrir onde quer que esteja, e `Path(nome)` sozinho
   resolveria contra o `cwd`, o que faria a mesma configuração abrir em
    máquinas diferentes.
    """
    nome = dados.get(CHAVE_KEYSTORE)
    if not isinstance(nome, str) or not nome:
        raise ConfigInvalida("configuração em formato v2 sem chave de keystore")
    # Um nome com `..` escaparia do directório da configuração — e o
    # JSON é um ficheiro que o utilizador pode editar à mão.
    if Path(nome).is_absolute() or ".." in Path(nome).parts:
        raise ConfigInvalida(f"caminho de keystore inválido: {nome}")
    keystore = origem.parent / nome
    if not keystore.exists():
        raise ConfigInvalida(f"keystore não encontrado: {keystore}")
    return keystore


def _abrir_cifrado(
    dados: dict[str, object],
    origem: Path,
    keystore: Path,
    passphrase: str | None = None,
) -> Config:
    """Abre o keystore e reconstrói a :class:`Config`.

    Com ``passphrase=None`` (o caminho de :func:`carregar`) não há
    como abrir, e a excepção é explícita sobre o que falta.
    """
    from messenger import storage

    if passphrase is None:
        raise PassphraseNecessaria(str(keystore))

    segredos = storage.carregar_keystore(keystore, passphrase)
    try:
        seed = bytes.fromhex(segredos["seed"])
        k5 = bytes.fromhex(segredos["k5"])
        k9 = bytes.fromhex(segredos["k9"])
        socket, pasta, ttl = _preferencias(dados)
    except (KeyError, ValueError, TypeError) as erro:
        raise ConfigInvalida(f"keystore malformado: {erro}") from erro

    try:
        identidade = Identidade(seed)
    except ValueError as erro:
        raise ConfigInvalida(f"seed inválida no keystore: {erro}") from erro

    return Config(
        identidade=identidade,
        k5=k5,
        k9=k9,
        socket=socket,
        pasta_dados=pasta,
        ttl_descoberta=ttl,
        keystore=keystore,
    )


def carregar(
    caminho: Path | None = None, passphrase: str | None = None
) -> Config:
    """Lê a configuração de qualquer um dos dois formatos.

    ``passphrase`` só é necessária (e só é usada) quando a configuração
    está em formato v2, cifrado. Sem ela, uma configuração v2 levanta
    :class:`PassphraseNecessaria` — que a CLI transforma num prompt, e
    não num erro.

    Erros de leitura ou de formato viram :class:`ConfigInvalida`; uma
    passphrase errada no keystore propaga a
    :class:`~messenger.storage.PassphraseErrada` do módulo de storage,
    que é mais específica e mais útil.
    """
    origem = Path(caminho) if caminho is not None else caminho_config()
    try:
        texto = origem.read_text(encoding="utf-8")
    except OSError as erro:
        raise ConfigInvalida(f"configuração ilegível: {origem}") from erro
    try:
        dados = json.loads(texto)
    except json.JSONDecodeError as erro:
        raise ConfigInvalida("configuração não é JSON válido") from erro
    if not isinstance(dados, dict):
        raise ConfigInvalida("configuração não é um objeto JSON")

    versao = dados.get("versao")
    if versao == 1:
        return _de_json_v1(dados)
    if versao == VERSAO_CONFIG_CIFRADA:
        keystore = _localizar_keystore(dados, origem)
        return _abrir_cifrado(dados, origem, keystore, passphrase)
    raise ConfigInvalida(f"versão de configuração desconhecida: {versao!r}")


def migrar_para_cifrado(
    passphrase: str, caminho: Path | None = None
) -> Path:
    """Reescreve uma configuração v1 em v2, com os segredos cifrados.

    A operação é **irreversível**: depois de migrar, a identidade só é
    legível com a passphrase. Por isso grava o keystore *antes* de
    reescrever o JSON — se a escrita do JSON falhar, o par antigo
    continua a abrir e o keystore novo é um ficheiro órfão que o
    próximo `criar --forcar` substituirá.

    Devolve o caminho do keystore, para a CLI poder dizer onde ficou.

    Levanta :class:`ConfigInvalida` se a configuração já estiver
    cifrada: migrar duas vezes não faria nada de útil e poderia
    sobrescrever um keystore bom com a mesma seed noutro formato.
    """
    origem = Path(caminho) if caminho is not None else caminho_config()
    if not origem.exists():
        raise ConfigInvalida(f"configuração inexistente: {origem}")

    dados = json.loads(origem.read_text(encoding="utf-8"))
    if not isinstance(dados, dict):
        raise ConfigInvalida("configuração não é um objeto JSON")
    if dados.get("versao") != 1:
        raise ConfigInvalida(
            f"a configuração já está cifrada (versão {dados.get('versao')!r})"
        )

    config = _de_json_v1(dados)
    keystore = caminho_keystore(origem)
    _escrever_keystore(config, origem, passphrase)

    cifrada = Config(
        identidade=config.identidade,
        k5=config.k5,
        k9=config.k9,
        socket=config.socket,
        pasta_dados=config.pasta_dados,
        ttl_descoberta=config.ttl_descoberta,
        keystore=keystore,
    )
    guardar(cifrada, origem)
    return keystore
