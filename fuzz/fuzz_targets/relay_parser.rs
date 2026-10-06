// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// relay_parser — fuzz do parser de frames (P2P / relay)
// ---------------------------------------------------------------------
// Fronteira: `ler_frame` lê `[u32 LE][tipo][corpo]` de qualquer stream
// (P2P direto, relay, frames de controlo) — docs/relay.md e
// docs/architecture.md §Transporte.
//
// Duas propriedades para além da não-crash:
//   1. round-trip do enquadramento: um frame aceito, re-enquadrado,
//      reproduz exactamente os bytes consumidos do stream;
//   2. o comprimento declarado nunca excede o limite do protocolo.
// =====================================================================

#![no_main]

use std::io::Cursor;

use libfuzzer_sys::fuzz_target;

use onyxchatd::p2p;

fuzz_target!(|dados: &[u8]| {
    let mut cursor = Cursor::new(dados);
    let resultado = p2p::ler_frame(&mut cursor);

    let Ok(frame) = resultado else {
        // Rejeição (EOF, comprimento 0, acima do limite, truncado) —
        // é o caminho previsto para inputs arbitrários.
        return;
    };

    // Quantos bytes o parser consumiu do stream (4 de cabeçalho +
    // comprimento declarado).
    let consumidos = &dados[..cursor.position() as usize];

    // O corpo devolvido tem de caber no limite do protocolo — um parser
    // que aceitasse um corpo acima de `TAM_MAX_CORPO` estaria a expor o
    // daemon a alocações ilimitadas a jusante.
    assert!(
        frame.corpo.len() <= p2p::TAM_MAX_CORPO,
        "corpo aceito acima do limite do protocolo"
    );

    // Re-enquadrar tem de reproduzir byte a byte o que foi lido.
    let reenquadrado =
        p2p::enquadrar(frame.tipo, &frame.corpo).expect("corpo do parser é sempre limitado");
    assert_eq!(
        reenquadrado, consumidos,
        "o frame re-enquadrado difere dos bytes lidos"
    );
});
