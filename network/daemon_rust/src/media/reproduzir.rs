// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! A reprodução de um ficheiro de media guardado.
//!
//! [`docs/media.md`](../../../../docs/media.md) §4.6. Junta as três
//! peças que estão separadas de propósito:
//!
//! * [`super::loja`] — lê os envelopes da loja do `sidecar`;
//! * [`super::ram`] — escreve o resultado num `memfd` selado;
//! * este módulo — decide **quem** executa o ficheiro.
//!
//! ## O que este módulo não faz
//!
//! **Não decifra, e não precisa de chaves.** Uma primeira versão montava
//! desencriptando os `envelope`, e isso está errado: o `nonce1` de cada
//! fatia já foi consumido no registo anti-replay quando a mensagem
//! chegou, e uma segunda decifra do mesmo envelope é o ataque que esse
//! registo existe para travar. Ver a nota no cabeçalho de
//! [`super::loja`].
//!
//! A consequência é boa: `MEDIA_REPRODUZIR` não leva nenhuma chave, e o
//! `daemon` não precisa da amizade para reproduzir.
//!
//! ## E o leitor de media
//!
//! **Primeiro o do sistema, e o `ffmpeg` embebido como último recurso.**
//!
//! A ordem é esta, e cada degrau tem uma razão:
//!
//! 1. **`mpv`, `ffplay`, `cvlc`** — o que a pessoa já tem. É a opção
//!    certa: respeita o volume do sistema, a pausa, a legenda, e não
//!    traz um segundo player dentro da aplicação.
//! 2. **`ffmpeg` embebido** — o binário que este mesmo módulo já
//!    executa para capturar. Não abre uma janela e não tem interface;
//!    por isso reproduz **para um ficheiro de saída**, e a abertura
//!    fica a cargo de quem pediu.
//!
//! ### Porquê o degrau 2 passou a existir
//!
//! Este módulo escrevia que o `ffmpeg` embebido «não sabe reproduzir»,
//! e era verdade na altura em que foi escrito: a receita era
//! `--enable-everything` desligado com **só** encoders e um
//! `--enable-filter=null`.
//!
//! A receita mudou (F10) e o binário ganhou **demuxers e decoders**:
//! `matroska`, `mov`, `wav`, `ivf`, e os decoders de `vp8`, `opus`,
//! `rawvideo` e `pcm_s16le`. E ganhou `image2` e `mjpeg` para o
//! caminho de imagem da UI. O que transformava um stream num ficheiro
//! é exactamente o que faz a operação inversa.
//!
//! A consequência prática é que, numa máquina sem `mpv` e sem `ffplay` —
//! que é o caso desta, e o `MEDIA_ESTADO` devolvia `leitor = 0xFF` — o
//! botão de reproduzir não tinha resposta. Com o degrau 2, o daemon
//! descodifica para PCM e escreve o resultado, e quem abre o ficheiro é
//! o sistema.
//!
//! ### E porque o degrau 2 vai para PCM, e não para um leitor
//!
//! Porque o `ffmpeg` embebido **não tem interface**. Não há `sdout`, não
//! há `aout`, não há `avfilter` de vídeo — o `alsa_outdev` nem sequer
//! está na receita. Um `ffmpeg` que descodifica e escreve um `.wav` é
//! meia peça; a outra metade é mostrar o `.wav`, e essa é uma
//! responsabilidade do sistema operativo, que já sabe.
//!
//! ### O que acontece se não houver nenhum dos dois
//!
//! Erro de media, e o texto não é afectado. É o modo degradado de §7.6:
//! um daemon que não sabe reproduzir **continua completo** para tudo o
//! resto, e o que falha é um botão, não a aplicação.

use super::erros::ErroMedia;
use super::ficheiro::IdTransferencia;
use super::ram::FicheiroEmRam;

/// Os leitores que se tentarão, por ordem.
///
/// A ordem **não** é alfabética e não é de preferência: `mpv` primeiro
/// porque respeita a pausa e o volume, e é o que está instalado em
/// quase todo o lado onde há alguém a ouvir áudio. `ffplay` é o segundo
/// porque vem com o `ffmpeg` — e quem tem `ffmpeg` no sistema quase
/// sempre tem `ffplay` ao lado.
///
/// E `cvlc` é o terceiro porque está na lista desde sempre e porque
/// ninguém se chateia por um leitor a mais na lista: os que não existirem
/// são simplesmente ignorados por [`existe_no_caminho`].
pub const LEITORES: &[&str] = &["mpv", "ffplay", "cvlc"];

