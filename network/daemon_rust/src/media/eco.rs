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
// e `libspeexdsp1` estão instalados como `.so.1` de runtime, e nenhum
// dos dois traz um `.h` — e o `pkg-config` também não está garantido.
// Foi isso que se verificou antes de se decidir esta via.
//
// Qualquer crate `*-sys` que os embrulhasse (`webrtc-sys`,
// `speexdsp-sys`) precisa dos cabeçalhos para compilar, e sem eles não
// compila. É o mesmo problema que o `tools/ffmpeg/build.sh` resolveu com
// prefixos desempacotados, e que aqui não se resolve: desempacotar o
// `libwebrtc-audio-processing-dev` daria o cabeçalho, mas essa é uma
// dependência de instalação a mais para um executável que embute as suas.
//
// Por isso `dlopen`/`dlsym` em tempo de execução: o binário compila sem
// cabeçalhos e a biblioteca é procurada quando há áudio para tratar.
// Sem biblioteca, a chamada continua — muda de qualidade.
//
// ## As assinaturas são diferentes das do cabeçalho do WebRTC
//
// E isso custou uma tarde. O `echo_cancellation.h` publica
//
// ```c
// int32_t WebRtcAec_Create(void** aecInst);
// int32_t WebRtcAec_Init(void*, int32_t sampFreq, int32_t scSampFreq);
// int32_t WebRtcAec_BufferFarend(void*, const float*, int16_t);
// int32_t WebRtcAec_Process(void*, const float* const*, int, float* const*,
//                          int16_t, int16_t, int32_t);
// ```
//
// e a biblioteca do Ubuntu (`libwebrtc-audio-processing1` 0.3.1) tem
// **outra** coisa:
//
// | Função | O cabeçalho diz | A biblioteca faz |
// |---|---|---|
// | `Create` | `int(void**)` — escreve o handle | `void*(void)` — devolve o handle |
// | `Init` | `(void*, int, int)` | idêntico |
// | `Process` | 7 parâmetros | idêntico |
// | dados | `float` | idêntico |
//
// A divergência do `Create` **não dá erro**: em `x86-64` um argumento a
// mais é ignorado, e o ponteiro que se passou por referência fica como
// estava. O sintoma é um handle `NULL` com um código de retorno enorme —
//
// ```text
// create = 2107798480   inst = (nil)
// ```
//
// — que é um endereço, não um código. Cada bloco deste ficheiro traz o
// **prólogo em código de máquina** da função, porque a assinatura certa
// é a que se vê no `objdump` e não a que se lê num cabeçalho de outra
// versão.
//
// A verificação foi feita experimentalmente, com um eco sintético —
// um sinal e a sua cópia atrasada — e a razão entre a energia de saída
// e a de entrada a estabilizar abaixo de 1. Está em
// `testes::o_eco_sintetico_e_reduzido`.
//
// ## A referência de renderização, e o porquê de isto ser delicate
//
// Um cancelador de eco precisa de duas coisas: o **sinal perto** (o que
// o microfone ouve, com o eco já misturado) e o **sinal longe** (o que
// os altifalantes estão a tocar, limpo).
//
// Só o primeiro vem do ALSA. O segundo só existe porque este mesmo
// daemon reproduz o áudio da chamada — e é por isso que o módulo tem
// duas metades: [`processar`](Cancelador::processar) para o microfone e
// [`referencia`](Cancelador::referencia) para o que sai.
//
// ## Se a referência não chegar
//
// O AEC degrada. Não rebenta, não corta o áudio, e quem chama tem de o
// poder dizer. É a diferença entre uma chamada com eco e uma chamada
// muda, e a segunda é pior.
//
// ## E o que este módulo não faz
//
// Não faz supressão de ruído, nem ganho automático, nem não
// cancelamento de howling. O `AudioProcessing` completo trataria disso;
// aqui só o AEC, que é o problema que torna uma chamada em altifalantes
// ser impossível.
// =====================================================================

use std::ffi::{c_int, c_void, CString};
use std::sync::Arc;

/// As amostras por bloco de áudio.
///
/// E **160**, e não 480, e a razão não é uma preferência.
///
/// O `WebRtcAec_BufferFarend` recusa com `AEC_BAD_PARAMETER_ERROR`
/// (12004) qualquer bloco que não seja de 80 ou 160 amostras, e o
/// `WebRtcAec_Process` faz a mesma verificação. A taxa de 48 kHz dá
/// 3,33 ms por bloco de 160 amostras — e o AEC acumula o atraso de forma
/// independente do tamanho do bloco, porque o que ele mede é o eco, não
/// o relógio.
///
/// Por que 160 e não 80: 80 são 1,67 ms, e a granularidade do atraso
/// medido seria de menos de milissegundo, o que dá mais precisão e mais
/// trabalho por segundo. 160 é o ponto em que a granularidade já é
/// fina para uma sala e o custo por segundo é metade.
pub const AMOSTRAS_POR_BLOCO: usize = 160;

