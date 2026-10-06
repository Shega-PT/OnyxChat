# P2P — frames ponto a ponto

## Introdução

O `p2p.rs` é a fronteira entre a **rede** (TCP/Tor ou relay) e o
daemon. Tudo o que chega da rede é um *frame*; tudo o que sai para a
rede é um *frame*. Nenhum byte é interpretado sem validação de
comprimento, e nenhum erro transporta conteúdo.

A rede é **não confiável** (Resumo §31): o par pode enviar comprimentos
falsos, frames truncados ou a mais. O daemon responde com códigos de
erro estruturados, fecha ligações inutilizáveis e nunca paniqueia.

Os frames são entregues ao cliente local pelo comando `RECEBER`
(`docs/ipc_spec.md`) como `tipo(1B) ‖ corpo`.

## Transporte e enquadramento

TCP (direto, via Tor) ou stream do relay, com o **mesmo** enquadramento
do IPC local, do handshake e do relay:

```
[comprimento: u32 little-endian][tipo: 1 byte][corpo: comprimento − 1 bytes]
```

* `comprimento` inclui o byte de tipo: `1 ≤ comprimento ≤ 1 + TAM_MAX_CORPO`,
  com `TAM_MAX_CORPO`, igual ao `MAX_PAYLOAD` do IPC. Ambos são
  derivados de um orçamento de 1 GB (ver
  `network/daemon_rust/src/orcamento.rs`), não escritos à mão.
* O corpo é opaco para a camada de transporte: o `p2p.rs` nunca o abre.

## Tipos de frame

| Tipo  | Nome             | Corpo                                          | Validação do conteúdo          |
| ----- | ---------------- | ---------------------------------------------- | ------------------------------ |
| `0x01` | `CHAT`          | envelope completo K1→K9 (101+N)                | `message_format.md` (assinatura antes de decifrar) |
| `0x10` | `FRIEND_REQUEST` | 209 bytes                                      | `handshake.md` (versão + assinatura) |
| `0x11` | `FRIEND_ACCEPT`  | 177 bytes                                      | `handshake.md` (versão + assinatura) |
| `0x12` | `FRIEND_REJECT`  | 81 bytes                                       | `handshake.md` (versão + assinatura) |
| `0x20` | `PING`           | vazio                                          | —                              |
| `0x21` | `PONG`           | vazio                                          | —                              |

* Os tipos `0x10/0x11/0x12` são os `TIPO_PEDIDO/TIPO_ACEITE/TIPO_RECUSA`
  do handshake — o valor é único em todo o projeto.
* **Tipos desconhecidos não são rejeitados pelo daemon**: são entregues
  ao cliente (`RECEBER` devolve `tipo ‖ corpo` sem filtro). A validação
  de significado é de quem recebe — o cliente ignora tipos que não
  reconhece.
* `PING`/`PONG` são a utilitária de keepalive `LigacaoDireta::ping`
  (envia `PING` e espera `PONG`); o daemon não inicia keepalive por
  conta própria.
* O modo escolhido em `LIGAR` decide só o **transporte** — o formato do
  frame é idêntico no direto e no relay.

## Validação e comportamento perante dados inválidos

| Entrada inválida                            | Resultado                                              |
| ------------------------------------------- | ------------------------------------------------------ |
| `comprimento == 0`                          | `0x02 PayloadMalformado`; o cabeçalho ficou consumido e o stream **continua alinhado** — a ligação sobrevive |
| `comprimento > 1 + TAM_MAX_CORPO` (receção) | `0x09 PayloadGrandeDemais` **e a ligação é fechada** — o corpo anunciado nunca é lido (lê-lo seria vector de memória e deixaria o stream desalinhado) |
| `comprimento > 1 + TAM_MAX_CORPO` (envio)   | `0x09`; nada foi escrito, o stream fica íntegro       |
| EOF antes do primeiro byte                  | `0x0A NaoLigado` (fecho limpo do par)                 |
| EOF/timeout **a meio** de um frame          | `0x0A NaoLigado` (`Desalinhado` — o stream deixa de ser fiável e a ligação morre) |
| timeout antes do primeiro byte              | `0x0E SemMensagem` — a ligação **continua útil**; nenhum frame está a meio |
| frame com corpo acima do limite no envio    | `0x09 PayloadGrandeDemais`, antes de escrever         |
| mais de 128 envios na janela                | `0x10 RateLimit`                                      |
| resposta a `PING` que não é `PONG`          | `0x11 Relay` (erro estruturado)                       |

Toda a mensagem de erro é só metadados (código + tamanho + nome do
campo): nunca corpo de frame, chaves ou nonces (Resumo §56).

## Timeouts