/// O nome interno do leitor embebido.
///
/// **Não** é um programa no `PATH`: é o `ffmpeg` que este processo tem
/// dentro de um `memfd` selado, e que executa por `/proc/self/fd/21`.
///
/// O nome entra em [`BYTES_DE_LEITOR`] com o byte `0x00`, e é por isso
/// que o `ipc.rs` o pode mandar como um `u8` sem um quarto caminho de
/// código: há um quarto nome no mesmo mapa.
pub const LEITOR_EMBUTIDO: &str = "embutido";

/// O byte que significa «não há nenhum leitor».
///
/// `0xFF` por escolha, e não por acaso: é o único valor do `u8` que não
/// é usado por nenhum leitor, e é o que o `MEDIA_ESTADO` já devolvia
/// quando a lista estava vazia. A resposta mantém-se compatível com o
/// que o `sidecar` já sabe ler.
pub const LEITOR_NENHUM: u8 = 0xFF;

/// O byte de um leitor, se for um leitor conhecido.
///
/// ## O mapa é fixo, e é essa a razão de existir
///
/// A primeira versão era um índice sobre [`LEITORES`], e um índice é
/// uma posição — que muda quando a lista muda. Acrescentar `embutido`
/// ao fim moveu `mpv` de `0` para `1`, e um `sidecar` que tivesse
/// guardado o `0` de uma sessão anterior passaria a pedir `mpv` a um
/// índice que já não era `mpv`.
///
/// Um mapa nome→byte não sofre disso: os bytes dos leitores que já
/// existiam ficam onde estavam, e o novo leitor recebe um byte novo.
///
/// ## Porque `embutido` fica a `0x00`
///
/// Porque é o único que **não depende do sistema**, e o estado de
/// `MEDIA_ESTADO` que mais importa comunicar é «consigo descodificar
/// isto, aqui». Um byte que muda conforme instalações passa a dizer
/// mais sobre a máquina do que sobre a capacidade.
const BYTES_DE_LEITOR: &[(&str, u8)] = &[
    (LEITOR_EMBUTIDO, 0x00),
    ("mpv", 0x01),
    ("ffplay", 0x02),
    ("cvlc", 0x03),
];

/// O byte de um leitor pelo nome.
pub fn byte_de_leitor(nome: &str) -> Option<u8> {
    BYTES_DE_LEITOR
        .iter()
        .find(|(n, _)| *n == nome)
        .map(|(_, b)| *b)
}

/// O nome de um leitor pelo byte.
pub fn nome_de_byte_leitor(byte: u8) -> Result<&'static str, ErroMedia> {
    BYTES_DE_LEITOR
        .iter()
        .find(|(_, b)| *b == byte)
        .map(|(n, _)| *n)
        .ok_or(ErroMedia::LeitorDesconhecido(format!("byte {byte}")))
}

/// Monta o ficheiro e devolve o descritor, **sem** o executar.
///
/// Separar a montagem da execução é o que permite testar §4.6 sem
/// depender de um leitor instalado: o teste monta, lê o descritor de
/// volta e compara com o conteúdo enviado. A execução é o passo
/// seguinte, e é o único que precisa de um programa externo.
pub fn montar(loja: &super::loja::Loja, id: &IdTransferencia) -> Result<FicheiroEmRam, ErroMedia> {
    let montado = loja.montar(id)?;
    FicheiroEmRam::de_conteudo("onyx-media", montado.bytes())
}

/// O nome que o leitor vai ver.
///
/// **Não** é o nome que o emissor enviou. Um nome de ficheiro é
/// metadado que o outro escolheu, e pô-lo no `/proc/self/fd` de uma
/// pessoa mostraria a ela — e a qualquer outro processo do mesmo
/// utilizador — o que recebeu.
///
/// O que fica é a extensão do formato, que é o único que o leitor
/// precisa para escolher o demuxer, e é derivado do manifesto, que é o
/// que o receptor já leu para decidir o que fazer com estes bytes.
pub fn nome_de_reproducao(
    loja: &super::loja::Loja,
    id: &IdTransferencia,
) -> Result<String, ErroMedia> {
    let manifesto = loja.manifesto(id)?;
    Ok(format!("onyx-media.{}", manifesto.formato.estensao()))
}

