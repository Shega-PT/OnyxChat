// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// build.rs — compilação das camadas C/C++ e embebimento do `ffmpeg`
// ---------------------------------------------------------------------
// Faz duas coisas, ambas pelo mesmo motivo: **tirar o daemon da
// dependência do exterior**.
//
// 1. Compila `k9_chacha.c` (K9, libsodium) e `transposition.cpp` (K4)
//    de `crypto/c_cpp/` e liga o `libsodium.a` vendored de forma
//    estática — o daemon não depende de libsodium no sistema em runtime.
//    O mesmo código é também construído pelo CMake para os testes
//    C/C++; aqui serve o binário final do daemon via FFI.
//
// 2. Embute o `ffmpeg` de `tools/ffmpeg/cache/`, comprimido com xz, em
//    `OUT_DIR/ffmpeg.xz`, que `media/ffmpeg_embutido.rs` inclui com
//    `include_bytes!`. Ver `docs/media.md` §Dependências.
//
// ## Porquê que o ficheiro é escrito **sempre**
//
// Porque `include_bytes!` resolve o caminho em tempo de compilação, e um
// caminho que não existe é um erro de compilação. A máquina de quem
// escreve código não tem o pacote — descarregá-lo a cada `cargo build`
// seria absurdo — e mesmo a máquina de release o tem só depois de
// `tools/ffmpeg/fetch.sh`.
//
// A saída é, portanto, um ficheiro de **zero bytes** quando não há
// artefacto, e `media/ffmpeg_embutido::disponivel()` pergunta pelo
// comprimento. A ausência passa a ser uma condição de execução e não
// um erro de compilação, que é o que torna o modo degradado de
// `docs/media.md` possível: o daemon arranca, o texto funciona, e
// `MEDIA_INICIAR` devolve `0x1F DependenciaAusente`.
//
// ## Porquê não descompactar aqui
//
// Porque `build.rs` é executado no *build* e o executável é executado
// runtime, e as duas coisas precisam de formatos diferentes: este
// produz `.xz` para o binário ficar 21 MB em vez de 80, e o
// descompressor em Rust puro (`lzma-rs`, zero dependências
// transitivas) é o que traz o que o `xz` comprime. Se o build
// descompactasse, o binário embutiria 80 MB e `lzma-rs` não teria
//razão de existir.
// =====================================================================

use std::fs;
use std::path::{Path, PathBuf};

/// Lê a entrada `ffmpeg = <versão> | <url> | <sha256> | <versão>`.
///
/// Devolve a **versão** declarada. `None` quando não há entrada — que é
/// um estado **válido**, não um erro: é o modo degradado de
/// `docs/media.md`.
///
/// ## Porque é a versão e não o caminho
///
/// O que se embebe é `cache/ffmpeg.xz`, que o `tools/ffmpeg/build.sh`
/// produz a partir da fonte que o `fetch.sh` traz. O caminho é fixo e
/// vive aqui uma vez; o que muda é a versão, e é a versão que interessa
/// ao diagnóstico — um `MEDIA_ESTADO` que diga qual `ffmpeg` está
/// embebido é o que permite a alguém reportar um problema sem
/// primeiro ter de adivinhar.
///
/// A fonte é o que se fixa por sumário. O executável é o produto de
/// uma receita que está no mesmo repositório — e é por isso que este
/// `build.rs` **confia no `build.sh`** de uma maneira que antes não
/// confiava em ninguém: confia que `build.sh` não deixou um binário GPL
/// em `cache/`. E essa confiança é justificada porque `build.sh`
/// **apaga** o executável se `ffmpeg -buildconf` não trouxer
/// `--disable-gpl`. A garantia está lá, e é do próprio binário.
fn versao_no_manifesto(raiz: &Path) -> Option<String> {
    let manifesto = fs::read_to_string(raiz.join("../../tools/ffmpeg/versoes.txt")).ok()?;

    for linha in manifesto.lines() {
        let linha = linha.trim();
        if linha.is_empty() || linha.starts_with('#') {
            continue;
        }
        let (chave, resto) = linha.split_once('=')?;
        if chave.trim() != "ffmpeg" {
            continue;
        }
        let campos: Vec<&str> = resto.split('|').map(str::trim).collect();
        if campos.is_empty() {
            continue;
        }
        return Some(campos[0].to_string());
    }
    None
}

/// Embete o executável que `tools/ffmpeg/build.sh` compilou, e escreve
/// as variáveis de ambiente que `media/ffmpeg_embutido.rs` lê.
///
/// O ficheiro em `OUT_DIR` é escrito **sempre** — vazio quando não há
/// executável. É o que permite que o `include_bytes!` não falhe numa
/// máquina sem `ffmpeg`, e é o que torna o modo degradado de
/// `docs/media.md` uma condição de execução e não um erro de
/// compilação.
fn embeber_ffmpeg(raiz: &Path) {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let destino = out.join("ffmpeg.xz");

    let cache = raiz.join("../../tools/ffmpeg/cache");
    let executavel = cache.join("ffmpeg.xz");
    let encontrado = executavel.is_file();

    if !encontrado {
        fs::write(&destino, b"").expect("escreve o ffmpeg.xz vazio");
        println!(
            "cargo:warning=onyxchat: sem ffmpeg embebido — corra tools/ffmpeg/fetch.sh \
             e tools/ffmpeg/build.sh. O daemon arranca em modo degradado (texto \
             intacto) e MEDIA_INICIAR devolve 0x1F DependenciaAusente. \
             Ver docs/media.md §Dependências."
        );
    } else {
        // Já vem comprimido: o `build.sh` escreve o `.xz` com
        // `xz -9e`, e descomprimir e recomprimir aqui dava o mesmo
        // resultado por duas vezes o trabalho.
        let bruto = fs::read(&executavel).expect("lê o executável do cache");
        fs::write(&destino, &bruto).expect("escreve o ffmpeg.xz");
        println!(
            "cargo:warning=onyxchat: ffmpeg embebido, {} bytes comprimidos. Ver \
             tools/ffmpeg/versoes.txt para a versão e a licença.",
            bruto.len()
        );
    }

    // A versão descreve o **executável embebido**, e não o crate. Sem
    // executável não há versão para descrever, e uma string que
    // mentisse — «0.1.0, licença nenhuma» — apareceria num
    // `MEDIA_ESTADO` e faria um utilizador procurar uma versão que não
    // existe.
    //
    // A licença é constante e não é lida do manifesto: quem a garante é
    // o `build.sh`, que não deixa um executável no cache sem que
    // `ffmpeg -buildconf` diga `--disable-gpl`. Ver `deny.toml` e
    // `THIRD-PARTY.md` §Componentes compilados de fonte.
    let versao = match (versao_no_manifesto(raiz), encontrado) {
        (Some(v), true) => format!("ffmpeg {v} (LGPL-2.1-or-later, --disable-gpl)"),
        _ => String::new(),
    };

    println!("cargo:rerun-if-changed=../../tools/ffmpeg/cache/ffmpeg.xz");
    println!("cargo:rerun-if-changed=../../tools/ffmpeg/versoes.txt");
    println!("cargo:rustc-env=ONYX_FFMPEG_VERSAO={versao}");
    println!(
        "cargo:rustc-env=ONYX_FFMPEG_ARTEFACTO={}",
        executavel.display()
    );
}

fn main() {
    // Raiz do crate (network/daemon_rust) — caminho independente do cwd.
    let raiz = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));

    embeber_ffmpeg(&raiz);

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
