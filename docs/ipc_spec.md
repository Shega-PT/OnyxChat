# Especificação de IPC Python ↔ Rust

## Introdução

Este documento é a **especificação de protocolo** do IPC local do
OnyxChat — não é apenas documentação de apoio. Define o enquadramento,
cada comando (opcode, direção, campos, comprimento, encoding, resposta,
erro, timeout e comportamento perante dados inválidos), a fronteira de
confiança e a política de erros. Uma implementação independente deve
conseguir falar com o daemon só com este documento.

O IPC é usado exclusivamente **dentro da mesma máquina** para cifrar
(`ENCODE`) e decifrar (`DECODE`) mensagens e para orquestrar a rede
antes de os dados serem enviados via Tor.

Todos os dados do IPC são sensíveis (chaves, plaintext) e o protocolo
foi desenhado para: enquadramento impossível de ambiguar, erros nunca
derrubarem o daemon, e nenhum dado sensível em logs.

---

## Fronteira de confiança

```text
Python client
      │
      ▼
     UDS          ← fronteira 1: permissões 0600 + SO_PEERCRED (uid)
      │
      ▼
 Rust daemon      ← fronteira 2: validação de opcode/comprimento/campos
      │
      ▼
    Tor           ← fronteira 3: BackendTor (Arti ou backend falso)
      │
      ▼
   rede
```

Cada fronteira tem formato, validação, erros, limites e comportamento
perante dados inválidos. O cliente Python **não pode contornar as
regras de segurança do daemon** por acesso ao socket: o daemon valida
tudo, independentemente de quem liga.

### Transporte (UDS)

* **Caminho:** `$ONYXCHAT_SOCKET` se definido; caso contrário
  `/tmp/onyxchat-{uid}.sock`.
* **Permissões:** ficheiro `0600`, criado numa diretoria própria.
* **Autenticação local:** o daemon verifica o **uid do processo par**
  via `SO_PEERCRED` e recusa qualquer ligação de outro utilizador,
  fechando o socket imediatamente **sem resposta** (um cliente não
  autorizado não recebe nem confirmação nem código de erro — a
  inexistência de resposta é a resposta).
* **Âmbito:** apenas local; mensagens entre utilizadores passam por
  Tor/.onion, nunca por aqui.

### Enquadramento (framing)

Cada mensagem no stream é:

```text
[comprimento: u32 little-endian][carga útil: comprimento bytes]
```

* `comprimento ≤ MAX_PAYLOAD` — acima disso o daemon
  responde `0x09 PayloadGrandeDemais` e **fecha** a ligação (proteção
  contra *memory exhaustion*: o corpo nunca é alocado).
* `comprimento == 0` → resposta `0x02 PayloadMalformado`.
* Mensagens síncronas: o cliente envia um pedido e bloqueia até à
  resposta; várias trocas podem acontecer na mesma ligação.

### Timeouts

| Contexto                | Timeout                       |
| ----------------------- | ----------------------------- |
| Pedido qualquer (exceto `RECEBER`) | sem timeout — o daemon responde imediatamente |
| `RECEBER`               | `timeout_ms` do pedido (`0` = infinito) |
| `LIGAR`                 | interno do backend (Tor/relay) |
| `OUVIR`                 | interno do backend (Tor)      |
| Fecho de ligação por erro de I/O | imediato                |

O daemon **nunca bloqueia indefinidamente a processar** um pedido:
apenas `RECEBER` espera, e só pelo valor que o próprio cliente pediu.

---

## Protocolo de versão (HELLO)

A sessão começa com um `HELLO` obrigatório. Isto dá ao IPC uma versão
explícita e autenticável, em vez de depender de "ambos têm a mesma
aplicação".

```text
cliente → daemon :  comando 0x00 ‖ versao(1B)
daemon  → cliente :  OK  ‖ versao_do_cliente(1B) ‖ build(UTF-8)
                     ou ERRO 0x12 VersaoIncompativel ‖ mensagem
```

* Versão actual: `0x01` (`major` no nibble alto, `minor` no baixo).
* **Regra de compatibilidade:** `major` igual → aceite; `major`
  diferente → `0x12`. O `minor` pode diferir em qualquer valor.

