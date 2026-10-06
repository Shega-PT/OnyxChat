// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// hill.rs — camada K7 do pipeline: cifra de Hill 2×2 em Z/256Z
// ---------------------------------------------------------------------
// Matriz fixa do projeto: [[3,3],[2,5]]  (det = 9; 9⁻¹ mod 256 = 57)
// Inversa: [[29,85],[142,171]]  — validada por teste de vector fixo e
// por roundtrip em todos os tamanhos de buffer.
//
// Operação por bloco [a, b]:
//   cifra:  [3a+3b, 2a+5b] mod 256
//   decifra: [29a+85b, 142a+171b] mod 256
//
// A matriz é bijetiva em Z/256Z (det invertível), mas o buffer precisa
// de padding PKCS#7 com bloco de 2 para ficar par — removido logo na
// decifragem de K7 (antes de K6⁻¹), tal como especificado em
// `docs/pipeline.md`.
//
// Implementação própria em vez de nalgebra: esta biblioteca não modela
// aritmética modular (Z/256Z) e a matriz 2×2 não justifica a
// dependência.
// =====================================================================

use crate::error::CryptoError;

/// Dimensão da matriz/bloco de Hill (2 bytes por bloco).
pub const DIMENSAO: usize = 2;

/// Matriz de cifragem de K7: [[3,3],[2,5]].
pub const MATRIZ: [[u8; DIMENSAO]; DIMENSAO] = [[3, 3], [2, 5]];

/// Matriz inversa de K7 em Z/256Z: [[29,85],[142,171]].
pub const MATRIZ_INVERSA: [[u8; DIMENSAO]; DIMENSAO] = [[29, 85], [142, 171]];

/// Padding PKCS#7 para o tamanho de bloco de K7.
fn pad_pkcs7(dados: &[u8]) -> Vec<u8> {
    // Nº de bytes necessários para completar o bloco (1..=DIMENSAO;
    // quando já está alinhado, adiciona um bloco inteiro — PKCS#7).
    let falta = DIMENSAO - (dados.len() % DIMENSAO);
    let mut out = Vec::with_capacity(dados.len() + falta);
    out.extend_from_slice(dados);
    out.extend(std::iter::repeat_n(falta as u8, falta));
    out
}

/// Remove o padding PKCS#7 de K7 (`Err(PaddingInvalido)` se adulterado).
fn unpad_pkcs7(dados: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let Some(&ultimo) = dados.last() else {
        return Err(CryptoError::PaddingInvalido { camada: "K7" });
    };
    let n = ultimo as usize;
    // PKCS#7 válido: n ∈ 1..=DIMENSAO e os últimos n bytes = n.
    if n == 0 || n > DIMENSAO || n > dados.len() {
        return Err(CryptoError::PaddingInvalido { camada: "K7" });
    }
    if !dados[dados.len() - n..].iter().all(|&b| b == ultimo) {
        return Err(CryptoError::PaddingInvalido { camada: "K7" });
    }
    Ok(dados[..dados.len() - n].to_vec())
}

/// Aplica a matriz a um bloco de 2 bytes (multiplicação módulo 256).
fn aplicar_matriz(matriz: &[[u8; DIMENSAO]; DIMENSAO], bloco: [u8; DIMENSAO]) -> [u8; DIMENSAO] {
    // u32 evita overflow (171·255 + 171·255 = 87 210 > u16::MAX);
    // o cast final para u8 descarta os bits altos ≡ reduce mod 256.
    let a = bloco[0] as u32;
    let b = bloco[1] as u32;
    [
        (matriz[0][0] as u32 * a + matriz[0][1] as u32 * b) as u8,
        (matriz[1][0] as u32 * a + matriz[1][1] as u32 * b) as u8,
    ]
}

/// Cifra `dados` com K7: padding PKCS#7 + transformação de Hill.
pub fn cifrar(dados: &[u8]) -> Vec<u8> {
    let com_pad = pad_pkcs7(dados);
    com_pad
        .chunks(DIMENSAO)
        .flat_map(|bloco| aplicar_matriz(&MATRIZ, [bloco[0], bloco[1]]))
        .collect()
}

