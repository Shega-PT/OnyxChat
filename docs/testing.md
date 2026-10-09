# Estratégia de Testes

## Categorias

| Categoria | O que valida | Onde vive |
| --- | --- | --- |
| **Unit tests** | comportamento isolado de uma função/módulo | `#[cfg(test)]`, `tests/*.py`, CTest |
| **Integration tests** | componente + dependências reais (daemon, UDS, relay) | `network/daemon_rust/tests/`, `tests/test_ipc_client.py` |
| **Cross-language tests** | que implementações diferentes produzem os mesmos bytes | `tests/vectors/*.json` consumidos por Rust, Python e C/C++ (Lua via `mlua`) |
| **Protocol tests** | cumprimento exato da spec (tamanhos, offsets, códigos) | testes de `envelope.rs`, `handshake.rs`, `ipc.rs`, `test_relay.py` |
| **Property tests** | propriedades válidas para *todos* os inputs de um domínio | `proptest` (Rust), `hypothesis` (Python) |
| **Fuzz tests** | que parsers nunca crasham/panicam com input arbitrário | `fuzz/` (cargo-fuzz, nightly) — `./scripts/testar.sh --fuzz` |
| **E2E tests** | sistema completo (dois daemons, relay real, binário real) | `tests/test_e2e_*.py` |
| **Coverage** | que linhas foram executadas | `llvm-cov`, `gcovr`, `coverage.py` |

A cobertura **não** é medida de segurança: demonstra execução, não
correção (`security_model.md` §Cobertura).

---

## Os portões de entrega

Nenhum destes scripts é opcional. Correm todos em [`../scripts/testar.sh`](../scripts/testar.sh),
e um que não corre não protege nada.

| Portão | O que apanha |
| --- | --- |
| [`verificar_gitignore.sh`](../scripts/verificar_gitignore.sh) | O que está e o que não está versionado; os artefactos de fuzzing |
| [`verificar_requisitos.sh`](../scripts/verificar_requisitos.sh) | Ferramentas em falta e versões abaixo do mínimo |
| [`verificar_estrutura.py`](../scripts/verificar_estrutura.py) | A `Estrutura.txt` e o `UI/README.md` a afirmar ficheiros que não existem, **e a afirmar contagens erradas** de ficheiros, documentos, componentes e ecrãs |
| [`verificar_roadmap.py`](../scripts/verificar_roadmap.py) | Um `PLANEADO` sem especificação — que, pela definição, é `CONCEITO` |
| [`verificar_ambiente.py`](../scripts/verificar_ambiente.py) | A documentação ou os comentários a falar da máquina de quem escreve, ou de um momento que já passou |
| [`verificar_seguranca.sh`](../scripts/verificar_seguranca.sh) | Segredos no histórico, advisories de Rust e de JavaScript, licenças copyleft e a integridade dos próprios workflows |
| [`verificar_portugues.py`](../scripts/verificar_portugues.py) | Americanismos em código e documentos, em 14 linguagens |
| [`verificar_frases.py`](../scripts/verificar_frases.py) | Sequências que não são de português |
| [`verificar_comentarios.py`](../scripts/verificar_comentarios.py) | Americanismos só na prosa, sem apanhar identificadores |
| [`verificar_alfabeto.py`](../scripts/verificar_alfabeto.py) | Caracteres fora do repertório português |
| [`verificar_imports.py`](../scripts/verificar_imports.py) | Imports nomeados que não existem no módulo de destino |

### As contagens dos documentos são verificadas

Um mapa pode não nomear um ficheiro que passou a existir — é um índice
seleccionado, e isso é legítimo. Mas um documento que diz «20
documentos» está a afirmar uma **aritmética**, e uma aritmética errada é
pior do que nenhuma: quem a usa para saber o tamanho de uma coisa recebe
um número, não uma dúvida.

Por isso [`verificar_estrutura.py`](../scripts/verificar_estrutura.py)
confere as contagens das duas árvores com que o projecto tem —
`Estrutura.txt` e o `UI/README.md`. Numa auditoria apanhou quatro: 19
documentos (são 20, e faltava `conta.md` na lista), 8 ecrãs (são 11), 24
componentes (são 45) e um ficheiro a menos em `components/ui/`.

