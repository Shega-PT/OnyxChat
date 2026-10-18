// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// eco.rs — cancelar o eco da linha
// ---------------------------------------------------------------------
// O que resolve: a voz que o sistema está a reproduzir volta ao
// microfone, e o par ouve-se a si próprio. Em auscultadores isso não
// acontece; em altifalantes, é o que torna uma chamada inutilizável.
//
// ## Porque isto é um módulo e não uma dependência de compilação
//
// Porque a máquina **não tem os cabeçalhos**. `libwebrtc-audio-processing1`
// está instalado como `.so.1` de runtime, e `libspeexdsp1` também, e
// nenhum dos dois tem um único `.h` — nem `pkg-config` existe nesta
// máquina, que foi verificado antes de se decidir isto.
//
// Qualquer crate `*-sys` que os embrulhasse (`webrtc-sys`, `speexdsp-sys`)
// precisa dos cabeçalhos para compilar, e sem eles não compila. É o mesmo
// problema que o `tools/ffmpeg/build.sh` resolveu com prefixos
// desempacotados, e que aqui não se resolve: desempacotar o
// `libwebrtc-audio-processing-dev` daria o cabeçalho, mas essa é uma
// dependência de instalação a mais para um executável que embute as suas.
//
// Por isso `dlopen`/`dlsym` em tempo de execução: o binário compila sem
// cabeçalhos e a biblioteca é procurada quando há áudio para tratar.
// Sem biblioteca, a chamada continua — muda de qualidade.
//
// ## A referência de renderização, e o porquê de isto ser delicate
//
// Um cancelador de eco precisa de duas coisas: o **sinal perto** (o que
// o microfone ouve, com o eco já misturado) e o **sinal longe** (o que
// os altifalantes estão a tocar, limpo).
//
// Só o primeiro vem do ALSA. O segundo só existe porque este mesmo
// daemon reproduz o áudio da chamada — e é por isso que o módulo tem
// duas metades: `processar` para o microfone e `referencia` para o que
// sai.
//
// ## Se a referência não chegar
//
// O AEC degrada. Não rebenta, não corta o áudio, e a documentação de
// quem está em modo degradado diz-o (ver `estado`). É a diferença entre
// uma chamada com eco e uma chamada muda, e a segunda é pior.
//
// ## E o que este módulo não faz
//
// Não faz supressão de ruído, nem gains automático, nem não
// cancelamento de howling. O WebRTC completo (`AudioProcessing`)
// trataria disso; aqui só o AEC, que é o problema que faz uma chamada
// em altifalantes ser impossível.
// =====================================================================

use std::ffi::{c_int, c_void, CString};
use std::sync::Arc;

/// Amostras por quadro de áudio.
///
/// 10 ms a 48 kHz. O valor vem do WebRTC: o AEC é um filtro de
/// convolução com uma janela de retardo máxima, e a janela é o que
/// limita a qualidade — uma janela maior apanha mais eco e demora mais
/// a convergir.
///
/// E 48 kHz e não 44,1 kHz porque a receita do Opus de `docs/media.md`
/// §7.1 é 48 kHz, e um resampler entre a captura e o codec seria
/// trabalho sem motivo.
pub const AMOSTRAS_POR_QUADRO: usize = 480;

/// A taxa de amostragem, em Hz.
///
/// Sommar a `AMOSTRAS_POR_QUADRO` para a taxa põe a taxa como
/// consequência da janela, e não as duas como constantes independentes
/// que podem divergir.
pub const TAXA: u32 = 48_000;

/// O número de descritores que o AEC mantém, por canal.
///
/// O WebRTC usa 1 para mono e 2 para estéreo, e é o que
/// `WebRtcAec_Create` espera para decidir quantos buffers aloca.
const DESCARTES: u32 = 1;

