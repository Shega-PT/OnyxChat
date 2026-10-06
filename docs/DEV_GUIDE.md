# Guia do Desenvolvedor — OnyxChat

> Documento de trabalho. Explica como compilar, testar, alterar e
> publicar o OnyxChat sem repetição de erros e sem adivinhações.
>
> Para *usar* o sistema: [`USER_GUIDE.md`](USER_GUIDE.md) ·
> Para *o compreender*: [`SYS_GUIDE.md`](SYS_GUIDE.md) ·
> Para os *bytes*: os specs normativos, via [`index.md`](index.md)

---

## 1. Ambiente e pré-requisitos

### 1.1 Ferramentas

| Ferramenta | Versão | Para quê |
| --- | --- | --- |
| `rustc` / `cargo` | ≥ 1.98 | `crypto_core` e `onyxchatd` |
| `cmake`, `g++` | ≥ 3.16 | K4 (transposição), libsodium estática |
| Python | ≥ 3.10 | cliente, CLI, discovery, relay |
| `cargo-llvm-cov` | — | cobertura Rust |
| `gcovr` | — | cobertura C/C++ |
| `pytest`, `hypothesis`, `pytest-cov` | ≥ 8 / ≥ 6 / ≥ 5 | testes Python |
| `cargo-fuzz` + toolchain nightly | — | fuzzing (opcional) |

Instalação Python (o projecto **não tem dependências de runtime**):

```bash
python3 -m venv .venv
.venv/bin/pip install -e '.[dev]'
```

### 1.2 A janela de hardware — leia antes de compilar

> **Use sempre `scripts/memoria.sh`.** Chamar o `cargo` directamente não
> põe tecto de memória, e sem tecto o kernel escolhe a vítima do OOM
> pelo `oom_score` — que nesta máquina foi o `rust-analyzer`, três
> vezes.

```bash
./scripts/memoria.sh          # estado da memória; não constrói nada
./scripts/testar.sh           # a matriz completa, já confinada
```

#### O que foi medido

Todos os valores vêm de `./scripts/memoria.sh --pico`, que põe cada
build dentro de uma scope de cgroup v2 e lê o `memory.peak` de dentro
dela. Máquina: i3-5005U, 2 núcleos físicos / 4 threads, 3,8 GB de RAM.

| build | crates | pico de RAM |
| --- | --- | --- |
| `check --workspace --no-default-features` (incremental) | 103 | 265 MB |
| `test --workspace --no-default-features --features docs` | 103 | **547 MB** |
| `check -p onyxchatd --features tor-real` | 458 | **1156 MB** |

O workspace leve foi medido em **547 MB** depois de três correcções
(descritas abaixo), contra os 881 MB que tinha antes. A árvore da arti
continua em 1156 MB: o pico dela é dominado pelos crates pesados de
streaming (tokio, rustls, zstd, icu), que as correcções não tocam.

#### Porque é que o workspace leve desceu de 881 MB para 547 MB

Três correcções, nesta ordem:

| correcção | o que era | o que passou a ser |
| --- | --- | --- |
| `orcamento.rs` | `MAX_PAYLOAD = 16 MiB` escrito à mão em 4 sítios | derivado de um orçamento de 1 GB, com override por `ONYXCHAT_MAX_PAYLOAD` |
| `lua_camadas.rs` | 8 interpretadores Lua por ciclo do pipeline | 1 por thread, reusado |
| `Cargo.toml` | `lua-src` herdava `opt-level = 0` | `opt-level = 2` por pacote |

A primeira foi a maior. Três testes alocavam `[0u8; 16_777_217]` — um
array na **pilha**, contra um `ulimit -s` de 8 MB. Isso não dá erro de
alocação: dá *stack overflow* e aborta o binário de testes inteiro. Um
teste que mata a suite não informa, e o pico que os testes impunham
ao build era de 16 MiB por thread de teste, em paralelo.

#### A superfície de exaustão de memória fechou com a mesma correcção

O limite de 16 MiB era **255× maior** do que o maior corpo que o
protocolo produz (65 820 B num `DECODE`). Não era margem defensiva,
era superfície: `ler_pedido` alocava `vec![0u8; comprimento]` **antes
de ler um byte**, e `thread::spawn` aceitava ligações sem limite. Um
cliente do mesmo uid — que é tudo o que `SO_PEERCRED` exige — podia
abrir N ligações e reservar N × 16 MiB sem escrever nada, bastando
reenviar 4 bytes de cabeçalho.

