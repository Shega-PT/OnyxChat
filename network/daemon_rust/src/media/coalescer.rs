// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Coalescing adaptativo e `HOLD`.
//!
//! [`docs/media.md`](../../../../docs/media.md) §8.3 e §8.4.
//!
//! ## «Acumular» significa guardar o último quadro, **não**
//! concatenar deltas
//!
//! A primeira leitura de §8.3 é acumular os **deltas** das fatias
//! pequenas e mandá-las juntas. Isso está errado, e a demonstração é um
//! contador:
//!
//! ```text
//! referência   telha 5 = 7
//! quadro 1     telha 5 = 200    → delta 1 escreve 200
//! quadro 2     telha 5 = 7      → delta 2 não escreve nada
//! ```
//!
//! Concatenados e aplicados à mesma referência, o receptor fica com
//! `200`. O quadro 2 tem `7`. O receptor mostra uma cor que **não está
//! no ecrã** de quem emite — e sem nenhum sinal, porque o SHA e o
//! envelope estão correctos.
//!
//! Por isso o coalescer guarda **um** quadro e calcula o delta no
//! momento de emitir. Os quadros intermédios não são codificados: é
//! uma perda, e §8.2 diz que perder vídeo é a operação correcta.
//!
//! ## E a decisão de acumular é por tamanho, não por tempo
//!
//! Medir o delta exige codificá-lo, e codificá-lo a 30 fps seria paying
//! o preço do encode inteiro para decidir se o encode se paga. O que se
//! mede é o **número de telhas alteradas** — que é um `memcmp` de 256
//! bytes por telha, e o mesmo dado de que o delta precisa.
//!
//! 8 KB é 31 telhas com variação (259 bytes cada), e é esse o limiar.

use super::erros::ErroMedia;
use super::ressincronizacao::{self, Emissor, Pedido as PedidoKeyframe};
use super::telhas::{Telhas, BYTES_POR_TELHA};

/// Abaixo deste tamanho de delta, acumula-se.
///
/// §8.3 diz 8 KB. Em telhas são 31 — e [`MAX_TELHAS_ACUMULAR`] é esse
/// número escrito, porque «31 telhas» se pode conferir contra um
/// `memcmp` e «8 KB» não.
pub const MAX_TELHAS_ACUMULAR: usize = 8 * 1024 / 259;

/// A janela de tempo a acumular, em milissegundos de media.
pub const JANELA_MS: i64 = 20;

/// O que o coalescer decide fazer com um quadro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisao {
    /// Guardar e esperar mais.
    Acumular,
    /// Emitir já.
    Emitir,
}

/// O que sai do coalescer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unidade {
    /// O corpo: `[base_seq: 4B][delta]`, ou `[base_seq: 4B]` num `HOLD`.
    ///
    /// §8.4: uma unidade cujo delta é vazio transporta só o `base_seq` e
    /// a moldura — 40 bytes de texto. Com ecrã estático isso são ~600
    /// bytes/s em vez de 5,8 KB/s.
    ///
    /// O `base_seq` **nunca** falta, nem num `HOLD`: é ele que diz ao
    /// receptor que a unidade não traz nada **de novo** e que a cadeia
    /// continua. Um `HOLD` sem `base_seq` seria uma unidade que o
    /// receptor não pode nem aplicar nem descartar com confiança.
    pub corpo: Vec<u8>,
    /// Se é um `HOLD` — isto é, se não há delta.
    pub hold: bool,
    /// Se é uma keyframe.
    ///
    /// Sai daqui e não se recalcula: quem já sabe que o `base_seq == seq`
    /// sabe, e recalcular aqui seria um segundo sítio a poder divergir.
    pub keyframe: bool,
    /// O `seq` que a unidade leva.
    pub seq: u32,
    /// O `clock_ms` que a unidade declara (§5.4).
    pub clock_ms: i64,
    /// As flags do preâmbulo (§2.2): `KEYFRAME` e `FIM_DE_LOTE`.
    ///
    /// Vêm **daqui** e não são recalculadas na pipeline de exportação: uma
    /// flag que é correcta aqui e falsa no preâmbulo dá um receptor que
    /// trata uma keyframe como uma delta e vice-versa, e nenhum dos dois
    /// lados dá sinal de nada.
    pub flags: super::moldura::Flags,
    /// O índice desta unidade dentro do lote, a zero.
    ///
    /// `1` numa unidade solitária — que é o caso comum, a delta de
    /// todos os dias — porque o cabeçalho só é escrito quando o lote
    /// tem mais de uma unidade.
    pub indice: u16,
    /// Quantas unidades tem o lote.
    pub n_unidades: u16,
}

