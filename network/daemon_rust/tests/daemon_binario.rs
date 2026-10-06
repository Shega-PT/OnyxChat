// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// daemon_binario.rs — testes de subproc do binário `onyxchatd`
// ---------------------------------------------------------------------
// Exercitam o caminho real de `main()` (interpretação + arranque) com o
// binário compilado — cobrem as linhas de entrada que os testes unitários
// não alcançam (a função `main` não é chamada dentro do processo de
// teste) e validam os códigos de saída documentados.
// =====================================================================

use std::path::PathBuf;
use std::process::Command;

/// Constrói um comando do binário do daemon (caminho dado pelo Cargo).
fn onyxchatd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_onyxchatd"))
}

/// Caminho temporário único por teste (evita colisões entre testes).
fn dir_temporario(rotulo: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("onyx-bin-{rotulo}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir temporário");
    dir
}

/// `--version` imprime a versão e sai com código 0.
#[test]
fn version_sai_com_sucesso() {
    let saida = onyxchatd().arg("--version").output().expect("spawn");
    assert!(saida.status.success(), "{saida:?}");
    let stdout = String::from_utf8_lossy(&saida.stdout);
    assert!(stdout.contains("onyxchatd"), "{stdout}");
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
}

/// `--help` imprime a ajuda e sai com código 0.
#[test]
fn help_sai_com_sucesso() {
    let saida = onyxchatd().arg("--help").output().expect("spawn");
    assert!(saida.status.success(), "{saida:?}");
    let stdout = String::from_utf8_lossy(&saida.stdout);
    assert!(stdout.contains("--socket"), "{stdout}");
    assert!(stdout.contains("--version"), "{stdout}");
}

/// Opção desconhecida → código de saída 1 e mensagem no stderr.
#[test]
fn opcao_desconhecida_sai_com_erro() {
    let saida = onyxchatd().arg("--lixo").output().expect("spawn");
    assert!(!saida.status.success(), "não podia ter sucesso");
    let stderr = String::from_utf8_lossy(&saida.stderr);
    assert!(stderr.contains("opção desconhecida"), "{stderr}");
    // Sem dados sensíveis no erro.
    assert!(stderr.len() < 200, "{stderr}");
}

/// `--socket` sem caminho → erro de linha de comando (exit ≠ 0).
#[test]
fn socket_sem_caminho_sai_com_erro() {
    let saida = onyxchatd().arg("--socket").output().expect("spawn");
    assert!(!saida.status.success());
    let stderr = String::from_utf8_lossy(&saida.stderr);
    assert!(stderr.contains("exige um caminho"), "{stderr}");
}

/// `--socket <diretório>` → falha de bind reportada no stderr (exit 1).
///
/// Um diretório não pode ser socket: o daemon entra no ramo de erro de
/// `ipc::servir` e sai sem rebentar (nunca panic com dados externos).
///
/// Usa `--tor nenhum` explícito: o objectivo é exercitar o caminho do
/// bind IPC, e não o do backend Tor. Assim o teste cobre a mesma coisa
/// em qualquer configuração de features.
#[test]
fn socket_invalido_reporta_falha_ipc() {
    let dir = dir_temporario("invalido");
    let saida = onyxchatd()
        .arg("--socket")
        .arg(&dir)
        .arg("--tor")
        .arg("nenhum")
        .output()
        .expect("spawn");
    assert!(!saida.status.success());
    let stderr = String::from_utf8_lossy(&saida.stderr);
    assert!(stderr.contains("falha no IPC"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **Propriedade de segurança, ao nível do binário:** um binário
/// compilado sem a feature `tor-real` **recusa arrancar** quando lhe
/// pedem o backend `arti`.
///
/// Este é o teste que garante que o modo sem Tor nunca se apresenta como
/// se tivesse Tor. Corre na configuração por omissão (sem `tor-real`),
/// que é a que a CI executa.
#[cfg(not(feature = "tor-real"))]
#[test]
fn arti_sem_a_feature_nao_arranca() {
    let dir = dir_temporario("sem-tor");
    let saida = onyxchatd()
        .arg("--socket")
        .arg(dir.join("m.sock"))
        .arg("--tor")
        .arg("arti")
        .output()
        .expect("spawn");
    assert!(
        !saida.status.success(),
        "pedir arti sem a feature tem de falhar, não arrancar"
    );
    let stderr = String::from_utf8_lossy(&saida.stderr);
    assert!(stderr.contains("tor-real"), "tem de citar a feature: {stderr}");
    assert!(
        stderr.contains("--features tor-real"),
        "tem de dar o comando de build: {stderr}"
    );
    // O ponto central: **não pode ter servido nada**.
    assert!(
        !dir.join("m.sock").exists(),
        "um arranque recusado não pode deixar socket — seria serviço sem Tor"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Com `--tor nenhum`, o binário arranca mesmo sem a feature `tor-real`:
/// o modo offline tem de continuar utilizável para testes e experimentação.
#[cfg(not(feature = "tor-real"))]
#[test]
fn tor_nenhum_arranca_sem_a_feature() {
    let dir = dir_temporario("offline-ok");
    let caminho = dir.join("m.sock");
    let mut filho = onyxchatd()
        .arg("--socket")
        .arg(&caminho)
        .arg("--tor")
        .arg("nenhum")
        .spawn()
        .expect("spawn");
    // O daemon tem de criar o socket: se vivo ao fim de uma pausa, o
    // arranque foi bem-sucedido.
    let mut vivo = false;
    for _ in 0..50 {
        if caminho.exists() {
            vivo = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(vivo, "o modo offline tem de arrancar sem a feature tor-real");
    let _ = std::fs::remove_dir_all(&dir);
}
