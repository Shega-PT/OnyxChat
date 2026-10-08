// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// gerar_vetores.rs — gera os vetores oficiais do OnyxChat
// ---------------------------------------------------------------------
// Fonte única de verdade: os JSON escritos em `tests/vectors/`. Este
// exemplo também escreve `docs/test_vectors.md` como forma legível dos
// mesmos dados (Resumo §25 e §47).
//
// Saídas:
//   tests/vectors/camadas.json   — KATs por camada (input/key/params/
//                                  expected; §25)
//   tests/vectors/pipeline.json  — vetores fim-a-fim K1..K9 + envelope
//                                  (§47)
//   tests/vectors/handshake.json — FRIEND_REQUEST/ACCEPT/REJECT
//                                  (docs/testing.md: handshake)
//   docs/test_vectors.md         — forma legível dos anteriores
//
// Consumidores:
//   Rust   → network/daemon_rust/tests/vetores.rs
//   Python → tests/test_vectors.py
//   C/C++  → crypto/c_cpp/tests/test_vectors.c (K4 + K9)
//   Lua    → exercitado pelo Rust via `mlua` (lua_camadas)
//
// Uso: cargo run -p onyxchatd --example gerar_vetores
// (ferramenta de desenvolvimento — escreve ficheiros do repositório)
// =====================================================================

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use crypto_core::signer::chave_publica_a_partir_da_seed;
use onyxchatd::anti_replay::RegistoNonce1;
use onyxchatd::pipeline::{self, Chaves};
use onyxchatd::{ffi_c, handshake, lua_camadas};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Hex minúsculo sem separadores (formato de todos os vetores).
fn hex(b: &[u8]) -> String {
    b.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Array de 32 bytes com `offset, offset+1, …` (chaves de fixture).
fn chave(offset: u8) -> [u8; 32] {
    let mut k = [0u8; 32];
    for (i, octeto) in k.iter_mut().enumerate() {
        *octeto = offset.wrapping_add(i as u8);
    }
    k
}

/// Nonce de 12 bytes com `base, base+1, …` (nonces de fixture).
fn nonce(base: u8) -> [u8; 12] {
    let mut n = [0u8; 12];
    for (i, octeto) in n.iter_mut().enumerate() {
        *octeto = base.wrapping_add(i as u8);
    }
    n
}

/// Nonce de 16 bytes do handshake com `base, base+1, …`.
fn nonce16(base: u8) -> [u8; handshake::TAM_NONCE] {
    let mut n = [0u8; handshake::TAM_NONCE];
    for (i, octeto) in n.iter_mut().enumerate() {
        *octeto = base.wrapping_add(i as u8);
    }
    n
}

/// Raiz do repositório (network/daemon_rust → sobe dois níveis).
fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Entrada de um KAT por camada (§25), com roundtrip obrigatório.
struct Kat<'a> {
    id: &'a str,
    nome: &'a str,
    implementacao: &'a str,
    parametros: &'a str,
    entrada: &'a [u8],
    saida: Vec<u8>,
    chave_hex: Option<String>,
    nonce_hex: Option<String>,
}

fn kat(k: Kat<'_>) -> Value {
    let mut e = json!({
        "id": k.id,
        "nome": k.nome,
        "implementacao": k.implementacao,
        "parametros": k.parametros,
        "input_hex": hex(k.entrada),
        "output_hex": hex(&k.saida),
    });
    if let Some(c) = k.chave_hex {
        e["chave_hex"] = json!(c);
    }
    if let Some(n) = k.nonce_hex {
        e["nonce_hex"] = json!(n);
    }
    e
}

// ---------------------------------------------------------------------
// §25 — uma entrada por camada
// ---------------------------------------------------------------------

