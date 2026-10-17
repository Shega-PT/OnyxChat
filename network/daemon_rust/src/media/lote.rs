// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/lote.rs — a keyframe que não cabe noutro lugar
// ---------------------------------------------------------------------
// ## O problema que este módulo resolve
//
// A medição de `examples/medir_media.rs` mostrou, contra o que
// `docs/media.md` §3.1 e §8.5 diziam, que uma **keyframe completa não
// cabe num envelope**:
//
// ```text
// 640×480 = 1 200 telhas de 16×16
// 1 200 × 259 bytes = 310 800 bytes binários
// base-64           = 414 456 bytes de texto
// MAX_PLAINTEXT     =  65 536
// ```
//
// 6,3× acima. E o tecto de uma unidade são 189 telhas, ou seja
// ~256×176: nenhuma largura de ecrã dava para uma keyframe.
//
// A resposta **não** é baixar a resolução — uma keyframe que não cabe
// não é uma keyframe que se mande pequena, é uma keyframe que não
// existe, e sem ela uma perda é definitiva. A resposta é a mesma que a
// §4 já dá para os ficheiros: **fraccionar**.
//
// Uma keyframe passa a ser um **lote** de N unidades, cada uma com
// ≤ 189 telhas, e o receptor volta a ter a imagem completa ao fim da
// última.
//
// ## O que a fracção NÃO é
//
// Não é concatenar unidades e aplicá-las uma a uma. Uma keyframe é o
// delta contra um quadro **zero**, e o receptor aplica a **soma** desse
// delta a um quadro zero. Concatenar os *bytes* das telhas e aplicar
// uma vez é o que dá a imagem certa; aplicar unidade a unidade contra
// o zero daria sempre a última telha de cada unidade.
//
// ## Porquê é que o `base_seq` não muda
//
// Cada unidade do lote é auto-suficiente em espaço de `seq`: a unidade
// `k` do lote leva `seq = s + k` e `base_seq = s + k`. Isto mantém a
// invariante de §8.5 — «`base_seq == seq` é uma keyframe» — **exacta**,
// e mantém a cadeia contínua sem excepções: a delta que vem depois do
// lote leva `base_seq` igual ao `seq` da última unidade do lote.
//
// E é por isso que [`crate::media::ressincronizacao::Receptor`]
// continua a funcionar sem mudar: `pode_aplicar` vê `base_seq == seq` e
// aceita, exactamente como antes. O que muda é que o corpo já não é um
// delta de 4 bytes de cabeçalho, e que **não se pode aplicar a unidade
// isolada** — a imagem só existe no fim.
//
// ## A geometria vai no lote, e porquê
//
// O cabeçalho leva `largura` e `altura` em cada unidade do lote. Podia
// ir só na primeira, e é o que se faria por economia: 4 bytes.
//
// Vai em todas porque uma keyframe é o **mecanismo de recuperação**, e
// o receptor que precisa dela pode ser um que esteve dez minutos sem
// ver nada — e um receptor às cegas não tem geometria nenhuma para
// inferir. Quatro bytes por unidade, uma vez por keyframe, é o preço
// de uma keyframe que sabe descrever-se sem ajuda.
// =====================================================================
use super::erros::ErroMedia;
use super::ressincronizacao::BYTES_DE_BASE;
use super::telhas::{aplicar, BYTES_INDICE, BYTES_POR_TELHA, LADO, MAX_TELHAS_POR_UNIDADE};

/// O maior número de telhas que uma imagem pode ter: 65 536, ou seja
/// 4096×4096.
///
/// ## Porque é preciso, e porque 16 bytes não bastam
///
/// A primeira validação do cabeçalho aceitava qualquer geometria que
/// fosse múltiplo de 16 — e `65520 × 65520` é múltiplo de 16, e são
/// 4,29 **gigabytes** alocados a partir de doze bytes de texto que
/// qualquer um pode enviar. Um check de «é múltiplo de 16» protege a
/// geometria mas não a **memória**, e é a memória que se ataca.
///
/// 65 536 telhas são 4096×4096 px, que é 16 MB por quadro — 64× mais
/// do que qualquer captura de ecrã plausível, e limitado.
pub const MAX_TELHAS_DE_IMAGEM: usize = 65_536;

