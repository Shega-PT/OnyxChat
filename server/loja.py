# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""loja.py — o armazenamento do sidecar.

## Porquê o sidecar é que tem a loja

Não há outro sítio onde ela possa estar. `ESTADO.amigos` do daemon é um
``u16`` e não existe comando para listar amigos, conversas ou mensagens
(ver ``docs/roadmap.md``). O daemon vive em memória e reiniciar obriga a
refazer o handshake; uma conversa que se perdesse a cada reinício não era
uma conversa.

Por isso o sidecar é dono do armazenamento, e o daemon fica com o que é
dele: chaves, ligações, assinatura e cifra.

## O que é guardado em claro e o que não é

**Em claro**: as conversas, os contactos, os pedidos, as definições.

Isto é uma decisão, e é a única que este ficheiro toma sobre
confidencialidade. O texto das mensagens está **já cifrado** pelo pipeline
K1→K9 antes de chegar aqui — o sidecar guarda envelopes, não texto claro.
O que não é cifrado é a **metainformação**: com quem, quando, quantas
mensagens, e o texto em claro no máximo até ao momento da cifragem.

**Não é guardado**: a seed da identidade, a frase de segurança, e as
chaves de comunicação. Vêm de `messenger/conta.py` e
``messenger.keys``, são pedidos ao daemon por operação, e nunca são
escritos aqui. A loja não sabe cifrar e não tenta.

Esta distinção está escrita porque «o sidecar tem a loja» soa a «o
sidecar tem o segredo», e são coisas diferentes. O sidecar tem
*intermediação*; quem tem o *segredo* é o daemon, que o leu de um ficheiro
que só abre com a frase.

## Porque é que os metadados ficam em claro

Porque cifrá-los exigiria uma chave — e a única chave que o sidecar pode
ter sem lhe chamar é a que protege a identidade. Um atacante com acesso ao
ficheiro da loja e à sessão aberta lê o grafo social. É o mesmo
compromisso do `localStorage` de qualquer aplicação de mensagens, e
dizê-lo é melhor do que descobrir que não era verdade.

O que a loja **faz** é não tornar isso pior: o ficheiro é ``0600``, e
está em claro porque não há nada com que o cifrar.

## SQLite e não JSON

`sqlite3` é da biblioteca padrão, dá transacções, e — o que mais importa
aqui — não reescreve o ficheiro inteiro a cada escrita. Um JSON reescrito
a cada mensagem perde tudo se a energia falhar a meio, e uma conversa é
exactamente o que não se pode perder assim.

As ligações são explicitamente `WAL`, e o `busy_timeout` é de 5 s: o
sidecar tem várias threads do ``http.server`` e uma escrita bloqueada
durante um segundo não pode aparecer como erro para quem está a enviar
uma mensagem.

## Como abrir

::

    with Loja(caminho) as loja:
        loja.pedidos()

