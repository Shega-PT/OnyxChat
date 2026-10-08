# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""discovery_server.py — mapeia ``{ID → .onion}`` com TTL efémero.

O servidor de descoberta é **propositalmente mínimo**: guarda apenas
pares `identificador → endereço .onion` durante ``ttl`` segundos e
nunca vê chaves, mensagens ou envelopes (ver ``docs/discovery.md`` e
``docs/handshake.md``). Ao expirar, a entrada desaparece — os clients
voltam a registar-se (hidden services efémeros).

Interface HTTP local (JSON), duas rotas:

* ``POST /registrar`` — corpo ``{"id", "onion", "ttl"?}`` → 200/400;
* ``GET /onion/<id>`` → 200 ``{"id", "onion"}`` ou 404.

O núcleo (:class:`Descoberta`) é puro e testável com relógio
injetado; o transport HTTP usa ``http.server`` da biblioteca padrão.
"""

from __future__ import annotations

import json
import re
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

#: TTL por omissão das entradas (segundos).
TTL_PREDEFINIDO = 300.0

#: Formato de um endereço .onion v3 (56 caracteres base32 + ".onion").
_ONION = re.compile(r"^[a-z2-7]{56}\.onion$")

#: Identificador aceite: 1–64 caracteres.
#:
#: O conjunto inclui os símbolos da cauda do identificador Onyx
#: (``!@$%*+~=``). Não é uma distinção arbitrária: sem eles, o servidor de
#: descoberta rejeitaria **o identificador que o próprio projecto produz**,
#: e a cadeia ``identificador → registo na descoberta → Finding peer``
#: partiria no primeiro passo.
#:
#: O que continua excluído, e é o que interessa:
#:
#: ``#``
#:     Delimitador de fragmento. Numa URL, um ``#`` no meio do valor
#:     truncá-lo-ia a partir daí. Mas o identificador Onyx **termina** em
#:     ``#`` — é o fecho do formato, desenhado com o logótipo. Por isso o
#:     ``#`` é aceite apenas na última posição, e a expressão regular
#:     escreve isso em vez de confiar em validação a jusante.
#:
#: ``&``
#:     Separador de parâmetros. Um ``&`` cortaria a lista no endereço
#:     errado.
#:
#: ``/`` e ``\``
#:     Separadores de caminho. Um identificador com ``/`` atravessaria-o
#:     e sairia do mapa.
#:
#: espaço, ``%``, ``?``
#:     Quebram a percentagem-codificação de qualquer jeito, mas um
#:     identificador legível nunca deve precisar de ser escapado.
#:
#: A classe de corpo usa o mesmo alfabeto da cauda em
#: ``messenger/identidade.py``, e o limite de 64 caracteres continua
#: valendo: o identificador mais longo tem 18.
#: As duas formas admitidas, em alternativa explícita: até 64
#: caracteres de corpo, ou até 63 **mais um** ``#`` de fecho. Um ``{1,63}#?``
#: seria mais curto e **errado** — rejeitaria um identificador de 64
#: caracteres sem fecho, que é uma forma válida.
_IDENTIFICADOR = re.compile(
    r"^(?:[A-Za-z0-9._~!@$*+=@-]{1,64}|[A-Za-z0-9._~!@$*+=@-]{1,63}#)$"
)


class DescobertaInvalida(ValueError):
    """Registo/consulta inválido (formato de ID ou .onion errado, TTL ≤ 0)."""


class Descoberta:
    """Núcleo do mapa ``ID → .onion`` com expiração por TTL.

    ``relogio`` é injetável — os testes avançam o tempo sem dormir.
    """

    def __init__(
        self,
        ttl_padrao: float = TTL_PREDEFINIDO,
        relogio: Callable[[], float] = time.monotonic,
    ) -> None:
        if ttl_padrao <= 0:
            raise DescobertaInvalida("o TTL tem de ser positivo")
        self._ttl_padrao = ttl_padrao
        self._relogio = relogio
        self._entradas: dict[str, tuple[str, float]] = {}

    @property
    def total(self) -> int:
        """Entradas vivas (inclui as que ainda não foram limpas)."""
        return len(self._entradas)

    def registar(
        self, identificador: str, onion: str, ttl: float | None = None
    ) -> None:
        """Regista/substitui ``identificador`` → ``onion`` com ``ttl`` segundos."""
        if not isinstance(identificador, str) or not _IDENTIFICADOR.match(
            identificador
        ):
            raise DescobertaInvalida("identificador inválido")
        if not isinstance(onion, str) or not _ONION.match(onion):
            raise DescobertaInvalida("endereço .onion inválido")
        duracao = self._ttl_padrao if ttl is None else ttl
        if duracao <= 0:
            raise DescobertaInvalida("o TTL tem de ser positivo")
        self._entradas[identificador] = (onion, self._relogio() + duracao)

    def consultar(self, identificador: str) -> str | None:
        """Devolve o ``.onion`` vivo, ou ``None`` (desconhecido/expirado)."""
        entrada = self._entradas.get(identificador)
        if entrada is None:
            return None
        onion, limite = entrada
        if limite <= self._relogio():
            # Entrada expirada: remoção imediata — TTL efémero por design.
            del self._entradas[identificador]
            return None
        return onion

    def limpar(self) -> int:
        """Remove todas as entradas expiradas; devolve quantas saíram."""
        agora = self._relogio()
        expiradas = [
            chave
            for chave, (_onion, limite) in self._entradas.items()
            if limite <= agora
        ]
        for chave in expiradas:
            del self._entradas[chave]
        return len(expiradas)


# ---------------------------------------------------------------------
# Transport HTTP (JSON)
# ---------------------------------------------------------------------


class _Manipulador(BaseHTTPRequestHandler):
    """Rotas ``GET /onion/<id>`` e ``POST /registrar`` sobre ``server.descoberta``."""

    def _responder(self, estado: int, dados: dict[str, object]) -> None:
        """Escreve uma resposta JSON com o ``estado`` HTTP indicado."""
        corpo = json.dumps(dados).encode("utf-8")
        self.send_response(estado)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(corpo)))
        self.end_headers()
        self.wfile.write(corpo)

    @property
    def _descoberta(self) -> Descoberta:
        """Núcleo associado ao servidor HTTP (atribuído em ``criar_servidor``)."""
        return self.server.descoberta  # type: ignore[attr-defined]

    def do_GET(self) -> None:
        """``GET /onion/<id>`` → 200 com o .onion, ou 404."""
        partes = urllib.parse.urlsplit(self.path).path.strip("/").split("/")
        if len(partes) == 2 and partes[0] == "onion":
            onion = self._descoberta.consultar(partes[1])
            if onion is None:
                self._responder(404, {"erro": "desconhecido ou expirado"})
            else:
                self._responder(200, {"id": partes[1], "onion": onion})
        else:
            self._responder(404, {"erro": "rota desconhecida"})

    def do_POST(self) -> None:
        """``POST /registrar`` com JSON ``{"id", "onion", "ttl"?}``."""
        if urllib.parse.urlsplit(self.path).path != "/registrar":
            self._responder(404, {"erro": "rota desconhecida"})
            return
        try:
            comprimento = int(self.headers.get("Content-Length", ""))
        except ValueError:
            self._responder(400, {"erro": "comprimento em falta"})
            return
        bruto = self.rfile.read(comprimento)
        try:
            dados = json.loads(bruto.decode("utf-8"))
            self._descoberta.registar(
                dados["id"],
                dados["onion"],
                dados.get("ttl"),
            )
        except (
            UnicodeDecodeError,
            json.JSONDecodeError,
            KeyError,
            TypeError,
            DescobertaInvalida,
        ) as erro:
            self._responder(400, {"erro": str(erro)})
            return
        self._responder(200, {"ok": True})

    def log_message(self, formato: str, *argumentos: object) -> None:
        """Silencia o log por omissão do ``http.server`` (sem ruído em testes)."""
        pass


def criar_servidor(
    host: str = "127.0.0.1",
    porta: int = 0,
    descoberta: Descoberta | None = None,
) -> ThreadingHTTPServer:
    """Cria o servidor HTTP (``porta=0`` → porta efémera, úteis em testes)."""
    servidor = ThreadingHTTPServer((host, porta), _Manipulador)
    servidor.descoberta = (  # type: ignore[attr-defined]
        descoberta if descoberta is not None else Descoberta()
    )
    return servidor


def consultar_descoberta(servidor: str, identificador: str) -> str | None:
    """Cliente HTTP: devolve o ``.onion`` de ``identificador`` ou ``None`` (404).

    Falhas de rede/protocolo levantam :class:`DescobertaInvalida` — o
    chamador distingue «não conheço» (``None``) de «não consegui
    perguntar» (exceção).
    """
    url = f"{servidor.rstrip('/')}/onion/{urllib.parse.quote(identificador)}"
    try:
        with urllib.request.urlopen(url, timeout=5) as resposta:
            corpo = json.loads(resposta.read().decode("utf-8"))
    except urllib.error.HTTPError as erro:
        if erro.code == 404:
            return None
        raise DescobertaInvalida(
            f"servidor devolveu HTTP {erro.code}"
        ) from erro
    except OSError as erro:
        raise DescobertaInvalida(f"servidor inacessível: {erro}") from erro
    return corpo["onion"]


# Ponto de entrada de subprocesso; `criar_servidor` e as rotas são
# testados directamente (não há como unit-testar `serve_forever` sem
# bloquear) — por isso, pragma de cobertura no bloco.
if __name__ == "__main__":  # pragma: no cover
    servidor = criar_servidor(porta=8789)
    print(f"descoberta a ouvir em {servidor.server_address}")
    servidor.serve_forever()