/// O tamanho da janela de eco, em milissegundos.
///
/// 500 ms é o valor de omissão do WebRTC e a razão de ser: o eco de
/// altifalantes numa sala normal tem uns dez milissegundos, e o filtro
/// precisa de uma janela muito maior para o apanhar.
///
/// Uma janela maior apanha mais eco e custa mais memória e mais atraso.
/// Uma menor é mais barata e deixa eco por apanhar. Para altifalantes
/// numa sala, 500 ms é o ponto em que a razão entre o que se apanha e o
/// que se paga deixa de melhorar.
const JANELA_MS: i32 = 500;

/// O buffer de dados que o `WebRtcAec_Create` escreve.
///
/// É um alias nomeado, e não `[i32; 11]` escrito no local: um tipo que
/// termina em `]` no fim de uma linha de tipo é lido pelo `rustdoc`
/// como um atributo, e o ficheiro deixa de compilar com «this doc
/// comment doesn't document anything». O alias tem a mesma informação e
/// não tem o problema.
#[repr(C)]
pub struct BufferAec(pub [i32; 11]);

/// A função de criação do AEC.
///
/// A assinatura vem do cabeçalho C do WebRTC, no caminho
/// `api/audio/aec/public/aec.h` da fonte, e é anotada com
/// `extern "C"` para que o Rust não a altere.
///
/// ```c
/// int WebRtcAec_Create(void** handle, int sample_rate, int channel_count,
///                       int samples_per_channel, int use_10ms_frames,
///                       int32_t data_size, int32_t* out_data);
/// ```
///
/// ## O `data_size` que o Rust não tem
///
/// A assinatura C leva um `data_size` que é o tamanho do buffer de
/// saída, e o Rust não o tem — porque em Rust o tipo do parâmetro já
/// diz o tamanho. É a diferença entre C e Rust que torna estas
/// assinaturas escritas à mão um pouco perigosas, e é a razão de cada
/// bloco trazer a assinatura C original ao lado: comparar é de olhos.
///
/// E `use_10ms_frames` **não** é um booleano sobre o tamanho do quadro:
/// é o número de milissegundos que cada quadro representa. Passar 10 é
/// o que diz ao AEC que o quadro é de 10 ms, e é o que faz a
/// latência ser uma fração do quadro em vez de uma escolha ao acaso.
type PfnCreate =
    unsafe extern "C" fn(handle: *mut *mut c_void, sample_rate: c_int, channel_count: c_int,
                         samples_per_channel: c_int, use_10ms_frames: c_int,
                         out_data: *mut BufferAec) -> c_int;

/// A função que destrói o AEC.
///
/// ```c
/// int WebRtcAec_Free(void* handle);
/// ```
type PfnFree = unsafe extern "C" fn(handle: *mut c_void) -> c_int;

/// A função que aplica o cancelador.
///
/// ```c
/// int WebRtcAec_Process(void* handle, const int16_t* nearend,
///                       const int16_t* farend, int16_t* out,
///                       int* out_delay);
/// ```
type PfnProcess = unsafe extern "C" fn(
    handle: *mut c_void,
    near: *const i16,
    far: *const i16,
    out: *mut i16,
    out_delay: *mut c_int,
) -> c_int;

/// A função que alimenta a referência de renderização.
///
/// Esta é a que faz o AEC ter o que comparar. Sem chamadas a ela — ou
/// com chamadas na ordem errada — o filtro não tem eco para cancelar e
/// degrada para transparente.
///
/// ```c
/// void WebRtcAec_BufferFarend(void* handle, const int16_t* farend,
///                             int samples_per_channel);
/// ```
type PfnBufferFarend = unsafe extern "C" fn(
    handle: *mut c_void,
    far: *const i16,
    samples_per_channel: c_int,
);

/// A função que diz o atraso entre o que se reproduziu e o que se ouviu.
///
/// ```c
/// void WebRtcAec_SetSystemDelay(void* handle, int delay_ms);
/// ```
type PfnSetSystemDelay = unsafe extern "C" fn(handle: *mut c_void, delay_ms: c_int);

