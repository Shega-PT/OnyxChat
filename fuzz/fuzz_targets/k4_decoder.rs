// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// k4_decoder — fuzz da camada K4 (transposição C++ + PKCS#7)
// ---------------------------------------------------------------------
// Fronteira: `k4_decifrar` recebe o output de K5 do transporte, ou
// seja bytes arbitrários (docs/pipeline.md §K4).
//
// Propriedades:
//   * **não-crash**: a camada C devolve código de erro, nunca aborta;
//   * **round-trip**: se a decifragem aceita (padding válido), re-cifrar
//     o plaintext tem de reproduzir exactamente a entrada — um decoder
//     que perdesse bytes seria indetetável nos testes de exemplos fixos.
// =====================================================================

#![no_main]

use libfuzzer_sys::fuzz_target;

use onyxchatd::ffi_c::{k4_cifrar, k4_decifrar};

fuzz_target!(|entrada: &[u8]| {
    let Ok(plaintext) = k4_decifrar(&entrada) else {
        // Rejeição (comprimento não múltiplo de 5, padding inválido) —
        // caminho previsto para ciphertext adulterado.
        return;
    };

    // Re-cifrar o output válido tem de devolver os bytes originais:
    // a permutação de colunas é bijetiva e o PKCS#7 é determinístico.
    let recifrado = k4_cifrar(&plaintext).expect("K4 cifra sempre qualquer bytes");
    assert_eq!(
        recifrado, entrada,
        "k4_cifrar(k4_decifrar(x)) não reproduz x"
    );

    // Idem pelo outro lado: decifrar o re-cifrado devolve o mesmo
    // plaintext (involução sobre o domínio aceite).
    let volta = k4_decifrar(&recifrado).expect("saída de cifragem é sempre decifrável");
    assert_eq!(volta, plaintext, "k4_decifrar(k4_cifrar(y)) não reproduz y");
});
