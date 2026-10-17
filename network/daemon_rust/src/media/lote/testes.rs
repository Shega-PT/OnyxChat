// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes de `media/lote.rs`.

use super::*;
use crate::media::telhas::Telhas;

/// Um ecrã sintético: `marcas` telhas com variação contra o zero.
fn ecra(largura: usize, altura: usize, marcas: &[(usize, u8)]) -> Vec<u8> {
    let mut q = vec![0u8; largura * altura];
    let colunas = largura / LADO;
    for (indice, valor) in marcas {
        let (col, lin) = (indice % colunas, indice / colunas);
        let base = lin * LADO * largura + col * LADO;
        for y in 0..LADO {
            for x in 0..LADO {
                q[base + y * largura + x] = valor.wrapping_add((x + y) as u8);
            }
        }
    }
    q
}

/// Todas as telhas com variação, para um delta de keyframe.
fn keyframe(largura: usize, altura: usize) -> (Vec<u8>, Vec<u8>) {
    let total = (largura / LADO) * (altura / LADO);
    let marcas: Vec<(usize, u8)> = (0..total).map(|i| (i, 3u8)).collect();
    let q = ecra(largura, altura, &marcas);
    let zero = vec![0u8; largura * altura];
    let t = Telhas::nova(&zero, largura, altura).expect("grelha");
    (q.clone(), t.diferenca(&q))
}

// ---------------------------------------------------------------------
// Fracção
// ---------------------------------------------------------------------

#[test]
fn uma_unidade_cabe_num_envelope() {
    // A propriedade que justifica o módulo inteiro. Cada parte sai com
    // 189 telhas ou menos, e o envelope leva 189.
    let (_, delta) = keyframe(640, 480);
    let partes = dividir(&delta, 0, 640, 480).expect("fracção");
    assert_eq!(partes.len(), 7, "1 200 telhas a 189 por unidade");

    for parte in &partes {
        assert!(
            indices_do_delta(&parte.delta).expect("índices").len() <= TELHAS_POR_UNIDADE,
            "parte com {} telhas",
            indices_do_delta(&parte.delta).unwrap().len()
        );
        let texto = super::super::telhas::texto_de_uma_unidade(
            indices_do_delta(&parte.delta).unwrap().len(),
        );
        assert!(
            texto <= crate::envelope::MAX_PLAINTEXT,
            "parte de {texto} bytes não cabe"
        );
    }
}

#[test]
fn o_lote_fracciona_a_keyframe_de_640x480() {
    // O caso que a spec dizia impossível. 1 200 telhas → 7 unidades.
    let (quadro, delta) = keyframe(640, 480);
    assert_eq!(delta.len(), 1_200 * BYTES_DE_ENTRADA);

    let partes = dividir(&delta, 100, 640, 480).expect("fracção");
    assert_eq!(partes.len(), 7);

    // Os `seq` são consecutivos e cada um é a sua própria base.
    for (k, parte) in partes.iter().enumerate() {
        assert_eq!(parte.cabecalho.base_seq, 100 + k as u32);
        assert_eq!(parte.cabecalho.indice, k as u16);
        assert_eq!(parte.cabecalho.n_unidades, 7);
        assert_eq!(parte.cabecalho.largura, 640);
        assert_eq!(parte.cabecalho.altura, 480);
        assert_eq!(parte.cabecalho.eh_ultima(), k == 6);
    }
    let _ = quadro;
}

#[test]
fn o_lote_cobre_as_telhas_sem_historico_e_a_imagem_e_a_certa() {
    // O teste que importa: o receptor tem de reconstruir **a imagem
    // original**, não uma aproximadamente parecida.
    let (quadro, delta) = keyframe(640, 480);
    let partes = dividir(&delta, 0, 640, 480).expect("fracção");

    let mut m = MontadorLote::novo();
    let mut montada: Option<Vec<u8>> = None;
    for (k, parte) in partes.iter().enumerate() {
        let mut corpo = Vec::new();
        parte.cabecalho.escrever(&mut corpo);
        corpo.extend_from_slice(&parte.delta);
        match m.aceitar(&corpo).expect("lote") {
            Montagem::EmCurso => assert!(k < partes.len() - 1, "fechou cedo demais"),
            Montagem::Completo(img) => {
                assert_eq!(k, partes.len() - 1, "fechou tarde demais");
                montada = Some(img);
            }
            Montagem::Descartada => panic!("a parte {k} foi descartada"),
        }
    }

    assert_eq!(montada.as_deref(), Some(quadro.as_slice()));
    assert!(m.esta_vazio(), "o montador ficou com meio lote");
    assert_eq!(m.descartados, 0);
}

