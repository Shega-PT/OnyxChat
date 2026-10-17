// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes de `media/desempate.rs`.

use super::*;

/// Simula uma chamada: 30 fps de vídeo, áudio a 20 ms, com rajadas.
///
/// ## Porque com rajadas
///
/// O `ffmpeg` escreve o áudio e o vídeo por descritores separados e
/// não os acopla. O áudio chega em blocos irregulares — uma rajada de
/// 100 ms, depois silêncio, depois outra. Uma simulação que entregasse
/// uma unidade de áudio por quadro de vídeo estaria a testar um
/// `ffmpeg` que não existe.
///
/// E é a rajada que faz o trabalho: sem limitador, o áudio de uma
/// rajada entra todo antes de o vídeo da rajada correspondente, e o
/// receptor descarta.
fn chamada(segundos: i64, fps: u32, rajada_ms: i64) -> (usize, usize) {
    let mut d = DesempateAudioVideo::novo();
    let ms_por_quadro = 1000 / fps as i64;
    let ms_audio = 20;
    let quadros = (segundos * 1000 / ms_por_quadro) as usize;
    // `unidades_audio` em **ms**: é o tempo total de áudio da chamada,
    // e é com ele que o laço compara.
    let unidades_audio: usize = ((segundos * 1000) / ms_audio) as usize;

    let mut emitidos = 0usize;
    let mut ms_video = 0i64;
    let mut proximo_audio = 0i64;

    for _ in 0..quadros {
        ms_video += ms_por_quadro;
        d.registar_video(ms_por_quadro);

        // O áudio tem o seu próprio ritmo: a cada `rajada_ms` de tempo
        // de vídeo chega a rajada de áudio correspondente.
        // O limite é em **ms**, como `proximo_audio`. A primeira
        // versão desta linha comparava `proximo_audio` (ms) com
        // `unidades_audio` (unidades), e o `while` parava aos 500 ms
        // em vez dos 10 000 — o que fazia o teste concluir que o
        // limitador emitia 25 de 500. O limitador estava certo; a
        // guarda é que media duas coisas diferentes.
        let total_audio_ms = unidades_audio as i64 * ms_audio;
        while proximo_audio <= ms_video && proximo_audio < total_audio_ms {
            let ate = (proximo_audio + rajada_ms).min(total_audio_ms);
            while proximo_audio < ate {
                if d.registar_audio(ms_audio) == Desempate::Emitir {
                    emitidos += 1;
                }
                proximo_audio += ms_audio;
            }
        }
    }
    (emitidos, unidades_audio)
}

#[test]
fn a_rajada_de_audio_nao_passa_da_janela() {
    // ## O que este teste diz, e o que ele NÃO diz
    //
    // Nao diz que o audio chega "1,67x mais rapido" que o video. Nao
    // diz, porque nao e verdade: os dois duram o mesmo tempo, e em 10 s
    // o video oferece 10 000 ms e o audio tambem. Uma versao anterior
    // deste ficheiro comparava 20 ms de audio com 33 ms de video como
    // se um fosse contagem e o outro tempo, e tirava dai que o receptor
    // descartava 4 em cada 5 amostras. Nao descarta: com o limitador,
    // este teste emite as 500 de 500.
    //
    // O que o teste diz e' outra coisa, e e' a que importa: uma rajada
    // de audio nao pode empurrar o audio para alem da janela, mesmo
    // quando o video ainda nao a companionhou. E o que impede que o
    // receptor encha o buffer e descarte.
    let mut d = DesempateAudioVideo::novo();
    // Uma rajada de 400 ms de audio contra 1 quadro de video.
    d.registar_video(33);
    let mut emitidas = 0usize;
    for _ in 0..20 {
        if d.registar_audio(20) == Desempate::Emitir {
            emitidas += 1;
        }
    }
    // A rajada toda e' 400 ms de audio; so cabe o que a janela deixa.
    assert!(
        emitidas < 20,
        "a rajada passou inteira: {emitidas} de 20, e o audio vai {} ms a frente",
        d.desvio_ms()
    );
    assert_eq!(
        emitidas,
        ((33 + d.janela_ms()) / 20) as usize,
        "a rajada encheu a janela e parou, como deve ser"
    );
}

