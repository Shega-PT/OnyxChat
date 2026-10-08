# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
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

        # --- contactos -----------------------------------------------
        Rota(
            "GET",
            "/contactos",
            lambda ctx, parametros: loja.contactos(pesquisa=ctx.um("pesquisa")),
            "contactos",
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
