# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""origem.py — quem tem o direito de falar com o sidecar.

Este módulo é a fronteira de segurança do sidecar, e é o ficheiro mais
importante de `server/` depois do próprio `sidecar.py`. A razão é que um
servidor em ``127.0.0.1`` **não é privado**.

## O problema, sem dramatização

O sidecar escuta em ``127.0.0.1:8787`` e serve a identidade, a impressão
digital e a cópia de segurança cifrada. A interface corre no mesmo
utilizador, no mesmo computador, no mesmo browser.

O browser **não** impede um `fetch` para esse endereço. Isto é permitido
pela especificação e é o que torna qualquer serviço local um alvo:

.. code-block:: javascript

    // Numa página qualquer, em qualquer sítio. Só precisa de o utilizador
    // ter o OnyxChat aberto noutro separador.
    const r = await fetch('http://127.0.0.1:8787/api/conta/exportar');
    const copia = await r.json();          // a cópia de segurança cifrada
    await fetch('http://127.0.0.1:8787/api/mensagens', { method: 'POST', ... });

Não há CORS que impeça o **envio** — só impede a **leitura** da resposta.
O que protege é o servidor recusar o pedido, e é o que este módulo faz.

## As quatro defesas, e o que cada uma apanha

``Host`` — ataque de *rebinding* de DNS
    O atacante serve ``atacante.example`` com um TTL de zero. Ao fim de
    alguns segundos o nome resolve para ``127.0.0.1``, e o browser believe
    que continua na origem hostil — mas o pedido chega ao sidecar com
    ``Host: atacante.example``. Um servidor local que não olhe para o
    ``Host`` não tem como distinguir esse pedido de um legítimo.

``Origin`` — o caso geral
    Toda a chamada de JavaScript para outra origem leva o ``Origin`` da
    página que a fez. Um ``fetch`` da mesma origem leva
    ``http://127.0.0.1:8787``; um de ``https://atacante.example`` leva o
    nome do atacante. Comparar é uma operação, e recusa-se tudo o que não
    seja a nossa própria origem.

``Sec-Fetch-Site`` — o reforço
    O browser envia este cabeçalho em todas as pedidos e diz a relação
    real: ``same-origin``, ``cross-site``, ``none``. Nenhum cliente
   pagável o consegue forjar, ao contrário do ``Origin`` (que um cliente
    HTTP bruto escreve à vontade). Quando vem, decide; quando não vem,
    decide o ``Origin``.

``Sec-Fetch-Mode``/``Method`` — o que não é leitura
    Um pedido sem ``Origin`` é quase sempre uma navegação, uma imagem ou
    um ``<form>`` — não uma chamada de JavaScript. Navegar é inofensivo;
    o que não é inofensivo é ``POST``/``PUT``/``DELETE`` sem ``Origin``.
    Esses são recusados sempre.

## O que este módulo NÃO faz

- **Não exige um segredo.** Não há token, não há cookie, não há nada que
  a página tenha de ir buscar. Um segredo que a página lê de um sítio que
  a página não pode ler seria um segredo bem guardado; lido do mesmo
  sítio de onde vem a resposta, não esconde nada. A defesa toda está em
  recusar, não em identificar.
- **Não protege contra quem está no próprio Chromium.** Um complemento
  malicioso, o DevTools aberto, ou uma pessoa com acesso à conta do
  utilizador passam por tudo isto. Isso é verdade de qualquer programa
  local, e vale a pena dizê-lo em vez de fingir que a barra é mais alta.

## A excepção única: o ``Origin`` ausente em navegação

Um pedido de navegação não leva ``Origin``, por definição — o browser só o
manda em chamadas que fazem script. Recusar o ``Origin`` ausente
destruiria a aplicação. Por isso:

``GET``/``HEAD`` sem ``Origin``
    Aceito, depois de o ``Host`` passar. É navegação ou recurso; não há
    efeito de estado.

``POST``/``PUT``/``DELETE``/``PATCH`` sem ``Origin``
    Recusado. Não há legitimate browser que faça isto, e um ``<form>``
    cross-site **tem** ``Origin`` — é o que o distingue de um ``curl``
    local, que é exactamente quem não queremos a escrever no sidecar.

## Como se prova que isto funciona

