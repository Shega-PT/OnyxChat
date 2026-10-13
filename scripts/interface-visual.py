#!/usr/bin/env python3
# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.
#
# =====================================================================
# interface-visual.py — monta a interface e o servidor para os validar
# ---------------------------------------------------------------------
# Este ficheiro existe porque avaliar uma interface exige três coisas ao
# mesmo tempo: o **servidor**, a **interface compilada** e uma **janela**
# onde a ver. Monte-as uma a uma e cada uma falha à sua maneira — a
# interface sem servidor mostra a demonstração e esconde se o backend
# funciona; o servidor sem interface não mostra nada; e sem janela não há
# avaliação visual.
#
# E «janela» aqui não é um navegador: `chromium`, `firefox` e
# `google-chrome` são dependências que o projecto não tem e este ficheiro
# não passa a ter. A janela é montada com `gjs`, GTK 4 e WebKit, e é o
# `interface-visual.js` que a desenha. Se algum faltar, o que falta está
# escrito na secção «Requisitos» de `requisitos.txt`.
#
# ## Duas montagens, e porquê duas
#
# ``--demo`` serve a interface com o ``mock-adapter`` (``?mock=1``): não
# há daemon, não há conta e não há loja. É o que permite ver o desenho
# dos ecrãs com conteúdo.
#
# ``--real`` arranca o daemon e serve com o ``bridge-adapter``
# (``?mock=0``), ou seja, o backend verdadeiro. É o que permite ver se o
# que a interface mostra veio mesmo do sidecar.
#
# A diferença entre as duas não é cosmética: é exactamente o que a Etapa 7
# do `docs/roadmap.md` tem de acertar. Ver um ecrã com ``--demo`` não diz
# nada sobre o modo como esse ecrã trata os dados reais.
#
# ## Porque é que isto corre o sidecar dentro do processo
#
# Porque ``onyxchat sidecar`` **não pode** ser lançado como subprocesso:
# `user/cli.py` pede a frase de segurança com `getpass` antes de servir,
# e `getpass` lê do terminal. Em `--real` a conta é descartável e a frase
# é conhecida, por isso o prompt é um obstáculo sem contrapartida.
#
# E há um segundo motivo, mais sério: `_cmd_sidecar` ignora
# ``$ONYXCHAT_PASSPHRASE`` de propósito — a variável existe para
# «uso programático», e o prompt de `sidecar` é deliberadamente
# interactivo porque um sidecar que arranca e só depois pergunta tem uma
# janela em que está a servir rotas sem saber se há conta. Um `subprocess`
# a este comando herda a decisão de desenho errada.
#
# ## Porque é que a conta e a loja são descartáveis
#
# Porque `user/loja.db` e `user/conta.keystore` são ficheiros de alguém.
# Um script de avaliação visual que escrevesse neles deixaria conversas
# de testes na conta real, e apagá-las depois é apagar depois de ter
# estragado. `ONYX_CONTA`, `ONYXCHAT_CONFIG` e `ONYXCHAT_SOCKET` são
# apontados para um directório temporário, que desaparece no fim — com ou
# sem `Ctrl-C`.
#
# ## Porquê `criar_servidor` e não `executar`
#
# `server.sidecar.executar` corre até o processo morrer e não devolve nada
# pelo que se possa parar. `criar_servidor` devolve o servidor sem o
# arrancar, que é a separação que `tests/test_sidecar.py` já usa para ter
# uma porta livre por teste. Aqui é o que permite desligar a pilha com
# ordem em vez de a deixar para a morte do processo.
#
# ## O ambiente é corrigido antes de o `gjs` arrancar
#
# Porque o `gjs` herda o ambiente de quem o lançou, e há ambientes de onde
# ele não arranca. Quando o projecto é aberto dentro do VS Code instalado
# como *snap*, o processo recebe ``GSETTINGS_SCHEMA_DIR`` a apontar para as
# esquemas do *snap* e ``XDG_DATA_DIRS`` com os directórios do *snap* à
# frente. O WebKitGTK lê ``org.gnome.settings-daemon.plugins.xsettings`` ao
# construir a ``WebView``, o esquema do *snap* não tem a chave
# ``antialiasing``, e o processo morre. Pior: o ``WebKitNetworkProcess``
# acabava por carregar a ``libpthread`` do *snap* ``core20`` e as páginas
# deixavam de carregar.
#
# Não é uma hipótese — foi medido, e a diferença entre «WebView morre» e
# «WebView carrega a página» é exactamente o conjunto de variáveis que
# `_ambiente_limpo` remove.
#
# ## A semeadura usa o caminho que a aplicação usa
#
# Os contactos não nascem de uma inserção directa: nascem de um pedido de
# amizade aceite. `_criar_contacto_de_pedido` em `server/loja.py` é o que
# os cria, e `Loja` não expõe nenhuma outra forma de os pôr na loja. Um
# cenário que os inseri-se por baixo da aplicação produziria um estado que
# o OnyxChat não sabe atingir, e um cenário que não sabe atingir não
# serve para avaliar o que a interface faz quando um estado aparece.
#
# ## Uso
#
#   scripts/interface-visual.py                       # demonstração
#   scripts/interface-visual.py --real                # backend verdadeiro
#   scripts/interface-visual.py --real --cenario cheio
#   scripts/interface-visual.py --guardar /tmp/ecras  # exporta PNG
#   scripts/interface-visual.py --construir           # compila antes
#
# Teclas na janela: ``1``–``8`` mudam de ecrã, ``d`` alterna entre
# demonstração e backend real, ``s`` guarda um PNG, ``q`` fecha.
# =====================================================================

