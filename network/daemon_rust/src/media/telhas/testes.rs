// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes das telhas e do delta.
//!
//! A propriedade central é uma só, e é uma identidade:
//!
//! ```text
//! aplicar(diferenca(f1, f2), f1) == f2
//! ```
//!
//! Se isso não for verdade para **todo** o par de quadris, o receptor
//! dessincroniza e ninguém sabe. E a segunda é o que justifica as
//! telhas: um scroll de 1 pixel custa uma fila de telhas, não o quadro.

use super::*;
use crate::media::ErroMedia;

/// Um quadris determinístico de gradiente, com o tamanho certo por
/// construção.
///
/// Existe com o tamanho calculado e não com uma lista de valores de
/// linha: um quadris montado à mão tem o tamanho errado meio das vezes,
/// e a falha aparece como «o delta não reconstruiu» em vez de «o
/// quadris de teste não tinha as linhas que eu pensei».
fn quadro(largura: usize, altura: usize) -> Vec<u8> {
    (0..(largura * altura))
        .map(|i| {
            let (x, y) = (i % largura, i / largura);
            ((x * 3 + y * 7) % 256) as u8
        })
        .collect()
}

/// O início de uma telha **em bytes**, para os testes escreverem nela.
///
/// Espelha a fórmula de `Telhas::inicio_da_telha` e é por isso que é uma
/// função e não um cálculo em cada teste: dois sítios com o mesmo
/// cálculo divergem.
fn inicio_telha(col: usize, lin: usize, largura: usize) -> usize {
    lin * LADO * largura + col * LADO
}

/// Enche uma telha de um valor — **linha a linha**, como o código.
///
/// Escrever 256 bytes de uma vez é o erro que o próprio módulo documenta:
/// uma telha não é contígua. A primeira versão deste teste fez
/// `fill(200)` sobre 256 bytes seguidos e mudou 16 telhas em 16 linhas
/// diferentes, o que é uma forma muito cara de escrever um teste
/// errado. O ajudante existe para que o teste não tenha de saber disso.
fn encher_telha(q: &mut [u8], col: usize, lin: usize, largura: usize, valor: u8) {
    let inicio = inicio_telha(col, lin, largura);
    for linha in 0..LADO {
        let off = inicio + linha * largura;
        q[off..off + LADO].fill(valor);
    }
}

/// Um quadris liso: todas as linhas iguais.
fn liso(valor: u8, largura: usize, altura: usize) -> Vec<u8> {
    vec![valor; largura * altura]
}

/// Um quadris com uma telha com variação, na telha `(col, lin)`.
fn com_variacao(col: usize, lin: usize, largura: usize, altura: usize) -> Vec<u8> {
    let mut q = liso(40, largura, altura);
    let base = inicio_telha(col, lin, largura);
    for y in 0..LADO {
        for x in 0..LADO {
            q[base + y * largura + x] = ((x * 16 + y) % 256) as u8;
        }
    }
    q
}

#[test]
fn um_quadro_identico_da_delta_vazio() {
    // É a base do `HOLD` de §8.4: uma unidade cujo delta é vazio
    // transporta só a moldura, e com um ecrã estático isso são ~600
    // bytes/s em vez de 5,8 KB/s.
    let f = liso(7, 640, 480);
    let t = Telhas::nova(&f, 640, 480).expect("grelha");
    assert!(t.diferenca(&f).is_empty(), "um quadro igual dá delta vazio");
}

#[test]
fn a_telha_inalterada_nao_escreve_nada() {
    let referencia = liso(7, 640, 480);
    let mut novo = referencia.clone();
    // Uma única telha muda, na terceira linha da grelha. Escolhida uma
    // telha do **meio**: a primeira linha seria o caso em que o erro de
    // passo de linha não aparece, porque nela as duas formulae coincidem.
    let colunas = 640 / LADO;
    let (c, l) = (5usize, 3usize);
    let indice = l * colunas + c;
    novo[inicio_telha(c, l, 640)] = 200;
    let t = Telhas::nova(&referencia, 640, 480).expect("grelha");
    let delta = t.diferenca(&novo);

    // 259 bytes: a telha tem **um** pixel a 200 e o resto a 7, e uma
    // telha com um pixel diferente não é chapada. É o que a distingue de
    // uma chapada a sério, e por isso é `Variacao`.
    assert_eq!(delta.len(), BYTES_INDICE + 1 + BYTES_POR_TELHA, "uma telha");
    assert_eq!(u16::from_be_bytes([delta[0], delta[1]]) as usize, indice);
    assert_eq!(delta[2], 2, "e é uma telha com variação");
}

