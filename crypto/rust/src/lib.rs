// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// crypto_core — núcleo de criptografia digital do OnyxChat
// ---------------------------------------------------------------------
// API pública usada pelo daemon Rust (e pelos testes end-to-end):
//
//   K1 → aead_chacha  ChaCha20-Poly1305 (chave do par)
//   K5 → encrypt_k5 / decrypt_k5  AES-256-GCM (chave do remetente)
//   K9 → ffi_c::k9_cifrar  ChaCha20-Poly1305 via libsodium, em C
//        [a referência em Rust é `aead_chacha`; a igualdade byte a byte
//         está verificada em `ffi_c::tests::k9_c_igual_a_k9_rust`]
//   ID → sign_message / verify_signature  Ed25519 (estabiliza identidade)
//
//   AMIZ → derivar_k1  K1 por mensagem a partir do segredo do par
//        (`amizade.rs`) — sem forward secrecy, declarado
//
//   SEG → trancar_paginas  mlockall — impede que páginas de material
//        sensível sejam descartadas para swap, onde a `zeroize` do
//        `Segredo` não as alcança (ver `paginas.rs`).
//
// Todas as funções devolvem `Result<_, CryptoError>`: o daemon converte
// o erro num status IPC em vez de abortar o processo.
// =====================================================================

pub mod aead_aes;
pub mod aead_chacha;
pub mod amizade;
pub mod error;
pub mod hill;
pub mod paginas;
pub mod rng;
pub mod secret;
pub mod signer;

pub use amizade::{derivar_k1, DOMINIO_AMIZADE, INFO_K1};
pub use error::CryptoError;
pub use paginas::{EstadoPaginas, MotivoFalha};
pub use secret::{Segredo, TAMANHO_CHAVE};
pub use signer::TAMANHO_ASSINATURA;