/// A função que devolve as métricas, para diagnóstico.
///
/// ```c
/// void WebRtcAec_GetMetrics(void* handle, AecMetrics* metrics);
/// ```
///
/// A estrutura é opaca para este módulo: o que interessa é saber se
/// chamarla é seguro, e o conteúdo dela mudaria entre versões sem que
/// nada aqui mudasse. Por isso é preciso que a exportação exista e que
/// o buffer tenha tamanho — e o tamanho nunca é lido.
type PfnGetMetrics = unsafe extern "C" fn(handle: *mut c_void, metrics: *mut c_void);

/// Os símbolos carregados da biblioteca de AEC.
///
/// ## Porque `Arc` e não uma referência
///
/// Porque o `Cancelador` é usado pelo laço de captura, que pode estar
/// num `thread` diferente do que o criou. E porque `Cancelador` tem de
/// ser `Send` para isso funcionar — e um `*mut c_void` não é.
///
/// O `Arc` torna a partilha segura sem `unsafe impl Send`, que é o que
/// um ponteiro cru exigiria e que seria uma promessa que este módulo
/// teria de manter sozinho.
#[derive(Clone)]
struct Simbolos {
    /// `WebRtcAec_Create`
    criar: PfnCreate,
    /// `WebRtcAec_Free`
    livre: PfnFree,
    /// `WebRtcAec_Process`
    processar: PfnProcess,
    /// `WebRtcAec_BufferFarend`
    referencia: PfnBufferFarend,
    /// `WebRtcAec_SetSystemDelay`
    atraso: PfnSetSystemDelay,
}

// `Send` e `Sync` porque o `Arc<Simbolos>` pode ser partilhado entre
// threads.
//
// `Simbolos` só tem ponteiros de função, que são `Copy` e não tocam
// estado — o estado está no `handle`, que fica com quem o criou. Por
// isso a derivação é segura: partilhar a *tabela* de ponteiros não é
// partilhar o AEC.
unsafe impl Send for Simbolos {}
unsafe impl Sync for Simbolos {}

/// Qual dos dois motores é o carregado, para diagnóstico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotorDeEco {
    /// O AEC3 do WebRTC.
    Webrtc,
    /// O `speexdsp`.
    Speex,
    /// Não há biblioteca: o áudio passa limpo.
    Nenhum,
}

/// O cancelador de eco de um fluxo de captura.
///
/// ## O que ele tem, e o porquê de não ser opcional
///
/// **Referência de renderização.** Um AEC sem a referência não cancela:
/// o filtro tem o microfone e não sabe o que subtract. Por isso este
/// objecto tem [`referencia`](Self::referencia), e o laço de áudio
/// chama-a com o que vai reproduzir *antes* de
/// [`processar`](Self::processar) o que o microfone ouve.
///
/// E a ordem importa: a referência é o que o sistema vai tocar nos
/// próximos 10 ms, e o microfone é o que já ouviu o que tocou há 10 ms.
/// Alimentar a referência depois do microfone é dizer ao filtro para
/// cancelar o eco de um som que ainda não tocou.
pub struct Cancelador {
    /// O handle opaco da biblioteca, ou `0` quando não há motor.
    ///
    /// O `0` e não um `Option<*mut c_void>` porque a biblioteca não tem
    /// handle nenhum neste caso, e um ponteiro nulo é a representação
    /// que ela própria usa para «não há handle».
    handle: *mut c_void,
    /// Os símbolos da API do WebRTC, se for esse o motor.
    ///
    /// Ficam para o `Drop` chamar a função de destruição correcta: sem
    /// o mesmo `livre` que criou o handle, a biblioteca não sabe o que
    /// libertar. São `None` quando o motor é o `speexdsp`, e é por isso
    /// que `atraso_do_sistema` verifica o motor antes de os usar.
    simbolos: Option<Arc<Simbolos>>,
    /// Os ponteiros de função do `speexdsp`, se for esse o motor.
    ///
    /// A API do `speexdsp` não é a mesma — não tem `Create`, tem
    /// `state_init` — e por isso vive em campos próprios em vez de
    /// ser forçada para dentro de [`Simbolos`]. Um motor que não é o
    /// do WebRTC não tem ponteiros de função do WebRTC, e um struct
    /// com campos de tipo errado seria `mem::transmute` disfarçado.
    speex: Option<Arc<SimbolosSpeex>>,
    /// O estado de preprocessamento do `speexdsp`, se for esse o motor.
    ///
    /// Vive aqui e não dentro de [`SimbolosSpeex`] porque é **por
    /// instância**, e a tabela de símbolos é partilhada entre todas. A
    /// distinção é a mesma de `handle` contra `simbolos`: o primeiro
    /// é estado, o segundo é tabela.
    speex_pre: *mut c_void,
    /// A referência de renderização mais recente.
    ///
    /// O `WebRtcAec_Process` quer a referência em dois sítios: o
    /// `farend` do-processing e o buffer interno que
    /// `WebRtcAec_BufferFarend` encheu. São o **mesmo** áudio, e é por
    /// isso que fica guardado aqui em vez de ser pedindo ao chamador
    /// duas vezes.
    referencia_atual: Vec<i16>,
}

