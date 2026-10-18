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

/// Corre o executável e devolve a saída de um dos seus relatórios.
fn relatorio(flag: &str) -> String {
    let fd = embutido::preparar().expect("prepara o memfd");
    let saida = std::process::Command::new(embutido::caminho())
        .args(["-hide_banner", flag])
        .output()
        .expect("o executável embebido não arrancou");
    embutido::fechar();
    assert_eq!(fd, embutido::SLOT_EMBUTIDO);
    String::from_utf8_lossy(&saida.stdout).into_owned()
}

/// O executável embebido sabe **capturar**.
///
/// ## O que este teste apanha, e por que precisava de existir
///
/// O binário que estava embebido declarava `--enable-indev=alsa` no
/// `-buildconf` e **não tinha `alsa`**. Compilava limpo, passava a
/// bateria toda, e só falhava quando um utilizador pedia uma chamada:
///
/// ```text
/// $ ./ffmpeg -f x11grab -i :0.0 -t 0.1 -f null -
/// [in#0 @ 0x…] Unknown input format: 'x11grab'
/// $ ./ffmpeg -i default -t 0.1 -f null -
/// Error opening input file default.
/// $ ./ffmpeg -devices
/// Devices:
///  ---                                (vazio)
/// ```
///
/// A causa é a receita: `--disable-everything` desligou também os
/// *formatos de entrada*, e ninguém os voltou a ligar. Nenhum codec
/// faltava — o VP8, o Opus e o rawvideo estavam todos lá. O que faltava
/// era a porta de entrada.
///
/// ## Porque não basta o `-buildconf`
///
/// Porque `-buildconf` imprime as opções **pedidas**, não as
/// activadas. O `configure` aceita `--enable-indev=alsa`, descobre que
/// a biblioteca não satisfaz o `snd_pcm_htimestamp`, desliga o
/// dispositivo com um `WARNING` na sua saída e continua. A linha fica
/// na configuração, o dispositivo não fica no binário, e a única forma
/// de os distinguir é perguntar ao binário — que é o que este teste faz.
///
/// ## Porque `v4l2` e `alsa`, e não `x11grab`
///
/// Porque uma chamada precisa de câmara e de microfone, e não de
/// ecrã. `v4l2` é `/dev/video*`, uma interface do kernel, e `alsa` é
/// o microfone directo. O `x11grab` saiu da receita, e a razão está
/// discutida em `tools/ffmpeg/build.sh`.
#[test]
fn o_executavel_embutido_sabe_capturar() {
    if !embutido::disponivel() {
        salta("sem executável embebido nesta compilação");
        return;
    }

    // `-devices` lista os que o binário reconhece; `-formats` lista os
    // demuxers. O `v4l2` é um demuxer (`video4linux2`) e o `alsa` é um
    // dispositivo: por isso as duas listas.
    let dispositivos = relatorio("-devices");
    let formatos = relatorio("-formats");

    for entrada in ["alsa", "video4linux2", "v4l2"] {
        assert!(
            dispositivos.contains(entrada) || formatos.contains(entrada),
            "o executável embebido NÃO tem a entrada de captura «{entrada}».\n\
             O `MEDIA_CAPTURAR` (0x0F) daria `ERR_MEDIA` ao primeiro uso, \
             com o daemon já a correr e a ligação estabelecida.\n\n\
             A receita tem `--disable-everything` e uma lista explícita de \
             `--enable-*`; uma entrada de captura que não esteja na lista \
             não entra, e nenhum codec a traz de volta.\n\n\
             --- devices ---\n{dispositivos}\n\
             --- formats ---\n{formatos}"
        );
    }
}

/// O executável embebido descodifica Opus.
///
/// Sem isto o áudio da chamada **não tem caminho de volta**: o
/// `--enable-encoder=libopus` codifica, e quem tem de ouvir é quem não
/// codificou. A receita habilitou o encoder e esqueceu o decoder — o
/// mesmo caso do `rawvideo` e do `pcm_s16le`, e o mesmo aviso.
#[test]
fn o_executavel_embutido_descodifica_opus() {
    if !embutido::disponivel() {
        salta("sem executável embebido nesta compilação");
        return;
    }

    let saida = relatorio("-decoders");
    assert!(
        saida.contains("opus"),
        "o executável embebido NÃO descodifica Opus.\n\
         O áudio é codificado com `libopus` e nunca poderia ser ouvido: \
         «Decoding requested, but no decoder found for: opus».\n\n{saida}"
    );
}

/// O executável embebido descodifica VP8 — e é o que a **`Captura`** não usa.
///
/// ## A ironia que este teste documenta
///
/// O `docs/media.md` §8.0.2 diz que «o daemon transporta; não
/// deserializa», e que «quem descodifica é a UI». A UI é um WebKitGTK 6
/// (`scripts/interface-visual.js`), e o WebKitGTK **não garante**
/// `VideoDecoder` — a API de WebCodecs. O `VideoChamada.js` foi escrito
/// contra essa hipótese e avisa no seu cabeçalho que «um browser sem
/// `VideoDecoder` não tem vídeo».
///
/// O binário embebido, aquele mesmo que o daemon executa e que já traz
/// `--enable-decoder=vp8` desde a receita, é um descodificador VP8
/// completo. Este teste não o usa ainda; existe para que, quando a Fase
/// de descodificação o usar, ninguém se esqueça de que a capacidade já
/// lá está.
#[test]
fn o_executavel_embutido_descodifica_vp8() {
    if !embutido::disponivel() {
        salta("sem executável embebido nesta compilação");
        return;
    }

    let saida = relatorio("-decoders");
    assert!(
        saida.contains("vp8"),
        "o executável embebido NÃO descodifica VP8.\n\
         Sem isto não há imagem para a UI mostrar: a UI não tem \
         `VideoDecoder`, e a descodificação tem de ser aqui.\n\n{saida}"
    );
}

/// O executável embebido sabe produzir **uma imagem**.
///
/// A UI não tem `VideoDecoder`, e um `<img>` não descodifica VP8. O
/// caminho que sobra é: o daemon descodifica o VP8 para pixéis e
/// re-codifica em JPEG, que qualquer motor de renderização mostra sem
/// descodificar nada. Este teste verifica que as duas peças existem.
#[test]
fn o_executavel_embutido_produz_imagem() {
    if !embutido::disponivel() {
        salta("sem executável embebido nesta compilação");
        return;
    }

    let encoders = relatorio("-encoders");
    assert!(
        encoders.contains("mjpeg"),
        "o executável embebido NÃO tem o encoder `mjpeg`.\n\
         Sem ele não há caminho de VP8 para uma imagem que a UI saiba \
         mostrar.\n\n{encoders}"
    );

    let muxers = relatorio("-muxers");
    assert!(
        muxers.contains("image2"),
        "o executável embebido NÃO tem o muxer `image2`, que é quem \
         escreve a imagem.\n\n{muxers}"
    );
}
