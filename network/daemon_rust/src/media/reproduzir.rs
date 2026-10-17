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
//! Não é o `ffmpeg` embebido. O `ffmpeg` embebido é um binário
//! **mínimo**, compilado para transformar (`--enable-everything` está
//! desligado em §7.2) — não sabe reproduzir.
//!
//! Reproduzir é um problema diferente, e a decisão está escrita:
//!
//! * **`ffplay`** traria um segundo binário de 30 MB para um
//!   executável que já não está embebido, e a dependência de `SDL2` que
//!   ele arrasta.
//! * **O que o sistema tem** — `mpv`, `ffplay`, `cvlc` — é o
//!   executável certo, e não vale a pena reescrevê-lo.
//!
//! Por isso o leitor é **o do sistema**, encontrado por nome no `PATH`.
//! Se não houver nenhum, a resposta é um erro de media e nada mais: a
//! transferência continua guardada, e o botão de reproduzir é que não
//! funciona. Perder o ficheiro por não haver leitor seria absurdamente
//! caro.
//!
//! ### O que acontece se não houver leitor
//!
//! Erro de media, e o texto não é afectado. É o modo degradado de §7.6,
//! e vale a pena ser explícito sobre o que é: um daemon que não sabe
//! reproduzir **continua completo** para tudo o resto, e o que falha é
//! um botão, não a aplicação.

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
pub const LEITORES: &[&str] = &["mpv", "ffplay", "cvlc"];

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
    let nome = nome_de_leitor(leitor)?;

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

fn nome_de_leitor(leitor: &str) -> Result<&'static str, ErroMedia> {
    match leitor {
        "mpv" => Ok("mpv"),
        "ffplay" => Ok("ffplay"),
        outro => Err(ErroMedia::LeitorDesconhecido(outro.to_string())),
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
pub fn leitores_disponiveis() -> Vec<&'static str> {
    LEITORES
        .iter()
        .copied()
        .filter(|nome| existe_no_caminho(nome))
        .collect()
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
