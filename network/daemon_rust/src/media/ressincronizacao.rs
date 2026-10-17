// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! `base_seq`, keyframes e ressincronização.
//!
//! [`docs/media.md`](../../../../docs/media.md) §8.5.
//!
//! ## Onde vive o `base_seq`
//!
//! No **corpo** da unidade de streaming, e não no preâmbulo: o
//! preâmbulo é de largura fixa e partilhado com o não-streaming (§2.2),
//! e um campo novo lá dentro mudaria o formato de tudo.
//!
//! ```text
//! corpo de uma unidade de vídeo = [base_seq: u32 BE][delta]
//! ```
//!
//! Quatro bytes por unidade, que é 5 % do orçamento de §3.4.
//!
//! ## E porquê declará-lo se o receptor pode contar
//!
//! Poderia. O receptor sabe o `seq` que aplicou por último e sabe que
//! uma unidade sem `base_seq` é relativa à anterior — a partir daí
//! basta ver que faltou uma no meio.
//!
//! Declarar é melhor por duas razões, e a segunda é a que importa:
//!
//! 1. Uma **keyframe** muda a referência sem seguir a cadeia. Sem
//!    `base_seq`, o receptor teria de adivinhar que a unidade `N` é
//!    auto-suficiente — e adivinhar é o que produz uma imagem corrompida
//!    que ninguém percebe.
//! 2. Um erro de numeração no emissor — um `seq` saltado por um bug —
//!    passaria despercebido na detecção por lacuna, e apareceria como
//!    «o ecrã do meu interlocutor está a tremer». Com `base_seq`, o
//!    receptor sabe exactamente qual unidade perdeu a base.
//!
//! ## E a invariante
//!
//! ```text
//! base_seq == seq  →  a unidade é auto-suficiente (keyframes)
//! ```
//!
//! Uma keyframe é o delta contra um quadro **todo a zero**, com o
//! próprio `seq` como base. O receptor aplica-a sem condição e passa a
//! esperar por `seq + 1`. Não há uma instrução separada: a
//! auto-suficiência **é** a instrução, e é verificável sem o receptor
//! precisar de saber a regra.

use super::erros::ErroMedia;
use super::telhas::aplicar;

/// Os bytes do `base_seq` à frente de cada corpo de vídeo.
pub const BYTES_DE_BASE: usize = 4;

/// A cadência mínima entre keyframes espontâneas, em ms.
///
/// §8.5 diz «limite de 1 por 2 s», e a leitura que se implementa é: as
/// **três causas** mandam uma keyframe já, e a cadência é o piso das
/// que não têm causa.
///
/// A leitura alternativa — um limite absoluto de 1 por 2 s — faria uma
/// keyframe pedida por um descarte esperar até 2 s, e a imagem ficava
/// corrompida esse tempo todo. §8.5 diz que isto «é o que torna o modo
/// streaming recuperável», e uma recuperação que demora 2 s não é
/// recuperação: é um congelamento diferente.
pub const KEYFRAME_MS: i64 = 2_000;

/// O `base_seq` de um corpo de unidade de vídeo.
pub fn base_seq_de(corpo: &[u8]) -> Result<u32, ErroMedia> {
    let quatro = corpo.get(..BYTES_DE_BASE).ok_or(ErroMedia::BaseTruncado)?;
    Ok(u32::from_be_bytes([
        quatro[0], quatro[1], quatro[2], quatro[3],
    ]))
}

/// O delta de um corpo, sem o `base_seq`.
pub fn delta_de(corpo: &[u8]) -> &[u8] {
    corpo.get(BYTES_DE_BASE..).unwrap_or(&[])
}