/// O comprimento do cabeçalho de uma unidade de lote.
///
/// ```text
/// base_seq      4 B
/// largura       2 B
/// altura        2 B
/// n_unidades    2 B
/// indice        2 B
/// --------------- --
///                 12 B
/// ```
///
/// São 12 bytes contra os 4 de uma delta normal. Uma delta **não** leva
/// este cabeçalho — só uma keyframe leva, e distingue-se pela flag
/// `KEYFRAME` do preâmbulo. O caso comum fica com os mesmos 4 bytes de
/// antes, e 8 bytes de overhead numa keyframe é irrelevante ao lado dos
/// 49 KB que ela transporta.
pub const BYTES_DE_CABECALHO: usize = 12;

/// Quantas telhas cabem numa unidade.
///
/// É [`MAX_TELHAS_POR_UNIDADE`] e é o que torna a fracção correcta: o
/// emissor nunca produz uma unidade com mais telhas do que isto, e por
/// isso nunca produz uma unidade que o `ENCODE` recuse.
pub const TELHAS_POR_UNIDADE: usize = MAX_TELHAS_POR_UNIDADE;

/// O tamanho de uma entrada de telha num delta: índice, código e pixéis.
///
/// ## Por que é sempre 259, e não «o índice ou os pixéis»
///
/// A primeira versão deste módulo aceitava uma entrada de 2 bytes para
/// «telha igual». É uma telha que **nunca sai**: `Telhas::diferenca`
/// salta as iguais com um `continue`, e por isso um delta só tem
/// entradas com variação.
///
/// E a leitura desse formato errado não dava erro — dava um delta que
/// «passava» com o número de telhas errado. Foi assim que o primeiro
/// teste de ponta-a-ponta passou a keyframe por cima de um lote com 4
/// das 12 telhas e o receptor montageu uma imagem com 8/16 do ecrã em
/// preto. Aceitar um formato que o emissor nunca produz é como se
/// aceita um ficheiro corrompido.
const BYTES_DE_ENTRADA: usize = BYTES_INDICE + 1 + BYTES_POR_TELHA;

/// Os índices de telha que o delta referencia, por ordem.
///
/// ## A propriedade que isto garante
///
/// [`Telhas::diferenca`](super::telhas::Telhas::diferenca) percorre a
/// grelha por ordem e salta as iguais. Portanto os índices de um delta
/// são **estritamente crescentes e sem buracos**.
///
/// É essa a integridade que um lote tem de verificar, e é mais forte do
/// que contar telhas: contar telhas não distingue «a telha 7 veio» de
/// «a telha 7 não veio mas a 8 sim», e a segunda imagem é a primeira com
/// um buraco negro do tamanho de um quadrante.
///
/// Buracos à **direita** são legítimos e são a norma: as telhas que não
/// mudam contra o quadro zero não entram na keyframe, e o receptor já
/// as tem a zero.
pub fn indices_do_delta(delta: &[u8]) -> Result<Vec<u16>, ErroMedia> {
    if delta.len() % BYTES_DE_ENTRADA != 0 {
        return Err(ErroMedia::LoteDeltaTruncado(delta.len()));
    }
    let mut indices = Vec::with_capacity(delta.len() / BYTES_DE_ENTRADA);
    for (k, entrada) in delta.chunks_exact(BYTES_DE_ENTRADA).enumerate() {
        let indice = u16::from_be_bytes([entrada[0], entrada[1]]);
        if let Some(&anterior) = indices.last() {
            if indice <= anterior {
                return Err(ErroMedia::LoteIndiceFora {
                    indice,
                    n_unidades: anterior,
                });
            }
        }
        let _ = k;
        indices.push(indice);
    }
    Ok(indices)
}

/// Onde acaba a telha `n`-ésima de um delta, e onde começa a próxima.
///
/// Devolve `(início, fim)`. `início == delta.len()` significa fim do
/// delta, e é o valor que o laço de fracção compara com o limite.
pub fn extentos_das_telhas(delta: &[u8]) -> Result<Vec<(usize, usize)>, ErroMedia> {
    Ok(indices_do_delta(delta)?
        .iter()
        .enumerate()
        .map(|(k, _)| (k * BYTES_DE_ENTRADA, (k + 1) * BYTES_DE_ENTRADA))
        .collect())
}

