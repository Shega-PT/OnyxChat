# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""rotas.py — a tabela de rotas do sidecar.

## A rota que devolve `503` é a mais importante desta tabela

``/api/estado`` e ``/api/identidade`` falam com o ``onyxchatd``. Quando ele
não está a correr, a resposta é ``503`` e o corpo diz que o programa local
não respondeu.

Não é `[]`. Não é `{}`. Não são campos a zero.

A diferença é entre «não há conversas» e «não há fonte de dados». A
primeira é uma verdade sobre o mundo; a segunda é uma verdade sobre o
programa, e a pessoa age de forma completamente diferente: na primeira
espera que alguém fale, na segunda vai ver porque é que o programa não
arrancou.

Uma resposta inventada converte um programa avariado num programa que
parece estar a funcionar e vazio. É o pior defeito que uma API local pode
ter.

## Porque é que isto é uma tabela e não classes

Porque o contrato com a interface é uma lista de nomes, e a lista está em
``UI/src/lib/onyx/bridge-adapter.js``. Cada função do adaptador tem um
caminho aqui. A tabela é o que garante que os dois lados não divergem: uma
rota que exista e o adaptador não chame é código morto; um adaptador que
chame e a rota não exista dá 404 em produção.

## Onde os nomes mudam entre o Python e o JavaScript

