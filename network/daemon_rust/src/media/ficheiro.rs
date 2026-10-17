// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/ficheiro.rs — o modo não-streaming: fracção, manifesto, montagem
// ---------------------------------------------------------------------
// Um ficheiro — um áudio, um vídeo — entra em pedaços de 48 KB, cada um
// vira uma mensagem `CHAT` normal, e o receptor volta a juntá-los.
//
// ## Porque este modo é o caminho por omissão
//
// Porque para tudo o que não seja uma chamada, é **objectivamente**
// melhor do que o streaming: o mesmo conteúdo em 10 segundos dá 26
// mensagens contra 300, porque o streaming paga 161 bytes de overhead
// **por frame** e este paga-os **por cada 48 KB**. Não há anel, não há
// relógio, não há telhas, e o resultado é reproduzível quantas vezes se
// quiser. `docs/media.md` §Introdução.
//
// ## O resumo que não é o do manifesto
//
// O SHA-256 é do **conteúdo montado**, não da concatenação das fatias
// codificadas. A diferença importa porque o conteúdo vai passar por
// base-64 (`super::base64`), e um resumo calculado antes de codificar
// não seria o resumo do que o receptor tem depois de descodificar —
// seria o resumo de outra coisa, e a comparação falharia sempre sem que
// ninguém percebesse porquê.
//
// ## Porque nada toca o disco, nem ao receber
//
// Porque o receptor monta o ficheiro **inteiro em memória** e executa o
// leitor de um `memfd`. Um ficheiro temporário de 50 MB seria visível a
// outro utilizador da mesma máquina (`docs/threat_model.md` §A8) e
// sobreviveria ao `unlink` até o último descritor fechar.
// =====================================================================

use std::io::Read;

use sha2::{Digest, Sha256};

use super::base64;
use super::erros::ErroMedia;
use super::moldura::{construir_com_flags, Flags, Modo, Tipo, TAM_MEDIA_MAX};

/// O tamanho de uma fatia, em bytes.
///
/// `TAM_MEDIA_MAX` e não um literal: o limite vem do `MAX_PLAINTEXT` e
/// do comprimento do preâmbulo, e um número escrito aqui divergiria do
/// que o `ENCODE` aceita — e a falha apareceria como
/// `0x09 PayloadGrandeDemais` a meio de uma transferência, um erro cujo
/// nome não diz nada sobre a sua causa.
pub const TAM_FATIA: usize = TAM_MEDIA_MAX;

/// O tamanho do campo `total_bytes` do manifesto.
pub const TAM_TOTAL: usize = 4;

/// O código do codec — **o que se descodifica**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// Opus: áudio comprimido. `0x0001`
    Opus,
    /// Luma de 8 bits por pixel, sem compressão. `0x0002`
    RawVideoGray,
    /// Matroska: só em não-streaming (§8.5). `0x0003`
    Matroska,
}

impl Codec {
    /// O valor no manifesto.
    pub fn valor(self) -> u16 {
        match self {
            Codec::Opus => 0x0001,
            Codec::RawVideoGray => 0x0002,
            Codec::Matroska => 0x0003,
        }
    }

    /// Converte um valor do manifesto, ou falha.
    ///
    /// Fecha por construção, como `super::moldura::Tipo`: um `u16`
    /// desconhecido **não pode** virar um codec, porque um codec errado
    /// descodifica um ficheiro para ruído e o utilizador vê ruído em
    /// vez de um erro.
    pub fn de_valor(v: u16) -> Result<Codec, ErroMedia> {
        match v {
            0x0001 => Ok(Codec::Opus),
            0x0002 => Ok(Codec::RawVideoGray),
            0x0003 => Ok(Codec::Matroska),
            _ => Err(ErroMedia::CodecDesconhecido(v)),
        }
    }

    /// O `Tipo` de moldura que este codec transporta.
    ///
    /// `Codec` e `Tipo` são enums separados porque respondem a
    /// perguntas diferentes: o `Tipo` escolhe a **chave**, o `Codec`
    /// descreve o **conteúdo**. Um vídeo Matroska tem `Tipo::Video` e
    /// `Codec::Matroska`; o áudio Opus dentro dele tem
    /// `Codec::Opus`.
    pub fn tipo_de_moldura(self) -> Tipo {
        match self {
            Codec::Opus => Tipo::Audio,
            Codec::RawVideoGray | Codec::Matroska => Tipo::Video,
        }
    }

    /// Este codec é válido em streaming?
    ///
    /// O Matroska não é (§8.5, §3.3): um contentor tem cabeçalhos e
    /// índices de timestamp que um delta por telhas não representa, e
    /// usá-lo em streaming daria uma imagem que só se recompõe no fim.
    /// A função existe para o emissor dizer «não» **antes** de gravar,
    /// e não no meio.
    pub fn valido_em_streaming(self) -> bool {
        !matches!(self, Codec::Matroska)
    }
}

/// O código do formato — **o contentor**.
///
/// Separado de [`Codec`] porque são coisas diferentes: o `Codec` diz o
/// que se descodifica, o `Formato` diz o que o contentor é. Um vídeo
/// Matroska com Opus dentro tem `Codec::Opus` e `Formato::Matroska`, e
/// o receptor precisa das duas coisas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formato {
    /// Opus cru, num contentor Ogg. `0x0001`
    Ogg,
    /// MP3. `0x0002`
    Mp3,
    /// WAV. `0x0003`
    Wav,
    /// `rawvideo` sem contentor. `0x0010`
    RawVideo,
    /// MP4. `0x0020`
    Mp4,
    /// Matroska. `0x0021`
    Matroska,
    /// WebM. `0x0022`
    WebM,
}

impl Formato {
    /// O valor no manifesto.
    pub fn valor(self) -> u16 {
        match self {
            Formato::Ogg => 0x0001,
            Formato::Mp3 => 0x0002,
            Formato::Wav => 0x0003,
            Formato::RawVideo => 0x0010,
            Formato::Mp4 => 0x0020,
            Formato::Matroska => 0x0021,
            Formato::WebM => 0x0022,
        }
    }

    /// Converte um valor do manifesto, ou falha.
    pub fn de_valor(v: u16) -> Result<Formato, ErroMedia> {
        match v {
            0x0001 => Ok(Formato::Ogg),
            0x0002 => Ok(Formato::Mp3),
            0x0003 => Ok(Formato::Wav),
            0x0010 => Ok(Formato::RawVideo),
            0x0020 => Ok(Formato::Mp4),
            0x0021 => Ok(Formato::Matroska),
            0x0022 => Ok(Formato::WebM),
            _ => Err(ErroMedia::FormatoDesconhecido(v)),
        }
    }

