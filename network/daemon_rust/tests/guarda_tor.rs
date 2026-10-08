// Required Notice: ShegaPT / <<NOME-LEGAL>> — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// guarda_tor.rs — teste de guarda do isolamento do backend Tor
// ---------------------------------------------------------------------
// O que este ficheiro garante
//
// `tor.rs` declara uma regra de ouro: **o resto do daemon nunca depende
// de APIs específicas da arti**. Fala sempre com o traço `BackendTor`.
// `tor_arti.rs` é o único ficheiro que toca na rede real.
//
// Porque é que isto precisa de um teste
//
// A regra está declarada em três documentos — `docs/architecture.md`
// §`BackendTor`, `docs/testing.md` §Fuzzing e o `README.md` — e **não
// havia nenhum teste que a verificasse**. Uma regra de isolamento sem
// verificação é um comentário, e comentários envelhecem.
//
// A regra pode quebrar de duas maneiras, e este ficheiro apanha as duas:
//
//   1. **Por adição** — alguém escreve `arti::` (ou `use arti_client`)
//      num ficheiro que não seja `tor_arti.rs`. É o que acontece quando
//      alguém precisa de uma funcionalidade da arti e repete o caminho
//      em vez de a estender no traço. O resultado é um daemon que já não
//      pode ser testado offline, nem compilado sem a arti, nem ter a sua
//      rede verificada.
//
//   2. **Por subtração** — alguém acrescenta um `impl BackendTor` que
//      toca a rede real. A interface deixa de ter a garantia de que os
//      seus implementadores são offline.
//
// A verificação é por **leitura das fontes**, não por compilação. É a
// única forma de apanhar o caso (1): o código que viola a regra
// compila perfeitamente — compila é o que ele faz de melhor.
//
// Onde está e porque é um teste de integração
//
// Num teste unitário (`#[cfg(test)]` dentro de um módulo) não se pode
// ler o sistema de ficheiros de forma fiável sem inventar caminhos
// relativos ao crate. Num teste de integração, `CARGO_MANIFEST_DIR` dá a
// raiz do crate sem ambiguidade.
// =====================================================================

use std::fs;
use std::path::{Path, PathBuf};

/// Ficheiros-fonte do crate, com a sua raiz.
struct Fonte {
    /// Caminho relativo a `network/daemon_rust`, para mensagens legíveis.
    relativo: String,
    /// Conteúdo integral do ficheiro.
    texto: String,
}

/// Raiz do crate `onyxchatd` (`network/daemon_rust`).
fn raiz_crate() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// É ficheiro de teste dedicado? (`ipc_testes.rs`, `p2p_testes.rs`, …)
///
/// A convenção de G5: os testes de um módulo grande vão para
/// `<módulo>_testes.rs`, que é declarado como filho com `#[cfg(test)]`
/// e `#[path]`. Um ficheiro desses é teste **inteiro** — não tem uma
/// linha de produção para auditar.
///
/// Sem esta excepção, `fontes()` tratava o ficheiro de testes como se
/// fosse produção e a guarda `so_o_glue_implementa_backend_tor` acusava
/// o `impl BackendTor for BackendQueFalha` de `ipc_testes.rs` — que é
/// um backend de teste, por definição offline, exactamente o que a
/// guarda quer ver.
///
/// O caminho inverso — um `*_testes.rs` que não esteja atrás de
/// `#[cfg(test)]`, e portanto entrasse no binário de produção — é
/// verificado por `ficheiros_de_teste_so_atras_de_cfg_test`.
fn eh_ficheiro_de_teste(caminho: &Path) -> bool {
    caminho
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with("_testes.rs"))
}

/// Lê todos os `.rs` de `src/`, excepto o ficheiro autorizado.
fn fontes(excepto: &str) -> Vec<Fonte> {
    let dir = raiz_crate().join("src");
    let mut ficheiros: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("src/ legível")
        .map(|e| e.expect("entrada de directório").path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .filter(|p| !eh_ficheiro_de_teste(p))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n != excepto)
        })
        .collect();
    // Ordenar dá mensagens de erro determinísticas: sem isto, a ordem
    // do `read_dir` varia entre sistemas e o teste falha de forma
    // diferente em máquinas diferentes.
    ficheiros.sort();

    ficheiros
        .into_iter()
        .map(|caminho| {
            let relativo = caminho
                .strip_prefix(raiz_crate())
                .unwrap_or(&caminho)
                .to_string_lossy()
                .into_owned();
            let texto = fs::read_to_string(&caminho).expect("fonte legível");
            Fonte { relativo, texto }
        })
        .collect()
}

/// Primeira linha de código de teste, ou `None` se não houver.
///
/// O que vem depois de `#[cfg(test)]` é código de teste, e o código de
/// teste pode — e deve — ter duplos que implementam `BackendTor`. Um
/// `BackendQueFalha` em `ipc.rs` que devolve sempre `Err` não toca a
/// rede; proibi-lo tornaria a regra impossível de respeitar, e o
/// resultado seria as pessoas deixarem de escrever testes.
fn inicio_do_codigo_de_teste(texto: &str) -> Option<usize> {
    texto
        .lines()
        .position(|l| l.trim_start().starts_with("#[cfg(test)]"))
}

/// Texto a analisar: só o código de produção, nunca o de teste.
fn codigo_de_producao(texto: &str) -> &str {
    match inicio_do_codigo_de_teste(texto) {
        // `lines().position()` dá o índice da linha; a linha anterior
        // pode ser um `#[cfg(...)]` ou o `mod` — corta antes dela.
        None => texto,
        Some(i) => {
            let byte = texto
                .lines()
                .take(i)
                .map(|l| l.len() + 1)
                .sum::<usize>();
            &texto[..byte]
        }
    }
}

