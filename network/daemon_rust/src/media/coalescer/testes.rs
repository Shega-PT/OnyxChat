// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes do coalescer.
//!
//! A propriedade central é a que `HOLD` e o delta partilham: **o
//! receptor tem de ver exactamente o último quadro emitido**, nem mais
//! nem menos. E o caso que a prova é o da telha que muda e volta.

use super::*;
use crate::media::ressincronizacao;
use crate::media::telhas::{aplicar, BYTES_POR_TELHA, LADO};

const LARGURA: usize = 64;
const ALTURA: usize = 48;

/// Uma geometria com **64 telhas**, para os testes que precisam de passar
/// dos 8 KB.
///
/// A de cima tem 4 × 3 = 12 telhas, e 31 é o limiar: um teste que
/// quer «muitas telhas» com 64 de largura está a escrever uma telha
/// fora do ecrã e a chamar-lhe nome.
const LARGURA_GRANDE: usize = 128;
const ALTURA_GRANDE: usize = 128;

fn preto() -> Vec<u8> {
    vec![0u8; LARGURA * ALTURA]
}

/// Um quadro com `n` telhas com variação, a partir de `semente`.
fn com_telhas(n: usize, semente: u8) -> Vec<u8> {
    com_telhas_de(LARGURA, ALTURA, n, semente)
}

/// Como [`com_telhas`], numa geometria à escolha.
fn com_telhas_de(largura: usize, altura: usize, n: usize, semente: u8) -> Vec<u8> {
    let mut q = vec![0u8; largura * altura];
    let colunas = largura / LADO;
    assert!(
        n <= colunas * (altura / LADO),
        "{n} telhas não cabem em {largura}x{altura}"
    );
    for i in 0..n {
        let col = i % colunas;
        let lin = i / colunas;
        let base = lin * LADO * largura + col * LADO;
        for y in 0..LADO {
            for x in 0..LADO {
                q[base + y * largura + x] = semente.wrapping_add((x * 7 + y * 3) as u8);
            }
        }
    }
    q
}

#[test]
fn um_delta_pequeno_acumula_e_um_grande_nao() {
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");

    // Trinta e uma telhas: 8 029 bytes, dentro dos 8 KB.
    let dentro = com_telhas_de(LARGURA_GRANDE, ALTURA_GRANDE, 31, 10);
    // Trinta e duas: 8 288 bytes, acima.
    let acima = com_telhas_de(LARGURA_GRANDE, ALTURA_GRANDE, 32, 10);

    let mut c2 = Coalescer::novo(
        &vec![0u8; LARGURA_GRANDE * ALTURA_GRANDE],
        LARGURA_GRANDE,
        ALTURA_GRANDE,
    )
    .expect("coalescer");
    assert_eq!(
        c2.decidir(&dentro, 0),
        Decisao::Acumular,
        "31 telhas são {} bytes, dentro dos 8 KB",
        31 * 259
    );
    assert_eq!(
        c2.decidir(&acima, 0),
        Decisao::Emitir,
        "32 telhas são {} bytes, acima dos 8 KB",
        32 * 259
    );
    // E o limiar é literalmente `8 KB / 259`.
    assert_eq!(MAX_TELHAS_ACUMULAR, 8 * 1024 / 259);
    let _ = &mut c;
}

#[test]
fn a_janela_de_20ms_forca_a_emissao() {
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");

    // Com a janela fechada, acumula-se mesmo com um delta pequeno.
    c.receber(&com_telhas(1, 10), 0);
    assert!(c.esvaziar().is_empty(), "o primeiro quadro acumulou");
    assert_eq!(c.decidir(&com_telhas(1, 20), 19), Decisao::Acumular);
    assert_eq!(
        c.decidir(&com_telhas(1, 20), 20),
        Decisao::Emitir,
        "aos 20 ms a janela fecha"
    );
}

#[test]
fn a_janela_fecha_aos_20ms_mesmo_com_quadros_pequenos_seguidos() {
    // A propriedade que a primeira versão não tinha: uma stream de
    // deltas pequenos **tem** de emitir. Com a janela a medir-se desde
    // o quadro anterior, um quadro de cada 6 ms nunca fechava a janela
    // e o coalescer acumulava para sempre — uma fila sem limite, que
    // é o que §8.2 proíbe ao anel e o que estoura o daemon.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    let mut total = 0usize;

    // Dois segundos de media a 60 fps: 120 quadros, todos com um delta
    // minúsculo.
    for i in 0..120i64 {
        c.receber(&com_telhas(1, 10u8.wrapping_add(i as u8)), i * 16);
        total += c.esvaziar().len();
    }
    c.fechar();
    total += c.esvaziar().len();

    assert!(total > 0, "nada saiu em dois segundos de media");
    // A 60 fps, 20 ms de janela dão ~50 unidades em 2 s. E **não** 120:
    // se saísse uma por quadro, o coalescing não estaria a fazer nada.
    assert!(
        total < 60,
        "{total} unidades para 120 quadros: o coalescing não está a fazer nada"
    );
}