/// Os símbolos do `speexdsp`, que tem uma API diferente da do WebRTC.
///
/// ## Porque não um `Simbolos` só
///
/// Porque as assinaturas **não** batem. `WebRtcAec_Create` devolve um
/// handle opaco e `WebRtcAec_Process` recebe-o; o `speexdsp` devolve um
/// `int` e usa o valor como índice numa tabela global. Forçar os dois no
/// mesmo struct seria um `transmute` que o compilador aceitaria e que
/// corromperia memória na primeira chamada.
#[derive(Clone)]
struct SimbolosSpeex {
    /// `speex_echo_state_init(int frame_size, int frame_length, int channels,
    /// int sample_rate, int *sample_spec, int *err)`
    iniciar: PfnSpeexInit,
    /// `speex_echo_cancellation(SpeexEchoState *st, const short *rec,
    /// const short *play, short *out)`
    cancelar: PfnSpeexCancel,
    /// `speex_echo_state_destroy(SpeexEchoState *st)`
    destruir: PfnSpeexDestroy,
    /// `speex_preprocess_state_init(int rate, int channels, int *err)`
    pre_iniciar: PfnSpeexPreInit,
    /// `speex_preprocess_run(SpeexEchoState *st, SpeexPreprocessState *ps,
    /// const short *x, short *y)`
    pre_correr: PfnSpeexPreRun,
    /// `speex_preprocess_state_destroy(SpeexPreprocessState *ps)`
    pre_destruir: PfnSpeexPreDestroy,
}

/// `int speex_echo_state_init(int, int, int, int, int*, int*)`
type PfnSpeexInit =
    unsafe extern "C" fn(frame_size: c_int, frame_length: c_int, channels: c_int,
                         sample_rate: c_int, spec: *mut c_int, err: *mut c_int) -> c_int;

/// `int speex_echo_cancellation(void*, const i16*, const i16*, i16*)`
type PfnSpeexCancel =
    unsafe extern "C" fn(st: *mut c_void, rec: *const i16, play: *const i16, out: *mut i16) -> c_int;

/// `void speex_echo_state_destroy(void*)`
type PfnSpeexDestroy = unsafe extern "C" fn(st: *mut c_void);

/// `int speex_preprocess_state_init(int, int, int*)`
type PfnSpeexPreInit =
    unsafe extern "C" fn(rate: c_int, channels: c_int, err: *mut c_int) -> *mut c_void;

/// `int speex_preprocess_run(void*, void*, const i16*, i16*)`
type PfnSpeexPreRun =
    unsafe extern "C" fn(st: *mut c_void, ps: *mut c_void, x: *const i16, y: *mut i16) -> c_int;

/// `void speex_preprocess_state_destroy(void*)`
type PfnSpeexPreDestroy = unsafe extern "C" fn(ps: *mut c_void);

// `Send` e `Sync` pela mesma razão que `Simbolos`: só ponteiros de
// função, que não tocam estado.
unsafe impl Send for SimbolosSpeex {}
unsafe impl Sync for SimbolosSpeex {}