fn gerar_camadas() -> Vec<Value> {
    let mut v = Vec::new();

    // --- K1 — ChaCha20-Poly1305 (Rust) ---
    let entrada = b"vetor K1 do OnyxChat";
    let k1 = chave(0x00);
    let n1 = nonce(0x00);
    let saida = crypto_core::aead_chacha::encrypt(entrada, &k1, &n1).expect("K1 cifra");
    let volta = crypto_core::aead_chacha::decrypt(&saida, &k1, &n1).expect("K1 decifra");
    assert_eq!(volta, entrada, "K1 roundtrip");
    v.push(kat(Kat {
        id: "K1",
        nome: "ChaCha20-Poly1305 (chave do par)",
        implementacao: "Rust (crypto_core)",
        parametros: "chave=000102..1f, nonce=000102030405060708090a0b",
        entrada,
        saida,
        chave_hex: Some(hex(&k1)),
        nonce_hex: Some(hex(&n1)),
    }));

    // --- K2 — deslocamento +3 (Lua) ---
    let entrada = b"Vetor K2: ABC xyz 012";
    let saida =
        lua_camadas::deslocamento(entrada, pipeline::DESLOCAMENTO_K2).expect("K2 cifra");
    let volta = lua_camadas::deslocamento_inverso(&saida, pipeline::DESLOCAMENTO_K2)
        .expect("K2 decifra");
    assert_eq!(volta, entrada, "K2 roundtrip");
    v.push(kat(Kat {
        id: "K2",
        nome: "Cesariana (adaptada)",
        implementacao: "Lua (crypto/python_lua/lua/deslocamento.lua)",
        parametros: "deslocamento=3",
        entrada,
        saida,
        chave_hex: None,
        nonce_hex: None,
    }));

    // --- K3 — Vigenère (Lua) ---
    let entrada = b"mensagem de teste K3";
    let saida = lua_camadas::vigenere(entrada, pipeline::CHAVE_K3).expect("K3 cifra");
    let volta = lua_camadas::vigenere_inverso(&saida, pipeline::CHAVE_K3).expect("K3 decifra");
    assert_eq!(volta, entrada, "K3 roundtrip");
    v.push(kat(Kat {
        id: "K3",
        nome: "Vigenère",
        implementacao: "Lua (crypto/python_lua/lua/vigenere.lua)",
        parametros: "chave=\"VIGENERE\"",
        entrada,
        saida,
        chave_hex: Some(hex(pipeline::CHAVE_K3)),
        nonce_hex: None,
    }));

    // --- K4 — Transposição 5 colunas (C++) ---
    let entrada = b"camada K4 de transposicao";
    let saida = ffi_c::k4_cifrar(entrada).expect("K4 cifra");
    let volta = ffi_c::k4_decifrar(&saida).expect("K4 decifra");
    assert_eq!(volta, entrada, "K4 roundtrip");
    v.push(kat(Kat {
        id: "K4",
        nome: "Transposição de 5 colunas (PKCS#7 bloco 5)",
        implementacao: "C++ (crypto/c_cpp/transposition.cpp)",
        parametros: "colunas=5, padding=PKCS#7",
        entrada,
        saida,
        chave_hex: None,
        nonce_hex: None,
    }));

    // --- K5 — AES-256-GCM (Rust) ---
    let entrada = b"camada K5 aes gcm";
    let k5 = chave(0x20);
    let n5 = nonce(0x10);
    let saida = crypto_core::encrypt_k5(entrada, &k5, &n5).expect("K5 cifra");
    let volta = crypto_core::decrypt_k5(&saida, &k5, &n5).expect("K5 decifra");
    assert_eq!(volta, entrada, "K5 roundtrip");
    v.push(kat(Kat {
        id: "K5",
        nome: "AES-256-GCM (chave do remetente)",
        implementacao: "Rust (crypto_core)",
        parametros: "chave=202122..3f, nonce=101112131415161718191a1b",
        entrada,
        saida,
        chave_hex: Some(hex(&k5)),
        nonce_hex: Some(hex(&n5)),
    }));

    // --- K6 — Playfair adaptado (Lua) ---
    let entrada = b"vetor K6 playfair";
    let saida = lua_camadas::playfair(entrada, pipeline::CHAVE_K6).expect("K6 cifra");
    let volta = lua_camadas::playfair_inverso(&saida, pipeline::CHAVE_K6).expect("K6 decifra");
    assert_eq!(volta, entrada, "K6 roundtrip");
    v.push(kat(Kat {
        id: "K6",
        nome: "Playfair adaptado (XOR periódico)",
        implementacao: "Lua (crypto/python_lua/lua/playfair.lua)",
        parametros: "chave=\"PLAYFAIR\"",
        entrada,
        saida,
        chave_hex: Some(hex(pipeline::CHAVE_K6)),
        nonce_hex: None,
    }));

    // --- K7 — Hill 2×2 (Rust) — âncora com o valor documentado ---
    let entrada = b"AB";
    let saida = crypto_core::hill::cifrar(entrada);
    // Âncora normativa: `docs/pipeline.md` §K7 documenta "AB" → 89cc0c0e.
    assert_eq!(hex(&saida), "89cc0c0e", "K7 = âncora documentada");
    let volta = crypto_core::hill::decifrar(&saida).expect("K7 decifra");
    assert_eq!(volta, entrada, "K7 roundtrip");
    v.push(kat(Kat {
        id: "K7",
        nome: "Hill 2×2 em Z/256Z",
        implementacao: "Rust (crypto_core)",
        parametros: "matriz=[[3,3],[2,5]], modulo=256, bloco=2, padding=PKCS#7",
        entrada,
        saida,
        chave_hex: None,
        nonce_hex: None,
    }));

    // --- K8 — deslocamento +7 (Lua) ---
    //
    // Mesma função que a K2, com outro deslocamento. O `implementacao`
    // aponta para o mesmo ficheiro das duas, de propósito: é essa a
    // informação que o vector carrega sobre onde a camada vive.
    let entrada = b"deslocamento K8 +7";
    let saida =
        lua_camadas::deslocamento(entrada, pipeline::DESLOCAMENTO_K8).expect("K8 cifra");
    let volta = lua_camadas::deslocamento_inverso(&saida, pipeline::DESLOCAMENTO_K8)
        .expect("K8 decifra");
    assert_eq!(volta, entrada, "K8 roundtrip");
    v.push(kat(Kat {
        id: "K8",
        nome: "Substituição bijetiva +7 mod 256",
        implementacao: "Lua (crypto/python_lua/lua/deslocamento.lua)",
        parametros: "deslocamento=7",
        entrada,
        saida,
        chave_hex: None,
        nonce_hex: None,
    }));

    // --- K9 — ChaCha20-Poly1305 via libsodium (C) ---
    // Mesmo vector que `crypto/c_cpp/tests/test_k9.c` (âncora com o
    // espelho Rust e com PyNaCl).
    let entrada = "Olá mundo!".as_bytes();
    let k9 = chave(0x00);
    let n9 = nonce(0x00);
    let saida = ffi_c::k9_cifrar(entrada, &k9, &n9).expect("K9 cifra");
    let volta = ffi_c::k9_decifrar(&saida, &k9, &n9).expect("K9 decifra");
    assert_eq!(volta, entrada, "K9 roundtrip");
    v.push(kat(Kat {
        id: "K9",
        nome: "ChaCha20-Poly1305 (chave do receptor)",
        implementacao: "C (crypto/c_cpp/k9_chacha.c, libsodium)",
        parametros: "chave=000102..1f, nonce=000102030405060708090a0b",
        entrada,
        saida,
        chave_hex: Some(hex(&k9)),
        nonce_hex: Some(hex(&n9)),
    }));

    v
}

