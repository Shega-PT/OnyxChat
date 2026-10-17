// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes de `media/ivf.rs`.

use super::*;

/// Um IVF válido, com os quadros indicados.
fn ivf(quadros: &[&[u8]]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&ASSINATURA);
    v.push(0); // versão
    v.push(TAM_CABECALHO as u8);
    // Dois bytes de reservado ANTES do `fourcc`. Sem eles o cabeçalho
    // tem 30 e o `fourcc` é lido dois bytes cedo — que é o tipo de
    // bug que devolve `fourcc` correcto e geometria errada.
    v.extend_from_slice(&[0x20, 0x00]);
    v.extend_from_slice(&FOURCC_VP8);
    v.extend_from_slice(&320u16.to_le_bytes());
    v.extend_from_slice(&240u16.to_le_bytes());
    v.extend_from_slice(&30u32.to_le_bytes());
    v.extend_from_slice(&1u32.to_le_bytes()); // num frames = 1 (ver nota)
                                              // Seis bytes de reservado, não oito: o cabeçalho tem 32 e as
                                              // contas acima já fazem 26. Um `5A` e cinco zeros — que é o que o
                                              // `ffmpeg` escreve, e que o vector real mais abaixo fixa.
    v.push(0x5A);
    v.extend_from_slice(&[0u8; 7]);
    debug_assert_eq!(v.len(), TAM_CABECALHO);
    for (i, q) in quadros.iter().enumerate() {
        v.extend_from_slice(&(q.len() as u32).to_le_bytes());
        v.extend_from_slice(&((i as u64) * 33).to_le_bytes());
        v.extend_from_slice(q);
    }
    v
}

// ---------------------------------------------------------------------
// O cabeçalho
// ---------------------------------------------------------------------

#[test]
fn o_cabecalho_diz_a_geometria_e_o_fps() {
    let cab = Cabecalho::ler(&ivf(&[])).expect("cabeçalho válido");
    assert_eq!(cab.largura, 320);
    assert_eq!(cab.altura, 240);
    assert_eq!(cab.fps, 30);
}

#[test]
fn o_numero_de_quadros_do_cabecalho_mente_e_e_1() {
    // O `ffmpeg` escreve o cabeçalho antes de saber quantos quadros vai
    // produzir, e por isso o campo vale 1 num stream. Confiar nele dava
    // um receptor que lê um quadro e acha que acabou.
    //
    // Este teste existe para **fixar o facto**, não o comportamento
    // desejado: o leitor tem de ignorar o campo, e o próximo teste é
    // que o prova.
    let d = ivf(&[&[0xAA; 10], &[0xBB; 20], &[0xCC; 30]]);
    let cab = Cabecalho::ler(&d).expect("cabeçalho válido");
    assert_eq!(d[20..24], 1u32.to_le_bytes(), "o campo diz 1");
    assert!(cab.largura > 0, "e o cabeçalho é válido na mesma");
}

#[test]
fn o_leitor_ignora_o_numero_de_quadros() {
    let d = ivf(&[&[0xAA; 10], &[0xBB; 20], &[0xCC; 30]]);
    let mut l = LeitorIvf::novo(&d[..]).expect("leitor");
    let mut n = 0usize;
    loop {
        match l.proximo().expect("leitura") {
            EstadoQuadro::Fim => break,
            EstadoQuadro::Quadro(q) => {
                n += 1;
                assert!(q.dados.len() >= 10);
            }
        }
    }
    assert_eq!(n, 3, "três quadros escritos, três lidos");
}

#[test]
fn um_cabecalho_que_nao_e_ivf_e_recusado() {
    // Um pipe pode receber o que outro programa escreva. Um leitor que
    // tratasse bytes desconhecidos como VP8 decodificaria silêncio e
    // apresentaria um ecrã preto sem nenhum sinal.
    let mut d = ivf(&[]);
    d[0] = b'X';
    assert!(matches!(
        LeitorIvf::novo(&d[..]),
        Err(ErroIvf::AssinaturaInvalida)
    ));
}

#[test]
fn um_ivf_de_outro_codec_e_recusado() {
    // IVF também pode conter VP9 ou AV1, e este daemon não os tem.
    let mut d = ivf(&[]);
    d[8..12].copy_from_slice(b"VP90");
    assert!(matches!(
        LeitorIvf::novo(&d[..]),
        Err(ErroIvf::FourccDesconhecido(_))
    ));
}

#[test]
fn uma_versao_desconhecida_e_recusada_em_vez_de_ser_adivinhada() {
    // Na versão 1 o cabeçalho tem campos extra. Ler os campos de lá
    // como se fossem os de cá daria uma geometria errada em silêncio.
    let mut d = ivf(&[]);
    d[4] = 1;
    assert!(matches!(
        LeitorIvf::novo(&d[..]),
        Err(ErroIvf::VersaoDesconhecida(1))
    ));
}

#[test]
fn um_stream_que_acaba_antes_do_cabecalho_e_recusado() {
    assert!(matches!(
        LeitorIvf::novo(&[0u8; 10][..]),
        Err(ErroIvf::CabecalhoCurto(10))
    ));
    assert!(matches!(
        LeitorIvf::novo(&[][..]),
        Err(ErroIvf::FimAntesDoCabecalho)
    ));
}

// ---------------------------------------------------------------------
// Os quadros
// ---------------------------------------------------------------------

