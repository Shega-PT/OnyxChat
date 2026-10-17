// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/base64.rs — base-64 (RFC 4648 §4), binário ↔ texto
// ---------------------------------------------------------------------
// É o passo que torna a media **texto**, e sem ele nada mais funciona:
// `network/daemon_rust/src/ipc.rs` recusa com `0x08 TextoInvalidoUtf8`
// um plaintext que não seja UTF-8, e um frame de vídeo não é UTF-8.
//
// O passo é reversível, inspeccionável e totalmente determinístico, e é
// por isso que é o passo certo: o receptor não tem de acreditar em
// nada. Desfaz o base-64, lê a moldura, e o que estava lá dentro está
// lá dentro.
//
// ## Porquê à mão e não a dependência
//
// `base64` está no `Cargo.lock` — trazido pela arti, para o Tor — mas
// **não** é dependência directa deste crate, e promovê-lo seria declarar
// que o daemon passa a depender dele para sempre. O formato tem 64
// caracteres de alfabeto e um passo de_grouping; são 60 linhas com
// vectors da RFC 4648 §10 como prova, e zero superfície nova.
//
// ## Porquê o alfabeto **padrão**, e não o URL-safe
//
// A RFC 4648 tem dois. O URL-safe troca `+` por `-` e `/` por `_`, e
// existe para coisas que vão num URL ou num nome de ficheiro. O
// plaintext do OnyxChat não vai num URL nem num nome de ficheiro: vai
// num `FRAME_CHAT`, cujo corpo é **opaco** (`docs/p2p.md` §A moldura
// nunca abre). O custo do alfabeto padrão são dois caracteres que
// nunca aparecem — e escolher o outro seria escolher um formato que
// resolve um problema que este sistema não tem.
//
// ## Porquê tolerante a espaços em branco e nenhum outro ruído
//
// **Não é** tolerante: `decodificar` recusa tudo o que não pertença ao
// alfabeto, incluindo espaços e quebras de linha. Um descodificador
// permissivo aceitaria texto que o emissor nunca produziu, e a
// diferença entre os dois só apareceria como conteúdo diferente do que
// foi enviado. Falhar é mais útil do que adivinhar.
// =====================================================================

/// O alfabeto da RFC 4648 §4, do valor 0 ao valor 63.
///
/// A ordem **é** o valor: `A` é 0, `a` é 26, `0` é 52, `+` é 62 e `/`
/// é 63. Uma tabela de conversão escrita por indexação — em vez de
/// uma cadeia com `.find()` — porque `find()` é uma busca linear em
/// cada byte e isto corre a 30 frames por segundo.
const ALFABETO: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// O byte de preenchimento (`=`).
///
/// Não é um valor do alfabeto: é o marcador de que o grupo **não
/// tinha** esses bytes. Um `=` no meio de uma cadeia é erro, e
/// [`decodificar`] recusa-o.
const PREENCHIMENTO: u8 = b'=';

/// O inverso do alfabeto: valor → caractere.
///
/// Construído uma vez, em tempo de compilação, pela **mesma** regra que
/// [`ALFABETO`] — `ALFABETO[0]` é o valor 0, `ALFABETO[63]` é 63. A
/// simetria é o que impede a tabela de ser invertida à mão e ficar com
/// um deslize de um sitio, que passaria em metade dos testes.
static INVERSO: [u8; 256] = {
    let mut tabela = [0xFFu8; 256];
    let mut i = 0;
    while i < 64 {
        tabela[ALFABETO[i] as usize] = i as u8;
        i += 1;
    }
    tabela
};

/// Erro de um base-64 malformado.
///
/// Não carrega o texto nem a posição: a mesma doutrina de
/// `docs/security_model.md` que impede uma mensagem de erro de conter
/// plaintext. A posição é útil no registo, não na resposta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroBase64 {
    /// O comprimento não é múltiplo de 4.
    ComprimentoInvalido(usize),
    /// Um carácter fora do alfabeto, incluindo espaços e quebras.
    CaractereInvalido,
    /// `=` fora do fim do grupo, ou mais de dois seguidos.
    PreenchimentoInvalido,
}

/// Codifica `dados` em base-64 padrão, com preenchimento.
///
/// O comprimento de saída é sempre `4 × ⌈n / 3⌉` — isto é, **múltiplo
/// de 4**, sempre. A razão de não existirem variantes sem
/// preenchimento é que um alinhamento por 4 deixa o comprimento
/// determinável só de ver a cadeia, e é isso que permite ao receptor
/// recusar `TEXTO_INVALIDO_UTF8` antes de gastar uma descifra.
pub fn codificar(dados: &[u8]) -> String {
    // Pré-alocado a partir do comprimento exacto. Um `String` que cresce
    // durante o codificar obriga o allocator a pedir três blocos
    // sucessivos, e isto corre por frame.
    let saida = codificar_bytes(dados);
    // `String::from_utf8` só falha com bytes inválidos, e o alfabeto é
    // ASCII por construção. É um `expect` a sério inalcançável, e
    // `unsafe { String::from_utf8_unchecked }` aqui trocaria uma prova
    // por um risco sem ganhar nada: a validação é um ciclo.
    String::from_utf8(saida).expect("o alfabeto base-64 é ASCII por construção")
}

