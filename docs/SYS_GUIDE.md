# Guia do Sistema — OnyxChat

> **Nível APROFUNDADO.** Este documento é o ponto de entrada para *compreender* o OnyxChat:
> - o que é
> - porque é assim
> - o que deliberadamente não faz.
>
> Absorve e funde o conteúdo histórico do `README.md` (filosofia,
> pipeline, tabela de arquitectura) com o de `docs/architecture.md`
> (componentes, fluxo, fronteiras), e expande ambos com as decisões de
> projecto e os limites declarados.
>
> Para *usar*: [`USER_GUIDE.md`](USER_GUIDE.md) ·
> Para *alterar*: [`DEV_GUIDE.md`](DEV_GUIDE.md) ·
> Para *bytes*: os specs normativos, via [`index.md`](index.md)

---

## 1. O que o OnyxChat é — e o que não é

### 1.1 O que é

O OnyxChat é uma **arquitetura de comunicação P2P privada** em que cada
instância funciona simultaneamente como cliente e como servidor/endpoint:

```text
                 ┌─────────────────────────┐
                 │        ONYX CHAT        │
                 │                         │
                 │  Identidade própria     │
                 │  Criptografia E2E       │
                 │  Tor                    │
                 │  P2P                    │
                 │  Discovery mínimo       │
                 │  Relay opcional         │
                 │  Routing descentralizado│
                 └────────────┬────────────┘
                              │
                  ┌───────────┴───────────┐
                  │                       │
              Endpoint                 Endpoint
                  │                       │
                  └───────────┬───────────┘
                              │
                       conteúdo protegido
```

### 1.2 O que **não** é

Isto é tão importante como o que é, e é o que a maioria dos projectos
desta área esconde:

* **Não** é «um mensageiro que usa Tor». A integração do Tor é uma
  consequência da arquitectura, não o objectivo.
* **Não** tem servidor central de mensagens, de histórico ou de
  contactos.
* **Não** é um produto pronto. É uma base de arquitectura com
  especificações formais e testes de paridade.
* **Não** esconde o IP real de forma absoluta. Ver §3.
* **Não** elimina metadados. Ver §3.4.
* **Não** garante entrega. Ver §10.4.

### 1.3 A afirmação correcta, e a incorrecta

A formulação correcta:

> O conteúdo não é acessível à infraestrutura de transporte; a identidade
> real do utilizador não é necessária para o encaminhamento; o
> transporte P2P é concebido para evitar a exposição directa do endereço
> IP real entre os endpoints.

A formulação **incorrecta**, e comum, seria:

> «Nenhum IP real é exposto.»

Cifrar conteúdo não elimina metadados (tamanho, frequência, timing) e,
em modo relay sobre clearnet, os IPs de ligação são visíveis ao relay.
Esta distinção é normativa e está detalhada em
[`privacy_model.md`](privacy_model.md).

---

## 2. O problema e a resposta de arquitectura

### 2.1 O problema

A comunicação privada convencional tem uma assimetria estrutural:

```text
   utilizador  ──────►  servidor central  ◄──────  utilizador
                              │
                    conhece ambos os lados
                    vê o tráfego
                    é o ponto de falha
                    é o ponto de censura
```

Eliminar essa assimetria exige que **os participantes forneçam os
próprios endpoints**. Isso cria um problema novo: como é que A encontra
B sem existir um ponto central?

### 2.2 A resposta em seis funções separadas

O OnyxChat separa seis funções que costuma vir coladas. A separação é
o ponto do projecto:

```text
IDENTIDADE
     ↓
COMUNICAÇÃO
     ↓
TRANSPORTE
     ↓
DISCOVERY
     ↓
ROUTING
     ↓
ARMAZENAMENTO
```

Nenhuma destas funções tem necessariamente de concentrar todo o
conhecimento sobre a comunicação. O objectivo final é uma rede onde os
próprios participantes fornecem os endpoints e, opcionalmente, o
transporte — **sem ponto central que conheça ou armazene a totalidade
das comunicações.**

### 2.3 O princípio central

Não é «esconder o IP». É:

> **Eliminar, tanto quanto possível, a necessidade de confiar numa
> infraestrutura central para comunicar.**

Isto implica que discovery, routing e armazenamento são *componentes
substituíveis*, não pilares. Hoje dois deles ainda são centralizados
por desenho pragmático (discovery, relay) — e ambos estão declarados
como tal, com a substituição planeada.

