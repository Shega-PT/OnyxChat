// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// pipeline.rs — orquestração completa do pipeline K1→K9 (envelope)
// ---------------------------------------------------------------------
// Cifragem (ENCODE):
//   K1 ChaCha20 (Rust) → K2 Cesariana (Lua) → K3 Vigenère (Lua) →
//   K4 Transposição (C++) → K5 AES-GCM (Rust) → K6 Playfair (Lua) →
//   K7 Hill (Rust) → K8 Substituição (Lua) → K9 ChaCha20 (C) →
//   envelope + assinatura Ed25519.
//
// Decifragem (DECODE): exatamente inverso, com a assinatura validada
// ANTES de qualquer operação de decifragem (docs/message_format.md).
//
// Parâmetros fixos conforme `docs/pipeline.md` — todos centralizados
// em constantes para garantir paridade com as referências Python/Lua.
// =====================================================================

use crate::anti_replay::RegistoNonce1;
use crate::envelope::{Envelope, MAX_PLAINTEXT, MIN_PLAINTEXT};
use crate::erros::ErroPipeline;
use crate::{ffi_c, lua_camadas};

/// Chave de sessão K1 — ChaCha20-Poly1305, chave única do par.
pub type Chave32 = [u8; 32];

/// Deslocamento da Cesariana K2 (docs/pipeline.md).
pub const DESLOCAMENTO_K2: i64 = 3;
/// Chave da Vigenère K3 (8 bytes ASCII).
pub const CHAVE_K3: &[u8] = b"VIGENERE";
/// Chave do Playfair adaptado K6 (8 bytes ASCII).
pub const CHAVE_K6: &[u8] = b"PLAYFAIR";
/// Deslocamento da substituição K8 (permutação bijetiva).
pub const DESLOCAMENTO_K8: i64 = 7;

/// Chaves de uma sessão de mensagem (as três camadas AEAD).
///
/// Todos os campos são 32 bytes; os tamanhos são validados pelo
/// chamador (IPC) antes de construir a struct.
///
/// **Zeroização (Resumo §22):** não é `Copy` — cada instância apaga
/// os três arrays no descarte, para que chaves de sessão não fiquem
/// em RAM depois de já não serem necessárias (política em
/// `docs/key_management.md`).
#[derive(Debug, Clone)]
pub struct Chaves {
    /// K1 — chave única do par (remetente + receptor).
    pub k1: Chave32,
    /// K5 — chave única do remetente.
    pub k5: Chave32,
    /// K9 — chave única do receptor.
    pub k9: Chave32,
}

impl Drop for Chaves {
    /// Apaga K1/K5/K9 no descarte (sem efeitos sobre o comportamento).
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.k1.zeroize();
        self.k5.zeroize();
        self.k9.zeroize();
    }
}

/// Camadas intermédias de uma cifragem, uma a uma (K1 → K9).
///
/// São ciphertexts — nunca segredos — e existem para paridade entre
/// linguagens camada a camada (`docs/test_vectors.md`, Resumo §47).
#[derive(Debug, Clone, Default)]
pub struct CamadasIntermedias {
    /// K1 — ChaCha20-Poly1305 sobre o plaintext.
    pub c1: Vec<u8>,
    /// K2 — Cesariana +3 (Lua).
    pub c2: Vec<u8>,
    /// K3 — Vigenère (Lua).
    pub c3: Vec<u8>,
    /// K4 — Transposição 5 colunas (C++).
    pub c4: Vec<u8>,
    /// K5 — AES-256-GCM.
    pub c5: Vec<u8>,
    /// K6 — Playfair adaptado (Lua).
    pub c6: Vec<u8>,
    /// K7 — Hill 2×2 (Rust).
    pub c7: Vec<u8>,
    /// K8 — Substituição +7 (Lua).
    pub c8: Vec<u8>,
    /// K9 — ChaCha20-Poly1305 via libsodium (C): ciphertext do envelope.
    pub c9: Vec<u8>,
}