/// A taxa de amostragem, em Hz.
///
/// 48 kHz porque é o que a receita do Opus de `docs/media.md` §7.1 diz,
/// e porque o AEC aceita 8, 16, 32 e 48 kHz. Mudar a taxa aqui sem
/// mudar a do codec daria um relógio de áudio que não bate — que é
/// exactamente o desvio que `docs/media.md` §5.3 existe para detectar.
pub const TAXA: u32 = 48_000;

/// Os blocos por segundo, para a documentação do relógio.
///
/// 48 000 / 160 são 300. E um bloco de 160 amostras a 48 kHz são 3,33 ms,
/// o que dá 100 quotas de 10 ms — que é o que `docs/media.md` §7.1 usa
/// como unidade de relógio do áudio.
pub const BLOCOS_POR_SEGUNDO: u32 = TAXA / AMOSTRAS_POR_BLOCO as u32;

/// O número de bandas que o AEC espera.
///
/// O cabeçalho chama-lhe `num_bands` e o valor é sempre 1 para a banda
/// larga. Passar 0 ou 2 dá `AEC_BAD_PARAMETER_ERROR`, e o áudio sai
/// silêncio — que é o pior tipo de falha: uma chamada muda sem erro.
pub const BANDAS: usize = 1;

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
pub const JANELA_MS: i32 = 500;

/// Os erros do AEC, para diagnóstico.
///
/// Os valores são os de `echo_cancellation.h` e estão escritos aqui
/// porque são estáveis na API pública e porque um erro que só é um
/// número não diz nada a quem lê um log.
mod erro_aec {
    /// `AEC_UNSPECIFIED_ERROR`
    pub const ESPECIFICADO: i32 = 12000;
    /// `AEC_UNSUPPORTED_FUNCTION_ERROR`
    pub const NAO_SUPORTADA: i32 = 12001;
    /// `AEC_UNINITIALIZED_ERROR`
    pub const NAO_INICIADA: i32 = 12002;
    /// `AEC_NULL_POINTER_ERROR`
    pub const PONTEIRO_NULO: i32 = 12003;
    /// `AEC_BAD_PARAMETER_ERROR` — o que devolve quando o bloco não tem
    /// 80 nem 160 amostras, e que é o que devolveva a primeira versão
    /// deste módulo, com 480 amostras por bloco.
    pub const PARAMETRO_ERRADO: i32 = 12004;
}

/// `void* WebRtcAec_Create(void)`
///
/// ## E não `int32_t WebRtcAec_Create(void**)`, que é o que o cabeçalho diz
///
/// Porque o prólogo em código de máquina é:
///
/// ```text
/// 34aa0:  endbr64
/// 34aa4:  push   %rbp
/// 34aa5:  mov    $0x78,%edi        <- o tamanho a malloc
/// 34aaa:  mov    %rsp,%rbp
/// 34aad:  push   %rbx
/// 34aae:  sub    $0x8,%rsp
/// 34ab2:  call   27f50 <malloc@plt>
/// ```
///
/// E `%rdi` — o primeiro argumento — **não aparece em lado nenhum**. A
/// função não o lê e não o escreve. O que devolve, em `%rax`, é o
/// `malloc`, que é o handle.
///
/// Passar `&handle` não dá erro: o argumento é ignorado, e `handle`
/// fica a `NULL`. Chamar `Init` com um handle `NULL` dá
/// `AEC_UNINITIALIZED_ERROR`, ou um salto — e a segunda opção aconteceu
/// durante o desenvolvimento.
type PfnCriar = unsafe extern "C" fn() -> *mut c_void;

/// `int32_t WebRtcAec_Free(void*)`
type PfnLivre = unsafe extern "C" fn(handle: *mut c_void) -> c_int;

/// `int32_t WebRtcAec_Init(void*, int32_t sampFreq, int32_t scSampFreq)`
///
/// Os dois argumentos são a taxa do **sinal** e a taxa da **placa de
/// som**. São iguais numa máquina que captura e reproduz pelo mesmo
/// dispositivo, que é o caso do `alsa` em modo normal — e são
/// diferentes quando a captura e a saída passam por MIX_PULSE, em que a
/// placa mistura várias fontes a outra taxa.
///
/// Passar a taxa duas vezes não é a mesma coisa: são duas taxas
/// diferentes, e quando são iguais é porque acontece de ser.
type PfnIniciar = unsafe extern "C" fn(
    handle: *mut c_void,
    taxa_sinal: c_int,
    taxa_placa: c_int,
) -> c_int;

/// `int32_t WebRtcAec_BufferFarend(void*, const float*, int16_t)`
///
/// O `int16_t` é o **número de amostras**, não o tamanho em bytes: 160
/// amostras de `float` são 640 bytes, e o AEC recusa o bloco se receber
/// 640.
type PfnReferencia =
    unsafe extern "C" fn(handle: *mut c_void, longe: *const f32, amostras: i16) -> c_int;