    /// A extensão que o leitor de media precisa para escolher o
    /// demuxer.
    ///
    /// ## Porque uma tabela e não `format!("{:?}")`
    ///
    /// O `Debug` de [`Formato::Matroska`] é `"Matroska"`, e
    /// `onyx-media.Matroska` é um nome que nenhum leitor reconhece. A
    /// tabela é o que liga o valor do manifesto ao que o `mpv` e o
    /// `ffplay` aceitam, e é escrita à mão por isso — um `Debug`
    /// automático seria uma segunda fonte de verdade sobre um nome que
    /// só o sistema pode validar.
    pub fn estensao(self) -> &'static str {
        match self {
            Formato::Ogg => "ogg",
            Formato::Mp3 => "mp3",
            Formato::Wav => "wav",
            Formato::RawVideo => "gray",
            Formato::Mp4 => "mp4",
            Formato::Matroska => "mkv",
            Formato::WebM => "webm",
        }
    }

    /// Descobre o formato pelo sufixo do nome do ficheiro.
    ///
    /// Para o emissor, que tem um caminho mas não sabe o que é. Uma
    /// extensão desconhecida **não** adivinha: devolve `None` e o
    /// chamador tem de dizer.
    ///
    /// E é por isso que `ffprobe` — que o `ffmpeg` embebido traz — não
    /// é usado aqui: o formato é um **facto declarado** no manifesto, e
    /// um detector é uma opinião. Opiniões divergem entre versões.
    pub fn de_nome(nome: &str) -> Option<Formato> {
        let extensao = nome.rsplit_once('.').map(|(_, e)| e)?.to_ascii_lowercase();
        match extensao.as_str() {
            "ogg" | "oga" | "opus" => Some(Formato::Ogg),
            "mp3" => Some(Formato::Mp3),
            "wav" => Some(Formato::Wav),
            "gray" | "raw" => Some(Formato::RawVideo),
            "mp4" | "m4v" => Some(Formato::Mp4),
            "mkv" => Some(Formato::Matroska),
            "webm" => Some(Formato::WebM),
            _ => None,
        }
    }

    /// O `Codec` que este formato transporta.
    ///
    /// A correspondência é o inverso de [`Codec`], e existe porque o
    /// emissor descobre o **formato** pelo nome e precisa de saber o
    /// que vai dentro. Um formato que pode conter duas coisas — o
    /// Matroska — devolve `None`, e o emissor tem de escolher.
    pub fn codec_por_defeito(self) -> Option<Codec> {
        match self {
            Formato::Ogg | Formato::Mp3 | Formato::Wav => Some(Codec::Opus),
            Formato::RawVideo => Some(Codec::RawVideoGray),
            Formato::Mp4 | Formato::WebM => Some(Codec::Matroska),
            // O Matroska não decide: é o contentor mais genérico que
            // existe, e escolher por ele seria chutar.
            Formato::Matroska => None,
        }
    }
}

/// O manifesto de uma transferência (`docs/media.md` §2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifesto {
    /// O tipo de conteúdo, que escolhe a chave.
    pub tipo: Tipo,
    /// O codec do conteúdo.
    pub codec: Codec,
    /// O contentor.
    pub formato: Formato,
    /// O tamanho total, em bytes.
    pub total_bytes: u32,
    /// Quantas fatias a transferência tem.
    pub total_pedacos: u32,
    /// O tamanho de cada fatia. Fica no manifesto para o receptor
    /// detectar uma transferência montada com um tamanho diferente.
    pub bytes_por_pedaco: u16,
    /// A duração, em segundos.
    pub duracao_s: u32,
    /// O SHA-256 do conteúdo montado.
    pub sha256: [u8; 32],
    /// O nome do ficheiro, sem bytes nulos.
    pub nome: String,
}

/// O tamanho fixo do manifesto, **sem** o nome: 17 bytes de campos +
/// 32 do resumo (`docs/media.md` §2.5).
pub const TAM_MANIFESTO_FIXO: usize = 1 + 4 + 4 + 2 + 2 + 2 + 4 + 32;

/// O identificador da transferência, tal como viaja no manifesto.
///
/// 16 caracteres hex, e não os 32 bytes do resumo: o resumo já está no
/// manifesto, e um `id` separado seria o mesmo valor duas vezes.
pub type IdTransferencia = [u8; 16];

/// Deriva o `id` da transferência do seu resumo.
///
/// Os **primeiros** 16 bytes do SHA-256, e não uma truncagem de texto
/// com o nome: o nome é escolhido por quem envia e mudaria o `id` sem
/// mudar o conteúdo, o que faria duas transferências do mesmo ficheiro
/// parecerem diferentes.
pub fn id_do_resumo(resumo: &[u8; 32]) -> IdTransferencia {
    let mut id = [0u8; 16];
    id.copy_from_slice(&resumo[..16]);
    id
}

/// O `id` em hexadecimal, como aparece no texto da resposta de retoma.
pub fn id_em_hex(id: &IdTransferencia) -> String {
    use std::fmt::Write as _;
    let mut saida = String::with_capacity(id.len() * 2);
    for byte in id {
        // `write!` sobre `String` não falha; um `format!` por byte
        // seriam 16 alocações.
        let _ = write!(saida, "{byte:02x}");
    }
    saida
}

impl Manifesto {
    /// Monta o manifesto a partir de um conteúdo já lido.
    ///
    /// `conteudo` é o ficheiro **inteiro**. Recebe-lo inteiro em vez de
    /// o ler em porções é deliberado: o SHA-256 é do conteúdo montado
    /// e o `total_bytes` é o seu tamanho, e as duas coisas só se sabem
    /// depois de o ter lido todo.
    ///
    /// Quereceba um `&[u8]` e não um `impl Read` porque a montagem
    /// final precisa do mesmo conteúdo e tê-lo duas vezes em memória
    /// seria duplicar o pico — que num vídeo de 100 MB num tecto de
    /// 1 GB (`orcamento.rs`) é a diferença entre caber e não caber.
    pub fn novo(
        tipo: Tipo,
        codec: Codec,
        formato: Formato,
        duracao_s: u32,
        nome: &str,
        conteudo: &[u8],
    ) -> Result<Manifesto, ErroMedia> {
        let total_bytes =
            u32::try_from(conteudo.len()).map_err(|_| ErroMedia::FicheiroGrandeDemais)?;

        Ok(Manifesto {
            tipo,
            codec,
            formato,
            total_bytes,
            total_pedacos: numero_de_fatias(total_bytes as usize),
            bytes_por_pedaco: u16::try_from(TAM_FATIA)
                .map_err(|_| ErroMedia::FicheiroGrandeDemais)?,
            duracao_s,
            sha256: resumo(conteudo),
            nome: nome.to_string(),
        })
    }

