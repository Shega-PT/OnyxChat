# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

r"""O ID Onyx e a impressão digital.

Este módulo é a referência: o cliente Python e a interface em JavaScript
têm de produzir **os mesmos valores**, e ``tests/vectors/identidade.json``
congela o valor de ambos os lados.

## As duas coisas, e porque são diferentes

.. code-block:: text

    sal       = 16 bytes aleatórios, gerados uma vez e guardados
    semente    = SHA-256(b"ONYX/ID/v1" ‖ sal ‖ utilizador_normalizado)
    identificador = "ONYX-" ‖ 6 caracteres ‖ "-" ‖ 4 caracteres ‖ "#"
    impressão   = SHA-256(b"ONYX/FP/v1" ‖ identificador)[:16]

O **identificador** é o que se partilha com outras pessoas para as
encontrar. A **impressão** é interna: é o valor que a rede guarda para
detectar que dois identificadores colidem.

São derivados um do outro em cadeia, e não ambos do utilizador, por uma
razão concreta: a impressão tem de poder ser recalculada a partir de
apenas o identificador. É isso que permite a quem tenha o ID, e só o ID,
verificar que esse ID pertence ao dono que diz ser — sem acesso a nenhum
segredo, e sem que o ID revele o nome de utilizador que o originou.

## Porque é que o sal é obrigatório

Sem sal, o identificador é ``SHA-256("ONYX/ID/v1" ‖ "ana")``. E
``SHA-256`` é rápido: um atacante com a lista dos mil nomes de utilizador
mais comuns no mundo calcula os seus mil identificadores em menos de um
segundo, e a partir daí sabe que ``ONYX-8F3Q5S-!R4%D2#`` é a «ana».

Isto não é hypothetical. O objectivo declarado de um identificador é
**não revelar a identidade real**, e um resumo rápido de um nome curto é
exactamente o que revela.

Com um sal de 16 bytes por conta, o resumo passa a depender de um valor
que **não sai da máquina** e que o atacante não tem. O resumo continua a
ser determinístico para quem tem o sal — que é o dono, e o sidecar — e
irreversível para quem não tem.

## A normalização do nome de utilizador

NFKC e depois ``casefold()``.

A normalização Unicode tem de acontecer porque um mesmo nome pode chegar
de várias formas — o S maiúsculo com cedilha da língua turca (U+015E) tem
cinco formas de composição, e todas têm de dar o mesmo identificador.

E é ``casefold``, não ``lower``, porque em várias línguas o ``lower`` não
é o inverso do ``upper``. O caso que falha sempre é o do carácter alemão
que se lê «es afiado» e não tem maiúscula própria (U+00DF): com ``lower``
fica «es afiado» em minúsculas; com ``casefold`` vira «ss». Um serviço de
identidade que usasse ``lower`` trataria quem escreve «strasse» e quem
escreve «strasse» com aquele caractere como duas pessoas — e as duas têm
o mesmo nome.

Os caracteres não são escritos aqui à mão porque o repertório do
repositório é o português; nos testes e nos vectores aparecem como
``\uXXXX``, que é a forma correcta de introduzir um carácter de fora do
repertório sem o introduzir.

## A parte que não é única: o bloco de seis caracteres

O formato é fixo pelo desenho da aplicação — seis caracteres de
identidade e uma cauda de quatro — e seis caracteres de um alfabeto de
32 dão ``32**6 ≈ 1,07 × 10⁹`` combinações. Por coincidência do
princípio do aniversário, duas pessoas colidem com probabilidade de 50%
a partir de cerca de **32 700** utilizadores.

Isto não é uma falha, é a repartição de funções que o projecto fez: o
bloco visível serve para **encontrar** alguém, e a impressão serve para
**distinguir**. Uma impressão de 128 bits não colide até ao fim do
universo observável, e é por isso que é a impressão que se guarda.

Dizer o número aqui é deliberado. Um bloco de seis caracteres que
«parece único» sem o ser é a mesma classe de erro que uma impressão
digital cosmetica.

## Os rótulos de domínio

``b"ONYX/ID/v1"`` e ``b"ONYX/FP/v1"``. São o que impede que o resumo do
identificador seja usado como resumo da impressão, ou vice-versa, e o
que permite que uma versão futura mude de fórmula sem colidir com a
antiga. É a mesma convenção de ``amizade.py``, ``p2p.rs`` e do resto do
protocolo: um rótulo por derivação, sempre.
"""

from __future__ import annotations

import hashlib
import os
import re
import unicodedata