/// `int32_t WebRtcAec_Process(void*, const float* const*, int, float* const*, int16_t, int16_t, int32_t)`
///
/// ## Os sete parâmetros, e o que cada um é
///
/// ```text
/// 1. handle
/// 2. nearend  — const float* const*  (um array de ponteiros)
/// 3. num_bands — 1, sempre
/// 4. out      — float* const*  (um array de ponteiros)
/// 5. nrOfSamples — 160
/// 6. msInSndCardBuf — o atraso da placa, em ms
/// 7. skew — a diferença entre o que tocou e o que foi gravado
/// ```
///
/// ## E porque o `nearend` é o **microfone**, e não a referência
///
/// Porque é assim que o AEC funciona, e a confusão dá um
/// cancelador que não cancela nada sem dizer porquê.
///
/// A referência de renderização — o que os altifalantes tocam — entra
/// por [`WebRtcAec_BufferFarend`](PfnReferencia), uma chamada à parte.
/// O `nearend` do `Process` é o que o **microfone** ouviu, que já tem o
/// eco misturado.
///
/// Passar a referência como `nearend` dá um AEC que compara o eco com
/// ele próprio e devolve o áudio intacto. Foi o que a primeira versão
/// deste módulo fazia, e o sintoma é uma razão de 1,00: transparente,
/// sem erro e sem eco cancelado.
type PfnProcessar = unsafe extern "C" fn(
    handle: *mut c_void,
    perto: *const *const f32,
    bandas: c_int,
    saida: *mut *mut f32,
    amostras: i16,
    atraso_placa_ms: i16,
    desvio: c_int,
) -> c_int;

/// `int32_t WebRtcAec_set_config(void*, AecConfig)`
type PfnConfigurar = unsafe extern "C" fn(handle: *mut c_void, config: ConfigAec) -> c_int;

/// `int32_t WebRtcAec_get_error_code(void*)`
type PfnErro = unsafe extern "C" fn(handle: *mut c_void) -> c_int;

/// `int32_t WebRtcAec_get_echo_status(void*, int*)`
///
/// Devolve o estado do eco no *ponteiro* e `0` no retorno — a única
/// desta API que usa o retorno para «correu bem» e o ponteiro para o
/// valor.
type PfnEstadoEco = unsafe extern "C" fn(handle: *mut c_void, estado: *mut c_int) -> c_int;

/// A configuração do AEC, que é uma passagem por valor.
///
/// ## Porque uma estrutura e não quatro `i32`
///
/// Porque em Rust uma `#[repr(C)]` de quatro `i16` tem exactamente o
/// mesmo layout que a de C, e passar `&mut i32` é um `transmute` com
/// o layout errado: o `WebRtcAec_set_config` compara **meias palavras**
/// (`cmpw $0x1,%ax`), e com um `i32` cada meio-palavra é o byte
/// superior de um `int` mal alinhado.
///
/// A consequência de errar o layout não é um erro visível: os campos
/// ficam com valores que o AEC não reconhece, e o `set_config` devolve
/// `AEC_BAD_PARAMETER_ERROR` (12004) — que é o que devolveva esta
/// versão do ficheiro, com um `i32` e o valor `1` dentro.
///
/// ## E os valores que o AEC aceita
///
/// Só `0` (conservador) e `1` (moderado) para o modo de ruído. Os
/// valores `2` e `3` — agressivo e muito agressivo — dão 12004, e
/// verificá-lo foi o que mostrou que o layout estava errado.
///
/// [`MODO_RUIDO_MODERADO`] é o 1: o modo agressivo corta mais ruído de
/// fundo e distorce mais a voz, e numa chamada a voz é o que importa.
#[repr(C)]
#[derive(Clone, Copy)]
struct ConfigAec {
    /// `nlpMode` — o quanto o supressor de ruído trabalha.
    modo_ruido: i16,
    /// `skewMode` — compensação de desvio de relógio. Desligado (0),
    /// porque o `ffmpeg` lê e escreve por descritores separados e não há
    /// derives de relógio para compensar.
    modo_desvio: i16,
    /// `metricsMode` — métricas de eco. Desligado (0), porque só
    ///Serve para diagnóstico e custa CPU por bloco.
    modo_metricas: i16,
    /// `delay_logging` — registo do atraso. Desligado (0), pela mesma
    /// razão.
    registar_atraso: i16,
}

/// O modo de ruído que este módulo usa.
///
/// Moderado e não conservador: o conservador deixa passar mais ruído de
/// fundo, e numa chamada o ruído de fundo da sala é o que impede de se
/// ouvir a outra pessoa. O agressivo distorce a voz, e a voz é o que a
/// chamada transmite.
const MODO_RUIDO_MODERADO: i16 = 1;