// O `Cancelador` é usado do laço de captura e destruído no fim dele,
// que podem ser threads diferentes — daí o `Send`.
//
// `Sync` **não** é declarado: dois threads a chamar `processar` no mesmo
// handle seria uma corrida sobre o estado do filtro, e ninguém precisa
// disso. Quem quiser compartilhar compartilha o `Arc<Cancelador>` e a
// serialização fica com quem o usa.
unsafe impl Send for Cancelador {}

impl Cancelador {
    /// Procura uma biblioteca de cancelamento de eco, se houver uma.
    ///
    /// Devolve `Ok(None)` — e não `Err` — quando não há nada a
    /// carregar. Um `Err` faria a captura falhar, e a alternativa a um
    /// AEC é uma chamada **com eco**, não uma chamada impossível.
    ///
    /// A procura é feita **uma vez**, no primeiro quadro, e o resultado
    /// fica. Procurar uma biblioteca a cada quadro seria um `dlopen` por
    /// quadro, e `dlopen` não é barata mesmo quando falha.
    ///
    /// ## E porque o motor escolhido é o primeiro que inicializa
    ///
    /// Porque uma biblioteca pode estar presente e não funcionar: o
    /// `WebRtcAec_Create` devolve um código de erro se a taxa ou o
    /// tamanho do quadro não forem os que ele quer, e ficar com esse
    /// motor seria uma chamada sem cancelamento quando o `speexdsp`
    /// estava ali a funcionar.
    pub fn carregar() -> Option<Self> {
        if let Some((handle, simbolos)) = tentar_webrtc() {
            return Some(Cancelador {
                handle,
                simbolos: Some(simbolos),
                speex: None,
                speex_pre: std::ptr::null_mut(),
                referencia_atual: Vec::new(),
            });
        }
        if let Some((speex, handle, pre)) = tentar_speex() {
            return Some(Cancelador {
                handle,
                simbolos: None,
                speex: Some(Arc::new(speex)),
                speex_pre: pre,
                referencia_atual: Vec::new(),
            });
        }
        None
    }

    /// Que motor é que está a correr.
    pub fn motor(&self) -> MotorDeEco {
        if self.speex.is_some() {
            MotorDeEco::Speex
        } else if !self.handle.is_null() {
            MotorDeEco::Webrtc
        } else {
            MotorDeEco::Nenhum
        }
    }

    /// Diz ao cancelador quanto atraso há entre tocar e ouvir.
    ///
    /// O número é o atraso do sistema de áudio em alta — o do buffer do
    /// ALSA, tipicamente 20 a 60 ms. Sem ele o AEC compara o eco com o
    /// som na posição errada e degrada-se a meio da chamada.
    ///
    /// Um valor estimado serve; um valor zero faz o filtro procurar o
    /// eco cedo demais, e é por isso que o valor **zero é ignorado**
    /// em vez de ser tomado como «não configure».
    pub fn atraso_do_sistema(&mut self, ms: i32) {
        if ms <= 0 || self.motor() != MotorDeEco::Webrtc {
            return;
        }
        if let Some(simbolos) = &self.simbolos {
            unsafe { (simbolos.atraso)(self.handle, ms) };
        }
    }

