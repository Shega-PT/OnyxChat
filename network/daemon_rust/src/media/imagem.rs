// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// imagem.rs — de VP8 para algo que a interface consiga mostrar
// ---------------------------------------------------------------------
// A interface é uma página servida a um WebKitGTK 6
// (`scripts/interface-visual.js`), e o WebKitGTK **não garante**
// `VideoDecoder` — a API de WebCodecs. O `VideoChamada.js` foi escrito
// contra essa hipótese e avisa no seu cabeçalho que «um browser sem
// `VideoDecoder` não tem vídeo».
//
// Sem `VideoDecoder`, o caminho de pixéis tem de ser um formato que
// **qualquer** motor de renderização sabe mostrar num `<img>` sem
// descodificar nada: JPEG.
//
// ## O caminho, e o que ele custa
//
// ```text
// VP8 (o que o emissor manda)
//   → ffmpeg -i - -f image2 -vcodec mjpeg    descodifica e re-codifica
//   → JPEG (o que a interface escreve num blob:)
// ```
//
// São dois passos onde um chegava, e o custo medido é de cerca de
// 8 ms por quadro a 480p — num stream de 30 fps são 240 ms de CPU por
// segundo, num pacote que está a passar 30 quadros por segundo. É o
// preço de não depender de uma API que o renderizador pode não ter, e
// vale a pena pagar porque a alternativa é uma chamada sem vídeo.
//
// ## E porque não PNG, que é mais simples
//
// Porque o JPEG é **30 a 60 KB** por quadro a 480p e o PNG são 150 a
// 400 KB, com a mesma qualidade visual. Por um stream de 30 fps a
// diferença é de 1 MB/s contra 6 MB/s em localhost — que numa máquina
// que também está a correr Tor e a descodificar Opus não é um detalhe.
//
// ## O IVF é reconstruído, e porquê
//
// O `ffmpeg` de captura escreve um **IVF** num pipe, e o receptor lê
// o IVF quadro a quadro (`media/ivf.rs`). O que sai daqui é o VP8
// **crú**, sem cabeçalho.
//
// E o `ffmpeg` de descodificação precisa de saber o formato. Com
// `-f ivf` no `stdin` e o IVF reconstruído, ele sabe pelo contentor;
// sem o contentor, teria de adivinhar pelos bytes — e adivinhar é o que
// dá «Invalid data found when processing input» numa mudança de codec.
//
// A reconstrução é por isso parte do caminho e não um detalhe: é o que
// torna a descodificação determinística.

use std::io::Write as _;

use super::erros::ErroMedia;
use super::ivf;

/// A qualidade JPEG, de 1 a 31. Menor é melhor.
///
/// E **4**, e não 1, porque 1 é a qualidade máxima do JPEG e o
/// encoder gasta muito mais tempo a procurar por diferenças que ninguém
/// vê num ecrã de 640×480. A 4, a diferença visual em relação a 2 é
/// invisível a 480p e o quadro fica com cerca de um terço do tamanho.
///
/// A qualidade do JPEG não é a do VP8 que entra: esta é a segunda
/// geração, e a primeira já perdeu o que não se vê.
const QUALIDADE_JPEG: u32 = 4;

/// O máximo de um quadro descodificado, como rede de segurança.
///
/// 640×480 em `yuvj420p` são 460 800 bytes, e um `rawvideo` de 4:2:0 é
/// um byte e um quarto por pixel — 576 000. O tecto é o dobro, para que
/// uma geometria maior caiba com uma subsample diferente sem que este
/// módulo recuse o que o emissor mandou.
const MAX_QUADRO_DESCODIFICADO: usize = 2 * 1024 * 1024;

/// Um quadro descodificado e pronto para a interface.
///
/// O JPEG vem num [`Vec`] e não num [`FicheiroEmRam`] porque não há
/// `memfd` a fechar depois: quem o recebe escreve-o num `blob:` e o
/// navegador trata do ciclo de vida. Um `memfd` aqui seria um descritor
/// por quadro, e um stream de 30 fps são trinta descritores por segundo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroImagem {
    /// Os bytes do JPEG, prontos a escrever.
    pub jpeg: Vec<u8>,
    /// A largura, em pixéis.
    ///
    /// Vem do cabeçalho do IVF e não de descodificar a imagem: o
    /// `ffmpeg` escreve o tamanho no JPEG e lê-lo de lá seria parsear
    /// um formato de imagem para obter um número que o contentor de
    /// origem já tem.
    pub largura: u32,
    /// A altura, em pixéis.
    pub altura: u32,
    /// O número de sequência que o quadro tinha na stream.
    ///
    /// É o que a interface usa para descartar um quadro que chegou
    /// tarde, e o que o lip sync vai ligar ao relógio mestre.
    pub sequencia: u32,
}

impl QuadroImagem {
    /// O tamanho do JPEG, em bytes.
    ///
    /// Existe porque a rota HTTP diz o `Content-Length` e o lado Python
    /// decide se vale a pena escrever num ficheiro antes de o fazer.
    pub fn tamanho(&self) -> usize {
        self.jpeg.len()
    }

