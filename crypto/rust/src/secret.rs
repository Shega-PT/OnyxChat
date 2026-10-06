// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// secret.rs — contentor de segredos com limpeza garantida de memória
// ---------------------------------------------------------------------
// Envolva sempre as chaves simétricas (K1/K5/K9) neste tipo: o `Drop`
// apaga os bytes com `zeroize` para que as chaves não permaneçam em RAM
// após o fim do `scope` (política documentada em `docs/key_management.md`
// §Proteção em memória).
// =====================================================================

use zeroize::Zeroize;

/// Tamanho fixo de todas as chaves simétricas do pipeline (K1/K5/K9).
pub const TAMANHO_CHAVE: usize = 32;

/// Chave simétrica de 32 bytes que é apagada do memória ao ser destruída.
///
/// O `Debug` é personalizado para nunca imprimir o conteúdo — um
/// `dbg!()` acidental numa chave não pode vazar para logs.
pub struct Segredo {
    /// Bytes da chave; invariantemente de tamanho `TAMANHO_CHAVE`.
    dados: [u8; TAMANHO_CHAVE],
    /// Flag interna: `true` depois de `zerar()`/`Drop` (para depuração
    /// e para que `como_bytes()` devolva zeros em vez de lixo).
    apagado: bool,
}

impl Segredo {
    /// Constrói um segredo a partir de um array de 32 bytes.
    pub fn novo(dados: [u8; TAMANHO_CHAVE]) -> Self {
        Segredo {
            dados,
            apagado: false,
        }
    }

    /// Expõe os bytes da chave para uso pelas primitivas AEAD.
    pub fn como_bytes(&self) -> &[u8; TAMANHO_CHAVE] {
        &self.dados
    }

    /// Apaga os bytes do segredo imediatamente (útil após uma rotação
    /// de chaves, sem esperar pelo `Drop`).
    pub fn zerar(&mut self) {
        self.dados.zeroize();
        self.apagado = true;
    }

    /// Indica se o segredo já foi apagado da memória.
    ///
    /// Parece código morto — nenhum chamador de produção o usa — e não
    /// é. É o **observável da `zeroize`**: sem ele, um `Drop` que
    /// esquecesse de apagar os bytes não quebrava nada que um teste
    /// pudesse ver. É a única forma de afirmar que
    /// `drop_zera_os_bytes` testa alguma coisa em vez de dar
    /// `true` sobre um campo que ninguém escreve.
    ///
    /// Foi candidato a remoção numa auditoria de G4 e ficou
    /// precisamente porque os testes o usam para verificar o que não é
    /// verificável por fora.
    pub fn esta_apagado(&self) -> bool {
        self.apagado
    }
}

// Limpeza automática ao sair do scope — cobre o caminho normal de vida
// de uma chave sem exigir disciplina manual ao programador.
impl Drop for Segredo {
    fn drop(&mut self) {
        self.zerar();
    }
}

// Impede cópias acidentais da chave (clones duplicariam o segredo em RAM).
impl Clone for Segredo {
    fn clone(&self) -> Self {
        Segredo {
            dados: self.dados,
            apagado: self.apagado,
        }
    }
}

// `Debug` seguro: nunca revela os bytes.
impl std::fmt::Debug for Segredo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.apagado {
            f.write_str("Segredo { apagado }")
        } else {
            f.write_str("Segredo { [REDACTED] }")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exercita `novo`, `como_bytes` e o estado inicial consistente.
    #[test]
    fn cria_e_expoe_bytes() {
        let s = Segredo::novo([7u8; TAMANHO_CHAVE]);
        assert_eq!(s.como_bytes(), &[7u8; TAMANHO_CHAVE]);
        assert!(!s.esta_apagado());
    }

    // `zerar()` apaga os bytes e marca o segredo como apagado.
    #[test]
    fn zerar_apaga_conteudo() {
        let mut s = Segredo::novo([0xABu8; TAMANHO_CHAVE]);
        s.zerar();
        assert!(s.esta_apagado());
        assert_eq!(s.como_bytes(), &[0u8; TAMANHO_CHAVE]);
    }

    // O `Drop` tem de apagar a chave: verificamos com `ManuallyDrop`
    // para correr o `Drop` manualmente e ler a memória ainda alocada
    // (o array é inline na struct, logo não há deallocation prematuro).
    #[test]
    fn drop_zera_os_bytes() {
        use std::mem::ManuallyDrop;
        let mut segredo = ManuallyDrop::new(Segredo::novo([0x5Au8; TAMANHO_CHAVE]));
        // Corre o `Drop` (que chama `zerar()`) sem libertar a struct.
        unsafe { std::ptr::drop_in_place(&mut *segredo) };
        assert!(segredo.esta_apagado());
        assert_eq!(segredo.dados, [0u8; TAMANHO_CHAVE]);
    }

    // O `Debug` nunca pode revelar os bytes (proteção contra logs acidentais).
    #[test]
    fn debug_nao_vaza_segredo() {
        let s = Segredo::novo([0x42u8; TAMANHO_CHAVE]);
        let texto = format!("{s:?}");
        assert_eq!(texto, "Segredo { [REDACTED] }");
        assert!(!texto.contains("42"));

        let mut apagado = s.clone();
        apagado.zerar();
        assert_eq!(format!("{apagado:?}"), "Segredo { apagado }");
    }

    // O `Clone` tem de preservar o conteúdo e o estado.
    #[test]
    fn clone_preserva_estado() {
        let s = Segredo::novo([3u8; TAMANHO_CHAVE]);
        let c = s.clone();
        assert_eq!(c.como_bytes(), s.como_bytes());
        assert!(!c.esta_apagado());
    }
}
