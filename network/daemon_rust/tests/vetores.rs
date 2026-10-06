// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// vetores.rs — os vetores oficiais observados pelo Rust
// ---------------------------------------------------------------------
// Lê `tests/vectors/*.json` (gerados por `examples/gerar_vetores.rs`)
// e verifica que TODAS as camadas reproduzem exatamente os outputs
// documentados, incluindo roundtrips (Resumo §25 e §47) e os corpos
// de handshake (REQUEST/ACCEPT/REJECT).
//
// Cobre aqui: K1/K5/K7 (crypto_core), K2/K3/K6/K8 (Lua real via
// `mlua`), K4 (C++ via FFI) e K9 (C via FFI) — ou seja, todas as
// implementações do pipeline numa só passagem.
// =====================================================================

use std::fs;
use std::path::PathBuf;

use crypto_core::signer::chave_publica_a_partir_da_seed;
use serde_json::Value;

use onyxchatd::anti_replay::RegistoNonce1;
use onyxchatd::pipeline::{self, Chaves};
use onyxchatd::{ffi_c, handshake, lua_camadas};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn hex(b: &[u8]) -> String {
    b.iter().map(|octeto| format!("{octeto:02x}")).collect()
}

fn dehex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2), "hex com comprimento ímpar: {s}");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex válido"))
        .collect()
}

fn dehex32(s: &str) -> [u8; 32] {
    dehex(s).try_into().expect("32 bytes")
}

fn dehex12(s: &str) -> [u8; 12] {
    dehex(s).try_into().expect("12 bytes")
}

fn carregar(nome: &str) -> Value {
    let caminho = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/vectors")
        .join(nome);
    let texto = fs::read_to_string(&caminho).expect("vetores presentes (correr o gerador)");
    serde_json::from_str(&texto).expect("JSON de vetores válido")
}

fn campo<'a>(v: &'a Value, chave: &str) -> &'a str {
    v[chave]
        .as_str()
        .unwrap_or_else(|| panic!("campo {chave} ausente"))
}

fn opcional<'a>(v: &'a Value, chave: &str) -> Option<&'a str> {
    v.get(chave).and_then(|x| x.as_str())
}

// ---------------------------------------------------------------------
// §25 — uma entrada por camada
// ---------------------------------------------------------------------

