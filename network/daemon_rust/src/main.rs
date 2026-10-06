// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// main.rs — arranque do daemon OnyxChat (onyxchatd)
// ---------------------------------------------------------------------
// Interpreta a linha de comando (`--version`, `--help`, `--socket`,
// `--tor`) e entra no ciclo IPC síncrono de `ipc::servir`. Toda a lógica
// vive em `interpretar`/`executar` (funções puras/testáveis); `main`
// apenas delega para a biblioteca `onyxchatd`.
// =====================================================================

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use onyxchatd::ipc;
use onyxchatd::tor::{BackendTor, TorFalso};
// Só existe o backend real com a feature `tor-real` (ligada por omissão
// no binário; desligável apenas em builds `--no-default-features`, como
// o do crate de fuzzing, que nem sequer compila este alvo).
#[cfg(feature = "tor-real")]
use onyxchatd::tor_arti::TorReal;

/// Versão do daemon (alinhada com o workspace).
const VERSAO: &str = env!("CARGO_PKG_VERSION");

/// Texto de ajuda mostrado por `--help`/`-h`.
const AJUDA: &str = concat!(
    "onyxchatd — daemon OnyxChat (pipeline K1→K9 + IPC + Tor)\n",
    "  --version, -v      imprime a versão\n",
    "  --help, -h         imprime esta ajuda\n",
    "  --socket <caminho> sobrepõe o caminho do socket IPC\n",
    "  --tor <arti|nenhum> escolhe o backend Tor (omissão: arti;\n",
    "                     também lido de $ONYXCHAT_TOR)\n",
    "  (sem argumentos)   serve em $ONYXCHAT_SOCKET ou /tmp/onyxchat-{uid}.sock"
);

/// Backend Tor escolhido para esta execução.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscolhaTor {
    /// arti-client embutido (hidden services reais).
    Arti,
    /// `TorFalso` — loopback puro, sem rede (testes e modo offline).
    Nenhum,
}

/// Backend Tor correspondente à escolha.
///
/// `Arti` cria o cliente arti (sem bootstrap); falhas de sistema de
/// ficheiros/configuração são rejeitadas aqui, antes de servir.
///
/// Sem a feature `tor-real` o ramo `Arti` devolve erro em vez de
/// construir: um binário compilado sem a feature continua a arrancar e a
/// servir com `--tor nenhum`, mas nunca fingiria ter rede real quando não
/// a tem (falhar alto é melhor que degradar em silêncio).
///
/// A mensagem de erro é deliberadamente accionável: quem encontra isto
/// precisa de saber o comando exacto a correr, não apenas que algo
/// falta.
fn construir_backend(escolha: EscolhaTor) -> Result<Box<dyn BackendTor>, String> {
    match escolha {
        EscolhaTor::Nenhum => Ok(Box::new(TorFalso::novo())),
        #[cfg(feature = "tor-real")]
        EscolhaTor::Arti => Ok(Box::new(TorReal::novo().map_err(|erro| erro.to_string())?)),
        #[cfg(not(feature = "tor-real"))]
        EscolhaTor::Arti => Err(
            "backend arti indisponível: este binário foi compilado sem a \
             feature `tor-real`, logo não tem Tor. Escolha uma de:\n  \
               · modo offline:  onyxchatd --tor nenhum\n  \
               · com Tor:       cargo build --release --features tor-real\n\
             Ver docs/DEV_GUIDE.md §2.3."
                .to_string(),
        ),
    }
}

/// Resolve o backend a partir de `--tor` ou de `$ONYXCHAT_TOR`.
///
/// `valor` é o argumento de linha de comando (se houver); caso contrário
/// usa-se `ambiente` (o valor da variável de ambiente, se definida);
/// por omissão → `arti`.
fn escolher_tor<'a>(
    valor: Option<&'a str>,
    ambiente: Option<&'a str>,
) -> Result<EscolhaTor, String> {
    match valor.or(ambiente).unwrap_or("arti") {
        "arti" => Ok(EscolhaTor::Arti),
        "nenhum" => Ok(EscolhaTor::Nenhum),
        outro => Err(format!(
            "backend Tor desconhecido: {outro} (esperado arti ou nenhum)"
        )),
    }
}