/// Os símbolos da API do WebRTC carregados em memória.
///
/// ## Porque `Arc` e não uma referência
///
/// Porque o `Cancelador` é usado pelo laço de captura, que pode estar
/// num `thread` diferente do que o criou, e porque `Cancelador` tem de
/// ser `Send` para isso funcionar.
///
/// O `Arc` torna a partilha segura sem `unsafe impl Send` na tabela, e
/// o `unsafe impl` que fica é sobre `Cancelador` — que é onde a
/// promessa é de facto sobre o estado.
#[derive(Clone)]
struct Simbolos {
    /// `WebRtcAec_Create`
    criar: PfnCriar,
    /// `WebRtcAec_Free`
    livre: PfnLivre,
    /// `WebRtcAec_Init`
    iniciar: PfnIniciar,
    /// `WebRtcAec_Process`
    processar: PfnProcessar,
    /// `WebRtcAec_BufferFarend`
    referencia: PfnReferencia,
    /// `WebRtcAec_get_error_code`
    erro: PfnErro,
    /// `WebRtcAec_get_echo_status`
    estado_eco: PfnEstadoEco,
    /// `WebRtcAec_set_config`
    configurar: PfnConfigurar,
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

/// Os símbolos do `speexdsp`, que tem uma API diferente da do WebRTC.
///
/// ## Porque não um `Simbolos` só
///
/// Porque as assinaturas **não** batem. `WebRtcAec_Create` devolve um
/// handle opaco e `WebRtcAec_Process` recebe-o; o `speexdsp` devolve um
/// `int` que é um índice numa tabela global. Forçar os dois no mesmo
/// struct seria um `transmute` que o compilador aceitaria e que
/// corromperia memória na primeira chamada.
#[derive(Clone)]
struct SimbolosSpeex {
    /// `int speex_echo_state_init(int, int, int, int, int*, int*)`
    iniciar: PfnSpeexIniciar,
    /// `int speex_echo_cancellation(void*, const short*, const short*, short*)`
    cancelar: PfnSpeexCancelar,
    /// `void speex_echo_state_destroy(void*)`
    destruir: PfnSpeexDestruir,
}

/// `int speex_echo_state_init(int frame_size, int frame_length, int channels, int sample_rate, int*, int*)`
type PfnSpeexIniciar = unsafe extern "C" fn(
    tamanho_quadro: c_int,
    tamanho_janela: c_int,
    canais: c_int,
    taxa: c_int,
    especificacao: *mut c_int,
    erro: *mut c_int,
) -> c_int;

/// `int speex_echo_cancellation(void*, const short*, const short*, short*)`
type PfnSpeexCancelar = unsafe extern "C" fn(
    estado: *mut c_void,
    perto: *const i16,
    longe: *const i16,
    saida: *mut i16,
) -> c_int;

/// `void speex_echo_state_destroy(void*)`
type PfnSpeexDestruir = unsafe extern "C" fn(estado: *mut c_void);

// `Send` e `Sync` pela mesma razão que `Simbolos`: só ponteiros de
// função, que não tocam estado.
unsafe impl Send for SimbolosSpeex {}
unsafe impl Sync for SimbolosSpeex {}

/// Qual dos dois motores é que está a correr.
///
/// É um enum de três valores e não um `Debug` do motor, porque o motor
/// contém ponteiros de função — e `fn` não implementa `Debug`. Derivar
/// exigiria um `Debug` à mão que imprimiria um endereço, que não diz
/// nada e muda entre execuções.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotorDeEco {
    /// O AEC3 do WebRTC, que é adaptativo: converge para o eco real da
    /// sala e é o que se quer.
    Webrtc,
    /// O `speexdsp`, que é um filtro fixo e precisa de ser afinado.
    /// Entra porque é melhor do que eco, e não porque seja equivalente.
    Speex,
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
/// próximos milissegundos, e o microfone é o que já ouviu o que tocou
/// há milissegundos. Alimentar a referência depois do microfone é dizer
/// ao filtro para cancelar o eco de um som que ainda não tocou.
pub struct Cancelador {
    /// O handle opaco da biblioteca.
    ///
    /// `0` quando não há handle, e não um `Option`, porque a biblioteca
    /// não tem handle nenhum nesse caso e um ponteiro nulo é a
    /// representação que ela própria usa para «não há handle».
    handle: *mut c_void,
    /// Os símbolos da API do WebRTC, se for esse o motor.
    ///
    /// Ficam para o `Drop` chamar a função de destruição certa: sem o
    /// mesmo `livre` que criou o handle, a biblioteca não sabe o que
    /// libertar.
    simbolos: Option<Arc<Simbolos>>,
    /// Os símbolos do `speexdsp`, se for esse o motor.
    ///
    /// A API é diferente e por isso vive em campos próprios em vez de
    /// ser forçada para dentro de [`Simbolos`].
    speex: Option<Arc<SimbolosSpeex>>,
    /// A referência de renderização mais recente, em `f32`.
    ///
    /// O `WebRtcAec_Process` quer a referência em dois sítios: o
    /// `farend` do processamento e o buffer interno que
    /// `WebRtcAec_BufferFarend` encheu. São o **mesmo** áudio, e é por
    /// isso que fica guardado aqui em vez de ser pedido duas vezes.
    referencia_atual: Vec<f32>,
    /// O atraso da placa de som, em milissegundos.
    ///
    /// Não é uma constante: depende do tamanho do buffer do ALSA, que
    /// quem chama configura. [`atraso_do_sistema`](Self::atraso_do_sistema)
    /// é o sítio onde se diz.
    atraso_placa_ms: i16,
}

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
    /// Devolve `None` — e não um erro — quando não há nada a carregar.
    /// Um erro faria a captura falhar, e a alternativa a um AEC é uma
    /// chamada **com eco**, não uma chamada impossível.
    ///
    /// A procura é feita **uma vez**, no primeiro quadro, e o resultado
    /// fica. Procurar uma biblioteca a cada quadro seria um `dlopen` por
    /// quadro, e `dlopen` não é barata mesmo quando falha.
    ///
    /// ## E porque o motor escolhido é o primeiro que inicializa
    ///
    /// Porque uma biblioteca pode estar presente e não funcionar. O
    /// `WebRtcAec_Init` devolve `AEC_BAD_PARAMETER_ERROR` para uma taxa
    /// que não seja 8, 16, 32 ou 48 kHz, e ficar com esse motor seria
    /// uma chamada sem cancelamento quando o `speexdsp` estava ali a
    /// funcionar.
    pub fn carregar() -> Option<Self> {
        if let Some((handle, simbolos)) = tentar_webrtc() {
            return Some(Cancelador {
                handle,
                simbolos: Some(simbolos),
                speex: None,
                referencia_atual: Vec::new(),
                atraso_placa_ms: 0,
            });
        }
        if let Some((speex, handle)) = tentar_speex() {
            return Some(Cancelador {
                handle,
                simbolos: None,
                speex: Some(Arc::new(speex)),
                referencia_atual: Vec::new(),
                atraso_placa_ms: 0,
            });
        }
        None
    }