    /// As fatias de um conteúdo que corresponda a este manifesto.
    ///
    /// Leva o conteúdo e **confirma que o tamanho bate** com
    /// `total_bytes`. Um conteúdo com um tamanho diferente produziria
    /// um número de fatias diferente do que o receptor espera, e a
    /// montagem fecharia com o resumo errado — que é o tipo de falha
    /// que só aparece na última peça.
    pub fn fatias<'a>(&self, conteudo: &'a [u8]) -> Result<Vec<&'a [u8]>, ErroMedia> {
        if conteudo.len() != self.total_bytes as usize {
            return Err(ErroMedia::ManifestoIncoerente(
                "o conteúdo não tem o tamanho declarado",
            ));
        }
        Ok(fatiar(conteudo, self.bytes_por_pedaco as usize).collect())
    }

    /// O texto do manifesto, pronto a ser montado numa [`super::moldura`].
    ///
    /// Devolve os **bytes do corpo**, não a moldura completa: quem chama
    /// isto sabe o `stream` e o `seq` que quer, e montá-lo aqui
    /// obrigaria a duplicar a [`super::moldura`] num sítio onde não
    /// vive o conhecimento do fluxo.
    pub fn corpo(&self) -> Vec<u8> {
        let mut corpo = Vec::with_capacity(
            TAM_MANIFESTO_FIXO + base64::codificar_bytes(self.nome.as_bytes()).len(),
        );
        corpo.push(self.tipo.caractere());
        corpo.extend_from_slice(&self.total_bytes.to_be_bytes());
        corpo.extend_from_slice(&self.total_pedacos.to_be_bytes());
        corpo.extend_from_slice(&self.bytes_por_pedaco.to_be_bytes());
        corpo.extend_from_slice(&self.codec.valor().to_be_bytes());
        corpo.extend_from_slice(&self.formato.valor().to_be_bytes());
        corpo.extend_from_slice(&self.duracao_s.to_be_bytes());
        corpo.extend_from_slice(&self.sha256);
        corpo.extend_from_slice(base64::codificar_bytes(self.nome.as_bytes()).as_slice());
        corpo
    }

    /// Lê um manifesto do corpo de uma moldura.
    ///
    /// Fechado por construção, e o mesmo caminho de erro do resto do
    /// módulo: o corpo tem de ter pelo menos os 48 bytes fixos, e
    /// qualquer coisa a menos é um corpo de outra moldura.
    pub fn de_corpo(corpo: &[u8]) -> Result<Manifesto, ErroMedia> {
        if corpo.len() < TAM_MANIFESTO_FIXO {
            return Err(ErroMedia::ManifestoCurto(corpo.len()));
        }

        let tipo = Tipo::de_caractere(
            *corpo
                .first()
                .ok_or(ErroMedia::ManifestoCurto(corpo.len()))?,
        )?;

        let total_bytes = campo_u32(corpo, 1)?;
        let total_pedacos = campo_u32(corpo, 5)?;
        let bytes_por_pedaco = campo_u16(corpo, 9)?;
        let codec = Codec::de_valor(campo_u16(corpo, 11)?)?;
        let formato = Formato::de_valor(campo_u16(corpo, 13)?)?;
        let duracao_s = campo_u32(corpo, 15)?;

        let mut sha256 = [0u8; 32];
        sha256.copy_from_slice(
            corpo
                .get(19..51)
                .ok_or(ErroMedia::ManifestoCurto(corpo.len()))?,
        );

        let nome =
            base64::descodificar(corpo.get(51..).unwrap_or_default()).map_err(ErroMedia::Base64)?;

        // O nome é UTF-8 e **sem bytes nulos**. O primeiro porque a
        // moldura é texto e o nome vai para uma UI; o segundo porque um
        // `String` em Rust com um zero no meio é um `CString` truncado
        // à espera de acontecer, e passa a um `snprintf` como
        // separador de argumentos.
        let nome = String::from_utf8(nome).map_err(|_| ErroMedia::NomeInvalido)?;
        if nome.contains('\0') {
            return Err(ErroMedia::NomeInvalido);
        }

        // O `tipo` vem do manifesto, e **não** do `codec`: um vídeo
        // Matroska com Opus dentro tem `Codec::Opus`, que daria
        // `Tipo::Audio` — e a chave errada. Ver `docs/media.md` §2.5.
        let m = Manifesto {
            tipo,
            codec,
            formato,
            total_bytes,
            total_pedacos,
            bytes_por_pedaco,
            duracao_s,
            sha256,
            nome,
        };

        // A coerência do manifesto é validada aqui e não na escrita: um
        // manifesto que diz 100 fatias de 48 KB para 10 bytes de
        // conteúdo é um manifesto de mentira, e aceitá-lo levaria a
        // uma montagem que devolve ficheiros diferentes a cada
        // tentativa.
        m.validar()?;
        Ok(m)
    }

    /// Confirma que os números do manifesto são coerentes.
    ///
    /// Três condições, e cada uma apanha um erro diferente que era
    /// silencioso:
    ///
    /// * `total_pedacos` tem de ser o número que `total_bytes` e
    ///   `bytes_por_pedaco` implicam — senão a montagem acaba num
    ///   ficheiro de tamanho diferente do declarado.
    /// * A última fatia pode ser mais pequena; as outras não. Se todas
    ///   forem do mesmo tamanho, o ficheiro é um múltiplo exacto, e
    ///   isso é válido.
    /// * `bytes_por_pedaco` não pode ser zero: uma divisão por zero
    ///   entra em pânico, e um manifesto chega de outro lado.
    pub fn validar(&self) -> Result<(), ErroMedia> {
        if self.bytes_por_pedaco == 0 {
            return Err(ErroMedia::ManifestoIncoerente("bytes por pedaço é zero"));
        }
        // Zero fatias só com um ficheiro vazio, e vice-versa. São a
        // mesma condição vista de dois lados, e escrevê-la uma vez é o
        // que impede que um lado mude e o outro não.
        if (self.total_pedacos == 0) != (self.total_bytes == 0) {
            return Err(ErroMedia::ManifestoIncoerente(
                "zero fatias com conteúdo, ou conteúdo sem fatias",
            ));
        }

        let esperado = numero_de_fatias(self.total_bytes as usize);
        if esperado != self.total_pedacos {
            return Err(ErroMedia::ManifestoIncoerente(
                "as fatias não batem com o tamanho",
            ));
        }

        // O tamanho por fatia tem de caber no que a moldura aceita. Se
        // o `TAM_MEDIA_MAX` mudar e o manifesto ficar antigo, isto
        // apanha — e o sintoma sem a verificação seria uma
        // `0x09 PayloadGrandeDemais` a meio de uma transferência.
        if usize::from(self.bytes_por_pedaco) > TAM_MEDIA_MAX {
            return Err(ErroMedia::ManifestoIncoerente(
                "a fatia não cabe num envelope",
            ));
        }

        Ok(())
    }

    /// O `id` desta transferência.
    pub fn id(&self) -> IdTransferencia {
        id_do_resumo(&self.sha256)
    }
}