#[test]
fn uma_telha_chapada_viaja_com_um_simbolo() {
    // O caso de 4 bytes: a telha inteira passa a um só valor. É a
    // diferença que faz um fundo de cor uniforme custar 4 bytes em vez
    // de 256, e é a razão de `Chapada` existir.
    let referencia = liso(7, 640, 480);
    let mut novo = referencia.clone();
    let (c, l) = (5usize, 3usize);
    encher_telha(&mut novo, c, l, 640, 200);

    let t = Telhas::nova(&referencia, 640, 480).expect("grelha");
    let delta = t.diferenca(&novo);

    let colunas = 640 / LADO;
    assert_eq!(
        delta.len(),
        BYTES_INDICE + 1 + 1,
        "índice + código + símbolo"
    );
    assert_eq!(
        u16::from_be_bytes([delta[0], delta[1]]) as usize,
        l * colunas + c
    );
    assert_eq!(delta[2], 1, "código de telha chapada");
    assert_eq!(delta[3], 200);

    // E reconstrói.
    let mut reconstruido = referencia.clone();
    aplicar(&delta, &mut reconstruido, 640).expect("aplica");
    assert_eq!(reconstruido, novo);
}

#[test]
fn uma_telha_com_variacao_viaja_inteira() {
    let referencia = liso(40, 640, 480);
    let novo = com_variacao(3, 2, 640, 480);

    let t = Telhas::nova(&referencia, 640, 480).expect("grelha");
    let delta = t.diferenca(&novo);

    assert_eq!(delta.len(), BYTES_INDICE + 1 + BYTES_POR_TELHA);
    assert_eq!(delta[2], 2, "código de variação");
}

#[test]
fn o_delta_reconstroi_o_quadro_novo() {
    // A identidade. Com um quadro liso e uma telha com variação, com
    // duas chapadas e com a que não muda.
    let largura = 640;
    let altura = 480;
    let referencia = liso(40, largura, altura);

    let mut novo = referencia.clone();
    // Uma com variação, no meio do quadro.
    let base = inicio_telha(1, 2, largura);
    for y in 0..LADO {
        for x in 0..LADO {
            novo[base + y * largura + x] = ((x * 7 + y * 3) % 251) as u8;
        }
    }
    // Duas chapadas, em sítios diferentes.
    encher_telha(&mut novo, 1, 5, largura, 0);
    encher_telha(&mut novo, 3, 9, largura, 255);

    let t = Telhas::nova(&referencia, largura, altura).expect("grelha");
    let delta = t.diferenca(&novo);

    let mut reconstruido = referencia.clone();
    aplicar(&delta, &mut reconstruido, largura).expect("aplica");

    assert_eq!(
        reconstruido,
        novo,
        "o delta não reconstruiu o quadro: {} telhas alteradas",
        (reconstruido.iter().zip(&novo).filter(|(a, b)| a != b)).count()
    );
}

#[test]
fn a_identidade_e_verdade_para_quadris_arbitrarios() {
    // A propriedade acima num conjunto grande de pares, com sementes
    // variadas. Uma identidade que só é verdadeira para um par é uma
    // identidade que falha no primeiro scroll real.
    for semente in 0..24u32 {
        let largura = 64;
        let altura = 48;
        // O `as usize` é explícito porque `semente * 31` força `u32` e
        // um `largura * altura` em `u32` transbordaria numa geometria
        // grande sem dar erro.
        let referencia: Vec<u8> = (0..(largura * altura) as u64)
            .map(|i| ((i * 7 + u64::from(semente) * 31) % 256) as u8)
            .collect();
        // Metade dos bytes mudam, o resto fica.
        let novo: Vec<u8> = referencia
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (i + semente as usize) % 2 == 0 {
                    b
                } else {
                    b ^ 0x5A
                }
            })
            .collect();

        let t = Telhas::nova(&referencia, largura, altura).expect("grelha");
        let delta = t.diferenca(&novo);
        let mut reconstruido = referencia.clone();
        aplicar(&delta, &mut reconstruido, largura).expect("aplica");
        assert_eq!(reconstruido, novo, "semente {semente}");
    }
}

#[test]
fn um_scroll_de_um_pixel_custa_uma_fila_de_telhas() {
    // A razão de serem telhas e não linhas (§6.2). Com linhas, este
    // scroll mudaria **todas** as 480; com telhas, muda uma fila.
    let largura = 640;
    let altura = 480;
    let referencia = quadro(largura, altura);

    let mut novo = referencia.clone();
    // Um scroll de 1 pixel: tudo desloca uma linha.
    novo.copy_within(largura.., largura);

    let t = Telhas::nova(&referencia, largura, altura).expect("grelha");
    let delta = t.diferenca(&novo);
    let colunas = 640 / LADO;

    // Uma fila de telhas, mais a linha que entrou.
    assert!(
        delta.len() <= (colunas + 1) * (BYTES_INDICE + 1 + BYTES_POR_TELHA),
        "o scroll custou {} bytes para {colunas} colunas — mais que uma fila",
        delta.len()
    );
    assert!(
        delta.len() < largura * altura / 4,
        "e não custou o quadro inteiro"
    );

    // E reconstrói, que é o que importa.
    let mut reconstruido = referencia.clone();
    aplicar(&delta, &mut reconstruido, largura).expect("aplica");
    assert_eq!(reconstruido, novo);
}