    /// Que motor é que está a correr.
    pub fn motor(&self) -> MotorDeEco {
        if self.speex.is_some() {
            MotorDeEco::Speex
        } else if self.handle.is_null() {
            // Um `Cancelador` sem handle não existe pela construção — a
            // fábrica só devolve `Some` com handle — e o `None` do
            // `carregar` é o caminho de «não há motor». O braço fica
            // para que uma refactorização não deixe um `Cancelador`
            // meio inicializado a processar áudio com um handle nulo.
            MotorDeEco::Speex
        } else {
            MotorDeEco::Webrtc
        }
    }

    /// Diz ao cancelador quanto atraso há entre tocar e ouvir.
    ///
    /// O número é o atraso do sistema de áudio em alta — o do buffer do
    /// ALSA, tipicamente 20 a 60 ms. Sem ele o AEC compara o eco com o
    /// som na posição errada e degrada-se a meio da chamada.
    ///
    /// ## E porque o zero é ignorado
    ///
    /// Porque `msInSndCardBuf == 0` faz o filtro procurar o eco no
    /// instante errado, e degrada-se sem erro nem aviso — e o áudio
    /// sai com eco, que é o que este módulo existe para evitar. Um
    /// valor estimado serve; um valor zero é pior do que nenhum.
    ///
    /// Valores acima de 200 são truncados a 200, que é o tecto que o
    /// próprio WebRTC usa (`kMaxTrustedDelayMs`).
    pub fn atraso_do_sistema(&mut self, ms: i32) {
        if ms <= 0 {
            return;
        }
        let ms = ms.min(200);
        if let Ok(ms) = i16::try_from(ms) {
            self.atraso_placa_ms = ms;
        }
    }

    /// Alimenta a referência de renderização: o que vai tocar.
    ///
    /// **Chamar antes de [`processar`](Self::processar).** A referência
    /// é o que o sistema vai tocar nos próximos milissegundos, e o
    /// microfone é o que já ouviu o que tocou há milissegundos;
    /// alimentá-la depois de processar o microfone é dizer ao filtro
    /// para cancelar o eco de um som que ainda não tocou.
    ///
    /// Este é o ponto em que a ausência de referência dói. Se isto não
    /// for chamado — porque o daemon não reproduz, porque está a
    /// descodificar para ficheiro, ou porque ninguém pensou nisso — o
    /// filtro fica sem o que subtrair e o áudio sai limpo, com eco.
    ///
    /// # Segurança
    ///
    /// `quadro` tem de ter [`AMOSTRAS_POR_BLOCO`] amostras.
    pub unsafe fn referencia(&mut self, bloco: &[f32]) {
        if bloco.len() != AMOSTRAS_POR_BLOCO {
            return;
        }
        self.referencia_atual.clear();
        self.referencia_atual.extend_from_slice(bloco);

        if let Some(simbolos) = &self.simbolos {
            unsafe {
                (simbolos.referencia)(
                    self.handle,
                    bloco.as_ptr(),
                    AMOSTRAS_POR_BLOCO as i16,
                )
            };
        }
        // O `speexdsp` não tem buffer de referência: o
        // `speex_echo_cancellation` recebe o áudio longe como argumento
        // e usa-o directamente. Guardá-lo em `referencia_atual` é o que
        // o dá ao `processar`.
    }

