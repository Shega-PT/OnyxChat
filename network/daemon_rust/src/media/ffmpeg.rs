// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/ffmpeg.rs — lançar o `ffmpeg` com dois canais de saída
// ---------------------------------------------------------------------
// O `ffmpeg` escreve o **vídeo** no descritor 1 e o **áudio** no
// descritor 3. O 3 é o que a proposta chama de canal `EXTRAFILES`, e é
// o segundo canal de que `ffmpeg` precisa porque dois `-f … -` escrevem
// ambos no stdout — e um pipe com vídeo e áudio entrelaçados é
// indistinguível de um stream sem separadores.
//
// ## Porque `pre_exec` e não `Stdio`
//
// Porque o [`std::process::Stdio`] só sabe ocupar 0, 1 e 2. O
// descritor 3 é o terceiro canal, e a única forma de o preparar é um
// `dup2` entre o `fork` e o `exec` — que é exactamente para isso que
// existe `std::os::unix::process::CommandExt::pre_exec`.
//
// E é por isso que um `dup2` falhado é **fatal**: se falhar em
// silêncio, o descritor 3 aponta para outra coisa, o áudio vai para o
// sítio errado, e o sintoma é um stream sem som — que parece um
// problema de codec e é um problema de aritmética de descritores. Por
// isso o `pre_exec` devolve o erro, e o Rust mata o filho antes do
// `exec` em vez de o deixar correr com o 3 errado.
//
// ## Porque o áudio tem uma thread própria
//
// Porque os dois pipes são bloqueantes e nenhum pode bloquear o outro.
// O vídeo é o que o anel consome (`super::anel`); o áudio é o que faz
// o relógio avançar (`super::relogio`), e **o relógio não pode esperar
// pelo vídeo**. Uma thread que lê o áudio e deposita num canal é o que
// garante que o relógio avança mesmo quando o quadro de vídeo ainda não
// chegou.
//
// ## Porque o canal do áudio é bloqueante
//
// Porque o áudio **não pode ser descartado** (`docs/media.md` §7.2).
// Um `try_send` que falhasse deixaria um buraco no relógio mestre, e o
// receptor passaria a apresentar vídeo sem som — que é pior do que
// silêncio. O áudio é pequeno (4 KB/s em Opus): bloquear é mais barato
// do que perder.
// =====================================================================

use std::io::{self, Read};
use std::os::unix::io::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};

use super::erros::ErroMedia;
use super::ffmpeg_embutido as embutido;
use super::moldura::Modo;

/// O descritor de saída do áudio dentro do filho.
///
/// O 3 porque é o primeiro livre depois de 0, 1 e 2, e porque é o que
/// a proposta chama de canal `EXTRAFILES`. O porquê de o executável
/// **não** estar aqui está em [`embutido::SLOT_EMBUTIDO`].
pub const FD_AUDIO_FILHO: RawFd = 3;

/// A profundidade do canal entre a thread do áudio e o anel.
///
/// Baixa de propósito. O áudio é pequeno — 4 KB/s em Opus — e um canal
/// fundo esconderia o atraso do áudio atrás do do vídeo, que é
/// exactamente o atraso que o relógio não pode ter
/// (`docs/media.md` §5.1).
const PROFUNDIDADE_CANAL_AUDIO: usize = 8;

/// O tamanho de um pedaço de áudio, em bytes: 20 ms a 4 KB/s.
///
/// 20 ms é o tamanho de frame nativo do Opus e o que
/// `docs/media.md` §5.3 assume. Não é um valor arbitrário: um pedaço
/// menor introduz latência de buffer no leitor, e um maior ocupa o anel
/// sem dar granularidade útil — porque o anel do áudio conta bytes, não
/// pedacos (`docs/media.md` §5.2).
const TAMANHO_PEDACO_AUDIO: usize = 8_192;

/// A geometria de captura pedida ao `ffmpeg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Captura {
    /// A largura do quadro, em pixels. Múltiplo de 16.
    pub largura: u16,
    /// A altura do quadro, em pixels. Múltiplo de 16.
    pub altura: u16,
    /// Os quadros por segundo pedidos à fonte.
    pub fps: u16,
    /// A taxa de bits do áudio, em kbps.
    pub audio_kbps: u16,
    /// A taxa de bits do vídeo, em kbps.
    ///
    /// ## Porque é um campo e não uma constante
    ///
    /// Porque é o **contrato** entre emissor e receptor. O VP8 vai
    /// codificar a este valor e o receptor vai dimensionar o anel por
    /// ele; se os dois lados tivessem o número em constantes
    /// diferentes, o desvio seria uma fila que cresce sem ninguém dar
    /// por isso.
    ///
    /// E é o número que decide se a chamada cabe: §3.4.2 mediu 854×480
    /// a 300 kbit/s em 55 % do orçamento, com o áudio incluído.
    pub video_kbps: u16,
}

impl Captura {
    /// Valida a geometria e as taxas.
    ///
    /// ## Os múltiplos de 16 saíram daqui
    ///
    /// Uma versão anterior exigia que a largura e a altura fossem
    /// múltiplos de 16, e a razão era a grelha de telhas de §6.2: uma
    /// largura que não fosse múltiplo deixava a última coluna de telhas
    /// fora do quadro, e o receptor reconstruía um quadro de largura
    /// diferente da que o emissor enviou — um deslize que se acumula, e
    /// não um erro.
    ///
    /// Com **VP8** essa grelha não existe. O quad é uma imagem de
    /// `largura × altura` e o codec trata do resto, e exigir múltiplos
    /// de 16 rejeitaria resoluções perfectly válidas como 640×360 ou
    /// 1280×720.
    ///
    /// O que fica é o que o VP8 não perdoa: um quadro de lado zero, e
    /// uma taxa a zero — que produziria um `ffmpeg` que corre e não
    /// transmite nada.
    pub fn validar(&self) -> Result<(), ErroMedia> {
        if self.largura == 0 || self.altura == 0 {
            return Err(ErroMedia::GeometriaInvalida("quadro de lado zero"));
        }
        if self.fps == 0 {
            return Err(ErroMedia::GeometriaInvalida("fps zero"));
        }
        if self.audio_kbps == 0 {
            return Err(ErroMedia::GeometriaInvalida("taxa de áudio zero"));
        }
        if self.video_kbps == 0 {
            return Err(ErroMedia::GeometriaInvalida("taxa de vídeo zero"));
        }
        Ok(())
    }

    /// O tamanho de um quadro de luma, em bytes.
    ///
    /// Um byte por pixel: `-pix_fmt gray` (`docs/media.md` §6.1). É esta
    /// função que fecha a possibilidade de emissor e receptor
    /// discordarem do tamanho do quadro — e é a única coisa que
    /// impede um [`CapturaViva::ler_quadro`] de ler para além do fim.
    pub fn tamanho_do_quadro(&self) -> usize {
        usize::from(self.largura) * usize::from(self.altura)
    }

    /// Os bytes de áudio por milissegundo, para o relógio.
    ///
    /// `docs/media.md` §5.3: `bytes_por_ms = bitrate_bps / 8000`. Com
    /// `-vbr off`, isto é exacto — e é por isso que o `-vbr off` não é
    /// um ajuste de qualidade.
    pub fn bytes_por_ms(&self) -> u16 {
        u16::try_from(u32::from(self.audio_kbps) * 1_000 / 8_000).unwrap_or(u16::MAX)
    }
}