Agora: o limite é derivado do orçamento, o comprimento é validado antes
da alocação, e as ligações concorrentes estão limitadas a 32
(`ONYXCHAT_LIGACOES_MAX`). Ver `network/daemon_rust/src/orcamento.rs`.

#### Porque é que `-j` não resolvia

Três OOM kills do kernel em 2026-10-05, com `jobs = 2` configurado:

| quando | vítima | RSS da vítima | RSS do `cargo` | RSS do `rustc` |
| --- | --- | --- | --- | --- |
| out. 04 17:32 | `rust-analyzer` | 680 MB | — | — |
| out. 04 18:25 | `rust-analyzer` | 446 MB | — | — |
| out. 05 16:44 | `rust-analyzer` | **1014 MB** | 26 MB | 39 + 41 MB |

No momento do OOM das 16:44 o build era minúsculo: `cargo` a 26 MB e
dois `rustc` a 39 e 41 MB. O que esgotou a máquina foi a soma, e o
`rust-analyzer`, com 1 GB, era o elo mais gordo. Como o OOM killer
escolhe por `oom_score`, matou o editor em vez do build.

A regra anterior desta secção era `jobs <= RAM_em_GB / 1.5`, que dava 2
nesta máquina. Está errada por duas razões independentes: a RAM **total**
não diz o que está **disponível** (com o editor aberto eram 548 MB), e
reduzir o paralelismo muda *qual* processo morre, não se a memória
acaba. A regra certa é sobre `MemAvailable` menos o crescimento esperado
do editor.

#### O que protege o editor

O tecto de cgroup. `scripts/memoria.sh --pico` corre o build dentro de

```bash
systemd-run --user --scope \
    -p MemoryHigh=<80% do tecto> \
    -p MemoryMax=<tecto> \
    -p MemorySwapMax=<tecto> \
    -p OOMPolicy=kill -- cargo ...
```

Com `MemoryMax`, o OOM ocorre **dentro** da scope, e como o `cargo` e
os `rustc` são os únicos lá dentro, o que morre é o build.
`MemoryHigh` abaixo do tecto faz o reclaim começar cedo — e o reclaim é
reversível, ao contrário da alocação. `OOMPolicy=kill` garante que a
scope morre inteira em vez de ficar a trocar.

Verificado nesta máquina: com o tecto posto, um processo que pedisse
2 GB foi morto com código 137 e o `code` do editor continuou a correr
com 1 GB. É este comportamento — build sacrificado, editor vivo — que
o gate existe para produzir.

#### Mitigações já incorporadas

| Medida | Efeito |
| --- | --- |
| **`scripts/memoria.sh --pico`** | **confinement em cgroup: o OOM mata o build, não o editor** |
| `.cargo/config.toml`: `jobs = 1` | piso para quem chama o cargo sem o gate |
| Perfis `dev`/`test` sem optimização | `opt-level = 0` e `codegen-units = 4` reduzem o pico do rustc |
| `debug = "line-tables-only"` | backtraces úteis sem o custo do DWARF completo |
| `tor-real` **não** é feature de omissão | o ciclo de testes compila 103 crates, não 548 |
| `llvm-cov` só sem arti | idem, num directório separado |
| `rust-analyzer` removido | poupou 446 MB a 1 GB; era a vítima nos três OOM |

**Se a sua máquina tiver 4 GB ou menos**, veja ainda
[`SYS_GUIDE.md`](SYS_GUIDE.md) §11.1 e considere zram — ver §1.3.

### 1.3 Afinar o sistema operativo (opcional, requer `sudo`)

Com 3–4 GB de RAM, o `swapfile` em disco é lento demais. Prefira zram
(swap comprimido em RAM):

```bash
sudo apt install systemd-zram-generator zstd
```

Ou um drop-in systemd próprio, com prioridade acima do swapfile:

