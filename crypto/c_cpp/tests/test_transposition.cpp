// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// test_transposition.cpp — testes da camada K4 (transposição 5 colunas)
// ---------------------------------------------------------------------
// Cobrem: vectores conhecidos, roundtrip 1..=17 bytes (pares e ímpares),
// caminhos de erro (ARG/CAP/TAMANHO/PAD) e validação do padding PKCS#7.
// =====================================================================

#include "../onyx_crypto.h"
#include "check.h"

#include <cstring>
#include <string>
#include <vector>

namespace {

/// Roundtrip K4 para um buffer de tamanho arbitrário.
///
/// Pressupõe `dados` não vazio: K4 recusa a entrada vazio, e o caso
/// é verificado à parte (`entrada_vazia_e_recusada`).
void roundtrip(const std::vector<uint8_t> &dados) {
    CHECK(!dados.empty());
    const size_t cap = onyx_k4_encrypt_len(dados.size());
    std::vector<uint8_t> cifrado(cap);
    size_t cifrado_len = 0;
    CHECK_EQ_LONG(onyx_k4_encrypt(dados.data(), dados.size(), cifrado.data(),
                                  cifrado.size(), &cifrado_len),
                  ONYX_OK);
    CHECK_EQ_LONG(cifrado_len, cap);

    std::vector<uint8_t> claro(dados.size() + ONYX_K4_COLUNAS);
    size_t claro_len = 0;
    CHECK_EQ_LONG(onyx_k4_decrypt(cifrado.data(), cifrado_len, claro.data(),
                                  claro.size(), &claro_len),
                  ONYX_OK);
    CHECK_EQ_LONG(claro_len, dados.size());
    CHECK(std::memcmp(claro.data(), dados.data(), dados.size()) == 0);
}

/// Fase F: uma decifragem K4 **rejeitada** tem de deixar o buffer de
/// saída limpo.
///
/// A K4 valida o padding PKCS#7 **depois** de escrever a
/// des-transposição no buffer de saída. Essa ordem é necessária — o
/// último byte do buffer des-transposto é justamente o byte de padding,
/// e não há como o validar antes de o escrever. A consequência é que,
/// num `ONYX_ERR_PAD`, o buffer fica cheio de dados que foram
/// aceitos como entrada e rejeitados só no fim.
///
/// O resultado devolvido é o erro, e o caller pode ignorar o buffer —
/// mas quem chama a C não é obrigado a saber disso, e o contrato da
/// função é que um erro não deixa saída utilizável. Um buffer cheio de
/// material des-transposto de uma mensagem que o AEAD de K9 já tinha
/// autenticado é pior do que um buffer vazio: parece bom.
///
/// Por isso os dois caminhos de `ONYX_ERR_PAD` limpam o que escreveram.
void saida_limpa_em_decifragem_rejeitada() {
    uint8_t out[64];
    size_t len = 0;

    // Preenchimento de 0xAA: se a função não limpar, sobram 0xAA
    // onde devia haver zeros, e o teste vê.
    memset(out, 0xAA, sizeof(out));

    // Padding de valor 7 (> 5) — rejeitado no primeiro `if`.
    const uint8_t pad_grande[5] = {1, 2, 3, 4, 7};
    CHECK_EQ_LONG(onyx_k4_decrypt(pad_grande, 5, out, sizeof(out), &len),
                  ONYX_ERR_PAD);
    for (size_t i = 0; i < 5; ++i) {
        CHECK_EQ_LONG(out[i], 0);
    }

    // Padding consistente no valor mas não em todos os bytes
    // (`{1,2,3,4,2}`: diz 2 e tem dois 2, mas o segundo não está onde
    // o PKCS#7 exige) — rejeitado no segundo `for`.
    memset(out, 0xAA, sizeof(out));
    const uint8_t pad_inconsistente[5] = {1, 2, 3, 4, 2};
    CHECK_EQ_LONG(
        onyx_k4_decrypt(pad_inconsistente, 5, out, sizeof(out), &len),
        ONYX_ERR_PAD);
    for (size_t i = 0; i < 5; ++i) {
        CHECK_EQ_LONG(out[i], 0);
    }

    // E o mesmo numa mensagem maior, para não ser um caso de 5 bytes.
    memset(out, 0xAA, sizeof(out));
    uint8_t grande[40];
    memset(grande, 0x5A, sizeof(grande));
    grande[39] = 9; // último byte = 9, inválido (> 5)
    CHECK_EQ_LONG(onyx_k4_decrypt(grande, sizeof(grande), out, sizeof(out),
                                  &len),
                  ONYX_ERR_PAD);
    for (size_t i = 0; i < sizeof(grande); ++i) {
        CHECK_EQ_LONG(out[i], 0);
    }
}

/// Uma decifragem K4 ** aceite** continua a devolver o plaintext.
///
/// O teste anterior exige que o buffer seja limpo no erro. Este exige o
/// contrário no caminho feliz — os dois juntos é que provam que a
/// limpeza está no ramo de erro e não em todo o lado, que seria
/// uma situação mais subtil de errar.
void saida_preenchida_em_decifragem_aceite() {
    uint8_t out[64];
    size_t len = 0;
    memset(out, 0xAA, sizeof(out));

    const uint8_t pad_ok[10] = {10, 15, 11, 16, 12, 17, 13, 2, 14, 2};
    CHECK_EQ_LONG(onyx_k4_decrypt(pad_ok, 10, out, sizeof(out), &len), ONYX_OK);
    CHECK_EQ_LONG(len, 8);
    for (size_t i = 0; i < 8; ++i) {
        CHECK_EQ_LONG(out[i], 10 + i);
    }
    // Os bytes do padding também são deixados como a função os pôs —
    // o contrato diz `*saida_len` bytes válidos, não `saida` limpa.
    CHECK_EQ_LONG(out[8], 2);
    CHECK_EQ_LONG(out[9], 2);
}

/// `onyx_limpar` limpa exactamente os bytes pedidos, e só esses.
void onyx_limpar_limpa_o_que_se_pede() {
    uint8_t buf[32];
    memset(buf, 0xAA, sizeof(buf));

    onyx_limpar(buf, 10);
    for (size_t i = 0; i < 10; ++i) {
        CHECK_EQ_LONG(buf[i], 0);
    }
    for (size_t i = 10; i < sizeof(buf); ++i) {
        CHECK_EQ_LONG(buf[i], 0xAA); // o resto não foi tocado
    }
}

/// `onyx_limpar` com ponteiro nulo ou comprimento zero não faz nada.
///
/// Um `sodium_memzero(NULL, 0)` é inofensivo, mas a nossa função é
/// chamada em caminhos de erro onde o ponteiro pode não ter sido
/// tocado — e o teste fixa que isso não é um crash.
void onyx_limpar_e_segura_nos_limites() {
    onyx_limpar(NULL, 0);
    onyx_limpar(NULL, 16);
    uint8_t buf[4] = {1, 2, 3, 4};
    onyx_limpar(buf, 0);
    CHECK_EQ_LONG(buf[0], 1);
    CHECK_EQ_LONG(buf[3], 4);
}

} // namespace