#[test]
fn camadas_reproduzem_os_vetores() {
    let doc = carregar("camadas.json");
    let camadas = doc["camadas"].as_array().expect("array de camadas");
    assert_eq!(camadas.len(), 9, "nove camadas documentadas");

    for e in camadas {
        let id = campo(e, "id");
        let entrada = dehex(campo(e, "input_hex"));
        let esperado = campo(e, "output_hex");
        let chave = opcional(e, "chave_hex").map(dehex);
        let nonce = opcional(e, "nonce_hex").map(dehex);

        // Codificação com a implementação nativa de cada camada.
        let saida: Vec<u8> =
            match id {
                "K1" => crypto_core::aead_chacha::encrypt(
                    &entrada,
                    chave.as_deref().expect("K1 tem chave"),
                    nonce.as_deref().expect("K1 tem nonce"),
                )
                .expect("K1 cifra"),
                "K2" => lua_camadas::deslocamento(&entrada, pipeline::DESLOCAMENTO_K2).expect("K2"),
                "K3" => lua_camadas::vigenere(&entrada, chave.as_deref().expect("K3 chave"))
                    .expect("K3"),
                "K4" => ffi_c::k4_cifrar(&entrada).expect("K4"),
                "K5" => crypto_core::encrypt_k5(
                    &entrada,
                    chave.as_deref().expect("K5 tem chave"),
                    nonce.as_deref().expect("K5 tem nonce"),
                )
                .expect("K5 cifra"),
                "K6" => lua_camadas::playfair(&entrada, chave.as_deref().expect("K6 chave"))
                    .expect("K6"),
                "K7" => crypto_core::hill::cifrar(&entrada),
                "K8" => lua_camadas::deslocamento(&entrada, pipeline::DESLOCAMENTO_K8).expect("K8"),
                "K9" => {
                    let chave_k9: &[u8; 32] = chave
                        .as_deref()
                        .expect("K9 tem chave")
                        .try_into()
                        .expect("K9 chave 32B");
                    let nonce_k9: &[u8; 12] = nonce
                        .as_deref()
                        .expect("K9 tem nonce")
                        .try_into()
                        .expect("K9 nonce 12B");
                    ffi_c::k9_cifrar(&entrada, chave_k9, nonce_k9).expect("K9 cifra")
                }
                outro => panic!("camada desconhecida {outro}"),
            };
        assert_eq!(hex(&saida), esperado, "{id}: output difere do vetor");

        // Roundtrip: a decifragem tem de devolver o input exato.
        let volta: Vec<u8> = match id {
            "K1" => crypto_core::aead_chacha::decrypt(
                &saida,
                chave.as_deref().unwrap(),
                nonce.as_deref().unwrap(),
            )
            .expect("K1 decifra"),
            "K2" => lua_camadas::deslocamento_inverso(&saida, pipeline::DESLOCAMENTO_K2).expect("K2"),
            "K3" => lua_camadas::vigenere_inverso(&saida, chave.as_deref().unwrap()).expect("K3"),
            "K4" => ffi_c::k4_decifrar(&saida).expect("K4"),
            "K5" => crypto_core::decrypt_k5(
                &saida,
                chave.as_deref().unwrap(),
                nonce.as_deref().unwrap(),
            )
            .expect("K5 decifra"),
            "K6" => lua_camadas::playfair_inverso(&saida, chave.as_deref().unwrap()).expect("K6"),
            "K7" => crypto_core::hill::decifrar(&saida).expect("K7"),
            "K8" => {
                lua_camadas::deslocamento_inverso(&saida, pipeline::DESLOCAMENTO_K8).expect("K8")
            }
            "K9" => {
                let chave_k9: &[u8; 32] = chave.as_deref().unwrap().try_into().expect("32B");
                let nonce_k9: &[u8; 12] = nonce.as_deref().unwrap().try_into().expect("12B");
                ffi_c::k9_decifrar(&saida, chave_k9, nonce_k9).expect("K9 decifra")
            }
            outro => panic!("camada desconhecida {outro}"),
        };
        assert_eq!(volta, entrada, "{id}: roundtrip diverge");
    }
}

/// Âncora normativa de K7: `docs/pipeline.md` §K7 documenta
/// `"AB" → 89cc0c0e` — se isto falhar, a documentação está desatualizada.
#[test]
fn k7_ancora_documentada() {
    let saida = crypto_core::hill::cifrar(b"AB");
    assert_eq!(hex(&saida), "89cc0c0e");
}

// ---------------------------------------------------------------------
// §47 — vetores fim-a-fim (K1..K9 + Final)
// ---------------------------------------------------------------------