/// Cifra `dados` com o pipeline completo e devolve o envelope fechado
/// (inclui a assinatura Ed25519 feita com `seed` do remetente).
///
/// O envelope gerado segue `docs/message_format.md` (versão 0x01).
///
/// # Erros
/// - `PayloadGrande` — `dados.len() > MAX_PLAINTEXT` (65 536),
///   rejeitado **antes** de K1: nunca se processa material que não vai
///   caber no envelope.
/// - `CifragemFalhou`/`ChaveInvalida` — falhas das camadas AEAD.
pub fn cifrar(dados: &[u8], chaves: &Chaves, seed: &[u8; 32]) -> Result<Vec<u8>, ErroPipeline> {
    // Nonces aleatórios por mensagem (nunca reutilizados) — único
    // caminho em produção. A variante determinística para os vetores
    // oficiais é `cifrar_com_nonces`.
    let nonce1 = crypto_core::gerar_nonce();
    let nonce5 = crypto_core::gerar_nonce();
    let nonce9 = crypto_core::gerar_nonce();
    cifrar_com_nonces(dados, chaves, seed, nonce1, nonce5, nonce9).map(|(envelope, _)| envelope)
}

/// Como [`cifrar`] mas com os três nonces fornecidos — **determinístico**.
///
/// Caminho dos vetores oficiais (`docs/test_vectors.md`; gerados por
/// `examples/gerar_vetores.rs` e verificados em `tests/vetores.rs`).
/// A produção usa sempre [`cifrar`], que gera nonces aleatórios.
///
/// Devolve também as [`CamadasIntermedias`] (ciphertexts K1..K9) para
/// verificação camada a camada entre linguagens (Resumo §47).
pub fn cifrar_com_nonces(
    dados: &[u8],
    chaves: &Chaves,
    seed: &[u8; 32],
    nonce1: [u8; crate::envelope::TAM_NONCE],
    nonce5: [u8; crate::envelope::TAM_NONCE],
    nonce9: [u8; crate::envelope::TAM_NONCE],
) -> Result<(Vec<u8>, CamadasIntermedias), ErroPipeline> {
    // 0) Limites de tamanho — validação barata feita primeiro, para que
    //    nenhuma camada corra sobre material que será descartado.
    //
    //    A ordem entre os dois limites é deliberada: o superior primeiro,
    //    porque é o teste mais barato (uma comparação), e o inferior só
    //    interessa quando o payload é pequeno de facto.
    if dados.len() > MAX_PLAINTEXT {
        return Err(ErroPipeline::PayloadGrande {
            obtido: dados.len(),
            maximo: MAX_PLAINTEXT,
        });
    }
    // Texto vazio é input degenerado, não payload grande demais. Rejeitar
    // aqui evita chegar a K4, que produziria 5 bytes de padding puro —
    // um envelope que a decifragem aceitaria como mensagem vazia.
    // `docs/pipeline.md` §Limites: `MIN_PLAINTEXT = 1`.
    debug_assert!(MIN_PLAINTEXT >= 1, "o mínimo tem de ser 1 byte");
    if dados.len() < MIN_PLAINTEXT {
        return Err(ErroPipeline::PayloadVazio);
    }

    // 1) K1 — ChaCha20-Poly1305 (chave do par).
    let c1 = crypto_core::aead_chacha::encrypt(dados, &chaves.k1, &nonce1)
        .map_err(ErroPipeline::from)?;

    // 2) K2 — deslocamento +3 (Lua; a mesma função que a K8).
    let c2 = lua_camadas::deslocamento(&c1, DESLOCAMENTO_K2)?;

    // 3) K3 — Vigenère "VIGENERE" (Lua).
    let c3 = lua_camadas::vigenere(&c2, CHAVE_K3)?;

    // 4) K4 — Transposição 5 colunas (C++, PKCS#7 bloco 5).
    let c4 = ffi_c::k4_cifrar(&c3).map_err(ErroPipeline::from)?;

    // 5) K5 — AES-256-GCM (chave do remetente).
    let c5 = crypto_core::encrypt_k5(&c4, &chaves.k5, &nonce5).map_err(ErroPipeline::from)?;

    // 6) K6 — Playfair adaptado "PLAYFAIR" (Lua).
    let c6 = lua_camadas::playfair(&c5, CHAVE_K6)?;

    // 7) K7 — Hill [[3,3],[2,5]] (Rust, PKCS#7 bloco 2).
    let c7 = crypto_core::hill::cifrar(&c6);

    // 8) K8 — deslocamento +7 (Lua; a mesma função que a K2).
    let c8 = lua_camadas::deslocamento(&c7, DESLOCAMENTO_K8)?;

    // 9) K9 — ChaCha20-Poly1305 via libsodium/C (chave do receptor).
    let c9 = ffi_c::k9_cifrar(&c8, &chaves.k9, &nonce9).map_err(ErroPipeline::from)?;

    // 10) Envelope + assinatura Ed25519 sobre a região assinada.
    //
    // A região é construída **uma** vez e assinada no sítio; a
    // assinatura é depois acrescentada a essa mesma alocação. Antes,
    // `envelope.fechar(...)` pedia a região outra vez e copiava o
    // envelope inteiro — num payload de 64 KiB eram três cópias do
    // ciphertext em voo (esta, a do `fechar` e o `c9.clone()`) em vez
    // de duas. `c9.clone()` fica porque `CamadasIntermedias` precisa de
    // devolver a K9 intacta para a paridade entre linguagens.
    let envelope = Envelope::novo(nonce1, nonce5, nonce9, c9.clone());
    let regiao = envelope.regiao_assinada();
    let assinatura = crypto_core::sign_message(&regiao, seed);
    let fechado =
        Envelope::fechar_regiao(regiao, &assinatura.try_into().expect("assinatura 64B"));

    let camadas = CamadasIntermedias {
        c1,
        c2,
        c3,
        c4,
        c5,
        c6,
        c7,
        c8,
        c9,
    };
    Ok((fechado, camadas))
}

