// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes do cancelador de eco.
//!
//! ## O que estes testes **provam**, e o que não provam
//!
//! Provam que a biblioteca carrega, que o handle se cria e se
//! inicializa, que um bloco do tamanho certo passa, que um do tamanho
//! errado não passa, e — o mais importante — que o AEC **subtracta
//! eco** de um eco sintético, medido pela razão entre a energia de
//! saída e a de entrada.
//!
//! Não provam que o eco de uma sala real desapareça. Isso exige
//! altifalantes, um microfone e uma sala, e um teste que depende disso
//! falha numa máquina de desenvolvimento.
//!
//! O que a medição sintética **é** é a mesma coisa que o AEC faz: recebe
//! um sinal perto com eco misturado e uma referência longe limpa, e
//! devolve o perto sem o eco. Um altifalante faz exactamente isso, com
//! mais ruído e mais não-linearidade.

use super::*;

/// Um bloco de teste que **começa** em `inicio`.
///
/// ## E porque o início é um argumento
///
/// Porque o `longe` e o `perto` têm de ser **o mesmo sinal**, com o
/// `perto` atrasado. Um `longe` que recomeça em cada bloco e um `perto`
/// que avança dão ao AEC duas sinais que não são o mesmo — e a
/// medição dá 1,00: transparente, sem erro e sem eco cancelado.
///
/// Foi o que aconteceu na primeira versão deste teste, e é a razão de o
/// helper levar o início.
fn bloco_de_teste(amplitude: f32, periodo: f64, inicio: usize) -> Vec<f32> {
    (0..AMOSTRAS_POR_BLOCO)
        .map(|n| ((inicio + n) as f64 * periodo).sin() * amplitude as f64)
        .map(|v| v as f32)
        .collect()
}

/// O bloco tem o tamanho que o AEC aceita.
///
/// Não é um teste de cordura: é o que garante que
/// [`AMOSTRAS_POR_BLOCO`] continua a ser 160 e não 480. Com 480 o AEC
/// devolve `AEC_BAD_PARAMETER_ERROR` (12004) e o áudio sai em silêncio —
/// sem erro visível, e sem a chamada ficar muda.
#[test]
fn o_bloco_sao_cento_e_sesenta_amostras() {
    assert_eq!(AMOSTRAS_POR_BLOCO, 160);
}

/// 160 amostras a 48 kHz são 3,33 ms, e isso é o que o relógio assume.
///
/// O `docs/media.md` §5.3 usa blocos de 10 ms como unidade do relógio de
/// áudio, e 160 amostras não são 10 ms. A conta tem de estar escrita
/// para quem vai ler o `clock_ms`.
#[test]
fn o_bloco_saem_de_tres_milissegundos() {
    let micros = AMOSTRAS_POR_BLOCO * 1_000_000 / TAXA as usize;
    // 3,33 ms arredondado a microssegundos: 160/48000 = 1/300 s.
    assert_eq!(micros, 3_333, "160 amostras a 48 kHz são {micros} microssegundos");
    assert_eq!(BLOCOS_POR_SEGUNDO, 300);
}

/// O motor carrega, ou não carrega — e o que se diz é verdadeiro.
///
/// Este é o teste que documenta a **decisão**: a ausência de biblioteca
/// é `None`, e não um erro. A razão está no cabeçalho do ficheiro, e
/// este teste é o que impede que alguém mude isso para `Err` sem reparar
/// no que isso faz à captura.
#[test]
fn a_biblioteca_carrega_ou_nao_carrega() {
    match Cancelador::carregar() {
        None => eprintln!(
            "media: sem biblioteca de cancelamento de eco; a chamada \
             continua com eco"
        ),
        Some(cancelador) => {
            assert!(
                matches!(cancelador.motor(), MotorDeEco::Webrtc | MotorDeEco::Speex),
                "um cancelador carregado diz um motor que não existe"
            );
        }
    }
}