---

## 3. Os quatro objectivos de privacidade

São **objectivos separados**. Confundi-los é a origem de quase todos os
erros de avaliação nesta área. O estado de cada um está em
[`privacy_model.md`](privacy_model.md).

### 3.1 Conteúdo

`IMPLEMENTADO`. Cifrado fim-a-fim com o pipeline K1→K9. Nenhum
componente de transporte tem as chaves de conteúdo.

### 3.2 Identidade

`IMPLEMENTADO`, com uma ressalva importante: a identidade Onyx é
**estável e correlacionável**. Estabilidade é o que a torna
autenticada; correlacionabilidade é o custo dessa estabilidade. Um
identificador descartável por conversa não é o que este projecto faz,
porque isso implicaria trocar de identidade a cada handshake — o que
não é o objectivo de uma identidade Ed25519.

### 3.3 Endereço de rede

`IMPLEMENTADO` no modo directo (hidden service ↔ hidden service).
`PARCIAL` no modo relay: um relay em clearnet vê os IPs de ligação.

### 3.4 Metadados

`MINIMIZADO`, não eliminado. O sistema **não** cria infraestrutura
central que mantenha histórico, contactos ou registo de quem falou com
quem — essa é a minimização por ausência de estrutura.

O que **continua observável**, e é declarado explicitamente:

| Metadado | Observável por |
| --- | --- |
| tamanho | relay, operador de rede |
| frequência e duração | relay, operador de rede |
| timing | relay, operador de rede |
| origem do tráfego | relay (clearnet), operador de Tor |
| destino observado | relay (mailbox), discovery (consultas) |
| número de mensagens | relay (contadores internos) |
| padrões de comunicação | quem correlaciona as observações acima |

Mitigação activa de tráfego (padding, temporização) é `CONCEITO`: não
implementada e não planeada neste momento.

---

## 4. Criptografia analógica

> No OnyxChat, *criptografia analógica* designa técnicas com origem
> histórica/manual — César, Vigenère, Playfair adaptado, substituição
> — onde a chave é um **procedimento** e não um segredo de 256 bits.

O termo descreve a **origem conceptual** da técnica, **não** o formato
dos dados. Em execução, todos os dados são digitais. Uma biblioteca
clássica implementada em software e strung num pipeline é digital do
ponto de vista da máquina; é analógica do ponto de vista da ideia.

A distinção importa porque evita duas leituras erradas:

* **Não** aumenta a força criptográfica. Uma chave «VIGENERE» é
  encontrável por análise de frequência; o que a salva é estar
  **entre** camadas cujas chaves **não** são adivinháveis.
* **Não** é redundância inútil. É transformação de estrutura.

---

## 5. O pipeline de 9 camadas

### 5.1 Ordem e responsável

```text
plaintext
  │ K1  ChaCha20-Poly1305 (chave do par)        [Rust]
  │ K2  Cesariana +3                            [Lua]
  │ K3  Vigenère "VIGENERE"                     [Lua]
  │ K4  Transposição 5 colunas (PKCS#7)         [C++]
  │ K5  AES-256-GCM (chave do remetente)        [Rust]
  │ K6  Playfair Adaptado Onyx (XOR "PLAYFAIR") [Lua]
  │ K7  Hill [[3,3],[2,5]] mod 256              [Rust]
  │ K8  Substituição b+7 mod 256                [Lua]
  │ K9  ChaCha20-Poly1305 (chave do receptor)   [C/libsodium]
  ▼
envelope = versão ‖ nonces ‖ ciphertext ‖ assinatura Ed25519
```

### 5.2 Classificação

| Tipo | Camadas | Garantia |
| --- | --- | --- |
| **Digital** (primitivas formais) | K1, K5, K9 | AEAD autenticado: confidencialidade + integridade |
| **Identidade** | assinatura Ed25519 no envelope | autenticidade e anti-downgrade |
| **Analógica** (clássicas) | K2, K3, K6, K8 | ofuscação estrutural |
| **Estrutural** (matriz) | K4, K7 | reordenação e difusão em blocos |

### 5.3 Defesa em profundidade, não soma

