// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// onyxchatd — biblioteca do daemon OnyxChat (pipeline K1→K9 + IPC)
// ---------------------------------------------------------------------
// Módulos públicos consumidos pelo binário `main.rs` e pelos testes:
//
//   * `envelope`   — envelope binário 101+N (docs/message_format.md);
//   * `erros`      — erro unificado + tabela de códigos IPC;
//   * `anti_replay`— registo circular de `nonce1` do chat;
//   * `ffi_c`      — ponte FFI para K4 (C++) e K9 (C/libsodium);
//   * `lua_camadas`— embedding Lua das camadas K2/K3/K6/K8;
//   * `pipeline`   — orquestração K1→K9 e assinatura Ed25519;
//   * `ipc`        — servidor UDS + protocolo ENCODE/DECODE;
//   * `tor`        — traço `BackendTor` + `TorFalso` (Etapa 7);
//   * `tor_arti`   — `TorReal`: arti-client embutido (único ficheiro
//                    que toca na rede real; fora da métrica de cobertura;
//                    só existe com a feature `tor-real`, que NÃO é de
//                    omissão — ver `Cargo.toml`).
//   * `p2p`        — frames, servidor/ligação P2P, fallback relay;
//   * `handshake`  — FRIEND_REQUEST/ACCEPT/REJECT + anti-replay;
//   * `docs_vec`   — renderização de `docs/test_vectors.md` (feature
//                    `docs`; função pura, verificada por teste).
// =====================================================================

pub mod anti_replay;
pub mod envelope;
pub mod erros;
pub mod ffi_c;
pub mod handshake;
pub mod ipc;
pub mod lua_camadas;
pub mod orcamento;
pub mod p2p;
pub mod pipeline;
pub mod tor;
/// `TorReal` — arti-client embutido, presente apenas com a feature
/// `tor-real`. Construtores da biblioteca sem essa feature (o crate de
/// fuzzing, por exemplo) compilam o daemon inteiro sem tocar na árvore
/// da arti: -355 crates, -70% de tempo de build e um pico de memória
/// do `rustc` várias vezes menor. Ver `network/daemon_rust/Cargo.toml`.
#[cfg(feature = "tor-real")]
pub mod tor_arti;
/// Renderização de `docs/test_vectors.md` — só com a feature
/// `docs`, porque precisa de `serde_json` e só é usada em build
/// e teste. Ver `Cargo.toml` e `docs/testing.md` §Test vectors.
#[cfg(feature = "docs")]
pub mod docs_vec;
