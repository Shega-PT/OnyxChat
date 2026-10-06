// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// check.h — micro-framework de asserções para os testes C/C++
// ---------------------------------------------------------------------
// Sem dependências externas (não usamos <cassert> porque o `assert` é
// removido com -DNDEBUG em build de release; aqui os testes falham
// sempre). `main` devolve o nº de falhas → código de saída CTest.
// =====================================================================

#ifndef ONYX_CHECK_H
#define ONYX_CHECK_H

#include <stdio.h>

/// Contador global de falhas (0 = teste ok).
static int onyx_falhas = 0;

/// Regista uma falha se `cond` for falso; continua a correr os restantes.
#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "FALHOU %s:%d: %s\n", __FILE__, __LINE__, #cond);  \
            onyx_falhas++;                                                     \
        }                                                                      \
    } while (0)

/// Igual a CHECK, mas compara dois valores longs com mensagem explícita.
#define CHECK_EQ_LONG(a, b)                                                    \
    do {                                                                       \
        long _a = (long)(a), _b = (long)(b);                                   \
        if (_a != _b) {                                                        \
            fprintf(stderr, "FALHOU %s:%d: %ld != %ld (%s)\n", __FILE__,       \
                    __LINE__, _a, _b, #a " == " #b);                           \
            onyx_falhas++;                                                     \
        }                                                                      \
    } while (0)

/// Fecha o teste devolvendo o código de saída para o CTest.
#define ONYX_FIM()                                                             \
    do {                                                                       \
        if (onyx_falhas == 0) {                                                \
            printf("OK\n");                                                    \
        }                                                                      \
        return onyx_falhas == 0 ? 0 : 1;                                       \
    } while (0)

#endif // ONYX_CHECK_H
