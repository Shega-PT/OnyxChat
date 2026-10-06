# Estratégia de Testes

## Categorias

| Categoria | O que valida | Onde vive |
| --- | --- | --- |
| **Unit tests** | comportamento isolado de uma função/módulo | `#[cfg(test)]`, `tests/*.py`, CTest |
| **Integration tests** | componente + dependências reais (daemon, UDS, relay) | `network/daemon_rust/tests/`, `tests/test_ipc_client.py` |
| **Cross-language tests** | que implementações diferentes produzem os mesmos bytes | `tests/vectors/*.json` consumidos por Rust, Python e C/C++ (Lua via `mlua`) |
| **Protocol tests** | cumprimento exato da spec (tamanhos, offsets, códigos) | testes de `envelope.rs`, `handshake.rs`, `ipc.rs`, `test_relay.py` |
| **Property tests** | propriedades válidas para *todos* os inputs de um domínio | `proptest` (Rust), `hypothesis` (Python) |
| **Fuzz tests** | que parsers nunca crasham/panicam com input arbitrário | `fuzz/` (cargo-fuzz) |
| **E2E tests** | sistema completo (dois daemons, relay real, binário real) | `tests/test_e2e_*.py` |
| **Coverage** | que linhas foram executadas | `llvm-cov`, `gcovr`, `coverage.py` |

A cobertura **não** é medida de segurança: demonstra execução, não
correção (`security_model.md` §Cobertura).

---

## Como correr

> **Todos os comandos Rust usam `--no-default-features`.** A feature
> `tor-real` **não** é feature de omissão, de propósito: é a única razão
> pela qual um build exigiria ~458 das 548 crates do projecto e vários GB
> de RAM. Os testes usam `TorFalso` (`--tor nenhum`) e não precisam de
> Tor. Ver [`DEV_GUIDE.md`](DEV_GUIDE.md) §2.3.

Todos os comandos Rust passam por [`scripts/memoria.sh`](../scripts/memoria.sh),
que os põe dentro de uma scope de cgroup com tecto de memória. Chamar o
`cargo` directamente **não tem tecto**, e é assim que o editor foi
fechado por OOM — ver §Gate de hardware.

```bash
# Estado da memória e decisão que o gate tomaria. Não constrói nada.
./scripts/memoria.sh

# Workspace Rust (unit + integração + property tests) — 103 crates
# A feature `docs` é necessária a `tests/vetores.rs`.
./scripts/memoria.sh --pico -- cargo test --workspace \
    --no-default-features --features docs

# Verificar que tor_arti.rs continua a compilar — 458 crates, ~9 min,
# com tecto. Este é o passo que provocou os OOM kills.
./scripts/memoria.sh --pico -- cargo check -p onyxchatd --features tor-real

# C/C++ (CTest) — sem gate: o compilador C pede muito menos que o rustc
cmake -S crypto/c_cpp -B crypto/c_cpp/build && cmake --build crypto/c_cpp/build
ctest --test-dir crypto/c_cpp/build

# Python (pytest + cobertura, meta 100% de linhas)
.venv/bin/pytest --cov

# E2E com o binário real (TorFalso — sem rede)
./scripts/memoria.sh --pico -- cargo build --no-default-features
.venv/bin/pytest tests/test_e2e_daemon.py tests/test_e2e_rede.py

# Cobertura Rust — --no-default-features é obrigatório
./scripts/memoria.sh --pico -- cargo llvm-cov --workspace \
    --no-default-features --ignore-filename-regex 'tor_arti'

# Fuzz (smoke)
cargo fuzz run envelope_parser -- -runs=10000
```

Ou num comando, que é o caminho recomendado: `./scripts/testar.sh`
(matriz 1–5) e `./scripts/testar.sh --cobertura` (acrescenta as
coberturas). O script aplica o gate a todos os passos Rust.

### O que `--ignore-filename-regex` não faz

Ele exclui um ficheiro do **relatório**. Não o exclui da **compilação**.
Para que a cobertura corra sobre 103 crates em vez de 548, é
`--no-default-features` que faz o trabalho. Usar apenas a regex continua
a pagar o build completo.

### Gate de hardware

A tabela abaixo foi **medida**, não estimada. Todos os valores vêm de
`./scripts/memoria.sh --pico`, que põe cada build dentro de uma scope de
cgroup e lê o `memory.peak` de dentro dela.

Máquina de medição: i3-5005U, **2 núcleos físicos / 4 threads**, **3,8 GB
de RAM**, `opt-level = 0`, `debug = line-tables-only`, `-j1`.

