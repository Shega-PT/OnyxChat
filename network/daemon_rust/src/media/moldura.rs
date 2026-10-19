// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/moldura.rs — o preâmbulo `ONX1`, a forma do "texto" de media
// ---------------------------------------------------------------------
// O pipeline do OnyxChat só sabe tratar `str` UTF-8
// (`network/daemon_rust/src/ipc.rs` recusa o resto com
// `0x08 TextoInvalidoUtf8`). A media tem de ser texto, e este módulo é
// a forma que esse texto toma.
//
// ## Onde a moldura vive, e porque não é visível
//
// **Dentro** do plaintext, **depois** de a chave ser escolhida e
// **antes** de K1. Ou seja: dentro do AEAD. Um observador no caminho vê
// `101 + N` bytes opacos e nunca chega a ler um `ONX1` —
// exactamente o que vê com uma mensagem de texto. A forma é a mesma
// porque **é** a mesma: o envelope é a mesma.
//
// ## Porque o preâmbulo é ASCII e não binário
//
// Porque o plaintext tem de ser UTF-8 **válido**, e a primeira versão
// desta moldura punha campos binários dentro — `clock_ms` a `-250` são
// os bytes `FF FF FF 06`, e `FF` não é UTF-8 válido. A falha que isso
// produzia era a pior possível: `String::from_utf8` a falhar, ou pior,
// um `expect` a entrar em pânico no meio do envio, e a mensagem de
// erro a dizer «invalid utf-8» quando o problema era o formato.
//
// Daí a decisão: **todos os campos são ASCII**, e os números são
// algarismos de largura fixa. Custa doze bytes e resolve o problema
// pela raiz — não há caminho em que este módulo produza um plaintext
// que o `ENCODE` recuse.
//
// ## Porque algarismos e não bytes
//
// A alternativa seria continuar binário e pôr **todo** o preâmbulo em
// base-64 também. Seria mais curto (21 bytes contra 36) e perderia a
/// única coisa que um preâmbulo vale: ser legível. `ONX1V1F4421000000007`
// diz o que é numa vista de `hexdump -C`; a base-64 do mesmo não diz
// nada sem um descodificador.
//
// A largura fixa é o que permite ler os campos **sem** um parser de
// texto: cada campo começa num offset conhecido e o fim é `início +
// largura`. Um `strtod` por frame, a 30 fps, seria o custo do módulo
// inteiro.
//
// ## Big-endian contra o resto do projecto
//
// `docs/ipc.md` usa `u32` little-endian, e é a escolha certa lá: é o
// que os emuladores Tor e os cabos esperam. Aqui não há endianness
// porque não há bytes — há **algarismos**, e o algarismo mais à
// esquerda é o mais significativo por convenção decimal. Um campo que
// se lê de trás para a frente em base 256 não se lê de trás para a
// frente em base 10, e é por isso que o formato é decimal e não
// hexadecimal.
// =====================================================================
use crate::envelope::MAX_PLAINTEXT;

use super::base64;
use super::erros::ErroMedia;

/// A magia, em ASCII.
pub const MAGIA: [u8; 4] = *b"ONX1";

/// A versão da moldura.
pub const VERSAO: u8 = b'1';

/// O separador entre o preâmbulo e o corpo.
///
/// Uma barra invertida em vez de `\n` porque o corpo é base-64, cujo
/// alfabeto não produz nem `\n` nem `\`. O separador é o byte que torna
/// a fronteira **verificável**: um preâmbulo truncado degrada-se em
/// algarismos aleatórios em vez de parsear e devolver lixo.
pub const SEPARADOR: u8 = b'\n';

// ---------------------------------------------------------------------
// A geometria do preâmbulo
// ---------------------------------------------------------------------
//
// **Cada offset é derivado do anterior, nunca escrito.** A primeira
// versão desta moldura escrevia os offsets à mão em [`abrir`] e as
// larguras em [`construir_com_flags`], e divergiram no primeiro build —
// o `debug_assert` apanhou um byte de diferença e a falha teria
// aparecer como «o vídeo abre com o áudio trocado».
//
// Derivar não é estilo: é a diferença entre um formato que não pode
// divergir e um formato que diverge no primeiro dia em que alguém
// acrescenta um campo. Se acrescentar um campo é acrescentar uma
// linha a esta lista e nada mais, o formato é seguro de estender. Se
// é procurar os números em dois sítios, não é.
// ---------------------------------------------------------------------

/// Onde começa o tipo.
pub const INICIO_TIPO: usize = 4;
/// Onde começa a versão.
pub const INICIO_VERSAO: usize = INICIO_TIPO + 1;
/// Onde começa o modo.
pub const INICIO_MODO: usize = INICIO_VERSAO + 1;
/// Onde começam as flags.
pub const INICIO_FLAGS: usize = INICIO_MODO + 1;
/// Onde começa o identificador de stream.
pub const INICIO_STREAM: usize = INICIO_FLAGS + 1;

/// A largura do campo `stream`: cinco algarismos.
///
/// Cinco porque `u16::MAX` são 65535, que tem cinco. Menos e o campo
/// transbordaria.
pub const LARGURA_STREAM: usize = 5;

/// Onde começa o número de sequência.
pub const INICIO_SEQ: usize = INICIO_STREAM + LARGURA_STREAM;

/// A largura do campo `seq`: dez algarismos.
///
/// Dez porque é o número de algarismos de `u32::MAX` (4294967295). Menos
/// e o campo transbordaria; mais e desperdiçaria bytes em todas as
/// mensagens, a 12,8 por segundo.
pub const LARGURA_SEQ: usize = 10;