// ---------------------------------------------------------------------
// §47 — vetores fim-a-fim (K1..K9 + Final)
// ---------------------------------------------------------------------

fn gerar_pipeline() -> Vec<Value> {
    let chaves = Chaves {
        k1: chave(0x00),
        k5: chave(0x20),
        k9: chave(0x40),
    };
    let seed = chave(0x60);
    let publica = chave_publica_a_partir_da_seed(&seed);

    // Os três casos cobrem o que é interessante para a paridade entre
    // linguagens: texto corrente, texto multibyte (UTF-8 com
    // acentuação, onde os erros de codificação aparecem) e o **limite
    // mínimo** do plaintext.
    //
    // O vector 003 foi durante muito tempo o caso «plaintext vazio».
    // Deixou de ser válido quando o pipeline adoptou `MIN_PLAINTEXT = 1`
    // (F2.2) — e trocá-lo pelo mínimo de 1 byte é uma melhoria: o limite
    // inferior passa a ser verificado por um vector oficial, em vez de
    // só por um teste unitário.
    let casos: [(&str, &str, u8); 3] = [
        (
            "001",
            "OnyxChat vector 001 — paridade entre linguagens.",
            0x00,
        ),
        ("002", "Mensagem com acentuação: çãé", 0x30),
        (
            "003",
            // 1 byte: o menor plaintext que o pipeline aceita.
            "x",
            0x60,
        ),
    ];

    let mut vetores = Vec::new();
    for (id, texto, base) in casos {
        let n1 = nonce(base);
        let n5 = nonce(base.wrapping_add(0x10));
        let n9 = nonce(base.wrapping_add(0x20));
        let (envelope, c) =
            pipeline::cifrar_com_nonces(texto.as_bytes(), &chaves, &seed, n1, n5, n9)
                .unwrap_or_else(|e| panic!("vector {id} cifra: {e}"));

        // Roundtrip completo (assinatura + anti-replay incluídos).
        let mut registo = RegistoNonce1::novo();
        let volta = pipeline::decifrar(&envelope, &chaves, &publica, &mut registo)
            .unwrap_or_else(|e| panic!("vector {id} decifra: {e}"));
        assert_eq!(volta, texto, "vector {id} roundtrip");

        vetores.push(json!({
            "id": id,
            "plaintext_utf8": texto,
            "plaintext_hex": hex(texto.as_bytes()),
            "chave_k1_hex": hex(&chaves.k1),
            "chave_k5_hex": hex(&chaves.k5),
            "chave_k9_hex": hex(&chaves.k9),
            "seed_hex": hex(&seed),
            "publica_hex": hex(&publica),
            "nonce1_hex": hex(&n1),
            "nonce5_hex": hex(&n5),
            "nonce9_hex": hex(&n9),
            "c1_hex": hex(&c.c1),
            "c2_hex": hex(&c.c2),
            "c3_hex": hex(&c.c3),
            "c4_hex": hex(&c.c4),
            "c5_hex": hex(&c.c5),
            "c6_hex": hex(&c.c6),
            "c7_hex": hex(&c.c7),
            "c8_hex": hex(&c.c8),
            "c9_hex": hex(&c.c9),
            "envelope_hex": hex(&envelope),
        }));
    }
    vetores
}

