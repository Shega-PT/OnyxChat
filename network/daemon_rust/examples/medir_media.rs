// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela versão da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Mede a taxa de media, e imprime.
//!
//! ```text
//! cargo run --release --example medir_media -p onyxchatd --features docs
//! ```
//!
//! Os números de [`docs/media.md`](../../../../docs/media.md) §3.4 foram
//! escritos **antes** do encoder existir, e a tabela que lá está é
//! aritmética feita à mão. Este exemplo existe para a substituir por
//! medidas, e existe como **exemplo** e não como teste porque um teste
//! que cronometra é um teste que falha numa máquina carregada.
//!
//! O que o exemplo mede:
//!
//! * o tamanho de uma unidade para cada percentagem de alteração — que
//!   é a coluna que interessa de §3.4, e a única que a implementação
//!   podia mudar;
//! * as mensagens por segundo que o coalescer produz a 30 fps;
//! * a velocidade do delta, que é o que decide se 30 fps é possível
//!   **antes** de a rede ser considerada.
//!
//! E o que **não** mede, e é uma distinção que vale a pena: rede,
//! cifra e latência. Isto mede o que a máquina faz, e §3.4 é sobre o que
//! a máquina faz.

use std::time::Instant;

use onyxchatd::media::coalescer::Coalescer;
use onyxchatd::media::moldura::{construir, Flags, Modo, Tipo};
use onyxchatd::media::ressincronizacao;
use onyxchatd::media::telhas::{aplicar, Telhas, BYTES_POR_TELHA};

/// A geometria de §3.1: 640×480, múltiplos de 16.
const LARGURA: usize = 640;
const ALTURA: usize = 480;
const FPS: u16 = 30;

/// O limite de `p2p.rs`, em mensagens por segundo.
const LIMITE_MSG_S: f64 = 12.8;

/// Um ecrã de teste que muda `percentagem` das telhas por quadro.
///
/// O conteúdo é **gradiente** e não aleatório, porque é o que o ecrã de
/// uma pessoa é: texto, janelas, botões. Aleatório mudaria todas as
/// telhas e mediria o pior caso possível num ecrã que ninguém tem.
fn ecra(percentagem: usize, fase: u8) -> Vec<u8> {
    ecra_de(LARGURA, ALTURA, percentagem, fase)
}

/// O mesmo ecrã, noutra geometria — para medir a taxa só nas que o
/// coalescer aceita (§3.6).
fn ecra_de(largura: usize, altura: usize, percentagem: usize, fase: u8) -> Vec<u8> {
    let mut q = vec![0u8; largura * altura];
    let total_telhas = (largura / 16) * (altura / 16);
    let alteradas = total_telhas * percentagem / 100;
    // Uma janela que se move: a alteração está sempre na mesma faixa,
    // que é o que um cursor ou uma barra de deslocamento faz.
    let inicio = (fase as usize * 7) % total_telhas;
    for i in 0..alteradas.max(1) {
        let t = (inicio + i) % total_telhas;
        let (col, lin) = (t % (largura / 16), t / (largura / 16));
        let base = lin * 16 * largura + col * 16;
        for y in 0..16 {
            for x in 0..16 {
                // O `as u8` em cada operação: a conta é feita em `usize`
                // para não estourar, e cortada **só** no fim. Cortar
                // antes daria um gradiente que se repete a cada 256
                // pixels — e mediria o encoder com um ecrã de riscas.
                let v = x * 13 + y * 7 + i * 31;
                q[base + y * largura + x] = (v % 256) as u8 ^ fase;
            }
        }
    }
    q
}

/// O número de bytes de texto de uma unidade, com o `base_seq` e a moldura.
fn texto_de(corpo: &[u8]) -> usize {
    let moldura = construir(Tipo::Video, Modo::Streaming, 0, 0, 0, &[0u8; 8]);
    onyxchatd::media::base64::codificar(corpo).len() + moldura.as_bytes().len()
}

fn cabecalho(casas: &[(&str, &str)]) {
    let largura = 52;
    println!("\n{}", "═".repeat(largura));
    println!("  {}", casas[0].1);
    println!("{}", "═".repeat(largura));
    let _ = casas;
}

