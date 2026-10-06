// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// erros.rs — erro unificado do pipeline K1→K9 do daemon
// ---------------------------------------------------------------------
// Concentra todas as falhas possíveis de cifrar/decifrar e a conversão
// para os códigos de estado do protocolo IPC (`docs/ipc_spec.md`):
//
//   EnvelopeInvalido   → 0x06   AssinaturaInvalida → 0x04
//   DecifragemFalhou   → 0x05   CifragemFalhou     → 0x07
//   ChaveInvalida      → 0x03   TextoInvalidoUtf8  → 0x08
//   PayloadGrande      → 0x09   NonceRepetido      → 0x13
//   Lua                → 0x07 (ENCODE) / 0x05 (DECODE)
//
// Nenhuma variante transporta chaves, nonces ou payloads — apenas
// metadados legíveis (regra: logging sem dados sensíveis).
// =====================================================================

use std::fmt;

use crate::envelope::MIN_PLAINTEXT;
use crate::ffi_c::FfiErro;
use crypto_core::CryptoError;

/// Falha em qualquer ponto do pipeline ou do parsing do envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroPipeline {
    /// Envelope malformado (comprimento mínimo / versão) — IPC `0x06`.
    EnvelopeInvalido(String),
    /// Assinatura Ed25519 rejeitada — IPC `0x04`.
    AssinaturaInvalida,
    /// Decifragem falhou (tag AEAD, padding ou bloco) — IPC `0x05`.
    DecifragemFalhou(String),
    /// Falha interna de cifragem — IPC `0x07`.
    CifragemFalhou(String),
    /// Tamanho de chave inválido — IPC `0x03`.
    ChaveInvalida { esperado: usize, obtido: usize },
    /// O plaintext final não é UTF-8 válido — IPC `0x08`.
    TextoInvalidoUtf8,
    /// Campo acima do limite do protocolo (`MAX_PLAINTEXT` ou
    /// `MAX_ENVELOPE`) — IPC `0x09`, rejeitado antes de processar.
    PayloadGrande { obtido: usize, maximo: usize },
    /// Plaintext vazio — IPC `0x02` (`PayloadMalformado`).
    ///
    /// Existe como variante própria, e não como `PayloadGrande`, porque
    /// a semântica é diferente: `0x09` diz «excede o limite», `0x02` diz
    /// «não é um pedido válido». Um envelope de mensagem vazia é um
    /// input degenerado, não um payload grande demais — e um cliente que
    /// receba `0x09` para um texto vazio não consegue corrigir-se.
    ///
    /// Rejeitado à entrada do pipeline (`MIN_PLAINTEXT = 1`), antes de K1.
    /// Ver `docs/pipeline.md` §Limites de tamanho.
    PayloadVazio,
    /// `nonce1` já registado no anti-replay do chat — IPC `0x13`.
    /// Só alcançável com assinatura válida (o nonce é coberto por ela),
    /// ou seja, é sempre o resultado de um reenvio de mensagem.
    NonceRepetido,
    /// Erro vindo do embedding Lua (K2/K3/K6/K8); a mensagem vem do
    /// próprio interpretador e contém apenas literais do script.
    Lua(String),
}

impl ErroPipeline {
    /// Código de erro IPC (`docs/ipc_spec.md`) e legenda estável.
    ///
    /// `em_cifragem` distingue a semântica do erro Lua: durante ENCODE
    /// é uma falha interna de cifragem (0x07); durante DECODE indica
    /// falha de decifragem (0x05).
    pub fn para_ipc(&self, em_cifragem: bool) -> u8 {
        match self {
            ErroPipeline::EnvelopeInvalido(_) => 0x06,
            ErroPipeline::AssinaturaInvalida => 0x04,
            ErroPipeline::DecifragemFalhou(_) => 0x05,
            ErroPipeline::CifragemFalhou(_) => 0x07,
            ErroPipeline::ChaveInvalida { .. } => 0x03,
            ErroPipeline::TextoInvalidoUtf8 => 0x08,
            ErroPipeline::PayloadGrande { .. } => 0x09,
            ErroPipeline::PayloadVazio => 0x02,
            ErroPipeline::NonceRepetido => 0x13,
            ErroPipeline::Lua(_) => {
                if em_cifragem {
                    0x07
                } else {
                    0x05
                }
            }
        }
    }
}

