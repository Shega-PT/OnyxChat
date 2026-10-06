# Formato da Mensagem (Envelope Binário)

## Introdução

Este documento define o **formato da mensagem** do OnyxChat. Cada
mensagem contém a **saída final do pipeline K1→K9**, os **nonces AEAD**
necessários à decifragem e a **assinatura Ed25519** do remetente — e
nada mais.

É uma especificação de protocolo: pode ser implementada sem ler o
código (`docs/message_format.md` é independente de Rust/Python/C; as
implementações é que o cumprem).

Objetivos do formato:

* **Confidencialidade** — o conteúdo da mensagem só está acessível a
  quem detém as chaves K1/K5/K9; os nonces e a assinatura viajam em
  claro por definição (são metadados obrigatórios do formato, não
  conteúdo).
* **Integridade e autenticidade** — a assinatura cobre versão + nonces +
  ciphertext; qualquer alteração rejeita a mensagem **antes** de
  qualquer decifragem.
* **Parsing trivial** — tudo por comprimento, nunca por delimitadores.

---

## Estrutura do Envelope

```text
 offset  campo                        tamanho      nota
 ------  ---------------------------  -----------  --------------------
    0    versão do formato            1 byte       0x01 (ver §Versionamento)
    1    nonce1 (K1)                  12 bytes     ChaCha20-Poly1305
   13    nonce5 (K5)                  12 bytes     AES-256-GCM
   25    nonce9 (K9)                  12 bytes     ChaCha20-Poly1305
   37    ciphertext final (K9)        N bytes      N ≥ 16 (tag Poly1305)
  37+N   assinatura Ed25519           64 bytes     sobre [0 .. 37+N)
 ------  ---------------------------  -----------
  total  101 + N bytes
```

### Regras de parsing

1. `comprimento ≥ 117` (101 fixos + 16 da menor tag AEAD) — caso
   contrário → rejeição (`EnvelopeCurto`).
2. `comprimento ≤ MAX_ENVELOPE` (65 692 bytes) — acima → rejeição,
   **antes** de alocar ou processar (`EnvelopeGrande`).
3. `versão == 0x01` — qualquer outro valor → rejeição
   (`VersaoDesconhecida`).
4. `N = comprimento − 101`.
5. A assinatura é o bloco final de 64 bytes; a **região assinada** são
   todos os bytes anteriores (offset `0 .. 37+N`).

**Não existe nenhum outro campo, header ou metadado no envelope** — o
servidor de discovery e a rede Tor nunca veem chaves, IDs de sessão nem
estruturas adicionais.

### Limites

| Limite          | Valor        | Observação                                  |
| --------------- | ------------ | ------------------------------------------- |
| mínimo          | 117 bytes    | 101 fixos + 16 da menor tag                 |
| `MAX_PLAINTEXT` | 65 536 bytes | limite do texto a cifrar (lado do emissor)  |
| `MAX_ENVELOPE`  | 65 692 bytes | limite total aceite pelo receptor           |

O `MAX_ENVELOPE` deriva do formato: `MAX_PLAINTEXT + 156`, onde 156 é o
sobretotal máximo (3 tags AEAD de 16, padding PKCS#7 de 5 e de 2, mais
os 101 bytes fixos do envelope) — ver `docs/pipeline.md` §Limites.
Mensagens acima do limite máximo são rejeitadas **antes** de processar
— nunca se aloca memória proporcional a um comprimento não validado.

---

## Assinatura

* Algoritmo: **Ed25519**, tamanho fixo **64 bytes**, posição **final**.
* Calculada sobre: `versão ‖ nonce1 ‖ nonce5 ‖ nonce9 ‖ ciphertext K9`
  (a região assinada completa, offset `0 .. 37+N`).
* **Propriedade exigida:** nenhum campo crítico do envelope pode ser
  alterado sem invalidar a assinatura — versão, qualquer nonce e
  qualquer byte do ciphertext estão todos cobertos.
* Cobrir os nonces impede ataques de *nonce-swapping*.
* Cobrir a versão dá **proteção contra downgrade**: alterar
  `versão = 0x01` para `0x02` invalida a assinatura (a deteção é
  criptográfica, não apenas estrutural).
* **Verificada antes de qualquer decifragem.** Fluxo normativo do
  receptor:

```text
receber envelope
      │
      ▼
validar formato (tamanho, versão)          ── inválido → rejeitar
      │
      ▼
selecionar chave pública do par
      │
      ▼
verificar assinatura Ed25519               ── inválida → rejeitar
      │
      ▼
K9⁻¹ → K8⁻¹ → K7⁻¹ → K6⁻¹ → K5⁻¹ → K4⁻¹ → K3⁻¹ → K2⁻¹ → K1⁻¹
      │
      ▼
plaintext UTF-8
```

Esta propriedade está fixada em testes (`tests/test_envelope.py`,
`pipeline.rs`) e é parte do contrato de segurança do protocolo.

---

## Seleção da chave de verificação

