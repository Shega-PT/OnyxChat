// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// error.rs — tipo de erro unificado do núcleo de criptografia
// ---------------------------------------------------------------------
// Todas as operações públicas do `crypto_core` devolvem
// `Result<_, CryptoError>` em vez de fazer `panic` com `.expect()`:
// o daemon Rust recebe estes erros via IPC e devolve um status de erro
// ao cliente Python (conforme `docs/ipc_spec.md`), nunca rebentando.
// =====================================================================

use std::fmt;

/// Erros produzidos pelas operações de cifragem/decifragem/assinatura.
///
/// Os campos `esperado`/`obtido` são metadados de depuração — nunca
/// contêm chaves, nonces nem payloads (regra: logging sem dados sensíveis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// Chave fornecida com tamanho diferente do exigido (32 bytes).
    TamanhoDeChaveInvalido { esperado: usize, obtido: usize },
    /// Nonce/IV fornecido com tamanho diferente do exigido (12 bytes).
    TamanhoDeNonceInvalido { esperado: usize, obtido: usize },
    /// Assinatura Ed25519 com tamanho diferente de 64 bytes.
    TamanhoDeAssinaturaInvalido { esperado: usize, obtido: usize },
    /// A operação AEAD falhou (chave/nonce malformados ou erro interno).
    CifragemFalhou,
    /// A tag AEAD não confere: ciphertext adulterado ou chave errada.
    DecifragemFalhou,
    /// Os 32 bytes fornecidos não formam um ponto Ed25519 válido.
    ChavePublicaInvalida,
    /// A assinatura Ed25519 não corresponde à mensagem/chave pública.
    AssinaturaInvalida,
    /// O texto decifrado não é UTF-8 válido (payload corrompido).
    ///
    /// **Sem produtor desde G4.** O único código que o produzia era
    /// `decrypt_k1`, um wrapper de uma linha sobre `aead_chacha` que
    /// convertia o plaintext e devolvia `String`; foi removido por não ter
    /// caller. A variante fica porque é parte da taxonomia pública do
    /// crate e porque o daemon tem o equivalente — `ErroPipeline::
    /// TextoInvalidoUtf8`, status IPC `0x08` — que é produzido onde a
    /// conversão vive hoje, em `pipeline::decifrar`.
    ///
    /// Fica escrito para que a próxima auditoria de código morto não a
    /// volte a propor: não é esquecimento, é uma decisão.
    TextoInvalidoUtf8,
    /// Padding PKCS#7 inválido numa camada analógica (K4/K7).
    PaddingInvalido { camada: &'static str },
    /// Dados não alinhados ao tamanho de bloco da camada analógica.
    BlocoIncompleto { esperado: usize, obtido: usize },
}

impl fmt::Display for CryptoError {
    /// Mensagem de erro legível, sempre sem expor material sensível.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::TamanhoDeChaveInvalido { esperado, obtido } => write!(
                f,
                "tamanho de chave inválido: esperado {esperado} bytes, obtido {obtido}"
            ),
            CryptoError::TamanhoDeNonceInvalido { esperado, obtido } => write!(
                f,
                "tamanho de nonce inválido: esperado {esperado} bytes, obtido {obtido}"
            ),
            CryptoError::TamanhoDeAssinaturaInvalido { esperado, obtido } => write!(
                f,
                "tamanho de assinatura inválido: esperado {esperado} bytes, obtido {obtido}"
            ),
            CryptoError::CifragemFalhou => write!(f, "operação de cifragem falhou"),
            CryptoError::DecifragemFalhou => {
                write!(f, "decifragem falhou: tag de autenticação inválida")
            }
            CryptoError::ChavePublicaInvalida => {
                write!(f, "chave pública Ed25519 inválida")
            }
            CryptoError::AssinaturaInvalida => {
                write!(f, "assinatura Ed25519 inválida")
            }
            CryptoError::TextoInvalidoUtf8 => {
                write!(f, "conteúdo decifrado não é UTF-8 válido")
            }
            CryptoError::PaddingInvalido { camada } => {
                write!(f, "padding PKCS#7 inválido na camada {camada}")
            }
            CryptoError::BlocoIncompleto { esperado, obtido } => {
                write!(
                    f,
                    "bloco incompleto: esperado múltiplo de {esperado}, resto {obtido}"
                )
            }
        }
    }
}

// Integração com a trait de erros da biblioteca padrão (necessário para
// `?` em funções que combinam múltiplas camadas e para logging genérico).
impl std::error::Error for CryptoError {}

#[cfg(test)]
mod tests {
    use super::*;

    // Cobre o `Display` de cada variante — garante que nenhuma mensagem
    // de erro vaza chaves/payloads e que o texto é estável.
    #[test]
    fn display_de_todas_as_variantes() {
        assert_eq!(
            CryptoError::TamanhoDeChaveInvalido {
                esperado: 32,
                obtido: 16
            }
            .to_string(),
            "tamanho de chave inválido: esperado 32 bytes, obtido 16"
        );
        assert_eq!(
            CryptoError::TamanhoDeNonceInvalido {
                esperado: 12,
                obtido: 8
            }
            .to_string(),
            "tamanho de nonce inválido: esperado 12 bytes, obtido 8"
        );
        assert_eq!(
            CryptoError::TamanhoDeAssinaturaInvalido {
                esperado: 64,
                obtido: 32
            }
            .to_string(),
            "tamanho de assinatura inválido: esperado 64 bytes, obtido 32"
        );
        assert_eq!(
            CryptoError::CifragemFalhou.to_string(),
            "operação de cifragem falhou"
        );
        assert_eq!(
            CryptoError::DecifragemFalhou.to_string(),
            "decifragem falhou: tag de autenticação inválida"
        );
        assert_eq!(
            CryptoError::ChavePublicaInvalida.to_string(),
            "chave pública Ed25519 inválida"
        );
        assert_eq!(
            CryptoError::AssinaturaInvalida.to_string(),
            "assinatura Ed25519 inválida"
        );
        assert_eq!(
            CryptoError::TextoInvalidoUtf8.to_string(),
            "conteúdo decifrado não é UTF-8 válido"
        );
        assert_eq!(
            CryptoError::PaddingInvalido { camada: "K7" }.to_string(),
            "padding PKCS#7 inválido na camada K7"
        );
        assert_eq!(
            CryptoError::BlocoIncompleto {
                esperado: 2,
                obtido: 1
            }
            .to_string(),
            "bloco incompleto: esperado múltiplo de 2, resto 1"
        );
    }

    // O tipo tem de ser tratável como `std::error::Error` (usado pelo
    // daemon ao converter erros para o protocolo IPC de status).
    #[test]
    fn integra_com_std_error() {
        fn aceita_erro(e: &(dyn std::error::Error + 'static)) -> String {
            e.to_string()
        }
        let e: Box<dyn std::error::Error> = Box::new(CryptoError::CifragemFalhou);
        assert_eq!(aceita_erro(e.as_ref()), "operação de cifragem falhou");
    }

    // `Debug` é usado em logs do daemon — tem de existir e ser estável.
    #[test]
    fn debug_e_utilizavel() {
        let e = CryptoError::AssinaturaInvalida;
        assert!(format!("{e:?}").contains("AssinaturaInvalida"));
    }
}