Duas decisões que os testes fixam, porque ambas deram erro primeiro:

* **A unidade de indentação é medida, não constante.** `Estrutura.txt`
  indenta com 4 colunas e `UI/README.md` com 3. Com a constante embutida,
  `3 // 4` dava zero, e `components/onyx/` era procurado onde não está —
  devolvendo zero **sem erro nenhum**.
* **O que não é contável a partir do disco fica por medir.** `crates`,
  `alvos` e `corpora` só um `cargo metadata` ou um `ls` de um directório
  crate os dá; preferimos não conferir a conferir mal.

### A regra que não depende da máquina

Um pico de 931 MB é uma propriedade do **build** — do grafo de crates e
dos perfis de `Cargo.toml` — e não do computador onde o build corre. A
máquina decide uma coisa só: se cabe. Por isso a documentação diz *onde*
foi medido e *o que do número é generalizável*, e nunca em primeira
pessoa — a máquina de quem escreve não é parte do número.

Isto não é preciosismo de redactoria. Uma tabela de picos escrita para
o computador de quem a mediu é inútil para quem a lê: ou o número não se
aplica, e então é ruído, ou aplica-se a tudo, e então a máquina não
precisa de ser nomeada.

[`verificar_ambiente.py`](../scripts/verificar_ambiente.py) aplica a
regra, com duas decisões que os seus 18 testes fixam:

* **Nos ficheiros de código só as linhas de comentário são lidas.** Uma
  cadeia de literais nunca é accusada, e é por isso que a interface
  pode falar do equipamento de quem a usa sem ser corrigida por um
  portão de documentação.
* **Há uma isenção em bloco**, marcada por
  `verificar_ambiente.py: ignorar`. Uma única citação da frase proibida
  existe em `verificar_gitignore.sh`, e reescrevê-la destruiria o
  sentido. Um portão que obriga a mutilar a prosa acaba desactivado.

### O verificador de rótulos, e porque um rótulo errado é o mais silencioso

`docs/roadmap.md` distingue `IMPLEMENTADO`, `PLANEADO` e `CONCEITO`, e
a definição de `PLANEADO` é «decidido, **especificado**, ainda sem
código». Houve uma linha na tabela do `PLANEADO` com a coluna
«Especificado em» a `—` — a negação da definição.

Não apanhou ninguém porque **um rótulo errado não dá sintaxe inválida**.
O Markdown continua bem formado, a linha parece igual às outras, e o
leitor lê «isto vai ser feito» sem perguntar porquê. É a divergência mais
silenciosa que um repositório tem: não está no código, não está num
erro de escrita, está numa palavra.

Daí [`verificar_roadmap.py`](../scripts/verificar_roadmap.py) e os seus
10 testes em [`test_verificar_roadmap.py`](../tests/test_verificar_roadmap.py),
que escrevem o erro de volta — coluna vazia, item nas duas tabelas,
tabela em falta — e exigem que seja apanhado. A regra aplicável está em
`docs/roadmap.md` §Como se decide.

### A superfície que não é código

Um teste cobre o que se escreve. Nenhum teste cobre o que se **traz**:
uma crate com advisory, uma tag de `actions/` que alguém move, uma
dependência com licença copyleft.

[`verificar_seguranca.sh`](../scripts/verificar_seguranca.sh) corre as
cinco verificações, e é o **mesmo** script na máquina de quem desenvolve
e no CI. Localmente uma ferramenta ausente é um skip anunciado com o
comando de instalação; no CI, com `--exigir`, é **falha** — porque uma
auditoria que salta em silêncio é a forma mais cara de não ter auditoria.

| Camada | Ferramenta | Superfície |
| --- | --- | --- |
| Segredos | `gitleaks` | histórico completo, não só a árvore |
| Advisories de Rust | `cargo audit` | 548 crates |
| Advisories de JS | `npm audit` comparado com as excepções | 32 pacotes, 14 de execução |
| Licenças | `cargo deny` | copyleft, que tornaria a licença do projecto impossível |
| Workflows | `zizmor` + `uses:` por SHA | injecção e acções mutáveis |

