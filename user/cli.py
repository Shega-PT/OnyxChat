# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""cli.py — interface de linha de comando do OnyxChat.

Subcomandos:

* ``iniciar`` — cria identidade + K5/K9 próprias e grava a config (0600);
* ``identidade`` — mostra a chave pública local (a seed nunca é impressa);
* ``enviar TEXTO`` — cifra pelo daemon (ENCODE) e imprime o envelope em hex;
* ``receber HEX`` — valida assinatura e decifra (DECODE), imprime o texto;
* ``descobrir ID`` — consulta o discovery server pelo ``.onion``;
* rede     — ``estado``, ``ouvir``, ``ligar``, ``enviar-p2p``,
  ``receber-p2p``, ``fechar``;
* handshake— ``pedir-amizade``, ``aceitar-amizade``, ``recusar-amizade``,
  ``confirmar-amizade`` (as chaves saem em ``nome=hex`` no stdout);
* ``relay`` — corre o servidor TURN em TCP (bloqueia);
* ``sidecar`` — serve a interface e a API local em ``127.0.0.1`` (bloqueia).

Erros esperados (config inválida, daemon em baixo, assinatura
rejeitada, hex malformado) saem como ``onyxchat: …`` no stderr e
código de saída 1 — ``main`` nunca levanta exceção para fora.
"""

from __future__ import annotations

import argparse
import getpass
import os
import sys
from pathlib import Path

from messenger import conta as conta_mod
from messenger import storage
from messenger.descoberta import SERVIDOR_DESCOBERTA, resolver_ou_usar
from messenger.envelope import EnvelopeInvalido
from messenger.ipc_client import (
    MODO_DIRETO,
    MODO_RELAY,
    NOMES_TOR,
    ClienteIpc,
    ErroIpc,
)
from messenger.keys import Chaves
from messenger.pipeline import AssinaturaInvalida, Pipeline
from server import relay_server
from server import sidecar as sidecar_mod
from server.discovery_server import DescobertaInvalida, consultar_descoberta
from server.relay_server import RelayErro
from user import config as config_mod
from user import ui
from user.config import ConfigInvalida, PassphraseNecessaria, carregar


def _add_argumentos_passphrase(parser: argparse.ArgumentParser) -> None:
    """Acrescenta as duas formas de fornecer a passphrase a um subcomando.

    Deliberadamente **não** há ``--passphrase TEXTO``: um argumento de
    linha de comandos é legível por qualquer utilizador da máquina em
    ``/proc/<pid>/cmdline`` (modo 0444) e fica no histórico do shell.
    A passphrase é a única coisa que mantém a identidade de quem a
    perdeu, e não pode estar em nenhum dos dois sítios.
    """
    grupo = parser.add_argument_group("passphrase (para configuração cifrada)")
    grupo.add_argument(
        "--passphrase-ficheiro",
        metavar="FICHEIRO",
        help=(
            "ficheiro cujo primeiro teor é a passphrase "
            "(evita deixá-la em argv ou no histórico do shell)"
        ),
    )
    grupo.add_argument(
        "--cifrar",
        action="store_true",
        help=(
            "pede a passphrase interactivamente quando o ficheiro não "
            "fornece uma (iniciar) ou quando já existe uma cifrada "
            "(identidade e os restantes subcomandos)"
        ),
    )


def _pedir_passphrase(confirmar: bool = False) -> str:
    """Pede a passphrase sem a repetir no ecrã.

    ``getpass`` desliga o eco do terminal. Repetir o valor na
    confirmação é o que apanha um erro de digitação **antes** de
    cifrar a identidade com a passphrase errada — depois já não há
    volta, porque a passphrase não é guardada em lado nenhum.

    Um `EOFError` (stdin fechado, por exemplo `onyxchat iniciar < /dev/null`)
    é uma excepção de sistema, não uma passphrase vazia: distinguish as
    duas dá uma mensagem útil em vez de um ``KeyError`` a meio da
    criação da identidade.
    """
    try:
        primeira = getpass.getpass("passphrase: ")
        if confirmar:
            segunda = getpass.getpass("repetir a passphrase: ")
            if primeira != segunda:
                raise SystemExit("as duas passphrases não coincidem")
        return primeira
    except EOFError:
        raise SystemExit(
            "passphrase pedida em modo interativo, mas a entrada está fechada\n"
            "usa --passphrase-ficheiro FICH, ou --cifrar com $ONYXCHAT_PASSPHRASE"
        ) from None


def _passphrase_de(argumentos: argparse.Namespace, *, confirmar: bool = False) -> str | None:
    """Resolve a passphrase de três fontes, por esta ordem de precedência.

    1. ``--passphrase-ficheiro`` — um ficheiro cujo **primeiro** teor é a
       passphrase. É a via para não a ter em `argv`, que é legível por
       qualquer utilizador em `/proc/<pid>/cmdline` (modo 0444) e fica
       no histórico do shell.
    2. ``$ONYXCHAT_PASSPHRASE`` — para uso programático. Menos seguro que
       (1): o ambiente é legível por tudo o que o utilizador executa.
    3. Prompt interactivo, se o terminal estiver attached.

    Devolve ``None`` quando o utilizador não pediu cifra e não há
    nenhuma fonte disponível — o que significa «formato legado, seed em
    claro», e é a opção por omissão para não trocar o formato de ninguém
    sem ele pedir.
    """
    caminho = getattr(argumentos, "passphrase_ficheiro", None)
    if caminho is not None:
        try:
            conteudo = Path(caminho).read_text(encoding="utf-8")
        except OSError as erro:
            raise SystemExit(f"não consegui ler a passphrase de {caminho}: {erro}")
        # Só a primeira linha: um ficheiro escrito com `echo` traz um
        # `\n` no fim, e esse byte faria parte da passphrase — o que
        # faria a segunda leitura não bater com a primeira.
        primeira_linha = conteudo.split("\n", 1)[0]
        if not primeira_linha:
            raise SystemExit(f"{caminho} está vazio")
        return primeira_linha

    do_ambiente = os.environ.get("ONYXCHAT_PASSPHRASE")
    if do_ambiente:
        return do_ambiente

    if getattr(argumentos, "cifrar", False):
        return _pedir_passphrase(confirmar=confirmar)

    return None


def _abrir_config(argumentos: argparse.Namespace):
    """Abre a configuração, pedindo a passphrase se ela for necessária.

    A excepção `PassphraseNecessaria` é o momento certo para perguntar:
    sabe-se que a configuração está cifrada e **qual** o keystore que a
    abre, e o utilizador tem o prompt à frente. Pedir antes seria
    adivinhar.
    """
    passphrase = _passphrase_de(argumentos)
    try:
        return carregar(passphrase=passphrase)
    except PassphraseNecessaria as erro:
        # Pedir mesmo que não tenha sido pedido: sem isto, uma
        # configuração cifrada dava «passphrase necessária» a um
        # utilizador que não sabe que o ficheiro existe.
        return carregar(passphrase=_pedir_passphrase())


def _cmd_iniciar(argumentos: argparse.Namespace) -> int:
    """Cria a configuração local e imprime a pública derivada.

    Com ``--cifrar``, a seed e as chaves vão para um keystore cifrado
    (PBKDF2 + ChaCha20-Poly1305) e o ``config.json`` fica só com
    preferências. Sem a flag, mantém-se o formato legado.
    """
    pasta = Path(argumentos.pasta) if argumentos.pasta else None
    # A confirmação só faz sentido numa identidade nova: quem está a
    # recriar já sabe o que escreveu.
    passphrase = _passphrase_de(argumentos, confirmar=True)
    config = config_mod.criar(
        forcar=argumentos.forcar,
        socket=argumentos.socket,
        pasta_dados=pasta,
        passphrase=passphrase,
    )
    print(ui.pintar(f"identidade criada: {config.pub.hex()}", "verde"))
    if config.keystore is not None:
        print(
            ui.pintar(
                f"chaves cifradas em {config.keystore}", "verde"
            )
        )
        print(
            "a passphrase não é guardada em lado nenhum; "
            "sem ela a identidade é irrecuperável"
        )
    return 0


def _cmd_cifrar(argumentos: argparse.Namespace) -> int:
    """Migrate a configuração legada para keystore cifrado.

    Irreversível: depois, a identidade só abre com a passphrase. Por
    isso confirma-se explicitamente, e avisa-se de que não há volta.
    """
    passphrase = _passphrase_de(argumentos, confirmar=True)
    if passphrase is None:
        raise SystemExit(
            "nenhuma passphrase fornecida\n"
            "usa --passphrase-ficheiro FICHEIRO, ou --cifrar para o prompt"
        )
    destino = config_mod.caminho_config()
    try:
        keystore = config_mod.migrar_para_cifrado(passphrase, destino)
    except ConfigInvalida as erro:
        print(ui.pintar(str(erro), "vermelho"), file=sys.stderr)
        return 1
    print(ui.pintar(f"configuração cifrada; chaves em {keystore}", "verde"))
    print("a passphrase não é guardada; sem ela a identidade é irrecuperável")
    return 0


def _cmd_identidade(argumentos: argparse.Namespace) -> int:
    """Imprime a chave pública da identidade local.

    É o subcomando que mais precisa de funcionar numa configuração
    cifrada: é por ele que o utilizador descobre que o seu endereço
    público é este, e é o único que precisa da passphrase **sem** tocar
    no daemon. Por isso pede a passphrase interactivamente quando a
    configuração está cifrada e não foi fornecida nenhuma.
    """
    config = _abrir_config(argumentos)
    print(ui.pintar(f"pub: {config.pub.hex()}", "azul"))
    return 0


def _cmd_enviar(argumentos: argparse.Namespace) -> int:
    """Cifra ``texto`` (K1→K9 no daemon) e imprime o envelope em hex."""
    config = _abrir_config(argumentos)
    chaves = Chaves(
        k1=bytes.fromhex(argumentos.k1),
        k5=config.k5,
        k9=bytes.fromhex(argumentos.k9_par),
    )
    caminho = (
        argumentos.socket if argumentos.socket is not None else config.socket
    )
    envelope = Pipeline(ClienteIpc(caminho)).cifrar(
        argumentos.texto, chaves, config.identidade
    )
    print(envelope.hex())
    return 0


def _cmd_receber(argumentos: argparse.Namespace) -> int:
    """Valida a assinatura localmente, decifra e imprime o plaintext."""
    config = _abrir_config(argumentos)
    chaves = Chaves(
        k1=bytes.fromhex(argumentos.k1),
        k5=bytes.fromhex(argumentos.k5_par),
        k9=config.k9,
    )
    caminho = (
        argumentos.socket if argumentos.socket is not None else config.socket
    )
    texto = Pipeline(ClienteIpc(caminho)).decifrar(
        bytes.fromhex(argumentos.envelope),
        chaves,
        bytes.fromhex(argumentos.pub_par),
    )
    print(texto)
    return 0


def _cmd_descobrir(argumentos: argparse.Namespace) -> int:
    """Consulta o discovery e imprime o ``.onion`` (ou falha com 1)."""
    onion = consultar_descoberta(argumentos.servidor, argumentos.identificador)
    if onion is None:
        print(
            ui.pintar(
                f"onyxchat: {argumentos.identificador}: desconhecido",
                "amarelo",
            ),
            file=sys.stderr,
        )
        return 1
    print(onion)
    return 0


#: Tradução do ``--modo`` da LIGAR para o byte do protocolo.
MODOS = {"direto": MODO_DIRETO, "relay": MODO_RELAY}


def _caminho(argumentos: argparse.Namespace) -> str:
    """Socket do daemon: ``--socket`` se existir, senão o da config."""
    config = _abrir_config(argumentos)
    return argumentos.socket if argumentos.socket is not None else config.socket


def _cliente(argumentos: argparse.Namespace) -> ClienteIpc:
    """Cliente IPC apontando ao socket escolhido."""
    return ClienteIpc(_caminho(argumentos))


def _cmd_estado(argumentos: argparse.Namespace) -> int:
    """Mostra backend Tor, ligação P2P, amigos e ``.onion`` publicado."""
    estado = _cliente(argumentos).estado()
    tor = NOMES_TOR.get(estado.tor, f"tor{estado.tor:#04x}")
    ligado = "sim" if estado.ligado else "nao"
    onion = estado.onion or "-"
    print(
        ui.pintar(
            f"tor: {tor} | ligado: {ligado} | amigos: {estado.amigos}"
            f" | onion: {onion}",
            "azul",
        )
    )
    return 0


def _cmd_ouvir(argumentos: argparse.Namespace) -> int:
    """Publica o hidden service e imprime o ``.onion`` (sem cor, p/ scripts)."""
    print(_cliente(argumentos).ouvir())
    return 0


def _cmd_ligar(argumentos: argparse.Namespace) -> int:
    """Abre a ligação P2P ao par (``--modo directo`` ou ``relay``).

    Em modo directo, um ID hex é resolvido em ``.onion`` pelo cliente antes
    de emitir ``LIGAR``: o daemon não tem cliente de discovery e recusaria
    um ID. Ver ``messenger/descoberta.py`` e ``docs/ipc_spec.md``
    §``destino``.
    """
    modo = MODOS[argumentos.modo]
    destino = resolver_ou_usar(
        argumentos.destino,
        modo,
        argumentos.servidor,
    )
    _cliente(argumentos).ligar(
        modo,
        argumentos.endpoint,
        destino,
        bytes.fromhex(argumentos.pub_propria),
        bytes.fromhex(argumentos.pub_par),
    )
    print(
        ui.pintar(
            f"ligado ({argumentos.modo}) a {destino}",
            "verde",
        )
    )
    return 0


def _cmd_enviar_p2p(argumentos: argparse.Namespace) -> int:
    """Envia um envelope (hex) já cifrado como frame ``CHAT``."""
    envelope = bytes.fromhex(argumentos.envelope)
    _cliente(argumentos).enviar(envelope)
    print(ui.pintar(f"enviado ({len(envelope)} bytes)", "verde"))
    return 0


def _cmd_receber_p2p(argumentos: argparse.Namespace) -> int:
    """Espera um frame e imprime ``<tipo:2 hex> <corpo:hex>``."""
    frame = _cliente(argumentos).receber(argumentos.timeout_ms)
    print(f"{frame.tipo:02x} {frame.corpo.hex()}")
    return 0


def _cmd_fechar(argumentos: argparse.Namespace) -> int:
    """Fecha a ligação P2P (idempotente no daemon)."""
    _cliente(argumentos).fechar()
    print(ui.pintar("ligacao fechada", "verde"))
    return 0


def _seed_local(argumentos: argparse.Namespace):
    """A seed local, de ``--seed`` ou do ``config.json``.

    Ler do config é o caminho por omissão, e a razão é
    ``/proc/<pid>/cmdline``: um ``--seed`` fica em modo ``0444`` para
    qualquer utilizador da máquina e no histórico do shell, e
    ``ptrace_scope = 1`` não protege o ``cmdline``.

    O ``--seed`` continua a existir porque o daemon pode estar a correr
    com outra identidade — mas passa a ser opcional, o que já elimina a
    exposição por omissão.
    """
    dado = getattr(argumentos, "seed", None)
    if dado is not None:
        try:
            return bytes.fromhex(dado)
        except ValueError:
            raise SystemExit("--seed tem de ser hex válido")
    return _abrir_config(argumentos).identidade.seed


def _cmd_pedir_amizade(argumentos: argparse.Namespace) -> int:
    """Inicia o handshake e imprime ``k1/k5/k9`` + corpo a entregar ao par."""
    pedido = _cliente(argumentos).pedir_amizade(
        _seed_local(argumentos), bytes.fromhex(argumentos.destino)
    )
    print(f"k1={pedido.k1.hex()}")
    print(f"k5={pedido.k5.hex()}")
    print(f"k9={pedido.k9.hex()}")
    print(f"corpo={pedido.corpo.hex()}")
    return 0


def _imprimir_chaves(chaves, corpo: bytes | None = None) -> None:
    """Escreve uma amizade em ``nome=hex`` (uma por linha)."""
    print(f"k1={chaves.k1.hex()}")
    print(f"k5_proprio={chaves.k5_proprio.hex()}")
    print(f"k9_proprio={chaves.k9_proprio.hex()}")
    print(f"k5_par={chaves.k5_par.hex()}")
    print(f"k9_par={chaves.k9_par.hex()}")
    print(f"publica={chaves.publica.hex()}")
    if corpo is not None:
        print(f"corpo={corpo.hex()}")


def _guardar_amizade(chaves, caminho: Path) -> bool:
    """Guarda a amizade no keystore, e diz se conseguiu.

    ## Porque é que isto não é opcional

    Até 2026-10-07 as chaves eram **só impressas**. A consequência
    estava escrita em ``docs/handshake.md``: o daemon só mantém
    identidades em memória, e portanto reiniciá-lo obriga a refazer o
    handshake com todas as pessoas. Uma amizade que não sobrevive a um
    reinício do daemon não é uma amizade, é um rito.

    ## Porque é que uma falha aqui **não** é fatal

    O handshake já aconteceu: o par tem o nosso pedido, o daemon já
    registou a sessão. Falhar a gravar significa que a amizade se
    perdeu, e perder a amizade é melhor do que mentir à pessoa — que é
    o que acontece se o comando sair com 0 depois de falhar a escrita.

    Por isso a falha é dita, o código de saída é 1, e as chaves já
    foram impressas para que a pessoa as possa guardar à mão.

    ## Porque é que a frase é pedida aqui

    Gravar é cifrar, e a cifra é a frase de segurança. Não há atalho:
    derivar uma chave de sessão guardada seria pôr o segredo em dois
    sítios, e um dos dois ficaria desatualizado.

    ## Porque é que uma conta que não existe não é erro

    O ``daemon_binario`` de testes e os comandos que correm sobre uma
    identidade do ``config.json`` não têm conta local. Ficar sem
    guardar é o comportamento correcto aí: o comando handshake cumpriu o
    que tinha a prometer, e quem não tem conta não tem onde guardar.
    """
    if not conta_mod.conta_existe(caminho):
        print(
            ui.pintar(
                "sem conta local: a amizade não foi guardada (o daemon "
                "precisa dela de cada vez)",
                "amarelo",
            ),
            file=sys.stderr,
        )
        return True

    try:
        frase = _pedir_passphrase()
    except SystemExit:
        # A pessoa cancelou a frase de segurança. A amizade está feita
        # do lado do daemon e já foram impressas acima; dizer «guardado» seria
        # mentira.
        print(
            ui.pintar("amizade feita mas não guardada", "amarelo"),
            file=sys.stderr,
        )
        return False

    aberta = conta_mod.entrar(frase, caminho)
    conta_mod.gravar(
        aberta.com_amizade(conta_mod.Amizade.de_chaves(chaves)), frase, caminho
    )
    print(
        ui.pintar(
            f"amizade guardada em {caminho} (sobrevive a reiniciar o daemon)",
            "verde",
        )
    )
    return True


def _cmd_aceitar_amizade(argumentos: argparse.Namespace) -> int:
    """Valida o ``FRIEND_REQUEST`` recebido e imprime chaves + aceite.

    As chaves são imprimidas **e** guardadas. Imprimir porque o par
    precisa do ``corpo`` do aceite; guardar porque as chaves são o que
    permite mandar mensagens depois, e perdê-las obriga a refazer o
    handshake. Ver :func:`_guardar_amizade`.
    """
    aceite = _cliente(argumentos).aceitar_amizade(
        _seed_local(argumentos), bytes.fromhex(argumentos.corpo)
    )
    _imprimir_chaves(aceite.chaves, aceite.corpo)
    return 0 if _guardar_amizade(aceite.chaves, conta_mod.caminho_conta()) else 1


def _cmd_recusar_amizade(argumentos: argparse.Namespace) -> int:
    """Assina a recusa do pedido e imprime o ``FRIEND_REJECT``."""
    corpo = _cliente(argumentos).recusar_amizade(
        _seed_local(argumentos), bytes.fromhex(argumentos.nonce)
    )
    print(f"corpo={corpo.hex()}")
    return 0


def _cmd_confirmar_amizade(argumentos: argparse.Namespace) -> int:
    """Valida o ``FRIEND_ACCEPT`` e fecha a amizade com as 6 chaves."""
    chaves = _cliente(argumentos).confirmar_amizade(
        _seed_local(argumentos), bytes.fromhex(argumentos.corpo)
    )
    _imprimir_chaves(chaves)
    return 0 if _guardar_amizade(chaves, conta_mod.caminho_conta()) else 1


def _cmd_amigos(argumentos: argparse.Namespace) -> int:
    """Lista as amizades que sobreviveram dentro do keystore.

    Pede a frase de segurança porque abrir o keystore **é** decifrar.
    Um `amigos` que não pede a frase seria uma lista de friendships
    legível por qualquer processo da máquina.

    A identidade de cada par é o **cabeçalho da pública**, e não o
    Onyx ID: o ID é um SHA-256 do sal com o nome, e sem o nome não se
    pode confirmar que a linha é quem diz ser. A pública é o que o
    protocolo valida, e é o que se compara com o que o daemon tem.
    """
    caminho = conta_mod.caminho_conta()
    if not conta_mod.conta_existe(caminho):
        print(ui.pintar("sem conta local: não há amizades guardadas", "amarelo"))
        return 1

    aberta = conta_mod.entrar(_pedir_passphrase(), caminho)
    if not aberta.amizades:
        print(ui.pintar("nenhuma amizade guardada", "amarelo"))
        print(
            ui.pintar(
                "faça um handshake (`onyxchat aceitar-amizade`) para a guardar",
                "amarelo",
            )
        )
        return 0

    print(ui.pintar(f"{len(aberta.amizades)} amizade(s) guardada(s)", "verde"))
    for amizade in aberta.amizades:
        print(f"par={amizade.publica_hex}")
        print(f"  desde={amizade.criada_em}")
        if argumentos.chaves:
            print(f"  k1={amizade.k1_hex}")
            print(f"  k5_proprio={amizade.k5_proprio_hex}")
            print(f"  k9_proprio={amizade.k9_proprio_hex}")
            print(f"  k5_par={amizade.k5_par_hex}")
            print(f"  k9_par={amizade.k9_par_hex}")
    return 0


def _cmd_relay(argumentos: argparse.Namespace) -> int:
    """Corre o relé TURN em TCP (bloqueia até o processo morrer)."""
    print(
        ui.pintar(
            f"relay a escuta em {argumentos.host}:{argumentos.porta}", "verde"
        )
    )
    relay_server.executar(argumentos.host, argumentos.porta)
    return 0


def _cmd_sidecar(argumentos: argparse.Namespace) -> int:
    """Corre o sidecar em ``127.0.0.1`` (bloqueia até o processo morrer).

    A frase de segurança é pedida **antes** de o servidor arrancar, e não
    depois. Um sidecar que arranca e só depois pergunta tem uma janela em
    que está a servir rotas sem saber se há conta — e a rota de identidade
    devolve ``503``, o que é correcto, mas a de registo criava a conta sem
    que ninguém a tenha pedido.

    Pedir a frase aqui também evita que o sidecar corra com a conta
    trancada e a interface fique a poder registar-se sem querer.
    """
    frase = _pedir_passphrase()
    caminho_conta = conta_mod.caminho_conta()

    aberta = None
    if conta_mod.conta_existe(caminho_conta):
        try:
            aberta = conta_mod.entrar(frase, caminho_conta)
        except storage.PassphraseErrada:
            print(
                ui.pintar(
                    "a frase de segurança não abre a conta; o sidecar vai "
                    "arrancar trancado e a interface vai pedir a frase",
                    "amarelo",
                ),
                file=sys.stderr,
            )
        except storage.KeystoreInvalido as erro:
            print(ui.pintar(f"onyxchat: {erro}", "vermelho"), file=sys.stderr)
            return 1

    print(ui.pintar(f"sidecar a escutar em {argumentos.host}:{argumentos.porta}", "verde"))
    if not sidecar_mod.e_loopback(argumentos.host):
        # A pergunta é ao **endereço**, e é a mesma que `criar_servidor`
        # faz. Comparar com `HOST_OMISSAO` dava dois falsos: `localhost` e
        # `127.0.0.2` são loopback e passariam a levar o aviso, enquanto
        # o aviso deixava de aparecer quando o endereço era o problema.
        print(
            ui.pintar(
                "o sidecar só escuta em loopback; este endereço vai ser recusado",
                "amarelo",
            ),
            file=sys.stderr,
        )

    sidecar_mod.executar(
        argumentos.porta, conta=aberta, host=argumentos.host,
        caminho_conta=caminho_conta,
    )
    return 0


def _construir_parser() -> argparse.ArgumentParser:
    """Monta a árvore de subcomandos (``func`` ligado a cada handler)."""
    parser = argparse.ArgumentParser(
        prog="onyxchat",
        description="Mensageiro E2E OnyxChat — camada Python sobre o daemon",
    )
    subcomandos = parser.add_subparsers(dest="comando", required=True)

    iniciar = subcomandos.add_parser("iniciar", help="cria identidade e config")
    iniciar.add_argument(
        "--forcar", action="store_true", help="substitui config existente"
    )
    iniciar.add_argument("--socket", help="caminho do socket do daemon")
    iniciar.add_argument("--pasta", help="pasta de dados local")
    _add_argumentos_passphrase(iniciar)
    iniciar.set_defaults(func=_cmd_iniciar)

    identidade = subcomandos.add_parser(
        "identidade", help="mostra a chave pública local"
    )
    _add_argumentos_passphrase(identidade)
    identidade.set_defaults(func=_cmd_identidade)

    cifrar = subcomandos.add_parser(
        "cifrar",
        help="migra a configuração para keystore cifrado (irreversível)",
    )
    _add_argumentos_passphrase(cifrar)
    cifrar.set_defaults(func=_cmd_cifrar)

    enviar = subcomandos.add_parser("enviar", help="cifra texto (ENCODE)")
    enviar.add_argument("texto", help="mensagem em claro")
    enviar.add_argument("--k1", required=True, help="chave K1 do par (hex)")
    enviar.add_argument(
        "--k9-par", required=True, help="chave K9 do receptor (hex)"
    )
    enviar.add_argument("--socket", help="socket do daemon (override)")
    _add_argumentos_passphrase(enviar)
    enviar.set_defaults(func=_cmd_enviar)

    receber = subcomandos.add_parser("receber", help="decifra envelope (DECODE)")
    receber.add_argument("envelope", help="envelope em hex")
    receber.add_argument("--k1", required=True, help="chave K1 do par (hex)")
    receber.add_argument(
        "--k5-par", required=True, help="chave K5 do remetente (hex)"
    )
    receber.add_argument(
        "--pub-par", required=True, help="pública Ed25519 do remetente (hex)"
    )
    receber.add_argument("--socket", help="socket do daemon (override)")
    _add_argumentos_passphrase(receber)
    receber.set_defaults(func=_cmd_receber)

    descobrir = subcomandos.add_parser(
        "descobrir", help="resolve ID → .onion"
    )
    descobrir.add_argument("identificador", help="ID do par")
    descobrir.add_argument(
        "--servidor",
        default=SERVIDOR_DESCOBERTA,
        help="endpoint do discovery (http://host:porta)",
    )
    descobrir.set_defaults(func=_cmd_descobrir)

    estado = subcomandos.add_parser("estado", help="estado do daemon (Etapa 7)")
    estado.add_argument("--socket", help="socket do daemon (override)")
    estado.set_defaults(func=_cmd_estado)

    ouvir = subcomandos.add_parser(
        "ouvir", help="publica o hidden service e imprime o .onion"
    )
    ouvir.add_argument("--socket", help="socket do daemon (override)")
    ouvir.set_defaults(func=_cmd_ouvir)

    ligar = subcomandos.add_parser("ligar", help="abre a ligação P2P")
    ligar.add_argument(
        "--modo", choices=sorted(MODOS), default="direto", help="modo da ligação"
    )
    ligar.add_argument(
        "--endpoint", default="", help="endpoint vazio = omissão do daemon"
    )
    ligar.add_argument(
        "--servidor",
        default=SERVIDOR_DESCOBERTA,
        help="discovery para resolver um ID hex (modo directo)",
    )
    ligar.add_argument(
        "destino",
        help="xxxx.onion ou ID hex da pública do par (resolvido em modo directo)",
    )
    ligar.add_argument("pub_propria", help="pública Ed25519 local (hex)")
    ligar.add_argument("pub_par", help="pública Ed25519 do par (hex)")
    ligar.add_argument("--socket", help="socket do daemon (override)")
    ligar.set_defaults(func=_cmd_ligar)

    enviar_p2p = subcomandos.add_parser(
        "enviar-p2p", help="envia envelope hex como frame CHAT"
    )
    enviar_p2p.add_argument("envelope", help="envelope em hex")
    enviar_p2p.add_argument("--socket", help="socket do daemon (override)")
    enviar_p2p.set_defaults(func=_cmd_enviar_p2p)

    receber_p2p = subcomandos.add_parser(
        "receber-p2p", help="espera um frame P2P e imprime tipo + corpo"
    )
    receber_p2p.add_argument(
        "--timeout-ms",
        type=int,
        default=5000,
        help="espera em ms (0 = infinita, omissão 5000)",
    )
    receber_p2p.add_argument("--socket", help="socket do daemon (override)")
    receber_p2p.set_defaults(func=_cmd_receber_p2p)

    fechar = subcomandos.add_parser("fechar", help="fecha a ligação P2P")
    fechar.add_argument("--socket", help="socket do daemon (override)")
    fechar.set_defaults(func=_cmd_fechar)

    pedir = subcomandos.add_parser(
        "pedir-amizade", help="envia FRIEND_REQUEST ao par"
    )
    pedir.add_argument("destino", help="pública Ed25519 do par (hex)")
    pedir.add_argument(
        "--seed",
        help=(
            "seed local em hex. OMITIR para a ler do config.json — "
            "obrigatório quando a configuração está cifrada, porque "
            "então só a passphrase a abre"
        ),
    )
    pedir.add_argument("--socket", help="socket do daemon (override)")
    _add_argumentos_passphrase(pedir)
    pedir.set_defaults(func=_cmd_pedir_amizade)

    aceitar = subcomandos.add_parser(
        "aceitar-amizade", help="aceita um FRIEND_REQUEST recebido"
    )
    aceitar.add_argument("corpo", help="corpo de FRIEND_REQUEST (hex)")
    aceitar.add_argument(
        "--seed",
        help="seed local em hex; omitir para a ler do config.json",
    )
    aceitar.add_argument("--socket", help="socket do daemon (override)")
    _add_argumentos_passphrase(aceitar)
    aceitar.set_defaults(func=_cmd_aceitar_amizade)

    recusar = subcomandos.add_parser(
        "recusar-amizade", help="recusa um FRIEND_REQUEST recebido"
    )
    recusar.add_argument("nonce", help="nonce do pedido (hex, 16 bytes)")
    recusar.add_argument(
        "--seed",
        help="seed local em hex; omitir para a ler do config.json",
    )
    recusar.add_argument("--socket", help="socket do daemon (override)")
    _add_argumentos_passphrase(recusar)
    recusar.set_defaults(func=_cmd_recusar_amizade)

    confirmar = subcomandos.add_parser(
        "confirmar-amizade", help="confirma um FRIEND_ACCEPT recebido"
    )
    confirmar.add_argument("corpo", help="corpo de FRIEND_ACCEPT (hex)")
    confirmar.add_argument(
        "--seed",
        help="seed local em hex; omitir para a ler do config.json",
    )
    confirmar.add_argument("--socket", help="socket do daemon (override)")
    _add_argumentos_passphrase(confirmar)
    confirmar.set_defaults(func=_cmd_confirmar_amizade)

    amigos = subcomandos.add_parser(
        "amigos", help="lista as amizades guardadas no keystore"
    )
    amigos.add_argument(
        "--chaves",
        action="store_true",
        help="mostra também as chaves, em hex (são segredo: use com cuidado)",
    )
    _add_argumentos_passphrase(amigos)
    amigos.set_defaults(func=_cmd_amigos)

    relay = subcomandos.add_parser("relay", help="corre o relé TURN em TCP")
    relay.add_argument("--host", default="127.0.0.1", help="interface de escuta")
    relay.add_argument("--porta", type=int, default=8788, help="porta TCP")
    relay.set_defaults(func=_cmd_relay)

    sidecar = subcomandos.add_parser(
        "sidecar", help="serve a interface e a API local em 127.0.0.1"
    )
    sidecar.add_argument(
        "--host",
        default=sidecar_mod.HOST_OMISSAO,
        help="interface de escuta (só loopback é aceite)",
    )
    sidecar.add_argument(
        "--porta", type=int, default=8787, help="porta TCP (a da interface é 8787)"
    )
    sidecar.set_defaults(func=_cmd_sidecar)
    return parser


def main(argv: list[str] | None = None) -> int:
    """Ponto de entrada do ``onyxchat`` — devolve o código de saída."""
    argumentos = _construir_parser().parse_args(argv)
    try:
        return argumentos.func(argumentos)
    except (
        ErroIpc,
        AssinaturaInvalida,
        DescobertaInvalida,
        EnvelopeInvalido,
        ConfigInvalida,
        RelayErro,
        ValueError,
    ) as erro:
        # EnvelopeInvalido/ConfigInvalida são ValueError; listados à
        # mesma para documentar o contrato de erros da CLI.
        print(ui.pintar(f"onyxchat: {erro}", "vermelho"), file=sys.stderr)
        return 1


# Ponto de entrada de subprocesso; `main()` e o parser são testados
# directamente em tests/test_cli.py — pragma de cobertura no bloco.
if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
