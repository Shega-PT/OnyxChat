// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// aead_chacha.rs — ChaCha20-Poly1305 (camadas K1 e K9 do pipeline)
// ---------------------------------------------------------------------
// K1: chave única do par (remetente+receptor)  → confidencialidade mútua
// K9: chave única do receptor                  → só o destinatário abre
//
// O AEAD devolve `ciphertext || tag(16B)`; o nonce (12B) é aleatório por
// mensagem e viaja no envelope binário (`docs/message_format.md`).
// Todos os erros são devolvidos como `Result` — nada de `panic`.
// =====================================================================

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use crate::error::CryptoError;
use crate::secret::TAMANHO_CHAVE;

/// Tamanho do nonce ChaCha20-Poly1305 (96 bits), em bytes.
pub const TAMANHO_NONCE: usize = 12;

/// Tamanho da tag de autenticação Poly1305, em bytes.
pub const TAMANHO_TAG: usize = 16;

/// Gera um nonce aleatório (CSPRNG do sistema) — um por mensagem.
///
/// Reutilizar um nonce com a mesma chave quebra o ChaCha20-Poly1305,
/// por isso este é o único caminho de geração permitido.
pub fn gerar_nonce() -> [u8; TAMANHO_NONCE] {
    let mut nonce = [0u8; TAMANHO_NONCE];
    crate::rng::preencher(&mut nonce);
    nonce
}

/// Valida os tamanhos de chave e nonce antes de tocar na biblioteca
/// (a crate faz `panic` em `from_slice` com tamanhos errados).
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

/// Cifra `mensagem` com ChaCha20-Poly1305.
///
/// Devolve `ciphertext || tag`. O nonce tem de ser único por mensagem
/// e deve ser obtido de `gerar_nonce()`.
pub fn encrypt(mensagem: &[u8], chave: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CryptoError> {
    validar_entradas(chave, nonce)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(chave));
    let n = Nonce::from_slice(nonce);
    cipher
        .encrypt(n, mensagem)
        .map_err(|_| CryptoError::CifragemFalhou)
}

/// Decifra e autentica `ciphertext` (ciphertext||tag).
///
/// Falha com `DecifragemFalhou` se a tag não conferir — mensagem
/// adulterada, chave errada ou nonce errado.
pub fn decrypt(ciphertext: &[u8], chave: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CryptoError> {
    validar_entradas(chave, nonce)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(chave));
    let n = Nonce::from_slice(nonce);
    cipher
        .decrypt(n, ciphertext)
        .map_err(|_| CryptoError::DecifragemFalhou)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Caminho feliz: cifrar → decifrar devolve o texto original.
    #[test]
    fn roundtrip_chacha() {
        let chave = [0x11u8; TAMANHO_CHAVE];
        let nonce = gerar_nonce();
        let msg = "OnyxChat — mensagem secreta".as_bytes();

        let ct = encrypt(msg, &chave, &nonce).expect("cifragem deve funcionar");
        assert!(ct.len() > msg.len(), "ct inclui a tag de 16 bytes");
        assert_ne!(&ct[..msg.len()], msg, "ct tem de diferir do pt");

        let pt = decrypt(&ct, &chave, &nonce).expect("decifragem deve funcionar");
        assert_eq!(pt, msg);
    }

    // Chaves diferentes não conseguem abrir a mesma mensagem.
    #[test]
    fn chave_errada_falha_na_decifragem() {
        let nonce = gerar_nonce();
        let ct = encrypt(b"segredo", &[0x22u8; TAMANHO_CHAVE], &nonce).unwrap();
        let erro =
            decrypt(&ct, &[0x33u8; TAMANHO_CHAVE], &nonce).expect_err("chave errada tem de falhar");
        assert_eq!(erro, CryptoError::DecifragemFalhou);
    }

    // Qualquer alteração ao ciphertext invalida a tag (integridade).
    #[test]
    fn ciphertext_adulterado_falha() {
        let chave = [0x44u8; TAMANHO_CHAVE];
        let nonce = gerar_nonce();
        let mut ct = encrypt(b"integridade", &chave, &nonce).unwrap();
        ct[0] ^= 0x01; // bit-flip no primeiro byte
        assert_eq!(
            decrypt(&ct, &chave, &nonce).unwrap_err(),
            CryptoError::DecifragemFalhou
        );
    }

    // Mensagem vazia também tem de ter roundtrip (caso de bordo).
    #[test]
    fn roundtrip_mensagem_vazia() {
        let chave = [0x55u8; TAMANHO_CHAVE];
        let nonce = gerar_nonce();
        let ct = encrypt(b"", &chave, &nonce).expect("AEAD aceita vazio");
        assert_eq!(ct.len(), TAMANHO_TAG, "só a tag, sem ciphertext");
        assert_eq!(decrypt(&ct, &chave, &nonce).unwrap(), b"");
    }

    // Validação de tamanhos: chave e nonce errados devolvem erros
    // específicos em vez de `panic`.
    #[test]
    fn valida_tamanhos_de_chave_e_nonce() {
        let nonce_ok = gerar_nonce();

        let e = encrypt(b"m", &[0u8; 16], &nonce_ok).unwrap_err();
        assert_eq!(
            e,
            CryptoError::TamanhoDeChaveInvalido {
                esperado: 32,
                obtido: 16
            }
        );

        let e = encrypt(b"m", &[0u8; 32], &[0u8; 8]).unwrap_err();
        assert_eq!(
            e,
            CryptoError::TamanhoDeNonceInvalido {
                esperado: 12,
                obtido: 8
            }
        );

        // Mesma validação no caminho de decifragem.
        let e = decrypt(&[0u8; 16], &[0u8; 31], &nonce_ok).unwrap_err();
        assert_eq!(
            e,
            CryptoError::TamanhoDeChaveInvalido {
                esperado: 32,
                obtido: 31
            }
        );
        let e = decrypt(&[0u8; 16], &[0u8; 32], &[0u8; 0]).unwrap_err();
        assert_eq!(
            e,
            CryptoError::TamanhoDeNonceInvalido {
                esperado: 12,
                obtido: 0
            }
        );
    }

    // Nonces têm de ser aleatórios: duas chamadas nunca coincidem
    // (probabilidade desprezível; falha só se o CSPRNG estiver morto).
    #[test]
    fn gerar_nonce_e_aleatorio() {
        let a = gerar_nonce();
        let b = gerar_nonce();
        assert_ne!(a, b, "nonces repetidos quebrariam o AEAD");
        assert!(a.iter().any(|&x| x != 0), "nonce não pode ser todo zero");
    }

    // Um nonce diferente não abre a mesma mensagem.
    #[test]
    fn nonce_diferente_falha() {
        let chave = [0x66u8; TAMANHO_CHAVE];
        let ct = encrypt(b"novo nonce", &chave, &gerar_nonce()).unwrap();
        assert_eq!(
            decrypt(&ct, &chave, &gerar_nonce()).unwrap_err(),
            CryptoError::DecifragemFalhou
        );
    }
}