| build | crates | pico de RAM | tempo |
| --- | --- | --- | --- |
| `cargo check --workspace --no-default-features` | 103 | 265 MB | 5 s (incremental) |
| `cargo test --workspace --no-default-features --features docs` | 103 | **547 MB** | ~45 s (incremental) |
| `cargo check -p onyxchatd --features tor-real` | 458 | **1156 MB** | ~7 min |

O número que interessa é o último: **os 458 crates da arti precisam de
1,1 GB**, com tecto de 2,6 GB + 1,5 GB de swap. Cabe, mas por uma
margem que desaparece se o editor consumir a máquina ao mesmo tempo.

O workspace leve desceu de 881 MB para 547 MB depois de três correcções
— `MAX_PAYLOAD` derivado de um orçamento em vez de 16 MiB escritos à
mão, um interpretador Lua reusado por thread em vez de 8 por ciclo do
pipeline, e `opt-level = 2` para o pacote `lua-src`. Ver
[`DEV_GUIDE.md`](DEV_GUIDE.md) §1.2 para a tabela detalhada e para a
razão pela qual três dos testes alocavam 16 MB na pilha.

#### Os limites de payload deixaram de ser literais

`MAX_PAYLOAD` e `TAM_MAX_CORPO` eram `16 * 1024 * 1024`, escritos à
mão em quatro sítios (dois em Rust, um em Python, um no relay). O maior
corpo que o protocolo produz é de 65 820 B num `DECODE` — o limite era
**255× maior** do que o necessário, o que não é margem defensiva mas
superfície de ataque.

São agora derivados de um orçamento de 1 GB
(`network/daemon_rust/src/orcamento.rs`), com override por
`ONYXCHAT_MAX_PAYLOAD` para quem precisar de mais. Um override inválido
não impede o arranque: o daemon usa o valor derivado e **avisa no
arranque** o que foi recusado e porquê.

#### A tabela por RAM total não serve

As tabelas por RAM total («≥ 8 GB: livre, 4–8 GB: `-j2`») são
enganadoras, e era essa a regra que causava o problema. Uma máquina de
3,8 GB com o editor aberto tinha `MemAvailable` de 548 MB: tem 3,8 GB e
não tem 3,8 GB livres. A regra certa é sobre **`MemAvailable` menos o
crescimento esperado do editor**, não sobre a RAM instalada.

Depois de remover o `rust-analyzer`, o `MemAvailable` subiu de 548 MB
para cerca de 1,4 GB — e com o pico do workspace leve em 547 MB, o
orçamento de 1 GB que o gate exige passa com folga.

#### O `-j` não protege o editor

Três OOM kills do kernel em 2026-10-05, com `jobs = 2` configurado:

| quando | vítima | RSS da vítima | RSS do `cargo` | RSS do `rustc` |
| --- | --- | --- | --- | --- |
| out. 04 17:32 | `rust-analyzer` | 680 MB | — | — |
| out. 04 18:25 | `rust-analyzer` | 446 MB | — | — |
| out. 05 16:44 | `rust-analyzer` | **1014 MB** | 26 MB | 39 + 41 MB |

O `-j` estava correcto e o editor morreu na mesma, três vezes. Reduzir o
paralelismo muda *qual* processo o kernel mata quando a memória acaba,
não se a memória acaba. Quem protege o editor é o **tecto de cgroup**:
com `MemoryMax` e `OOMPolicy=kill`, o OOM ocorre dentro da scope do
build e mata o `rustc`. É o que `scripts/memoria.sh --pico` faz.

#### O passo da arti

`tor_arti.rs` fica por verificar por `cargo check` — compilar,
optimizar e instrumentar 458 crates para um ficheiro que só toca a rede
real não é um bom negócio em hardware limitado. É também o passo que
escreveu 498 fingerprints às 16:43 e provocou o OOM das 16:44. Continua
no modo por omissão, com tecto.

---

## Test vectors (`test_vectors.md`)

Fonte única de verdade para paridade entre linguagens:

```text
tests/vectors/camadas.json    →  KATs por camada (K1..K9)
tests/vectors/pipeline.json   →  vetores fim-a-fim (c1..c9 + envelope)
tests/vectors/handshake.json  →  REQUEST/ACCEPT/REJECT
docs/test_vectors.md          →  forma legível dos mesmos dados

Consumidores:
  Rust    network/daemon_rust/tests/vetores.rs    (K1..K9, pipeline,
                                                    handshake; Lua via mlua)
  Python  tests/test_vectors.py                   (K2/K3/K6/K8, envelope,
                                                    handshake)
  C/C++   crypto/c_cpp/tests/test_vectors.c       (K4 e K9)
```

