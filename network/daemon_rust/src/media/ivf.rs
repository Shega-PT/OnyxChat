// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/ivf.rs — partir o contentor VP8 em quadros
// ---------------------------------------------------------------------
// ## O que é isto
//
// O `ffmpeg` escreve o vídeo em **IVF** no descritor 1 (§8.1), e o
// receptor tem de saber onde acaba um quadro para poder enviá-lo como
// uma unidade. Este módulo é essa leitura.
//
// ## Por que IVF e não WebM
//
// IVF é o contentor mínimo que dá: 32 bytes de cabeçalho e depois
// `tamanho:u32le ‖ timestamp:u64le ‖ quadro`, repetido. Um pipe não
// pode usar o índice do WebM, e o índice era a única coisa que o WebM
// trazia a mais.
//
// ## Os dois factos do formato que mordem
//
// **O cabeçalho diz `numero_de_frames = 1`.** O `ffmpeg` escreve o
// cabeçalho antes de saber quantos quadros vai produzir, e por isso o
// campo vale 1 num stream. Confiar nele dava um receptor que leia um
// quadro e acha que acabou — e uma chamada que mostra um frame de
// dez em dez.
//
// **Os quadros não estão em pacotes independentes.** O VP8 tem
// frames-I e frames-P, e um frame-P é um delta contra os anteriores.
// Isto **não é um problema** para o transporte — a unidade é o
// frame-P e o receptor reconstrói — mas significa que o receptor não
// pode descartar um quadro ao acaso: descartar o I força todos os
// seguintes a cair até ao próximo I. É a razão de o §8.2 pedir
// descarte pelo **antigo** e nunca pelo recente.
// =====================================================================

/// O tamanho do cabeçalho de um IVF.
pub const TAM_CABECALHO: usize = 32;

/// O tamanho do preâmbulo de cada quadro: `u32` + `u64`.
pub const TAM_PREAMBULO: usize = 12;

/// A assinatura de um IVF.
pub const ASSINATURA: [u8; 4] = *b"DKIF";

/// O `fourcc` de VP8 dentro de um IVF.
pub const FOURCC_VP8: [u8; 4] = *b"VP80";

/// O que um cabeçalho de IVF diz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cabecalho {
    /// A largura declarada, em pixels.
    ///
    /// E **declarada**, não verificada: o receptor descobre a
    /// geometria real dos bits do VP8, e este campo é o que o `ffmpeg`
    /// disse. Se divergirem, quem diverge é o emissor — e ver §6.1.
    pub largura: u16,
    /// A altura declarada, em pixels.
    pub altura: u16,
    /// O denominador do tempo, em fps.
    pub fps: u32,
}

impl Cabecalho {
    /// Lê o cabeçalho de 32 bytes.
    ///
    /// ## O que esta função **recusa**, e porquê
    ///
    /// Recusa um cabeçalho cuja assinatura não é `DKIF` ou cujo
    /// `fourcc` não é `VP80`. Não é desconfiança do emissor: é que um
    /// pipe pode receber o que outro programa escreva, e um leitor que
    /// tratasse bytes desconhecidos como quadros VP8 decodificaria
    /// silêncio e apresentaria um ecrã preto sem nenhum sinal.
    pub fn ler(bytes: &[u8]) -> Result<Cabecalho, ErroIvf> {
        if bytes.len() < TAM_CABECALHO {
            return Err(ErroIvf::CabecalhoCurto(bytes.len()));
        }
        if bytes[0..4] != ASSINATURA {
            return Err(ErroIvf::AssinaturaInvalida);
        }
        if bytes[8..12] != FOURCC_VP8 {
            // Um IVF pode conter VP9, AV1 ou qualquer outro. Aceitar
            // seria aceitar um codec que este daemon não tem.
            return Err(ErroIvf::FourccDesconhecido([
                bytes[8], bytes[9], bytes[10], bytes[11],
            ]));
        }
        // A versão tem de ser 0. A 1 o cabeçalho tem campos extra, e
        // ler os campos de lá como se fossem os daqui daria uma
        // geometria errada em silêncio.
        if bytes[4] != 0 {
            return Err(ErroIvf::VersaoDesconhecida(bytes[4]));
        }
        Ok(Cabecalho {
            largura: u16::from_le_bytes([bytes[12], bytes[13]]),
            altura: u16::from_le_bytes([bytes[14], bytes[15]]),
            fps: u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
        })
    }
}

