// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// propriedades.rs — property tests do núcleo criptográfico (Fase 3.2)
// ---------------------------------------------------------------------
// Invariântes verificadas para TODO o domínio de inputs gerados
// (Resumo §48 — categorias de teste), não apenas para exemplos fixos:
//
//   * roundtrip das camadas digitais K1/K5/K7 (qualquer mensagem);
//   * assinatura Ed25519: válida na mensagem original e rejeitada
//     perante QUALQUER corrupção da mensagem ou da assinatura;
//   * derivação da K1 por mensagem: simétrica na ordem das chaves,
//     direccional na identidade do emissor, e sensível a cada entrada.
//
// O binário não é alterado; estes testes só exercitam a API pública.
// =====================================================================

use proptest::prelude::*;

use crypto_core::{aead_aes, aead_chacha, derivar_k1, hill, sign_message, verify_signature};

proptest! {
    // --- K1/K9 — ChaCha20-Poly1305 --------------------------------

    /// Qualquer mensagem/chave/nonce roundtrips e a tag AEAD cresce
    /// o ciphertext em 16 bytes.
    #[test]
    fn k1_roundtrip_para_inputs_arbitrarios(
        msg in prop::collection::vec(any::<u8>(), 0..2048),
        chave in any::<[u8; 32]>(),
        nonce in any::<[u8; 12]>(),
    ) {
        let ct = aead_chacha::encrypt(&msg, &chave, &nonce).expect("K1 cifra");
        prop_assert_eq!(ct.len(), msg.len() + 16, "tag Poly1305");
        let volta = aead_chacha::decrypt(&ct, &chave, &nonce).expect("K1 decifra");
        prop_assert_eq!(volta, msg);
    }

    /// Qualquer corrupção de um byte do ciphertext é rejeitada (AEAD).
    #[test]
    fn k1_rejeita_corrupcao(
        msg in prop::collection::vec(any::<u8>(), 1..512),
        chave in any::<[u8; 32]>(),
        nonce in any::<[u8; 12]>(),
        idx in any::<usize>(),
    ) {
        let mut ct = aead_chacha::encrypt(&msg, &chave, &nonce).expect("K1 cifra");
        let pos = idx % ct.len();
        ct[pos] ^= 0x01;
        prop_assert!(aead_chacha::decrypt(&ct, &chave, &nonce).is_err());
    }

    // --- K5 — AES-256-GCM -----------------------------------------

    /// Roundtrip da camada AES-256-GCM para qualquer input.
    #[test]
    fn k5_roundtrip_para_inputs_arbitrarios(
        msg in prop::collection::vec(any::<u8>(), 0..2048),
        chave in any::<[u8; 32]>(),
        nonce in any::<[u8; 12]>(),
    ) {
        let ct = aead_aes::encrypt(&msg, &chave, &nonce).expect("K5 cifra");
        prop_assert_eq!(ct.len(), msg.len() + 16, "tag GCM");
        let volta = aead_aes::decrypt(&ct, &chave, &nonce).expect("K5 decifra");
        prop_assert_eq!(volta, msg);
    }

    // --- K7 — Hill 2×2 --------------------------------------------

    /// Roundtrip de Hill (PKCS#7 + blocos de 2) para qualquer tamanho.
    #[test]
    fn k7_roundtrip_para_inputs_arbitrarios(
        msg in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        let ct = hill::cifrar(&msg);
        let volta = hill::decifrar(&ct).expect("K7 decifra");
        prop_assert_eq!(volta, msg);
    }

    // --- Assinaturas Ed25519 --------------------------------------

    /// A assinatura valida sempre sobre a mensagem original.
    #[test]
    fn assinatura_valida_na_mensagem_original(
        msg in prop::collection::vec(any::<u8>(), 0..1024),
        seed in any::<[u8; 32]>(),
    ) {
        let sig = sign_message(&msg, &seed);
        let pub_a = crypto_core::signer::chave_publica_a_partir_da_seed(&seed);
        prop_assert!(verify_signature(&msg, &sig, &pub_a).is_ok());
    }

    /// Qualquer corrupção de um byte da assinatura é rejeitada.
    #[test]
    fn assinatura_rejeita_corrupcao_da_assinatura(
        msg in prop::collection::vec(any::<u8>(), 0..512),
        seed in any::<[u8; 32]>(),
        idx in any::<usize>(),
    ) {
        let mut sig = sign_message(&msg, &seed);
        let pos = idx % sig.len();
        sig[pos] ^= 0x01;
        let pub_a = crypto_core::signer::chave_publica_a_partir_da_seed(&seed);
        prop_assert!(verify_signature(&msg, &sig, &pub_a).is_err());
    }

    /// Qualquer corrupção de um byte da mensagem é rejeitada.
    #[test]
    fn assinatura_rejeita_corrupcao_da_mensagem(
        msg in prop::collection::vec(any::<u8>(), 1..512),
        seed in any::<[u8; 32]>(),
        idx in any::<usize>(),
    ) {
        let sig = sign_message(&msg, &seed);
        let mut alterada = msg.clone();
        let pos = idx % alterada.len();
        alterada[pos] ^= 0x01;
        let pub_a = crypto_core::signer::chave_publica_a_partir_da_seed(&seed);
        prop_assert!(verify_signature(&alterada, &sig, &pub_a).is_err());
    }

    // --- Derivação da K1 por mensagem -----------------------------

    /// A derivação é **simétrica** na ordem das chaves, para qualquer
    /// par.
    ///
    /// A propriedade de que a vida do projecto depende: os dois lados
    /// conhecem o mesmo par, cada um na sua posição, e têm de obter a
    /// mesma chave. Um exemplo fixo provaria isso para aquele exemplo;
    /// esta prova que é verdade por construção.
    #[test]
    fn derivacao_e_simetrica_para_qualquer_par(
        k_a in any::<[u8; 32]>(),
        k_b in any::<[u8; 32]>(),
        pub_a in any::<[u8; 32]>(),
        nonce in any::<[u8; 12]>(),
    ) {
        prop_assert_eq!(
            derivar_k1(&k_a, &k_b, &pub_a, &nonce),
            derivar_k1(&k_b, &k_a, &pub_a, &nonce),
            "a semente tem de ser independente da ordem das chaves"
        );
    }

    /// A derivação é **direccional**: a identidade do emissor muda a
    /// chave, mesmo com o mesmo par e o mesmo nonce.
    ///
    /// Esta é a barreira contra reflexão — um envelope de Alice
    /// devolvido por Bob não pode abrir como se fosse de Bob. O
    /// `pub_b` é construído invertendo um byte de `pub_a`, o que
    /// garante que são diferentes sem depender de uma constante.
    #[test]
    fn derivacao_e_direccional_para_qualquer_entrada(
        k_a in any::<[u8; 32]>(),
        k_b in any::<[u8; 32]>(),
        pub_a in any::<[u8; 32]>(),
        nonce in any::<[u8; 12]>(),
    ) {
        let mut pub_b = pub_a;
        pub_b[0] ^= 0x01;
        prop_assert_ne!(
            derivar_k1(&k_a, &k_b, &pub_a, &nonce),
            derivar_k1(&k_b, &k_a, &pub_b, &nonce),
            "A→B e B→A não podem partilhar chave"
        );
    }

    /// Um nonce diferente dá uma K1 diferente.
    ///
    /// A separação entre mensagens é o que o nonce garante; se a
    /// derivação o ignorasse, duas mensagens do mesmo par partilhariam
    /// chave e a reutilização de nonce passaria a ser um problema
    /// catastrophicamente pior do que já é.
    #[test]
    fn nonce_diferente_da_k1_diferente(
        k_a in any::<[u8; 32]>(),
        k_b in any::<[u8; 32]>(),
        pub_a in any::<[u8; 32]>(),
        nonce in any::<[u8; 12]>(),
    ) {
        let mut outro = nonce;
        // Um nonce de 12 bytes chega 256^12^0.5 ≈ 2^72 mensagens para
        // uma colisão; a propriedade que se testa é a de "o nonce
        // entra", não a improbabilidade.
        outro[11] ^= 0x01;
        prop_assert_ne!(
            derivar_k1(&k_a, &k_b, &pub_a, &nonce),
            derivar_k1(&k_a, &k_b, &pub_a, &outro),
        );
    }

    /// Trocar qualquer uma das duas chaves muda a K1 derivada.
    #[test]
    fn chave_diferente_da_k1_diferente(
        k_a in any::<[u8; 32]>(),
        k_b in any::<[u8; 32]>(),
        pub_a in any::<[u8; 32]>(),
        nonce in any::<[u8; 12]>(),
    ) {
        let mut outra_a = k_a;
        outra_a[0] ^= 0x01;
        prop_assert_ne!(derivar_k1(&k_a, &k_b, &pub_a, &nonce), derivar_k1(&outra_a, &k_b, &pub_a, &nonce));

        let mut outra_b = k_b;
        outra_b[0] ^= 0x01;
        prop_assert_ne!(derivar_k1(&k_a, &k_b, &pub_a, &nonce), derivar_k1(&k_a, &outra_b, &pub_a, &nonce));
    }

    /// A K1 derivada nunca é uma das entradas nem zero.
    #[test]
    fn saida_nao_revela_as_entradas(
        k_a in any::<[u8; 32]>(),
        k_b in any::<[u8; 32]>(),
        pub_a in any::<[u8; 32]>(),
        nonce in any::<[u8; 12]>(),
    ) {
        let derivada = derivar_k1(&k_a, &k_b, &pub_a, &nonce);
        prop_assert_ne!(derivada, k_a, "devolveu a chave de A");
        prop_assert_ne!(derivada, k_b, "devolveu a chave de B");
        prop_assert_ne!(derivada, pub_a, "devolveu a identidade do emissor");
        prop_assert_ne!(derivada, [0u8; 32], "devolveu zeros");
    }
}
