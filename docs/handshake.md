# Handshake de Amizade (Troca de Chaves)

## Introdução

Antes de qualquer mensagem de chat, dois utilizadores trocam chaves
através de um **handshake de amizade**. O handshake ocorre **sobre a
ligação P2P já estabelecida** (hidden service Tor) e é protegido por
**assinaturas Ed25519** — não há dependência de TLS nem de um terceiro
de confiança.

Todas as mensagens de handshake são **assinadas pelo remetente** e o
receptor valida a assinatura **antes** de persistir qualquer chave.

> **Nota de confidencialidade.** A assinatura dá autenticidade e
> integridade, **não** confidencialidade: `K1`, `K5` e `K9` trafegam
> em claro *dentro* da ligação P2P. A confidencialidade destes campos
> depende exclusivamente da confidencialidade do transporte (Tor). O
> handshake não cria privacidade adicional sobre o canal.

---

## Transcript canónico

O *transcript* é o conjunto exato de bytes que é assinado. A regra é
uniforme em todas as mensagens:

```text
TRANSCRIPT = domínio ‖ versão ‖ campos do corpo
assinatura = Ed25519_sign(seed_privada, TRANSCRIPT)
```

* **Codificação:** concatenação de campos de tamanho fixo, pela ordem
  exata abaixo, **sem** comprimento, marcador ou delimitador. Campos de
  tamanho fixo → o transcript é canónico: qualquer implementação monta
  os mesmos bytes.
* **Domínio (domain separator):** literal ASCII **sem terminador NUL**.
  Impede ataques de reinterpretação entre tipos de mensagem (um
  `ACCEPT` nunca pode ser lido como um `REQUEST`).
* **Versão:** byte `0x01`, **primeiro byte do corpo**. Faz parte do
  transcript → é autenticada.
* **Anti-downgrade e tolerância não se contradizem.** Trocar o byte
  invalida a assinatura, logo um atacante **não** consegue forçar uma
  versão. E aceitar uma versão que se suporta nativamente não é
  downgrade: é compatibilidade. Ver
  [`index.md`](index.md) §Princípio da tolerância de versões.

  > Uma versão anterior deste documento descrevia o byte como
  > `major`/`minor` em nibbles. A implementação nunca o fez. Corrigido
  > para opaco, por causa da mesma razão que no envelope
  > ([`message_format.md`](message_format.md) §Versionamento).

### Tolerância de versões

O byte é **opaco**, e o que decide é se a versão está na lista de versões
que esta implementação lê:

```rust
// network/daemon_rust/src/handshake.rs
pub const VERSAO_PROTOCOLO: u8 = 0x01;             // o que escrevemos
pub const VERSOES_ACEITE: &[u8] = &[VERSAO_PROTOCOLO]; // o que lemos
```

O receptor **rejeita** qualquer versão fora da lista
(`ErroHandshake::VersaoDesconhecida`, IPC `0x0C`) —
fail-closed.

---

## Chaves envolvidas por par de utilizadores

| Chave    | Quem gera | Quem a conhece (após o handshake) | Uso                                            |
| -------- | --------- | --------------------------------- | ---------------------------------------------- |
| `K1`     | A (iniciador) | A e B                          | primeira camada — confidencialidade mútua      |
| `K5_A`   | A         | A e B                             | camada do remetente em mensagens **A → B**     |
| `K9_A`   | A         | A e B                             | última camada de mensagens **B → A**           |
| `K5_B`   | B         | A e B                             | camada do remetente em mensagens **B → A**     |
| `K9_B`   | B         | A e B                             | última camada de mensagens **A → B**           |
| `pub_A`  | derivada da seed de A | A e B                    | identidade e verificação de assinaturas        |
| `pub_B`  | derivada da seed de B | A e B                    | idem                                            |

Clarificações normativas:

* **Geração:** `K1`, `K5` e `K9` são geradas pelo **CSPRNG do sistema**
  (32 bytes aleatórios, nunca derivadas — não existe KDF no protocolo).
  A `seed` Ed25519 é gerada uma vez e persistida.
* **Âmbito:** a coluna "Âmbito" antiga sugeria que `K5_A`/`K9_A` eram
  secretas só de A — **não é**: B precisa delas para decifrar/cifrar e
  por isso as recebe no `FRIEND_REQUEST`. O que distingue as chaves é
  o **papel** (par/remetente/receptor), não o segredo em relação ao par.
* Ciclo de vida completo (persistência, destruição, rotação, proteção
  em memória) → [`key_management.md`](key_management.md).

---

## Mensagens do protocolo

Enquadramento na ligação P2P: `[comprimento: u32 LE][tipo: 1B][corpo]`.
Corpos têm **comprimento exato fixo** — qualquer desvio é rejeitado
antes de verificar assinaturas.