/// O cabeçalho de uma unidade de lote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CabecalhoLote {
    /// O `seq` a que esta unidade se aplica — igual ao seu `seq`.
    pub base_seq: u32,
    /// A largura da imagem, em pixéis.
    pub largura: u16,
    /// A altura da imagem, em pixéis.
    pub altura: u16,
    /// Quantas unidades tem o lote, no total.
    pub n_unidades: u16,
    /// O índice desta unidade, a zero.
    pub indice: u16,
}

impl CabecalhoLote {
    /// Escreve o cabeçalho no início de `corpo`.
    pub fn escrever(&self, corpo: &mut Vec<u8>) {
        corpo.extend_from_slice(&self.base_seq.to_be_bytes());
        corpo.extend_from_slice(&self.largura.to_be_bytes());
        corpo.extend_from_slice(&self.altura.to_be_bytes());
        corpo.extend_from_slice(&self.n_unidades.to_be_bytes());
        corpo.extend_from_slice(&self.indice.to_be_bytes());
    }

    /// Lê o cabeçalho do início de `corpo`.
    pub fn ler(corpo: &[u8]) -> Result<(CabecalhoLote, &[u8]), ErroMedia> {
        if corpo.len() < BYTES_DE_CABECALHO {
            return Err(ErroMedia::LoteTruncado(corpo.len()));
        }
        let c = CabecalhoLote {
            base_seq: u32::from_be_bytes([corpo[0], corpo[1], corpo[2], corpo[3]]),
            largura: u16::from_be_bytes([corpo[4], corpo[5]]),
            altura: u16::from_be_bytes([corpo[6], corpo[7]]),
            n_unidades: u16::from_be_bytes([corpo[8], corpo[9]]),
            indice: u16::from_be_bytes([corpo[10], corpo[11]]),
        };
        c.validar()?;
        Ok((c, &corpo[BYTES_DE_CABECALHO..]))
    }

    /// Verifica os campos que não são livres.
    ///
    /// A geometria é validada aqui e não em [`aplicar`] por uma razão
    /// concreta: a geometria decide **quantos bytes de memória** este
    /// cabeçalho manda o receptor alocar. Um `largura` de 65534 com
    /// `altura` de 65534 é quatro gigabytes a partir de doze bytes de
    /// texto, e isso é um amplificador que não se quer num protocolo
    /// de texto.
    fn validar(&self) -> Result<(), ErroMedia> {
        if self.n_unidades == 0 {
            return Err(ErroMedia::LoteVazio);
        }
        if self.indice >= self.n_unidades {
            return Err(ErroMedia::LoteIndiceFora {
                indice: self.indice,
                n_unidades: self.n_unidades,
            });
        }
        if self.largura as usize % LADO != 0 || self.altura as usize % LADO != 0 {
            return Err(ErroMedia::GeometriaInvalida(
                "geometria do lote não é múltiplo de 16",
            ));
        }
        if self.largura == 0 || self.altura == 0 {
            return Err(ErroMedia::GeometriaInvalida("geometria do lote é zero"));
        }
        let telhas = (self.largura as usize / LADO) * (self.altura as usize / LADO);
        if telhas > MAX_TELHAS_DE_IMAGEM {
            return Err(ErroMedia::ImagemEnorme {
                telhas,
                tecto: MAX_TELHAS_DE_IMAGEM,
            });
        }
        Ok(())
    }

    /// Se esta unidade é a última do lote.
    pub fn eh_ultima(&self) -> bool {
        self.indice + 1 == self.n_unidades
    }
}

/// Quantas unidades um delta de `n_telhas` telhas ocupa.
pub fn n_unidades(n_telhas: usize) -> usize {
    n_telhas.div_ceil(TELHAS_POR_UNIDADE).max(1)
}

/// Uma parte de um lote: o cabeçalho e os bytes das suas telhas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parte {
    pub cabecalho: CabecalhoLote,
    pub delta: Vec<u8>,
}