/// Quantas fatias um conteúdo de `bytes` ocupa, em [`TAM_FATIA`].
pub fn numero_de_fatias(bytes: usize) -> u32 {
    if bytes == 0 {
        // **Zero**, e a primeira versão devolvia 1.
        //
        // O motivo de ser um bug: `numero_de_fatias(0)` dizia 1 e
        // `fatiar(&[], …)` dava 0, porque `chunks()` sobre um slice
        // vazio não produz nenhum. Um manifesto a dizer «1 fatia» que
        // o emissor nunca produz fechava a transferência no receptor
        // com uma fatia em falta — para sempre.
        //
        // Zero é a resposta coerente e simplifica o caso: um ficheiro
        // vazio é **só** o manifesto, e o receptor monta de imediato.
        return 0;
    }
    // Divisão com arredondamento para cima, feita **sem** `+ TAM_FATIA -
    // 1` para não transbordar num `usize` cheio. Um ficheiro de 4 GiB
    // num `usize` de 32 bits transbordava a soma — e o `u32` do
    // manifesto é, precisamente, o limite do tamanho.
    let n = bytes / TAM_FATIA;
    let resto = bytes % TAM_FATIA;
    u32::try_from(n + usize::from(resto > 0)).unwrap_or(u32::MAX)
}

/// Divide `conteudo` em fatias de `tam` bytes.
///
/// Devolve fatias **emprestado**, não copiadas: um vídeo de 100 MB
/// fraccionado sem copiar mantém o pico num único exemplar do
/// conteúdo.
pub fn fatiar<'a>(conteudo: &'a [u8], tam: usize) -> impl Iterator<Item = &'a [u8]> {
    conteudo.chunks(tam.max(1))
}

/// O SHA-256 de um conteúdo.
pub fn resumo(conteudo: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(conteudo);
    h.finalize().into()
}

/// Um ficheiro montado, ainda por verificar.
///
/// O tipo existe para que a verificação do resumo **não seja
/// opcional**: quem tem um `Montado` já passou pelo resumo, e quem
/// tem uns `Vec<u8>` ainda não. Um `Vec<u8>` que qualquer lado pode
/// construir é um conteúdo em que ninguém confia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Montado {
    bytes: Vec<u8>,
}

impl Montado {
    /// O conteúdo.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// O tamanho, em bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// O conteúdo está vazio?
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Escreve o conteúdo num `memfd` e devolve o descritor.
    ///
    /// **Nada toca o disco.** Ver `docs/media.md` §7.4 para a
    /// comparação com um ficheiro temporário, que sobrevive ao
    /// `unlink` até o último descritor fechar e é visível a outro
    /// utilizador da máquina.
    ///
    /// O descritor sai com `FD_CLOEXEC` **limpo**, para que o leitor de
    /// media o possa abrir por `/proc/self/fd/N` depois do `execve`. É
    /// o mesmo contrato que [`super::ffmpeg_embutido::preparar`], e a
    /// diferença é que aqui o conteúdo é do utilizador e não é
    /// executável — logo não há `F_SEAL_SEAL`, e não deve haver: um
    /// leitor que queira escrever no seu próprio ficheiro de entrada é
    /// um caso em que não vale a pena selar.
    pub fn em_memfd(&self) -> Result<i32, ErroMedia> {
        let nome = std::ffi::CString::new("onyx-media")
            .map_err(|_| ErroMedia::ExecucaoFalhou(libc::EINVAL))?;
        let fd = unsafe { libc::memfd_create(nome.as_ptr(), 0) };
        if fd < 0 {
            return Err(ErroMedia::ExecucaoFalhou(ultimo_erro()));
        }

        let escrito = unsafe { libc::write(fd, self.bytes.as_ptr().cast(), self.bytes.len()) };
        if escrito < 0 || escrito as usize != self.bytes.len() {
            let errno = ultimo_erro();
            unsafe { libc::close(fd) };
            return Err(ErroMedia::ExecucaoFalhou(errno));
        }

        // `0400`: só leitura. Um media player que tente escrever no
        // ficheiro de entrada está a fazer algo que não precisa, e
        // fechar-lhe a porta cedo dá um erro claro em vez de um
        // ficheiro alterado a meio.
        if unsafe { libc::fchmod(fd, 0o400) } < 0 {
            let errno = ultimo_erro();
            unsafe { libc::close(fd) };
            return Err(ErroMedia::ExecucaoFalhou(errno));
        }

        Ok(fd)
    }
}

/// Junta as fatias recebidas e confirma o resumo.
///
/// ## Porque o resumo é a única condição de aceitação
///
/// Porque o relay é *entrega única, sem duplicados*
/// (`docs/relay.md` §Regras de retenção) e **não há
/// retransmissão**: uma fatia perdida é uma fatia perdida, e o
/// receptor precisa de saber se a montagem está completa **antes** de
/// a mostrar. Um leitor que toca um ficheiro truncado não sabe — e o
/// utilizador vê um vídeo que acaba a meio e não tem porquê.
///
/// ## Porquê aceitar as fatias fora de ordem
///
/// Porque o relay pode reordenar e porque o `seq` de cada uma é o que
/// ordena. [`Montador::receber`] recebe pelo `seq` e `montar` usa a
/// posição — a ordem de chegada não entra em nenhum dos dois.
/// o [`Montador`] — que guarda o `seq`.
#[derive(Debug)]
pub struct Montador {
    total_pedacos: usize,
    bytes_por_pedaco: usize,
    resumo_esperado: [u8; 32],
    /// As fatias, por índice. `None` é «ainda não chegou».
    fatias: Vec<Option<Vec<u8>>>,
}

impl Montador {
    /// Abre um montador a partir de um manifesto.
    pub fn novo(manifesto: &Manifesto) -> Result<Montador, ErroMedia> {
        manifesto.validar()?;
        let total = manifesto.total_pedacos as usize;
        Ok(Montador {
            total_pedacos: total,
            bytes_por_pedaco: manifesto.bytes_por_pedaco as usize,
            resumo_esperado: manifesto.sha256,
            fatias: (0..total).map(|_| None).collect(),
        })
    }

    /// Quantas fatias faltam.
    pub fn faltam(&self) -> usize {
        self.fatias.iter().filter(|f| f.is_none()).count()
    }