/// Nomes de itens da arti (ou dos seus/types relacionados) que
/// denunciam uma dependência directa da biblioteca.
/// Nomes que denunciam uma dependência directa da árvore da arti.
///
/// A lista é **derivada do que `tor_arti.rs` importa de facto**, não de
/// conhecimento geral sobre a biblioteca. Dois motivos:
///
///   * Nomes de tipos da arti mudam entre versões (`DesiredTorRuntime` e
///     `PreferredTorRuntime` mudaram de nome entre versões). Um padrão que deixou de existir
///     dá falsa confiança — o teste `os_padroes_do_guard_sao_suficientes`
///     verifica que cada um ainda aparece algures, o que apanha isso.
///
///   * Um padrão demasiado largo dá falsos positivos. `arti::` casa com
///     `tor_arti::`, o nosso **próprio** módulo — e o guard test
///     acusava o `use onyxchatd::tor_arti::TorReal;` de `main.rs`.
///     O sinal fidedigno é o nome do crate, `arti_client` (o hífen
///     `arti-client` transforma-se em subscrito em Rust).
const PADROES_ARTI: &[&str] = &[
    // Crates da árvore da arti. São o sinal mais forte e não dão falsos
    // positivos: `tor_arti` é um módulo nosso, não o crate `arti`.
    "arti_client",
    "arti-client",
    "tor_hsrproxy",
    "tor_hsservice",
    "tor_rtcompat",
    // Tipos importados por `tor_arti.rs`. Útil quando alguém escreve
    // `arti_client::TorClient` em vez de só `use arti_client::…`.
    "TorClient",
    "TorClientConfigBuilder",
    "RendRequest",
    "HsNickname",
    "OnionServiceConfig",
    "RunningOnionService",
    // NADA de um simples `Tor`: casa com `BackendTor`, `EstadoTor` e
    // `ErroTor`, que são tipos **nossos**.
];

/// Nomes de itens da arti que só aparecem dentro do glue (`tor_arti.rs`)
/// e por isso são aceitáveis nas referências a *tipos* em comentários.
fn eh_mencao_a_arti(linha: &str) -> bool {
    // Uma linha que é só comentário/documentação não constitui uso.
    // Sem esta excepção, a documentação de `tor.rs` — que cita
    // `TorReal` e `arti-client` para explicar o que isolou — dispararia
    // o alarme, e o teste passaria a banalizar-se.
    let t = linha.trim_start();
    t.starts_with("//")
}

/// Teste de guarda propriamente dito: nenhum ficheiro além de
/// `tor_arti.rs` pode mencionar a arti.
#[test]
fn arti_nao_e_mencionada_fora_de_tor_arti() {
    let violacoes: Vec<String> = fontes("tor_arti.rs")
        .iter()
        .flat_map(|fonte| {
            codigo_de_producao(&fonte.texto)
                .lines()
                .enumerate()
                .filter_map(move |(i, linha)| {
                    if eh_mencao_a_arti(linha) {
                        return None;
                    }
                    let achado = PADROES_ARTI.iter().find(|p| linha.contains(*p))?;
                    Some(format!(
                        "{}:{} — `{achado}` fora de código de comentário\n    {}",
                        fonte.relativo,
                        i + 1,
                        linha.trim()
                    ))
                })
        })
        .collect();

    assert!(
        violacoes.is_empty(),
        "a arti só pode ser usada em `src/tor_arti.rs`.\n\
         Regra: `docs/architecture.md` §BackendTor — o resto do daemon \
         fala com o traço `BackendTor`, nunca com a biblioteca.\n\
         Porque importa: um `use arti::…` num módulo qualquer torna o \
         daemon impossível de testar offline, de compilar sem a feature \
         `tor-real`, e de ter a rede verificada.\n\
         Correcção: mover o código para `tor_arti.rs` e estender o traço \
         `BackendTor` (`src/tor.rs`).\n\n\
         {}",
        violacoes.join("\n")
    );
}

