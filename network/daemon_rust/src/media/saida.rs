// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// saida.rs — por onde o áudio de uma chamada sai
// ---------------------------------------------------------------------
// A interface é uma página servida a um WebKitGTK 6
// (`scripts/interface-visual.js`), e o WebKitGTK **não garante**
// `AudioContext` com as garantias de que a chamada precisa — nem
// `AudioDecoder` (ver `media/imagem.rs` para o caso do vídeo).
//
// Por isso o áudio é descodificado e reproduzido **aqui**, no daemon:
// o Opus chega pela rede, vira PCM, e sai pelo servidor de som do
// sistema. A interface nunca vê o áudio — e por isso nunca o pode
// tocar.
//
// ## E porque o `ffmpeg` embebido e não a `libpulse` directamente
//
// Porque o `ffmpeg` **já está** embebido, já sabe descodificar Opus
// (o `--enable-decoder=opus` da receita), e precisa de uma saída de
// áudio que a receita agora tem (`--enable-outdev=alsa`).
//
// A alternativa seria falar o protocolo do PulseAudio com `dlopen` —
// que é o que a primeira versão deste módulo fazia, e o que custou uma
// tarde.
//
// ## E o que há de errado com a `libpulse` directamente
//
// Que o `pa_stream_connect_playback` **exige** a thread do laço do
// PulseAudio, e que o `pa_context_connect` **exige** a mesma thread —
// e que as duas exigências são mutuamente incompatíveis a partir de
// outra thread, porque uma espera pela rede enquanto a outra está
// à espera dela.
//
// A forma canónica de resolver — o `pa_threaded_mainloop_lock`, que o
// `cubeb` usa — funciona para o `connect` e **não** para o
// `pa_stream_connect_playback`, que aborta o processo com
//
// ```text
// Assertion 's' failed at ../src/pulse/stream.c:1422,
//                 function pa_stream_connect_playback(). Aborting.
// ```
//
// E o `abort()` é um `SIGABRT` do PulseAudio, que aparece como um
// `SIGSEGV` sem mensagem do lado do Rust.
//
// ## E porque o `ffmpeg` não tem esse problema
//
// Porque o `ffmpeg` é um **programa** e não uma biblioteca: ele abre
// o dispositivo de som, escreve nele e sai. Ninguém lhe exige que a
// chamada venha de uma thread em particular.
//
// E o custo é o de sempre: um processo por sessão de áudio. Um
// `ffmpeg` que fica a correr a receber PCM num pipe e a escrevê-lo
// no ALSA.
//
// ## A referência de renderização
//
// E o que o AEC precisa: o que vai sair. A [`Saida`] regista cada
// bloco que escreve, e o `media/eco.rs` lê esse registo para dar ao
// cancelador a referência de que ele precisa.
//
// Sem isto o AEC degrada — que é o que a documentação do
// `media/eco.rs` chama «uma chamada com eco», e é melhor do que «uma
// chamada muda».

use std::io::Write as _;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

/// A taxa de amostragem da saída, em Hz.
///
/// E a mesma da captura: 48 kHz, que é o que a receita do Opus de
/// `docs/media.md` §7.1 diz. Uma diferença entre a taxa de captura e a
/// de reprodução obriga a um resampler, e um resampler no caminho de
/// uma chamada é latência que ninguém pediu.
pub const TAXA: u32 = 48_000;

/// Os canais de saída.
///
/// **Mono**, e não estéreo: a voz de uma chamada é mono, e o áudio
/// estéreo do outro lado seria metade bytes por nada. E o cancelador de
/// eco trabalha com uma banda, que é o que o WebRTC faz para a banda
/// larga.
pub const CANAIS: u16 = 1;

/// O estado da saída, guardado para o diagnóstico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    /// O fluxo está a escrever.
    Aberto,
    /// O `ffmpeg` não pôde ser executado.
    SemExecutavel,
    /// O `ffmpeg` não aceitou o comando de saída.
    SemSaida,
}

