// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/desempate.rs — o áudio não pode chegar 5× mais rápido que o vídeo
// ---------------------------------------------------------------------
// =====================================================================
// media/desempate.rs — o áudio não pode chegar muito mais rápido que o vídeo
// ---------------------------------------------------------------------
// ## O problema
//
// O áudio e o vídeo são dois ritmos, e o `ffmpeg` entrega cada um à
// sua. 30 fps de vídeo são 33,3 ms por quadro; áudio a 20 ms são 50
// unidades por segundo.
//
// Feitas as contas, **os dois duram o mesmo tempo**: em 10 s o vídeo
// oferece 10 000 ms e o áudio também. A diferença é de 1 % — o
// arredondamento de 33,3 para 33, e o `ffmpeg` a escrever o último
// bloco de áudio mais tempo do que o último quadro de vídeo.
//
// ## E onde está então o problema?
//
// Não num factor multiplicativo. Num **acúmulo**: o áudio chega em
// rajadas, e o `ffmpeg` não acopla os dois fluxos. Se o emissor
// escreve o áudio assim que o tem, e o vídeo quando o tem, o áudio
// ganha corrida numa rajada e perde noutra. O buffer do receptor
// enche e alguém tem de o descartar.
//
// Descartar áudio **ouve-se**: é um estalo, e depois a voz volta
// atrasada. Descartar vídeo não se ouve — é um quadro que o seguinte
// torna irrelevante (§8.2). Por isso a assimetria: o áudio é o que se
// tem de proteger, e é por isso que o limitador mede os dois em
// **tempo** e não em contagem.
//
// Uma versão anterior deste ficheiro dizia que o áudio chegava «1,67×
// mais rápido» e comparava 20 ms com 33 ms como se um deles fosse
// contagem de unidades e o outro tempo. Não é, e a conclusão daí
// tirada — cortar 95 % do áudio — partia de uma divisão errada.
// =====================================================================
use super::relogio::RelogioMedia;

/// A janela de acúmulo por omissão, em milissegundos.
///
/// Ver a nota de [`DesempateAudioVideo::com_janela`] para a conta.
pub const JANELA_MS: i64 = 200;

/// O que o desempate decidiu fazer com uma unidade de áudio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desempate {
    /// A unidade entra: o áudio está atrasado em relação ao vídeo, e
    /// precisa de catching up.
    Emitir,
    /// A unidade espera: há áudio suficiente para o tempo já
    /// percorrido pelo vídeo.
    Esperar,
}

/// Mantém o áudio e o vídeo ao mesmo ritmo.
///
/// ## O que ele **não** faz
///
/// Não decide *qual* é o ritmo. Quem decide é o relógio (§5): o áudio
/// tem o seu relógio declarado e o vídeo o seu. Este módulo só impede
/// que um dos dois corra muito mais depressa que o outro — e o faz
/// contando **tempo**, não unidades, porque 48 kHz e 30 fps não têm
/// unidades em comum.
///
/// ## Por que é `x` para `x` e não uma janela
///
/// Uma janela de áudio com folga é mais simples e mais difícil de
///explainar. A regra `x` para `x` é verificável de uma linha: ao fim
/// de `n` milissegundos de vídeo chegou áudio de `n` milissegundos. É
/// o que se pode testar.
#[derive(Debug)]
pub struct DesempateAudioVideo {
    /// Milissegundos de **vídeo** já emitidos.
    ///
    /// O vídeo é a referência porque o áudio é o que se estraga: um
    /// frame perdido é invisível, uma amostra perdida ouve-se.
    ///
    /// A escolha não é simétrica por acaso. É também a que dá menos
    /// latência: se fosse o áudio a dar o ritmo e o vídeo a acompanhar,
    /// um áudio atrasado arrastaria o vídeo e a imagem pararia.
    ms_video: i64,
    /// Milissegundos de **áudio** já emitidos.
    ms_audio: i64,
    /// Janela de **acúmulo**, em milissegundos.
    ///
    /// Não é uma tolerância que se pode gastar uma vez e acabou: é o
    /// tamanho do buffer que se aceita ter à frente do vídeo. Passados
    /// `janela_ms`, o áudio espera pelo vídeo.
    ///
    /// ## Porque 200 ms
    ///
    /// O áudio e o vídeo duram o mesmo tempo, e a diferença entre eles
    /// é o jitter da entrega: o `ffmpeg` escreve o áudio e o vídeo por
    /// descritores separados e não os acopla, e a rede entrega em
    /// rajadas. Numa rajada chega 200 ms de áudio de uma vez.
    ///
    /// O limite tem de ser maior do que a rajada, ou o áudio é cortado
    /// em cada rajada — que é o que uma versão anterior deste
    /// limitador fazia, e o resultado era uma chamada com 95 % do áudio
    /// perdido. E tem de ser pequeno, ou o áudio atrasa-se de mais e a
    /// voz chega tarde.
    ///
    /// 200 ms é a janela de jitter de rede de uma chamada normal: abaixo
    /// disso corta-se áudio que ia chegar; acima disso a voz atrasa.
    ///
    /// ## O que se descarta quando a janela transborda
    ///
    /// **Nada.** O áudio espera. Descartar áudio é o que o limitador
    /// existe para evitar, e um emissor que bloqueia à espera do vídeo
    /// é um emissor lento — que é um custo muito menor do que uma voz
    /// entrecortada.
    janela_ms: i64,
}