    /// Os índices das fatias que faltam, por ordem.
    ///
    /// É o que a resposta de retoma lista: *«9f3a: faltam 7 42 118»*.
    pub fn indices_em_falta(&self) -> Vec<u32> {
        self.fatias
            .iter()
            .enumerate()
            .filter(|(_, f)| f.is_none())
            .map(|(i, _)| u32::try_from(i).unwrap_or(u32::MAX))
            .collect()
    }

    /// Recebe uma fatia pelo seu `seq`.
    ///
    /// `seq` e não o índice, porque é o `seq` que viaja no manifesto de
    /// cada fatia e é o que o receptor tem. Um índice calculado a partir
    /// da ordem de chegada seria a mesma fatia contada duas vezes se o
    /// relay repetisse, e nenhuma se reordenasse.
    pub fn receber(&mut self, seq: u32, dados: &[u8]) -> Result<(), ErroMedia> {
        // O `seq` que chega pela rede conta o manifesto como 0, para que
        // a sequência do fluxo seja a mesma para as duas peças e não
        // haja dois 0. A fatia de índice 0 no manifesto é, portanto, a
        // `seq` 1 — e é aqui que a diferença entre «posição no
        // ficheiro» e «posição na rede» se resolve.
        let i = match seq.checked_sub(1) {
            Some(j) => j as usize,
            None => return Err(ErroMedia::PedacoForaDeIntervalo(seq)),
        };
        if i >= self.total_pedacos {
            return Err(ErroMedia::PedacoForaDeIntervalo(seq));
        }
        if dados.len() > self.bytes_por_pedaco {
            return Err(ErroMedia::PedacoGrandeDemais(dados.len()));
        }
        // Duplicada: fica a que chegou. Tentar juntar as duas daria um
        // ficheiro mais comprido do que o `sha256` diz, e o receptor
        // acabaria com um erro de resumo em vez de ignorar um
        // reenvio. A segunda leitura é a mais tolerante e a correcta.
        self.fatias[i] = Some(dados.to_vec());
        Ok(())
    }

    /// Junta e verifica, se todas as fatias tiverem chegado.
    ///
    /// Devolve `None` enquanto faltar alguma — que é o caso comum de
    /// uma transferência a meio, e não é um erro.
    pub fn montar(&self) -> Result<Option<Montado>, ErroMedia> {
        if self.faltam() > 0 {
            return Ok(None);
        }

        let mut bytes = Vec::with_capacity(self.total_pedacos * self.bytes_por_pedaco);
        for fatia in &self.fatias {
            // `faltam()` já disse que não há `None` aqui. O `.expect`
            // é a sério inalcançável e um `unwrap` daria um pânico
            // num caminho de rede.
            bytes.extend_from_slice(
                fatia
                    .as_ref()
                    .expect("verificado por `faltam` imediatamente acima"),
            );
        }

        if resumo(&bytes) != self.resumo_esperado {
            return Err(ErroMedia::ResumoNaoConfere);
        }

        Ok(Some(Montado { bytes }))
    }
}

/// Lê um `u32` big-endian a partir de `at`.
///
/// Existe para as seis leituras de [`Manifesto::de_corpo`] não serem
/// seis linhas de `try_into(...).map_err(|_| ...)`. A versão repetida
/// tinha vinte e quatro caracteres de `map_err` por campo, e um erro
/// de `+1` num deles não dava sintaxe inválida — dava o valor errado.
fn campo_u32(corpo: &[u8], at: usize) -> Result<u32, ErroMedia> {
    corpo
        .get(at..at + 4)
        .and_then(|p| <[u8; 4]>::try_from(p).ok())
        .map(u32::from_be_bytes)
        .ok_or(ErroMedia::ManifestoCurto(corpo.len()))
}

/// Lê um `u16` big-endian a partir de `at`.
fn campo_u16(corpo: &[u8], at: usize) -> Result<u16, ErroMedia> {
    corpo
        .get(at..at + 2)
        .and_then(|p| <[u8; 2]>::try_from(p).ok())
        .map(u16::from_be_bytes)
        .ok_or(ErroMedia::ManifestoCurto(corpo.len()))
}

fn ultimo_erro() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

