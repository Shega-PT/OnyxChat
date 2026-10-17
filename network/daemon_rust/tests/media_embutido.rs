// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media_embutido.rs — o executável embebido arranca mesmo?
// ---------------------------------------------------------------------
// Os testes de unidade em `src/media/ffmpeg_embutido.rs` provam a
// **mecânica** (o slot, o selo, o caminho). Este prova o **fim**: que
// o que está dentro do binário é um programa que corre.
//
// A diferença importa. Uma `memfd` que se escreve sem erro prova que se
// escreve; não prova que o que se escreveu é executável, nem que o
// `execve` de `/proc/self/fd/N` é aceite pelo kernel, nem que a
// imagem tem um ponto de entrada válido. Isto é o único sítio onde se
// descobre.
//
// ## Porque o teste salta quando não há executável
//
// Porque a árvore de compilação dos testes não compila o FFmpeg: isso
// são dois minutos de CPU e 600 MB de RAM, e `cargo test` tem de correr
// numa máquina de desenvolvimento modesta. Sem executável o teste não
// tem o que testar, e um teste que falha por ausência de um passo
// opcional é um teste que obriga a repetir a suite inteira só para
// executar uma coisa que não mudou.
//
// O que salta é **dito**: `eprintln` com o motivo. Um salto silencioso
// é indistinguível de um teste verde, e `docs/testing.md` §Cobertura
// exige que o que não foi medido esteja declarado.
// =====================================================================

use onyxchatd::media::ffmpeg_embutido as embutido;

/// Sai com o motivo do salto, para a saída de erro do teste.
fn salta(motivo: &str) {
    eprintln!("media: {motivo}");
    eprintln!("        corra tools/ffmpeg/fetch.sh e tools/ffmpeg/build.sh");
}

/// O executável embebido executa a partir do `memfd`.
#[test]
fn o_executavel_embutido_arranca_do_memfd() {
    if !embutido::disponivel() {
        salta("sem executável embebido nesta compilação");
        return;
    }

    let fd = embutido::preparar().expect("prepara o memfd");
    assert_eq!(fd, embutido::SLOT_EMBUTIDO, "o fd não é o slot reservado");

    let saida = std::process::Command::new(embutido::caminho())
        .args(["-hide_banner", "-version"])
        .output()
        .expect("o executável embebido não arrancou");

    assert!(
        saida.status.success(),
        "o executável embebido saiu com {:?}\nstderr: {}",
        saida.status.code(),
        String::from_utf8_lossy(&saida.stderr)
    );

    let versao = String::from_utf8_lossy(&saida.stdout);
    assert!(
        versao.contains("ffmpeg version"),
        "a saída não é a de um ffmpeg:\n{versao}"
    );

    embutido::fechar();
}

/// O executável embebido declara a configuração que a excepção promete.
///
/// **Este é o teste que sustenta a excepção de licença.** A garantia de
/// `THIRD-PARTY.md` §Componentes compilados de fonte não é uma nota: é
/// que o binário, sobre si próprio, diz que foi compilado com
/// `--disable-gpl`.
///
/// E é um teste no Rust e não só uma linha no `build.sh`, porque o
/// `build.sh` verifica o binário que *ele* acabou de compilar, e o
/// `build.rs` embete o que está em `cache/`. Entre as duas coisas pode
/// ficar um `cache/ffmpeg.xz` de uma compilação antiga, de uma receita
/// antiga, com um `--enable-gpl` que alguém pôs lá. Este teste lê o que
/// está **dentro do binário em teste**, e não o que o script fez.
#[test]
fn o_executavel_embutido_declara_sem_gpl() {
    if !embutido::disponivel() {
        salta("sem executável embebido nesta compilação");
        return;
    }

    let fd = embutido::preparar().expect("prepara o memfd");
    let saida = std::process::Command::new(embutido::caminho())
        .args(["-hide_banner", "-buildconf"])
        .output()
        .expect("o executável embebido não arrancou");
    embutido::fechar();
    assert_eq!(fd, embutido::SLOT_EMBUTIDO);

    let conf = String::from_utf8_lossy(&saida.stdout);
    assert!(
        conf.contains("--disable-gpl"),
        "o executável embebido NÃO foi compilado com --disable-gpl.\n\
         A excepção de THIRD-PARTY.md não o cobre, e isto é um binário \
         GPL dentro do daemon.\n\n{conf}"
    );
    assert!(
        conf.contains("--disable-nonfree"),
        "o executável embebido não declara --disable-nonfree:\n{conf}"
    );
}

/// A versão embebida descreve o executável, e só existe se ele existir.
#[test]
fn a_versao_descreve_o_executavel() {
    let versao = embutido::versao_embutida();

    if !embutido::disponivel() {
        assert!(
            versao.is_empty(),
            "sem executável não há versão para descrever, e «{versao}» é uma versão que não existe"
        );
        return;
    }

    assert!(
        versao.contains("LGPL"),
        "a versão embebida não declara a licença: «{versao}»"
    );
    assert!(
        versao.contains("--disable-gpl"),
        "a versão embebida não declara a receita: «{versao}»"
    );
}