### 1. `FRIEND_REQUEST` (A → B) — tipo `0x10`

```text
corpo (209 bytes) = versao(1) ‖ pub_A(32) ‖ K1(32) ‖ K5_A(32) ‖ K9_A(32) ‖ nonce(16) ‖ sig(64)

TRANSCRIPT = "ONYX/FRIEND/REQ" ‖ 0x01 ‖ pub_A ‖ K1 ‖ K5_A ‖ K9_A ‖ nonce
```

* `nonce(16)` aleatório torna cada pedido único (anti-replay e
  impossibilidade de reutilizar assinaturas).
* A escolha de `K1` é do iniciador — ambos passam a usá-la.

### 2. `FRIEND_ACCEPT` (B → A) — tipo `0x11`

```text
corpo (177 bytes) = versao(1) ‖ pub_B(32) ‖ K5_B(32) ‖ K9_B(32) ‖ nonce(16) ‖ sig(64)

TRANSCRIPT = "ONYX/FRIEND/ACC" ‖ 0x01 ‖ pub_B ‖ K5_B ‖ K9_B ‖ nonce
```

* `nonce(16)` é o **eco** do nonce do pedido — liga o aceite ao pedido.

### 3. `FRIEND_REJECT` (B → A) — tipo `0x12`

```text
corpo (81 bytes) = versao(1) ‖ nonce(16) ‖ sig(64)

TRANSCRIPT = "ONYX/FRIEND/REJ" ‖ 0x01 ‖ nonce
```

* `nonce(16)` é o nonce do pedido rejeitado — **liga a recusa ao
  pedido**, impedindo que uma recusa legítima seja reaproveitada para
  outro pedido.
* A assinatura é de **B** (o rejeitante) e é validada contra `pub_B`.

---

## Validação (ordem normativa)

```text
corpo recebido
   │
   ▼
exigir comprimento exacto          ── ≠ esperado → ComprimentoInvalido
   │
   ▼
exigir versao == 0x01              ── ≠        → VersaoDesconhecida
   │
   ▼
verificar assinatura Ed25519       ── inválida  → AssinaturaInvalida
   │
   ▼
anti-replay (nonce já visto)       ── repetido  → NonceRepetido
   │
   ▼
aceitar / persistir chaves
```

**A assinatura é sempre verificada antes de qualquer chave ser
aceite.** A versão é verificada antes da assinatura (é barata e
rejeita cedo).

---

## Fluxo

```text
 A                                              B
 │ ─── FRIEND_REQUEST (v, pub_A, K1, K5_A, K9_A) ──▶ │
 │                                                  │ valida versão + assinatura de A
 │                                                  │ anti-replay do nonce
 │                                                  │ deriva pub_A, persiste chaves
 │ ◀── FRIEND_ACCEPT (v, pub_B, K5_B, K9_B) ──────── │
 │ valida versão + assinatura de B                   │
 │ deriva pub_B, persiste chaves                     │
 │=============== chat seguro ativo ================│
```

**Regras obrigatórias:**

1. Rejeitar qualquer handshake com assinatura inválida, sem log de
   conteúdo (apenas o código do erro).
2. Rejeitar qualquer handshake com versão diferente de `0x01`
   (anti-downgrade).
3. Rejeitar pedidos repetidos com o mesmo `nonce` (anti-replay).
4. Uma só amizade ativa por `pub` — novo `FRIEND_REQUEST` de uma chave
   já conhecida **substitui** as chaves antigas (rotação).
5. Só depois do `FRIEND_ACCEPT` validado é que o chat é funcional.
6. Identidades (`pub`) são fixas; a assinatura em cada handshake prova
   posse da seed correspondente — é isto que **estabiliza a
   identidade** e **vincula as chaves de sessão à identidade**:

```text
Ed25519 identity
       │  autentica
       ▼
   handshake  (transcript com identidades + chaves + versão)
       │
       ▼
 session keys   →  K1/K5/K9 ficam criptograficamente associadas
                   à identidade que participou no handshake
```

Uma chave de sessão **não pode** ser apresentada como pertencente a
outra identidade: apresentá-la exigiria re-assinar o transcript com a
seed dessa outra identidade, o que é impossível sem a chave privada.

---

## Anti-replay do handshake

* **Mecanismo:** `nonce` aleatório de 16 bytes por pedido, ecoado no
  aceite/recusa.
* **Estado:** memória circular de `CAPACIDADE_NONCES = 256` nonces
  vistos, por processo (`RegistoAmigos`).
* **Comportamento após reinício:** o estado é em memória — um reinício
  limpa o registo. Não persistimos nonces por filosofia (sem histórico
  em disco).
