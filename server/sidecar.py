# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""sidecar.py — a ponte entre o browser e o que é verdade.

O sidecar é um servidor HTTP em ``127.0.0.1`` que faz duas coisas:
serve a interface já construída e fala com o ``messenger``, que é onde
o ``onyxchatd`` é alcançado. É a peça que faltava entre um browser e um
socket Unix binário.

## A regra que rege todas as rotas

**Cada rota devolve um estado do mundo, não uma resposta improvisada.**

A alternativa — devolver `[]`, devolver `{}`, devolver um exemplo —
transforma «não há conversas» numa frase verdadeira quando o que é
verdade é «não há fonte de dados». São mensagens diferentes e a pessoa
age de forma diferente em cada uma. Por isso:

- o que **não existe** é `404` com o caminho;
- o que **existe mas está indisponível** é `503`;
- o que **falhou a sério** é `500` com um código, sem a excepção.

E nenhuma rota inventa um valor. A identidade que o daemon não devolveu
não é uma identidade com campos a zero: é um `503`.

## A verificação de origem vem antes de tudo

``server/origem.py`` decide, e a decisão é tomada antes do routing. Um
servidor local sem essa verificação é legível por qualquer página da
Internet — a especificação permite o pedido e o browser não o impede.

Ver esse ficheiro para o porquê das quatro defesas. O que importa aqui é
que a verificação acontece **antes** de qualquer ``self.path`` ser lido
para decidir o que fazer: se o código lê o caminho primeiro e decide
depois, uma rota mal escrita deixa de estar protegida.

## As rotas

São as de ``UI/src/lib/onyx/bridge-adapter.js``, uma a uma. A fachada
``data-adapter.js`` é o contrato, e escrevê-lo aqui como uma tabela é o
que garante que nenhuma rota existe sem que a interface a peça, nem é
pedida sem existir.

## Static files

O ``dist/`` é servido com três cuidados:

1. **O caminho é resolvido e confirmado dentro de ``dist/``.** Um
   ``../`` a mais resolveria para fora da directoria. Isto é verificado
   depois de resolver, não antes: normalizar antes deixa passar
   ``..%2f..%2f``, que o browser descodifica depois.
2. **Sem listagem de directório.** Um directório sem ``index.html``
   responde ``404``, e a lista de ficheiros do build não interessa a
   ninguém.
3. **Os ficheiros são resolvidos para dentro de ``dist/`` com
   ``realpath``**, e o ``realpath`` do directório base é calculado uma
   vez no arranque. Sem isto, um link simbólico dentro de ``dist/``
   apontaria para fora.

## Onde não escutar