#[test]
fn vetores_fim_a_fim_sao_reproduzidos() {
    let doc = carregar("pipeline.json");
    let vetores = doc["vetores"].as_array().expect("array de vetores");
    assert_eq!(vetores.len(), 3, "três vetores fim-a-fim");

    for v in vetores {
        let id = campo(v, "id");
        let texto = campo(v, "plaintext_utf8");
        let plaintext = dehex(campo(v, "plaintext_hex"));
        assert_eq!(
            plaintext,
            texto.as_bytes(),
            "vector {id}: hex/utf8 coerentes"
        );

        let chaves = Chaves {
            k1: dehex32(campo(v, "chave_k1_hex")),
            k5: dehex32(campo(v, "chave_k5_hex")),
            k9: dehex32(campo(v, "chave_k9_hex")),
        };
        let seed = dehex32(campo(v, "seed_hex"));
        let publica = chave_publica_a_partir_da_seed(&seed);
        assert_eq!(
            hex(&publica),
            campo(v, "publica_hex"),
            "vector {id}: pub derivada da seed"
        );

        // Cifragem determinística → envelope e camadas idênticos.
        let (envelope, c) = pipeline::cifrar_com_nonces(
            &plaintext,
            &chaves,
            &seed,
            dehex12(campo(v, "nonce1_hex")),
            dehex12(campo(v, "nonce5_hex")),
            dehex12(campo(v, "nonce9_hex")),
        )
        .unwrap_or_else(|e| panic!("vector {id}: {e}"));

        let camadas = [
            &c.c1, &c.c2, &c.c3, &c.c4, &c.c5, &c.c6, &c.c7, &c.c8, &c.c9,
        ];
        for (i, conteudo) in camadas.iter().enumerate() {
            assert_eq!(
                hex(conteudo),
                campo(v, &format!("c{}_hex", i + 1)),
                "vector {id}: c{} difere",
                i + 1
            );
        }
        assert_eq!(
            hex(&envelope),
            campo(v, "envelope_hex"),
            "vector {id}: envelope final"
        );

        // Roundtrip completo: assinatura + anti-replay + pipeline.
        let mut registo = RegistoNonce1::novo();
        let volta = pipeline::decifrar(&envelope, &chaves, &publica, &mut registo)
            .unwrap_or_else(|e| panic!("vector {id} decifra: {e}"));
        assert_eq!(volta, texto, "vector {id}: roundtrip");
    }
}

// ---------------------------------------------------------------------
// Handshake — FRIEND_REQUEST (209) / ACCEPT (177) / REJECT (81)
// ---------------------------------------------------------------------

#[test]
fn handshake_reproduz_os_vetores() {
    let doc = carregar("handshake.json");
    let vetores = doc["vetores"].as_array().expect("array de handshake");
    assert_eq!(vetores.len(), 1, "um vector de handshake");

    for v in vetores {
        let id = campo(v, "id");
        let seed_a = dehex32(campo(v, "seed_a_hex"));
        let seed_b = dehex32(campo(v, "seed_b_hex"));
        let publica_b = chave_publica_a_partir_da_seed(&seed_b);
        assert_eq!(
            hex(&publica_b),
            campo(v, "publica_b_hex"),
            "vector {id}: pub B derivada da seed"
        );
        let nonce: [u8; handshake::TAM_NONCE] =
            dehex(campo(v, "nonce_hex")).try_into().expect("nonce 16B");
        let k1 = dehex32(campo(v, "k1_hex"));
        let k5 = dehex32(campo(v, "k5_hex"));
        let k9 = dehex32(campo(v, "k9_hex"));
        let k5_b = dehex32(campo(v, "k5_b_hex"));
        let k9_b = dehex32(campo(v, "k9_b_hex"));

        // Montagem determinística → corpos idênticos aos documentados.
        let (pedido, request) = handshake::montar_pedido_com(&seed_a, nonce, k1, k5, k9);
        assert_eq!(request.len(), handshake::TAM_CORPO_PEDIDO, "REQUEST 209");
        assert_eq!(
            hex(&request),
            campo(v, "request_hex"),
            "vector {id}: FRIEND_REQUEST"
        );

        let (_, aceite) = handshake::aceitar_pedido_com(&seed_b, &pedido, k5_b, k9_b);
        assert_eq!(aceite.len(), handshake::TAM_CORPO_ACEITE, "ACCEPT 177");
        assert_eq!(
            hex(&aceite),
            campo(v, "accept_hex"),
            "vector {id}: FRIEND_ACCEPT"
        );

        let recusa = handshake::montar_recusa(&seed_b, &nonce);
        assert_eq!(recusa.len(), handshake::TAM_CORPO_RECUSA, "REJECT 81");
        assert_eq!(
            hex(&recusa),
            campo(v, "reject_hex"),
            "vector {id}: FRIEND_REJECT"
        );

        // Validações: B valida o pedido, A confirma, recusa comprova.
        let pedido_v = handshake::validar_pedido(&request).expect("pedido válido");
        assert_eq!(pedido_v, pedido, "pedido idêntico ao validado");
        let amizade = handshake::confirmar_aceite(&seed_a, &request, &aceite).expect("A confirma");
        assert_eq!(hex(&amizade.publica), campo(v, "publica_b_hex"));
        assert_eq!(amizade.k1, k1, "amizade guarda K1");
        assert_eq!(amizade.k5_proprio, k5, "K5_A é própria em A");
        assert_eq!(amizade.k9_proprio, k9, "K9_A é própria em A");
        assert_eq!(amizade.k5_par, k5_b, "K5_B viaja no aceite");
        assert_eq!(amizade.k9_par, k9_b, "K9_B viaja no aceite");
        handshake::validar_recusa_com(&recusa, &nonce, &publica_b).expect("recusa válida");
    }
}