/// Ação decidida a partir dos argumentos de linha de comando.
#[derive(Debug, PartialEq, Eq)]
enum Acao {
    /// Mostrar uma mensagem e sair com sucesso (`--version`/`--help`).
    Imprimir(String),
    /// Entrar no ciclo de escuta IPC no caminho indicado.
    Servir {
        /// Caminho do socket IPC.
        caminho: PathBuf,
        /// Backend Tor a usar durante o serviço.
        tor: EscolhaTor,
    },
}

/// Interpreta os argumentos (sem o próprio nome do programa).
///
/// `ambiente` é o valor de `$ONYXCHAT_TOR` (ausente → omissão `arti`).
///
/// `--version`/`--help` valem apenas como primeiro argumento; os restantes
/// são opções com valor e qualquer argumento solto é rejeitado.
///
/// `Err(msg)` = opção inválida (mensagem pronta para stderr).
fn interpretar(args: &[String], ambiente: Option<&str>) -> Result<Acao, String> {
    let mut caminho: Option<PathBuf> = None;
    let mut tor: Option<&str> = None;
    let mut iter = args.iter();
    // Valor já lido pela opção anterior e ainda por processar.
    let mut em_espera: Option<&String> = None;
    let mut indice = 0usize;
    while let Some(opcao) = em_espera.take().or_else(|| iter.next()) {
        match opcao.as_str() {
            "--version" | "-v" if indice == 0 => {
                return Ok(Acao::Imprimir(format!("onyxchatd {VERSAO}")))
            }
            "--help" | "-h" if indice == 0 => return Ok(Acao::Imprimir(AJUDA.to_string())),
            "--socket" => {
                if caminho.is_some() {
                    return Err("--socket aceita exatamente um caminho".into());
                }
                let valor = iter.next().ok_or("--socket exige um caminho")?;
                match iter.next() {
                    // Próximo token é opção → processa-se na volta seguinte.
                    Some(proximo) if proximo.starts_with("--") => {
                        caminho = Some(PathBuf::from(valor));
                        em_espera = Some(proximo);
                    }
                    // Argumento solto depois do caminho → caminho a mais.
                    Some(_) => return Err("--socket aceita exatamente um caminho".into()),
                    None => caminho = Some(PathBuf::from(valor)),
                }
            }
            "--tor" => {
                if tor.is_some() {
                    return Err("--tor aceita exatamente um backend".into());
                }
                tor = Some(iter.next().ok_or("--tor exige arti ou nenhum")?.as_str());
            }
            outro => return Err(format!("opção desconhecida: {outro}")),
        }
        indice += 1;
    }
    Ok(Acao::Servir {
        caminho: caminho.unwrap_or_else(ipc::caminho_socket),
        tor: escolher_tor(tor, ambiente)?,
    })
}

/// Escreve em `stderr` o aviso de páginas não trancadas, se for o caso.
///
/// Devolve `true` se tiver avisado. Existe como função separada do
/// arranque porque o caminho de falha do `mlockall` **não** pode ser
///provocado com fiabilidade numa máquina com `RLIMIT_MEMLOCK`
/// suficiente — e mesmo assim tem de estar testado, porque é
/// exactamente o caminho que alguém com pouca RAM vai encontrar.
///
/// Separar também evita que uma função de 30 linhas no arranque fique
/// com uma cadeia de `format!` impossivel de cobrir.
fn avisar_paginas_desbloqueadas(
    paginas: crypto_core::EstadoPaginas,
    limite: Option<u64>,
) -> bool {
    let crypto_core::EstadoPaginas::Desbloqueadas(motivo) = paginas else {
        return false;
    };
    // O limite é útil mesmo quando a falha não é de limite: um número
    // concreto é mais accionável do que "insuficiente".
    let detalhe = match limite {
        Some(bytes) => format!(" (RLIMIT_MEMLOCK = {bytes} bytes)"),
        None => String::new(),
    };
    eprintln!(
        "onyxchatd: aviso: {paginas}{detalhe}\n\
         onyxchatd: o material sensível pode ser descartado para swap, \
         onde a limpeza em memória não o alcança.\n\
         onyxchatd: motivo: {motivo}\n\
         onyxchatd: para corrigir: ulimit -l unlimited  \
         (ver docs/key_management.md §Exposição a swap)"
    );
    true
}

