// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// docs_vec — renderização de `docs/test_vectors.md` a partir dos vectores
// ---------------------------------------------------------------------
// Porquê isto vive na biblioteca e não no exemplo `gerar_vetores`
//
// O documento legível é uma **função pura** dos três JSON. Se a
// renderização estivesse dentro do exemplo, não haveria forma de provar
// que `docs/test_vectors.md` não divergiu de `tests/vectors/*.json` —
// e o documento diz «não editar à mão», o que é uma regra que precisa
// de um mecanismo que a faça cumprir, não de boa vontade.
//
// Com a renderização aqui, `tests/vetores.rs` re-renderiza o documento a
// partir dos JSON e compara com o ficheiro em disco. Editar o `.md` à
// mão passa a falhar o teste.
// =====================================================================

use serde_json::Value;

use crate::handshake;

pub fn gerar_markdown(camadas: &[Value], vetores: &[Value], hs: &[Value]) -> String {
    let mut md = String::new();
    md.push_str(
        "# Test vectors\n\n\
         > **Gerado automaticamente** por \
         `cargo run -p onyxchatd --example gerar_vetores` a partir de \
         `tests/vectors/*.json` — **não editar à mão**. Os JSON são a \
         fonte única de verdade; este documento é a forma legível dos \
         mesmos dados (Resumo §25 e §47).\n>\n\
         > Consumidores: `network/daemon_rust/tests/vetores.rs` (Rust), \
         `tests/test_vectors.py` (Python), \
         `crypto/c_cpp/tests/test_vectors.c` (C/C++, K4+K9) e as \
         camadas Lua através do Rust (`mlua`). Todas as implementações \
         têm de produzir exatamente os mesmos bytes.\n\n\
         ---\n\n## Vetores por camada (Resumo §25)\n\n\
         Cada vector tem `input`, `key`/parâmetros e `expected output`.\n\n",
    );
    for e in camadas {
        md.push_str(&format!(
            "### {} — {} — {}\n\n```text\nparametros: {}\n",
            e["id"].as_str().unwrap_or("?"),
            e["nome"].as_str().unwrap_or("?"),
            e["implementacao"].as_str().unwrap_or("?"),
            e["parametros"].as_str().unwrap_or("?"),
        ));
        if let Some(c) = e.get("chave_hex").and_then(|x| x.as_str()) {
            md.push_str(&format!("chave:     {c}\n"));
        }
        if let Some(n) = e.get("nonce_hex").and_then(|x| x.as_str()) {
            md.push_str(&format!("nonce:     {n}\n"));
        }
        md.push_str(&format!(
            "input:     {}\noutput:    {}\n```\n\n",
            e["input_hex"].as_str().unwrap_or("?"),
            e["output_hex"].as_str().unwrap_or("?"),
        ));
    }

    md.push_str("---\n\n## Vetores fim-a-fim (Resumo §47)\n\n");
    for v in vetores {
        md.push_str(&format!(
            "### Vector {}\n\n**Input:**\n\n```text\n{}\nhex: {}\n```\n",
            v["id"].as_str().unwrap_or("?"),
            v["plaintext_utf8"].as_str().unwrap_or(""),
            v["plaintext_hex"].as_str().unwrap_or("?"),
        ));
        md.push_str("\n**Camadas** (ciphertext de cada camada):\n\n```text\n");
        for n in 1..=9 {
            md.push_str(&format!(
                "K{n}: {}\n",
                v[format!("c{n}_hex")].as_str().unwrap_or("?"),
            ));
        }
        md.push_str("```\n\n**Final (envelope completo):**\n\n```text\n");
        md.push_str(v["envelope_hex"].as_str().unwrap_or("?"));
        md.push_str("\n```\n\n");
    }

    md.push_str("---\n\n## Handshake (`docs/handshake.md`)\n\n");
    for h in hs {
        md.push_str(&format!(
            "### Vector {} — corpos do handshake\n\n```text\n\
             nonce:    {}\nk1:       {}\nk5_A:     {}\nk9_A:     {}\n\
             k5_B:     {}\nk9_B:     {}\n\n\
             FRIEND_REQUEST ({} B):\n{}\n\n\
             FRIEND_ACCEPT ({} B):\n{}\n\n\
             FRIEND_REJECT ({} B):\n{}\n```\n\n",
            h["id"].as_str().unwrap_or("?"),
            h["nonce_hex"].as_str().unwrap_or("?"),
            h["k1_hex"].as_str().unwrap_or("?"),
            h["k5_hex"].as_str().unwrap_or("?"),
            h["k9_hex"].as_str().unwrap_or("?"),
            h["k5_b_hex"].as_str().unwrap_or("?"),
            h["k9_b_hex"].as_str().unwrap_or("?"),
            handshake::TAM_CORPO_PEDIDO,
            h["request_hex"].as_str().unwrap_or("?"),
            handshake::TAM_CORPO_ACEITE,
            h["accept_hex"].as_str().unwrap_or("?"),
            handshake::TAM_CORPO_RECUSA,
            h["reject_hex"].as_str().unwrap_or("?"),
        ));
    }
    md
}