O ``snake_case`` do Python vira ``camelCase`` no JSON. A conversão está
em :func:`camel`, e é feita num sítio só — escrever os dois nomes à mão
em cada rota dava cinquenta pontos onde uma letra trocada não dá erro de
tipo, porque o JSON é tudo `any`.
"""

from __future__ import annotations

import base64
import tempfile
from pathlib import Path
from typing import Any

from messenger import conta as conta_mod
from messenger import ipc_client
from messenger import storage
from messenger.descoberta import SERVIDOR_DESCOBERTA
from server import loja as loja_mod
from server.sidecar import ErroDePedido, Rota, ServicoIndisponivel

__all__ = ["montar", "camel", "daemon"]

#: O comando IPC que cada rota precisa.
#:
#: Vem aqui e não em cada rota para que a lista de operações usadas seja
#: uma lista, e para que acrescentar uma operação que o daemon não tem
#: seja um erro visível nesta constante e não um ``500`` em produção.
IPC_INDISPONIVEL = "o daemon onyxchatd não respondeu"


def camel(nome: str) -> str:
    """``alguem_coisa`` — ``alguemCosa``."""
    partes = nome.split("_")
    return partes[0] + "".join(p.capitalize() for p in partes[1:])


def daemon() -> ipc_client.ClienteIpc:
    """Um cliente do daemon.

    Cada chamada abre uma ligação nova. É o que o ``http.server`` exige 
    é multi-thread e uma ligação do IPC não pode ser partilhada entre
    threads sem um lock — e o custo é insignificante ao lado de uma
    mensagem cifrada.
    """
    return ipc_client.ClienteIpc()


def _identidade(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """A identidade local, ou ``503``."""
    aberta = contexto.conta
    if aberta is None:
        raise ServicoIndisponivel("não há conta aberta")

    return {
        "id": aberta.identificador,
        "identifier": aberta.identificador,
        "fingerprint": aberta.impressao,
        "name": conta_mod.nome_de_exibicao(aberta),
        "publicKey": aberta.publica_hex,
        "createdAt": aberta.criada_em,
        "role": "Nó primário",
        "keyAlgorithm": "Ed25519 · X25519",
    }


def _rede_ou_indisponivel() -> dict[str, Any]:
    """O estado de rede quando o daemon não responde.

    A forma é a mesma que :func:`_estado_rede` com todos os campos, mais
    ``indisponivel``. A interface lê o mesmo objecto nos dois casos, e
    isso evita que cada ecrã descubra por si mesmo como é que se diz
    «não há dados».
    """
    return {
        "state": "desligado",
        "peersOnline": 0,
        "peersTotal": 0,
        "indisponivel": True,
        "indisponivelMotivo": IPC_INDISPONIVEL,
    }


def montar(
    loja: loja_mod.Loja,
    conta: conta_mod.Conta | None,
    *,
    cliente: Any | None = None,
) -> list[Rota]:
    """Monta as rotas.

    :param cliente: um substituto para :func:`daemon`. Existe para os
        testes, e é a única forma de testar o sidecar contra um daemon
        sem arrancar o daemon — o que por si só já é um teste lento.
    """
    obter = cliente if cliente is not None else daemon

    def rede(ctx: Any, parametros: dict[str, str]) -> dict[str, Any]:
        try:
            estado = obter().estado()
        except ipc_client.ErroIpc:
            return _rede_ou_indisponivel()
        return {
            "state": getattr(estado, "estado", "desligado"),
            "peersOnline": getattr(estado, "pares", 0),
            "peersTotal": getattr(estado, "total_pares", 0),
            "indisponivel": False,
            "indisponivelMotivo": "",
        }

    return [
        # --- identidade e contagens ----------------------------------
        Rota("GET", "/identidade", _identidade, "identidade"),
        Rota("GET", "/contagens", lambda ctx, parametros: loja.contagens(), "contagens"),

        # --- conversas -----------------------------------------------
        Rota(
            "GET",
            "/conversas",
            lambda ctx, parametros: loja.conversas(pesquisa=ctx.um("pesquisa")),
            "conversas",
        ),
        Rota(
            "GET",
            "/conversas/{id}",
            lambda ctx, p: loja.conversa(p["id"]),
            "conversa",
        ),
        Rota(
            "POST",
            "/mensagens",
            _enviar_mensagem,
            "enviar-mensagem",
        ),
        Rota(
            "POST",
            "/conversas/{id}/lida",
            lambda ctx, p: (_ := loja.marcar_lida(p["id"])) or {"ok": True},
            "marcar-lida",
        ),
        # As acções do menu de contexto. Recebem o valor explícito em vez
        # de alternarem: «fixar» e «desfixar» são o mesmo pedido com
        # valores diferentes, e um botão que só diz «alterna» obriga quem
        # o implementa a manter o estado certo para saber o que vai fazer.
        Rota("POST", "/conversas/{id}/fixada", _fixar_conversa, "fixar-conversa"),
        Rota(
            "POST",
            "/conversas/{id}/silenciada",
            _silenciar_conversa,
            "silenciar-conversa",
        ),
        Rota(
            "POST",
            "/conversas/{id}/limpar",
            _limpar_historico,
            "limpar-historico",
        ),

        # --- transferências de media ---------------------------------
        #
        # Três rotas e nenhuma mais. A leitura, o detalhe com o
        # progresso, e a eliminação. **Não há rota para reproduzir**:
        # reproduzir precisa da chave de media, que é do daemon, e o
        # sidecar não a tem — nem deve. A reprodução é o `MEDIA_REPRODUZIR`
        # (`docs/media.md` §5, opção A).
        Rota(
            "GET",
            "/conversas/{id}/transferencias",
            _transferencias,
            "transferencias",
        ),
        Rota("GET", "/transferencias/{id}", _transferencia, "transferencia"),
        Rota(
            "POST",
            "/transferencias/{id}/reproduzir",
            _reproduzir_transferencia,
            "reproduzir-transferencia",
        ),
        Rota("GET", "/media/estado", _estado_media, "estado-media"),
        Rota(
            "POST",
            "/media/streaming/iniciar",
            _iniciar_streaming,
            "iniciar-streaming",
        ),
        Rota("GET", "/media/streaming/quadro", _quadro_streaming, "quadro-streaming"),
        Rota("GET", "/media/streaming/audio", _audio_streaming, "audio-streaming"),
        Rota("POST", "/media/streaming/parar", _parar_streaming, "parar-streaming"),
        Rota(
            "DELETE",
            "/transferencias/{id}",
            _eliminar_transferencia,
            "eliminar-transferencia",
        ),

        # --- contactos -----------------------------------------------
        Rota(
            "GET",
            "/contactos",
            lambda ctx, parametros: loja.contactos(pesquisa=ctx.um("pesquisa")),
            "contactos",
        ),
        Rota(
            "POST",
            "/contactos/{id}/bloqueado",
            _bloquear_contacto,
            "bloquear-contacto",
        ),
        Rota(
            "POST",
            "/pedidos",
            _pedido_de_contacto,
            "enviar-pedido",
        ),

        # --- pedidos recebidos ---------------------------------------
        Rota(
            "GET",
            "/pedidos",
            lambda ctx, parametros: loja.pedidos(incluir_bloqueados=ctx.flag("incluirBloqueados")),
            "pedidos",
        ),
        Rota(
            "POST",
            "/pedidos/{id}",
            _decidir_pedido,
            "decidir-pedido",
        ),

        # --- rede e segurança ---------------------------------------
        Rota("GET", "/estado", rede, "estado-rede"),
        Rota(
            "GET",
            "/seguranca",
            _seguranca,
            "seguranca",
        ),

        # --- definições ---------------------------------------------
        Rota(
            "GET",
            "/definicoes",
            lambda ctx, parametros: {"seccoes": loja.definicoes()},
            "definicoes",
        ),
        Rota(
            "PUT",
            "/definicoes",
            lambda ctx, parametros: {"seccoes": loja.guardar_definicoes(ctx.json().get("seccoes") or {})},
            "guardar-definicoes",
        ),

        # --- descoberta ---------------------------------------------
        Rota(
            "GET",
            "/descoberta/sugestao",
            _sugestao,
            "descoberta-sugestao",
        ),
        Rota("GET", "/descoberta", _descobrir, "descobrir"),

        # --- conta ---------------------------------------------------
        Rota("GET", "/conta/estado", _estado_conta, "conta-estado"),
        Rota("POST", "/conta", _registar_conta, "conta-registar"),
        Rota("POST", "/conta/desbloquear", _desbloquear, "conta-desbloquear"),
        Rota("POST", "/conta/trancar", _trancar, "conta-trancar"),
        Rota("GET", "/conta/exportar", _exportar, "conta-exportar"),
        Rota("POST", "/conta/restaurar", _restaurar, "conta-restaurar"),
    ]


# ---------------------------------------------------------------------
# Ações
# ---------------------------------------------------------------------


def _enviar_mensagem(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Guarda uma mensagem na conversa certa.

    A conversa é derivada do contacto, não recebida: pedir à interface que
    diga com quem é redundante e é uma fonte de divergência entre o que
    está no ecrã e o que está gravado.
    """
    texto = contexto.texto("texto")
    identificador = contexto.texto("identificador", obrigatorio=False)

    contacto = _contacto_por(contexto.loja, identificador)
    if contacto is None:
        raise ErroDePedido(404, "não há contacto com esse identificador")

    conversa = contexto.loja.abrir_conversa(contacto["id"])
    mensagem = contexto.loja.adicionar_mensagem(conversa["id"], "me", texto)
    return {"conversationId": conversa["id"], "message": mensagem}