```ini
# /etc/systemd/system/zram-swap.service
[Unit]
Description=Swap comprimido em RAM (OnyxChat)
Before=swap.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/lib/systemd/systemd-zram-setup@zstd%i
ExecStop=/usr/lib/systemd/systemd-zram-setup@zstd%i stop
Environment=ZRAM_SIZE=2G
Environment=ZRAM_COMPRESSION_ALGO=zstd

[Install]
WantedBy=swap.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now zram-swap.service
```

Ajuste de sysctl (opcional, em `/etc/sysctl.d/`):

```ini
vm.overcommit_memory = 1    # permite alocar além de RAM+swap
vm.swappiness = 10          # descarta cache em vez de trocar
vm.vfs_cache_pressure = 150
```

```bash
sudo sysctl --system
```

> `overcommit_memory = 1` só é seguro depois de a pressão de memória
> estar resolvida pela F0. Com a configuração de build em vigor, o pico
> de RAM dos testes cabe folgado.

---

## 2. Compilar

### 2.1 Perfis

| Perfil | `opt-level` | `debug` | `codegen-units` | Quando |
| --- | --- | --- | --- | --- |
| `dev` | 0 | `line-tables-only` | 4 | desenvolvimento, testes |
| `test` | 0 | `line-tables-only` | 4 | `cargo test` |
| `release` | 3 | por omissão | 16 | **produção** |

Os perfis `dev`/`test` não'optimizam de propósito: a optimização é o
maior consumidor de RAM do rustc. Com `opt-level = 0`, as verificações
`overflow-checks` e `debug-assertions` ficam activas — os testes
apertam **mais**, não menos.

Estes perfis já estavam correctos e ficam como estavam. O que estava
errado era o `jobs = 2` de `.cargo/config.toml` e a ausência de um
tecto de memória — ver §1.2. O `opt-level = 0` e o
`debug = "line-tables-only"` nunca foram a causa dos OOM: com eles, o
pico medido do workspace leve é 547 MB e o da arti é 1156 MB.

Há uma excepção, e é um pacote: `[profile.*.package.lua-src]` tem
`opt-level = 2`. O cargo propaga o `opt-level` do perfil às dependências,
e o `build.rs` do `lua-src` lê `OPT_LEVEL` do ambiente — o que deixava
o interpretador Lua, que executa **quatro das nove camadas do pipeline**,
sem optimização em todos os builds de desenvolvimento e de teste. Código
C compilado por um `build.rs` não é `rustc`, e a economia de RAM que o
`opt-level = 0` compra não se aplica a ele.

### 2.2 Comandos

Todos com o gate de `scripts/memoria.sh`, que põe tecto de memória à
build — ver §1.2.

```bash
# binário de produção — inclui Tor, 458 crates, pico medido 1156 MB
./scripts/memoria.sh --pico -- cargo build --release --features tor-real

# binário de desenvolvimento — sem Tor, para testes (103 crates)
./scripts/memoria.sh --pico -- cargo build

# o binário de testes é Debug; `--features tor-real` só é necessário
# se quiser exercitar o caminho de rede real
```

### 2.3 `tor-real` não é feature de omissão

```toml
# network/daemon_rust/Cargo.toml
[features]
default = []          # ← sem arti
tor-real = ["dep:arti-client", ...]
```

Consequências:

* `cargo test --workspace` e `cargo build` compilam **sem** arti.
* `cargo build --release` (produção) precisa de
  `--features tor-real` explícito.
* `cargo check --features tor-real` na CI garante que `tor_arti.rs`
  continua a compilar, sem pagar o custo completo.

Quando compilar sem arti, o daemon com `--tor arti` **falha com
mensagem explícita** — não degrada em silêncio para um modo sem Tor.
Ver [`SYS_GUIDE.md`](SYS_GUIDE.md) §10.2.

---

## 3. Testes

### 3.1 Matriz completa

```bash
# 1. Rust — unit + integração + property tests (103 crates)
#    Inclui `tests/vetores.rs`, que precisa da feature `docs`.
./scripts/memoria.sh --pico -- cargo test --workspace \
    --no-default-features --features docs

# 2. C/C++ — K4, K9, vectors (sem gate: o gcc pede muito menos que o rustc)
cmake -S crypto/c_cpp -B crypto/c_cpp/build
cmake --build crypto/c_cpp/build
ctest --test-dir crypto/c_cpp/build

# 3. Python — pytest + cobertura (meta: 100% de linhas)
.venv/bin/pytest --cov

# 4. E2E — com o binário real (sem Tor; usa TorFalso)
./scripts/memoria.sh --pico -- cargo build --no-default-features
.venv/bin/pytest tests/test_e2e_daemon.py tests/test_e2e_rede.py

# 5. Tor — verificação de compilação de tor_arti.rs (458 crates, ~7 min)
./scripts/memoria.sh --pico -- cargo check -p onyxchatd --features tor-real
```