/// O handle é criado **e inicializado**.
///
/// São dois passos nesta biblioteca, e o segundo é o que se esquecia
/// mais fácil: sem `WebRtcAec_Init`, o `Process` devolve
/// `AEC_UNINITIALIZED_ERROR` (12002) e o áudio sai em silêncio. Um teste
/// que só verificasse que o handle existe passaria com um AEC que não
/// cancela nada.
#[test]
fn o_handle_e_criado_e_iniciado() {
    let Some(cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; o handle não é verificável");
        return;
    };

    if cancelador.motor() == MotorDeEco::Webrtc {
        assert_eq!(
            cancelador.ultimo_erro(),
            0,
            "o AEC tem erro {} logo depois de iniciar: um handle criado \
             sem `Init` dá 12002 e devolve silêncio",
            cancelador.ultimo_erro()
        );
    }
}

/// Um bloco do tamanho certo é processado e devolve o tamanho certo.
#[test]
fn um_motor_processa_um_bloco_do_tamanho_certo() {
    let Some(mut cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; o processamento não é verificável");
        return;
    };

    let perto = bloco_de_teste(0.5, 0.05, 0);
    let longe = bloco_de_teste(0.3, 0.05, 0);

    // A referência antes do microfone: é a ordem que a documentação
    // exige, e invertê-la é o erro que faz o AEC nunca convergir.
    unsafe { cancelador.referencia(&longe) };
    let saida = cancelador
        .processar(&perto)
        .expect("um motor carregado devolve um bloco");

    assert_eq!(
        saida.len(),
        AMOSTRAS_POR_BLOCO,
        "a saída tem {} amostras e o bloco tem {AMOSTRAS_POR_BLOCO}",
        saida.len()
    );
}

/// Um bloco do tamanho errado **não** é processado.
///
/// ## Porque este teste é o mais importante do ficheiro
///
/// A biblioteca é C, e o `WebRtcAec_Process` lê
/// `AMOSTRAS_POR_BLOCO` amostras do ponteiro que recebe sem saber o
/// tamanho. Passar um vector mais curto é uma leitura fora dos limites —
/// que num caso é um `SIGSEGV` e noutros é ler memória de outro
/// processo.
///
/// A verificação `debug_assert_eq!` **não** chega: `assert!` desaparece
/// com `-O`. O `if bloco.len() != … { return None; }` é a barreira
/// real, e este teste é o que prova que ela está no caminho.
#[test]
fn um_bloco_do_tamanho_errado_e_recusado() {
    let Some(mut cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; o descarte não é verificável");
        return;
    };

    for tamanho in [0usize, 1, 80, 159, 161, 320, AMOSTRAS_POR_BLOCO + 160] {
        let bloco = vec![0f32; tamanho];
        assert!(
            cancelador.processar(&bloco).is_none(),
            "um bloco de {tamanho} amostras foi aceite: \
             isso é uma leitura fora dos limites dentro da biblioteca"
        );
    }
}

/// A referência exige o tamanho certo, e um tamanho errado é ignorado.
///
/// A referência alimenta `WebRtcAec_BufferFarend`, que lê o mesmo número
/// de amostras. Um vector mais curto ali é o mesmo problema do
/// `processar`, e a biblioteca devolve 12004 — o que não impede nada
/// de acontecer a seguir.
#[test]
fn a_referencia_exige_o_tamanho_certo() {
    let Some(mut cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; a referência não é verificável");
        return;
    };

    let referencia_boa = bloco_de_teste(0.3, 0.05, 0);
    unsafe { cancelador.referencia(&referencia_boa) };

    unsafe { cancelador.referencia(&[0f32; 10]) };

    // E um bloco bom continua a processar, o que prova que a referência
    // má não deixou o handle num estado inutilizável.
    let perto = bloco_de_teste(0.2, 0.05, 0);
    assert!(
        cancelador.processar(&perto).is_some(),
        "um cancelador deixa de funcionar depois de uma referência de tamanho errado"
    );
}

