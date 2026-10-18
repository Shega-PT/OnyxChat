# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""controle.py — o que uma chamada diz, e como um ficheiro viaja.

O convite de uma chamada e as fatias de um ficheiro **não** são
molduras ``ONX1`` e não são texto de conversa. São envelopes cujo corpo
é uma linha ASCII versionada, cifrados com a chave derivada do seu tipo
(``call`` e ``ficheiro``, ver :mod:`messenger.amizade`).

Este módulo é a implementação Python do formato; o daemon não o conhece,
porque para o daemon um envelope de controlo é indistinguível de um
qualquer — e é essa a propriedade que o ``docs/media.md`` §1.2 exige.

## O invariante, e porque é este módulo que o sustenta

    O que sai para a rede é um ``FRAME_CHAT`` com um envelope normal.
    A rede não sabe se é texto, um convite de chamada ou um ficheiro.

A chave derivada é o que separa os três, e é por isso que
:mod:`messenger.amizade` tem quatro tipos e não dois. O controlo não
introduziu um caminho de rede novo, um opcode novo, nem um tipo de frame
novo: introduce uma **linha de texto** que só o par sabe abrir.

## E porque ASCII e não JSON

Porque um leitor humano tem de conseguir desdobrar um problema. Um
convite que não abre aparece nos testes como ``ONYX-CALL/1 RING
9f3a…`` — legível — e um JSON apareceria como uma parede de aspas e
barras invertidas com a mesma informação.

O custo é real e está escrito em :data:`CAMPOS_ESPERADOS`: uma chave
por linha, e chaves repetidas não são permitidas, porque a segunda
seria silenciosamente ignorada e o emissor pensaria que a tinha
enviado.

## A versão no início

``ONYX-CALL/1`` e ``ONYX-FICHEIRO/1``. A versão é o que permite que uma
linha antiga seja recusada com «versão desconhecida» em vez de ser
malinterpretada. E o número é ``1`` porque é a primeira versão; o dia em
que o formato mudar, sobe para ``2`` e as linhas antigas continuam a
abrir — o que é o mesmo princípio do handshake (``docs/handshake.md``
§Tolerância de versões).

## E o que este módulo **não** faz

* Não cifra. O :mod:`messenger.pipeline` e o daemon fazem-no.
* Não decide quem pode chamar quem. Isso é da sessão, e a sessão lê
  isto.
* Não guarda estado. Um ``CONTROLAR`` é uma função pura de bytes em
  bytes: quem chama com a mesma linha recebe a mesma resposta, e uma
  linha malformada levanta ``ErroControlo`` em vez de devolver meia
  coisa.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

__all__ = [
    "VERSAO_CALL",
    "VERSAO_FICHEIRO",
    "PREFIXO_CALL",
    "PREFIXO_FICHEIRO",
    "TAM_ID_SESSAO",
    "TAM_ID_TRANSFERENCIA",
    "COMANDOS_CALL",
    "CAMPOS_ESPERADOS",
    "ErroControlo",
    "Comando",
    "linha_call",
    "linha_ficheiro",
    "abrir",
    "e_de_call",
    "e_de_ficheiro",
    "sessao_nova",
]

#: A versão do formato de controlo de chamada.
#:
#: Congelada: uma linha escrita com a versão ``1`` tem de continuar a
#: abrir depois de o formato mudar, e a única forma disso é uma versão
#: que não muda.
VERSAO_CALL = "1"

#: A versão do formato de controlo de ficheiro.
VERSAO_FICHEIRO = "1"

#: O que começa uma linha de controlo de chamada.
PREFIXO_CALL = f"ONYX-CALL/{VERSAO_CALL}"

#: O que começa uma linha de controlo de ficheiro.
PREFIXO_FICHEIRO = f"ONYX-FICHEIRO/{VERSAO_FICHEIRO}"

#: Os bytes do identificador de sessão.
#:
#: 16 bytes são 128 bits, e 128 bits é o que a RFC 4226 recomenda para
#: um valor que **não** pode ser adivinhado. A sessão é o que amarra o
#: convite ao media que vem a seguir, e por isso tem de ser aleatória
#: e não sequencial: com um contador, quem resolve o par descobre a sessão
#: seguinte.
TAM_ID_SESSAO = 16

