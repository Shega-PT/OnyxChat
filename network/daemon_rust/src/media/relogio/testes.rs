// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes do relógio.
//!
//! O que tentam estragar: um relógio que arredonda sem resto (e perde
//! 40% do tempo), e uma regra de sincronia que se traduz em `sleep`
//! algures. Nenhum dos dois aparece aqui porque este módulo não tem
//! relógio de parede — e é essa a propriedade que os testes fixam.

use super::*;

#[test]
fn o_audio_avanca_o_relogio_mestre() {
    let mut r = RelogioMedia::novo(96); // PCM s16le 48 kHz
    let mut resto = 0u32;
    r.avancar_audio(96, &mut resto);
    assert_eq!(r.media_ms, 1, "96 bytes a 96 bytes/ms são 1 ms");
}

#[test]
fn o_resto_nao_se_perde() {
    // 4 bytes/ms (Opus CBR 32 kbps) durante 50 frames: 200 bytes são
    // exactamente 50 ms. Sem o resto, cada frame arredondaria para 1 ms
    // e o relógio andaria a 50% da velocidade — que é um áudio que
    // arrasta dois segundos por minuto de vídeo.
    let mut r = RelogioMedia::novo(4);
    let mut resto = 0u32;
    for _ in 0..50 {
        r.avancar_audio(4, &mut resto);
    }
    assert_eq!(r.media_ms, 50, "200 bytes a 4 bytes/ms são 50 ms");
}

#[test]
fn o_relogio_e_monotonico_na_divisao_irregular() {
    // 7 bytes/ms e pacotes de 5 bytes: 5/7 = 0, resto 5; 5+5=10/7 = 1,
    // resto 3. A média tem de bater ao fim de 7 pacotes.
    let mut r = RelogioMedia::novo(7);
    let mut resto = 0u32;
    for _ in 0..7 {
        r.avancar_audio(5, &mut resto);
    }
    assert_eq!(r.media_ms, 5, "35 bytes a 7 bytes/ms são 5 ms");
    assert_eq!(resto, 0, "e não sobrou resto");
}

#[test]
fn zero_bytes_nao_avanca() {
    // Um pacote de áudio vazio é um pacote perdido que chegou vazio. Não
    // é um erro, e não avança o relógio.
    let mut r = RelogioMedia::novo(96);
    let mut resto = 0u32;
    r.avancar_audio(0, &mut resto);
    assert_eq!(r.media_ms, 0);
}

#[test]
fn ms_por_frame_arredonda_para_cima_nunca_para_zero() {
    assert_eq!(RelogioMedia::ms_por_frame(30), 33, "1000/30 arredonda");
    assert_eq!(RelogioMedia::ms_por_frame(60), 16, "1000/60 arredonda");
    assert_eq!(RelogioMedia::ms_por_frame(1000), 1);
    // 2000 fps dá 0 em divisão inteira, e um frame que não avança o
    // relógio é um frame que o relógio não consegue ordenar.
    assert_eq!(RelogioMedia::ms_por_frame(2000), 1, "nunca zero");
}

#[test]
fn a_verificacao_cruza_aceita_a_tolerancia() {
    let r = RelogioMedia {
        media_ms: 1000,
        bytes_por_ms: 96,
    };
    assert!(r.verificar(1000).is_ok());
    assert!(r.verificar(1005).is_ok(), "5 ms é tolerância, e é o limite");
    assert!(r.verificar(995).is_ok(), "e o desvio negativo também");
}

#[test]
fn a_verificacao_cruza_recusa_o_resto() {
    let r = RelogioMedia {
        media_ms: 1000,
        bytes_por_ms: 96,
    };
    assert!(r.verificar(1006).is_err(), "6 ms é divergência");
    assert!(r.verificar(994).is_err(), "e do outro lado também");
    match r.verificar(1100) {
        Err(ErroMedia::RelogioDivergente {
            declarado,
            contado,
            tolerancia,
        }) => {
            assert_eq!(declarado, 1100);
            assert_eq!(contado, 1000);
            assert_eq!(tolerancia, TOLERANCIA_MS as i32);
        }
        outro => panic!("devia ser divergência: {outro:?}"),
    }
}

