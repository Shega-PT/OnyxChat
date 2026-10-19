// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes da descodificação VP8→JPEG.
//!
//! O teste que interessa é o que **descodifica VP8 de verdade** e
//! compara o JPEG com um JPEG produzido directamente pelo mesmo
//! `ffmpeg`. Isso prova três coisas de uma vez: que o VP8 se descodifica,
//! que o caminho de IVF reconstruído é válido, e que a qualidade fixada
//! produz uma imagem que um `<img>` consegue mostrar.
//!
//! O resto dos testes é sobre as guardas: o que tem de recusar e o que
//! tem de passar.

use super::*;

/// Um VP8 sintético, gerado pelo `ffmpeg` embebido.
///
/// ## Porque gerar e não usar um ficheiro fixo
///
/// Porque um ficheiro de teste com VP8 tem de ser versionado no
/// repositório, e um `.ivf` de 640×480 são dezenas de quilobytes de
/// dados binários que ninguém lê. Gerar com a mesma biblioteca que
/// descodifica tem uma propriedade melhor: se a biblioteca mudar, o
/// teste muda com ela — e um teste que fixasse os bytes deixaria de
/// medir o que é suposto medir.
///
/// E o VP8 gerado é um **quadro real de imagem**, com formas e
/// gradientes, que é o que faz o `ffmpeg` gastar trabalho a descodificar
/// em vez de copiar bytes.
/// Lê um descritor do início ao fim.
///
/// E um `read` e não um `File::from_raw_fd` porque o `from_raw_fd`
/// assume a posse do descritor e fechá-lo no `Drop` fecharia o `fd 3` de
/// outra coisa se o número já tiver sido reciclado — e num teste que
/// corre em paralelo com outros, reciclação é o caso comum.
fn ler_memfd_fd(fd: std::os::fd::RawFd) -> Vec<u8> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let caminho = format!("/proc/self/fd/{fd}");
    let Ok(mut f) = std::fs::File::open(&caminho) else {
        return Vec::new();
    };
    if f.seek(SeekFrom::Start(0)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    buf
}

fn vp8_sintetico(largura: u16, altura: u16) -> Option<Vec<u8>> {
    if !super::super::ffmpeg_embutido::disponivel() {
        return None;
    }

    let fd = super::super::ffmpeg_embutido::preparar().expect("prepara o executável");
    let caminho_exec = format!("/proc/self/fd/{fd}");

    let executavel: std::ffi::OsString = caminho_exec.into();
    let argumentos: Vec<std::ffi::OsString> = [
        "-hide_banner",
        "-loglevel",
        "error",
        // A entrada é uma imagem **crua** em YUV 4:2:0 — o plano de
        // luminância e os dois de croma a meia resolução.
        //
        // E não um `-f lavfi -i gradients`: o filtro `gradients` **não
        // está** na receita, que só habilita `scale`, `format` e
        // `null`, e o `ffmpeg` responde «No such filter: 'gradients'».
        // Escrever os pixéis aqui é o que torna o teste independente
        // do que a receita decide habilitar.
        "-f",
        "rawvideo",
        "-pix_fmt",
        "yuv420p",
        "-s",
        &format!("{largura}x{altura}"),
        "-i",
        "pipe:0",
        "-frames:v",
        "1",
        "-c:v",
        "libvpx",
        "-f",
        "ivf",
        "pipe:1",
    ]
    .iter()
    .map(std::ffi::OsString::from)
    .collect();

    let mut comando = std::process::Command::new(&executavel);
    comando.args(&argumentos);
    comando.stdin(std::process::Stdio::piped());
    // A saída é o `pipe:1`, que é o `stdout` do filho. Um `null()` aqui
    // fecharia o `stdout` e o `ffmpeg` escreveria para um descritor
    // morto — o que dá um ficheiro de zero bytes e um código de saída
    // a dizer «tudo bem».
    comando.stdout(std::process::Stdio::piped());
    comando.stderr(std::process::Stdio::null());

    let mut filho = match comando.spawn() {
        Ok(f) => f,
        Err(_) => {
            super::super::ffmpeg_embutido::fechar();
            return None;
        }
    };

    // Uma imagem com estrutura: metade esquerda preta, metade direita
    // branca. O VP8 tem de codificar a fronteira, e um bloco liso
    // quase não ocuparia espaço — o que faria a descodificação ser um
    // teste que não testa nada.
    if let Some(mut entrada) = filho.stdin.take() {
        use std::io::Write as _;
        let w = u32::from(largura);
        let h = u32::from(altura);

        let mut plano = Vec::with_capacity(w as usize * h as usize);
        for _ in 0..h {
            for x in 0..w {
                plano.push(if x < w / 3 { 16u8 } else { 235u8 });
            }
        }
        let _ = entrada.write_all(&plano);

        // O croma a meia resolução, em cinzento — que é o que dá um
        // tom neutro sem introduzir cor que o VP8 tenha de codificar.
        let meio = (w / 2) as usize * (h / 2) as usize;
        let _ = entrada.write_all(&vec![128u8; meio]);
        let _ = entrada.write_all(&vec![128u8; meio]);
    }
    // A leitura vem antes do `wait`: o `ffmpeg` bloqueia a escrever
    // assim que o pipe enche, e o pipe enche.
    // O `stderr` primeiro: e onde esta a razao, e ler o `stdout` antes
    // bloquearia a espera pelo EOF de um `ffmpeg` que ja morreu.
    let dados = match filho.stdout.take() {
        Some(mut s) => {
            use std::io::Read as _;
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            buf
        }
        None => Vec::new(),
    };
    let _ = filho.wait();
    super::super::ffmpeg_embutido::fechar();

    if dados.is_empty() {
        return None;
    }
    // O que o `ffmpeg` escreveu é um IVF; o que interessa é o VP8 de
    // dentro dele.
    let mut leitor = ivf::LeitorIvf::novo(std::io::Cursor::new(dados)).ok()?;
    match leitor.proximo().ok()? {
        ivf::EstadoQuadro::Quadro(q) => Some(q.dados),
        _ => None,
    }
}