#: Os hexadecimais do identificador de sessão, como texto.
#:
#: O dobro de :data:`TAM_ID_SESSAO`, e é o que vai na linha. Texto e
#: não bytes porque uma linha ASCII tem de ser imprimível, e o custo
#: de 2 bytes por byte é irrelevante numa linha que viaja uma vez por
#: chamada.
TAM_HEX_SESSAO = TAM_ID_SESSAO * 2

#: Os bytes do identificador de transferência, que é o `sha256` da
#: primeira metade do conteúdo — o mesmo que a rota não-streaming já
#: usa, e pelo mesmo motivo: é o que permite a dois lados falar do
#: mesmo ficheiro sem um segundo canal de controlo.
TAM_ID_TRANSFERENCIA = 16

#: Os hexadecimais do identificador de transferência.
TAM_HEX_TRANSFERENCIA = TAM_ID_TRANSFERENCIA * 2

#: Os comandos de uma chamada, e o que cada um pede ao receptor.
#:
#: A lista é **fechada** e é a ordem que o estado da chamada percorre.
#: Um comando fora daqui é erro, e não um comando futuro que se
#: ignora: um `RING` que ninguém reconhece ficaria à espera de resposta
#: para sempre.
COMANDOS_CALL = {
    # O emissor está a tocar. É o único comando que pode chegar sem
    # resposta, e por isso é o único que um par desligado não
    # responde.
    "RING": "convite a falar",
    "ACCEPT": "a chamada foi aceite",
    "REJECT": "a chamada foi recusada",
    # O emissor desistiu. Vem de quem tocou, e é o que impede um
    # convite órfão quando o outro lado desaparece a meio.
    "CANCEL": "quem tocou desistiu",
    # Fim de chamada, por quem quer que seja. Não é o mesmo que
    # `CANCEL`: `CANCEL` é antes de haver chamada, `BYE` é depois.
    "BYE": "a chamada terminou",
}

#: Os campos que cada comando aceita, e em que posições.
#:
#: ## Porque uma lista e não ``**kwargs``
#:
#: Porque ``linha_call("RING", destino="x")`` compila e produz uma
#: linha que ninguém sabe ler. Uma lista é conferida em tempo de
#: execução, e o erro de escrita é um ``TypeError`` com o nome do
#: campo — não um convite que fica à espera.
#:
#: ## A sessão **não** está nesta lista
#:
#: Porque é o **argumento** de :func:`linha_call`, e não um campo a
#: combinar. Toda a linha de chamada leva a sessão na posição 2, e ela
#: ter de estar na lista de campos seria repetir em cinco linhas o que
#: a assinatura já diz — e um sítio que se repete é um sítio que
#: diverge.
#:
#: ## E porque a ordem importa
#:
#: Porque a linha é posicional. ``RING`` leva dois campos e ``ACCEPT``
#: não leva nenhum, e uma ordem fixa é o que impede dois emissores de
#: produzirem linhas diferentes para o mesmo comando.
CAMPOS_ESPERADOS = {
    "RING": ("video", "audio"),
    "ACCEPT": (),
    "REJECT": (),
    "CANCEL": (),
    "BYE": (),
}


class ErroControlo(ValueError):
    """Uma linha de controlo que não é válida.

    A excepção é um ``ValueError`` e não um tipo novo, pela mesma razão
    que em :mod:`messenger.amizade`: quem chama está a passar bytes de
    uma rede, e uma hierarquia de excepções para quatro casos não paga
    o que custa.

    O que importa é que é sempre **explícita**. Uma linha malformada
    que devolvesse um comando vazio faria um receptor tratar uma
    mensagem de texto como um convite, e o sintoma — uma chamada que
    toca sozinha — não aponta para o parser.
    """


#: Um identificador de sessão: 32 hexadecimais minúsculos.
#:
#: A expressão é deliberadamente mais apertada do que ``[0-9a-f]{32}``:
#: ``fullmatch`` garante que a linha inteira é o identificador. Um
#: ``search`` aceitaria um prefixo válido dentro de uma linha maior, e
#: isso é um convite de outra coisa.
RE_ID = re.compile(r"[0-9a-f]{32}")