    /// Alimenta a referência de renderização: o que vai tocar.
    ///
    /// **Chamar antes de [`processar`](Self::processar).** A referência
    /// é o que o sistema vai tocar nos próximos 10 ms, e o microfone é o
    /// que já ouviu o que tocou há 10 ms; alimentá-la depois de
    /// processar o microfone é dizer ao filtro para cancelar o eco de um
    /// som que ainda não tocou.
    ///
    /// Este é o ponto em que a ausência de referência dói. Se isto não
    /// for chamado — porque o daemon não reproduz, porque está a
    /// descodificar para ficheiro, ou porque ninguém pensou nisso — o
    /// filtro fica sem o que subtrair e o áudio sai limpo, com eco.
    ///
    /// # Segurança
    ///
    /// `quadro` tem de ter [`AMOSTRAS_POR_QUADRO`] amostras em `i16`.
    pub unsafe fn referencia(&mut self, quadro: &[i16]) {
        if quadro.len() != AMOSTRAS_POR_QUADRO {
            return;
        }
        self.referencia_atual.clear();
        self.referencia_atual.extend_from_slice(quadro);

        if self.motor() == MotorDeEco::Webrtc {
            // O `WebRtcAec_BufferFarend` é a via do WebRTC desta
            // chamada: enche o buffer interno que `Process` vai ler. É
            // diferente de passar o `farend` ao `Process`, e é a
            // diferença entre um AEC que adaptou e um que não adaptou.
            let simbolos = self
                .simbolos
                .as_ref()
                .expect("um cancelador WebRTC tem símbolos carregados");
            unsafe {
                (simbolos.referencia)(self.handle, quadro.as_ptr(), AMOSTRAS_POR_QUADRO as c_int)
            };
        }
        // O `speexdsp` não tem buffer de referência: o
        // `speex_echo_cancellation` recebe o áudio longe como argumento
        // e usa-o directamente. Guardá-lo em `referencia_atual` é o que
        // o dá ao `processar`.
    }

    /// Aplica o cancelador ao que o microfone ouviu.
    ///
    /// Devolve o quadro limpo, ou `None` se o handle não puder ser
    /// usado — o que num laço de captura significa fim de captura, e não
    /// um erro a repetir 30 vezes por segundo.
    ///
    /// # Segurança
    ///
    /// `perto` tem de ter [`AMOSTRAS_POR_QUADRO`] amostras em `i16`.
    pub fn processar(&mut self, perto: &[i16]) -> Option<Vec<i16>> {
        if perto.len() != AMOSTRAS_POR_QUADRO {
            return None;
        }

        match self.motor() {
            MotorDeEco::Webrtc => {
                let mut saida = vec![0i16; AMOSTRAS_POR_QUADRO];
                // E o `farend` que vai aqui é o **mesmo** que foi para
                // `WebRtcAec_BufferFarend`: o WebRTC usa os dois, um
                // para o filtro adaptativo e o outro para a referência
                // de adaptação. E quando a referência está vazia —
                // ninguém chamou [`referencia`](Self::referencia) — o
                // que entra é silêncio, que é o que o AEC espera e o
                // que faz o áudio sair sem eco **e** sem cortar.
                let referencia = if self.referencia_atual.len() == AMOSTRAS_POR_QUADRO {
                    self.referencia_atual.as_ptr()
                } else {
                    std::ptr::null()
                };
                let mut atraso: c_int = 0;
                let erro = unsafe {
                    (self.simbolos.as_ref().expect("motor WebRTC tem símbolos").processar)(
                        self.handle,
                        perto.as_ptr(),
                        referencia,
                        saida.as_mut_ptr(),
                        &mut atraso,
                    )
                };
                if erro != 0 {
                    return None;
                }
                Some(saida)
            }
            MotorDeEco::Speex => {
                let speex = self.speex.as_ref()?;
                let mut saida = vec![0i16; AMOSTRAS_POR_QUADRO];
                let referencia = if self.referencia_atual.len() == AMOSTRAS_POR_QUADRO {
                    self.referencia_atual.as_ptr()
                } else {
                    std::ptr::null()
                };
                let ok = unsafe { (speex.cancelar)(self.handle, perto.as_ptr(), referencia, saida.as_mut_ptr()) };
                if ok != 0 {
                    return None;
                }
                // E o preprocessamento por cima: ganho automático e
                // supressão de ruído. Sem isto o `speexdsp` cancela eco
                // mas fica com mais ruído de fundo do que devia, e o
                // ganho automático evita que uma voz baixa fique
                // inaudível depois de o filtro mexer no nível.
                if !self.speex_pre.is_null() {
                    let ok = unsafe {
                        (speex.pre_correr)(
                            self.handle,
                            self.speex_pre,
                            perto.as_ptr(),
                            saida.as_mut_ptr(),
                        )
                    };
                    if ok != 0 {
                        return None;
                    }
                }
                Some(saida)
            }
            MotorDeEco::Nenhum => Some(perto.to_vec()),
        }
    }
}

