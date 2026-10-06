// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// test_vectors.c — os vetores oficiais observados pelo C/C++
// ---------------------------------------------------------------------
// Lê `VETORES_DIR/camadas.json` (caminho absoluto, definido pelo
// CMake; os JSON são gerados por `examples/gerar_vetores.rs` no Rust)
// e verifica que as camadas C/C++ reproduzem exatamente os outputs
// documentados (Resumo §25):
//
//   K4 → transposição 5 colunas (C++, transposition.cpp)
//   K9 → ChaCha20-Poly1305 via libsodium (C, k9_chacha.c)
//
// Parser JSON mínimo: cobre o subconjunto que o serde_json produz para
// este ficheiro (objetos com campos string; array de objetos).
// =====================================================================

#include "../onyx_crypto.h"
#include "check.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef VETORES_DIR
#error "VETORES_DIR tem de ser definido pelo CMake (tests/vectors)"
#endif

// ---------------------------------------------------------------------
// Parser mínimo de JSON (só strings dentro de objetos)
// ---------------------------------------------------------------------

/// Avança para o próximo carácter não branco.
static void pular_espacos(const char **p) {
    while (**p == ' ' || **p == '\t' || **p == '\n' || **p == '\r') {
        (*p)++;
    }
}

/// Lê uma string JSON (aponta para `"`), avança `*p` e devolve
/// memória própria com o conteúdo sem escape. Caller liberta.
static char *ler_string(const char **p) {
    if (**p != '"') {
        return NULL;
    }
    (*p)++;
    size_t capacidade = 64;
    size_t comprimento = 0;
    char *texto = (char *)malloc(capacidade);
    if (texto == NULL) {
        return NULL;
    }
    while (**p != '\0' && **p != '"') {
        char c = **p;
        if (c == '\\') {
            (*p)++;
            c = **p;
            if (c == 'n') {
                c = '\n';
            } else if (c == 't') {
                c = '\t';
            }
            // `\"` e `\\` ficam com o próprio carácter.
        }
        if (comprimento + 2 > capacidade) {
            capacidade *= 2;
            char *crescido = (char *)realloc(texto, capacidade);
            if (crescido == NULL) {
                free(texto);
                return NULL;
            }
            texto = crescido;
        }
        texto[comprimento++] = c;
        (*p)++;
    }
    if (**p == '"') {
        (*p)++;
    }
    texto[comprimento] = '\0';
    return texto;
}

/// Converte hex minúsculo para bytes; devolve 0 em sucesso.
static int hex_para_bytes(const char *hex, uint8_t *out, size_t max,
                          size_t *len) {
    size_t n = strlen(hex);
    if (n % 2 != 0 || n / 2 > max) {
        return -1;
    }
    for (size_t i = 0; i < n; i += 2) {
        char par[3] = {hex[i], hex[i + 1], '\0'};
        char *fim = NULL;
        long v = strtol(par, &fim, 16);
        if (fim == NULL || *fim != '\0' || v < 0 || v > 255) {
            return -1;
        }
        out[i / 2] = (uint8_t)v;
    }
    *len = n / 2;
    return 0;
}

// ---------------------------------------------------------------------
// Verificação das camadas C/C++
// ---------------------------------------------------------------------

#define TAM_BUF 1024

/// K4: recomputa o output documentado e valida o roundtrip.
static void verificar_k4(const char *input_hex, const char *output_hex) {
    uint8_t entrada[TAM_BUF];
    uint8_t esperado[TAM_BUF];
    uint8_t saida[TAM_BUF];
    uint8_t volta[TAM_BUF];
    size_t entrada_len = 0;
    size_t esperado_len = 0;
    size_t saida_len = 0;
    size_t volta_len = 0;

    CHECK_EQ_LONG(hex_para_bytes(input_hex, entrada, sizeof entrada, &entrada_len), 0);
    CHECK_EQ_LONG(hex_para_bytes(output_hex, esperado, sizeof esperado, &esperado_len), 0);

    CHECK_EQ_LONG(onyx_k4_encrypt(entrada, entrada_len, saida, sizeof saida, &saida_len),
                  ONYX_OK);
    CHECK(saida_len == esperado_len && memcmp(saida, esperado, saida_len) == 0);

    CHECK_EQ_LONG(onyx_k4_decrypt(saida, saida_len, volta, sizeof volta, &volta_len),
                  ONYX_OK);
    CHECK(volta_len == entrada_len && memcmp(volta, entrada, volta_len) == 0);
}