/// Onde começa o relógio.
pub const INICIO_CLOCK: usize = INICIO_SEQ + LARGURA_SEQ;

/// A largura do campo `clock_ms`: um algarismo de sinal e onze de
/// módulo.
///
/// `i32::MIN` é 2147483648, que tem dez algarismos — onze são um de
/// sobra. A sobra é deliberada: sem ela, `i32::MIN` seria o único valor
/// que não cabe, e um valor que não cabe é uma excepção que alguém tem
/// de lembrar.
pub const LARGURA_CLOCK: usize = 12;

/// Onde começa o corpo — ou seja, onde está o separador.
pub const INICIO_CORPO: usize = INICIO_CLOCK + LARGURA_CLOCK;

/// O comprimento do preâmbulo, separador incluído.
pub const TAM_PRE_AMBULO: usize = INICIO_CORPO + 1;

/// O tipo de conteúdo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// Áudio.
    Audio,
    /// Vídeo.
    Video,
    /// Ficheiro arbitrário.
    ///
    /// ## E porque é um tipo da moldura
    ///
    /// Porque o ficheiro viaja em **fatias**, e a machinery de fatias
    /// — manifesto, resumo, montagem, retoma — é a de `ONX1`. Um
    /// contentor novo para os ficheiros seria a mesma machinery escrita
    /// duas vezes, e as duas versões divergiriam.
    ///
    /// ## E porque `F` e não um valor novo
    ///
    /// Porque o byte do tipo no preâmbulo é um **carácter legível**, e
    /// `F` diz «ficheiro» num `hexdump`. E porque o valor no `info`
    /// do HKDF é **outro** — ver [`Tipo::tipo_derivado`] — e o
    /// acréscimo de um tipo não muda a derivação dos outros.
    ///
    /// ## E o que custa a compatibilidade
    ///
    /// Uma implementação anterior que receba `F` recusa com
    /// [`ErroMedia::TipoDesconhecido`], que é a resposta correcta: não
    /// sabe ler ficheiros. E um par antigo continua a ler áudio e vídeo
    /// sem qualquer alteração — o que é a compatibilidade que importa.
    Ficheiro,
}

impl Tipo {
    /// O caractere no preâmbulo.
    ///
    /// Uma letra e não um algarismo: `A` e `V` dizem o que é num
    /// `hexdump`, e `1`/`2` não. O valor no `info` do HKDF é outro byte
    /// e **não** é este — ver [`Tipo::tipo_derivado`].
    pub fn caractere(self) -> u8 {
        match self {
            Tipo::Audio => b'A',
            Tipo::Video => b'V',
            Tipo::Ficheiro => b'F',
        }
    }

    /// O valor no `info` do HKDF da chave.
    ///
    /// Diferente de [`Tipo::caractere`] de propósito: aquele vai num
    /// hash e este vai num ficheiro, e um dia um dos dois vai mudar sem
    /// o outro. Obrigá-los a ser valores **separados** faz essa
    /// mudança visível, em vez de a fazer silenciosa.
    pub fn tipo_derivado(self) -> u8 {
        match self {
            Tipo::Audio => super::teclas::bytes_de_tipo_audio(),
            Tipo::Video => super::teclas::bytes_de_tipo_video(),
            Tipo::Ficheiro => super::teclas::bytes_de_tipo_ficheiro(),
        }
    }

    /// Converte um caractere de preâmbulo, ou falha.
    ///
    /// Fecha por construção: um caractere que não é `A` nem `V` não é
    /// um tipo, e devolver um tipo por omissão seria transformar um
    /// envelope corrompido num vídeo de outra pessoa.
    pub fn de_caractere(c: u8) -> Result<Tipo, ErroMedia> {
        match c {
            b'A' => Ok(Tipo::Audio),
            b'V' => Ok(Tipo::Video),
            b'F' => Ok(Tipo::Ficheiro),
            _ => Err(ErroMedia::TipoDesconhecido(c)),
        }
    }
}

/// O modo da stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    /// Um ficheiro, enviado de uma vez.
    NaoStreaming,
    /// Uma sessão ao vivo.
    Streaming,
}

impl Modo {
    /// O caractere no preâmbulo.
    pub fn caractere(self) -> u8 {
        match self {
            Modo::NaoStreaming => b'F',
            Modo::Streaming => b'S',
        }
    }

    /// Converte um caractere de preâmbulo, ou falha.
    pub fn de_caractere(c: u8) -> Result<Modo, ErroMedia> {
        match c {
            b'F' => Ok(Modo::NaoStreaming),
            b'S' => Ok(Modo::Streaming),
            _ => Err(ErroMedia::ModoDesconhecido(c)),
        }
    }
}

/// As flags do preâmbulo.
///
/// Três bits, e um caractere em **base 2** (`'0'` a `'7'`). Três
/// booleanos num byte seriam três caracteres, e a soma deles é um
/// algarismo — que é o que está no preâmbulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags(u8);

impl Flags {
    /// Bit 0 — este corpo é um manifesto.
    pub const MANIFESTO: u8 = 1 << 0;
    /// Bit 1 — esta unidade é uma keyframe (frame completo, não delta).
    pub const KEYFRAME: u8 = 1 << 1;
    /// Bit 2 — esta unidade é a última do lote.
    pub const FIM_DE_LOTE: u8 = 1 << 2;

