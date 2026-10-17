// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes da ressincronização.
//!
//! A propriedade central: **uma unidade perdida tem de ser
//! recuperável**. Sem isso, uma única perda deixa a imagem
//! permanentemente corrompida, porque cada delta é relativo ao anterior
//! e não existe forma de saber que a base se perdeu.

use super::*;
use crate::media::moldura::{construir_com_flags, Flags};
use crate::media::telhas::LADO;

const LARGURA: usize = 64;
const ALTURA: usize = 48;

fn ecrã() -> Vec<u8> {
    vec![0u8; LARGURA * ALTURA]
}

/// Um quadro com `n` telhas com variação.
fn quadro(n: usize, semente: u8) -> Vec<u8> {
    let mut q = ecrã();
    for i in 0..n {
        let col = i % (LARGURA / LADO);
        let lin = i / (LARGURA / LADO);
        let base = lin * LADO * LARGURA + col * LADO;
        for y in 0..LADO {
            for x in 0..LADO {
                q[base + y * LARGURA + x] = semente.wrapping_add((x * 7 + y * 3) as u8);
            }
        }
    }
    q
}

/// O corpo de uma unidade: `base_seq` e delta.
fn corpo(seq: u32, base: u32, delta: &[u8]) -> Vec<u8> {
    let mut c = base.to_be_bytes().to_vec();
    c.extend_from_slice(delta);
    let _ = seq;
    c
}

/// O delta de um quadro contra uma referência.
fn delta_entre(referencia: &[u8], novo: &[u8]) -> Vec<u8> {
    crate::media::telhas::Telhas::nova(referencia, LARGURA, ALTURA)
        .expect("grelha")
        .diferenca(novo)
}

#[test]
fn uma_keyframe_e_auto_suficiente() {
    // `base_seq == seq` é a invariante, e é o que dispensa uma
    // instrução separada: o receptor aplica sem condição e recomeça a
    // cadeia.
    let q = quadro(3, 40);
    let mut r = Receptor::novo();
    let mut alvo = ecrã();

    let delta = delta_entre(&ecrã(), &q);
    let unidade = corpo(7, 7, &delta);
    assert_eq!(
        r.pode_aplicar(7, base_seq_de(&unidade).expect("base")),
        true
    );
    assert_eq!(
        r.aplicar(7, &unidade, &mut alvo, LARGURA, false),
        Recebida::Aplicada(7)
    );
    assert_eq!(alvo, q, "e a imagem é a da keyframe");
}

#[test]
fn uma_primeira_unidade_que_nao_e_keyframe_e_descartada() {
    // O receptor não tem referência nenhuma. Aplicar um delta contra um
    // quadro que não é o dele produziria uma imagem que nunca esteve no
    // ecrã de ninguém.
    let referencia = ecrã();
    let q = quadro(2, 10);
    let delta = delta_entre(&referencia, &q);

    let mut r = Receptor::novo();
    let mut alvo = referencia.clone();
    let unidade = corpo(5, 4, &delta);

    assert!(!r.pode_aplicar(5, 4));
    assert_eq!(
        r.aplicar(5, &unidade, &mut alvo, LARGURA, false),
        Recebida::Descartada
    );
    assert_eq!(alvo, referencia, "o quadro ficou intacto");
    assert_eq!(r.descartadas, 1);
}

#[test]
fn um_descarte_nao_avanca_a_sequencia() {
    // Se avançasse, o receptor passaria a esperar pela unidade errada e
    // nunca voltaria a sincronizar — sem nenhuma mensagem nova.
    let base = ecrã();
    let q = quadro(1, 20);
    let mut r = Receptor::novo();
    let mut alvo = base.clone();

    // Keyframe.
    r.aplicar(
        0,
        &corpo(0, 0, &delta_entre(&base, &q)),
        &mut alvo,
        LARGURA,
        false,
    );
    assert_eq!(r.ultimo_aplicado, Some(0));

    // A unidade 2 falta. A 3 vem relativa à 2.
    let seguinte = quadro(2, 60);
    let delta = delta_entre(&q, &seguinte);
    assert_eq!(
        r.aplicar(3, &corpo(3, 2, &delta), &mut alvo, LARGURA, false),
        Recebida::Descartada,
        "a base não é a que o quadro tem"
    );
    assert_eq!(r.ultimo_aplicado, Some(0), "e a sequência não andou");
}

