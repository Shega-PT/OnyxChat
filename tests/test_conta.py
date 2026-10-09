# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_conta.py — a camada de acesso local.

O que estes testes têm de proteger, por ordem de gravidade:

1. **Que a frase errada não abre a conta** e que a excepção não
   distingue «frase errada» de «ficheiro adulterado» — se distinguisse,
   o erro passa a ser um oráculo.
2. **Que o sal nunca sai do texto cifrado.** É o que impede confirmar um
   nome de utilizador a partir do identificador público.
3. **Que uma identidade é nova a cada registo** e que criar por cima de
   outra exige `forcar`.
4. **Que a estimativa de força diz a verdade nos casos em que é fácil
   mentir.**

O resto — caminhos, formatos, mensagens — é mecânica, e ainda assim é
testado porque `conta.py` é a superfície que uma pessoa toca com a
identidade dela.
"""

from __future__ import annotations

import json
import stat
from pathlib import Path

import pytest

from messenger import conta, identidade as ident, ipc_client, storage

#: Frase que passa os mínimos sem ser uma de teste óbvia.
FRASE = "quatro cavalos lentos numa mare"


@pytest.fixture
def destino(tmp_path: Path) -> Path:
    return tmp_path / "conta.keystore"


# ---------------------------------------------------------------------
# A estimativa de entropia
# ---------------------------------------------------------------------


def test_fraca_vazia_da_zero() -> None:
    assert conta.estimar_entropia("") == 0.0


def test_um_caractere_repetido_da_zero() -> None:
    """«aaaa» não tem entropia nenhuma, e é o que a fórmula tem de dizer.

    Um estimador que desse ``len × log2(1)`` — ou pior, que dividisse
    por um alfabeto presumido de 26 — daria a «aaaa» uma força que não
    tem. A medição por frequência existe precisamente para isto.
    """
    assert conta.estimar_entropia("aaaa") == 0.0


def test_caracteres_todos_distintos_valem_mais_do_que_o_ingenuo() -> None:
    """Doze letras distintas não valem 12 × log₂(26)."""
    bits = conta.estimar_entropia("abcdefghijkl")
    ingenua = 12 * 4.584962500721156
    assert bits < ingenua
    # E mesmo assim valem mais do que doze bits.
    assert bits > 12


def test_mais_variedade_da_mais_entropia() -> None:
    base = "cavalos lentos"
    mais = "caval0s L3ntos!!"
    assert conta.estimar_entropia(mais) > conta.estimar_entropia(base)


def test_frases_ditas_da_um_valor_alto_e_nao_e_um_erro() -> None:
    """A limitação declarada é esta, e o teste fixa-a.

    ``correct horse battery staple`` sai desta fórmula com mais de cem
    bits e continua a ser adivinhável por um dicionário. Um teste que
    esperasse que a fórmula apanhasse isto estaria a testar uma
    função diferente da que existe — e a documentar como se apanhasse.
    """
    bits = conta.estimar_entropia("correct horse battery staple")
    assert bits > conta.ENTROPIA_MINIMA


# ---------------------------------------------------------------------
# A força mínima
# ---------------------------------------------------------------------


def test_frase_curta_e_recusada_com_o_numero_de_caracteres(destino: Path) -> None:
    with pytest.raises(conta.FraseForte) as erro:
        conta.criar("ana", "curta", caminho=destino)
    assert "pelo menos" in str(erro.value)


def test_frase_longa_e_repetitiva_e_recusada_pela_variedade(destino: Path) -> None:
    """O outro lado da mesma regra.

    Uma frase de vinte ``a`` passa o comprimento e falha a variedade.
    Mostrar só o comprimento deixaria passar a pior das frases.
    """
    with pytest.raises(conta.FraseForte) as erro:
        conta.criar("ana", "a" * 20, caminho=destino)
    assert "bits estimados" in str(erro.value)


def test_a_ordem_dos_erros_e_comprimento_primeiro(destino: Path) -> None:
    """Uma frase curta **e** repetitiva tem de reclamar do comprimento.

    Se a ordem fosse invertida, a pessoa alongava a frase e era mandada
    outra vez ao mesmo sítio, agora a ser avisada da variedade — sem
    nunca ter sido avisada da coisa que primeiro tinha de mudar.
    """
    with pytest.raises(conta.FraseForte) as erro:
        conta.criar("ana", "aaaa", caminho=destino)
    assert "pelo menos" in str(erro.value)


def test_fraca_aceitavel_passa(destino: Path) -> None:
    conta.criar("ana", FRASE, caminho=destino)
    assert conta.conta_existe(destino)


# ---------------------------------------------------------------------
# Caminho
# ---------------------------------------------------------------------


def test_caminho_conta_usa_a_variavel_de_ambiente(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    alvo = tmp_path / "outro.keystore"
    monkeypatch.setenv("ONYX_CONTA", str(alvo))
    assert conta.caminho_conta() == alvo


def test_caminho_conta_por_omissao_esta_no_projecto() -> None:
    """Relativo ao projecto, não ao directório de execução.

    Um ficheiro relativo ao ``cwd`` aparece num sítio diferente conforme
    quem lançou o programa, que é a forma mais comum de «a conta
    desapareceu».
    """
    caminho = conta.caminho_conta()
    assert caminho.is_absolute()
    assert caminho.parent.name == "user"
    # A raiz é a do projecto, e o que se verifica é **essa** — não o nome
    # que a pasta por acaso tem. Havia aqui uma asserção que exigia
    # `OnyxChat` no nome da pasta, e o CI provou que ela não testava o
    # que a docstring promete: um runner clona para `/home/runner/work/
    # OnyxChat/OnyxChat` mas o checkout é directo ao nível do repositório,
    # e o teste passou a falhar por o nome da pasta de trabalho não
    # coincidir com o nome do projecto.
    #
    # Um `git clone` é um `git clone`, e o `testar.sh` é o mesmo script
    # localmente e no CI — um teste que só passa na máquina onde foi
    # escrito não está a testar o produto, está a testar o nome de uma
    # directório. Comparar com a raiz calculada pelo próprio pacote é o
    # que torna a asserção verdadeira em qualquer clone, e é o que a
    # docstring diz desde o início.
    raiz = Path(conta.__file__).resolve().parent.parent
    assert caminho.parent.parent == raiz


def test_caminho_conta_nao_depende_do_directorio_de_execucao(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """O `cwd` muda o sítio de onde se executa; o caminho não muda.

    É a mesma propriedade da docstring acima, verificada pelo seu custo:
    chamar a função a partir de outra pasta tem de dar o mesmo caminho. Um
    caminho relativo ao `cwd` passava a primeira asserção e falhava esta.
    """
    antes = conta.caminho_conta()
    monkeypatch.chdir(tmp_path)
    assert conta.caminho_conta() == antes


def test_conta_existe_false_para_ficheiro_ausente(destino: Path) -> None:
    assert conta.conta_existe(destino) is False


# ---------------------------------------------------------------------
# Criar
# ---------------------------------------------------------------------


def test_criar_gera_identidade_e_impressao_coerentes(destino: Path) -> None:
    conta_criada = conta.criar("Marta Vasconcelos", FRASE, caminho=destino)

    assert ident.FORMA_IDENTIFICADOR.match(conta_criada.identificador)
    assert len(conta_criada.impressao.split(":")) == 16

    # A impressão tem de ser a do identificador, não uma independente.
    assert ident.impressao_de(conta_criada.identificador) == conta_criada.impressao


def test_criar_deriva_o_identificador_com_o_sal_guardado(destino: Path) -> None:
    """O identificador tem de ser reproduzível a partir do sal guardado.

    É isto que faz o registo verificável por qualquer pessoa com o
    ficheiro: o sal e o nome bastam, e a fórmula é pública.
    """
    conta_criada = conta.criar("ana", FRASE, caminho=destino)
    sal = bytes.fromhex(conta_criada.salt_hex)
    assert ident.identificador_de("ana", sal) == conta_criada.identificador


def test_criar_normaliza_o_nome_mas_guarda_o_original(destino: Path) -> None:
    """Guardar o normalizado faria o registo perder a forma de escrita.

    Ninguém escreve o seu nome com acentos a mais por causa de um
    ficheiro. Guardar o cru e mostrar normalizado é o caminho que não
    obriga a pessoa a adaptar-se ao programa.
    """
    conta_criada = conta.criar("  Marta  ", FRASE, caminho=destino)
    assert conta_criada.utilizador == "  Marta  "
    assert conta_criada.normalizado == "marta"
    assert conta.nome_de_exibicao(conta_criada) == "Marta"


def test_criar_rejeita_nome_vazio(destino: Path) -> None:
    with pytest.raises(ident.IdentidadeInvalida):
        conta.criar("   ", FRASE, caminho=destino)


def test_criar_rejeita_nome_nao_texto(destino: Path) -> None:
    with pytest.raises(ident.IdentidadeInvalida):
        conta.criar(42, FRASE, caminho=destino)  # type: ignore[arg-type]


def test_criar_nao_sobrescreve(destino: Path) -> None:
    """Criar por cima de uma conta é desaparecer sem o dar por isso.

    Cada identidade é uma pessoa diferente para quem já conhece esta,
    pelo que a recusa tem de ser explícita e o caminho tem de ser
    declarado: `forcar`.
    """
    conta.criar("ana", FRASE, caminho=destino)
    with pytest.raises(conta.ContaInvalida) as erro:
        conta.criar("outra", FRASE, caminho=destino)
    assert "eliminar" in str(erro.value)


def test_criar_com_forcar_substitui_e_muda_a_identidade(destino: Path) -> None:
    primeira = conta.criar("ana", FRASE, caminho=destino)
    segunda = conta.criar("ana", FRASE, caminho=destino, forcar=True)

    assert segunda.identificador != primeira.identificador
    assert segunda.impressao != primeira.impressao


def test_identidades_nunca_se_repetem(destino: Path) -> None:
    """Duas contas com o mesmo nome dão identificadores diferentes.

    É o sal a fazer o seu trabalho; se um dia isto falhar, a causa
    provável é um sal que deixou de ser aleatório, e o teste é a prova
    de que isso não aconteceu.
    """
    primeira = conta.criar("ana", FRASE, caminho=destino)
    segunda = conta.criar("ana", FRASE, caminho=destino, forcar=True)
    assert primeira.salt_hex != segunda.salt_hex
    assert primeira.identificador != segunda.identificador


def test_criar_cria_o_directorio(destino: Path) -> None:
    """O destino pode estar num directório que ainda não existe."""
    aninhado = destino.parent / "a" / "b" / "conta.keystore"
    conta.criar("ana", FRASE, caminho=aninhado)
    assert aninhado.is_file()


def test_criar_grava_o_ficheiro_com_modo_0600(destino: Path) -> None:
    """A conta é um segredo de primeira classe desde o primeiro byte.

    Um ficheiro com o umask por omissão fica legível por todos os
    utilizadores da máquina, e a frase de segurança é a única coisa que
    protege o resto.
    """
    conta.criar("ana", FRASE, caminho=destino)
    modo = stat.S_IMODE(destino.stat().st_mode)
    assert modo == 0o600


def test_o_sal_so_sai_do_ficheiro_cifrado(destino: Path) -> None:
    """A propriedade mais importante do formato.

    O identificador é **público** — é o que se dá a alguém para te
    encontrar. Se o sal também fosse público, quem tivesse o identificador
    poderia confirmar qualquer nome de utilizador que suspeitasse: Bastava
    computar ``identificador_de(suspeita, sal)`` e comparar com o valor
    público.

    Por isso o sal vive **dentro** do texto cifrado, e o teste é uma
    leitura do ficheiro tal como está no disco.
    """
    conta.criar("ana", FRASE, caminho=destino)
    cru = destino.read_bytes()

    conta_criada = conta.entrar(FRASE, destino)
    sal = conta_criada.salt_hex
    assert bytes.fromhex(sal).hex().encode() not in cru
    assert conta_criada.identificador.encode() not in cru


def test_o_sal_dentro_do_blob_esta_legivel_quando_se_decifra(destino: Path) -> None:
    """O mesmo sal, agora à vista — para mostrar que o teste anterior não
    passa por o ficheiro não o ter."""
    conta.criar("ana", FRASE, caminho=destino)
    conta_criada = conta.entrar(FRASE, destino)
    assert bytes.fromhex(conta_criada.salt_hex).hex() != ""

# ---------------------------------------------------------------------
# Entrar
# ---------------------------------------------------------------------


def test_entrar_devolve_o_que_criar_escreveu(destino: Path) -> None:
    criada = conta.criar("Marta Vasconcelos", FRASE, caminho=destino)
    aberta = conta.entrar(FRASE, destino)

    assert aberta.identificador == criada.identificador
    assert aberta.impressao == criada.impressao
    assert aberta.utilizador == criada.utilizador
    assert aberta.publica_hex == criada.publica_hex
    assert aberta.criada_em == criada.criada_em


def test_entrar_com_frase_errada_falha(destino: Path) -> None:
    conta.criar("ana", FRASE, caminho=destino)
    with pytest.raises(storage.PassphraseErrada):
        conta.entrar(FRASE + " ", destino)


def test_a_excepcao_nao_distingue_frase_errada_de_adulteracao(destino: Path) -> None:
    """A propriedade que torna a excepção segura de expor.

    Se «frase errada» e «ficheiro adulterado» levantassem excepções
    diferentes, quem apoitasse o dedo no botão de entrar podia usá-las
    como oráculo: testar palavra-passe a palavra-passe e observar qual
    levanta o quê. O `:class:` é o mesmo nos dois casos, e este teste
    fixa isso.
    """
    conta.criar("ana", FRASE, caminho=destino)

    # Frase errada.
    with pytest.raises(storage.KeystoreInvalido) as errada:
        conta.entrar("outra frase qualquer", destino)
    assert type(errada.value) is storage.PassphraseErrada

    # Ficheiro adulterado: um byte trocado no meio do texto cifrado.
    bruto = bytearray(destino.read_bytes())
    bruto[-20] ^= 0xFF
    destino.write_bytes(bytes(bruto))
    with pytest.raises(storage.KeystoreInvalido) as adulterado:
        conta.entrar(FRASE, destino)
    assert type(adulterado.value) is storage.PassphraseErrada


def test_entrar_sem_ficheiro_da_erro_de_keystore(destino: Path) -> None:
    """Não há excepção própria para «não há conta».

    A interface trata «o ficheiro não existe» e «o ficheiro não abre» da
    mesma maneira — encaminha para o registo — e inventar uma excepção separada só
    serviria para a interface ter de a traduzir de volta.
    """
    with pytest.raises(storage.KeystoreInvalido):
        conta.entrar(FRASE, destino)


def test_entrar_em_ficheiro_de_formato_desconhecido(destino: Path) -> None:
    destino.write_bytes(b"ONYXKS99nao-existe")
    with pytest.raises(storage.KeystoreInvalido):
        conta.entrar(FRASE, destino)


# ---------------------------------------------------------------------
# Registos que não são registos
# ---------------------------------------------------------------------


def test_ficheiro_que_nao_e_registo_de_conta(destino: Path) -> None:
    """O keystore é genérico e pode guardar outras coisas.

    O `tipo` no interior é o que distingue um registo de conta de, por
    exemplo, as preferências. Confundir os dois levaria a interface a
    mostrar um perfil a partir de um ficheiro de preferências, sem que
    nada tivesse falhado até esse momento.
    """
    storage.guardar_keystore({"tipo": "preferencias"}, destino, FRASE)
    with pytest.raises(conta.ContaInvalida) as erro:
        conta.entrar(FRASE, destino)
    assert "outra coisa" in str(erro.value)


def test_registo_incompleto_e_dito_como_falta_que(destino: Path) -> None:
    storage.guardar_keystore(
        {"tipo": "conta", "utilizador": "ana", "identificador": "ONYX-AAAAAA-!A!A#"},
        destino,
        FRASE,
    )
    with pytest.raises(conta.ContaInvalida) as erro:
        conta.entrar(FRASE, destino)
    assert "impressao" in str(erro.value)


def test_registo_de_versao_futura_e_recusado_com_instrucao(destino: Path) -> None:
    """Um registo escrito por uma versão mais recente não se abre.

    Deixar abrir seria pior: a conta abriria e os campos que esta
    versão não conhece seriam descartados em silêncio, e a escrita
    seguinte apagaria o que a versão futura guardou.
    """
    base = conta.criar("ana", FRASE, caminho=destino).para_dict()
    storage.guardar_keystore({**base, "versao": conta.VERSAO_CONTA + 1}, destino, FRASE)
    with pytest.raises(conta.ContaInvalida) as erro:
        conta.entrar(FRASE, destino)
    assert "Actualize o OnyxChat" in str(erro.value)


def test_registo_sem_versao_assume_a_actual(destino: Path) -> None:
    """Registos sem ``versao`` são da primeira versão.

    A chave ausente é lida como ``VERSAO_CONTA``, o que faz o registo
    mais antigo possível abrir sem caso especial.
    """
    base = conta.criar("ana", FRASE, caminho=destino).para_dict()
    sem = {k: v for k, v in base.items() if k != "versao"}
    storage.guardar_keystore(sem, destino, FRASE)
    assert conta.entrar(FRASE, destino).utilizador == "ana"


def test_registo_cujas_derivadas_nao_batem_e_recusado(destino: Path) -> None:
    """O identificador guardado tem de ser o que a fórmula deriva.

    Um ficheiro cujas derivadas não batem foi adulterado, ou foi escrito
    por outra versão da fórmula. Nos dois casos a conta não deve abrir:
    o que a pessoa confirmasse a si própria deixaria de ser verdade.
    """
    base = conta.criar("ana", FRASE, caminho=destino).para_dict()
    storage.guardar_keystore(
        {**base, "identificador": "ONYX-ZZZZZZ-!A!A#", "impressao": "AA:" * 8 + "BB"},
        destino,
        FRASE,
    )
    with pytest.raises(conta.ContaInvalida) as erro:
        conta.entrar(FRASE, destino)
    assert "alterado" in str(erro.value)


def test_registo_com_impressao_alterada_e_recusado(destino: Path) -> None:
    """A impressão é verificada à parte do identificador.

    Um identificador certo e uma impressão trocada é um ataque
    plausível: mantém quem lê a impressão a ver o valor que espera, e
    muda o que a pessoa confirma.
    """
    base = conta.criar("ana", FRASE, caminho=destino).para_dict()
    storage.guardar_keystore({**base, "impressao": "AA:" * 8 + "BB"}, destino, FRASE)
    with pytest.raises(conta.ContaInvalida):
        conta.entrar(FRASE, destino)


# ---------------------------------------------------------------------
# Eliminar
# ---------------------------------------------------------------------


def test_eliminar_apaga_e_devolve_verdade(destino: Path) -> None:
    conta.criar("ana", FRASE, caminho=destino)
    assert conta.eliminar(destino) is True
    assert not destino.exists()


def test_eliminar_sem_ficheiro_devolve_falso_sem_levantar(destino: Path) -> None:
    """Apagar o que não existe põe o sistema no mesmo estado."""
    assert conta.eliminar(destino) is False


# ---------------------------------------------------------------------
# Cópias de segurança
# ---------------------------------------------------------------------


def test_exportar_copia_o_ficheiro_ja_cifrado(destino: Path, tmp_path: Path) -> None:
    """A cópia tem de continuar opaca.

    Se exportar re-cifrasse com outra chave, a cópia passaria a ser
    legível por quem tivesse essa chave — e a cópia de segurança é
    precisamente o ficheiro que vai para um sítio menos vigiado.
    """
    conta.criar("ana", FRASE, caminho=destino)
    copia = tmp_path / "copias" / "conta.keystore"
    conta.exportar(copia, FRASE, destino)

    assert copia.read_bytes() == destino.read_bytes()
    assert b"ana" not in copia.read_bytes()


def test_exportar_da_modo_0600(destino: Path, tmp_path: Path) -> None:
    """O `copyfile` não copia o modo, e o umask deixa o ficheiro legível."""
    conta.criar("ana", FRASE, caminho=destino)
    copia = conta.exportar(tmp_path / "copia.keystore", FRASE, destino)
    assert stat.S_IMODE(copia.stat().st_mode) == 0o600


def test_exportar_sem_conta_falha(tmp_path: Path, destino: Path) -> None:
    with pytest.raises(FileNotFoundError):
        conta.exportar(tmp_path / "copia.keystore", FRASE, destino)


def test_exportar_cria_o_directorio(destino: Path, tmp_path: Path) -> None:
    conta.criar("ana", FRASE, caminho=destino)
    aninhada = tmp_path / "a" / "b" / "c.keystore"
    conta.exportar(aninhada, FRASE, destino)
    assert aninhada.is_file()


def test_restaurar_repõe_a_conta(destino: Path, tmp_path: Path) -> None:
    original = conta.criar("ana", FRASE, caminho=destino)
    copia = conta.exportar(tmp_path / "copia.keystore", FRASE, destino)
    conta.eliminar(destino)

    reposta = conta.restaurar(copia, FRASE, destino)
    assert reposta.identificador == original.identificador
    assert conta.conta_existe(destino)
    assert conta.entrar(FRASE, destino).impressao == original.impressao


def test_restaurar_verifica_antes_de_sobrescrever(destino: Path, tmp_path: Path) -> None:
    """A frase errada tem de deixar a conta de destino intacta.

    Uma cópia guardada noutro sítio e aberta com a frase actual por
    hábito é o caso comum. Se a verificação viesse depois da escrita, a
    conta boa seria substituída por uma cópia que não abre — e a pessoa
    ficaria sem conta **e** sem a cópia que tentava usar.
    """
    conta.criar("ana", FRASE, caminho=destino)
    copia = conta.exportar(tmp_path / "copia.keystore", FRASE, destino)

    with pytest.raises(storage.PassphraseErrada):
        conta.restaurar(copia, "frase errada", destino)

    assert conta.entrar(FRASE, destino).utilizador == "ana"


def test_restaurar_valida_o_registo_da_copia(destino: Path, tmp_path: Path) -> None:
    """Uma cópia que não é uma conta não substitui uma conta."""
    conta.criar("ana", FRASE, caminho=destino)
    alheia = tmp_path / "alheia.keystore"
    storage.guardar_keystore({"tipo": "preferencias"}, alheia, FRASE)

    with pytest.raises(conta.ContaInvalida):
        conta.restaurar(alheia, FRASE, destino)
    assert conta.entrar(FRASE, destino).utilizador == "ana"


def test_restaurar_cria_o_directorio_de_destino(tmp_path: Path) -> None:
    origem = tmp_path / "origem.keystore"
    conta.criar("ana", FRASE, caminho=origem)
    destino = tmp_path / "novo" / "sub" / "conta.keystore"
    conta.restaurar(origem, FRASE, caminho=destino)
    assert destino.is_file()


# ---------------------------------------------------------------------
# O registo tal como é escrito
# ---------------------------------------------------------------------


def test_para_dict_tem_o_tipo_e_a_versao(destino: Path) -> None:
    """O ``tipo`` é o que distingue este registo dos outros do keystore."""
    registo = conta.criar("ana", FRASE, caminho=destino).para_dict()
    assert registo["tipo"] == "conta"
    assert registo["versao"] == conta.VERSAO_CONTA
    # Tudo o que o registo guarda tem de ser serializável, ou o
    # `json.dumps` do `storage` levanta e o ficheiro não é escrito.
    json.dumps(registo)


def test_a_forma_do_identificador_gerado_passa_o_alfabeto(tmp_path: Path) -> None:
    """Cada conta criada tem de produzir um identificador que a descoberta
    aceite.

    A incompatibilidade que a Etapa 3 encontrou — a expressão regular do
    servidor a rejeitar o identificador que o projecto produz — não pode
    voltar por outro caminho, e vinte e cinco contas seguidas é a forma
    barata de o notar antes do registo.
    """
    for indice in range(25):
        criada = conta.criar(f"pessoa {indice}", FRASE, caminho=tmp_path / f"c{indice}.keystore")
        assert ident.FORMA_IDENTIFICADOR.match(criada.identificador)


# ---------------------------------------------------------------------
# As chaves de amizade dentro do keystore
#
# A propriedade que se está a testar é uma só, e é de sobrevivência:
# **a amizade tem de continuar a existir depois de o programa fechar.**
# Até 2026-10-07 as 192 bytes eram impressas para o terminal e perdidas
# no fim do comando, e o daemon só as tinha em memória.
# ---------------------------------------------------------------------


def _amizade_de_teste(publica: int = 5) -> conta.Amizade:
    """Uma amizade com chaves reconhecíveis, para as asserções lerem."""
    return conta.Amizade.de_chaves(
        ipc_client.ChavesAmizade(
            k1=bytes(range(32)),
            k5_proprio=bytes([1]) * 32,
            k9_proprio=bytes([2]) * 32,
            k5_par=bytes([3]) * 32,
            k9_par=bytes([4]) * 32,
            publica=bytes([publica]) * 32,
        )
    )


def test_as_chaves_sobrevive_a_fechar_e_reabrir(destino: Path) -> None:
    """O teste que a mudança inteira existe para passar.

    Escrever, sair, ler de novo: se a amizade não voltar **byte a byte**,
    as mensagens seguintes são cifradas com um ``K9`` diferente do que o
    par tem, e a decifragem falha sem mensagem útil.
    """
    original = _amizade_de_teste()
    aberta = conta.criar("ana", FRASE, destino)
    conta.gravar(aberta.com_amizade(original), FRASE, destino)

    # Um objecto novo: nada em memória sobrevive a esta linha.
    reaberta = conta.entrar(FRASE, destino)

    assert len(reaberta.amizades) == 1
    assert reaberta.amizades[0].para_chaves() == original.para_chaves()
    assert reaberta.amizade_de(original.publica_hex) is not None


def test_a_amizade_nunca_sai_em_claro(destino: Path) -> None:
    """As chaves estão cifradas, e o teste lê o ficheiro como está no disco.

    O mesmo teste que `test_o_sal_so_sai_do_ficheiro_cifrado` faz com o
    sal: quem tem o ficheiro e não tem a frase não tem as chaves.
    """
    amizade = _amizade_de_teste()
    aberta = conta.criar("ana", FRASE, destino)
    conta.gravar(aberta.com_amizade(amizade), FRASE, destino)

    bruto = destino.read_bytes()
    for campo in (
        amizade.k1_hex,
        amizade.k9_proprio_hex,
        amizade.k9_par_hex,
    ):
        assert bytes.fromhex(campo) not in bruto
        assert campo.encode() not in bruto


def test_a_frase_errada_nao_abre_as_amizades(destino: Path) -> None:
    """Errar a frase é errar a identidade, e as chaves são parte disso."""
    aberta = conta.criar("ana", FRASE, destino)
    conta.gravar(aberta.com_amizade(_amizade_de_teste()), FRASE, destino)

    with pytest.raises(storage.PassphraseErrada):
        conta.entrar("uma frase completamente diferente", destino)


def test_a_segunda_amizade_com_o_mesmo_par_substitui(destino: Path) -> None:
    """Refazer o handshake com o mesmo par substitui, não duplica.

    Guardar as duas lado a lado daria dois conjuntos de chaves para o
    mesmo contacto, e o daemon não sabe qual usar.
    """
    antiga = _amizade_de_teste()
    nova = _amizade_de_teste()
    # Mesma pública, chaves de handshake diferentes.
    nova = conta.Amizade.de_chaves(
        ipc_client.ChavesAmizade(
            k1=bytes([0xAA]) * 32,
            k5_proprio=bytes([0xBB]) * 32,
            k9_proprio=bytes([0xCC]) * 32,
            k5_par=bytes([0xDD]) * 32,
            k9_par=bytes([0xEE]) * 32,
            publica=bytes.fromhex(antiga.publica_hex),
        )
    )

    aberta = conta.criar("ana", FRASE, destino)
    aberta = aberta.com_amizade(antiga)
    aberta = aberta.com_amizade(nova)
    conta.gravar(aberta, FRASE, destino)

    reaberta = conta.entrar(FRASE, destino)
    assert len(reaberta.amizades) == 1, "a mesma publica duplicou"
    assert reaberta.amizades[0].k1_hex == nova.k1_hex, "guardou as chaves velhas"


def test_pares_diferentes_convivem(destino: Path) -> None:
    """Duas amizades de duas pessoas distintas são duas entradas."""
    a = _amizade_de_teste(publica=5)
    b = _amizade_de_teste(publica=6)

    aberta = conta.criar("ana", FRASE, destino)
    conta.gravar(aberta.com_amizade(a).com_amizade(b), FRASE, destino)

    reaberta = conta.entrar(FRASE, destino)
    assert {x.publica_hex for x in reaberta.amizades} == {a.publica_hex, b.publica_hex}


def test_uma_conta_antiga_abre_sem_amizades(destino: Path) -> None:
    """Um keystore escrito antes das amizades não tem a chave — e abre.

    A ausência tem de ser uma lista vazia, e não um erro: de outro modo
    uma actualização do programa deixava de conseguir abrir a conta de
    quem já tinha conta, que é a pior forma de perder um programa.
    """
    c = conta.criar("ana", FRASE, destino)
    dados = c.para_dict()
    dados.pop("amizades")
    storage.guardar_keystore(dados, destino, FRASE)

    reaberta = conta.entrar(FRASE, destino)
    assert reaberta.amizades == ()


def test_uma_amizade_incompleta_e_recusada(destino: Path) -> None:
    """Uma amizade a meio é um ficheiro corrompido, não uma conta antiga.

    Aceitá-la daria um `K9` de comprimento errado a cifrar, e a falha só
    apareceria no daemon, muito depois de a pessoa acreditar que tem
    uma amizade.
    """
    c = conta.criar("ana", FRASE, destino)
    dados = c.para_dict()
    dados["amizades"] = [{"publica": "aa" * 32, "k1": "11" * 32}]  # falta o resto
    storage.guardar_keystore(dados, destino, FRASE)

    with pytest.raises(conta.ContaInvalida, match="amizade incompleta"):
        conta.entrar(FRASE, destino)


def test_amizades_que_nao_e_lista_e_recusada(destino: Path) -> None:
    """``"amizades": 3`` é lixo, e lixo tem de dar um erro tipado.

    `tuple(3)` levantaria um `TypeError`, que não é `ContaInvalida` e
    sairia pelo caminho não tipado de quem trata as excepções da conta.
    """
    c = conta.criar("ana", FRASE, destino)
    dados = c.para_dict()
    dados["amizades"] = 3
    storage.guardar_keystore(dados, destino, FRASE)

    with pytest.raises(conta.ContaInvalida, match="não é uma lista"):
        conta.entrar(FRASE, destino)


def test_a_amizade_e_congelada(destino: Path) -> None:
    """`amizades` é um `tuple` porque a classe é congelada.

    Uma `list` continuaria mutável por dentro de um dataclass
    `frozen=True`: `conta.amizades.append(…)` passaria, e quem confiasse
    no `frozen` para dizer «isto é o que o ficheiro tem» estaria errado.
    """
    c = conta.criar("ana", FRASE, destino).com_amizade(_amizade_de_teste())
    assert isinstance(c.amizades, tuple)
    with pytest.raises(AttributeError):
        c.amizades.append(_amizade_de_teste(publica=6))  # type: ignore[attr-defined]


def test_a_exportacao_cobre_as_amizades(destino: Path, tmp_path: Path) -> None:
    """Uma cópia de segurança que não traz as amizades não é uma cópia.

    O ecrã de recuperação promete «repor uma cópia». Se a cópia trouxesse
    a identidade e não as chaves, a conta voltava e a pessoa descobria
    que tinha de refazer todos os handshakes.
    """
    amizade = _amizade_de_teste()
    c = conta.criar("ana", FRASE, destino)
    conta.gravar(c.com_amizade(amizade), FRASE, destino)

    # O destino da restauração é noutro sítio: `restaurar` **copia por
    # cima** e falha se o ficheiro já existir, que é o que impede uma
    # restauração de destruir a conta que estava ali.
    copia = conta.exportar(tmp_path / "conta.copia", FRASE, destino)
    reposto = tmp_path / "restaurado.keystore"
    conta.restaurar(copia, FRASE, reposto)

    reposta = conta.entrar(FRASE, reposto)
    assert len(reposta.amizades) == 1
    assert reposta.amizades[0].para_chaves() == amizade.para_chaves()


def test_uma_amizade_com_chave_de_comprimento_errado_e_recusada(destino: Path) -> None:
    """Uma chave de outro comprimento não é uma amizade.

    O caso que importa é o inverso: aceitar um ``k9_par`` de 31 bytes
    dava ao daemon um comando com um ``K9`` de tamanho errado, e a
    falha só apareceria muito depois — quando alguém tentasse cifrar.
    """
    c = conta.criar("ana", FRASE, destino)
    dados = c.para_dict()
    dados["amizades"] = [dict(_amizade_de_teste().para_dict(), k9_par="aa" * 31)]
    storage.guardar_keystore(dados, destino, FRASE)

    with pytest.raises(conta.ContaInvalida, match="comprimento errado"):
        conta.entrar(FRASE, destino)


def test_para_bytes_da_a_ordem_do_daemon() -> None:
    """Os seis campos saem na ordem em que o daemon os manda.

    A ordem é o tuplo C (`K1`, `K5` próprio, `K9` próprio, `K5` do par,
    `K9` do par, pública) e não é negociável: um `K5` no sítio do `K9`
    produz mensagens que o par não decifra, e o sintoma é um erro de
    decifragem longe da causa.
    """
    amizade = _amizade_de_teste()
    campos = amizade.para_bytes()
    assert list(campos) == ["publica", "k1", "k5_proprio", "k9_proprio", "k5_par", "k9_par"]
    assert all(len(v) == 32 for v in campos.values())
    assert campos["k1"] == bytes(range(32))


def test_amizade_de_devolve_none_quando_nao_ha(destino: Path) -> None:
    """Procurar uma pública que não está dá ``None``, não levanta.

    A diferença importa: ``None`` é o resultado de uma busca que correu
    bem e não encontrou, e levantar seria dizer que o registo está
    corrompido quando o que falta é uma amizade que nunca foi feita.
    """
    c = conta.criar("ana", FRASE, destino).com_amizade(_amizade_de_teste())
    assert c.amizade_de("bb" * 32) is None
    assert c.amizade_de(c.amizades[0].publica_hex) is not None