/// **`docs/test_vectors.md` não pode divergir de `tests/vectors/*.json`.**
///
/// O documento legível diz «não editar à mão», e essa instrução não é um
/// mecanismo. Este teste é o mecanismo: re-renderiza o documento a partir
/// dos JSON, usando a **mesma função** que o gerador usa, e compara com o
/// ficheiro em disco.
///
/// Sem isto, alguém podia corrigir um valor à mão no `.md`, o `.md` ficaria
/// com uma paridade de implementações que não existe, e nenhum teste
/// notaria — a documentação passaria a mentir sem ninguém saber quando.
#[test]
fn test_vectors_md_bate_com_os_json() {
    let camadas = carregar("camadas.json");
    let pipeline = carregar("pipeline.json");
    let handshake = carregar("handshake.json");

    // `.as_array()` converte `&Value` em `&Vec<Value>`, que é o que a
    // função de renderização espera. Um vector em falta dá `None` e o
    // `.expect` diz qual dos três — não um `panic` de índice.
    let esperado = onyxchatd::docs_vec::gerar_markdown(
        camadas["camadas"].as_array().expect("camadas.json tem `camadas`"),
        pipeline["vetores"].as_array().expect("pipeline.json tem `vetores`"),
        handshake["vetores"].as_array().expect("handshake.json tem `vetores`"),
    );

    let caminho = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/test_vectors.md");
    let actual = fs::read_to_string(&caminho).expect("docs/test_vectors.md presente");

    if esperado != actual {
        // Diagnóstico útil: a primeira linha que diverge, para não cuspir
        // dois documentos de 200 linhas no terminal.
        let esperado_linhas: Vec<&str> = esperado.lines().collect();
        let actual_linhas: Vec<&str> = actual.lines().collect();
        for (i, (e, a)) in esperado_linhas.iter().zip(actual_linhas.iter()).enumerate() {
            if e != a {
                panic!(
                    "docs/test_vectors.md divergiu dos vectores na linha {}:\n                       esperado: {e:?}\n  no ficheiro: {a:?}\n\
                     ({} vs {} linhas)\n\
                     Correcção: correr `cargo run -p onyxchatd --example \
                     gerar_vetores --features docs` — o documento é gerado, \
                     não editado.",
                    i + 1,
                    esperado_linhas.len(),
                    actual_linhas.len(),
                );
            }
        }
        panic!(
            "docs/test_vectors.md divergiu dos vectores: {} linhas contra {}",
            esperado_linhas.len(),
            actual_linhas.len(),
        );
    }
}

/// Os vectores fim-a-fim não podem incluir plaintext vazio.
///
/// Regista a razão pela qual o vector 003 deixou de ser o caso «vazio»:
/// o pipeline adoptou `MIN_PLAINTEXT = 1` (F2.2). Sem este teste, um
/// vector vazio podia ser reintroduzido e o `cargo run` do gerador
/// passaria a falhar só em tempo de execução, com um erro pouco claro.
#[test]
fn vectores_fim_a_fim_tem_plaintext_nao_vazio() {
    let pipeline = carregar("pipeline.json");
    for v in pipeline["vetores"].as_array().expect("array de vectores") {
        let id = campo(v, "id");
        let plaintext = campo(v, "plaintext_utf8");
        assert!(
            !plaintext.is_empty(),
            "vector {id}: plaintext vazio — o pipeline rejeita \
             (MIN_PLAINTEXT = 1, docs/pipeline.md §Limites)"
        );
    }
}