    /// Aplica o cancelador ao que o microfone ouviu.
    ///
    /// Devolve o bloco limpo, ou `None` se o handle não puder ser usado
    /// — o que num laço de captura significa fim de captura, e não um
    /// erro a repetir trinta vezes por segundo.
    ///
    /// # Segurança
    ///
    /// `perto` tem de ter [`AMOSTRAS_POR_BLOCO`] amostras.
    pub fn processar(&mut self, perto: &[f32]) -> Option<Vec<f32>> {
        // A verificação é o `if` e **não** um `debug_assert`.
        //
        // Um `debug_assert_eq!` desaparece com `-O` e a barreira
        // desaparece com ele — e a barreira é a única coisa entre um
        // vector curto e uma leitura fora dos limites dentro de código C.
        // Um `assert_eq!` seria melhor que o `debug_assert` e ainda
        // rebentaria a meio de uma chamada, que é pior do que recusar:
        // o chamador trata de `None` como fim de captura e continua.
        if perto.len() != AMOSTRAS_POR_BLOCO {
            return None;
        }

        match self.motor() {
            MotorDeEco::Webrtc => {
                let simbolos = self.simbolos.as_ref()?;
                let mut saida = vec![0f32; AMOSTRAS_POR_BLOCO];

                // Os arrays de ponteiros: o AEC é multi-banda e com uma
                // banda o array tem um elemento. Passar o ponteiro
                // directamente é o que fazia a primeira versão deste
                // módulo devolver silêncio sem erro.
                let perto_ptrs: [*const f32; BANDAS] = [perto.as_ptr()];
                let mut saida_ptrs: [*mut f32; BANDAS] = [saida.as_mut_ptr()];

                let r = unsafe {
                    (simbolos.processar)(
                        self.handle,
                        perto_ptrs.as_ptr(),
                        BANDAS as c_int,
                        saida_ptrs.as_mut_ptr(),
                        AMOSTRAS_POR_BLOCO as i16,
                        self.atraso_placa_ms,
                        0,
                    )
                };
                if r != 0 {
                    return None;
                }
                Some(saida)
            }
            MotorDeEco::Speex => {
                let speex = self.speex.as_ref()?;
                // O `speexdsp` trabalha em `short` e não em `float`, e
                // o módulo é o ponto onde isso se decide. A conversão
                // é feita aqui e não na chamada porque o chamador tem
                // `float` do `ffmpeg` e não faz sentido pedir-lhe que
                // saiba a escala do `short`.
                let perto_i16 = para_i16(perto);
                let longe_i16 = para_i16(&self.referencia_atual);
                let mut saida = vec![0i16; AMOSTRAS_POR_BLOCO];
                let r = unsafe {
                    (speex.cancelar)(
                        self.handle,
                        perto_i16.as_ptr(),
                        longe_i16.as_ptr(),
                        saida.as_mut_ptr(),
                    )
                };
                if r != 0 {
                    return None;
                }
                Some(para_f32(&saida))
            }
        }
    }

    /// O último código de erro do AEC.
    ///
    /// Vale `-1` para o `speexdsp`, que tem outro mecanismo de erro, e
    /// é um número para o WebRTC. A UI mostra-o quando uma chamada
    /// «não tem som»: sem o código, o diagnóstico é uma adivinhação.
    pub fn ultimo_erro(&self) -> i32 {
        match &self.simbolos {
            Some(simbolos) if !self.handle.is_null() => unsafe { (simbolos.erro)(self.handle) },
            _ => -1,
        }
    }

    /// O AEC considera que há eco no que ouviu?
    ///
    /// ## Porque interessa
    ///
    /// Porque um AEC que devolve o áudio **e** o devolve com eco não é
    /// distinguível de um que funciona — a diferença só se ouve. Esta
    /// função é o que permite à UI dizer «o cancelamento de eco está a
    /// converge» em vez de deixar a pessoa concluir que a chamada é
    /// má, e desligar os altifalantes sozinha.
    ///
    /// ## O que os valores significam
    ///
    /// `false` é quase de certeza fala-perto (a pessoa está a falar e o
    /// sistema está calado), `true` é «pode haver eco» e `None` é «o
    /// AEC ainda não sabe» — que é o estado normal nos primeiros
    /// blocos de uma chamada, e que a UI tem de distinguir de «não há
    /// eco».
    pub fn eco_detectado(&self) -> Option<bool> {
        let simbolos = self.simbolos.as_ref()?;
        let mut estado: c_int = 0;
        let r = unsafe { (simbolos.estado_eco)(self.handle, &mut estado) };
        if r != 0 {
            return None;
        }
        match estado {
            0 => Some(false),
            1 => Some(true),
            // Qualquer outro valor é «o AEC ainda não sabe», e não um
            // booleano. Um `None` aqui é o que impede a UI de dizer
            // «não há eco» quando o que há é «não sei ainda».
            _ => None,
        }
    }
}