/// Decifra o output de K7: Hill inverso + remoção do padding.
///
/// # Erros
/// - `BlocoIncompleto` — comprimento ímpar (bloco de 2 incompleto);
/// - `PaddingInvalido` — padding PKCS#7 ausente ou adulterado.
pub fn decifrar(ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if !ciphertext.len().is_multiple_of(DIMENSAO) {
        return Err(CryptoError::BlocoIncompleto {
            esperado: DIMENSAO,
            obtido: ciphertext.len() % DIMENSAO,
        });
    }
    let claro: Vec<u8> = ciphertext
        .chunks(DIMENSAO)
        .flat_map(|bloco| aplicar_matriz(&MATRIZ_INVERSA, [bloco[0], bloco[1]]))
        .collect();
    unpad_pkcs7(&claro)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vector conhecido: "AB" → [3·65+3·66, 2·65+5·66] = [137, 204].
    #[test]
    fn vector_conhecido() {
        // "AB" (2 bytes = bloco cheio) → PKCS#7 acrescenta [2,2]:
        // Hill([65,66]) = [137,204] e Hill([2,2]) = [12,14].
        assert_eq!(cifrar(b"AB"), vec![0x89, 0xCC, 0x0C, 0x0E]);
        assert_eq!(decifrar(&[0x89, 0xCC, 0x0C, 0x0E]).unwrap(), b"AB");
        // Output truncado (metade perdida) → padding inválido.
        assert_eq!(
            decifrar(&[0x89, 0xCC]).unwrap_err(),
            CryptoError::PaddingInvalido { camada: "K7" }
        );
    }

    // Roundtrip para todos os comprimentos 0..=17 (cobre pares e ímpares).
    #[test]
    fn roundtrip_todos_tamanhos() {
        for n in 0..=17usize {
            let dados: Vec<u8> = (0..n as u8).map(|i| i.wrapping_mul(37)).collect();
            let ct = cifrar(&dados);
            assert_eq!(ct.len() % DIMENSAO, 0, "output tem de ser par");
            assert_eq!(decifrar(&ct).unwrap(), dados, "roundtrip n={n}");
        }
    }

    // O padding amplia sempre o buffer (nunca encolhe) e é removido.
    #[test]
    fn padding_pkcs7() {
        assert_eq!(pad_pkcs7(&[]), vec![2, 2]); // vazio → bloco inteiro
        assert_eq!(pad_pkcs7(&[1]), vec![1, 1]); // ímpar → 1 byte
        assert_eq!(pad_pkcs7(&[1, 2]), vec![1, 2, 2, 2]); // par → bloco
                                                          // Remoção: n=1 remove 1 byte; n=2 exige os 2 últimos = 2.
        assert_eq!(unpad_pkcs7(&[9, 1]).unwrap(), vec![9]);
        assert_eq!(unpad_pkcs7(&[9, 2, 2]).unwrap(), vec![9]);
        assert_eq!(unpad_pkcs7(&[9, 9, 2, 2]).unwrap(), vec![9, 9]);
    }

    // Buffers adulterados/curtos produzem erros estruturados.
    #[test]
    fn erros_estruturados() {
        // Comprimento ímpar → bloco de 2 incompleto.
        assert_eq!(
            decifrar(&[1, 2, 3]).unwrap_err(),
            CryptoError::BlocoIncompleto {
                esperado: 2,
                obtido: 1
            }
        );
        // Vazio → não existe último byte para ler o padding.
        assert_eq!(
            decifrar(&[]).unwrap_err(),
            CryptoError::PaddingInvalido { camada: "K7" }
        );
        // Último byte 0 não é padding PKCS#7 válido.
        assert_eq!(
            unpad_pkcs7(&[9, 0]).unwrap_err(),
            CryptoError::PaddingInvalido { camada: "K7" }
        );
        // n = 3 maior que o bloco de 2 → inválido.
        assert_eq!(
            unpad_pkcs7(&[1, 3]).unwrap_err(),
            CryptoError::PaddingInvalido { camada: "K7" }
        );
        // n = 2 mas o corpo só tem 1 byte (insuficiente).
        assert_eq!(
            unpad_pkcs7(&[5, 2]).unwrap_err(),
            CryptoError::PaddingInvalido { camada: "K7" }
        );
        // n = 1: só o último byte conta (é sempre consistente) → válido.
        assert_eq!(unpad_pkcs7(&[7, 2, 2, 1]).unwrap(), vec![7, 2, 2]);
        // Último byte = 2 mas o anterior também tem de ser 2.
        assert_eq!(
            unpad_pkcs7(&[7, 5, 9, 2]).unwrap_err(),
            CryptoError::PaddingInvalido { camada: "K7" }
        );
        // n = 2 maior que os dados disponíveis (len 1) → inválido.
        assert_eq!(
            unpad_pkcs7(&[2]).unwrap_err(),
            CryptoError::PaddingInvalido { camada: "K7" }
        );
    }

    // A inversa multiplica à identidade: M·M⁻¹ = I (mod 256).
    #[test]
    fn inversa_valida() {
        for a in 0..=3u16 {
            for b in 0..=3u16 {
                // aplica M⁻¹ ∘ M sobre todos os pares pequenos
                let bloco = [(a * 61) as u8, (b * 97) as u8];
                let cifrado = aplicar_matriz(&MATRIZ, bloco);
                let volta = aplicar_matriz(&MATRIZ_INVERSA, cifrado);
                assert_eq!(volta, bloco, "inversa falhou para {bloco:?}");
            }
        }
    }

    // O output de K7 nunca revela o plaintext diretamente (ofuscação).
    #[test]
    fn difunde_bytes() {
        let ct = cifrar(b"AAAA");
        assert_ne!(&ct[..2], &b"AA"[..]);
        assert_ne!(
            ct,
            cifrar(b"AAAB"),
            "blocos diferentes → outputs diferentes"
        );
    }
}
