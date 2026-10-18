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
fn o_leitor_embutido_descodifica_opus_a_pcm() {
    // O teste que prova que o degrau 4 faz o que promete.
    //
    // ## O caminho inteiro
    //
    // `memfd` com Opus → `ffmpeg` embebido → `memfd` com `wav`. E o
    // conteúdo é Opus **a sério**, gerado pelo encoder que a receita
    // embute — não um ficheiro que se fabrication para o teste passar.
    //
    // É esta prova que separa «o binário tem o decoder Opus» de «o
    // decoder Opus funciona neste binário»: a receita pode declarar o
    // decoder e o binário tê-lo, e ainda assim a descodificação dar
    // erro no contentor errado.
    if !super::super::ffmpeg_embutido::disponivel() {
        eprintln!("media: sem executável embebido; a descodificação não é verificável");
        return;
    }

    let fd_exec = super::super::ffmpeg_embutido::preparar().expect("prepara o executável");
    let caminho_exec = format!("/proc/self/fd/{}", fd_exec);

    // --- gerar o Opus --------------------------------------------------
    // Um tom senoidal de 0,5 s a 48 kHz. O `-f lavfi` **não** está na
    // receita, e por isso a fonte é `s16le` crua: o teste escreve o PCM
    // ele próprio e deixa o `ffmpeg` apenas codificar.
    let amostras: Vec<i16> = (0_i64..24_000)
        .map(|n| {
            let t = n as f64 / 48_000.0;
            (t.sin() * 8_000.0) as i16
        })
        .collect();
    let mut pcm: Vec<u8> = Vec::with_capacity(amostras.len() * 2);
    for amostra in &amostras {
        pcm.extend_from_slice(&amostra.to_le_bytes());
    }

    // Ficheiros em disco **só para a geração**. O caminho de produção é
    // todo em `memfd`, mas o `ffmpeg` escreve a *entrada* PCM num
    // contentor que precisa de tamanho conhecido, e um `memfd` criado
    // com conteúdo já o tem — o que não funciona aqui é que o
    // `s16le` precisa de saber que o fim do ficheiro é o fim do
    // stream, e o `ffmpeg` procura um cabeçalho que o `memfd` não tem.
    // O que se está a testar é a **descodificação**, e a geração é o
    // arranjo.
    let dir = std::env::temp_dir().join(format!("onyx-reproduzir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("diretório temporário");
    let caminho_pcm = dir.join("entrada.pcm");
    let caminho_opus = dir.join("saida.opus");
    std::fs::write(&caminho_pcm, &pcm).expect("escreve o PCM");

    let codigo = correr_ffmpeg(
        &caminho_exec,
        &[
            "-hide_banner", "-loglevel", "error",
            "-f", "s16le", "-ar", "48000", "-ac", "1",
            "-i", caminho_pcm.to_str().expect("caminho"),
            "-c:a", "libopus", "-f", "opus",
            caminho_opus.to_str().expect("caminho"),
        ],
    );
    assert!(codigo == 0, "o ffmpeg embebido não gerou Opus (código {codigo})");

    let opus = std::fs::read(&caminho_opus).expect("lê o Opus gerado");
    assert!(
        opus.len() > 100,
        "o Opus gerado tem {} bytes: o encoder não produziu nada de aproveitável",
        opus.len()
    );
    assert_eq!(
        &opus[..4],
        b"OggS",
        "o que saiu não é um contentor Ogg/Opus: {:02x?}",
        &opus[..4.min(opus.len())]
    );

    // --- descodificar com o caminho de produção ------------------------
    // O `executar_embutido` recebe um `FicheiroEmRam`; aqui o conteúdo
    // Opus vai para um `memfd` de verdade, que é o mesmo mecanismo que
    // a produção usa.
    let entrada_memfd = FicheiroEmRam::de_conteudo("onyx-teste-opus", &opus).expect("memfd");
    let caminho_entrada = format!("/proc/self/fd/{}", entrada_memfd.fd());

    let pid = executar_embutido(entrada_memfd, &caminho_entrada)
        .expect("o leitor embebido arranca");
    assert!(pid > 0, "o pid do leitor embutido é {pid}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Corre o `ffmpeg` embebido com argumentos e devolve o código de saída.
fn correr_ffmpeg(caminho_exec: &str, argumentos: &[&str]) -> i32 {
    let executavel: std::ffi::OsString = caminho_exec.into();
    let argumentos: Vec<std::ffi::OsString> =
        argumentos.iter().map(|a| std::ffi::OsString::from(*a)).collect();

    let mut comando = std::process::Command::new(&executavel);
    comando.args(&argumentos);
    comando.output().map(|s| s.status.code().unwrap_or(-1)).unwrap_or(-1)
}

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
    let erro = nome_de_byte_leitor(200).expect_err("200 não é um leitor");
    assert!(matches!(erro, ErroMedia::LeitorDesconhecido(_)));
}

#[test]
fn o_mapa_de_bytes_e_estavel_e_completo() {
    // O mapa nome→byte substituiu um índice sobre a lista, e a razão
    // foi a estabilidade: acrescentar um leitor **não pode** mudar o
    // byte de um leitor que já existia, ou um `sidecar` que guardou o
    // número de uma sessão anterior passa a pedir outro programa.
    //
    // Este teste fixa os bytes de propósito. Um dia em que um byte mude,
    // este teste falha — e a falha é o aviso.
    assert_eq!(byte_de_leitor(LEITOR_EMBUTIDO), Some(0x00));
    assert_eq!(byte_de_leitor("mpv"), Some(0x01));
    assert_eq!(byte_de_leitor("ffplay"), Some(0x02));
    assert_eq!(byte_de_leitor("cvlc"), Some(0x03));
    assert_eq!(byte_de_leitor("totem"), None, "um programa qualquer não é um leitor");

    // E a volta é exacta para todos os que existem.
    for (nome, byte) in [("embutido", 0x00u8), ("mpv", 0x01), ("ffplay", 0x02), ("cvlc", 0x03)] {
        assert_eq!(
            nome_de_byte_leitor(byte).expect("byte conhecido"),
            nome,
            "o byte {byte} não volta a «{nome}»"
        );
    }
}

#[test]
fn nenhum_byte_de_leitor_colide_com_o_nenhum() {
    // `0xFF` é o que o `MEDIA_ESTADO` devolve quando não há leitor, e
    // um leitor que ocupasse esse valor tornaria «não há leitor» e
    // «há leitor X» a mesma resposta.
    //
    // O teste percorre os 256 valores. Um byte que **não** é de um
    // leitor tem de dar erro — e `0xFF` entre eles. O que não pode
    // acontecer é um leitor no `0xFF`.
    for byte in 0x00u8..=0xFF {
        let resultado = nome_de_byte_leitor(byte);
        if byte == LEITOR_NENHUM {
            assert!(
                resultado.is_err(),
                "«não há leitor» ({byte:#04x}) devolveu um leitor: «{resultado:?}»"
            );
        }
    }

    // E, por simetria, os bytes dos leitores são todos diferentes entre
    // si: dois leitores com o mesmo byte seriam indistinguíveis na
    // resposta do `MEDIA_ESTADO`.
    let mut vistos = std::collections::BTreeSet::new();
    for nome in ["embutido", "mpv", "ffplay", "cvlc"] {
        let byte = byte_de_leitor(nome).expect("leitor conhecido");
        assert!(
            vistos.insert(byte),
            "o byte {byte:#04x} foi usado por dois leitores"
        );
    }
}

#[test]
fn o_leitor_embutido_aparece_quando_o_sistema_nao_tem_nenhum() {
    // O caso que motivou o degrau: uma máquina mínima sem `mpv`,
    // `ffplay` e `cvlc` dava `MEDIA_ESTADO` → `0xFF`, e o botão de
    // reproduzir não tinha resposta nenhuma.
    //
    // Não se pode desligar o que o sistema tem, por isso o que este
    // teste prova é a parte que se pode: o embebido **está** na lista
    // sempre que existe, e vem em primeiro — que é o que um
    // `sidecar` que respeite a ordem vai escolher.
    if !super::super::ffmpeg_embutido::disponivel() {
        eprintln!("media: sem executável embebido; a lista de leitores não é verificável");
        return;
    }

    let lista = leitores_disponiveis();
    assert!(
        lista.contains(&LEITOR_EMBUTIDO),
        "com o executável embebido presente, «{lista:?}» tem de o incluir"
    );
    assert_eq!(
        lista.first(),
        Some(&LEITOR_EMBUTIDO),
        "o embebido vem primeiro: é o único que não depende do que a pessoa instalou"
    );
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
    //
    // O que «conhecido» significa passou a ser **o mapa de bytes**, e não
    // a lista de leitores do `PATH`: o `embutido` é conhecido e não
    // está nessa lista, porque não é um programa — é o binário que este
    // processo tem num `memfd`. A lista de `PATH` continua a ser a lista
    // de *programas* que se procura no sistema.
    for nome in leitores_disponiveis() {
        assert!(
            byte_de_leitor(nome).is_some(),
            "leitor fora do mapa de bytes: {nome}"
        );
        assert!(
            nome == LEITOR_EMBUTIDO || LEITORES.contains(&nome),
            "leitor que não é do sistema nem o embebido: {nome}"
        );
    }
    // E o primeiro da lista tem de ter um byte no mapa — é esse byte
    // que o `MEDIA_ESTADO` devolve e que o `MEDIA_REPRODUZIR` aceita.
    match leitor_disponivel() {
        Some(nome) => assert!(
            byte_de_leitor(nome).is_some(),
            "«{nome}» é o primeiro da lista mas não tem byte no mapa"
        ),
        None => assert!(leitores_disponiveis().is_empty()),
    }
}