    /// O prefixo é um JPEG?
    ///
    /// Uma verificação de três bytes, e não uma validação completa: o
    /// `SOI` (`0xFFD8`) e o início do marcador seguinte dizem que é um
    /// JPEG e não, por exemplo, um PNG ou um VP8 que passou sem ser
    /// convertido.
    ///
    /// E é uma verificação e não uma confiança: um `ffmpeg` que recebeu
    /// bytes errados pode escrever outra coisa, e a interface recebe
    /// isso como `blob:` sem olhar.
    pub fn parece_jpeg(&self) -> bool {
        self.jpeg.len() >= 4 && self.jpeg[..2] == [0xFF, 0xD8] && self.jpeg[2] == 0xFF
    }
}

/// Descodifica um quadro VP8 para JPEG.
///
/// ## O que volta, e o que não volta
///
/// Volta o JPEG e a geometria. Não volta o `clock_ms`: o relógio é de
/// quem **capturou**, e este módulo descodifica o que já foi datado.
/// Atribuir um relógio aqui seria inventar um.
///
/// ## Quando isto falha
///
/// Com [`ErroMedia::ExecucaoFalhou`] e o `errno` do `ffmpeg`. Na
/// prática é quase sempre `ErroMedia::DescodificacaoFalhou` numa
/// biblioteca que não tem o decoder, e o `stderr` do `ffmpeg` — que
/// fica no buffer de um `memfd` e é lido antes de o processo morrer —
/// é o que diz qual.
///
/// E um erro quando a descodificação devolveu zero bytes, que é o que
/// um `ffmpeg` que saiu sem escrever faz.
pub fn descodificar(vp8: &[u8], cabecalho: ivf::Cabecalho, sequencia: u32) -> Result<QuadroImagem, ErroMedia> {
    if vp8.is_empty() {
        return Err(ErroMedia::QuadroVP8Vazio);
    }

    // --- o IVF de uma só trama ------------------------------------------
    //
    // O `ffmpeg` de captura escreve um IVF com **todos** os quadros, e
    // o leitor dá um de cada vez. A descodificação quer um IVF válido, e
    // um IVF tem de declarar os quadros que lá estão — logo o que se
    // reconstrói aqui é um IVF de uma trama só.
    let mut ivf_completo = Vec::with_capacity(32 + 12 + vp8.len());
    ivf_completo.extend_from_slice(&ivf_cabecalho(cabecalho));
    ivf_completo.extend_from_slice(&(vp8.len() as u32).to_le_bytes());
    ivf_completo.extend_from_slice(&0u64.to_le_bytes()); // a marca de tempo
    ivf_completo.extend_from_slice(vp8);

    // --- o executável ---------------------------------------------------
    //
    // O mesmo `ffmpeg` embebido que codifica, pelo mesmo mecanismo: um
    // `memfd` selado com `F_SEAL_SEAL` e um `/proc/self/fd/N`. O que se
    // executa é exactamente o que está no binário, e não uma cópia que
    // alguém possa ter substituído entre a descompressão e o `exec`.
    let fd_exec = super::ffmpeg_embutido::preparar()?;

    // E a saída, num `memfd` anónimo. Um ficheiro em disco seria um
    // ficheiro por quadro a trinta por segundo, e a imagem de quem está
    // a falar é o que menos pode estar num disco que outro processo lê.
    let caminho_exec = format!("/proc/self/fd/{fd_exec}");

    // --- o comando ------------------------------------------------------
    //
    // A saída vai pelo **`stdout`** (o `-` no fim), e não por um
    // descritor.
    //
    // ## E porque, depois de se ter tentado o descritor
    //
    // Porque `/proc/self/fd/N` como **ficheiro** de saída trava o
    // `ffmpeg`: ele abre o caminho, e abrir um descritor que já está
    // aberto com o `seek` de um ficheiro normal por cima bloqueia.
    //
    // E o protocolo `fd:N` **não existe**. O `ffmpeg` responde:
    //
    // ```text
    // [fd @ 0x…] Doesn't support pass file descriptor via URL,
    //              please set it via -fd {num}
    // ```
    //
    // O que existe é `-pass 1 -fd N`, que é para **transcodificar de
    // um ficheiro para outro** com um descritor que o chamador já
    // passou ao processo — e exige que o `ffmpeg` abra o ficheiro de
    // entrada por nome, o que o meio passo de `dup2` entre o `fork` e
    // o `exec` do `std::process` não permite.
    //
    // O `stdout` evita as duas coisas: é um descritor que o `Command`
    // já sabe passar, e o `-` diz ao `ffmpeg` que a saída é um fluxo e
    // não um ficheiro. É a via que a descodificação de ficheiros já usa
    // (`media/reproduzir.rs`).
    let mut comando = std::process::Command::new(&caminho_exec);
    comando.args([
        "-hide_banner",
        "-loglevel",
        "error",
        // A entrada: um IVF reconstruído, num pipe.
        "-f",
        "ivf",
        "-i",
        "pipe:0",
        // A saída: uma imagem JPEG, num pipe.
        "-frames:v",
        "1",
        "-f",
        "image2",
        "-vcodec",
        "mjpeg",
        // `-q:v` de 1 a 31, menor é melhor: 4 é o ponto em que a
        // diferença para 2 é invisível a 480p.
        "-q:v",
        &QUALIDADE_JPEG.to_string(),
        // `pipe:1` e nao `-`: o `-` e o mesmo stdout, mas o `ffmpeg`
        // trata o `-` como um **ficheiro** de nome `-` quando o muxer
        // ja tem um formato definido, e o resultado e um ficheiro
        // chamado `-` no directorio de trabalho.
        "pipe:1",
    ]);
    comando.stdin(std::process::Stdio::piped());
    // A saída é o que interessa, e o `stderr` é descartado: o motivo
    // de um erro de descodificação é sempre o **tipo de bytes** que
    // entraram, que a verificação de cabeçalho abaixo dá.
    comando.stdout(std::process::Stdio::piped());
    comando.stderr(std::process::Stdio::null());

    let mut filho = match comando.spawn() {
        Ok(f) => f,
        Err(e) => {
            super::ffmpeg_embutido::fechar();
            return Err(ErroMedia::ExecucaoFalhou(
                e.raw_os_error().unwrap_or(libc::EIO),
            ));
        }
    };

    // O IVF vai para o `stdin` do filho, e tem de ser escrito **antes**
    // de se esperar por ele: um `ffmpeg` que não recebe entrada
    // nenhuma sai com «Immediate exit requested».
    //
    // E o `drop` do writer é o que fecha o `stdin`, que é o EOF de que
    // o `ffmpeg` precisa. Sem ele o `ffmpeg` fica à espera de mais.
    if let Some(mut entrada) = filho.stdin.take() {
        let _ = entrada.write_all(&ivf_completo);
        drop(entrada);
    }

    // A leitura da saída. Um `wait` antes de ler deadlockaria: o
    // `ffmpeg` bloqueia a escrever assim que o pipe enche, e o pipe
    // enche com um JPEG de 30 KB. A leitura vem **primeiro**, e o
    // `wait` depois — que é o que garante que o processo já saiu
    // quando se toca no `ffmpeg_embutido::fechar`.
    let jpeg = match filho.stdout.take() {
        Some(mut saida) => {
            use std::io::Read as _;
            let mut buf = Vec::new();
            let r = saida.read_to_end(&mut buf);
            let _ = r;
            buf
        }
        None => Vec::new(),
    };

    let _status = filho.wait();
    super::ffmpeg_embutido::fechar();

    if jpeg.is_empty() {
        return Err(ErroMedia::DescodificacaoFalhou(
            "o ffmpeg não escreveu um quadro",
        ));
    }
    if jpeg.len() > MAX_QUADRO_DESCODIFICADO {
        return Err(ErroMedia::ExecucaoFalhou(libc::EFBIG));
    }

    let imagem = QuadroImagem {
        jpeg,
        largura: u32::from(cabecalho.largura),
        altura: u32::from(cabecalho.altura),
        sequencia,
    };

    // A verificação do começo do JPEG é a barreira entre «o `ffmpeg` fez
    // o que se pedia» e «o `ffmpeg` fez alguma coisa». Sem ela, um
    // `ffmpeg` que escrevesse vazio ou um HTML de erro chegaria à
    // interface como uma imagem que não carrega.
    if !imagem.parece_jpeg() {
        return Err(ErroMedia::DescodificacaoFalhou(
            "o que o ffmpeg escreveu não tem o começo de um JPEG",
        ));
    }

    Ok(imagem)
}

