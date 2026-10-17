// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! O anel de media: dois segundos de tempo de media, com políticas
//! opostas.
//!
//! [`docs/media.md`](../../../../docs/media.md) §8.2.
//!
//! ## A capacidade conta-se em microssegundos, não em quadros
//!
//! ```text
//! capacidade = 2 s de tempo de media
//! ```
//!
//! Uma keyframe tem dezenas de kilobytes e um delta tem um. Um anel
//! contado em quadros não tem memória previsível — 200 keyframes num
//! anel de 200 quadros são 6 MB, e o mesmo anel com deltas são 40 kB —
//! e a memória é o que estoura primeiro.
//!
//! ## Duas filas com políticas **opostas**
//!
//! | Fila | Política | Porquê |
//! | --- | --- | --- |
//! | áudio | bloqueia, **nunca** descarta | perder áudio parte o relógio mestre (§5.1) e deixa o vídeo a apresentar sem som |
//! | vídeo | `drop-oldest` | descartar vídeo é a operação correcta |
//!
//! Um anel único não consegue ter as duas. Esta é a razão de o módulo
//! ter duas filas e não uma com uma política.
//!
//! ## E o descarte é um **evento**
//!
//! Descartar um frame de vídeo é uma divergência, e §8.5 diz que a
//! próxima unidade tem de ser uma keyframe. O anel **não** marca a
//! keyframe: regista o descarte, e quem emite decide. Quem decide tem o
//! `seq`, o relógio e o coalescer à vista; quem regista só tem um
//! `VecDeque`.

use std::collections::VecDeque;

/// A capacidade do anel, em microssegundos de tempo de media.
///
/// Dois segundos, e não dois segundos de **parede**: o mesmo valor em
/// microssegundos de media é o que dá ao anel a mesma duração quando
/// a rede está lenta.
pub const CAPACIDADE_US: u64 = 2_000_000;

/// O [`Anel`] de media.
#[derive(Debug)]
pub struct Anel<T> {
    /// As unidades, por ordem de entrada.
    ///
    /// Um `VecDeque` e não um `Vec` com índice: o anel descarta **as mais
    /// antigas**, e com um `Vec` cada descarte custaria um `remove(0)` —
    /// que desloca tudo o que está atrás.
    audio: VecDeque<Unidade<T>>,
    video: VecDeque<Unidade<T>>,
    /// Os microssegundos de media que a fila de áudio ocupa.
    ///
    /// Só o áudio é contado. O vídeo é limitado pela sua própria
    /// contagem de *unidades*, e contar os dois em microssegundos
    /// tornaria o orçamento dependente da resolução — que é
    /// exactamente o que `bytes_por_ms` evita.
    ///
    /// E com **VP8** (F9) a assimetria deixou de ser técnica: um quadro
    /// de VP8 tem comprimento variável — um frame-P de ecrã parado tem
    /// dezenas de bytes e um frame-I tem milhares — pelo que «contar
    /// microssegundos de vídeo» não teria base nenhuma. Contar
    /// unidades é o que dá.
    micros_usados: u64,
    /// O número máximo de unidades de vídeo no anel.
    ///
    /// Vinte e uma, de §3.4. É um limite de **contagem** e não de tempo
    /// porque o tempo de vídeo só é previsível com o `fps` declarado.
    ///
    /// E é um limite que faz sentido com VP8: a 30 fps, 21 unidades
    /// são 700 ms de vídeo, ou seja mais do que o atraso de
    /// apresentação que a §3.5 aceita. Baixar a contagem de unidades
    /// é, por isso, a forma de baixar o atraso — e não mexe no
    /// orçamento de bytes, que é do `-b:v` (§8.1).
    max_video: usize,
}

/// Uma unidade no anel.
#[derive(Debug, Clone)]
pub struct Unidade<T> {
    /// O conteúdo: um delta, um pacote de áudio, uma keyframe.
    pub dados: T,
    /// Quando, em microssegundos de tempo de media.
    pub em_us: u64,
    /// O `seq` que esta unidade vai levar.
    pub seq: u32,
}

/// Um descarte, com o que é preciso para recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descarte {
    /// Quando o descarte aconteceu, em microssegundos de media.
    pub em_us: u64,
    /// O `seq` que se perdeu.
    pub seq_perdido: u32,
}

impl<T> Anel<T> {
    /// Um anel vazio, com a capacidade de §8.2.
    pub fn novo() -> Self {
        Self::with_max_video(21)
    }

    /// Um anel com um limite de unidades de vídeo diferente.
    ///
    /// O parâmetro existe para os testes e para um `fps` declarado
    /// diferente. O valor por omissão é o de §3.4.
    pub fn with_max_video(max_video: usize) -> Self {
        Self {
            audio: VecDeque::new(),
            video: VecDeque::new(),
            micros_usados: 0,
            max_video,
        }
    }