/// K9: recomputa o output documentado e valida o roundtrip (tag AEAD).
static void verificar_k9(const char *input_hex, const char *output_hex,
                         const char *chave_hex, const char *nonce_hex) {
    uint8_t entrada[TAM_BUF];
    uint8_t esperado[TAM_BUF];
    uint8_t saida[TAM_BUF];
    uint8_t volta[TAM_BUF];
    uint8_t chave[ONYX_K9_KEY_LEN];
    uint8_t nonce[ONYX_K9_NONCE_LEN];
    size_t entrada_len = 0;
    size_t esperado_len = 0;
    size_t saida_len = 0;
    size_t volta_len = 0;
    size_t chave_len = 0;
    size_t nonce_len = 0;

    CHECK_EQ_LONG(hex_para_bytes(input_hex, entrada, sizeof entrada, &entrada_len), 0);
    CHECK_EQ_LONG(hex_para_bytes(output_hex, esperado, sizeof esperado, &esperado_len), 0);
    CHECK_EQ_LONG(hex_para_bytes(chave_hex, chave, sizeof chave, &chave_len), 0);
    CHECK_EQ_LONG(hex_para_bytes(nonce_hex, nonce, sizeof nonce, &nonce_len), 0);
    CHECK(chave_len == ONYX_K9_KEY_LEN && nonce_len == ONYX_K9_NONCE_LEN);

    CHECK_EQ_LONG(
        onyx_k9_encrypt(entrada, entrada_len, chave, nonce, saida, &saida_len), ONYX_OK);
    CHECK(saida_len == esperado_len && memcmp(saida, esperado, saida_len) == 0);

    CHECK_EQ_LONG(
        onyx_k9_decrypt(saida, saida_len, chave, nonce, volta, &volta_len), ONYX_OK);
    CHECK(volta_len == entrada_len && memcmp(volta, entrada, volta_len) == 0);
}

// ---------------------------------------------------------------------
// main
// ---------------------------------------------------------------------

int main(void) {
    const char *caminho = VETORES_DIR "/camadas.json";
    FILE *ficheiro = fopen(caminho, "rb");
    CHECK(ficheiro != NULL);
    if (ficheiro == NULL) {
        fprintf(stderr, "não abre %s (correr o gerador de vetores)\n", caminho);
        ONYX_FIM();
    }

    fseek(ficheiro, 0, SEEK_END);
    long tamanho = ftell(ficheiro);
    fseek(ficheiro, 0, SEEK_SET);
    CHECK(tamanho > 0);
    if (tamanho <= 0) {
        fclose(ficheiro);
        ONYX_FIM();
    }
    char *json = (char *)malloc((size_t)tamanho + 1);
    CHECK(json != NULL);
    if (json == NULL) {
        fclose(ficheiro);
        ONYX_FIM();
    }
    size_t lidos = fread(json, 1, (size_t)tamanho, ficheiro);
    json[lidos] = '\0';
    fclose(ficheiro);

    // Localiza o array "camadas" e itera os objetos (todos planos).
    const char *p = json;
    const char *campo = strstr(p, "\"camadas\"");
    CHECK(campo != NULL);
    if (campo == NULL) {
        free(json);
        ONYX_FIM();
    }
    p = strchr(campo, '[');
    CHECK(p != NULL);
    if (p == NULL) {
        free(json);
        ONYX_FIM();
    }
    p++;

    int entradas = 0;
    int k4_visto = 0;
    int k9_visto = 0;
    for (;;) {
        pular_espacos(&p);
        if (*p == ']') {
            break;
        }
        if (*p == ',') {
            p++;
            continue;
        }
        if (*p != '{') {
            CHECK(*p == '{');
            break;
        }
        p++;

        // Campos do objeto (todos string).
        char id[16] = "";
        char input_hex[TAM_BUF] = "";
        char output_hex[TAM_BUF] = "";
        char chave_hex[128] = "";
        char nonce_hex[64] = "";
        for (;;) {
            pular_espacos(&p);
            if (*p == '}') {
                p++;
                break;
            }
            if (*p == ',') {
                p++;
                continue;
            }
            char *chave = ler_string(&p);
            pular_espacos(&p);
            if (*p == ':') {
                p++;
            }
            pular_espacos(&p);
            char *valor = ler_string(&p);
            if (chave != NULL && valor != NULL) {
                if (strcmp(chave, "id") == 0) {
                    snprintf(id, sizeof id, "%s", valor);
                } else if (strcmp(chave, "input_hex") == 0) {
                    snprintf(input_hex, sizeof input_hex, "%s", valor);
                } else if (strcmp(chave, "output_hex") == 0) {
                    snprintf(output_hex, sizeof output_hex, "%s", valor);
                } else if (strcmp(chave, "chave_hex") == 0) {
                    snprintf(chave_hex, sizeof chave_hex, "%s", valor);
                } else if (strcmp(chave, "nonce_hex") == 0) {
                    snprintf(nonce_hex, sizeof nonce_hex, "%s", valor);
                }
            }
            free(chave);
            free(valor);
        }
        entradas++;
        if (strcmp(id, "K4") == 0) {
            k4_visto = 1;
            verificar_k4(input_hex, output_hex);
        } else if (strcmp(id, "K9") == 0) {
            k9_visto = 1;
            verificar_k9(input_hex, output_hex, chave_hex, nonce_hex);
        }
    }

    CHECK_EQ_LONG(entradas, 9);
    CHECK(k4_visto);
    CHECK(k9_visto);

    free(json);
    ONYX_FIM();
}
