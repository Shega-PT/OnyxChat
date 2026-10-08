# Arquitetura do OnyxChat

> **Nível MÉDIO.** Descreve *como* o sistema está organizado e *como* funciona:
> - componentes
> - responsabilidades
> - fluxo
> - fronteiras

> Para o *porquê* e as decisões: [`SYS_GUIDE.md`](SYS_GUIDE.md) ·
> Para os *bytes*: os specs normativos, via [`index.md`](index.md)

---

## Pergunta a que este documento responde

> Como o OnyxChat está construído, e como uma mensagem o atravessa?

---

## Princípio central: cada instância é cliente e servidor

Cada instalação é, simultaneamente:

Este mapa é um resumo. O índice completo, com o papel de cada
ficheiro, está em `../Estrutura.txt`, e é conferido contra a árvore
por `scripts/verificar_estrutura.py`.

```text
         ┌─────────────────────┐
         │    ONYX INSTANCE    │
         │                     │
         │  Cliente            │
         │  Servidor/endpoint  │
         │  Endpoint Tor       │
         │  Identidade         │
         │  Criptografia       │
         └──────────┬──────────┘
                    │
        ┌───────────┴───────────┐
        │                       │
  comunica com            aceita comunicação
  outros Onyx              de outros Onyx
```

**`crypto/python_lua/` não tem implementações em Python.** Uma versão
anterior desta árvore anunciava «Lua = produção · Python = referência»,
e a `Estrutura.txt` listava quatro ficheiros Python que nunca foram
escritos. A referência dos vectores é o JSON em `tests/vectors/`, não
uma segunda implementação — que era, de resto, o que se queria evitar
num projecto onde duas implementações da mesma camada divergem sem
ninguém dar por isso.

**K2 e K8 são o mesmo módulo Lua**, `deslocamento.lua`, com
deslocamentos diferentes (+3 e +7). São a única camada com duas
entradas no mesmo ficheiro, e é deliberado: `lua_camadas.rs` documenta
que um módulo único elimina a classe de bug em que as duas divergem sem
nenhum teste as distinguir.

Um Onyx **não** é um cliente que se liga a um servidor central:

```text
Onyx A <───────────────> Onyx B

Servidor/endpoint A <──> Servidor/endpoint B
       ▲                         │
       │                         │
    cliente                   cliente
```

Isto sustenta a descentralização: não existe papel fixo de «servidor de
mensagens». Os próprios participantes fornecem os endpoints.

---

## Componentes

| Componente | Linguagem | Responsabilidade |
| --- | --- | --- |
| `crypto/rust` (`crypto_core`) | Rust | K1, K5, K7, espelho de K9, Ed25519, `Segredo`+zeroize, CSPRNG |
| `crypto/c_cpp` | C/C++ | K4 (transposição), K9 em produção (libsodium) |
| `crypto/python_lua/lua` | Lua | K2, K3, K6, K8 **em produção** (embutidas no daemon) |
| `crypto/python_lua/lua` | Lua | K2, K3, K6, K8 em produção, embutidas no daemon |
| `network/daemon_rust` (`onyxchatd`) | Rust | orquestra o pipeline, IPC UDS, Tor, P2P, handshake, relay |
| `messenger/` | Python | cliente IPC, envelope, chaves, keystore cifrado |
| `user/` | Python | CLI/TUI e gestão de identidade local |
| `server/discovery_server.py` | Python | bootstrap `ID → .onion` com TTL |
| `server/relay_server.py` | Python | relay TCP opcional (fallback cego) |

### Separação de responsabilidades

A documentação evita frases como «Python-Lua faz a criptografia». A
formulação correcta:

> O subsistema Python/Lua integra o cliente Python com as camadas
> criptográficas em Lua, que correm dentro do daemon.

**Não há implementação de referência em Python.** A formulação
«Lua (produção) e Python (referência)» circulou em versões anteriores
deste documento e da `Estrutura.txt`, e era falsa nos dois sítios: não
existe `crypto/python_lua/python/`. Quem precisou de uma referência
implementou-a nos vectores em `tests/vectors/`, que são a mesma coisa
com uma propriedade que o código não tem — são lidos por
`test_vectors.py` em vez de confiar na memória de quem os escreveu.

Da mesma forma, «Rust» não é apenas uma linguagem: é o componente
responsável pelas primitivas digitais e pelo daemon.

---

## Organização de directórios

```text
crypto/
├── rust/          primitivas digitais (crate crypto_core)
├── c_cpp/         K4, K9 (C/C++)
└── python_lua/    Lua: K2, K3, K6, K8

network/
└── daemon_rust/   daemon onyxchatd

server/            discovery + relay + sidecar
messenger/         cliente Python
user/              CLI/TUI
docs/              guias, modelos e especificações
tests/             pytest (Python)
fuzz/              alvos cargo-fuzz
UI/                interface React/Vite, e o Node local de ferramentas
scripts/           portões de entrega e auditores de texto
```

---

## Fluxo de uma mensagem

### Emissor

```text
utilizador (CLI/TUI)
      │  texto
      ▼
messenger/pipeline.py
      │  ENCODE (0x01) + chaves + seed
      ▼
┌─────────────── onyxchatd (Rust) ───────────────┐
│ ipc.rs ──► pipeline.rs (K1→K9) ──► envelope.rs │
│                     │                          │
│                     ▼                          │
│            assinatura Ed25519                  │
└─────────────────────┬──────────────────────────┘
                      │ frame CHAT (0x01)
                      ▼
    p2p.rs  ────  BackendTor  ────►  rede
                   (tor.rs)
```