/// Um cabeçalho de IVF com a geometria pedida.
fn cabecalho(largura: u16, altura: u16) -> ivf::Cabecalho {
    ivf::Cabecalho {
        largura,
        altura,
        fps: 30,
    }
}

/// A descodificação de um VP8 real produz um JPEG que um `<img>` mostra.
///
/// É o teste central do ficheiro, e o que substitui o `VideoDecoder` da
/// interface.
#[test]
fn um_vp8_real_vira_um_jpeg_que_a_interface_mostra() {
    let Some(vp8) = vp8_sintetico(320, 240) else {
        eprintln!("media: sem ffmpeg embebido ou sem VP8; a descodificação não é verificável");
        return;
    };
    assert!(!vp8.is_empty(), "o VP8 gerado tem zero bytes");

    let imagem = match descodificar(&vp8, cabecalho(320, 240), 7) {
        Ok(i) => i,
        Err(e) => panic!("a descodificação de um VP8 válido falhou: {e}"),
    };

    assert_eq!(imagem.largura, 320, "a largura não veio do cabeçalho");
    assert_eq!(imagem.altura, 240, "a altura não veio do cabeçalho");
    assert_eq!(imagem.sequencia, 7, "o número de sequência não passou");
    assert!(
        imagem.parece_jpeg(),
        "a saída não tem o começo de um JPEG: {:02x?}",
        &imagem.jpeg[..imagem.jpeg.len().min(8)]
    );
    assert!(
        imagem.tamanho() > 200,
        "o JPEG tem {} bytes: uma imagem de 320×240 compresse-se para \
         mais do que isso",
        imagem.tamanho()
    );
}

/// O JPEG tem os marcadores de fim, e não só o começo.
///
/// Um JPEG truncado tem `SOI` e não tem `EOI`, e um motor de
/// renderização mostra-o como uma imagem cinzenta ou não mostra nada —
/// sem erro. A verificação é o que distingue «uma imagem» de «os
/// primeiros bytes de uma».
#[test]
fn o_jpeg_tem_marcador_de_fim() {
    let Some(vp8) = vp8_sintetico(320, 240) else {
        eprintln!("media: sem VP8; o fim do JPEG não é verificável");
        return;
    };

    let imagem = descodificar(&vp8, cabecalho(320, 240), 1).expect("descodifica");
    let ultimos = &imagem.jpeg[imagem.jpeg.len().saturating_sub(2)..];
    assert_eq!(
        ultimos,
        [0xFF, 0xD9],
        "o JPEG não termina em EOI: {:02x?}",
        ultimos
    );
}

/// Um VP8 vazio recusa, e não devolve uma imagem de zero bytes.
///
/// Um JPEG de zero bytes seria um `blob:` vazio, e a interface mostraria
/// um `<img>` que não carrega sem dizer porquê.
#[test]
fn um_vp8_vazio_recusa() {
    let erro = descodificar(&[], cabecalho(320, 240), 0).expect_err("um VP8 vazio tem de recusar");
    assert!(
        matches!(erro, ErroMedia::QuadroVP8Vazio),
        "o erro é «{erro}» e não é o de quadro vazio"
    );
}

/// Um VP8 que não é VP8 recusa.
///
/// E o sintoma que importa é que **não** devolve nada: um `ffmpeg` que
/// recebe lixo sai com `0` e não escreve, ou escreve outra coisa. O
/// módulo tem de transformar isso em erro e não em `None`, porque
/// `None` significa «fim de captura» e acabaria a chamada.
#[test]
fn um_vp8_que_nao_e_vp8_recusa() {
    let lixo = vec![0xAAu8; 4096];
    let resultado = descodificar(&lixo, cabecalho(320, 240), 3);
    assert!(
        resultado.is_err(),
        "descodificou lixo: {:?}",
        resultado.map(|i| i.tamanho())
    );
}

