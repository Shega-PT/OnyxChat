# Modelo de Privacidade

## Enquadramento

A privacidade **não é um único objetivo**. O OnyxChat distingue quatro
objetivos separados, com estados diferentes e limites diferentes. Este
documento diz, para cada um, **o que é protegido, o que não é protegido
e quem pode observar o quê**.

---

## 1. Privacidade do conteúdo (Content Privacy)

**Objetivo:** o conteúdo da mensagem ser inacessível aos componentes
que apenas transportam a comunicação.

```text
Onyx A
  │
  ▼
Relay 1      ← não tem chaves
  │
  ▼
Relay 2      ← não tem chaves
  │
  ▼
Onyx B
```

| | |
| --- | --- |
| **Protegido** | texto das mensagens; chaves de sessão; handshake |
| **Não protegido** | nada — este é o objetivo mais forte do sistema |
| **Quem observa** | relays/discovery veem apenas `ciphertext` + nonces |

**Estado:** IMPLEMENTADO. Pipeline K1→K9 + assinatura Ed25519,
verificação antes da decifragem.

---

## 2. Privacidade da identidade (Identity Privacy)

**Objetivo:** a infraestrutura não necessitar do nome real ou da
identidade civil do utilizador.

```text
identidade Onyx   ≠   identidade civil
```

| | |
| --- | --- |
| **Protegido** | o protocolo só conhece chaves Ed25519; nada de dados pessoais |
| **Não protegido** | a identidade Onyx é estável e reutilizável — quem observa múltiplas sessões da mesma identidade correlaciona-as |
| **Quem observa** | discovery vê `ID → .onion`; relay vê mailboxes derivadas da `pub` |

**Estado:** IMPLEMENTADO (nenhum campo do protocolo transporta
identidade civil). A unicidade/estabilidade da identidade é uma
propriedade *desejada* (estabilização por Ed25519), não um vazamento.

---

## 3. Privacidade do endereço de rede (IP Privacy)

**Objetivo:** impedir que a comunicação P2P exija que os participantes
revelem diretamente os seus endereços IP reais.

```text
Onyx A ──Tor──▶ .onion ──▶ Onyx B
```

**Formulação rigorosa (obrigatória na documentação):**

> O transporte P2P do OnyxChat é concebido para evitar a exposição
> direta do endereço IP real entre os endpoints, utilizando Tor e
> hidden services.

**Não se afirma** "nenhum IP real exposto" — seria uma garantia
absoluta contra qualquer forma de observação da rede, que o sistema
não pode fazer. Exceções declaradas:

| Modo                | IP observável por quem                        |
| ------------------- | --------------------------------------------- |
| direto (.onion)     | IPs dos nós de saída/entrada do Tor (não os endpoints entre si) |
| relay em clearnet   | o relay vê o IP de ligação de cada cliente    |
| discovery           | vê o IP de quem consulta/regista (servidor HTTP) |

**Estado:** IMPLEMENTADO para o modo direto; **dependente do deployment**
para relay/discovery.

Uma linha que estava aqui foi removida: **«relay em `.onion` → sem IP
directo para o relay»**. Não é implementável hoje — o modo relay abre um
TCP directo, e um `.onion` não é resolvido por esse caminho. Ver
[`relay.md`](relay.md) §«O relay em `.onion` não funciona».

O que o daemon garante no modo relay é que o custo é **declarado**: valida o
endpoint antes de qualquer I/O e escreve um aviso no `stderr` antes de ligar.
O aviso não evita a exposição — nada o faz — mas distribui-a em vez de a
esconder.

---

## 4. Minimização de metadados (Metadata Minimization)

**Objetivo:** não criar infraestrutura central que mantenha histórico,
contactos ou registo de quem falou com quem.

O sistema **não** mantém centralmente:

* histórico de mensagens;
* histórico de comunicação;
* lista central permanente de contactos;
* registo central de quem falou com quem;
* armazenamento central de conteúdo;
* sistema central obrigatório de routing.

**O discovery server é bootstrap, não centro de comunicação**
(`discovery.md`): guarda `ID → .onion` com TTL e nada mais.

### O que os metadados ainda revelam (reconhecido explicitamente)

> **Cifrar o conteúdo não elimina automaticamente todos os metadados.**

| Metadado            | Observável por                          |
| ------------------- | --------------------------------------- |
| tamanho             | relay, operador de rede                 |
| frequência/duração  | relay, operador de rede                 |
| timing              | relay, operador de rede                 |
| origem do tráfego   | relay (clearnet), operador de Tor       |
| destino observado   | relay (mailbox), discovery (consultas)  |
| número de mensagens | relay (contadores internos)             |
| padrões de comunicação | quem correlaciona as observações acima |

**O objetivo atual do OnyxChat não é esconder todos estes padrões.**
É declarar com precisão que eles existem e que não são eliminados pela
cifra de conteúdo.

**Estado:** parcial — minimização por ausência de infraestrutura
central (implementada); mitigação activa de tráfego (não implementada,
não planeada neste momento).

### `PLANEADO` (F2.1) — a frequência de consultas ao discovery

Automatizar a resolução `ID → .onion` dentro de `ligar` muda **uma
coisa** do que o discovery observa: deixa de ver apenas consultas
explícitas do operador e passa a ver **uma consulta por tentativa de
ligação**, sem intervenção humana.

| | Antes | Depois de F2.1 |
| --- | --- | --- |
| **Quem** consulta | o operador, quando quer | o software, a cada tentativa |
| **Que** ID | o mesmo | o mesmo |
| **Quando** | uma vez, a pedido | correlacionado com instantes de actividade |
| **Frequência** | baixa, deliberada | proporcional ao número de tentativas |

Isto **não** é uma classe de metadado nova — é a mesma
(`discovery.md` §Metadados observáveis, linha «destino observado»), com
**granularidade maior**. Um observador do discovery passa a poder
correlacionar o momento de uma consulta com um pico de actividade de
rede.

Mitigação, se for relevante para a ameaça: correr instâncias próprias
do discovery (`discovery.md` §Componente substituível), ou resolver o
`.onion` por fora e passar o endereço já resolvido a `ligar`.

---

## Discovery ≠ central de comunicação

O servidor de discovery é descrito como infraestrutura de
**bootstrap/discovery**, nunca como servidor central de mensagens. Não
é responsável por: armazenar mensagens, manter conversações, controlar
sessão criptográfica, decifrar conteúdo ou funcionar como roteador
obrigatório (`discovery.md`).

---

## Resumo por objetivo

| Objetivo | Estado | Limite principal |
| --- | --- | --- |
| Conteúdo | IMPLEMENTADO | depende da segurança do pipeline |
| Identidade civil | IMPLEMENTADO | identidade Onyx é estável e correlacionável |
| Endereço IP | IMPLEMENTADO (modo direto) | relay/discovery em clearnet veem IPs |
| Metadados | MINIMIZADO | tamanho/timing/frequência permanecem observáveis |
| Frequência de consultas ao discovery | MINIMIZADO (a agravar em `PLANEADO` F2.1) | um consulta por tentativa de ligação |