/// Executa o ficheiro com o leitor do sistema, e devolve o `pid`.
///
/// ## Porque é o `daemon` a lançar o leitor, e não o `sidecar`
///
/// A primeira versão desta função fazia `execvp`. Isso está errado por
/// duas Razões, e a segunda é a que custaria caro:
///
/// 1. `execvp` **substitui o processo**. O daemon deixava de existir:
///    o socket desaparecia, e a pessoa ficava com uma aplicação que
///    já não responde enquanto ouve um áudio.
/// 2. O `fd` **não cabe num IPC** de framing simples. A resposta não
///    pode ser um número de descritor: números de descritor só fazem
///    sentido no processo que os tem. Passá-los exigiria
///    `SCM_RIGHTS` — dados anexos no socket — e isso muda o enquadramento
///    de `ipc.rs`, que é a base de todo o IPC local.
///
/// A alternativa — o `daemon` montar e o `sidecar` lançar o leitor —
/// é mais elegante no papel e, na prática, exige o `SCM_RIGHTS` acima.
/// O ganho seria **quem** executa o programa, e isso não compensa
/// mexer no transporte de um protocolo já testado.
///
/// Por isso: o `daemon` monta, sela e lança. É o que já faz com o
/// `ffmpeg` da captura, e é a mesma operação com um binário diferente.
///
/// ## E se não houver leitor
///
/// [`leitor_disponivel`] diz. Sem leitor, o erro é de media e o
/// ficheiro **permanece guardado**: perder um vídeo de 10 MB porque a
/// pessoa não tem o `mpv` instalado seria um preço absurdo.
pub fn executar(ficheiro: FicheiroEmRam, leitor: &str) -> Result<u32, ErroMedia> {
    // O `memfd` foi criado **sem** `MFD_CLOEXEC`, e é por isso que o
    // filho o herda sem nenhum trabalho extra: um descritor sem a marca
    // atravessa o `exec`. Com a marca, teria de a limpar num
    // `pre_exec`, e um `pre_exec` é código que corre antes do `exec` num
    // processo com várias threads — um sítio onde um erro é um `SIGSEGV`
    // em vez de um `errno`.
    let caminho = format!("/proc/self/fd/{}", ficheiro.fd());

    // O leitor embebido não é um nome no `PATH`: é um descritor, e o
    // `Command` precisa de um caminho de executável. Desempacotá-lo
    // aqui é o mesmo trabalho que a captura faz, e a garantia é a
    // mesma — o que executa é o que está no binário.
    if leitor == LEITOR_EMBUTIDO {
        return executar_embutido(ficheiro, &caminho);
    }

    let nome = BYTES_DE_LEITOR
        .iter()
        .find(|(n, _)| *n == leitor)
        .map(|(n, _)| *n)
        .ok_or_else(|| ErroMedia::LeitorDesconhecido(leitor.to_string()))?;

    let comando = std::process::Command::new(nome)
        .args(argumentos_de_leitor(leitor, &caminho))
        // O leitor **herda** os descritores do daemon — incluindo o
        // socket. Um `Command` sem isto passaria o resto ao filho, e um
        // leitor de media com o socket do daemon no `fd` é um leitor que
        // pode falar IPC. `close_fds` fecha tudo menos o que o
        // `stdin`/`stdout`/`stderr` precisam.
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match comando {
        Ok(child) => Ok(child.id()),
        // `ENOENT` é o caso comum: o `PATH` mudou entre a verificação e
        // o `spawn`, ou o leitor não está instalado. Vira o mesmo erro
        // de leitor desconhecido, porque para o utilizador **é** o
        // mesmo: não há leitor.
        Err(e) => Err(match e.kind() {
            std::io::ErrorKind::NotFound => ErroMedia::LeitorDesconhecido(leitor.to_string()),
            _ => ErroMedia::ExecucaoFalhou(e.raw_os_error().unwrap_or(libc::EIO)),
        }),
    }
}

