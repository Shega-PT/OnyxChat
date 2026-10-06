# Relay/TURN (fallback de transporte)

## Introdução

Quando a ligação **direta** entre dois hidden services falha (NAT agressivo,
parque de saída bloqueado, arranque lento da rota Tor), o OnyxChat usa um
**relay** como ponto de rendezvous. O relay é um relé **cieco**: só vê
bytes cifrados pelo pipeline K1→K9 e nunca participa no handshake de
amizade nem tem chaves.

O relay está **implementado** em Python (`server/relay_server.py`,
asyncio da stdlib) e pode correr:

* em `localhost` (testes/dev),
* em clearnet (instância pública configurada).

O endpoint é **configurável** pelo utilizador (config do cliente ou
`$ONYXCHAT_RELAY`).

### O relay em clearnet expõe o IP — e o daemon avisa

Uma ligação a um relay alcançável é um TCP **directo**: o relay e quem
estiver no caminho veem o IP real do utilizador. Não há modo de evitar
isto — um relay alcançável directamente só pode ser alcançado
directamente. É a contrapartida de ter um relay quando a ligação entre
hidden services não se establish.

O que o daemon garante é que o custo é **declarado**:

* o `endpoint` é **validado** antes de qualquer I/O (`Endpoint::validar`,
  `p2p.rs`). Sem validação, um endpoint malformado dava um erro de
  resolução de DNS em vez de um erro de validação — indistinguível de
  «o relay está em baixo» — e a query DNS saía do sistema, fora do Tor;
* o daemon escreve um **aviso no `stderr` antes de ligar**, com o
  destino. O aviso é anterior à ligação, e não um efeito dela: se
  dependesse da ligação, nunca apareceria no caso que interessa — o
  caso em que ela tem sucesso.

O aviso não contém material criptográfico (o relay vê a mailbox e o
ciphertext, nunca chaves), e a sua escrita **não pode derrubar o
daemon**: se o `stderr` estiver fechado ou o disco cheio, a ligação
continua e o aviso é perdido. Perder um aviso é melhor do que uma
queda.

### O relay em `.onion` **não** funciona

Uma versão anterior deste documento e de `privacy_model.md` §3 listava
«atrás de uma hidden service (relay `.onion`)» como modo de deployment.
**Não é implementável com o código actual.** O modo relay abre um TCP
directo (`LigacaoRelay::abrir`); um endereço `.onion` não é resolvido
por esse caminho, e `Endpoint::validar` aceita o formato sem que a
ligação funcione.

Faria falta um `BackendTor::ligar` para o endpoint do relay, com o
proxy inverso que o `tor_arti.rs` já faz para o servidor P2P local. É
trabalho por fazer, não uma opção de configuração.

---

## Identidade e mailbox

* **ID de utilizador no discovery** = `hex(pub Ed25519)` — 64 caracteres
  hexadecimais minúsculos; cabe no regex do discovery
  (`^[A-Za-z0-9._-]{1,64}$`) e permite derivar o mailbox sem conhecer
  chaves de cifragem.
* **mailbox** = `SHA-256("ONYX/RELAY/v1" ‖ pub_do_destinatário)` —
  32 bytes. O hash é **estável e não-reversível**: o relay guarda o
  mailbox, não a identidade; quem não conhece a `pub` do destinatário
  não consegue depositar mensagens na sua caixa.

---

## Transporte e enquadramento

TCP (ou stream Tor equivalente) com enquadramento por comprimento,
**idêntico** ao IPC local:

```
[comprimento: u32 little-endian][tipo: 1 byte][corpo: comprimento-1 bytes]
```

* `comprimento` inclui o byte de tipo; `1 ≤ comprimento ≤ 1 + TAM_MAX_CORPO`
  (derivado do mesmo orçamento de 1 GB que o IPC —
  `network/daemon_rust/src/orcamento.rs`).
* Comprimentos a `0` ou acima do limite fecham a ligação.
* Não há mensagens de log com conteúdo — apenas tipos e tamanhos.

---

## Mensagens do protocolo

### Cliente → relay

| Tipo  | Nome       | Corpo                                          |
|-------|------------|------------------------------------------------|
| `0x01`| `SUBSCREVER` | `mailbox(32)`                                |
| `0x02`| `ENVIAR`     | `mailbox(32) ‖ frame_tipo(1) ‖ frame_corpo(N)` (N ≥ 1) |
| `0x03`| `FECHO`      | vazio                                        |

### Relay → cliente

| Tipo  | Nome       | Corpo                                          |
|-------|------------|------------------------------------------------|
| `0x81`| `ERRO`      | `código: 1 byte ‖ mensagem UTF-8 (opcional)` |
| `0x84`| `CAIXA`     | `frame_tipo(1) ‖ frame_corpo(N)` — *push* ao subscritor |
| `0x83`| `OK`        | vazio — confirmação de `SUBSCREVER`/`ENVIAR`   |

> **O corpo de `ENVIAR` e de `CAIXA` embute o frame P2P completo**, não
> só o envelope. O byte `frame_tipo` é um tipo de
> [`p2p.md`](p2p.md) §Tipos de frame — `0x01 CHAT` para mensagens, ou
> `0x10`/`0x11`/`0x12` para os frames de handshake. Uma versão anterior
> deste documento omitia esse byte e descrevia o corpo como
> `envelope(N)`, o que não descrevia o que o daemon produz
> (`p2p.rs`, `enfileirar_caixa`).

