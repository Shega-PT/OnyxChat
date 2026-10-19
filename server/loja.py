# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
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
    envelope      BLOB,
    -- A que transferência esta linha pertence, quando é uma fatia.
    -- `NULL` é o caso comum — texto — e é o que impede a eliminação de
    -- uma transferência de levar o texto com ela. Ver
    -- `eliminar_transferencia`.
    transferencia_id TEXT REFERENCES transferencias(id) ON DELETE CASCADE
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

-- Transferências de media (`docs/media.md` §4). As **fatias** continuam a
-- ser linhas de `mensagens`: o `envelope` já existe lá e é ciphertext
-- (`store envelopes, not plaintext`), e uma transferência de 10 MB são
-- 204 linhas de mensagem que o utilizador **não** quer ver 204 vezes.
--
-- Esta tabela guarda o **manifesto** — os números, o resumo e o nome —
-- e é o que permite à interface mostrar «[vídeo] 42/204» e ao
-- `MEDIA_REPRODUZIR` saber o que montar.
--
-- `completo` é 0 enquanto falta alguma fatia. E uma transferência
-- incompleta **nunca** é reproduzida (`docs/media.md` §4.6): o leitor
-- receberia um ficheiro truncado e não saberia.
--
-- A eliminação de um lado **não** propaga: cada participante tem as
-- suas linhas, e não há — nem deve haver — canal de propagação. Ver
-- `docs/media.md` §4.5.
--
-- A tabela é declarada **depois** de `mensagens` mas é referenciada por
-- ela, e a ordem no `ESQUEMA` não importa: o sqlite resolve as chaves
-- estrangeiras no momento da escrita, não no da criação.
CREATE TABLE IF NOT EXISTS transferencias (
    id            TEXT PRIMARY KEY,        -- os 16 hex do resumo
    conversa_id   TEXT NOT NULL REFERENCES conversas(id) ON DELETE CASCADE,
    de            TEXT NOT NULL,           -- 'me' | 'them'
    tipo          TEXT NOT NULL,           -- 'audio' | 'video'
    codec         TEXT NOT NULL,
    formato       TEXT NOT NULL,
    nome          TEXT NOT NULL,
    total_bytes   INTEGER NOT NULL,
    total_pedacos INTEGER NOT NULL,
    bytes_por_pedaco INTEGER NOT NULL,
    duracao_s     INTEGER,
    sha256        BLOB NOT NULL,
    recebido_em   REAL NOT NULL,
    completo      INTEGER NOT NULL DEFAULT 0
);