/// O que o receptor faz com um IVF.
#[derive(Debug)]
pub struct LeitorIvf<R> {
    /// O cabeçalho, lido uma vez.
    cab: Cabecalho,
    /// A fonte. Um `BufRead` para não ler um byte de cada vez.
    fonte: R,
    /// Já se leu o cabeçalho?
    ///
    /// Sem esta flag o primeiro [`LeitorIvf::proximo`] teria de tentar
    /// ler o cabeçalho outra vez, e o segundo quadro seria tomado por
    /// cabeçalho.
    cabecalho_lido: bool,
}

/// Um quadro lido: os bytes e o tempo em que o `ffmpeg` o produziu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroIvf {
    /// Os bytes do quadro, tal e qual do contentor.
    ///
    /// **Não** decodificados. Decodificar é trabalho do `ffmpeg`
    /// receptor, que tem o VP8 — e o daemon embebido só tem o
    /// *encoder*, porque o `build.sh` não liga os decoders que não são
    /// necessários para codificar.
    pub dados: Vec<u8>,
    /// O timestamp do quadro, na unidade do cabeçalho.
    pub timestamp: u64,
}

/// Um quadro lido, ou o fim do stream.
#[derive(Debug)]
pub enum EstadoQuadro {
    /// Havia um quadro.
    Quadro(QuadroIvf),
    /// O stream acabou.
    ///
    /// O `ffmpeg` morreu. Não é um erro a recuperar: a chamada termina
    /// aqui, e quem aterminate quem decide.
    Fim,
}

impl<R: std::io::Read> LeitorIvf<R> {
    /// Cria um leitor a partir de uma fonte de bytes.
    pub fn novo(mut fonte: R) -> Result<LeitorIvf<R>, ErroIvf> {
        // O cabeçalho é lido aqui e não no primeiro `proximo`, para que
        // um stream que não é IVF seja recusado **antes** de a
        // captura ser declarada viva. Um leitor que só descobrisse no
        // primeiro quadro deixaria um pipe aberto à espera de um IVF
        // que nunca chega.
        let mut cab = [0u8; TAM_CABECALHO];
        let mut lidos = 0usize;
        while lidos < TAM_CABECALHO {
            match ler_ate(&mut fonte, &mut cab[lidos..])? {
                0 if lidos == 0 => return Err(ErroIvf::FimAntesDoCabecalho),
                0 => return Err(ErroIvf::CabecalhoCurto(lidos)),
                n => lidos += n,
            }
        }
        let cab = Cabecalho::ler(&cab)?;
        Ok(LeitorIvf {
            cab,
            fonte,
            cabecalho_lido: true,
        })
    }

    /// O cabeçalho, que já foi lido e validado.
    pub fn cabecalho(&self) -> Cabecalho {
        self.cab
    }

    /// Se a geometria do cabeçalho é a esperada.
    ///
    /// Não é uma validação de segurança — é de consistência. Um
    /// cabeçalho a dizer 320×240 quando o emissor disse 854×480 é um
    /// emissor que mudou de ideias a meio, e o receptor tem de
    /// descobrir isso antes de apresentar uma imagem distorcida.
    pub fn geometria_esperada(&self, largura: u16, altura: u16) -> bool {
        self.cab.largura == largura && self.cab.altura == altura
    }