```text
SEGURANÇA ≠ segurança(K1) + segurança(K2) + … + segurança(K9)

SEGURANÇA prática =
      composição correcta
    + separação de chaves
    + autenticação (Ed25519 + tags AEAD)
    + protecção de transporte (Tor)
    + implementação correcta
```

As nove camadas **não** se somam. Comprometer uma camada intermédia não
entrega plaintext: o atacante fica perante as restantes transformações e
perde a autenticação. É isso que a profundidade faz — não
multiplicar.

### 5.4 Separação de papéis das chaves

```text
autenticação de identidade .... Ed25519 (handshake + envelope)
confidencialidade mútua ....... K1 (chave do par)
confidencialidade de origem ... K5 (chave do remetente)
confidencialidade de destino .. K9 (chave do receptor)
ofuscação estrutural .......... K2, K3, K4, K6, K7, K8
```

Comprometer K5 não revela K1 nem K9. **Não existe chave global.**
Detalhe em [`key_management.md`](key_management.md).

### 5.5 Propriedades do envelope

* A assinatura é **sempre verificada antes de qualquer decifragem**.
* Os nonces (`nonce1`, `nonce5`, `nonce9`) são aleatórios por mensagem
  e estão dentro da região assinada.
* A versão do protocolo está dentro da região assinada — logo
  anti-downgrade sem mecanismo adicional.
* `nonce1` alimenta o registo anti-replay do receptor.

Norma completa: [`message_format.md`](message_format.md).

---

## 6. Componentes e responsabilidades por linguagem

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

### 6.1 Porquê várias linguagens

Não é poluição arbitrária. Cada linguagem está onde é mais forte para
aquela camada, e a **paridade é testada**, não asumida:

* **Rust** — memory safety no daemon e nas primitivas. Um parser de
  rede sem `unsafe` nem panics é um argumento de segurança verificável.
* **C/C++** — as camadas K4 (transposição em bloco) e K9 (produção
  sobre libsodium, biblioteca auditada fora do projecto).
* **Lua** — as camadas analógicas ficam em código que se lê como o
  algoritmo clássico, embebido no daemon via `mlua`. Legibilidade do
  algoritmo analógico, não do digital.
* **Python** — o cliente, e implementações de referência das camadas
  analógicas usadas como oráculo de paridade nos testes.

O custo é real: **a paridade entre languages tem de ser provada**, não
afirmada. É por isso que existem test vectors (§9) e uma matriz de
interoperabilidade (`testing.md`).

### 6.2 Separação de responsabilidades, formulada correctamente

A documentação evita frases como «Python-Lua faz a criptografia». A
formulação correcta:

> O subsistema Python/Lua integra o cliente Python com as camadas
> criptográficas em Lua, que correm dentro do daemon.

Da mesma forma, «Rust» não é apenas uma linguagem: é o componente
responsável pelas primitivas digitais e pelo daemon.

**Não existe implementação de referência em Python.** A formulação
«Lua (produção) e Python (referência)» esteve em
`docs/architecture.md` e na `Estrutura.txt` até 2026-10-07, e era falsa:
`crypto/python_lua/python/` nunca existiu. A referência está nos
vectores em `tests/vectors/`, lidos por `tests/test_vectors.py` — que é
uma referência melhor, porque falha quando alguém se engana.

---

## 7. Princípio central: cada instância é cliente e servidor

Cada instalação é, simultaneamente:

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

Um Onyx **não** é um cliente que se liga a um servidor central:

```text
Onyx A <───────────────> Onyx B

Servidor/endpoint A <──> Servidor/endpoint B
       ▲                         │
       │                         │
    cliente                   cliente
```

Os próprios participantes fornecem os endpoints. Não existe papel fixo
de «servidor de mensagens».

---

## 8. Ciclo de vida de uma mensagem

### 8.1 Emissor

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

### 8.2 Receptor

Caminho inverso, com uma ordem obrigatória:

```text
parse do envelope
      ▼
verificar assinatura Ed25519      ◄── falha aqui = rejeição, SEM decifrar
      ▼
reservar nonce1 (anti-replay)
      ▼
decifrar K9 → K8 → … → K1
      ▼
verificar tag AEAD
      ▼
UTF-8 válido → plaintext
```

A verificação da assinatura **antes** de qualquer decifragem é uma
propriedade testada (`pipeline.rs`, `assinatura_adulterada`), não uma
convenção. Ver [`security_model.md`](security_model.md) §Propriedades.

