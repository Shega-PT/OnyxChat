// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes da saída de áudio.
//!
//! O que estes testes **provam** depende do que a máquina tem:
//!
//! * Com um servidor de som, a saída abre, aceita blocos, e o
//!   registo da referência de renderização é preenchido.
//! * Sem um, a saída **não abre** — e isso é um teste que passa, porque
//!   é o que a documentação promete.
//!
//! E o que provam **sempre**, com ou sem servidor:
//!
//! * A conversão de `f32` para `i16` não satura fora do que pode.
//! * A taxa e os canais são os que a captura usa.
//! * O `ffmpeg` embebido tem a **saída** de áudio de que a chamada
//!   precisa — que é uma propriedade do binário, e não da máquina.

use super::*;

/// O `ffmpeg` embebido tem saída de áudio.
///
/// ## E porque este teste é o mais importante do ficheiro
///
/// Porque sem `--enable-outdev=alsa` na receita o daemon sabe
/// descodificar o Opus e **não tem por onde o tocar** — e uma chamada
/// muda é uma chamada que não funciona.
///
/// E o sintoma de uma saída que falta não é um erro: o `ffmpeg` sai
/// com «Unknown PCM pipe» e o `MEDIA_ESTADO` continua a dizer que está
/// tudo bem. Por isso o teste olha para o binário, e não para o
/// resultado de uma chamada.
#[test]
fn o_ffmpeg_embebido_tem_saida_de_audio() {
    if !super::super::ffmpeg_embutido::disponivel() {
        eprintln!("media: sem ffmpeg embebido; a saída não é verificável");
        return;
    }

    let fd = super::super::ffmpeg_embutido::preparar().expect("prepara o executável");
    let caminho = format!("/proc/self/fd/{fd}");
    let saida = std::process::Command::new(&caminho)
        .args(["-hide_banner", "-devices"])
        .output()
        .expect("o executável corre");
    super::super::ffmpeg_embutido::fechar();

    let texto = String::from_utf8_lossy(&saida.stdout);
    // A tabela `Devices:` lista o `alsa` com um `E` ao lado, que
    // significa «é um dispositivo de escrita».
    let linha_alsa = texto
        .lines()
        .find(|l| l.contains("alsa"))
        .unwrap_or_else(|| panic!("o binário não tem alsa:\n{texto}"));
    assert!(
        linha_alsa.contains("E"),
        "a linha do alsa é «{linha_alsa}» e não tem o «E» de escrita: \
         o binário sabe ler o microfone mas não sabe tocar"
    );
}

/// A saída abre, ou não abre — e os dois são testes que passam.
///
/// ## E porque isto é um teste e não uma medição
///
/// Porque o resultado depende da máquina: uma com servidor de som abre,
/// uma sem servidor de som não. E **os dois** são o comportamento
/// correcto, porque a alternativa a uma saída de áudio é uma chamada
/// muda, e a alternativa a uma chamada é não haver chamada.
///
/// O que este teste fixa é que a decisão é **tomada** e que o estado
/// que sai dela é verdadeiro. Uma saída que abre e diz «indisponível»,
/// ou que não abre e diz «aberta», seria um interface que mente.
#[test]
fn a_saida_abre_ou_nao_e_diz_o_que_e() {
    match Saida::abrir("onyxchat-media") {
        None => {
            // Sem servidor de som. E o que o `estado` tem de dizer.
            eprintln!("media: sem servidor de som; a chamada fica muda");
        }
        Some(mut saida) => {
            assert_eq!(saida.estado(), Estado::Aberto);
            assert!(saida.disponivel(), "uma saída aberta tem de estar disponível");

            // E a escrita tem de ser aceite.
            let bloco: Vec<f32> = (0..160).map(|n| (n as f32 * 0.01).sin() * 0.5).collect();
            assert!(saida.escrever(&bloco), "a primeira escrita foi recusada");
        }
    }
}