Cada vector contém: `input`, `key`, `parameters` e `expected output`
por camada, mais o vector do pipeline completo (envelope fechado) e
os corpos de handshake (209/177/81 bytes).

**Gerador:** `cargo run -p onyxchatd --example gerar_vetores` —
regenera os JSON a partir da implementação de referência (nunca edite
JSON à mão).

Cobertura obrigatória: K1..K9 individuais + pipeline completo +
envelope + handshake.

---

## Propriedades obrigatórias

```text
decode(encode(x)) == x                    (round-trip, por camada)
alterar ciphertext      → falha
alterar assinatura      → falha
alterar nonce assinado  → falha
alterar versão          → falha
mensagem truncada       → falha
envelope abaixo do mínimo → falha
```

Toda a alteração de um campo autenticado tem de resultar em **falha**,
nunca em aceitação parcial.

---

## Testes de corrupção (matriz obrigatória)

| Vetor de corrupção | Resultado esperado |
| --- | --- |
| ciphertext alterado | rejeição segura (tag) |
| nonce alterado | rejeição (assinatura/tag) |
| assinatura alterada | rejeição (`AssinaturaInvalida`) |
| versão alterada | rejeição (`EnvelopeInvalido`) |
| comprimento inválido | rejeição (`EnvelopeCurto`/`Grande`) |
| padding inválido | rejeição (`PaddingInvalido`) |
| chave errada | rejeição (tag) |
| mensagem truncada | rejeição |

**Resultado esperado = rejeição segura.** Nunca *crash*, nunca
*plaintext parcial*.

---

## Interoperabilidade

Sempre que existem implementações correspondentes, há teste de
paridade:

| Par | O que compara |
| --- | --- |
| Lua ↔ Python | K2, K3, K6, K8 (mesmos hex) |
| Rust ↔ C | K9 (`crypto_core` vs libsodium) |
| Rust ↔ Python | K7, envelope |
| C ↔ Python | K4 |
| Python ↔ Rust daemon | IPC completo (E2E) |
| daemon ↔ daemon | frames P2P e relay (E2E de rede) |

Objetivo: garantir que a arquitetura multi-linguagem não introduz
diferenças **silenciosas**.

---

## Fuzzing

Alvos (parsers que recebem dados externos):

```text
envelope_parser    docs/message_format.md
ipc_parser         framing UDS
handshake_parser   FRIEND_*
relay_parser        frames do relay
k4_decoder         padding/transposição
k7_decoder         Hill/PKCS#7
```

Objectivo: crashes, panics, loops, overflow, parsing inconsistente.
Complementa, não substitui, os testes tradicionais.

Cada alvo vai **além** de «não crashar» — verifica uma propriedade de
round-trip sobre o input aceite (reenquadrar reproduz os bytes
consumidos; cifrar(decifrar(x)) == x; um envelope aceite
re-serializa-se byte a byte igual).

### Corpora de seed

Os corpora são **gerados** por `cargo vectors` a partir de
`tests/vectors/*.json` e versionados (`fuzz/.gitignore` deixa de os
ignorar). Sem seeds válidas, o libFuzzer não ganha profundidade nos
ramos que interessam (envelope válido, corpo de handshake válido,
limites exactos).

Antes desta mudança, `fuzz/.gitignore` ignorava `corpus` por inteiro: um
clone começava com os 6 alvos sem uma única seed, e `envelope_parser` e
`handshake_parser` estavam ambos a zero.

```bash
# regenerar os JSON, o documento legível e os corpora
cargo vectors
```

---

## Cobertura

| Linguagem | Ferramenta | Meta | Medido |
| --- | --- | --- | --- |
| Rust | `cargo-llvm-cov` | 100% de linhas | **99,79%** (12 linhas) |
| C/C++ | `gcovr` | 100% de linhas | **100%** (62/62) |
| Python | `coverage.py` | 100% de linhas | **100%** (1224/1224) |

**A meta é 100%. O Rust não a atinge, e a diferença está declarada
abaixo.** Uma tabela que diz «100%» sem relatório é uma afirmação, não
uma medição.

### Execução registada

```text
Data:     2026-10-05 12:26 UTC
Rust:     rustc 1.98.1 (48a229cea 2026-09-01)
Python:   3.12.3
CMake:    3.28.3
```

| Matriz | Resultado |
| --- | --- |
| `cargo test --workspace --no-default-features` | **309 ok**, 0 falhados (244 no `onyxchatd` + 65 no `crypto_core`) |
| `ctest --test-dir crypto/c_cpp/build` | **3/3 ok** |
| `.venv/bin/pytest --cov` | **278 ok**, cobertura 100,00% |
| `.venv/bin/pytest tests/test_e2e_*.py` | **17 ok** (incluídos nos 278) |
| `cargo llvm-cov … --no-default-features` | 99,79% de linhas |