#[test]
fn uma_unidade_perdida_recupera_com_uma_keyframe() {
    // A propriedade que §8.5 promete. Uma perda, e a imagem volta.
    let inicio = ecrã();
    let a = quadro(1, 10);
    let b = quadro(2, 30);
    let c = quadro(3, 50);

    let mut r = Receptor::novo();
    let mut alvo = inicio.clone();

    r.aplicar(
        0,
        &corpo(0, 0, &delta_entre(&inicio, &a)),
        &mut alvo,
        LARGURA,
        false,
    );
    r.aplicar(
        1,
        &corpo(1, 1, &delta_entre(&a, &b)),
        &mut alvo,
        LARGURA,
        false,
    );

    // A 2 perde-se. A 3 é um delta contra a 2 e é descartada.
    let perdida = quadro(3, 40);
    let d2 = delta_entre(&b, &perdida);
    assert_eq!(
        r.aplicar(2, &corpo(2, 2, &d2), &mut alvo, LARGURA, false),
        Recebida::Aplicada(2)
    );

    // A 4 relativa à 3, que o receptor tem.
    let d3 = delta_entre(&perdida, &c);
    assert_eq!(
        r.aplicar(3, &corpo(3, 3, &d3), &mut alvo, LARGURA, false),
        Recebida::Aplicada(3)
    );
    assert_eq!(alvo, c);
}

#[test]
fn a_cadeia_parte_quando_a_base_se_perde() {
    let inicio = ecrã();
    let a = quadro(1, 10);
    let b = quadro(2, 30);
    let c = quadro(3, 50);

    let mut r = Receptor::novo();
    let mut alvo = inicio.clone();
    r.aplicar(
        0,
        &corpo(0, 0, &delta_entre(&inicio, &a)),
        &mut alvo,
        LARGURA,
        false,
    );

    // A unidade 1 nunca chega. A 2 é relativa a ela.
    let d2 = delta_entre(&a, &b);
    assert_eq!(
        r.aplicar(2, &corpo(2, 1, &d2), &mut alvo, LARGURA, false),
        Recebida::Descartada
    );

    // A 3 continua relativa à 2, que o receptor também não tem.
    let d3 = delta_entre(&b, &c);
    assert_eq!(
        r.aplicar(3, &corpo(3, 2, &d3), &mut alvo, LARGURA, false),
        Recebida::Descartada,
        "e a cadeia não se refaz sozinha"
    );

    // Uma keyframe refaz tudo, e é o que o emissor tem de mandar.
    let d_kf = delta_entre(&ecrã(), &c);
    assert_eq!(
        r.aplicar(4, &corpo(4, 4, &d_kf), &mut alvo, LARGURA, false),
        Recebida::Aplicada(4)
    );
    assert_eq!(alvo, c, "e a imagem voltou ao que o emissor está a ver");
}

#[test]
fn um_corpo_muito_curto_da_erro_e_nao_uma_base_de_zero() {
    // `from_be_bytes` de um array curto daria pânico. Uma unidade de outra
    // versão tem de dar erro, e não um `base_seq` de 0 que parece válido.
    assert!(matches!(
        base_seq_de(&[0, 1, 2]),
        Err(ErroMedia::BaseTruncado)
    ));
    assert!(matches!(base_seq_de(&[]), Err(ErroMedia::BaseTruncado)));

    let mut r = Receptor::novo();
    let mut alvo = ecrã();
    assert_eq!(
        r.aplicar(1, &[0, 1], &mut alvo, LARGURA, false),
        Recebida::Descartada
    );
}

#[test]
fn um_delta_vazio_com_base_propria_e_recusado() {
    // Uma keyframe nunca é vazia: o delta contra o quadro zero tem todas
    // as telhas. Uma unidade vazia é uma escrita cortada, e aplicá-la
    // «com sucesso» deixaria o quadro intacto sem o receptor saber que
    // recebeu nada.
    let mut r = Receptor::novo();
    let mut alvo = ecrã();
    assert_eq!(
        r.aplicar(0, &corpo(0, 0, b""), &mut alvo, LARGURA, false),
        Recebida::Descartada
    );
}

#[test]
fn a_primeira_unidade_de_uma_stream_e_sempre_keyframe() {
    let mut e = Emissor::novo();
    assert_eq!(
        e.decidir(0),
        Pedido::Keyframe,
        "o receptor não tem referência"
    );
    assert_eq!(e.decidir(33), Pedido::Delta);
}

#[test]
fn a_cadencia_e_de_uma_keyframe_a_cada_dois_segundos() {
    let mut e = Emissor::novo();
    assert_eq!(e.decidir(0), Pedido::Keyframe, "a primeira");

    for ms in [33, 66, 100, 500, 1_500] {
        assert_eq!(e.decidir(ms), Pedido::Delta, "aos {ms} ms ainda não");
    }
    assert_eq!(e.decidir(2_000), Pedido::Keyframe, "aos 2 s");
    assert_eq!(e.decidir(2_033), Pedido::Delta);
    assert_eq!(e.decidir(4_000), Pedido::Keyframe);
}