/// O que o coalescer guarda à espera de janela.
#[derive(Debug, Clone)]
struct Pendente {
    /// O quadro **inteiro**, para calcular o delta no momento certo.
    ///
    /// E não o delta, que é a razão de o módulo existir: guardar o
    /// delta obrigaria a aplicá-lo a uma referência que ainda pode
    /// mudar, e a concatenar deltas que não se podem concatenar.
    quadro: Vec<u8>,
    /// Quando o **quadro** chegou, em tempo de media.
    ///
    /// Entra no `clock_ms` da unidade (§5.4): é o tempo que este quadro
    /// representa.
    chegou_ms: i64,
    /// Quando a **janela** abriu, em tempo de media.
    ///
    /// ## Porque é um campo à parte, e o que acontece sem ele
    ///
    /// A primeira versão tinha só `chegou_ms`, e substituí-lo a cada
    /// quadro. Com isso, uma stream de deltas pequenos **nunca** fechava
    /// a janela: o tempo de espera media-se a partir do quadro anterior,
    /// e a soma nunca chegava a 20 ms. O coalescer acumulava para
    /// sempre — uma fila sem limite, que é exactamente o que §8.2 proíbe
    /// ao anel e o que estoura a memória do daemon.
    ///
    /// A janela é uma janela: começa uma vez e tem um fim. O que muda
    /// dentro dela é o **conteúdo**, e o tempo de cada quadro é o seu
    /// `clock_ms`.
    iniciou_ms: i64,
}

/// O coalescer de uma captura de vídeo.
#[derive(Debug)]
pub struct Coalescer {
    /// O quadro de referência — o último **emitido**.
    ///
    /// E não o último recebido: a referência tem de ser a última coisa
    /// que o receptor viu, ou o próximo delta aplica-se a uma imagem que
    /// ele não tem.
    referencia: Vec<u8>,
    /// Um quadro de zeros, do mesmo tamanho, para as keyframes.
    ///
    /// Existe como campo e não se aloca por keyframe porque a
    /// **frequência** importa: uma keyframe a cada 2 s em 640×480 são
    /// 192 kB de zeros por keyframe, e alocar isso a cada vez seria
    /// alocar e deitar fora duas vezes por minuto para sempre.
    zero: Vec<u8>,
    largura: usize,
    altura: usize,
    pendente: Option<Pendente>,
    /// O `seq` da próxima unidade.
    ///
    /// Contado aqui e não pelo anel, porque o `seq` é da **unidade que
    /// sai** e não da que entrou: um delta acumulado cobre três quadros
    /// e leva um `seq` só.
    seq: u32,
    /// As unidades à espera de sair.
    ///
    /// Interna, e não um valor de retorno de [`Coalescer::receber`],
    /// porque um quadro pode produzir duas unidades — ver a nota lá.
    saida: Vec<Unidade>,
    /// A janela de congestionamento está aberta (§8.3).
    saturado: bool,
    /// Quando é que a próxima unidade tem de ser uma keyframe (§8.5).
    ///
    /// Vive **no coalescer** e não no chamador, porque o coalescer é
    /// quem sabe o `seq` e quem constrói a unidade — e um `keyframe`
    /// pedido de fora chegaria tarde: a unidade já estaria construída.
    chaves: Emissor,
}

