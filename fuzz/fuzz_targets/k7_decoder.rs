// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// k7_decoder — fuzz da camada K7 (Hill 2×2 em Z/256Z)
// ---------------------------------------------------------------------
// Fronteira: `hill::decifrar` recebe o output de K6 do transporte —
// bytes arbitrários (docs/pipeline.md §K7).
//
// Propriedades:
//   * **não-crash**: comprimento ímpar e padding adulterado são erros
//     tipados, nunca panics ou sub-índices;
//   * **round-trip**: se a decifragem aceita, re-cifrar reproduz a
//     entrada — a matriz inversa [[29,85],[142,171]] tem de ser mesmo
//     a inversa em Z/256Z para *todos* os blocos, não só para os dos
//     vetores oficiais.
// =====================================================================

#![no_main]

use libfuzzer_sys::fuzz_target;

use crypto_core::hill;

fuzz_target!(|entrada: &[u8]| {
    let Ok(plaintext) = hill::decifrar(&entrada) else {
        // Rejeição: bloco incompleto (comprimento ímpar) ou PKCS#7
        // inválido — o caminho previsto para ciphertext adulterado.
        return;
    };

    // Re-cifrar o output válido tem de devolver os bytes originais:
    // a matriz é bijetiva (det = 9 invertível mod 256) e o padding é
    // função determinística do comprimento.
    let recifrado = hill::cifrar(&plaintext);
    assert_eq!(
        recifrado, entrada,
        "hill::cifrar(hill::decifrar(x)) não reproduz x"
    );

    // Involução no domínio aceite: decifrar o re-cifrado devolve o
    // mesmo plaintext.
    let volta = hill::decifrar(&recifrado).expect("saída de cifragem é sempre decifrável");
    assert_eq!(volta, plaintext, "hill::decifrar(hill::cifrar(y)) não reproduz y");
});