/// O estado do receptor: a sequência a que o quadro actual pertence.
#[derive(Debug, Default)]
pub struct Receptor {
    /// O `seq` da última unidade aplicada, se alguma.
    ///
    /// `None` antes da primeira: a primeira unidade tem de ser uma
    /// keyframe, e `base_seq == seq` é o que o garante.
    pub ultimo_aplicado: Option<u32>,
    /// Quantas unidades foram descartadas por falta de base.
    ///
    /// Não é um contador de decays: é o que justifica pedir uma
    /// keyframe. Um receptor que descarta duas unidades seguidas está
    /// desalinhado, e o próximo pedido tem de dizer isso.
    pub descartadas: u32,
    /// O montador de lotes de keyframe.
    ///
    /// Vive aqui e não no receptor da stream porque é **estado entre
    /// unidades**, e o `Receptor` é estado sobre `seq`. São ciclos de
    /// vida diferentes, e a única coisa que os liga é a regra de §8.5 —
    /// que `pode_aplicar` já implementa sem saber da existência de lotes.
    ///
    /// `MontadorLote` é o seu próprio tipo com o seu próprio ciclo de
    /// vida, e a única coisa que os liga é a regra de §8.5 — que é a
    /// regra que `pode_aplicar` já implementa sem saber da existência
    /// de lotes.
    pub lotes: super::lote::MontadorLote,
}

/// O que o receptor fez com uma unidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recebida {
    /// Aplicada, e o `seq` que o quadro passou a ser.
    Aplicada(u32),
    /// Descartada: a base não é a que o quadro tem.
    ///
    /// O quadro fica **intacto** e a sequência não avança. Um descarte
    /// que mexesse no quadro produziria uma imagem que nunca esteve no
    /// ecrã de ninguém.
    Descartada,
}

/// Aplica uma unidade a um quadro, se a base coincidir.
impl Receptor {
    /// Um receptor sem base.
    pub fn novo() -> Self {
        Self::default()
    }

    /// Verifica se a unidade pode ser aplicada, sem a aplicar.
    ///
    /// Existe separada de [`Receptor::aplicar`] porque o emissor também
    /// precisa da regra — para saber quando uma keyframe é obrigatória
    /// — e duas implementações da mesma condição divergem.
    pub fn pode_aplicar(&self, seq: u32, base_seq: u32) -> bool {
        // Auto-suficiente: não depende de nada.
        if base_seq == seq {
            return true;
        }
        match self.ultimo_aplicado {
            // A primeira unidade tem de ser uma keyframe. Aceitar uma
            // delta aqui aplicaria bytes contra um quadro que não é o
            // dela.
            None => false,
            Some(ultimo) => base_seq == ultimo,
        }
    }

    /// Aplica uma unidade a `quadro`, se a base coincidir.
    ///
    /// ## O que muda com os lotes
    ///
    /// `eh_lote` diz se a unidade é parte de uma keyframe fraccionada
    /// (§3.6). Quando é, o corpo tem 12 bytes de cabeçalho e não 4, e
    /// a imagem só existe quando o lote fecha.
    ///
    /// E a imagem que sai pode ser de outra geometria, porque a
    /// keyframe descreve-se a si mesma. Por isso `quadro` é um slice
    /// com a largura de quem chama: se a geometria do lote não couber
    /// nele, a unidade é descartada em vez de escrever por fora.
    ///
    /// Quem muda de geometria tem de recriar o quadro; isso é trabalho
    /// do dono da stream, e é onde a mudança de resolução pertence.
    pub fn aplicar(
        &mut self,
        seq: u32,
        corpo: &[u8],
        quadro: &mut [u8],
        largura: usize,
        eh_lote: bool,
    ) -> Recebida {
        let Ok(base) = base_seq_de(corpo) else {
            self.descartadas = self.descartadas.saturating_add(1);
            return Recebida::Descartada;
        };

        if !self.pode_aplicar(seq, base) {
            self.descartadas = self.descartadas.saturating_add(1);
            return Recebida::Descartada;
        }

        if eh_lote {
            return self.aplicar_lote(seq, corpo, quadro, largura);
        }

        let delta = delta_de(corpo);
        // Um delta vazio com `base_seq == seq` é uma keyframe de ecrã
        // todo a zero, e `aplicar` de nada é um erro — mas o corpo é
        // exactamente o que a keyframe traz, e uma keyframe nunca é
        // vazia. Se chegar vazia, é uma unidade corrompida.
        if delta.is_empty() {
            self.descartadas = self.descartadas.saturating_add(1);
            return Recebida::Descartada;
        }

        // `aplicar` recusa o que não cabe, e recusa **antes** de
        // escrever: uma largura que não seja múltiplo de 16 faria a
        // telha do canto de baixo direito cair fora do quadro. Por isso
        // não há uma validação da geometria aqui — `aplicar` já é a
        // validação, e duplicá-la seria um segundo sítio a divergir.
        if aplicar(delta, quadro, largura).is_err() {
            self.descartadas = self.descartadas.saturating_add(1);
            return Recebida::Descartada;
        }

        self.ultimo_aplicado = Some(seq);
        Recebida::Aplicada(seq)
    }

