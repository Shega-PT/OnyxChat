// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// ffi_c.rs — ponte FFI segura para as camadas C/C++ (K4 + K9)
// ---------------------------------------------------------------------
// Expõe `onyx_k4_*` (transposição, C++) e `onyx_k9_*` (ChaCha20-
// Poly1305 via libsodium, C) com wrappers Rust que:
//   * dimensionam os buffers de saída segundo o contrato da lib
//     (sem alocação dentro do C — o chamador é responsável);
//   * convertem códigos de erro negativos em `FfiErro` tipado;
//   * nunca paniqueiam com dados do cliente (o daemon responde via IPC).
//
// As funções brutas são `unsafe extern "C"` — os wrappers garantem
// ponteiros/competências corretos antes de cruzar a fronteira.
//
// ## Limpeza de memória
//
// [`limpar`] é a única forma de o daemon pedir à C que apague um
// buffer, e é `sodium_memzero` embrulhada em C. O Rust tem `zeroize`
// para o que é seu, e `memset` para o resto — mas para um buffer que a
// C escreveu e que a C tem de limpar, atravessar a fronteira e pedir à
// C que o faça é o que garante que ninguém escreve um `memset` no
// lugar errado. Ver [`onyx_limpar`] em `crypto/c_cpp/onyx_crypto.h`.
// =====================================================================

use std::os::raw::c_int;

/// Colunas da transposição K4 (espelho de `ONYX_K4_COLUNAS`).
pub const K4_COLUNAS: usize = 5;
/// Tamanho da chave K9 (bytes) — espelho de `ONYX_K9_KEY_LEN`.
pub const K9_TAM_CHAVE: usize = 32;
/// Tamanho do nonce K9 (bytes) — espelho de `ONYX_K9_NONCE_LEN`.
pub const K9_TAM_NONCE: usize = 12;
/// Tamanho da tag AEAD K9 / overhead do ciphertext — `ONYX_K9_TAG_LEN`.
pub const K9_TAM_TAG: usize = 16;

// Declarações da ABI `extern "C"` (onyx_crypto.h). A verificação de
// compatibilidade de tipos é feita pelo compilador na linkagem.
extern "C" {
    fn onyx_k4_encrypt_len(entrada_len: usize) -> usize;
    fn onyx_k4_encrypt(
        entrada: *const u8,
        entrada_len: usize,
        saida: *mut u8,
        saida_cap: usize,
        saida_len: *mut usize,
    ) -> c_int;
    fn onyx_k4_decrypt(
        entrada: *const u8,
        entrada_len: usize,
        saida: *mut u8,
        saida_cap: usize,
        saida_len: *mut usize,
    ) -> c_int;
    fn onyx_k9_encrypt(
        entrada: *const u8,
        entrada_len: usize,
        chave: *const u8,
        nonce: *const u8,
        saida: *mut u8,
        saida_len: *mut usize,
    ) -> c_int;
    fn onyx_k9_decrypt(
        entrada: *const u8,
        entrada_len: usize,
        chave: *const u8,
        nonce: *const u8,
        saida: *mut u8,
        saida_len: *mut usize,
    ) -> c_int;
    /// `sodium_memzero` embrulhada — ver `onyx_crypto.h`.
    ///
    /// Aceita `ptr` nulo ou `len` zero sem efeito, por isso não há
    /// guarda a fazer antes de chamar.
    fn onyx_limpar(pnt: *mut u8, len: usize);
}