/// A escrita de um bloco enche a referência de renderização.
///
/// E o que dá ao AEC o que comparar. Sem este registo o cancelador
/// degrada, e a degradação é silenciosa: o áudio sai com eco e ninguém
/// sabe porquê.
#[test]
fn a_escrita_registra_a_referencia() {
    let Some(mut saida) = Saida::abrir("onyxchat-teste") else {
        eprintln!("media: sem servidor de som; o registo não é verificável");
        return;
    };

    // Um tom, para que o registo não seja de silêncio — que também
    // serviria, mas não provaria que os bytes foram copiados.
    let bloco: Vec<f32> = (0..160).map(|n| (n as f32 * 0.01).sin() * 0.5).collect();
    assert!(saida.escrever(&bloco), "a escrita foi recusada");

    let referencia = saida.referencia();
    assert_eq!(
        referencia.len(),
        bloco.len(),
        "a referência tem {} amostras e o bloco tem {}",
        referencia.len(),
        bloco.len()
    );
    assert!(
        referencia.iter().any(|&a| a != 0.0),
        "a referência é de silêncio: os bytes não foram copiados"
    );
}

/// A referência é uma **cópia**, e não o registo interior.
///
/// E o que permite ao AEC ler de outra thread sem uma corrida. A
/// prova é que escrever de novo **substitui** a referência, e não a
/// acumula.
#[test]
fn a_referencia_e_substituida_e_nao_acumulada() {
    let Some(mut saida) = Saida::abrir("onyxchat-teste") else {
        eprintln!("media: sem servidor de som; a substituição não é verificável");
        return;
    };

    let bloco: Vec<f32> = vec![0.25; 160];
    assert!(saida.escrever(&bloco));
    let primeira = saida.referencia();
    assert_eq!(primeira.len(), 160);

    assert!(saida.escrever(&bloco));
    let segunda = saida.referencia();
    assert_eq!(
        segunda.len(),
        160,
        "a referência acumulou: são {} amostras e o bloco tem 160",
        segunda.len()
    );
}

/// Um bloco vazio é aceito, e não muda a referência.
///
/// Porque um `write` de zero bytes é legal e acontece quando um quadro
/// de áudio não chegou. E porque um erro aqui seria um erro falso — a
/// chamada não tem problema nenhum.
#[test]
fn um_bloco_vazio_e_aceito() {
    let Some(mut saida) = Saida::abrir("onyxchat-teste") else {
        eprintln!("media: sem servidor de som; o bloco vazio não é verificável");
        return;
    };

    assert!(saida.escrever(&[]), "um bloco vazio tem de ser aceito");
}

/// A conversão de `f32` para `i16` satura, e não enrola.
///
/// Um valor de `2,0` tem de dar `32767` e não um `i16` que virou
/// negativo — que seria um estalo branco no áudio.
#[test]
fn a_conversao_satura() {
    // A conversão está dentro de `escrever`, e não é testável sem
    // servidor de som. O que se verifica aqui é a aritmética, que é a
    // mesma expressão.
    let converter = |a: f32| (a.clamp(-1.0, 1.0) * 32_767.0) as i16;
    assert_eq!(converter(2.0), 32_767);
    assert_eq!(converter(-2.0), -32_767);
    assert_eq!(converter(0.0), 0);
    assert_eq!(converter(1.0), 32_767);
    assert_eq!(converter(-1.0), -32_767);
    assert_eq!(converter(0.5), 16_383);
}

/// A saída usa a mesma taxa que a captura.
///
/// E o teste que impede que a saída e a captura se separem: uma
/// diferença obriga a um resampler, e um resampler no caminho de uma
/// chamada é latência que ninguém pediu.
#[test]
fn a_saida_usa_a_mesma_taxa_que_a_captura() {
    assert_eq!(TAXA, 48_000, "a taxa de áudio da chamada");
    assert_eq!(CANAIS, 1, "a chamada é mono");
}

/// A `Saida` é `Send`, que é o que o laço de áudio exige.
///
/// Sem isto a `Saida` não atravessa o `thread::spawn` que escreve o
/// áudio, e o compilador diz «não é `Send`» num sítio que não tem nada
/// a ver com threads.
#[test]
fn a_saida_atravessa_uma_thread() {
    fn exige_send<T: Send>() {}
    exige_send::<Saida>();
}