/// Lê um `Read` até ao fim, com um tecto.
///
/// Existe porque `ler` devolve um `Result<usize>` por chamada e
/// acumular em RAM sem limite é o que transforma um ficheiro
/// desconhecido num `OOM kill`. O tecto é [`TAM_TOTAL`] vezes 1024 —
/// 4 GiB, o mesmo limite do `u32` do manifesto, e pelo mesmo motivo: é
/// o maior ficheiro que o formato descreve.
pub fn ler_com_tecto<R: Read>(mut leitor: R, tecto: usize) -> Result<Vec<u8>, ErroMedia> {
    let mut bytes = Vec::new();
    let mut pedaco = vec![0u8; 64 * 1024];

    loop {
        match leitor.read(&mut pedaco) {
            Ok(0) => break,
            Ok(n) => {
                if bytes.len() + n > tecto {
                    return Err(ErroMedia::FicheiroGrandeDemais);
                }
                bytes.extend_from_slice(&pedaco[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErroMedia::LeituraFalhou),
        }
    }
    Ok(bytes)
}

/// Monta o texto de uma fatia, pronto para o `ENCODE`.
pub fn moldura_de_pedaco(
    manifesto: &Manifesto,
    stream: u16,
    seq: u32,
    pedaco: &[u8],
) -> super::moldura::Moldura {
    construir_com_flags(
        manifesto.tipo,
        Modo::NaoStreaming,
        Flags::VAZIO,
        stream,
        seq,
        0,
        pedaco,
    )
}

/// Monta o texto do manifesto, que leva a flag.
pub fn moldura_de_manifesto(
    manifesto: &Manifesto,
    stream: u16,
    seq: u32,
) -> super::moldura::Moldura {
    let corpo = manifesto.corpo();
    construir_com_flags(
        manifesto.tipo,
        Modo::NaoStreaming,
        Flags::VAZIO.com(Flags::MANIFESTO),
        stream,
        seq,
        0,
        &corpo,
    )
}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod testes {
    use super::*;
    use crate::media::moldura;

    /// Um conteúdo de teste que não é uma repetição — um delta de
    /// bytes constantes faria o SHA-256 não testar nada.
    fn conteudo(n: usize) -> Vec<u8> {
        (0..n)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(7))
            .collect()
    }

    /// Um manifesto coerente para um conteúdo de `n` bytes.
    ///
    /// O conteúdo é derivado de `n` **dentro** da função, e não
    /// passado à parte: um manifesto diz `total_bytes` e a função
    /// `fatias` compara o conteúdo com ele, e contentê-los em separado
    /// era o que fazia o teste de `n = 0` usar um manifesto de 1 byte
    /// com um conteúdo de 0.
    fn manifesto_e_conteudo(n: usize) -> (Manifesto, Vec<u8>) {
        let dados = conteudo(n);
        let m = Manifesto::novo(
            Tipo::Video,
            Codec::Matroska,
            Formato::Matroska,
            12,
            "exemplo.mkv",
            &dados,
        )
        .expect("manifesto");
        (m, dados)
    }

    /// Só o manifesto, quando o conteúdo não interessa ao teste.
    fn manifesto_de_teste(n: usize) -> Manifesto {
        manifesto_e_conteudo(n).0
    }

    /// O número de fatias é o que a divisão com resto implica.
    ///
    /// Os três casos que importam: exacto, resto, e zero. O zero é o
    /// que uma implementação que devolve `ceil(n/tam)` sem o `max(1)`
    /// falha — e um ficheiro vazio **tem** de ter uma fatia, para que
    /// a transferência se complete e não fique pendurada para sempre.
    #[test]
    fn o_numero_de_fatias_cobre_o_exacto_o_resto_e_o_zero() {
        assert_eq!(numero_de_fatias(TAM_FATIA), 1, "exactamente uma fatia");
        assert_eq!(numero_de_fatias(TAM_FATIA + 1), 2, "sobra um byte");
        assert_eq!(numero_de_fatias(2 * TAM_FATIA), 2, "exactamente duas");
        assert_eq!(numero_de_fatias(0), 0, "um ficheiro vazio não tem fatias");
    }

    /// Uma fracção tem `n` fatias que reassem o original.
    ///
    /// A propriedade que substitui todos os vectores: concatenar as
    /// fatias tem de dar **exactamente** o conteúdo. Um `off-by-one` no
    /// tamanho da última fatia é o erro mais discreto que há — produz
    /// um ficheiro que se abre e falta-lhe um byte no fim.
    #[test]
    fn fraccionar_e_reassemblar_da_o_original() {
        for n in [
            0usize,
            1,
            TAM_FATIA - 1,
            TAM_FATIA,
            TAM_FATIA + 1,
            3 * TAM_FATIA + 7,
        ] {
            let (m, original) = manifesto_e_conteudo(n);
            let partes = m.fatias(&original).expect("as fatias");
            assert_eq!(
                partes.len(),
                m.total_pedacos as usize,
                "{n} bytes: número de fatias errado"
            );
            assert!(
                partes.iter().all(|p| p.len() <= TAM_FATIA),
                "{n} bytes: uma fatia excede o limite"
            );
            // Todas menos a última têm o tamanho cheio.
            for (i, p) in partes
                .iter()
                .enumerate()
                .take(partes.len().saturating_sub(1))
            {
                assert_eq!(
                    p.len(),
                    TAM_FATIA,
                    "a fatia {i} de {n} bytes não está cheia"
                );
            }

            let juntado: Vec<u8> = partes.concat();
            if n == 0 {
                assert!(juntado.is_empty());
            } else {
                assert_eq!(
                    juntado, original,
                    "{n} bytes não sobreviveram à ida e volta"
                );
            }
        }
    }

    /// O manifesto faz a ida e volta, campos e resumo.
    #[test]
    fn o_manifesto_faz_a_ida_e_volta() {
        let original = manifesto_de_teste(TAM_FATIA / 2);
        let lido = Manifesto::de_corpo(&original.corpo()).expect("o manifesto abre");

        assert_eq!(lido.total_bytes, original.total_bytes);
        assert_eq!(lido.total_pedacos, original.total_pedacos);
        assert_eq!(lido.bytes_por_pedaco, original.bytes_por_pedaco);
        assert_eq!(lido.codec, original.codec);
        assert_eq!(lido.formato, original.formato);
        assert_eq!(lido.duracao_s, original.duracao_s);
        assert_eq!(lido.sha256, original.sha256, "o resumo não sobreviveu");
        assert_eq!(lido.nome, original.nome);
    }

    /// O `id` sai dos primeiros 16 bytes do resumo.
    ///
    /// Testado porque um `id` que mudasse com o nome daria duas
    /// transferências do mesmo ficheiro com `id`s diferentes — e o
    /// receptor veria «faltam 0» para um ficheiro que já tinha.
    #[test]
    fn o_id_vem_do_resumo_e_nao_do_nome() {
        let a = manifesto_de_teste(1000);
        let b = Manifesto::novo(
            Tipo::Video,
            Codec::Matroska,
            Formato::Matroska,
            12,
            "outro-nome.mkv",
            &conteudo(1000),
        )
        .expect("manifesto");

        assert_eq!(a.id(), b.id(), "o mesmo conteúdo deu `id` diferente");
        assert_eq!(a.id()[..], a.sha256[..16]);
        assert_eq!(id_em_hex(&a.id()).len(), 32);
    }

    /// Um conteúdo diferente dá um `id` diferente.
    #[test]
    fn conteudos_diferentes_da_ids_diferentes() {
        assert_ne!(
            manifesto_de_teste(1000).id(),
            manifesto_de_teste(1001).id(),
            "conteúdos diferentes com o mesmo `id`"
        );
    }

    /// O resumo bate com o de uma implementação de fora.
    ///
    /// `sha256sum` é a referência, e um resumo que não bate com o
    /// comando do sistema não é um resumo — é uma função de hash com
    /// o nome certo.
    #[test]
    fn o_resumo_bate_com_o_sha256_do_sistema() {
        let dados = b"abc";
        let esperado = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];
        assert_eq!(resumo(dados), esperado, "SHA-256 de «abc»");
    }

    /// A montagem só fecha com o resumo a bater.
    ///
    /// O caso que importa é o do meio: uma fatia **trocada** por outra
    /// do mesmo tamanho passa todas as verificações de tamanho e falha
    /// só no resumo. Sem ele, o receptor tocaria um ficheiro que não é o
    /// que foi enviado.
    #[test]
    fn a_fatia_trocada_e_apanhada_pelo_resumo() {
        let original = conteudo(TAM_FATIA + 100);
        let m = manifesto_de_teste(original.len());
        let mut montador = Montador::novo(&m).expect("montador");

        let partes = m.fatias(&original).expect("as fatias");
        montador.receber(1, partes[0]).expect("fatia 1");
        montador.receber(2, partes[1]).expect("fatia 2");

        assert!(
            montador.montar().is_ok(),
            "com o conteúdo certo tem de montar"
        );

        // Agora com a segunda fatia trocada.
        let mut montador = Montador::novo(&m).expect("montador");
        montador.receber(1, partes[0]).expect("fatia 0");
        montador.receber(2, &conteudo(TAM_FATIA)).expect("fatia 1");
        assert_eq!(
            montador.montar(),
            Err(ErroMedia::ResumoNaoConfere),
            "uma fatia trocada passou"
        );
    }

    /// As fatias podem chegar fora de ordem, e a montagem não sabe.
    ///
    /// É o que o `seq` resolve e o que a reordenação exercita: quem
    /// chama entrega pelo `seq`, e a montagem usa a ordem do `seq` e
    /// não a ordem de chegada.
    #[test]
    fn as_fatias_podem_chegar_desordenadas() {
        let original = conteudo(3 * TAM_FATIA + 5);
        let m = manifesto_de_teste(original.len());
        let partes = m.fatias(&original).expect("as fatias");
        let mut montador = Montador::novo(&m).expect("montador");

        // `3 * TAM_FATIA + 5` dá **quatro** fatias, não três: as três
        // primeiras estão cheias e a quarta tem os 5 bytes que sobram.
        // A primeira versão deste teste entregava três e ficava à
        // espera de uma quarta que não chegava — o que é exactamente
        // o estado que «a meio da transferência» devia descrever, e
        // que outro teste já descreve.
        montador.receber(4, partes[3]).expect("4.ª");
        montador.receber(3, partes[2]).expect("3.ª");
        montador.receber(1, partes[0]).expect("1.ª");
        assert_eq!(montador.faltam(), 1, "das quatro, entregaram-se três");
        montador.receber(2, partes[1]).expect("2.ª");
        assert_eq!(montador.faltam(), 0);

        let montado = montador.montar().expect("monta").expect("agora sim");
        assert_eq!(
            montado.bytes(),
            partes.concat(),
            "a montagem seguiu a ordem de chegada, não o seq"
        );
    }

    /// Uma transferência a meio dá `None`, e não um erro.
    #[test]
    fn a_meio_da_transferencia_da_none_e_nao_erro() {
        let m = manifesto_de_teste(4 * TAM_FATIA);
        let mut montador = Montador::novo(&m).expect("montador");

        assert_eq!(montador.montar().expect("sem erro"), None, "não havia nada");
        montador.receber(1, &[0u8; TAM_FATIA]).expect("0");
        assert_eq!(montador.faltam(), 3);
        assert_eq!(montador.montar().expect("sem erro"), None);

        montador.receber(2, &[0u8; TAM_FATIA]).expect("1");
        montador.receber(3, &[0u8; TAM_FATIA]).expect("2");
        montador.receber(4, &[0u8; TAM_FATIA]).expect("3");
        assert_eq!(montador.faltam(), 0);
    }

    /// Os índices em falta saem por ordem, para a resposta de retoma.
    #[test]
    fn os_indices_em_falta_saem_por_ordem() {
        let m = manifesto_de_teste(5 * TAM_FATIA);
        let mut montador = Montador::novo(&m).expect("montador");

        montador.receber(1, &[7u8; TAM_FATIA]).expect("0");
        montador.receber(3, &[7u8; TAM_FATIA]).expect("2");
        montador.receber(5, &[7u8; TAM_FATIA]).expect("4");

        assert_eq!(montador.indices_em_falta(), vec![1, 3]);
    }

    /// O manifesto é o `seq` 0, e as fatias começam no 1.
    ///
    /// A sequência do fluxo tem de ser a mesma para as duas peças. Se
    /// as fatias começassem no 0, o manifesto e a primeira fatia
    /// teriam o mesmo `seq` — e um receptor que ordenasse por ele não
    /// saberia qual dos dois veio primeiro.
    #[test]
    fn o_zero_e_do_manifesto() {
        let original = conteudo(100);
        let m = manifesto_de_teste(original.len());
        let mut montador = Montador::novo(&m).expect("montador");

        assert_eq!(
            montador.receber(0, &original),
            Err(ErroMedia::PedacoForaDeIntervalo(0)),
            "o seq 0 é do manifesto, e uma fatia não o pode reclamar"
        );

        let partes = m.fatias(&original).expect("as fatias");
        montador.receber(1, &partes[0]).expect("fatia 1");
        assert!(montador.montar().is_ok());
    }

    /// Uma fatia fora do intervalo é recusada, não ignorada.
    #[test]
    fn um_pedaco_fora_de_intervalo_e_recusado() {
        let m = manifesto_de_teste(2 * TAM_FATIA);
        let mut montador = Montador::novo(&m).expect("montador");
        assert_eq!(
            montador.receber(3, &[0u8; 10]),
            Err(ErroMedia::PedacoForaDeIntervalo(3)),
            "um seq além do último não pode ser aceite"
        );
    }

    /// Uma fatia maior do que o declarado é recusada.
    ///
    /// Sem esta verificação, uma fatia gigante faz o `Vec` crescer sem
    /// tecto — que é o que o orçamento de `orcamento.rs` existe para
    /// impedir, mas por um caminho que o orçamento não vê.
    #[test]
    fn um_pedaco_grande_demais_e_recusado() {
        let m = manifesto_de_teste(TAM_FATIA);
        let mut montador = Montador::novo(&m).expect("montador");
        assert!(montador.receber(1, &vec![0u8; TAM_FATIA + 1]).is_err());
    }

    /// Um manifesto incoerente é recusado ao ser lido.
    ///
    /// Os três casos são incoerências que um emissor poderia enviar sem
    /// querer e que, sem verificação, passariam: um `bytes_por_pedaco`
    /// a zero (divisão por zero), fatias que não batem com o tamanho, e
    /// zero fatias.
    #[test]
    fn um_manifesto_incoerente_e_recusado() {
        let base = manifesto_de_teste(10 * TAM_FATIA).corpo();

        // `bytes_por_pedaco` está no offset 9, depois do novo byte do
        // `tipo`. Os offsets são os de `docs/media.md` §2.5 e estão
        // escritos à mão **de propósito**: um offset escrito à mão que
        // diverge da spec é um bug que só este teste apanha.
        let mut sem_tamanho = base.clone();
        sem_tamanho[9..11].copy_from_slice(&0u16.to_be_bytes());
        assert!(
            Manifesto::de_corpo(&sem_tamanho).is_err(),
            "bytes_por_pedaco a zero"
        );

        let mut sem_fatias = base.clone();
        sem_fatias[5..9].copy_from_slice(&99u32.to_be_bytes());
        assert!(
            Manifesto::de_corpo(&sem_fatias).is_err(),
            "fatias que não batem"
        );

        let mut sem_fatias = base.clone();
        sem_fatias[5..9].copy_from_slice(&0u32.to_be_bytes());
        assert!(Manifesto::de_corpo(&sem_fatias).is_err(), "zero fatias");

        assert!(
            Manifesto::de_corpo(&base).is_ok(),
            "o manifesto bom tem de passar"
        );
    }

    /// Um corpo curto demais é recusado, e sem entrar em pânico.
    #[test]
    fn um_manifesto_curto_e_recusado() {
        for n in 0..TAM_MANIFESTO_FIXO {
            assert!(
                Manifesto::de_corpo(&vec![0u8; n]).is_err(),
                "um corpo de {n} bytes devia ser recusado"
            );
        }
        // Exactamente os bytes fixos abre, **desde que seja um
        // manifesto coerente**. Um corpo de 51 bytes a zeros é recusado
        // — e é o certo: o byte 0 não é `A` nem `V`, e o `tipo` é o
        // primeiro campo (docs/media.md §2.5).
        let bom = manifesto_de_teste(TAM_FATIA).corpo();
        assert!(
            Manifesto::de_corpo(&bom[..TAM_MANIFESTO_FIXO]).is_ok(),
            "um manifesto sem nome tem de abrir"
        );
        // E o corpo a zeros, do mesmo tamanho, **não**: a diferença
        // entre os dois é o `tipo`, e é ela que o byte 0 apanha.
        assert!(
            Manifesto::de_corpo(&[0u8; TAM_MANIFESTO_FIXO]).is_err(),
            "um corpo a zeros não pode abrir — o byte 0 não é um tipo"
        );
    }

    /// Um nome com bytes nulos é recusado.
    #[test]
    fn um_nome_com_nulos_e_recusado() {
        let m = manifesto_de_teste(100);
        let mut corpo = m.corpo();
        // Troca o nome codificado por um que contenha um zero.
        let nome_com_nulo = base64::codificar_bytes(b"a\0b");
        corpo.truncate(TAM_MANIFESTO_FIXO);
        corpo.extend_from_slice(nome_com_nulo.as_slice());
        assert_eq!(Manifesto::de_corpo(&corpo), Err(ErroMedia::NomeInvalido));
    }

    /// O `Codec` diz o que se descodifica; o `Formato` diz o
    /// contentor. E os dois são independentes.
    #[test]
    fn codec_e_formato_sao_campos_independentes() {
        let m = Manifesto::novo(
            Tipo::Video,
            Codec::Opus,
            Formato::Matroska,
            5,
            "x.mkv",
            &conteudo(10),
        )
        .expect("manifesto");
        assert_eq!(m.codec, Codec::Opus, "o codec é o conteúdo");
        assert_eq!(m.formato, Formato::Matroska, "o formato é o contentor");
        // E o `tipo` **sobrevive**, porque é um campo do manifesto e
        // não uma dedução do `codec`. Sem ele, este `Video` chegaria
        // ao receptor como `Audio` e a chave seria a errada.
        let lido = Manifesto::de_corpo(&m.corpo()).expect("o manifesto abre");
        assert_eq!(lido.tipo, Tipo::Video, "o tipo não sobreviveu");
        assert_eq!(lido.codec, m.codec);
        assert_eq!(lido.formato, m.formato);
    }

    /// O Matroska não é válido em streaming, e a função diz.
    #[test]
    fn o_matroska_nao_e_valido_em_streaming() {
        assert!(!Codec::Matroska.valido_em_streaming());
        assert!(Codec::Opus.valido_em_streaming());
        assert!(Codec::RawVideoGray.valido_em_streaming());
    }

    /// Um codec ou formato desconhecido é recusado, não adivinhado.
    #[test]
    fn codec_e_formato_desconhecidos_sao_recusados() {
        assert!(Codec::de_valor(0x9999).is_err());
        assert!(Formato::de_valor(0x9999).is_err());
        assert!(Codec::de_valor(0x0001).is_ok());
    }

    /// O formato descobre-se pelo nome, e uma extensão desconhecida não
    /// é adivinhada.
    #[test]
    fn o_formato_descoobre_se_pelo_nome() {
        assert_eq!(Formato::de_nome("video.MKV"), Some(Formato::Matroska));
        assert_eq!(Formato::de_nome("som.opus"), Some(Formato::Ogg));
        assert_eq!(Formato::de_nome("som.MP3"), Some(Formato::Mp3));
        assert_eq!(Formato::de_nome("desconhecido.xyz"), None);
        assert_eq!(Formato::de_nome("sem-extensao"), None);
    }

    /// O texto de uma fatia cabe no que o `ENCODE` aceita.
    ///
    /// A propriedade que liga este módulo ao limite do
    /// `MAX_PLAINTEXT`: a maior fatia possível tem de produzir uma
    /// moldura dentro do orçamento, e a seguinte tem de não caber.
    #[test]
    fn a_maior_fatia_cabe_e_a_seguinte_nao() {
        let m = manifesto_de_teste(TAM_FATIA);

        let cheia = moldura_de_pedaco(&m, 1, 0, &vec![0u8; TAM_FATIA]);
        assert!(cheia.as_bytes().len() <= crate::envelope::MAX_PLAINTEXT);

        let excesso = moldura_de_pedaco(&m, 1, 0, &vec![0u8; TAM_FATIA + 1]);
        assert!(excesso.as_bytes().len() > crate::envelope::MAX_PLAINTEXT);
    }

    /// O manifesto também cabe — é menor, mas tem de caber.
    #[test]
    fn o_manifesto_cabe_num_envelope() {
        let m = manifesto_de_teste(1000);
        let moldura = moldura_de_manifesto(&m, 1, 0);
        assert!(moldura.as_bytes().len() <= crate::envelope::MAX_PLAINTEXT);

        // E a flag de manifesto está posta, que é o que distingue um
        // manifesto de uma fatia comum.
        let aberta = moldura::abrir(moldura.as_bytes()).expect("abre");
        assert!(aberta.flags.tem(moldura::Flags::MANIFESTO));
        assert_eq!(aberta.modo, Modo::NaoStreaming);
    }

    #[test]
    fn DEPURAR() {
        let m = manifesto_de_teste(TAM_FATIA);
        let corpo = m.corpo();
        eprintln!("TAM_FIXO={} corpo={}", TAM_MANIFESTO_FIXO, corpo.len());
        let so = &corpo[..TAM_MANIFESTO_FIXO];
        eprintln!("so={}", so.len());
        match Manifesto::de_corpo(so) {
            Ok(x) => eprintln!("OK {x:?}"),
            Err(e) => eprintln!("ERRO {e}"),
        }
    }

    /// Ler com tecto não deixa passar o que o excede.
    #[test]
    fn ler_com_tecto_recusa_o_que_excede() {
        let dados = conteudo(10_000);
        assert_eq!(
            ler_com_tecto(&dados[..], 10_000).expect("cabe").len(),
            10_000
        );
        assert_eq!(
            ler_com_tecto(&dados[..], 9_999),
            Err(ErroMedia::FicheiroGrandeDemais),
            "um byte a mais passou"
        );
    }
}