fn main() {
    let total_telhas = (LARGURA / 16) * (ALTURA / 16);
    println!("OnyxChat · medição de media");
    println!("  geometria     {LARGURA}×{ALTURA} cinza, {total_telhas} telhas de 16×16");
    println!("  limite p2p    {LIMITE_MSG_S} mensagens/s (128 frames / 10 s)");

    // -----------------------------------------------------------------
    cabecalho(&[(
        "taxa",
        "tamanho de uma unidade, por percentagem de alteração",
    )]);
    // -----------------------------------------------------------------
    println!(
        "  {:>4}  {:>10}  {:>10}  {:>9}  {:>9}",
        "alt", "delta B", "texto B", "msg/s", "folga"
    );

    let anterior = ecra(0, 0);
    for percentagem in [0usize, 1, 3, 10, 30, 60, 100] {
        let actual = ecra(percentagem, (percentagem + 1) as u8);
        let t = Telhas::nova(&anterior, LARGURA, ALTURA).expect("grelha");
        let delta = t.diferenca(&actual);

        let mut corpo = Vec::with_capacity(ressincronizacao::BYTES_DE_BASE + delta.len());
        corpo.extend_from_slice(&0u32.to_be_bytes());
        corpo.extend_from_slice(&delta);

        let texto = texto_de(&corpo);
        // Uma unidade por janela de 20 ms (§8.3): 30 fps ÷ 20 ms dá
        // 1,5 quadros por unidade, e portanto 1,5 mensagens/s.
        let por_segundo = FPS as f64 / 20.0;
        let folga = LIMITE_MSG_S / por_segundo;

        println!(
            "  {percentagem:>3}%  {:>10}  {texto:>10}  {por_segundo:>9.1}  {folga:>8.1}×",
            delta.len()
        );
    }

    // -----------------------------------------------------------------
    cabecalho(&[("coalescer", "mensagens por segundo a 30 fps")]);
    // -----------------------------------------------------------------
    println!(
        "  {:>6}  {:>12}  {:>12}  {:>8}",
        "telhas", "quadros/s", "msg/s", "folga"
    );

    // A 256×176, e não a 640×480: é a maior que o coalescer aceita,
    // porque uma keyframe tem de caber num envelope (§3.6). Medir a
    // taxa a 640×480 daria um número que nunca acontece em produção,
    // porque essa geometria é recusada à construção.
    const L_TAXA: usize = 256;
    const A_TAXA: usize = 176;

    for percentagem in [0usize, 3, 10, 30] {
        let inicial = ecra_de(L_TAXA, A_TAXA, percentagem, 0);
        let mut c = Coalescer::novo(&inicial, L_TAXA, A_TAXA).expect("coalescer");

        let quadros = 300usize; // 10 s a 30 fps
        for i in 0..quadros {
            c.receber(
                &ecra_de(L_TAXA, A_TAXA, percentagem, (i % 251) as u8),
                (i * 1000 / FPS as usize) as i64,
            );
        }
        c.fechar();
        let msg_s = c.esvaziar().len() as f64 / 10.0;

        println!(
            "  {percentagem:>5}%  {:>12}  {msg_s:>12.1}  {:>7.1}×",
            FPS as f64,
            LIMITE_MSG_S / msg_s
        );
    }

    // -----------------------------------------------------------------
    cabecalho(&[("delta", "velocidade de codificar um quadro")]);
    // -----------------------------------------------------------------
    let referencia = ecra(3, 0);
    let novo = ecra(3, 1);
    let t = Telhas::nova(&referencia, LARGURA, ALTURA).expect("grelha");

    // Um passo de aquecimento, que a cache de instruções conta como
    // trabalho mas não como medição.
    let _ = t.diferenca(&novo);

    let quadros = 200usize;
    let inicio = Instant::now();
    let mut total = 0usize;
    for _ in 0..quadros {
        total += t.diferenca(&novo).len();
    }
    let decorrido = inicio.elapsed();

    let ms_por_quadro = decorrido.as_secs_f64() * 1000.0 / quadros as f64;
    println!(
        "  {quadros} quadros de {LARGURA}×{ALTURA} em {:.1} ms",
        decorrido.as_secs_f64() * 1000.0
    );
    println!("  {ms_por_quadro:.3} ms por quadro, a {FPS} fps");
    println!(
        "  {:.0}× mais rápido do que os {FPS} fps, antes de cifrar",
        (1000.0 / ms_por_quadro) / FPS as f64
    );
    println!("  {} bytes de delta por quadro", total / quadros);

    // -----------------------------------------------------------------
    cabecalho(&[("aplicar", "velocidade de reconstruir no receptor")]);
    // -----------------------------------------------------------------
    let mut alvo = referencia.clone();
    let inicio = Instant::now();
    for _ in 0..quadros {
        let _ = aplicar(&t.diferenca(&novo), &mut alvo, LARGURA);
    }
    let decorrido = inicio.elapsed();
    let ms_por_quadro = decorrido.as_secs_f64() * 1000.0 / quadros as f64;
    println!("  {ms_por_quadro:.3} ms por quadro reconstruído");
    println!(
        "  {:.0}× mais rápido do que os {FPS} fps",
        (1000.0 / ms_por_quadro) / FPS as f64
    );
    let _ = BYTES_POR_TELHA;
    let _ = Flags::VAZIO;
    println!("\n  notado: isto mede a máquina, e não a rede. §3.4 é sobre a máquina.");
}
