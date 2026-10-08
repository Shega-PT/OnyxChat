# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Conta local: registo, entrada, eliminação e cópias de segurança.

Este módulo é a camada de acesso do OnyxChat, e é a primeira em que o
projecto tem de responder à pergunta «o que é uma conta?» sem a
translation de um serviço hospedado.

## O que uma conta é

Não é um nome de utilizador e uma palavra-passe guardados num servidor.
É **uma identidade criptográfica num ficheiro cifrado**:

- uma *seed* Ed25519 de 32 bytes (`messenger.keys.Identidade`), que é o
  que assina;
- um *sal* de 16 bytes por conta (`messenger.identidade.gerar_sal`), que
  é o que impede que dois «ana» tenham o mesmo Onyx ID;
- o Onyx ID e a impressão digital de 128 bits, derivados uma vez.

A frase de segurança não é uma credencial que se verifica contra um
registo: é a **chave** do keystore. Não há campo onde a comparar, e não
deveria haver — a verificação é a etiqueta AEAD do
`messenger.storage`, que é mais forte do que qualquer comparação.

## Porque é que não há recuperação por correio

Porque não há servidor para o qual mandar o correio. Um ecrã de
«recuperar» que pede um endereço de correio está a prometer uma
recuperação que o sistema não pode fazer, e promessas que o sistema não
cumpre são piores do que a ausência do ecrã.

O que existe, e é tudo o que existe:

1. **Tentar de novo.** A causa mais comum é uma maiúscula perdida.
2. **Restaurar de uma cópia de segurança.** Exportar é uma operação
   local: copia o ficheiro cifrado, que abre com a mesma frase.
3. **Recomeçar.** Cria uma identidade **nova**, que é uma pessoa
   diferente para quem já conhece esta. A interface diz isso na altura.

Um quarto caminho — «derive a identidade de uma frase-mestra em vez de
um ficheiro» — foi considerado e rejeitado por uma razão concreta: o
identificador é público, e um sal público transforma o identificador
público num oráculo de confirmação de nomes de utilizador. Quem conhece
o teu Onyx ID e o teu sal pode confirmar qualquer nome que sospeite,
tentando um por um. Mantendo o sal **dentro** do texto cifrado, quem tem
o identificador não tem o sal, e a busca não se pode fazer. É a razão de
o sal não ser um campo legível do registo.

## O que este módulo deliberadamente não faz

- **Não aceita palavras-passe.** Uma palavra-passe é uma coisa que se
  escolhe da cabeça, e o que protege o keystore é a entropia. A
  estimativa de :func:`estimar_entropia` existe para dizer ao utilizador
  o que está a fazer, não para o enganar: é um **limite inferior** de
  esforço, e está escrita para não ser lida como uma prova.
- **Não guarda o sal fora do texto cifrado.** Ver acima.
- **Não tem caminho de migração de palavras-passe.** Não há palavra-passe
  para migrar.