// ---------------------------------------------------------------------
// A integridade é de índices, não de contagem
// ---------------------------------------------------------------------

#[test]
fn um_lote_com_um_buraco_e_descartado() {
    // Perde-se a unidade do meio. A imagem que resultaria de aplicar o
    // resto tem um buraco do tamanho de um quadrante, e o receptor tem
    // de recusar a imagem inteira em vez de a mostrar.
    let (_, delta) = keyframe(640, 480);
    let partes = dividir(&delta, 0, 640, 480).expect("fracção");

    let corpo_de = |p: &Parte| {
        let mut c = Vec::new();
        p.cabecalho.escrever(&mut c);
        c.extend_from_slice(&p.delta);
        c
    };

    let mut m = MontadorLote::novo();
    assert!(matches!(
        m.aceitar(&corpo_de(&partes[0])),
        Ok(Montagem::EmCurso)
    ));
    // Salta a 1 e dá a 2: um buraco.
    assert_eq!(m.aceitar(&corpo_de(&partes[2])), Ok(Montagem::Descartada));
    assert!(m.esta_vazio(), "um lote com buraco tem de ficar vazio");
    assert!(m.descartados > 0);
}

#[test]
fn uma_keyframe_so_com_telhas_iguais_ao_zero_e_valida() {
    // Um ecrã todo preto tem **zero** telhas com variação. É um caso
    // real — ecrã apagado, ecrã bloqueado — e a versão que contava
    // telhas exigia que houvesse alguma, e portanto o rejeitava.
    let largura = 64;
    let altura = 48;
    let zero = vec![0u8; largura * altura];
    let delta = Telhas::nova(&zero, largura, altura)
        .expect("grelha")
        .diferenca(&zero);
    assert!(delta.is_empty(), "um ecrã preto não tem delta");

    let partes = dividir(&delta, 0, largura as u16, altura as u16).expect("fracção");
    assert_eq!(partes.len(), 1, "nada a fracções, uma unidade só");

    let mut m = MontadorLote::novo();
    let r = m.aceitar(&partes[0].delta_padded()).expect("lote");
    assert_eq!(
        r,
        Montagem::Completo(zero.clone()),
        "uma keyframe preta tem de dar um ecrã preto"
    );
}

#[test]
fn uma_continuacao_sem_inicio_e_lixo() {
    // Uma unidade de índice 3 sem nada antes não tem de onde ser
    // montada. A única resposta segura é deitar fora.
    let (_, delta) = keyframe(640, 480);
    let partes = dividir(&delta, 0, 640, 480).expect("fracção");
    let mut c = Vec::new();
    partes[3].cabecalho.escrever(&mut c);
    c.extend_from_slice(&partes[3].delta);

    let mut m = MontadorLote::novo();
    assert_eq!(m.aceitar(&c), Ok(Montagem::Descartada));
    assert_eq!(m.descartados, 1);
}

#[test]
fn um_lote_novo_descarta_o_que_estava_a_meio() {
    // Uma keyframe nova substitui a anterior por completo. Misturar
    // telhas das duas daria uma imagem que não é de nenhuma.
    let (_, delta) = keyframe(640, 480);
    let partes = dividir(&delta, 0, 640, 480).expect("fracção");
    let corpo_de = |p: &Parte| {
        let mut c = Vec::new();
        p.cabecalho.escrever(&mut c);
        c.extend_from_slice(&p.delta);
        c
    };

    let mut m = MontadorLote::novo();
    m.aceitar(&corpo_de(&partes[0])).expect("lote");
    assert!(!m.esta_vazio());
    // A mesma keyframe outra vez, do princípio.
    assert!(matches!(
        m.aceitar(&corpo_de(&partes[0])),
        Ok(Montagem::EmCurso)
    ));
    assert_eq!(m.telhas_recebidas(), TELHAS_POR_UNIDADE);
}