// ---------------------------------------------------------------------
// Handshake — FRIEND_REQUEST (209) / ACCEPT (177) / REJECT (81)
// ---------------------------------------------------------------------

fn gerar_handshake() -> Vec<Value> {
    let seed_a = chave(0x60);
    let seed_b = chave(0x80);
    let publica_a = chave_publica_a_partir_da_seed(&seed_a);
    let publica_b = chave_publica_a_partir_da_seed(&seed_b);

    // Camadas de A (viajam no pedido) e de B (viajam no aceite).
    let k1 = chave(0x00);
    let k5 = chave(0x20);
    let k9 = chave(0x40);
    let k5_b = chave(0xA0);
    let k9_b = chave(0xC0);
    let nonce = nonce16(0x40);

    let (pedido, request) = handshake::montar_pedido_com(&seed_a, nonce, k1, k5, k9);
    assert_eq!(request.len(), handshake::TAM_CORPO_PEDIDO, "pedido 209");
    let (amizade_b, aceite) = handshake::aceitar_pedido_com(&seed_b, &pedido, k5_b, k9_b);
    assert_eq!(aceite.len(), handshake::TAM_CORPO_ACEITE, "aceite 177");
    let recusa = handshake::montar_recusa(&seed_b, &nonce);
    assert_eq!(recusa.len(), handshake::TAM_CORPO_RECUSA, "recusa 81");

    // Validações que os consumidores têm de reproduzir (roundtrip).
    let pedido_v = handshake::validar_pedido(&request).expect("valida pedido");
    assert_eq!(pedido_v, pedido, "pedido idêntico");
    let aceite_v = handshake::validar_aceite(&aceite).expect("valida aceite");
    assert_eq!(aceite_v.publica, publica_b, "aceite é de B");
    assert_eq!(aceite_v.nonce, nonce, "aceite ecoa o nonce");
    let amizade_a = handshake::confirmar_aceite(&seed_a, &request, &aceite).expect("A confirma");
    assert_eq!(amizade_a.publica, publica_b);
    assert_eq!(amizade_a.k1, k1);
    assert_eq!(amizade_a.k5_proprio, k5);
    assert_eq!(amizade_a.k9_proprio, k9);
    assert_eq!(amizade_a.k5_par, k5_b);
    assert_eq!(amizade_a.k9_par, k9_b);
    assert_eq!(amizade_b.publica, publica_a, "B guarda A");
    assert_eq!(amizade_b.k5_proprio, k5_b);
    assert_eq!(amizade_b.k9_proprio, k9_b);
    handshake::validar_recusa_com(&recusa, &nonce, &publica_b).expect("recusa válida");

    vec![json!({
        "id": "001",
        "seed_a_hex": hex(&seed_a),
        "publica_a_hex": hex(&publica_a),
        "seed_b_hex": hex(&seed_b),
        "publica_b_hex": hex(&publica_b),
        "nonce_hex": hex(&nonce),
        "k1_hex": hex(&k1),
        "k5_hex": hex(&k5),
        "k9_hex": hex(&k9),
        "k5_b_hex": hex(&k5_b),
        "k9_b_hex": hex(&k9_b),
        "request_hex": hex(&request),
        "accept_hex": hex(&aceite),
        "reject_hex": hex(&recusa),
    })]
}

