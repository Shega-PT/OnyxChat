# Modelo de Ameaças (Threat Model)

## Âmbito

Este documento define **quem** é considerado adversário no OnyxChat, o
que cada adversário pode observar, modificar e controlar, e o que **não
deve** conseguir obter. É a base para `security_model.md` (propriedades
pretendidas) e `privacy_model.md` (propriedades de privacidade).

| | |
| --- | --- |
| **A1–A7** | adversários de rede e de protocolo |
| **A8** | atacante local não privilegiado (outro utilizador do host) |
| **A9** | atacante que compromete uma camada do pipeline |
| **A10** | atacante com acesso privilegiado ao sistema — **fora do âmbito**, declarado como limitação |

A secção *«O que o modelo NÃO cobre»* é tão normativa como a lista de
adversários: um modelo que não declara os seus limites não é um modelo
fável.

---

## Adversários considerados

### A1 — Atacante externo (rede)

| | |
| --- | --- |
| **Pode observar** | tráfego cifrado na rede; tamanhos; timing; existência de ligações |
| **Pode modificar** | bytes em trânsito (se controlar um salto) |
| **Pode controlar** | nenhum endpoint |
| **Não deve obter** | plaintext; chaves; identidade civil; IP real dos endpoints (via Tor) |

### A2 — Relay malicioso

| | |
| --- | --- |
| **Pode observar** | envelopes cifrados; mailboxes (hashes de `pub`); IPs de ligação (modo clearnet); timing e tamanhos |
| **Pode modificar** | envelopes que encaminha; pode reenviar, atrasar ou descartar |
| **Pode controlar** | entrega de mensagens (disponibilidade) |
| **Não deve obter** | plaintext; chaves K1/K5/K9; conteúdo de conversas; participação no handshake |

*Nota:* a retransmissão de um envelope capturado é travada pelo
registo anti-replay de `nonce1` (`message_format.md` §Anti-replay).

### A3 — Discovery malicioso

| | |
| --- | --- |
| **Pode observar** | quem consulta que ID; quem se regista e quando |
| **Pode modificar** | mapeamentos `ID → .onion` (pode devolver endpoint errado) |
| **Pode controlar** | falha de descoberta (disponibilidade) |
| **Não deve obter** | mensagens; chaves; histórico de comunicação |

*Nota:* entregar um `.onion` errado não permite ler nem forjar nada:
o handshake é autenticado por Ed25519 e o chat é cifrado fim-a-fim.
O dano possível é de disponibilidade.

### A4 — Routing node malicioso (futuro)

Estado: **conceito** (`roadmap.md`). Regras previstas: um nó de
transporte não deve receber confiança criptográfica e não deve
armazenar permanentemente o que transporta.

### A5 — Atacante com acesso ao tráfego (observação passiva)

| | |
| --- | --- |
| **Pode observar** | tudo o que trafega: envelopes, nonces, assinaturas, tamanho, frequência |
| **Não deve obter** | plaintext; chaves; capacidade de gerar assinaturas válidas |

### A6 — Atacante com mensagem capturada (replay)

| | |
| --- | --- |
| **Pode fazer** | reenviar um envelope ou um pedido de handshake já visto |
| **Não deve conseguir** | que a mensagem reenviada seja aceite novamente |

**Defesas:** `nonce1` anti-replay (chat), memória circular de nonces
(handshake), tags AEAD (integridade).

### A7 — Atacante de impersonation

| | |
| --- | --- |
| **Tenta** | apresentar-se como outro utilizador; trocar chaves de sessão por chaves suas |
| **Não deve conseguir** | porque o transcript do handshake é assinado com a seed do identificador apresentado — sem a chave privada, é impossível produzir o transcript válido |

### A8 — Atacante local (mesma máquina, outro utilizador)

| | |
| --- | --- |
| **Pode tentar** | ligar ao UDS do daemon; ler keystore e `config.json` em disco; ler a swap do sistema |
| **Defesas** | socket `0600` + verificação de uid (`SO_PEERCRED`); **keystore cifrado** com PBKDF2 + ChaCha20-Poly1305, depois de `onyxchat cifrar` (antes disso a seed está em claro no `config.json`); ficheiros `0600`; `/swapfile` com modo `0600 root:root`; `kernel.yama.ptrace_scope = 1` restringe `/proc/<pid>/mem` |

> **Lacuna conhecida: `--seed` e `--k1` na linha de comandos.** Os
> subcomandos de amizade aceitam a seed e as chaves por `--seed` /
> `--k1` / `--k5-par` / `--k9-par`. Em Linux, `/proc/<pid>/cmdline` é
> **modo `0444`** — legível por qualquer utilizador da máquina — e a
> linha fica no histórico do shell. `ptrace_scope = 1` **não** protege
> o `cmdline` (restringe `ptrace` e `/proc/<pid>/mem`). Nenhuma das
> cinco defesas desta linha cobre esta via.
>
> Mitigação **implementada**: `--seed` passou a ser opcional nos quatro
> subcomandos de amizade. Omiti-la faz a CLI ler a seed do
> `config.json` — que é `0600` e não é legível por outro utilizador.
> A flag continua a existir para quando o daemon corre com outra
> identidade, mas a exposição por omissão desapareceu.
>
> **O que a mitigação não cobre:** as chaves de sessão (`--k1`,
> `--k5-par`, `--k9-par`) continuam obrigatórias em `argv`, e as
> *imagens* de `k1`/`k5`/`k9` que o comando imprime saem para o stdout.
> São chaves de uma única sessão, não a identidade — mas são secretas.
> Uma evolução natural seria escrevê-las num ficheiro `0600` em vez do
> stdout.