    /// Lê o próximo quadro, ou o fim do stream.
    ///
    /// Um EOF limpo no meio do preâmbulo de um quadro é
    /// [`ErroIvf::QuadroTruncado`] e não fim: o `ffmpeg` morreu a meio
    /// de escrever, e isso é diferente de ter terminado.
    pub fn proximo(&mut self) -> Result<EstadoQuadro, ErroIvf> {
        debug_assert!(self.cabecalho_lido);
        let mut pre = [0u8; TAM_PREAMBULO];
        match ler_ate(&mut self.fonte, &mut pre)? {
            0 => return Ok(EstadoQuadro::Fim),
            n if n < TAM_PREAMBULO => return Err(ErroIvf::QuadroTruncado(n)),
            _ => {}
        }
        let tamanho = u32::from_le_bytes([pre[0], pre[1], pre[2], pre[3]]);
        let timestamp = u64::from_le_bytes([
            pre[4], pre[5], pre[6], pre[7], pre[8], pre[9], pre[10], pre[11],
        ]);

        // Um quadro de tamanho zero é um quadro que não existe, e
        // aceitá-lo seria um quadro de comprimento zero a seguir no
        // fluxo — que leria o próximo cabeçalho como se fosse payload.
        if tamanho == 0 {
            return Err(ErroIvf::QuadroVazio);
        }
        // O `usize` tem 64 bits nesta plataforma, e um `u32` cabe
        // sempre. A conversão não falha, e escrevê-la como se falhasse
        // seria um `unwrap` deitado fora com mais linhas.
        let tamanho = tamanho as usize;

        let mut dados = vec![0u8; tamanho];
        let mut lidos = 0usize;
        while lidos < tamanho {
            match ler_ate(&mut self.fonte, &mut dados[lidos..])? {
                0 => return Err(ErroIvf::QuadroTruncado(lidos)),
                n => lidos += n,
            }
        }
        Ok(EstadoQuadro::Quadro(QuadroIvf { dados, timestamp }))
    }
}

/// Lê uma vez, tolerando um `EINTR`.
///
/// Um `read` interrupted é normal em Rust e **não** é fim de stream.
/// Ignorá-lo dava um receptor que truncava o stream ao primeiro sinal
/// do sistema — e o sintoma seria «a chamada pára a meio sem erro».
fn ler_ate<R: std::io::Read>(fonte: &mut R, buf: &mut [u8]) -> Result<usize, ErroIvf> {
    loop {
        match fonte.read(buf) {
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(ErroIvf::Leitura(e)),
        }
    }
}

/// O que pode dar errado a ler um IVF.
#[derive(Debug)]
pub enum ErroIvf {
    /// O stream acabou antes dos 32 bytes do cabeçalho.
    FimAntesDoCabecalho,
    /// O cabeçalho tem menos de 32 bytes.
    CabecalhoCurto(usize),
    /// A assinatura não é `DKIF`.
    AssinaturaInvalida,
    /// A versão do IVF não é 0.
    VersaoDesconhecida(u8),
    /// O `fourcc` não é `VP80`.
    FourccDesconhecido([u8; 4]),
    /// Um quadro ficou a meio.
    ///
    /// Leva quantos bytes chegaram, porque «truncado» sem número
    /// obriga a ir ao `hexdump` para descobrir se foi um quadro
    /// perdido ou um cabeçalho por ler.
    QuadroTruncado(usize),
    /// Um quadro de tamanho zero.
    ///
    /// Numa leitura de quatro em quatro bytes significaria que o
    /// `ffmpeg` escreveu um comprimento em vez de um tamanho, e sem
    /// este erro o receptor alocaria e leria o resto do stream como
    /// se fosse payload.
    QuadroVazio,
    /// Um tamanho de quadro que não cabe em `usize` — e num `u32`
    /// num sistema de 64 bits isso só pode ser lixo.
    QuadroGigante(u32),
    /// A leitura falhou.
    Leitura(std::io::Error),
}

impl std::fmt::Display for ErroIvf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErroIvf::FimAntesDoCabecalho => {
                write!(f, "o stream de vídeo acabou antes do cabeçalho IVF")
            }
            ErroIvf::CabecalhoCurto(n) => {
                write!(f, "cabeçalho IVF com {n} bytes, e são 32")
            }
            ErroIvf::AssinaturaInvalida => write!(f, "isto não é um IVF: assinatura diferente"),
            ErroIvf::VersaoDesconhecida(v) => {
                write!(f, "versão de IVF {v}, e o que se conhece é 0")
            }
            ErroIvf::FourccDesconhecido(fourcc) => write!(
                f,
                "o IVF não traz VP8: fourcc {}",
                String::from_utf8_lossy(fourcc)
            ),
            ErroIvf::QuadroTruncado(n) => {
                write!(f, "quadro IVF truncado em {n} bytes")
            }
            ErroIvf::QuadroVazio => write!(f, "quadro IVF de tamanho zero"),
            ErroIvf::QuadroGigante(n) => {
                write!(f, "quadro IVF de {n} bytes, que não cabe em memória")
            }
            ErroIvf::Leitura(e) => write!(f, "a leitura do IVF falhou: {e}"),
        }
    }
}

impl std::error::Error for ErroIvf {}

#[cfg(test)]
mod testes;