from __future__ import annotations

import argparse
import contextlib
import errno
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

#: Raiz do projecto — a pasta que contém ``Cargo.toml``.
RAIZ = Path(__file__).resolve().parent.parent

if str(RAIZ) not in sys.path:
    sys.path.insert(0, str(RAIZ))

#: O driver em ``gjs``. Vive ao lado porque depende do mesmo ambiente e
#: da mesma versão de GTK, e porque um ficheiro ``.js`` dentro de ``UI/``
#: entraria na contagem de ficheiros que ``verificar_estrutura.py``
#: confere contra ``UI/README.md``.
DRIVER = Path(__file__).resolve().parent / "interface-visual.js"

#: A interface compilada. ``--construir`` reconstrói isto.
RAIZ_UI = RAIZ / "UI" / "dist"

#: O daemon. O binário de *debug* chega e sobra para avaliar a interface:
#: o que se está a avaliar é o sidecar e a interface, não o desempenho do
#: pipeline, e um binário de *release* com Tor são 458 crates a mais para
#: o mesmo ecrã.
DAEMON = RAIZ / "target" / "debug" / "onyxchatd"

#: Frase da conta descartável. Não é um segredo — a conta vive num
#: directório temporário que é apagado no fim, e o objectivo é poder
#: repetir a mesma montagem sem digitar nada. Está escrita à mão em vez
#: de gerada para que seja a mesma em todas as execuções: uma falha de
#: determinismo na semeadura seria difícil de distinguir de uma falha
#: real, e o que se quer é que a segunda montagem se pareça com a
#: primeira.
FRASE_DESCARTAVEL = "onyxchat-visual-descartavel"

#: Nome de utilizador da conta descartável, pelo mesmo motivo.
UTILIZADOR_DESCARTAVEL = "Avaliacao Visual"

#: As variáveis que o *snap* do editor injecta e que o WebKitGTK não
#: tolera. A lista não é decorativa: cada uma delas foi observada a
#: quebrar o arranque, e a ordem é a de Instance.
VARIAVEIS_DO_SNAP = (
    "GSETTINGS_SCHEMA_DIR",
    "GTK_PATH",
    "GIO_MODULE_DIR",
    "GTK_IM_MODULE_FILE",
    "GTK_EXE_PREFIX",
    "LOCPATH",
    "GTK_IM_MODULE",
    "GTK_THEME",
    "GDK_PIXBUF_MODULE_FILE",
    "GDK_PIXBUF_MODULE_PATH",
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "LIBGL_DRIVERS_PATH",
    "__GLX_VENDOR_LIBRARY_NAME",
)

#: Pares ``(variável, variável de origem, omissão)``. O *snap* do Code
#: guarda o valor de origem em ``<NOME>_VSCODE_SNAP_ORIG``; sem isso não
#: há como saber o que era, e adivinhar um ``XDG_DATA_DIRS`` é pior do que
#: não mexer.
PARES_DE_ORIGEM = (
    ("XDG_DATA_DIRS", "XDG_DATA_DIRS_VSCODE_SNAP_ORIG", "/usr/local/share:/usr/share"),
    ("XDG_CONFIG_DIRS", "XDG_CONFIG_DIRS_VSCODE_SNAP_ORIG", "/etc/xdg"),
)

#: As rotas da aplicação, lidas de ``UI/src/App.jsx``. A lista não está
#: escrita aqui porque uma lista escrita aqui desatualiza-se em silêncio
#: quando se acrescenta um ecrã, e um ecrã que o harness não abre é um
#: ecrã que ninguém avalia.
PADRAO_ROTA = re.compile(
    r'<Route\s+path="([^"]*)"\s+element=\{<(\w+)\s*/>\}', re.MULTILINE
)