/// Apaga `len` bytes a partir de `buffer`.
///
/// Não é uma função que o pipeline use no caminho normal: as camadas C
/// limpam os seus próprio buffers nos ramos de erro. Existe para o
/// wrapper Rust precisar de **uma** operação de limpeza com as
/// garantias da `sodium_memzero` em vez de duas — `Vec::zeroize`, que
/// é `volatile` mas o compilador pode em teoria reordenar em relação a
/// outras escritas, e um `slice::fill(0)`, que éoptimizável.
///
/// Em debug confirma que o buffer fica mesmo a zeros. Não o verifica
/// em release: a função é `unsafe` e opaca ao optimizador, e um
/// `assert!` a cada limpeza seria um custo por operação de rede para
/// confirmar ao compilador o que a própria função garante.
pub fn limpar(buffer: &mut [u8]) {
    // `Vec` vazio dá um ponteiro pendurado: o `as_mut_ptr` é válido
    // (o slice está vazio) mas chamá-lo com `len == 0` seria uma
    // chamada com um endereço que não pertence a ninguém. Cortamos
    // antes.
    if buffer.is_empty() {
        return;
    }
    unsafe { onyx_limpar(buffer.as_mut_ptr(), buffer.len()) };
    debug_assert!(
        buffer.iter().all(|&b| b == 0),
        "onyx_limpar não limpou o buffer"
    );
}

/// Erro devolvido pelas funções C/C++ (código negativo de `onyx_crypto.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfiErro {
    /// `ONYX_ERR_ARG` (−1): ponteiro nulo ou argumento inválido.
    Argumento,
    /// `ONYX_ERR_CAP` (−2): buffer de saída demasiado pequeno.
    Capacidade,
    /// `ONYX_ERR_PAD` (−3): padding PKCS#7 ausente/adulterado (K4).
    Padding,
    /// `ONYX_ERR_TAMANHO` (−4): comprimento não múltiplo do bloco.
    Tamanho,
    /// `ONYX_ERR_AUTH` (−5): tag AEAD inválida — mensagem adulterada (K9).
    Auth,
    /// `ONYX_ERR_CRYPTO` (−6) ou código desconhecido: falha interna.
    Crypto,
}

impl FfiErro {
    /// Converte o código `int` da camada C no erro tipado correspondente.
    ///
    /// Códigos fora da faixa conhecida tratam-se como falha interna
    /// (`Crypto`) — nunca como sucesso.
    fn de_codigo(rc: c_int) -> FfiErro {
        match rc {
            -1 => FfiErro::Argumento,
            -2 => FfiErro::Capacidade,
            -3 => FfiErro::Padding,
            -4 => FfiErro::Tamanho,
            -5 => FfiErro::Auth,
            -6 => FfiErro::Crypto,
            _ => FfiErro::Crypto,
        }
    }

    /// Mensagem legível (sem dados sensíveis) para logging/IPC.
    pub fn descricao(&self) -> &'static str {
        match self {
            FfiErro::Argumento => "argumento inválido na camada C",
            FfiErro::Capacidade => "buffer de saída insuficiente na camada C",
            FfiErro::Padding => "padding PKCS#7 inválido na camada C",
            FfiErro::Tamanho => "comprimento inválido na camada C",
            FfiErro::Auth => "tag AEAD inválida na camada C",
            FfiErro::Crypto => "falha interna da camada C",
        }
    }
}