__all__ = [
    "DOMINIO_ID",
    "DOMINIO_IMPRESSAO",
    "ALFABETO",
    "SIMBOLOS",
    "TAM_SAL",
    "TAM_BLOCO",
    "TAM_CAUDA",
    "TAM_IMPRESSAO",
    "FORMA_IDENTIFICADOR",
    "IdentidadeInvalida",
    "gerar_sal",
    "normalizar_utilizador",
    "identificador_de",
    "impressao_de",
]

#: Rótulo de derivação do identificador. Separado de
#: `DOMINIO_IMPRESSAO` para que um nunca possa ser usado no lugar do
#: outro.
DOMINIO_ID = b"ONYX/ID/v1"

#: Rótulo de derivação da impressão digital.
DOMINIO_IMPRESSAO = b"ONYX/FP/v1"

#: Alfabeto do bloco de identidade e das posições alfanuméricas da cauda.
#:
#: Trinta e dois caracteres, deliberadamente. Um número **próprio** de duas
#: potências permite consumir cinco bits por caractere sem resto e sem
#: viés — trinta e cinco daria a mesma leitura com `módulo`, e trinta e
#: quatro exigiria mais aritmética para o mesmo resultado.
#:
#: Não entram o `I`, o `O`, o `0` nem o `1`. As quatro exclusões são um
#: problema só: são os caracteres que se trocam entre si quando um ID é
#: lido em voz alta ou copiado de um sítio para outro. O `0` fica
#: ambíguo sem o `O`, e o `1` sem o `I` — por isso saem os dois.
ALFABETO = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"

#: Símbolos das posições pares da cauda. Oito caracteres, três bits.
#:
#: Não entram três símbolos, por razões diferentes e todas reais:
#:
#: ``#``
#:     É o terminador do identificador. Com ele dentro da cauda, um
#:     identificador como ``ONYX-F2Y2SZ-@T#3#`` tem dois ``#`` e deixa de
#:     se saber onde a cauda acaba. Um formato que não se consegue
#:     desdobrar não é um formato.
#:
#: ``-``
#:     O separador entre o bloco e a cauda. Com ele dentro da cauda, um
#:     identificador como ``ONYX-2P3BFT--8@3#`` tem dois traços e deixa
#:     de se conseguir separar em três partes. Foi um teste de forma a
#:     apanhar — e é o mesmo defeito do ``#``, com o mesmo reparo.
#:
#: ``&``
#:     Separador de parâmetros em endereço. Um identificador copiado para
#:     um endereço fica truncado, e o destinatário vê metade do ID e
#:     conclui que é inválido.
#:
#: O conjunto tem de ser **exactamente oito** caracteres: a cauda consome
#: três bits por símbolo, e um alfabeto de sete deixaria o índice sete
#: sem destino.
#:
#: Nenhum carácter repetido nem ambíguo: um símbolo que se confunda com
#: outro transforma dois identificadores distintos num só, e é o peer que
#: deixa de encontrar a pessoa.
SIMBOLOS = "!@$%*+~="

#: Tamanho do sal, em bytes.
TAM_SAL = 16

#: Caracteres do bloco de identidade.
TAM_BLOCO = 6

#: Caracteres da cauda de verificação.
TAM_CAUDA = 4

#: Bytes da impressão digital. Dezasseis — 128 bits.
TAM_IMPRESSAO = 16

#: A forma de um identificador Onyx, como expressão regular.
#:
#: Fica escrita a partir de :data:`ALFABETO` e :data:`SIMBOLOS` de propósito.
#: Reescrevê-la à mão seria criar uma segunda definição do formato, e as
#: duas divergiriam sem que nada as notice — que é o que aconteceu com a
#: validação anterior, que aceitava ``ONYX-<qualquer-coisa>#``.
#:
#: A cauda alterna símbolo e letra: símbolo, letra, símbolo, letra. Cada
#: par encerra em letra, o que impede que a leitura bloqueada se
#: ambiguamente com o fim do identificador.
FORMA_IDENTIFICADOR = re.compile(
    "^ONYX-"
    + f"[{re.escape(ALFABETO)}]{{{TAM_BLOCO}}}"
    + "-"
    + f"[{re.escape(SIMBOLOS)}][{re.escape(ALFABETO)}]"
    + f"[{re.escape(SIMBOLOS)}][{re.escape(ALFABETO)}]"
    + "#$"
)


class IdentidadeInvalida(ValueError):
    """Utilizador vazio, ou resultado que não cumpre o formato."""