| Contexto                                        | Valor                                        |
| ----------------------------------------------- | -------------------------------------------- |
| `RECEBER` (espera total)                        | `timeout_ms` do pedido; `0` = infinito       |
| Primeiro byte de um frame (receção faseada)     | 500 ms (`FATIA_ESPERA`), ou o prazo restante do pedido se for menor |
| Continuação de um frame **já começado**         | prazo restante do pedido, nunca abaixo de 5 s (`PISO_LEITURA`) |
| Espera por `OK` do relay (comando/`ENVIAR`)     | 5 s (`TIMEOUT_ACK_RELAY`)                    |

A receção é **faseada**: a espera curta pelo primeiro byte permite ao
`RECEBER` libertar o bloqueio do estado do daemon entre tentativas;
assim que chega um byte, o resto do frame é lido com espera total (com
piso de 5 s) — um prazo curto do cliente **nunca** mata o stream a meio
de um frame. Com `timeout_ms` expirado sem qualquer byte → `0x0E
SemMensagem`.

## Rate-limit (anti-flood)

* Contador de **envios por ligação** (direta e relay partilham a mesma
  implementação): no máximo `LIMITE_FRAMES = 128` frames por
  `JANELA_RATE = 10 s`.
* A janela reinicia quando expira; o contador é local a cada ligação e
  não tem estado partilhado entre ligações.
* Exceder o teto → `0x10 RateLimit` (o frame não é escrito).

## Erros (`ErroP2P` → código IPC)

| Erro `ErroP2P`                          | Código | Significado no IPC              |
| --------------------------------------- | ------ | ------------------------------- |
| `Io`, `Fechada`, `Desalinhado`, `SemLigacao` | `0x0A` | ligação inutilizável          |
| `TempoEsgotado`                         | `0x0E` | espera expirou (ligação útil)   |
| `PayloadGrande`                         | `0x09` | acima de `TAM_MAX_CORPO`        |
| `ComprimentoZero`                       | `0x02` | comprimento inválido            |
| `RateLimit`                             | `0x10` | teto de frames da janela        |
| `DestinoInvalido`                       | `0x0D` | modo/endpoint `.onion` malformado |
| `Relay`, `SemRelay`                     | `0x11` | erro do relay ou modo sem endpoint |
| `Tor(erro)`                             | código do erro Tor (`0x0B`…) | backend Tor    |

Erros **mortos** (`Fechada`, `Io`, `Desalinhado`, e `PayloadGrande` na
receção) limpam a ligação e o espelho `ligado` do `ESTADO` — os outros
deixam a ligação ativa.

## Estado da ligação

```text
sem ligação ──OUVIR──► a escuta (thread de aceitação) ──aceite──► ligado
     ▲                                                          │
     └────────────── FECHAR / erro morto ◄──────────────────────┘

sem ligação ──LIGAR (direto .onion | relay)──► ligado
```

* `OUVIR` abre o servidor P2P local e publica o hidden service
  (idempotente). A ligação de entrada **mais recente substitui** a
  anterior (o par reconectou).
* `LIGAR` com uma ligação já ativa → `0x0F EstadoInvalido` («use
  `FECHAR` primeiro»); modo direto exige destino `.onion` (`0x0D`).
* `FECHAR` fecha a ligação e é idempotente.
* Só existe **uma** ligação ativa por daemon; a sessão de rede
  sobrevive a clientes IPC que fechem.

## Encapsulamento no relay

No modo relay o frame é depositado na mailbox do par:
`ENVIAR = mailbox_par(32) ‖ tipo(1) ‖ corpo`, e o relay devolve frames
em `0x84 CAIXA = tipo(1) ‖ corpo` (ver `relay.md`). O relay vê
apenas o hash da mailbox, os tamanhos e os bytes cifrados.

## Metadados observáveis (reconhecimento explícito)

* Em ambos os modos o transporte vê: comprimentos, tipos de frame,
  timestamps e os ciphertexts dos envelopes.
* O relay vê, adicionalmente: `SHA-256("ONYX/RELAY/v1" ‖ pub)` e
  padrões de tráfego (sem TTL próprio além do buffer de 300 s).
* Nenhum dos dois vê chaves, plaintext nem identidades em claro.

## Documentos relacionados

| Documento              | Conteúdo                                     |
| ---------------------- | -------------------------------------------- |
| [`ipc_spec.md`](ipc_spec.md)      | comandos locais (`ENVIAR`/`RECEBER`/`LIGAR`) |
| [`handshake.md`](handshake.md)    | corpos e assinaturas dos frames `0x10..0x12`  |
| [`message_format.md`](message_format.md) | envelope do frame `CHAT`               |
| [`relay.md`](relay.md)            | transporte de fallback e mailbox             |
| [`threat_model.md`](threat_model.md)   | flooding e transporte não confiável       |