impl Coalescer {
    /// Um coalescer com um quadro inicial de referência.
    ///
    /// A primeira referência é o quadro que o receptor **já tem** — o
    /// que veio na keyframe anterior, ou um ecrã preto no primeiro uso.
    pub fn novo(referencia: &[u8], largura: usize, altura: usize) -> Result<Self, ErroMedia> {
        // Valida pela mesma via que as telhas, para que um coalescer mal
        // formado dê erro aqui e não um delta de lixo.
        Telhas::nova(referencia, largura, altura)?;

        Ok(Self {
            referencia: referencia.to_vec(),
            zero: vec![0u8; referencia.len()],
            largura,
            altura,
            pendente: None,
            saida: Vec::new(),
            seq: 0,
            saturado: false,
            chaves: Emissor::novo(),
        })
    }

    /// Conta as telhas que diferem da referência.
    ///
    /// Só a contagem: é o que decide, e o delta completo é caro.
    pub fn telhas_alteradas(&self, quadro: &[u8]) -> usize {
        let telhas = super::telhas::LADO;
        let colunas = self.largura / telhas;
        let mut alteradas = 0;

        for lin in 0..self.altura / telhas {
            for col in 0..colunas {
                let inicio = lin * telhas * self.largura + col * telhas;
                let mudou = (0..telhas).any(|y| {
                    let a = inicio + y * self.largura;
                    quadro[a..a + telhas] != self.referencia[a..a + telhas]
                });
                if mudou {
                    alteradas += 1;
                }
            }
        }
        alteradas
    }

    /// A decisão para um quadro, sem a aplicar.
    pub fn decidir(&self, quadro: &[u8], media_ms: i64) -> Decisao {
        if self.saturado {
            return Decisao::Emitir;
        }
        if self.telhas_alteradas(quadro) > MAX_TELHAS_ACUMULAR {
            return Decisao::Emitir;
        }
        // Sem nada pendente, acumula-se na mesma: o que se está a fazer
        // é abrir a janela, e esperar 20 ms não custa nada porque não
        // havia uma mensagem para enviar.
        match &self.pendente {
            None => Decisao::Acumular,
            // A janela mede-se desde `iniciou_ms` e **não** desde o
            // quadro anterior — ver a nota em [`Pendente`].
            Some(p) if media_ms - p.iniciou_ms < JANELA_MS => Decisao::Acumular,
            Some(_) => Decisao::Emitir,
        }
    }

    /// Recebe um quadro. Não devolve nada.
    ///
    /// ## Porque não devolve
    ///
    /// Porque **um** quadro pode produzir **duas** unidades: a que
    /// estava pendente e fechou a janela, e a deste quadro, que é grande
    /// demais para esperar. Devolver `Option` obrigaria a escolher uma
    /// e a perder a outra; devolver `Vec` em cada chamada seria uma
    /// alocação por quadro para a maioria dos casos em que não há nada.
    ///
    /// A fila é interna e [`Coalescer::esvaziar`] dá-a de uma vez. O
    /// chamador decide o que faz com zero, uma ou duas.
    pub fn receber(&mut self, quadro: &[u8], media_ms: i64) {
        debug_assert_eq!(
            quadro.len(),
            self.largura * self.altura,
            "o quadro não tem o tamanho da geometria"
        );

        if self.decidir(quadro, media_ms) == Decisao::Acumular {
            // A janela que já estava aberta continua aberta; só o
            // conteúdo muda.
            let iniciou_ms = self.pendente.as_ref().map_or(media_ms, |p| p.iniciou_ms);
            self.pendente = Some(Pendente {
                quadro: quadro.to_vec(),
                chegou_ms: media_ms,
                iniciou_ms,
            });
            return;
        }

        // O pendente é **coberto** por este delta — que é calculado
        // contra a mesma referência e descreve o quadro inteiro. Vai à
        // vida; mandá-lo era uma mensagem por janela sem acrescentar um
        // pixel à imagem que o receptor vê.
        self.pendente = None;
        self.emitir(quadro, media_ms);
    }