### Porquê tolerar o `minor`

Aplicação do princípio de
[`index.md`](index.md) §Princípio da tolerância de versões:

```text
major igual  + minor diferente  →  ACEITE
major diferente                  →  0x12 VersaoIncompativel
```

Uma alteração de `minor` **tem de continuar compatível** — só se
acrescenta, nunca muda a semântica do que já existia. É esse o
compromisso que justifica o nibble `minor` existir: dá espaço para
evoluir sem partir o mundo.

O que isto resolve na prática: sem tolerância de `minor`, um patch
release do daemon partia **todos** os clientes instalados, porque
`0x01` e `0x02` seriam ambos «incompatíveis» com `0x01`. Um utilizador
com software antigo deixaria de poder falar com o sistema.

### A resposta ecoa a versão do **cliente**

O `OK` devolve a versão **enviada pelo cliente**, não a do daemon. Quem
lê a resposta precisa de confirmar que a *sua* versão foi aceite; se o
daemon devolvesse a dele, o cliente não teria como saber.

* O `HELLO` é o **primeiro pedido de qualquer ligação**. Pedidos
  anteriores a um `HELLO` válido são rejeitados com `0x02` — o portão
  precede a tabela de comandos (um comando desconhecido enviado sem
  `HELLO` devolve `0x02`, não `0x01`).
* Um `HELLO` **inválido não abre a sessão**: a ligação continua sem
  sessão e o comando seguinte volta a ser rejeitado com `0x02` até um
  `HELLO` válido. Um `HELLO` **válido repetido** é idempotente
  (revalida a versão); uma versão incompatível a meio da sessão volta a
  fechá-la.
* O cliente valida a versão ecoada pela **mesma** regra de
  compatibilidade (`major`), e rejeita com `0x12` se for incompatível.
* A mensagem de `0x12` nomeia as duas versões. Um número de versão é
  público, não é segredo, e sem ele o utilizador não sabe que software
  tem do outro lado — o que é incompatível com a política de erros sem
  vazamentos, que proíbe segredos, não diagnosis.

---

## Pedidos (cliente → daemon)

```text
[comando: 1 byte][corpo]
```

A coluna **Comprimento** da tabela indica o comprimento **total da
mensagem** (opcode incluído), mínimo possível — ou seja,
`1 + len(corpo mínimo)`.

| Comando             | Valor  | Direção        | Corpo do pedido                                              | Comprimento | Encoding            |
| ------------------- | ------ | -------------- | ------------------------------------------------------------ | ----------- | ------------------- |
| `HELLO`             | `0x00` | cliente→daemon | `versao(1)`                                                  | 2           | byte                |
| `ENCODE`            | `0x01` | cliente→daemon | `k1(32) ‖ k5(32) ‖ k9(32) ‖ seed(32) ‖ mensagem`             | ≥ 129       | binário + UTF-8     |
| `DECODE`            | `0x02` | cliente→daemon | `k1(32) ‖ k5(32) ‖ k9(32) ‖ pub(32) ‖ envelope`              | ≥ 246       | binário             |
| `ESTADO`            | `0x03` | cliente→daemon | vazio                                                        | 1           | —                   |
| `OUVIR`             | `0x04` | cliente→daemon | vazio                                                        | 1           | —                   |
| `LIGAR`             | `0x05` | cliente→daemon | `modo:1B ‖ u16LE(len) ‖ endpoint ‖ u16LE(len) ‖ destino ‖ pub_propria(32) ‖ pub_par(32)` | ≥ 70 | UTF-8 com prefixo `u16 LE` |
| `ENVIAR`            | `0x06` | cliente→daemon | `envelope` (bytes completos)                                 | ≥ 118       | binário             |
| `RECEBER`           | `0x07` | cliente→daemon | `timeout_ms: u32 LE`                                         | 5           | inteiro             |
| `FECHAR`            | `0x08` | cliente→daemon | vazio                                                        | 1           | —                   |
| `PEDIR_AMIZADE`     | `0x09` | cliente→daemon | `seed(32) ‖ pub_dest(32)`                                    | 65          | binário             |
| `ACEITAR_AMIZADE`   | `0x0A` | cliente→daemon | `seed(32) ‖ request_corpo(209)`                              | 242         | binário             |
| `RECUSAR_AMIZADE`   | `0x0B` | cliente→daemon | `seed(32) ‖ nonce(16)`                                       | 49          | binário             |
| `CONFIRMAR_AMIZADE` | `0x0C` | cliente→daemon | `seed(32) ‖ accept_corpo(177)`                               | 210         | binário             |

