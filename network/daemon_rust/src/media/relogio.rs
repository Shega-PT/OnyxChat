// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! O relógio de media: tempo que passa no ecrã, não no relógio de parede.
//!
//! [`docs/media.md`](../../../../docs/media.md) §5.
//!
//! ## O invariante, e porque é o que é
//!
//! O `media_ms` é **tempo de media**, monotónico, e nunca tempo de
//! parede. Um [`std::time::Instant`] do sistema não entra nesta
//! aritmética: entra apenas para decidir *quando apresentar*, e essa
//! decisão é de quem apresenta, não de quem conta.
//!
//! A distinção não é preciosismo. Se o relógio de vídeo fosse o relógio
//! do sistema, um `sleep` de 20 ms num laço de apresentação produziria
//! um atraso de 20 ms por frame — 1,2 segundos num vídeo de um minuto —
//! e nenhum sistema de sincronia perceberia porque é que o áudio e o
//! vídeo se separam.
//!
//! ## Quem avança, e com quê
//!
//! **O áudio é o mestre**, porque o ritmo dele é constante por
//! construção: `bytes / bytes_por_ms` é exacto com `-vbr off` (§5.3), e
//! o do vídeo não — o `fps` varia com o que a fonte está a fazer.
//!
//! ## E o que não há aqui
//!
//! Não há `sleep`, nem `Instant`, nem espera. Isto é aritmética sobre
//! números, e é por isso que cada propriedade desta secção se testa sem
//! nenhum ecrã.

use super::erros::ErroMedia;

/// A margem de divergência entre o clock contado e o declarado, em ms.
///
/// §5.4. Cinco e não um porque o clock declarado **é uma contagem** do
/// emissor, e as duas contagens divergem por arredondamento de um
/// frame de áudio. Um milissegundo de tolerância transformaria o jitter
/// normal em «stream dessincronizada».
pub const TOLERANCIA_MS: i64 = 5;

/// O atraso acima do qual um frame é descartado, em ms.
///
/// §5.6: três vezes o piso do Tor (~40 ms de ida e volta). Abaixo
/// disso o jitter normal seria reportado como perda de sincronia.
pub const LIMITE_MS: i64 = 120;

/// RelogioMedia { media_ms: i64, bytes_por_ms: u32 }.
///
/// O nome é o do §5.1 porque o campo é o que o resto do módulo lê, e
/// dois nomes para a mesma coisa é um sítio onde os dois divergem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelogioMedia {
    /// Tempo de media em milissegundos, monotónico.
    pub media_ms: i64,
    /// Bytes de áudio por milissegundo — **declarado**, nunca presumido.
    ///
    /// É `max(1)` em uso: um `0` faria a divisão por zero, e um store
    /// com `bytes_por_ms` a zero é melhor tratado como «um byte por
    /// milissegundo» do que como um pânico no meio de uma chamada.
    ///
    /// §5.3: `bitrate_bps / 8000`. É declarado no manifesto precisamente
    /// porque com Opus os frames são de duração variável e o valor
    /// fixo seria uma média — e um relógio construído sobre uma média
    /// acumula desvio sem nenhum sinal.
    pub bytes_por_ms: u32,
}

impl RelogioMedia {
    /// Um relógio parado, com o `bytes_por_ms` declarado.
    pub fn novo(bytes_por_ms: u32) -> Self {
        Self {
            media_ms: 0,
            bytes_por_ms,
        }
    }

    /// Avança com o áudio — o relógio mestre.
    ///
    /// ```text
    /// media_ms += bytes_recebidos / bytes_por_ms
    /// ```
    ///
    /// A divisão é **inteira** e o resto fica acumulado — em **bytes**,
    /// não em milissegundos.
    ///
    /// O resto é em bytes porque é o resto dos bytes que não deu um
    /// milissegundo inteiro. A primeira versão acumulava
    /// `bytes_por_ms * resto_ms + bytes`, o que misturava as duas
    /// unidades: um resto de 5 ms a 7 bytes/ms valia 35 bytes, e o
    /// relógio andava a sete vezes. O teste do relógio monotónico
    /// apanhou — os números davam 30 ms onde deviam dar 5.
    ///
    /// Sem resto nenhum, um stream a 4 bytes/ms e 50 fps perderia 40% do
    /// tempo: cada pacote de 1 ms arredondava para zero e o relógio não
    /// andava. Com o resto, a média é exacta em qualquer número de
    /// pacotes.
    pub fn avancar_audio(&mut self, bytes: usize, resto_bytes: &mut u32) {
        let taxa = u64::from(self.bytes_por_ms.max(1));
        let total = u64::from(*resto_bytes) + bytes as u64;
        let avancado = total / taxa;
        *resto_bytes = (total % taxa) as u32;

        self.media_ms += avancado as i64;
    }