impl Drop for Cancelador {
    fn drop(&mut self) {
        if let Some(speex) = self.speex.take() {
            // O `speexdsp` tem dois estados com_cycle de vida própria, e
            // um `Drop` que só liberte um deles deixa o outro — e o
            // `echo_state` tem uma tabela de filtros que é a maior
            // porção de memória do módulo.
            if !self.speex_pre.is_null() {
                unsafe { (speex.pre_destruir)(self.speex_pre) };
                self.speex_pre = std::ptr::null_mut();
            }
            if !self.handle.is_null() {
                unsafe { (speex.destruir)(self.handle) };
                self.handle = std::ptr::null_mut();
            }
        }
        // O handle do WebRTC só é destruído se for dele. Com `speex` a
        // Some, `handle` é o estado do `speexdsp` e já foi libertado
        // acima — e libertá-lo com `WebRtcAec_Free` seria um
        // `free` num ponteiro que a biblioteca não fez.
        else if let Some(simbolos) = self.simbolos.take() {
            if !self.handle.is_null() {
                unsafe { (simbolos.livre)(self.handle) };
                self.handle = std::ptr::null_mut();
            }
        }
    }
}

/// Carrega a API do WebRTC, se existir e inicializar.
///
/// Os três passos são deliberados nesta ordem e cada um pode falhar
/// por si: carregar a biblioteca, encontrar os símbolos, e criar o
/// handle. Falhar em qualquer um é «não há motor» — e é o mesmo
/// desfecho para os três, porque um AEC a meio não é um AEC.
fn tentar_webrtc() -> Option<(*mut c_void, Arc<Simbolos>)> {
    let simbolos = carregar_simbolos_webrtc("libwebrtc_audio_processing.so.1")?;

    let mut handle: *mut c_void = std::ptr::null_mut();
    // O buffer de dados do `Create` é um `[i32; 11]` que a biblioteca
    // escreve com configuração interna. Não se lê: o conteúdo muda
    // entre versões e nenhuma decisão aqui depende dele. É alocado na
    // pilha porque a função copia o que precisa.
    let mut dados = BufferAec([0; 11]);

    let erro = unsafe {
        (simbolos.criar)(
            &mut handle,
            TAXA as c_int,
            DESCARTES as c_int,
            AMOSTRAS_POR_QUADRO as c_int,
            JANELA_MS,
            &mut dados,
        )
    };
    if erro != 0 || handle.is_null() {
        return None;
    }
    Some((handle, simbolos))
}

/// Carrega o `speexdsp`, se existir e inicializar.
fn tentar_speex() -> Option<(SimbolosSpeex, *mut c_void, *mut c_void)> {
    let simbolos = carregar_simbolos_speex("libspeexdsp.so.1")?;

    let mut erro: c_int = 0;
    // O `frame_length` é a janela de eco, em amostras: 500 ms a 48 kHz
    // são 24 000 amostras, que é o mesmo número que `JANELA_MS` dá no
    // outro motor. E `sample_spec` é o buffer de delay que a função
    // escreve.
    let frame_length = (JANELA_MS as usize * TAXA as usize / 1000) as c_int;
    let mut spec: c_int = 0;
    let handle = unsafe {
        (simbolos.iniciar)(
            AMOSTRAS_POR_QUADRO as c_int,
            frame_length,
            DESCARTES as c_int,
            TAXA as c_int,
            &mut spec,
            &mut erro,
        )
    };
    if erro != 0 || handle <= 0 {
        return None;
    }

    // O preprocessamento — que faz o ganho automático e a supressão de
    // ruído — é um estado separado, e é o que faz o `speexdsp` melhor
    // do que um simple `Subtract`. Sem ele o módulo ainda cancela eco,
    // mas com mais ruído de fundo.
    let mut erro_pre: c_int = 0;
    let pre = unsafe { (simbolos.pre_iniciar)(TAXA as c_int, DESCARTES as c_int, &mut erro_pre) };
    if erro_pre != 0 || pre.is_null() {
        unsafe { (simbolos.destruir)(handle as *mut c_void) };
        return None;
    }

    Some((simbolos, handle as *mut c_void, pre))
}