def sessao_nova(bits: bytes | None = None) -> str:
    """Um identificador de sessão novo, em hex.

    Com ``bits`` dados, é a segunda parte de um teste de vectores — e o
    teste passa a ser reprodutível. Sem eles, são 16 bytes do
    ``os.urandom``, que é a única fonte que serve: um identificador de
    sessão previsível deixa quem observa as chamadas entrar nelas.

    :returns: 32 caracteres hex minúsculos.
    :raises ErroControlo: se ``bits`` não tiver :data:`TAM_ID_SESSAO`
        bytes.
    """
    import os as _os

    bruto = _os.urandom(TAM_ID_SESSAO) if bits is None else bytes(bits)
    if len(bruto) != TAM_ID_SESSAO:
        raise ErroControlo(
            f"a sessão são {TAM_ID_SESSAO} bytes, e chegaram {len(bruto)}"
        )
    return bruto.hex()


@dataclass(frozen=True)
class Comando:
    """Uma linha de controlo já desdobrada.

    O dataclass é **imutável** (``frozen=True``) e o motivo é o mesmo
    de :class:`messenger.media.Manifesto`: um comando que muda depois
    de ter sido cifrado é um comando que não é o que foi assinado.

    :param tipo: ``"call"`` ou ``"ficheiro"``.
    :param accao: o que se pede — em ``call``, uma chave de
        :data:`COMANDOS_CALL`.
    :param sessao: o identificador de sessão, em hex. Vazio nos
        comandos de ficheiro, que não têm sessão.
    :param campos: os campos que o comando leva, por nome.
    """

    tipo: str
    accao: str
    sessao: str = ""
    campos: dict[str, str] = field(default_factory=dict)

    def __post_init__(self) -> None:
        """Confere a forma, para que um comando inválido não exista.

        O objecto é imutável, por isso a validação vai no
        ``__post_init__`` e não numa função separada: um
        :class:`Comando` malformado que chegasse ao emissor seria
        cifrado e não abriria do outro lado, e o sintoma seria um erro
        de descifragem sem causa.
        """
        if self.tipo not in ("call", "ficheiro"):
            raise ErroControlo(f"tipo de controlo desconhecido: {self.tipo!r}")
        if self.sessao and not RE_ID.fullmatch(self.sessao):
            raise ErroControlo(f"sessão mal formada: {self.sessao!r}")


def linha_call(acao: str, sessao: str, **campos: str) -> str:
    """A linha de um comando de chamada.

    :param acao: uma chave de :data:`COMANDOS_CALL`.
    :param sessao: 32 hexadecimais, de :func:`sessao_nova`.
    :param campos: os que :data:`CAMPOS_ESPERADOS` espera para ``acao``.

    :raises ErroControlo: se ``acao`` não for conhecida, se a sessão
        não tiver a forma, ou se os campos não baterem.
    """
    if acao not in COMANDOS_CALL:
        raise ErroControlo(
            f"comando de chamada desconhecido: {acao!r} "
            f"(conhecidos: {', '.join(sorted(COMANDOS_CALL))})"
        )
    if not RE_ID.fullmatch(sessao):
        raise ErroControlo(f"sessão mal formada: {sessao!r}")

    esperados = CAMPOS_ESPERADOS[acao]
    if set(campos) != set(esperados):
        faltam = sorted(set(esperados) - set(campos))
        sobram = sorted(set(campos) - set(esperados))
        detalhe = []
        if faltam:
            detalhe.append(f"faltam {', '.join(faltam)}")
        if sobram:
            detalhe.append(f"a mais {', '.join(sobram)}")
        esperado_txt = ", ".join(esperados) or "nenhum campo"
        raise ErroControlo(f"«{acao}» quer {esperado_txt} — " + " e ".join(detalhe))

    valores = [sessao] + [_campo(acao, nome, campos[nome]) for nome in esperados]
    return " ".join([PREFIXO_CALL, acao] + valores)