O teste certo não é «a função devolve ``True``». É uma tentativa de ataque
real, escrita como tal: um pedido com ``Origin`` de atacante tem de ser
recusado com ``403`` e a resposta **não pode** conter a identidade. Está em
``tests/test_sidecar.py``.
"""

from __future__ import annotations

import os

__all__ = [
    "AMBIENTE_EXTRAS",
    "METODOS_COM_EFFECTO",
    "METODOS_DE_LEITURA",
    "ORIGENS_PERMITIDAS",
    "Decisao",
    "avaliar",
]


#: Os únicos métodos que **não** mudam estado.
#:
#: A lista é de leitura, e não de escrita, porque uma lista de escrita é
#: um *fail-open*: amanhã alguém acrescenta um método novo ao router e
#: esquece-se desta constante, e o método novo passa a ser tratado como
#: leitura — a decision a dizer que não há efeito quando há.
#:
#: Já aconteceu neste projecto, noutro ficheiro: um guard de licença
#: excluía ``UI/tools/node`` comparando o caminho contra o directório
#: corrente em vez do raiz, e a exclusão estava escrita mas inerte. A
#: lição aplica-se a toda a lista de excepções.
#:
#: Qualquer método fora daqui é tratado como se mudasse estado, que é a
#: leitura correcta para um método que o sidecar não conhece.
METODOS_DE_LEITURA = frozenset({"GET", "HEAD", "OPTIONS"})

#: Mantido por compatibilidade de leitura com o que a documentação e os
#: testes diziam antes da inversão. Deriva de :data:`METODOS_DE_LEITURA`.
METODOS_COM_EFFECTO = frozenset({"POST", "PUT", "PATCH", "DELETE"})


def _tem_efeito(metodo: str) -> bool:
    """O método muda estado?

    Falha para o lado seguro: só os três métodos de leitura são isentos.
    """
    return metodo not in METODOS_DE_LEITURA


#: As origens que a aplicação serve a si mesma.
#:
#: ``localhost`` e ``127.0.0.1`` são o mesmo endereço escrito de duas
#: maneiras, e o browser trata-os como origens **distintas**. A interface
#: é servida em ``127.0.0.1`` (ver `docs/sidecar.md`), e a origem fica por
#: omissão em ``127.0.0.1`` — mas quem abrir o URL à mão com ``localhost``
#: tem de funcionar igual, ou a defesa passa a ser um problema em vez de
#: uma protecção.
ORIGENS_PERMITIDAS = frozenset(
    {
        "http://127.0.0.1:8787",
        "http://localhost:8787",
        "http://[::1]:8787",
    }
)

#: Variável de ambiente com origens adicionais, separada por vírgulas.
#:
#: Existe para um caso só: desenvolver a interface com o servidor de
#: desenvolvimento do Vite, que fica noutra porta e por isso tem outra
#: origem. Sem isto, `npm run dev` não conseguiria falar com o sidecar, e
#: a resposta natural de quem está a desenvolver seria desligar a defesa —
#: o que transformaria uma excepção de desenvolvimento numa excepção em
#: toda a instalação.
#:
#: **Não está ligada por omissão, e ligar-se é uma decisão consciente.**
#: A lista é lida em cada avaliação para que mudar a variável faça efeito
#: sem reiniciar, e uma origem inválida é ignorada em vez de fazer o
#: sidecar recusar tudo — um erro de escrita que torna o programa
#: inutilizável é pior do que um erro de escrita que abre uma origem a
#: mais.
AMBIENTE_EXTRAS = "ONYX_ORIGENS"


def _origens(porta: int) -> frozenset[str]:
    """As origens permitidas na porta dada, com as excepções de ambiente.

    :param porta: a porta em que o sidecar escuta.
    """
    permitidas = {
        f"http://127.0.0.1:{porta}",
        f"http://localhost:{porta}",
        f"http://[::1]:{porta}",
    }

    for extra in os.environ.get(AMBIENTE_EXTRAS, "").split(","):
        limpa = _normalizar_origem(extra)
        if limpa:
            permitidas.add(limpa)

    return frozenset(permitidas)


class Decisao:
    """O resultado de avaliar um pedido.

    É uma dataclass e não um booleano porque a resposta ao cliente muda com
    a causa: ``403`` com ``403 Forbidden`` e um corpo vazio para um
    ``Origin`` estranho é a resposta certa; a mesma coisa com um ``Origin``
    ausente num pedido de escrita é um ``400``, porque o pedido está
    malformado em vez de ser hostil. Dizer a diferença ao cliente honesto
    ajuda-o a corrigir-se.
    """

    __slots__ = ("permitido", "codigo", "motivo")

    def __init__(self, permitido: bool, codigo: int = 200, motivo: str = "") -> None:
        self.permitido = permitido
        #: O código HTTP a devolver quando ``permitido`` é falso.
        self.codigo = codigo
        #: Explicação, só para os registos. **Nunca** vai para o corpo da
        #: resposta: dizer a um atacante *qual* das quatro defesas o apanhou
        #: é dar-lhe o trabalho de tentar as outras.
        self.motivo = motivo

    def __bool__(self) -> bool:
        return self.permitido

    def __repr__(self) -> str:  # pragma: no cover - só para depuração
        return (
            f"Decisao(permitido={self.permitido!r}, codigo={self.codigo!r}, "
            f"motivo={self.motivo!r})"
        )


def _normalizar_origem(valor: str) -> str:
    """Baixa a caixa e tira a barra final, para comparar sem surprises.

    A especificação diz que a origem não tem barra final, mas um
    ``Origin`` escrito à mão por uma ferramenta pode trazer uma, e um
    servidor local que recusa o seu próprio nome por causa de uma barra é
    um servidor que a pessoa vai contornar — desligando-o.
    """
    return valor.strip().rstrip("/").lower()


def _host_aceite(cabecalho_host: str | None, porta: int) -> bool:
    """O ``Host`` é um dos nossos?

    A lista é explícita e não «qualquer coisa que resolvede para o
    loopback»: a verificação existe para apanhar o *rebinding*, e o
    rebinding traz sempre um nome stranger. Admitir ``localhost`` é uma
    conveniência deliberada — é o endereço que as pessoas escrevem.
    """
    if not cabecalho_host:
        # HTTP/1.0 e cliente sem Host. Uma chamada de browser traz sempre.
        return False

    permitido = {
        f"127.0.0.1:{porta}",
        f"localhost:{porta}",
        f"[::1]:{porta}",
    }
    return cabecalho_host.strip().lower() in permitido


def avaliar(
    metodo: str,
    cabecalhos: dict[str, str],
    porta: int = 8787,
) -> Decisao:
    """Decide se o pedido pode ser servido.

    A ordem das verificações é deliberada: ``Host`` primeiro, porque um
    pedido com o ``Host`` errado é manifestamente de outro sítio, e não
    vale a pena gastar nele um ``Origin`` de aspecto legítimo. Um atacante
    que faça *rebinding* controla o ``Origin`` que quiser — controlar o
    ``Origin`` não o faz passar pelo ``Host``, que é precisamente por isso
    que a verificação do ``Host`` vem primeiro.

    :param metodo: método HTTP, em maiúsculas.
    :param cabecalhos: cabeçalhos do pedido. As chaves são comparadas em
        minúsculas, e o valor é sensível às maiúsculas quando é URL.
    :param porta: a porta em que o sidecar escuta.
    :returns: :class:`Decisao`.
    """
    normalizados = {k.lower(): v for k, v in cabecalhos.items()}
    metodo = metodo.upper()
    tem_efeito = _tem_efeito(metodo)

    # --- 1. Host, contra rebinding de DNS -----------------------------
    host = normalizados.get("host")
    if not _host_aceite(host, porta):
        return Decisao(False, 403, f"Host não reconhecido: {host!r}")

    # --- 2. Origin, quando vem ----------------------------------------
    #
    # O `Origin` é **autoritário**. É o browser que o preenche. Um
    # cliente HTTP bruto pode escrever o nosso `Origin` à vontade, mas
    # então tem de passar o passo 1, e o passo 1 não se deixa enganar por
    # cabeçalhos: é o `Host` que decide a que endereço o pedido se
    # destinava, e nenhum `Origin` o altera.
    #
    # Uma página que falsificasse o nosso `Origin` deixaria de ser uma
    # página: seria um programa. E um programa na mesma máquina já não
    # precisa de falsificar o `Origin` para fazer o que quer.
    #
    # Por isso, quando o `Origin` está presente e é o nosso, o pedido
    # passa e o passo 3 nem é consultado. Uma versão anterior deste
    # código invertia a ordem e dava poder de veto ao `Sec-Fetch-Site`,
    # que é o inverso do que se quer: esse cabeçalho **acrescenta** defesa,
    # não tira permissão.
    origem = normalizados.get("origin")
    if origem is not None:
        if _normalizar_origem(origem) in {_normalizar_origem(o) for o in _origens(porta)}:
            return Decisao(True)
        return Decisao(False, 403, f"Origin não permitida: {origem!r}")

    # --- 3. Sem Origin: o `Sec-Fetch-Site` é a única pista ---------------
    #
    # Chegou aqui sem `Origin`, o que significa navegação, recurso
    # (`<img>`, `<script>`) ou um cliente bruto.
    sitio = normalizados.get("sec-fetch-site")
    if sitio is not None:
        sitio = sitio.lower()
        if sitio == "cross-site":
            # Recurso de outra origem. Ler a resposta é bloqueado pela
            # política de origem do browser, por isso passar aqui é
            # inofensivo — e recusar partiria a interface, que carrega
            # recursos de si própria.
            if not tem_efeito:
                return Decisao(True)
            return Decisao(False, 403, "método com efeito e Sec-Fetch-Site: cross-site")

        # `same-origin`, `same-site` e `none` com leitura são todos
        # navegação ou recurso: não há efeito de estado a proteger.
        if not tem_efeito:
            return Decisao(True)

        # Com efeito, `same-origin` é a aplicação a falar consigo mesma.
        if sitio == "same-origin":
            return Decisao(True)

        # `none` com efeito é navegação com `<form>`, e `same-site` com
        # efeito é outra aplicação no mesmo computador. A aplicação não
        # faz nenhuma das duas coisas.
        return Decisao(False, 403, f"método com efeito e Sec-Fetch-Site: {sitio}")

    # --- 4. Nem Origin nem Sec-Fetch-Site -------------------------------
    #
    # Leitura: navegação ou recurso. Passa, depois do `Host` ter passado.
    if not tem_efeito:
        return Decisao(True)

    # Efeito sem nenhum dos dois cabeçalhos: nenhum browser faz isto.
    return Decisao(False, 403, f"{metodo} sem Origin e sem Sec-Fetch-Site")