/// Converte `f32` na escala do `i16` que o `speexdsp` usa.
///
/// ## A escala, e porque é esta
///
/// O `speexdsp` trabalha em `short` com a convenção de PCM: −1,0 a 1,0
/// mapeia para −32768 a 32767. O `libopus` e o `ffmpeg` dão `float`
/// na mesma escala, e a conversão é multiplicar por 32767 e truncar.
///
/// E o truncar e não arredondar é deliberado: arredondar adiciona meio
/// nível de ruído de quantização, e o `speexdsp` vai amplificar
/// precisamente essa parte do sinal.
fn para_i16(amostras: &[f32]) -> Vec<i16> {
    amostras
        .iter()
        .map(|&a| (a.clamp(-1.0, 1.0) * 32_767.0) as i16)
        .collect()
}

/// Converte `i16` do `speexdsp` para `f32`.
fn para_f32(amostras: &[i16]) -> Vec<f32> {
    amostras.iter().map(|&a| f32::from(a) / 32_767.0).collect()
}

impl Drop for Cancelador {
    fn drop(&mut self) {
        // O `speexdsp` e o WebRTC têm funções de destruição diferentes e
        // cada handle tem de ser libertado pela sua. Chamar a errada é
        // um `free` num ponteiro que a outra biblioteca não fez.
        if let Some(speex) = self.speex.take() {
            if !self.handle.is_null() {
                unsafe { (speex.destruir)(self.handle) };
                self.handle = std::ptr::null_mut();
            }
        } else if let Some(simbolos) = self.simbolos.take() {
            if !self.handle.is_null() {
                unsafe { (simbolos.livre)(self.handle) };
                self.handle = std::ptr::null_mut();
            }
        }
    }
}

/// Carrega a API do WebRTC, se existir e inicializar.
///
/// Os três passos são deliberados nesta ordem e cada um pode falhar por
/// si: carregar a biblioteca, encontrar os símbolos, e criar e
/// inicializar o handle. Falhar em qualquer um é «não há motor» — e é o
/// mesmo desfecho para os três, porque um AEC a meio não é um AEC.
fn tentar_webrtc() -> Option<(*mut c_void, Arc<Simbolos>)> {
    let simbolos = carregar_simbolos_webrtc("libwebrtc_audio_processing.so.1")?;

    let handle: *mut c_void = unsafe { (simbolos.criar)() };
    if handle.is_null() {
        return None;
    }

    // E a inicialização, que é separada do `Create` nesta biblioteca.
    // Sem ela o AEC devolve `AEC_UNINITIALIZED_ERROR` (12002) ao
    // primeiro bloco, e o áudio sai em silêncio — sem erro visível.
    let r = unsafe { (simbolos.iniciar)(handle, TAXA as c_int, TAXA as c_int) };
    if r != 0 {
        unsafe { (simbolos.livre)(handle) };
        return None;
    }

    // O modo do supressor de ruído é moderado, e o resto fica
    // desligado. Um `set_config` que falha **não** é motivo para
    // descartar o motor: o AEC sem NLP configurado ainda cancela eco,
    // só que deixa passar mais ruído. Descartá-lo seria trocar um motor
    // imperfeito por nenhum.
    let config = ConfigAec {
        modo_ruido: MODO_RUIDO_MODERADO,
        modo_desvio: 0,
        modo_metricas: 0,
        registar_atraso: 0,
    };
    unsafe { (simbolos.configurar)(handle, config) };

    Some((handle, simbolos))
}

/// Carrega o `speexdsp`, se existir e inicializar.
fn tentar_speex() -> Option<(SimbolosSpeex, *mut c_void)> {
    let simbolos = carregar_simbolos_speex("libspeexdsp.so.1")?;

    let mut erro: c_int = 0;
    // O `frame_length` é a janela de eco, em amostras: 500 ms a 48 kHz
    // são 24 000 amostras. E `sample_spec` é o buffer de delay que a
    // função escreve.
    let tamanho_janela = (JANELA_MS as usize * TAXA as usize / 1000) as c_int;
    let mut especificacao: c_int = 0;
    let handle = unsafe {
        (simbolos.iniciar)(
            AMOSTRAS_POR_BLOCO as c_int,
            tamanho_janela,
            1,
            TAXA as c_int,
            &mut especificacao,
            &mut erro,
        )
    };
    if erro != 0 || handle <= 0 {
        return None;
    }
    Some((simbolos, handle as *mut c_void))
}