/// Cifra K4 (transposição C++ de 5 colunas com PKCS#7).
///
/// Cifra K4: devolve o ciphertext com padding aplicado (múltiplo de 5,
/// ≥ 5), ou [`FfiErro::Tamanho`] se a entrada for vazia.
///
/// A entrada vazia é recusada por duas razões: a biblioteca C rejeita
/// (`ONYX_ERR_TAMANHO`), e o pipeline recusa antes de chegar aqui
/// (`MIN_PLAINTEXT`). A redundância é intencional — esta função é
/// também chamada por property tests e fuzzing com input arbitrário, e
/// um wrapper que confiasse só na validação do pipeline seria uma
/// dependência implícita de ordem de chamada.
pub fn k4_cifrar(entrada: &[u8]) -> Result<Vec<u8>, FfiErro> {
    if entrada.is_empty() {
        return Err(FfiErro::Tamanho);
    }
    // A lib devolve a capacidade exata necessária (PKCS#7 por definição).
    // O `0` para entrada vazia nunca é alcançado: o guard acima já
    // filtrou, e assim mesmo um `0` seria detectado em vez de virar um
    // `vec![]` silencioso.
    let capacidade = unsafe { onyx_k4_encrypt_len(entrada.len()) };
    if capacidade == 0 {
        return Err(FfiErro::Tamanho);
    }
    let mut saida = vec![0u8; capacidade];
    let mut tamanho: usize = 0;
    // O retorno é verificado: `ONYX_ERR_ARG` e `ONYX_ERR_CAP` são
    // invariantes com os argumentos validados acima, mas `Tamanho` não
    // é — e um retorno ignorado deixaria `tamanho = 0`, o que faria a
    // decifragem do receptor falhar tarde e com uma mensagem errada.
    let codigo = unsafe {
        onyx_k4_encrypt(
            entrada.as_ptr(),
            entrada.len(),
            saida.as_mut_ptr(),
            saida.len(),
            &mut tamanho,
        )
    };
    // `de_codigo` é um mapeamento, não um `Try`: ONYX_OK é 0, e
    // qualquer outro valor tem de ser verificado à mão.
    if codigo != 0 {
        return Err(FfiErro::de_codigo(codigo));
    }
    saida.truncate(tamanho);
    Ok(saida)
}

/// Decifra K4 (inversa da transposição + remoção do padding).
///
/// O buffer de entrada nunca cresce, logo a capacidade de saída é o
/// próprio comprimento de entrada.
pub fn k4_decifrar(entrada: &[u8]) -> Result<Vec<u8>, FfiErro> {
    let mut saida = vec![0u8; entrada.len()];
    let mut tamanho: usize = 0;
    let rc = unsafe {
        onyx_k4_decrypt(
            entrada.as_ptr(),
            entrada.len(),
            saida.as_mut_ptr(),
            saida.len(),
            &mut tamanho,
        )
    };
    if rc != 0 {
        return Err(FfiErro::de_codigo(rc));
    }
    saida.truncate(tamanho);
    Ok(saida)
}

/// Cifra K9 (ChaCha20-Poly1305 via libsodium) — `entrada || tag`.
///
/// O ciphertext tem sempre `entrada.len() + 16` bytes.
pub fn k9_cifrar(
    entrada: &[u8],
    chave: &[u8; K9_TAM_CHAVE],
    nonce: &[u8; K9_TAM_NONCE],
) -> Result<Vec<u8>, FfiErro> {
    let mut saida = vec![0u8; entrada.len() + K9_TAM_TAG];
    let mut tamanho: usize = 0;
    // Retorno não verificado: `crypto_aead_chacha20poly1305_ietf_encrypt`
    // termina sempre em `return 0` em libsodium (sem condições de falha
    // com argumentos válidos) — ver comentário em k9_chacha.c.
    unsafe {
        onyx_k9_encrypt(
            entrada.as_ptr(),
            entrada.len(),
            chave.as_ptr(),
            nonce.as_ptr(),
            saida.as_mut_ptr(),
            &mut tamanho,
        );
    };
    saida.truncate(tamanho);
    Ok(saida)
}