/// Cifra bytes arbitrários com K5 (chave única do remetente).
pub fn encrypt_k5(mensagem: &[u8], chave: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CryptoError> {
    aead_aes::encrypt(mensagem, chave, nonce)
}

/// Decifra e autentica K5.
pub fn decrypt_k5(ciphertext: &[u8], chave: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CryptoError> {
    aead_aes::decrypt(ciphertext, chave, nonce)
}

// K1 e K9 não têm função própria neste crate, e é deliberado.
//
// K1 chamava-se `encrypt_k1`/`decrypt_k1` e K9 `encrypt_k9`/`decrypt_k9`:
// quatro wrappers de uma linha sobre `aead_chacha`, sem um único caller em
// todo o repositório — nem de produção, nem de teste. K1 morria sozinho
// por um motivo melhor: a assinatura era `&str`, e o pipeline passa
// `&[u8]`, porque o plaintext já não é garantidamente UTF-8 a esta
// altura. Um wrapper que não dá para usar com o seu próprio caller é a
// forma mais cara de código morto.
//
// K9 é mais interessante. O wrapper dizia, com a segurança de quem não
// foi verificar: «os testes garantem que ambas as implementações
// produzem outputs indistinguíveis». Não havia teste nenhum. A garantia
// estava escrita, não verificada. Passou a estar verificada — e num sítio
// onde não se pode perder de vista — em `ffi_c::tests::k9_c_igual_a_k9_rust`,
// que compara byte a byte a K9 de produção (C/libsodium) com
// `aead_chacha`. A API morta foi-se; a promessa ficou.
//
// Quem precisar da K9 de referencia escreve `aead_chacha::encrypt`, que
// é o que o pipeline já fazia.

/// Gera um nonce aleatório para qualquer camada AEAD (K1/K5/K9).
///
/// Um nonce por mensagem, nunca reutilizado com a mesma chave.
pub fn gerar_nonce() -> [u8; aead_chacha::TAMANHO_NONCE] {
    aead_chacha::gerar_nonce()
}

/// Gera uma chave simétrica aleatória de 32 bytes (K1/K5/K9).
pub fn gerar_chave() -> [u8; TAMANHO_CHAVE] {
    let mut chave = [0u8; TAMANHO_CHAVE];
    rng::preencher(&mut chave);
    chave
}

/// Gera a identidade Ed25519 do utilizador: (seed privada, pública).
pub fn gerar_identidade() -> signer::ParDeIdentidade {
    signer::gerar_identidade()
}

/// Assina bytes com a identidade do remetente (64 bytes).
pub fn sign_message(mensagem: &[u8], seed_privada: &[u8; 32]) -> Vec<u8> {
    signer::sign(mensagem, seed_privada)
}

/// Verifica a assinatura do remetente; `Err` rejeita a mensagem.
pub fn verify_signature(
    mensagem: &[u8],
    assinatura: &[u8],
    chave_publica: &[u8; 32],
) -> Result<(), CryptoError> {
    signer::verify(mensagem, assinatura, chave_publica)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Utilitário mínimo de hex para vectores de teste (evita dependência
    /// externa e cobre-se a si próprio nos testes).
    fn para_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Converte hex para bytes; `panic` apenas em vectores de teste mal escritos.
    fn de_hex(hex: &str) -> Vec<u8> {
        assert!(hex.len().is_multiple_of(2), "hex com comprimento ímpar");
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex válido"))
            .collect()
    }

    // ----------------------------------------------------------------
    // Vectores conhecidos — regressão da API pública K1/K5/K9.
    // ----------------------------------------------------------------

    // K1: roundtrip + vector determinístico (chave/nonce fixos).
    //
    // Repara no que mudou: o teste passou de `encrypt_k1("Olá mundo!",…)`
    // para `aead_chacha::encrypt(b"Olá mundo!",…)`. O wrapper aceitava
    // `&str` e convertia; a primitiva trabalha em bytes. A conversão
    // desapareceu do caminho do daemon há muito — o pipeline cifra
    // `&[u8]` porque o plaintext a esta altura já não é garantidamente
    // texto — e o wrapper ficou atrás, à espera de um caller que nunca
    // apareceu. O vector é o mesmo, o que confirma que o caminho novo é
    // o mesmo caminho.
    #[test]
    fn k1_roundtrip_e_vector() {
        let chave = [0x01u8; 32];
        let nonce = [0x02u8; 12];
        let texto = "Olá mundo!".as_bytes();

        let ct = aead_chacha::encrypt(texto, &chave, &nonce).expect("K1 cifra");
        // Vector conhecido (chave 01×32, nonce 02×12): trava a primitiva
        // e o layout ciphertext||tag contra regressões silenciosas.
        assert_eq!(
            para_hex(&ct),
            "a970318fadce9db41542c8edf71fd102ba9672ab11dc78acc68099"
        );
        assert_eq!(aead_chacha::decrypt(&ct, &chave, &nonce).unwrap(), texto);
    }

    // K1 com chave/nonce errados → erro estruturado, nunca panic.
    #[test]
    fn k1_erros_estruturados() {
        let nonce = [0u8; 12];
        assert!(matches!(
            aead_chacha::encrypt(b"m", &[0u8; 15], &nonce),
            Err(CryptoError::TamanhoDeChaveInvalido { .. })
        ));
        assert!(matches!(
            aead_chacha::encrypt(b"m", &[0u8; 32], &[0u8; 9]),
            Err(CryptoError::TamanhoDeNonceInvalido { .. })
        ));
    }

    // Decifrar lixo (tag inválida) devolve `DecifragemFalhou`.
    #[test]
    fn k1_decifragem_invalida() {
        let chave = gerar_chave();
        let nonce = gerar_nonce();
        assert_eq!(
            aead_chacha::decrypt(&[0u8; 32], &chave, &nonce).unwrap_err(),
            CryptoError::DecifragemFalhou
        );
    }

    // O plaintext decifrado não é validado como UTF-8 pela primitiva.
    //
    // O wrapper `decrypt_k1` fazia a conversão e devolvia `TextoInvalidoUtf8`
    // num payload que não decifrasse para texto válido. Wrapper removido,
    // a conversão passou para o dono do plaintext: o daemon valida em
    // `pipeline::decifrar`, onde o erro é `ErroPipeline::TextoInvalidoUtf8`
    // e o status IPC é `0x08`. Este teste fica a fixar que a primitiva
    // **não** faz essa validação — para que ninguém reintroduza a
    // conversão aqui a pretexto de «o payload é sempre texto».
    #[test]
    fn k1_primitiva_nao_valida_utf8() {
        let chave = gerar_chave();
        let nonce = gerar_nonce();
        let ct = aead_chacha::encrypt(&[0xC3, 0x28], &chave, &nonce).unwrap();
        let bytes = aead_chacha::decrypt(&ct, &chave, &nonce).expect("decifra");
        // Bytes inválidos como UTF-8 saem intactos: a decisão é do dono.
        assert_eq!(bytes, vec![0xC3, 0x28]);
        assert!(String::from_utf8(bytes).is_err());
    }

    // K5: roundtrip de bytes arbitrários (inclui zeros).
    #[test]
    fn k5_roundtrip() {
        let chave = gerar_chave();
        let nonce = gerar_nonce();
        let dados = vec![0u8, 1, 2, 255, 128, 0];
        let ct = encrypt_k5(&dados, &chave, &nonce).expect("K5 cifra");
        assert_eq!(decrypt_k5(&ct, &chave, &nonce).unwrap(), dados);
        // Erros também propagam.
        assert_eq!(
            decrypt_k5(&[], &chave, &[0u8; 5]).unwrap_err(),
            CryptoError::TamanhoDeNonceInvalido {
                esperado: 12,
                obtido: 5
            }
        );
    }

    // A K9 de referência em Rust é roundtrip da própria primitiva.
    //
    // Este teste era, antes de G4, uma tautologia: comparava
    // `encrypt_k9` com `aead_chacha::encrypt`, e as duas eram a mesma
    // chamada com um nome diferente. Passava sem verificar nada
    // nada. Ficou aqui porque continua a valer o que valeva — o roundtrip
    // da primitiva que o daemon usa de especificação — e a comparação
    // que importava (C contra Rust, que é onde as implementações
    // divergem) está em `ffi_c::tests::k9_c_igual_a_k9_rust`.
    #[test]
    fn k9_primitiva_roundtrip() {
        let chave = gerar_chave();
        let nonce = gerar_nonce();
        let texto = b"\xc3\xbaltima camada";
        let ct = aead_chacha::encrypt(texto, &chave, &nonce).expect("cifra");
        assert_eq!(aead_chacha::decrypt(&ct, &chave, &nonce).unwrap(), texto);
    }

    // ----------------------------------------------------------------
    // Funções utilitárias e assinaturas
    // ----------------------------------------------------------------

    // Nonces e chaves gerados são aleatórios e com tamanho correto.
    #[test]
    fn geradores_aleatorios() {
        assert_ne!(gerar_nonce(), gerar_nonce());
        assert_ne!(gerar_chave(), gerar_chave());
        assert_eq!(gerar_chave().len(), 32);
        assert_eq!(gerar_nonce().len(), 12);
    }

    // Pipeline digital completo: K1 → K5 → K9 → assina → verifica → K9⁻¹…
    #[test]
    fn pipeline_digital_completo() {
        let k1 = gerar_chave(); // chave do par
        let k5 = gerar_chave(); // chave do remetente
        let k9 = gerar_chave(); // chave do receptor
        let (seed, publica) = gerar_identidade();

        let n1 = gerar_nonce();
        let n5 = gerar_nonce();
        let n9 = gerar_nonce();

        let c1 = aead_chacha::encrypt(b"Ol\xc3\xa1 mundo!", &k1, &n1).unwrap();
        let c5 = encrypt_k5(&c1, &k5, &n5).unwrap();
        let c9 = aead_chacha::encrypt(&c5, &k9, &n9).unwrap();

        // O remetente assina o ciphertext final mais os nonces do envelope.
        let mut pacote = c9.clone();
        pacote.extend_from_slice(&n1);
        pacote.extend_from_slice(&n5);
        pacote.extend_from_slice(&n9);
        let assinatura = sign_message(&pacote, &seed);

        // Receptor: verifica identidade ANTES de decifrar.
        verify_signature(&pacote, &assinatura, &publica).expect("assinatura válida");

        let d5 = aead_chacha::decrypt(&c9, &k9, &n9).unwrap();
        let d1 = decrypt_k5(&d5, &k5, &n5).unwrap();
        assert_eq!(
            aead_chacha::decrypt(&d1, &k1, &n1).unwrap(),
            b"Ol\xc3\xa1 mundo!"
        );
    }

    // Adulteração do pacote assinado → rejeição total.
    #[test]
    fn pacote_adulterado_rejeitado() {
        let (seed, publica) = gerar_identidade();
        let mut pacote = b"ciphertext".to_vec();
        pacote.extend_from_slice(&[0u8; 36]); // nonces
        let assinatura = sign_message(&pacote, &seed);

        pacote[0] ^= 0x01;
        assert_eq!(
            verify_signature(&pacote, &assinatura, &publica).unwrap_err(),
            CryptoError::AssinaturaInvalida
        );
    }

    // Vectores hex dos utilitários de teste (auto-teste do helper).
    #[test]
    fn helper_hex() {
        assert_eq!(para_hex(&[0x00, 0xAB, 0xFF]), "00abff");
        assert_eq!(de_hex("00abff"), vec![0x00, 0xAB, 0xFF]);
        assert_eq!(para_hex(&de_hex("deadbeef")), "deadbeef");
    }
}