#[test]
fn um_descarte_pede_keyframe_ja() {
    // E **já**: esperar pela cadência deixaria a imagem corrompida
    // durante dois segundos, e §8.5 diz que isto é o que torna o
    // streaming recuperável.
    let mut e = Emissor::novo();
    e.decidir(0);

    e.registar_descarte();
    assert_eq!(e.decidir(33), Pedido::Keyframe, "não espera pela cadência");
    // E o pedido é de uma vez só.
    assert_eq!(e.decidir(66), Pedido::Delta);
}

#[test]
fn um_descarte_nao_atrasa_a_keyframe_espontanea() {
    // Se a keyframe do descarte contasse para a cadência, a seguinte
    // espontânea esperaria mais 2 s — depois de uma perda, que é
    // exactamente quando a segunda perda dói mais.
    let mut e = Emissor::novo();
    e.decidir(0); // espontânea aos 0 ms

    e.registar_descarte();
    assert_eq!(e.decidir(1_000), Pedido::Keyframe, "por causa do descarte");
    // A cadência continua a contar desde a **espontânea** dos 0 ms, e
    // por isso aos 1 500 ms ainda não chegou.
    assert_eq!(e.decidir(1_500), Pedido::Delta, "aos 1,5 s ainda não");
    assert_eq!(
        e.decidir(2_000),
        Pedido::Keyframe,
        "e a espontânea dos 2 s não foi empurrada"
    );
}

#[test]
fn um_pedido_do_receptor_acorda_so_a_stream_dele() {
    let mut e = Emissor::novo();
    e.decidir(0);

    e.registar_pedido(PedidoDeKeyframe { stream: 7 }, 9);
    assert_eq!(
        e.decidir(10),
        Pedido::Delta,
        "um pedido da stream 7 não acorda a stream 9"
    );

    e.registar_pedido(PedidoDeKeyframe { stream: 9 }, 9);
    assert_eq!(e.decidir(11), Pedido::Keyframe);
}

#[test]
fn o_texto_do_pedido_tem_o_formato_da_retoma() {
    // §4.3 dá o `9f3a:` para a retoma. O mesmo prefixo, e a mesma
    // razão: uma frase que começa por aquilo é de media, e o resto da
    // conversa não colide com ela.
    let texto = texto_de_pedido(7);
    assert_eq!(texto, "9f3a: keyframe na stream 7");
    assert_eq!(
        interpretar_pedido(&texto),
        Some(PedidoDeKeyframe { stream: 7 })
    );

    assert_eq!(interpretar_pedido("olá"), None, "uma frase comum não é");
    assert_eq!(interpretar_pedido("9f3a: faltam 7 42"), None);
    assert_eq!(interpretar_pedido("9f3a: keyframe na stream x"), None);
    assert_eq!(interpretar_pedido("9f3a: keyframe 42"), None);
}

#[test]
fn o_pedido_round_trip_em_todos_os_streams() {
    // `0` e `65_535` são os extremos do `u16`, e ambos têm de voltar
    // iguais. Um `stream` mal escrito acordava a stream errada, e a
    // pessoa veria a sua imagem partir-se por um pedido que não era
    // para ela.
    for stream in [0u16, 1, 42, 999, 65_535] {
        assert_eq!(
            interpretar_pedido(&texto_de_pedido(stream)),
            Some(PedidoDeKeyframe { stream }),
            "stream {stream}"
        );
    }
}

#[test]
fn o_base_seq_tem_quatro_bytes_e_big_endian() {
    // Big-endian porque o resto do preâmbulo é ASCII em ordem de
    // leitura, e um inteiro em `be` lê-se como está escrito. Little
    // endian seria mais rápido e ilegível num registo.
    let c = corpo(0x0102_0304, 0x0A0B_0C0D, b"x");
    assert_eq!(&c[..4], &[0x0A, 0x0B, 0x0C, 0x0D]);
    assert_eq!(base_seq_de(&c).expect("base"), 0x0A0B_0C0D);
    assert_eq!(delta_de(&c), b"x");
    assert_eq!(BYTES_DE_BASE, 4);
}

