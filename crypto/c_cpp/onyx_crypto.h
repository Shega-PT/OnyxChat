// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// onyx_crypto.h — API C/C++ das camadas pesadas do pipeline OnyxChat
// ---------------------------------------------------------------------
// K4 → transposição (C++):  onyx_k4_encrypt / onyx_k4_decrypt
// K9 → libsodium (C):      onyx_k9_encrypt / onyx_k9_decrypt
//
// Contrato comum:
//  * sem alocação dentro da lib — o chamador dimensa o buffer de saída
//    (onyx_k4_encrypt_len devolve a capacidade necessária);
//  * devolvem ONYX_OK (0) ou código de erro negativo;
//  * ABI `extern "C"` estável para consumo via FFI (Rust) e CMake.
// =====================================================================

#ifndef ONYX_CRYPTO_H
#define ONYX_CRYPTO_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ---- Códigos de erro (negativos; 0 = sucesso) -----------------------
enum {
    ONYX_OK = 0,          // operação concluída
    ONYX_ERR_ARG = -1,    // ponteiro nulo ou argumento inválido
    ONYX_ERR_CAP = -2,    // buffer de saída demasiado pequeno
    ONYX_ERR_PAD = -3,    // padding PKCS#7 ausente/adulterado (K4)
    ONYX_ERR_TAMANHO = -4, // comprimento não múltiplo do bloco (K4/K9)
    ONYX_ERR_AUTH = -5,   // tag AEAD inválida — mensagem adulterada (K9)
    ONYX_ERR_CRYPTO = -6, // falha interna da biblioteca crypto (K9)
};

// ---- Limpeza de memória ---------------------------------------------

/// Apaga `len` bytes a partir de `pnt`, sem os deixar no lixo.
///
/// É `sodium_memzero` embrulhada. Existe por três razões:
///
///   * **Ponto único.** Toda a limpeza do projecto passa por aqui, e
///     um ponto único é auditável. `sodium_memzero` espalhado por três
///     ficheiros é três sítios onde ninguém vai procurar.
///   * **O cabeçalho do libsodium não entra no Rust.** O daemon fala
///     com a C por declarações escritas à mão em `ffi_c.rs`; expor uma
///     função nossa evita duplicar a `sodium.h` em Rust e evita que
///     alguém ligue ao `sodium_memzero` de uma biblioteca diferente.
///   * **Contrato explícito.** `sodium_memzero` garante que o
///     compilador não elimina a escrita; esta função expõe isso como
///     parte da API Onyx, e o `ONYX_ERR_*` de uma operação passa a
///     poder prometer que não deixou saída.
///
/// Aceita `pnt == NULL` ou `len == 0` sem efeito. Não é tolerância a
/// bugs: é o que torna seguro chamar `onyx_limpar(saida, escrita)`
/// num caminho de erro onde se sabe que nada foi escrito.
///
/// ## Porque `sodium_memzero` e não `memset(…, 0)`
///
/// Porque o compilador tem o direito de eliminar um `memset` cujo
/// resultado não é usado, e alguns compiladores eliminam-no mesmo
/// quando é: o buffer já não é lido, logo a escrita é morta. Numa
/// camada de cifra, essa optimização apaga a única protecção que
/// havia. `sodium_memzero` é opaco ao optimizador — é uma chamada
/// externa que o compilador não pode provar como irrelevante.
void onyx_limpar(void *pnt, size_t len);

// ---- K4: transposição de 5 colunas (C++) ----------------------------

/// Número de colunas da transposição (parâmetro fixo de K4).
#define ONYX_K4_COLUNAS 5

/// Capacidade de saída de `onyx_k4_encrypt` para `entrada_len` bytes
/// (o tamanho arredondado ao bloco seguinte — PKCS#7).
///
/// **Devolve 0** se `entrada_len == 0`: a entrada vazia é inválida e
/// esta função não tem canal de erro. O valor 0 nunca é uma capacidade
/// válida (o menor ciphertext tem 5 bytes), pelo que o chamador o pode
/// tratar como sinal de erro sem ambiguidade.
size_t onyx_k4_encrypt_len(size_t entrada_len);

/// Cifra K4: PKCS#7 (bloco 5) + escrita linha-a-linha/leitura por colunas.
/// Devolve ONYX_OK ou erro; em sucesso `*saida_len` = bytes escritos.
///
/// `ONYX_ERR_TAMANHO` se `entrada_len == 0`. Entrada vazia rejeitada
/// deliberadamente: o resultado seria 5 bytes de padding puro, que a
/// decifragem aceitaria como mensagem vazia. A decifragem rejeita
/// igualmente `entrada_len == 0` — a simetria é o que impede que um
/// envelope construído à mão contorne a validação do pipeline.
int onyx_k4_encrypt(const uint8_t *entrada, size_t entrada_len,
                    uint8_t *saida, size_t saida_cap, size_t *saida_len);

/// Decifra K4: leitura por colunas invertida + remoção do padding.
/// Devolve ONYX_ERR_TAMANHO se `entrada_len` não for múltiplo de 5,
/// ONYX_ERR_PAD se o padding for inválido/adulterado.
///
/// ## Contrato do buffer de saída em caso de erro
///
/// O padding PKCS#7 só pode ser validado **depois** de escrever a
/// des-transposição — o último byte do buffer des-transposto é o byte
/// de padding. Nos dois caminhos de `ONYX_ERR_PAD`, os bytes escritos
/// são limpos com [`onyx_limpar`] antes de regressar.
///
/// O mesmo vale para `ONYX_ERR_ARG` e `ONYX_ERR_TAMANHO`: nesses
/// pontos nada foi escrito, logo não há o que limpar. `ONYX_ERR_CAP`
/// também precede qualquer escrita.
///
/// A regra geral: **devolveu erro, não há saída utilizável**. Um buffer
/// que pareça conter uma mensagem des-transposta é pior do than um
/// buffer vazio, porque convida a ser lido.
int onyx_k4_decrypt(const uint8_t *entrada, size_t entrada_len,
                    uint8_t *saida, size_t saida_cap, size_t *saida_len);

// ---- K9: ChaCha20-Poly1305 via libsodium (C) ------------------------

/// Tamanho da chave K9 (bytes).
#define ONYX_K9_KEY_LEN 32
/// Tamanho do nonce K9 (bytes).
#define ONYX_K9_NONCE_LEN 12
/// Tamanho da tag AEAD e overhead do ciphertext (bytes).
#define ONYX_K9_TAG_LEN 16

/// Cifra K9 com ChaCha20-Poly1305 (libsodium).
/// `saida` tem de ter `entrada_len + ONYX_K9_TAG_LEN` bytes.
int onyx_k9_encrypt(const uint8_t *entrada, size_t entrada_len,
                    const uint8_t chave[ONYX_K9_KEY_LEN],
                    const uint8_t nonce[ONYX_K9_NONCE_LEN],
                    uint8_t *saida, size_t *saida_len);

/// Decifra/autentica K9. Exige `entrada_len >= ONYX_K9_TAG_LEN`.
/// Devolve ONYX_ERR_AUTH se a tag não conferir.
int onyx_k9_decrypt(const uint8_t *entrada, size_t entrada_len,
                    const uint8_t chave[ONYX_K9_KEY_LEN],
                    const uint8_t nonce[ONYX_K9_NONCE_LEN],
                    uint8_t *saida, size_t *saida_len);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // ONYX_CRYPTO_H