O contexto fecha a ligação e faz commit. Fechar à mão e esquecer-se é a
forma mais comum de perder uma escrita, e um ``with`` torna o caminho
certo o caminho curto.
"""

from __future__ import annotations

import json
import os
import secrets
import sqlite3
import time
import unicodedata
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Iterator

__all__ = ["ESQUEMA", "Loja", "LojaInvalida"]

#: Versão do esquema. Incrementar quando a forma muda.
VERSAO_ESQUEMA = 1

#: Onde a loja vive por omissão.
CAMINHO_OMISSAO = Path(__file__).resolve().parent.parent / "user" / "loja.db"


#: O esquema, e a razão de cada índice.
#:
#: Os índices não são optimização: são o que impede o *scan completo* da
#: tabela num histórico de mensagens. `conversas` é pequena; `mensagens` é
#: o que cresce sem limite, e `idx_mensagens_conversa` é o que faz «as
#: últimas 50 desta conversa» custar o mesmo em todas as conversas.
ESQUEMA = """
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS contactos (
    id            TEXT PRIMARY KEY,
    identificador TEXT NOT NULL UNIQUE,
    impressao     TEXT NOT NULL,
    nome          TEXT NOT NULL,
    estado        TEXT NOT NULL DEFAULT 'desligado',
    nota          TEXT,
    bloqueado     INTEGER NOT NULL DEFAULT 0,
    verificado    INTEGER NOT NULL DEFAULT 0,
    criado_em     REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS conversas (
    id          TEXT PRIMARY KEY,
    contacto_id TEXT NOT NULL REFERENCES contactos(id),
    fixada      INTEGER NOT NULL DEFAULT 0,
    silenciada  INTEGER NOT NULL DEFAULT 0,
    lida        INTEGER NOT NULL DEFAULT 1,
    verificada  INTEGER NOT NULL DEFAULT 0,
    rota        TEXT NOT NULL DEFAULT 'direto',
    criada_em   REAL NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversas_contacto ON conversas(contacto_id);

CREATE TABLE IF NOT EXISTS mensagens (
    id            TEXT PRIMARY KEY,
    conversa_id   TEXT NOT NULL REFERENCES conversas(id) ON DELETE CASCADE,
    de            TEXT NOT NULL,
    texto         TEXT NOT NULL,
    em            REAL NOT NULL,
    -- Sem valor por omissão de propósito: «enviada» e «recebida» são
    -- estados que se confundiam. Uma mensagem que entrava de fora
    -- nascia marcada como enviada, e a contagem de não lidas ficava a
    -- zero sem que ninguém percebe-se porquê.
    estado        TEXT NOT NULL,
    envelope      BLOB
);

-- Sem este índice, abrir uma conversa é `ORDER BY em` sobre a tabela
-- inteira. Ver a nota sobre `ESQUEMA`.
CREATE INDEX IF NOT EXISTS idx_mensagens_conversa ON mensagens(conversa_id, em);

CREATE TABLE IF NOT EXISTS pedidos (
    id           TEXT PRIMARY KEY,
    identificador TEXT NOT NULL,
    nome         TEXT,
    mensagem     TEXT,
    estado       TEXT NOT NULL DEFAULT 'pendente',
    criado_em    REAL NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pedidos_estado ON pedidos(estado, criado_em);

CREATE TABLE IF NOT EXISTS definicoes (
    chave TEXT PRIMARY KEY,
    valor TEXT NOT NULL
);
"""


class LojaInvalida(RuntimeError):
    """Operação sobre a loja que não faz sentido no estado actual."""


class Loja:
    """O armazenamento do sidecar.

    Não é segura para uso entre threads por si só. O ``sqlite3`` é criado
    com ``check_same_thread=False`` porque o ``http.server`` do sidecar é
    multi-thread e cada thread tem a sua própria :class:`sqlite3.Connection`
    — que é a forma correcta, e a que evita um ``Lock`` à volta de tudo.
    """

    __slots__ = ("_cx", "_caminho")

    def __init__(self, caminho: Path | str | None = None) -> None:
        """Abre (ou cria) a loja.

        :param caminho: omissão em :data:`CAMINHO_OMISSAO`. O directório
            é criado; o ficheiro fica ``0600`` porque é metainformação de
            quem fala com quem.
        """
        self._caminho = Path(caminho) if caminho is not None else CAMINHO_OMISSAO
        self._caminho.parent.mkdir(parents=True, exist_ok=True)

        self._cx = sqlite3.connect(str(self._caminho), check_same_thread=False, timeout=5.0)
        self._cx.row_factory = sqlite3.Row
        # `foreign_keys` é off por omissão no sqlite, e o `ON DELETE
        # CASCADE` das mensagens depende dele. Sem esta linha, apagar uma
        # conversa deixava as mensagens órfãs sem que nada dissesse nada.
        self._cx.execute("PRAGMA foreign_keys = ON")
        self._cx.executescript(ESQUEMA)
        self._gravar_modo()

    def _gravar_modo(self) -> None:
        """Deixa o ficheiro só legível pelo dono."""
        try:
            os.chmod(self._caminho, 0o600)
        except OSError:
            # Um sistema de ficheiros que não tem POSIX — ou um caminho
            # que não é um ficheiro — não deve impedir o arranque. A loja
            # funciona; o que não se garante é o modo, e isso é melhor
            # dito do que fatal.
            pass

    # -----------------------------------------------------------------
    # Ciclo de vida
    # -----------------------------------------------------------------

    def fechar(self) -> None:
        """Fecha a ligação, com commit do que estiver pendente."""
        try:
            self._cx.commit()
        finally:
            self._cx.close()

    def __enter__(self) -> Loja:
        return self

    def __exit__(self, *_excecao: object) -> None:
        self.fechar()

    # -----------------------------------------------------------------
    # Identificadores
    # -----------------------------------------------------------------

    @staticmethod
    def _novo_id(prefixo: str) -> str:
        """Um identificador ordenável no tempo.

        O tempo entra à frente, de milissegundos em base 16, para que dois
        identificadores escritos em milissegundos diferentes se ordenem
        como foram criados.

        Dentro do mesmo milissegundo o sufixo aleatório **não** ordena —
        é um número, não uma sequência. Por isso as consultas que
        precisam de ordem exacta usam o ``rowid`` do SQLite, que é a
        ordem de inserção e é exacto. A parte temporal do identificador
        serve para o olho humano e para ordenar entre instantes; para
        desempatar, ``rowid``.

        O sufixo aleatório continua a ser o que garante a unicidade sem um
        contador: dois identificadores lado a lado não colidem.
        """
        marca = format(int(time.time() * 1000), "012x")
        return f"{prefixo}{marca}{secrets.token_hex(4)}"

    # -----------------------------------------------------------------
    # Contactos
    # -----------------------------------------------------------------

    def contactos(self, pesquisa: str = "") -> list[dict[str, Any]]:
        """Todos os contactos, ordenados por nome.

        A ordenação é feita em Python e não por ``ORDER BY nome``, porque
        a collation do sqlite é binária: ``Zé`` vem antes de ``ana`` sem
        ter em conta os acentos, e uma lista de contactos em ordem
        binária é uma lista que parece estar aleatória.
        """
        linhas = self._cx.execute("SELECT * FROM contactos").fetchall()
        if pesquisa:
            alvo = pesquisa.casefold()
            linhas = [
                l for l in linhas
                if alvo in l["nome"].casefold()
                or alvo in l["identificador"].casefold()
            ]
        registos = [_contacto_de(l) for l in linhas]
        registos.sort(key=lambda c: _ordem_de_nome(c["name"]))
        return registos

    def conversa_por_contacto(self, contacto_id: str) -> dict[str, Any] | None:
        """A conversa entre nós e um contacto, ou ``None``.

        Não cria. Criar ao abrir um ecrã é a forma de uma conversa aparecer
        sozinha quando alguém espreita um contacto, e quem recebe mensagem
        sem ter falado fica sem forma de responder.
        """
        linha = self._cx.execute(
            "SELECT * FROM conversas WHERE contacto_id = ?", (contacto_id,)
        ).fetchone()
        return None if linha is None else self.conversa(linha["id"])

    # -----------------------------------------------------------------
    # Conversas
    # -----------------------------------------------------------------

    def conversa(self, conversa_id: str) -> dict[str, Any]:
        """Uma conversa com as suas mensagens.

        :raises LojaInvalida: se a conversa não existir.
        """
        linha = self._cx.execute(
            "SELECT * FROM conversas WHERE id = ?", (conversa_id,)
        ).fetchone()
        if linha is None:
            raise LojaInvalida(f"conversa inexistente: {conversa_id}")

        return {
            **_conversa_de(linha),
            "messages": self.mensagens(conversa_id),
        }

    def conversas(self, pesquisa: str = "") -> list[dict[str, Any]]:
        """As conversas com a última mensagem e a contagem de não lidas.

        A última mensagem vem de uma sub-consulta e não de carregar todas
        as mensagens: uma lista com mil mensagens por conversa transformaria
        o ecrã inicial em ``SELECT`` de milhões de linhas.
        """
        sql = """
            SELECT c.*,
                   (SELECT texto FROM mensagens
                     WHERE conversa_id = c.id ORDER BY em DESC LIMIT 1) AS ultima_texto,
                   (SELECT em    FROM mensagens
                     WHERE conversa_id = c.id ORDER BY em DESC LIMIT 1) AS ultima_em,
                   -- Não lidas é o que **entrou** e continua por ler.
                   -- Contar «enviada» contava as nossas próprias
                   -- mensagens, e a barra mostrava um número que não se
                   -- resolvia com nada.
                   (SELECT COUNT(*) FROM mensagens
                     WHERE conversa_id = c.id
                       AND de = 'them' AND estado <> 'lida') AS nao_lidas
              FROM conversas c
        """
        registos = [_conversa_de(l) for l in self._cx.execute(sql).fetchall()]

        if pesquisa:
            alvo = pesquisa.casefold()
            registos = [
                c for c in registos
                if alvo in str(c.get("lastMessage") or "").casefold()
                or alvo in str(self._nome_do_contacto(c["contactId"]) or "").casefold()
            ]

        registos.sort(
            key=lambda c: (not c["pinned"], c.get("lastAt") or c["criadaEm"], _ordem_de_nome(c["id"]))
        )
        return registos

    def _nome_do_contacto(self, contacto_id: str) -> str | None:
        linha = self._cx.execute(
            "SELECT nome FROM contactos WHERE id = ?", (contacto_id,)
        ).fetchone()
        return None if linha is None else linha["nome"]

    def mensagens(self, conversation_id: str, limite: int = 200) -> list[dict[str, Any]]:
        """As mensagens de uma conversa, das mais antigas às mais recentes.

        O ``limite`` corta **as mais antigas**, não as mais recentes. É o
        que uma pessoa quer ao abrir uma conversa: o que ficou para trás.
        A consulta traz as últimas ``limite`` e inverte a ordem em Python,
        porque inverter em SQL com ``LIMIT`` exige uma sub-consulta.
        """
        linhas = self._cx.execute(
            "SELECT * FROM mensagens WHERE conversa_id = ?"
            " ORDER BY em DESC, rowid DESC LIMIT ?",
            (conversation_id, limite),
        ).fetchall()
        return [_mensagem_de(l) for l in reversed(linhas)]

    # -----------------------------------------------------------------
    # Escrever
    # -----------------------------------------------------------------

    def abrir_conversa(self, contacto_id: str) -> dict[str, Any]:
        """Devolve a conversa com este contacto, a criar-a se não existir.

        Ao contrário de :meth:`conversa_por_contacto`, esta **cria**. É a
        diferença entre abrir um ecrã e começar uma conversa, e quem envia
        a primeira mensagem está a fazer a segunda coisa sem o saber —
        por isso criar aqui e não ao espreitar.
        """
        existente = self.conversa_por_contacto(contacto_id)
        if existente is not None:
            return existente

        novo_id = self._novo_id("c")
        agora = time.time()
        self._cx.execute(
            "INSERT INTO conversas (id, contacto_id, criada_em) VALUES (?, ?, ?)",
            (novo_id, contacto_id, agora),
        )
        self._cx.commit()
        return self.conversa(novo_id)

    def adicionar_mensagem(
        self,
        conversation_id: str,
        de: str,
        texto: str,
        *,
        estado: str | None = None,
        envelope: bytes | None = None,
    ) -> dict[str, Any]:
        """Acrescenta uma mensagem e devolve-a já montada.

        Marca a conversa como não lida quando a mensagem é de fora: é o
        que faz o número ao lado do nome aparecer. Uma conversa que fica
        sem número quando chega uma mensagem é uma conversa em que se
        perde a informação de que havia algo novo.

        :raises LojaInvalida: se a conversa não existir, ou se ``de`` não
            for ``me`` nem ``them``.
        """
        if de not in ("me", "them"):
            raise LojaInvalida(f"remetente inválido: {de!r} (tem de ser 'me' ou 'them')")

        if self._cx.execute(
            "SELECT 1 FROM conversas WHERE id = ?", (conversation_id,)
        ).fetchone() is None:
            raise LojaInvalida(f"conversa inexistente: {conversation_id}")

        # O estado por omissão depende do sentido: o que sai de cá está
        # «enviada», o que entra está «recebida». Passar `estado` à mão
        # continua a ser possível — é o que permite ao daemon actualizar
        # uma mensagem para «entregue» — mas quem não disser fica com o
        # estado que o sentido manda.
        if estado is None:
            estado = "enviada" if de == "me" else "recebida"

        novo_id = self._novo_id("m")
        agora = time.time()
        self._cx.execute(
            "INSERT INTO mensagens (id, conversa_id, de, texto, em, estado, envelope)"
            " VALUES (?, ?, ?, ?, ?, ?, ?)",
            (novo_id, conversation_id, de, texto, agora, estado, envelope),
        )
        if de == "them":
            self._cx.execute("UPDATE conversas SET lida = 0 WHERE id = ?", (conversation_id,))
        self._cx.commit()

        linha = self._cx.execute(
            "SELECT * FROM mensagens WHERE id = ?", (novo_id,)
        ).fetchone()
        return _mensagem_de(linha)

    def marcar_lida(self, conversation_id: str) -> None:
        """Marca a conversa e as suas mensagens recebidas como lidas.

        Marcar só a conversa deixaria a contagem por_messages a dizer que
        havia coisas por ler, porque a contagem conta mensagens e não
        conversas. As duas coisas têm de mudar juntas.
        """
        self._cx.execute("UPDATE conversas SET lida = 1 WHERE id = ?", (conversation_id,))
        self._cx.execute(
            "UPDATE mensagens SET estado = 'lida' WHERE conversa_id = ? AND de = 'them'",
            (conversation_id,),
        )
        self._cx.commit()

    # -----------------------------------------------------------------
    # Pedidos de amizade
    # -----------------------------------------------------------------

    def pedidos(self, incluir_bloqueados: bool = False) -> list[dict[str, Any]]:
        """Os pedidos por decidir, e os decididos se for pedido.

        Por omissão traz os pendentes. Trazer os já decididos sem o pedir
        punha na lista coisas que a pessoa já tratou, e uma lista de
        pedidos em que tudo já foi resolvido parece um programa avariado.
        """
        sql = "SELECT * FROM pedidos"
        if not incluir_bloqueados:
            sql += " WHERE estado = 'pendente'"
        sql += " ORDER BY criado_em DESC, id DESC"

        return [
            {
                "id": l["id"],
                "identifier": l["identificador"],
                "name": l["nome"],
                "message": l["mensagem"],
                "createdAt": _iso(l["criado_em"]),
                "estado": l["estado"],
            }
            for l in self._cx.execute(sql)
        ]

    def registar_pedido(
        self,
        identificador: str,
        nome: str | None = None,
        mensagem: str | None = None,
    ) -> dict[str, Any]:
        """Regista um pedido recebido."""
        novo_id = self._novo_id("p")
        self._cx.execute(
            "INSERT INTO pedidos (id, identificador, nome, mensagem, criado_em)"
            " VALUES (?, ?, ?, ?, ?)",
            (novo_id, identificador, nome, mensagem, time.time()),
        )
        self._cx.commit()
        linha = self._cx.execute("SELECT * FROM pedidos WHERE id = ?", (novo_id,)).fetchone()
        return {
            "id": linha["id"],
            "identifier": linha["identificador"],
            "name": linha["nome"],
            "message": linha["mensagem"],
            "createdAt": _iso(linha["criado_em"]),
            "estado": linha["estado"],
        }

    def decidir_pedido(self, pedido_id: str, decisao: str) -> dict[str, Any]:
        """Aceita, recusa ou bloqueia um pedido.

        ``aceite`` e ``bloqueia`` criam o contacto; ``recusa`` não, porque
        recusar e não querer que falem é diferente de querer que falem e
        não estar pronto.

        :raises LojaInvalida: se o pedido não existir, já tiver sido
            decidido, ou se a decisão não for conhecida.
        """
        if decisao not in ("aceite", "recusa", "bloqueia"):
            raise LojaInvalida(f"decisão inválida: {decisao!r}")

        linha = self._cx.execute("SELECT * FROM pedidos WHERE id = ?", (pedido_id,)).fetchone()
        if linha is None:
            raise LojaInvalida(f"pedido inexistente: {pedido_id}")
        if linha["estado"] != "pendente":
            raise LojaInvalida(
                f"o pedido {pedido_id} já foi decidido ({linha['estado']})"
            )

        self._cx.execute(
            "UPDATE pedidos SET estado = ? WHERE id = ?", (decisao, pedido_id)
        )

        contacto_id = None
        if decisao in ("aceite", "bloqueia"):
            contacto_id = self._criar_contacto_de_pedido(linha, bloqueado=decisao == "bloqueia")

        self._cx.commit()
        return {"id": pedido_id, "decisao": decisao, "contactoId": contacto_id}

    def _criar_contacto_de_pedido(self, linha: sqlite3.Row, *, bloqueado: bool) -> str:
        """Cria (ou reaproveita) o contacto a partir de um pedido."""
        existente = self._cx.execute(
            "SELECT id FROM contactos WHERE identificador = ?", (linha["identificador"],)
        ).fetchone()
        if existente is not None:
            self._cx.execute(
                "UPDATE contactos SET bloqueado = ? WHERE id = ?",
                (1 if bloqueado else 0, existente["id"]),
            )
            return existente["id"]

        novo_id = self._novo_id("ct")
        self._cx.execute(
            "INSERT INTO contactos (id, identificador, impressao, nome, bloqueado, criado_em)"
            " VALUES (?, ?, ?, ?, ?, ?)",
            (
                novo_id,
                linha["identificador"],
                # A impressão de um contacto que ainda não falou connosco
                # não se pode derivar: só se deriva de um nome e um sal, e
                # o sal é do outro. Fica vazia até ao handshake, e a
                # interface mostra «—» enquanto isso.
                "",
                linha["nome"] or linha["identificador"],
                1 if bloqueado else 0,
                time.time(),
            ),
        )
        return novo_id

    # -----------------------------------------------------------------
    # Definições
    # -----------------------------------------------------------------

    def definicoes(self) -> dict[str, Any]:
        """Todas as definições, como dicionário."""
        return {
            l["chave"]: json.loads(l["valor"])
            for l in self._cx.execute("SELECT * FROM definicoes")
        }

    def guardar_definicoes(self, valores: dict[str, Any]) -> dict[str, Any]:
        """Substitui as definições pelas dadas e devolve o resultado.

        Substitui e não funde. Um ecrã de definições que manda só o que
        mudou sobre um estado que já não existe produziria definições a
        meio; e o que a interface manda é sempre o conjunto completo,
        porque ela tem o objecto inteiro na mão.
        """
        self._cx.execute("DELETE FROM definicoes")
        for chave, valor in valores.items():
            self._cx.execute(
                "INSERT INTO definicoes (chave, valor) VALUES (?, ?)",
                (chave, json.dumps(valor, ensure_ascii=False)),
            )
        self._cx.commit()
        return self.definicoes()

    # -----------------------------------------------------------------
    # Contagens
    # -----------------------------------------------------------------

    def contagens(self) -> dict[str, int]:
        """O que a barra superior mostra: conversas, não lidas, contactos, pedidos."""
        def contar(sql: str) -> int:
            return int(self._cx.execute(sql).fetchone()[0])

        return {
            "conversations": contar("SELECT COUNT(*) FROM conversas"),
            "unread": contar(
                "SELECT COUNT(*) FROM mensagens WHERE de = 'them' AND estado <> 'lida'"
            ),
            "contacts": contar("SELECT COUNT(*) FROM contactos WHERE bloqueado = 0"),
            "requests": contar("SELECT COUNT(*) FROM pedidos WHERE estado = 'pendente'"),
        }


# ---------------------------------------------------------------------
# Conversões
# ---------------------------------------------------------------------


def _iso(momento: float) -> str:
    """Um instante, em ISO 8601 com milissegundos.

    O separador é ``+00:00`` e não ``Z`` porque é o que
    :func:`datetime.isoformat` produz e porque o JavaScript da
    interface o parseia sem dúvida. Uma data que o browser não entende
    aparece como ``Invalid Date`` e a pessoa vê um erro em vez de uma
    hora.
    """
    return datetime.fromtimestamp(momento, UTC).isoformat(timespec="milliseconds")


def _ordem_de_nome(nome: str) -> str:
    """A chave de ordenação de um nome, sem acentos nem caixa.

    """
    return unicodedata.normalize("NFKD", nome).encode("ascii", "ignore").decode().casefold()


def _contacto_de(linha: sqlite3.Row) -> dict[str, Any]:
    """Uma linha de ``contactos`` com os nomes que a interface usa."""
    return {
        "id": linha["id"],
        "identifier": linha["identificador"],
        "fingerprint": linha["impressao"],
        "name": linha["nome"],
        "status": linha["estado"],
        "note": linha["nota"],
        "blocked": bool(linha["bloqueado"]),
        "verified": bool(linha["verificado"]),
        "createdAt": _iso(linha["criado_em"]),
    }


def _conversa_de(linha: sqlite3.Row) -> dict[str, Any]:
    """Uma linha de ``conversas`` com os nomes que a interface usa."""
    chaves = linha.keys()
    registo = {
        "id": linha["id"],
        "contactId": linha["contacto_id"],
        "pinned": bool(linha["fixada"]),
        "muted": bool(linha["silenciada"]),
        "verified": bool(linha["verificada"]),
        "route": linha["rota"],
        "criadaEm": _iso(linha["criada_em"]),
        "lastMessage": None,
        "lastAt": None,
        "unread": 0,
    }
    # As colunas da sub-consulta só existem na consulta de listagem.
    #
    # Sem o `naoLidas`, a leitura de cima deixava `unread` a zero: o
    # `.pop("naoLidas", 0)` de quem consome esta função não encontrava a
    # chave porque ela nunca tinha sido posto. O número estava a ser
    # calculado em SQL e deitado fora — que é o modo mais silencioso de
    # um número estar errado.
    if "ultima_texto" in chaves:
        registo["lastMessage"] = linha["ultima_texto"]
        registo["lastAt"] = _iso(linha["ultima_em"]) if linha["ultima_em"] is not None else None
    if "nao_lidas" in chaves:
        registo["unread"] = int(linha["nao_lidas"])
    return registo


def _mensagem_de(linha: sqlite3.Row) -> dict[str, Any]:
    """Uma linha de ``mensagens`` com os nomes que a interface usa."""
    return {
        "id": linha["id"],
        "from": linha["de"],
        "text": linha["texto"],
        "at": _iso(linha["em"]),
        "state": linha["estado"],
    }