-- Sem este índice, «as transferências desta conversa» é uma varredura
-- da tabela inteira. Ver a nota sobre `ESQUEMA` no topo deste ficheiro:
-- a conversa é a unidade de leitura, e o índice é o que a faz custar o
-- mesmo em todas as conversas.
CREATE INDEX IF NOT EXISTS idx_transferencias_conversa
    ON transferencias(conversa_id, recebido_em);


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
        self._migrar()
        self._gravar_modo()

    def _migrar(self) -> None:
        """Acrescenta às lojas já existentes o que o `ESQUEMA` não pode.

        `CREATE TABLE IF NOT EXISTS` não acrescenta colunas a uma tabela
        que já existe — é silencioso e deixa a base a funcionar com um
        esquema incompleto. A coluna `transferencia_id` de `mensagens` é
        o primeiro caso disto, e é um caso que **não** se pode ignorar:
        sem a coluna, `adicionar_mensagem` falha com `no such column` e
        a loja não abre.

        Adicionar uma coluna é barato em sqlite (a tabela não é
        reescrita), e `NOT NULL` sem valor por omissão é proibido de
        propósito: a coluna nova é `NULL` em todas as linhas antigas, e
        é isso que as torna «não pertencem a nenhuma transferência».
        """
        colunas = {
            linha["name"]
            for linha in self._cx.execute("PRAGMA table_info(mensagens)").fetchall()
        }
        if "transferencia_id" not in colunas:
            self._cx.execute(
                "ALTER TABLE mensagens ADD COLUMN transferencia_id TEXT"
                " REFERENCES transferencias(id) ON DELETE CASCADE"
            )
            self._cx.commit()

        # O índice vive aqui e não no `ESQUEMA` **por causa da ordem**:
        # numa loja antiga a coluna ainda não existe quando o
        # `executescript` corre, e um `CREATE INDEX` sobre uma coluna
        # inexistente falha com `no such column` — ou seja, a loja
        # antiga deixava de abrir. `IF NOT EXISTS` torna isto idempotente
        # para as duas lojas, a nova e a migrada.
        self._cx.execute(
            "CREATE INDEX IF NOT EXISTS idx_mensagens_transferencia"
            " ON mensagens(transferencia_id) WHERE transferencia_id IS NOT NULL"
        )
        self._cx.commit()

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
        transferencia_id: str | None = None,
    ) -> dict[str, Any]:
        """Acrescenta uma mensagem e devolve-a já montada.

        Marca a conversa como não lida quando a mensagem é de fora: é o
        que faz o número ao lado do nome aparecer. Uma conversa que fica
        sem número quando chega uma mensagem é uma conversa em que se
        perde a informação de que havia algo novo.

        :param transferencia_id: a transferência a que a linha pertence,
            quando é uma fatia. É ``None`` para o texto — e é o ``None``
            que impede ``eliminar_transferencia`` de levar o texto
            junto. Tem de existir uma transferência com esse ``id``:
            uma linha que aponte para uma transferência inexistente
            ficaria órfã, e uma órfã não se conta nem se apaga.
        :raises LojaInvalida: se a conversa não existir, se ``de`` não for
            ``me`` nem ``them``, ou se a transferência não existir.
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

        if transferencia_id is not None and self._cx.execute(
            "SELECT 1 FROM transferencias WHERE id = ?", (transferencia_id,)
        ).fetchone() is None:
            raise LojaInvalida(f"transferência inexistente: {transferencia_id}")
        novo_id = self._novo_id("m")
        agora = time.time()
        self._cx.execute(
            "INSERT INTO mensagens (id, conversa_id, de, texto, em, estado, envelope,"
            " transferencia_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                novo_id,
                conversation_id,
                de,
                texto,
                agora,
                estado,
                envelope,
                transferencia_id,
            ),
        )
        if de == "them":
            self._cx.execute("UPDATE conversas SET lida = 0 WHERE id = ?", (conversation_id,))
        self._cx.commit()

        linha = self._cx.execute(
            "SELECT * FROM mensagens WHERE id = ?", (novo_id,)
        ).fetchone()
        return _mensagem_de(linha)

    def registar_transferencia(
        self,
        conversation_id: str,
        de: str,
        *,
        id: str,
        tipo: str,
        codec: str,
        formato: str,
        nome: str,
        total_bytes: int,
        total_pedacos: int,
        bytes_por_pedaco: int,
        duracao_s: int | None,
        sha256: bytes,
        completo: bool = False,
    ) -> dict[str, Any]:
        """Regista o manifesto de uma transferência e devolve-o montado.

        **Idempotente pelo `id`**, e essa é a propriedade que faz a
        retoma funcionar: um receptor que pede «faltam 7 42 118» e volta
        a receber o mesmo manifesto actualiza a linha em vez de criar
        uma segunda, e as duas_transferências não se duplicam na lista
        de media da conversa.

        Um `id` é o `sha256` truncado (`docs/media.md` §4.3), e por isso
        dois emissores do mesmo ficheiro usam o mesmo `id`. Isso é o
        que permite retomar uma transferência que começou com outro
        emissor — que é o caso normal quando alguém reenvia porque a
        outra pessoa não recebeu.

        :param transferencia_id: a transferência a que a linha pertence,
            quando é uma fatia. É ``None`` para o texto — e é o ``None``
            que impede ``eliminar_transferencia`` de levar o texto
            junto. Tem de existir uma transferência com esse ``id``:
            uma linha que aponte para uma transferência inexistente
            ficaria órfã, e uma órfã não se conta nem se apaga.
        :raises LojaInvalida: se a conversa não existir, se ``de`` não for
            ``me`` nem ``them``, ou se a transferência não existir.
        """
        if de not in ("me", "them"):
            raise LojaInvalida(f"remetente inválido: {de!r} (tem de ser 'me' ou 'them')")
        if len(sha256) != 32:
            raise LojaInvalida(f"o resumo tem {len(sha256)} bytes, tem de ter 32")
        if total_pedacos < 0 or total_bytes < 0:
            raise LojaInvalida("totais negativos")

        if self._cx.execute(
            "SELECT 1 FROM conversas WHERE id = ?", (conversation_id,)
        ).fetchone() is None:
            raise LojaInvalida(f"conversa inexistente: {conversation_id}")

        self._cx.execute(
            "INSERT INTO transferencias (id, conversa_id, de, tipo, codec, formato,"
            " nome, total_bytes, total_pedacos, bytes_por_pedaco, duracao_s,"
            " sha256, recebido_em, completo)"
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            " ON CONFLICT(id) DO UPDATE SET"
            "   total_bytes = excluded.total_bytes,"
            "   total_pedacos = excluded.total_pedacos,"
            "   bytes_por_pedaco = excluded.bytes_por_pedaco,"
            "   duracao_s = excluded.duracao_s,"
            "   nome = excluded.nome"
            # `completo` **não** está no `UPDATE`: uma transferência que
            # já estava completa não volta a ficar incompleta por causa
            # de um manifesto reenviado. Reverter seria devolver o
            # ficheiro a um estado que já não existe na máquina.
            ,
            (
                id,
                conversation_id,
                de,
                tipo,
                codec,
                formato,
                nome,
                total_bytes,
                total_pedacos,
                bytes_por_pedaco,
                duracao_s,
                sqlite3.Binary(sha256),
                time.time(),
                1 if completo else 0,
            ),
        )
        self._cx.commit()
        return self.transferencia(id)

    def transferencia(self, transferencia_id: str) -> dict[str, Any]:
        """Um manifesto pelo seu `id`.

        :raises LojaInvalida: se não existir.
        """
        linha = self._cx.execute(
            "SELECT * FROM transferencias WHERE id = ?", (transferencia_id,)
        ).fetchone()
        if linha is None:
            raise LojaInvalida(f"transferência inexistente: {transferencia_id}")
        return _transferencia_de(linha)

    def transferencias(
        self, conversation_id: str, *, completas: bool | None = None
    ) -> list[dict[str, Any]]:
        """As transferências de uma conversa, mais recentes primeiro.

        ``completas`` filtra: ``True`` só as prontas, ``False`` só as
        incompletas, ``None`` todas. O filtro existe porque a
        pergunta da interface é quase sempre «o que posso tocar?» — e
        responder com uma transferência a meio seria oferecer um
        ficheiro que não existe.
        """
        sql = "SELECT * FROM transferencias WHERE conversa_id = ?"
        args: list[Any] = [conversation_id]
        if completas is not None:
            sql += " AND completo = ?"
            args.append(1 if completas else 0)
        sql += " ORDER BY recebido_em DESC"

        return [
            _transferencia_de(linha) for linha in self._cx.execute(sql, args).fetchall()
        ]

    def armazenamento(self, *, completas: bool | None = None) -> list[dict[str, Any]]:
        """**Todas** as transferências da conta, de todas as conversas.

        E a vista de armazenamento: «o que tenho guardado?».

        ## E porque é um método e não `transferencias()` sem conversa

        Porque [`transferencias`](Loja.transferencias) é **por
        conversa**, e a pergunta do armazenamento é por **conta**. São
        perguntas diferentes, e responder à segunda percorrendo as
        conversas seria a interface a fazer o trabalho da base de dados.

        ## E a quem pertence cada transferência

        A uma conta. A loja é de uma conta — `criar_tabela` cria o
        esquema todo e a conta é o que a abre — e por isso não há
        filtro de dono: **toda** a tabela é desta conta.

        E é por isso que a vista não precisa de pedir o dono: um
        `WHERE conta_id = ?` seria redundante, e um filtro que não
        filtra nada é pior do que não ter filtro — porque dá a ilusão
        de que o filtro existe.

        ## E o que devolve

        Uma linha por transferência, com o **nome de quem enviou** e o
        **nome da conversa** — porque um ficheiro sem contexto é
        «report.pdf» numa lista de trinta, e o contexto é o que
        distingue «o PDF que me mandaram» de «o PDF que eu mandei».
        """
        # O nome está no **contacto**, e não na conversa: a conversa é
        # uma ligação para o contacto, e o contacto é quem tem o nome.
        sql = (
            "SELECT t.*, ct.nome AS nome_contacto "
            "FROM transferencias t "
            "JOIN conversas c ON c.id = t.conversa_id "
            "JOIN contactos ct ON ct.id = c.contacto_id"
        )
        args: list[Any] = []
        if completas is not None:
            sql += " WHERE t.completo = ?"
            args.append(1 if completas else 0)
        sql += " ORDER BY t.recebido_em DESC"

        saida = []
        for linha in self._cx.execute(sql, args).fetchall():
            transferencia = _transferencia_de(linha)
            transferencia["contacto"] = linha["nome_contacto"]
            saida.append(transferencia)
        return saida

    def contar_pedacos(self, transferencia_id: str) -> int:
        """Quantas fatias já chegaram.

        Conta as linhas ligadas a esta transferência pelo
        `transferencia_id` — e **nem sequer olha para o `envelope`**. A
        primeira implementação filtrava por
        `conversa_id AND envelope IS NOT NULL`, e dava um número que não
        correspondia a nenhuma transferência; a segunda por
        `transferencia_id AND envelope IS NOT NULL`, e dava zero sempre
        que a linha tivesse sido gravada sem envelope.

        A ligação é o `transferencia_id` e nada mais. Um `envelope` é um
        dado **opcional** de uma linha — o texto não o tem, e o texto é
        a maioria das linhas — e contar por um campo opcional é contar
        por uma coisa que ninguém prometeu.

        A contagem é **aproximada por natureza** — é um `COUNT` de linhas
        presentes, não de `seq` distintos. A verdade é o `Montador` do
        receptor, que sabe os `seq` que faltam (`docs/media.md` §4.3), e
        é ele que decide o que reenviar. O `min` abaixo é o que impede
        um reenvio de contar duas vezes a mesma fatia como duas.
        """
        t = self.transferencia(transferencia_id)
        total = self._cx.execute(
            "SELECT COUNT(*) FROM mensagens WHERE transferencia_id = ?",
            (transferencia_id,),
        ).fetchone()[0]
        return min(int(total), t["totalPedacos"])

    def marcar_transferencia_completa(self, transferencia_id: str) -> None:
        """Marca a transferência como pronta a reproduzir.

        Chamar só depois de o resumo bater (`docs/media.md` §4.2). Sem
        essa verificação, ``completo`` é uma promessa e não um facto —
        e a interface ofereceria um ficheiro truncado.
        """
        self._cx.execute(
            "UPDATE transferencias SET completo = 1 WHERE id = ?", (transferencia_id,)
        )
        self._cx.commit()

    def eliminar_transferencia(self, transferencia_id: str) -> None:
        """Apaga a transferência e **as suas mensagens**.

        As mensagens vão com ela porque são as fatias, e deixar as
        fatias depois de apagar o manifesto seria deixar 204 linhas de
        envelope que não pertencem a nada.

        E não propaga para o outro participante: cada um tem as suas
        linhas, e um `DELETE` que chegasse ao outro lado seria uma
        capacidade de destruição remota (`docs/media.md` §4.5).

        As fatias saem pelo `transferencia_id`, **não** por um filtro de
        conversa. A primeira versão filtrava por
        `conversa_id AND envelope IS NOT NULL`, e apagava tudo o que
        tivesse `envelope` na conversa: o texto encriptado e as outras
        transferências. Um «apagar um vídeo de 10 MB» que levasse a
        conversa inteira é o género de bug que só aparece quando já
        custou dados.
        """
        self.transferencia(transferencia_id)
        self._cx.execute(
            "DELETE FROM mensagens WHERE transferencia_id = ?", (transferencia_id,)
        )
        self._cx.execute("DELETE FROM transferencias WHERE id = ?", (transferencia_id,))
        self._cx.commit()

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
    # Acções de conversa — os botões do menu de contexto
    # -----------------------------------------------------------------
    #
    # As colunas `fixada`, `silenciada` e `bloqueado` existem no esquema
    # desde o início e eram **lidas** sem nunca serem escritas: a interface
    # as mostrava e nenhum caminho as alterava. Um campo que se lê e não
    # se escreve é um campo em que a interface está a mentir — mostra um
    # estado que nada produz.
    #
    # Todas usam o mesmo formato: uma instrução, um `commit`, e nada mais.
    # A ausência de retorno não é descuido: quem chama recarrega a lista.

    def fixar_conversa(self, conversation_id: str, valor: bool) -> None:
        """Fixa ou desfixa a conversa.

        A lista de conversas ordena as fixadas primeiro, de modo que
        fixar é uma operação que muda a ordem e não só um ícone. Ver
        :meth:`conversas`.
        """
        self._cx.execute(
            "UPDATE conversas SET fixada = ? WHERE id = ?",
            (1 if valor else 0, conversation_id),
        )
        self._cx.commit()

    def silenciar_conversa(self, conversation_id: str, valor: bool) -> None:
        """Silencia ou dessilencia a conversa.

        Silenciar é uma promessa de não ser perturbado; a diferença é
        que o utilizador tem de conseguir desfazê-la, e por isso o valor
        é explícito em vez de um botão «alternar».
        """
        self._cx.execute(
            "UPDATE conversas SET silenciada = ? WHERE id = ?",
            (1 if valor else 0, conversation_id),
        )
        self._cx.commit()

    def limpar_historico(self, conversation_id: str) -> None:
        """Apaga as mensagens da conversa, sem tocar em nada mais.

        A conversa fica, porque a relação com o contacto é o que a pessoa
        não quer perder — é a conversa que se limpa, não o contacto. Uma
        pessoa que limpa o histórico continua a ter o contacto, e a
        próxima mensagem recomeça a conversa sem desfazer o bloqueio nem
        a fixação.

        :raises LojaInvalida: se a conversa não existir.
        """
        self.conversa(conversation_id)  # levanta se não existir
        self._cx.execute(
            "DELETE FROM mensagens WHERE conversa_id = ?", (conversation_id,)
        )
        self._cx.execute(
            "UPDATE conversas SET lida = 1 WHERE id = ?", (conversation_id,)
        )
        self._cx.commit()

    def bloquear_contacto(self, contacto_id: str, valor: bool) -> None:
        """Bloqueia ou desbloqueia o contacto.

        Bloquear um contacto é mais do que esconder a conversa: o daemon
        tem de recusar o envio. Este método escreve a intenção; quem a
        faz chegar ao daemon é o ``ipc.rs`` através do comando de
        bloqueio, e é aí que a recusa passa a ser do protocolo e não
        apenas de um filtro do ecrã.

        A conversa em si não muda de estado — o que muda é o contacto, e
        a conversa herda. Silenciar, pelo contrário, é por conversa.

        :raises LojaInvalida: se o contacto não existir.
        """
        linha = self._cx.execute(
            "SELECT id FROM contactos WHERE id = ?", (contacto_id,)
        ).fetchone()
        if linha is None:
            raise LojaInvalida(f"contacto inexistente: {contacto_id}")
        self._cx.execute(
            "UPDATE contactos SET bloqueado = ? WHERE id = ?",
            (1 if valor else 0, contacto_id),
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


def _transferencia_de(linha: sqlite3.Row) -> dict[str, Any]:
    """Converte uma linha de ``transferencias`` no dicionário do JSON.

    Os nomes saem em ``camelCase`` porque é o que a interface lê — a
    conversão está em ``rotas.camel`` e esta função segue a mesma
    convenção que as outras três, para que um campo novo não precise de
    uma decisão tomada outra vez.

    O ``sha256`` vai em hexadecimal e não em base-64: é o valor que
    aparece na resposta de retoma e no registo, e hexadecimal é o que
    um humano copia sem partir. O resumo são 32 bytes e a base-64 eram
    44 caracteres com dois especiais no fim.
    """
    sha = bytes(linha["sha256"])
    return {
        "id": linha["id"],
        "conversaId": linha["conversa_id"],
        "de": linha["de"],
        "tipo": linha["tipo"],
        "codec": linha["codec"],
        "formato": linha["formato"],
        "nome": linha["nome"],
        "totalBytes": int(linha["total_bytes"]),
        "totalPedacos": int(linha["total_pedacos"]),
        "bytesPorPedaco": int(linha["bytes_por_pedaco"]),
        "duracaoS": linha["duracao_s"],
        "sha256": sha.hex(),
        "recebidoEm": _iso(linha["recebido_em"]),
        "completo": bool(linha["completo"]),
    }


def _mensagem_de(linha: sqlite3.Row) -> dict[str, Any]:
    """Uma linha de ``mensagens`` com os nomes que a interface usa."""
    return {
        "id": linha["id"],
        "from": linha["de"],
        "text": linha["texto"],
        "at": _iso(linha["em"]),
        "state": linha["estado"],
        # `null` no texto, o `id` da transferência numa fatia. É o que
        # permite à interface mostrar uma transferência **uma vez** em
        # vez de 204 vezes, e o que faz a eliminação saber o que é
        # seu. Ver `docs/media.md` §4.4.
        "mediaId": linha["transferencia_id"],
    }