* `SUBSCREVER` liga a ligação à mailbox: todos os `ENVIAR` posteriores
  para essa mailbox são encaminhados como `0x84 CAIXA`.
* Uma ligação pode subscrever **uma** mailbox (a mais recente substitui a
  anterior — re-subscrição).
* `ENVIAR` entrega de forma **fire-and-forget**: se não houver
  subscritor, o frame fica pendente até ao TTL.

### Códigos de erro (`0x81`)

| Código | Nome                     | Significado                                |
|--------|--------------------------|--------------------------------------------|
| `0x01` | `PayloadMalformado`      | corpo curto/ausente para o tipo            |
| `0x02` | `MailboxInvalida`        | mailbox ≠ 32 bytes                         |
| `0x03` | `PayloadGrandeDemais`    | acima de `TAM_MAX_CORPO`                   |
| `0x04` | `SemEspaco`              | caixa cheia (máx. 64 envelopes pendentes)  |
| `0x05` | `RateLimit`              | demasiadas mensagens por intervalo        |
| `0x06` | `ComandoDesconhecido`    | tipo fora de {0x01, 0x02, 0x03}            |

---

## Regras de retenção

A retenção do relay é **buffer temporário de transporte**, não
armazenamento de mensagens:

```text
buffer temporário  ≠  message storage
```

* **TTL** de cada frame pendente: **300 s** (5 minutos). Passado o
  TTL o relay descarta o frame silenciosamente.
* **Máximo de frames pendentes por mailbox: 64.** Excedido →
  `SemEspaco` (o mais antigo **não** é descartado: nada se perde em
  silêncio; o emissor decide reenviar).
* **Rate-limit** por ligação: no máximo 32 `ENVIAR` por segundo de
  janela — acima disso `RateLimit`.
* Ao entregar um `CAIXA`, o frame é removido da caixa (entrega
  única — sem duplicados).
* O relay **não** mantém histórico, conversações, sessões criptográficas
  nem qualquer registo de quem falou com quem para além da vida útil
  da caixa (TTL).

---

## Metadados observáveis (reconhecimento explícito)

Cifrar o conteúdo **não** elimina automaticamente todos os
metadados. Quando o relay corre em clearnet, ele observa:

* **endereços IP** das ligações de entrada dos clientes;
* **correlação de tráfego** — que mailbox enviou para que mailbox e
* quando (timing, frequência, tamanho dos envelopes).

Isto é uma propriedade do **modo de transporte**, não uma falha do
pipeline: o conteúdo continua inacessível. Quem queira eliminar esta
superfície hoje só tem uma opção — **não usar relay**, e falar
directo por `.onion` (ver [`p2p.md`](p2p.md)).

Correr o relay atrás de uma hidden service resolveria, mas **não está
implementado** (ver §«O relay em `.onion` não funciona», acima).

O modelo de privacidade completo (o que é protegido e o que não é)
está em [`privacy_model.md`](privacy_model.md).

---

## Segurança e privacidade

1. O relay **nunca** vê plaintext: os envelopes já saem cifrados com
   K1→K9 e assinados; o relay não tem chaves.
2. O relay **não** participa no handshake (`docs/handshake.md`) — as
   chaves trocam ponto a ponto pelo P2P/Tor.
3. **Sem log de conteúdo**: o servidor regista apenas tipos, tamanhos e
   contadores — nunca bytes de envelope. Os endereços IP de ligação
   são observáveis *em memória* (inerente ao TCP) mas **não** são
   escritos em ficheiros de log; a exposição de IP que importa é a do
   modo de deployment (ver §Metadados observáveis).
4. O relay não autentica emissores (é ciego por desenho): a segurança
   fim-a-fim vem do pipeline e da assinatura Ed25519, verificadas **antes**
   de qualquer chave ser aceite.

---

## Fluxo de fallback

```
 A ── tenta ligação direta (.onion do B) ── timeout (ex. 10 s)
 │
 └─ falhou → A liga ao relay:
      SUBSCREVER mailbox(A) ──┐
      ENVIAR mailbox(B), env  │
                              ▼
                        ┌───────────┐
                        │   relay   │
                        └───────────┘
                              │
      SUBSCREVER mailbox(B) ──┘  (B já ligado, a escuta)
      ← CAIXA envelope            B processa o envelope
      ← (B responde via mailbox(A)) A recebe a resposta
```

Cada lado abre **uma** ligação ao relay (subscrição da própria mailbox);
as mensagens seguem o pipeline normal e chegam intactas.

---

## Observações finais

* O discovery server **nunca** fala com o relay: um mapeia
  `ID → .onion`, o outro encaminha bytes cifrados.
* Relay e discovery são **infraestrutura de transporte/bootstrap
  opcional e substituível** — nenhum deles é servidor central de
  mensagens nem proprietário de conversas.
* O relay **não é proprietário da conversa**: não precisa de armazenar
  mensagens, não possui as chaves de conteúdo e é apenas uma opção de
  transporte (Modo B). O Modo A (`A ↔ B` direto) não usa relay.
* Remover o relay degrada a *funcionalidade de fallback* e pode
  alterar a superfície de observação do modo deployment — pelo que a
  frase correta não é "nunca afeta segurança", mas: **a segurança fim-a-fim
  (pipeline + assinatura) mantém-se, porque não depende do relay**; o
  que muda é a disponibilidade e a topologia de transporte.