#: Os rótulos em português, lidos de ``UI/src/lib/onyx/nav.js``. Os nomes
#: dos componentes React são ingleses (``Conversations``, ``Contacts``) e
#: usá-los nos ficheiros exportados produce ``08-contacts.png`` ao lado de
#: um ecrã que se chama *Contactos*. Os ecrãs de acesso não estão em
#: ``nav.js`` — nunca aparecem na barra — e ficam sem rótulo, que o
#: :func:`_rotulo` deriva do caminho.
PADRAO_ROTULO = re.compile(r"\{\s*to:\s*'([^']*)'\s*,\s*label:\s*'([^']*)'")

#: Os cenários de que a loja pode ser semeada.
CENARIOS = ("vazio", "uma-conversa", "cheio", "bloqueada")


# =====================================================================
# O ambiente
# =====================================================================


def _ambiente_limpo(base: dict[str, str]) -> dict[str, str]:
    """Devolve o ambiente sem a herança do *snap*.

    A remoção é de variáveis inteiras, não de valores: o WebKitGTK não
    falha por causa de um caminho errado em ``GTK_PATH``, falha por
    haver um caminho. Deixar o valor correcto seria pedir ao utilizador
    que o descobrisse.

    O que não é removido e porquê: ``HOME``, ``PATH`` e ``DISPLAY``.
    ``HOME`` é lido pelo WebKit para odirectório de dados e ``PATH``
    precisa de ``gjs``; ``DISPLAY`` é o que faz a janela aparecer, e
    limpá-lo daria uma janela invisível em vez de uma janela correcta.

    :param base: o ambiente a limpar, normalmente ``os.environ``.
    :returns: o ambiente com as substituições aplicadas.
    """
    limpo = dict(base)
    for nome in VARIAVEIS_DO_SNAP:
        limpo.pop(nome, None)

    for nome, origem, omissao in PARES_DE_ORIGEM:
        if limpo.get(nome) == omissao:
            continue
        substituto = limpo.get(origem) or omissao
        if limpo.get(nome) != substituto:
            limpo[nome] = substituto

    return limpo


# =====================================================================
# Os ecrãs
# =====================================================================


def _rotulos() -> dict[str, str]:
    """Os nomes em português dos ecrãs, lidos da barra lateral.

    :returns: um dicionário de caminho para rótulo.
    """
    nav = RAIZ / "UI" / "src" / "lib" / "onyx" / "nav.js"
    try:
        texto = nav.read_text(encoding="utf-8")
    except OSError:
        return {}

    return {caminho: rotulo for caminho, rotulo in PADRAO_ROTULO.findall(texto)}


def _rotulo(caminho: str, componente: str, rotulos: dict[str, str]) -> str:
    """O nome de um ecrã, em português.

    A ordem é: a barra lateral primeiro, porque é a interface que o
    utilizador conhece; depois o caminho, para os ecrãs de acesso; e o nome
    do componente só em último recurso, para nunca devolver uma string
    vazia que acabaria num ficheiro chamado ``.png``.

    :param caminho: o caminho da rota.
    :param componente: o nome do componente React.
    :param rotulos: o que devolve :func:`_rotulos`.
    :returns: um nome seguro para o nome de um ficheiro.
    """
    if caminho in rotulos:
        return rotulos[caminho]

    if caminho != "/":
        derivada = caminho.strip("/")
        # Um nome de ficheiro não leva barras nem acentos que mudem
        # entre sistemas de ficheiros; o corte aqui é o que garante que
        # a mesma captura sai com o mesmo nome em qualquer máquina.
        return derivada.replace("/", "-")

    return componente.lower() or "raiz"


def ecras_do_projeto() -> list[tuple[str, str]]:
    """Os ecrãs que a aplicação tem, lidos do código das rotas.

    A leitura é por expressão regular e não por um analisador de JSX
    porque não se quer um analisador: quer-se a lista de ``path``, e o
    ficheiro escreve-os todos na mesma forma. A alternativa — uma lista
    escrita à mão — é uma lista que fica a envelhecer sem dizer nada.

    **A duplicação é deliberada e é preciso desfazê-la.** ``App.jsx``
    tem dois blocos ``<Routes>``: um no portão de acesso, que só é
    desenhado quando não há conta, e outro na casca, que é o que se vê
    com a conta aberta. Os dois declaram ``/registar``, ``/entrar`` e
    ``/recuperar``, e quem lê o ficheiro sem saber disto conta catorze
    ecrãs em vez de onze.

    Fica a **primeira** ocorrência de cada caminho, e é a correcta: o
    portão vem primeiro no ficheiro, os três ecrãs de acesso ficam com o
    rótulo certo, e os ecrãs da casca entram sem repetição porque os seus
    caminhos não aparecem no portão.

    O ecrã 404 fica de fora: o seu ``path`` é ``*`` e não é um sítio para
    onde se navega, é o que se vê quando se chega a um sítio onde não há
    nada. Fica fora do ciclo e é alcançável apontando a navegação para um
    caminho à mão, que é como se chega lá.

    :returns: pares ``(caminho, nome)`` na ordem do ficheiro.
    """
    app = RAIZ / "UI" / "src" / "App.jsx"
    try:
        texto = app.read_text(encoding="utf-8")
    except OSError:
        return []

    rotulos = _rotulos()
    ecras: list[tuple[str, str]] = []
    vistos: set[str] = set()

    for caminho, componente in PADRAO_ROTA.findall(texto):
        if caminho == "*":
            continue
        normalizado = caminho or "/"
        if normalizado in vistos:
            continue
        vistos.add(normalizado)
        ecras.append((normalizado, _rotulo(normalizado, componente, rotulos)))

    return ecras