Notas:

* `seed` = 32 bytes da chave privada Ed25519 do remetente (para
  assinar). Ciclo de vida em `key_management.md`.
* `pub` = 32 bytes da chave pública Ed25519 esperada.
* Cadeias UTF-8 no corpo (`endpoint`, `destino`) são prefixadas por
  `u16 LE` com o comprimento em bytes (`tirar_cadeia`). Cadeia vazia é
  `u16 = 0` (o campo continua presente).
* `ENCODE` limita a `mensagem` a `MAX_PLAINTEXT` (65 536 bytes) —
  acima → `0x09 PayloadGrandeDemais`, **antes** de cifrar.
* `DECODE` limita o envelope a `MAX_ENVELOPE` (65 692 bytes) — acima →
  `0x09`, **antes** de alocar/processar.
* `DECODE` verifica a assinatura **antes** de decifrar e consulta o
  registo anti-replay de `nonce1` entre as duas etapas — reenvio →
  `0x13 NonceRepetido` (`message_format.md` §Anti-replay).
* `DECODE` mantém o mutex do registo anti-replay durante a decifragem
  (janela *check-and-insert* atómica).

### Respostas por comando (`OK`, `0x00`)

| Comando             | Corpo da resposta `OK`                                        | Tamanho |
| ------------------- | ------------------------------------------------------------- | ------- |
| `HELLO`             | `versao(1) ‖ build UTF-8`                                     | ≥ 2     |
| `ENCODE`            | envelope completo (101+N)                                      | ≥ 117   |
| `DECODE`            | plaintext UTF-8                                                | ≥ 0     |
| `ESTADO`            | `tor:1B ‖ ligado:1B ‖ amigos:u16 LE ‖ onion UTF-8`           | ≥ 4     |
| `OUVIR`             | `onion: UTF-8`                                                 | ≥ 0     |
| `LIGAR`             | vazio                                                          | 1       |
| `ENVIAR`            | vazio                                                          | 1       |
| `RECEBER`           | `tipo:1B ‖ corpo` (frame)                                      | ≥ 1     |
| `FECHAR`            | vazio (idempotente)                                            | 1       |
| `PEDIR_AMIZADE`     | `k1(32) ‖ k5(32) ‖ k9(32) ‖ request_corpo(209)`               | 305     |
| `ACEITAR_AMIZADE`   | `k1‖k5_b‖k9_b‖k5_a‖k9_a‖pub_a (192) ‖ accept_corpo(177)`      | 369     |
| `RECUSAR_AMIZADE`   | `reject_corpo(81)`                                             | 81      |
| `CONFIRMAR_AMIZADE` | `k1‖k5_a‖k9_a‖k5_b‖k9_b‖pub_b` (`TAM_AMIZADE`)                | 192     |

### Tipos de frame P2P (corpo de `RECEBER`)

| Tipo   | Nome             | Corpo                                            | Tamanho     |
| ------ | ---------------- | ------------------------------------------------ | ----------- |
| `0x01` | `CHAT`           | `envelope` (101+N)                                | ≥ 117       |
| `0x10` | `FRIEND_REQUEST` | `versao(1)‖pub(32)‖k1(32)‖k5(32)‖k9(32)‖nonce(16)‖sig(64)` | 209 |
| `0x11` | `FRIEND_ACCEPT`  | `versao(1)‖pub(32)‖k5(32)‖k9(32)‖nonce(16)‖sig(64)` | 177     |
| `0x12` | `FRIEND_REJECT`  | `versao(1)‖nonce(16)‖sig(64)`                     | 81          |
| `0x20` | `PING`           | vazio                                             | 1           |
| `0x21` | `PONG`           | vazio                                             | 1           |