/// A saída de áudio de uma chamada.
///
/// ## O que ela tem
///
/// O `ffmpeg` a correr, o `stdin` onde se escreve o PCM, e o **registo
/// do que foi escrito** — que é a referência de renderização de que o
/// cancelador de eco precisa.
///
/// ## E o registo
///
/// É o último bloco escrito, e não um anel. O AEC quer o que os
/// altifalantes estão a tocar **agora**, e o que está a tocar agora é
/// o último bloco — os anteriores já saíram. Um anel daria ao AEC um
/// eco com alguns milissegundos de atraso a mais, e a subtração ficaria
/// sempre errada.
pub struct Saida {
    /// O `ffmpeg` a correr.
    ///
    /// E `Option` porque o `Drop` precisa de distinguir «nunca houve
    /// processo» de «o processo já foi recolhido».
    filho: Option<Child>,
    /// Onde se escreve o PCM.
    ///
    /// E `Option` pelo mesmo motivo: quando o `ffmpeg` morre, o pipe
    /// fecha, e escrever nele dá `EPIPE`.
    entrada: Option<ChildStdin>,
    /// O último bloco escrito, para a referência do AEC.
    referencia: Arc<Mutex<Vec<f32>>>,
    /// O estado, para o diagnóstico.
    estado: Estado,
}

// `Send` porque a escrita e o `Drop` podem acontecer em threads
// diferentes — o `Drop` é do `daemon` e a escrita é do laço de áudio.
unsafe impl Send for Saida {}

impl Saida {
    /// Abre a saída, se o `ffmpeg` embebido estiver disponível.
    ///
    /// Devolve `None` — e não um erro — quando não há executável. A
    /// chamada continua **muda**, e quem chama tem de saber que está
    /// muda: [`Estado`] diz o quê.
    ///
    /// ## E porque o `ffmpeg` é executado aqui e não na captura
    ///
    /// Porque a captura já tem um `ffmpeg` a correr, e dois `ffmpeg` a
    /// compete pelo microfone seria uma guerra de dispositivos. A
    /// saída é um processo **separado**, que só escreve no
    /// altifalante.
    pub fn abrir(nome: &str) -> Option<Self> {
        if !super::ffmpeg_embutido::disponivel() {
            return None;
        }

        let fd = super::ffmpeg_embutido::preparar().ok()?;
        let caminho_exec = format!("/proc/self/fd/{fd}");

        // O comando.
        //
        // `-f alsa` com o `default` no fim é o dispositivo de saída
        // por omissão do ALSA — e o `pipe` que o `ffmpeg` documenta
        // como omissão é um dispositivo `null` que não existe numa
        // máquina com PulseAudio.
        //
        // E o `-` no fim é a entrada, que é o `stdin` do processo.
        let executavel: std::ffi::OsString = caminho_exec.into();
        let argumentos: Vec<std::ffi::OsString> = [
            "-hide_banner",
            "-loglevel",
            "error",
            // A entrada: PCM cru, pelo `stdin`.
            "-f",
            "s16le",
            "-ar",
            "48000",
            "-ac",
            "1",
            "-i",
            "pipe:0",
            // A saída: o dispositivo de som do ALSA.
            "-f",
            "alsa",
            "-ac",
            "1",
            "-ar",
            "48000",
            "default",
        ]
        .iter()
        .map(std::ffi::OsString::from)
        .collect();

        let mut comando = Command::new(&executavel);
        comando.args(&argumentos);
        comando.stdin(Stdio::piped());
        // O `stdout` e o `stderr` vão para o vazio: o `ffmpeg` escreve
        // o erro no `stderr`, e um `stderr` de um processo de media não
        // é um sítio para segredos — mas também não é um sítio para
        // diagnostics que ninguém lê.
        comando.stdout(Stdio::null());
        comando.stderr(Stdio::null());

        let mut filho = match comando.spawn() {
            Ok(f) => f,
            Err(_) => {
                super::ffmpeg_embutido::fechar();
                return None;
            }
        };

        let entrada = filho.stdin.take();
        super::ffmpeg_embutido::fechar();

        // O `ffmpeg` com `-f alsa` escreve no dispositivo, mas só
        // depois de ter lido o suficiente para conhecer o formato. E
        // ele **sai** com erro se o dispositivo não aceitar.
        //
        // A verificação é esperar um pouco e ver se ainda está vivo:
        // um `ffmpeg` que não consegue abrir o dispositivo de som sai
        // em milissegundos, e um que consegue fica à espera de áudio.
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Ok(Some(_)) = filho.try_wait() {
            return None;
        }

        Some(Saida {
            filho: Some(filho),
            entrada,
            referencia: Arc::new(Mutex::new(Vec::new())),
            estado: Estado::Aberto,
        })
    }