def linha_ficheiro(acao: str, identificador: str, **campos: str) -> str:
    """A linha de um comando de ficheiro.

    :param acao: ``"MANIFESTO"`` ou ``"PEDACO"``.
    :param identificador: 32 hexadecimais do ``sha256``.
    :param campos: os do comando.
    """
    if acao not in ("MANIFESTO", "PEDACO"):
        raise ErroControlo(f"comando de ficheiro desconhecido: {acao!r}")
    if not RE_ID.fullmatch(identificador):
        raise ErroControlo(f"identificador mal formado: {identificador!r}")

    valores = [identificador] + [f"{k}={v}" for k, v in sorted(campos.items())]
    return " ".join([PREFIXO_FICHEIRO, acao] + valores)


def _campo(acao: str, nome: str, valor: str) -> str:
    """Valida um campo antes de o meter na linha.

    A validação é do **formato**, e não do significado: ``video`` tem de
    ser ``vp8`` ou ``vp9`` porque o receptor vai escolher o descodificador
    por isso, e um valor novo que ninguém sabe descodificar é melhor
    recusado na escrita.
    """
    if nome in ("video", "audio") and valor not in ("vp8", "vp9", "opus"):
        raise ErroControlo(f"«{acao}» não sabe {nome} «{valor}»")
    if not valor or " " in valor:
        raise ErroControlo(f"o campo «{nome}» é {valor!r} e não pode ter espaço")
    return valor


def abrir(linha: str) -> Comando | None:
    """Dobra uma linha de controlo, ou devolve ``None`` se não for uma.

    ``None`` é a resposta a «esta linha não é de controlo» e não a
    «esta linha é inválida». A diferença importa: o receptor vai
    sondar o texto, a media, o ``call`` e o ``ficheiro``, e uma linha de
    texto normal tem de dar ``None`` sem erro.

    Uma linha que **começa** por um prefixo de controlo e depois está
    malformada levanta ``ErroControlo``. Truncar em ``None`` deixaria
    um convite corrompido passar a conversa, e o receptor mostraria um
    convite que nunca toca.

    :raises ErroControlo: se a linha começa por um prefixo conhecido e
        o resto não é válido.
    """
    if not linha.startswith((PREFIXO_CALL + " ", PREFIXO_FICHEIRO + " ")):
        return None

    partes = linha.split(" ")
    tipo = "call" if linha.startswith(PREFIXO_CALL) else "ficheiro"
    accao = partes[1]

    if tipo == "call":
        if accao not in COMANDOS_CALL:
            raise ErroControlo(
f"comando de chamada desconhecido: {accao!r} "
                f"(conhecidos: {', '.join(sorted(COMANDOS_CALL))})"
            )
        campos = CAMPOS_ESPERADOS[accao]
        # `2 + 1 + n` é o prefixo, o comando e a sessão. A sessão não é
        # um campo combinável — é o segundo argumento de
        # `linha_call` — e por isso não está em `CAMPOS_ESPERADOS`.
        if len(partes) != 3 + len(campos):
            levantados = len(partes) - 3
            quer = len(campos)
            if levantados < quer:
                raise ErroControlo(
                    f"«{accao}» quer {quer} campo(s) e a linha traz {levantados}"
                )
            raise ErroControlo(
                f"«{accao}» quer {quer} campo(s) e a linha traz {levantados}"
            )
        return Comando(
            tipo="call",
            accao=accao,
            sessao=partes[2],
            campos=dict(zip(campos, partes[3:])),
        )

    # Ficheiro: o identificador e pares ``chave=valor``.
    identificador = partes[2]
    campos = {}
    for parte in partes[3:]:
        chave, sep, valor = parte.partition("=")
        if not sep or not chave or not valor:
            raise ErroControlo(f"campo de ficheiro mal formado: {parte!r}")
        if chave in campos:
            raise ErroControlo(f"o campo «{chave}» vem duas vezes")
        campos[chave] = valor
    return Comando(tipo="ficheiro", accao=accao, campos=campos)


def e_de_call(linha: str) -> bool:
    """Se a linha é de controlo de chamada, **sem** a desdobrar.

    Existe para quem só quer filtrar e não quer tratar `None`: o
    receptor tem de decidir o que é antes de tentar abrir.
    """
    return linha.startswith(PREFIXO_CALL + " ")


def e_de_ficheiro(linha: str) -> bool:
    """Se a linha é de controlo de ficheiro, sem a desdobrar."""
    return linha.startswith(PREFIXO_FICHEIRO + " ")