int main() {
    // ---- Vectores conhecidos ----------------------------------------
    // 5 bytes (alinhado) → PKCS#7 acrescenta bloco inteiro [5×5]:
    // linhas = 2 → leitura por colunas = "A5B5C5D5E5".
    {
        const char *pt = "ABCDE";
        uint8_t out[16];
        size_t out_len = 0;
        CHECK_EQ_LONG(onyx_k4_encrypt(reinterpret_cast<const uint8_t *>(pt), 5,
                                      out, sizeof(out), &out_len),
                      ONYX_OK);
        CHECK_EQ_LONG(out_len, 10);
        // pad byte 0x05 (PKCS#7), não o caractere '5' — array explícito
        // porque "\x05B" num literal C seria lido como um só hex 0x5B.
        const uint8_t esperado[10] = {'A', 5, 'B', 5, 'C', 5, 'D', 5, 'E', 5};
        CHECK(std::memcmp(out, esperado, 10) == 0);
    }
    // 2 bytes → padding 3 → uma única linha (transposição identidade).
    {
        uint8_t out[8];
        size_t out_len = 0;
        CHECK_EQ_LONG(onyx_k4_encrypt(reinterpret_cast<const uint8_t *>("AB"), 2,
                                      out, sizeof(out), &out_len),
                      ONYX_OK);
        CHECK_EQ_LONG(out_len, 5);
        CHECK(std::memcmp(out, "AB\x03\x03\x03", 5) == 0);
    }
    // "AB" decifra de volta (vector inverso).
    {
        const uint8_t ct[5] = {'A', 'B', 3, 3, 3};
        uint8_t out[8];
        size_t out_len = 0;
        CHECK_EQ_LONG(onyx_k4_decrypt(ct, 5, out, sizeof(out), &out_len),
                      ONYX_OK);
        CHECK_EQ_LONG(out_len, 2);
        CHECK(std::memcmp(out, "AB", 2) == 0);
    }

    // ---- Roundtrip exaustivo 1..=17 ---------------------------------
    // Começa em 1: a entrada vazia é recusada e tem teste próprio.
    for (size_t n = 1; n <= 17; ++n) {
        std::vector<uint8_t> dados(n);
        for (size_t i = 0; i < n; ++i) {
            dados[i] = static_cast<uint8_t>(i * 37 + n);
        }
        roundtrip(dados);
    }
    // Buffer grande (várias linhas) com todos os bytes possíveis.
    {
        std::vector<uint8_t> dados(256);
        for (size_t i = 0; i < 256; ++i) {
            dados[i] = static_cast<uint8_t>(i);
        }
        roundtrip(dados);
    }

    // ---- `onyx_k4_encrypt_len` --------------------------------------
    // Entrada vazia é inválida: 0 é o sinal de erro, e nunca uma
    // capacidade válida (o menor ciphertext tem 5 bytes).
    CHECK_EQ_LONG(onyx_k4_encrypt_len(0), 0);
    CHECK_EQ_LONG(onyx_k4_encrypt_len(4), 5);
    CHECK_EQ_LONG(onyx_k4_encrypt_len(5), 10); // alinhado → +5
    CHECK_EQ_LONG(onyx_k4_encrypt_len(6), 10);

    // ---- Erros: argumentos ------------------------------------------
    {
        uint8_t buf[16];
        size_t len = 0;
        CHECK_EQ_LONG(onyx_k4_encrypt(buf, 1, nullptr, 16, &len),
                      ONYX_ERR_ARG); // saída nula
        CHECK_EQ_LONG(onyx_k4_encrypt(buf, 1, buf, 16, nullptr),
                      ONYX_ERR_ARG); // len nulo
        CHECK_EQ_LONG(onyx_k4_encrypt(nullptr, 4, buf, 16, &len),
                      ONYX_ERR_ARG); // entrada nula com dados
        CHECK_EQ_LONG(onyx_k4_decrypt(buf, 5, nullptr, 16, &len),
                      ONYX_ERR_ARG);
        CHECK_EQ_LONG(onyx_k4_decrypt(buf, 5, buf, 16, nullptr),
                      ONYX_ERR_ARG);
        CHECK_EQ_LONG(onyx_k4_decrypt(nullptr, 5, buf, 16, &len),
                      ONYX_ERR_ARG);
        // Entrada nula com comprimento 0 é recusada pelo mesmo motivo
        // que a entrada vazia não nula: cifrar 0 bytes produziria só
        // padding, que a decifragem leria como mensagem vazia.
        CHECK_EQ_LONG(onyx_k4_encrypt(nullptr, 0, buf, 16, &len),
                      ONYX_ERR_TAMANHO);
        CHECK_EQ_LONG(onyx_k4_encrypt(buf, 0, buf, 16, &len),
                      ONYX_ERR_TAMANHO);
    }

    // ---- Entrada vazia: recusada nos dois sentidos ------------------
    {
        // A recusa tem de ser simétrica. Se um lado aceitasse 0 bytes,
        // um envelope construído à mão poderia contornar a validação do
        // pipeline: a cifra de vazio seria lida como decifra de vazio.
        uint8_t buf[16];
        size_t len = 0;
        CHECK_EQ_LONG(onyx_k4_encrypt(buf, 0, buf, 16, &len),
                      ONYX_ERR_TAMANHO);
        CHECK_EQ_LONG(len, 0); // nada escrito em caso de erro
        CHECK_EQ_LONG(onyx_k4_decrypt(buf, 0, buf, 16, &len),
                      ONYX_ERR_TAMANHO);

        // O mínimo válido é 1 byte, e faz roundtrip exacto.
        const uint8_t minimo[1] = {'x'};
        size_t cifrado_len = 0;
        CHECK_EQ_LONG(onyx_k4_encrypt(minimo, 1, buf, sizeof(buf),
                                      &cifrado_len),
                      ONYX_OK);
        CHECK_EQ_LONG(cifrado_len, ONYX_K4_COLUNAS);
        // PKCS#7 com bloco 5: 1 byte de dados → 4 bytes de padding de
        // valor 4. (Não é 1: o valor do padding é sempre o número de
        // bytes acrescentados, e para alcançar o bloco completo são
        // necessários 4.)
        CHECK_EQ_LONG(buf[1], 4);
        CHECK_EQ_LONG(buf[2], 4);
        CHECK_EQ_LONG(buf[3], 4);
        CHECK_EQ_LONG(buf[ONYX_K4_COLUNAS - 1], 4);

        size_t claro_len = 0;
        CHECK_EQ_LONG(onyx_k4_decrypt(buf, cifrado_len, buf, sizeof(buf),
                                      &claro_len),
                      ONYX_OK);
        CHECK_EQ_LONG(claro_len, 1);
        CHECK(buf[0] == 'x');
    }

    // ---- Erros: capacidade insuficiente -----------------------------
    {
        uint8_t pequeno[3];
        size_t len = 0;
        CHECK_EQ_LONG(onyx_k4_encrypt(reinterpret_cast<const uint8_t *>("A"), 1,
                                      pequeno, sizeof(pequeno), &len),
                      ONYX_ERR_CAP);
        CHECK_EQ_LONG(onyx_k4_decrypt(reinterpret_cast<const uint8_t *>("ABCDE"),
                                      5, pequeno, sizeof(pequeno), &len),
                      ONYX_ERR_CAP);
    }

    // ---- Erros: comprimento do ciphertext ---------------------------
    {
        uint8_t buf[16];
        size_t len = 0;
        CHECK_EQ_LONG(onyx_k4_decrypt(buf, 0, buf, sizeof(buf), &len),
                      ONYX_ERR_TAMANHO); // vazio
        CHECK_EQ_LONG(onyx_k4_decrypt(buf, 6, buf, sizeof(buf), &len),
                      ONYX_ERR_TAMANHO); // não múltiplo de 5
    }

    // ---- Erros: padding inválido ------------------------------------
    {
        // 1 linha (5 bytes) → decifra como identidade:
        const uint8_t pad_nulo[5] = {1, 2, 3, 4, 0}; // pad = 0 → inválido
        uint8_t out[16];
        size_t len = 0;
        CHECK_EQ_LONG(onyx_k4_decrypt(pad_nulo, 5, out, sizeof(out), &len),
                      ONYX_ERR_PAD);

        const uint8_t pad_grande[5] = {1, 2, 3, 4, 7}; // pad > 5 → inválido
        CHECK_EQ_LONG(onyx_k4_decrypt(pad_grande, 5, out, sizeof(out), &len),
                      ONYX_ERR_PAD);

        const uint8_t pad_maior_buffer[5] = {9, 9, 9, 9, 6}; // pad > len
        CHECK_EQ_LONG(onyx_k4_decrypt(pad_maior_buffer, 5, out, sizeof(out),
                                      &len),
                      ONYX_ERR_PAD);

        const uint8_t pad_inconsistente[5] = {1, 2, 3, 4, 2}; // bytes ≠ pad
        CHECK_EQ_LONG(onyx_k4_decrypt(pad_inconsistente, 5, out, sizeof(out),
                                      &len),
                      ONYX_ERR_PAD);

        // 2 linhas (10 bytes): ciphertext real de "10..17" (8 bytes)
        // com padding 2 — colunas [10,15][11,16][12,17][13,2][14,2].
        const uint8_t pad_ok[10] = {10, 15, 11, 16, 12, 17, 13, 2, 14, 2};
        CHECK_EQ_LONG(onyx_k4_decrypt(pad_ok, 10, out, sizeof(out), &len),
                      ONYX_OK);
        CHECK_EQ_LONG(len, 8);
        for (uint8_t i = 0; i < 8; ++i) {
            CHECK(out[i] == 10 + i);
        }
    }

    // ---- Fase F: limpeza de memória nos caminhos de erro ----------
    saida_limpa_em_decifragem_rejeitada();
    saida_preenchida_em_decifragem_aceite();
    onyx_limpar_limpa_o_que_se_pede();
    onyx_limpar_e_segura_nos_limites();

    ONYX_FIM();
}