### As 12 linhas por cobrir, e porquê

Não são testes em falta: são ramos **inalcançáveis por construção**.

| Ficheiro:linha | Ramo | Porque é inalcançável |
| --- | --- | --- |
| `paginas.rs:220` | `getrlimit` falha | `RLIMIT_MEMLOCK` é sempre legível num Linux normal |
| `paginas.rs:228` | limite `RLIM_INFINITY` | exige `root` para fixar o limite *soft* a infinito |
| `paginas.rs:266` | `setrlimit` recusado | exige `CAP_SYS_RESOURCE` |
| `paginas.rs:327,360` | `panic!` de «não consegui forçar a falha» | ramo do `else` dos testes de degradação |
| `paginas.rs:441-444` | `limite_memlock()` devolve `None` | ver linha 220 |
| `ipc.rs:343` | `partes_fixas` falha em `DECODE` | o `len` já foi validado acima — o ramo é defesa em profundidade |
| `ipc.rs:1082` | `continue` por uid estrangeiro | exige um segundo utilizador real no host |

As duas últimas são **fail-closed por construção**: o ataque é rejeitado
um passo antes, e o ramo existe para o caso de o check anterior mudar.
Retirá-las reduziria a defesa; cobri-las exigiria root ou um segundo
utilizador.

**Exclusões declaradas:**

* `tor_arti.rs` — único ficheiro que toca a rede real; não é testável
  offline. A compilação é assegurada por `cargo check --features
  tor-real`, e o isolamento por um teste de guarda
  (`docs/testing.md` §Fuzzing).
* As 12 linhas da tabela acima — ramos defensivos inatingíveis.
* `branch = false` em Python — a meta é de linhas, não de ramos.

**A cobertura não é usada como argumento de segurança.** Demonstração:
um bug de protocolo pode viver em linhas 100% cobertas, e os 99,79%
dizem-nos exactamente que linhas *não* foram executadas — o que é mais
informação do que «100%».

---

## Testes que sustentam as propriedades de segurança

| Propriedade | Teste |
| --- | --- |
| verificação antes da decifragem | `assinatura_adulterada` (`pipeline.rs`) |
| anti-downgrade do envelope | `versao_desconhecida` + property "alterar versão → falha" |
| anti-downgrade do handshake | `pedido_downgrade_de_versao` (`handshake.rs`) + downgrade no aceite (`aceite_invalidos`) |
| sessão `HELLO` obrigatória | `pedidos_antes_do_hello_dao_0x02` (`ipc.rs`) + `test_hello_obrigatorio_no_daemon_real` |
| versão do IPC no `HELLO` | `hello_versao_incompativel_0x12` (`ipc.rs`) + `test_hello_versao_incompativel_no_daemon_real` |
| uid do par (`SO_PEERCRED`) | `uid_do_par_do_socket_e_o_atual` + `outro_uid_e_rejeitado_pelo_comparador` (`ipc.rs`) |
| anti-replay handshake | `nonce_visto` / `NonceRepetido` (`handshake.rs`) |
| anti-replay chat | `anti_replay_rejeita_reenvio` (`pipeline.rs`) + `decode_nonce_repetido` (`ipc.rs`) |
| **isolamento Tor** | teste de guarda `arti::` fora de `tor_arti.rs` — `network/daemon_rust/tests/guarda_tor.rs` |
| **falha explícita sem `tor-real`** | `construir_backend` devolve `Err` (`main.rs`) |
| permissões do UDS | teste `0600` + teste de uid |
| limites de tamanho | teste por constante (`MAX_PLAINTEXT`/`MAX_ENVELOPE`/`MIN_PLAINTEXT`) |
| fronteira P2P (limites/desalinhamento) | `receber_comprimento_gigante_derruba_a_ligacao` (`ipc.rs`) + `limpar_se_morta_classifica_erros` |
| zeroização de segredos (§22) | `chaves_sao_zeroizadas_no_drop`, `pedido/aceite/amizade_zeroiza_chaves_no_drop`, `pedido_pendente_zeroiza_corpo` |
| `mlockall` e a sua degradação | `guarda_tor.rs` / `main.rs` — caminho com e sem `RLIMIT_MEMLOCK` |
| erros sem vazamentos (§56) | `erros_de_ipc_nunca_ecoam_os_dados_do_pedido` (`ipc.rs`) |
| `docs/test_vectors.md` não divergiu dos JSON | `test_vectors_md_bate_com_os_json` (`tests/vetores.rs`) |