impl fmt::Display for ErroPipeline {
    /// Mensagem legível para a resposta IPC — nunca com dados sensíveis.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroPipeline::EnvelopeInvalido(m) => write!(f, "{m}"),
            ErroPipeline::AssinaturaInvalida => write!(f, "assinatura Ed25519 inválida"),
            ErroPipeline::DecifragemFalhou(m) => write!(f, "decifragem falhou: {m}"),
            ErroPipeline::CifragemFalhou(m) => write!(f, "cifragem falhou: {m}"),
            ErroPipeline::ChaveInvalida { esperado, obtido } => write!(
                f,
                "tamanho de chave inválido: esperado {esperado} bytes, obtido {obtido}"
            ),
            ErroPipeline::TextoInvalidoUtf8 => {
                write!(f, "conteúdo não é UTF-8 válido")
            }
            ErroPipeline::PayloadGrande { obtido, maximo } => {
                write!(
                    f,
                    "payload demasiado grande: {obtido} bytes (máximo {maximo})"
                )
            }
            ErroPipeline::PayloadVazio => {
                // A mensagem diz o mínimo, para que o cliente saiba o
                // que corrigir sem consultar a documentação.
                write!(f, "mensagem vazia (mínimo {MIN_PLAINTEXT} byte)")
            }
            ErroPipeline::NonceRepetido => {
                write!(f, "nonce1 já visto (mensagem repetida)")
            }
            ErroPipeline::Lua(m) => write!(f, "camada Lua: {m}"),
        }
    }
}

// ---------------------------------------------------------------------
// Conversões a partir das bibliotecas internas — cada braço é um ponto
// de mapeamento documentado e coberto por teste unitário próprio.
// ---------------------------------------------------------------------

impl From<CryptoError> for ErroPipeline {
    fn from(erro: CryptoError) -> Self {
        match erro {
            CryptoError::AssinaturaInvalida | CryptoError::ChavePublicaInvalida => {
                ErroPipeline::AssinaturaInvalida
            }
            CryptoError::DecifragemFalhou => {
                ErroPipeline::DecifragemFalhou("tag AEAD inválida".into())
            }
            CryptoError::PaddingInvalido { camada } => {
                ErroPipeline::DecifragemFalhou(format!("padding PKCS#7 inválido na {camada}"))
            }
            CryptoError::BlocoIncompleto { esperado, obtido } => ErroPipeline::DecifragemFalhou(
                format!("bloco incompleto: esperado múltiplo de {esperado}, obtido {obtido}"),
            ),
            CryptoError::CifragemFalhou => {
                ErroPipeline::CifragemFalhou("operação AEAD falhou".into())
            }
            CryptoError::TamanhoDeChaveInvalido { esperado, obtido } => {
                ErroPipeline::ChaveInvalida { esperado, obtido }
            }
            CryptoError::TextoInvalidoUtf8 => ErroPipeline::TextoInvalidoUtf8,
            CryptoError::TamanhoDeNonceInvalido { esperado, obtido } => {
                ErroPipeline::CifragemFalhou(format!(
                    "nonce interno inválido: esperado {esperado}, obtido {obtido}"
                ))
            }
            CryptoError::TamanhoDeAssinaturaInvalido { esperado, obtido } => {
                ErroPipeline::CifragemFalhou(format!(
                    "assinatura com tamanho inválido: esperado {esperado}, obtido {obtido}"
                ))
            }
        }
    }
}