/// Decifra um envelope com o pipeline inverso e devolve o plaintext
/// UTF-8, com anti-replay obrigatório.
///
/// A ordem das três primeiras etapas é normativa
/// (`docs/message_format.md`): parsing → assinatura → **reserva do
/// `nonce1`** → decifragem. Nenhuma camada K9→K1 corre sobre uma
/// mensagem que já tinha sido vista.
///
/// A reserva é **libertada** se a decifragem falhar: o que estava errado
/// eram as chaves apresentadas, não a mensagem, e um retry legítimo
/// não pode ficar bloqueado. Um plaintext autêntico que não seja UTF-8
/// *mantém* a reserva — a mensagem foi consumida e vista.
///
/// # Erros
/// `EnvelopeInvalido` (parsing), `AssinaturaInvalida` (Ed25519),
/// `NonceRepetido` (anti-replay), `DecifragemFalhou` (tags/padding),
/// `TextoInvalidoUtf8` (K1 ilegível).
pub fn decifrar(
    envelope: &[u8],
    chaves: &Chaves,
    publica: &[u8; 32],
    registo: &mut RegistoNonce1,
) -> Result<String, ErroPipeline> {
    // 1) parsing por comprimento (versão/tamanho) — ainda sem chaves.
    let (env, assinatura) = Envelope::abrir(envelope).map_err(ErroPipeline::from)?;

    // 2) verificação Ed25519 ANTES de decifrar (anti nonce-swapping).
    //    Só uma assinatura válida pode ter gerado este `nonce1`, pelo
    //    que a reserva da etapa 3 só é atingível por replay genuíno.
    let regiao = env.regiao_assinada();
    crypto_core::verify_signature(&regiao, &assinatura, publica).map_err(ErroPipeline::from)?;

    // 3) anti-replay: check-and-insert do `nonce1`, depois da
    //    assinatura e antes de qualquer camada de decifragem.
    if !registo.reservar(&env.nonce1) {
        return Err(ErroPipeline::NonceRepetido);
    }

    // 4..12) camadas K9→K1; em falha, liberta a reserva (ver doc).
    let texto = match decifrar_camadas(&env, chaves) {
        Ok(t) => t,
        Err(erro) => {
            registo.libertar(&env.nonce1);
            return Err(erro);
        }
    };
    String::from_utf8(texto).map_err(|_| ErroPipeline::TextoInvalidoUtf8)
}