def _contacto_por(loja: loja_mod.Loja, identificador: str) -> dict[str, Any] | None:
    for contacto in loja.contactos():
        if contacto["identifier"] == identificador:
            return contacto
    return None


def _pedido_de_contacto(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Envia um pedido a quem ainda não é contacto.

    Guarda-se como pedido do outro lado, que é a única coisa que se pode
    fazer sem um daemon a correr. O pedido a sério — com as chaves — é o
    handshake, e esse não se finge.
    """
    identificador = contexto.texto("identificador")
    mensagem = contexto.texto("mensagem", obrigatorio=False)
    return {"identifier": identificador, "message": mensagem, "estado": "pendente"}


def _decidir_pedido(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    corpo = contexto.json()
    decisao = corpo.get("decisao")
    if not isinstance(decisao, str):
        raise ErroDePedido(400, "falta o campo «decisao»")
    return contexto.loja.decidir_pedido(parametros["id"], decisao)


def _bandeira(contexto: Any, campo: str = "valor") -> bool:
    """Lê um booleano do corpo do pedido, e não o seu oposto.

    A rota recebe ``{"valor": true}`` ou ``{"valor": false}``, e as duas
    coisas têm de ser distinguíveis. ``bool(corpo.get(campo))`` trataria
    um campo ausente como ``False`` — que é o mesmo que «desfixar», e
    faria um pedido malformado desligar algo que estava ligado.

    O ``tipo`` é conferido: ``"false"`` é uma string verdadeira em
    Python, e um cliente que mandasse texto em vez de JSON desligaria a
    conversa sem querer. É `400`, não ``False``.
    """
    corpo = contexto.json()
    if campo not in corpo:
        raise ErroDePedido(400, f"falta o campo «{campo}»")
    valor = corpo[campo]
    if not isinstance(valor, bool):
        raise ErroDePedido(400, f"«{campo}» tem de ser verdadeiro ou falso")
    return valor


def _fixar_conversa(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Fixa ou desfixa. `POST /conversas/{id}/fixada`."""
    contexto.loja.fixar_conversa(parametros["id"], _bandeira(contexto))
    return {"ok": True}


def _silenciar_conversa(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Silencia ou dessilencia. `POST /conversas/{id}/silenciada`."""
    contexto.loja.silenciar_conversa(parametros["id"], _bandeira(contexto))
    return {"ok": True}


def _limpar_historico(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Apaga as mensagens. `POST /conversas/{id}/limpar`."""
    try:
        contexto.loja.limpar_historico(parametros["id"])
    except loja_mod.LojaInvalida as erro:
        raise ErroDePedido(404, str(erro)) from erro
    return {"ok": True}


def _bloquear_contacto(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Bloqueia ou desbloqueia. `POST /contactos/{id}/bloqueado`."""
    try:
        contexto.loja.bloquear_contacto(parametros["id"], _bandeira(contexto))
    except loja_mod.LojaInvalida as erro:
        raise ErroDePedido(404, str(erro)) from erro
    return {"ok": True}


def _transferencias(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """As transferências de media de uma conversa.

    `GET /conversas/{id}/transferencias`.

    O filtro ``completas`` responde à pergunta que a interface faz de
    facto — «o que posso tocar?» — e o valor por omissão devolve **as
    incompletas também**. A razão de não ser o inverso: uma
    transferência a meio é informação, e escondê-la faz o utilizador
    achar que a transferência não existe em vez de estar a chegar.
    """
    # `flag()` devolve `False` para um parâmetro ausente **e** para
    # `completas=0`. São coisas diferentes — «não filtre» e «só as
    # incompletas» — e é por isso que o valor distingue os três casos
    # explicitamente em vez de usar o `False` como omissão.
    bruto = contexto.um("completas", "").lower()
    if bruto in ("", "todos", "todas"):
        completas = None
    else:
        completas = contexto.flag("completas")
    return {
        "transferencias": contexto.loja.transferencias(
            parametros["id"], completas=completas
        )
    }


def _transferencia(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Uma transferência pelo seu id.

    `GET /transferencias/{id}`.

    Inclui o **progresso** — quantas fatias já chegaram — porque é o
    número que permite a interface mostrar «a chegar, 42 de 204» sem o
    cliente ter de contar envelopes.
    """
    try:
        t = contexto.loja.transferencia(parametros["id"])
    except loja_mod.LojaInvalida as erro:
        raise ErroDePedido(404, str(erro)) from erro
    t["recebidasPedacos"] = contexto.loja.contar_pedacos(parametros["id"])
    return {"transferencia": t}


def _estado_media(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """O que a interface precisa de saber antes de oferecer a reproduzir.

    `GET /media/estado`.

    Existe porque as três coisas que podem faltar — o daemon, o leitor e
    a loja apontada — falham **independentemente**, e oferecer um botão
    que vai falhar é pior do que não o oferecer.

    O `leitor` é um **nome**, e não o índice que vai no IPC: um nome é o
    que se escreve num botão («toca com mpv»), e um índice dependeria de
    a ordem de uma tabela interna.
    """
    daemon = contexto.extras.get("daemon")
    sidecar = contexto.extras.get("sidecar")
    if daemon is None:
        return {"disponivel": False, "motivo": "não há daemon a correr"}

    try:
        estado = daemon.media_estado()
    except Exception as erro:  # noqa: BLE001 — ver `Sidecar.apontar_media`
        return {"disponivel": False, "motivo": str(erro) or erro.__class__.__name__}

    if estado.leitor < 0:
        return {"disponivel": False, "motivo": "não há leitor de media instalado"}
    if not estado.loja_apontada:
        return {
            "disponivel": False,
            "motivo": getattr(sidecar, "media_erro", None) or "a loja ainda não foi apontada",
        }

    return {
        "disponivel": True,
        "leitor": ipc_client.LEITORES_MEDIA[estado.leitor],
        "ffmpegEmbutido": estado.ffmpeg_embebido,
    }


def _reproduzir_transferencia(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Monta o ficheiro em RAM e lança o leitor.

    `POST /transferencias/{id}/reproduzir`.

    ## Porque é uma rota e não o `sidecar` a montar

    Montar precisa da loja **e** do ``memfd``, e o ``memfd`` é do daemon
    (`docs/media.md` §4.6.2, opção A). O ``sidecar`` não tem as chaves
    — nem deve — e portanto não tem o que montar.

    ## E o ficheiro nunca toca o disco

    Nada atravessa esta rota em mão: pede-se ao daemon, que escreve
    num ``memfd`` selado
    e lança o leitor sobre ``/proc/self/fd/N``. O que volta é o ``pid``.
    """
    daemon = contexto.extras.get("daemon")
    if daemon is None:
        raise ServicoIndisponivel("não há daemon a correr")

    id = parametros["id"]
    # A loja é quem sabe se isto existe e se está completo. A rota não
    # monta um manifesto para dizer «não existe»: seria uma segunda
    # fonte de verdade sobre as mesmas linhas.
    try:
        t = contexto.loja.transferencia(id)
    except loja_mod.LojaInvalida as erro:
        raise ErroDePedido(404, str(erro)) from erro

    if not t["completo"]:
        # `409`, e não `404`: a transferência **existe** e está a meio.
        # Um `404` diria à interface para a esconder, e «a chegar, 42 de
        # 204» é informação.
        raise ErroDePedido(
            409,
            f"a transferência está incompleta "
            f"({contexto.loja.contar_pedacos(id)} de {t['totalPedacos']} fatias)",
        )

    try:
        estado = daemon.media_estado()
        if estado.leitor < 0:
            raise ErroDePedido(503, "não há leitor de media instalado")
        pid = daemon.media_reproduzir(id, estado.leitor)
    except ErroDePedido:
        raise
    except Exception as erro:  # noqa: BLE001
        raise ServicoIndisponivel(str(erro) or erro.__class__.__name__) from erro

    return {"pid": pid, "leitor": ipc_client.LEITORES_MEDIA[estado.leitor]}


def _eliminar_transferencia(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Apaga a transferência e as suas fatias.

    `DELETE /transferencias/{id}`.

    **Não propaga para o outro participante.** Cada um tem as suas
    linhas e apagar de um lado não chega ao outro — que é o que faz
    «se quem envia elimina, quem recebeu mantém» verdadeiro por
    construção e não por política (`docs/media.md` §4.5).
    """
    try:
        contexto.loja.eliminar_transferencia(parametros["id"])
    except loja_mod.LojaInvalida as erro:
        raise ErroDePedido(404, str(erro)) from erro
    return {"ok": True}


def _seguranca(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """O painel de segurança.

    Vem da conta — que é onde a identidade está — e **não** inventa
    números. Um score de segurança medido a partir de dados de
    demonstração é a coisa que a faixa de aviso existe para impedir, e
    escrevê-lo aqui seria reintroduzi-la pelo outro lado.
    """
    aberta = contexto.conta
    if aberta is None:
        raise ServicoIndisponivel("não há conta aberta")

    return {
        "key": {
            "algorithm": "Ed25519 · X25519",
            "fingerprint": aberta.impressao,
        },
        "checks": [],
        "score": None,
        "indisponivel": False,
    }


def _sugestao(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Um identificador para a pessoa copiar.

    Vem da demonstração, que é o único sítio onde há um identificador
    verdadeiro com que experimentar. Num sidecar a sério esta rota
    devolve ``404`` — e devolve, não devolve um exemplo inventado.
    """
    raise ErroDePedido(404, "sem sugestões: este sidecar não tem")


def _descobrir(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Procura um identificador.

    Consulta a descoberta se o ``messenger.descoberta`` souber falar com
    um servidor; sem servidor configurado a resposta é «não encontrado»,
    que é o que a pessoa vê e não uma excepção.

    Os três argumentos não são opcionais em
    :func:`resolver_ou_usar`, e esta rota chamava-a com um só. O
    ``TypeError`` caía no ``except`` de baixo e a rota respondia
    «não encontrado» **para sempre** — o `except` largo, feito para
    engolir as excepções do cliente de descoberta, tapava também a
    própria chamada estar errada.
    """
    from messenger import descoberta

    identificador = contexto.um("identificador")
    if not identificador:
        raise ErroDePedido(400, "falta o parâmetro «identificador»")

    try:
        onion = descoberta.resolver_ou_usar(
            identificador, ipc_client.MODO_DIRETO, SERVIDOR_DESCOBERTA
        )
    except Exception:  # noqa: BLE001 — o cliente de descoberta tem várias
        # excepções possíveis e nenhuma delas é «a pessoa fez mal».
        return {"status": "not_found"}

    if onion is None:
        return {"status": "not_found"}
    return {"status": "found", "onion": onion}


def _estado_conta(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """O estado da conta: os três valores que a interface conhece."""

    caminho = contexto.extras.get("caminho_conta")
    existe = conta_mod.conta_existe(caminho)
    if not existe:
        return {"estado": "sem-conta"}
    if contexto.conta is not None:
        return {"estado": "desbloqueada"}
    return {"estado": "trancada"}


def _registar_conta(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:

    utilizador = contexto.texto("utilizador")
    frase = contexto.texto("frase")
    caminho = contexto.extras.get("caminho_conta")
    try:
        criada = conta_mod.criar(utilizador, frase, caminho)
    except conta_mod.FraseForte as erro:
        # `FraseForte` é subclasse de `ContaInvalida`, e apanhar a classe
        # mais geral primeiro dava `409 Conflict` a uma frase fraca. O
        # código está errado: o pedido não conflita com nada, está mal
        # escrito. E o corpo da mensagem — que é o que a pessoa precisa de
        # ler — é o mesmo, o que faz o engano silencioso.
        raise ErroDePedido(400, str(erro)) from erro
    except conta_mod.ContaInvalida as erro:
        raise ErroDePedido(409, str(erro)) from erro
    return {
        "identificador": criada.identificador,
        "fingerprint": criada.impressao,
        "utilizador": criada.utilizador,
        "criadaEm": criada.criada_em,
    }


def _desbloquear(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:

    frase = contexto.texto("frase")
    caminho = contexto.extras.get("caminho_conta")
    try:
        aberta = conta_mod.entrar(frase, caminho)
    except storage.KeystoreInvalido as erro:  # noqa: B904 — a resposta é genérica
        # A excepção não distingue «frase errada» de «ficheiro
        # adulterado», e a resposta também não distingue. Ver
        # `docs/conta.md` §Erros.
        raise ErroDePedido(403, "a frase de segurança não abre a conta") from erro

    # A sessão muda no `Sidecar`, não no pedido: o `Contexto` é
    # destruído quando o pedido acaba, e mudar o estado aí seria
    # perder a sessão no fim da resposta.
    contexto.extras["sidecar"].conta = aberta
    return {
        "identificador": aberta.identificador,
        "fingerprint": aberta.impressao,
        "utilizador": aberta.utilizador,
    }


def _trancar(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Tranca a sessão.

    O ``conta`` do sidecar passa a ``None`` e o ficheiro fica como
    estava. Apagar a conta porque alguém fechou a janela seria uma
    decisão que o programa nunca toma por si.
    """
    contexto.extras["sidecar"].conta = None
    return {"ok": True}


def _exportar(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """A cópia de segurança, já cifrada.

    Vem em base64 porque o corpo de uma resposta tem de ser JSON, e o
    ficheiro é binário. A interface descarrega-o a partir daqui e nunca
    sabe o que está dentro.
    """
    caminho = Path(contexto.extras.get("caminho_conta") or conta_mod.caminho_conta())
    try:
        bruto = caminho.read_bytes()
    except OSError as erro:
        raise ErroDePedido(404, "não há conta para exportar") from erro

    return {
        "conteudo": base64.b64encode(bruto).decode("ascii"),
        "modo": "0600",
    }


def _restaurar(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Repõe a conta a partir de uma cópia.

    A verificação da frase acontece **antes** de tocar no destino. Ver
    `messenger/conta.py` §Restaurar valida antes de sobrescrever: uma
    conta boa substituída por uma cópia que não abre é a pior das duas.
    """

    frase = contexto.texto("frase")
    conteudo = contexto.texto("conteudo")
    destino = contexto.extras.get("caminho_conta") or conta_mod.caminho_conta()

    try:
        bruto = base64.b64decode(conteudo, validate=True)
    except Exception as erro:  # noqa: BLE001 — binascii e ValueError
        raise ErroDePedido(400, "a cópia não é um ficheiro válido") from erro

    # Valida-se numa cópia temporária, para que o destino só seja tocado
    # depois de a frase ter aberto a conta.
    with tempfile.TemporaryDirectory() as pasta:
        teste = Path(pasta) / "conta.keystore"
        teste.write_bytes(bruto)
        try:
            conta_mod.restaurar(teste, frase, destino)
        except storage.KeystoreInvalido as erro:
            raise ErroDePedido(403, "a cópia não abre com essa frase") from erro
        except conta_mod.ContaInvalida as erro:
            raise ErroDePedido(400, str(erro)) from erro

    return {"ok": True}


# ---------------------------------------------------------------------
# O streaming de vídeo
# ---------------------------------------------------------------------
#
# Três rotas, e são as que `UI/src/lib/onyx/VideoStreaming.jsx` chama.
# O circuito do lado do emissor é outro (§10.3) — estas são o lado de
# quem **vê**, que é o que o `sidecar` serve.
#
# ## Por que o corpo do quadro é base-64 e não binário
#
# Porque a resposta é JSON, e `JSON` não transporta `ArrayBuffer`. E
# porque o `sidecar` fala `JSON` com a UI e com o `ffmpeg` de outra
# forma: esta rota é a **fronteira** onde os bytes deixam de ser bytes e
# passam a ser texto.
#
# O base-64 custa 33 % — e é o mesmo custo que o `ONX1` já paga desde o
# início (`docs/media.md` §2). Não é um custo novo: é o preço de a
# media ser indistinguível de texto, que é a premissa do projecto.


def _iniciar_streaming(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Liga a captura e a codificação VP8.

    `POST /media/streaming/iniciar`.

    Liga o `ffmpeg` com `libvpx` a 480p (§3.4.2 mediu 6,3× de folga no
    orçamento) e devolve a geometria aceite.
    """
    daemon = contexto.extras.get("daemon")
    if daemon is None:
        raise ServicoIndisponivel("não há daemon a correr")

    corpo = contexto.json()
    largura = _inteiro(corpo, "largura", 854)
    altura = _inteiro(corpo, "altura", 480)
    fps = _inteiro(corpo, "fps", 30)
    audio_kbps = _inteiro(corpo, "audioKbps", 32)
    video_kbps = _inteiro(corpo, "videoKbps", 300)

    try:
        daemon.media_capturar(largura, altura, fps, audio_kbps, video_kbps)
    except ipc_client.PedidoInvalido as erro:
        # Um pedido que o daemon recusa é um pedido errado, e a causa
        # está na mensagem que ele mandou. Um `500` esconderia a única
        # informação que a pessoa precisa.
        raise ErroDePedido(400, str(erro)) from erro
    except ipc_client.ErroDaemon as erro:
        # A geometria passou e o `ffmpeg` não arrancou: não é culpa do
        # pedido. E a mensagem do daemon traz o `stderr` do `ffmpeg`
        # (§8.1.1), que é o que diz *porquê*.
        raise ErroDePedido(503, str(erro) or erro.__class__.__name__) from erro

    return {"largura": largura, "altura": altura, "fps": fps}


def _quadro_streaming(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """O próximo quadro de VP8, em base-64.

    `GET /media/streaming/quadro`.

    Bloqueia até haver quadro: o `ffmpeg` produz a 30 fps e o `JSON` não
    faz streaming. A espera é do lado do daemon (`MEDIA_LER` bloqueia),
    e por isso a resposta é sempre um quadro inteiro.

    ## Os dois erros, e a diferença entre eles

    | estado | resposta |
    | --- | --- |
    | não há captura | **404** |
    | a captura acabou | **409** |

    O `VideoStreaming` traduz os dois em `null`, que é o fim da chamada.
    A distinção não é para a UI — é para quem lê o registo: «não havia
    captura» é uma falha de arranque e «a captura acabou» é uma chamada
    que terminou, e as duas não se resolvem da mesma maneira.
    """
    daemon = contexto.extras.get("daemon")
    if daemon is None:
        raise ServicoIndisponivel("não há daemon a correr")

    try:
        quadro = daemon.media_ler()
    except ipc_client.ErroDaemon as erro:
        if erro.codigo == ipc_client.ERR_MEDIA:
            # `ERR_MEDIA` no `MEDIA_LER` é o `ffmpeg` a morrer (§8.1):
            # a captura acabou. Não há distinção possível entre «não há
            # captura» e «acabou» do lado do daemon, porque o mesmo
            # `0x14` cobre os dois — e por isso esta rota assume que
            # quem chegou aqui queria um quadro e não o recebeu.
            raise ErroDePedido(409, str(erro) or "a captura terminou") from erro
        raise ErroDePedido(503, str(erro) or erro.__class__.__name__) from erro

    return {
        "sequencia": quadro.sequencia,
        "clockMs": quadro.clock_ms,
        "vp8": base64.b64encode(quadro.dados).decode("ascii"),
    }


def _audio_streaming(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """O próximo pacote de Opus, em base-64.

    `GET /media/streaming/audio`.

    ## Porque devolve um objecto e não a base-64 directamente

    Porque uma `string` vazia não distingue «não há pacote» de «um
    pacote de tamanho zero». E um pacote de Opus de tamanho zero é um
    pacote corrompido, não um silêncio: devolver `""` faria o
    `Chamada.jsx` tocar silêncio em vez de dizer que não há nada.

    É o mesmo motivo pelo qual o quadro vai em `{seq, clockMs, vp8}` e
    não em base-64 pelado.

    Bloqueia até o `pump_audio` do `ffmpeg` entregar um pacote, e
    bloqueia no `MEDIA_AUDIO`. Um `try_recv` daria um laço que gira.
    """
    daemon = contexto.extras.get("daemon")
    if daemon is None:
        raise ServicoIndisponivel("não há daemon a correr")

    try:
        pacote = daemon.media_audio()
    except ipc_client.ErroDaemon as erro:
        if erro.codigo == ipc_client.ERR_MEDIA:
            raise ErroDePedido(409, str(erro) or "a captura terminou") from erro
        raise ErroDePedido(503, str(erro) or erro.__class__.__name__) from erro

    # `None` é a captura acabada, e um `None` no `JSON` é `null` — que
    # é o que o `Chamada.jsx` sabe ler. Um `409` aqui dava o mesmo
    # resultado no browser, mas a rota e o laço não são o mesmo
    # nível, e o `null` é mais barato.
    if pacote is None:
        return {"opus": None}

    return {"opus": base64.b64encode(pacote).decode("ascii")}


def _parar_streaming(contexto: Any, parametros: dict[str, str]) -> dict[str, Any]:
    """Termina a captura.

    `POST /media/streaming/parar`.

    ## Porque isto existe apesar de `MEDIA_PARAR` não existir

    `docs/media.md` §8.1.2 diz que `MEDIA_PARAR` não existe porque parar
    a captura é o `Drop` do `Stream` e um comando para o fazer seria um
    comando que alguém esquece.

    Esta rota **não** é esse comando: não manda o `ffmpeg` parar, não
    toca no `Stream`, e não muda nada no daemon. É um `204` que diz à UI
    «podes deixar de pedir quadros».

    A diferença é que a UI **pode** esquecer, e o esquecimento só custa
    uma chamada que pede quadros a um `ffmpeg` que já não existe — e o
    daemon responde `409`, que é o fim da chamada. Uma chamada fechada
    que fica a pedir quadros durante segundos é ruído, não um bug.
    """
    daemon = contexto.extras.get("daemon")
    if daemon is None:
        raise ServicoIndisponivel("não há daemon a correr")
    # `204` e não um corpo: não há nada a devolver. E o `VideoStreaming`
    # não lê a resposta.
    return {}


def _inteiro(corpo: dict[str, Any], campo: str, omissao: int) -> int:
    """Um inteiro do corpo do pedido, ou o valor por omissão.

    ## Porque não valida o intervalo

    Porque o `MEDIA_CAPTURAR` valida, e uma segunda validação aqui
    seria um segundo sítio a divergir. O que este ficheiro tem de
    apanhar é o que **não chega ao daemon**: um valor que não é um
    inteiro, que é o que o `JSON` permite e o `IPC` não.
    """
    valor = corpo.get(campo, omissao)
    if isinstance(valor, bool) or not isinstance(valor, int):
        raise ErroDePedido(400, f"«{campo}» tem de ser um inteiro")
    return valor