Sem ligação ativa, `ENVIAR`/`RECEBER` devolvem `0x0A NaoLigado`.
`endpoint_relay` vazio → endpoint por omissão.

> **Nota sobre a coluna `Tamanho`:** conta o corpo **sem** o byte `OK`
> (`0x00`) inicial. É por isso que `LIGAR`/`ENVIAR`/`FECHAR` valem 1
> (corpo vazio ⇒ 1 byte de resposta total) e `ESTADO` vale 4.

### `destino` — `.onion` ou ID hex

`destino` tem semântica **dependente do modo**, e a distinção é
deliberada:

| Modo | `destino` aceite | O que o daemon faz com ele |
| --- | --- | --- |
| `direto` | `xxxx.onion` **apenas** | pede ao `BackendTor` que ligue a esse endereço |
| `relay` | ID hex de 64 caracteres (a `pub` do par) | **valida** que `pub_de_hex(destino) == pub_par` e deriva a mailbox do par |

Um ID hex em modo `directo` produz `0x0D DestinoInvalido`, porque o
daemon não tem cliente de discovery — a resolução `ID → .onion` é feita
**pelo cliente Python** antes de emitir `LIGAR` (`messenger/`). Ver
[`SYS_GUIDE.md`](SYS_GUIDE.md) §6.

Em modo `relay`, `destino` é redundante por construção (a `pub` já vem
no corpo), e a validação existe para detectar um pedido incoerente: um
`destino` que não corresponda a `pub_par` é `0x0D`, não um envelope
entregue à mailbox errada.

**Validação da forma (normativa):** um `.onion` só é aceite com 56
caracteres de base32 (`a-z`, `2-7`) seguidos de `.onion`. Um `.onion`
curto, ou com caracteres fora do alfabeto, é `0x0D`. A validação é
estrita nas duas direções de propósito: aceitar «qualquer coisa que
acabe em `.onion`» transformaria a validação em decorado, e um pedido
incoerente que passa despercebido é pior do que um pedido recusado.

**`PLANEADO` (F2.1).** A validação `pub_de_hex(destino) == pub_par` e
a resolução `ID → .onion` no cliente Python ainda não estão
implementadas. Estado actual: `pub_de_hex()` existe em `p2p.rs` mas é
código morto; em modo relay o `destino` é simplesmente ignorado.

### Comandos de handshake

| Comando             | Ação                                                          |
| ------------------- | ------------------------------------------------------------- |
| `PEDIR_AMIZADE`     | monta `FRIEND_REQUEST` assinado com a `seed`                   |
| `ACEITAR_AMIZADE`   | valida `FRIEND_REQUEST` (versão + assinatura + anti-replay) e monta o `ACCEPT` |
| `RECUSAR_AMIZADE`   | monta `FRIEND_REJECT` assinado sobre o `nonce` do pedido       |
| `CONFIRMAR_AMIZADE` | valida `FRIEND_ACCEPT` (versão + assinatura) e fecha a amizade |

* As chaves devolvidas são **persistidas pelo cliente** no keystore
  cifrado (`messenger/storage.py`); o daemon guarda também a amizade
  em memória (rotação: um novo `FRIEND_REQUEST` da mesma `pub`
  substitui as chaves antigas).
* Handshake **nunca** passa pelo pipeline K1→K9 — são mensagens de
  controlo assinadas (`docs/handshake.md`).

---

## Respostas (daemon → cliente)

```text
[estado: 1 byte][corpo]
```

| Estado  | Valor  | Corpo                                                |
| ------- | ------ | ---------------------------------------------------- |
| `OK`    | `0x00` | corpo do comando (ver tabela acima)                   |
| `ERRO`  | `0x01` | `código: 1 byte ‖ mensagem-legível: UTF-8` (opcional) |

### Códigos de erro