# =====================================================================
# A rede
# =====================================================================


#: A porta que o ``bridge-adapter`` fala. Está escrita em
#: ``UI/src/lib/onyx/bridge-adapter.js`` como constante e não é
#: configurável, e é por isso que ``--real`` **tem** de servir em 8787.
PORTA_DO_BRIDGE = 8787


def porta_livre() -> int:
    """Uma porta TCP livre, agora.

    Escolher uma porta e rezar para que esteja livre é a forma de uma
    falha aparecer na máquina de quem testou e nunca na de quem mais
    testou. Pedir ao sistema uma porta é o mesmo que ``tests/test_sidecar.py``
    faz, e a razão pela qual aquela suite pode correr em paralelo.

    :returns: um número de porta que estava livre no instante da chamada.
    """
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as tampa:
        tampa.bind(("127.0.0.1", 0))
        return int(tampa.getsockname()[1])


def porta_ocupada(porta: int) -> bool:
    """Já há alguém a escutar nesta porta?

    Existe por causa do modo ``--real``. Servir o sidecar noutra porta
    não dá erro nenhum: o ``bridge-adapter`` continua a falar em 8787, não
    responde a ninguém, e a interface mostra uma lista vazia com um
    aspecto perfeitamente normal. Uma falha que se apresenta como sucesso
    é pior do que uma que não arranca, e recusa-se a montar é o que a
    transforma numa falha.

    :param porta: a porta a sondar.
    :returns: ``True`` se algo já estiver a escutar.
    """
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as tampa:
        tampa.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            tampa.bind(("127.0.0.1", porta))
        except OSError as erro:
            return erro.errno in (errno.EADDRINUSE, errno.EACCES)
        return False


def esperar_ate_resposta(url: str, segundos: float = 15.0) -> bool:
    """Espera que um URL responda.

    A espera é por resposta e não por tempo: o sidecar fica a servir no
    instante em que o *bind* acaba, e uma espera fixa seria ao mesmo
    tempo mais lenta nos casos rápidos e mais curta nos lentos.

    O ``404`` conta como estar vivo. Uma rota que não existe é resposta
    do mesmo servidor, e tratá-la como falha faria o harness esperar por
    uma página que nunca chega.

    :param url: o endereço a sondar.
    :param segundos: quanto tempo esperar, no total.
    :returns: ``True`` se respondeu dentro do prazo.
    """
    fim = time.monotonic() + segundos
    ultimo: Exception | None = None
    while time.monotonic() < fim:
        try:
            pedido = urllib.request.Request(url, headers={"Host": "127.0.0.1"})
            with urllib.request.urlopen(pedido, timeout=1.0) as resposta:
                return 200 <= resposta.status < 500
        except urllib.error.HTTPError:
            # O servidor respondeu; o código é que não interessa aqui.
            return True
        except (urllib.error.URLError, OSError) as erro:
            ultimo = erro
            time.sleep(0.15)

    if ultimo is not None:
        print(f"  o servidor não respondeu: {ultimo}", file=sys.stderr)
    return False


# =====================================================================
# A pilha
# =====================================================================