/// Fraciona um delta de keyframe em unidades que cabem num envelope.
///
/// `delta` é o delta contra um quadro zero, e o que sai são
/// [`TELHAS_POR_UNIDADE`] telhas por unidade — **nos limites de telha**,
/// nunca a meio de uma. Cortar a meio de uma telha daria um corpo que
/// o receptor não distingue de um corrompido, e o `SHA-256` do §4
/// passaria a falhar por uma razão que não é de integridade.
pub fn dividir(
    delta: &[u8],
    base_seq: u32,
    largura: u16,
    altura: u16,
) -> Result<Vec<Parte>, ErroMedia> {
    if largura as usize % LADO != 0 || altura as usize % LADO != 0 {
        return Err(ErroMedia::GeometriaInvalida(
            "a geometria de um lote tem de ser múltiplo de 16",
        ));
    }
    let total = extentos_das_telhas(delta)?;

    // Um delta **vazio** é um ecrã todo a zero, e é um caso real —
    // ecrã apagado, ecrã bloqueado. A versão anterior indexava
    // `total[ate - 1]` com `ate == 0` e entrava em pânico. Panic num
    // caminho de rede é uma negação de serviço por quem apaga o ecrã.
    if total.is_empty() {
        return Ok(vec![Parte {
            cabecalho: CabecalhoLote {
                base_seq,
                largura,
                altura,
                n_unidades: 1,
                indice: 0,
            },
            delta: Vec::new(),
        }]);
    }

    let n = n_unidades(total.len());
    if n > u16::MAX as usize {
        return Err(ErroMedia::LoteGrandeDemais(n));
    }

    let mut partes = Vec::with_capacity(n);
    for indice in 0..n {
        let de = indice * TELHAS_POR_UNIDADE;
        let ate = (de + TELHAS_POR_UNIDADE).min(total.len());
        let (ini, _) = total[de];
        let (_, fim) = total[ate - 1];
        let cabecalho = CabecalhoLote {
            base_seq: base_seq.wrapping_add(indice as u32),
            largura,
            altura,
            n_unidades: n as u16,
            indice: indice as u16,
        };
        partes.push(Parte {
            cabecalho,
            delta: delta[ini..fim].to_vec(),
        });
    }
    Ok(partes)
}

/// O que um lote em curso sabe.
#[derive(Debug, Clone)]
struct EmCurso {
    cabecalho: CabecalhoLote,
    esperado: u16,
    delta: Vec<u8>,
}

/// O que o montador diz ao receptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Montagem {
    /// Ainda a juntar: esta unidade entrou, e as seguintes são o resto.
    EmCurso,
    /// O lote está completo e a imagem pode ser montada.
    Completo(Vec<u8>),
    /// A unidade não entrava num lote em curso e foi descartada.
    Descartada,
}

/// O receptor de lotes: junta as unidades de uma keyframe.
///
/// ## O que um lote interrompido faz
///
/// Nada. Uma unidade com um índice que não é o esperado **descarta o
/// lote inteiro** e devolve [`Montagem::Descartada`]. A unidade que
/// chega a seguir com índice 0 começa outro lote, e o receptor fica
/// sem imagem até uma keyframe nova — que é o que o [`crate::media::ressincronizacao::Receptor`]
/// pede, porque `ultimo_aplicado` não avançou.
///
/// A alternativa — guardar as partes e esperar — seria memória que
/// cresce sem limite à espera de uma unidade que talvez nunca venha.
#[derive(Debug, Default)]
pub struct MontadorLote {
    em_curso: Option<EmCurso>,
    /// Quantas vezes um lote foi descartado por perda.
    ///
    /// Vale mais que um contador genérico porque permite distinguir
    /// «perdi o meio de uma keyframe» (este número sobe) de «chegou
    /// uma keyframe intacta» (não sobe).
    pub descartados: usize,
}

impl MontadorLote {
    /// Um montador sem lote em curso.
    pub fn novo() -> Self {
        Self::default()
    }

    /// Se não há nada a meio.
    pub fn esta_vazio(&self) -> bool {
        self.em_curso.is_none()
    }

    /// Quantas telhas do lote em curso já chegaram.
    pub fn telhas_recebidas(&self) -> usize {
        match &self.em_curso {
            None => 0,
            Some(e) => extentos_das_telhas(&e.delta).map(|t| t.len()).unwrap_or(0),
        }
    }