### Receptor

```text
parse do envelope
      ▼
verificar assinatura Ed25519   ◄── falha = rejeição, SEM decifrar
      ▼
reservar nonce1 (anti-replay)
      ▼
decifrar K9 → K8 → … → K1
      ▼
verificar tag AEAD
      ▼
UTF-8 válido → plaintext
```

A assinatura é verificada **antes** de qualquer decifragem — propriedade
testada, não convenção. O cliente Python verifica-a localmente antes de
emitir `DECODE`, para que um envelope com assinatura inválida nunca
chegue ao pipeline.

---

## `BackendTor` — abstração de rede

```text
daemon (onyxchatd)
   │
   ▼
   trait BackendTor       (tor.rs)
   │
   ├── TorReal            (tor_arti.rs — único contacto com a rede)
   └── TorFalso           (loopback — testes e E2E)
```

Regras:

1. O restante daemon **nunca** depende de APIs específicas da arti —
   fala sempre com o traço `BackendTor`.
2. `tor_arti.rs` é o **único** ficheiro que toca na rede real. Isto é
   verificado por um teste de guarda (`testing.md`).
3. Tudo o resto corre offline com `TorFalso` (`--tor nenhum`), o que
   permite testes determinísticos e desenvolvimento sem rede.

Consequência prática: os testes nunca dependem de Tor. A feature
`tor-real` é opt-in para que o ciclo de testes não compile a árvore da
arti (`DEV_GUIDE.md` §2.3).

---

## Discovery ≠ Routing

```text
DISCOVERY (existe hoje)          ROUTING (PLANEADO)
────────────────────────         ─────────────────────────────
Onyx-ID → .onion, com TTL        transportar dados entre
função: encontrar o endpoint     participantes por nós
não guarda mensagens             intermédios
não guarda conversações
não controla sessões
não decifra nada
```

O discovery server é **infraestrutura de bootstrap**, não servidor
central de comunicação. Ver [`discovery.md`](discovery.md).

---

## Relay como transporte opcional

```text
Modo A:  Onyx A ←────────────→ Onyx B        (directo)
Modo B:  Onyx A ←──→ Relay ←──→ Onyx B       (fallback)
```

O relay **não** é servidor de mensagens; **não** é proprietário da
conversa; **não** possui as chaves de conteúdo; mantém um buffer
temporário com TTL de 300s e um máximo de 64 envelopes pendentes por
mailbox. É **apenas** uma opção de transporte. Ver [`relay.md`](relay.md).

---

## Fronteiras de confiança

```text
Python client  ──►  UDS  ──►  Rust daemon  ──►  Tor  ──►  rede
```

Cada fronteira tem: formato, validação, erros, timeout, limites e
comportamento perante dados inválidos. Ver `ipc_spec.md`
Fronteira de confiança (fronteira local) e [`p2p.md`](p2p.md) (fronteira rede).

A regra de confiança é simples:

> Todo componente de transporte é considerado **não confiável** —
> discovery, relay, infraestrutura Tor e nós intermediários.
> A confiança criptográfica existe **entre os endpoints**.

---

## Código organizado por responsabilidade

| Ficheiro | Responsabilidade |
| --- | --- |
| `pipeline.rs` | orquestração K1→K9 (ordem fixa, constantes únicas) |
| `envelope.rs` | formato da mensagem, região assinada |
| `anti_replay.rs` | registo circular de `nonce1` (anti-replay do chat) |
| `handshake.rs` | troca de chaves, anti-replay e anti-downgrade |
| `ipc.rs` | fronteira local (validação total dos pedidos) |
| `p2p.rs` | frames, ligações, rate-limit, encapsulamento relay |
| `lua_camadas.rs` | embedding Lua (K2/K3/K6/K8) |
| `ffi_c.rs` | FFI para C/C++ (K4/K9) |
| `tor.rs` / `tor_arti.rs` | abstração de rede e o seu único implementador real |
| `erros.rs` | códigos de erro do protocolo |
| `secret.rs` | contentor `Segredo` com `zeroize` |

---

## Documentos relacionados

| Documento | Conteúdo |
| --- | --- |
| [`SYS_GUIDE.md`](SYS_GUIDE.md) | visão geral aprofundada e decisões |
| [`USER_GUIDE.md`](USER_GUIDE.md) | manual do utilizador |
| [`DEV_GUIDE.md`](DEV_GUIDE.md) | guia do desenvolvedor |
| [`threat_model.md`](threat_model.md) | adversários e o que cada um pode |
| [`security_model.md`](security_model.md) | propriedades pretendidas vs testadas |
| [`privacy_model.md`](privacy_model.md) | conteúdo, identidade, IP, metadados |
| [`pipeline.md`](pipeline.md) | especificação K1→K9 |
| [`message_format.md`](message_format.md) | envelope |
| [`handshake.md`](handshake.md) | troca de chaves |
| [`key_management.md`](key_management.md) | ciclo de vida das chaves |
| [`ipc_spec.md`](ipc_spec.md) | protocolo local |
| [`discovery.md`](discovery.md) | bootstrap `ID → .onion` |
| [`p2p.md`](p2p.md) | frames ponto a ponto e limites |
| [`relay.md`](relay.md) | transporte opcional |
| [`test_vectors.md`](test_vectors.md) | vectors oficiais multi-linguagem (gerado) |
| [`testing.md`](testing.md) | categorias de testes |
| [`roadmap.md`](roadmap.md) | implementado / planeado / conceito |