#[test]
fn a_regra_dos_tres_cenarios() {
    // O áudio é o mestre e o vídeo é que atrasa ou adianta.
    assert_eq!(decidir(100, 0), Decisao::Sincronizar, "iguais");
    assert_eq!(
        decidir(200, 100),
        Decisao::Sincronizar,
        "100 ms atrás: sincronia"
    );
    assert_eq!(decidir(121, 0), Decisao::Descartar, "acima do limite");
    assert_eq!(
        decidir(120, 0),
        Decisao::Sincronizar,
        "no limite, ainda sincronia"
    );
    assert_eq!(
        decidir(0, 200),
        Decisao::Apresentar,
        "o vídeo está à frente"
    );
}

/// Um passo de apresentação com a regra dos três cenários.
///
/// Existe para que os testes seguintes usem **a mesma** regra. Um teste
/// que reimplementa a regra está a testar a sua própria cópia, e as
/// cópias divergem.
///
/// ## O sentido do atraso
///
/// ```text
/// atraso = video_ms − audio_ms
/// ```
///
/// `atraso > 0` é o **vídeo à frente do áudio**, e é o que se
/// descarta: um quadro que já vai 200 ms à frente do áudio está a
/// antecipar o que o áudio ainda não disse. `atraso < 0` é o vídeo
/// atrasado, e presents-se já — que é o caso comum, porque Tor tem TCP
/// por baixo e **atrasa muito e perde pouco** (§5.5).
fn apresentar(video_ms: &mut i64, audio_ms: i64, descartes: &mut u32, apresentados: &mut u32) {
    *video_ms += RelogioMedia::ms_por_frame(30);
    match decidir(*video_ms, audio_ms) {
        Decisao::Descartar => {
            // §5.5: descartar o frame **e somar o tempo do frame** ao
            // clock de vídeo. Sem o segundo passo, o vídeo fica cada vez
            // mais à frente e cada frame seguinte também é descartado —
            // uma imagem morta em vez de uma que volta.
            *video_ms += RelogioMedia::ms_por_frame(30);
            *descartes += 1;
        }
        Decisao::Apresentar | Decisao::Sincronizar => *apresentados += 1,
    }
}

#[test]
fn um_descartado_avanca_o_relogio_de_video() {
    // O áudio não chega: os pacotes de áudio atrasam e o vídeo, que
    // continua a vir, passa à frente.
    let mut video_ms = 0i64;
    let mut descartes = 0;
    let mut apresentados = 0;

    for _ in 0..30 {
        apresentar(&mut video_ms, 0, &mut descartes, &mut apresentados);
    }

    assert!(descartes > 0, "o áudio em silêncio teve de dar descartes");
    assert_eq!(
        apresentados, 3,
        "só os primeiros 120 ms são sincronia; o resto descarta"
    );
    // E o clock de vídeo disparou para a frente, que é o que permite
    // recuperar: o descarte **consome** tempo de vídeo, não o apaga.
    assert!(
        video_ms > 1_000,
        "o relógio de vídeo não avançou com os descartes: {video_ms} ms"
    );
}

#[test]
fn o_video_recupera_quando_o_audio_chega() {
    // O cenário completo: o áudio atrasa, o vídeo descarta, e quando o
    // áudio chega em rajada o vídeo **volta a ser apresentado** sem
    // esperar por nada.
    let mut video_ms = 0i64;
    let mut audio_ms = 0i64;
    let mut descartes = 0;
    let mut apresentados = 0;

    for _ in 0..15 {
        apresentar(&mut video_ms, audio_ms, &mut descartes, &mut apresentados);
    }
    let durante = descartes;
    assert!(durante > 0, "o atraso do áudio teve de dar descartes");

    // O áudio chega: uma rajada de tudo o que estava preso na rede.
    audio_ms = video_ms;
    let antes = apresentados;
    for _ in 0..20 {
        apresentar(&mut video_ms, audio_ms, &mut descartes, &mut apresentados);
        audio_ms += RelogioMedia::ms_por_frame(30);
    }

    assert_eq!(
        descartes, durante,
        "com o áudio presente não há mais descartes: {durante} → {descartes}"
    );
    assert!(
        apresentados > antes,
        "o vídeo voltou a ser apresentado: {antes} → {apresentados}"
    );
}