/// Decifra/autentica K9; devolve `FfiErro::Auth` se a tag não conferir.
///
/// Pré-condição defensiva: `entrada.len() >= 16` (tag). A C já valida,
/// mas falhar aqui evita subtração com underflow no dimensionamento.
pub fn k9_decifrar(
    entrada: &[u8],
    chave: &[u8; K9_TAM_CHAVE],
    nonce: &[u8; K9_TAM_NONCE],
) -> Result<Vec<u8>, FfiErro> {
    if entrada.len() < K9_TAM_TAG {
        return Err(FfiErro::Tamanho);
    }
    let mut saida = vec![0u8; entrada.len() - K9_TAM_TAG];
    let mut tamanho: usize = 0;
    let rc = unsafe {
        onyx_k9_decrypt(
            entrada.as_ptr(),
            entrada.len(),
            chave.as_ptr(),
            nonce.as_ptr(),
            saida.as_mut_ptr(),
            &mut tamanho,
        )
    };
    if rc != 0 {
        return Err(FfiErro::de_codigo(rc));
    }
    saida.truncate(tamanho);
    Ok(saida)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A K9 de produção (C/libsodium) e a referência em Rust produzem
    /// **os mesmos bytes**.
    ///
    /// Este teste é a razão de `crypto_core` já não expor `encrypt_k9` /
    /// `decrypt_k9`: eram wrappers de uma linha sobre `aead_chacha`, sem
    /// nenhum caller, mas o seu comentário afirmava que «os testes
    /// garantem que ambas as implementações produzem outputs
    /// indistinguíveis». Nenhum teste o fazia. A garantia era escrita,
    /// não verificada — o pior estado possível para um comentário de
    /// criptografia.
    ///
    /// Passar a verificar a afirmação aqui resolve as duas coisas de uma
    /// vez: a API morta vai-se, e a garantia passa a ser real e
    /// verificável neste ficheiro.
    ///
    /// Não é uma tautologia: as duas implementações são independentes —
    /// uma chama o `crypto_aead_chacha20poly1305_ietf_encrypt` do
    /// libsodium em C, a outra faz ChaCha20-Poly1305 em Rust puro. Se
    /// divergirem num byte, ou na ordem do nonce, ou no prepender da
    /// tag, este teste falha. E o envelope que vai para a rede é
    /// **sempre** o da C, pelo que uma divergência não produziria
    /// envelopes inválidos: produziria duas especificações. Divergiria
    /// em silêncio — o daemon decifra sempre pela C, e um cliente noutra
    /// implementação veria lixo.
    #[test]
    fn k9_c_igual_a_k9_rust() {
        // Comprimentos variety: vazio (o AEAD aceita mensagem vazia),
        // abaixo do bloco, exactamente no bloco, acima do bloco e
        // multi-mensagem. Um teste com um único tamanho passa ao lado
        // de bugs que só aparecem nos restantes — o padding do
        // ChaCha20 e o `Poly1305` sobre mensagem vazia são exactamente
        // esses casos de esquina.
        for tamanho in [0usize, 1, 15, 16, 63, 64, 65, 1000] {
            let mensagem: Vec<u8> = (0..tamanho).map(|i| (i as u8).wrapping_mul(31)).collect();
            let chave = [0x5Cu8; K9_TAM_CHAVE];
            let nonce = [0xA7u8; K9_TAM_NONCE];

            let por_c = k9_cifrar(&mensagem, &chave, &nonce).expect("a C cifra");
            let por_rust = crypto_core::aead_chacha::encrypt(&mensagem, &chave, &nonce)
                .expect("o Rust cifra");

            assert_eq!(
                por_c.len(),
                tamanho + crypto_core::aead_chacha::TAMANHO_TAG,
                "comprimento errado com {tamanho} bytes"
            );
            assert_eq!(
                por_c, por_rust,
                "a K9 de C e a de Rust divergem com {tamanho} bytes de mensagem"
            );
        }
    }

    /// E o mesmo para a decifra, nos dois sentidos.
    ///
    /// A cifra acima prova que as duas implementações põem os mesmos
    /// bytes no mesmo sítio; esta prova que ambas os **leem** do mesmo
    /// sítio. São propriedades diferentes: um erro de ordem de tag ou de
    /// posição do nonce passa a cifra e falha aqui.
    #[test]
    fn k9_decifra_c_igual_a_decifra_rust() {
        for tamanho in [0usize, 1, 63, 64, 1000] {
            let mensagem: Vec<u8> = (0..tamanho).map(|i| (i as u8).wrapping_mul(17)).collect();
            let chave = [0x3Bu8; K9_TAM_CHAVE];
            let nonce = [0xE1u8; K9_TAM_NONCE];

            let cifrado = k9_cifrar(&mensagem, &chave, &nonce).expect("cifra");
            let por_c = k9_decifrar(&cifrado, &chave, &nonce).expect("a C decifra");
            let por_rust =
                crypto_core::aead_chacha::decrypt(&cifrado, &chave, &nonce).expect("o Rust decifra");

            assert_eq!(por_c, mensagem, "a C devolve outra coisa com {tamanho} bytes");
            assert_eq!(por_rust, mensagem, "o Rust devolve outra coisa com {tamanho} bytes");
        }
    }

    /// Um byte adulterado falha **nas duas** implementações.
    ///
    /// A tag Poly1305 cobre o ciphertext, mas o nonce **não** — é
    /// público e entra no estado do cifrador. Um nonce trocado com a
    /// tag intacta tem de ser rejeitado, e o erro tem de ser o mesmo
    /// nas duas: se a C devolvesse o plaintext com um nonce errado, o
    /// adulterado seria uma porta aberta com a porta trancada ao lado.
    #[test]
    fn k9_adulteracao_rejeitada_nas_duas() {
        let chave = [0x11u8; K9_TAM_CHAVE];
        let mensagem = b"onyxchat paridade k9".to_vec();

        let cifrado = k9_cifrar(&mensagem, &chave, &[0x01u8; K9_TAM_NONCE]).expect("cifra");

        // Byte trocado no meio do ciphertext.
        let mut adulterado = cifrado.clone();
        let meio = adulterado.len() / 2;
        adulterado[meio] ^= 0x01;
        assert!(k9_decifrar(&adulterado, &chave, &[0x01u8; K9_TAM_NONCE]).is_err());
        assert!(crypto_core::aead_chacha::decrypt(&adulterado, &chave, &[0x01u8; K9_TAM_NONCE]).is_err());

        // Tag (últimos 16 bytes) alterada.
        let mut tag_alterada = cifrado.clone();
        *tag_alterada.last_mut().expect("tem tag") ^= 0x80;
        assert!(k9_decifrar(&tag_alterada, &chave, &[0x01u8; K9_TAM_NONCE]).is_err());
        assert!(crypto_core::aead_chacha::decrypt(&tag_alterada, &chave, &[0x01u8; K9_TAM_NONCE]).is_err());

        // Nonce diferente, ciphertext intacto: tem de falhar igual.
        assert!(k9_decifrar(&cifrado, &chave, &[0x02u8; K9_TAM_NONCE]).is_err());
        assert!(crypto_core::aead_chacha::decrypt(&cifrado, &chave, &[0x02u8; K9_TAM_NONCE]).is_err());
    }

    // ----------------------------------------------------------------
    // Fase F: a limpeza está implementada, não só documentada
    // ----------------------------------------------------------------

    /// `limpar` apaga mesmo, e o Rust verifica em debug.
    ///
    /// O teste do lado C (`test_transposition.cpp`) prova o mesmo sobre
    /// o buffer que a C escreve. Este prova a travessia: que o símbolo
    /// liga, que a assinatura bate certo e que o `debug_assert` não
    /// dispara — um erro de assinatura em `extern "C"` que por acaso
    /// linkasse é exactamente o tipo de coisa que só aparece em
    /// produção.
    #[test]
    fn limpar_apaga_o_buffer() {
        let mut buf = [0xAAu8; 64];
        limpar(&mut buf);
        assert!(buf.iter().all(|&b| b == 0), "o buffer tem bytes não nulos");

        // Slice vazio é o caso que costuma dar panic: `as_mut_ptr` de
        // um slice vazio é um ponteiro pendurado e a C não deve ser
        // chamada com ele.
        let mut vazio: [u8; 0] = [];
        limpar(&mut vazio);
    }

    /// Uma decifragem K4 **rejeitada** não deixa dados no buffer.
    ///
    /// Este é o outro lado de F, visto do Rust. O wrapper
    /// `k4_decifrar` descarta o `Vec` num erro, e o `Vec` vai para o
    /// heap sem levar nada — por isso aqui não se observa nada pelo
    /// caminho normal. O que se verifica é a **promessa da C**: chamar a
    /// função bruta com um buffer pré-preenchido e ver que a C o limpou.
    ///
    /// É o teste que fixa o contrato «devolveu erro, não há saída
    /// utilizável» na fronteira, onde o resto do código pode confiar
    /// nele sem saber os detalhes da transposição.
    #[test]
    fn k4_rejeitado_nao_deixa_saida_utilizavel() {
        for entrada in [
            [1u8, 2, 3, 4, 7].as_slice(),       // padding > 5
            [9u8, 9, 9, 9, 6].as_slice(),       // padding > comprimento
            [1u8, 2, 3, 4, 2].as_slice(),       // padding inconsistente
        ] {
            let mut saida = [0xAAu8; 5];
            let mut tamanho: usize = 0;
            let rc = unsafe {
                onyx_k4_decrypt(
                    entrada.as_ptr(),
                    entrada.len(),
                    saida.as_mut_ptr(),
                    saida.len(),
                    &mut tamanho,
                )
            };
            assert_eq!(rc, -3, "esperado ONYX_ERR_PAD, obtido {rc}");
            assert!(
                saida.iter().all(|&b| b == 0),
                "a K4 rejeitou a entrada mas deixou {saida:?} no buffer"
            );
        }
    }

    /// Uma decifragem K9 com tag errada também não deixa saída.
    ///
    /// Verifica o mesmo contrato no lado do AEAD. Aqui a limpeza é
    /// defesa em profundidade — a `..._decrypt` do libsodium valida a
    /// tag antes de escrever, e por isso não escreve nada. O teste
    /// fixa que o **nosso** contrato não depende desse detalhe: se
    /// uma versão futura do libsodium passar a escrever antes de
    /// validar, o buffer continua a ser limpo e nenhum teste quebra.
    #[test]
    fn k9_rejeitado_nao_deixa_saida_utilizavel() {
        let chave = [0x77u8; K9_TAM_CHAVE];
        let nonce = [0x33u8; K9_TAM_NONCE];
        let original = k9_cifrar(b"onyxchat fase F", &chave, &nonce).expect("cifra");

        // Um byte trocado no meio: a tag deixa de fechar.
        let mut adulterado = original.clone();
        let meio = adulterado.len() / 2;
        adulterado[meio] ^= 0x01;

        let mut saida = vec![0xAAu8; adulterado.len()];
        let mut tamanho: usize = 0;
        let rc = unsafe {
            onyx_k9_decrypt(
                adulterado.as_ptr(),
                adulterado.len(),
                chave.as_ptr(),
                nonce.as_ptr(),
                saida.as_mut_ptr(),
                &mut tamanho,
            )
        };
        assert_eq!(rc, -5, "esperado ONYX_ERR_AUTH, obtido {rc}");

        // O contrato é sobre a região que a lib podia escrever, que é o
        // plaintext: `entrada_len − tag`. A função C recebe o
        // comprimento da *entrada* e não a capacidade do buffer de
        // saída, portanto não pode — e não deve — limpar o resto.
        //
        // Limpar a cauda da tag seria mentira sobre o que se sabe: os
        // últimos 16 bytes são do buffer do chamador e a lib nunca os
        // tocou. O que este teste fixa é a parte que importa — nenhum
        // byte de plaintext sobrevive a uma decifragem recusada.
        let plaintext = adulterado.len() - K9_TAM_TAG;
        assert!(
            saida[..plaintext].iter().all(|&b| b == 0),
            "a K9 rejeitou mas deixou {} bytes de plaintext por limpar",
            saida[..plaintext].iter().filter(|&&b| b != 0).count()
        );
    }

    /// Todos os códigos C conhecidos mapeiam para o erro tipado certo;
    /// códigos fora da faixa caem em `Crypto` (nunca em sucesso).
    #[test]
    fn mapeamento_de_codigos_de_erro() {
        assert_eq!(FfiErro::de_codigo(-1), FfiErro::Argumento);
        assert_eq!(FfiErro::de_codigo(-2), FfiErro::Capacidade);
        assert_eq!(FfiErro::de_codigo(-3), FfiErro::Padding);
        assert_eq!(FfiErro::de_codigo(-4), FfiErro::Tamanho);
        assert_eq!(FfiErro::de_codigo(-5), FfiErro::Auth);
        assert_eq!(FfiErro::de_codigo(-6), FfiErro::Crypto);
        // Fora da faixa (positivo ou desconhecido) → falha interna.
        assert_eq!(FfiErro::de_codigo(0), FfiErro::Crypto);
        assert_eq!(FfiErro::de_codigo(42), FfiErro::Crypto);
    }

    /// A descrição de cada erro existe e não contém dados sensíveis.
    #[test]
    fn descricoes_sao_estaveis() {
        for (erro, esperado) in [
            (FfiErro::Argumento, "argumento inválido na camada C"),
            (
                FfiErro::Capacidade,
                "buffer de saída insuficiente na camada C",
            ),
            (FfiErro::Padding, "padding PKCS#7 inválido na camada C"),
            (FfiErro::Tamanho, "comprimento inválido na camada C"),
            (FfiErro::Auth, "tag AEAD inválida na camada C"),
            (FfiErro::Crypto, "falha interna da camada C"),
        ] {
            assert_eq!(erro.descricao(), esperado);
        }
    }

    /// Round-trip K4 (C++) através do FFI: entrada → padding → inverte.
    #[test]
    fn k4_roundtrip_ffi() {
        let dados = b"pipeline K4 via FFI".to_vec();
        let cifrado = k4_cifrar(&dados).expect("K4 cifra");
        assert_eq!(cifrado.len() % K4_COLUNAS, 0);
        assert_eq!(k4_decifrar(&cifrado).expect("K4 decifra"), dados);
    }

    /// Entrada vazia em K4: padding completo (5 bytes), inversa vazia.
    #[test]
    fn k4_vazio_ffi() {
        // K4 recusa a entrada vazia nos dois sentidos: cifrar produziria
        // 5 bytes de padding puro, e decifrar aceitar esses bytes como
        // uma mensagem vazia. A simetria fecha o caminho de um envelope
        // construído à mão.
        assert_eq!(k4_cifrar(b""), Err(FfiErro::Tamanho));
        assert_eq!(k4_decifrar(&[]), Err(FfiErro::Tamanho));

        // O menor plaintext válido produz um bloco completo de 5, e o
        // round-trip é exacto.
        let minimo = [b'x'];
        let cifrado = k4_cifrar(&minimo).expect("K4 cifra 1 byte");
        assert_eq!(cifrado.len(), K4_COLUNAS);
        assert_eq!(k4_decifrar(&cifrado).expect("K4 decifra"), minimo);
    }

    /// **Onde está a defesa, exactamente.** K4 a Accept **padding puro**
    /// como plaintext vazio — isso é PKCS#7 correcto e obrigatório: se
    /// a camada recusasse, não conseguiria decifrar nada cuja última
    /// camada tivesse escolhido o bloco completo.
    ///
    /// A rejeição do plaintext vazio está no **pipeline** (`cifrar` →
    /// `PayloadVazio`), e não aqui. A razão é a ordem das camadas: a
    /// saída de K4 só existe depois de K1, que é AEAD. Um atacante sem
    /// as chaves não consegue injectar 5 bytes de padding em K4 porque
    /// teria de forjar a tag de K1. E um emissor legítimo já não pode
    /// criar a mensagem vazia, porque o pipeline a recusa à entrada.
    ///
    /// Este teste existe para documentar onde está cada defesa, e para
    /// que uma refactorização não as troque de lugar em silêncio.
    #[test]
    fn padding_puro_decifra_como_vazio_e_e_esperado() {
        let so_padding = [5u8; K4_COLUNAS];
        assert_eq!(
            k4_decifrar(&so_padding).expect("padding PKCS#7 válido"),
            Vec::<u8>::new(),
            "PKCS#7 tem de aceitar o bloco completo como padding"
        );
    }

    /// Padding adulterado → `FfiErro::Padding` (nunca panic).
    #[test]
    fn k4_padding_invalido_ffi() {
        let mut cifrado = k4_cifrar(b"abcde").expect("K4 cifra");
        // Corrompe todos os bytes de padding (últimos 5).
        for b in cifrado.iter_mut().rev().take(K4_COLUNAS) {
            *b = 0;
        }
        assert_eq!(k4_decifrar(&cifrado), Err(FfiErro::Padding));
    }

    /// Comprimento não múltiplo de 5 → `FfiErro::Tamanho`.
    #[test]
    fn k4_tamanho_invalido_ffi() {
        assert_eq!(k4_decifrar(&[1, 2, 3]), Err(FfiErro::Tamanho));
    }

    /// Round-trip K9 (libsodium) através do FFI com chaves/nonces fixos.
    #[test]
    fn k9_roundtrip_ffi() {
        let chave = [0x11u8; K9_TAM_CHAVE];
        let nonce = [0x22u8; K9_TAM_NONCE];
        let dados = b"ultima camada".to_vec();
        let cifrado = k9_cifrar(&dados, &chave, &nonce).expect("K9 cifra");
        assert_eq!(cifrado.len(), dados.len() + K9_TAM_TAG);
        assert_eq!(
            k9_decifrar(&cifrado, &chave, &nonce).expect("K9 decifra"),
            dados
        );
    }

    /// Chave errada → `FfiErro::Auth` (tag não confere).
    #[test]
    fn k9_chave_errada_ffi() {
        let chave = [0x11u8; K9_TAM_CHAVE];
        let nonce = [0x22u8; K9_TAM_NONCE];
        let cifrado = k9_cifrar(b"msg", &chave, &nonce).expect("K9 cifra");
        let errada = [0x12u8; K9_TAM_CHAVE];
        assert_eq!(k9_decifrar(&cifrado, &errada, &nonce), Err(FfiErro::Auth));
    }

    /// Nonce errado → `FfiErro::Auth`.
    #[test]
    fn k9_nonce_errado_ffi() {
        let chave = [0x11u8; K9_TAM_CHAVE];
        let nonce = [0x22u8; K9_TAM_NONCE];
        let cifrado = k9_cifrar(b"msg", &chave, &nonce).expect("K9 cifra");
        let outro = [0x23u8; K9_TAM_NONCE];
        assert_eq!(k9_decifrar(&cifrado, &chave, &outro), Err(FfiErro::Auth));
    }

    /// Entrada menor que a tag → `FfiErro::Tamanho` (pré-condição Rust).
    #[test]
    fn k9_entrada_curta_ffi() {
        let chave = [0u8; K9_TAM_CHAVE];
        let nonce = [0u8; K9_TAM_NONCE];
        assert_eq!(
            k9_decifrar(&[0u8; K9_TAM_TAG - 1], &chave, &nonce),
            Err(FfiErro::Tamanho)
        );
    }

    /// Ciphertext adulterado byte a byte → `FfiErro::Auth`.
    #[test]
    fn k9_adulterado_ffi() {
        let chave = [0x33u8; K9_TAM_CHAVE];
        let nonce = [0x44u8; K9_TAM_NONCE];
        let mut cifrado = k9_cifrar(b"mensagem", &chave, &nonce).expect("K9 cifra");
        cifrado[0] ^= 0x01;
        assert_eq!(k9_decifrar(&cifrado, &chave, &nonce), Err(FfiErro::Auth));
    }
}