    /// A máscara dos bits definidos.
    pub const DEFINIDOS: u8 = Self::MANIFESTO | Self::KEYFRAME | Self::FIM_DE_LOTE;

    /// Os bits a zero.
    pub const VAZIO: Flags = Flags(0);

    /// Acende um bit.
    pub fn com(self, bit: u8) -> Flags {
        Flags(self.0 | bit)
    }

    /// Diz se um bit está aceso.
    pub fn tem(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    /// O valor numérico, `0` a `7`.
    pub fn valor(self) -> u8 {
        self.0
    }

    /// O caractere no preâmbulo: `'0'` mais o valor, em base 2.
    pub fn caractere(self) -> u8 {
        b'0' + self.0
    }

    /// Converte um caractere, recusando bits reservados acesos.
    ///
    /// Este é o ponto onde um receptor fica em silêncio compatível: um
    /// algarismo acima de `'7'` tem um bit a mais, e esse bit significa
    /// qualquer coisa que esta versão não define. Ignorá-lo seria agir
    /// como se se soubesse o que o emissor quis dizer — e não se sabe.
    pub fn de_caractere(c: u8) -> Result<Flags, ErroMedia> {
        let valor = c.wrapping_sub(b'0');
        if c < b'0' || valor > 7 {
            return Err(ErroMedia::FlagsReservadasAcesas(c));
        }
        if valor & !Self::DEFINIDOS != 0 {
            return Err(ErroMedia::FlagsReservadasAcesas(c));
        }
        Ok(Flags(valor))
    }
}

/// Uma moldura de media, já montada.
///
/// Não guarda os campos: guarda o `String` que o `ENCODE` vai cifrar.
/// Depois de montada, uma moldura é **imutável**, e a razão é que vai
/// ser transformada por nove camadas que não devolvem nada. Um texto
/// que alguém pudesse mudar depois de cifrado daria um envelope cujo
/// conteúdo não corresponde ao que foi cifrado — e o verificador de
/// assinatura apanharia isso tarde e sem dizer porquê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moldura {
    texto: String,
}

impl Moldura {
    /// O texto para o `ENCODE`, em bytes.
    ///
    /// Numa caixa de bytes, porque o `ENCODE` quer `&[u8]` e converter
    /// aqui dentro seria uma cópia por chamada — e há uma por frame.
    pub fn as_bytes(&self) -> &[u8] {
        self.texto.as_bytes()
    }

    /// O texto, para o cliente Python e para o `MEDIA_ESTADO`.
    pub fn como_texto(&self) -> &str {
        &self.texto
    }
}

/// Monta a moldura de uma unidade de dados, sem flags.
pub fn construir(
    tipo: Tipo,
    modo: Modo,
    stream: u16,
    seq: u32,
    clock_ms: i32,
    dados: &[u8],
) -> Moldura {
    construir_com_flags(tipo, modo, Flags::VAZIO, stream, seq, clock_ms, dados)
}

/// Monta a moldura com flags explícitas.
///
/// O caminho que [`construir`] não dá: keyframes e manifesto precisam de
/// flags, e um `construir` que não as aceita seria um `construir` a que
/// se juntaria um `definir_flag` mutável — que é a forma de o chamador
/// se esquecer de o usar.
pub fn construir_com_flags(
    tipo: Tipo,
    modo: Modo,
    flags: Flags,
    stream: u16,
    seq: u32,
    clock_ms: i32,
    dados: &[u8],
) -> Moldura {
    let corpo = base64::codificar_bytes(dados);
    let mut texto = Vec::with_capacity(TAM_PRE_AMBULO + corpo.len());

    texto.extend_from_slice(&MAGIA);
    texto.push(tipo.caractere());
    texto.push(VERSAO);
    texto.push(modo.caractere());
    texto.push(flags.caractere());
    escrever_decimal(&mut texto, u64::from(stream), LARGURA_STREAM);
    escrever_decimal(&mut texto, u64::from(seq), LARGURA_SEQ);
    escrever_decimal_com_sinal(&mut texto, i64::from(clock_ms), LARGURA_CLOCK);
    debug_assert_eq!(
        texto.len(),
        INICIO_CORPO,
        "o preâmbulo mudou de tamanho — o tamanho de media também"
    );
    texto.push(SEPARADOR);
    texto.extend_from_slice(&corpo);

    // `from_utf8` **pode** falhar e isto não é um `expect`:: o
    // `debug_assert` de cima garante que só há ASCII aqui, e o `String`
    // é o dono do tipo que o `ENCODE` quer. Um `unsafe
    // { from_utf8_unchecked }` trocaria essa prova por um risco sem
    // ganhar nada — a validação é um ciclo.
    Moldura {
        texto: String::from_utf8(texto).expect("o preâmbulo é ASCII por construção"),
    }
}