/// O `ffmpeg` a correr, com os pipes de saída.
///
/// O [`Drop`] mata o filho. Não é gentileza: um `ffmpeg` vivo depois de
/// a stream parar é um `x11grab` que continua a ler o ecrã sem que
/// ninguém consuma, e `MEDIA_INICIAR` repetido dez vezes deixa dez
/// leitores de ecrã a competir pela mesma fonte.
pub struct CapturaViva {
    filho: Child,
    /// O leitor de IVF, que é o que sabe onde acaba um quadro.
    ///
    /// Não é um `ChildStdout` cru: desde F9 o `ffmpeg` escreve **VP8
    /// em IVF** (§8.1), e um quadro tem comprimento variável — o VP8
    /// põe no quadro só o que mudou, e um frame-P de ecrã parado tem
    /// dezenas de bytes enquanto um frame-I tem milhares. Ler
    /// `largura × altura` bytes por quadro dava um quadro deslocado a
    /// partir do primeiro.
    ///
    /// O leitor também valida o cabeçalho antes de a captura ser
    /// declarada viva, e recusa um stream que não seja VP8 — em vez de
    /// decodificar silêncio.
    video: super::ivf::LeitorIvf<std::process::ChildStdout>,
    /// A geometria pedida, que o consumidor precisa para saber o que
    /// esperar de cada quadro.
    pub captura: Captura,
    /// A sequência do quadro que vai ser lido a seguir.
    ///
    /// **Não** vem do IVF: o `ffmpeg` não escreve uma, e o receptor
    /// precisa de uma para saber que um quadro se perdeu sem esperar
    /// pelo timestamp — que é o mesmo tempo, dito de duas maneiras.
    ///
    /// Um contador local e não o `timestamp` porque o `timestamp` é o
    /// tempo de **produção** e a sequência é a ordem de **transporte**.
    /// Um quadro pode chegar fora de ordem e continuar a ser o mesmo
    /// instante, e é a ordem que o receptor precisa.
    pub(crate) sequencia: u32,
    /// O relógio de media, para carimbar cada quadro.
    ///
    /// É o §5.3: o tempo viaja com o quadro, e o receptor compara-o
    /// com o seu relógio contado. Um quadro sem tempo é um quadro que
    /// o receptor não sabe quando mostrar.
    pub(crate) relogio: super::relogio::RelogioMedia,
}

impl Stream {
    /// As últimas linhas do `stderr`, para um diagnóstico.
    ///
    /// ## Porque só metadados e nunca o `stderr` inteiro
    ///
    /// O `ffmpeg` escreve ali caminhos, nomes de dispositivo e
    /// mensagens do sistema. `docs/security_model.md` não quer conteúdo
    /// de outra máquina em registos — e um `stderr` copiado para um
    /// erro que vai para um log é exactamente isso.
    ///
    /// O que fica são as últimas [`LINHAS_DIAGNOSTICO`], e quem as lê é o
    /// mesmo daemon, para responder a quem pediu a captura.
    pub fn diagnostico(&self) -> Vec<String> {
        match self.diagnostico.lock() {
            Ok(g) => g.clone(),
            // Um mutex envenenado significaria que uma thread morreu com
            // o panic dentro. Perder o diagnóstico é melhor do que
            // propagar o pânico para a do IPC.
            Err(_) => Vec::new(),
        }
    }
}

impl CapturaViva {
    /// Lê o próximo quadro de vídeo, em **bytes de VP8**.
    ///
    /// ## O que volta, e o que não volta
    ///
    /// Volta o quadro **comprimido**, como o `ffmpeg` o escreveu. Não
    /// volta um quadro de pixéis: decodificar VP8 é trabalho de um
    /// decodificador, e o daemon embebido só tem o *encoder* — o
    /// `build.sh` liga `libvpx` para codificar, e ligar também para
    /// decodicar duplicaria o tamanho do executável por uma coisa que o
    /// emissor não precisa.
    ///
    /// É o receptor que decodifica, e é o receptor que tem o `ffmpeg`
    /// completo. O daemon transporta; não deserializa.
    ///
    /// E é por isso que o quadro não tem tamanho fixo: um frame-P de
    /// ecrã parado tem algumas dezenas de bytes, um frame-I tem
    /// milhares, e um leitor que assumisse um tamanho daria um quadro
    /// deslocado a partir do primeiro.
    ///
    /// `None` é fim de stream — o `ffmpeg` morreu, e quem chama decide
    /// o que isso significa para a chamada.
    pub fn ler_quadro(&mut self) -> Option<Vec<u8>> {
        match self.video.proximo() {
            Ok(super::ivf::EstadoQuadro::Quadro(q)) => Some(q.dados),
            // Fim limpo, e **também** um quadro truncado: um `ffmpeg`
            // que morreu a meio de escrever e um `ffmpeg` que terminou
            // dão o mesmo symptom para quem só quer continuar ou não.
            // O diagnóstico fino é o `stderr`, que §8.1.1 já recolhe.
            Ok(super::ivf::EstadoQuadro::Fim) | Err(_) => None,
        }
    }

    /// O cabeçalho do IVF que o `ffmpeg` escreveu.
    ///
    /// Serve para confirmar a geometria: um cabeçalho a dizer outra
    /// coisa é um emissor que mudou de ideias a meio, e um receptor que
    /// não verifica apresenta uma imagem distorcida sem dizer nada.
    pub fn cabecalho_video(&self) -> super::ivf::Cabecalho {
        self.video.cabecalho()
    }

    /// O `pid` do filho, para diagnóstico.
    pub fn pid(&self) -> u32 {
        self.filho.id()
    }