`npm audit` não tem ficheiro de excepções, e as duas saídas que oferece
não servem: `--omit=dev` esconderia advisories das ferramentas que
escrevem o bundle, e `--audit-level=critical` trocaria cobertura por
silêncio. Por isso o relatório é comparado com
[`seguranca-excepcoes.toml`](../seguranca-excepcoes.toml) — e cada
advisory aceite tem uma **razão verificável no código** e uma **data de
revisão**. As quatro excepções que existem estão escritas em
[`security_model.md`](security_model.md) §5.

Detalhe de porquê em §A regra que não depende da máquina: a fixação de
`uses:` por SHA segue a mesma lógica de
[`UI/tools/versoes.txt`](../UI/tools/versoes.txt), que recusa
descarregar o sumário do Node de quem o entrega.

### Os auditores de texto têm testes, e porquê

Cada auditor tem um `tests/test_verificar_*.py` que não se limita a correr
a ferramenta: **introduz a divergência e exige que ela seja apanhada**.

Um verificador que passa porque não viu nada é indistinguível, na saída,
de um que funciona. Em 2026-10-07, `verificar_comentarios.py` devolvia
lista vazia para ficheiros JavaScript — porque não conhecia o sufixo, e
não por não haver nada a reportar — e a interface, 91 ficheiros
JavaScript versionados, estava por verificar sem que a saída dissesse
nada. Um
`assert not findings` não teria apanhado isso; um teste que escreve um
americanismo num ficheiro temporário e exige que ele apareça, sim.

O mesmo se aplica a `verificar_estrutura.py`, que reconstrói a árvore do
mapa a partir da indentação e tem 10 testes — entre os quais um que
escreve o mapa anterior, com os seus sete ficheiros inventados, e exige
que o verificador o rejeite.

### O que o CI executa, e o que não executa

`.github/workflows/portoes.yml` corre **esta lista, inteira**, em cada
push. `verificacao.yml` corre a matriz de `testar.sh`. O detalhamento
está em [`DEV_GUIDE.md`](DEV_GUIDE.md) §3.6.

> **Os workflows não foram executados.** Até ao primeiro push não há
> forma de os correr, e um YAML válido não é um workflow que passa: um
> `uses:` com um tag errado, ou um passo a depender de um ficheiro que
> o runner não tem, só falha no GitHub. Até lá, o que está verificado é
> que **o YAML é válido** e que **cada comando existe no repositório** —
> `./scripts/testar.sh`, `UI/tools/onyxchat instalar`, `instalar-ui`,
> `ver`, e os alvos de `cargo fuzz`. A primeira execução vai ser
> diagnóstico, e isso fica escrito aqui para que o primeiro sinal
> vermelho não se leia como um portão que nunca funcionou.

### Onde vivem os testes, e porquê não em `scripts/`

Os testes ficaram em `tests/`, ao lado dos outros, e não junto dos
auditores que testam. `pyproject.toml` tem `testpaths = ["tests"]`: um
`tests/test_verificar_*.py` em `scripts/` era descoberto por quem o
chamasse à mão e **nunca pela matriz** — que é a forma mais silenciosa
de um teste não existir. Os testes vivem onde a configuração os encontra.


## Como correr

> **Todos os comandos Rust usam `--no-default-features`.** A feature
> `tor-real` **não** é feature de omissão, de propósito: é a única razão
> pela qual um build exigiria ~458 das 548 crates do projecto e vários GB
> de RAM. Os testes usam `TorFalso` (`--tor nenhum`) e não precisam de
> Tor. Ver [`DEV_GUIDE.md`](DEV_GUIDE.md) §2.3.

Todos os comandos Rust passam por [`scripts/memoria.sh`](../scripts/memoria.sh),
que os põe dentro de uma scope de cgroup com tecto de memória. O
`pytest` passa por `scripts/testar.sh`, que lhe põe o mesmo tipo de
tecto. Chamar o `cargo` directamente **não tem tecto**, e é assim que o
editor foi fechado por OOM — ver §Gate de hardware.

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

# C/C++ (CTest) — sem gate: o compilador C pede muito menos que o rustc.
# Medido em 2026-10-07: 2 MB, contra os 677 MB da suite Rust.
cmake -S crypto/c_cpp -B crypto/c_cpp/build && cmake --build crypto/c_cpp/build
ctest --test-dir crypto/c_cpp/build

