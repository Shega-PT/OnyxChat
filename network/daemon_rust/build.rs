// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// build.rs — compilação das camadas C/C++ do pipeline dentro do Cargo
// ---------------------------------------------------------------------
// Compila `k9_chacha.c` (K9, libsodium) e `transposition.cpp` (K4) de
// `crypto/c_cpp/` e liga o `libsodium.a` vendored de forma estática —
// o daemon não depende de libsodium no sistema em runtime.
//
// O mesmo código é também construído pelo CMake (`crypto/c_cpp/`) para
// os testes C/C++; aqui serve o binário final do daemon via FFI.
// =====================================================================

use std::path::PathBuf;

fn main() {
    // Raiz do crate (network/daemon_rust) — caminho independente do cwd.
    let raiz = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let c_cpp = raiz.join("../../crypto/c_cpp");
    let sodium = c_cpp.join("vendor/libsodium");

    // Rebuild automático quando qualquer fonte C/C++/header muda.
    for fico in [
        c_cpp.join("k9_chacha.c"),
        c_cpp.join("transposition.cpp"),
        c_cpp.join("onyx_crypto.h"),
        sodium.join("include/sodium.h"),
        sodium.join("lib/libsodium.a"),
    ] {
        println!("cargo:rerun-if-changed={}", fico.display());
    }

    // Compila as duas camadas numa única biblioteca estática interna.
    // Sem `-std` explícito: o g++/gcc padrão (gnu++17/gnu11) satisfaz
    // os requisitos, evitando flags aplicadas a linguagens erradas.
    cc::Build::new()
        .cpp(true)
        .file(c_cpp.join("k9_chacha.c"))
        .file(c_cpp.join("transposition.cpp"))
        .include(&c_cpp)
        .include(sodium.join("include"))
        .compile("onyx_pipeline_c");

    // Liga o libsodium vendored (estático) + libm (requisito do sodium).
    println!(
        "cargo:rustc-link-search=native={}",
        sodium.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=sodium");
    println!("cargo:rustc-link-lib=m");
}