/// O mesmo que [`codificar`], em bytes.
///
/// Existe separado porque [`montar`] precisa dos bytes para escrever
/// num buffer de tamanho já conhecido, e voltar a passar por
/// [`codificar`] ali seria uma cópia que a razão do módulo justifica
/// evitar.
pub fn codificar_bytes(dados: &[u8]) -> Vec<u8> {
    let mut saida = Vec::with_capacity(dados.len().div_ceil(3) * 4);

    // Grupos completos de 3 → 4 caracteres, sem casos especiais.
    let mut grupos = dados.chunks_exact(3);
    for grupo in grupos.by_ref() {
        let n = u32::from(grupo[0]) << 16 | u32::from(grupo[1]) << 8 | u32::from(grupo[2]);
        saida.push(ALFABETO[(n >> 18) as usize & 0x3F]);
        saida.push(ALFABETO[(n >> 12) as usize & 0x3F]);
        saida.push(ALFABETO[(n >> 6) as usize & 0x3F]);
        saida.push(ALFABETO[n as usize & 0x3F]);
    }

    // sobra: 1 ou 2 bytes. `chunks_exact` não os devolve.
    let resto = grupos.remainder();
    match resto.len() {
        1 => {
            let n = u32::from(resto[0]) << 16;
            saida.push(ALFABETO[(n >> 18) as usize & 0x3F]);
            saida.push(ALFABETO[(n >> 12) as usize & 0x3F]);
            saida.push(PREENCHIMENTO);
            saida.push(PREENCHIMENTO);
        }
        2 => {
            let n = u32::from(resto[0]) << 16 | u32::from(resto[1]) << 8;
            saida.push(ALFABETO[(n >> 18) as usize & 0x3F]);
            saida.push(ALFABETO[(n >> 12) as usize & 0x3F]);
            saida.push(ALFABETO[(n >> 6) as usize & 0x3F]);
            saida.push(PREENCHIMENTO);
        }
        _ => {}
    }

    saida
}

