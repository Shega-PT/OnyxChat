// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// rng.rs — CSPRNG do sistema (fonte de aleatoriedade única do projeto)
// ---------------------------------------------------------------------
// Todos os nonces e chaves do OnyxChat saem daqui: `OsRng` do rand
// (getrandom/OS). Centralizar a geração num só sítio garante que
// nenhum código recorre a PRNGs previsíveis ou a `rand::rng()` (que é
// apenas thread-local, não adequado para segredos de longa duração).
// =====================================================================

use rand::rngs::OsRng;
use rand::TryRngCore;

/// Preenche `destino` com bytes criptograficamente aleatórios.
///
/// Em Linux o `OsRng` lê `/dev/urandom` via `getrandom(2)`; a falha é
/// praticamente impossível (exige boot incompleto) e, como não há como
/// produzir material seguro sem entropia, abortamos com mensagem clara
/// em vez de gerar chaves fracas.
pub fn preencher(destino: &mut [u8]) {
    OsRng
        .try_fill_bytes(destino)
        .expect("CSPRNG do sistema indisponível: sem entropia, recuso-me a continuar");
}

#[cfg(test)]
mod tests {
    use super::*;

    // O preenchimento escreve todos os bytes e não devolve lixo óbvio.
    #[test]
    fn preenche_totamente() {
        let mut buf = [0u8; 64];
        preencher(&mut buf);
        // Um buffer de 64 bytes aleatórios nunca fica todo igual a zero
        // nem todo idêntico (probabilidade < 2^-500).
        assert!(buf.iter().any(|&b| b != 0));
        assert!(buf.iter().any(|&b| b != buf[0]));

        // Duas chamadas consecutivas não repetem o mesmo padrão.
        let mut outro = [0u8; 64];
        preencher(&mut outro);
        assert_ne!(buf, outro);
    }

    // Vários tamanhos (12B de nonce, 32B de chave) funcionam.
    #[test]
    fn tamanhos_comuns() {
        let mut nonce = [0u8; 12];
        let mut chave = [0u8; 32];
        preencher(&mut nonce);
        preencher(&mut chave);
        assert_ne!(nonce, [0u8; 12]);
        assert_ne!(chave, [0u8; 32]);
    }
}