#[test]
fn tres_quadros_pequenos_dao_uma_unidade() {
    // 30 fps: três quadros cabem em 20 ms, e dão **uma** mensagem.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    c.receber(&com_telhas(1, 10), 0);
    c.receber(&com_telhas(1, 20), 6);
    c.receber(&com_telhas(1, 30), 13);
    assert!(c.esvaziar().is_empty(), "nada saiu ainda");

    // O quarto fecha a janela: sai **uma** unidade, e vale pelo último.
    c.receber(&com_telhas(1, 40), 20);
    let saida = c.esvaziar();
    assert_eq!(saida.len(), 1, "três quadros num ciclo dão uma mensagem");
    assert_eq!(saida[0].seq, 0);
}

#[test]
fn um_delta_grande_cobre_o_pendente() {
    let grande = vec![0u8; LARGURA_GRANDE * ALTURA_GRANDE];
    let mut c = Coalescer::novo(&grande, LARGURA_GRANDE, ALTURA_GRANDE).expect("coalescer");
    // O delta grande é calculado contra a **mesma** referência e
    // descreve o quadro inteiro, portanto o pendente está coberto por
    // ele. Emitir os dois custava uma mensagem extra sem acrescentar um
    // pixel à imagem que o receptor vê.
    c.receber(&com_telhas_de(LARGURA_GRANDE, ALTURA_GRANDE, 1, 10), 0);
    assert!(c.esvaziar().is_empty());

    c.receber(&com_telhas_de(LARGURA_GRANDE, ALTURA_GRANDE, 40, 99), 33);
    let saida = c.esvaziar();
    assert_eq!(saida.len(), 1, "só o grande; o pendente estava coberto");
    assert_eq!(saida[0].seq, 0, "e o seq do pendente nunca foi gasto");
}

#[test]
fn a_janela_nao_custa_uma_mensagem_por_quadro_a_60fps() {
    // A medição que fez mudar o desenho. A 60 fps, quadros a 0, 16 e
    // 33 ms: a versão que emitia pendente + novo dava duas mensagens
    // em 33 ms — 60/s, contra o orçamento de ~29/s de §3.4.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    let mut total = 0usize;
    for (i, ms) in [0i64, 16, 33, 50, 66, 83, 100, 116, 133].iter().enumerate() {
        c.receber(&com_telhas(1, 10u8.wrapping_add(i as u8 * 3)), *ms);
        total += c.esvaziar().len();
    }
    c.fechar();
    total += c.esvaziar().len();

    // 150 ms de media a 60 fps são 9 quadros, e o coalescer tem de
    // produzir bem menos que 9 mensagens.
    assert!(
        total < 9,
        "{total} mensagens para 9 quadros: o coalescing não está a fazer nada"
    );
    assert!(total >= 1);
}

#[test]
fn um_ecra_estatico_da_um_hold() {
    // §8.4: com o ecrã parado, o delta é vazio e a unidade transporta
    // só a moldura — 36 bytes de texto em vez de um quadro.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    // Um ecrã parado tem de ter sido **emitido** uma vez para existir
    // como referência. Contra o ecrã preto, um quadro com três telhas
    // cheias tem delta — e chamar-lhe «estático» seria estar a testar
    // outra coisa.
    let estatico = com_telhas(3, 50);
    c.receber(&estatico, 0);
    c.saturar();
    c.drenar();
    c.esvaziar();

    // O segundo quadro abre uma janela nova; `fechar` é o que a
    // fecha, e é o que produz o `HOLD`.
    c.receber(&estatico, 40);
    c.fechar();

    let saida = c.esvaziar();
    assert_eq!(saida.len(), 1);
    assert!(saida[0].hold, "o delta contra a referência era vazio");
    // O corpo **não** é vazio: são os 4 bytes do `base_seq` e nada
    // mais. O receptor precisa dele para saber que a cadeia continua.
    assert_eq!(
        saida[0].corpo.len(),
        ressincronizacao::BYTES_DE_BASE,
        "um HOLD é o base_seq e nada mais"
    );
    assert!(ressincronizacao::delta_de(&saida[0].corpo).is_empty());
}