    /// Os milissegundos de video que um quadro ocupa.
    ///
    /// `1000 / fps` em inteiro, e não em ponto flutuante: a aritmética
    /// é feita uma vez por frame e o resultado entra num `i64` que é
    /// comparado com limites. Um `f64` aqui seria um `as i64` algures
    /// e uma comparação com `120.0` algures, e os dois `as` podem
    /// discordar em 1 ms — que é exactamente a tolerância do §5.4.
    ///
    /// ## E quando o `fps` é tão alto que dá zero
    ///
    /// 2000 fps dá `1000/2000 = 0`. A resposta é **1**, e não 0: um
    /// frame que não avança o relógio é um frame que o relógio não
    /// consegue ordenar, e a ordenação é o que esta aritmética serve.
    pub fn ms_por_frame(fps: u16) -> i64 {
        debug_assert!(fps > 0, "fps zero");
        (1000 / i64::from(fps.max(1))).max(1)
    }

    /// Compara o clock contado com o declarado (§5.4).
    ///
    /// ```text
    /// |declarado − contado| > TOLERANCIA_MS → stream dessincronizada
    /// ```
    pub fn verificar(&self, declarado: i64) -> Result<(), ErroMedia> {
        let desvio = (declarado - self.media_ms).abs();
        if desvio > TOLERANCIA_MS {
            return Err(ErroMedia::RelogioDivergente {
                declarado: declarado as i32,
                contado: self.media_ms as i32,
                tolerancia: TOLERANCIA_MS as i32,
            });
        }
        Ok(())
    }
}

/// O que fazer com um frame de vídeo que chegou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisao {
    /// Apresentar já: o vídeo está à frente do áudio.
    Apresentar,
    /// Apresentar quando o áudio chegar.
    Sincronizar,
    /// Descartar: o vídeo está tanto atrás que já não é sincronia.
    Descartar,
}

/// A regra dos três cenários (§5.5).
///
/// ```text
/// atraso = relogio_video_ms − relogio_audio_ms
///
/// atraso >  +LIMITE  →  DESCARTAR o frame
/// |atraso| ≤ LIMITE  →  APRESENTAR
/// atraso <  −LIMITE  →  APRESENTAR já
/// ```
///
/// ## Porque [`Decisao::Sincronizar`] **não** significa «dorme»
///
/// Esta é a correcção mais importante de §5.5. Sincronizar é uma
/// **decisão**, e a apresentação é de outra pessoa e de outro módulo —
/// o anel, que decide quando presents. Um `sleep` aqui transformaria um
/// problema de rede (atraso) num problema de renderização (congelamento
/// do receptor), e o vídeo chegar mais tarde não ficaria melhor.
///
/// Tor tem TCP por baixo: **atrasa muito e perde pouco**. A perda é o
/// caso raro e é tratada pelo descarte; o atraso é o caso comum e é
/// tratado por quem apresenta.
pub fn decidir(video_ms: i64, audio_ms: i64) -> Decisao {
    let atraso = video_ms - audio_ms;
    if atraso > LIMITE_MS {
        Decisao::Descartar
    } else if atraso < -LIMITE_MS {
        Decisao::Apresentar
    } else {
        Decisao::Sincronizar
    }
}

/// Um descarte de vídeo é um **evento de divergência**.
///
/// §8.5: marca a próxima unidade como keyframe. Existe como tipo
/// próprio — e não um `bool` solto — porque quem o recebe tem de saber
/// *que* houve um descarte, e um `bool` que éposto a `false` noutro
/// sítio é um `bool` que se perde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divergencia {
    /// Quando, em tempo de media, o descarte aconteceu.
    pub em_ms: i64,
    /// O `seq` da unidade que se perdeu.
    pub seq_perdido: u32,
}

#[cfg(test)]
mod testes;