/// Executa a ação interpretada (com paragem opcional para testes).
///
/// Em produção `paragem` é `None` e o ciclo IPC é infinito; os testes
/// passam um sinalizador para exercitar o caminho de sucesso de saída.
///
/// O `Estado` (rede P2P + amizades) é criado aqui e partilhado por
/// todas as ligações do socket — a sessão sobrevive a clientes que
/// fecham e ao próprio `RECEBER` bloqueante.
fn executar(acao: Acao, paragem: Option<Arc<AtomicBool>>) -> ExitCode {
    match acao {
        Acao::Imprimir(mensagem) => {
            println!("{mensagem}");
            ExitCode::SUCCESS
        }
        Acao::Servir { caminho, tor } => {
            // Protecção de memória ANTES de qualquer material sensível
            // existir: o tranco tem de preceder a criação do backend e
            // do estado, para que páginas futuras também fiquem
            // protegidas (`MCL_FUTURE`).
            //
            // Degradação graciosa: avisar e continuar. Perder a
            // protecção contra swap é grave; não arrancar é pior.
            // Ver `docs/threat_model.md` §A10.
            let paginas = crypto_core::paginas::trancar_paginas();
            avisar_paginas_desbloqueadas(paginas, crypto_core::paginas::limite_memlock());

            let backend = match construir_backend(tor) {
                Ok(backend) => backend,
                Err(erro) => {
                    eprintln!("onyxchatd: backend Tor: {erro}");
                    crypto_core::paginas::destrancar_paginas();
                    return ExitCode::FAILURE;
                }
            };
            let estado = Arc::new(ipc::Estado::novo(backend));
            let saida = match ipc::servir_com_paragem(&caminho, paragem, estado) {
                Ok(()) => ExitCode::SUCCESS,
                Err(erro) => {
                    eprintln!("onyxchatd: falha no IPC: {erro}");
                    ExitCode::FAILURE
                }
            };
            // O tranco só é libertado depois de o estado — que tem as
            // chaves em memória — ter sido descartado.
            crypto_core::paginas::destrancar_paginas();
            saida
        }
    }
}