    /// As unidades à espera de sair, por ordem.
    pub fn esvaziar(&mut self) -> Vec<Unidade> {
        std::mem::take(&mut self.saida)
    }

    /// Fecha a janela à força — o `RateLimit` de §8.3.
    ///
    /// ## Porque o `RateLimit` **é** o sensor
    ///
    /// Não há canal de controlo novo. `p2p.rs` já produz o sinal e o
    /// módulo já o recebe: o mecanismo de protecção contra flood do
    /// protocolo **é** o sensor de congestão do media.
    ///
    /// E o que se faz com ele é **emitir já**, não abrandar. Enviar mais
    /// depressa quando a rede está saturada parece o contrário do
    /// certo, e não é: o coalescer existe para que não haja uma
    /// mensagem por frame, e a **taxa de mensagens** é o que satura.
    /// Emitir já entrega o excesso ao anel, e o anel descarta **vídeo** —
    /// que é o que se pode perder sem quebrar o áudio.
    pub fn saturar(&mut self) {
        self.saturado = true;
        self.emitir_pendente();
    }

    /// Fecha a janela de congestão.
    pub fn drenar(&mut self) {
        self.saturado = false;
    }

    /// Se a janela de congestionamento está aberta.
    pub fn saturado(&self) -> bool {
        self.saturado
    }

    /// Emite o que estiver pendente, se houver.
    ///
    /// Chamar no fim da stream: sem isto, o último accumulate ficava
    /// preso e o receptor via uma stream que acaba a meio.
    pub fn fechar(&mut self) {
        self.emitir_pendente();
    }

    /// O `seq` da próxima unidade.
    pub fn proximo_seq(&self) -> u32 {
        self.seq
    }

    /// O quadro de referência actual.
    pub fn referencia(&self) -> &[u8] {
        &self.referencia
    }

    /// Quantas unidades estão à espera de sair.
    pub fn por_sair(&self) -> usize {
        self.saida.len()
    }

    /// Emite o pendente. `emitir` já avança a referência, por isso aqui
    /// não se repete a atribuição — as duas versões divergentemente.
    fn emitir_pendente(&mut self) {
        let Some(p) = self.pendente.take() else {
            return;
        };
        self.emitir(&p.quadro, p.chegou_ms);
    }

    /// A janela aberta há mais tempo, em tempo de media.
    ///
    /// Só para diagnóstico: um coalescer que não emite há muito tempo
    /// é um bug, e «quantos milissegundos» é a pista.
    pub fn janela_ha(&self) -> Option<i64> {
        self.pendente.as_ref().map(|p| p.iniciou_ms)
    }