    /// Aplica uma unidade de lote de keyframe.
    ///
    /// ## Porque é que uma unidade a meio devolve `Descartada` e não um erro
    ///
    /// O receptor é chamado a 15–30 unidades por segundo e não pode
    /// devolver `Err` a cada quadro que chega a meio de uma keyframe —
    /// seria um erro por quadro, e a imagem ficaria a piscar enquanto a
    /// keyframe não fechasse. Um `Descartada` é o certo: **nada se
    /// aplica**, a sequência **não avança**, e o quadro fica intacto.
    fn aplicar_lote(
        &mut self,
        seq: u32,
        corpo: &[u8],
        quadro: &mut [u8],
        largura: usize,
    ) -> Recebida {
        use super::lote::Montagem;

        match self.lotes.aceitar(corpo) {
            Err(_) => {
                self.descartadas = self.descartadas.saturating_add(1);
                self.lotes.limpar();
                Recebida::Descartada
            }
            Ok(Montagem::Descartada) => {
                self.descartadas = self.descartadas.saturating_add(1);
                Recebida::Descartada
            }
            Ok(Montagem::EmCurso) => Recebida::Descartada,
            Ok(Montagem::Completo(novo)) => {
                // A geometria do lote tem de caber no quadro de quem
                // chama. Não se recria o quadro aqui — quem tem o quadro
                // é quem sabe que a resolução mudou, e recriá-lo às
                // cegas devolveria um quadro do tamanho errado para um
                // receptor que nem pediu outra resolução.
                //
                // O teste é o comprimento: a keyframe do lote é sempre
                // `largura × altura` do seu cabeçalho, e o quadro de
                // quem chama é sempre `largura × altura` da captura
                // dele. Iguais os comprimentos, iguais as geometrias.
                if novo.len() != quadro.len() {
                    self.descartadas = self.descartadas.saturating_add(1);
                    self.lotes.limpar();
                    return Recebida::Descartada;
                }
                quadro.copy_from_slice(&novo);
                self.ultimo_aplicado = Some(seq);
                Recebida::Aplicada(seq)
            }
        }
    }
}

/// O estado do emissor: quando é que uma keyframe é obrigatória.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Emissor {
    /// Quando foi a última keyframe **espontânea**, em tempo de media.
    ///
    /// `None` antes da primeira. E uma keyframe por causa de um descarte
    /// **não** escreve aqui: essa conta-se à parte, e é o que impede que
    /// uma keyframe gastosca empurre a seguinte para fora da cadência.
    ultima_espontanea_ms: Option<i64>,
    /// Houve um descarte desde a última unidade emitida.
    ///
    /// É o gatilho de §8.5.1: um descarte **sabe** que se perdeu, e a
    /// keyframe é a resposta.
    por_pedir: bool,
}

/// O que o emissor tem de fazer com a próxima unidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pedido {
    /// Mandar um delta contra a referência.
    Delta,
    /// Mandar o quadro inteiro, contra um quadro zero.
    ///
    /// O `base_seq` da keyframe é o **próprio** `seq`.
    Keyframe,
}

/// Um `pedido` de keyframe do receptor, em texto.
///
/// ## Porquê texto e não um comando
///
/// §8.5 fala em «pedido explícito do receptor» sem dizer como. A
/// resposta é a mesma que §4.3 dá para a retoma: **uma mensagem de
/// texto**, que atravessa o relay como texto e não precisa de canal
/// nenhum. Um comando novo seria um opcode por um pedido que acontece
/// uma vez por cada duas unidades perdidas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PedidoDeKeyframe {
    /// A stream a que o pedido se refere.
    pub stream: u16,
}