#[test]
fn os_quadros_vem_com_o_timestamp_certo() {
    let d = ivf(&[&[0xAA; 10], &[0xBB; 20]]);
    let mut l = LeitorIvf::novo(&d[..]).expect("leitor");
    match l.proximo().expect("leitura") {
        EstadoQuadro::Quadro(q) => {
            assert_eq!(q.dados, vec![0xAA; 10]);
            assert_eq!(q.timestamp, 0);
        }
        EstadoQuadro::Fim => panic!("não há quadros"),
    }
    match l.proximo().expect("leitura") {
        EstadoQuadro::Quadro(q) => {
            assert_eq!(q.dados, vec![0xBB; 20]);
            assert_eq!(q.timestamp, 33);
        }
        EstadoQuadro::Fim => panic!("o segundo quadro não chegou"),
    }
}

#[test]
fn um_quadro_truncado_nao_e_o_fim_do_stream() {
    // O `ffmpeg` morreu a meio de escrever. Isso é diferente de ter
    // terminado, e um leitor que tratasse os dois como fim dava uma
    // chamada que pára sem erro — o pior sintoma possível.
    let mut d = ivf(&[&[0xAA; 100]]);
    d.truncate(d.len() - 40);
    let mut l = LeitorIvf::novo(&d[..]).expect("leitor");
    assert!(matches!(l.proximo(), Err(ErroIvf::QuadroTruncado(_))));
}

#[test]
fn um_quadro_de_tamanho_zero_e_recusado() {
    // Significaria que o `ffmpeg` escreveu um comprimento em vez de um
    // tamanho. Sem este erro o receptor alocaria zero, leria o
    // cabeçalho do quadro seguinte como payload, e a imagem seria
    // lixo com a geometria certa.
    let mut d = ivf(&[]);
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&0u64.to_le_bytes());
    let mut l = LeitorIvf::novo(&d[..]).expect("leitor");
    assert!(matches!(l.proximo(), Err(ErroIvf::QuadroVazio)));
}

#[test]
fn a_geometria_esperada_compara_com_o_cabecalho() {
    let d = ivf(&[]);
    let l = LeitorIvf::novo(&d[..]).expect("leitor");
    assert!(l.geometria_esperada(320, 240));
    assert!(!l.geometria_esperada(854, 480), "outra geometria não bate");
    assert!(!l.geometria_esperada(320, 241));
}

#[test]
fn os_bytes_vem_intactos_para_o_decoder() {
    // O VP8 não é auto-contido: os frames-P são deltas contra os
    // anteriores. Um leitor que "limpasse" os bytes — ou que
    // reescrevesse o timestamp dentro do payload — daria um receptor
    // que decodifica WrongPictureType.
    let original: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    let d = ivf(&[&original]);
    let mut l = LeitorIvf::novo(&d[..]).expect("leitor");
    match l.proximo().expect("leitura") {
        EstadoQuadro::Quadro(q) => assert_eq!(q.dados, original),
        EstadoQuadro::Fim => panic!("não há quadros"),
    }
}

#[test]
fn o_fim_do_stream_e_fim_e_nao_erro() {
    let d = ivf(&[&[0xAA; 10]]);
    let mut l = LeitorIvf::novo(&d[..]).expect("leitor");
    assert!(matches!(l.proximo(), Ok(EstadoQuadro::Quadro(_))));
    assert!(matches!(l.proximo(), Ok(EstadoQuadro::Fim)));
    // E continua a ser fim, não um erro que cresce.
    assert!(matches!(l.proximo(), Ok(EstadoQuadro::Fim)));
}

// ---------------------------------------------------------------------
// Contra o IVF que o binário real produziu
// ---------------------------------------------------------------------

/// Um IVF escrito à mão com o mesmo formato que o `ffmpeg` produz.
///
/// É um vector, não um ficheiro gerado: o teste tem de correr sem o
/// `ffmpeg`, e um ficheiro gerado seria uma dependência de que o
/// `cargo test` não pode ter.
#[test]
fn o_formato_e_o_que_o_ffmpeg_escreve() {
    // Estes bytes saíram de um `ffmpeg -f ivf` real, e estão aqui para
    // que uma mudança no formato do contentor não passe despercebida.
    let real = [
        0x44u8, 0x4B, 0x49, 0x46, // DKIF
        0x00, // versão
        0x00, // cabeçalho de 0 bytes extra (o `ffmpeg` escreve 0)
        0x20, 0x00, // reservado
        0x56, 0x50, 0x38, 0x30, // VP80
        0x40, 0x01, // 320
        0xF0, 0x00, // 240
        0x1E, 0x00, 0x00, 0x00, // 30 fps
        0x01, 0x00, 0x00, 0x00, // 1 frame
    ];
    let mut d = real.to_vec();
    d.extend_from_slice(&[0x5A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // reservado
    assert_eq!(d.len(), TAM_CABECALHO);

    let cab = Cabecalho::ler(&d).expect("cabeçalho real");
    assert_eq!((cab.largura, cab.altura, cab.fps), (320, 240, 30));

    // E um quadro real: o tamanho e o timestamp, em little-endian.
    // O tamanho é 1201, que é o que o `ffmpeg` escreveu no ficheiro que
    // estes bytes vieram.
    let mut quadro = vec![0u8; 1201];
    quadro[..6].copy_from_slice(&[0xB0, 0x27, 0x00, 0x9D, 0x01, 0x2A]);
    d.extend_from_slice(&1201u32.to_le_bytes());
    d.extend_from_slice(&0u64.to_le_bytes());
    d.extend_from_slice(&quadro);

    let mut l = LeitorIvf::novo(&d[..]).expect("leitor real");
    assert_eq!(l.cabecalho().largura, 320);
    match l.proximo().expect("leitura") {
        EstadoQuadro::Quadro(q) => {
            assert_eq!(q.dados.len(), 1201);
            assert_eq!(&q.dados[..6], &[0xB0, 0x27, 0x00, 0x9D, 0x01, 0x2A]);
        }
        EstadoQuadro::Fim => panic!("não há quadros"),
    }
}