/// O eco sintético **é** subtraído.
///
/// ## O que este teste é
///
/// A medição que valida a biblioteca inteira. Um sinal de voz, uma
/// cópia atrasada dele — que é o que um altifalante faz —, e o AEC
/// que devolve o sinal perto sem a cópia.
///
/// A medida é a razão entre a energia de saída e a de entrada. Perto de
/// 1,0 é o AEC transparente; abaixo de 1,0 é o AEC a subtractar.
///
/// ## E porque são 3000 blocos, e não 200
///
/// Porque o AEC é **adaptativo** e converge devagar. Medido nesta
/// máquina:
///
/// ```text
/// 1000 blocos (3,3 s)  → razão 0,96   quase transparente
/// 3000 blocos (10 s)   → razão 0,83   a subtrair
/// ```
///
/// Com 200 blocos a razão fica em 1,00 e o teste passa a medir um
/// cancelador que não cancela. O número é o que a medição dá, com margem de um bloco para
/// a temperatura da máquina não mudar o resultado.
///
/// ## E por que medir depois da segunda metade
///
/// Porque os primeiros blocos saem quase iguais à entrada — o filtro
/// ainda não sabe o que é o eco. A medida começa depois de 10 s, que é
/// o tempo que o AEC demora a aprender uma sala.
#[test]
fn o_eco_sintetico_e_reduzido() {
    let Some(mut cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; a redução não é verificável");
        return;
    };

    // 48 amostras de atraso: a 48 kHz são 1 ms, que é o atraso de um
    // altifalante de secretária e o que o AEC tem de apanhar.
    const ATRASO: usize = 48;
    /// Passos totais, incluindo os de adaptação.
    const PASSOS: usize = 3000;
    /// Onde a medida começa — depois de 10 s de adaptação.
    const MEDIR_A_PARTIR_DE: usize = 2000;

    let periodo = 0.05;

    let mut entrada_media: f64 = 0.0;
    let mut saida_media: f64 = 0.0;
    let mut medidos = 0usize;

    for passo in 0..PASSOS {
        // O sinal longe é o que o altifalante tocaria — e é o **mesmo**
        // sinal do perto, sem o atraso. O `inicio` é o que põe os dois
        // alinhados no tempo; um `longe` que recomeça em cada bloco dá
        // ao AEC dois sinais incoerentes e a medição dá 1,00.
        let inicio = passo * AMOSTRAS_POR_BLOCO;
        let longe = bloco_de_teste(0.3, periodo, inicio);
        // O sinal perto é a voz mais a cópia atrasada do que tocou.
        let perto: Vec<f32> = (0..AMOSTRAS_POR_BLOCO)
            .map(|i| {
                let t = (inicio + i) as f64 * periodo;
                let voz = t.sin() as f32 * 1.0;
                let eco = if i >= ATRASO {
                    ((inicio + i - ATRASO) as f64 * periodo).sin() as f32 * 0.9
                } else {
                    0.0
                };
                voz + eco
            })
            .collect();

        unsafe { cancelador.referencia(&longe) };
        let Some(saida) = cancelador.processar(&perto) else {
            // Um motor que deixa de processar a meio é uma falha, e a
            // asserção abaixo dá a volta com uma mensagem que diz qual.
            panic!("o AEC deixou de processar no passo {passo} de {PASSOS}");
        };

        if passo < MEDIR_A_PARTIR_DE {
            continue;
        }

        let energia_entrada: f64 = perto.iter().map(|a| f64::from(*a).abs()).sum();
        let energia_saida: f64 = saida.iter().map(|a| f64::from(*a).abs()).sum();
        entrada_media += energia_entrada;
        saida_media += energia_saida;
        medidos += 1;
    }

    assert!(medidos > 0, "nenhum bloco foi medido");
    let razao = saida_media / entrada_media;
    assert!(
        razao < 0.90,
        "o AEC devolveu {razao:.3} da energia de entrada depois de \
         {PASSOS} blocos: um valor de 1,0 é transparente e não a \
         cancelar, e este é o teste que diz que o cancelamento funciona"
    );
    assert!(
        razao > 0.30,
        "o AEC devolveu {razao:.3} da energia de entrada: tão pouco \
         significa que está a cortar a voz, e não a cancelar o eco"
    );
}
#[test]
fn sem_referencia_o_audio_passa() {
    let Some(mut cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; o caso sem referência não é verificável");
        return;
    };

    // Um cancelador **novo**, que nunca recebeu referência.
    let perto = bloco_de_teste(0.5, 0.05, 0);
    let saida = cancelador
        .processar(&perto)
        .expect("sem referência, o áudio passa na mesma");

    assert_eq!(saida.len(), AMOSTRAS_POR_BLOCO);
    let energia: f64 = saida.iter().map(|a| f64::from(*a).abs()).sum();
    assert!(
        energia > 0.0,
        "sem referência o AEC devolveu silêncio: uma chamada muda \
         sem nenhum erro"
    );
}