// ---------------------------------------------------------------------
// docs/test_vectors.md — forma legível (gerado dos JSON)
// ---------------------------------------------------------------------


// ---------------------------------------------------------------------
// main
// ---------------------------------------------------------------------

fn main() {
    let camadas = gerar_camadas();
    let vetores = gerar_pipeline();
    let hs = gerar_handshake();
    let raiz_projeto = raiz();

    let camadas_json = serde_json::to_string_pretty(&json!({
        "versao": "1",
        "descricao": "KATs por camada (input/key/parameters/expected) — Resumo §25",
        "camadas": camadas,
    }))
    .expect("JSON camadas");
    let pipeline_json = serde_json::to_string_pretty(&json!({
        "versao": "1",
        "descricao": "Vetores fim-a-fim K1..K9 + envelope — Resumo §47",
        "vetores": vetores,
    }))
    .expect("JSON pipeline");
    let handshake_json = serde_json::to_string_pretty(&json!({
        "versao": "1",
        "descricao": "FRIEND_REQUEST (209) / FRIEND_ACCEPT (177) / FRIEND_REJECT (81)",
        "vetores": hs,
    }))
    .expect("JSON handshake");
    // Os vectores de identidade são **lidos**, não gerados: a fórmula vive
    // em `messenger/identidade.py`. Se este exemplo os produzisse, seria uma
    // segunda implementação da fórmula, a competir com a primeira — e os
    // vectores deixavam de provar o que existem para provar.
    let identidade_json = fs::read_to_string(raiz_projeto.join("tests/vectors/identidade.json"))
        .expect("tests/vectors/identidade.json presente");
    let identidade_doc: serde_json::Value =
        serde_json::from_str(&identidade_json).expect("identidade.json é JSON válido");
    let identidade: Vec<serde_json::Value> = identidade_doc["vetores"]
        .as_array()
        .expect("identidade.json tem `vetores`")
        .clone();

    let md = onyxchatd::docs_vec::gerar_markdown(&camadas, &vetores, &hs, &identidade);

    let dir = raiz_projeto.join("tests/vectors");
    fs::create_dir_all(&dir).expect("mkdir tests/vectors");
    fs::write(dir.join("camadas.json"), &camadas_json).expect("escreve camadas.json");
    fs::write(dir.join("pipeline.json"), &pipeline_json).expect("escreve pipeline.json");
    fs::write(dir.join("handshake.json"), &handshake_json).expect("escreve handshake.json");
    fs::write(raiz_projeto.join("docs/test_vectors.md"), &md).expect("escreve test_vectors.md");

    println!(
        "{} camadas, {} vetores fim-a-fim, {} handshake",
        camadas.len(),
        vetores.len(),
        hs.len()
    );
    println!(
        "escrito: tests/vectors/camadas.json, pipeline.json, handshake.json, docs/test_vectors.md"
    );
    println!(
        "        identidade.json foi lido, não gerado — a fórmula é de Python"
    );

    // ---- Corpora de fuzz (F3.1) ---------------------------------------
    //
    // Escritos depois dos JSON, porque derivam deles. Um corpus que
    // divergisse dos vectores seria um corpus a testar um formato que já
    // não existe.
    let raiz_corpus = raiz_projeto.join("fuzz/corpus");
    let mut total = 0usize;
    match escrever_corpus("envelope_parser", &corpus_envelope(&vetores)) {
        Ok(n) => total += n,
        Err(erro) => eprintln!("aviso: corpus envelope_parser: {erro}"),
    }
    match escrever_corpus("handshake_parser", &corpus_handshake(&hs)) {
        Ok(n) => total += n,
        Err(erro) => eprintln!("aviso: corpus handshake_parser: {erro}"),
    }
    match escrever_corpus("relay_parser", &corpus_relay()) {
        Ok(n) => total += n,
        Err(erro) => eprintln!("aviso: corpus relay_parser: {erro}"),
    }
    match escrever_corpus("ipc_parser", &corpus_ipc()) {
        Ok(n) => total += n,
        Err(erro) => eprintln!("aviso: corpus ipc_parser: {erro}"),
    }
    match escrever_corpus("k4_decoder", &corpus_k4(&camadas)) {
        Ok(n) => total += n,
        Err(erro) => eprintln!("aviso: corpus k4_decoder: {erro}"),
    }
    match escrever_corpus("k7_decoder", &corpus_k7(&camadas)) {
        Ok(n) => total += n,
        Err(erro) => eprintln!("aviso: corpus k7_decoder: {erro}"),
    }

    println!(
        "corpora de fuzz: {total} seeds em {}",
        raiz_corpus.display()
    );
}

