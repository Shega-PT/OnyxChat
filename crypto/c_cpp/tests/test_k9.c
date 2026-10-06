// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// test_k9.c — testes da camada K9 (ChaCha20-Poly1305 via libsodium)
// ---------------------------------------------------------------------
// Cobrem: vector conhecido (igual ao gerado por PyNaCl — âncora
// cross-language com o espelho Rust), roundtrips, adulteração, chave
// errada, troncagem e validação de argumentos.
// =====================================================================

#include "../onyx_crypto.h"
#include "check.h"

#include <string.h>

// Vector conhecido (gerado com PyNaCl crypto_aead_chacha20poly1305_ietf):
//   chave = 000102..1f · nonce = 000101..0b · pt = "Olá mundo!" (11B)
//   ct‖tag (27B) = c697cba1097ad02ed3ec1e0d6d90cb2e
//                  d51ad4e0f6c833ee3f1dee...
static const uint8_t VETOR_CHAVE[32] = {0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
                                        0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
                                        0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
                                        0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
                                        0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
                                        0x1e, 0x1f};
static const uint8_t VETOR_NONCE[12] = {0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
                                        0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b};
static const uint8_t VETOR_PT[11] = {'O', 'l', 0xc3, 0xa1, ' ', 'm',
                                     'u', 'n', 'd', 'o', '!'};
static const uint8_t VETOR_CT[27] = {
    0xc6, 0x97, 0xcb, 0xa1, 0x09, 0x7a, 0xd0, 0x2e, 0xd3, 0xec,
    0x1e, 0x0d, 0x6d, 0x90, 0xcb, 0x2e, 0xd5, 0x1a, 0xd4, 0xe0,
    0xf6, 0xc8, 0x33, 0xee, 0x3f, 0x1d, 0xee};

/// Roundtrip K9 com os parâmetros indicados (capacidade documentada).
void roundtrip(const uint8_t *pt, size_t pt_len, const uint8_t *chave,
               const uint8_t *nonce) {
    uint8_t ct[512];
    uint8_t volta[512];
    size_t ct_len = 0;
    size_t volta_len = 0;

    CHECK(pt_len + ONYX_K9_TAG_LEN <= sizeof(ct));
    CHECK_EQ_LONG(onyx_k9_encrypt(pt, pt_len, chave, nonce, ct, &ct_len),
                  ONYX_OK);
    CHECK_EQ_LONG(ct_len, pt_len + ONYX_K9_TAG_LEN);
    CHECK_EQ_LONG(onyx_k9_decrypt(ct, ct_len, chave, nonce, volta, &volta_len),
                  ONYX_OK);
    CHECK_EQ_LONG(volta_len, pt_len);
    CHECK(memcmp(volta, pt, pt_len) == 0);
}