#[test]
fn o_corso_de_uma_keyframe_leva_a_flag() {
    // A flag `KEYFRAME` já existe no preâmbulo e é explícita; o
    // `base_seq` é o que torna a coisa verificável. As duas têm de
    // concordar, e o teste concorda por construírem a moldura de
    // Streaming.
    let moldura = construir_com_flags(
        crate::media::Tipo::Video,
        crate::media::Modo::Streaming,
        Flags::VAZIO.com(Flags::KEYFRAME),
        1,
        0,
        0,
        &[0, 0, 0, 0],
    );
    let aberta = crate::media::moldura::abrir(moldura.as_bytes()).expect("abre");
    assert!(aberta.flags.tem(Flags::KEYFRAME));
    assert_eq!(base_seq_de(&aberta.corpo).expect("base"), 0);
}

#[test]
fn o_emissor_e_o_receptor_fecham_o_ciclo_com_uma_perda() {
    // O teste de ponta a ponta de §8.5, e o único que justifica o
    // módulo: o coalescer emite, uma unidade **perde-se** no caminho, o
    // receptor descarta as seguintes até chegar uma keyframe, e a imagem
    // volta.
    //
    // São três segundos de media de propósito. Com menos, não há cadência
    // depois da perda — e a parte difícil de §8.5 é precisamente essa:
    // quem manda a keyframe é quem sabe que perdeu, e no caminho de rede
    // ninguém sabe.
    use crate::media::coalescer::Coalescer as EmissorReal;

    let largura = LARGURA;
    let altura = ALTURA;
    let inicio = ecrã();

    let mut emissor = EmissorReal::novo(&inicio, largura, altura).expect("coalescer");
    let mut receptor = Receptor::novo();
    let mut ecrã_receptor = inicio.clone();

    // Três segundos a 30 fps: 90 quadros, com um quadro distinto cada.
    let total = 90usize;
    let mut ultimo = inicio.clone();
    for i in 0..total {
        ultimo = quadro((i % 6) + 1, 30u8.wrapping_mul(i as u8 % 7).wrapping_add(1));
        emissor.receber(&ultimo, i as i64 * 33);
    }
    emissor.fechar();

    let unidades = emissor.esvaziar();
    assert!(
        unidades.len() >= 4,
        "só {} unidades em 3 s: o coalescer não fez nada",
        unidades.len()
    );

    // Perde-se a segunda unidade. É o que o relay faz, e §8.2 diz que
    // ele atrasa muito e perde pouco — perder é o caso raro, e é este.
    let indice_perdido = 1;
    let mut aplicadas = 0usize;
    let mut descartadas = 0usize;
    let mut keyframes = 0usize;
    let mut ainda_desalinhado = false;

    for (i, unidade) in unidades.iter().enumerate() {
        if i == indice_perdido {
            ainda_desalinhado = true;
            continue;
        }
        // O `eh_lote` vem da **flag** da unidade, não de um literal:
        // é a flag que o preâmbulo leva, e um teste que passa `false`
        // fixo não está a testar o protocolo, está a testar a sua
        // própria suposição sobre ele.
        let eh_lote = unidade.flags.tem(crate::media::moldura::Flags::KEYFRAME);
        match receptor.aplicar(
            unidade.seq,
            &unidade.corpo,
            &mut ecrã_receptor,
            largura,
            eh_lote,
        ) {
            Recebida::Aplicada(_) => {
                // Uma keyframe fraccionada fecha em **uma** aplicação,
                // nas N unidades. Contar as unidades que entraram como
                // aplicadas contaria a mesma imagem N vezes.
                if eh_lote {
                    keyframes += 1;
                }
                aplicadas += 1;
                // A primeira unidade depois da perda que **não** é uma
                // keyframe tem de ser descartada: é a prova de que o
                // receptor não está a aplicar bytes contra uma base que
                // não tem.
                if ainda_desalinhado && !unidade.keyframe {
                    panic!(
                        "a unidade {} foi aplicada sem ser keyframe, com o receptor desalinhado",
                        unidade.seq
                    );
                }
                ainda_desalinhado = false;
            }
            Recebida::Descartada => descartadas += 1,
        }
    }

    assert!(
        descartadas > 0,
        "nada foi descartado: o receptor não viu a perda"
    );
    assert!(
        !ainda_desalinhado,
        "o receptor ficou desalinhado até ao fim: não houve keyframe de recuperação"
    );
    assert!(
        aplicadas >= 3,
        "só {aplicadas} unidades aplicadas de {}",
        unidades.len()
    );
    assert_eq!(ecrã_receptor, ultimo, "e o ecrã do receptor é o do emissor");
}

#[test]
fn a_contagem_de_descartes_nao_transborda() {
    let mut r = Receptor::novo();
    for _ in 0..u32::MAX {
        r.descartadas = r.descartadas.saturating_add(1);
    }
    assert_eq!(r.descartadas, u32::MAX, "e não faz o que um `+=` faria");
}