// ---------------------------------------------------------------------
// fuzz/corpus — corpora de seed do `cargo fuzz` (F3.1)
// ---------------------------------------------------------------------
//
// Porquê gerar os corpora em vez de os deixar à mercê do investigador:
//
//   * `fuzz/.gitignore` ignorava `corpus`, logo um clone começava com
//     os 6 alvos sem uma única seed — e sem uma seed **válida** o
//     libFuzzer não ganha profundidade nos ramos que interessam
//     (envelope bem formado, corpo de handshake bem formado, limites
//     exactos). Passa a haver seeds versionadas.
//
//   * As seeds saem da **fonte única de verdade** (`tests/vectors/*.json`),
//     não de Bytes escritos à mão. Um corpus que pode divergir dos
//     vectores é um corpus que testa o formato antigo.
//
// O que se coloca em cada alvo:
//
//   * o(s) input(s) válido(s) — para o fuzzer mutar a partir de algo
//     que o parser aceita;
//   * os **limites exactos** (mínimo, máximo, e o valor a moins um) —
//     é onde os parsers falham mais, e é o que a propriedade de
//     rejeição segura tem de cobrir;
//   * os corpos de handshake, que é a estrutura mais complexa que entra
//     por rede.
//
// As trincas **não** entram no corpus: um crash é um artefacto de
// debugging, não uma seed. Ficam em `fuzz/artifacts/`.

/// Escreve um corpus de seed para um alvo do `cargo fuzz`.
///
/// Apaga o directório antes de escrever. Sem isto, seeds de versões
/// anteriores continuariam no corpus e o `cargo fuzz` queixaria de
/// ficheiros obsoletos — ou pior, manteria seeds de um formato que já
/// não existe.
fn escrever_corpus(alvo: &str, seeds: &[Vec<u8>]) -> std::io::Result<usize> {
    let dir = raiz().join("fuzz/corpus").join(alvo);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    for (indice, seed) in seeds.iter().enumerate() {
        // Nome estável e legível: `00-<prefixo>.bin`. O prefixo permite
        // identificar a origem sem abrir o ficheiro, e o índice mantém a
        // ordem determinística entre execuções.
        let prefixo = hex(&seed[..seed.len().min(8)]);
        let nome = format!("{indice:02}-{prefixo}.bin");
        fs::write(dir.join(nome), seed)?;
    }
    Ok(seeds.len())
}

/// Corpus de `envelope_parser`: envelopes e limites do formato.
///
/// Inclui o envelope mínimo válido e o plaintext de 1 byte, para que o
/// alvo veja o formato real e não apenas entradas absurdas.
fn corpus_envelope(vetores: &[Value]) -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = Vec::new();

    // Envelopes completos dos vectors fim-a-fim — entradas que o parser
    // aceita, a partir das quais o fuzzer pode mutar preservando a
    // estrutura.
    for v in vetores {
        if let Some(envelope) = v["envelope_hex"].as_str() {
            seeds.push(dehex(envelope));
        }
    }

    // Os limites, que é onde os parsers falham.
    seeds.push(Vec::new());                                   // 0 bytes
    seeds.push(vec![0u8; onyxchatd::envelope::TAM_MINIMO - 1]); // 116
    seeds.push(vec![0u8; onyxchatd::envelope::TAM_MINIMO]);     // 117, mínimo
    seeds.push(vec![0u8; onyxchatd::envelope::TAM_MINIMO + 1]); // 118
    seeds.push(vec![0x00u8; onyxchatd::envelope::MAX_ENVELOPE]); // máximo

    // Versão errada, com o resto bem formado: o parser tem de recusar
    // *antes* de tentar decifrar, e esta seed exercita esse caminho.
    if let Some(mut envelope) = vetores
        .first()
        .and_then(|v| v["envelope_hex"].as_str())
        .map(dehex)
    {
        let mut errada = envelope.clone();
        errada[0] = 0x02; // versão desconhecida
        seeds.push(errada);
        envelope.truncate(envelope.len() - 1);
        seeds.push(envelope); // truncado
    }

    seeds
}