| Código | Nome                      | Significado                                       | Quando ocorre                          |
| ------ | ------------------------- | -------------------------------------------------- | -------------------------------------- |
| `0x01` | `ComandoDesconhecido`     | opcode fora da tabela                              | qualquer byte não listado              |
| `0x02` | `PayloadMalformado`       | corpo curto demais para o comando                  | `len` inesperado                       |
| `0x03` | `ChaveInvalida`           | tamanho/campo de chave inválido                    | **não alcançável por um pedido bem formado** — ver nota |
| `0x04` | `AssinaturaInvalida`      | Ed25519 rejeitada                                  | `DECODE`/handshake                     |
| `0x05` | `DecifragemFalhou`        | tag AEAD inválida                                  | `DECODE`                               |
| `0x06` | `EnvelopeInvalido`        | versão/comprimento do envelope errados             | `DECODE`                               |
| `0x07` | `CifragemFalhou`          | falha interna AEAD                                 | `ENCODE`                               |
| `0x08` | `TextoInvalidoUtf8`       | plaintext/corpo não é UTF-8 válido                 | `ENCODE`                               |
| `0x09` | `PayloadGrandeDemais`     | acima de `MAX_PAYLOAD`/`MAX_PLAINTEXT`/`MAX_ENVELOPE` | qualquer comando                     |
| `0x0A` | `NaoLigado`               | operação de rede sem ligação ativa                 | `ENVIAR`/`RECEBER`                     |
| `0x0B` | `TorIndisponivel`         | backend Tor indisponível                           | `OUVIR`/`LIGAR`                        |
| `0x0C` | `HandshakeInvalido`       | corpo/assinatura/nonce/versão de handshake inválido | comandos `0x09..0x0C`                 |
| `0x0D` | `DestinoInvalido`         | `.onion`/ID/endereço malformado                    | `LIGAR`                                |
| `0x0E` | `SemMensagem`             | `RECEBER` expirou sem frames                       | `RECEBER` com timeout                  |
| `0x0F` | `EstadoInvalido`          | comando incoerente com o estado atual              | `LIGAR` com ligação já ativa; `CONFIRMAR_AMIZADE` sem pedido pendente |
| `0x10` | `RateLimit`               | teto de frames/janela excedido                     | `p2p`, relay                           |
| `0x11` | `Relay`                   | modo relay sem endpoint ou erro do relé            | `LIGAR`/`ENVIAR` em modo relay         |
| `0x12` | `VersaoIncompativel`      | versão do protocolo IPC incompatível                | `HELLO`                                |
| `0x13` | `NonceRepetido`           | `nonce1` de chat já visto (anti-replay)            | `DECODE`                               |

### Notas sobre dois códigos

**`0x03 ChaveInvalida` — defensivo, não alcançável pelo framing.**
Os comandos `ENCODE`/`DECODE` recebem as chaves em campos de tamanho
fixo: o parser (`partes_fixas`) recorta exactamente `[u8; 32]` de cada
campo e falha com `0x02 PayloadMalformado` se o corpo não tiver
comprimento exacto. Logo, `TamanhoDeChaveInvalido`
(`crypto/rust/src/error.rs`) nunca é alcançado a partir do IPC. Isto é
**fail-closed por construção** — o ataque é rejeitado um passo antes, no
framing. O código e o mapeamento existem para defesa em profundidade e
para uma futura alteração do formato que passe a aceitar chaves de
comprimento variável.

**`0x0F EstadoInvalido` — `FECHAR` é idempotente.**
`FECHAR` devolve `0x00 OK` mesmo sem ligação activa: fechar uma sessão
que já está fechada é um pedido bem formado, não um erro. Uma versão
anterior deste documento dava `FECHAR` sem ligação como exemplo de
`0x0F`; está corrigido. A idempotência é coerente com
[`p2p.md`](p2p.md) §Estado da ligação e é testada.

**Política de erros:** as mensagens de erro **nunca** incluem chaves,
nonces, plaintext, ciphertext, caminhos de ficheiros nem detalhes de
infraestrutura. São mensagens curtas e fixas — o detalhe sensível
fica no log interno do daemon como *metadados* (código + tamanhos).

---

## Validação por comando (comportamento perante dados inválidos)