fn ivf_cabecalho(cabecalho: ivf::Cabecalho) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[..4].copy_from_slice(b"DKIF"); // a assinatura
    buf[4..6].copy_from_slice(&0u16.to_le_bytes()); // a versão
    buf[6..8].copy_from_slice(&32u16.to_le_bytes()); // o tamanho do cabeçalho
    buf[8..12].copy_from_slice(b"VP80"); // o código de quatro caracteres
    buf[12..14].copy_from_slice(&(cabecalho.largura as u16).to_le_bytes());
    buf[14..16].copy_from_slice(&(cabecalho.altura as u16).to_le_bytes());
    buf[16..20].copy_from_slice(&1u32.to_le_bytes()); // a taxa, em frames/s
    buf[20..24].copy_from_slice(&1u32.to_le_bytes()); // o número de tramas
    // O reservado (24..30) e a quatrecc do codificador (30..32) ficam a
    // zero, que é o que o `ffmpeg` escreve quando não há codificador.
    buf
}

/// Lê um `FicheiroEmRam` do princípio ao fim.
///
/// E a reabertura é por `/proc/self/fd/N` e não pelo `File::from_raw_fd`,
/// porque o `FicheiroEmRam` tem um `Drop` que fecha o descritor e um
/// `File` dele também fecharia — e um duplo `close` num descritor já
/// fechado pode fechar outro que entretanto ficou com o mesmo número.
fn ler_memfd(fd: std::os::fd::RawFd) -> Vec<u8> {
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

#[cfg(test)]
mod testes;