Ou, num comando só — e é o caminho recomendado, porque aplica o gate a
todos os passos Rust sem se esquecer de nenhum:

```bash
./scripts/testar.sh               # 1–5
./scripts/testar.sh --cobertura   # acrescenta as coberturas 6–7
```

Os passos 1, 4 e 5 são os que passam pelo gate. O 5 é o que escrevia
498 fingerprints da arti às 16:43 e provocou o OOM das 16:44; continua
no modo por omissão, mas dentro de uma scope com tecto.

### 3.2 Cobertura

```bash
# Rust — tor_arti.rs excluído (único ficheiro com rede real)
./scripts/memoria.sh --pico -- cargo llvm-cov --workspace \
  --no-default-features --ignore-filename-regex 'tor_arti'

# C/C++ — gcovr
cmake -S crypto/c_cpp -B crypto/c_cpp/build -DONYX_COVERAGE=ON
cmake --build crypto/c_cpp/build && ctest --test-dir crypto/c_cpp/build
.venv/bin/gcovr --root . \
  --filter 'crypto/c_cpp/(k9_chacha|transposition)\.(c|cpp)' \
  --exclude 'tests/' crypto/c_cpp/build
```

> `--no-default-features` é obrigatório. `--ignore-filename-regex`
> exclui do relatório, **não** da compilação: usá-lo sozinho continua a
> compilar as 458 crates da arti.

### 3.3 Test vectors — a fonte única de verdade

```text
tests/vectors/camadas.json    KATs por camada (K1..K9)
tests/vectors/pipeline.json   vectors fim-a-fim (c1..c9 + envelope)
tests/vectors/handshake.json  REQUEST/ACCEPT/REJECT
docs/test_vectors.md          forma legível — GERADO
```

Para regenerar (após alterar uma camada ou o formato):

```bash
cargo vectors
#  = cargo run -p onyxchatd --example gerar_vetores \
#      --no-default-features --features docs
```

O gerador escreve **quatro** coisas, todas derivadas dos vectors:

```text
tests/vectors/{camadas,pipeline,handshake}.json   fonte única de verdade
docs/test_vectors.md                             forma legível (GERADO)
fuzz/corpus/<alvo>/NN-<hex>.bin                  seeds do cargo-fuzz
```

> **Nunca editar os JSON nem o `docs/test_vectors.md` à mão.**
>
> Isto deixou de ser uma instrução e passou a ser um mecanismo: a
> renderização do `.md` vive na biblioteca (`docs_vec::gerar_markdown`)
> e `tests/vetores.rs` re-renderiza o documento a partir dos JSON para o
> comparar com o ficheiro em disco. Editar o `.md` à mão faz o teste
> falhar, indicando a linha exacta que divergiu e o comando que corrige.

A feature `docs` dá acesso ao `serde_json` de que a renderização
precisa. É opcional de propósito: um binário de produção não tem de
linkar um parser de JSON para gerar um documento em tempo de build.

### 3.4 Fuzzing

```bash
# smoke rápido
cargo fuzz run envelope_parser -- -runs=10000

# alvo individual
cargo fuzz run k7_decoder -- -runs=60000
```

O crate `fuzz/` está **excluído** do workspace (precisa de nightly e
sanitizadores). Depende de `onyxchatd` com `default-features = false`
— os alvos nunca tocam na rede real.

Os corpora de seed são gerados por `cargo vectors` a partir dos
vectors oficiais, e versionados. Sem seeds válidas — um envelope bem
formado, um corpo de handshake bem formado, os limites exactos — o
libFuzzer não ganha profundidade nos ramos onde os parsers falham.

### 3.5 Onde está o quê