> **Alargamento (F0c).** A8 inclui agora a tentativa de ler **swap** —
  a área onde o kernel pode colocar páginas de memória sensível. O
> `/swapfile` do sistema é `0600 root:root`, pelo que um utilizador
  **não-root** não o consegue ler directamente. Isto é o que separa A8
> de A10.

### A9 — Atacante que compromete uma camada do pipeline

| | |
| --- | --- |
| **Obtém** | uma transformação intermédia (ex.: saída de K4) |
| **Ainda enfrenta** | as restantes camadas + autenticação AEAD + assinatura Ed25519 |

Isto é o objetivo da **defesa em profundidade** (`pipeline.md`
§Introdução) — não é "nove vezes mais segurança".

### A10 — Atacante com acesso privilegiado ao sistema *(novo, F0c)*

| | |
| --- | --- |
| **Pode tentar** | ler `/swapfile` e `/proc/*/mem` como root; fazer *cold-boot attack* à RAM física; analisar o SSD em busca de restos de páginas descartadas; capturar dumps de núcleo |
| **Defesas** | **`mlockall(MCL_CURRENT\|MCL_FUTURE)`** no arranque do daemon impede o page-out de material sensível (`PLANEADO` — F0c); `zeroize` no `Drop` no Rust reduz a janela em RAM; desde a fase F o C limpa a saída das suas camadas nos caminhos de erro com `sodium_memzero` (`onyx_limpar`) |
| **Não é mitigado** | acesso físico ao hardware, forense de disco, *cold-boot*, dumps de núcleo |

Isto está deliberadamente **fora** do âmbito de A8 e é declarado como
limitação, não escondido. Detalhe da exposição a swap e do que o
`mlockall` resolve — e o que não resolve — em
[`key_management.md`](key_management.md) §Exposição a swap e page-out.

**Princípio:** o que se assume é que `root` ou acesso físico ao host
estão fora do modelo. Um sistema cujas chaves vivem em RAM não sobrevive
a quem controla a máquina. `mlockall` é defesa em profundidade, não uma
garantia.

---

## O que o modelo NÃO cobre (limitações declaradas)

1. **Metadados.** Cifrar o conteúdo não elimina tamanho, frequência,
   duração, timing nem padrões de comunicação (`privacy_model.md`).
2. **Endpoint comprometido.** Se a seed ou o keystore do utilizador
   forem obtidos, o sistema não tem defesa — as chaves são o sistema.
3. **Análise global do Tor.** O OnyxChat depende das garantias do Tor;
   um adversário global de observação está fora do alcance deste modelo.
4. **Disponibilidade.** Nenhum mecanismo do OnyxChat garante entrega:
   relay, discovery e Tor podem recusar serviço.
5. **Traffic analysis em modo relay clearnet.** O relay vê IPs de
   ligação quando corre em clearnet (`relay.md` §Metadados).
6. **Acesso privilegiado ao host (A10).** `root`, forense de disco,
   *cold-boot attack* e dumps de núcleo estão **fora** do modelo. O
   `mlockall` reduz o page-out, mas chaves em RAM não sobrevivem a quem
   controla a máquina. Ver A10 e
   [`key_management.md`](key_management.md) §Exposição a swap.
7. **Zeroização sob pressão de memória.** A garantia de que a memória
   sensível é limpa no `Drop` pressupõe que a página não foi descartada
   para swap. `mlockall` reduz o risco; não o elimina.
8. **Anti-replay persistente, mas não contra um atacante local.** O
   registo de `nonce1` sobrevive a reinícios do daemon (fase B3,
   `docs/message_format.md` §Persistência), o que fecha a janela em que
   um envelope capturado passava depois de um reinício. **Não fecha** a
   variante em que o atacante escreve no directório de estado antes do
   arranque: precisa de acesso de escrita a `$XDG_STATE_HOME`, o que já
   não é ataque de rede nem de disco — é o utilizador.
