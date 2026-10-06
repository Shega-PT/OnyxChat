# Roadmap — estado do projecto

Este documento **descreve o estado**, não compromete datas. Serve para
que uma ideia futura nunca seja confundida com funcionalidade existente.

Rótulos usados:

```text
IMPLEMENTADO  →  existe, funciona e está testado
PLANEADO      →  decidido como próximo passo, especificado, ainda sem código
CONCEITO      →  investigação, sem especificação formal
```

> **Regra de sincronização.** Este documento é actualizado no **fim** de
> cada fase de trabalho, depois de o código aterrar — nunca antes. Um
> `PLANEADO` que vira `IMPLEMENTADO` é um rótulo que reflecte a
> realidade, não uma intenção.

---

## IMPLEMENTADO

### Núcleo criptográfico

| Item | Onde |
| --- | --- |
| Pipeline K1→K9 completo | [`pipeline.md`](pipeline.md), `pipeline.rs` |
| Envelope binário com assinatura | [`message_format.md`](message_format.md), `envelope.rs` |
| Handshake de amizade assinado (com versão) | [`handshake.md`](handshake.md), `handshake.rs` |
| Anti-replay (handshake + chat por `nonce1`) | `handshake.rs`, `anti_replay.rs` |
| Anti-downgrade (versão assinada) | envelope + transcript |
| Identidade Ed25519 | `crypto/rust/src/signer.rs` |
| Gestão de chaves e zeroização | [`key_management.md`](key_management.md), `secret.rs` |

### Rede e processos

| Item | Onde |
| --- | --- |
| Daemon Rust (`onyxchatd`) | `network/daemon_rust` |
| Tor embutido (`BackendTor` + arti) | `tor.rs`, `tor_arti.rs` |
| Falha explícita quando `--tor arti` sem a feature | `main.rs`, `construir_backend` |
| P2P directo (hidden service ↔ hidden service) | `p2p.rs` |
| Transporte por relay opcional | [`relay.md`](relay.md), `relay_server.py` |
| Discovery `ID → .onion` com TTL | [`discovery.md`](discovery.md), `discovery_server.py` |
| IPC local UDS (autenticado por uid) | [`ipc_spec.md`](ipc_spec.md), `ipc.rs` |
| Cliente Python (mensageiro, keystore cifrado) | `messenger/`, `user/` |

### Qualidade

| Item | Onde |
| --- | --- |
| Testes unitários, integração e E2E | `tests/`, `#[cfg(test)]`, CTest |
| Test vectors multi-linguagem | [`test_vectors.md`](test_vectors.md), `tests/vectors/*.json` |
| Property tests (proptest/hypothesis) | [`testing.md`](testing.md) |
| Fuzzing dos parsers | `fuzz/` |
| Cobertura de linhas 100% (3 linguagens) | [`testing.md`](testing.md) §Cobertura |

### Documentação

| Item | Onde |
| --- | --- |
| Guias: sistema, utilizador, desenvolvedor | `SYS_GUIDE.md`, `USER_GUIDE.md`, `DEV_GUIDE.md` |
| Índice e ordem de leitura | [`index.md`](index.md) |
| Estrutura técnica | [`architecture.md`](architecture.md) |
| Modelos de ameaça/segurança/privacidade | [`threat_model.md`](threat_model.md), [`security_model.md`](security_model.md), [`privacy_model.md`](privacy_model.md) |
| Especificações de protocolo | `pipeline.md`, `message_format.md`, `handshake.md`, `ipc_spec.md`, `p2p.md`, `relay.md`, `discovery.md` |

---

## PLANEADO

Decidido como próximo passo, especificado nos documentos normativos,
**ainda sem implementação**:

| Item | Especificado em | Nota |
| --- | --- | --- |
| `destino` por ID hex: validação em relay (`pub_de_hex(destino) == pub_par`) e resolução `ID → .onion` no cliente Python | [`ipc_spec.md`](ipc_spec.md) §`destino`, [`discovery.md`](discovery.md) §Implementação | `pub_de_hex()` existe mas é código morto |
| K4 rejeita entrada vazia na cifragem; pipeline rejeita `MIN_PLAINTEXT` | [`pipeline.md`](pipeline.md) §K4 e §Limites | fecha um vector de entrada degenerada |
| Keystore v2: AAD = `mágica ‖ sal`, bump para `ONYXKS2` | [`key_management.md`](key_management.md) §Keystore | quebra keystores v1 — exige migração |
| `mlockall(MCL_CURRENT\|MCL_FUTURE)` no arranque do daemon | [`key_management.md`](key_management.md) §Exposição a swap, [`threat_model.md`](threat_model.md) §A10 | fecha a lacuna entre zeroização e page-out |
| Teste de guarda do isolamento Tor | [`testing.md`](testing.md) §Propriedades | `testing.md` e `README` prometiam-no sem existir |
| Teste de não-divergência `docs/test_vectors.md` ↔ JSON | [`testing.md`](testing.md) §Propriedades | fecha a regra «não editar à mão» |
| Corpora de fuzzing gerados a partir dos vectores | [`testing.md`](testing.md) §Fuzzing | 2 dos 6 corpora estão vazios e todos são gitignored |
| Routing nodes (nós de transporte entre instâncias) | [`SYS_GUIDE.md`](SYS_GUIDE.md) §10.4 | preparação em [`architecture.md`](architecture.md) |
| Rede de transporte descentralizada | [`SYS_GUIDE.md`](SYS_GUIDE.md) §10.4 | depende de routing nodes |
| Discovery descentralizado / distribuído | [`discovery.md`](discovery.md) §Componente substituível | substitui o servidor único |
| IDs de algoritmo por camada (negociação de pipeline) | — | exigirá nova versão de envelope |
| **Ligar a K1 derivada ao caminho por omissão** (fase I) | [`key_management.md`](key_management.md) §Chaves de comunicação, [`threat_model.md`](threat_model.md) §Limitações 8 | a derivação está implementada e testada nas duas linguagens, mas `ipc_client.cifrar()` continua a passar a K1 de longa duração. **É uma decisão, não um bug**: clientes velhos e novos não se entendem, e o que se perde sem forward secrecy é a major parte do benefício declarado |

---

## CONCEITO

Investigação — **sem especificação formal**, não deve ser descrito
como funcionalidade:

| Item | Nota |
| --- | --- |
| Fake-IP (camada adicional de anonimização) | exigiria análise completa do modelo de privacidade |
| Pipelines multimédia (áudio, imagem, vídeo) | o pipeline actual é textual, por decisão de âmbito |
| Suporte a ficheiros arbitrários | idem |
| Novos pipelines por tipo de conteúdo | idem |
| Mitigação activa de tráfego (padding, temporização) | declarada em [`privacy_model.md`](privacy_model.md) §4; não planeada |

---

## Estado actual vs futuro — regra editorial

Toda a documentação do projecto distingue estes três rótulos. Se um
documento descrever routing nodes, discovery distribuído, Fake-IP ou
multimédia, está a descrever **PLANEADO** ou **CONCEITO** — nunca
**IMPLEMENTADO**.

Um parágrafo que descreva comportamento do sistema sem dizer em que
estado está é um parágrafo que vai divergir do código. Rótulo sem
equivalente estrutural é proibido.