* **Âmbito:** protege os *pedidos de handshake*. O anti-replay das
  *mensagens de chat* é independente e vive no registo de `nonce1` do
  envelope (ver `message_format.md` §Anti-replay).

---

## Onde vivem as chaves depois do handshake

O daemon **não** as guarda. `RegistoAmigos` é um `HashSet` das
identidades (`publica`) dos amigos — 32 bytes por par, e não os 192 de
uma `Amizade` completa (`publica` + K1 + K5/K9 de cada lado).

Não é uma optimização de estética: o registo nunca precisou das chaves.
Só contava e confirmava presenças. As chaves saem do daemon na resposta
IPC do `ACEITAR_AMIZADE`/`CONFIRMAR_AMIZADE` (192 bytes) e é o **cliente**
que as persiste, no keystore da conta (`messenger/conta.py`), cifradas
com a frase de segurança.

A consequência prática é que o estado volátil do daemon fica sem material
simétrico: um `Debug` do `Estado`, um `Clone` acidental ou um dump de
memória já não expõem K1/K5/K9 de nenhuma amizade.

## Onde as chaves são guardadas, e desde quando

Até 2026-10-07 as 192 bytes eram **impressas para o terminal e perdidas no
fim do comando**. A consequência estava escrita aqui mesmo: «recomeçar o
daemon obriga a repetir o handshake», e uma amizade que não sobrevive a
um reinício do daemon não é uma amizade, é um rito.

Agora vivem no **keystore da conta** (`messenger/conta.py`, classe
`Amizade`), cifradas com a frase de segurança, no mesmo dicionário e
com a mesma cifra que a identidade.

Três razões, e a escolha de não ir para a loja do sidecar é a primeira:

* **A cifra é a da frase de segurança.** Guardar chaves de mensagem num
  `sqlite3` ao lado punha material que só o daemon devia ter, num
  ficheiro que ninguém abre com uma frase;
* **A cópia de segurança passa a cobri-las.** `exportar` copia o
  keystore inteiro, e portanto uma conta restaurada traz as amizades —
  que é o que «repor uma cópia» promete em [`conta.md`](conta.md);
* **Um ficheiro, uma frase.** Um keystore separado obrigaria a
  exportá-lo e restaurá-lo a par, e o ecrã de recuperação passaria a ter
  duas operações em vez de uma.

`onyxchat aceitar-amizade` e `onyxchat confirmar-amizade` continuam a
imprimir as chaves — o par precisa do `corpo` — e passam a guardá-las.
`onyxchat amigos` lista o que sobreviveu, e pede a frase porque abrir o
keystore **é** decifrar.

### Quando a guardar falha

A falha é dita e o código de saída é 1. O handshake **já aconteceu**:
o par tem o pedido e o daemon já registou a sessão. Perder a amizade é
melhor do que mentir à pessoa — e o comando não diz «guardada» quando
não guardou.

**Uma** das duas dá 0 com aviso em vez de erro: não haver conta local.
O `daemon_binario` de testes e quem corre sobre a identidade do
`config.json` não têm onde guardar, e ficar sem guardar é o
comportamento correcto aí — o comando handshake cumpriu o que tinha a
prometer, e quem não tem conta não tem onde pôr as chaves.

Cancelar a frase de segurança dá **1**, como qualquer outra falha. A
distinção é deliberada: sem conta, o pedido foi cumprido e não havia
onde persistir; a cancelar, **havia** onde persistir e a pessoa
escolheu não deixar. Sair com 0 depois de uma gravação pedida e não
feita ensinaria a quem automatiza isto que pode deixar de verificar —
e é a razão de `user/cli.py` insistir: cancelar a frase é uma falha,
não um detalhe de interface.

> Uma versão anterior deste parágrafo dizia que as duas situações davam
> 0. `tests/test_cli.py` diz o contrário, e o código também: a pessoa
> cancelou, a amizade está feita do lado do daemon, e dizer «guardada»
> seria mentira.

---

## Relação com o pipeline de chat

Depois do handshake, cada mensagem de chat usa o envelope de
`message_format.md` e é cifrada com `K1 → K9`. A assinatura de chat é
**sempre verificada antes de decifrar** — tal como as assinaturas do
handshake são sempre verificadas antes de qualquer chave ser aceite.

As mensagens de handshake **não** passam pelo pipeline K1→K9 — são
mensagens de controlo assinadas, para que as chaves possam ser trocadas
antes de existirem chaves partilhadas.

---

## Observações finais

* O discovery server nunca participa no handshake: apenas entrega
  `ID → .onion` (`discovery.md`).
* Nonces do handshake são aleatórios (16 bytes) e nunca reutilizados.
* Referência histórica: a génese destas melhorias está no ficheiro
  `Resumo.md`, no directório *acima* do projecto (documento de trabalho
  — a especificação normativa deste protocolo é este documento).