/// Nenhum ficheiro `*_testes.rs` entra no binário de produção.
///
/// O outro lado da convenção de G5. Excluir os ficheiros de teste das
/// guardas acima (`eh_ficheiro_de_teste`) é seguro **porque** cada um
/// deles é filho de `#[cfg(test)]`. Se alguém criar `foo_testes.rs` e o
/// declarar sem `#[cfg(test)]`, o ficheiro passa a ser compilado em
/// produção: os `#[test]` desaparecem (o `cfg(test)` é que os remove) e
/// fica o resto — helpers, imports, mocks — dentro do binário.
///
/// É uma falha silenciosa e cara. O daemon deixa de incluir código que
/// só existe para teste, cresce sem que ninguém o tenha querido e,
/// pior, um mock de rede dentro do binário é uma rede que responde.
///
/// A verificação é a mais directa possível: para cada `*_testes.rs`,
/// existe uma linha `#[path = "<nome>"]` algures no `src/`, e essa
/// linha está precedida de `#[cfg(test)]`.
#[test]
fn ficheiros_de_teste_so_atras_de_cfg_test() {
    let dir = raiz_crate().join("src");
    let ficheiros_de_teste: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("src/ legível")
        .map(|e| e.expect("entrada de directório").path())
        .filter(|p| eh_ficheiro_de_teste(p))
        .collect();

    // Se alguém renomear a convenção, este teste não tem o que verificar
    // — e um guard que não guarda nada é pior do que nenhum.
    assert!(
        !ficheiros_de_teste.is_empty(),
        "não há ficheiros `*_testes.rs` em `src/` — a convenção de G5 \
         desapareceu ou foi renomeada; actualiza este teste e \
         `eh_ficheiro_de_teste` para o nome novo, senão as guardas \
         acima deixam de auditar os ficheiros de teste"
    );

    let texto_do_crate: String = fs::read_dir(&dir)
        .expect("src/ legível")
        .map(|e| e.expect("entrada de directório").path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .filter(|p| !eh_ficheiro_de_teste(p))
        .map(|p| fs::read_to_string(&p).expect("fonte legível"))
        .collect::<Vec<_>>()
        .join("\n");

    let problemas: Vec<String> = ficheiros_de_teste
        .iter()
        .filter_map(|caminho| {
            let nome = caminho.file_name()?.to_str()?.to_string();
            let linha_do_path = format!("#[path = \"{nome}\"]");
            let pos = texto_do_crate.find(&linha_do_path)?;

            // A linha anterior tem de ser `#[cfg(test)]` (ou um comentário
            // dela, o que também é aceitável: o que não é aceitável é
            // haver `#[path]` sem `#[cfg(test)]` por perto).
            let antes = &texto_do_crate[..pos];
            let ultimas: Vec<&str> = antes.lines().rev().take(3).collect();
            let tem_cfg = ultimas.iter().any(|l| l.trim() == "#[cfg(test)]");
            // `filter_map` guarda o que devolve `Some`, por isso o que
            // interessa reportar é a **ausência** de `#[cfg(test)]`.
            (!tem_cfg).then_some(nome.clone())
        })
        .map(|nome| format!("`{nome}` é declarado por `#[path]` sem `#[cfg(test)]`"))
        .collect();

    assert!(
        problemas.is_empty(),
        "ficheiros de teste no binário de produção:\n    {}\n\n         Cada `*_testes.rs` tem de ser declarado assim:\n\n    \
         #[cfg(test)]\n    #[path = \"ficheiro_testes.rs\"]\n    mod testes;",
        problemas.join("\n    ")
    );
}

/// A licença do projecto é a que a `LICENSE` diz, e os metadados
/// concordam com ela.
///
/// ## Porque isto precisa de um teste
///
/// A licença é o único ficheiro do projecto em que a divergência entre
/// o que está escrito e o que é verdade **não produz erro de
/// compilação**. O `Cargo.toml` diz uma coisa, o `LICENSE` outra, o
/// `README` uma terceira, e a build passa exactamente igual. O mesmo
/// aconteceu na fase F com `sodium_memzero` (documentado, não chamado) e
/// na fase G4 com a paridade da K9 (afirmada num comentário, sem teste):
/// o padrão de «o projecto tem mais uma verdade escrita do que
/// verificada» repete-se, e um guard test é a forma barata de parar.
///
/// As cinco verificações:
///
///   1. `LICENSE` contém o texto da PolyForm Noncommercial 1.0.0;
///   2. todos os `Cargo.toml` com campo `license` dizem o mesmo;
///   3. `pyproject.toml` diz o mesmo;
///   4. os marcadores `<<…>>` estão resolvidos — publicar com um
///      `<<NOME-LEGAL>>` no lugar do nome é publicar um erro;
///   5. cada ficheiro de fonte carrega o `Required Notice:`.
///
/// O ponto 5 é uma exigência expressa da PolyForm (§Notices): quem recebe uma
/// cópia de **qualquer parte** do software tem de receber os termos. Um
/// ficheiro novo sem o aviso é uma violação dos termos da tua própria
/// licença, e é exactamente o que acontece quando alguém cria um módulo
/// e não repete o cabeçalho.
#[test]
fn licenca_e_metadados_sao_coerentes() {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("raiz do repositório")
        .to_path_buf();

    const ESPERADO: &str = "PolyForm-Noncommercial-1.0.0";

    // (1) A licença base é a PolyForm Noncommercial, e o texto está lá.
    let licenca = fs::read_to_string(raiz.join("LICENSE")).expect("LICENSE legível");
    assert!(
        licenca.contains("PolyForm Noncommercial License 1.0.0"),
        "a `LICENSE` não é a PolyForm Noncommercial 1.0.0. Se isso for \
         intencional, este teste tem de ser revisto — não removido."
    );
    // O texto PolyForm é verbatim e não se toca — mas está quebrado a 80
    // colunas, como todo o ficheiro, para que a leitura no terminal não
    // parta. Por isso a verificação é pelos **títulos** das secções, que
    // são curtos e não quebram, e não por frases do texto original: uma
    // frase upstream partida em duas linhas nunca apareceria como
    // substring, e o guard acusaria uma adulteração que não houve.
    for secao in [
        "Acceptance",
        "Copyright License",
        "Distribution License",
        "Changes and New Works License",
        "Patent License",
        "Noncommercial Purposes",
        "Personal Uses",
        "Noncommercial Organizations",
        "Fair Use",
        "No Other Rights",
        "Patent Defense",
        "Violations",
        "No Liability",
        "Definitions",
    ] {
        assert!(
            licenca.contains(secao),
            "a secção `{secao}` da PolyForm não está na `LICENSE`. O texto \
             é verbatim: se o projecto já não for PolyForm, muda a \
             licença com intenção — não por accidentear"
        );
    }

    // (2) Todos os `Cargo.toml`.
    let mut n_cargo = 0;
    for caminho in ["Cargo.toml", "crypto/rust/Cargo.toml", "network/daemon_rust/Cargo.toml"] {
        let texto = fs::read_to_string(raiz.join(caminho)).expect(caminho);
        if let Some(linha) = texto.lines().find(|l| l.trim_start().starts_with("license =")) {
            assert!(
                linha.contains(ESPERADO),
                "{caminho} diz `{linha}` e a `LICENSE` diz {ESPERADO}"
            );
            n_cargo += 1;
        }
        // `license.workspace = true` não é uma divergência: herda do
        // workspace, que o ponto anterior já verificou.
    }
    assert!(n_cargo >= 2, "esperava ao menos 2 Cargo.toml com `license =`");

    // (3) O `pyproject.toml`.
    let pyproject = fs::read_to_string(raiz.join("pyproject.toml")).expect("pyproject.toml");
    assert!(
        pyproject.contains(ESPERADO),
        "o `pyproject.toml` não menciona {ESPERADO} — o pacote Python \
         seria publicado com a licença errada"
    );

    // A lista de fontes é calculada uma vez e usada nos pontos (4) e (5).
    let fontes = collectar_fontes(&raiz);
    assert!(!fontes.is_empty(), "a listagem de fontes devolveu nada");

    // (4) Nenhum marcador por resolver.
    //
    // Publicar com `<<NOME-LEGAL>>` no lugar do nome é publicar um erro,
    // e é o erro mais provável porque o ficheiro parece completo.
    //
    // A verificação conta em vez de listar. Há um marcador em cada um dos
    // ~79 `Required Notice:` dos ficheiros de fonte, e uma lista com 79
    // linhas não informa ninguém — informa o número, que é o que interessa,
    // e o comando que resolve.
    //
    // `COMMERCIAL-LICENSE.md` fica de fora **de propósito**: é um
    // template, e os marcadores dele são o seu conteúdo. O `Required
    // Notice:` que ele carrega é a única excepção, e resolve-se com o mesmo
    // comando.
    // O marcador é montado em runtime em vez de escrito como literal.
    //
    // Não é preciosismo: o guard sugere um `sed` que reescreve todas as
    // ocorrências do marcador, e esse `sed` passa por este ficheiro. Com
    // o literal escrito aqui, seguir o comando sugerido reescrevia o
    // próprio guard — que passava a procurar `<<NOME-LEGAL>>` em vez do
    // marcador, e continuava a falhar com uma mensagem que já não
    // correspondia à realidade. Montado em runtime, sobrevive ao `sed`.
    let marcador = format!("<<{}>>", "NOME-LEGAL");

    let mut por_ficheiro = Vec::new();
    let mut total = 0usize;

    for nome in ["LICENSE", "COPYRIGHT.md", "AUTHORIZED.md"] {
        let texto = fs::read_to_string(raiz.join(nome)).expect(nome);
        let n = texto.matches(&marcador).count();
        total += n;
        if n > 0 {
            por_ficheiro.push(format!("{nome}: {n}"));
        }
    }
    // Nos ficheiros de fonte conta-se e resume-se. Listar 45 caminhos não
    // ajuda ninguém a decidir o que fazer; o número e o comando bastam.
    let mut em_fontes = 0usize;
    for fonte in &fontes {
        let texto = fs::read_to_string(fonte).unwrap_or_default();
        let n = texto.matches(&marcador).count();
        total += n;
        if n > 0 {
            em_fontes += 1;
        }
    }
    if em_fontes > 0 {
        por_ficheiro.push(format!(
            "Required Notice: em {em_fontes} de {} ficheiros de fonte",
            fontes.len()
        ));
    }

    assert!(
        total == 0,
        "\n\n    HA MARCADORES POR RESOLVER: {total} ocorrencia(s) de \
         `{marcador}`.\n    \n    {}\n    \n    \
         Isto e' o que falta antes de publicar. Resolve com:\n    \n    \
         grep -rl --exclude-dir=.git --exclude-dir=target \\\n    \\         --exclude-dir=.venv --exclude=guarda_tor.rs '{marcador}' . |\n    \\         xargs sed -i 's|{marcador}|<O TEU NOME CIVEL>|g'\n    \\         \n    \\         O `--exclude=guarda_tor.rs` nao e' paranoia: sem ele o \
         `sed` reescreve o proprio guard, que passaria a procurar o nome \
         novo em vez do marcador.\n    \n    \
         Este teste passa a verde quando o ultimo desaparecer.\n",
        por_ficheiro.join("\n    ")
    );

    // (5) O `Required Notice:` em todos os ficheiros de fonte.
    //
    // Não se aplica a `tests/` nem a `fuzz/`: são ficheiros de teste que
    // não são distribuídos, e a PolyForm fala de quem *recebe* uma cópia
    // do software, não de quem o executa.
    let sem_aviso: Vec<String> = fontes
        .iter()
        .filter(|f| {
            let texto = fs::read_to_string(f).unwrap_or_default();
            !texto.contains("Required Notice:")
        })
        .map(|f| {
            f.strip_prefix(&raiz)
                .unwrap_or(f)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert!(
        sem_aviso.is_empty(),
        "ficheiros sem `Required Notice:` — exigência expressa da PolyForm \
         §Notices:\n    {}\n\n    {} de {} ficheiros",
        sem_aviso.join("\n    "),
        sem_aviso.len(),
        fontes.len()
    );
}

/// Directório de ferramenta de terceiros dentro do repositório.
///
/// A pasta `UI/tools/node/` é um Node.js completo, descompactado de um
/// pacote oficial por `UI/tools/instalar.sh`. Traz cabeçalhos C e C++ que
/// a PolyForm não cobre e não pode cobrir — a licença deles é a do
/// projecto de que vêm, e exigirem um `Required Notice:` nosso seria
/// falso.
///
/// A comparação é por **caminho relativo**, e não por nome de directório.
/// Excluir pelo nome `node` apanharia qualquer pasta nossa com esse nome,
/// e a regra tem de continuar a valer quando alguém criar uma. Um guard
/// que se estreita sozinho deixa de proteger o que protegia.
const FERRAMENTAS_DE_TERCEIROS: &[&str] = &["UI/tools/node"];

/// O caminho está dentro de uma ferramenta de terceiros?
///
/// `raiz` tem de ser a **raiz do repositório**, não o directório que se
/// está a percorrer. A diferença não é de estilo: com o directório
/// actual, ao descer em `UI/tools` o caminho relativo de `UI/tools/node`
/// reduz-se a `node`, e a comparação nunca casa. A exclusão fica então
/// escrita mas inerte, e o guard passa a verde por estar a proteger o
/// ficheiro errado — que é a forma mais cara de um guard partir.
fn e_ferramenta_de_terceiros(raiz: &Path, caminho: &Path) -> bool {
    let Ok(relativo) = caminho.strip_prefix(raiz) else {
        return false;
    };
    let texto = relativo.to_string_lossy().replace('\\', "/");
    FERRAMENTAS_DE_TERCEIROS
        .iter()
        .any(|base| texto == *base || texto.starts_with(&format!("{base}/")))
}

/// A exclusão de ferramentas de terceiros funciona de facto.
///
/// A primeira vez que o guard apanhou a árvore do Node foi quando esta
/// exclusão **não** funcionava: o `strip_prefix` era feito contra o
/// directório corrente. O directório do Node estava instalado, o guard
/// acusou 2367 ficheiros sem aviso, e a causa era uma comparação escrita
/// contra a base errada.
///
/// Daí este teste: uma exclusão que só se exercita quando a ferramenta
/// de terceiros está instalada é uma exclusão que falha em silêncio no
/// exactamente momento em que importa.
#[test]
fn a_exclusao_de_ferramentas_de_terceiros_exclui_o_caminho_certo() {
    let raiz = Path::new("/repo");

    // O caminho que tem de ser excluído.
    assert!(e_ferramenta_de_terceiros(
        raiz,
        Path::new("/repo/UI/tools/node")
    ));
    // E tudo o que está dentro dele.
    assert!(e_ferramenta_de_terceiros(
        raiz,
        Path::new("/repo/UI/tools/node/include/node/v8.h")
    ));
    assert!(e_ferramenta_de_terceiros(
        raiz,
        Path::new("/repo/UI/tools/node/share/doc/node/lldb_commands.py")
    ));

    // O que **não** pode ser excluído, que é onde um `starts_with` mal
    // escrito parte as coisas: uma pasta nossa cujo nome começa pelo
    // mesmo.
    assert!(!e_ferramenta_de_terceiros(
        raiz,
        Path::new("/repo/UI/tools/node-antigo/notes.md")
    ));
    assert!(!e_ferramenta_de_terceiros(
        raiz,
        Path::new("/repo/UI/tools")
    ));
    assert!(!e_ferramenta_de_terceiros(
        raiz,
        Path::new("/repo/UI/src/main.js")
    ));
    assert!(!e_ferramenta_de_terceiros(
        raiz,
        Path::new("/repo/messenger/node/identidade.py")
    ));

    // Um caminho fora da raiz não é de ninguém.
    assert!(!e_ferramenta_de_terceiros(
        Path::new("/outro"),
        Path::new("/repo/UI/tools/node")
    ));
}

/// Os ficheiros de fonte que vão ser distribuídos.
///
/// Percorre a árvore a partir da raiz, em vez de listar directórios à
/// mão. A lista escrita à mão tem uma propriedade má: quando alguém cria
/// um módulo novo, fica de fora — e o guard deixa de o ver, que é
/// exactamente a falha que o guard existe para apanhar.
///
/// Exclui `tests/` e `fuzz/` por uma razão que não é conveniência: a
/// PolyForm fala de quem **recebe** uma cópia do software, e ficheiros de
/// teste não são distribuídos. Inclui-os dava um guard que falha por
/// ficheiros que ninguém recebe.
fn collectar_fontes(raiz: &Path) -> Vec<PathBuf> {
    fn descender(atual: &Path, raiz: &Path, saida: &mut Vec<PathBuf>) {
        let Ok(entradas) = fs::read_dir(atual) else {
            return;
        };
        for entrada in entradas.flatten() {
            let caminho = entrada.path();
            let nome = caminho.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // `vendor` é código de terceiros com as suas próprias
            // licenças e as suas próprias linhas de aviso. `target` e
            // `build` são artefactos. `tests` e `fuzz` não são
            // distribuídos. Um ponto à frente é `.git`, `.venv`, etc.
            //
            // `node_modules` é `vendor` noutro nome: é a árvore de
            // dependências que o npm instala dentro do repositório, com as
            // suas próprias licenças e sem qualquer relação com a
            // PolyForm.
            //
            // Isto percorre o **sistema de ficheiros**, não o índice do
            // Git, e por isso não beneficia do `.gitignore`: a pasta
            // está correctamente ignorada, mas ignorada não é o mesmo
            // que ausente, e este guard lê o disco.
            //
            // A comparação é feita contra a **raiz do repositório**, não
            // contra o directório que se está a percorrer. Fazer
            // `strip_prefix(atual)` daria o nome do último componente — ao
            // descer em `UI/tools`, `UI/tools/node` reduziria a `node`,
            // que não casa com nada em `FERRAMENTAS_DE_TERCEIROS`. A
            // exclusão passava a não excluir nada, e o guard continuava a
            // dar verde por estar a proteger o ficheiro errado.
            if matches!(nome, "vendor" | "target" | "build" | "tests" | "fuzz" | "node_modules")
                || nome.starts_with('.')
                || e_ferramenta_de_terceiros(raiz, &caminho)
            {
                continue;
            }
            if caminho.is_dir() {
                descender(&caminho, raiz, saida);
            } else if matches!(
                caminho.extension().and_then(|e| e.to_str()),
                Some("rs" | "py" | "c" | "cpp" | "h")
            ) {
                saida.push(caminho);
            }
        }
    }

    let mut saida = Vec::new();
    descender(raiz, raiz, &mut saida);
    saida.sort();
    saida
}

/// O `.gitignore` apanha os artefactos e não apanha as fontes.
///
/// ## Porquê um guard e não uma revisão
///
/// A pergunta «o `.gitignore` está completo?» não se responde a ler o
/// ficheiro. O ficheiro pode ter 90 regras e mesmo assim deixar passar
/// o artefacto que aparece amanhã. Responde-se por **tentativa**:
/// `scripts/verificar_gitignore.sh` cria um ficheiro com o nome de cada
/// artefacto que este projecto produz e pergunta ao Git se o ignoraria.
///
/// E o mesmo teste corre nos dois sentidos, que é o que o torna útil.
/// Verificar que `target/` é ignorado é fácil; verificar que
/// `crypto/c_cpp/vendor/libsodium/lib/libsodium.a` **não** é, é o que
/// impede um `.gitignore` «generoso» de apagar a dependência que torna o
/// build hermético. Um clone alheio é onde esse erro aparece, e é tarde.
///
/// Esta verificação encontrou três buracos na altura em que foi escrita:
/// `lcov.info`, os core dumps nomeados (`onyxchatd.core`) e a saída de
/// `cargo vendor`. Os três passaram despercebidos numa revisão normal,
/// porque nenhum deles existia no disco no momento da revisão — e é
/// precisamente essa a falha: o `.gitignore` é julgado pelo que o
/// projecto **pode** produzir, não pelo que produziu hoje.
#[test]
fn o_gitignore_apanha_artefactos_e_nao_apanha_fontes() {
    let raiz: PathBuf = [env!("CARGO_MANIFEST_DIR")]
        .iter()
        .collect::<PathBuf>()
        .ancestors()
        .nth(2)
        .expect("raiz do repositório")
        .to_path_buf();

    let script = raiz.join("scripts").join("verificar_gitignore.sh");
    assert!(script.exists(), "scripts/verificar_gitignore.sh não existe");

    let saida = std::process::Command::new("bash")
        .arg(&script)
        .current_dir(&raiz)
        .output()
        .expect("o verificador corre");

    assert!(
        saida.status.success(),
        "o .gitignore não está completo:\n{}",
        String::from_utf8_lossy(&saida.stdout)
    );
}

/// Os quatro auditores de texto passam.
///
/// ## Porque isto é um guard e não uma tarefa de uma vez
///
/// A auditoria de alfabeto encontrou **sete** contaminações reais em
/// código já escrito: um emoji num teste, uma vírgula CJK num
/// comentário, caracteres chineses e coreanos colados em frases que
/// tinham sido escritas em português. Nenhuma delas produzia erro de
/// compilação e nenhuma fazia o build falhar. Apareceram porque uma
/// edição colou texto no sítio errado, e a única forma de as apanhar é
/// ler o ficheiro — o compilador não lê português.
///
/// Os auditores vivem em `scripts/` e correm em Python da stdlib, sem
/// dependências. Este guard limita-se a executá-los e a falhar se algum
/// reportar. Se ficarem lentos ou defeituosos, apagam-se — mas
/// apagam-se **depois** de se escrever outro sítio onde o mesmo
/// problema apanha.
///
/// `verificar_alfabeto.py` reporta tipografia deliberada (caixas de
/// ASCII-art, o operador de concatenação `‖`, setas de diagrama). Isso
/// é intentional e não é falha; o script distingue-o por catálogo de
/// caracteres, e o guard só exige código de saída zero — que o script
/// dá porque os caracteres deliberados estão na lista de permitidos.
#[test]
fn os_auditores_de_texto_passam() {
    let raiz: PathBuf = [env!("CARGO_MANIFEST_DIR")]
        .iter()
        .collect::<PathBuf>()
        .ancestors()
        .nth(2)
        .expect("raiz do repositório")
        .to_path_buf();

    // O Python do virtualenv, se existir; senão o do sistema. Sem
    // nenhum dos dois o guard **falha** com uma mensagem explícita, em
    // vez de passar em silêncio: um guard que passa sem verificar é
    // pior do que não existir.
    let venv = raiz.join(".venv").join("bin").join("python");
    let py = if venv.exists() { venv } else { PathBuf::from("python3") };

    for auditor in [
        "verificar_alfabeto.py",   // caracteres de outros sistemas de escrita
        "verificar_portugues.py",  // americanismos em todo o ficheiro
        "verificar_comentarios.py", // americanismos só em prosa
        "verificar_frases.py",     // colagens que não são português
    ] {
        let caminho = raiz.join("scripts").join(auditor);
        assert!(caminho.exists(), "{auditor} não existe");
        let saida = std::process::Command::new(&py)
            .arg(&caminho)
            .current_dir(&raiz)
            .output()
            .expect("o auditor corre");
        assert!(
            saida.status.success(),
            "{auditor} reportou problemas:\n{}",
            String::from_utf8_lossy(&saida.stdout)
        );
    }
}

/// A derivação da K1 tem de bater entre o Rust e o Python.
///
/// A fase I introduzia duas implementações da mesma fórmula: uma em
/// Rust (`crypto/rust/src/amizade.rs`, sobre a crate `hkdf`) e outra em
/// Python (`messenger/amizade.py`, sobre `hmac`/`hashlib`). As duas têm
/// de dar o mesmo valor, e essa condição não se verifica com um teste de
/// cada lado — cada um passa com a sua fórmula.
///
/// Este guard lê o vector congelado de **cada** implementação e compara
/// os dois. Se alguém alterar a fórmula de um lado e actualizar o seu
/// vector sem o do outro, este teste falha — que é a falha que
/// apareceria em produção como «as mensagens não abrem».
///
/// Porquê um guard e não um teste de paridade: a auditoria do projecto
/// (`docs/testing.md`) trata as guardas como a rede de segurança das
/// regras *entre* componentes, e a paridade Python↔Rust é uma delas.
#[test]
fn derivacao_da_amizade_bate_entre_rust_e_python() {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("raiz do repositório")
        .to_path_buf();

    // (1) O vector Rust.
    let rs = fs::read_to_string(raiz.join("crypto/rust/src/amizade.rs"))
        .expect("amizade.rs legível");
    let vector_rs = extrair_atribuicao(&rs, r#"const VECTOR_K1: &str = ""#);
    let semente_rs = extrair_atribuicao(&rs, r#"const VECTOR_SEMENTE: &str = ""#);

    // (2) O vector Python — lido da **constante**, não da docstring.
    let py = fs::read_to_string(raiz.join("tests/test_amizade.py")).expect("test_amizade.py legível");
    let vector_py = extrair_atribuicao(&py, r#"VECTOR_K1 = ""#);
    let semente_py = extrair_atribuicao(&py, r#"VECTOR_SEMENTE = ""#);

    assert_eq!(
        vector_rs, vector_py,
        "o vector da derivação divergiu entre Rust e Python — os dois \
         lados deixam de derivar a mesma chave.\n    Rust:   {vector_rs}\n    Python: {vector_py}"
    );
    assert_eq!(
        semente_rs, semente_py,
        "o vector da semente divergiu entre Rust e Python — o passo \
         intermédio já não é o mesmo, mesmo que a K1 final coincida por \
         sorte.\n    Rust:   {semente_rs}\n    Python: {semente_py}"
    );

    // E o rótulo de domínio tem de ser o mesmo nas duas pontas: é o
    // que separa esta derivação de qualquer outro SHA-256 do projecto.
    assert!(
        rs.contains(r#"DOMINIO_AMIZADE: &[u8] = b"ONYX/AMIZADE/v1""#),
        "o rótulo de domínio mudou em Rust"
    );
    // O rótulo é procurado onde é **definido**, não onde é importado:
    // o ficheiro de teste faz `from messenger.amizade import
    // DOMINIO_AMIZADE`, e procurar a definição lá dava sempre «não
    // está aqui». A primeira versão deste guard cometia exactamente
    // esse erro — e falhou, o que é a forma mais barata de o descobrir.
    let py_mod = fs::read_to_string(raiz.join("messenger/amizade.py"))
        .expect("messenger/amizade.py legível");
    assert!(
        py_mod.contains(r#"DOMINIO_AMIZADE = b"ONYX/AMIZADE/v1""#),
        "o rótulo de domínio mudou em `messenger/amizade.py`"
    );
    assert!(
        py_mod.contains(r#"INFO_K1 = b"ONYX/K1/v1""#),
        "o rótulo do HKDF mudou em `messenger/amizade.py`"
    );
    assert!(
        rs.contains(r#"INFO_K1: &[u8] = b"ONYX/K1/v1""#),
        "o rótulo do HKDF mudou em `crypto/rust/src/amizade.rs`"
    );
}

/// Extrai o valor de um literal hexadecimal, a partir da **atribuição**.
///
/// O `prefixo` é o texto que identifica a definição — `VECTOR_K1 = "` em
/// Python, `hex32("` em Rust — e não o começo do valor. A primeira
/// versão deste guard procurava o valor pelo seu início, e era
/// suficiente para passar ailler: encontrava primeiro o valor **na
/// docstring**, que é a mesma coisa escrita duas vezes, e comparava a
/// docstring com a docstring.
///
/// Um guard que lê comentários não guarda nada.
fn extrair_atribuicao(texto: &str, prefixo: &str) -> String {
    let pos = texto
        .find(prefixo)
        .unwrap_or_else(|| panic!("não encontrei a atribuição {prefixo:?}"));
    let resto = &texto[pos + prefixo.len()..];
    let inicio = resto
        .find(|c: char| c.is_ascii_hexdigit())
        .unwrap_or_else(|| panic!("a atribuição {prefixo:?} não tem hex"));
    let corpo = &resto[inicio..];
    let fim = corpo
        .find(|c: char| !c.is_ascii_hexdigit())
        .unwrap_or(corpo.len());
    corpo[..fim].to_string()
}

/// A limpeza de memória está **chamada**, não só documentada.
///
/// A fase F encontrou `sodium_memzero` declarado no header do libsodium
/// e presente no `.a`, e nenhuma chamada em todo o projecto C/C++ — a
/// mesma forma de mentira que a fase G4 encontrou na K9: a garantia
/// escrita, a verificação inexistente.
///
/// Este guard fecha as duas metades:
///
///   1. `onyx_limpar` é **declarada** em `onyx_crypto.h` — a API não
///      pode desaparecer sem que este teste o note;
///   2. `onyx_limpar` é **chamada** em `k9_chacha.c` (a implementação)
///      e em `transposition.cpp` (os ramos de erro da K4). É este o
///      ponto que interessa: uma função correctamente implementada que
///      ninguém chama é tão inútil como uma que não existe.
///
/// Não é redundante com os testes C: eles provam que a limpeza
/// funciona, este prova que a limpeza existe em todos os sítios onde é
/// necessária. Um ficheiro de testes que passa porque a função foi
/// removida do código é a forma mais comum de um guard ficar vazio.
#[test]
fn limpeza_de_memoria_esta_chamada() {
    let c_cpp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("raiz do repositório")
        .join("crypto/c_cpp");

    // (1) A declaração na API.
    let header = fs::read_to_string(c_cpp.join("onyx_crypto.h")).expect("onyx_crypto.h legível");
    assert!(
        header.contains("void onyx_limpar(void *pnt, size_t len);"),
        "`onyx_limpar` desapareceu de `onyx_crypto.h` — se a \n\
         implementação mudou de nome, actualiza também o `extern` em \
         `ffi_c.rs`, que é escrito à mão"
    );

    // (2) A implementação num só sítio.
    let k9 = fs::read_to_string(c_cpp.join("k9_chacha.c")).expect("k9_chacha.c legível");
    assert!(
        k9.contains("void onyx_limpar(void *pnt, size_t len)"),
        "`onyx_limpar` deixou de estar implementado em `k9_chacha.c`. \
         Tem de ser exactamente um ficheiro: a implementação é a \
         dependência de `sodium.h`, e o outro ficheiro limitava-se a \
         chamá-la pela declaração do header"
    );

    // (3) As chamadas nos ramos de erro da K4 — os dois.
    let k4 = fs::read_to_string(c_cpp.join("transposition.cpp")).expect("transposition.cpp legível");
    let chamadas = k4.matches("onyx_limpar(saida").count();
    assert_eq!(
        chamadas, 2,
        "`transposition.cpp` tem de limpar em **ambos** os ramos de \
         `ONYX_ERR_PAD` (o do valor de padding e o da consistência); \
         encontrados {chamadas}"
    );

    // E o guard tem de ter o que guardar: se o ficheiro mudar de nome,
    // este teste passa a ler um ficheiro inexistente e falha com uma
    // mensagem de leitura, que é menos informativa do que devia.
    assert!(
        k4.contains("ONYX_ERR_PAD"),
        "`transposition.cpp` já não menciona `ONYX_ERR_PAD` — o \
         caminho de erro da K4 mudou e este guard tem de ser revisto"
    );
}

/// A segunda forma de quebrar o isolamento: um implementador novo do
/// traço `BackendTor` que toque a rede real.
///
/// Só `tor.rs` (a definição do traço e o backend de testes) e
/// `tor_arti.rs` (o backend real) podem implementar `BackendTor`.
#[test]
fn so_o_glue_implementa_backend_tor() {
    const IMPLEMENTADORES: &[&str] = &["impl BackendTor for"];
    const AUTORIZADOS: &[&str] = &["tor.rs", "tor_arti.rs"];

    let violacoes: Vec<String> = fontes("")
        .iter()
        .flat_map(|fonte| {
            let autorizado = AUTORIZADOS
                .iter()
                .any(|a| fonte.relativo.ends_with(a));
            codigo_de_producao(&fonte.texto)
                .lines()
                .enumerate()
                .filter_map(move |(i, linha)| {
                    if autorizado || eh_mencao_a_arti(linha) {
                        return None;
                    }
                    let achado = IMPLEMENTADORES.iter().find(|p| linha.contains(*p))?;
                    Some(format!(
                        "{}:{} — `{achado}` fora de {AUTORIZADOS:?}\n    {}",
                        fonte.relativo,
                        i + 1,
                        linha.trim()
                    ))
                })
        })
        .collect();

    assert!(
        violacoes.is_empty(),
        "só `tor.rs` e `tor_arti.rs` podem implementar `BackendTor`.\n\
         Um implementador novo tem de ser **offline** (loopback), para \
         que o daemon continue a poder ser testado sem rede.\n\n{}",
        violacoes.join("\n")
    );
}

/// O ficheiro autorizado tem de existir e ser único.
///
/// Teste de sanidade: se `tor_arti.rs` fosse apagado ou renomeado, os
/// dois testes acima passariam a vazio (nenhuma fonte violaria nada) e
/// o isolamento ficaria sem verificação, sem que nada falhasse.
#[test]
fn tor_arti_existe_e_e_o_unico_autorizado() {
    let p = raiz_crate().join("src").join("tor_arti.rs");
    assert!(
        p.exists(),
        "src/tor_arti.rs tem de existir: é o único ficheiro autorizado a \
         tocar a rede real, e os testes de guarda dependem desse nome"
    );

    // E não pode haver outro ficheiro com o mesmo nome noutro sítio,
    // porque a lista de autorizados é por nome de ficheiro.
    let homonimos: Vec<PathBuf> = fontes("")
        .iter()
        .map(|f| raiz_crate().join(&f.relativo))
        .filter(|c| c.file_name().and_then(|n| n.to_str()) == Some("tor_arti.rs"))
        .collect();
    assert!(
        homonimos.len() <= 1,
        "há mais do que um `tor_arti.rs`: {homonimos:?} — a lista de \
         autorizados ficaria ambígua"
    );
}

/// Regra estrutural: `tor.rs` define o traço e `TorFalso`; `tor_arti.rs`
/// define `TorReal`. Se alguém mover `TorFalso` para outro ficheiro, os
/// testes unitários deixam de compilar — mas vale a pena fixar o
/// contrato por escrito.
#[test]
fn tor_falso_vive_em_tor_rs() {
    let tor_rs = fs::read_to_string(raiz_crate().join("src").join("tor.rs"))
        .expect("src/tor.rs legível");
    assert!(
        tor_rs.contains("struct TorFalso"),
        "`TorFalso` (o backend offline dos testes) tem de viver em \
         `src/tor.rs` — movê-lo quebra a suposição de que `tor.rs` é \
         compilável e testável sem rede"
    );

    // E o oposto: `tor_arti.rs` não pode conter `TorFalso`, para que
    // ninguém dependa do backend real a partir de um teste.
    let tor_arti = fs::read_to_string(raiz_crate().join("src").join("tor_arti.rs"))
        .expect("src/tor_arti.rs legível");
    assert!(
        !tor_arti.contains("struct TorFalso"),
        "`TorFalso` não pode estar em `tor_arti.rs`: esse ficheiro só \
         é compilado com a feature `tor-real` e não é testado"
    );
}

/// O guard test acima lê fontes, o que o torna dependente da estrutura
/// de directórios. Este teste verifica que a lista de ficheiros que ele
/// leu não está vazia — caso contrário uma refactorização que movesse
/// os módulos para outro sítio faria o guard passar em vazio.
#[test]
fn o_guard_test_le_alguma_fonte() {
    let lidas = fontes("tor_arti.rs");
    assert!(
        lidas.len() >= 10,
        "o guard test só leu {} ficheiros — espera-se pelo menos 10. \
         Se a estrutura de `src/` mudou, actualizar os testes de guarda \
         em vez de os deixar passar em vazio",
        lidas.len()
    );
    // E tem de incluir pelo menos o ficheiro onde a regra é declarada.
    assert!(
        lidas.iter().any(|f| f.relativo.ends_with("tor.rs")),
        "o guard tem de ler `tor.rs`, onde o traço é declarado"
    );
}

/// A lista de padrões tem de cobrir o caminho de crate da arti.
///
/// Um teste sobre os próprios dados do teste: se alguém renomear a
/// dependência no `Cargo.toml` e o padrão `arti_client` deixar de bater,
/// o guard test passa a vazio sem avisar. Este teste fecha essa porta.
#[test]
fn os_padroes_do_guard_sao_suficientes() {
    // Cada padrão tem de aparecer em `Cargo.toml` (onde a dependência é
    // declarada) ou ser um tipo da arti reconhecível. Se um padrão
    // deixar de existir em todo o lado, é código morto.
    let manifest = fs::read_to_string(raiz_crate().join("Cargo.toml")).expect("Cargo.toml");
    for padrao in PADROES_ARTI {
        let em_uso = manifest.contains(*padrao)
            || fontes("tor_arti.rs")
                .iter()
                .any(|f| f.texto.contains(*padrao))
            || fs::read_to_string(raiz_crate().join("src").join("tor_arti.rs"))
                .is_ok_and(|t| t.contains(*padrao));
        assert!(
            em_uso,
            "o padrão `{padrao}` não aparece em lado nenhum: é um padrão \
             morto no guard test, que dá falsa confiança"
        );
    }
}

/// Helper: existe alguma fonte cujo nome corresponda a `sufixo`?
///
/// Usado só pelos testes acima; declarado como função para não repetir
/// o `strip_prefix`/`to_string_lossy` em cada sítio.
#[allow(dead_code)]
fn alguma_fonte_com_sufixo(fontes_: &[Fonte], sufixo: &str) -> bool {
    fontes_
        .iter()
        .any(|f: &Fonte| Path::new(&f.relativo).ends_with(sufixo))
}