#[test]
fn o_audio_acompanha_o_video_dentro_da_tolerancia() {
    // Ao fim de tudo, o áudio não pode ficar muito à frente do vídeo.
    // Esta é a regra `x` para `x` verificada.
    let mut d = DesempateAudioVideo::novo();
    let ms_por_quadro = 33;
    for _ in 0..30 {
        d.registar_video(ms_por_quadro);
    }
    for _ in 0..50 {
        d.registar_audio(20);
    }
    // A janela é `tolerancia_ms`; um limitador que deixa passar muito
    // mais do que isso não está a limitar nada.
    assert!(
        d.desvio_ms() <= d.janela_ms(),
        "desvio de {} ms acima da tolerância de {}",
        d.desvio_ms(),
        d.janela_ms()
    );
}

#[test]
fn o_audio_atrasado_e_sempre_emitido() {
    // O caso inverso, e o que a de cima esconde: se o vídeo não chegou
    // ainda, o áudio tem de sair na mesma. Uma chamada cujo áudio se
    // segura à espera de vídeo que nunca chega é uma chamada muda.
    let mut d = DesempateAudioVideo::novo();
    for _ in 0..5 {
        assert_eq!(d.registar_audio(20), Desempate::Emitir);
    }
    assert!(d.desvio_ms() > 0, "o áudio está adiantado, e deve sair");
}

#[test]
fn o_equilibrio_nao_prende_a_chamada() {
    // O caso exacto: áudio exactamente na janela. `>` e não `>=`, senão
    // o limitador rejeita o equilíbrio e a chamada pára.
    let mut d = DesempateAudioVideo::com_janela(200);
    d.registar_video(100);
    // O vídeo já viu 100 ms, a janela são 200, e o limite de áudio são
    // 100 + 200 = 300 ms.
    //
    // A comparação é `>`, não `>=`: o caso exacto — áudio a 300 ms
    // exactamente — é o estado de equilíbrio, e um limitador que
    // reprova o equilíbrio vê a chamada parar.
    //
    // 14 unidades dão 280 ms de áudio, 180 ms de desvio.
    for _ in 0..14 {
        assert_eq!(d.registar_audio(20), Desempate::Emitir);
    }
    assert_eq!(d.desvio_ms(), 180, "280 de áudio contra 100 de vídeo");

    // A 15ª leva o áudio a **exactamente** 300 ms, o limite. E é aqui
    // que a comparação ser `>` e não `>=` se vê: com `>=` esta unidade
    // seria recusada, a 16ª também, e o áudio ficaria preso em 280 ms
    // enquanto o vídeo não avançasse — uma chamada que cala a voz.
    assert_eq!(
        d.registar_audio(20),
        Desempate::Emitir,
        "o equilíbrio tem de emitir: é o estado normal, não uma excepção"
    );
    assert_eq!(d.desvio_ms(), 200);
    // A 16ª leva a 320 ms: aí sim, espera pelo vídeo.
    assert_eq!(d.registar_audio(20), Desempate::Esperar);
}

#[test]
fn repor_e_necessario_ou_o_audio_fica_mudo() {
    // Um limitador com o áudio muito adiantado rejeita tudo até o tempo
    // apanhar. Sem `repor` no reinício do stream, a segunda chamada
    // fica muda.
    let mut d = DesempateAudioVideo::novo();
    for _ in 0..40 {
        d.registar_audio(20);
    }
    assert!(d.desvio_ms() >= 0);

    d.repor();
    assert_eq!(d.desvio_ms(), 0);
    assert_eq!(d.registar_audio(20), Desempate::Emitir);
}

#[test]
fn o_clock_ms_que_volta_nao_faz_a_janela_crescer() {
    // Um `clock_ms` menor é o emissor a recomeçar. Somar na mesma daria
    // uma janela que cresce sem parar, e o áudio parava de sair.
    let mut d = DesempateAudioVideo::novo();
    for _ in 0..30 {
        d.registar_video(33);
    }
    for _ in 0..20 {
        d.registar_audio(20);
    }
    assert!(
        d.desvio_ms() < 0,
        "o áudio devia estar atrasado depois de 30 quadros de vídeo e 20 de áudio: {}",
        d.desvio_ms()
    );

    // O `repor` é o que trata do recomeço, e é explícito: quem reinicia
    // o stream chama-o. Sem ele o áudio atrasado de antes ficava a
    // atrasar-se, e as unidades seguintes eram todas recusadas.
    d.repor();
    assert_eq!(d.desvio_ms(), 0);
    assert_eq!(
        d.registar_audio(20),
        Desempate::Emitir,
        "depois de repor, o áudio volta a sair"
    );
}

#[test]
fn a_janela_comeca_em_200_ms() {
    // A janela de jitter de uma chamada normal.
    assert_eq!(DesempateAudioVideo::novo().janela_ms(), 200);
}