# Python (pytest + cobertura, meta 100% de linhas) — com tecto
# O passo mais barato de correr com o gate, e o que mais vezes se
# esquecia: `scripts/testar.sh` põe-lhe 389 MB e mede o pico.
./scripts/testar.sh --rapido

# Ou directamente, com o mesmo tecto à mão:
#   systemd-run --user --scope -p MemoryMax=389M -p OOMPolicy=kill -- \
#       .venv/bin/pytest --cov

# E2E com o binário real (TorFalso — sem rede)
./scripts/memoria.sh --pico -- cargo build --no-default-features
F=$(mktemp) && systemd-run --user --scope -p MemoryMax=389M -p OOMPolicy=kill \
    -- env ONYXCHAT_PICO="$F" ONYXCHAT_TECTO=389 ./scripts/memoria-envolver.sh \
    .venv/bin/pytest tests/test_e2e_daemon.py tests/test_e2e_rede.py -q
rm -f "$F"

# Cobertura Rust — --no-default-features é obrigatório
./scripts/memoria.sh --pico -- cargo llvm-cov --workspace \
    --no-default-features --ignore-filename-regex 'tor_arti'

# Fuzz (smoke)
cargo fuzz run envelope_parser -- -runs=10000
```

Ou num comando, que é o caminho recomendado: `./scripts/testar.sh`
(matriz 1–5) e `./scripts/testar.sh --cobertura` (acrescenta as
coberturas). O script aplica o gate a todos os passos Rust **e ao
`pytest`**, que desde 2026-10-07 corre dentro de uma scope com
`MemoryMax` de 389 MB.

### A regra que a matriz impõe

**Um passo cujo pico medido não cabe no que está disponível não corre.**
Não é uma falha, e é contada à parte: `scripts/testar.sh` distingue
«o código está errado» de «a máquina não tem memória», e só a primeira
faz a matriz falhar. Recusar antes de começar é deliberado — um build
que sabemos que vai ser morto a meio gasta minutos de CPU e não devolve
nada.

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
| `cargo check -p onyxchatd --features tor-real` | 458 | **931 MB** | ~9 min |
| `pytest --cov` (suite Python completa) | — | **89 MB** | ~2 min |

A última linha da tabela Rust desceu de **1156 MB para 931 MB**, e a
razão está em §A verificação da arti sem fechar o editor.

O `pytest` também tem de ir com tecto, e a tabela só passou a tê-lo
depois de medido. Sem um número, o passo corria sem `MemoryMax` — e é
justamente o que aconteceu em 2026-10-07: a suite inteira foi lançada
de uma vez, sem confinamento, e o editor levou com ela.

> **Medido em 2026-10-07**, i3-5005U, 3,8 GB, `MemAvailable` de ~1200 MB
> com o editor aberto. O tecto que `scripts/testar.sh` põe ao `pytest`
> é de **389 MB** (89 + 300 de margem), e 89 MB é o pior pico
> observado — ver §Execuções registadas. Os passos Rust com tecto de
> 847 MB mediram 677 MB (suite) e 93 MB (build incremental).

O número que interessa é o da arti: **os 458 crates precisam de 931 MB**,
e a máquina com o editor aberto dá cerca de 850 utilizáveis. A diferença
vai para swap — ver §A verificação da arti sem fechar o editor.

O workspace leve desceu de 881 MB para 547 MB depois de três correcções
— `MAX_PAYLOAD` derivado de um orçamento em vez de 16 MiB escritos à
mão, um interpretador Lua reusado por thread em vez de 8 por ciclo do
pipeline, e `opt-level = 2` para o pacote `lua-src`. Ver
[`DEV_GUIDE.md`](DEV_GUIDE.md) §1.2 para a tabela detalhada e para a
razão pela qual três dos testes alocavam 16 MB na pilha.

#### Um teste Python que alocava meio gigabyte

`tests/test_ipc_client.py::test_enquadramento_comprimento_grande`
medido em **1047 MB** — quase o dobro do pico do build Rust inteiro, e
quatro vezes o tecto que o gate daria a uma suite completa.

A causa é a mesma que a dos três testes de 16 MB na pilha, pelo outro
lado. O teste fazia

```python
resposta = b"\x00" * (ipc_client.MAX_PAYLOAD + 1)
```

para que o cabeçalho anunciasse um comprimento acima do limite. E
`MAX_PAYLOAD` deixou de ser 16 MiB escritos à mão: passou a ser derivado
de um orçamento de 1 GB. **A correcção que fechou a superfície de
exaustão no daemon tornou o teste 32× mais caro**, porque o teste
materializava o limite em vez de o anunciar.

A propriedade a provar — «um cabeçalho acima do limite é recusado» — é
do cabeçalho. O teste passa a anunciar o comprimento e a não enviar
corpo nenhum (`servidor_falso(..., anunciar=...)`), o que prova mais: um
cliente que lesse o corpo antes de recusar ficaria à espera dos 512 MiB
que nunca chegariam. Ficou em **34 MB**, e a suite inteira em 71 MB.

O que fica da lição é a regra: **um teste que escreve um limite tem de
pagar esse limite em memória.** A verificação vem antes da alocação em
todo o produto — `Content-Length` conferido antes de ler, comprimento
validado antes de reservar — e um teste que aloca o limite para o provar
estava a refazer pela inversa a coisa que o limite existe para impedir.

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
escreveu 498 fingerprints e, no minuto seguinte, provocou o OOM. Continua
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

### Correr

```bash
./scripts/testar.sh --fuzz
```

Não está no modo por omissão porque precisa de **nightly** e porque
2000 execuções por alvo não é uma sessão de fuzzing. O que apanha na
realidade não são crashes que 2000 execuções encontrem — são alvos que
**deixaram de compilar** depois de uma API mudar, que é o que acontece
quando ninguém os constrói.

O `cargo fuzz` vai com tecto, e o tecto é medido:

| passo | pico | tecto |
| --- | --- | --- |
| `cargo +nightly fuzz build <alvo>` | **770 MB** (o primeiro; os seguintes são incrementais) | 770 MB |
| `cargo +nightly fuzz run <alvo>` | 33–117 MB, conforme o alvo | 117 MB |

Os alvos correm **um de cada vez**, e não por moderação: o libFuzzer
é multi-threaded por defeito, e seis motores concorrentes a hundred MB
cada um é a receita para repetir o que a árvore da arti fez.

`cargo fuzz build` aceita **um** alvo por invocação. Passar a lista
inteira dá `Usage: cargo-fuzz build` e código 2 — foi o que a primeira
versão deste passo fez, e o sumário da matriz acusou a falha sem dizer
qual dos seis era.

### Execuções registadas

**Um pico de memória é a observação de uma execução, não uma propriedade
do código.** Depende da máquina, do que estava em cache, do número de
ficheiros que o `pytest` recolhe e — neste projecto — de quantos
auditores cada teste de auditor corre num subprocesso. Duas execuções do
mesmo commit dão números diferentes, e ambos são verdadeiros.

Por isso o registo é uma tabela datada, e não um número solto. Um número
solto envelhece sozinho e deixa de ser verdade: em 2026-10-07 este
documento afirmava três picos diferentes para a mesma medição — 71 MB,
74 MB e 81 MB — e as execuções desse dia mediram 75, 78, 78 e 89 MB.
Nenhum dos cinco estava errado; o que estava errado era **afirmar um
número como se fosse estável**.

| Data | Passo | Pico | Tecto | Nota |
| --- | --- | --- | --- | --- |
| 2026-10-07 | `pytest --cov` | 89 MB | 389 MB | pior observado; 46 testes de auditores |
| 2026-10-07 | `pytest --cov` | 78 MB | 389 MB | execução intermédia |
| 2026-10-07 | `pytest --cov` | 75 MB | 389 MB | execução mais leve |
| 2026-10-07 | `cargo check --features tor-real` | 931 MB | 1051 MB RAM + swap | — perfil de referência |
| 2026-10-07 | `cargo test --workspace --features docs` | 677 MB | 847 MB | 399 testes |
| 2026-10-07 | `ctest` (C/C++) | 2 MB | sem tecto | 3/3 |

O tecto do `pytest` é `PICO_PYTEST_MB + 300` em `scripts/testar.sh`, e
`PICO_PYTEST_MB` é o **pior observado** — não uma média, e não um
número redondo. Subiu de 71 para 89 MB em 2026-10-07: a referência estava
abaixo do que a máquina já pedia, e um tecto calculado a partir de um
pico que não acontece é um tecto calculado às cegas.

### A última execução completa

```text
Data:     2026-10-07 (o registo anterior: 2026-10-05 12:26 UTC)
Rust:     rustc 1.98.1 (48a229cea 2026-09-01)
Python:   3.12.3 · pytest 9.1.1
CMake:    3.28.3
Perfil de referência: 2 núcleos, 3,8 GB de RAM, sem swap
```

Todos os passos Rust e o `pytest` correram dentro de uma scope com
`MemoryMax`. Os picos estão na tabela acima.

| Matriz | Resultado | Pico | Tecto |
| --- | --- | --- | --- |
| `cargo test --workspace --no-default-features --features docs` | **399 ok**, 0 falhados | 677 MB | 847 MB |
| `ctest --test-dir crypto/c_cpp/build` | **3/3 ok** | 2 MB | sem tecto (medido) |
| `.venv/bin/pytest --cov` | **ok**, cobertura **100,00%** (2307/2307) | 89 MB | 389 MB |
| `pytest tests/test_e2e_{daemon,rede,sidecar}.py` | **34 ok** | 43 MB | 389 MB |
| `cargo build --no-default-features` | ok | 93 MB | 847 MB |
| `cargo check -p onyxchatd --features tor-real` | **ok** — `Compiling onyxchatd` | 931 MB | RAM 1051 MB + swap |


A última linha é a que era recusada antes do gate passar a contar RAM e
swap em tectos separados. Ver §A verificação da arti sem fechar o
editor.

A matriz completa (`./scripts/testar.sh`) deu **11 passos ok, 0
falhados** — e nenhum saltado, que era o que acontecia quando a
verificação da arti não cabia.

O pico do `pytest` subiu de 74 MB para **81 MB** em 2026-10-07, com os
46 testes dos auditadores de texto (`tests/test_verificar_*.py`). Não é
um aumento do código de produção — é cada teste correr um auditor sobre
o repositório inteiro, num subprocesso. O custo é uma forked tree por
teste, e a cobertura de produção não se mexeu: continua a ser 2307/2307.

A partir da mesma data, a matriz tem um **passo 3b** novo — o passo da
estrutura. Ver §Os portões de entrega.

### A verificação da arti sem fechar o editor

Até 2026-10-07 este passo era **recusado pelo gate** em qualquer máquina
de 3,8 GB com o editor aberto: pedia 1156 MB de pico mais 300 de margem,
e havia cerca de 850 MB utilizáveis. Um build morto a meio teria gasto
minutos de CPU e não devolvido nada — a recusa era a decisão certa, com
os números que existiam.

O que mudou foram os números, e eles medem-se.

`scripts/medir-arti.sh` corre o mesmo comando três vezes, cada uma com o
seu `CARGO_TARGET_DIR` — sem o que a segunda encontraria a primeira feita
e mediria o `cargo` a ler fingerprints em vez de um `rustc` a compilar:

| | configuração | pico |
| --- | --- | --- |
| A | a de sempre (`debug = "line-tables-only"`) | 1089 MB |
| B | `CARGO_PROFILE_DEV_DEBUG=none` | **931 MB** |
| C | B mais `split-debuginfo = "unpacked"` | 1058 MB |

**B entra, e C ser pior do que B merece registo.** A ideia natural de
`split-debuginfo = "unpacked"` é que mover o DWARF para `.dwo` baixe a
memória do compilador; mede-se 127 MB a mais. O que acrescenta é manter
cada `.dwo` aberto durante a geração, e um ficheiro por crate sai mais
caro aqui do que um DWARF que nunca chega a existir. B não escreve
debuginfo nenhum, e a ausência é mais barata do que a partilha.

931 contra os ~850 MB que a máquina dá com o editor aberto. O que fecha é
a segunda regra:

| | o que é | valor |
| --- | --- | --- |
| `MemoryMax` | tecto de RAM — **não sobe** com o swap, porque é ele que protege o editor | 1051 MB |
| `MemorySwapMax` | o que o build escreve em disco além do `MemoryMax` | livre − 256 MB |

E a margem passou a ser **por perfil**: 120 MB na verificação da arti
(não há link nem fixtures num `cargo check`) contra 300 MB no workspace
leve (que liga e corre). Com 300 MB a verificação era recusada por 180 MB
de folga para um link que não existe.

Quando o build vai usar swap, o gate **diz** — porque uma execução que
escreve em disco não é a mesma que uma em RAM, e reportar o tempo de uma
como se fosse a outra é a forma de o gate mentir sobre o que mediu.

`ONYXCHAT_DEBUG=1` traz o debuginfo de volta, para quem depura e
prefere pagar em memória o que ganha em backtraces.

### As 45 linhas por cobrir, e porquê

**Não são todas ramos inatingíveis.** A tabela de 2026-10-05 dizia que
as 12 eram «defensivos inatingíveis», e essa afirmação não sobreviveu à
medição: das 45, **16 eram alcançáveis** e quatro testes novos
cobrem-nas. As que ficam classificam-se por **porque** não correm — e a
classificação é a parte que interessa, porque «não foi testado» e «não
pode ser testado» são coisas diferentes, e só a segunda se declara.

| Grupo | Linhas | Porque não correm |
| --- | --- | --- |
| **`paginas.rs` — precisa de `root`** | 226, 234, 272, 333, 366, 447–450 (9) | `getrlimit`/`setrlimit` a falhar, `RLIM_INFINITY` a fixar, e os `panic!` dos testes de degradação. Exigem `CAP_SYS_RESOURCE`, que a verificação não tem |
| **Código de teste** | `docs_vec.rs:137`, `envelope.rs:341`, `lua_camadas.rs:195`, `ffi_c.rs:505`, `ipc_testes.rs:193,1087,1517,2253`, `p2p_testes.rs:1308,1327-1329`, `anti_replay.rs:862,919`, `orcamento.rs:432` (15) | São mensagens de `assert!` e um `flush` de writer de teste: só executam se o teste **falhar**. Contá-las é contar texto que nunca corre |
| **Defesa em profundidade** | `anti_replay.rs:207-209`, `ffi_c.rs:178,198`, `ipc.rs:462` (6) | Documentadas no próprio código. `anti_replay.rs:207` é a rotação durante a carga: o `min(…, CAPACIDADE_NONCES1)` trunca **antes** de percorrer, e portanto nunca há uma entrada a expulsar — o teste `um_ficheiro_acima_da_capacidade_e_truncado` prova isso pela positiva |
| **Segundo utilizador** | `ipc.rs:1478` (1) | Descartar uma ligação cujo `SO_PEERCRED` é outro uid. Exige uma segunda conta no host |
| **Escrita que falha** | `anti_replay.rs:234,236,258,261-262` (5), `ipc.rs:1417` (1) | `create_dir_all`/`open`/`write` a falhar, e um `cv.wait` que precisa de contenção real entre dois atendimentos |

O último ponto é o que fica em aberto: **6 destas linhas são
alcançáveis** e estão por testar. Declará-las inatingíveis seria a mesma
mentira que a tabela de 2026-10-05 fazia com 12.

**Exclusões declaradas:**

* `tor_arti.rs` — único ficheiro que toca a rede real; não é testável
  offline. A compilação é assegurada por `cargo check --features
  tor-real`, e o isolamento por um teste de guarda
  (`network/daemon_rust/tests/guarda_tor.rs`).
* As 45 linhas da tabela acima, com a classificação acima.
* Os **`*_testes.rs` não são excluídos** da métrica. São 5411 das
  14151 regiões medidas — **39% do que se mede são os testes**. Isso
  distorce a métrica para cima e é uma escolha deliberada, porque
  código morto dentro de um teste não é uma propriedade do produto.
  Mas é uma escolha, e por isso está escrita.
* `branch = false` em Python — a meta é de linhas, não de ramos.

**A cobertura não é usada como argumento de segurança.** Demonstração:
um bug de protocolo pode viver em linhas 100% cobertas, e os 99,38%
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