impl DesempateAudioVideo {
    /// Um desempate com a janela de 200 ms.
    pub fn novo() -> Self {
        DesempateAudioVideo {
            ms_video: 0,
            ms_audio: 0,
            janela_ms: JANELA_MS,
        }
    }

    /// Um desempate com a janela à medida.
    ///
    /// Existe para os testes e para uma chamada com rede pior do que o
    /// normal. Não é um parâmetro de produção: mudar isto muda a
    /// latência do áudio, e quem decide isso é §5.
    pub fn com_janela(janela_ms: i64) -> Self {
        DesempateAudioVideo {
            janela_ms,
            ..DesempateAudioVideo::novo()
        }
    }

    /// O vídeo avançou `ms`.
    ///
    /// O vídeo é a referência porque o áudio é o que se estraga: um
    /// quadro perdido é invisível, uma amostra perdida ouve-se.
    pub fn registar_video(&mut self, ms: i64) {
        self.ms_video = self.ms_video.saturating_add(ms);
    }

    /// Decide o que se faz com uma unidade de áudio que cobre `ms`.
    ///
    /// Se o áudio, depois desta unidade, couber na janela, emite. Se a
    /// ultrapassaria, espera.
    ///
    /// A comparação é `>` e não `>=`: o caso exacto é o estado de
    /// equilíbrio, e um limitador que reprova o equilíbrio vê a chamada
    /// parar.
    pub fn registar_audio(&mut self, ms: i64) -> Desempate {
        // O áudio nunca recua. Um `clock_ms` menor que o anterior
        // significa que o emissor recomeçou, e somar na mesma daria
        // uma janela que cresce sem parar.
        self.ms_audio = self.ms_audio.max(self.ms_audio.saturating_add(0));
        let seguinte = self.ms_audio.saturating_add(ms);

        if seguinte > self.ms_video.saturating_add(self.janela_ms) {
            Desempate::Esperar
        } else {
            self.ms_audio = seguinte;
            Desempate::Emitir
        }
    }

    /// Onde está o áudio em relação ao vídeo.
    ///
    /// Positivo é áudio adiantado, negativo é áudio atrasado.
    pub fn desvio_ms(&self) -> i64 {
        self.ms_audio.saturating_sub(self.ms_video)
    }

    /// A janela actual, em milissegundos.
    pub fn janela_ms(&self) -> i64 {
        self.janela_ms
    }

    /// Repõe os dois relógios a zero.
    ///
    /// Chamar isto quando o stream **não** recomeça é um bug: um
    /// limitador com o áudio muito adiantado rejeita as unidades
    /// seguintes até o tempo voltar a apanhar, e a chamada fica muda.
    pub fn repor(&mut self) {
        self.ms_video = 0;
        self.ms_audio = 0;
    }
}

impl Default for DesempateAudioVideo {
    fn default() -> Self {
        Self::novo()
    }
}

/// O limitador ligado ao relógio de media, que é quem dá os tempos.
///
/// ## Porque o relógio e não o chamador
///
/// Porque `media_ms` é a mesma base que `clock_ms` leva no preâmbulo
/// (§5.4), e um limitador com uma base diferente do relógio produz
/// decisões que não correspondem ao que o receptor vai ver. Passar o
/// relógio é o que garante que emissor e receptor contam a mesma
/// coisa.
pub fn desempate_com(relogio: &RelogioMedia) -> DesempateComRelogio<'_> {
    DesempateComRelogio {
        interno: DesempateAudioVideo::novo(),
        relogio,
    }
}

/// [`DesempateAudioVideo`] com o relógio anexado.
pub struct DesempateComRelogio<'a> {
    interno: DesempateAudioVideo,
    relogio: &'a RelogioMedia,
}

impl<'a> DesempateComRelogio<'a> {
    /// Um quadro de vídeo de `ms` chegou.
    pub fn registar_video(&mut self, ms: i64) {
        self.interno.registar_video(ms);
    }

    /// Uma unidade de áudio de `ms` chegou; decide-se o seu destino.
    pub fn registar_audio(&mut self, ms: i64) -> Desempate {
        self.interno.registar_audio(ms)
    }

    /// O relógio de media, para quem precisar do tempo actual.
    pub fn media_ms(&self) -> i64 {
        self.relogio.media_ms
    }

    /// O desvio actual, em milissegundos.
    pub fn desvio_ms(&self) -> i64 {
        self.interno.desvio_ms()
    }
}

#[cfg(test)]
mod testes;