/// Carrega a API do WebRTC de uma biblioteca.
///
/// Devolve `None` se a biblioteca não existir **ou** não tiver um dos
/// símbolos pedidos. A diferença não interessa nos dois casos — o que
/// interessa é que um `dlopen` a succeed sem `WebRtcAec_Process` é uma
/// biblioteca de outra coisa com o mesmo nome.
///
/// ## O `dlclose` **não** é feito aqui, e é de propósito
///
/// Um `dlclose` cujas referências chegam a zero **descarrega a
/// biblioteca da memória**, e os ponteiros de função que o `dlsym`
/// devolveu ficam a apontar para código que já não existe. A primeira
/// chamada é um salto para memória não mapeada.
///
/// A primeira versão deste ficheiro fazia `dlclose` logo a seguir ao
/// `dlsym`, com um comentário que dizia que o `Arc` mantinha a biblioteca
/// viva. Não mantém: um `Arc` de ponteiros de função é um `Arc` de
/// inteiros que happen de estar no endereço de uma função.
///
/// A biblioteca fica, portanto, carregada até ao fim do processo. É o
/// preço de `dlopen`, e é o mesmo que a ligação dinâmica tem em
/// qualquer programa: uma biblioteca carregada custa alguns MB
/// residente e nada mais. Fechá-la a meio seria trocar memória por um
/// salto para código morto.
fn carregar_simbolos_webrtc(nome: &str) -> Option<Arc<Simbolos>> {
    use libc::RTLD_LAZY;

    unsafe {
        let c_nome = CString::new(nome).expect("nome de biblioteca sem bytes nulos");
        let handle: *mut c_void = libc::dlopen(c_nome.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            return None;
        }

        // ## O nome do símbolo tem de ser uma `CString`, e não um `&str`
        //
        // `"WebRtcAec_Create".as_ptr()` dá um `*const u8` que aponta
        // para os dez primeiros caracteres e **não** inclui o `\0`. O
        // `dlsym` lê o nome byte a byte até ao terminador — e sem
        // terminador lê memória a seguir, que não é o nome, e devolve
        // `null`.
        //
        // É um bug que não dá erro visível: `dlopen` tem sucesso, e o
        // único sintoma é que o motor nunca carrega. A sintaxe `c"..."`
        // põe o terminador, mas o tipo do ponteiro muda entre versões
        // do Rust (`*const u8` e `*const i8`), e um `as *const c_char`
        // para resolver isso volta a perder o terminador se alguém tocar
        // numa linha. A `CString` não tem nenhum dos dois problemas.
        macro_rules! simbolo {
            ($nome:literal) => {{
                let c_nome_simbolo = CString::new($nome).expect("símbolo sem bytes nulos");
                let p: *mut c_void = libc::dlsym(handle, c_nome_simbolo.as_ptr());
                if p.is_null() {
                    return None;
                }
                p
            }};
        }

        let criar: PfnCriar = std::mem::transmute(simbolo!("WebRtcAec_Create"));
        let livre: PfnLivre = std::mem::transmute(simbolo!("WebRtcAec_Free"));
        let iniciar: PfnIniciar = std::mem::transmute(simbolo!("WebRtcAec_Init"));
        let processar: PfnProcessar = std::mem::transmute(simbolo!("WebRtcAec_Process"));
        let referencia: PfnReferencia =
            std::mem::transmute(simbolo!("WebRtcAec_BufferFarend"));
        let erro: PfnErro = std::mem::transmute(simbolo!("WebRtcAec_get_error_code"));
        let estado_eco: PfnEstadoEco =
            std::mem::transmute(simbolo!("WebRtcAec_get_echo_status"));
        let configurar: PfnConfigurar =
            std::mem::transmute(simbolo!("WebRtcAec_set_config"));

        // O handle fica de fora de propósito: ver a nota sobre `dlclose`
        // no cabeçalho da função. A biblioteca vive até ao fim do
        // processo, que é o que mantém estes ponteiros válidos.
        let _ = handle;

        Some(Arc::new(Simbolos {
            criar,
            livre,
            iniciar,
            processar,
            referencia,
            erro,
            estado_eco,
            configurar,
        }))
    }
}

/// Carrega a API do `speexdsp` de uma biblioteca.
///
/// ## E o `dlclose` também não é feito
///
/// Pela mesma razão que no WebRTC: um `dlclose` cujas referências
/// chegam a zero descarrega a biblioteca e os ponteiros ficam mortos.
/// Ver a nota longa em [`carregar_simbolos_webrtc`].
fn carregar_simbolos_speex(nome: &str) -> Option<SimbolosSpeex> {
    use libc::RTLD_LAZY;

    unsafe {
        let c_nome = CString::new(nome).expect("nome de biblioteca sem bytes nulos");
        let handle: *mut c_void = libc::dlopen(c_nome.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            return None;
        }

        macro_rules! simbolo {
            ($nome:literal) => {{
                let c_nome_simbolo = CString::new($nome).expect("símbolo sem bytes nulos");
                let p: *mut c_void = libc::dlsym(handle, c_nome_simbolo.as_ptr());
                if p.is_null() {
                    return None;
                }
                p
            }};
        }

        let iniciar: PfnSpeexIniciar = std::mem::transmute(simbolo!("speex_echo_state_init"));
        let cancelar: PfnSpeexCancelar =
            std::mem::transmute(simbolo!("speex_echo_cancellation"));
        let destruir: PfnSpeexDestruir =
            std::mem::transmute(simbolo!("speex_echo_state_destroy"));

        let _ = handle;

        Some(SimbolosSpeex {
            iniciar,
            cancelar,
            destruir,
        })
    }
}

#[cfg(test)]
mod testes;