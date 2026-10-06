// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// handshake_parser — fuzz dos validadores FRIEND_REQUEST/ACCEPT/REJECT
// ---------------------------------------------------------------------
// Fronteira: corpos de handshake chegam pelo P2P e pelo IPC
// (docs/handshake.md §Corpos). São parsers assinados — qualquer
// corrupção tem de resultar em erro tipado, nunca em panic.
//
// Além da não-crash, verifica-se a **consistência estrutural**: um
// corpo só é aceite se tiver exactamente o comprimento e a versão da
// spec.
// =====================================================================

#![no_main]

use libfuzzer_sys::fuzz_target;

use onyxchatd::handshake;

fuzz_target!(|dados: &[u8]| {
    // Nonce esperado fixo para a recusa — o validador compara-o com o
    // ecoado no corpo; um nonce arbitrário não altera a robustez.
    let nonce_esperado = [0u8; handshake::TAM_NONCE];

    // --- FRIEND_REQUEST ------------------------------------------------
    match handshake::validar_pedido(dados) {
        Err(_) => {}
        Ok(pedido) => {
            // Aceite → comprimento e versão têm de ser os da spec
            // (docs/handshake.md: 209 B, versão 0x01).
            assert_eq!(
                dados.len(),
                handshake::TAM_CORPO_PEDIDO,
                "pedido aceite com comprimento fora da spec"
            );
            assert_eq!(
                dados[0],
                handshake::VERSAO_PROTOCOLO,
                "pedido aceite com versão fora da spec"
            );
            // Os campos têm de ter as dimensões declaradas — um parser
            // que os deslocasse ficaria aqui detetado.
            assert_eq!(pedido.nonce.len(), handshake::TAM_NONCE);
        }
    }

    // --- FRIEND_ACCEPT -------------------------------------------------
    match handshake::validar_aceite(dados) {
        Err(_) => {}
        Ok(aceite) => {
            assert_eq!(
                dados.len(),
                handshake::TAM_CORPO_ACEITE,
                "aceite aceite com comprimento fora da spec"
            );
            assert_eq!(dados[0], handshake::VERSAO_PROTOCOLO);
            assert_eq!(aceite.nonce.len(), handshake::TAM_NONCE);
        }
    }

    // --- FRIEND_REJECT -------------------------------------------------
    if handshake::validar_recusa(dados, &nonce_esperado).is_ok() {
        assert_eq!(
            dados.len(),
            handshake::TAM_CORPO_RECUSA,
            "recusa aceite com comprimento fora da spec"
        );
        assert_eq!(dados[0], handshake::VERSAO_PROTOCOLO);
    }
});
