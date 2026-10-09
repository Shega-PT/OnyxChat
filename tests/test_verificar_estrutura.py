# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes do verificador de estrutura.

## A pergunta que estes testes respondem

`scripts/verificar_estrutura.py` passa hoje. A pergunta que interessa não
é «passa?», é **«passaria se o mapa estivesse errado?»** — e a resposta
sem estes testes é «não se sabe».

O caminho já mostrou o custo disso: a versão anterior da `Estrutura.txt`
listava sete ficheiros que não existiam, durante meses, sem que nada
desse sinal. Um verificador que só corre contra o estado actual e nunca
contra um estado errado é um teste de fumo com uniforme de fiscal.

Por isso cada teste abaixo **introduz a divergência** e exige que o
verificador a apanhe.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

RAIZ = Path(__file__).resolve().parent.parent
MAPA = RAIZ.parent / "Estrutura.txt"


def _modulo():
    spec = importlib.util.spec_from_file_location(
        "verificar_estrutura", RAIZ / "scripts" / "verificar_estrutura.py"
    )
    assert spec and spec.loader
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


ve = _modulo()


@pytest.fixture
def com_mapa(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    """Um verificador que lê um mapa escrito à mão, num directório só seu."""

    def _escrever(texto: str) -> Path:
        destino = tmp_path / "Estrutura.txt"
        destino.write_text(texto, encoding="utf-8")
        monkeypatch.setattr(ve, "MAPA", destino)
        return destino

    return _escrever


# ---------------------------------------------------------------------
# A árvore é recontruída
# ---------------------------------------------------------------------

def test_um_directorio_de_seccao_da_o_contexto_aos_filhos() -> None:
    """`crypto/` seguido de `├─ rust/` dá `crypto/rust/…`.

    Sem esta regra o mapa leria `rust/` na raiz, e cada ficheiro do
    núcleo de criptografia apareceria em falta.
    """
    nomes = {a.caminho for a in ve.afirmacoes()}
    assert "crypto/rust/src/lib.rs" in nomes
    assert "rust/lib.rs" not in nomes


def test_o_profundimento_vem_da_coluna() -> None:
    """`│   ├─ Cargo.toml` é um nível mais fundo que `├─ rust/`."""
    nomes = {a.caminho for a in ve.afirmacoes()}
    assert "crypto/rust/Cargo.toml" in nomes
    assert "network/daemon_rust/src/ipc.rs" in nomes
    assert "network/daemon_rust/ipc.rs" not in nomes


def test_um_nome_com_barras_e_relativo(
    com_mapa, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """`components/onyx/` dentro de `UI/src/` é `UI/src/components/onyx`.

    Um nome com barras não é um caminho completo: escrevia-se
    `components/onyx/` precisamente para não repetir `UI/src/` quatro
    vezes. Tratá-lo como absoluto dava `components/onyx/OnyxBadge.jsx`,
    que não existe em lado nenhum.
    """
    com_mapa(
        "UI/\n"
        "├─ src/\n"
        "│   ├─ components/onyx/\n"
        "│   │   └─ OnyxBadge.jsx\n"
    )
    nomes = {a.caminho for a in ve.afirmacoes()}
    assert "UI/src/components/onyx/OnyxBadge.jsx" in nomes


def test_tres_ficheiros_na_mesma_linha() -> None:
    """`main.jsx  App.jsx  index.css` são três afirmações, e não uma.

    Com `\\S+` no padrão do marcador, só o primeiro nome entrava na
    lista — e o verificador passava por cima de dois ficheiros que o
    mapa afirma existirem. É o silêncio do buraco original noutra
    forma, e só um teste o apanha.
    """
    nomes = {a.caminho for a in ve.afirmacoes()}
    for n in ("UI/src/main.jsx", "UI/src/App.jsx", "UI/src/index.css"):
        assert n in nomes


def test_um_comentario_nao_e_uma_afirmacao(
    com_mapa, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Um caminho citado **num comentário** não é uma afirmação.

    O mapa explica coisas citando ficheiros — «`docs/test_vectors.md` é
    gerado», «só `gen/` e `target/`». Se o comentário entrasse na lista,
    o verificador trataria a explicação de uma afirmação, e num mapa que
    citasse ali um caminho inexistente acusaria um ficheiro que ninguém
    afirmou ter.

    O teste usa um mapa escrito à mão porque no mapa real o caminho
    citado no comentário **também** é uma entrada — e o teste não
    distinguiria as duas coisas.
    """
    raiz = tmp_path / "repo"
    raiz.mkdir()
    monkeypatch.setattr(ve, "RAIZ", raiz)
    com_mapa("docs/\n├─ index.md      # este é uma entrada\n")

    nomes = [a.caminho for a in ve.afirmacoes()]
    assert nomes == ["docs/index.md"]

    com_mapa("docs/\n├─ index.md      # daqui vem o resto, ver guia.md\n")
    nomes = [a.caminho for a in ve.afirmacoes()]
    assert nomes == ["docs/index.md"], nomes
    assert "docs/guia.md" not in nomes


# ---------------------------------------------------------------------
# As duas direcções da divergência
# ---------------------------------------------------------------------

def test_um_ficheiro_que_desaparece_e_erro(
    com_mapa, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Nomear um ficheiro que não existe tem de dar erro."""
    raiz = tmp_path / "repo"
    (raiz / "docs").mkdir(parents=True)
    (raiz / "docs" / "index.md").write_text("x", encoding="utf-8")
    monkeypatch.setattr(ve, "RAIZ", raiz)
    com_mapa("# docs\n-----\ndocs/index.md\n")

    af = ve.afirmacoes()
    assert [a.caminho for a in af] == ["docs/index.md"]
    # O ficheiro existe: o verificador passa. Apagá-lo tem de falhar.
    assert ve.afirmacoes()[0].caminho in ve.ficheiros_reais()
    (raiz / "docs" / "index.md").unlink()
    assert ve.afirmacoes()[0].caminho not in ve.ficheiros_reais()


def test_o_til_diz_que_o_ficheiro_nao_existe(
    com_mapa, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """`~ficheiro` é uma afirmação de ausência, e é verificada como tal.

    Sem o til, a secção histórica seria indistinguível de uma lista de
    ficheiros reais e o verificador reportaria como «em falta» aquilo que
    o próprio mapa diz que nunca existiu.
    """
    raiz = tmp_path / "repo"
    raiz.mkdir()
    monkeypatch.setattr(ve, "RAIZ", raiz)
    com_mapa("# Historico\n----------\n| `~plugins.py` | nunca existiu |\n")

    af = ve.afirmacoes()
    assert len(af) == 1
    assert af[0].caminho == "plugins.py"
    assert af[0].existe is False
    assert af[0].caminho not in ve.ficheiros_reais()

    # Se um dia alguém criar o ficheiro, a nota histórica passa a ser
    # falsa — e o verificador tem de dizer isso.
    (raiz / "plugins.py").write_text("x", encoding="utf-8")
    assert af[0].caminho in ve.ficheiros_reais()


def test_o_verificador_repassa_num_mapa_que_inventa(
    com_mapa, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A versão anterior da Estrutura.txt tem de dar erro, não verde.

    Este é o teste que fecha a porta por onde passou o problema: três
    caminhos que não existem, escritos como se fossem reais.
    """
    raiz = tmp_path / "repo"
    (raiz / "crypto").mkdir(parents=True)
    (raiz / "crypto" / "real.rs").write_text("x", encoding="utf-8")
    monkeypatch.setattr(ve, "RAIZ", raiz)
    com_mapa(
        "# Raiz\n"
        "----------\n"
        "crypto/\n"
        "├─ real.rs\n"
        "├─ inventado.py\n"
        "└─ outro_que_nao_existe.lua\n"
    )

    reais = ve.ficheiros_reais()
    faltam = [a.caminho for a in ve.afirmacoes()
              if a.existe and a.caminho not in reais]
    assert faltam == ["crypto/inventado.py", "crypto/outro_que_nao_existe.lua"]


# ---------------------------------------------------------------------
# O mapa real
# ---------------------------------------------------------------------

def test_o_mapa_real_nao_tem_nada_a_lembrar() -> None:
    """Nenhuma afirmação do mapa real fica por resolver."""
    reais = ve.ficheiros_reais()
    problemas = [
        (a.caminho, a.existe)
        for a in ve.afirmacoes()
        if a.existe != (a.caminho in reais)
    ]
    assert not problemas, problemas


# ---------------------------------------------------------------------
# As contagens
# ---------------------------------------------------------------------

def test_a_unidade_de_indentacao_e_medida() -> None:
    """A árvore do `UI/README.md` indenta com 3 colunas; a do mapa, com 4.

    Uma constante embutida dava `3 // 4 = 0`, e `src/`, `lib/`,
    `components/` e `pages/` ficavam ao mesmo nível. O caminho passava a
    ser `components/onyx` — que não existe — e a contagem devolvia zero
    **sem dar erro nenhum**, que é a forma mais cara de um verificador
    falhar.
    """
    assert ve.unidade_de_indentacao(ve.ler("Estrutura.txt")) == 4
    assert ve.unidade_de_indentacao(ve.ler("UI/README.md")) == 3


def test_as_contagens_das_duas_arvores_conferem() -> None:
    """Cada contagem que um documento afirma tem de bater com o disco.

    Foram quatro números errados numa só auditoria — 19 documentos, 8
    ecrãs, 24 componentes, e um ficheiro a menos — e nenhum era
    verificável a olho.
    """
    for documento, raiz in ve.RAIZES_DAS_ARVORES.items():
        achados = ve.contagens(documento, raiz)
        assert achados, f"{documento} não tem contagens para conferir"
        for descricao, afirmado, medido in achados:
            assert afirmado == medido, f"{documento}: {descricao}"


def test_as_contagens_versionadas_conferem() -> None:
    """As sementes e os ficheiros JavaScript são contados no **índice**.

    A diferença entre o disco e o índice é a lição inteira de B6: as
    sementes estavam todas no disco e nenhuma no índice — 173 ficheiros,
    50 versionados — e o portão dizia «ok» porque contava o que estava
    à vista. É por isso que estas duas contagens usam `git ls-files`.
    """
    assert ve._conta(ve.RAIZ / "fuzz" / "corpus", "sementes") == 50
    js = ve._conta(ve.RAIZ / "UI", "ficheiros JavaScript versionados")
    assert js == 91, js


def test_a_contagem_de_sementes_ignora_o_disco() -> None:
    """A contagem vem do índice, e o índice não depende de quem executou.

    Havia aqui uma segunda asserção que comparava o disco com o índice e
    afirmava que o disco tinha **sempre** mais sementes — a diferença que
    o `cargo fuzz` vai escrevendo à medida que encontra entradas novas.

    É uma afirmação sobre o estado da máquina, não sobre o verificador, e
    o CI provou-o: num runner limpo nunca se correu fuzzing, o disco tem
    exactamente as 50 sementes versionadas, e a comparação dá
    `50 > 50` — falso. O teste passava em todas as máquinas onde alguém
    já tinha corrido `cargo fuzz` e falhava em todas as outras, incluindo
    a que mais importa, que é a de quem não tem nada gerado.

    O que fica é o que a asserção queria de facto provar: a contagem
    segue o índice, e o índice não muda com o que o fuzzing gera. Um
    clone tem de dar o mesmo número que a máquina onde o fuzzing já
    correu, e é isso que se verifica aqui.
    """
    em_disco = len([p for p in (ve.RAIZ / "fuzz" / "corpus").rglob("*") if p.is_file()])
    indexado = ve._conta(ve.RAIZ / "fuzz" / "corpus", "sementes")
    assert indexado == 50
    # O disco nunca pode ter **menos** do que o índice: um `git rm` de um
    # ficheiro versionado é a única forma de isso acontecer, e nesse caso
    # a falha é do índice, não da contagem.
    assert em_disco >= indexado
    # E a contagem não muda porque o disco tenha mais. `cargo fuzz` escreve
    # sementes novas a cada execução; o número versionado não pode seguir
    # esse ritmo, que é a razão de ser de `git ls-files` nesta contagem.
    (ve.RAIZ / "fuzz" / "corpus" / "k4_decoder").mkdir(parents=True, exist_ok=True)
    descartada = ve.RAIZ / "fuzz" / "corpus" / "k4_decoder" / "gerada-pelo-fuzz.bin"
    descartada.write_bytes(b"\x00")
    try:
        assert ve._conta(ve.RAIZ / "fuzz" / "corpus", "sementes") == indexado
    finally:
        descartada.unlink()


def test_a_palavra_composta_vence_a_simples() -> None:
    """«ficheiros JavaScript versionados» conta JavaScript, não tudo.

    Numa alternância o Python devolve a primeira que casa. Se `ficheiros`
    vier antes da forma composta, o número sai errado **sem erro** — e
    era o que aconteceria com a lista por ordem alfabética.
    """
    # A tupla é ``(descrição, afirmado, medido)``; o substantivo está
    # dentro da descrição, entre aspas angulares.
    substantivos = {c[0].split("«")[1].split("»")[0] for c in ve.contagens()
                    if "«" in c[0]}
    assert "ficheiros JavaScript versionados" in substantivos, substantivos
    assert "sementes" in substantivos, substantivos
    m = ve._CONTAGEM.search("91 ficheiros JavaScript versionados")
    assert m and m.group(2) == "ficheiros JavaScript versionados"


def test_uma_contagem_errada_e_lida_como_errada(com_mapa, monkeypatch) -> None:
    """Um número que não bate tem de ser lido, não aceite.

    Sem esta prova, o verificador das contagens podia estar a devolver
    sempre o que o disco tem e nunca o que o documento diz — e passar
    em todos os testes.
    """
    original = ve.ler()
    try:
        ve.MAPA.write_text(
            "# docs/ — 3 documentos\n"
            "docs/\n"
            "├─ index.md\n",
            encoding="utf-8",
        )
        lidos = ve.contagens()
        assert lidos, "a contagem errada não foi lida"
        assert lidos[0][1] == 3, lidos
    finally:
        ve.MAPA.write_text("\n".join(original), encoding="utf-8")


def test_o_mapa_e_lido_como_uma_arvore_com_pilha() -> None:
    """Sanidade do leitor: o número de afirmações não é irrealmente baixo.

    Um mapa de 145 afirmações lidas como 12 significaria que o formato
    mudou e o verificador deixou de fazer o que diz — e passaria, porque
    `verificar_estrutura.py` só falha quando encontra uma divergência.
    """
    af = ve.afirmacoes()
    assert len(af) > 140
    assert sum(1 for a in af if "/" in a.caminho) > 100
    assert any(not a.existe for a in af), "o til deixou de ser lido"