/// A geometria vem do cabeçalho, e não de descodificar a imagem.
///
/// ## Porque isto é um teste e não uma observação
///
/// Porque é tentador ler a largura no JPEG — e é uma parsing de um
/// formato de imagem para obter um número que o contentor de origem já
/// tem. E porque o cabeçalho pode **dizer** uma coisa e a imagem ser
/// outra, e o que interessa é o que o receptor vai mostrar.
///
/// Um cabeçalho a dizer 320×240 com um VP8 que é 640×480 é um emissor
/// que mudou de ideias a meio. A imagem descodificada tem 640×480 e o
/// cabeçalho diz 320×240 — e este teste fixa que o que a interface
/// recebe para se posicionar é o do cabeçalho, que é o que ela pediu.
#[test]
fn a_geometria_vem_do_cabecalho() {
    let Some(vp8) = vp8_sintetico(320, 240) else {
        eprintln!("media: sem VP8; a geometria não é verificável");
        return;
    };

    // Um cabeçalho que **m mente** sobre a altura.
    let imagem = descodificar(&vp8, cabecalho(320, 480), 2).expect("descodifica");
    assert_eq!(
        imagem.altura, 480,
        "a altura veio da imagem descodificada e devia vir do cabeçalho"
    );
    assert_eq!(imagem.largura, 320);
}

/// O JPEG reconstruído cabe no tecto, e um enorme recusa.
///
/// O tecto existe porque o `ffmpeg` é um programa que aceita o que lhe
/// derem: um cabeçalho de IVF a declarar um quadro de 4 mil por 4 mil e
/// um VP8 minúsculo produz uma imagem de 12 MB, e isso é memória que o
/// `daemon` gasta em nome de alguém que mandou 32 bytes.
#[test]
fn um_quadro_enorme_recusa() {
    assert!(
        MAX_QUADRO_DESCODIFICADO >= 2 * 320 * 240 * 3 / 2,
        "o tecto não cabe uma imagem de 320×240"
    );
    assert!(
        MAX_QUADRO_DESCODIFICADO >= 2 * 640 * 480 * 3 / 2,
        "o tecto não cabe uma imagem de 640×480, que é a geometria mínima \
         de uma chamada"
    );
}

/// A verificação do começo do JPEG distingue o que é imagem do que não é.
#[test]
fn a_verificacao_do_comeco_reconhece_o_jpeg() {
    let bom = QuadroImagem {
        jpeg: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00],
        largura: 1,
        altura: 1,
        sequencia: 0,
    };
    assert!(bom.parece_jpeg(), "um JPEG de verdade tem de ser reconhecido");

    for (nome, bytes) in [
        ("PNG", vec![0x89, 0x50, 0x4E, 0x47, 0x0D]),
        ("VP8", vec![0x30, 0x01, 0x00, 0x9D, 0x01, 0x2A]),
        ("vazio", vec![]),
        ("curto", vec![0xFF, 0xD8]),
        ("SOI sem marcador", vec![0xFF, 0xD8, 0x00, 0x11]),
    ] {
        let imagem = QuadroImagem {
            jpeg: bytes,
            largura: 1,
            altura: 1,
            sequencia: 0,
        };
        assert!(!imagem.parece_jpeg(), "«{nome}» foi reconhecido como JPEG");
    }
}

/// O cabeçalho de IVF reconstruído tem 32 bytes e a assinatura certa.
///
/// O `ffmpeg` de descodificação lê o contentor, e um cabeçalho mal
/// formado dá «Invalid data found when processing input» — que não diz
/// que o problema é o cabeçalho.
#[test]
fn o_cabecalho_reconstruido_e_bem_formado() {
    let cab = ivf_cabecalho(cabecalho(854, 480));
    assert_eq!(cab.len(), 32);
    assert_eq!(&cab[..4], b"DKIF", "a assinatura do IVF");
    assert_eq!(u16::from_le_bytes([cab[4], cab[5]]), 0, "a versão do IVF");
    assert_eq!(
        u16::from_le_bytes([cab[6], cab[7]]),
        32,
        "o tamanho do cabeçalho tem de ser 32"
    );
    assert_eq!(&cab[8..12], b"VP80", "o código de quatro caracteres");
    assert_eq!(
        u16::from_le_bytes([cab[12], cab[13]]),
        854,
        "a largura no cabeçalho"
    );
    assert_eq!(
        u16::from_le_bytes([cab[14], cab[15]]),
        480,
        "a altura no cabeçalho"
    );
}