/// Descodifica base-64 padrão para bytes.
///
/// **Estrito**: recusa comprimento não múltiplo de 4, caracteres fora do
/// alfabeto (espaços e quebras de linha incluídos), e `=` fora do fim
/// do grupo. A explicação está no cabeçalho — a permissividade aqui
/// seria uma fonte de divergência silenciosa entre o que foi emitido e o
/// que é lido.
pub fn descodificar(texto: &[u8]) -> Result<Vec<u8>, ErroBase64> {
    if texto.len() % 4 != 0 {
        return Err(ErroBase64::ComprimentoInvalido(texto.len()));
    }

    let mut saida = Vec::with_capacity(texto.len() / 4 * 3);
    let mut grupos = texto.chunks_exact(4);

    for grupo in grupos.by_ref() {
        // Quantos dos quatro caracteres são `=`. Só pode ser 0, 2 ou 3:
        // 1 byte sobrando dá dois `=`, 2 bytes dão um, 3 bytes dão
        // nenhum. Um `=` nas duas primeiras posições nunca é válido —
        // o preenchimento é sempre à direita.
        let preenchidos = grupo.iter().filter(|&&b| b == PREENCHIMENTO).count();
        if preenchidos > 2 || (preenchidos > 0 && grupo[3] != PREENCHIMENTO) {
            return Err(ErroBase64::PreenchimentoInvalido);
        }
        // Com 2 preenchidos, a segunda posição tem de ser `=`; com 1, a
        // terceira. Isto é o que recusa `A=BC` e `AB=C`.
        if preenchidos == 2 && grupo[2] != PREENCHIMENTO {
            return Err(ErroBase64::PreenchimentoInvalido);
        }

        let mut n: u32 = 0;
        for &b in grupo.iter().take(4 - preenchidos) {
            let valor = INVERSO[b as usize];
            if valor == 0xFF {
                return Err(ErroBase64::CaractereInvalido);
            }
            n = n << 6 | u32::from(valor);
        }
        // Desloca à esquerda pelos bits que o preenchimento representava,
        // para que os bytes saiam alinhados à esquerda.
        n <<= 6 * preenchidos;

        saida.push((n >> 16) as u8);
        if preenchidos < 2 {
            saida.push((n >> 8) as u8);
        }
        if preenchidos < 1 {
            saida.push(n as u8);
        }
    }

    Ok(saida)
}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::moldura;

    /// Os vectors da RFC 4648 §10, tal e qual.
    ///
    /// Não são inventados: são os oito casos da norma, e a norma é
    /// a norma que os escreveu. Um codec de base-64 que não passa nisto
    /// não é base-64, é outra coisa — e o receptor que falha em silêncio
    /// é pior do que o que falha.
    const VECTORES_RFC: &[(&str, &str)] = &[
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];

    #[test]
    fn os_vectors_da_rfc_4648_passam() {
        for (claro, codificado) in VECTORES_RFC {
            let bytes = claro.as_bytes();
            assert_eq!(
                codificar(bytes),
                *codificado,
                "«{claro}» devia dar «{codificado}»"
            );
            assert_eq!(
                descodificar(codificado.as_bytes()).unwrap(),
                bytes,
                "«{codificado}» devia dar «{claro}»"
            );
        }
    }

    /// A propriedade que substitui todos os vectores: ida e volta.
    ///
    /// Os vectores da RFC provam que o codec está de acordo com a norma
    /// em oito pontos. Isto prova que está de acordo **em todo o lado**,
    /// inclusive nos comprimentos que a norma não exemplifica.
    #[test]
    fn ida_e_volta_em_todos_os_comprimentos() {
        let original: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
        for n in 0..=original.len() {
            let entrada = &original[..n];
            let codificado = codificar_bytes(entrada);
            assert_eq!(
                codificado.len() % 4,
                0,
                "o comprimento de saída tem de ser múltiplo de 4 (n = {n})"
            );
            assert_eq!(
                descodificar(&codificado).unwrap(),
                entrada,
                "a ida e volta falhou em {n} bytes"
            );
        }
    }

    /// O comprimento de saída é exactamente `4 × ⌈n/3⌉`.
    ///
    /// A fórmula é o que o receptor usa para saber o tamanho do payload
    /// a decifrar **antes** de o decifrar, e portanto para validar
    /// `MAX_PLAINTEXT` sem alocar o que quer que seja. Se a fórmula
    /// estiver errada, o receptor aloca a mais ou recusa o que cabia.
    #[test]
    fn o_comprimento_de_saida_e_exacto() {
        for n in 0..=256usize {
            let dados = vec![0u8; n];
            assert_eq!(
                codificar_bytes(&dados).len(),
                n.div_ceil(3) * 4,
                "comprimento errado para {n} bytes"
            );
        }
    }

    /// O limite da §3.2 de `docs/media.md`: 49 125 bytes de media com o
    /// preâmbulo de 36 bytes.
    ///
    /// O número está nas **duas** metades do teste porque um erro pode
    /// estar em qualquer uma: se o codec estufar, o segundo falha; se o
    /// preâmbulo crescer, o primeiro falha. E ambos precisam de ser
    /// verificados contra `MAX_PLAINTEXT`, que é uma constante de outro
    /// ficheiro — se alguém a mexer, este teste tem de dizer que o
    /// limite de media mudou.
    #[test]
    fn o_limite_de_media_cai_dentro_do_plaintext() {
        // Derivado de `MAX_PLAINTEXT` e do tamanho do preâmbulo, que é
        // 31. Um literal aqui divergiria do que `TAM_MEDIA_MAX` diz, e
        // a divergência apareceria como um `0x09 PayloadGrandeDemais` a
        // meio de uma transferência.
        let maximo = moldura::TAM_MEDIA_MAX;

        let moldura = moldura::construir(
            moldura::Tipo::Video,
            moldura::Modo::NaoStreaming,
            0,
            1,
            0,
            &vec![0u8; maximo],
        );
        assert!(
            moldura.as_bytes().len() <= crate::envelope::MAX_PLAINTEXT,
            "{} bytes de media dão {} de plaintext, acima de {}",
            maximo,
            moldura.as_bytes().len(),
            crate::envelope::MAX_PLAINTEXT,
        );

        // E três bytes a mais já não cabem — o limite é real, não
        // folgado. Três e não um, porque o base-64 só produz grupos de
        // 3: um byte a mais seria o mesmo grupo de saída.
        let excesso = moldura::construir(
            moldura::Tipo::Video,
            moldura::Modo::NaoStreaming,
            0,
            1,
            0,
            &vec![0u8; maximo + 3],
        );
        assert!(
            excesso.as_bytes().len() > crate::envelope::MAX_PLAINTEXT,
            "{} bytes deviam exceder MAX_PLAINTEXT e não excedem",
            maximo + 3
        );
    }

    /// Caracteres fora do alfabeto são recusados, incluindo o espaço.
    ///
    /// Um descodificador que saltasse os brancos aceitaria uma cadeia que
    /// o emissor nunca produziu. A diferença apareceria como conteúdo
    /// diferente do que foi enviado, e a culpa seria de quem escreveu o
    /// emissor.
    #[test]
    fn caracteres_fora_do_alfabeto_sao_recusados() {
        // Todas com comprimento múltiplo de 4 **de propósito**: a
        // verificação do comprimento vem primeiro (ver `descodificar`), e
        // uma cadeia de 9 caracteres seria recusada por isso — a
        // passar, o teste deixaria de testar o que diz testar.
        for texto in [
            "Zm9v YmF",                         // espaço
            "Zm9v\nYmE",                        // quebra de linha
            "Zm9v\tYmE",                        // tabulador
            "Zm9vYmF-",                         // '-' é o alfabeto URL-safe, não o padrão
            "Zm9vYmF_",                         // '_' idem
            "Z!9v",                             // pontuação
            "\u{00e9}\u{00e8}\u{00e9}\u{00e8}", // acentuadas: UTF-8 válido, base-64 não
        ] {
            assert_eq!(
                descodificar(texto.as_bytes()),
                Err(ErroBase64::CaractereInvalido),
                "«{texto}» devia ser recusado"
            );
        }
    }

    /// O preenchimento só é válido nas duas últimas posições.
    ///
    /// `A=BC` e `AB=C` têm o comprimento certo e caracteres do
    /// alfabeto, e são as duas formas de preenchimento mal colocado que
    /// um verificador tolerante aceitaria.
    #[test]
    fn preenchimento_mal_colocado_e_recusado() {
        for texto in ["A=BC", "AB=C", "=ABC", "A===", "=AAA", "AAAA="] {
            let r = descodificar(texto.as_bytes());
            assert!(r.is_err(), "«{texto}» devia ser recusado, deu {r:?}");
        }
    }

    /// Um comprimento que não é múltiplo de 4 é recusado antes de
    /// qualquer outra verificação.
    ///
    /// A ordem importa: é mais barato rejeitar pelo comprimento do que
    /// percorrer a cadeia à procura de um carácter mau, e o comprimento
    /// é a coisa que um emissor corrompido estraga primeiro.
    #[test]
    fn comprimento_nao_multiplo_de_quatro_e_recusado() {
        // A lista é explícita e **não** um intervalo, porque um
        // intervalo esconderia o caso válido no meio: `1..8` inclui o 4,
        // que é um comprimento perfeitamente válido, e o teste passava
        // por cima dele em silêncio.
        for n in [1usize, 2, 3, 5, 6, 7] {
            let texto = vec![b'A'; n];
            assert_eq!(
                descodificar(&texto),
                Err(ErroBase64::ComprimentoInvalido(n)),
                "comprimento {n} devia ser recusado"
            );
        }
        // E o 4 é válido, para o teste não ser apenas de recusas.
        assert!(descodificar(b"AAAA").is_ok());
    }

    /// Um único byte não produz duas saídas, e dois não produzem três.
    ///
    /// É o erro de fora-memória mais discreto que existe num
    /// descodificador: empurrar o byte seguinte à frente e devolver
    /// `len + 1`. Nenhum dos vectores da RFC o apanha, porque nenhum
    /// deles compara comprimentos.
    #[test]
    fn o_preenchimento_nao_cria_bytes_fantasma() {
        assert_eq!(descodificar(b"Zg==").unwrap().len(), 1);
        assert_eq!(descodificar(b"Zm8=").unwrap().len(), 2);
        assert_eq!(descodificar(b"Zm9v").unwrap().len(), 3);
    }

    /// A tabela inversa é o inverso exacto do alfabeto.
    ///
    /// Um teste de tabelas: se alguém inverter `ALFABETO` à mão, este
    /// falha com um nome de carácter em vez de com bytes corrompidos
    /// muito mais tarde.
    #[test]
    fn a_tabela_inversa_coincide_com_o_alfabeto() {
        for (valor, &caractere) in ALFABETO.iter().enumerate() {
            assert_eq!(
                INVERSO[caractere as usize], valor as u8,
                "o valor {valor} devia mapear para {:?}",
                caractere as char
            );
        }
        // E os que não são do alfabeto têm de dar `0xFF`, nunca um valor
        // válido — senão um carácter aleatório descodificava em silêncio.
        assert_eq!(INVERSO[b'=' as usize], 0xFF);
        assert_eq!(INVERSO[b' ' as usize], 0xFF);
        assert_eq!(INVERSO[0x00], 0xFF);
    }
}