### 8.3 Onde a assinatura é verificada

| Passo | Executado por | Razão |
| --- | --- | --- |
| `enviar` / cifragem | daemon (Rust) | o daemon tem o pipeline completo |
| `receber` / decifragem | **cliente (Python)** primeiro | rejeitar assinatura falsa *antes* de pedir decifra ao daemon |
| decifragem | daemon (Rust) | K1→K9 |

O cliente verifica a assinatura localmente antes de emitir `DECODE`
(`messenger/pipeline.py`). Assim, um envelope com assinatura inválida
nunca chega a ser processado pelo pipeline.

---

## 9. Formas de garantir paridade entre languages

### 9.1 Test vectors como fonte única

```text
tests/vectors/camadas.json    →  KATs por camada (K1..K9)
tests/vectors/pipeline.json   →  vectors fim-a-fim (c1..c9 + envelope)
tests/vectors/handshake.json  →  REQUEST/ACCEPT/REJECT
docs/test_vectors.md          →  forma legível dos mesmos dados (GERADO)
```

Gerados por `cargo run -p onyxchatd --example gerar_vetores` a partir
da implementação de referência. **Nunca editar à mão** — um teste
verifica que o documento legível não divergiu dos JSON.

### 9.2 Quem consome

```text
Rust    network/daemon_rust/tests/vetores.rs   (K1..K9, pipeline, handshake; Lua via mlua)
Python  tests/test_vectors.py                  (K2/K3/K6/K8, envelope, handshake)
C/C++   crypto/c_cpp/tests/test_vectors.c      (K4 e K9)
Lua     exercitada através do Rust (mlua)
```

### 9.3 Matriz de interoperabilidade

| Par | O que compara |
| --- | --- |
| Lua ↔ Python | K2, K3, K6, K8 (mesmos hex) |
| Rust ↔ C | K9 (`crypto_core` vs libsodium) |
| Rust ↔ Python | K7, envelope |
| C ↔ Python | K4 |
| Python ↔ Rust daemon | IPC completo (E2E) |
| daemon ↔ daemon | frames P2P e relay (E2E de rede) |

Objectivo: garantir que a arquitectura multi-linguagem não introduz
diferenças **silenciosas**. Detalhe em [`testing.md`](testing.md).

---

## 10. Fronteiras de confiança e transportes

### 10.1 Regra de confiança

> **Todo componente de transporte é considerado não confiável** —
> discovery, relay, infraestrutura Tor e nós intermediários. A
> confiança criptográfica existe **entre os endpoints**.

```text
Python client  ──►  UDS  ──►  Rust daemon  ──►  Tor  ──►  rede
```

Cada fronteira tem formato, validação, erros, timeout, limites e
comportamento perante dados inválidos definidos em spec.

### 10.2 `BackendTor` — abstração de rede

```text
daemon (onyxchatd)
   │
   ▼
   trait BackendTor          (tor.rs)
   │
   ├── TorReal            (tor_arti.rs — único contacto com a rede)
   └── TorFalso           (loopback — testes e E2E)
```

Regras:

1. O restante daemon **nunca** depende de APIs específicas da arti —
   fala sempre com o traço `BackendTor`.
2. `tor_arti.rs` é o **único** ficheiro que toca na rede real. Isto é
   verificado por um teste de guarda (`testing.md` §Fuzzing).
3. Tudo o resto corre offline com `TorFalso` (`--tor nenhum`), o que
   permite testes determinísticos e desenvolvimento sem rede.

### 10.3 Os dois transportes

```text
Modo A:  Onyx A ←────────────→ Onyx B        (directo)
Modo B:  Onyx A ←──→ Relay ←──→ Onyx B       (fallback)
```

O relay **não** é servidor de mensagens, **não** é proprietário da
conversa, **não** possui as chaves de conteúdo. Mantém um buffer
temporário com TTL de 300 s e 64 envelopes pendentes por mailbox.
Ver [`relay.md`](relay.md).

### 10.4 Discovery ≠ Routing

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

### 10.5 O relay não é TURN despite o nome

O nome `relay_server.py` sugere TURN (relay de transporte UDP para
NAT). O que está implementado é uma mailbox cega sobre TCP, com
semântica deRendezvous: o emissor entrega numa caixa derivada de
`SHA-256("ONYX/RELAY/v1" ‖ pub)`, o receptor subscreve essa caixa.
Não há atribuição de portas, não há relay de tráfego de rede
arbitrário. Ver
[`relay.md`](relay.md) §Identidade.