A assinatura é validada contra a **chave pública Ed25519 do par** com
que se comunicam (a chave trocada no handshake e guardada localmente).
O envelope **não** transporta a identidade do remetente — quem
fornecer a `pub` errada obtém sempre `AssinaturaInvalida`.

Consequência documentada: o envelope é válido apenas no contexto de
uma amizade já estabelecida. Mensagens de pares desconhecidos não são
processáveis (não há chave de verificação).

---

## Versionamento

O byte de versão é **opaco**, e o que decide é se esta implementação
suporta a versão recebida:

```rust
// network/daemon_rust/src/envelope.rs
pub const VERSAO: u8 = 0x01;                    // o que escrevemos
pub const VERSOES_ACEITE: &[u8] = &[VERSAO];    // o que lemos
```

Regras:

1. O receptor **rejeita** qualquer versão fora de `VERSOES_ACEITE`
   (`VersaoDesconhecida` → IPC `0x06`) — fail-closed, nunca «tentar
   assim mesmo».
2. A versão está **dentro da região assinada**, logo é autenticada.
   Aceitar uma versão que se suporta nativamente **não é downgrade** —
   a reconciliação está em [`index.md`](index.md) §Princípio da
   tolerância de versões.
3. Nenhum campo pode ser acrescentado ao envelope sem incrementar a
   versão.
4. Identificadores de algoritmo por camada **não existem hoje**: o
   pipeline é um perfil fixo (K1..K9 com os parâmetros de
   `docs/pipeline.md`) e o byte de versão identifica esse perfil. Se
   surgir um segundo perfil, recebe nova versão (ex.: `0x02`) — e passa
   a ser acrescentado a `VERSOES_ACEITE`.

### Porquê opaco, e não `major`/`minor`

Uma versão anterior deste documento descrevia o byte como `major`/`minor`
em nibbles. **A implementação nunca fez isso** — e um documento que
descreve uma convenção que o código não segue é pior do que nenhum
documento, porque passa a ser uma fonte de erro.

O envelope tem uma só versão e uma forma de crescimento. Se um dia
precisar de distinguir «layout incompatível» de «extensão compatível»,
o mecanismo é o da lista de versões: `0x01` e `0x02` podem ser ambos
compatíveis, e o aparecimento de um layout incompatível é uma decisão de
`PLANEADO`, não uma convenção a antecipar.

O IPC é diferente e **tem** `major`/`minor`, porque é um protocolo vivo
com clientes e daemons em máquinas diferentes. Ver
[`ipc_spec.md`](ipc_spec.md) §Protocolo de versão.

### Tolerância

`VERSOES_ACEITE` tem um elemento hoje. Quando existir uma v0x02,
acrescentar o byte à lista é **suficiente**: a leitura passa a aceitá-la
sem mais uma linha de código, e `tests/propriedades.rs` tem testes que
verificam precisamente isso — que a versão escrita está na lista de
leitura, que a lista é aceite, e que tudo o resto é recusado.

---

## Chaves e handshake

Todas as chaves necessárias à decifragem (`K1`, `K5`, `K9`) e a chave
pública do par são trocadas **antes do chat**, através do handshake de
amizade descrito em [`handshake.md`](handshake.md). O servidor de
descoberta apenas armazena pares `ID → endereço .onion` (com TTL) e
nunca vê chaves nem mensagens (ver [`discovery.md`](discovery.md)).

O ciclo de vida completo de cada chave está em
[`key_management.md`](key_management.md).

---

## Anti-replay

A propriedade anti-replay do chat usa o **`nonce1`** como identificador
de mensagem:

* `nonce1` tem 12 bytes aleatórios, é único por mensagem e está
  **dentro da região assinada** — não pode ser forjado nem trocado sem
  invalidar a assinatura.
* O receptor mantém um registo circular de `nonce1` vistos (com TTL),
  de memória limitada por design, **persistente entre reinícios** do
  daemon (ver §Persistência do registo).
* Uma mensagem capturada e reenviada é rejeitada no registo, **após**
  a verificação da assinatura e **antes** da decifragem.

### Parâmetros do registo

| Parâmetro          | Valor                                        |
| ------------------ | -------------------------------------------- |
| capacidade         | 4096 `nonce1` (circular, mais antigo sai)    |
| TTL                | 600 s, em memória; relógio de parede ao carregar |
| erro no reenvio    | `0x13 NonceRepetido` (IPC `DECODE`)          |
| implementação      | `anti_replay.rs` → `Estado.nonces1`          |
| ficheiro           | `$ONYXCHAT_ESTADO/anti-replay-v1.bin`        |
| formato            | `ONYXAR1` ‖ versão ‖ contagem ‖ (nonce ‖ epoch)×n |

* A ordem das etapas do `DECODE` é: parsing → assinatura → **reserva
  do `nonce1`** → decifragem K9→K1. Um forjador não pode encher o
  registo: envelopes sem assinatura válida nunca chegam à reserva.
* A reserva é *check-and-insert* atómica (mutex do daemon durante a
  decifragem) — dois reenvios simultâneos nunca passam ambos.