/// O texto de um pedido de keyframe.
///
/// ```text
/// 9f3a: keyframe na stream 7
/// ```
///
/// O prefixo é o da retoma (§4.3), e a razão é a mesma: uma frase que
/// começa por aquilo é de media, e o resto da conversa não colide.
///
/// ## Porque **não** leva o `seq` do receptor
///
/// A primeira versão mandava, e escrevia `0` tanto para «ainda não
/// apliquei nada» como para «apliquei a unidade 0». O round-trip perdia a
/// distinção, e um campo que se perde ao ir e voltar é um campo que
/// não está.
///
/// E o emissor não o usa: a decisão de §8.5 é «manda uma keyframe»,
/// não «manda a partir da unidade 42». Guardar o `seq` era
/// especulativo.
pub fn texto_de_pedido(stream: u16) -> String {
    format!("9f3a: keyframe na stream {stream}")
}

/// Lê um pedido de keyframe de um texto, ou `None`.
///
/// `None` e não um erro: o texto chega pelo mesmo caminho que todas as
/// mensagens de uma conversa, e levantar por causa de uma frase comum
/// seria trocar uma mensagem por um erro.
pub fn interpretar_pedido(texto: &str) -> Option<PedidoDeKeyframe> {
    let resto = texto.strip_prefix("9f3a: keyframe na stream ")?;
    Some(PedidoDeKeyframe {
        stream: resto.parse().ok()?,
    })
}

impl Emissor {
    /// Um emissor que ainda não mandou nenhuma keyframe.
    pub fn novo() -> Self {
        Self::default()
    }

    /// Regista um descarte no anel.
    ///
    /// É o gatilho de §8.5.1 e é **gratuito**: quem descarta já sabe
    /// que perdeu, e perguntar ao receptor seria esperar que ele
    /// descobrisse sozinho.
    pub fn registar_descarte(&mut self) {
        self.por_pedir = true;
    }

    /// Regista um pedido explícito do receptor.
    ///
    /// Aplica-se ao `stream` do pedido e não globalmente: duas streams
    /// no mesmo daemon não podem acordar uma à parte da outra.
    pub fn registar_pedido(&mut self, pedido: PedidoDeKeyframe, stream_actual: u16) {
        if pedido.stream == stream_actual {
            self.por_pedir = true;
        }
    }

    /// A próxima unidade tem de ser uma keyframe.
    ///
    /// Três gatilhos, todos locais e todos gratuitos (§8.5):
    ///
    /// 1. um descarte no anel;
    /// 2. um pedido do receptor;
    /// 3. a cadência — [`KEYFRAME_MS`] desde a última espontânea.
    pub fn decidir(&mut self, media_ms: i64) -> Pedido {
        if self.por_pedir {
            self.por_pedir = false;
            return Pedido::Keyframe;
        }
        match self.ultima_espontanea_ms {
            // A primeira unidade de uma stream é sempre uma keyframe: o
            // receptor não tem referência nenhuma.
            None => {
                self.ultima_espontanea_ms = Some(media_ms);
                Pedido::Keyframe
            }
            Some(ultima) if media_ms - ultima >= KEYFRAME_MS => {
                self.ultima_espontanea_ms = Some(media_ms);
                Pedido::Keyframe
            }
            Some(_) => Pedido::Delta,
        }
    }

    /// Regista que uma keyframe foi emitida.
    ///
    /// Só as **espontâneas** actualizam a cadência. Uma keyframe
    /// disparada por um descarte acontece no momento em que é precisa, e
    /// contá-la faria a seguinte esperar mais 2 s — que é o tempo que o
    /// receptor passou a ver uma imagem corrompida.
    pub fn registar_keyframe(&mut self, media_ms: i64, espontanea: bool) {
        if espontanea {
            self.ultima_espontanea_ms = Some(media_ms);
        }
    }
}

#[cfg(test)]
mod testes;