| Tipo | Ficheiro |
| --- | --- |
| Test vectors multi-linguagem | `tests/vectors/*.json` |
| Property tests Rust | `network/daemon_rust/tests/propriedades.rs`, `crypto/rust/tests/propriedades.rs` |
| Property tests Python | `tests/test_propriedades.py` |
| Testes do binário | `network/daemon_rust/tests/daemon_binario.rs` |
| E2E | `tests/test_e2e_daemon.py`, `tests/test_e2e_rede.py` |

### 3.6 Onde ficam os testes unitários

Os testes unitários do daemon **não** vivem no fim do ficheiro que
testam. Vivem em `<módulo>_testes.rs`, no mesmo directório, e o módulo
declara-os como filho:

```rust
#[cfg(test)]
#[path = "ipc_testes.rs"]
mod testes;
```

`ipc.rs`, `p2p.rs` e `handshake.rs` — os três com mais de mil linhas de
produção — seguem esta convenção. Os módulos mais pequenos
(`envelope.rs`, `pipeline.rs`, `tor.rs`, …) mantêm o `mod tests` no
próprio ficheiro: abaixo de ~300 linhas de produção a separação custaria
mais a seguir do que dá.

`#[path]` e não `mod` no `lib.rs` por uma razão concreta: os testes
precisam do que é **privado** — `processar_pedido`, `bloquear`,
`Estado::novo`, `ligacao_de_teste`. Um módulo só vê o privado do pai e
dos seus ancestrais; por isso têm de ser filhos. Um `tests/` de
integração vê apenas `pub`, e mover estes testes para lá obrigaria a
tornar público o interior do daemon — trocando legibilidade por
encapsulamento, o trade ao contrário.

Ao acrescentar testes a um módulo grande, ponha-os em
`<módulo>_testes.rs` com `use super::*;`. Ao acrescentar a um módulo
pequeno, ponha-os no `mod tests` do próprio ficheiro.

---

## 4. Convenções de código

### 4.1 Linguagem

O projecto é **integralmente em português europeu** — nomes de
ficheiros, variáveis, funções, tipos, mensagens de erro e comentários.
Não introduza nomes em inglês. Exceção: os termos técnicos
consagrados (`nonces`, `padding`, `handshake`, `relay`, `discovery`,
`routing`, `property tests`, `fuzzing`) mantêm-se.

### 4.2 Rust

* `Result<T, ErroX>` em toda a API pública — nunca `panic!` em input
  externo.
* Erros tipados por `enum`, com conversão explícita para códigos IPC
  (`erros.rs`).
* Segredos via `Segredo<T>` (`secret.rs`), que faz `zeroize` no `Drop`.
* Doc-comments em português; o cabeçalho de cada módulo explica o
  *porquê*, não o *o quê*.
* Nada de `unsafe` fora de `ffi_c.rs`, e lá apenas sobre FFI de C.

### 4.3 Python

* Tipos explícitos nas funções públicas; `from __future__ import
  annotations`.
* Erros de domínio como `Exception`/`ValueError` com nome próprio
  (`ErroIpc`, `EnvelopeInvalido`, `ConfigInvalida`).
* Docstrings em português; `# pragma: no cover` só no `__main__`.
* Zero dependências de runtime.

### 4.4 C/C++

* Sem alocação dinâmica nas camadas (saída calculada por índice).
* `sodium_memzero` / `explicit_bzero` em buffers de material sensível —
  **ainda por fazer**: `crypto/c_cpp` não chama nenhuma das duas.
* Erros via `int` (constantes `ONYX_ERR_*`), nunca por valor de retorno
  ambíguo.

### 4.5 Comentários

Comentários explicam **porquê** e as **decisões**, nunca o óbvio. Um
comentário que repete o código envelhece mal e é ruído. Ao alterar
código com um porque, actualize o comentário e o spec.

---

## 5. Mapa: onde tocar