    /// O estado da saída, para a interface mostrar o que se passa.
    pub fn estado(&self) -> Estado {
        self.estado
    }

    /// Há saída de áudio?
    ///
    /// E `false` quando a chamada vai ficar **muda**, e a interface tem
    /// de dizer isso em vez de a pessoa ficar à espera de ouvir alguém
    /// que não está a falar.
    pub fn disponivel(&self) -> bool {
        self.estado == Estado::Aberto
    }

    /// Escreve um bloco de PCM.
    ///
    /// ## E porque o `f32` e não o `i16`
    ///
    /// Porque o `media/eco.rs` trabalha em `f32` — o AEC do WebRTC usa
    /// `float` — e converter aqui e lá seria uma conversão por bloco,
    /// trezentas vezes por segundo, para nada.
    ///
    /// ## E o que a escrita faz com a referência
    ///
    /// Regista o bloco. É esse registo que o cancelador de eco lê para
    /// saber o que os altifalantes estão a tocar.
    ///
    /// ## E porque o erro é `false` e não um erro
    ///
    /// Porque um `write` que falha é um `write` que falha: a chamada
    /// continua, o áudio para de sair, e a interface mostra que está
    /// muda. Um erro aqui subiria pelo laço de chamada e acabaria a
    /// chamada — que é o que o `media/eco.rs` chama «cortar a pessoa a
    /// meio de uma frase».
    pub fn escrever(&mut self, pcm: &[f32]) -> bool {
        if pcm.is_empty() {
            return true;
        }

        // A conversão para `i16` é a escala do PCM: −1,0 a 1,0 dá
        // −32768 a 32767.
        let i16s: Vec<i16> = pcm
            .iter()
            .map(|&a| (a.clamp(-1.0, 1.0) * 32_767.0) as i16)
            .collect();
        let bytes: Vec<u8> = i16s.iter().flat_map(|a| a.to_le_bytes()).collect();

        let escrita = match self.entrada.as_mut() {
            Some(entrada) => entrada.write_all(&bytes).is_ok(),
            None => false,
        };
        if !escrita {
            // O `ffmpeg` morreu — e o pipe fechou. O estado passa a
            // «sem saída», e a interface diz que a chamada está muda.
            self.estado = Estado::SemSaida;
            return false;
        }

        // E o registo da referência, que é o que o AEC vai ler.
        if let Ok(mut referencia) = self.referencia.lock() {
            referencia.clear();
            referencia.extend_from_slice(pcm);
        }
        true
    }

    /// A referência de renderização: o que os altifalantes estão a tocar.
    ///
    /// E uma **cópia** e não uma referência ao registo interior, porque
    /// o AEC chama isto da sua thread e o registo muda por baixo. A
    /// cópia de 160 amostras é de 640 bytes, e a cópia por bloco a 300
    /// por segundo é 192 kB/s de memória — que é o preço de não ter uma
    /// corrida.
    pub fn referencia(&self) -> Vec<f32> {
        self.referencia
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// O `Arc` da referência, para o AEC ler sem copiar a cada bloco.
    ///
    /// E o que o `media/eco.rs` usa: o AEC precisa de uma referência
    /// **por bloco**, e copiá-la trezentas vezes por segundo seria
    /// trabalho que não mede nada.
    pub fn referencia_partilhada(&self) -> Arc<Mutex<Vec<f32>>> {
        Arc::clone(&self.referencia)
    }
}

impl Drop for Saida {
    fn drop(&mut self) {
        // O `stdin` primeiro, para o `ffmpeg` ver o EOF e sair por
        // si. E o `kill` a seguir, porque um `ffmpeg` à espera de áudio
        // que nunca chega não sai sozinho.
        self.entrada = None;
        if let Some(mut filho) = self.filho.take() {
            let _ = filho.kill();
            let _ = filho.wait();
        }
    }
}

#[cfg(test)]
mod testes;