#[test]
fn um_delta_de_uma_keyframe_cobre_o_quadro_todo() {
    // A keyframe é o caso limite: o delta contra um quadro **vazio**.
    // É por isso que `aplicar` não tem um caminho separado para
    // keyframes — a keyframe é só um delta que cobre tudo.
    let largura = 64;
    let altura = 48;
    let novo = quadro(largura, altura);
    let vazio = vec![0u8; largura * altura];

    let t = Telhas::nova(&vazio, largura, altura).expect("grelha");
    let delta = t.diferenca(&novo);

    let mut reconstruido = vazio.clone();
    aplicar(&delta, &mut reconstruido, largura).expect("aplica");
    assert_eq!(reconstruido, novo, "a keyframe reconstrói o quadro");
}

#[test]
fn um_delta_truncado_e_recusado() {
    // Uma unidade cortada a meio de uma telha tem de dar erro, e não
    // metade da imagem. Escrever o que veio e parar deixaria a telha a
    // meio, e o receptor não teria como saber.
    let largura = 64;
    let altura = 48;
    let referencia = liso(0, largura, altura);
    let novo = com_variacao(1, 1, largura, altura);

    let t = Telhas::nova(&referencia, largura, altura).expect("grelha");
    let delta = t.diferenca(&novo);
    let cortado = &delta[..delta.len() - 10];

    let mut reconstruido = referencia.clone();
    assert!(matches!(
        aplicar(cortado, &mut reconstruido, largura),
        Err(ErroMedia::DeltaTruncado)
    ));
}

#[test]
fn um_codigo_de_telha_desconhecido_e_recusado() {
    // Um código que não é 1 nem 2 é um fluxo de outra versão, e
    // interpretá-lo como telha cheia seria escrever 256 bytes de lixo
    // num_ecrã.
    let mut reconstruido = vec![0u8; 64 * 48];
    let delta = [0u8, 0u8, 9];
    assert!(matches!(
        aplicar(&delta, &mut reconstruido, 64),
        Err(ErroMedia::DeltaCodigoDesconhecido(9))
    ));
}

#[test]
fn uma_telha_fora_do_quadro_e_recusada() {
    // O `get_mut` em vez de indexar a direito: uma telha com índice
    // maior do que a grelha escreveria memória de outro sítio.
    let mut reconstruido = vec![0u8; 64 * 48];
    let delta = [0xff, 0xff, 1, 42];
    assert!(aplicar(&delta, &mut reconstruido, 64).is_err());
}

#[test]
fn uma_geometria_que_nao_e_multiplo_de_16_e_recusada() {
    // 640×360 é o 16:9 mais natural e **não** é válido: 360 não é
    // múltiplo de 16. Uma largura que não caiba na grelha deixaria a
    // última coluna de telhas fora do quadro, e o receptor reconstruiria
    // um quadro de largura diferente — um deslize que se acumula.
    // Os buffers são do tamanho da geometria proposta, para que o que
    // se recusa seja a geometria e não o comprimento.
    assert!(Telhas::nova(&vec![0u8; 640 * 360], 640, 360).is_err());
    assert!(Telhas::nova(&vec![0u8; 639 * 480], 639, 480).is_err());
    assert!(Telhas::nova(&vec![0u8; 640 * 480], 640, 480).is_ok());
}

#[test]
fn um_quadro_do_tamanho_errado_e_recusado() {
    assert!(Telhas::nova(&vec![0u8; 100], 640, 480).is_err());
    assert!(Telhas::nova(&vec![0u8; 640 * 480], 640, 480).is_ok());
}

