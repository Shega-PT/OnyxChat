// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes da reprodução.
//!
//! O teste central é [`um_filho_real_le_o_memfd`], que não usa nenhum
//! leitor de media: usa o `cat`. É o que prova a parte que importa —
//! que o descritor **atravessa** um `exec` e o conteúdo chega inteiro
//! do outro lado — sem depender de um `mpv` instalado.

use super::*;

#[test]
fn o_caminho_e_o_ultimo_argumento() {
    // Um caminho a meio dos argumentos seria um erro silencioso: o
    // leitor abriria o ficheiro errado, ou nenhum. E `cat` sem
    // argumentos esperaria por `stdin`, que aqui é `/dev/null`.
    let caminho = "/proc/self/fd/7";
    for leitor in ["mpv", "ffplay"] {
        let args = argumentos_de_leitor(leitor, caminho);
        assert_eq!(
            args.last(),
            Some(&caminho),
            "o caminho não é o último em {leitor}"
        );
        assert!(
            !args[..args.len() - 1].contains(&caminho),
            "o caminho repete-se"
        );
    }
}

#[test]
fn os_argumentos_fecham_a_janela_e_saem() {
    // Um áudio que abre uma janela é uma janela que fica, e o `ffplay`
    // sem `-autoexit` fica à espera de uma tecla que ninguém carrega.
    let mpv = argumentos_de_leitor("mpv", "/x");
    assert!(mpv.contains(&"--no-video"), "o mpv pode abrir uma janela");

    let ffplay = argumentos_de_leitor("ffplay", "/x");
    assert!(ffplay.contains(&"-nodisp"));
    assert!(ffplay.contains(&"-autoexit"));
}

#[test]
fn um_leitor_desconhecido_e_erro_e_nao_fallback() {
    // Sem ramo de recurso. Um leitor desconhecido que caísse num
    // programa qualquer seria pior do que falhar: a pessoa ouviria o
    // áudio no programa errado, ou não ouviria nada sem saber porquê.
    let erro = nome_de_leitor("totem").expect_err("totem não é um leitor");
    assert!(matches!(erro, ErroMedia::LeitorDesconhecido(n) if n == "totem"));
}

#[test]
fn um_filho_real_le_o_memfd() {
    // O caminho de produção, inteiro: `memfd`, selo, `spawn`, e o
    // filho abre `/proc/self/fd/N` e lê o conteúdo.
    //
    // `cat` em vez de `mpv` é deliberado. O que se está a testar é o
    // mecanismo — que o descritor sobrevive ao `exec` e tem o conteúdo
    // certo — e `mpv` introduziria uma dependência de som, de
    // biblioteca gráfica e de estar instalado. Se este teste passar, a
    // parte que é do projecto está provada.
    let conteudo: Vec<u8> = (0..=255u8).cycle().take(50_000).collect();
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", &conteudo).expect("memfd");
    let caminho = format!("/proc/self/fd/{}", ficheiro.fd());

    let saida = std::process::Command::new("cat")
        .arg(&caminho)
        .output()
        .expect("cat");

    assert!(saida.status.success(), "cat falhou: {saida:?}");
    assert_eq!(saida.stdout, conteudo, "o conteúdo não chegou inteiro");
}

#[test]
fn o_socket_do_daemon_nao_vai_para_o_leitor() {
    // O leitor herda os descritores do daemon, e um leitor com o socket
    // do IPC no seu `fd` é um leitor que pode falar o protocolo inteiro.
    //
    // A listagem é feita **dentro** do filho, e não a partir de
    // aqui: os números de descritor são por processo, e usar os do
    // filho para abrir `/proc/self/fd/N` do pai lê os descritores do
    // **pai** — que têm o socket do próprio daemon. Era isso que
    // fazia este teste passar sozinho e falhar na suite.
    let conteudo = b"privado";
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", conteudo).expect("memfd");
    let caminho = format!("/proc/self/fd/{}", ficheiro.fd());

    let saida = std::process::Command::new("sh")
        .arg("-c")
        .arg("for f in /proc/self/fd/*; do readlink \"$f\"; done")
        .arg(caminho.as_str())
        .output()
        .expect("sh");

    let alvos = String::from_utf8_lossy(&saida.stdout);
    assert!(
        !alvos.contains("socket"),
        "o leitor herdou um socket: {alvos:?}"
    );
    assert!(
        alvos.contains("memfd:onyx-media"),
        "o ficheiro não chegou ao filho: {alvos:?}"
    );
}

#[test]
fn os_leitores_disponiveis_sao_os_conhecidos() {
    // A lista é fechada. Um programa que apareça no `PATH` e se chame
    // `mpv` seria executado, e isso é o tipo de coisa que só se descobre
    // depois.
    for nome in leitores_disponiveis() {
        assert!(LEITORES.contains(&nome), "leitor fora da lista: {nome}");
    }
    match leitor_disponivel() {
        Some(nome) => assert_eq!(Some(nome), LEITORES.iter().find(|n| **n == nome).copied()),
        None => assert!(leitores_disponiveis().is_empty()),
    }
}