class Pilha:
    """O que fica de pé enquanto a janela está aberta.

    Tudo o que é criado aqui vive num directório temporário, e o
    ``__exit__`` apaga-o. A excepção ao que apaga é o daemon, que é um
    processo externo: mata-se, e o que ele deixou é o directório, que é
    descartável por definição.

    Os três membros — ``daemon``, ``servidor`` e ``loja`` — existem
    porque a ordem de desligamento importa e não é a ordem de escrita:

    1. a janela do WebKit fecha, porque é ela que faz pedidos;
    2. o sidecar deixa de servir;
    3. o daemon morre;
    4. a loja fecha, ou o ficheiro fica bloqueado para a próxima vez.
    """

    def __init__(self, *, modo: str, cenario: str, porta: int | None) -> None:
        self.modo = modo
        self.cenario = cenario

        # O ``--real`` fica preso à porta do ``bridge-adapter``; o
        # ``--demo`` não fala com o sidecar por JavaScript e pode ficar
        # onde calhar, o que permite duas avaliações em paralelo.
        if porta is not None:
            self.porta = porta
        elif modo == "real":
            self.porta = PORTA_DO_BRIDGE
        else:
            self.porta = porta_livre()

        self.directorio = Path(tempfile.mkdtemp(prefix="onyxchat-visual-"))
        self.daemon: subprocess.Popen[bytes] | None = None
        self.servidor: Any = None
        self.loja: Any = None
        self.conta: Any = None

    # -----------------------------------------------------------------
    # Paths
    # -----------------------------------------------------------------

    @property
    def caminho_conta(self) -> Path:
        return self.directorio / "conta.keystore"

    @property
    def caminho_config(self) -> Path:
        return self.directorio / "config.json"

    @property
    def caminho_loja(self) -> Path:
        return self.directorio / "loja.db"

    @property
    def caminho_socket(self) -> Path:
        return self.directorio / "onyxchat.sock"

    # -----------------------------------------------------------------
    # Ambiente
    # -----------------------------------------------------------------

    def preparar_ambiente(self) -> None:
        """Aponta a conta, a configuração e o socket para o temporário.

        As três variáveis são lidas por `messenger.conta.caminho_conta` e
        `user.config.caminho_config` **no momento da chamada**, e não no
        momento da importação — por isso as definir aqui, antes de usar
        qualquer uma das duas, é o que faz a conta ir para o sítio
        descartável. Um `setdefault` depois da importação não chegaria,
        e a falha seria uma conta criada no sítio real.

        :returns: nada.
        """
        os.environ["ONYX_CONTA"] = str(self.caminho_conta)
        os.environ["ONYXCHAT_CONFIG"] = str(self.caminho_config)
        os.environ["ONYXCHAT_SOCKET"] = str(self.caminho_socket)
        # O Tor é desactivado: avaliar a interface não precisa de Tor, e
        # o `TorFalso` é o que `tests/test_e2e_daemon.py` usa pelo mesmo
        # motivo — a rede não é o que se está a medir.
        os.environ["ONYXCHAT_TOR"] = "nenhum"

    # -----------------------------------------------------------------
    # O daemon
    # -----------------------------------------------------------------

    def arrancar_daemon(self) -> bool:
        """Arranca o ``onyxchatd`` em modo sem Tor.

        O binário que se procura é o de *debug*, e a ausência dele é um
        erro dito com as instruções para o construir, não um
        ``FileNotFoundError``: quem chega aqui quer avaliar a interface,
        e um traceback sobre um caminho não lhe diz o que fazer.

        :returns: ``True`` se o daemon ficou a correr.
        """
        if not DAEMON.exists():
            print(
                f"onyxchat: falta o daemon em {DAEMON}\n"
                "       constrói-o com: cargo build --no-default-features",
                file=sys.stderr,
            )
            return False

        self.daemon = subprocess.Popen(
            [
                str(DAEMON),
                "--socket", str(self.caminho_socket),
                "--tor", "nenhum",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        # O daemon não avisa quando está pronto; o que se faz é esperar
        # que o socket apareça. Sem `--tor` nem `--socket` ele usaria o
        # socket de sempre, que é o de quem está a usar o OnyxChat a sério.
        for _ in range(100):
            if self.caminho_socket.exists():
                return True
            if self.daemon.poll() is not None:
                print(
                    "onyxchat: o daemon saiu logo; o socket não apareceu",
                    file=sys.stderr,
                )
                return False
            time.sleep(0.1)

        print("onyxchat: o daemon não criou o socket a tempo", file=sys.stderr)
        return False

    # -----------------------------------------------------------------
    # A conta e a loja
    # -----------------------------------------------------------------

    def criar_conta(self) -> bool:
        """Cria a conta descartável.

        Em ``--demo`` não há conta nenhuma: é o que distingue a
        demonstração do backend verdadeiro, e criar uma para a
        demonstração seria mentir sobre o que está a ser avaliado.

        No cenário ``bloqueada`` a conta **é criada e de seguida
        descartada**, e é aqui que isso acontece. A conta não se apaga:
        deixa de ser entregue ao sidecar, que é como a conta trancada se
        manifesta do ponto de vista de quem chama. Apagar o ficheiro
        seria um estado diferente — o de quem ainda não tem conta — e a
        diferença entre «não tens conta» e «a tua conta está trancada»
        é uma das coisas que a interface tem de mostrar.

        :returns: ``True`` se a conta ficou criada, ou se não era preciso.
        """
        if self.modo == "demo":
            return True

        from messenger import conta as conta_mod

        conta = conta_mod.criar(
            UTILIZADOR_DESCARTAVEL,
            FRASE_DESCARTAVEL,
            self.caminho_conta,
        )

        if self.cenario == "bloqueada":
            self.conta = None
            return True

        self.conta = conta
        return True

    def abrir_loja(self) -> bool:
        """Abre a loja e semeia o cenário.

        A loja é sempre criada, mesmo em ``--demo``. Não é preciso para
        a demonstração — o ``mock-adapter`` não fala com ela — mas é o
        que garante que ``--demo`` e ``--real`` não difiram em mais nada
        além do que se quer comparar.

        :returns: ``True`` se a loja ficou aberta.
        """
        from server import loja as loja_mod

        self.loja = loja_mod.Loja(self.caminho_loja)
        semear(self.loja, self.cenario)
        return True

    # -----------------------------------------------------------------
    # O sidecar
    # -----------------------------------------------------------------

    def arrancar_sidecar(self) -> bool:
        """Arranca o sidecar a servir ``UI/dist`` e a API.

        Usa `server.sidecar.criar_servidor` e não `executar`, pela razão
        que está no cabeçalho do ficheiro: ``executar`` não devolve nada
        e o harness precisa de desligar a pilha com ordem.

        :returns: ``True`` se o sidecar ficou a servir.
        """
        from server import sidecar as sidecar_mod

        self.servidor = sidecar_mod.criar_servidor(
            self.loja,
            self.conta,
            sidecar_mod.aplicacao(self.loja, self.conta),
            porta=self.porta,
            raiz_ui=RAIZ_UI,
            caminho_conta=self.caminho_conta,
            silencioso=True,
        )

        fio = threading.Thread(
            target=self.servidor.serve_forever,
            name="sidecar",
            daemon=True,
        )
        fio.start()

        base = f"http://127.0.0.1:{self.porta}"
        return esperar_ate_resposta(base + "/api/estado")

    # -----------------------------------------------------------------
    # A janela
    # -----------------------------------------------------------------

    def correr_janela(self, argumentos: argparse.Namespace) -> int:
        """Lança o driver e espera que a janela termine.

        O ``gjs`` corre como subprocesso e não em thread porque é um
        programa com ciclo de eventos próprio: pô-lo dentro de uma thread
        a correr em paralelo seria gerir dois ciclos de eventos ao mesmo
        tempo, e o ``GLib.MainLoop`` do driver tem de ser o do processo.

        **O ``SIGTERM`` é tratado como o ``Ctrl-C``.** Sem isso, um
        ``kill`` — ou um ``timeout`` do próprio harness — matava o
        processo Python a meio e o ``finally`` nunca corria: o daemon
        ficava vivo e o directório temporário ficava no disco com a
        ``loja.db`` dentro. Como ``Pilha.parar`` apaga esse directório, o
        sintoma é um directório órfão que ninguém sabe de onde veio, e é
        por isso que o sinal é traduzido em vez de ignorado.

        A propagação do sinal ao driver é o inverso disso: o teclado é do
        utilizador e o sinal é do terminal, e o harness não pode ficar
        entre os dois a engolir o sinal sem o restaurar.

        :param argumentos: a linha de comando já analisada.
        :returns: o código de saída do driver.
        """
        ambiente = _ambiente_limpo(os.environ)
        ambiente["ONYX_VISUAL_BASE"] = f"http://127.0.0.1:{self.porta}"
        ambiente["ONYX_VISUAL_ECRAS"] = json.dumps(ecras_do_projeto())
        ambiente["ONYX_VISUAL_MOCK"] = "1" if self.modo == "demo" else "0"
        ambiente["ONYX_VISUAL_GUARDAR"] = str(argumentos.guardar or "")
        ambiente["ONYX_VISUAL_CENARIO"] = self.cenario

        processo = subprocess.Popen(
            ["gjs", str(DRIVER)],
            env=ambiente,
        )

        def terminar(*_: Any) -> None:
            with contextlib.suppress(Exception):
                processo.terminate()

        anteriores = {
            sinal: signal.signal(sinal, terminar)
            for sinal in (signal.SIGINT, signal.SIGTERM)
        }

        try:
            return processo.wait()
        finally:
            for sinal, anterior in anteriores.items():
                signal.signal(sinal, anterior)

    # -----------------------------------------------------------------
    # Desligar
    # -----------------------------------------------------------------

    def parar(self) -> None:
        """Desliga tudo pela ordem inversa à de arranque."""
        if self.servidor is not None:
            with contextlib.suppress(Exception):
                self.servidor.shutdown()
            self.servidor = None

        if self.daemon is not None:
            with contextlib.suppress(Exception):
                self.daemon.terminate()
                self.daemon.wait(timeout=5)
            with contextlib.suppress(Exception):
                if self.daemon.poll() is None:
                    self.daemon.kill()
            self.daemon = None

        if self.loja is not None:
            with contextlib.suppress(Exception):
                self.loja.fechar()
            self.loja = None

        shutil.rmtree(self.directorio, ignore_errors=True)


# =====================================================================
# A semeadura
# =====================================================================


def _aceitar(loja: Any, identificador: str, nome: str, mensagem: str) -> str:
    """Pede a amizade e aceita-a, devolvendo o identificador do contacto.

    É o caminho que a aplicação percorre, e é o único: `Loja` não tem um
    «criar contacto». Passar por aqui em vez de por SQL directo não é
    estilo — é o que garante que um cenário semeado é um estado que o
    OnyxChat sabe atingir.

    :param loja: a loja a semear.
    :param identificador: o identificador do contacto distante.
    :param nome: o nome a mostrar.
    :param mensagem: a mensagem do pedido.
    :returns: o ``id`` interno do contacto criado.
    """
    pedido = loja.registar_pedido(identificador, nome, mensagem)
    resposta = loja.decidir_pedido(pedido["id"], "aceite")
    return resposta["contactoId"]


def _identificador_de(nome: str) -> str:
    """Um identificador com a forma que `messenger.identidade` produz.

    Derivado do nome e não tirado do acaso, para que duas execuções do
    mesmo cenário vejam os mesmos valores — o que faz a diferença entre
    duas capturas da mesma interface ser a interface e não o acaso.

    :param nome: o nome de quem é o contacto.
    :returns: um identificador no formato ``ONYX-XXXXXX-...#``.
    """
    import hashlib

    sal = hashlib.sha256(f"onyxchat-visual:{nome}".encode("utf-8")).hexdigest()
    return f"ONYX-{sal[:6].upper()}-{sal[6:10].upper()}#{sal[10:14].upper()}"


def semear(loja: Any, cenario: str) -> None:
    """Põe a loja no estado do cenário.

    :param loja: a loja a semear.
    :param cenario: um nome de :data:`CENARIOS`.
    :returns: nada.
    """
    if cenario == "vazio":
        return

    if cenario == "bloqueada":
        # Um cenário bloqueado é um cenário cheio sem a conta aberta: é
        # o que muda, e o que a interface tem de mostrar é a diferença.
        semear(loja, "cheio")
        return

    helena = _aceitar(
        loja,
        _identificador_de("Helena Braga"),
        "Helena Braga",
        "Ficheiro assinado, com as verificações em anexo.",
    )
    # `abrir_conversa` devolve a conversa já montada, não o `id`. É o
    # mesmo que `server/rotas.py` usa, e a diferença passava
    # despercebida até ao primeiro `sqlite3.ProgrammingError`.
    helena_id = loja.abrir_conversa(helena)["id"]
    loja.adicionar_mensagem(helena_id, "them", "Chegou o pacote.")
    loja.adicionar_mensagem(helena_id, "me", "Vi. Vou confirmar a assinatura.")

    if cenario == "uma-conversa":
        return

    restantes = (
        ("Rui Camacho", "Obrigada. Do meu lado fica marcado como verificado.", True),
        ("Inês Faria", "Obrigada!", False),
        ("Sofia Quintela", "Combinado.", False),
        ("Tomás Neves", "A rota por relay ainda não voltou.", True),
    )
    for nome, texto, nao_lida in restantes:
        contacto = _aceitar(
            loja, _identificador_de(nome), nome, "Pedido de amizade."
        )
        conversa_id = loja.abrir_conversa(contacto)["id"]
        loja.adicionar_mensagem(conversa_id, "them", texto)
        if nao_lida:
            # O número ao lado do nome é o estado, não o texto: deixar a
            # conversa por ler é o que faz o número aparecer, e é
            # precisamente o número que o cenário quer mostrar.
            continue
        loja.marcar_lida(conversa_id)

    if cenario == "cheio":
        for nome, mensagem in (
            ("Ana Matos", "Oi, é a Ana."),
            ("Bruno Reis", "Bom dia!"),
        ):
            loja.registar_pedido(_identificador_de(nome), nome, mensagem)


# =====================================================================
# A linha de comando
# =====================================================================


def construir_parser() -> argparse.ArgumentParser:
    """Monta o analisador da linha de comando."""
    analisador = argparse.ArgumentParser(
        prog="interface-visual.py",
        description=(
            "Monta a interface, o servidor e a janela interna para os "
            "avaliar à mão."
        ),
        epilog=(
            "Teclas na janela: 1-8 mudam de ecrã, d alterna entre "
            "demonstração e backend real, s guarda um PNG, q fecha."
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    grupo = analisador.add_mutually_exclusive_group()
    grupo.add_argument(
        "--demo",
        action="store_true",
        help="demonstração com dados fictícios (predefinição)",
    )
    grupo.add_argument(
        "--real",
        action="store_true",
        help="backend verdadeiro: daemon, conta e loja",
    )

    analisador.add_argument(
        "--cenario",
        choices=CENARIOS,
        default="vazio",
        help=(
            "conteúdo da loja em --real: "
            + ", ".join(CENARIOS)
            + " (predefinição: vazio)"
        ),
    )
    analisador.add_argument(
        "--porta",
        type=int,
        help=(
            "porta do sidecar (predefinição: 8787 em --real, porque o "
            "bridge-adapter tem-no fixo; uma livre em --demo)"
        ),
    )
    analisador.add_argument(
        "--guardar",
        metavar="DIRECTORIO",
        help="exporta um PNG de cada ecrã e sai, sem abrir a janela",
    )
    analisador.add_argument(
        "--construir",
        action="store_true",
        help="compila a interface antes de servir",
    )
    # `--ajuda` ao lado de `--help`: o projecto é português e um utilizador
    # que escreve `--ajuda` e recebe «unrecognized arguments» pensa que
    # o programa está partido, não que escreveu em inglês.
    analisador.add_argument(
        "--ajuda",
        action="help",
        help="mostra esta ajuda",
    )
    return analisador


def principal(argumentos: list[str] | None = None) -> int:
    """Ponto de entrada. Devolve o código de saída."""
    opcoes = construir_parser().parse_args(argumentos)
    modo = "real" if opcoes.real else "demo"

    if opcoes.construir:
        if not _construir_interface():
            return 1

    if not RAIZ_UI.is_dir():
        print(
            f"onyxchat: falta a interface compilada em {RAIZ_UI}\n"
            "       constrói-a com: UI/tools/onyxchat build",
            file=sys.stderr,
        )
        return 1

    pilha = Pilha(
        modo=modo,
        cenario=opcoes.cenario,
        porta=opcoes.porta,
    )
    pilha.preparar_ambiente()

    # Antes de arrancar nada: um `SIGTERM` a meio da montagem — um `kill`
    # do utilizador, um `timeout` — matava o processo sem passar pelo
    # `finally`, e o daemon ficava vivo com o directório temporário no
    # disco. Traduzir o sinal numa excepção é o que devolve o controlo ao
    # `finally`, que é quem sabe o que limpar.
    def ao_sinal(*_: Any) -> None:
        raise KeyboardInterrupt

    anteriores = {
        sinal: signal.signal(sinal, ao_sinal)
        for sinal in (signal.SIGINT, signal.SIGTERM)
    }

    if porta_ocupada(pilha.porta):
        print(
            f"onyxchat: a porta {pilha.porta} já está ocupada\n"
            + (
                "       o bridge-adapter fala com 127.0.0.1:8787 e essa porta "
                "é fixa;\n"
                "       fecha o OnyxChat que esteja a correr, ou passa "
                "--porta 8788\n"
                "       para avaliar só a demonstração."
                if modo == "real"
                else "       escolhe outra com --porta."
            ),
            file=sys.stderr,
        )
        pilha.parar()
        return 1

    try:
        if modo == "real":
            print("onyxchat: a arrancar o daemon")
            if not pilha.arrancar_daemon():
                return 1

        pilha.criar_conta()

        print(f"onyxchat: a abrir a loja ({opcoes.cenario})")
        pilha.abrir_loja()

        print(f"onyxchat: a servir em http://127.0.0.1:{pilha.porta}")
        if not pilha.arrancar_sidecar():
            return 1

        ecras = ecras_do_projeto()
        print(f"onyxchat: {len(ecras)} ecrãs lidos de UI/src/App.jsx")
        return pilha.correr_janela(opcoes)
    except KeyboardInterrupt:
        print("onyxchat: interrompido")
        return 0
    finally:
        for sinal, anterior in anteriores.items():
            signal.signal(sinal, anterior)
        pilha.parar()


def _construir_interface() -> bool:
    """Compila a interface com a ferramenta do projecto.

    Não se chama o ``npm`` directamente porque `UI/tools/onyxchat` é o que
    escolhe o Node certo — o do projecto, quando existe, porque o do
    sistema pode ser antigo e o Vite falha com um erro que não menciona
    versões.

    :returns: ``True`` se a compilação passou.
    """
    ferramenta = RAIZ / "UI" / "tools" / "onyxchat"
    if not ferramenta.exists():
        print(
            f"onyxchat: falta {ferramenta}; não há como compilar a interface",
            file=sys.stderr,
        )
        return False

    return subprocess.call([str(ferramenta), "build"]) == 0


if __name__ == "__main__":
    raise SystemExit(principal())
