// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// k9_chacha.c — camada K9 do pipeline: ChaCha20-Poly1305 (libsodium)
// ---------------------------------------------------------------------
// Última camada de cifragem: só quem detém a chave do receptor (K9)
// consegue abrir a mensagem. Usa a primitiva `chacha20poly1305_ietf`
// do libsodium (a mesma do K1, garantindo compatibilidade de vectores
// com a referência em Rust `crypto_core::aead_chacha`).
//
// A igualdade byte a byte entre esta camada e a referência em Rust não
// é uma suposição: está verificada em
// `ffi_c::tests::k9_c_igual_a_k9_rust`, que compara as duas saídas
// para os mesmos (chave, nonce, mensagem) em vários tamanhos.
//
// Layout do output: ciphertext ‖ tag(16 bytes) — igual às restantes
// camadas AEAD do projeto. Ficheiro C11 puro (sem sintaxe C++).
// =====================================================================

#include "onyx_crypto.h"

#include <sodium.h>

// ---------------------------------------------------------------------
// onyx_limpar — o único ponto de zeroização do projecto
// ---------------------------------------------------------------------
// Implementada aqui, e não em `transposition.cpp`, porque este é o
// ficheiro que já inclui `sodium.h`. Pôr a implementação no único
// sítio onde a dependência existe evita ter de dar ao TU de C++ um
// include de libsodium que ele não usa para mais nada — e a
// declaração em `onyx_crypto.h` chega para a C++ a chamar.

void onyx_limpar(void *pnt, size_t len) {
    // `NULL`/`0` é o caso explícito e não um acidente: quem chama isto
    // está a limpar o que *talvez* tenha escrito, e "talvez" inclui
    // "nada". `sodium_memzero` já toleria, mas tornar o contrato
    // explícito aqui evita que alguém teste se a lib tolera — e
    // depende de uma garantia que é dela, não nossa.
    if (pnt == NULL || len == 0) {
        return;
    }
    sodium_memzero(pnt, len);
}

// `sodium_init()` é idempotente e consultivo para estas primitivas:
//  * 0 = inicializado agora, 1 = já ativo, -1 = outra thread em curso.
// Mesmo com -1 o ChaCha20-Poly1305 continua correto — a falha só
// desliga dispatch otimizado (fallback genérico é válido). Por isso o
// resultado é ignorado: um ramo de erro aqui seria código inexequível.

int onyx_k9_encrypt(const uint8_t *entrada, size_t entrada_len,
                    const uint8_t chave[ONYX_K9_KEY_LEN],
                    const uint8_t nonce[ONYX_K9_NONCE_LEN], uint8_t *saida,
                    size_t *saida_len) {
    if (saida_len == NULL || chave == NULL || nonce == NULL ||
        (saida == NULL && entrada_len > 0)) {
        return ONYX_ERR_ARG;
    }
    const int pronto = sodium_init(); /* ver comentário acima */
    (void)pronto;
    unsigned long long clen = 0;
    // `..._encrypt` de chacha20poly1305 termina sempre em `return 0`
    // em libsodium (sem alocação nem condições de falha) — não
    // verificamos o retorno para não deixar ramo inexequível; a tag
    // Poly1305 garante sempre a integridade do output.
    (void)crypto_aead_chacha20poly1305_ietf_encrypt(
        saida, &clen, entrada, entrada_len,
        /*ad=*/NULL, /*adlen=*/0, /*nsec=*/NULL, nonce, chave);
    *saida_len = (size_t)clen;
    return ONYX_OK;
}

int onyx_k9_decrypt(const uint8_t *entrada, size_t entrada_len,
                    const uint8_t chave[ONYX_K9_KEY_LEN],
                    const uint8_t nonce[ONYX_K9_NONCE_LEN], uint8_t *saida,
                    size_t *saida_len) {
    if (saida_len == NULL || chave == NULL || nonce == NULL ||
        (saida == NULL && entrada_len > 0)) {
        return ONYX_ERR_ARG;
    }
    // Ciphertext sem a tag completa é estruturalmente inválido.
    if (entrada_len < ONYX_K9_TAG_LEN) {
        return ONYX_ERR_TAMANHO;
    }
    const int pronto = sodium_init(); /* ver comentário acima */
    (void)pronto;
    unsigned long long plen = 0;
    const int rc = crypto_aead_chacha20poly1305_ietf_decrypt(
        saida, &plen, /*nsec=*/NULL, entrada, entrada_len,
        /*ad=*/NULL, /*adlen=*/0, nonce, chave);
    if (rc != 0) {
        // Tag inválida: mensagem adulterada, chave errada ou nonce errado.
        //
        // Limpar a saída é defesa em profundidade, não reparo de um bug
        // conhecido. A `..._decrypt` do libsodium valida a tag **antes**
        // de escrever, e por isso hoje não escreve nada neste caminho —
        // o buffer continua com o que o chamador lá pôs. Limpar
        // mesmo assim significa que o contrato «devolveu erro, não há
        // saída utilizável» não depende de um detalhe de implementação
        // do libsodium que pode mudar de versão sem aviso.
        if (saida != NULL) {
            onyx_limpar(saida, entrada_len - ONYX_K9_TAG_LEN);
        }
        return ONYX_ERR_AUTH;
    }
    *saida_len = (size_t)plen;
    return ONYX_OK;
}
