// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// signer.rs — assinaturas digitais Ed25519 (identidade do utilizador)
// ---------------------------------------------------------------------
// Conforme `docs/pipeline.md` e `docs/security_model.md`, as assinaturas
// NÃO fazem parte da cifragem: seguem no envelope junto da mensagem para
// "estabilizar a identidade" — o receptor verifica a assinatura ANTES
// de correr o pipeline K9⁻¹→K1⁻¹ e rejeita mensagens desviadas.
// =====================================================================

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};

use crate::error::CryptoError;

/// Tamanho da chave privada/pública Ed25519 (seed de 256 bits).
pub const TAMANHO_CHAVE_IDENTIDADE: usize = 32;

/// Tamanho fixo da assinatura Ed25519.
pub const TAMANHO_ASSINATURA: usize = 64;

/// Par de chaves de identidade: (seed privada 32B, pública 32B).
///
/// A seed é o "segredo" que o utilizador guarda localmente; a pública
/// distribui-se no handshake de amizade (`docs/message_format.md`).
pub type ParDeIdentidade = (
    [u8; TAMANHO_CHAVE_IDENTIDADE],
    [u8; TAMANHO_CHAVE_IDENTIDADE],
);

/// Gera um novo par de identidade Ed25519 a partir do CSPRNG do sistema.
pub fn gerar_identidade() -> ParDeIdentidade {
    // A "seed" Ed25519 é simplesmente 32 bytes aleatórios seguros; a
    // chave pública é derivada deterministicamente a partir dela.
    let mut seed = [0u8; TAMANHO_CHAVE_IDENTIDADE];
    crate::rng::preencher(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    (seed, signing_key.verifying_key().to_bytes())
}

/// Deriva a chave pública a partir da seed privada (sem rede, determinístico).
pub fn chave_publica_a_partir_da_seed(seed: &[u8; TAMANHO_CHAVE_IDENTIDADE]) -> [u8; 32] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

/// Assina `mensagem` com a seed privada; devolve 64 bytes de assinatura.
///
/// A assinatura é sempre calculada sobre o ciphertext final (K9) mais
/// os campos de integridade do envelope, nunca sobre o plaintext.
pub fn sign(mensagem: &[u8], seed_privada: &[u8; TAMANHO_CHAVE_IDENTIDADE]) -> Vec<u8> {
    let signing_key = SigningKey::from_bytes(seed_privada);
    signing_key.sign(mensagem).to_bytes().to_vec()
}

/// Verifica a assinatura Ed25519 sobre `mensagem`.
///
/// # Erros
/// - `TamanhoDeAssinaturaInvalido` — assinatura com ≠ 64 bytes;
/// - `ChavePublicaInvalida` — os 32 bytes não formam ponto válido;
/// - `AssinaturaInvalida` — assinatura não corresponde à mensagem.
pub fn verify(
    mensagem: &[u8],
    assinatura: &[u8],
    chave_publica: &[u8; TAMANHO_CHAVE_IDENTIDADE],
) -> Result<(), CryptoError> {
    if assinatura.len() != TAMANHO_ASSINATURA {
        return Err(CryptoError::TamanhoDeAssinaturaInvalido {
            esperado: TAMANHO_ASSINATURA,
            obtido: assinatura.len(),
        });
    }
    // `from_bytes` devolve `Result` na ed25519-dalek 2.x — tratamos
    // explicitamente em vez de fazer `unwrap` (era o bug E0599).
    let verifying_key =
        VerifyingKey::from_bytes(chave_publica).map_err(|_| CryptoError::ChavePublicaInvalida)?;
    let mut sig = [0u8; TAMANHO_ASSINATURA];
    sig.copy_from_slice(assinatura);
    let signature = Signature::from_bytes(&sig);
    // `verify_strict` e não `verify`: a verificação tolerante aceita
    // assinaturas de "lixo" (ex.: pub e assinatura todas a zeros) quando
    // a chave não é um ponto da curva — a forjar um handshake de amizade
    // inteiro. A forma estrita rejeita pontos de pequena ordem, R/A não
    // canónicos e a equação de verificação sem cofator.
    verifying_key
        .verify_strict(mensagem, &signature)
        .map_err(|_| CryptoError::AssinaturaInvalida)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Roundtrip assinar→verificar com identidade recém-gerada.
    #[test]
    fn sign_verify_roundtrip() {
        let (seed, publica) = gerar_identidade();
        let msg = b"assinatura estabiliza a identidade";

        let sig = sign(msg, &seed);
        assert_eq!(sig.len(), TAMANHO_ASSINATURA);
        assert!(verify(msg, &sig, &publica).is_ok());
    }

    // A identidade derivada da seed tem de coincidir com a gerada.
    #[test]
    fn chave_publica_deterministica() {
        let (seed, pub_a) = gerar_identidade();
        let pub_b = chave_publica_a_partir_da_seed(&seed);
        assert_eq!(pub_a, pub_b);
        // Duas gerações independentes não podem colidir.
        let (outra_seed, outra_pub) = gerar_identidade();
        assert_ne!(seed, outra_seed);
        assert_ne!(pub_a, outra_pub);
    }

    // Mensagem alterada → `AssinaturaInvalida` (rejeição no receptor).
    #[test]
    fn mensagem_alterada_rejeitada() {
        let (seed, publica) = gerar_identidade();
        let sig = sign(b"mensagem original", &seed);
        assert_eq!(
            verify(b"mensagem alterada", &sig, &publica).unwrap_err(),
            CryptoError::AssinaturaInvalida
        );
    }

    // Assinatura de outro utilizador não valida na chave pública esperada.
    #[test]
    fn identidade_trocada_rejeitada() {
        let (_seed_a, pub_a) = gerar_identidade();
        let (seed_b, _pub_b) = gerar_identidade();
        let sig = sign(b"msg", &seed_b);
        assert_eq!(
            verify(b"msg", &sig, &pub_a).unwrap_err(),
            CryptoError::AssinaturaInvalida
        );
        // Sanity: a mesma assinatura valida com a chave certa.
        assert!(verify(b"msg", &sig, &chave_publica_a_partir_da_seed(&seed_b)).is_ok());
    }

    // Tamanho errado de assinatura → erro estruturado imediato.
    #[test]
    fn assinatura_tamanho_errado() {
        let (_seed, publica) = gerar_identidade();
        let erro = verify(b"msg", &[0u8; 32], &publica).unwrap_err();
        assert_eq!(
            erro,
            CryptoError::TamanhoDeAssinaturaInvalido {
                esperado: 64,
                obtido: 32
            }
        );
        // Tamanho zero também é rejeitado.
        assert!(matches!(
            verify(b"msg", &[], &publica).unwrap_err(),
            CryptoError::TamanhoDeAssinaturaInvalido { .. }
        ));
    }

    // Bytes que não formam ponto Ed25519 válido → `ChavePublicaInvalida`.
    #[test]
    fn chave_publica_invalida() {
        let (seed, _pub) = gerar_identidade();
        let sig = sign(b"msg", &seed);
        // y = 2 (bytes [2,0,…]) não corresponde a nenhum ponto da
        // curva Ed25519 → `from_bytes` devolve `Err`.
        let mut invalida = [0u8; 32];
        invalida[0] = 2;
        assert_eq!(
            verify(b"msg", &sig, &invalida).unwrap_err(),
            CryptoError::ChavePublicaInvalida
        );
    }

    // Assinatura corrompida (bit-flip) → `AssinaturaInvalida`.
    #[test]
    fn assinatura_corrompida() {
        let (seed, publica) = gerar_identidade();
        let mut sig = sign(b"integridade", &seed);
        sig[10] ^= 0x01;
        assert_eq!(
            verify(b"integridade", &sig, &publica).unwrap_err(),
            CryptoError::AssinaturaInvalida
        );
    }

    // Mensagem vazia também é assinável (roundtrip de bordo).
    #[test]
    fn mensagem_vazia() {
        let (seed, publica) = gerar_identidade();
        let sig = sign(b"", &seed);
        assert!(verify(b"", &sig, &publica).is_ok());
    }
}