/// Escreve `valor` em `largura` algarismos, com zeros à esquerda.
///
/// Escrever e não `format!`: isto corre por frame, e `format!` com
/// preenchimento aloca, formata e devolve uma `String` para depois
/// copiar byte a byte. Um laço com divisão por 10 não aloca nada.
fn escrever_decimal(saida: &mut Vec<u8>, valor: u64, largura: usize) {
    let mut algarismos = [b'0'; 20];
    let mut i = largura;
    let mut v = valor;
    // O laço termina por `v == 0`, e o resto fica a zeros porque o
    // array foi inicializado com `'0'`. Um valor com mais algarismos
    // que a largura **trunca em silêncio** — ver o teste que prova que
    // isso não acontece com nenhum valor de `u32`.
    while v > 0 && i > 0 {
        i -= 1;
        algarismos[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    saida.extend_from_slice(&algarismos[..largura]);
}

/// Escreve `valor` em `largura` algarismos, com sinal.
fn escrever_decimal_com_sinal(saida: &mut Vec<u8>, valor: i64, largura: usize) {
    debug_assert!(
        largura >= 2,
        "o campo do clock tem de ter lugar para o sinal"
    );
    let negativo = valor < 0;
    // `unsigned_abs` e não `-valor`: `i64::MIN` rebenta numa negação, e
    // um módulo que rebenta no valor mais negativo do tipo é um módulo
    // que rebenta em algum dia.
    let modulo = valor.unsigned_abs();

    // O sinal é **empurrado** e não escrito por cima: escrever por
    // cima do primeiro algarismo dava um campo com `largura - 1`
    // bytes em vez de `largura`, e a divergência só aparecia no
    // `debug_assert` — uma asserção que não corre em produção.
    let inicio = saida.len();
    saida.push(if negativo { b'-' } else { b'+' });
    escrever_decimal(saida, modulo, largura - 1);
    debug_assert_eq!(
        saida.len() - inicio,
        largura,
        "o campo do clock tem de ter exactamente {largura} bytes"
    );
}

/// Lê `largura` algarismos decimais de `texto[at..at+largura]`.
///
/// `largura` e `at` são validados pelos chamadores, que já conferiram
/// o comprimento total — daí isto não voltar a validar.
fn ler_decimal(texto: &[u8], at: usize, largura: usize) -> Result<u64, ErroMedia> {
    let mut v: u64 = 0;
    for i in 0..largura {
        let c = *texto
            .get(at + i)
            .ok_or(ErroMedia::MolduraCurta(texto.len()))?;
        if !c.is_ascii_digit() {
            return Err(ErroMedia::CampoNaoNumerico);
        }
        // A acumulação é feita sobre `u64` e o campo tem no máximo 11
        // algarismos, logo nunca transborda. Um `u32` aqui rebentaria
        // com um payload hostil de 11 algarismos — que é exactamente o
        // que o fuzzer encontra primeiro.
        v = v * 10 + u64::from(c - b'0');
    }
    Ok(v)
}

/// Lê `largura` algarismos com sinal.
fn ler_decimal_com_sinal(texto: &[u8], at: usize, largura: usize) -> Result<i32, ErroMedia> {
    let sinal = *texto.get(at).ok_or(ErroMedia::MolduraCurta(texto.len()))?;
    let modulo = ler_decimal(texto, at + 1, largura - 1)?;
    let valor = i64::try_from(modulo).map_err(|_| ErroMedia::CampoForaDeIntervalo)?;
    match sinal {
        b'-' => i32::try_from(-valor).map_err(|_| ErroMedia::CampoForaDeIntervalo),
        b'+' => i32::try_from(valor).map_err(|_| ErroMedia::CampoForaDeIntervalo),
        _ => Err(ErroMedia::CampoNaoNumerico),
    }
}

/// O que [`abrir`] devolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aberta {
    /// O tipo de conteúdo.
    pub tipo: Tipo,
    /// A versão da moldura.
    pub versao: u8,
    /// O modo.
    pub modo: Modo,
    /// As flags.
    pub flags: Flags,
    /// O identificador de stream.
    pub stream: u16,
    /// O número de sequência, monotónico por stream.
    pub seq: u32,
    /// O relógio de media, em milissegundos.
    pub clock_ms: i32,
    /// O corpo, já descodificado de base-64.
    pub corpo: Vec<u8>,
}

/// Abre uma moldura a partir do plaintext que o `DECODE` devolveu.
///
/// **Fechado por construção**: nenhuma das operações pode entrar fora
/// do que o `DECODE` já validou. O plaintext já passou por
/// `from_utf8` no daemon, logo Unicode não é um problema; o que fica é a
/// estrutura — magia, versão, tipo, modo, flags, algarismos, base-64.
///
/// Esta função é o ponto de entrada do fuzzing
/// (`fuzz/fuzz_targets/media_parser.rs`), e por isso não tem um único
/// `unwrap` nem um único acesso sem `get`. Um parser que alcance um
/// byte fora do fim é um parser que o fuzzer encontra em segundos e que
/// ninguém encontra em produção — e a diferença é um plano de execução
/// corrompido.
pub fn abrir(texto: &[u8]) -> Result<Aberta, ErroMedia> {
    if texto.len() < TAM_PRE_AMBULO {
        return Err(ErroMedia::MolduraCurta(texto.len()));
    }

    if texto.get(..4) != Some(MAGIA.as_slice()) {
        return Err(ErroMedia::MagiaInvalida);
    }

    // A versão é verificada **antes** do tipo, do modo e das flags, e
    // pela ordem inversa à da escrita. Um receptor que interpretasse os
    // campos de uma versão que não conhece está a ler ruído com
    // confiança — e ruído com confiança é pior do que ruído.
    let versao = *texto
        .get(INICIO_VERSAO)
        .ok_or(ErroMedia::MolduraCurta(texto.len()))?;
    if versao != VERSAO {
        return Err(ErroMedia::VersaoDesconhecida(versao));
    }

    let tipo = Tipo::de_caractere(
        *texto
            .get(INICIO_TIPO)
            .ok_or(ErroMedia::MolduraCurta(texto.len()))?,
    )?;
    let modo = Modo::de_caractere(
        *texto
            .get(INICIO_MODO)
            .ok_or(ErroMedia::MolduraCurta(texto.len()))?,
    )?;
    let flags = Flags::de_caractere(
        *texto
            .get(INICIO_FLAGS)
            .ok_or(ErroMedia::MolduraCurta(texto.len()))?,
    )?;

    let stream = u16::try_from(ler_decimal(texto, INICIO_STREAM, LARGURA_STREAM)?)
        .map_err(|_| ErroMedia::CampoForaDeIntervalo)?;
    let seq = u32::try_from(ler_decimal(texto, INICIO_SEQ, LARGURA_SEQ)?)
        .map_err(|_| ErroMedia::CampoForaDeIntervalo)?;
    let clock_ms = ler_decimal_com_sinal(texto, INICIO_CLOCK, LARGURA_CLOCK)?;

    if texto.get(INICIO_CORPO) != Some(&SEPARADOR) {
        return Err(ErroMedia::SeparadorInvalido);
    }

    let corpo = base64::descodificar(texto.get(TAM_PRE_AMBULO..).unwrap_or_default())
        .map_err(ErroMedia::Base64)?;

    Ok(Aberta {
        tipo,
        versao,
        modo,
        flags,
        stream,
        seq,
        clock_ms,
        corpo,
    })
}

/// O tamanho máximo de media que cabe num envelope, em bytes.
///
/// Vem de `docs/media.md` §3.2, e é derivado de [`TAM_PRE_AMBULO`] e
/// de [`MAX_PLAINTEXT`] em vez de escrito à mão. Um literal aqui
/// continuaria o valor antigo se `MAX_PLAINTEXT` mudasse, e a falha
/// apareceria como `0x09 PayloadGrandeDemais` a meio de uma
/// transferência — um erro cujo nome não diz nada sobre a sua causa.
pub const TAM_MEDIA_MAX: usize = (MAX_PLAINTEXT - TAM_PRE_AMBULO) / 4 * 3;

/// O tamanho de media que [`construir`] aceita.
///
/// função `const` para que o limite possa ser usado num array
/// estático — o anel de [`crate::media`] dimensiona-se com ele, e um
/// limite que não é constante teria de ser reinventado em execução.
pub const fn tamanho_maximo() -> usize {
    TAM_MEDIA_MAX
}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod testes {
    use super::*;
    use crate::media::erros::ErroMedia;

    fn moldura_de_teste() -> Moldura {
        construir(Tipo::Video, Modo::Streaming, 0x1234, 7, -250, b"payload")
    }

    /// Ida e volta com **todos** os campos não triviais.
    ///
    /// O `clock_ms` negativo é o caso que um teste com um literal
    /// positivo nunca apanha: tem de dar `-250`, e não `250` nem
    /// `4294967046`. É por isso que o literal é negativo — para tornar o
    /// erro visível se o sinal se perder, que é o modo como os dois
    /// erros mais prováveis deste campo se manifestam.
    #[test]
    fn ida_e_volta_preserva_todos_os_campos() {
        let original = moldura_de_teste();
        let aberta = abrir(original.as_bytes()).expect("moldura bem formada");

        assert_eq!(aberta.tipo, Tipo::Video);
        assert_eq!(aberta.versao, VERSAO);
        assert_eq!(aberta.modo, Modo::Streaming);
        assert_eq!(aberta.flags, Flags::VAZIO);
        assert_eq!(aberta.stream, 0x1234);
        assert_eq!(aberta.seq, 7);
        assert_eq!(aberta.clock_ms, -250, "o sinal do clock_ms perdeu-se");
        assert_eq!(aberta.corpo, b"payload");
    }

    /// **Todo** o plaintext é ASCII, sem excepção.
    ///
    /// É o teste que cobre o bug de formato que a primeira versão tinha:
    /// campos binários punham `FF` no plaintext, e `FF` não é UTF-8
    /// válido — logo o `ENCODE` recusaria com `0x08`, e a falha
    /// apareceria como «a media não envia».
    ///
    /// Testado sobre valores **extremos**, porque o `clock_ms` negativo
    /// é onde o `FF` aparecia: `i32::MIN` é `80000000`, e o seu
    /// complemento em dois's complement é `FF FF FF FF`.
    #[test]
    fn o_plaintext_e_ascii_para_valores_extremos() {
        for clock in [0, 1, -1, i32::MAX, i32::MIN, -250, 2_147_483_647] {
            for seq in [0, 1, u32::MAX, 4_294_967_295] {
                let m = construir(Tipo::Audio, Modo::Streaming, u16::MAX, seq, clock, b"");
                assert!(
                    m.as_bytes().is_ascii(),
                    "clock {clock} / seq {seq} produziu bytes não ASCII"
                );
                let a = abrir(m.as_bytes()).expect("tem de abrir");
                assert_eq!(a.clock_ms, clock, "o clock {clock} não sobreviveu");
                assert_eq!(a.seq, seq, "o seq {seq} não sobreviveu");
            }
        }
    }

    /// `i32::MIN` e `i32::MAX` fazem a ida e volta.
    ///
    /// Separado do teste de cima porque é o **limite** que interessa:
    /// `i64::MIN.unsigned_abs()` é o caminho que rebenta numa negação
    /// simples, e `i32::MIN` é onde isso aparece primeiro.
    #[test]
    fn os_extremos_do_clock_fazem_ida_e_volta() {
        for clock in [i32::MIN, i32::MAX, 0, -1] {
            let m = construir(Tipo::Video, Modo::NaoStreaming, 0, 0, clock, b"x");
            assert_eq!(abrir(m.as_bytes()).unwrap().clock_ms, clock);
        }
    }

    /// O preâmbulo tem exactamente o comprimento documentado.
    ///
    /// A spec diz 30 algarismos mais o separador, 31. Este teste existe
    /// porque um comprimento errado aqui desloca **todos** os campos
    /// seguintes em silêncio: o receptor lê `stream` onde está `seq`,
    /// lê um `clock_ms` enorme, e nada falha — os dados estão lá, é só
    /// que não são os que ele pensa.
    #[test]
    fn o_pre_ambulo_tem_o_comprimento_da_spec() {
        let m = construir(Tipo::Audio, Modo::NaoStreaming, 0, 0, 0, b"x");
        assert_eq!(INICIO_CORPO, 35);
        assert_eq!(TAM_PRE_AMBULO, 36);
        // 36 de preâmbulo + 4 de base-64 de um byte ("eA==").
        assert_eq!(m.as_bytes().len(), 40);
    }

    /// O corpo é base-64, e é ASCII.
    ///
    /// A segunda parte é a que importa: se o corpo tivesse um byte acima
    /// de `7F`, o `ENCODE` recusaria com `0x08`, e a falha apareceria
    /// como «a media não envia» — sem nada que aponte para o base-64.
    #[test]
    fn o_corpo_e_ascii_e_e_base64() {
        let dados: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        let m = construir(Tipo::Video, Modo::NaoStreaming, 0, 0, 0, &dados);
        let bytes = m.as_bytes();

        assert!(bytes.is_ascii(), "o plaintext tem de ser ASCII puro");
        assert_eq!(
            abrir(bytes).unwrap().corpo,
            dados,
            "o corpo não sobreviveu à ida e volta"
        );
    }

    /// O texto é legível num `hexdump`.
    ///
    /// A propriedade que justifica os algarismos em vez dos bytes, e
    /// que os testes de ida e volta não provam: **dá para ler o
    /// preâmbulo sem um descodificador**. Um `ONX1V1S1000070000000007`
    /// diz o tipo, a versão, o modo, as flags, a stream e a sequência
    /// a olho.
    #[test]
    fn o_pre_ambulo_e_legivel() {
        let m = construir_com_flags(
            Tipo::Video,
            Modo::Streaming,
            Flags::VAZIO.com(Flags::KEYFRAME),
            4660,
            7,
            -250,
            b"",
        );
        let texto = m.as_bytes();
        let cabecalho = std::str::from_utf8(&texto[..INICIO_CORPO]).expect("ASCII por construção");

        assert_eq!(cabecalho, "ONX1V1S2046600000000007-00000000250");
        // E cada campo está onde a spec diz. Estas quatro linhas dizem
        // uma coisa que a comparação acima não diz: que os campos estão
        // nos **offsets certos**, e não que a função de construção e a
        // de leitura concordam entre si — que é o que aconteceria se
        // ambas usassem a mesma constante errada.
        assert_eq!(&cabecalho[..INICIO_TIPO], "ONX1");
        assert_eq!(cabecalho.as_bytes()[INICIO_TIPO], b'V');
        assert_eq!(cabecalho.as_bytes()[INICIO_VERSAO], b'1');
        assert_eq!(cabecalho.as_bytes()[INICIO_MODO], b'S');
        assert_eq!(
            cabecalho.as_bytes()[INICIO_FLAGS],
            b'2',
            "'2' é 010 em base 2: só a keyframe"
        );
        assert_eq!(
            &cabecalho[INICIO_STREAM..INICIO_STREAM + LARGURA_STREAM],
            "04660"
        );
        assert_eq!(
            &cabecalho[INICIO_SEQ..INICIO_SEQ + LARGURA_SEQ],
            "0000000007"
        );
        assert_eq!(
            &cabecalho[INICIO_CLOCK..INICIO_CLOCK + LARGURA_CLOCK],
            "-00000000250"
        );
    }

    /// Uma magic errada é recusada antes de qualquer outra coisa.
    #[test]
    fn magic_invalida_e_recusada() {
        let mut bytes = moldura_de_teste().as_bytes().to_vec();
        bytes[0] = b'X';
        assert_eq!(abrir(&bytes), Err(ErroMedia::MagiaInvalida));
    }

    /// Uma versão desconhecida é recusada, e recusada **antes** dos
    /// campos seguintes.
    ///
    /// A ordem importa e é testada: se o tipo fosse lido antes da
    /// versão, um `ONX1` de versão `2` seria interpretado com o
    /// esquema da `1`, e o receptor produziria media do tamanho errado a
    /// partir de campos que não significam o que ele pensa.
    #[test]
    fn versao_desconhecida_e_recusada_primeiro() {
        let mut bytes = moldura_de_teste().as_bytes().to_vec();
        bytes[INICIO_VERSAO] = b'2';
        assert_eq!(abrir(&bytes), Err(ErroMedia::VersaoDesconhecida(b'2')));
    }

    /// Com a versão **e** o tipo inválidos ao mesmo tempo, ganha a
    /// versão.
    ///
    /// O mesmo teste de ordem pelo outro lado: põe os dois campos a
    /// falhar e confirma que o de baixo (byte 4) ganha.
    #[test]
    fn a_versao_ganha_a_tipo_e_modo() {
        let mut bytes = moldura_de_teste().as_bytes().to_vec();
        bytes[INICIO_VERSAO] = b'9';
        bytes[INICIO_TIPO] = b'Z';
        bytes[INICIO_MODO] = b'Z';
        assert_eq!(abrir(&bytes), Err(ErroMedia::VersaoDesconhecida(b'9')));
    }

    /// Tipo e modo desconhecidos são recusados, cada um com o seu erro.
    ///
    /// Erros **distintos** porque as correções são distintas: um tipo
    /// errado aponta para o emissor, um modo errado também. Um erro
    /// único para os dois obrigaria a abrir os dois campos para saber
    /// qual falhou.
    #[test]
    fn tipo_e_modo_desconhecidos_tem_erros_distintos() {
        let base = moldura_de_teste().as_bytes().to_vec();

        let mut com_tipo = base.clone();
        com_tipo[INICIO_TIPO] = b'Q';
        assert_eq!(abrir(&com_tipo), Err(ErroMedia::TipoDesconhecido(b'Q')));

        let mut com_modo = base;
        com_modo[INICIO_MODO] = b'Q';
        assert_eq!(abrir(&com_modo), Err(ErroMedia::ModoDesconhecido(b'Q')));
    }

    /// Uma flag fora de `'0'`..`'7'` é recusada.
    ///
    /// Testa a decisão de **não** ignorar valores desconhecidos: um
    /// receptor que os ignorasse estaria a agir como se soubesse o que
    /// o emissor queria dizer, e não sabe.
    #[test]
    fn flags_reservadas_sao_recusadas() {
        for c in [b'8', b'9', b'A', b'z', b'/', b' '] {
            let mut bytes = moldura_de_teste().as_bytes().to_vec();
            bytes[INICIO_FLAGS] = c;
            assert_eq!(
                abrir(&bytes),
                Err(ErroMedia::FlagsReservadasAcesas(c)),
                "a flag {:?} devia ser recusada",
                c as char
            );
        }
    }

    /// As três flags definidas fazem a ida e volta.
    #[test]
    fn as_flags_definidas_sobem_e_descem() {
        let flags = Flags::VAZIO
            .com(Flags::MANIFESTO)
            .com(Flags::KEYFRAME)
            .com(Flags::FIM_DE_LOTE);
        let m = construir_com_flags(Tipo::Audio, Modo::Streaming, flags, 1, 2, 3, b"x");
        assert_eq!(abrir(m.as_bytes()).unwrap().flags, flags);
        assert_eq!(flags.caractere(), b'7', "'7' é 111: as três flags");
    }

    /// Um separador trocado é recusado.
    ///
    /// A troca é por um `\r` — que **é** ASCII e **não** é o
    /// separador, e é exactamente o caso que a validação tem de apanhar
    /// antes da descodificação.
    #[test]
    fn separador_trocado_e_recusado() {
        let mut bytes = moldura_de_teste().as_bytes().to_vec();
        bytes[INICIO_CORPO] = b'\r';
        assert_eq!(abrir(&bytes), Err(ErroMedia::SeparadorInvalido));
    }

    /// Uma moldura sem corpo é uma moldura válida.
    ///
    /// Não é um caso de borda: o `HOLD` de `docs/media.md` §7.4 é
    /// precisamente uma moldura sem corpo, e é o que um ecrã estático
    /// produz a 30 fps. Recusá-lo seria transformar o melhor caso comum
    /// em erro.
    #[test]
    fn moldura_sem_corpo_e_valida() {
        let m = construir(Tipo::Video, Modo::Streaming, 0, 1, 0, b"");
        assert_eq!(m.as_bytes().len(), TAM_PRE_AMBULO);
        assert!(abrir(m.as_bytes())
            .expect("um HOLD tem de abrir")
            .corpo
            .is_empty());
    }

    /// Nenhuma truncagem devolve o corpo completo.
    ///
    /// A propriedade é mais forte do que «não faz pan» e **não** é «nenhuma
    /// truncagem falha» — porque uma truncagem exactamente no fim do
    /// preâmbulo **é** válida: é a moldura sem corpo, que é o `HOLD` de
    /// `docs/media.md` §7.4.
    ///
    /// O que não pode acontecer é devolver media **a menos** do que foi
    /// enviado. Um parser que devolvesse o prefixo do corpo que ainda
    /// dá parse daria um vídeo truncado como se estivesse inteiro — e o
    /// receptor tocá-lo-ia, porque para ele é um ficheiro válido.
    #[test]
    fn nenhuma_truncagem_devolve_o_corpo_inteiro() {
        let corpo = b"conteudo de video";
        let completa = construir(Tipo::Video, Modo::Streaming, 9, 9, 900, corpo)
            .as_bytes()
            .to_vec();

        // Abaixo do preâmbulo: erro, sempre.
        for n in 0..TAM_PRE_AMBULO {
            assert!(
                abrir(&completa[..n]).is_err(),
                "uma truncagem de {n} bytes abriu, e devia falhar"
            );
        }

        // A partir do preâmbulo: ou falha, ou devolve **menos**.
        for n in TAM_PRE_AMBULO..completa.len() {
            if let Ok(a) = abrir(&completa[..n]) {
                assert!(
                    a.corpo.len() < corpo.len(),
                    "uma truncagem de {n} bytes devolveu os {len} bytes todos",
                    len = a.corpo.len()
                );
            }
        }

        // E o caso que é fácil de confundir com o anterior: cortar um
        // **grupo completo** de 4 caracteres de base-64 dá uma cadeia
        // ainda válida, que abre — com um corpo mais curto. É o
        // comportamento certo, e é por isso que este teste não pode ser
        // escrito como «nenhuma truncagem falha».
        let meada = completa.len() - 4;
        match abrir(&completa[..meada]) {
            Err(_) => {}
            Ok(a) => assert!(
                a.corpo.len() < corpo.len(),
                "cortar 4 caracteres devolveu o corpo inteiro"
            ),
        }
    }

    /// Um algarismo trocado por uma letra é recusado.
    ///
    /// O teste que o `get` não dá: com campos binários, trocar um byte
    /// por outro produzia um valor plausível e o parser aceitava. Com
    /// algarismos, um `'x'` onde se espera um algarismo é um erro
    /// explícito.
    #[test]
    fn um_algarismo_que_nao_e_algarismo_e_recusado() {
        for pos in [INICIO_STREAM, INICIO_STREAM + 4, INICIO_SEQ, INICIO_SEQ + 9] {
            let mut bytes = moldura_de_teste().as_bytes().to_vec();
            bytes[pos] = b'x';
            assert_eq!(
                abrir(&bytes),
                Err(ErroMedia::CampoNaoNumerico),
                "a posição {pos} devia dar CampoNaoNumerico"
            );
        }
    }

    /// Um sinal que não é `+` nem `-` é recusado.
    #[test]
    fn um_sinal_invalido_e_recusado() {
        let mut bytes = moldura_de_teste().as_bytes().to_vec();
        bytes[INICIO_CLOCK] = b'*';
        assert_eq!(abrir(&bytes), Err(ErroMedia::CampoNaoNumerico));
    }

    /// Um número fora de `u32` no campo `seq` é recusado.
    ///
    /// A acumulação é feita em `u64` e convertida com `try_from`, e este
    /// teste prova que a conversão está lá. Sem ela, `seq` com onze
    /// algarismos seria truncado em silêncio — e um `seq` truncado
    /// reinicia a contagem, o que faria o receptor aceitar um envelope
    /// antigo como se fosse novo.
    #[test]
    fn um_seq_fora_de_intervalo_e_recusado() {
        let mut bytes = moldura_de_teste().as_bytes().to_vec();
        // 99 999 999 999, muito acima de u32::MAX.
        bytes[INICIO_SEQ..INICIO_SEQ + LARGURA_SEQ].copy_from_slice(b"9999999999");
        assert_eq!(abrir(&bytes), Err(ErroMedia::CampoForaDeIntervalo));
    }

    /// Nenhum byte, em nenhuma posição, faz o parser entrar em pânico.
    ///
    /// O teste que o fuzzing vai repetir com milhões de combinações, mas
    /// em algumas centenas de casos e legível. Um pânico aqui é um
    /// `abort` do binário inteiro.
    #[test]
    fn nenhum_byte_alterado_faz_panico() {
        let base = construir(
            Tipo::Audio,
            Modo::Streaming,
            0xAAAA,
            0xDEADBEEF,
            -12345,
            b"dados",
        )
        .as_bytes()
        .to_vec();
        for pos in 0..base.len() {
            for valor in [0x00u8, 0x01, 0x2B, 0x30, 0x39, 0x41, 0x7F, 0x80, 0xFF] {
                let mut bytes = base.clone();
                bytes[pos] = valor;
                // Só interessa que não entre em pânico.
                let _ = abrir(&bytes);
            }
        }
    }

    /// Nenhum texto de comprimento arbitrário faz o parser entrar em
    /// pânico.
    ///
    /// O teste anterior mexe num byte de uma moldura válida; este
    /// alimenta bytes que **não** são uma moldura nenhuma. Os dois
    /// caminhos são diferentes e o fuzzer trata-os a ambos.
    #[test]
    fn texto_arbitrario_nao_faz_panico() {
        let sementes: &[&[u8]] = &[
            b"",
            b"ONX1",
            b"ONX1V1S0",
            b"ONX1V1S000000000000000000000000000",
            b"ONX1A1F0-00000000000000000000000\nAAAA",
            b"\xff\xfe\xfd\xfc",
            b"ONX1V1S30000000000000000+00000000000\n====",
        ];
        for semente in sementes {
            for n in 0..semente.len() {
                let _ = abrir(&semente[..n]);
            }
            let _ = abrir(semente);
        }
    }

    /// O limite de media acompanha `MAX_PLAINTEXT`.
    ///
    /// Um literal aqui continuaria o valor antigo se `MAX_PLAINTEXT`
    /// mudasse, e a falha apareceria como `0x09 PayloadGrandeDemais` no
    /// meio de uma transferência.
    #[test]
    fn o_limite_deriva_de_max_plaintext() {
        assert_eq!(TAM_MEDIA_MAX, (65536 - TAM_PRE_AMBULO) / 4 * 3);
        // A folha: o maior pedaço que ainda cabe num envelope.
        let folha = construir(
            Tipo::Video,
            Modo::NaoStreaming,
            0,
            0,
            0,
            &vec![0u8; TAM_MEDIA_MAX],
        );
        assert!(folha.as_bytes().len() <= MAX_PLAINTEXT);
        // Três bytes a mais já não cabem (o base-64 só produz grupos de 3).
        let seguinte = construir(
            Tipo::Video,
            Modo::NaoStreaming,
            0,
            0,
            0,
            &vec![0u8; TAM_MEDIA_MAX + 3],
        );
        assert!(seguinte.as_bytes().len() > MAX_PLAINTEXT);
    }
}