    /// Mata o processo e espera por ele.
    ///
    /// Explícito e não só [`Drop`]: o anel quer saber que o `ffmpeg`
    /// acabou **antes** de decidir o que fazer com o que ficou, e um
    /// `Drop` não dá esse momento.
    pub fn parar(mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

impl Drop for CapturaViva {
    fn drop(&mut self) {
        // `kill` e não `wait`: o `wait` bloqueia, e um `Drop` que
        // bloqueia trava a thread que deitou o objecto fora — que nesta
        // aplicação é a do IPC.
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

/// A stream viva: o `ffmpeg`, o pipe de vídeo e o canal de áudio.
pub struct Stream {
    /// O `ffmpeg` e o pipe de vídeo.
    pub captura: CapturaViva,
    /// Os pedacos de áudio, à medida que a thread os lê.
    pub audio: Receiver<Vec<u8>>,
    /// As últimas linhas do `stderr` do `ffmpeg`.
    ///
    /// ## Porque isto existe
    ///
    /// O `stderr` ia para um pipe e **ninguém o lia**. Duas
    /// consequências, e a segunda é a pior:
    ///
    /// 1. Um `ffmpeg` que falha — ecrã não encontrado, formato não
    ///    suportado — escrevia o erro para um pipe que ninguém lia, e o
    ///    `MEDIA_CAPTURAR` respondia `OK`. A pessoa via «a capturar» e
    ///    não acontecia nada, sem qualquer pista.
    /// 2. O buffer de um pipe são 64 kB. Um `ffmpeg` que escrevesse mais
    ///    do que isso **bloqueava para sempre** à espera de um leitor que
    ///    não existia — e a captura ficava pendurada sem que nada
    ///    estivesse errado do ponto de vista de ninguém.
    ///
    /// A thread consome o pipe e guarda as últimas linhas, e
    /// [`lancar`] usa-as quando o filho morre a seguir ao arranque.
    diagnostico: Arc<Mutex<Vec<String>>>,
    /// Mantém o canal de áudio aberto depois de o `Stream` cair.
    ///
    /// Sem isto, a thread do áudio vê o pipe fechar quando o `SyncSender`
    /// é largado, e interpreta o fim como fim de stream — o que
    /// acontece exactamente quando o `ffmpeg` ainda está a correr. É o
    /// mesmo papel que o `Guarda` cumpre em qualquer producer/consumer
    /// onde o consumidor é quem sai primeiro.
    _guarda_audio: SyncSender<Vec<u8>>,
}

/// Quantas linhas de diagnóstico se guardam.
const LINHAS_DIAGNOSTICO: usize = 8;

/// Quanto se espera, em milissegundos, para o filho dizer que não
/// arrancou.
///
/// §8.1 não fala disto, e a escolha é uma: ou `MEDIA_CAPTURAR` responde
/// `OK` para um processo que já morreu, ou bloqueia um instante para
/// poder dizer a verdade. **Dizer a verdade**, porque «a capturar» sem
/// captura é um ecrã que não reage e nenhuma mensagem — e a pessoa
/// reinicia a aplicação à sorte.
///
/// O custo é 60 ms numa acção que acontece uma vez por captura, e o
/// que se recebe em troca é a mensagem do próprio `ffmpeg`.
const ESPERA_DE_ARRANQUE_MS: u64 = 60;

/// Lança o `ffmpeg` com os dois canais.
pub fn lancar(captura: Captura, modo: Modo) -> Result<Stream, ErroMedia> {
    let _ = modo; // o modo não muda a linha de comandos: muda o consumo

    // **A geometria valida primeiro.** A ordem não éIndiferente: o
    // executável embebido é um `memfd` de ~80 MB, e descartar esse
    // `memfd` porque a largura não é múltipla de 16 é alocar 80 MB para
    // depois deitar fora. A validação é uma divisão e um resto.
    //
    // A ordem também não éIndiferente para o **diagnóstico**: numa
    // máquina sem o `ffmpeg`, `MEDIA_INICIAR` com uma largura inválida
    // tem de dizer qual das duas coisas está errada. A da geometria
    // primeiro, porque é a que o utilizador pode corrigir sem instalar
    // nada.
    captura.validar()?;

    if !embutido::disponivel() {
        return Err(ErroMedia::DependenciaAusente);
    }

    let _executavel = embutido::preparar()?;

    let (leitura, escrita) = criar_pipe_audio()?;

    let linha = linha_de_comandos(captura);
    let mut comando = Command::new(embutido::caminho());
    comando
        .args(&linha)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // O `stderr` num pipe e não herdado: o `ffmpeg` escreve lá todo
        // o diagnóstico, e esse texto pode conter caminhos e nomes de
        // dispositivo. Passá-lo a um `Stdio::herdado()` punha-o na
        // consola do daemon, que é um registo — e
        // `docs/security_model.md` não quer conteúdo em registos.
        .stderr(Stdio::piped());

    let escrita_fd = escrita;
    unsafe {
        comando.pre_exec(move || {
            // A falha tem de chegar ao pai: o Rust transforma um `Err`
            // de `pre_exec` num exit status, e o filho não chega a fazer
            // `exec`. Sem isto, o `ffmpeg` correria com um 3 que não é o
            // seu, e o áudio desapareceria sem erro.
            if libc::dup2(escrita_fd, FD_AUDIO_FILHO) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut filho = match comando.spawn() {
        Ok(f) => f,
        Err(e) => {
            let errno = e.raw_os_error().unwrap_or(libc::EIO);
            // O pai ainda tem os dois descritores: fecha os dois.
            unsafe {
                libc::close(leitura);
                libc::close(escrita);
            }
            return Err(ErroMedia::ExecucaoFalhou(errno));
        }
    };

    // O pai já não precisa da escrita — o filho tem o seu 3. Sem este
    // `close`, o pai mantém a escrita aberta e a thread do áudio nunca
    // vê EOF: o fim de stream que nunca chega, e um anel de áudio que
    // espera por um pedaço que não existe.
    unsafe { libc::close(escrita) };

    let video = match filho.stdout.take() {
        Some(v) => v,
        None => {
            let _ = filho.kill();
            unsafe { libc::close(leitura) };
            return Err(ErroMedia::ExecucaoFalhou(libc::ENXIO));
        }
    };

    let ficheiro_audio = unsafe { std::fs::File::from_raw_fd(leitura) };
    let (envio, rececao): (SyncSender<Vec<u8>>, Receiver<Vec<u8>>) =
        sync_channel(PROFUNDIDADE_CANAL_AUDIO);
    // O clone é feito **antes** do `move`, e não dentro do closure: o
    // closure consome a `envio` original, e é o clone que fica no
    // `Stream` a segurar o canal aberto. Ver `_guarda_audio`.
    let para_a_thread = envio.clone();

    // E o relógio do áudio, que é o mestre da chamada (§5.3).
    //
    // ## E porque é uma cópia e não uma referência
    //
    // Porque o `RelogioMedia` avança **na thread do áudio** — é lá que
    // os bytes passam — e o `Stream` tem o seu próprio, que avança com
    // o vídeo.
    //
    // ## E porque não é o mesmo relógio
    //
    // Porque o áudio e o vídeo são **dois relógios** que correm à
    // mesma taxa mas com origens diferentes: o do áudio conta bytes de
    // Opus, e o do vídeo conta bytes de VP8. A §5.3 diz que o áudio é o
    // mestre, e é por isso que o relógio do áudio é o que vai no
    // cabeçalho de cada pacote.
    let relogio_audio = super::relogio::RelogioMedia::novo(u32::from(captura.bytes_por_ms()));

    let lida = std::thread::Builder::new()
        .name("onyx-media-audio".into())
        .spawn(move || pump_audio(ficheiro_audio, para_a_thread, relogio_audio))
        .is_ok();

    if !lida {
        let _ = filho.kill();
        return Err(ErroMedia::ExecucaoFalhou(libc::EAGAIN));
    }

    // O `stderr` é lido por outra via: uma thread que o consome e guarda as últimas
    // linhas. Sem ela, o pipe enche e o `ffmpeg` bloqueia.
    let diagnostico = Arc::new(Mutex::new(Vec::new()));
    let para_o_diagnostico = Arc::clone(&diagnostico);
    let consumo = filho.stderr.take().map(|errore| {
        std::thread::Builder::new()
            .name("onyx-media-stderr".into())
            .spawn(move || bombear_diagnostico(errore, para_o_diagnostico))
            .is_ok()
    });
    if consumo == Some(false) {
        let _ = filho.kill();
        return Err(ErroMedia::ExecucaoFalhou(libc::EAGAIN));
    }

    // E a espera de arranque: se o `ffmpeg` morreu, o diagnóstico já
    // está no buffer e a resposta pode dizer porquê.
    if esperar_arranque(&mut filho).is_some() {
        let motivo = ultimo_diagnostico(&diagnostico);
        let _ = filho.kill();
        let _ = filho.wait();
        return Err(ErroMedia::CapturaRecusada(motivo));
    }

    // O leitor de IVF é construído **aqui**, e não no primeiro
    // `ler_quadro`: é o que lê o cabeçalho e valida que o `ffmpeg`
    // está a escrever VP8. Uma captura declarada viva cujo pipe vai
    //trazer código não-VP8 seria uma chamada que falha vinte segundos
    // depois, no primeiro quadro, longe da causa.
    let video = match super::ivf::LeitorIvf::novo(video) {
        Ok(l) => l,
        Err(_) => {
            // O `ffmpeg` pode ter morrido antes de escrever os 32
            // bytes do cabeçalho — e nesse caso o diagnóstico é o que
            // diz porquê, não «cabeçalho curto».
            let motivo = ultimo_diagnostico(&diagnostico);
            let _ = filho.kill();
            let _ = filho.wait();
            return Err(ErroMedia::CapturaRecusada(motivo));
        }
    };

    Ok(Stream {
        captura: CapturaViva {
            filho,
            video,
            captura,
            sequencia: 0,
            // `bytes_por_ms` vem do áudio: o relógio de §5.3 conta
            // **uma** taxa, e a do vídeo é a do áudio porque o VP8
            // já entregue é codificado — o que se transporta não é
            // tempo de media, são bytes que o receptor conta.
            relogio: super::relogio::RelogioMedia::novo(u32::from(captura.bytes_por_ms())),
        },
        audio: rececao,
        diagnostico,
        _guarda_audio: envio,
    })
}

/// Consome o `stderr` do `ffmpeg`, guardando as últimas linhas.
///
/// Uma linha de cada vez, e com um limite: o `stderr` do `ffmpeg` pode
/// ser do tamanho que quiser, e guardá-lo inteiro seria pôr o
/// diagnóstico de uma máquina noutro sítio — que é o que §7.1 e
/// `docs/security_model.md` recusam.
fn bombear_diagnostico(mut fonte: ChildStderr, destino: Arc<Mutex<Vec<String>>>) {
    use std::io::BufRead as _;
    let leitor = std::io::BufReader::new(&mut fonte);
    for linha in leitor.lines() {
        let Ok(linha) = linha else { break };
        let Ok(mut guarda) = destino.lock() else {
            return;
        };
        guarda.push(truncar(&linha, 200));
        if guarda.len() > LINHAS_DIAGNOSTICO {
            guarda.remove(0);
        }
    }
}

/// Espera pelo arranque, e devolve como o filho morreu.
///
/// [`ESPERA_DE_ARRANQUE_MS`] no total, em passos de 2 ms. Devolve
/// `None` se o filho continuar vivo — que é o caso comum — e o código de
/// saída se não.
fn esperar_arranque(filho: &mut Child) -> Option<i32> {
    let passos = ESPERA_DE_ARRANQUE_MS / 2;
    for _ in 0..passos {
        // `try_wait` faz o `wait` quando o filho já saiu, e devolve o
        // estado. O `Child` fica como se tivesse terminado, e o `kill`
        // que vem a seguir é um erro inofensivo.
        match filho.try_wait() {
            // `Ok(None)` é o filho **ainda vivo** — e a primeira
            // versão fazia `return` com isso, o que saía do ciclo no
            // primeiro passo e transformava a verificação num `noop`.
            // `Ok(Some)` é o filho que já saiu: `try_wait` fez o `wait`,
            // e o `kill` que vem a seguir é inofensivo.
            Ok(None) => {}
            Ok(saida) => return saida.and_then(|s| s.code()),
            Err(_) => return Some(libc::EIO),
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    // Chegou aqui o filho continua vivo, que é o caso comum.
    None
}

/// A última linha de diagnóstico que vale a pena mostrar.
fn ultimo_diagnostico(diagnostico: &Arc<Mutex<Vec<String>>>) -> String {
    diagnostico
        .lock()
        .ok()
        .and_then(|g| g.last().cloned())
        .unwrap_or_else(|| "o ffmpeg não disse porquê".to_string())
}

/// Trunca uma linha de diagnóstico.
fn truncar(texto: &str, max: usize) -> String {
    if texto.chars().count() <= max {
        return texto.to_string();
    }
    let cortada: String = texto.chars().take(max.saturating_sub(1)).collect();
    format!("{cortada}…")
}

/// Cria o pipe do áudio, com `FD_CLOEXEC` nos dois extremos.
///
/// O `FD_CLOEXEC` nos dois é o que garante que o pai não os herda no
/// filho por acidente: o `dup2` em `pre_exec` recria o 3 **sem** a
/// marca, e os outros ficam fechados no filho, que é o que se quer.
fn criar_pipe_audio() -> Result<(RawFd, RawFd), ErroMedia> {
    let mut par = [0 as RawFd; 2];
    if unsafe { libc::pipe(par.as_mut_ptr()) } != 0 {
        return Err(ErroMedia::ExecucaoFalhou(io_errno()));
    }
    for fd in par {
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            let errno = io_errno();
            unsafe {
                libc::close(par[0]);
                libc::close(par[1]);
            }
            return Err(ErroMedia::ExecucaoFalhou(errno));
        }
    }
    Ok((par[0], par[1]))
}

/// A linha de comandos completa, como `Vec<String>`.
///
/// Separada de [`lancar`] porque a **linha de comandos é a
/// especificação** do que o `ffmpeg` faz, e o que ela faz tem de ser
/// testável sem um ecrã, um microfone ou um `ffmpeg` instalado. Um
/// teste de [`lancar`] sem ecrã não prova nada; um teste desta função
/// prova cada argumento.
pub fn linha_de_comandos(captura: Captura) -> Vec<String> {
    // Vec<String> e não Vec<&str> porque três dos argumentos são
    // `format!` — o filtro de escala e a taxa de áudio dependem da
    // geometria. Compreender cada argumento é mais útil do que uma
    // closure que os empurre todos para dentro, e uma closure que
    // toma `&mut` do `Vec` impede os `push` seguintes.
    let mut linha: Vec<String> = Vec::with_capacity(24);

    // --- entrada de vídeo -------------------------------------------------
    for parte in entrada_de_video() {
        linha.push(parte.to_string());
    }

    // --- escala ------------------------------------------------------------
    // A conversão para `gray` acontece **aqui**, não no consumidor: é o
    // `ffmpeg` que tem o escalonador, e um `resize` em Rust daria um
    // segundo caminho para o mesmo número — que é onde as divergências
    // nascem.
    linha.push("-filter:v".into());
    linha.push(format!("scale={}:{}", captura.largura, captura.altura));

    // --- saída de vídeo: VP8 no descritor 1 ---------------------------------
    //
    // ## Porque VP8 e não `rawvideo`
    //
    // A primeira versão mandava `rawvideo` em cinzento e o daemon
    // codificava em telhas 16×16. A medição de `examples/medir_media.rs`
    // mostrou que esse caminho não fecha: uma keyframe a 640×480 são
    // 414 456 bytes de texto contra 65 536 de `MAX_PLAINTEXT` — 6,3×
    // acima — e o orçamento de 12,8 msg/s dá 629 kbit/s no total.
    //
    // O VP8 é o codec de videochamada de baixo bitrate, e resolve o
    // problema da raiz porque faz **compensação de movimento** e
    // **codificação entrópica**, que é o que as telhas não faziam.
    // Medido a 854×480 a 300 kbit/s, que é 55 % do orçamento com o
    // áudio incluído (§3.4.2).
    //
    // E é a mesma `-b:v` que o receptor vai afinar o anel com: o
    // `bitrate` é o contrato entre os dois lados, e por isso vem do
    // mesmo `captura`.
    linha.extend(
        [
            "-c:v",
            "libvpx",
            "-b:v",
            &format!("{}k", captura.video_kbps),
            // `realtime` e `cpu-used 8` são o que torna isto uma
            // codificação de chamada e não uma de ficheiro: sem eles o
            // VP8 procura um bitstream melhor do que a latência permite
            // e o `ffmpeg` acumula atraso. Com eles, o `ffmpeg` gasta o
            // que precisa por quadro e o quadro sai a tempo.
            "-deadline",
            "realtime",
            "-cpu-used",
            "8",
            // A chave de quadros: um I-frame a cada 2 s. Mais curto
            // seria mais resilience e mais bandwidth — e o VP8 já
            // gasta 55 % do orçamento, não 100 %.
            "-g",
            "60",
        ]
        .map(String::from),
    );
    linha.extend(["-map", "0:v", "-f", "ivf", "-"].map(String::from));

    // --- entrada de áudio --------------------------------------------------
    linha.extend(entrada_de_audio(captura_usa_pulse()));

    // --- saída de áudio: descritor 3 ---------------------------------------
    linha.extend(["-map", "1:a", "-c:a", "libopus", "-b:a"].map(String::from));
    linha.push(format!("{}k", captura.audio_kbps));
    linha.extend(
        [
            // `-vbr off` é **obrigatório** e não é um ajuste de qualidade:
            // `docs/media.md` §5.3, o relógio avança por contagem de
            // bytes dividida por `bytes_por_ms`, e com VBR o valor
            // declarado é uma média e o relógio acumula desvio sem
            // nenhum sinal.
            "-vbr", "off", "-f", "opus",
            // `fd:3` — o protocolo `fd:` do `ffmpeg` com um número de
            // descritor. A alternativa, `pipe:3`, abriria um ficheiro
            // chamado «3» no directório de trabalho em vez de usar o
            // descritor 3.
            "fd:3",
        ]
        .map(String::from),
    );

    linha
}

/// Os argumentos de entrada de vídeo, antes do `-filter:v`.
///
/// ## O que isto procura, e o que deixou de procurar
///
/// Uma **câmara**, e não um ecrã. A primeira versão escolhia `x11grab`
/// em X11 e `pipewiregrab` em Wayland, o que era captura de ecrã — e
/// `docs/media.md` §10.3.1.2 chega a dizer que uma chamada «não escolhe
/// quem chama». Um convite a mostrar a cara de quem chamou é a coisa
/// mais óbvia que uma chamada faz, e o código não a fazia.
///
/// ## A ordem, e porquê nesta ordem
///
/// 1. **`ONYXCAMERA`**, se a pessoa puser.
/// 2. **`/dev/video*`** — a câmara USB ou a webcam, por V4L2. É a
///    primeira porque não depende de nada: `/dev/video*` é uma
///    interface do kernel, não uma biblioteca, e a receita do FFmpeg
///    liga `v4l2` sem pedir header nenhum.
/// 3. **`pipewiregrab`** — para quem usa PipeWire e não tem
///    `/dev/video*`, o que é raro mas possível (câmara só por PipeWire).
///    Fica depois do V4L2 porque o `pipewiregrab` precisa da biblioteca
///    do PipeWire, e o V4L2 não.
///
/// ## E o `x11grab` ficou fora
///
/// Porque ecrã não é câmara. E porque habilitar `libxcb` aborta o
/// `configure` numa máquina sem `xau`/`xdmcp` — ver a nota em
/// `tools/ffmpeg/build.sh`.
///
/// ## A variável de ambiente
///
/// `ONYXCAMERA` é o caminho do dispositivo, ou o índice numérico do
/// `/dev/video*`. É a saída para quem tem duas câmaras e quer escolher,
/// e é a forma de testar sem hardware: `ONYXCAMERA=/dev/video1`.
pub fn entrada_de_video() -> Vec<String> {
    // --- o que a pessoa pediu ------------------------------------------
    if let Ok(escolhido) = std::env::var("ONYXCAMERA") {
        let escolhido = escolhido.trim();
        if !escolhido.is_empty() {
            // Numérico é um índice de `/dev/video*`; o resto é um
            // caminho. A distinção é `/dev/` no começo, e não «são
            // dígitos», porque um caminho pode ser tudo menos dígitos
            // e um índice pode ter zeros à esquerda.
            if escolhido.bytes().all(|b| b.is_ascii_digit()) {
                return vec![
                    "-f".into(),
                    "v4l2".into(),
                    "-i".into(),
                    format!("/dev/video{escolhido}"),
                ];
            }
            return vec![
                "-f".into(),
                "v4l2".into(),
                "-i".into(),
                escolhido.to_string(),
            ];
        }
    }

    // --- a primeira câmara que existe -----------------------------------
    //
    // A enumeração é feita aqui e não pelo `ffmpeg` porque o V4L2 do
    // FFmpeg não sabe qual é o primeiro dispositivo: `-i default` no
    // `v4l2` é um nome de dispositivo específico, não um atalho.
    if let Some(caminho) = primeira_camera() {
        return vec![
            "-f".into(),
            "v4l2".into(),
            "-i".into(),
            caminho.to_string_lossy().into_owned(),
        ];
    }

    // --- PipeWire, que não tem /dev/video* -------------------------------
    vec![
        "-f".into(),
        "pipewiregrab".into(),
        "-i".into(),
        "default".into(),
    ]
}

/// O primeiro `/dev/video*` que existe, em ordem numérica.
///
/// ## Porque se enumera e não se adivinha
///
/// Porque `/dev/video0` pode não existir. Numa máquina com uma câmara
/// USB e um cartão de captura, os índices são 0 e 2, e o 1 é um
/// dispositivo que não é câmara. Adivinhar o 0 daria um erro do
/// `ffmpeg` sobre um dispositivo que existe mas não é o que se quer.
///
/// ## E porque em ordem numérica
///
/// Porque `read_dir` devolve na ordem do sistema de ficheiros, que num
/// directório de dispositivos é por ordem de criação. Num `/dev` recriado
/// a cada arranque, isso muda entre arranques — e uma chamada que usa
/// uma câmara num arranque e outra no seguinte, sem dizer qual, é o tipo
/// exacto de falha que ninguém consegue reproduzir.
fn primeira_camera() -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new("/dev");
    let entradas = std::fs::read_dir(dir).ok()?;

    let mut indices: Vec<(u32, std::path::PathBuf)> = entradas
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let nome = e.file_name();
            let nome = nome.to_str()?;
            let resto = nome.strip_prefix("video")?;
            // Só dígitos: `/dev/video0` é câmara e `/dev/video0-dv` é o
            // nó de controlo do mesmo dispositivo — e o V4L2 de saída
            // precisa do nó de *dados*, não do de controlo.
            if resto.is_empty() || !resto.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let indice = resto.parse::<u32>().ok()?;
            Some((indice, e.path()))
        })
        .collect();

    indices.sort_by_key(|(i, _)| *i);
    indices.into_iter().next().map(|(_, caminho)| caminho)
}

/// Os argumentos de entrada de áudio, antes do `-c:a`.
///
/// ## O `-i default` e o que ele significa
///
/// `default` é o dispositivo de captura por omissão do ALSA, e é o que
/// o `ffmpeg` documenta. Mas `default` no ALSA é muitas vezes o
/// dispositivo `null`, que devolve silêncio sem erro — e uma chamada
/// muda que não se ouve nenhum é o pior tipo de falha: não há
/// mensagem, não há erro, e o sintoma é «o outro não me ouve».
///
/// A distinção é esta: se `ONYXMICROFONE` estiver definido, usa-se esse
/// dispositivo; senão usa-se `default`, que é o que a maioria das
/// máquinas tem e é o que funciona.
///
/// ## E o `pulse`
///
/// Não é uma entrada separada aqui. `-f pulse -i default` dá acesso ao
/// microfone quando há PulseAudio/PipeWire a gerir os dispositivos, e é
/// isso que :data:`captura_usa_pulse` decide.
pub fn entrada_de_audio(pulse: bool) -> Vec<String> {
    if let Ok(escolhido) = std::env::var("ONYXMICROFONE") {
        let escolhido = escolhido.trim();
        if !escolhido.is_empty() {
            return if pulse {
                vec![
                    "-f".into(),
                    "pulse".into(),
                    "-i".into(),
                    escolhido.to_string(),
                ]
            } else {
                vec![
                    "-f".into(),
                    "alsa".into(),
                    "-i".into(),
                    escolhido.to_string(),
                ]
            };
        }
    }

    if pulse {
        vec!["-f".into(), "pulse".into(), "-i".into(), "default".into()]
    } else {
        vec!["-f".into(), "alsa".into(), "-i".into(), "default".into()]
    }
}

/// Há PulseAudio/PipeWire a gerir o áudio.
///
/// Por variável de ambiente e por existência do socket, e não por
/// tentativa: tentar significaria um `ffmpeg` que arranca, não captura e
/// morre — e o utilizador veria um stream sem som com um erro no
/// `stderr` que ele não vê.
///
/// O `XDG_RUNTIME_DIR` é onde o PulseAudio põe o socket, e é a forma
/// canónica de o encontrar sem saber o `$HOME` de ninguém.
fn captura_usa_pulse() -> bool {
    let dir_runtime = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) => std::path::PathBuf::from(d),
        None => return false,
    };
    dir_runtime.join("pulse/native").exists()
        || std::path::Path::new("/run/user").join(current_uid_str()).join("pulse/native").exists()
}

/// O `uid` como texto, para o caminho do `/run/user`.
fn current_uid_str() -> String {
    // `unsafe { libc::getuid() }` e não `std::process::id()`: o
    // `XDG_RUNTIME_DIR` é do **utilizador**, e o `pid` é do processo. São
    // números diferentes e confundir daria o caminho de outra pessoa.
    unsafe { libc::getuid() }.to_string()
}

/// Lê o pipe de áudio até ao fim, depositando no canal.
///
/// Bloqueante no envio, de propósito: o áudio não pode ser descartado
/// (`docs/media.md` §7.2), e um `try_send` que falhasse abriria um
/// buraco no relógio mestre.
/// Lê o pipe de áudio até ao fim, depositando no canal.
///
/// ## E porque cada pacote leva o relógio
///
/// Porque o `clock_ms` é o que permite ao receptor saber **quando** o
/// áudio que recebeu foi produzido — e sem isso o lip sync é
/// impossível: o receptor teria de adivinhar o tempo a partir do
/// tamanho do pacote, e o tamanho só diz quanto áudio **há**, não
/// quando foi capturado.
///
/// O formato do canal é `seq: u32 LE ‖ clock_ms: i64 LE ‖ opus`, e é
/// o mesmo que o `MEDIA_LER` usa para o vídeo. A simetria não é
/// estética: o receptor tem um caminho de código para os dois.
///
/// ## E porque o relógio avança **aqui**
///
/// Porque o `pump_audio` é quem lê os bytes, e o tempo de media é o
/// tempo **decorrido** de áudio — que é o que se conta à medida que os
/// bytes passam. Contar no receptor seria contar os bytes recebidos, e
/// a rede altera a ordem e o tempo de chegada.
fn pump_audio(
    mut ficheiro: std::fs::File,
    envio: SyncSender<Vec<u8>>,
    mut relogio: super::relogio::RelogioMedia,
) {
    let mut pedaco = vec![0u8; TAMANHO_PEDACO_AUDIO];
    let mut sequencia: u32 = 0;
    // O resto dos bytes que não deram um milissegundo inteiro.
    //
    // E o que faz a média ser **exacta**: um pacote de 1 ms a 4
    // bytes/ms arredondaria para zero a cada vez, e o relógio não
    // andava. Com o resto acumulado em bytes, a média é exacta em
    // qualquer número de pacotes.
    let mut resto_bytes: u32 = 0;
    loop {
        match ficheiro.read(&mut pedaco) {
            Ok(0) => return,
            Ok(n) => {
                // E o relógio avança com os bytes que passaram, e
                // **antes** de o pacote ser enviado — porque o tempo
                // de um pacote é o tempo até ao fim desse pacote, e
                // mandá-lo antes de o contar daria ao receptor um
                // tempo atrasado de um pacote.
                relogio.avancar_audio(n, &mut resto_bytes);

                let mut mensagem = Vec::with_capacity(12 + n);
                mensagem.extend_from_slice(&sequencia.to_le_bytes());
                mensagem.extend_from_slice(&relogio.media_ms.to_le_bytes());
                mensagem.extend_from_slice(&pedaco[..n]);
                sequencia = sequencia.wrapping_add(1);

                if envio.send(mensagem).is_err() {
                    // O receptor largou o canal — o `Stream` foi
                    // dropado. Não há ninguém para quem entregar.
                    return;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

fn io_errno() -> i32 {
    io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod testes {
    use super::*;

    /// Uma geometria **válida**: 320 = 20×16 e 240 = 15×16.
    ///
    /// A primeira versão deste ficheiro punha `640×360` — e 360/16 dá
    /// 22,5, que não é múltiplo de 16. A geometria de teste era inválida
    /// ao ponto de o teste de validação estar a testar a geometria de
    /// teste e não a regra. Está aqui o valor certo e o comentário
    /// explica porque é que estes dois números não são óbvios.
    fn captura() -> Captura {
        Captura {
            largura: 320,
            altura: 240,
            fps: 30,
            audio_kbps: 32,
            video_kbps: 300,
        }
    }

    /// A linha de comandos tem as duas saídas, nos descritores certos.
    ///
    /// O teste mais importante do ficheiro: `-` no fim da saída de
    /// vídeo e `fd:3` no fim da de áudio. Trocar `fd:3` por `pipe:3`
    /// faria o `ffmpeg` abrir um ficheiro chamado «3» no directório de
    /// trabalho, e o sintoma seria um stream sem som — que é quase
    /// impossível de diagnosticar a partir do `ffmpeg`.
    #[test]
    fn as_duas_saidas_vao_para_os_descritores_certos() {
        let linha = linha_de_comandos(captura());

        let v = linha.iter().position(|a| a == "-map").expect("há um -map");
        assert_eq!(linha[v + 1], "0:v", "o primeiro mapa é o vídeo");
        // O separador depois do `-f rawvideo` é o stdout.
        let raw = linha.iter().position(|a| a == "ivf").expect("há ivf");
        assert_eq!(linha[raw + 1], "-", "a vídeo sai no stdout");

        assert_eq!(linha[linha.len() - 1], "fd:3", "o áudio sai no descritor 3");
        assert!(
            !linha.iter().any(|a| a.starts_with("pipe:")),
            "pipe: cria um ficheiro, não um descritor"
        );
    }

    /// O `-vbr off` está na linha, e é obrigatório.
    ///
    /// A propriedade é `docs/media.md` §5.3: sem `-vbr off`, o
    /// `bytes_por_ms` declarado é uma média, o relógio acumula desvio, e
    /// a divergência só aparece como áudio que começa a atrasar-se ao
    /// fim de um minuto de conversa.
    #[test]
    fn o_vbr_esta_desligado() {
        let linha = linha_de_comandos(captura());
        let pos = linha.iter().position(|a| a == "-vbr").expect("há -vbr");
        assert_eq!(linha[pos + 1], "off", "o VBR tem de estar desligado");
    }

    /// O `format=gray` está no filtro, e antes do `-map`.
    ///
    /// A ordem importa: `-filter:v` aplica-se ao fluxo que o `-map`
    /// seguinte escolhe, e `-map` antes de `-filter:v` deixa o filtro a
    /// aplicado a todos os fluxos de vídeo — o que, com um filtro de
    /// escala, produz um quadro de tamanho errado em silêncio.
    #[test]
    fn o_filtro_esta_antes_do_primeiro_map() {
        let linha = linha_de_comandos(captura());
        let filtro = linha
            .iter()
            .position(|a| a == "-filter:v")
            .expect("há filtro");
        let mapa = linha.iter().position(|a| a == "-map").expect("há mapa");
        assert!(filtro < mapa, "o filtro tem de vir antes do -map");

        let valor = &linha[filtro + 1];
        // Sem `format=gray`: o VP8 quer YUV. Uma versão anterior
        // mandava cinzento e o codec recebia um quadro de um só canal,
        // que ele teria de expandir para três antes de comprimir — ou
        // seja, três vezes a largura de banda por uma cor que de
        // qualquer forma ia ser reconstruída no receptor.
        assert_eq!(valor, "scale=320:240");
    }

    /// A linha de comandos codifica o vídeo em VP8, e diz a que taxa.
    ///
    /// ## Por que este teste existe
    ///
    ///porque o VP8 entrou tarde. Até F9 o vídeo ia em `rawvideo` e as
    /// telhas faziam a compressão em Rust — e esse caminho estava
    /// medido, especificado e errado: 6,3× acima do envelope.
    ///
    /// `-b:v` é o **contrato** com o receptor. Se voltar ao `rawvideo`
    /// ou perder o `-b:v`, a chamada ou não arranca ou enche o anel do
    /// receptor, e nenhum dos dois dá erro — dá uma chamada que não
    /// funciona, que é a falha mais difícil de encontrar.
    #[test]
    fn o_video_vai_em_vp8_a_taxa_pedida() {
        let linha = linha_de_comandos(captura());

        let codec = linha.iter().position(|a| a == "-c:v").expect("há -c:v");
        assert_eq!(
            linha[codec + 1],
            "libvpx",
            "o codec de video tem de ser VP8: e' o que cabe no orcamento"
        );

        let taxa = linha.iter().position(|a| a == "-b:v").expect("há -b:v");
        assert_eq!(
            linha[taxa + 1],
            "300k",
            "a taxa de video e' o contrato com o receptor"
        );

        // `-deadline realtime` e `-cpu-used 8` é o que distingue uma
        // codificação de chamada de uma de ficheiro. Sem eles o VP8
        // procura um bitstream melhor do que a latência permite e o
        // `ffmpeg` acumula atraso — que numa chamada é um vídeo que
        // chega tarde, que é indistinguível de rede lenta.
        let deadline = linha
            .iter()
            .position(|a| a == "-deadline")
            .expect("há -deadline");
        assert_eq!(linha[deadline + 1], "realtime");
        let cpu = linha
            .iter()
            .position(|a| a == "-cpu-used")
            .expect("há -cpu-used");
        assert_eq!(linha[cpu + 1], "8");

        // E o `rawvideo` tem de ter desaparecido: é o caminho que não
        // fecha, e um `-f rawvideo` esquecido ao lado do VP8 faria o
        // `ffmpeg` escolher um dos dois sem dizer qual.
        assert!(
            !linha.iter().any(|a| a == "rawvideo"),
            "o rawvideo ficou na linha ao lado do VP8"
        );
    }

    /// A taxa de áudio que vai para o `-b:a` é a pedida.
    #[test]
    fn a_taxa_de_audio_vai_para_o_b() {
        let linha = linha_de_comandos(captura());
        let pos = linha.iter().position(|a| a == "-b:a").expect("há -b:a");
        assert_eq!(linha[pos + 1], "32k");
    }

    /// `bytes_por_ms` é `bitrate / 8000`, como a spec diz.
    ///
    /// A tabela de `docs/media.md` §5.3, em teste. Sem isto, o relógio
    /// e o `ffmpeg` discordariam do número por uma razão que nenhum
    /// teste apanharia — o relógio correria a um outro ritmo e a
    /// sincronia pareceria um problema de latência.
    #[test]
    fn os_bytes_por_ms_sao_a_taxa_divida_por_oito_mil() {
        for (kbps, esperado) in [(128u16, 16u16), (32, 4), (768, 96)] {
            let c = Captura {
                audio_kbps: kbps,
                ..captura()
            };
            assert_eq!(
                c.bytes_por_ms(),
                esperado,
                "{kbps} kbps deviam dar {esperado}"
            );
        }
    }

    /// A geometria tem de ser múltipla de 16, e recusada se não for.
    ///
    /// A razão está no docstring de [`Captura::validar`], e é a razão
    /// de isto ser um erro e não um arredondamento: um arredondamento
    /// daria um quadro de largura diferente da que o emissor mandou, e
    /// o receptor reconstruiria mal a partir do segundo quadro sem
    /// nenhum erro.
    #[test]
    fn a_geometria_ja_nao_precisa_de_ser_multipla_de_dezasseis() {
        // ## O que mudou
        //
        // A regra dos múltiplos de 16 veio com a grelha de telhas de §6.2,
        // e o VP8 tirou a grelha. Um `ffmpeg` que recusa 1280×720 ou
        // 640×360 está a recusar resoluções perfectly válidas por causa de
        // um detalhe de um codec que já não está no caminho.
        //
        // E há um segundo motivo, que é mais sério: 720p a 30 fps é
        // 1280×720, e com múltiplos de 16 havia 80×45 telhas — que cabiam.
        // Mas 1080p são 120×68.25, que **não** cabe. A regra recusava a
        // 1080p por causa de uma contagem de telhas que deixou de existir.
        let r = captura();
        assert!(r.validar().is_ok());

        let mut nao_multipla = r;
        nao_multipla.largura = 321;
        assert!(
            nao_multipla.validar().is_ok(),
            "321 de largura e' uma largura valida; so nao e' multipla de 16"
        );

        // O que continua recusado é o que o VP8 nao perdoa.
        let mut zero = r;
        zero.largura = 0;
        assert!(matches!(
            zero.validar(),
            Err(ErroMedia::GeometriaInvalida(_))
        ));

        let mut sem_fps = r;
        sem_fps.fps = 0;
        assert!(matches!(
            sem_fps.validar(),
            Err(ErroMedia::GeometriaInvalida(_))
        ));

        let mut sem_video = r;
        sem_video.video_kbps = 0;
        assert!(
            matches!(sem_video.validar(), Err(ErroMedia::GeometriaInvalida(_))),
            "uma taxa de video zero produz um ffmpeg que corre e nao transmite"
        );
    }

    #[test]
    fn o_fps_e_a_taxa_de_audio_tem_de_ser_positivos() {
        assert!(Captura {
            fps: 0,
            ..captura()
        }
        .validar()
        .is_err());
        assert!(Captura {
            audio_kbps: 0,
            ..captura()
        }
        .validar()
        .is_err());
    }

    /// O tamanho do quadro é a área, num byte por pixel.
    ///
    /// A fórmula que `ler_quadro` usa para o `read_exact`. Um erro aqui
    /// — um byte por pixel a mais, ou uma altura trocada — faz o
    /// consumidor ler o tamanho errado e o receptor reconstruir um
    /// quadro desalinhado, que é um erro que **não aparece** em lado
    /// nenhum.
    #[test]
    fn o_tamanho_do_quadro_e_a_area() {
        assert_eq!(captura().tamanho_do_quadro(), 320 * 240);
        let c = Captura {
            largura: 320,
            altura: 240,
            ..captura()
        };
        assert_eq!(c.tamanho_do_quadro(), 76_800);
        let grande = Captura {
            largura: 640,
            altura: 352,
            ..captura()
        };
        assert_eq!(grande.tamanho_do_quadro(), 225_280);
    }

    /// O `WAYLAND_DISPLAY` decide o dispositivo de captura.
    ///
    /// Testado com a variável manipulada dentro do teste, e não com um
    /// mock, porque `std::env` num teste é seguro com
    /// `RUST_TEST_THREADS=1` — que é o que `.cargo/config.toml` já
    /// declara precisamente para isto.
    /// A câmara vem de `/dev/video*` antes de qualquer coisa.
    ///
    /// O teste não assume que existe uma câmara: verifica a **forma**
    /// do que a função devolve, e que o `x11grab` deixou de aparecer.
    /// Numa máquina sem câmara a função devolve o `pipewiregrab`, e o
    /// teste tem de passar nas duas.
    #[test]
    fn a_entrada_de_video_procura_uma_camera() {
        let anterior = std::env::var_os("ONYXCAMERA");
        std::env::remove_var("ONYXCAMERA");

        let args = entrada_de_video();
        let linha = args.join(" ");

        assert!(
            linha.contains("v4l2") || linha.contains("pipewiregrab"),
            "a entrada de vídeo é «{linha}» e não é nem V4L2 nem PipeWire"
        );
        assert!(
            !linha.contains("x11grab"),
            "a entrada de vídeo ainda é «{linha}»: é captura de ecrã, não de câmara"
        );

        match anterior {
            Some(v) => std::env::set_var("ONYXCAMERA", v),
            None => std::env::remove_var("ONYXCAMERA"),
        }
    }

    /// `ONYXCAMERA` manda, e um índice vira caminho.
    ///
    /// Os dois ramos existem porque quem escreve `ONYXCAMERA=1` espera
    /// `/dev/video1`, e quem escreve `ONYXCAMERA=/dev/video2` espera
    /// esse caminho. Tratar o número como caminho daria um erro de
    /// «ficheiro não encontrado» para um valor que é perfeitamente
    /// válido.
    #[test]
    fn a_variavel_de_ambiente_escolhe_a_camera() {
        let anterior = std::env::var_os("ONYXCAMERA");

        std::env::set_var("ONYXCAMERA", "1");
        assert_eq!(
            entrada_de_video(),
            vec!["-f", "v4l2", "-i", "/dev/video1"]
        );

        std::env::set_var("ONYXCAMERA", "/dev/video2");
        assert_eq!(
            entrada_de_video(),
            vec!["-f", "v4l2", "-i", "/dev/video2"]
        );

        // Uma variável vazia ou só com espaços não escolhe nada: o
        // `x11grab` não volta, e o que acontece é a enumeração normal.
        std::env::set_var("ONYXCAMERA", "   ");
        let linha = entrada_de_video().join(" ");
        assert!(!linha.contains("ONYXCAMERA"));

        match anterior {
            Some(v) => std::env::set_var("ONYXCAMERA", v),
            None => std::env::remove_var("ONYXCAMERA"),
        }
    }

    /// O áudio vai ao ALSA, ou ao PulseAudio se houver socket.
    ///
    /// E o `-f default` sem `-f` explícito era o que a receita antiga
    /// tinha: `-i default` sem `-f` é um nome de ficheiro, e o `ffmpeg`
    /// adivinha o formato a partir do conteúdo — o que para um socket
    /// não funciona.
    #[test]
    fn a_entrada_de_audio_declara_o_formato() {
        let args = entrada_de_audio(false);
        assert_eq!(
            args.first().map(String::as_str),
            Some("-f"),
            "a entrada de áudio é {args:?} e não declara o formato"
        );
        assert_eq!(args.get(1).map(String::as_str), Some("alsa"));

        let args = entrada_de_audio(true);
        assert_eq!(args.get(1).map(String::as_str), Some("pulse"));
    }

    /// Sem executável embebido, `lancar` dá `DependenciaAusente`.
    ///
    /// O modo degradado de `docs/media.md`, verificado: esta compilação
    /// de teste não tem o artefacto, e o daemon continua a funcionar com
    /// texto porque nada aqui é um caminho de texto.
    #[test]
    fn sem_executavel_embutido_da_erro_de_dependencia() {
        if embutido::disponivel() {
            return;
        }
        // `matches!` e não `assert_eq!`: um `Stream` tem um `Child`
        // dentro, e `Child` não implementa `PartialEq` nem `Debug`.
        // Implementá-los seria escrever duas implorações que só existem
        // para este teste.
        assert!(matches!(
            lancar(captura(), Modo::Streaming),
            Err(ErroMedia::DependenciaAusente)
        ));
    }

    /// A geometria é validada **antes** de o executável ser preparado.
    ///
    /// A ordem importa por uma razão concreta: preparar o executável
    /// cria um `memfd` de 80 MB, e descartá-lo por causa de um `fps` zero
    /// é alocar 80 MB para depois deitar fora.
    ///
    /// A geometria de teste deixou de ser «largura não múltipla de 16»
    /// porque essa regra foi com a grelha de telhas (F9). Passou a ser
    /// `fps` zero, que é o que continua a ser erro e o que faz o
    /// `ffmpeg` correr sem transmitir nada.
    #[test]
    fn a_geometria_invalida_e_recusada_antes_do_memfd() {
        let mau = Captura {
            fps: 0,
            ..captura()
        };
        let r = lancar(mau, Modo::Streaming);
        assert!(matches!(r, Err(ErroMedia::GeometriaInvalida(_))));
    }
}