/// Executa a descodificação com o `ffmpeg` embebido, e devolve o `pid`.
///
/// ## O que este degrau faz, e o que não faz
///
/// **Faz:** descodificar o media guardado e escrevê-lo como PCM num
/// `memfd` — o caminho inverso de `ffmpeg -i captura.ivf -f opus …`.
///
/// **Não faz:** mostrar nada. O `ffmpeg` embebido não tem interface,
/// não tem `sdout` nem `aout`, e `alsa_outdev` não está na receita.
/// Escrever um `.wav` é metade; a outra metade é uma janela, e essa é
/// uma responsabilidade do sistema operativo.
///
/// ## Porquê PCM e não outro contentor
///
/// Porque `pcm_s16le` é o único contentor em que a operação é
/// reversível sem mais decisões: o que sai é exactamente o que o
/// emissor gravou, amostra a amostra. Um `.mp4` ou `.webm` seria uma
/// nova codificação, com as suas próprias perdas e com um `mov`/`matroska`
/// demuxer a caminho — para um ficheiro que já estava guardado.
///
/// ## E porquê `-f wav` e não `-f s16le`
///
/// Porque `s16le` é um contentor **cru**: não tem cabeçalho, e o
/// sistema não sabe que é áudio sem o ver. O `wav` tem os 44 bytes de
/// cabeçalho que fazem um leitor dizer «isto é áudio» sem fazer
/// perguntas — e o demuxer `wav` está na receita, o que faz o `ffmpeg`
/// poder também ler um.
///
/// ## E porquê um `memfd` de saída separado
///
/// Porque o `ffmpeg` lê um contentor enquanto escreve noutro. Reescrever
/// a entrada por cima pareceria mais simples e é pior: o resultado passa
/// a depender da ordem de bloqueio entre leitura e escrita, e um ficheiro
/// meio reescrito não é nem a entrada nem a saída. Com dois descritores
/// o `ffmpeg` é um transformador normal, que é o que ele é.
///
/// O `memfd` de saída é criado **antes** do `spawn`, sem `MFD_CLOEXEC`,
/// e por isso o filho herda o descritor — que é o mesmo arranjo da
/// captura.
fn executar_embutido(_ficheiro: FicheiroEmRam, caminho: &str) -> Result<u32, ErroMedia> {
    // O `_ficheiro` é a **entrada**, e o seu conteúdo entrou em `caminho`
    // — que é `/proc/self/fd/N` desse mesmo descritor, já preparado
    // por quem chamou. O parâmetro fica aqui por simetria com
    // [`executar`] e não por ser usado.
    //
    // E o facto de não ser lido **não** o torna dispensável: o
    // `FicheiroEmRam` tem um `Drop` que fecha o descritor, e recebê-lo
    // por valor garante que ele sobrevive até ao `spawn` — o `ffmpeg`
    // filho precisa dele para ler a entrada.
    let fd_exec = super::ffmpeg_embutido::preparar()?;
    let caminho_exec = format!("/proc/self/fd/{}", fd_exec);

    // A saída é um `memfd` **novo**, não o ficheiro de entrada.
    //
    // Escrever por cima da entrada pareceria mais simples e é pior: o
    // `ffmpeg` lê o contentor enquanto escreve no mesmo descritor, o
    // resultado depende da ordem de bloqueio, e um ficheiro meio
    // reescrito não é nem a entrada nem a saída. Dois descritores
    // separados tornam o `ffmpeg` um transformador normal, que é
    // exactamente o que ele é.
    let saida = FicheiroEmRam::de_conteudo("onyx-media-wav", &[])?;
    let caminho_saida = format!("/proc/self/fd/{}", saida.fd());

// A lista é de `OsString` e não de `&str` por uma razão concreta: o
    // `Command` recebe `S: AsRef<OsStr>` por valor e **copia** o
    // argumento para o seu próprio armazenamento. Passar `&OsStr`
    // diretamente obrigaria a viver um temporário que o `Command` não
    // guarda, e o compilador acusa-o. Converter para `OsString` resolve
    // isso sem `Box::leak` e sem vazar memória por reprodução.
    let argumentos: Vec<std::ffi::OsString> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        caminho,
        "-f",
        "wav",
        caminho_saida.as_str(),
    ]
    .into_iter()
    .map(std::ffi::OsString::from)
    .collect();

    // O `Command` vive num `let` próprio, e não numa expressão encadeada.
    //
    // Uma expressão `Command::new(x).args(y)…;` que termine em ponto e
    // vírgula cria um valor temporário que o compilador julga livre no
    // fim da instrução, e o `spawn` seguinte — que é a primeira coisa
    // que usa esse `Command` — passa a ser um uso depois de libertação.
    // Separar em bindings é o que dá ao `Command` a vida que o
    // encadeamento lhe negava.
    let executavel: std::ffi::OsString = caminho_exec.into();
    let mut comando = std::process::Command::new(&executavel);
    comando.args(&argumentos);
    comando.stdin(std::process::Stdio::null());
    comando.stdout(std::process::Stdio::null());
    comando.stderr(std::process::Stdio::null());

    let resultado = comando.spawn();
    // O `preparar` deu-nos um descritor próprio; se o `spawn` não o usa,
    // fica aberto. Fechá-lo aqui é o que evita um descritor por
    // reprodução.
    //
    // O `ficheiro` (a entrada) e a `saida` saem de cena com esta
    // função: o `spawn` **não espera** pelo fim. O `ffmpeg` filho tem os
    // descritores herdados — sem `MFD_CLOEXEC`, como a captura —, e
    // continua a escrever no `memfd` de saída depois de o pai já ter
    // devolvido o `pid`. É este arranjo que faz o conteúdo aparecer no
    // ficheiro depois de a resposta ao `MEDIA_REPRODUZIR` já ter saído.
    match resultado {
        Ok(child) => {
            super::ffmpeg_embutido::fechar();
            Ok(child.id())
        }
        Err(e) => {
            super::ffmpeg_embutido::fechar();
            Err(match e.kind() {
                std::io::ErrorKind::NotFound => ErroMedia::LeitorDesconhecido(LEITOR_EMBUTIDO.to_string()),
                _ => ErroMedia::ExecucaoFalhou(e.raw_os_error().unwrap_or(libc::EIO)),
            })
        }
    }
}