def gerar_sal() -> bytes:
    """Gera um sal novo.

    Chamada **uma vez**, na criação da conta. O sal não é derivado de
    nada: a função dele é ser imprevisível, e voltar a gerá-lo muda o
    identificador de quem já o tinha.

    :returns: ``TAM_SAL`` bytes aleatórios.
    """
    return os.urandom(TAM_SAL)


def normalizar_utilizador(utilizador: str) -> str:
    """Reduz o nome de utilizador à forma canónica.

    :param utilizador: nome tal como foi escrito.
    :returns: a forma canónica, em UTF-8.
    :raises IdentidadeInvalida: se ficar vazio.
    """
    if not isinstance(utilizador, str):
        raise IdentidadeInvalida("o utilizador tem de ser texto")

    normalizado = unicodedata.normalize("NFKC", utilizador.strip()).casefold()
    if not normalizado:
        raise IdentidadeInvalida("o utilizador não pode ficar vazio")
    return normalizado


def _bits(dados: bytes, inicio: int, total: int) -> int:
    """Lê ``total`` bits de ``dados`` a partir do bit ``inicio``.

    Ordem de leitura: bit mais significativo primeiro, dentro de cada
    byte. A ordem está escrita porque a leitura de bits é o único sítio
    desta derivação onde duas implementações podem divergir sem que uma
    delas pareça errada — todos os bytes são usados, todos os caracteres
    são válidos, e a diferença é invisível.

    :param dados: sequência de origem.
    :param inicio: índice do primeiro bit, contado desde o princípio.
    :param total: quantidade de bits a ler.
    :returns: inteiro sem sinal de ``total`` bits.
    """
    valor = 0
    for passo in range(total):
        posicao = inicio + passo
        byte = dados[posicao // 8]
        bit = (byte >> (7 - (posicao % 8))) & 1
        valor = (valor << 1) | bit
    return valor


def identificador_de(utilizador: str, sal: bytes) -> str:
    """Deriva o identificador Onyx de um nome de utilizador.

    :param usuario: nome de utilizador, ainda por normalizar.
    :param sal: sal de ``TAM_SAL`` bytes.
    :returns: o identificador, com a forma ``ONYX-AAAAAA-!X!A#``.
    :raises IdentidadeInvalida: se o utilizador ficar vazio, ou se o sal
        não tiver o tamanho certo.
    """
    if len(sal) != TAM_SAL:
        raise IdentidadeInvalida(
            f"o sal tem de ter {TAM_SAL} bytes, recebeu {len(sal)}"
        )

    normalizado = normalizar_utilizador(utilizador)
    semente = hashlib.sha256(
        DOMINIO_ID + sal + normalizado.encode("utf-8")
    ).digest()

    # Bloco de identidade: seis caracteres de cinco bits. Começa no bit 0
    # e acaba no bit 29, o que cabe nos primeiros quatro bytes.
    bloco = "".join(
        ALFABETO[_bits(semente, indice * 5, 5)] for indice in range(TAM_BLOCO)
    )

    # Cauda: quatro caracteres que alternam símbolo e alfanumérico.
    # Três bits para o símbolo, cinco para a letra — 2×3 + 2×5 = 16 bits,
    # que é o que os bytes 4 e 5 dariam se fossem usados directamente.
    # Sobe a seis bytes porque os bits não caem em fronteiras de byte.
    cauda = ""
    deslocamento = TAM_BLOCO * 5
    for indice in range(TAM_CAUDA):
        if indice % 2 == 0:
            cauda += SIMBOLOS[_bits(semente, deslocamento, 3)]
            deslocamento += 3
        else:
            cauda += ALFABETO[_bits(semente, deslocamento, 5)]
            deslocamento += 5

    return f"ONYX-{bloco}-{cauda}#"


def impressao_de(identificador: str) -> str:
    """Deriva a impressão digital a partir de um identificador.

    Não recebe o sal nem o nome de utilizador: é essa a propriedade que
    permite a quem tem o ID, e só o ID, confirmar que o ID é autêntico.

    :param identificador: o identificador Onyx, completo.
    :returns: ``TAM_IMPRESSAO`` bytes em hexadecimal, separados por
        dois-pontos — ``AA:BB:…:FF``.
    :raises IdentidadeInvalida: se o identificador não estiver na forma
        esperada.
    """
    if not FORMA_IDENTIFICADOR.match(identificador):
        raise IdentidadeInvalida(
            f"identificador fora da forma ONYX-…#: {identificador!r}"
        )

    resumo = hashlib.sha256(
        DOMINIO_IMPRESSAO + identificador.encode("utf-8")
    ).digest()[:TAM_IMPRESSAO]
    return ":".join(f"{octeto:02X}" for octeto in resumo)