| Mudança | Ficheiros | Spec a actualizar |
| --- | --- | --- |
| Alterar uma camada K1–K9 | `crypto/rust/src/`, `crypto/c_cpp/`, `crypto/python_lua/` | `pipeline.md` §K<n> |
| Alterar o envelope | `network/daemon_rust/src/envelope.rs`, `messenger/envelope.py` | `message_format.md` |
| Alterar o handshake | `network/daemon_rust/src/handshake.rs`, `messenger/ipc_client.py` | `handshake.md` |
| Alterar o protocolo IPC | `network/daemon_rust/src/ipc.rs`, `messenger/ipc_client.py` | `ipc_spec.md` |
| Alterar frames P2P | `network/daemon_rust/src/p2p.rs` | `p2p.md` |
| Alterar o relay | `server/relay_server.py`, `network/daemon_rust/src/p2p.rs` | `relay.md` |
| Alterar o discovery | `server/discovery_server.py` | `discovery.md` |
| Alterar chaves/keystore | `messenger/storage.py`, `messenger/keys.py`, `user/config.py` | `key_management.md` |
| Alterar gestão de segredos | `crypto/rust/src/secret.rs`, `network/daemon_rust/src/ipc.rs` | `key_management.md` |
| Alterar Tor | `network/daemon_rust/src/tor.rs`, `tor_arti.rs` | `architecture.md`, `security_model.md` |
| Adicionar um subcomando | `user/cli.py` + `tests/test_cli.py` | `USER_GUIDE.md` §11 |

Ao alterar uma camada ou o formato, **regenerar os vectors** (§3.3) e
actualizar a spec correspondente. São as duas coisas que os testes
cross-linguagem vão verificar.

---

## 6. Regras editoriais da documentação

### 6.1 Os três rótulos

Toda a documentação distingue:

```text
IMPLEMENTADO   existe, funciona e está testado
PLANEADO       decidido, especificado, ainda sem código
CONCEITO       investigação, sem especificação formal
```

Um documento que descreva routing nodes, discovery distribuído,
Fake-IP ou multimédia está a descrever `PLANEADO` ou `CONCEITO`.
Nunca `IMPLEMENTADO`.

### 6.2 Onde vai o quê

| Tipo de conteúdo | Ficheiro |
| --- | --- |
| Visão geral do sistema | `SYS_GUIDE.md` |
| Uso pelo utilizador | `USER_GUIDE.md` |
| Fluxo de trabalho do devs | este documento |
| Estrutura técnica | `architecture.md` |
| Bytes, offsets, códigos | os specs normativos |
| O que está prometido | `threat_model.md`, `security_model.md`, `privacy_model.md` |
| Estado | `roadmap.md` |

Se um facto já está escrito num nível mais profundo, os níveis mais
rasos **remetem** — não repetem. Repetição divergente é a forma mais
comum de documentação falsehood.

### 6.3 Ao mudar um spec

1. Actualize o spec.
2. Se mudou bytes, regenere os vectors (§3.3).
3. Se mudou a CLI, actualize `USER_GUIDE.md`.
4. Se mudou um comando de build/teste, actualize este documento **e**
   o `README.md`.
5. Actualize o `roadmap.md` se mudou o estado.

---

## 7. Checklist de pull request

```text
[ ] ./scripts/testar.sh                     # aplica o gate de memória a todos os passos Rust
[ ] ctest --test-dir crypto/c_cpp/build
[ ] .venv/bin/pytest --cov
[ ] tor_arti.rs compila                     # incluído no passo 5 do testar.sh
[ ] vectors regenerados, se tocou em bytes
[ ] docs actualizados (spec + guias relevantes)
[ ] roadmap.md reflecte o novo estado
[ ] sem TODOs nem código morto introduzido
```

`./scripts/testar.sh` é o comando único que vale para os três primeiros
itens: ele corre a matriz, e cada passo Rust passa por
`scripts/memoria.sh --pico`. Correr `cargo` à mão deixa o build sem
tecto — ver §1.2.

---

## 8. Limpeza de disco

Os builds podem ocupar vários GB. Para libertar:

```bash
# artefactos de build
cargo clean
rm -rf crypto/c_cpp/build

# fuzzing (só se não em curso)
rm -rf fuzz/target
```

A cobertura usa um directório separado (`target/llvm-cov-target`). É
descartável entre execuções.

---

## 9. Documentos relacionados

| Documento | Para quê |
| --- | --- |
| [`SYS_GUIDE.md`](SYS_GUIDE.md) | compreender a arquitectura e as decisões |
| [`USER_GUIDE.md`](USER_GUIDE.md) | usar o sistema |
| [`architecture.md`](architecture.md) | estrutura técnica por módulo |
| [`testing.md`](testing.md) | estratégia de testes detalhada |
| [`roadmap.md`](roadmap.md) | estado do projecto |