#[test]
fn um_hold_nao_changa_a_referencia() {
    // Um `HOLD` não move a referência: se movesse, o delta seguinte
    // seria calculado contra um quadro que nunca foi enviado.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    let antes = c.referencia().to_vec();

    let quadro = com_telhas(2, 70);
    c.receber(&quadro, 0);
    c.saturar();
    c.drenar();
    let emitido = c.esvaziar();
    assert_eq!(emitido.len(), 1);
    assert!(
        !emitido[0].hold,
        "o primeiro quadro tem delta contra o ecrã preto"
    );
    assert_eq!(
        c.referencia(),
        &quadro[..],
        "a referência passou a ser o quadro emitido"
    );

    // E agora o `HOLD`: o mesmo quadro outra vez não diz nada, e a
    // referência não se mexe.
    let referencia_apos_hold = c.referencia().to_vec();
    c.receber(&quadro, 40);
    c.fechar();
    let saida = c.esvaziar();
    assert_eq!(saida.len(), 1);
    assert!(saida[0].hold);
    assert_eq!(
        c.referencia(),
        &referencia_apos_hold[..],
        "um HOLD não move a referência"
    );
    assert_ne!(antes, referencia_apos_hold);
}

#[test]
fn o_receptor_vê_o_ultimo_quadro_e_nao_um_intermedio() {
    // A propriedade que justifica o módulo inteiro, e o caso que a
    // concatenação de deltas falha.
    //
    // A telha muda para 200 e **volta** a 7. Concatenar os dois deltas
    // deixaria o receptor com 200 — uma cor que não está no ecrã de
    // quem emite, sem nenhum sinal de erro.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    let original = preto();
    let base = 0; // telha (0, 0)

    let mut mudado = original.clone();
    for y in 0..LADO {
        for x in 0..LADO {
            mudado[base + y * LARGURA + x] = 200;
        }
    }

    // Três quadros na janela: a telha a 200, a 200 outra vez, e de volta
    // ao original. Só o último interessa.
    c.receber(&mudado, 0);
    c.receber(&mudado, 6);
    c.receber(&original, 13);
    c.receber(&original, 20);

    let saida = c.esvaziar();
    assert_eq!(saida.len(), 1);
    assert!(
        saida[0].hold,
        "voltou ao original: o delta contra a referência é vazio"
    );

    // E o receptor aplica o que recebeu ao que tinha, e fica com o
    // quadro certo.
    let mut reconstruido = c.referencia().to_vec();
    for u in &saida {
        if !u.hold {
            aplicar(&u.corpo, &mut reconstruido, LARGURA).expect("aplica");
        }
    }
    assert_eq!(reconstruido, original, "o receptor vê o último quadro");
}

#[test]
fn o_rate_limit_faz_emitir_ja() {
    // §8.3: o `RateLimit` já é o sensor de congestão. Emitir já entrega
    // o excesso ao anel, e o anel descarta vídeo — que é o que se pode
    // perder sem partir o áudio.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    c.receber(&com_telhas(1, 10), 0);
    assert!(c.esvaziar().is_empty());

    c.saturar();
    assert!(c.saturado());
    assert_eq!(
        c.decidir(&com_telhas(1, 20), 5),
        Decisao::Emitir,
        "com a janela aberta nunca se acumula"
    );

    c.receber(&com_telhas(1, 20), 5);
    // **Duas** unidades, e é o que se quer: o pendente saiu ao saturar e
    // o quadro novo também sai, porque com a janela aberta não há
    // coalescing nenhum. Mandar menos seria fingir que não estamos
    // saturados.
    assert_eq!(c.esvaziar().len(), 2, "o pendente e o quadro novo");
}

#[test]
fn saturar_limpa_o_pendente_e_drenar_nao() {
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");

    c.receber(&com_telhas(1, 10), 0);
    c.saturar();
    assert_eq!(
        c.esvaziar().len(),
        1,
        "saturar emitiu o que estava pendente"
    );
    assert!(c.janela_ha().is_none(), "e a janela ficou limpa");

    c.drenar();
    assert!(!c.saturado(), "drenar fecha a janela de congestão");

    // E `drenar` **não** emite nada: é o que fecha e nada mais. Emitir
    // aqui seria mandar uma mensagem a mais sem congestionamento
    // nenhum — e alguém a ler o nome da função adivinharia o contrário.
    c.receber(&com_telhas(1, 20), 40);
    assert!(c.esvaziar().is_empty(), "drenar não emite");
    assert_eq!(c.janela_ha(), Some(40));

    // O accumulate ficou à espera e sai quando se fecha.
    c.fechar();
    assert_eq!(c.esvaziar().len(), 1, "e ficou na fila até `fechar`");
}

#[test]
fn fechar_nao_perde_o_ultimo_acumulado() {
    // Sem isto, uma chamada acabava a meio: o receptor via uma stream
    // que terminava sem razão.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    c.receber(&com_telhas(1, 10), 0);
    assert!(c.esvaziar().is_empty());

    c.fechar();
    assert_eq!(c.esvaziar().len(), 1);
    c.fechar();
    assert!(c.esvaziar().is_empty(), "e fechar duas vezes não inventa");
}