// ---------------------------------------------------------------------
// O cabeçalho é Bytes de memória que o receptor aloca
// ---------------------------------------------------------------------

#[test]
fn a_geometria_do_cabecalho_e_validada() {
    // Doze bytes de texto que mandam o receptor alocar quatro
    // gigabytes. Se isto passar, é um amplificador de graça.
    let corpo = |largura: u16, altura: u16| {
        let mut c = CabecalhoLote {
            base_seq: 0,
            largura,
            altura,
            n_unidades: 1,
            indice: 0,
        };
        let mut v = Vec::new();
        c.escrever(&mut v);
        v
    };

    // Múltiplo de 16, e ainda assim 4,29 GB. É este o caso que o check
    // de multiplicidade **não** apanha, e é o que o tecto de memória
    // apanha.
    assert!(matches!(
        CabecalhoLote::ler(&corpo(65520, 65520)),
        Err(ErroMedia::ImagemEnorme { .. })
    ));
    assert!(
        CabecalhoLote::ler(&corpo(4096, 4096)).is_ok(),
        "16 MB passam"
    );
    assert!(
        CabecalhoLote::ler(&corpo(17, 16)).is_err(),
        "não é múltiplo de 16"
    );
    assert!(CabecalhoLote::ler(&corpo(0, 16)).is_err(), "largura zero");
    assert!(CabecalhoLote::ler(&corpo(16, 16)).is_ok());

    let mut curto = Vec::new();
    CabecalhoLote {
        base_seq: 0,
        largura: 16,
        altura: 16,
        n_unidades: 1,
        indice: 0,
    }
    .escrever(&mut curto);
    curto.truncate(11);
    assert!(matches!(
        CabecalhoLote::ler(&curto),
        Err(ErroMedia::LoteTruncado(11))
    ));
}

#[test]
fn os_indices_tem_de_crescer() {
    // Um delta com o índice a andar para trás é um delta corrompido, e
    // dizer isso é melhor do que aplicar bytes na ordem errada.
    let mut d = Vec::new();
    for indice in [5u16, 3, 9] {
        d.extend_from_slice(&indice.to_be_bytes());
        d.push(1);
        d.extend_from_slice(&[7u8; BYTES_POR_TELHA]);
    }
    assert!(matches!(
        indices_do_delta(&d),
        Err(ErroMedia::LoteIndiceFora {
            indice: 3,
            n_unidades: 5
        })
    ));
}

#[test]
fn um_delta_truncado_e_um_delta_que_nao_e_multiplo_de_259() {
    // A telha tem 259 bytes e o delta é uma sequência delas. Um resto
    // no fim é truncamento, não uma telha meia.
    let mut d = vec![7u8; BYTES_DE_ENTRADA - 1];
    assert!(matches!(
        indices_do_delta(&d),
        Err(ErroMedia::LoteDeltaTruncado(_))
    ));
}

// ---------------------------------------------------------------------
// O que o módulo diz ao coalescer
// ---------------------------------------------------------------------

#[test]
fn o_tecto_de_telhas_por_unidade_e_o_que_se_encaixa() {
    // 189 é o maior número de telhas cuja unidade cabe num envelope, e
    // é um cálculo derivado do `MAX_PLAINTEXT` — não um literal.
    assert!(
        super::super::telhas::texto_de_uma_unidade(TELHAS_POR_UNIDADE)
            <= crate::envelope::MAX_PLAINTEXT
    );
    assert!(
        super::super::telhas::texto_de_uma_unidade(TELHAS_POR_UNIDADE + 1)
            > crate::envelope::MAX_PLAINTEXT
    );
}

impl Parte {
    /// O corpo completo: cabeçalho seguido do delta.
    fn delta_padded(&self) -> Vec<u8> {
        let mut c = Vec::new();
        self.cabecalho.escrever(&mut c);
        c.extend_from_slice(&self.delta);
        c
    }
}