* Se a decifragem falhar, a reserva é **libertada**: o que estava
  errado eram as chaves apresentadas, e um retry legítimo não pode
  ficar bloqueado. Um plaintext autêntico que não seja UTF-8 mantém a
  reserva — a mensagem foi consumida e vista.

### Persistência do registo

**O registo sobrevive a um reinício do daemon.** Sem isto, o anti-replay
desaparecia durante toda a vida do processo, e um reinício era uma janela
de replay: capturar um envelope, esperar pelo reinício, reenviar. Numa
mansageira cujo objectivo declarado é resistir a captura de tráfego, era
a falha mais concreta que o projecto tinha.

| Aspecto | Decisão | Porquê |
| --- | --- | --- |
| Local | `$ONYXCHAT_ESTADO` → `$XDG_STATE_HOME/onyxchat` → `~/.local/state/onyxchat` | **não** é `/tmp`: o ficheiro diz quais `nonce1` já foram vistos, e `/tmp` é legível por qualquer conta da máquina |
| Escrita | temporário no mesmo directório → `fsync` → `rename` | o `rename` é atómico; um corte de energia deixa o registo antigo inteiro ou o novo inteiro, nunca metade |
| Permissões | `0600` na criação (`OpenOptions::mode`) | aplicadas na criação, não num `chmod` posterior que abriria uma janela |
| Formato | mágica `ONYXAR1` + versão + contagem | distingue um ficheiro **deste** formato de um ficheiro qualquer |
| Falha de leitura | registo vazio | fechar, nunca abrir. Um `Err` faria o daemon não arrancar |
| TTL ao carregar | filtra pelo timestamp de parede | o registo não cresce sem limite entre arranques |
| Tecto de leitura | `min(contagem, disponíveis, CAPACIDADE)` | um ficheiro adulterado não aloca por indicação própria |
| Erro de escrita | ignorado | uma falha de disco não pode derrubar o daemon nem uma decifragem |

**Limitações declaradas, e são reais:**

* **O TTL em memória passa a duplicar-se ao carregar.** Uma entrada
  gravada há 9 minutos e carregada agora é filtrada pelo TTL (sobrevive),
  mas ganha uma janela nova de 10 minutos em memória. O prazo efectivo
  fica entre 600 e 1200 segundos. Exacto exigiria um `Instant`
  reconstruível, e o `Instant` não sobrevive ao processo — é relativo ao
  arranque.
* **O relógio de parede é usado no carregamento.** Um salto do relógio
  para a frente encurta a janela de replay; para trás prolonga-a (e
  prolongar é falhar para o lado seguro — rejeita mais do que devia).
  Durante a execução de um processo, o TTL em memória continua monotónico.
* **Não é defence contra um atacante com acesso ao directório de
  estado.** Quem consiga escrever o ficheiro antes do arranque reintroduz
  o replay. O `0700`/`0600` é a defesa, e um atacante com escrita nessa
 _directoria_ já não é um atacante de disco: é o utilizador.

Limitação documentada: o estado anti-replay é **em memória** — depois
de um reinício do daemon, mensagens antigas poderiam ser aceites
novamente dentro da janela de TTL do transporte. Isto é uma decisão de
âmbito (o sistema não persiste histórico de mensagens, por filosofia).
A capacidade fixa acrescenta uma segunda limitação: com mais de 4096
mensagens dentro do TTL, as mais antigas saem da janela e deixam de ser
rejeitadas como replay.

---

## Exemplo

O exemplo canónico (chaves, nonces, plaintext e todos os outputs por
camada) está em [`test_vectors.md`](test_vectors.md) e nos ficheiros
`tests/vectors/*.json`. Exemplos "ilustrativos" com hex não verificável
foram removidos — não servem como especificação.

---

## Fluxo de handshake e armazenamento de chaves

1. **Pedido de amizade:** o utilizador 1 envia `FRIEND_REQUEST`
   (assinado) contendo a sua chave pública e as chaves do par.
2. **Aceitação:** o utilizador 2 responde `FRIEND_ACCEPT` (assinado)
   com a sua chave pública e as chaves dele.
3. **Armazenamento local:** cada utilizador guarda `K1`, `K5`, `K9`
   (próprias e do par) e a chave pública do outro, no keystore cifrado
   (`messenger/storage.py`).
4. **Início do chat:** só após confirmação da troca é que o chat é
   considerado seguro e funcional.

Ver [`handshake.md`](handshake.md) para os campos exatos e
[`key_management.md`](key_management.md) para o ciclo de vida.

---

## Observações finais

* Parsing sempre por comprimento — nunca por delimitadores.
* Assinatura sempre verificada **antes** da decifragem.
* Nenhum dado adicional pode ser acrescentado ao envelope sem mudar a
  versão do formato.
* Mensagens interceptadas sem assinatura válida são rejeitadas
  imediatamente.
* Mensagens já vistas (mesmo `nonce1`) são rejeitadas pelo registo
  anti-replay.