"""

from __future__ import annotations

import math
import os
import secrets
import shutil
import unicodedata
from dataclasses import dataclass, replace
from datetime import UTC, datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

from messenger import identidade as ident
from messenger import storage
from messenger.keys import Identidade

if TYPE_CHECKING:  # pragma: no cover — só para o anotador
    from messenger.ipc_client import ChavesAmizade

__all__ = [
    "ENTROPIA_MINIMA",
    "FRASE_MINIMA",
    "TAM_PUBLICA_HEX",
    "VERSAO_CONTA",
    "Amizade",
    "Conta",
    "ContaInvalida",
    "FraseForte",
    "caminho_conta",
    "conta_existe",
    "criar",
    "eliminar",
    "entrar",
    "estimar_entropia",
    "exportar",
    "gravar",
    "restaurar",
]


#: Versão do registo que este código escreve.
#:
#: Vive no interior do texto cifrado, e é diferente do formato do
#: keystore: o registo da conta é um dicionário nosso dentro de um blob
#: cujo formato é do `storage`. Se um dia o registo precisar de mudar de
#: forma sem recifrar, isto é onde isso se declara.
VERSAO_CONTA = 1

#: Comprimento mínimo da frase, em caracteres.
#:
#: Existe **a par** da estimativa de entropia, e não em vez dela. Uma
#: frase de oito caracteres muito varietyada e uma de trinta caracteres
#: todos diferentes passam ambas na estimativa; a segunda é incomparavelmente
#: melhor, e um limite só de comprimento puniria a primeira em excesso
#: enquanto a estimativa puniria a segunda o que deve.
FRASE_MINIMA = 12

#: Entropia estimada mínima, em bits.
#:
#: O valor é uma **decisão de interface**, não uma propriedade
#: criptográfica. Desce-se abaixo dos 128 bits do sal porque o objectivo
#: não é resistir a um atacante com capacidade de GPU dedicada — para isso
#: não há frase que baste, e a resposta é hardware. É para evitar que
#: alguém entre por uma frase que escolheria sem pensar.
ENTROPIA_MINIMA = 60.0

#: Comprimento da pública de uma amizade, em hex (32 bytes = 64 chars).
#:
#: Existe para validar o que é lido de um registo. Uma amizade com uma
#: pública de outro comprimento não é uma amizade, e aceitá-la daria um
#: `K9` de tamanho errado a cifrar — que o daemon recusa, mas só depois
#: de o bytes ter entrado no comando.
TAM_PUBLICA_HEX = 64


@dataclass(frozen=True)
class Amizade:
    """As chaves de uma amizade confirmada, guardadas cifradas.

    ## Onde vivem, e porquê dentro do keystore

    As 192 bytes de :class:`messenger.ipc_client.ChavesAmizade` saem de
    ``ACEITAR_AMIZADE`` e ``CONFIRMAR_AMIZADE`` e, até esta mudança, eram
    **impressas para o terminal e perdidas no fim do comando**. A
    consequência estava declarada em ``docs/handshake.md``: reiniciar o
    daemon obriga a refazer o handshake.

    A escolha é o keystore cifrado, e não a loja do sidecar, por três
    razões que valem a pena escrever:

    * **A cifra é a da frase de segurança.** As chaves de amizade
      cifram uma mensagem por par. Guardá-las num ``sqlite3`` ao lado —
      que é a opção descartada — punha material que só o daemon devia
      ter, num ficheiro que ninguém abre com uma frase;
    * **A cópia de segurança passa a cobri-las.** ``exportar`` copia o
      keystore inteiro. Numa segunda distribuição de ficheiros, a conta
      restaurada traria as amizades — o que é o que «repor uma cópia»
      promete em :doc:`conta`;
    * **Um ficheiro, uma frase.** A alternativa mais óbvia — um keystore
      separado só para as amizades — obriga a exportá-lo e restaurá-lo
      a par, e o ecrã de recuperação passa a ter duas operações em vez
      de uma.

    ## Porque é um valor dentro do registo e não uma tabela

    O registo é um dicionário JSON dentro de um blob cifrado, e
    :class:`Conta` é **congelado**. Acrescentar uma amizade devolve uma
    :class:`Conta` nova — é o mesmo contrato que a identidade tem, e a
    razão é a mesma: um objecto que se altera depois de lido é um
    objecto cujo estado já não é o que o ficheiro diz.

    ## Porque ``publica_hex`` é a chave

    A pública do par é o que identifica a amizade, e é pública: não é
    segredo e por isso não tem custo guardá-la em claro. O
    Onyx ID seria o mesmo, e a pública é o que o protocolo já valida
    (``ACEITAR_AMIZADE`` confere que a pública do pedido bate com o
    par). Duas amizades com a mesma pública são a mesma amizade, e a
    segunda substitui a primeira em vez de duplicar.
    """

    publica_hex: str
    k1_hex: str
    k5_proprio_hex: str
    k9_proprio_hex: str
    k5_par_hex: str
    k9_par_hex: str
    criada_em: str

    #: O que vai cifrado, nome por nome.
    #:
    #: São os nomes **do registo**, e não os dos atributos Python. A
    #: diferença — `k1` no ficheiro, `k1_hex` na memória — é deliberada:
    #: o que vai para o disco é o registo, e o `hex` é um pormenor da
    #: representação. Por isso os dois conjuntos são declarados à parte,
    #: e `de_dict` valida o primeiro e o `para_bytes` converte o segundo.
    #:
    #: A ordem do tuplo do daemon é `K1`, `K5` próprio, `K9` próprio,
    #: `K5` do par, `K9` do par, pública — 6 × 32 = 192 bytes.
    CAMPOS_REGISTO = (
        "publica",
        "k1",
        "k5_proprio",
        "k9_proprio",
        "k5_par",
        "k9_par",
    )
    #: Os mesmos campos, com o sufixo que têm no atributo.
    CAMPOS_ATRIBUTO = tuple(f"{c}_hex" for c in CAMPOS_REGISTO)

    def para_dict(self) -> dict[str, str]:
        """O registo tal como vai cifrado."""
        return {
            "publica": self.publica_hex,
            "k1": self.k1_hex,
            "k5_proprio": self.k5_proprio_hex,
            "k9_proprio": self.k9_proprio_hex,
            "k5_par": self.k5_par_hex,
            "k9_par": self.k9_par_hex,
            "criada_em": self.criada_em,
        }

    @staticmethod
    def de_dict(dados: dict[str, Any]) -> Amizade:
        """Lê uma amizade de um dicionário, validando o que falta.

        Um registo incompleto é recusado com :class:`ContaInvalida` em vez
        de ser aceite com campos vazios: uma amizade sem ``k9_par`` não é
        uma amizade a recuperar, é um ficheiro corrompido, e aceitá-la
        daria um ``K9`` vazio a cifrar com.
        """
        faltam = [c for c in Amizade.CAMPOS_REGISTO if c not in dados]
        if "criada_em" not in dados:
            faltam.append("criada_em")
        if faltam:
            raise ContaInvalida(
                f"amizade incompleta, faltam: {', '.join(sorted(set(faltam)))}"
            )
        for campo in Amizade.CAMPOS_REGISTO:
            valor = dados[campo]
            if not isinstance(valor, str) or len(valor) != TAM_PUBLICA_HEX:
                raise ContaInvalida(f"amizade com «{campo}» de comprimento errado")
        return Amizade(
            publica_hex=dados["publica"],
            k1_hex=dados["k1"],
            k5_proprio_hex=dados["k5_proprio"],
            k9_proprio_hex=dados["k9_proprio"],
            k5_par_hex=dados["k5_par"],
            k9_par_hex=dados["k9_par"],
            criada_em=str(dados["criada_em"]),
        )

    def para_bytes(self) -> dict[str, bytes]:
        """Os seis campos como bytes, na ordem do tuplo do daemon."""
        return {
            campo: bytes.fromhex(getattr(self, f"{campo}_hex"))
            for campo in self.CAMPOS_REGISTO
        }

    def para_chaves(self) -> ChavesAmizade:
        """Volta ao tuplo que o daemon devolveu.

        A conversão inversa existe porque o outro lado do código — a CLI,
        que vai cifrar mensagens com estas chaves — fala a língua do
        cliente IPC, e não a do registo.
        """
        from messenger.ipc_client import ChavesAmizade

        return ChavesAmizade(
            k1=bytes.fromhex(self.k1_hex),
            k5_proprio=bytes.fromhex(self.k5_proprio_hex),
            k9_proprio=bytes.fromhex(self.k9_proprio_hex),
            k5_par=bytes.fromhex(self.k5_par_hex),
            k9_par=bytes.fromhex(self.k9_par_hex),
            publica=bytes.fromhex(self.publica_hex),
        )

    @staticmethod
    def de_chaves(chaves: ChavesAmizade, criada_em: str | None = None) -> Amizade:
        """Converte o tuplo do daemon em registo cifrável.

        O ``criada_em`` é o instante em que **este cliente** fechou a
        amizade, e não uma data que venha do daemon: o daemon não a
        envia, e inventar uma seria preencher o registo com um valor
        que ninguém mediu.
        """
        from datetime import datetime

        stamp = criada_em or datetime.now(UTC).isoformat()
        return Amizade(
            publica_hex=chaves.publica.hex(),
            k1_hex=chaves.k1.hex(),
            k5_proprio_hex=chaves.k5_proprio.hex(),
            k9_proprio_hex=chaves.k9_proprio.hex(),
            k5_par_hex=chaves.k5_par.hex(),
            k9_par_hex=chaves.k9_par.hex(),
            criada_em=stamp,
        )


class ContaInvalida(ValueError):
    """Operação de conta pedida de forma impossível ou incoerente."""


class FraseForte(ContaInvalida):
    """A frase de segurança não cumpre os mínimos exigidos."""


@dataclass(frozen=True)
class Conta:
    """O registo de uma conta, já decifrado.

    Fica congelada: é o que se lê de um ficheiro, e um objecto que se
    pode alterar depois de lido é um objecto cujo estado já não é o que o
    ficheiro diz. Tudo o que é derivado — a pública da chave, a forma
    canónica do nome — é calculado em :meth:`__post_init__` e nunca
    guardado, para que não possa divergir do que o ficheiro contém.
    """

    utilizador: str
    identificador: str
    impressao: str
    salt_hex: str
    publica_hex: str
    criada_em: str
    versao: int = VERSAO_CONTA

    #: As amizades confirmadas, dentro do texto cifrado.
    #:
    #: Um `tuple` e não uma `list` porque a classe é congelada: uma
    #: `list` continuaria mutável por dentro de um dataclass
    #: `frozen=True`, que é a falha clássica desse padrão — o `frozen`
    #: impede `conta.amizades = …`, e não `conta.amizades.append(…)`.
    #: O valor por omissão é vazio para que um keystore anterior a esta
    #: mudança abra sem dar erro.
    amizades: tuple[Amizade, ...] = ()

    #: O nome na forma canónica, para comparação. Não é um campo: é
    #: derivado, e um campo poderia divergir do ``utilizador``.
    normalizado: str = ""

    def __post_init__(self) -> None:
        object.__setattr__(self, "normalizado", ident.normalizar_utilizador(self.utilizador))

    def para_dict(self) -> dict[str, Any]:
        """O registo tal como vai cifrado.

        A ordem das chaves é a da inserção porque
        ``storage.guardar_keystore`` faz ``sort_keys=True`` — o que torna
        a ordenação irrelevante para o ficheiro, e mantém esta função
        legível.
        """
        return {
            "tipo": "conta",
            "versao": self.versao,
            "utilizador": self.utilizador,
            "identificador": self.identificador,
            "impressao": self.impressao,
            "salt_hex": self.salt_hex,
            "publica_hex": self.publica_hex,
            "criada_em": self.criada_em,
            # A lista de amizades vai no mesmo dicionário, e por isso
            # na mesma cifra. Ver `Amizade` para o porque de não ser uma
            # segunda distribuição de ficheiros.
            "amizades": [a.para_dict() for a in self.amizades],
        }


    def com_amizade(self, amizade: Amizade) -> Conta:
        """Uma :class:`Conta` nova com ``amizade`` acrescentada.

        ## Porque é que devolve e não altera

        A classe é congelada, e por isso ``conta.amizades += (…)`` não
        existe. A alternativa — tornar a classe mutável — desfaria a
        garantia que a identidade tem: um registo lido de um ficheiro é
        exactamente o que o ficheiro diz, e passa a ser outra coisa
        depois de uma chamada a um método.

        Devolver uma instância nova mantém essa garantia e torna
        explícito onde o ficheiro é reescrito: só quem chama esta função
        e depois :func:`gravar` faz a alteração *=durável*.

        ## Substitui em vez de duplicar

        Duas amizades com a mesma ``publica_hex`` são a mesma
        amizade — e é o par que é a chave, não a ordem nem o instante.
        Uma segunda com a mesma pública **substitui** a primeira, que é
        o que acontece quando um handshake é refeito com o mesmo par: as
        chaves velhas deixaram de valer, e guardá-las ao lado das novas
        daria dois conjuntos para o mesmo contacto.
        """
        restantes = tuple(a for a in self.amizades if a.publica_hex != amizade.publica_hex)
        return replace(
            self,
            amizades=(*restantes, amizade),
        )

    def amizade_de(self, publica_hex: str) -> Amizade | None:
        """A amizade com ``publica_hex``, ou ``None``.

        A procura é linear porque a lista é de uma pessoa — meia dúzia
        de amizades na prática. Um índice por chave seria uma estrutura
        para resolver uma lista de seis elementos.
        """
        for amizade in self.amizades:
            if amizade.publica_hex == publica_hex:
                return amizade
        return None


def caminho_conta() -> Path:
    """O caminho do ficheiro de conta.

    A variável de ambiente existe pelo mesmo motivo que em
    :func:`user.config.caminho_config`: os testes precisam de escrever
    numdirectório temporário, e abrir o ficheiro a sério num teste é a
    forma mais rápida de perder a conta de alguém.

    O caminho por omissão é relativo ao directório do projecto, e não ao
    directório de onde o programa foi chamado. Um programa que dependesse
    do ``cwd`` encontraria a conta num sítio diferente conforme quem o
    lançou — que é uma forma de «conta desaparecida» que ninguém consegue
    depois explicar.

    :returns: o caminho, que pode não existir.
    """
    sobreposto = os.environ.get("ONYX_CONTA")
    if sobreposto:
        return Path(sobreposto)
    raiz = Path(__file__).resolve().parent.parent
    return raiz / "user" / "conta.keystore"


def conta_existe(caminho: Path | str | None = None) -> bool:
    """Há uma conta neste caminho?

    Só verifica a existência do ficheiro. **Não** é uma verificação de
    que a conta está intacta: um ficheiro truncado existe e não abre, e
    um ficheiro com uma frase errada abre mal. O que a interface precisa
    de saber aqui é apenas «devo mostrar registo ou entrada?», e para
    isso a existência do ficheiro é a pergunta certa.

    :param caminho: por omissão, :func:`caminho_conta`.
    """
    destino = Path(caminho) if caminho is not None else caminho_conta()
    return destino.is_file()


# ---------------------------------------------------------------------
# A força da frase de segurança
# ---------------------------------------------------------------------


def estimar_entropia(frase: str) -> float:
    """Estima os bits de entropia de uma frase, por cima.

    A fórmula é a entropia empírica do multiconjunto de caracteres,
    multiplicada pelo comprimento::

        H = −∑ pᵢ·log₂(pᵢ)       _bits ≈ H · comprimento_

    onde ``pᵢ`` é a frequência relativa de cada carácter distinto. É a
    **limite superior** do que a frase pode valer: dá o máximo que uma
    sequência com estas frequências teria, e ignora a ordem, os padrões
    e o dicionário.

    ## Porque é que estes limites são honestos

    - **«aaaa» dá zero.** Um carácter só tem entropia zero, e é o que
      acontece.
    - **«abcdefghijkl» dá cerca de 43 bits**, e não 12 × 4. Um alfabeto
      pequeno com todos os caracteres diferentes vale muito menos do que
      a multiplicação ingenua sugere.
    - **Palavras ditas em inglês dá um valor alto e a frase é fraca.**
      ``correct horse battery staple`` sai com mais de 100 bits por este
      cálculo e continua a ser adivinhável por um dicionário em segundos.
      É a limitação que a função declara: mede variedade, não adivinhação.

    Nenhuma estimativa de força resolve o problema dos dicionários. O que
    resolve é a frase ter de ser longa e ter de ser lembrada por quem a
    escolheu — e a interface dizer isso com todas as letras, em vez de
    mostrar um número e deixar a pessoa achar que está protegida.

    :param frase: a frase tal como foi escrita.
    :returns: bits estimados; ``0.0`` para a frase vazia.
    """
    if not frase:
        return 0.0

    contagens: dict[str, int] = {}
    for caractere in frase:
        contagens[caractere] = contagens.get(caractere, 0) + 1

    total = len(frase)
    entropia = -sum(
        (n / total) * math.log2(n / total) for n in contagens.values()
    )
    return entropia * total


def _exigir_frase(frase: str) -> None:
    """Verifica os mínimos, ou levanta :class:`FraseForte`.

    A ordem é comprimento, depois entropia. Comprimir a mensagem para
    «tem N caracteres» quando o problema é a variedade, ou «dá N bits»
    quando o problema é o comprimento, faz a pessoa corrigir a coisa
    errada — e uma frase de doze ``a`` não fica melhor por ser longa.
    """
    if len(frase) < FRASE_MINIMA:
        raise FraseForte(
            f"a frase de segurança tem de ter pelo menos {FRASE_MINIMA} caracteres "
            f"(tem {len(frase)})"
        )

    bits = estimar_entropia(frase)
    if bits < ENTROPIA_MINIMA:
        raise FraseForte(
            f"a frase de segurança dá cerca de {bits:.0f} bits estimados e o mínimo "
            f"é {ENTROPIA_MINIMA:.0f}. Não faltam caracteres: falta variedade, ou "
            "falta uma palavra ou frase que só você conheça."
        )


# ---------------------------------------------------------------------
# Registo
# ---------------------------------------------------------------------


def criar(
    utilizador: str,
    frase: str,
    caminho: Path | str | None = None,
    *,
    forcar: bool = False,
) -> Conta:
    """Cria a conta e grava o ficheiro cifrado.

    Idempotente por omissão: se já há uma conta, levanta
    :class:`ContaInvalida` em vez de a substituir. Substituir uma
    identidade sem o mandar é o modo mais rápido de perder a conta de
    alguém — e como cada identidade é uma pessoa para os contactos, é
    também o modo de desaparecer sem o dar por isso.

    A identidade Ed25519 e o sal são gerados aqui, uma vez, e nunca
    reaproveitados: reutilizar o sal entre contas seria exactamente o
    defeito que o sal existe para evitar.

    :param utilizador: o nome tal como quer ser mostrado.
    :param frase: a frase de segurança; é a chave do keystore.
    :param caminho: destino; por omissão, :func:`caminho_conta`.
    :param forcar: substituir uma conta existente.
    :raises ContaInvalida: se já houver conta e ``forcar`` for falso, ou
        se o destino não for um ficheiro.
    :raises FraseForte: se a frase não cumprir os mínimos.
    :raises IdentidadeInvalida: se o nome ficar vazio.
    :returns: a conta criada.
    """
    destino = Path(caminho) if caminho is not None else caminho_conta()

    if conta_existe(destino) and not forcar:
        raise ContaInvalida(
            f"já existe uma conta em {destino}. Criar outra é criar outra pessoa: "
            "para recomeçar, elimine a existente de propósito com `eliminar`."
        )

    _exigir_frase(frase)

    # `normalizar_utilizador` levanta se o nome ficar vazio, e a validação
    # da forma do identificador vem do próprio módulo de identidade.
    ident.normalizar_utilizador(utilizador)

    sal = ident.gerar_sal()
    identificador = ident.identificador_de(utilizador, sal)
    impressao = ident.impressao_de(identificador)
    identidade = Identidade.gerar()

    conta = Conta(
        utilizador=utilizador,
        identificador=identificador,
        impressao=impressao,
        salt_hex=sal.hex(),
        publica_hex=identidade.pub.hex(),
        criada_em=datetime.now(UTC).isoformat(timespec="seconds"),
    )

    destino.parent.mkdir(parents=True, exist_ok=True)
    storage.guardar_keystore(conta.para_dict(), destino, frase)
    return conta


# ---------------------------------------------------------------------
# Entrada
# ---------------------------------------------------------------------


def entrar(frase: str, caminho: Path | str | None = None) -> Conta:
    """Abre a conta com a frase de segurança.

    A frase não é comparada com nada: é a chave com que o keystore é
    decifrado, e é a etiqueta AEAD que confirma se a chave é a certa.
    Uma frase errada levanta :class:`~messenger.storage.PassphraseErrada`,
    que é exactamente a mesma excepção que um ficheiro adulterado levanta
    — **e essa é a propriedade importante**: quem recebe a excepção não
    consegue distinguir as duas coisas, e portanto não pode usá-la como
    oráculo para testar palavras-passe uma a uma e observar a diferença
    no tempo ou no tipo de erro.

    A função não distingue «não há conta» de «a conta está corrompida».
    Em ambos os casos o ficheiro não abriu, e dizer qual dos dois
    aconteceu à interface é dar mais informação do que é preciso.

    :raises KeystoreInvalido: se o ficheiro não existe ou tem formato
        desconhecido.
    :raises PassphraseErrada: se a frase não é a chave.
    """
    destino = Path(caminho) if caminho is not None else caminho_conta()
    # `PassphraseErrada` é subclasse de `KeystoreInvalido`, e o erro de
    # «não existe» também é `KeystoreInvalido`. Deixar o `storage` falar é
    # deliberado: reescrever a mensagem obrigaria a decidir qual dos dois
    # esconder, e não há nada a ganhar com essa distinção aqui.
    dados = storage.carregar_keystore(destino, frase)
    return _de_dados(dados)


def gravar(
    conta: Conta,
    frase: str,
    caminho: Path | str | None = None,
) -> None:
    """Re-cifra ``conta`` em ``caminho`` (0600), com a ``frase``.

    ## Porque é que isto é separado de :meth:`Conta.com_amizade`

    ``com_amizade`` devolve uma :class:`Conta` **em memória**. Escrever no
    disco é uma segunda operação, com uma segunda fonte de erro — a
    frase pode estar errada, o disco pode estar cheio — e as duas juntas
    num método só dariam um ``Conta`` que parece gravado e pode não estar.

    Quem chama tem de dizer as duas coisas, e a sequência fica visível:

    ```python
    conta = conta.com_amizade(amizade)
    gravar(conta, frase)          # se isto falhar, a amizade não existe
    ```

    ## Porque é que gravar exige a frase

    Porque gravar é **cifrar**, e a cifra é a frase. Não há atalho, e
    derivar uma chave de sessão guardada seria guardar o segredo em
    dois sítios — que é a forma de um deles ficar desatualizado.

    ## Porque é que isto não é «atomicamente seguro»

    ``storage.guardar_keystore`` grava num temporário e faz ``rename``,
    que é atómico dentro do mesmo sistema de ficheiros. O ficheiro
    anterior fica intacto se a escrita falhar a meio. O que **não** é
    garantido é que o utilizador se lembre de fazer isto: sem a
    chamada a :func:`gravar`, a amizade vive só na memória.
    """
    destino = Path(caminho) if caminho is not None else caminho_conta()
    storage.guardar_keystore(conta.para_dict(), destino, frase)


def _de_dados(dados: dict[str, Any]) -> Conta:
    """Constrói a :class:`Conta` a partir do dicionário lido do ficheiro.

    Separado de :func:`entrar` para que o mesmo validador sirva o que
    vem de um ficheiro e o que vem de uma cópia de segurança restaurada.
    """
    if dados.get("tipo") != "conta":
        raise ContaInvalida(
            "o ficheiro não é um registo de conta; é cifrado com a mesma "
            "mecânica mas pertence a outra coisa"
        )

    faltam = [
        campo
        for campo in ("utilizador", "identificador", "impressao", "salt_hex", "publica_hex", "criada_em")
        if campo not in dados
    ]
    if faltam:
        raise ContaInvalida(f"registo de conta incompleto, faltam: {', '.join(faltam)}")

    versao = dados.get("versao", VERSAO_CONTA)
    if versao > VERSAO_CONTA:
        raise ContaInvalida(
            f"o registo é da versão {versao} e este código conhece até à {VERSAO_CONTA}. "
            "Actualize o OnyxChat."
        )

    # As amizades são **tolerantes**: um keystore escrito antes desta
    # mudança não tem a chave, e tem de abrir. A ausência é uma lista
    # vazia — que é o estado correcto de quem ainda não fez um handshake.
    #
    # Já uma amizade presente e malformada é um erro: um ficheiro com uma
    # amizade a meio não é um registo antigo, é um registo corrompido,
    # e aceitá-lo deixaria um `K9` de comprimento errado a cifrar.
    #
    # A lista também não é do tipo certo no ficheiro: um `"amizades": 3`
    # é lixo, e `tuple(3)` levantaria um `TypeError` que não é
    # `ContaInvalida` e sairia pelo caminho não tipado.
    bruto_amizades = dados.get("amizades", [])
    if not isinstance(bruto_amizades, list):
        raise ContaInvalida("«amizades» não é uma lista")
    amizades = tuple(Amizade.de_dict(a) for a in bruto_amizades)

    conta = Conta(
        utilizador=str(dados["utilizador"]),
        identificador=str(dados["identificador"]),
        impressao=str(dados["impressao"]),
        salt_hex=str(dados["salt_hex"]),
        publica_hex=str(dados["publica_hex"]),
        criada_em=str(dados["criada_em"]),
        versao=int(versao),
        amizades=amizades,
    )

    # O identificador e a impressão são re-derivados e comparados. Um
    # ficheiro cujas derivadas não batem é um ficheiro adulterado ou
    # escrito por outra versão da fórmula — e nos dois casos a conta não
    # deve abrir, porque o que a pessoa confirmaria a si própria deixaria
    # de ser verdade.
    sal = bytes.fromhex(conta.salt_hex)
    esperado = ident.identificador_de(conta.utilizador, sal)
    if esperado != conta.identificador or ident.impressao_de(esperado) != conta.impressao:
        raise ContaInvalida(
            "o identificador do registo não corresponde ao que a fórmula deriva "
            "para este nome e este sal. O ficheiro foi alterado depois de escrito."
        )

    return conta


# ---------------------------------------------------------------------
# Eliminar e cópias de segurança
# ---------------------------------------------------------------------


def eliminar(caminho: Path | str | None = None) -> bool:
    """Apaga o ficheiro de conta.

    Devolve ``True`` se apagou e ``False`` se não havia nada.

    O ``False`` não é um erro: apagar uma conta que não existe põe o
    sistema no mesmo estado que apagar uma que existe. E apagar é a
    operação que responde a «comecei de novo», que é o que alguém faz
    quando a frase se perdeu e aceita ser outra pessoa.

    Por isso **não** pede confirmação: a confirmação é da interface, que
    é onde vive o texto que diz o que se perde. Uma função que apaga a
    identidade e devolve um booleano sem perguntar é o lugar certo para
    que a pergunta seja explícita.

    :returns: se havia ficheiro.
    """
    destino = Path(caminho) if caminho is not None else caminho_conta()
    if not destino.is_file():
        return False
    destino.unlink()
    return True


def exportar(
    destino: Path | str,
    frase: str,
    caminho: Path | str | None = None,
) -> Path:
    """Escreve uma cópia do ficheiro cifrado.

    Copia o ficheiro **já cifrado**, sem o abrir. É a consequência de a
    cifra ser do ficheiro inteiro: uma cópia de segurança é um ficheiro
    opaco que não pode ser lido sem a frase, e por isso pode ficar num
    sítio inseguro sem vazar nada. Re-derivar a conta e re-cifrá-la com
    outra frase seria o mesmo resultado com mais uma chave a viver num
    sítio onde não deve.

    A cópia abre com a mesma frase e no mesmo caminho de leitura: voltar
    a pô-la no sítio de origem é o mesmo gesto que :func:`restaurar` faz.

    :raises FileNotFoundError: se não houver conta para copiar.
    """
    origem = Path(caminho) if caminho is not None else caminho_conta()
    alvo = Path(destino)
    if not origem.is_file():
        raise FileNotFoundError(f"não há conta em {origem} para exportar")

    alvo.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(origem, alvo)
    # A cópia é um segredo com o mesmo peso do original. O `copyfile` não
    # copia o modo, e um ficheiro com o umask por omissão fica legível
    # por outros utilizadores da máquina.
    os.chmod(alvo, 0o600)
    return alvo


def restaurar(origem: Path | str, frase: str, caminho: Path | str | None = None) -> Conta:
    """Repõe uma conta a partir de uma cópia de segurança.

    Verifica a frase **antes** de tocar no destino: uma cópia com outra
    frase é o caso comum de quem guarda a cópia noutro sítio e a abre com
    a frase actual por hábito. Verificar primeiro evita substituir uma
    conta boa por uma cópia que não abre.

    :param origem: a cópia de segurança.
    :param frase: a frase com que a cópia foi cifrada.
    :param caminho: destino; por omissão, :func:`caminho_conta`.
    :raises KeystoreInvalido: se a cópia não abrir.
    :raises PassphraseErrada: se a frase não for a da cópia.
    """
    origem_p = Path(origem)
    destino = Path(caminho) if caminho is not None else caminho_conta()

    # Abrir a cópia valida a frase e valida o registo, com o mesmo
    # código do login. Se isto levantar, o destino fica como estava.
    conta = _de_dados(storage.carregar_keystore(origem_p, frase))

    destino.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(origem_p, destino)
    os.chmod(destino, 0o600)
    return conta


# ---------------------------------------------------------------------
# O que a interface precisa de saber
# ---------------------------------------------------------------------


def nome_de_exibicao(conta: Conta) -> str:
    """O nome como se mostra, normalizado para não ter espaços sobrando.

    Guardar o nome **tal como foi escrito** e mostrá-lo normalizado são
    duas decisões separadas. Guardar o normalizado faz o registo perder a
    forma como a pessoa quis ser chamada — e ninguém escreve o seu nome
    com acentos a mais por causa de um ficheiro. Mostrar o cru, com
    espaços nas pontas, faz a interface exibir um nome que não existe.
    """
    return unicodedata.normalize("NFKC", conta.utilizador).strip()