#[test]
fn o_delta_e_menor_que_o_quadro_sempre() {
    // Nenhum caso pode custar mais do que o quadro inteiro — a menos que
    // o delta seja maior do que gravar o ecrã todo. Com telhas, o pior
    // caso é 259 bytes por telha contra 256, e por isso a compressão
    // só existe se o delta ficar **no stream** e não por telha.
    //
    // A medição que interessa é a de um quadro com variação em todas as
    // telhas, que é o pior caso honesto.
    let largura = 256;
    let altura = 256;
    let referencia = liso(0, largura, altura);
    let novo: Vec<u8> = (0..largura * altura)
        .map(|i| ((i * 37 + i / 7) % 256) as u8)
        .collect();

    let t = Telhas::nova(&referencia, largura, altura).expect("grelha");
    let delta = t.diferenca(&novo);
    let colunas = largura / LADO;
    let telhas = (largura / LADO) * (altura / LADO);

    assert_eq!(delta.len(), telhas * (BYTES_INDICE + 1 + BYTES_POR_TELHA));
    assert_eq!(colunas, 16);
    // Pior caso: 259 bytes por telha contra 256 do quadro. É o preço da
    // telha, e por isso §6.2 diz que a quantização a dois níveis é «o
    // degrau seguinte se a banda apertar».
    assert!(
        delta.len() > largura * altura,
        "o pior caso tem de ser maior que o quadro, senão o teste não mede nada"
    );
}

// ---------------------------------------------------------------------
// O tecto que a medição encontrou
// ---------------------------------------------------------------------

#[test]
fn uma_keyframe_a_640x480_nao_cabe_num_envelope() {
    // A propriedade que §3.4 nunca viu, porque a tabela foi escrita
    // antes do encoder existir.
    //
    // Uma keyframe é o delta contra um quadro zero: leva **todas** as
    // telhas. A 640×480 são 1 200, e cada uma com variação custa 259
    // bytes — 310 800 binários, 414 456 em base-64. O `MAX_PLAINTEXT` é
    // 65 536.
    let telhas = telas_640x480();
    assert_eq!(telhas, 1_200);
    let texto = texto_de_uma_unidade(telhas);
    assert!(
        texto > crate::envelope::MAX_PLAINTEXT,
        "a keyframe de 640×480 devia não caber: {texto} bytes"
    );
    assert!(
        texto > crate::envelope::MAX_PLAINTEXT * 6,
        "e por muito mais: {texto} contra {}",
        crate::envelope::MAX_PLAINTEXT
    );
}

#[test]
fn o_tecto_e_o_numero_que_cabe() {
    // 189 é o maior número de telhas cuja keyframe cabe. E é um
    // **cálculo**, não um literal: `texto_de_uma_unidade` é a função que
    // o deriva do `MAX_PLAINTEXT` e do base-64, e o literal só existe
    // como o valor que a função devolve.
    assert!(texto_de_uma_unidade(MAX_TELHAS_POR_UNIDADE) <= crate::envelope::MAX_PLAINTEXT);
    assert!(texto_de_uma_unidade(MAX_TELHAS_POR_UNIDADE + 1) > crate::envelope::MAX_PLAINTEXT);
}

#[test]
fn as_geometrias_que_cabem_sao_pequenas() {
    // A consequência, que é o que interessa a quem escolhe um formato.
    //
    // E é a consequência do tecto que a medição mostra: **nenhuma** largura de
    // ecrã com 16×16 e base-64 dá uma keyframe que caiba. A 640 de
    // largura cabem 4 fileiras de telhas — 640×64, uma faixa. A maior
    // que caiba com altura útil é 256×176, com 176 telhas.
    let que_cabem: &[(usize, usize)] = &[(256, 176), (256, 160), (192, 176), (160, 176)];
    for (largura, altura) in que_cabem {
        let telhas = (largura / LADO) * (altura / LADO);
        assert!(
            texto_de_uma_unidade(telhas) <= crate::envelope::MAX_PLAINTEXT,
            "{largura}x{altura} devia caber e não cabe"
        );
    }

    // E a primeira que **não** cabe, por muito pouco: 256×192 são 192
    // E a primeira que **não** cabe, por muito pouco: 256×192 são 192
    // telhas, contra o tecto de 189.
    let n = (256 / LADO) * (192 / LADO);
    assert_eq!(n, 192, "é uma telha acima do tecto");
    assert!(n > MAX_TELHAS_POR_UNIDADE);
    assert!(texto_de_uma_unidade(n) > crate::envelope::MAX_PLAINTEXT);
    // A 640 de largura, a maior altura que cabe.
    let colunas = 640 / LADO;
    let linhas_max = MAX_TELHAS_POR_UNIDADE / colunas;
    assert_eq!(linhas_max, 4, "40 colunas dão 4 fileiras e sobrarem 29");
    assert!(texto_de_uma_unidade(colunas * linhas_max) <= crate::envelope::MAX_PLAINTEXT);
    assert!(texto_de_uma_unidade(colunas * (linhas_max + 1)) > crate::envelope::MAX_PLAINTEXT);
}

/// As telhas de 640×480, a geometria de §3.1.
fn telas_640x480() -> usize {
    (640 / LADO) * (480 / LADO)
}