Só em ``127.0.0.1``. O endereço por omissão é um argumento, e
:func:`executar` recusa explicitamente um endereço que não seja de
loopback. Um sidecar que escute em ``0.0.0.0`` está a dar a identidade a
toda a rede — e é a alteração mais pequena deste ficheiro com as
consequências mais graves.
"""

from __future__ import annotations

import json
import mimetypes
import os
import posixpath
import traceback
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Callable
from urllib.parse import parse_qs, unquote, urlsplit

from messenger import conta as conta_mod
from server import loja as loja_mod
from server.origem import avaliar

__all__ = [
    "ERROS",
    "Rota",
    "ServicoIndisponivel",
    "HOST_OMISSAO",
    "Manipulador",
    "Sidecar",
    "aplicacao",
    "criar_servidor",
    "e_loopback",
    "executar",
]

#: Raiz da interface construída. Existe a partir de ``npm run build``.
RAIZ_UI = Path(__file__).resolve().parent.parent / "UI" / "dist"

#: O endereço de escuta por omissão.
#:
#: Loopback, e não ``0.0.0.0``. Um sidecar que escute em todas as
#: interfaces está a dar a identidade e a cópia de segurança a toda a
#: rede local, e a verificação de :mod:`server.origem` deixa de proteger
#: alguma coisa — passaria a proteger um serviço alcançável de fora.
#:
#: Ver :func:`e_loopback`, que recusa qualquer outro.
HOST_OMISSAO = "127.0.0.1"

#: O maior corpo que se aceita ler.
#:
#: O valor é generoso para uma mensagem e pequeno para não ser um
#: ``POST`` de exaustão de memória. Uma mensagem de conversa são KB; um
#: corpo de 4 MB é alguém a tentar, e recusa-lo é mais barato do que o
#: ler.
MAX_CORPO = 4 * 1024 * 1024

#: Quanto se descarta antes de fechar a ligação a recusar um corpo grande.
#:
#: É o próprio :data:`MAX_CORPO`, e não um número mais pequeno. Um corpo
#: de cinco megabytes tem de ser lido até ao fim para que o cliente
#: consiga ler a resposta `413`; se o servidor fechar a meio, o cliente
#: recebe uma ligação quebrada e nunca sabe porque foi recusado.
#:
#: O tecto continua a existir porque `Content-Length` é do cliente: um
#: que anuncie um gigabyte não pode fazer o servidor esperar por um
#: gigabyte para depois dizer que não. Esse caso fecha a ligação, e é o
#: que se quer.
_POUCA_DRENAGEM = MAX_CORPO


class ServicoIndisponivel(RuntimeError):
    """O que está do outro lado não respondeu.

    Separado de qualquer outra excepção porque a resposta é outra: `503`
    com «o programa local não respondeu» diz à pessoa que pode
    reiniciar o programa, e `500` com um trace diz-lhe que há um erro.
    A pessoa sabe o que fazer em cada um.
    """


#: Os erros, por código. A interface traduz a chave; o corpo traz o
#: código e a mensagem em português.
#:
#: Não há detalhe técnico no corpo. Um atacante que descubra a porta tem
#: uma lista de mensagens de excepção que o ajuda a mapear o programa.
ERROS: dict[int, str] = {
    400: "pedido malformado",
    403: "origem não permitida",
    404: "não encontrado",
    405: "método não permitido",
    413: "corpo demasiado grande",
    501: "não implementado",
    415: "tipo de conteúdo não suportado",
    500: "erro interno",
    503: "o programa local não está disponível",
}


@dataclass
class Rota:
    """Uma rota: método, caminho e o que faz.

    ``caminho`` é o que vem depois de ``/api`` e sem a barra. Os
    parâmetros de caminho usam a forma ``{nome}`` e são preenchidos a
    partir dos segmentos.

    Vem de :mod:`server.rotas` quando há; aqui só o tipo, para que as
    dependências fiquem numa só ordem.
    """

    metodo: str
    caminho: str
    accao: Callable[["Contexto", dict[str, str]], Any]
    #: Só para os registos. Um nome é melhor do que ``<lambda>`` quando
    #: se procura porque é que uma rota devolveu 500.
    nome: str = ""

    def casa(self, metodo: str, caminho: str) -> dict[str, str] | None:
        """Os parâmetros se este método e caminho casam, senão ``None``.

        O casamento é exacto por segmento, o que evita o problema de
        uma expressão regular mal escrita aceitar o que não deve. Não há
        wildcards: se uma rota precisa de um prefixo, isso é outra rota.
        """
        if metodo.upper() != self.metodo.upper():
            return None

        meus = self.caminho.strip("/").split("/") if self.caminho.strip("/") else []
        seus = caminho.strip("/").split("/") if caminho.strip("/") else []

        if len(meus) != len(seus):
            return None

        parametros: dict[str, str] = {}
        for meu, seu in zip(meus, seus):
            if meu.startswith("{") and meu.endswith("}"):
                parametros[meu[1:-1]] = unquote(seu)
            elif meu != seu:
                return None

        return parametros


@dataclass
class Contexto:
    """O que uma rota recebe.

    Só tem o que a rota precisa. Uma rota que recebe a ligação inteira
    pode escrever cabeçalhos, e uma rota que escreve cabeçalhos é uma
    rota que pode decidir servir o que quiser a quem não tem o direito.
    """

    caminho: str
    consulta: dict[str, list[str]]
    corpo: bytes
    loja: loja_mod.Loja
    #: Preenchido por rotas de conta, que precisam da conta aberta.
    conta: conta_mod.Conta | None = None
    extras: dict[str, Any] = field(default_factory=dict)

    def um(self, chave: str, por_omissao: str = "") -> str:
        """O primeiro valor de um parâmetro, ou o valor por omissão."""
        valores = self.consulta.get(chave)
        return valores[0] if valores else por_omissao

    def flag(self, chave: str) -> bool:
        """Um parâmetro booleano, ao estilo do que a interface manda."""
        bruto = self.um(chave, "").lower()
        return bruto in ("1", "true", "sim", "verdade")

    def json(self) -> dict[str, Any]:
        """O corpo já convertido, e recusado se não for um objecto.

        :raises ServicoIndisponivel: com código 400, porque é o que o
            chamador tem de tratar como pedido malformado.
        """
        if not self.corpo:
            return {}
        try:
            dados = json.loads(self.corpo)
        except (UnicodeDecodeError, json.JSONDecodeError) as erro:
            raise ErroDePedido(400, f"JSON inválido: {erro}") from erro
        if not isinstance(dados, dict):
            raise ErroDePedido(400, "o corpo tem de ser um objecto JSON")
        return dados

    def texto(self, chave: str, obrigatorio: bool = True) -> str:
        """Um campo de texto do corpo.

        A verificação de comprimento vive aqui e não em cada rota: uma
        rota que recebe um texto tem de recusar um texto de um
        megabyte, e escrevê-lo em todas as rotas é como uma das rotas
        fica sem ele.
        """
        valor = self.json().get(chave)
        if not isinstance(valor, str) or not valor:
            if obrigatorio:
                raise ErroDePedido(400, f"falta o campo «{chave}»")
            return ""
        if len(valor) > 64 * 1024:
            raise ErroDePedido(413, f"«{chave}» é demasiado grande")
        return valor


class ErroDePedido(Exception):
    """Um erro com o código HTTP que lhe corresponde."""

    def __init__(self, codigo: int, mensagem: str = "") -> None:
        super().__init__(mensagem or ERROS.get(codigo, "erro"))
        self.codigo = codigo
        self.mensagem = mensagem or ERROS.get(codigo, "erro")


@dataclass
class Sidecar:
    """O estado do sidecar: a loja, a conta e as rotas.

    Vive separado do manipulador HTTP porque os dois têm ciclos de vida
    diferentes. A loja e a conta existem enquanto o sidecar corre; um
    ``Handler`` é criado e destruído a cada pedido. Juntá-los dava um
    objecto cujo estado de fundo mudava de instância a cada pedido, e
    uma rota a receber ``self`` era uma rota com acesso a tudo.

    Mantê-los separados também é o que torna uma rota testável sem um
    servidor: ela recebe a loja e a conta, e nada mais.
    """

    loja: loja_mod.Loja
    conta: conta_mod.Conta | None = None
    rotas: list[Rota] = field(default_factory=list)
    raiz_ui: Path = RAIZ_UI
    porta: int = 8787
    #: Onde está o ficheiro de conta.
    #:
    #: Vive aqui, e não é lido por cada rota a partir de
    #: `conta.caminho_conta()`, porque as rotas de conta precisam dele e o
    #: `Sidecar` é o estado que sobrevive aos pedidos. Chamá-lo a cada
    #: rota faria cada uma delas repetir o mesmo acesso, e um dia uma
    #: delas usaria um caminho diferente sem que nada notasse.
    caminho_conta: Path | None = None


class Manipulador(BaseHTTPRequestHandler):
    """Cada pedido HTTP.

    :class:`~http.server.BaseHTTPRequestHandler` é o que a biblioteca
    padrão traz. O que não traz é cuidado nenhum, e a parte que importa
    aqui é que escuta em todas as interfaces por omissão — daí
    :func:`executar` recusar um endereço que não seja de loopback.
    """

    #: O estado vive em ``self.server.sidecar``, não no ``Handler``.
    #:
    #: Um ``Handler`` é criado e destruído a cada pedido; o estado de
    #: fundo tem de sobreviver a todos eles. Posto como atributo de
    #: instância seria posto pelo mesmo `Handler` que o vai ler — e o
    #: `Handler` seguinte não o encontraria. É o servidor que o carrega.
    @property
    def sidecar(self) -> Sidecar:
        return self.server.sidecar  # type: ignore[attr-defined]

    # -- configuração de rede -----------------------------------------
    server_version = "onyxchat-sidecar"
    sys_version = ""
    protocol_version = "HTTP/1.1"

    # -----------------------------------------------------------------
    # O que toda a gente chama
    # -----------------------------------------------------------------

    def do_GET(self) -> None:  # noqa: N802 — nome imposto pela base
        self._atender("GET")

    def do_HEAD(self) -> None:  # noqa: N802
        self._atender("HEAD")

    def do_POST(self) -> None:  # noqa: N802
        self._atender("POST")

    def do_PUT(self) -> None:  # noqa: N802
        self._atender("PUT")

    def do_PATCH(self) -> None:  # noqa: N802
        self._atender("PATCH")

    def do_DELETE(self) -> None:  # noqa: N802
        self._atender("DELETE")

    def do_OPTIONS(self) -> None:  # noqa: N802
        """Pré-voo.

        Responde com o que o browser precisa para decidir se pode fazer o
        pedido. Sendo a aplicação da mesma origem, quase nunca é pedido —
        mas um ``fetch`` de outra origem pergunta antes de tentar, e a
        resposta tem de ser rápida e correcta mesmo quando o pedido
        depois é recusado por :mod:`server.origem`.
        """
        self._atender("OPTIONS")

    # -----------------------------------------------------------------
    # O caminho de todos os pedidos
    # -----------------------------------------------------------------

    def _atender(self, metodo: str) -> None:
        """Da entrada à resposta, para qualquer método.

        A ordem é a mesma para todos, e nenhuma rota a pode alterar:

        1. **Origem.** Antes de ler o caminho. Uma rota mal escrita não
           deixa de estar protegida.
        2. **Rota.** `/api/…` é uma rota; o resto é um ficheiro de `dist/`.
        3. **Erro.** Cada erro sai por :meth:`_falhar`, que é o único
           sítio que escreve uma resposta de erro.
        """
        decisao = avaliar(metodo, dict(self.headers.items()), self.server.server_port)  # type: ignore[attr-defined]
        if not decisao:
            self._falhar(decisao.codigo)
            return

        partes = urlsplit(self.path)
        caminho = unquote(partes.path)

        try:
            if caminho == "/api/eventos" and metodo == "GET":
                self._atender_eventos()
                return

            if caminho.startswith("/api"):
                self._atender_api(metodo, caminho, parse_qs(partes.query))
                return

            self._servir_estatico(metodo, caminho)
        except ErroDePedido as erro:
            self._falhar(erro.codigo, erro.mensagem)
        except ServicoIndisponivel:
            self._falhar(503)
        except Exception:  # noqa: BLE001 — a nota está no topo do ficheiro
            # O traceback vai para o registo e **nunca** para a resposta.
            #
            # A primeira versão escrevia só «excepção não tratada em GET
            # /api/…», que é o mesmo que não escrever nada: quem depura
            # fica a saber que houve uma excepção e nada mais. Descobrir
            # o motivo passa a exigir um `print` no código, o que é um
            # ciclo de trabalho infinito para um erro de um carácter.
            self._log(
                "excepção não tratada em %s %s\n%s",
                metodo,
                caminho,
                traceback.format_exc(),
            )
            self._falhar(500)

    # -----------------------------------------------------------------
    # A API
    # -----------------------------------------------------------------

    def _atender_api(self, metodo: str, caminho: str, consulta: dict[str, list[str]]) -> None:
        """Encaminha `/api/…` para a rota."""
        app = self.sidecar

        if metodo == "OPTIONS":
            # O pré-voo é do **protocolo**, não da aplicação: nenhum
            # router responde a ele, e responder com 404 diria à pessoa
            # que o caminho não existe. O que o browser quer saber é se
            # pode mandar o pedido — e a resposta é o que a página
            # resposta é o que decide se pode, por isso basta dizer que sim.
            self.send_response(204)
            self.send_header("Allow", "GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS")
            self.send_header("Content-Length", "0")
            self._cabecalhos_de_seguranca()
            self.end_headers()
            return
        resto = caminho[len("/api") :]

        corpo = b""
        if metodo in ("POST", "PUT", "PATCH"):
            corpo = self._ler_corpo()

        for rota in app.rotas:
            parametros = rota.casa(metodo, resto)
            if parametros is None:
                continue

            contexto = Contexto(
                caminho=resto,
                consulta=consulta,
                corpo=corpo,
                loja=app.loja,
                conta=app.conta,
                extras={
                    # O `Sidecar` vem no `extras` e não como campo, porque
                    # quem o põe é a rota que **muda** a sessão — e mudar a
                    # sessão é exactamente o que uma rota de conta faz. Um
                    # campo próprio só de leitura não chegaria.
                    "sidecar": app,
                    "caminho_conta": app.caminho_conta,
                },
            )
            resultado = rota.accao(contexto, parametros)
            self._json(200, resultado if resultado is not None else {})
            return

        self._falhar(404)

    def _atender_eventos(self) -> None:
        """O canal de eventos, que **ainda não existe**.

        Devolve ``501`` com a razão. Não devolve um WebSocket a meio, nem
        um ``404`` que não diz nada.

        O plano desta Etapa previa «HTTP + WebSocket», e a parte do
        WebSocket ficou por fazer de propósito — ver ``docs/sidecar.md``
        §O canal de eventos, onde a decisão está escrita com a
        razão. Resumo: as vinte e uma funções do contrato em
        ``bridge-adapter.js`` são todas pedido/resposta, e não há
        consumidor para um canal de empurrar eventos. Uma implementação
        de RFC 6455 feita à mão, sem consumidor, seria exactamente o
        mecanismo especulativo que o resto deste projecto recusa.
        """
        self._falhar(
            501,
            "o canal de eventos não está implementado; "
            "todas as rotas de dados são pedido/resposta",
        )

    # -----------------------------------------------------------------
    # Ficheiros
    # -----------------------------------------------------------------

    def _servir_estatico(self, metodo: str, caminho: str) -> None:
        """Serve ``dist/``, ou devolve ``404``.

        A resolução é feita e **depois** confirmada: normalizar o caminho
        antes de resolver deixa passar ``..%2f..%2f``, que o browser
        descodifica depois de o servidor o ter dado por bom.
        """
        if metodo not in ("GET", "HEAD"):
            self._falhar(405)
            return

        raiz = self.sidecar.raiz_ui.resolve()

        # O caminho já vem descodificado, e essa é a armadilha.
        #
        # `..%2f..%2fetc%2fpasswd` chega como uma só peça e o browser
        # descodifica-a para `../../etc/passwd`. Normalizar **antes** de
        # resolver não resolve nada: normalizar trata a barra como
        # separador, e aqui a fuga só se vê depois de descodificar.
        #
        # A regra é: `..` no caminho descodificado é recusado, sem
        # discussão. Nenhum ficheiro de uma aplicação construída tem
        # `..` no nome, e recusar não custa nada a ninguém.
        if ".." in caminho.split("/"):
            self._falhar(404)
            return

        # Um caminho de SPA: `/contactos` tem de servir `index.html`, e
        # não 404, porque é assim que uma aplicação de página única
        # funciona sem saber que o browser não recarrega.
        #
        # A regra é sobre o **nome do ficheiro**, e não sobre o caminho
        # inteiro: uma versão anterior testava `"." in caminho`, o que
        # mandava `/assets/app.min.js` para o `index.html` e devolvia o
        # index a um pedido de JavaScript.
        relativo = caminho.lstrip("/") or "index.html"
        if "." not in posixpath.basename(relativo):
            relativo = "index.html"

        alvo = (raiz / relativo).resolve()
        # A confirmação final é sobre o caminho **resolvido**, e é o que
        # apanha um link simbólico dentro de `dist/` que aponte para fora.
        if not alvo.is_file() or raiz not in alvo.parents:
            self._falhar(404)
            return

        tipo, _ = mimetypes.guess_type(str(alvo))
        dados = alvo.read_bytes()

        self.send_response(200)
        self.send_header("Content-Type", tipo or "application/octet-stream")
        self.send_header("Content-Length", str(len(dados)))
        self.send_header("X-Content-Type-Options", "nosniff")
        # A interface é uma aplicação: o seu estado vive no servidor, não
        # no browser. Um `frame-ancestors 'self'` fecha o *clickjacking*,
        # que num servidor local é a iframe de um atacante por cima do
        # formulário de desbloqueio.
        self.send_header("Content-Security-Policy", "default-src 'self'; frame-ancestors 'self'")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        if metodo != "HEAD":
            self.wfile.write(dados)

    # -----------------------------------------------------------------
    # Respostas
    # -----------------------------------------------------------------

    def _ler_corpo(self) -> bytes:
        """Lê o corpo, e recusa o que é grande demais.

        O ``Content-Length`` é conferido **antes** de ler, e não depois:
        ler primeiro e verificar o tamanho depois é a forma de gastar a
        memória antes de ter decidido que não se devia.
        """
        bruto = self.headers.get("Content-Length")
        if bruto is None:
            return b""

        try:
            tamanho = int(bruto)
        except ValueError as erro:
            raise ErroDePedido(400, "Content-Length inválido") from erro

        if tamanho < 0:
            raise ErroDePedido(400, "Content-Length negativo")
        if tamanho > MAX_CORPO:
            # Esvazia-se o corpo antes de responder.
            #
            # Sem isto, o servidor responde `413` e fecha a ligação
            # enquanto o cliente ainda está a enviar cinco megabytes: o
            # cliente recebe uma ligação quebrada em vez da resposta, e
            # a mensagem que o informaria de que o pedido era grande
            # demais nunca chega a ser lida.
            #
            # Esvaziar não é ler para a memória — vai-se ao dejeto, que é o
            # que a ligação faz quando não se está a responder.
            self._esvaziar(tamanho)
            raise ErroDePedido(413)

        return self.rfile.read(tamanho)

    def handle_expect_100(self) -> bool:
        """Aceita ou recusa o corpo antes de ele ser enviado.

        Este é o sítio certo, e a primeira versão
        verificava o tamanho dentro de
        :meth:`_ler_corpo` — tarde demais. A classe base manda
        automaticamente `100 Continue` **antes** de chamar `do_POST`, de
        modo que o cliente já estava a mandar os cinco megabytes quando a
        verificação corria.

        Devolver ``True`` faz a base mandar o `100`. Devolver ``False``
        manda o `413` e fecha, que é a resposta ao `Expect` que o cliente
        ESPERA — a especificação diz que um servidor recusa o corpo
        assim, e não que interrompe a meio.

        Quando o corpo é aceitável, deixa-se a base fazer o que sabe.
        """
        if bruto_tamanho(self.headers) > MAX_CORPO:
            self._falhar(413)
            self.close_connection = True
            return False
        return super().handle_expect_100()

    def _esvaziar(self, tamanho: int) -> None:
        """Descarta o corpo que o cliente já disse que ia enviar.

        Esvazia-se até um tecto e corta-se a ligação a seguir. Esvaziar sem
        limite seria pior: um cliente que anuncie um gigabyte
        transformaria o `413` numa espera de um gigabyte.
        """
        restante = min(tamanho, _POUCA_DRENAGEM)
        lidos = 0
        while lidos < restante:
            pedaco = self.rfile.read(min(65536, restante - lidos))
            if not pedaco:
                return
            lidos += len(pedaco)
        self.close_connection = True

    def _json(self, codigo: int, dados: Any) -> None:
        """Responde em JSON."""
        corpo = json.dumps(dados, ensure_ascii=False).encode("utf-8")
        self.send_response(codigo)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(corpo)))
        self.send_header("Cache-Control", "no-store")
        self._cabecalhos_de_seguranca()
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(corpo)

    def _falhar(self, codigo: int, mensagem: str | None = None) -> None:
        """Responde com um erro, e é o único sítio que o faz."""
        self._json(codigo, {
            "erro": ERROS.get(codigo, "erro"),
            "codigo": codigo,
            "detalhe": mensagem or "",
        })

    def _cabecalhos_de_seguranca(self) -> None:
        """Os cabeçalhos que valem em todas as respostas."""
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("X-Frame-Options", "DENY")

    def _log(self, formato: str, *argumentos: object) -> None:
        """Para os registos, e nunca para a resposta.

        A excepção que apanha o ``500`` vai para aqui e não para o corpo:
        um traceback na resposta é um mapa do programa para quem deu de
        encontrar a porta.
        """
        self.log_message(formato, *argumentos)

    def log_message(self, formato: str, *argumentos: object) -> None:
        """Silencia o registo do servidor durante os testes.

        Não é por serem testes que não se quer ver o que aconteceu — os
        registos continuam a estar em `self._log`. É que o
        ``log_message`` da base escreve em `stderr` e um teste que passa
        a inundar o terminal de pedidos é um teste cuja falha se perde no
        ruído.
        """
        if getattr(self.server, "silencioso", False):  # type: ignore[attr-defined]
            return
        super().log_message(formato, *argumentos)


def bruto_tamanho(cabecalhos: Any) -> int:
    """O `Content-Length` como inteiro, e ``0`` quando não há ou é lixo.

    Existe para o `Expect: 100-continue` decidir **antes** de a leitura
    começar, e para não repetir a conversão — e a excepção de conversão
    — em dois sítios.
    """
    bruto = cabecalhos.get("Content-Length") if cabecalhos else None
    if bruto is None:
        return 0
    try:
        return max(0, int(bruto))
    except ValueError:
        # Um `Content-Length` que não é número será tratado como `400`
        # mais abaixo; aqui só interessa não recusar por ser grande.
        return 0


class _Servidor(ThreadingHTTPServer):
    """O servidor, com o que a base não sabe."""

    daemon_threads = True
    allow_reuse_address = True


def criar_servidor(
    loja: loja_mod.Loja,
    conta: conta_mod.Conta | None,
    rotas: list[Rota],
    *,
    porta: int = 8787,
    host: str = HOST_OMISSAO,
    raiz_ui: Path | None = None,
    caminho_conta: Path | None = None,
    silencioso: bool = False,
) -> _Servidor:
    """Cria o servidor, sem o arrancar.

    Separar a criação do arranque é o que permite a um teste ter um
    servidor numa porta livre e falar com ele, em vez de adivinhar uma
    porta e rezar para que esteja livre.

    :raises ValueError: se ``host`` não for de loopback. Um sidecar em
        ``0.0.0.0`` está a dar a identidade a toda a rede, e é a
        alteração mais pequena deste ficheiro com as consequências mais
        graves.
    """
    if not e_loopback(host):
        raise ValueError(
            f"o sidecar só escuta em loopback; {host!r} não é. "
            "Isto não é um limite arbitrário: ver o topo deste ficheiro."
        )

    app = Sidecar(
        loja=loja,
        conta=conta,
        rotas=rotas,
        raiz_ui=raiz_ui if raiz_ui is not None else RAIZ_UI,
        porta=porta,
        caminho_conta=caminho_conta,
    )

    servidor = _Servidor((host, porta), Manipulador)
    servidor.sidecar = app  # type: ignore[attr-defined]
    servidor.silencioso = silencioso  # type: ignore[attr-defined]
    return servidor


def e_loopback(host: str) -> bool:
    """O endereço é de loopback?

    A pergunta é feita com `socket`, e não com uma lista escrita à mão,
    porque a lista não acompanha o que o sistema aceita e uma excepção
    não o diz.
    """
    import ipaddress

    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return host == "localhost"


def aplicacao(loja: loja_mod.Loja, conta: conta_mod.Conta | None = None) -> list[Rota]:
    """As rotas, montadas a partir da loja e da conta.

    Função separada de :func:`criar_servidor` porque é isto que os testes
    montam e desmontam, e porque as rotas dependem de estar abertas ou
    trancadas — o que a conta decide.
    """
    from server import rotas

    return rotas.montar(loja, conta)


def executar(
    porta: int = 8787,
    loja: loja_mod.Loja | None = None,
    conta: conta_mod.Conta | None = None,
    *,
    host: str = HOST_OMISSAO,
    raiz_ui: Path | None = None,
    caminho_conta: Path | None = None,
) -> None:
    """Corre o sidecar até ser interrompido.

    A loja é criada aqui quando não é passada, e fechada no fim. Criar
    uma loja e não a fechar deixa o ficheiro bloqueado para a próxima
    vez que o sidecar arranque, e o erro que aparece é «o ficheiro está
    em uso», que não aponta para lado nenhum.

    :param caminho_conta: onde está o ficheiro de conta. Vem do
        chamador porque é `messenger.conta.caminho_conta()` — e ficar a
        decidirmos aqui o caminho da conta daria dois sítios que dizem
        onde ela está, que é a forma de divergirem em silêncio.
    """
    aberta = loja or loja_mod.Loja()
    servidor = criar_servidor(
        aberta,
        conta,
        aplicacao(aberta, conta),
        porta=porta,
        host=host,
        raiz_ui=raiz_ui,
        caminho_conta=caminho_conta,
    )
    try:
        servidor.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        servidor.server_close()
        if loja is None:
            aberta.fechar()