#[test]
fn um_salto_de_rede_nao_mata_a_imagem() {
    // 1 s de silêncio de rede de uma vez. O áudio fica parado, o vídeo
    // descarta o que passa do limite, e a rajada seguinte repõe tudo.
    //
    // É a propriedade que faz o streaming **recuperável**, e é o que
    // §8.5 acrescenta com as keyframes: um descarte tem de ser
    // recuperável, não acumulativo.
    let mut video_ms = 0i64;
    let audio_ms = 0i64;
    let mut descartes = 0;
    let mut apresentados = 0;

    for _ in 0..60 {
        apresentar(&mut video_ms, audio_ms, &mut descartes, &mut apresentados);
    }
    assert!(
        descartes > 40,
        "um segundo de silêncio descarta quase tudo: {descartes}"
    );

    // A rajada: o áudio recupera tudo o que tinha perdido, mais
    // tempo de vídeo.
    let mut audio_recuperado = video_ms + RelogioMedia::ms_por_frame(30);
    let antes = apresentados;
    for _ in 0..20 {
        apresentar(
            &mut video_ms,
            audio_recuperado,
            &mut descartes,
            &mut apresentados,
        );
        audio_recuperado += RelogioMedia::ms_por_frame(30);
    }

    assert!(
        apresentados > antes,
        "depois do salto o vídeo nunca mais foi apresentado: {antes} → {apresentados}"
    );
}

#[test]
fn o_video_atrasado_apresenta_se_e_nao_se_ordena() {
    // O caso comum de Tor: o vídeo chega atrasado. A decisão é
    // **apresentar já**, e nunca «esperar» — dormir aqui seria
    // transformar um problema de rede num congelamento no receptor.
    assert_eq!(
        decidir(0, 500),
        Decisao::Apresentar,
        "vídeo 500 ms atrasado apresenta-se logo"
    );
    assert_eq!(
        decidir(0, 121),
        Decisao::Apresentar,
        "passado o limite, também"
    );
    assert_eq!(
        decidir(0, 120),
        Decisao::Sincronizar,
        "dentro da janela, sincroniza"
    );
}

fn a_decisao_nao_dorme() {
    // A propriedade estrutural de §5.5: decidir não pode ter um relógio
    // de parede dentro. Não há como testar isso num teste de execução, e
    // é por isso que é um teste sobre o **código**: se alguém meter um
    // `sleep` ou um `Instant` aqui dentro, a asserção lê a fonte e
    // falha.
    //
    // É a mesma técnica que o teste do `id` em `loja.rs`: o que não se
    // pode afirmar por execução afirma-se por leitura da fonte.
    let fonte = include_str!("../relogio.rs");
    let corpo = fonte
        .split("#[cfg(test)]")
        .next()
        .expect("o corpo do módulo");

    // As linhas de comentário saem antes da comparação, porque o
    // docstring **fala** de `Instant` e de `sleep` para dizer que não
    // entram. Procurar as palavras em vez das chamadas dava um teste que
    // proibia o próprio comentário que explica a regra.
    let codigo: String = corpo
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n");

    for proibido in [
        "thread::sleep",
        "sleep(",
        "Instant::now",
        "Duration",
        "SystemTime",
    ] {
        assert!(
            !codigo.contains(proibido),
            "o relógio tem `{proibido}` no código, e §5.5 diz que o clock é tempo de media"
        );
    }
}
