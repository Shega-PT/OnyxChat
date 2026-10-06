// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// transposition.cpp — camada K4 do pipeline: transposição de 5 colunas
// ---------------------------------------------------------------------
// Especificação (docs/pipeline.md):
//   1. PKCS#7 com bloco de 5: acrescenta sempre 1..5 bytes (mesmo com
//      comprimento já alinhado — bloco inteiro).
//   2. Escrita linha-a-linha (5 colunas), leitura coluna-a-coluna.
//   3. Decifragem inverte a leitura e só depois remove o padding —
//      nunca avança para K3⁻¹ com bytes de padding a mais.
//
// Sem alocação dinâmica: o output é calculado diretamente por índice
// (out[c·linhas + r] = in[r·5 + c]), o que torna a função reentrante e
// segura para uso via FFI.
// =====================================================================

#include "onyx_crypto.h"

#include <cstddef>

namespace {

/// Comprimento depois do PKCS#7 (sempre múltiplo de 5 e > entrada).
constexpr size_t arredondar_pkcs7(size_t n) {
    // bloco completo extra quando já está alinhado (PKCS#7 puro)
    return (n / ONYX_K4_COLUNAS + 1) * ONYX_K4_COLUNAS;
}

} // namespace

extern "C" size_t onyx_k4_encrypt_len(size_t entrada_len) {
    // Entrada vazia é inválida (ver `onyx_k4_encrypt`). O valor de
    // retorno não tem canal de erro, então 0 é o sinal: nunca é um
    // comprimento válido, porque o menor ciphertext tem 5 bytes.
    if (entrada_len == 0) {
        return 0;
    }
    return arredondar_pkcs7(entrada_len);
}

extern "C" int onyx_k4_encrypt(const uint8_t *entrada, size_t entrada_len,
                               uint8_t *saida, size_t saida_cap,
                               size_t *saida_len) {
    // Validação de argumentos: nada de dereferences em ponteiros nulos.
    if (saida == nullptr || saida_len == nullptr ||
        (entrada == nullptr && entrada_len > 0)) {
        return ONYX_ERR_ARG;
    }
    // Entrada vazia não se transpõe: o resultado seria um bloco de 5
    // bytes de padding puro, que a decifragem aceitaria como uma
    // mensagem vazia. A mesma entrada tem de ser rejeitada pelos dois
    // lados, senão um envelope construído à mão pode contornar a
    // validação do pipeline.
    if (entrada_len == 0) {
        return ONYX_ERR_TAMANHO;
    }
    const size_t total = arredondar_pkcs7(entrada_len);
    if (saida_cap < total) {
        return ONYX_ERR_CAP;
    }
    const size_t linhas = total / ONYX_K4_COLUNAS;
    const auto pad = static_cast<uint8_t>(total - entrada_len);

    // Escrita por colunas: para cada coluna c, percorre todas as linhas.
    for (size_t c = 0; c < ONYX_K4_COLUNAS; ++c) {
        for (size_t l = 0; l < linhas; ++l) {
            const size_t idx = l * ONYX_K4_COLUNAS + c;
            // bytes reais vêm da entrada; os de padding são o valor PKCS#7
            saida[c * linhas + l] = (idx < entrada_len) ? entrada[idx] : pad;
        }
    }
    *saida_len = total;
    return ONYX_OK;
}

extern "C" int onyx_k4_decrypt(const uint8_t *entrada, size_t entrada_len,
                               uint8_t *saida, size_t saida_cap,
                               size_t *saida_len) {
    if (saida == nullptr || saida_len == nullptr ||
        (entrada == nullptr && entrada_len > 0)) {
        return ONYX_ERR_ARG;
    }
    // O output cifrado é sempre múltiplo de 5 e nunca vazio.
    if (entrada_len == 0 || entrada_len % ONYX_K4_COLUNAS != 0) {
        return ONYX_ERR_TAMANHO;
    }
    if (saida_cap < entrada_len) {
        return ONYX_ERR_CAP;
    }
    const size_t linhas = entrada_len / ONYX_K4_COLUNAS;

    // Reconstrói a ordem original: saída[l·5 + c] = entrada[c·linhas + l].
    for (size_t l = 0; l < linhas; ++l) {
        for (size_t c = 0; c < ONYX_K4_COLUNAS; ++c) {
            saida[l * ONYX_K4_COLUNAS + c] = entrada[c * linhas + l];
        }
    }

    // Remove o padding PKCS#7 validando cada byte.
    //
    // A validação vem **depois** da escrita e não antes porque não pode
    // vir antes: o último byte do buffer des-transposto é justamente o
    // byte de padding, e é só de lá que se pode saber quanto vale. A
    // consequência é que, ao rejeitar, a buffer já tem os dados dentro —
    // e a função tem de os levar consigo.
    //
    // Os dois `return ONYX_ERR_PAD` daqui em baixo limpam antes de sair.
    const uint8_t pad = saida[entrada_len - 1];
    if (pad == 0 || pad > ONYX_K4_COLUNAS ||
        static_cast<size_t>(pad) > entrada_len) {
        onyx_limpar(saida, entrada_len);
        return ONYX_ERR_PAD;
    }
    for (size_t i = entrada_len - pad; i < entrada_len; ++i) {
        if (saida[i] != pad) {
            onyx_limpar(saida, entrada_len);
            return ONYX_ERR_PAD;
        }
    }
    *saida_len = entrada_len - pad;
    return ONYX_OK;
}