9. **Sem forward secrecy.** A derivação da K1 por mensagem
   (`crypto/rust/src/amizade.rs`, `messenger/amizade.py`) é
   determinística em função de duas chaves de **longa duração** e de um
   nonce público:

   ```text
   semente = SHA-256(b"ONYX/AMIZADE/v1" ‖ menor(k_a,k_b) ‖ maior(k_a,k_b))
   k1      = HKDF-SHA256(semente, salt=nonce1, info=b"ONYX/K1/v1" ‖ pub_emissor)
   ```

   Quem compromisesse `k5_proprio` ou `k5_par` **depois** da conversa
   abre todas as mensagens, passadas e futuras. Não há segredo
   efémero em lado nenhum: a semente é reconstruível a partir do
   material de longa duração.

   **O que a derivação dá, e é real:** a chave de longa duração não
   atravessa a IPC nem entra na memória do daemon. O daemon recebe uma
   chave por mensagem; um dump do processo do daemon — que o item 6
   declara fora do modelo para o *endpoint*, mas que um atacante local
   sem privilégios pode tentar por `/proc/<pid>/mem` — dá chaves de
   mensagens isoladas e não o segredo da amizade. E a chave é
   **direccional** (a identidade do remetente entra no `info`), pelo
   que um envelope reflectido não abre na outra direcção.

   **O que não dá:** forward secrecy. Só a haveria com um segredo
   efémero por sessão que nunca seja transmitido — um handshake
   diferente (Noise IK com efémeros), não uma optimização.

---

## Superfície de ataque por fronteira

| Fronteira              | Risco principal                    | Controlo                                  |
| ---------------------- | ---------------------------------- | ----------------------------------------- |
| rede → `p2p.rs`        | frames malformados, flood          | comprimento fixo, rate-limit, timeouts    |
| rede → `envelope.rs`   | parsing exploratório               | análise por comprimento, limites          |
| cliente → `ipc.rs`     | pedidos inválidos, outro uid       | validação total + `SO_PEERCRED` + `0600`  |
| disco → keystore       | leitura por outro utilizador       | `0600` + cifra com PBKDF2                 |
| RAM → swap             | page-out de material sensível       | `mlockall` (`PLANEADO` F0c) + swapfile `0600` |
| relay/discovery → nós  | envenenamento de mapeamento        | autenticação fim-a-fim (Ed25519)          |
| cadeia de fornecimentos | dependência comprometida         | auditoria contínua, excepções datadas    |

Detalhe da fronteira de rede (enquadramento, limites, rate-limit,
timeouts e comportamento perante dados inválidos) em
[`p2p.md`](p2p.md).

### A sétima fronteira: a cadeia de fornecimentos

As outras seis são superfícies de **execução**. Esta é de **construção**,
e num projecto com 548 crates em `Cargo.lock`, 32 pacotes npm e uma
libsodium vendorizada, é a maior das sete: ninguém revê o código de que
depende, e uma alteração happen no lugar errado muda o binário de todas
as formas que continuam a passar os testes.

O risco não é abstrato. Um crate com advisory de exaustão de memória
ataca o `build`. Uma licença copyleft num crate novo, depois de
publicado, torna a PolyForm Noncommercial impossível de cumprir. Um
`package-lock.json` com um `to=` de redireccionamento controlado por
input transforma a navegação da interface num redireccionamento aberto.

Os controlos, e o que cada um **não** cobre:

| Ameaça | Controlo | O que não cobre |
| --- | --- | --- |
| Advisory de segurança | `cargo audit`, `npm audit` | Não avalia a correcção, só a existência do aviso |
| Licença copyleft | `cargo deny` com [`deny.toml`](../deny.toml) | Avalia o campo e a expressão SPDX declarados, não o texto da licença |
| Segredo no histórico | `gitleaks` | Vê padrões conhecidos; um segredo com outro formato passa |
| Acção maliciosa no CI | `uses:` por SHA de commit | Fixa **quem** executa, não **o que** o código faz depois de correr |
| Dependência abandonada | Dependabot semanal | Actualiza; não decide se a actualização é segura |

A licença tem uma excepção declarada e é a que mais dói: `cargo deny`
confirma que hoje **nenhum** crate é GPL, AGPL, SSPL, OSL, EUPL, CDDL ou
CC-BY-SA, mas confirma-o lendo o que o crate **declara**. Um crate que
declare `MIT` e distribua GPL é exactamente o furo que nenhum scanner
fecha. Por isso [`THIRD-PARTY.md`](../THIRD-PARTY.md) existe e declara
esse limite por escrito.

E o mesmo vale para o resto: um verificador que passa é uma medição, não
uma promessa. O que está medido está em
[`security_model.md`](security_model.md) §4.

---

## Requisitos resultantes

1. **Verificar antes de decifrar** — nunca processar material não
   autenticado (`message_format.md`).
2. **Todo transporte é não-confiável** — nenhuma confiança criptográfica
   é delegada a relay, discovery ou Tor.
3. **Fail-closed** — versão desconhecida, assinatura inválida ou
   limite excedido ⇒ rejeição, nunca "tentar".
4. **Sem vazamentos de erro** — erros com códigos fixos, sem chaves,
   nonces, plaintext ou detalhes de infraestrutura.
5. **Limites antes de alocar** — todo comprimento externo é validado
   antes de qualquer alocação proporcional.
6. **Falhar alto, nunca degradar em silêncio** — um binário sem backend
   Tor recusa arrancar em vez de fingir ter protecção que não tem
   (`main.rs`, `construir_backend`).
7. **A zeroização pressupõe páginas em RAM** — sem `mlockall`, uma
   chave pode sobreviver num dispositivo persistente depois de
   zeroizada em RAM (`key_management.md` §Exposição a swap).