impl From<FfiErro> for ErroPipeline {
    fn from(erro: FfiErro) -> Self {
        match erro {
            FfiErro::Auth => ErroPipeline::DecifragemFalhou("tag AEAD inválida (camada C)".into()),
            FfiErro::Padding => {
                ErroPipeline::DecifragemFalhou("padding PKCS#7 inválido (camada C)".into())
            }
            FfiErro::Tamanho => {
                ErroPipeline::DecifragemFalhou("comprimento inválido (camada C)".into())
            }
            FfiErro::Argumento | FfiErro::Capacidade | FfiErro::Crypto => {
                ErroPipeline::CifragemFalhou(erro.descricao().to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cada variante de `CryptoError` mapeia para o erro de pipeline
    /// esperado (tabela de conversão completa, sem braços por verificar).
    #[test]
    fn mapeamento_de_crypto_error() {
        assert_eq!(
            ErroPipeline::from(CryptoError::AssinaturaInvalida),
            ErroPipeline::AssinaturaInvalida
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::ChavePublicaInvalida),
            ErroPipeline::AssinaturaInvalida
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::DecifragemFalhou),
            ErroPipeline::DecifragemFalhou("tag AEAD inválida".into())
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::PaddingInvalido { camada: "K7" }),
            ErroPipeline::DecifragemFalhou("padding PKCS#7 inválido na K7".into())
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::BlocoIncompleto {
                esperado: 2,
                obtido: 1
            }),
            ErroPipeline::DecifragemFalhou(
                "bloco incompleto: esperado múltiplo de 2, obtido 1".into()
            )
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::CifragemFalhou),
            ErroPipeline::CifragemFalhou("operação AEAD falhou".into())
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::TamanhoDeChaveInvalido {
                esperado: 32,
                obtido: 16
            }),
            ErroPipeline::ChaveInvalida {
                esperado: 32,
                obtido: 16
            }
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::TextoInvalidoUtf8),
            ErroPipeline::TextoInvalidoUtf8
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::TamanhoDeNonceInvalido {
                esperado: 12,
                obtido: 8
            }),
            ErroPipeline::CifragemFalhou("nonce interno inválido: esperado 12, obtido 8".into())
        );
        assert_eq!(
            ErroPipeline::from(CryptoError::TamanhoDeAssinaturaInvalido {
                esperado: 64,
                obtido: 4
            }),
            ErroPipeline::CifragemFalhou(
                "assinatura com tamanho inválido: esperado 64, obtido 4".into()
            )
        );
    }

    /// Cada variante de `FfiErro` mapeia para o erro de pipeline.
    #[test]
    fn mapeamento_de_ffi_erro() {
        assert_eq!(
            ErroPipeline::from(FfiErro::Auth),
            ErroPipeline::DecifragemFalhou("tag AEAD inválida (camada C)".into())
        );
        assert_eq!(
            ErroPipeline::from(FfiErro::Padding),
            ErroPipeline::DecifragemFalhou("padding PKCS#7 inválido (camada C)".into())
        );
        assert_eq!(
            ErroPipeline::from(FfiErro::Tamanho),
            ErroPipeline::DecifragemFalhou("comprimento inválido (camada C)".into())
        );
        assert_eq!(
            ErroPipeline::from(FfiErro::Argumento),
            ErroPipeline::CifragemFalhou("argumento inválido na camada C".into())
        );
        assert_eq!(
            ErroPipeline::from(FfiErro::Capacidade),
            ErroPipeline::CifragemFalhou("buffer de saída insuficiente na camada C".into())
        );
        assert_eq!(
            ErroPipeline::from(FfiErro::Crypto),
            ErroPipeline::CifragemFalhou("falha interna da camada C".into())
        );
    }

    /// A tabela de códigos IPC cobre todas as variantes, nos dois lados.
    #[test]
    fn codigos_ipc_de_todas_as_variantes() {
        assert_eq!(
            ErroPipeline::EnvelopeInvalido("x".into()).para_ipc(true),
            0x06
        );
        assert_eq!(ErroPipeline::AssinaturaInvalida.para_ipc(false), 0x04);
        assert_eq!(
            ErroPipeline::DecifragemFalhou("x".into()).para_ipc(false),
            0x05
        );
        assert_eq!(
            ErroPipeline::CifragemFalhou("x".into()).para_ipc(true),
            0x07
        );
        assert_eq!(
            ErroPipeline::ChaveInvalida {
                esperado: 32,
                obtido: 1
            }
            .para_ipc(true),
            0x03
        );
        assert_eq!(ErroPipeline::TextoInvalidoUtf8.para_ipc(false), 0x08);
        // Payload acima do limite → 0x09 (nos dois lados da semântica).
        assert_eq!(
            ErroPipeline::PayloadGrande {
                obtido: 70_000,
                maximo: 65_692
            }
            .para_ipc(true),
            0x09
        );
        assert_eq!(
            ErroPipeline::PayloadGrande {
                obtido: 70_000,
                maximo: 65_692
            }
            .para_ipc(false),
            0x09
        );
        // Replay de chat (nonce1 repetido) → 0x13.
        assert_eq!(ErroPipeline::NonceRepetido.para_ipc(false), 0x13);
        assert_eq!(ErroPipeline::NonceRepetido.para_ipc(true), 0x13);
        // Lua: 0x07 em cifragem, 0x05 em decifragem.
        assert_eq!(ErroPipeline::Lua("e".into()).para_ipc(true), 0x07);
        assert_eq!(ErroPipeline::Lua("e".into()).para_ipc(false), 0x05);
    }

    /// O `Display` de todas as variantes é estável e sem dados sensíveis.
    #[test]
    fn display_de_todas_as_variantes() {
        assert_eq!(
            ErroPipeline::EnvelopeInvalido("versão inválida".into()).to_string(),
            "versão inválida"
        );
        assert_eq!(
            ErroPipeline::AssinaturaInvalida.to_string(),
            "assinatura Ed25519 inválida"
        );
        assert_eq!(
            ErroPipeline::DecifragemFalhou("tag".into()).to_string(),
            "decifragem falhou: tag"
        );
        assert_eq!(
            ErroPipeline::CifragemFalhou("interna".into()).to_string(),
            "cifragem falhou: interna"
        );
        assert_eq!(
            ErroPipeline::ChaveInvalida {
                esperado: 32,
                obtido: 10
            }
            .to_string(),
            "tamanho de chave inválido: esperado 32 bytes, obtido 10"
        );
        assert_eq!(
            ErroPipeline::TextoInvalidoUtf8.to_string(),
            "conteúdo não é UTF-8 válido"
        );
        assert_eq!(
            ErroPipeline::Lua("chave vazia".into()).to_string(),
            "camada Lua: chave vazia"
        );
        // O limite é sempre explícito no erro (obtido + máximo).
        assert_eq!(
            ErroPipeline::PayloadGrande {
                obtido: 70_000,
                maximo: 65_692
            }
            .to_string(),
            "payload demasiado grande: 70000 bytes (máximo 65692)"
        );
        // A mensagem de replay nunca expõe o próprio nonce.
        assert_eq!(
            ErroPipeline::NonceRepetido.to_string(),
            "nonce1 já visto (mensagem repetida)"
        );
    }
}
