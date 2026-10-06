// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// ipc_parser — fuzz do protocolo local UDS (Python ↔ Rust)
// ---------------------------------------------------------------------
// Fronteira: `processar_sessao` recebe o corpo de um pedido vindo do
// socket Unix (docs/ipc_spec.md §Pedidos) — a fronteira de confiança
// entre o cliente Python e o daemon.
//
// Duas passagens por construção:
//   1. `hello_feito = false` — só pode aceitar `HELLO`; todo o resto
//      tem de devolver erro sem executar nada;
//   2. `hello_feito = true`  — atinge `processar_pedido`, ou seja o
//      parser completo de comandos (ENCODE/DECODE/handshake/...).
//
// Propriedade: o daemon nunca panica com dados do cliente — toda a
// resposta é um frame `Vec<u8>` bem formado, mesmo para corpos
// arbitrários.
// =====================================================================

#![no_main]

use libfuzzer_sys::fuzz_target;

use onyxchatd::ipc::{processar_sessao, Estado};
use onyxchatd::tor::TorFalso;

fuzz_target!(|dados: &[u8]| {
    // Estado inerte: `TorFalso` não toca na rede, pelo que o alvo é
    // determinístico e corre offline (mesmo contrato dos testes).
    let estado = Estado::novo(Box::new(TorFalso::novo()));

    // 1) Sem HELLO — todo o comando excepto `HELLO` é rejeitado cedo.
    let mut sem_hello = false;
    let _antes = processar_sessao(&estado, dados, &mut sem_hello);

    // 2) Com HELLO dado — chega ao parser de pedidos propriamente dito.
    let mut com_hello = true;
    let _depois = processar_sessao(&estado, dados, &mut com_hello);

    // O resultado é consumido sem assert: o contrato verificado aqui é
    // a ausência de panic/loop perante bytes arbitrários (o enquadramento
    // e os erros tipados são cobertos por `tests/ipc.rs` e pelos testes
    // Python de `_interpretar`).
});