int main() {
    // ---- Vector conhecido (cross-language com PyNaCl/Rust) ----------
    {
        uint8_t ct[64];
        size_t ct_len = 0;
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, sizeof(VETOR_PT), VETOR_CHAVE,
                                      VETOR_NONCE, ct, &ct_len),
                      ONYX_OK);
        CHECK_EQ_LONG(ct_len, 27);
        CHECK(memcmp(ct, VETOR_CT, 27) == 0);

        // E o inverso recupera exatamente o plaintext UTF-8.
        uint8_t volta[64];
        size_t volta_len = 0;
        CHECK_EQ_LONG(onyx_k9_decrypt(ct, ct_len, VETOR_CHAVE, VETOR_NONCE,
                                      volta, &volta_len),
                      ONYX_OK);
        CHECK_EQ_LONG(volta_len, sizeof(VETOR_PT));
        CHECK(memcmp(volta, VETOR_PT, sizeof(VETOR_PT)) == 0);
    }

    // ---- Roundtrips de tamanhos variados ----------------------------
    roundtrip(VETOR_PT, sizeof(VETOR_PT), VETOR_CHAVE, VETOR_NONCE);
    roundtrip((const uint8_t *)"", 0, VETOR_CHAVE, VETOR_NONCE);
    roundtrip((const uint8_t *)"A", 1, VETOR_CHAVE, VETOR_NONCE);
    {
        uint8_t grande[300];
        for (size_t i = 0; i < sizeof(grande); ++i) {
            grande[i] = (uint8_t)(i * 131);
        }
        roundtrip(grande, sizeof(grande), VETOR_CHAVE, VETOR_NONCE);
    }

    // ---- Nonce diferente → ciphertext diferente ---------------------
    {
        uint8_t nonce2[12];
        memcpy(nonce2, VETOR_NONCE, 12);
        nonce2[0] = 0xff;
        uint8_t a[64], b[64];
        size_t a_len = 0, b_len = 0;
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, sizeof(VETOR_PT), VETOR_CHAVE,
                                      VETOR_NONCE, a, &a_len),
                      ONYX_OK);
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, sizeof(VETOR_PT), VETOR_CHAVE,
                                      nonce2, b, &b_len),
                      ONYX_OK);
        CHECK(a_len == b_len && memcmp(a, b, a_len) != 0);
    }

    // ---- Chave errada → ONYX_ERR_AUTH -------------------------------
    {
        uint8_t chave_errada[32];
        memcpy(chave_errada, VETOR_CHAVE, 32);
        chave_errada[0] ^= 0x01;
        uint8_t ct[64], out[64];
        size_t ct_len = 0, out_len = 0;
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, sizeof(VETOR_PT), VETOR_CHAVE,
                                      VETOR_NONCE, ct, &ct_len),
                      ONYX_OK);
        CHECK_EQ_LONG(onyx_k9_decrypt(ct, ct_len, chave_errada, VETOR_NONCE,
                                      out, &out_len),
                      ONYX_ERR_AUTH);
    }

    // ---- Adulteração de um byte → ONYX_ERR_AUTH ---------------------
    {
        uint8_t ct[64], out[64];
        size_t ct_len = 0, out_len = 0;
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, sizeof(VETOR_PT), VETOR_CHAVE,
                                      VETOR_NONCE, ct, &ct_len),
                      ONYX_OK);
        ct[3] ^= 0x01;
        CHECK_EQ_LONG(onyx_k9_decrypt(ct, ct_len, VETOR_CHAVE, VETOR_NONCE,
                                      out, &out_len),
                      ONYX_ERR_AUTH);
    }

    // ---- Troncagem abaixo da tag → ONYX_ERR_TAMANHO -----------------
    {
        uint8_t out[64];
        size_t out_len = 0;
        uint8_t curto[15] = {0};
        CHECK_EQ_LONG(onyx_k9_decrypt(curto, sizeof(curto), VETOR_CHAVE,
                                      VETOR_NONCE, out, &out_len),
                      ONYX_ERR_TAMANHO);
        CHECK_EQ_LONG(onyx_k9_decrypt(curto, 0, VETOR_CHAVE, VETOR_NONCE, out,
                                      &out_len),
                      ONYX_ERR_TAMANHO);
    }

    // ---- Argumentos inválidos → ONYX_ERR_ARG ------------------------
    {
        uint8_t buf[64];
        size_t len = 0;
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, 1, VETOR_CHAVE, VETOR_NONCE,
                                      NULL, &len),
                      ONYX_ERR_ARG); // saída nula com dados
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, 1, VETOR_CHAVE, VETOR_NONCE,
                                      buf, NULL),
                      ONYX_ERR_ARG); // len nulo
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, 1, NULL, VETOR_NONCE, buf,
                                      &len),
                      ONYX_ERR_ARG); // chave nula
        CHECK_EQ_LONG(onyx_k9_encrypt(VETOR_PT, 1, VETOR_CHAVE, NULL, buf,
                                      &len),
                      ONYX_ERR_ARG); // nonce nulo
        CHECK_EQ_LONG(onyx_k9_decrypt(VETOR_PT, 20, NULL, VETOR_NONCE, buf,
                                      &len),
                      ONYX_ERR_ARG);
        CHECK_EQ_LONG(onyx_k9_decrypt(VETOR_PT, 20, VETOR_CHAVE, NULL, buf,
                                      &len),
                      ONYX_ERR_ARG);
        CHECK_EQ_LONG(onyx_k9_decrypt(VETOR_PT, 20, VETOR_CHAVE, VETOR_NONCE,
                                      NULL, &len),
                      ONYX_ERR_ARG);
        CHECK_EQ_LONG(onyx_k9_decrypt(VETOR_PT, 20, VETOR_CHAVE, VETOR_NONCE,
                                      buf, NULL),
                      ONYX_ERR_ARG);
        // Entrada nula com comprimento 0 é aceite (plaintext vazio).
        CHECK_EQ_LONG(onyx_k9_encrypt(NULL, 0, VETOR_CHAVE, VETOR_NONCE,
                                      buf, &len),
                      ONYX_OK);
        CHECK_EQ_LONG(len, ONYX_K9_TAG_LEN); // só a tag
    }

    ONYX_FIM();
}