#[test]
fn o_seq_e_o_do_que_sai_e_nao_do_que_entra() {
    // Três quadros acumulados dão **um** `seq`, porque dão uma
    // mensagem. E o `seq` é o que §8.5 usa para pedir uma keyframe.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    assert_eq!(c.proximo_seq(), 0);

    c.receber(&com_telhas(1, 10), 0);
    c.receber(&com_telhas(1, 20), 6);
    c.receber(&com_telhas(1, 30), 13);
    c.receber(&com_telhas(1, 40), 20);

    let saida = c.esvaziar();
    assert_eq!(saida.len(), 1);
    assert_eq!(saida[0].seq, 0);
    assert_eq!(c.proximo_seq(), 1, "e só consomeu um seq");
}

#[test]
fn o_clock_declarado_vem_do_que_sai() {
    // §5.4: o clock declarado é a **verificação cruzada** do contado.
    // E é o da unidade, não o do quadro que ficou pelo caminho — o
    // receptor conta pelo tempo de media que a unidade representa.
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    c.receber(&com_telhas(1, 10), 0);
    c.receber(&com_telhas(1, 20), 6);
    c.receber(&com_telhas(1, 30), 13);
    c.receber(&com_telhas(1, 40), 20);

    let saida = c.esvaziar();
    assert!(!saida.is_empty(), "saiu nada; seq={}", c.proximo_seq());
    assert_eq!(saida[0].clock_ms, 20, "o clock do último quadro da janela");
}

#[test]
fn contar_telhas_alteradas_e_uma_comparacao_de_memcmp() {
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");
    let q = com_telhas(5, 10);

    assert_eq!(c.telhas_alteradas(&preto()), 0);
    assert_eq!(c.telhas_alteradas(&q), 5);

    // E contra a referência já avançada, o mesmo quadro tem **zero**
    // telhas alteradas — que é o que faz o `HOLD`. O `saturar` força a
    // emissão, que um primeiro quadro pequeno não faria sozinho: sem
    // nada pendente, acumular abre a janela.
    c.receber(&q, 0);
    c.saturar();
    c.drenar();
    c.esvaziar();
    assert_eq!(c.telhas_alteradas(&q), 0, "é agora a referência");
}

#[test]
fn a_geometria_invalida_e_recusada_a_construcao() {
    // Uma referência com o tamanho errado daria um coalescer que produz
    // deltas de lixo, e nenhum sinal.
    let ecrã = vec![0u8; 640 * 480];
    assert!(
        Coalescer::novo(&ecrã, 640, 360).is_err(),
        "360 não é múltiplo de 16"
    );
    // 640×480 **é** aceite, desde F9. Era recusado porque uma keyframe
    // de 1200 telhas são 414 456 bytes de texto contra 65 536 — e a
    // resposta a isso não é recusar a captura, é fracionar a keyframe
    // em sete unidades (§3.6).
    assert!(Coalescer::novo(&ecrã, 640, 480).is_ok());
    assert!(
        Coalescer::novo(&vec![0u8; 10], 64, 48).is_err(),
        "tamanho errado"
    );
    assert!(Coalescer::novo(&preto(), 64, 48).is_ok());
}

#[test]
fn o_custo_de_uma_unidade_e_o_que_a_spec_promete() {
    // §8.4: com ecrã estático, ~600 bytes/s em vez de 5,8 KB/s.
    //
    // A medição é do **corpo** do delta, que é o que entra no
    // envelope; a moldura são 36 bytes em ambos os casos e não muda a
    // ordem de grandeza.
    // Um ecrã parado **depois de já ter sido emitido**: o primeiro
    // quadro é um delta contra o ecrã preto, e os seguintes já não têm
    // nada a dizer.
    let estatico = com_telhas(3, 50);
    let mut c = Coalescer::novo(&preto(), LARGURA, ALTURA).expect("coalescer");

    c.receber(&estatico, 0);
    c.saturar();
    c.drenar();
    assert_eq!(c.esvaziar().len(), 1, "o primeiro quadro emite o delta");

    c.receber(&estatico, 40);
    c.fechar();
    let saida = c.esvaziar();

    assert_eq!(saida.len(), 1);
    // Quatro bytes, os do `base_seq`. A 30 fps com coalescing de 20 ms
    // são ~25 unidades por segundo, todas com 4 bytes: 100 bytes/s do
    // delta, contra os 5,8 KB/s de um quadro inteiro.
    assert_eq!(saida[0].corpo.len(), ressincronizacao::BYTES_DE_BASE);
    assert_eq!(saida[0].corpo.len() * 25, 100, "bytes por segundo do delta");
    assert!(
        BYTES_POR_TELHA * 1200 > saida[0].corpo.len() * 25,
        "e o quadro inteiro daria {}/s",
        BYTES_POR_TELHA * 1200
    );
}