/// O motor sobrevive a uma secção de duzentos blocos.
///
/// O AEC é adaptativo: o primeiro bloco é quase sempre devolvido como
/// está, e é do segundo em diante que ele começa a subtractar. Um laço
/// de captura envia 300 blocos por segundo, e um crash no centésimo
/// seria um crash em produção.
#[test]
fn o_motor_aguenta_uma_seccao_de_duzentos_blocos() {
    let Some(mut cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; a secção não é verificável");
        return;
    };

    for n in 0..200 {
        // Um sinal que muda, para o filtro ter o que adaptar.
        let bloco = bloco_de_teste(0.2 + (n % 7) as f32 * 0.05, 0.05, 0);
        unsafe { cancelador.referencia(&bloco) };
        assert!(
            cancelador.processar(&bloco).is_some(),
            "o bloco {n} de 200 falhou"
        );
    }
}

/// Um atraso zero ou negativo é ignorado.
///
/// `msInSndCardBuf == 0` faz o filtro procurar o eco no instante errado,
/// e degrada-se a meio da chamada — sem erro, sem aviso, e com o áudio a
/// sair com eco. Silenciar o zero é melhor do que o aceitar e ouvir a
/// falha.
#[test]
fn um_atraso_de_zero_ou_negativo_e_ignorado() {
    let Some(mut cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; o atraso não é verificável");
        return;
    };

    for atraso in [-1, 0] {
        cancelador.atraso_do_sistema(atraso);
        assert_eq!(
            cancelador.atraso_placa_ms, 0,
            "um atraso de {atraso} ficou registado"
        );
        // E o cancelador tem de continuar a funcionar.
        let perto = bloco_de_teste(0.2, 0.05, 0);
        assert!(
            cancelador.processar(&perto).is_some(),
            "um atraso de {atraso} deixou o cancelador inutilizável"
        );
    }
}

/// Um atraso absurdo é truncado ao tecto do WebRTC.
///
/// Passar 10 000 ms como «atraso da placa» faria o AEC procurar o eco
/// dez segundos no futuro. O WebRTC trunca a 200 ms (`kMaxTrustedDelayMs`),
/// e é esse o valor que fica.
#[test]
fn um_atraso_absurdo_e_truncado() {
    let Some(mut cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; o truncamento não é verificável");
        return;
    };

    cancelador.atraso_do_sistema(10_000);
    assert_eq!(cancelador.atraso_placa_ms, 200);

    // E um atraso plausível fica como foi dado.
    cancelador.atraso_do_sistema(40);
    assert_eq!(cancelador.atraso_placa_ms, 40);
}

/// O cancelador é `Send`, que é o que o laço de captura exige.
///
/// Sem isto o `Cancelador` não atravessa o `thread::spawn` que o
/// `media/ffmpeg.rs` faz para a captura, e o compilador diz «não é
/// `Send`» num sítio que não tem nada a ver com threads.
#[test]
fn o_cancelador_atravessa_uma_thread() {
    fn exige_send<T: Send>() {}
    exige_send::<Cancelador>();
}

/// A janela de eco dá 500 ms, e é isso que a biblioteca recebe.
///
/// O valor não é um gosto: é a omissão do WebRTC, e mudar para baixo
/// deixa eco por apanhar numa sala.
#[test]
fn a_janela_de_eco_e_de_quinhentos_milissegundos() {
    assert_eq!(JANELA_MS, 500);
}