---

## 11. Decisões de projecto e trade-offs

Cada linha indica o que se ganhava e o que se perdia. Uma decisão sem
custo declarado não é uma decisão, é um preference.

| Decisão | Ganho | Custo |
| --- | --- | --- |
| Tor embutido (arti) | sem dependência de instalação; hidden services eféméricos | 458 das 548 crates do build; pico medido de 931 MB |
| `tor-real` **não** em `default` | ciclo de build/teste leve (103 vs 548 crates; pico de 547 MB) | exige `--features tor-real` explícito em produção |
| Lua para as camadas analógicas | algoritmo legível no formato original | 1 dependência embutida (`mlua`, `vendored`) |
| C para K4 e K9 | transposição em bloco; libsodium auditado fora | superfície de `unsafe`/FFI, mitigada por wrappers testados |
| Python puro no cliente | zero dependências de runtime; legibilidade | Ed25519 e ChaCha20 em Python puro são *best-effort* para zeroização |
| Keystore com passphrase | o ficheiro em disco não basta | PBKDF2 em cada desbloqueio |
| Pipeline de 9 camadas | ofuscação estrutural e defesa em profundidade | custo de CPU; complexidade; K2/K3/K6/K8 não são criptografia |
| Assinatura fora do pipeline | a assinatura não é transformada (pode ser verificada antes de decifrar) | o envelope tem duas regiões com tratamento distinto |
| Anti-replay por `nonce1` | rejeição de reenvio sem estado por pares | registo circular de 4096 nonces com TTL 600 s |

### 11.1 A janela de hardware

O build completo com arti pede **931 MB de RAM** (pico medido,
`cargo check -p onyxchatd --features tor-real`, 458 crates), e o
workspace leve pede **547 MB** (103 crates). São valores do **build**:
dependem do grafo de crates e dos perfis de `Cargo.toml`, não da
máquina onde correm. O que a máquina decide é se cabe — e 931 MB cabem
em 3,8 GB **com um editor aberto**, o que era precisamente o que não
cabia e obrigava a fechar o editor para verificar.

Quatro mitigações, e a última é a que fecha:

1. `tor-real` é **opt-in**, e o ciclo de testes corre com
   `--no-default-features` (103 crates em vez de 548);

2. **todo o build corre dentro de uma scope de cgroup com tecto de
   memória** (`scripts/memoria.sh --pico`). Sem tecto, o kernel escolhe a
   vítima do OOM pelo `oom_score` — o processo **maior**, não o que está
   a ser construído. Em três OOM kills registados em 2026-10-05 a vítima
   foi um editor de linguagem com ~1 GB, com o `rustc` a 39 MB e o
   `cargo` a 26 MB ao lado;

3. **o `cargo check` não escreve debuginfo** (`CARGO_PROFILE_DEV_DEBUG=none`).
   Não há link nem binário, por isso a tabela de símbolos não serve para
   nada: desceu o pico da arti de 1089 MB para 931 MB;

4. **o gate conta RAM e swap em tectos separados.** O `MemoryMax` não
   sobe — é ele que protege o editor — e o `MemorySwapMax` absorve a
   diferença. A margem passou a ser por perfil: 300 MB onde há link e
   fixtures, 120 MB num `cargo check` que não tem as duas coisas.

Uma afirmação anterior deste documento dizia que `cargo check` da arti
é «muito mais barato do que compilar». É verdade em tempo, e falso em
memória: 458 crates são 458 processos `rustc`, e 498 fingerprints foram
escritos num minuto em que, logo a seguir, houve OOM. A verificação
continua a ser `cargo check`, mas dentro de um tecto.

Detalhe operacional e medições em [`DEV_GUIDE.md`](DEV_GUIDE.md) §1.2.

---

## 12. Estado: IMPLEMENTADO / PLANEADO / CONCEITO

Tabela completa e autoritativa em [`roadmap.md`](roadmap.md).
Resumo:

### IMPLEMENTADO

