// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes do anel.
//!
//! As duas propriedades de §9 que o anel responde: **P9** (o anel nunca
//! ultrapassa o orçamento) e **P10** (o descarte de áudio é impossível
//! por construção).

use super::*;

/// Uma unidade de `()`, com o tempo e o `seq` que o teste precisar.
fn unidade(seq: u32, em_us: u64) -> Unidade<()> {
    Unidade {
        dados: (),
        em_us,
        seq,
    }
}

#[test]
fn um_anel_novo_vazio() {
    let a = Anel::<()>::novo();
    assert!(a.vazio());
    assert_eq!(a.quantas(), (0, 0));
    assert_eq!(a.micros_usados(), 0);
}

#[test]
fn o_audio_cabe_dentro_do_orcamento() {
    // 4 bytes/ms a 48 kHz são 96 B/ms; um pacote de 20 ms são 1920 B.
    // Com 48 000 B/s, dois segundos são 96 000 B — e um pacote são
    // 1920 us de media.
    let mut a = Anel::<()>::novo();
    for seq in 0..20 {
        assert!(a.cabe_audio(1_920), "o pacote {seq} não cabia");
        assert!(a.empurrar_audio(unidade(seq, 1_920)).is_empty());
    }
    assert_eq!(a.micros_usados(), 20 * 1_920);
}

#[test]
fn o_anel_nunca_ultrapassa_o_orcamento() {
    // P9. Encher muito mais do que a capacidade e conferir a cada passo.
    let mut a = Anel::<()>::novo();
    for seq in 0..500 {
        a.empurrar_audio(unidade(seq, 1_920));
        assert!(
            a.micros_usados() <= CAPACIDADE_US,
            "o anel chegou a {} us depois de {seq} pacotes",
            a.micros_usados()
        );
    }
    assert!(
        a.micros_usados() <= CAPACIDADE_US,
        "no fim tinha {} us",
        a.micros_usados()
    );
}

#[test]
fn o_audio_descarta_o_mais_antigo_e_nao_o_mais_recente() {
    // Descartar o áudio novo tiraria a voz a quem está a falar agora.
    // Dez pacotes de 200 ms de media cabem exactamente nos 2 s. Com
    // unidades maiores o anel já teria descartado antes da 11.ª, e o
    // teste mediria os descartes todos em vez do último.
    let mut a = Anel::<()>::novo();
    for seq in 0..10 {
        assert!(
            a.empurrar_audio(unidade(seq, 200_000)).is_empty(),
            "o pacote {seq} não devia ter descartado"
        );
    }
    assert_eq!(a.micros_usados(), CAPACIDADE_US);

    let descartes = a.empurrar_audio(unidade(10, 200_000));
    assert_eq!(descartes.len(), 1, "só um saiu para caber");
    assert_eq!(
        descartes[0].seq_perdido, 0,
        "e saiu o mais antigo, não o mais recente"
    );
    assert_eq!(a.quantas().0, 10);
}

#[test]
fn uma_unidade_maior_que_a_capacidade_nao_prende() {
    // O `while`, não o `if`. Uma unidade que cueste mais do que a
    // capacidade toda tem de sair ela própria, ou o anel ficaria acima
    // do orçamento para sempre — que é a propriedade P9 a falhar.
    let mut a = Anel::<()>::novo();
    a.empurrar_audio(unidade(0, 10_000));
    let descartes = a.empurrar_audio(unidade(1, CAPACIDADE_US * 2));

    assert_eq!(descartes.len(), 2, "saiu a antiga e a nova");
    assert!(a.micros_usados() <= CAPACIDADE_US);
    assert!(a.quantas().0 <= 1);
}

#[test]
fn o_video_descarta_a_mais_antiga() {
    let mut a = Anel::<()>::with_max_video(4);
    for seq in 0..4 {
        assert!(a.empurrar_video(unidade(seq, 1_000)).is_none());
    }

    let d = a.empurrar_video(unidade(4, 1_000)).expect("descartou");
    assert_eq!(d.seq_perdido, 0);
    assert_eq!(a.quantas().1, 4, "e o tamanho não mudou");
}

#[test]
fn o_audio_e_o_video_tem_limites_independentes() {
    // A razão de serem duas filas: um stream de áudio cheio não pode
    // fazer o vídeo ser descartado, e vice-versa.
    let mut a = Anel::<()>::with_max_video(2);
    for seq in 0..50 {
        a.empurrar_audio(unidade(seq, 50_000));
    }
    for seq in 0..2 {
        a.empurrar_video(unidade(seq, 50_000));
    }

    let (n_audio, n_video) = a.quantas();
    assert!(
        n_audio > 2,
        "o áudio não respeita o limite do vídeo: {n_audio}"
    );
    assert_eq!(n_video, 2);
}