/// Ponto de entrada: delega para `executar` (código coberto por testes).
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ambiente = std::env::var("ONYXCHAT_TOR").ok();
    executar(
        match interpretar(&args, ambiente.as_deref()) {
            Ok(acao) => acao,
            Err(mensagem) => {
                eprintln!("onyxchatd: {mensagem}");
                return ExitCode::FAILURE;
            }
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use onyxchatd::tor::EstadoTor;

    /// `ONYXCHAT_ARTI_DIR` é variável global do processo: os testes que a
    /// escrevem são serializados entre si para nunca se misturarem
    /// (o `catch` do envenenamento preserva o teste seguinte).
    ///
    /// Só existe **com** a feature `tor-real`: sem ela não há nenhum
    /// teste a escrever a variável, e um helper sem chamador seria
    /// código morto que contaria contra a própria cobertura.
    #[cfg(feature = "tor-real")]
    fn trava_ambiente_arti() -> std::sync::MutexGuard<'static, ()> {
        static TRAVA: std::sync::Mutex<()> = std::sync::Mutex::new(());
        destravar(TRAVA.lock())
    }

    /// Devolve o valor dentro de uma trava envenenada.
    ///
    /// O envenenamento nunca aborta o teste seguinte: o guard é
    /// recuperado do `PoisonError` e segue-se com ele.
    fn destravar<T>(resultado: std::sync::LockResult<T>) -> T {
        resultado.unwrap_or_else(|envenenada| envenenada.into_inner())
    }

    /// Raiz temporária dos testes com estado da arti.
    ///
    /// NÃO se usa `/tmp`: o `fs_mistrust` da arti exige que a árvore do
    /// diretório de estado seja propriedade do utilizador e não-gravável
    /// por outros, e `/tmp` é 1777 — daí o `$HOME/.cache` (700/750).
    #[cfg(feature = "tor-real")]
    fn raiz_teste(nome: &str) -> PathBuf {
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let raiz = base
            .join(".cache")
            .join(format!("{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        std::fs::create_dir_all(&raiz).expect("raiz de teste");
        raiz
    }

    /// Converte uma lista de argumentos literais em `Vec<String>`.
    fn args(literais: &[&str]) -> Vec<String> {
        literais.iter().map(|s| s.to_string()).collect()
    }

    /// Desempacota uma ação `Servir` (caminho, backend Tor).
    ///
    /// `Imprimir` (`--help`/`--version`) não serve nada → `None`.
    fn servir(acao: &Acao) -> Option<(PathBuf, EscolhaTor)> {
        match acao {
            Acao::Servir { caminho, tor } => Some((caminho.clone(), *tor)),
            Acao::Imprimir(_) => None,
        }
    }

    /// `--version`/`-v` → mensagem de versão; `--help`/`-h` → ajuda.
    ///
    /// Comparações de igualdade total (sem ramo alternativo morto):
    /// `Acao` deriva `PartialEq`, o que torna a asserção completa.
    #[test]
    fn interpretar_version_e_help() {
        for flag in ["--version", "-v"] {
            assert_eq!(
                interpretar(&args(&[flag]), None),
                Ok(Acao::Imprimir(format!("onyxchatd {VERSAO}")))
            );
        }
        for flag in ["--help", "-h"] {
            assert_eq!(
                interpretar(&args(&[flag]), None),
                Ok(Acao::Imprimir(AJUDA.to_string()))
            );
            // `--help` não serve nada → `servir` devolve `None`.
            let ajuda = interpretar(&args(&[flag]), None).expect("help");
            assert!(servir(&ajuda).is_none(), "--help nunca serve");
        }
    }

    /// Sem argumentos → socket por omissão e backend `arti` (produção).
    #[test]
    fn interpretar_sem_argumentos() {
        assert_eq!(
            interpretar(&[], None),
            Ok(Acao::Servir {
                caminho: ipc::caminho_socket(),
                tor: EscolhaTor::Arti,
            })
        );
    }

    /// `--socket <caminho>` → caminho livre do teste (backend por omissão).
    #[test]
    fn interpretar_socket_override() {
        assert_eq!(
            interpretar(&args(&["--socket", "/tmp/x.sock"]), None),
            Ok(Acao::Servir {
                caminho: PathBuf::from("/tmp/x.sock"),
                tor: EscolhaTor::Arti,
            })
        );
    }

    /// `--tor` vence `$ONYXCHAT_TOR`; sem `--tor` usa-se o ambiente.
    #[test]
    fn interpretar_tor_arg_e_ambiente() {
        let (caminho, tor) =
            servir(&interpretar(&args(&["--tor", "nenhum"]), Some("arti")).expect("--tor nenhum"))
                .expect("servir");
        assert_eq!(caminho, ipc::caminho_socket());
        assert_eq!(tor, EscolhaTor::Nenhum);

        let (_, tor) =
            servir(&interpretar(&[], Some("nenhum")).expect("ambiente")).expect("servir");
        assert_eq!(tor, EscolhaTor::Nenhum);

        let (_, tor) =
            servir(&interpretar(&[], Some("arti")).expect("ambiente arti")).expect("servir");
        assert_eq!(tor, EscolhaTor::Arti);
    }

    /// `--socket <caminho> --tor <v>`: o valor lido fica "em espera" e
    /// é processado na volta seguinte do ciclo (não é caminho a mais).
    #[test]
    fn interpretar_opcao_depois_do_socket() {
        assert_eq!(
            interpretar(&args(&["--socket", "/tmp/y.sock", "--tor", "nenhum"]), None),
            Ok(Acao::Servir {
                caminho: PathBuf::from("/tmp/y.sock"),
                tor: EscolhaTor::Nenhum,
            })
        );
    }

    /// Erros de linha de comando: sem caminho, caminho a mais, opção
    /// desconhecida, `--tor` incompleto/desconhecido — todos legíveis.
    #[test]
    fn interpretar_erros() {
        let casos: &[(&[&str], &str)] = &[
            (&["--socket"], "exige um caminho"),
            (&["--socket", "/a", "/b"], "exatamente um"),
            (&["--socket", "/a", "--socket", "/b"], "exatamente um"),
            (&["--tor"], "exige arti ou nenhum"),
            (&["--tor", "arti", "--tor", "nenhum"], "exatamente um"),
            (&["--trotes", "x"], "opção desconhecida: --trotes"),
            (&["--tor", "tor-real"], "desconhecido"),
        ];
        for (literais, esperado) in casos {
            let erro = interpretar(&args(literais), None).expect_err("tem de falhar");
            assert!(erro.contains(esperado), "{erro} ∌ {esperado}");
        }
    }

    /// `escolher_tor`: omissão `arti`, ambiente por omissão e erro de
    /// valor desconhecido (mensagem a indicar as opções válidas).
    #[test]
    fn escolher_tor_semeadura_e_ambiente() {
        assert_eq!(escolher_tor(None, None), Ok(EscolhaTor::Arti));
        assert_eq!(
            escolher_tor(Some("nenhum"), Some("arti")),
            Ok(EscolhaTor::Nenhum)
        );
        assert_eq!(escolher_tor(Some("arti"), None), Ok(EscolhaTor::Arti));
        let erro = escolher_tor(None, Some("banana")).expect_err("inválido");
        assert!(erro.contains("arti") && erro.contains("nenhum"), "{erro}");
    }

    /// Os dois backends constróem: `nenhum` dá `TorFalso`, `arti` cria
    /// o cliente real (estado num diretório temporário, sem bootstrap).
    ///
    /// Só corre **com** a feature `tor-real`: sem ela o binário não tem
    /// Tor de todo, e o caminho a testar é o `Err` (ver
    /// `construir_backend_sem_tor_falha_explicitamente`).
    #[cfg(feature = "tor-real")]
    #[test]
    fn construir_backend_arti_e_nenhum() {
        let _trava = trava_ambiente_arti();
        let dir = raiz_teste("onyx-arti");
        // Seguro em edition 2021; a trava acima serializa os testes.
        std::env::set_var("ONYXCHAT_ARTI_DIR", &dir);
        let backend = construir_backend(EscolhaTor::Arti).expect("arti constrói");
        assert_eq!(backend.estado(), EstadoTor::Parado);
        assert!(dir.join("state").is_dir() && dir.join("cache").is_dir());
        std::env::remove_var("ONYXCHAT_ARTI_DIR");
        drop(backend);
        let _ = std::fs::remove_dir_all(&dir);

        let backend = construir_backend(EscolhaTor::Nenhum).expect("nenhum constrói");
        assert_eq!(backend.estado(), EstadoTor::Parado);
    }

    /// `TorFalso` constrói em qualquer configuração de features — é o
    /// backend que os testes E2E usam.
    #[test]
    fn construir_backend_nenhum_sempre() {
        let backend = construir_backend(EscolhaTor::Nenhum).expect("nenhum constrói");
        assert_eq!(backend.estado(), EstadoTor::Parado);
    }

    /// **Propriedade de segurança: falha explícita, nunca degradação
    /// silenciosa.** Sem a feature `tor-real`, pedir o backend `arti`
    /// tem de devolver `Err` — nunca um backend silenciosamente sem
    /// rede.
    ///
    /// Este é o teste que garante que um binário compilado sem Tor não
    /// finge ter Tor. Corre na configuração **por omissão**, que é a
    /// que a CI usa.
    #[cfg(not(feature = "tor-real"))]
    #[test]
    fn construir_backend_sem_tor_falha_explicitamente() {
        let erro = construir_backend(EscolhaTor::Arti)
            .err()
            .expect("sem a feature tor-real, arti tem de falhar");
        // A mensagem tem de ser accionável: diz o que falta E o que
        // fazer. Um erro genérico obriga o utilizador a procurar.
        assert!(erro.contains("tor-real"), "tem de citar a feature: {erro}");
        assert!(
            erro.contains("--tor nenhum") || erro.contains("nenhum"),
            "tem de oferecer o modo offline: {erro}"
        );
        assert!(
            erro.contains("--features tor-real"),
            "tem de dar o comando de build: {erro}"
        );
    }

    /// E o mesmo, visto pelo `executar`: pedir `arti` sem a feature faz
    /// o daemon **não arrancar**, com código de saída de falha.
    #[cfg(not(feature = "tor-real"))]
    #[test]
    fn executar_com_arti_sem_tor_nao_arranca() {
        let dir = std::env::temp_dir().join(format!("onyx-main-semtor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let acao = Acao::Servir {
            caminho: dir.join("m.sock"),
            tor: EscolhaTor::Arti,
        };
        assert_eq!(
            executar(acao, Some(Arc::new(AtomicBool::new(true)))),
            ExitCode::FAILURE,
            "sem tor-real, pedir arti tem de abortar o arranque — \
             nunca servir em modo degradado"
        );
        // E não pode ter deixado socket para trás.
        assert!(
            !dir.join("m.sock").exists(),
            "um arranque falhado não pode deixar socket"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `executar(Imprimir, …)` devolve sucesso (stdout coberto pelo
    /// subprocesso de teste; aqui cobre-se o ramo da função).
    #[test]
    fn executar_imprimir_sucesso() {
        let acao = interpretar(&args(&["--version"]), None).expect("ok");
        assert_eq!(executar(acao, None), ExitCode::SUCCESS);
    }

    /// `executar(Servir, …)` com backend `nenhum` e paragem já ativa →
    /// sai com sucesso sem servir nenhum pedido (caminho `Ok(())`).
    #[test]
    fn executar_servir_paragem_imediata() {
        let dir = std::env::temp_dir().join(format!("onyx-main-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let socket = dir.join("m.sock");
        let paragem = Arc::new(AtomicBool::new(true));
        let acao = Acao::Servir {
            caminho: socket.clone(),
            tor: EscolhaTor::Nenhum,
        };
        assert_eq!(executar(acao, Some(paragem)), ExitCode::SUCCESS);
        // O socket foi criado antes de a paragem interromper o ciclo.
        assert!(socket.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `executar(Servir, …)` com backend `arti` e paragem já ativa →
    /// o cliente arti arranca offline e o ciclo IPC sai com sucesso.
    ///
    /// Só corre **com** a feature `tor-real`.
    #[cfg(feature = "tor-real")]
    #[test]
    fn executar_servir_com_backend_arti() {
        let _trava = trava_ambiente_arti();
        let dir = raiz_teste("onyx-main-arti");
        std::env::set_var("ONYXCHAT_ARTI_DIR", dir.join("arti"));
        let acao = Acao::Servir {
            caminho: dir.join("m.sock"),
            tor: EscolhaTor::Arti,
        };
        assert_eq!(
            executar(acao, Some(Arc::new(AtomicBool::new(true)))),
            ExitCode::SUCCESS
        );
        std::env::remove_var("ONYXCHAT_ARTI_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// O backend arti não consegue sequer criar o seu diretório de
    /// estado (`ONYXCHAT_ARTI_DIR` aponta para um ficheiro) →
    /// `executar` falha antes de abrir o socket.
    ///
    /// Só corre **com** a feature `tor-real`: sem ela, o `arti` falha
    /// antes por falta da própria biblioteca.
    #[cfg(feature = "tor-real")]
    #[test]
    fn executar_servir_erro_de_backend() {
        let _trava = trava_ambiente_arti();
        let dir = raiz_teste("onyx-main-backend");
        let bloqueio = dir.join("bloqueio");
        std::fs::write(&bloqueio, b"ficheiro, nao diretorio").expect("ficheiro");
        std::env::set_var("ONYXCHAT_ARTI_DIR", &bloqueio);
        let acao = Acao::Servir {
            caminho: dir.join("m.sock"),
            tor: EscolhaTor::Arti,
        };
        assert_eq!(
            executar(acao, Some(Arc::new(AtomicBool::new(true)))),
            ExitCode::FAILURE
        );
        std::env::remove_var("ONYXCHAT_ARTI_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `executar(Servir, …)` com caminho impossível → falha impressa
    /// e código de saída distinto de zero (caminho `Err` do bind).
    #[test]
    fn executar_servir_erro_de_bind() {
        let dir = std::env::temp_dir().join(format!("onyx-main-err-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        // Caminho = diretório: bind falha com IsADirectory.
        let acao = Acao::Servir {
            caminho: dir.clone(),
            tor: EscolhaTor::Nenhum,
        };
        assert_eq!(executar(acao, None), ExitCode::FAILURE);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Uma trava envenenada devolve o guard em vez de abortar.
    #[test]
    fn destravar_trava_envenenada() {
        let trava = std::sync::Mutex::new(0u8);
        let guard = trava.lock().expect("trava livre");
        let envenenada: std::sync::LockResult<std::sync::MutexGuard<'_, u8>> =
            Err(std::sync::PoisonError::new(guard));
        let mut guard = destravar(envenenada);
        *guard = 7;
        drop(guard);
        assert_eq!(*trava.lock().expect("volta a ficar utilizável"), 7);
    }

    // -----------------------------------------------------------------
    // Aviso de páginas não trancadas (F0c)
    // -----------------------------------------------------------------

    /// Páginas trancadas ⇒ **não** há aviso. O arranque tem de ser
    /// silencioso quando tudo está bem: um aviso que aparece sempre
    /// deixa de ser lido.
    #[test]
    fn sem_aviso_quando_as_paginas_estao_trancadas() {
        assert!(
            !avisar_paginas_desbloqueadas(crypto_core::EstadoPaginas::Trancadas, Some(64 * 1024)),
            "com páginas trancadas não há nada para avisar"
        );
    }

    /// Páginas desbloqueadas ⇒ aviso, com o motivo e o limite. Este é o
    /// caminho que alguém com pouca RAM encontra, e tem de ser
    /// accionável.
    #[test]
    fn aviso_quando_as_paginas_estao_desbloqueadas() {
        assert!(avisar_paginas_desbloqueadas(
            crypto_core::EstadoPaginas::Desbloqueadas(crypto_core::MotivoFalha::LimiteMemlock),
            Some(65_536),
        ));
        assert!(avisar_paginas_desbloqueadas(
            crypto_core::EstadoPaginas::Desbloqueadas(crypto_core::MotivoFalha::Outro(libc::EIO)),
            Some(65_536),
        ));
    }

    /// Sem `RLIMIT_MEMLOCK` legível (`getrlimit` falha), o aviso tem de
    /// sair na mesma — a ausência do limite não pode silenciar a
    /// protecção em falta.
    #[test]
    fn aviso_mesmo_sem_limite_conhecido() {
        assert!(
            avisar_paginas_desbloqueadas(
                crypto_core::EstadoPaginas::Desbloqueadas(crypto_core::MotivoFalha::LimiteMemlock),
                None,
            ),
            "desconhecer o limite não é razão para não avisar"
        );
    }
}