Pipeline K1→K9 · envelope binário assinado · handshake assinado com
versão · anti-replay (handshake e chat) · anti-downgrade · identidade
Ed25519 · daemon Rust · Tor embutido · P2P directo · relay opcional ·
discovery `ID → .onion` · IPC UDS autenticado por uid · cliente Python
com keystore cifrado · test vectors multi-linguagem · property tests ·
fuzzing · especificações · modelos de ameaça/segurança/privacidade ·
gestão de chaves e zeroização.

### PLANEADO

Routing nodes · rede de transporte descentralizada · discovery
descentralizado.

### CONCEITO

Identificadores de algoritmo por camada · Fake-IP · pipelines multimédia
(áudio, imagem, vídeo) · suporte a ficheiros arbitrários ·
mitigação activa de tráfego.

---

## 13. O que o sistema deliberadamente **não** faz

Lista explícita, para evitar leitura excessiva.

```text
NÃO stores mensagens em servidor central
NÃO  mantém histórico central de conversas ou contactos
NÃO  exige um servidor central para funcionar
NÃO  elimina metadados de tamanho/timing/frequência
NÃO  esconde o IP real de forma absoluta (ver §3.3)
NÃO  garante entrega de mensagens
NÃO  implementa routing descentralizado (PLANEADO)
NÃO  implementa discovery descentralizado (PLANEADO)
NÃO  suporta áudio, imagem, vídeo ou ficheiros (CONCEITO)
NÃO  implementa Fake-IP (CONCEITO)
NÃO  protege contra um observador total (depende do Tor)
NÃO  resiste à compromissão da seed do utilizador — as chaves são o sistema
```

Os dois últimos pontos são as limitações mais consequentes e estão em
[`threat_model.md`](threat_model.md) §O que o modelo NÃO cobre.

---

## 14. Janela operacional

Comandos essenciais, para não repetir aqui o que está nos guias:

Todos os comandos Rust levam o gate de memória, e o `pytest` também.
Sem tecto, o build não tem `MemoryMax` e o kernel pode matar o editor
em vez do build.

```bash
# testes — leve (103 crates, sem arti); atalho:
./scripts/testar.sh
./scripts/memoria.sh --pico -- cargo test --workspace --no-default-features

# produção — inclui Tor embutido (458 crates, pico medido 931 MB)
./scripts/memoria.sh --pico -- cargo build --release --features tor-real

# vectors: gerar (nunca editar à mão)
./scripts/memoria.sh --pico -- cargo run -p onyxchatd \
    --example gerar_vetores --no-default-features --features docs
```

Guia completo em [`DEV_GUIDE.md`](DEV_GUIDE.md) §2 e
[`USER_GUIDE.md`](USER_GUIDE.md).

---

## 15. Leitura seguinte

| Se queres… | Lê |
| --- | --- |
| usar o sistema | [`USER_GUIDE.md`](USER_GUIDE.md) |
| alterar o código | [`DEV_GUIDE.md`](DEV_GUIDE.md) |
| implementar uma camada | [`pipeline.md`](pipeline.md) |
| reimplementar o protocolo | specs via [`index.md`](index.md) |
| saber o que é prometido e o que não | [`threat_model.md`](threat_model.md) |

---

## 16. Descrição canónica

> **OnyxChat é uma arquitectura de comunicação P2P privada na qual cada
> instância funciona simultaneamente como cliente e endpoint servidor,
> utilizando Tor para abstrair a conectividade, um mecanismo de discovery
> sem armazenamento de mensagens, transporte directo ou por relay e um
> pipeline criptográfico híbrido de nove camadas.**
>
> **O sistema procura proteger simultaneamente o conteúdo, a identidade,
> os endereços de rede e minimizar a dependência de infraestrutura
> central.**
>
> **As nove camadas criptográficas não são apresentadas como nove vezes
> mais segurança, mas como uma estratégia de defesa em profundidade,
> combinando primitivas criptográficas modernas com transformações
> definidas pelo projecto.**
>
> **A identidade é estabilizada e autenticada através de Ed25519, enquanto
> as chaves de comunicação são utilizadas separadamente dentro do
> pipeline. A infraestrutura de transporte é considerada não confiável e
> não deve necessitar de acesso ao conteúdo das comunicações.**
>
> **A arquitectura actual utiliza Tor, discovery e relay, mas foi concebida
> de forma a permitir uma evolução futura para routing descentralizado
> através de outras instâncias Onyx, sem transformar esses nós em
> servidores centrais de armazenamento ou comunicação.**