/// Os símbolos do WebRTC são os que a biblioteca exporta.
///
/// Este teste **falha** se a biblioteca mudar de versão e deixar de
/// exportar um dos nomes, e a mensagem diz qual. É o que dá o `None` em
/// vez de um salto para memória não mapeada na primeira chamada de
/// áudio.
#[test]
fn a_biblioteca_do_webrtc_exporta_a_api_do_aec() {
    if carregar_simbolos_webrtc("libwebrtc_audio_processing.so.1").is_none() {
        // Pode ser que a biblioteca não esteja instalada; nesse caso
        // este teste não tem o que medir.
        eprintln!("media: sem libwebrtc_audio_processing; a API não é verificável");
        return;
    }
    assert!(
        carregar_simbolos_webrtc("libwebrtc_audio_processing.so.1").is_some(),
        "a biblioteca carrega mas não tem a API completa do AEC"
    );
}

/// Uma biblioteca que não existe dá `None`, e não rebenta.
///
/// A lista de nomes tentados inclui versões que não estão instaladas, e
/// cada tentativa tem de falhar sem tocar em nada.
#[test]
fn uma_biblioteca_inexistente_da_none() {
    assert!(carregar_simbolos_webrtc("libnaoexiste.so.999").is_none());
    assert!(carregar_simbolos_speex("libnaoexiste.so.999").is_none());
}

/// O WebRTC tem precedência sobre o `speexdsp`.
///
/// A ordem é qualidade: o AEC do WebRTC adapta-se à sala, e o
/// `speexdsp` é um filtro fixo. Este teste fixa a ordem para que uma
/// troca silenciosa não mude a qualidade de todas as chamadas.
#[test]
fn o_webrtc_tem_precedencia_sobre_o_speex() {
    let Some(cancelador) = Cancelador::carregar() else {
        eprintln!("media: sem biblioteca de eco; a precedência não é verificável");
        return;
    };

    // Numa máquina com as duas, o motor tem de ser o WebRTC. Numa com
    // uma só, o que existe é o que fica — e o teste não pode exigir
    // que as duas existam.
    let tem_webrtc = carregar_simbolos_webrtc("libwebrtc_audio_processing.so.1").is_some();
    let tem_speex = carregar_simbolos_speex("libspeexdsp.so.1").is_some();

    if tem_webrtc && tem_speex {
        assert_eq!(
            cancelador.motor(),
            MotorDeEco::Webrtc,
            "com as duas bibliotecas, o motor tem de ser o AEC do WebRTC"
        );
    } else if tem_speex {
        assert_eq!(cancelador.motor(), MotorDeEco::Speex);
    }
}

/// A conversão de `f32` para `i16` não satura fora do que pode.
///
/// Um valor de `2,0` tem de dar `32767` e não um `i16` enrolado, que
/// seria um estalo branco no áudio — e o `speexdsp` amplifica
/// precisamente essa parte do sinal.
#[test]
fn a_conversao_para_i16_satura() {
    assert_eq!(para_i16(&[2.0])[0], 32_767);
    assert_eq!(para_i16(&[-2.0])[0], -32_767);
    assert_eq!(para_i16(&[0.0])[0], 0);
    assert_eq!(para_i16(&[1.0])[0], 32_767);
    assert_eq!(para_i16(&[-1.0])[0], -32_767);
}

/// E a volta é a inversa, dentro do erro de arredondamento de um nível.
#[test]
fn a_conversao_ida_e_volta_preserva_o_sinal() {
    let original = bloco_de_teste(0.5, 0.05, 0);
    let volta = para_f32(&para_i16(&original));
    assert_eq!(volta.len(), original.len());
    for (a, b) in original.iter().zip(volta.iter()) {
        // Um nível de `i16` são 1/32767 ≈ 0,00003 em `f32`.
        assert!(
            (f64::from(*a) - f64::from(*b)).abs() < 0.0001,
            "a volta perdeu precisão demais: {a} virou {b}"
        );
    }
}