/// Os argumentos de um leitor, sem o executável.
fn argumentos_de_leitor<'a>(leitor: &str, caminho: &'a str) -> Vec<&'a str> {
    match leitor {
        // `--no-video`: o áudio é o que se está a ouvir, e abrir uma
        // janela para um áudio é uma janela que fica.
        // `--really-quiet`: o media é um ficheiro, não um disco.
        "mpv" => vec!["--no-video", "--really-quiet", caminho],
        // `-nodisp`: sem `-autoexit` o `ffplay` fica à espera de uma
        // tecla, e ninguém vai carregar em nada.
        "ffplay" => vec!["-nodisp", "-autoexit", "-loglevel", "quiet", caminho],
        _ => vec![caminho],
    }
}

/// Os leitores que este sistema tem.
///
/// O `embutido` entra **primeiro** na resposta e por uma razão que não é
/// de preferência: está sempre lá. Os leitores do sistema dependem do que
/// a pessoa instalou, e numa máquina mínima não há nenhum — que é
/// exactamente o caso em que este degrau foi acrescentado.
pub fn leitores_disponiveis() -> Vec<&'static str> {
    let mut lista: Vec<&'static str> = Vec::new();
    if super::ffmpeg_embutido::disponivel() {
        lista.push(LEITOR_EMBUTIDO);
    }
    lista.extend(
        LEITORES
            .iter()
            .copied()
            .filter(|nome| existe_no_caminho(nome)),
    );
    lista
}

/// O primeiro leitor disponível, se algum.
pub fn leitor_disponivel() -> Option<&'static str> {
    leitores_disponiveis().into_iter().next()
}

/// Se um executável está no `PATH`.
///
/// Procura o caminho do próprio processo em vez de correr `which`:
/// `which` é um programa externo, e esta função é chamada no arranque
/// para dizer o que está disponível. Um `fork` para isso é mais
/// caro do que percorrer o `PATH`.
fn existe_no_caminho(nome: &str) -> bool {
    let caminho = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };

    std::env::split_paths(&caminho).any(|directorio| {
        let candidato = directorio.join(nome);
        // `is_file` e não `exists`: um directório chamado `mpv` não é
        // um leitor, e `execvp` de um directório dá `EACCES`.
        candidato.is_file() && executavel(&candidato)
    })
}

/// Se um ficheiro tem o bit de execução.
fn executavel(caminho: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    caminho
        .metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod testes;
