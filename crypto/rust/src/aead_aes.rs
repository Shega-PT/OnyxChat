// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// aead_aes.rs — AES-256-GCM (camada K5 do pipeline)
// ---------------------------------------------------------------------
// K5: chave única do remetente → blindagem adicional que autentica a
// origem no meio do pipeline (entre a transposição K4 e o Playfair K6).
//
// Em CPUs com AES-NI o hardware acelera esta camada (requisito de
// desempenho de `docs/architecture.md`). Estrutura idêntica ao `aead_chacha`:
// `Result` em todos os caminhos, nonce aleatório por mensagem.
// =====================================================================

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};

use crate::aead_chacha::{gerar_nonce, TAMANHO_NONCE};
use crate::error::CryptoError;
use crate::secret::TAMANHO_CHAVE;

/// Reexporta os tamanhos do formato (K5 usa o mesmo layout AEAD: 12B/16B).
pub use crate::aead_chacha::{TAMANHO_NONCE as NONCE_K5, TAMANHO_TAG as TAG_K5};

/// Gera o nonce específico de K5 ( delegado ao CSPRNG comum do sistema).
pub fn gerar_nonce_k5() -> [u8; TAMANHO_NONCE] {
    gerar_nonce()
}

/// Valida os tamanhos antes de construir a chave GCM (evita `panic`).
fn validar_entradas(chave: &[u8], nonce: &[u8]) -> Result<(), CryptoError> {
    if chave.len() != TAMANHO_CHAVE {
        return Err(CryptoError::TamanhoDeChaveInvalido {
            esperado: TAMANHO_CHAVE,
            obtido: chave.len(),
        });
    }
    if nonce.len() != TAMANHO_NONCE {
        return Err(CryptoError::TamanhoDeNonceInvalido {
            esperado: TAMANHO_NONCE,
            obtido: nonce.len(),
        });
    }
    Ok(())
}

/// Cifra `mensagem` com AES-256-GCM; devolve `ciphertext || tag`.
pub fn encrypt(mensagem: &[u8], chave: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CryptoError> {
    validar_entradas(chave, nonce)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(chave));
    let n = Nonce::from_slice(nonce);
    cipher
        .encrypt(n, mensagem)
        .map_err(|_| CryptoError::CifragemFalhou)
}

/// Decifra e autentica o output de K5; `DecifragemFalhou` se a tag falhar.
pub fn decrypt(ciphertext: &[u8], chave: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CryptoError> {
    validar_entradas(chave, nonce)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(chave));
    let n = Nonce::from_slice(nonce);
    cipher
        .decrypt(n, ciphertext)
        .map_err(|_| CryptoError::DecifragemFalhou)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aead_chacha::TAMANHO_TAG;

    // Roundtrip completo de K5 com mensagem UTF-8 multibyte.
    #[test]
    fn roundtrip_aes() {
        let chave = [0xA1u8; TAMANHO_CHAVE];
        let nonce = gerar_nonce_k5();
        // Texto simples de propósito: um emoji aqui passaria a auditoria
        // de alfabeto, e a auditoria existe para apanhar exactamente
        // esse tipo de entrada.
        let msg = "Olá mundo!".as_bytes();

        let ct = encrypt(msg, &chave, &nonce).expect("cifragem K5 deve funcionar");
        assert!(ct.len() >= msg.len() + TAMANHO_TAG);
        assert_eq!(decrypt(&ct, &chave, &nonce).unwrap(), msg);
    }

    // Chave errada → tag inválida → erro estruturado (sem panic).
    #[test]
    fn chave_errada_falha() {
        let nonce = gerar_nonce_k5();
        let ct = encrypt(b"origem", &[0xB2u8; TAMANHO_CHAVE], &nonce).unwrap();
        assert_eq!(
            decrypt(&ct, &[0xC3u8; TAMANHO_CHAVE], &nonce).unwrap_err(),
            CryptoError::DecifragemFalhou
        );
    }

    // Adulteração de um byte no meio do ciphertext é detetada.
    #[test]
    fn adulteracao_falha() {
        let chave = [0xD4u8; TAMANHO_CHAVE];
        let nonce = gerar_nonce_k5();
        let mut ct = encrypt(b"payload longo para adulterar", &chave, &nonce).unwrap();
        let meio = ct.len() / 2;
        ct[meio] ^= 0xFF;
        assert_eq!(
            decrypt(&ct, &chave, &nonce).unwrap_err(),
            CryptoError::DecifragemFalhou
        );
    }

    // Validação de tamanhos em ambos os sentidos (K5).
    #[test]
    fn valida_tamanhos() {
        let nonce = gerar_nonce_k5();
        assert_eq!(
            encrypt(b"m", &[0u8; 8], &nonce).unwrap_err(),
            CryptoError::TamanhoDeChaveInvalido {
                esperado: 32,
                obtido: 8
            }
        );
        assert_eq!(
            encrypt(b"m", &[0u8; 32], &[0u8; 11]).unwrap_err(),
            CryptoError::TamanhoDeNonceInvalido {
                esperado: 12,
                obtido: 11
            }
        );
        assert_eq!(
            decrypt(&[0u8; 4], &[0u8; 32], &[0u8; 12]).unwrap_err(),
            CryptoError::DecifragemFalhou
        );
        assert_eq!(
            decrypt(&[0u8; 4], &[0u8; 33], &nonce).unwrap_err(),
            CryptoError::TamanhoDeChaveInvalido {
                esperado: 32,
                obtido: 33
            }
        );
    }

    // Mensagem vazia: só a tag (16B) no output.
    #[test]
    fn roundtrip_vazio() {
        let chave = [0xE5u8; TAMANHO_CHAVE];
        let nonce = gerar_nonce_k5();
        let ct = encrypt(b"", &chave, &nonce).unwrap();
        assert_eq!(ct.len(), TAMANHO_TAG);
        assert_eq!(decrypt(&ct, &chave, &nonce).unwrap(), b"");
    }

    // Nonce K5 aleatório e distinto do K1 (mesmo CSPRNG, sem colisões).
    #[test]
    fn nonces_sao_distintos() {
        assert_ne!(gerar_nonce_k5(), gerar_nonce_k5());
    }

    // Os reexports do formato (NONCE_K5/TAG_K5) têm os tamanhos certos.
    #[test]
    fn tamanhos_reexportados() {
        assert_eq!(NONCE_K5, 12);
        assert_eq!(TAG_K5, 16);
    }
}