#[test]
fn o_audio_aparece_primeiro() {
    // §5.1: o áudio é o relógio mestre. Um leitor que apresenta vídeo
    // antes do áudio que lhe corresponde produz áudio corrido.
    let mut a = Anel::<()>::novo();
    a.empurrar_video(unidade(1, 1_000));
    a.empurrar_audio(unidade(2, 1_000));

    let (genero, u) = a.seguinte().expect("há unidades");
    assert_eq!(genero, Genero::Audio);
    assert_eq!(u.seq, 2, "e é a de maior seq, porque chegou depois");

    let (genero, u) = a.seguinte().expect("há unidades");
    assert_eq!(genero, Genero::Video);
    assert_eq!(u.seq, 1);
}

#[test]
fn tirar_do_anel_liberta_o_orcamento() {
    // Sem isto, apresentar não libertaria nada e o anel descartaria
    // unidades que já foram vistas.
    let mut a = Anel::<()>::novo();
    for seq in 0..10 {
        a.empurrar_audio(unidade(seq, 300_000));
    }
    let antes = a.micros_usados();

    a.seguinte();
    assert!(
        a.micros_usados() < antes,
        "apresentar não libertou: {antes} → {}",
        a.micros_usados()
    );
}

#[test]
fn drenar_diz_o_que_se_perdeu() {
    // Uma pessoa que recebeu metade de uma chamada precisa de saber o
    // que não tem.
    let mut a = Anel::<()>::novo();
    a.empurrar_video(unidade(7, 1_000));
    a.empurrar_audio(unidade(3, 1_000));

    let perdidos = a.drenar();
    assert!(a.vazio());
    assert_eq!(a.micros_usados(), 0);
    assert_eq!(
        perdidos.iter().map(|d| d.seq_perdido).collect::<Vec<_>>(),
        vec![3, 7],
        "ordenados por seq, e não por fila"
    );
}

#[test]
fn o_descarte_traz_o_tempo_e_o_seq() {
    // §8.5 precisa dos dois: o tempo para saber há quanto tempo, o
    // `seq` para pedir a keyframe certa.
    let mut a = Anel::<()>::with_max_video(1);
    a.empurrar_video(unidade(41, 1_234_567));
    let d = a.empurrar_video(unidade(42, 2_000_000)).expect("descartou");

    assert_eq!(d.em_us, 1_234_567);
    assert_eq!(d.seq_perdido, 41);
}

#[test]
fn as_conversoes_de_tempo_arredondam_para_baixo() {
    assert_eq!(us_para_ms(999), 0, "999 us não são 1 ms");
    assert_eq!(us_para_ms(1_000), 1);
    assert_eq!(us_para_ms(1_999), 1, "o orçamento arredonda para baixo");
    assert_eq!(us_para_ms(2_000_000), 2_000);
}

#[test]
fn fps_zero_nao_divide_por_zero() {
    // O anel não depende do `ffmpeg`, e um `fps` declarado a zero
    // mataria o daemon aqui — num sítio onde ninguém espera um erro.
    assert_eq!(us_por_frame(0), 1_000_000);
    assert_eq!(us_por_frame(30), 33_333);
    assert_eq!(us_por_frame(60), 16_666);
}

#[test]
fn o_anel_nao_derrama_o_audio_a_todos() {
    // P10 não é «o áudio nunca se descarta» — isso não é verdade e não
    // pode ser. P10 é que o descarte de áudio é **impossível por
    // construção** no caminho normal, e aqui o que se prova é que
    // quem chama tem uma forma de **não** descartar.
    let mut a = Anel::<()>::novo();
    a.empurrar_audio(unidade(0, 1_900_000));

    // Com 1,9 s no anel sobram 100 ms: 100 ms cabem exactamente, e
    // 200 ms não. É a distinção entre «cheio» e «não há espaço».
    assert!(a.cabe_audio(100_000), "100 ms é exactamente o que resta");
    assert!(!a.cabe_audio(200_000));
    // E o anel não se mexeu por o facto de se ter perguntado.
    assert_eq!(a.micros_usados(), 1_900_000);
    assert_eq!(a.quantas().0, 1);
}

#[test]
fn o_video_tem_o_seu_proprio_espaco() {
    // O espelho de `cabe_audio`: perguntar antes de empurrar.
    let mut a = Anel::<()>::with_max_video(2);
    assert!(a.cabe_video());
    a.empurrar_video(unidade(0, 1_000));
    assert!(a.cabe_video());
    a.empurrar_video(unidade(1, 1_000));
    assert!(
        !a.cabe_video(),
        "a terceira não cabe, e perguntar não empurrou"
    );

    // E o limite por omissão é o de §3.4.
    assert_eq!(Anel::<()>::novo().max_video(), 21);
}