/// Carrega a API do WebRTC de uma biblioteca.
///
/// Devolve `None` se a biblioteca não existir **ou** não tiver um dos
/// símbolos pedidos. A diferença não interessa nos dois casos — o que
/// interessa é que um `dlopen` a succeed sem `WebRtcAec_Process` é uma
/// biblioteca de outra coisa com o mesmo nome, e acceptá-la daria um
/// salto para `None` na primeira chamada.
fn carregar_simbolos_webrtc(nome: &str) -> Option<Arc<Simbolos>> {
    use libc::RTLD_LAZY;

    unsafe {
        let c_nome = CString::new(nome).expect("nome de biblioteca sem bytes nulos");
        let handle: *mut c_void = libc::dlopen(c_nome.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            return None;
        }

        // Cada `dlsym` que não encontra o símbolo fecha a biblioteca e
        // devolve `None`. Sem o `dlclose` ali, uma sequência de
        // tentativas deixaria uma referência pendurada por biblioteca —
        // e o `dlopen` seguinte da mesma biblioteca contaria mais uma.
        macro_rules! simbolo {
            ($nome:expr) => {{
                let p: *mut c_void = libc::dlsym(handle, $nome.as_ptr() as *const libc::c_char);
                if p.is_null() {
                    libc::dlclose(handle);
                    return None;
                }
                p
            }};
        }

        let criar: PfnCreate = std::mem::transmute(simbolo!("WebRtcAec_Create"));
        let livre: PfnFree = std::mem::transmute(simbolo!("WebRtcAec_Free"));
        let processar: PfnProcess = std::mem::transmute(simbolo!("WebRtcAec_Process"));
        let referencia: PfnBufferFarend =
            std::mem::transmute(simbolo!("WebRtcAec_BufferFarend"));
        let atraso: PfnSetSystemDelay =
            std::mem::transmute(simbolo!("WebRtcAec_SetSystemDelay"));

        // O `dlclose` devolve a referência do `dlopen`. A biblioteca
        // continua resident porque o `Arc` dos símbolos a mantém viva,
        // e o `dlclose` garante que uma sequência de tentativas não
        // deixa referências penduradas.
        libc::dlclose(handle);

        Some(Arc::new(Simbolos {
            criar,
            livre,
            processar,
            referencia,
            atraso,

        }))
    }
}

/// Carrega a API do `speexdsp` de uma biblioteca.
fn carregar_simbolos_speex(nome: &str) -> Option<SimbolosSpeex> {
    use libc::RTLD_LAZY;

    unsafe {
        let c_nome = CString::new(nome).expect("nome de biblioteca sem bytes nulos");
        let handle: *mut c_void = libc::dlopen(c_nome.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            return None;
        }

        macro_rules! simbolo {
            ($nome:expr) => {{
                let p: *mut c_void = libc::dlsym(handle, $nome.as_ptr() as *const libc::c_char);
                if p.is_null() {
                    libc::dlclose(handle);
                    return None;
                }
                p
            }};
        }

        let iniciar: PfnSpeexInit = std::mem::transmute(simbolo!("speex_echo_state_init"));
        let cancelar: PfnSpeexCancel =
            std::mem::transmute(simbolo!("speex_echo_cancellation"));
        let destruir: PfnSpeexDestroy =
            std::mem::transmute(simbolo!("speex_echo_state_destroy"));
        let pre_iniciar: PfnSpeexPreInit =
            std::mem::transmute(simbolo!("speex_preprocess_state_init"));
        let pre_correr: PfnSpeexPreRun =
            std::mem::transmute(simbolo!("speex_preprocess_run"));
        let pre_destruir: PfnSpeexPreDestroy =
            std::mem::transmute(simbolo!("speex_preprocess_state_destroy"));

        libc::dlclose(handle);

        Some(SimbolosSpeex {
            iniciar,
            cancelar,
            destruir,
            pre_iniciar,
            pre_correr,
            pre_destruir,
        })
    }
}

#[cfg(test)]
mod testes;