    /// Codifica um quadro, guarda a unidade e avança a referência.
    ///
    /// ## A keyframe decide-se **antes** de codificar
    ///
    /// E a ordem é o que faz a coisa funcionar. Uma keyframe é o delta
    /// contra um quadro **todo a zero**, não contra a referência — e
    /// codificá-la primeiro é o que garante que a unidade leva todas as
    /// telhas, mesmo as que não mudaram desde a última.
    fn emitir(&mut self, quadro: &[u8], media_ms: i64) {
        let pedido = self.chaves.decidir(media_ms);
        let seq = self.seq;
        let (referencia, base_seq) = match pedido {
            PedidoKeyframe::Keyframe => (&self.zero[..], seq),
            PedidoKeyframe::Delta => (&self.referencia[..], self.seq.saturating_sub(1)),
        };

        let t = match Telhas::nova(referencia, self.largura, self.altura) {
            Ok(t) => t,
            // A referência foi validada em `novo` e nunca muda de
            // tamanho, então isto é um bug e não uma condição.
            // `unreachable!` é o que transforma um bug num pânico com a
            // linha certa; um delta de lixo daria um receptor com lixo
            // no ecrã e nenhum sinal.
            Err(_) => unreachable!("a referência foi validada em `Coalescer::novo`"),
        };
        let delta = t.diferenca(quadro);

        // Uma keyframe pode não caber num envelope: a 640×480 são 310 800
        // bytes de delta contra um quadro zero, e um envelope leva
        // 49 153. Por isso uma keyframe sai em **N** unidades, e é por
        // isso que `emitir` empurra em vez de substituir o último.
        //
        // Uma delta normal cabe sempre — cabia já antes de este módulo,
        // porque um delta só leva as telhas que mudaram. Só a keyframe é
        // o caso difícil, e só ela ganha cabeçalho.
        if matches!(pedido, PedidoKeyframe::Keyframe) && !delta.is_empty() {
            let partes = super::lote::dividir(&delta, seq, self.largura as u16, self.altura as u16)
                // O `delta` vem de `Telhas::diferenca`, que só produz
                // telhas bem formadas, e a geometria foi validada em
                // `novo`. Se isto falha, é um bug: um bug que se manifesta
                // num pânico com a linha certa, e não num receptor com
                // lixo no ecrã e nenhum sinal.
                .expect("um delta de `Telhas::diferenca` sempre fraciona");

            // A keyframe **tem** de ser registada antes de este
            // `return`. A primeira versão não registava, e o efeito foi
            // o pior possível: `decidir` continuava a pedir keyframe, o
            // coalescer emitia um lote a cada quadro, e a cadência de
            // §8.5 nunca existiu. O sintoma era «demasiadas unidades»,
            // e a causa era um `return` quatro linhas acima.
            self.chaves
                .registar_keyframe(media_ms, matches!(pedido, PedidoKeyframe::Keyframe));

            let quantas = partes.len() as u32;
            for parte in partes {
                let mut corpo =
                    Vec::with_capacity(super::lote::BYTES_DE_CABECALHO + parte.delta.len());
                parte.cabecalho.escrever(&mut corpo);
                corpo.extend_from_slice(&parte.delta);

                let flags = super::moldura::Flags::VAZIO
                    .com(super::moldura::Flags::KEYFRAME)
                    .com(if parte.cabecalho.eh_ultima() {
                        super::moldura::Flags::FIM_DE_LOTE
                    } else {
                        0
                    });

                self.saida.push(Unidade {
                    corpo,
                    hold: false,
                    keyframe: true,
                    seq: parte.cabecalho.base_seq,
                    clock_ms: media_ms,
                    flags,
                    indice: parte.cabecalho.indice,
                    n_unidades: parte.cabecalho.n_unidades,
                });
            }

            self.seq = self.seq.wrapping_add(quantas);
            self.referencia = quadro.to_vec();
            return;
        }

        let mut corpo = Vec::with_capacity(ressincronizacao::BYTES_DE_BASE + delta.len());
        corpo.extend_from_slice(&base_seq.to_be_bytes());
        corpo.extend_from_slice(&delta);

        // Uma keyframe não pode ter delta vazio: o quadro contra o
        // zero tem sempre alguma telha, a não ser que o quadro seja
        // todo a zero — e nesse caso o receptor já tem a imagem
        // certa e um `HOLD` serve.
        let keyframe = matches!(pedido, PedidoKeyframe::Keyframe) && !delta.is_empty();
        let hold = delta.is_empty();

        self.chaves
            .registar_keyframe(media_ms, matches!(pedido, PedidoKeyframe::Keyframe));

        self.saida.push(Unidade {
            corpo,
            hold,
            keyframe,
            seq,
            clock_ms: media_ms,
            flags: super::moldura::Flags::VAZIO,
            indice: 0,
            n_unidades: 1,
        });
        self.seq += 1;
        self.referencia = quadro.to_vec();
    }
}

/// Os bytes de uma telha com variação, para a conta do limiar.
pub const BYTES_DE_TELHA_VARIADA: usize = 2 + 1 + BYTES_POR_TELHA;

#[cfg(test)]
mod testes;