    /// Aceita uma unidade de lote.
    ///
    /// ## Porque a geometria vem do cabeçalho e não de um campo à parte
    ///
    /// Vem do cabeçalho porque a keyframe **é** a imagem, e a imagem é
    /// que tem a geometria. Uma keyframe que precisa de contexto para
    /// ser montada não é auto-suficiente, e auto-suficiência é a
    /// única coisa que torna uma keyframe útil.
    pub fn aceitar(&mut self, corpo: &[u8]) -> Result<Montagem, ErroMedia> {
        let (c, delta) = CabecalhoLote::ler(corpo)?;

        if c.indice == 0 {
            // Um lote novo **descarta** qualquer lote em curso. Uma
            // keyframe nova substitui a antiga por completo, e misturar
            // telhas de duas keyframes daria uma imagem que não é de
            // nenhuma das duas.
            self.em_curso = Some(EmCurso {
                cabecalho: c,
                esperado: 1,
                delta: delta.to_vec(),
            });
        } else {
            let Some(mut e) = self.em_curso.take() else {
                // Uma continuação sem início é lixo, e a única resposta
                // segura é descartá-la: não há de que montar.
                self.descartados += 1;
                return Ok(Montagem::Descartada);
            };
            if c.indice != e.esperado || c.n_unidades != e.cabecalho.n_unidades {
                // Falta uma unidade pelo meio. O lote não se recupera.
                self.descartados += 1;
                return Ok(Montagem::Descartada);
            }
            e.delta.extend_from_slice(delta);
            e.esperado += 1;
            self.em_curso = Some(e);
        }

        let e = self.em_curso.as_ref().expect("acabou de se colocar");
        if !c.eh_ultima() {
            return Ok(Montagem::EmCurso);
        }

        // A última. Verifica a integridade **antes** de alocar, e a
        // verificação é de índices e não de contagem.
        //
        // Uma keyframe é o delta contra um quadro zero, e `diferenca`
        // salta as telhas iguais — por isso um delta de keyframe **não**
        // tem todas as telhas, e exigir `recebidas == total` seria
        // exigir que a imagem tivesse zero em todas as casas onde o
        // ecrã tem zero. Foi esse o erro da primeira versão, e o
        // sintoma era uma imagem com um quadrante em preto.
        //
        // O que tem de ser verdade é o que [`indices_do_delta`]
        // garante: índices crescentes e sem buracos. Um buraco é uma
        // telha que mudava e não chegou.
        let c = e.cabecalho;
        let total = (c.largura as usize / LADO) * (c.altura as usize / LADO);
        let indices = indices_do_delta(&e.delta)?;
        match indices.last() {
            // Zero telhas com variação **é** uma imagem: a imagem
            // toda a zero. Um ecrã apagado ou bloqueado produz isto, e
            // recusá-lo seria recusar uma keyframe legítima — e o
            // receptor ficaria sem imagem nenhuma à espera de uma
            // segunda, porque a próxima também seria preta.
            None => {}
            Some(&ultimo) if (ultimo as usize) >= total => {
                self.descartados += 1;
                self.em_curso = None;
                return Err(ErroMedia::LoteIndiceFora {
                    indice: ultimo,
                    n_unidades: total as u16,
                });
            }
            Some(_) => {}
        }

        let mut quadro = vec![0u8; c.largura as usize * c.altura as usize];
        aplicar(&e.delta, &mut quadro, c.largura as usize)?;
        self.em_curso = None;
        Ok(Montagem::Completo(quadro))
    }

    /// Deita fora um lote em curso, se houver.
    ///
    /// Existe para a largura/altura de captura **mudar**: um lote a meio
    /// da geometria antiga não pode ser completado por um da nova.
    pub fn limpar(&mut self) {
        if self.em_curso.take().is_some() {
            self.descartados += 1;
        }
    }
}

/// O cabeçalho de uma unidade de lote, sem o `base_seq`.
///
/// Existe porque [`super::ressincronizacao::delta_de`] corta 4 bytes e
/// numa unidade de lote tem de cortar 12. Uma função que sabe o
/// comprimento do cabeçalho é uma função que não tem de ser mudada em
/// dois sítios quando o cabeçalho muda.
pub fn delta_de(corpo: &[u8]) -> Result<&[u8], ErroMedia> {
    let (_, delta) = CabecalhoLote::ler(corpo)?;
    Ok(delta)
}

#[cfg(test)]
mod testes;