| Entrada inválida                     | Resultado                                |
| ------------------------------------ | ---------------------------------------- |
| `comprimento == 0`                   | `0x02 PayloadMalformado`                 |
| corpo < mínimo do comando            | `0x02 PayloadMalformado`                 |
| qualquer comando antes do `HELLO`    | `0x02 PayloadMalformado`                 |
| `HELLO` com corpo ≠ `versao(1)`      | `0x02 PayloadMalformado`                 |
| `HELLO` (ou corpo do OK) ≠ `0x01`    | `0x12 VersaoIncompativel`                |
| opcode desconhecido                  | `0x01 ComandoDesconhecido`               |
| qualquer campo acima do limite       | `0x09 PayloadGrandeDemais`               |
| `ENCODE` com não-UTF-8               | `0x08 TextoInvalidoUtf8`                 |
| `DECODE` com versão de envelope ≠ 0x01 | `0x06 EnvelopeInvalido`                |
| `DECODE` com `nonce1` já visto       | `0x13 NonceRepetido`                     |
| assinatura inválida                  | `0x04 AssinaturaInvalida`                |
| uid do par ≠ uid do daemon           | ligação fechada, **sem resposta**        |
| dados que causem erro interno        | código fixo correspondente, **nunca panic** |

O daemon **nunca aborta** por dados do cliente — todas as falhas
viram respostas `ERRO` tipadas.

Nota sobre "envelope curto": via IPC este caso **colapsa** na regra
`corpo < mínimo do comando`. O corpo mínimo do `DECODE` (246 =
1 + 4·32 + 117) já inclui o envelope mínimo de 117 bytes, pelo que um
envelope curto nunca chega ao parser — a resposta é `0x02
PayloadMalformado`. O `0x06 EnvelopeInvalido` aplica-se a envelopos
com comprimento válido mas **versão** inválida, e ao caminho de
parsing do próprio envelope (mín → máx → versão), usado internamente
pelo pipeline.

---

## Assincronismo

* **Opção escolhida:** síncrono — o cliente envia um pedido e espera a
  resposta antes de continuar. Adequado a um chat local com um
  utilizador por máquina; evita estado de correlação de IDs.
* Várias ligações podem estar ativas em paralelo (uma thread por
  ligação); a sessão de rede é partilhada e sobrevive a clientes que
  fechem.

---

## Tratamento de erros

* **Opção escolhida:** retorno estruturado + logging interno.
* Erros esperados viram `estado=ERRO` + código; o daemon nunca aborta.
* O daemon regista apenas código + metadados de tamanho — nunca chaves,
  plaintexts ou ciphertexts.
* O cliente expõe tudo como `ErroIpc`/`ErroDaemon` tipados — nunca
  exceções genéricas (`messenger/ipc_client.py`).

---

## Exemplo de fluxo (`ENCODE`)

Mensagem: `"Olá mundo!"` (11 bytes UTF-8).

**Pedido:**

```text
enquadramento : 8c 00 00 00             ; comprimento = 140 = 1 + 128 + 11
corpo         : 01                      ; comando ENCODE
                <k1 32B> <k5 32B> <k9 32B> <seed 32B>
                4f 6c c3 a1 20 6d 75 6e 64 6f 21   ; "Olá mundo!"
```

Comprimento derivado, nunca pré-calculado: `1 (opcode) + 4×32 (campos
fixos) + 11 (mensagem) = 140`.

**Resposta:**

```text
enquadramento : <u32 LE do tamanho>
corpo         : 00                     ; estado OK
                01 <nonces 36B> <ciphertext N B> <assinatura 64B>
```

---

## Observações finais

* O IPC é **estritamente local**, síncrono e autenticado (uid).
* O pipeline de cifragem vive no daemon Rust, que orquestra Rust (K1,
  K5, K7), C/C++ (K4, K9) e Lua (K2, K3, K6, K8).
* Não existem campos além dos descritos; mensagens entre utilizadores
  ocorrem **apenas** via Tor/.onion.
* Este documento é a fonte de verdade do protocolo — `ipc.rs` e
  `ipc_client.py` implementam-no, e `docs/testing.md` descreve como é
  validado.