/// Camadas K9→K1 sobre um envelope já validado (assinatura + registo).
///
/// Devolve os bytes do plaintext; a conversão para UTF-8 fica à
/// chamada, para não libertar a reserva de uma mensagem autêntica.
fn decifrar_camadas(env: &Envelope, chaves: &Chaves) -> Result<Vec<u8>, ErroPipeline> {
    // 1) K9 — autentica e decifra via C/libsodium.
    let c8 =
        ffi_c::k9_decifrar(&env.ciphertext, &chaves.k9, &env.nonce9).map_err(ErroPipeline::from)?;

    // 2) K8 — inversa da substituição (Lua).
    let c7 = lua_camadas::deslocamento_inverso(&c8, DESLOCAMENTO_K8)?;

    // 3) K7 — Hill inverso + remoção do padding de 2 (Rust).
    let c6 = crypto_core::hill::decifrar(&c7).map_err(ErroPipeline::from)?;

    // 4) K6 — Playfair inverso (Lua; XOR é involutivo).
    let c5 = lua_camadas::playfair_inverso(&c6, CHAVE_K6)?;

    // 5) K5 — AES-256-GCM inverso (autentica a camada do remetente).
    let c4 = crypto_core::decrypt_k5(&c5, &chaves.k5, &env.nonce5).map_err(ErroPipeline::from)?;

    // 6) K4 — transposição inversa + padding de 5 (C++).
    let c3 = ffi_c::k4_decifrar(&c4).map_err(ErroPipeline::from)?;

    // 7) K3 — Vigenère inverso (Lua).
    let c2 = lua_camadas::vigenere_inverso(&c3, CHAVE_K3)?;

    // 8) K2 — Cesariana inverso (Lua).
    let c1 = lua_camadas::deslocamento_inverso(&c2, DESLOCAMENTO_K2)?;

    // 9) K1 — autentica a camada do par e recupera o texto.
    crypto_core::aead_chacha::decrypt(&c1, &chaves.k1, &env.nonce1).map_err(ErroPipeline::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto_core::signer;

    /// Gera chaves e identidade determinísticas suficientes p/ testes.
    fn cenario() -> (Chaves, [u8; 32], [u8; 32]) {
        let chaves = Chaves {
            k1: crypto_core::gerar_chave(),
            k5: crypto_core::gerar_chave(),
            k9: crypto_core::gerar_chave(),
        };
        let (seed, publica) = crypto_core::gerar_identidade();
        (chaves, seed, publica)
    }

    /// `decifrar` com um registo anti-replay descartável.
    ///
    /// Cada teste é assim isolado (nenhum estado entre testes); os
    /// testes de anti-replay usam `decifrar` diretamente com um registo
    /// próprio.
    fn decifrar_teste(
        envelope: &[u8],
        chaves: &Chaves,
        publica: &[u8; 32],
    ) -> Result<String, ErroPipeline> {
        decifrar(envelope, chaves, publica, &mut RegistoNonce1::novo())
    }

    /// Round-trip completo com mensagem UTF-8 real (acentos incluídos).
    ///
    /// A mensagem contém caracteres de **fora** do repertório
    /// latino-português — ômega grego, aproximadamente igual, raiz
    /// quadrada — de propósito: o pipeline tem de devolver byte a byte
    /// o que lhe deram, e um teste que só usa acentos não prova isso.
    ///
    /// Estão escritos como escapes unicode e não como caracteres
    /// literais, por duas razões que convergem. A primeira é a auditoria
    /// de alfabeto: um carácter grego no código-fonte seria uma
    /// violação da regra que ela própria fiscaliza. A segunda é a
    /// legibilidade em terminais e diffs, onde glifos de outra escrita
    /// têm larguras diferentes e a linha deixa de estar alinhada.
    ///
    /// O valor testado é exactamente o mesmo nas duas formas.
    #[test]
    fn roundtrip_mensagem_utf8() {
        const MENSAGEM: &str = "Ol\u{e1} mundo! \u{3a9}\u{2248}\u{e7} \u{221a}";
        let (chaves, seed, publica) = cenario();
        let envelope = cifrar(MENSAGEM.as_bytes(), &chaves, &seed).expect("cifra");
        // Envelope válido: ≥117 bytes e versão 0x01.
        assert!(envelope.len() >= crate::envelope::TAM_MINIMO);
        assert_eq!(envelope[0], 0x01);
        let texto = decifrar_teste(&envelope, &chaves, &publica).expect("decifra");
        assert_eq!(texto, MENSAGEM);
    }

    /// Round-trip com bytes arbitrários (inclui 0x00/0xFF — camada K1
    /// trata bytes, não texto; o UTF-8 só é exigido no fim).
    #[test]
    fn roundtrip_binario() {
        let (chaves, seed, publica) = cenario();
        let brutos: Vec<u8> = (0..=255u8).collect();
        let envelope = cifrar(&brutos, &chaves, &seed).expect("cifra");
        // O plaintext bruto chega como String? Não — cifrar aceita
        // bytes; decifrar devolve String apenas se for UTF-8 válido.
        // Neste caso NÃO é, por isso validamos o caminho alternativo:
        // decifragem falha em UTF-8 com erro estruturado.
        let erro = decifrar_teste(&envelope, &chaves, &publica).unwrap_err();
        assert_eq!(erro, ErroPipeline::TextoInvalidoUtf8);
    }

    /// Assinatura adulterada → rejeição imediata, antes de decifrar.
    #[test]
    fn assinatura_adulterada() {
        let (chaves, seed, publica) = cenario();
        let mut envelope = cifrar("secreto".as_bytes(), &chaves, &seed).expect("cifra");
        let ultimo = envelope.len() - 1;
        envelope[ultimo] ^= 0x01;
        let erro = decifrar_teste(&envelope, &chaves, &publica).unwrap_err();
        assert_eq!(erro, ErroPipeline::AssinaturaInvalida);
    }

    /// Chave pública errada (outro par) → assinatura inválida.
    #[test]
    fn chave_publica_de_outro() {
        let (chaves, seed, _) = cenario();
        let (_, outra_publica) = crypto_core::gerar_identidade();
        let envelope = cifrar("secreto".as_bytes(), &chaves, &seed).expect("cifra");
        let erro = decifrar_teste(&envelope, &chaves, &outra_publica).unwrap_err();
        assert_eq!(erro, ErroPipeline::AssinaturaInvalida);
    }

    /// Chave pública não-ponto-Ed25519 → também `AssinaturaInvalida`.
    #[test]
    fn chave_publica_invalida() {
        let (chaves, seed, _) = cenario();
        let envelope = cifrar("secreto".as_bytes(), &chaves, &seed).expect("cifra");
        // 32 bytes que não formam um ponto válido em Ed25519.
        let lixo = [0xFFu8; 32];
        let erro = decifrar_teste(&envelope, &chaves, &lixo).unwrap_err();
        assert_eq!(erro, ErroPipeline::AssinaturaInvalida);
    }

    /// Ciphertext K9 adulterado → tag inválida (após assinatura OK é
    /// preciso re-assinar para passar a verificação; aqui alteramos a
    /// assinatura em conjunto para simular um atacante competente).
    #[test]
    fn ciphertext_adulterado_com_assinatura_valida() {
        let (chaves, seed, publica) = cenario();
        let mut envelope = cifrar("secreto".as_bytes(), &chaves, &seed).expect("cifra");
        // Altera um byte do ciphertext (zona assinada)…
        envelope[40] ^= 0x01;
        // …e re-assina a região com a seed legítima (o atacante não a
        // tem; aqui serve para isolar a falha da tag K9).
        let regiao: Vec<u8> = envelope[..envelope.len() - 64].to_vec();
        let nova = crypto_core::sign_message(&regiao, &seed);
        let inicio = envelope.len() - 64;
        envelope[inicio..].copy_from_slice(&nova);
        let erro = decifrar_teste(&envelope, &chaves, &publica).unwrap_err();
        // O ciphertext alterado quebra a tag K9 (C) ou, no limite, o
        // padding K4/K7 — ambos mapeados para decifragem falhada.
        assert!(
            matches!(erro, ErroPipeline::DecifragemFalhou(_)),
            "{erro:?}"
        );
    }

    /// Chave K9 errada → tag inválida na camada C.
    #[test]
    fn chave_k9_errada() {
        let (chaves, seed, publica) = cenario();
        let envelope = cifrar("secreto".as_bytes(), &chaves, &seed).expect("cifra");
        let erradas = Chaves {
            k9: crypto_core::gerar_chave(),
            ..chaves
        };
        let erro = decifrar_teste(&envelope, &erradas, &publica).unwrap_err();
        assert!(
            matches!(erro, ErroPipeline::DecifragemFalhou(_)),
            "{erro:?}"
        );
    }

    /// Chave K5 errada → falha na camada AES-GCM (K5).
    #[test]
    fn chave_k5_errada() {
        let (chaves, seed, publica) = cenario();
        // Ciframos com K9 inalterada mas K5 trocada → a tag K9 passa,
        // K8/K7/K6 invertem-se (não autenticam), e K5 falha.
        // Para garantir que K5 falha, ciframos e deciframos com K5
        // diferente desde o início:
        let envelope = cifrar("secreto".as_bytes(), &chaves, &seed).expect("cifra");
        let erradas = Chaves {
            k5: crypto_core::gerar_chave(),
            ..chaves
        };
        let erro = decifrar_teste(&envelope, &erradas, &publica).unwrap_err();
        // K9 valida; as camadas simétricas (K8/K6/K3/K2) são
        // reversíveis sem chave; o erro chega exatamente em K5 (tag).
        assert!(
            matches!(erro, ErroPipeline::DecifragemFalhou(_)),
            "{erro:?}"
        );
    }

    /// Envelope curto demais → `EnvelopeInvalido` (IPC 0x06).
    #[test]
    fn envelope_curto_no_pipeline() {
        let (chaves, _, publica) = cenario();
        let erro = decifrar_teste(&[0x01u8; 50], &chaves, &publica).unwrap_err();
        assert!(matches!(erro, ErroPipeline::EnvelopeInvalido(_)));
    }

    /// Versão desconhecida → `EnvelopeInvalido`.
    #[test]
    fn versao_desconhecida_no_pipeline() {
        let (chaves, _, publica) = cenario();
        let mut pacote = vec![0x09u8; crate::envelope::TAM_MINIMO];
        pacote[0] = 0x09;
        let erro = decifrar_teste(&pacote, &chaves, &publica).unwrap_err();
        assert!(matches!(erro, ErroPipeline::EnvelopeInvalido(_)));
    }

    /// Mensagem vazia também atravessa o pipeline inteiro.
    #[test]
    fn mensagem_vazia() {
        // Texto vazio é input degenerado e é recusado à entrada, antes de
        // K1 — logo, antes de qualquer nonce ser gerado.
        let (chaves, seed, publica) = cenario();
        assert_eq!(cifrar(b"", &chaves, &seed), Err(ErroPipeline::PayloadVazio));
        // `publica` fica sem uso aqui, mas mantê-lo mantém a assinatura
        // do cenário igual à dos outros testes.
        let _ = publica;

        // O limite mínimo é 1 byte: um único byte é uma mensagem válida.
        // `decifrar_teste` devolve `String` (o plaintext é validado como
        // UTF-8), por isso a comparação é com o texto, não com os bytes.
        let minima = [0x41u8];
        let envelope = cifrar(&minima, &chaves, &seed).expect("cifra 1 byte");
        assert_eq!(
            decifrar_teste(&envelope, &chaves, &publica).expect("decifra"),
            "A"
        );
    }

    /// O erro de texto vazio diz o mínimo, para que o cliente se possa
    /// corrigir sem consultar a documentação.
    #[test]
    fn mensagem_vazia_diz_o_minimo() {
        let texto = ErroPipeline::PayloadVazio.to_string();
        assert!(texto.contains("vazia"), "{texto}");
        assert!(
            texto.contains(&MIN_PLAINTEXT.to_string()),
            "tem de indicar o mínimo: {texto}"
        );
        // E o código IPC tem de ser 0x02, não 0x09: `0x09` diz
        // «excede o limite», que não é o problema.
        assert_eq!(ErroPipeline::PayloadVazio.para_ipc(true), 0x02);
    }

    /// Plaintext acima de `MAX_PLAINTEXT` → `PayloadGrande` (IPC 0x09)
    /// **antes** de qualquer camada correr.
    #[test]
    fn plaintext_acima_do_limite() {
        let (chaves, seed, _) = cenario();
        let grande = vec![0x41u8; crate::envelope::MAX_PLAINTEXT + 1];
        let erro = cifrar(&grande, &chaves, &seed).unwrap_err();
        assert_eq!(
            erro,
            ErroPipeline::PayloadGrande {
                obtido: crate::envelope::MAX_PLAINTEXT + 1,
                maximo: crate::envelope::MAX_PLAINTEXT,
            }
        );
        // O limite exato (65 536) continua a ser aceite.
        let no_limite = vec![0x41u8; crate::envelope::MAX_PLAINTEXT];
        cifrar(&no_limite, &chaves, &seed).expect("no limite é válido");
    }

    /// Envelope acima de `MAX_ENVELOPE` → `PayloadGrande` no receptor,
    /// antes de verificar assinatura ou decifrar qualquer camada.
    #[test]
    fn envelope_acima_do_limite() {
        let (chaves, _, publica) = cenario();
        // 101 fixos + ciphertext de um byte acima do máximo possível.
        let excedente = crate::envelope::MAX_ENVELOPE + 1;
        let mut pacote = vec![0x01u8; excedente];
        pacote[0] = 0x01;
        let erro = decifrar_teste(&pacote, &chaves, &publica).unwrap_err();
        assert_eq!(
            erro,
            ErroPipeline::PayloadGrande {
                obtido: excedente,
                maximo: crate::envelope::MAX_ENVELOPE,
            }
        );
    }

    /// O sobretotal documentado é exatamente o derivado do formato —
    /// se as camadas mudarem, este teste obriga a rever os docs.
    #[test]
    fn sobretotal_documentado_e_exato() {
        assert_eq!(crate::envelope::SOBRETOTAL_MAXIMO, 156);
        assert_eq!(crate::envelope::MAX_ENVELOPE, 65_692);
        assert_eq!(crate::envelope::MAX_PLAINTEXT, 65_536);
    }

    /// Duas cifragens da mesma mensagem nunca coincidem (nonces novos).
    #[test]
    fn nonces_unicos_por_mensagem() {
        let (chaves, seed, _) = cenario();
        let a = cifrar("repetida".as_bytes(), &chaves, &seed).expect("a");
        let b = cifrar("repetida".as_bytes(), &chaves, &seed).expect("b");
        assert_ne!(a, b);
    }

    /// Identidade: `signer` disponível para construir vetores extras.
    #[test]
    fn identidade_utilizavel() {
        let (seed, publica) = signer::gerar_identidade();
        let msg = b"teste";
        let sig = crypto_core::sign_message(msg, &seed);
        crypto_core::verify_signature(msg, &sig, &publica).expect("valida");
    }

    // ----------------------------------------------------------------
    // Anti-replay do chat (registo de `nonce1`)
    // ----------------------------------------------------------------

    /// O mesmo envelope não é decifrado duas vezes no mesmo registo.
    #[test]
    fn anti_replay_rejeita_reenvio() {
        let (chaves, seed, publica) = cenario();
        let envelope = cifrar("só uma vez".as_bytes(), &chaves, &seed).expect("cifra");
        let mut registo = RegistoNonce1::novo();
        let texto = decifrar(&envelope, &chaves, &publica, &mut registo).expect("primeira");
        assert_eq!(texto, "só uma vez");
        // Reenvio do envelope capturado → NonceRepetido, sem decifrar.
        let erro = decifrar(&envelope, &chaves, &publica, &mut registo).unwrap_err();
        assert_eq!(erro, ErroPipeline::NonceRepetido);
        assert_eq!(erro.para_ipc(false), 0x13);
        // Mensagens distintas (nonces novos) continuam aceites.
        let outra = cifrar("só uma vez".as_bytes(), &chaves, &seed).expect("cifra");
        assert_ne!(outra, envelope, "cifragens nunca coincidem");
        decifrar(&outra, &chaves, &publica, &mut registo).expect("mensagem nova");
    }

    /// Chaves erradas na decifragem não podem bloquear a mensagem:
    /// a reserva é libertada e o retry legítimo passa.
    #[test]
    fn falha_de_decifragem_liberta_a_reserva() {
        let (chaves, seed, publica) = cenario();
        let envelope = cifrar("retry".as_bytes(), &chaves, &seed).expect("cifra");
        let erradas = Chaves {
            k1: crypto_core::gerar_chave(),
            k5: chaves.k5,
            k9: chaves.k9,
        };
        let mut registo = RegistoNonce1::novo();
        let erro = decifrar(&envelope, &erradas, &publica, &mut registo).unwrap_err();
        assert!(
            matches!(erro, ErroPipeline::DecifragemFalhou(_)),
            "esperava falha de decifragem, obteve {erro:?}"
        );
        // Reserva libertada → a tentativa correta é aceite.
        let texto = decifrar(&envelope, &chaves, &publica, &mut registo).expect("retry");
        assert_eq!(texto, "retry");
        // E continua protegida depois de decifrada com sucesso.
        assert_eq!(
            decifrar(&envelope, &chaves, &publica, &mut registo).unwrap_err(),
            ErroPipeline::NonceRepetido
        );
    }

    /// Assinatura inválida é rejeitada **antes** de reservar — um
    /// atacante não consegue encher o registo com envelopes forjados.
    #[test]
    fn assinatura_invalida_nao_ocupa_o_registo() {
        let (chaves, seed, publica) = cenario();
        let envelope = cifrar("válida".as_bytes(), &chaves, &seed).expect("cifra");
        let mut adulterado = envelope.clone();
        let ultimo = adulterado.len() - 1;
        adulterado[ultimo] ^= 0x01;
        let mut registo = RegistoNonce1::novo();
        let erro = decifrar(&adulterado, &chaves, &publica, &mut registo).unwrap_err();
        assert_eq!(erro, ErroPipeline::AssinaturaInvalida);
        // Nada foi reservado: a mensagem autêntica continua aceite.
        decifrar(&envelope, &chaves, &publica, &mut registo).expect("aceite");
    }

    // ----------------------------------------------------------------
    // Zeroização (Resumo §22 — `docs/key_management.md`)
    // ----------------------------------------------------------------

    /// `Chaves` tem de apagar K1/K5/K9 no descarte (não é `Copy`).
    #[test]
    fn chaves_sao_zeroizadas_no_drop() {
        use std::mem::ManuallyDrop;
        let mut chaves = ManuallyDrop::new(Chaves {
            k1: [0xAB; 32],
            k5: [0xCD; 32],
            k9: [0xEF; 32],
        });
        // Corre o `Drop` sem libertar a struct (arrays inline → a
        // memória continua legível para a verificação).
        unsafe { std::ptr::drop_in_place(&mut *chaves) };
        assert_eq!(chaves.k1, [0u8; 32], "K1 apagada");
        assert_eq!(chaves.k5, [0u8; 32], "K5 apagada");
        assert_eq!(chaves.k9, [0u8; 32], "K9 apagada");
    }
}