/// Corpus de `handshake_parser`: os três corpos + limites.
fn corpus_handshake(hs: &[Value]) -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = Vec::new();
    if let Some(v) = hs.first() {
        for campo in ["request_hex", "accept_hex", "reject_hex"] {
            if let Some(hexs) = v[campo].as_str() {
                seeds.push(dehex(hexs));
                // E cada corpo truncado em 1, para o comprimento exacto.
                let mut cortada = dehex(hexs);
                cortada.truncate(cortada.len().saturating_sub(1));
                seeds.push(cortada);
            }
        }
    }
    // Os limites declarados em `docs/handshake.md`, para que o alvo veja
    // a fronteira exacta.
    seeds.push(Vec::new());
    seeds.push(vec![0u8; handshake::TAM_CORPO_PEDIDO]);
    seeds.push(vec![0u8; handshake::TAM_CORPO_ACEITE]);
    seeds.push(vec![0u8; handshake::TAM_CORPO_RECUSA]);
    seeds
}

/// Corpus de `relay_parser`: frames com mailbox, tipos e corpos.
fn corpus_relay() -> Vec<Vec<u8>> {
    let mut seeds = vec![
        Vec::new(),
        vec![0u8; 1],
        vec![0u8; 1 + 32],
        vec![0u8; onyxchatd::p2p::TAM_MAX_CORPO.min(4096)],
    ];
    // Um frame `CHAT` real: `[comprimento u32 LE][tipo][corpo]`.
    let corpo = vec![0x11u8; 40];
    let mut frame = (1 + corpo.len() as u32).to_le_bytes().to_vec();
    frame.push(0x01); // FRAME_CHAT
    frame.extend_from_slice(&corpo);
    seeds.push(frame);
    seeds
}

/// Corpus de `ipc_parser`: um pedido `HELLO` bem formado + limites.
fn corpus_ipc() -> Vec<Vec<u8>> {
    let mut seeds = vec![Vec::new(), vec![0u8], vec![0x00u8; 2]];
    // `[comprimento u32 LE][0x00 HELLO][0x01 versão]`
    let mut pedido = 2u32.to_le_bytes().to_vec();
    pedido.push(0x00);
    pedido.push(0x01);
    seeds.push(pedido);
    seeds
}

/// Corpus de `k4_decoder`: camadas K1 (o input de K4) de vários
/// comprimentos, para o alvo exercitar o padding e a transposição.
fn corpus_k4(camadas: &[Value]) -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = vec![Vec::new(), vec![0u8], vec![0u8; 5]];
    // A entrada de K4 (c2) de cada KAT — o input real da camada.
    for c in camadas {
        if c["id"].as_str() == Some("K4") {
            if let Some(entrada) = c["input_hex"].as_str() {
                seeds.push(dehex(entrada));
            }
            if let Some(saida) = c["output_hex"].as_str() {
                // O output também: é o pior caso de decifragem.
                seeds.push(dehex(saida));
            }
        }
    }
    // Uma matriz de comprimentos 0..=17, que é o intervalo que os testes
    // C++ exercitam — e onde o padding muda de tamanho.
    for n in 0..=17usize {
        seeds.push((0..n).map(|i| (i * 37 + n) as u8).collect());
    }
    seeds
}

/// Corpus de `k7_decoder`: matrizes Hill de vários comprimentos.
fn corpus_k7(camadas: &[Value]) -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = vec![Vec::new(), vec![0u8], vec![0u8; 2]];
    for c in camadas {
        if c["id"].as_str() == Some("K7") {
            if let Some(entrada) = c["input_hex"].as_str() {
                seeds.push(dehex(entrada));
            }
            if let Some(saida) = c["output_hex"].as_str() {
                seeds.push(dehex(saida));
            }
        }
    }
    seeds
}

/// Converte hex em bytes; panic se inválido (é um vector nosso).
fn dehex(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "hex com comprimento ímpar: {s}");
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex válido"))
        .collect()
}