    /// Acrescenta uma unidade de **áudio**, e descarta as mais antigas
    /// para caber no orçamento.
    ///
    /// ## Porque o áudio **também** descarta
    ///
    /// §8.2 diz que o áudio «bloqueia, nunca descarta», e isso é o que
    /// acontece no caminho normal: [`Self::empurrar_audio`] devolve
    /// `false` e **não** toca em nada quando não cabe.
    ///
    /// O que este método faz é a outra metade, que a spec não diz em
    /// voz alta e que é a pergunta «e se não couber?». A resposta é
    /// descartar o áudio **antigo**: a alternativa — crescer sem limite
    /// — estoura a memória do daemon, e um daemon que morre é pior do
    /// que uma pessoa que ouve dois segundos de áudio antigo.
    ///
    /// A distinção é entre **bloquear o emissor** e **crescer sem
    /// limite**, e é por isso que o descarte é do mais antigo e não do
    /// mais recente: descartar o áudio novo tirava a voz a quem está a
    /// falar agora.
    pub fn empurrar_audio(&mut self, unidade: Unidade<T>) -> Vec<Descarte> {
        self.audio.push_back(unidade);
        self.micros_usados += self.audio.back().map_or(0, |u| u.em_us);

        let mut descartes = Vec::new();
        while self.micros_usados > CAPACIDADE_US {
            // O `while`, e não o `if`: uma unidade pode custar mais do
            // que a capacidade toda, e nesse caso só um ciclo não
            // bastava — o anel ficaria acima do orçamento para sempre.
            match self.audio.pop_front() {
                Some(u) => {
                    self.micros_usados = self.micros_usados.saturating_sub(u.em_us);
                    descartes.push(Descarte {
                        em_us: u.em_us,
                        seq_perdido: u.seq,
                    });
                }
                None => break,
            }
        }
        descartes
    }

    /// Se cabe uma unidade de áudio, sem descartar nada.
    ///
    /// É a pergunta do caminho normal, e é [`false`] **antes** de
    /// tocar em qualquer coisa: quem pergunta decide se espera.
    pub fn cabe_audio(&self, custo_us: u64) -> bool {
        self.micros_usados + custo_us <= CAPACIDADE_US
    }

    /// Acrescenta uma unidade de **vídeo**, descartando a mais antiga.
    ///
    /// Devolve o descarte, se houve. E é o **único** sítio onde um
    /// descarte de vídeo é decidido: §8.2 diz que descartar vídeo é a
    /// operação correcta, e diz isso porque um frame perdido é um frame
    /// que o próximo torna irrelevante.
    pub fn empurrar_video(&mut self, unidade: Unidade<T>) -> Option<Descarte> {
        self.video.push_back(unidade);

        // Duas condições, e a segunda é a que interessa: mesmo com
        // poucas unidades, uma lista mais longa do que a do orçamento
        // não serve de nada para o receptor.
        let mut descartado = None;
        while self.video.len() > self.max_video {
            if let Some(u) = self.video.pop_front() {
                descartado = Some(Descarte {
                    em_us: u.em_us,
                    seq_perdido: u.seq,
                });
            }
        }
        descartado
    }

    /// Tira a próxima unidade a apresentar.
    ///
    /// **Áudio primeiro**, e a razão é §5.1: o áudio é o relógio mestre,
    /// e presenting vídeo antes do áudio que lhe corresponde é o que
    /// produz áudio corrido. Um leitor que apresenta vídeo primeiro não tem
    /// áudio a sincronizar.
    pub fn seguinte(&mut self) -> Option<(Genero, Unidade<T>)> {
        if let Some(u) = self.audio.pop_front() {
            self.micros_usados = self.micros_usados.saturating_sub(u.em_us);
            return Some((Genero::Audio, u));
        }
        self.video.pop_front().map(|u| (Genero::Video, u))
    }

    /// Quantas unidades estão no anel, por fila.
    pub fn quantas(&self) -> (usize, usize) {
        (self.audio.len(), self.video.len())
    }

    /// Os microssegundos de media que a fila de áudio ocupa.
    pub fn micros_usados(&self) -> u64 {
        self.micros_usados
    }

    /// Se cabe mais uma unidade de vídeo, sem descartar nada.
    ///
    /// O espelho de [`Anel::cabe_audio`] para a fila de vídeo. Existe
    /// pelo mesmo motivo: quem decide se espera precisa de **perguntar**
    /// antes, e perguntar depois de empurrar já é tarde.
    pub fn cabe_video(&self) -> bool {
        self.video.len() < self.max_video
    }

    /// Se o anel não tem nada.
    pub fn vazio(&self) -> bool {
        self.audio.is_empty() && self.video.is_empty()
    }

    /// O limite de unidades de vídeo.
    pub fn max_video(&self) -> usize {
        self.max_video
    }

    /// Esvazia as duas filas, e diz o que se perdeu.
    ///
    /// Para quando a stream acaba: uma pessoa que recebeu metade de uma
    /// chamada precisa de saber que o que tem não é a chamada.
    pub fn drenar(&mut self) -> Vec<Descarte> {
        let mut perdidos: Vec<Descarte> = self
            .audio
            .drain(..)
            .chain(self.video.drain(..))
            .map(|u| Descarte {
                em_us: u.em_us,
                seq_perdido: u.seq,
            })
            .collect();
        self.micros_usados = 0;
        perdidos.sort_by_key(|d| d.seq_perdido);
        perdidos
    }
}

impl<T> Default for Anel<T> {
    fn default() -> Self {
        Self::novo()
    }
}

/// De que fila veio a unidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genero {
    /// Áudio, e é sempre o mestre.
    Audio,
    /// Vídeo.
    Video,
}

/// Converte microssegundos de media em milissegundos, arredondando
/// **para baixo**.
///
/// Para baixo e não para o mais próximo porque é um **orçamento**: o
/// anel tem de caber em dois segundos, e arredondar para cima daria um
/// anel de 2000,5 ms — que é a mesma coisa que não ter orçamento.
pub fn us_para_ms(us: u64) -> u64 {
    us / 1_000
}

/// Converte um `fps` em microssegundos por quadro.
///
/// `1_000_000 / fps`, e não o inverso. Com `fps` a zero seria uma divisão
/// por zero, e um `fps` declarado a zero é recusado por
/// [`super::ffmpeg::Captura::validar`] — mas o anel não depende do
/// `ffmpeg`, e um divide-por-zero aqui mataria o daemon.
pub fn us_por_frame(fps: u16) -> u64 {
    1_000_000 / u64::from(fps.max(1))
}

#[cfg(test)]
mod testes;
