# Índice da documentação

> **Este documento é o ponto de entrada da documentação do OnyxChat.**
> Diz o que existe, a quem pertence cada documento, e por que ordem
> ler, conforme o que se quer fazer.

Os documentos do OnyxChat não são um conjunto homogéneo. Misturam
três coisas de natureza diferente:

1. **Guias** — dirigidas a um público, com um propósito de leitura.
2. **Modelos** — o que o sistema pretende proteger e contra quem.
3. **Especificações normativas** — bytes, offsets, códigos, limites.

Um leitor novo que abra um ficheiro normativo ao acaso não entende
onde está. Este índice existe para evitar isso.

---

## Escada de conhecimento

A documentação está organizada em **três níveis de profundidade**, mais
as especificações. Cada nível é autónomo e remete para o seguinte.

```text
   ┌─────────────────────────────────────────────┐
   │  README.md                    nível BÁSICO  │  o que é, em 1 minuto
   └──────────────────────┬──────────────────────┘
                          ▼
   ┌─────────────────────────────────────────────┐
   │  docs/architecture.md        nível MÉDIO    │  como é construído
   └──────────────────────┬──────────────────────┘
                          ▼
   ┌─────────────────────────────────────────────┐
   │  docs/SYS_GUIDE.md        nível APROFUNDADO │  porque é assim
   └─────────────────────────────────────────────┘

   Especificações normativas  →  docs/*.md (§ abaixo)
   Modelos                    →  docs/*.md (§ abaixo)
```

| Documento | Nível | Responde a | Leitor típico |
| --- | --- | --- | --- |
| [`../README.md`](../README.md) | básico | «o que é isto?» | qualquer pessoa |
| [`architecture.md`](architecture.md) | médio | «como está organizado?» | quem vai mexer no código |
| [`SYS_GUIDE.md`](SYS_GUIDE.md) | aprofundado | «porquê que é assim?» | quem avalia ou estuda |
| [`USER_GUIDE.md`](USER_GUIDE.md) | prático | «como uso?» | utilizador final |
| [`DEV_GUIDE.md`](DEV_GUIDE.md) | prático | «como desenvolvo?» | quem contribui |

A regra que mantém a escada honesta: **cada nível só repete o anterior
para dar contexto, e remete para a spec em vez de a reescrever.** Um
nível que duplica outro é um nível que diverge.

---

## Ordem de leitura por perfil

### Utilizador novo

```text
1. USER_GUIDE.md          (§1 pré-requisitos → §3 primeiros passos)
2. SYS_GUIDE.md           (§3 objectivos de privacidade)
3. USER_GUIDE.md          (§8 diagnóstico)
```

### Quem avalia o projecto

```text
1. README.md
2. SYS_GUIDE.md           (completo — é o documento de avaliação)
3. threat_model.md        (o que promete e o que NÃO promete)
4. privacy_model.md       (limites explícitos do anonimato)
5. roadmap.md             (o que é real hoje)
```

> Aviso: [`threat_model.md`](threat_model.md) e
> [`privacy_model.md`](privacy_model.md) contêm secções intituladas
> *«o que o modelo NÃO cobre»*. **Lê-las antes de avaliar** — um
> projecto que não declara os seus limites não é um projecto fiável.

### Quem vai implementar ou reimplementar

```text
1. SYS_GUIDE.md
2. architecture.md
3. threat_model.md        (fronteiras de confiança)
4. As specs da tabela «Normativo» abaixo, pela ordem da tabela
```

### Quem vai alterar o código

```text
1. DEV_GUIDE.md           (§2 comandos, §5 convenções, §6 mapa)
2. testing.md             (que teste sustenta que propriedade)
3. As specs afectadas pela mudança
```

### Quem só quer o estado actual

```text
1. roadmap.md             (IMPLEMENTADO / PLANEADO / CONCEITO)
2. testing.md             (§Cobertura — o que está medido)
```

---

## Especificações normativas

Documentos que definem **bytes**. Uma implementação independente
consome apenas esta secção e nada mais.

| Documento | Especifica | Consumidores |
| --- | --- | --- |
| [`pipeline.md`](pipeline.md) | as 9 camadas K1→K9: entrada, chave, operação, saída, inversa, limites | Rust, Python, C/C++, Lua |
| [`message_format.md`](message_format.md) | envelope binário: layout, região assinada, versionamento, anti-replay | Rust, Python |
| [`handshake.md`](handshake.md) | transcript canónico, `FRIEND_REQUEST`/`ACCEPT`/`REJECT`, anti-replay, anti-downgrade | Rust, Python |
| [`ipc_spec.md`](ipc_spec.md) | protocolo local Python↔daemon: framing, comandos, erros, timeouts | Rust, Python |
| [`p2p.md`](p2p.md) | frames ponto a ponto: tipos, validação, rate-limit, timeouts | Rust |
| [`relay.md`](relay.md) | transporte opcional: mailbox, mensagens, retenção | Rust, Python |
| [`discovery.md`](discovery.md) | bootstrap `ID → .onion`: rotas, validação, TTL | Python |
| [`key_management.md`](key_management.md) | ciclo de vida das chaves, keystore, zeroização, exposição a swap | Rust, Python |
| [`test_vectors.md`](test_vectors.md) | **gerado** — os vectors oficiais | Rust, Python, C/C++ |
| [`testing.md`](testing.md) | estratégia de testes, matriz de corrupção, interop, fuzzing, cobertura | — |

> [`test_vectors.md`](test_vectors.md) **não se edita à mão**. É gerado
> por `cargo run -p onyxchatd --example gerar_vetores` a partir de
> `tests/vectors/*.json`, que são a fonte única de verdade. Um teste
> (`tests/vetores.rs`) falha se o documento divergir dos JSON.

---

## Modelos

Documentos que declaram **o que se pretende proteger** e **o que não
se pretende**.

| Documento | Declara |
| --- | --- |
| [`threat_model.md`](threat_model.md) | adversários A1–A10, superfície por fronteira, limitações |
| [`security_model.md`](security_model.md) | propriedades pretendidas vs demonstradas; defesa em profundidade |
| [`privacy_model.md`](privacy_model.md) | conteúdo, identidade, IP e metadados, separadamente |
| [`roadmap.md`](roadmap.md) | IMPLEMENTADO / PLANEADO / CONCEITO e a regra editorial |

---

## Princípio da tolerância de versões

> **Se existe mais do que uma versão de um artefacto, o sistema tem de
> tolerar versões diferentes.** Um utilizador com a versão antiga e outro
> com a versão nova têm de poder trabalhar.

O OnyxChat tem formatos versionados em três sítios: o **keystore**
(`ONYXKS<n>`), o **protocolo IPC** (o `HELLO` transporta uma versão) e os
**formatos de rede** (envelope e handshake, ambos com versão assinada).

A regra é sempre a mesma, e vale para todos:

```text
LEITURA   aceita todas as versões nativamente suportadas
ESCRITA   produz sempre a versão mais recente
```

### Onde está o limite: tolerância não é downgrade

Isto parece contraditório, e a reconciliação é o ponto.

Um *downgrade* é obrigar o receptor a aceitar uma versão antiga **por
causa de um atacante**. Aceitar uma versão antiga **porque se suporta
nativamente** não é downgrade — é compatibilidade.

O que torna a distinção verificável é que, nos formatos de rede, **a
versão está dentro da região assinada**:

```text
aceitar v1 quando se suporta v1  →  compatibilidade
aceitar v1 porque alguém forçou → downgrade, e a assinatura denuncia-o
```

O receptor sabe sempre **qual** versão foi usada, porque está assinada.
Pode então aplicar as regras certas a cada uma.

A regra prática, por consequência:

| Situação | Decisão |
| --- | --- |
| A versão recebida está na minha lista de suporte | aceitar |
| A versão é desconhecida | recusar, com o número na mensagem |
| A versão é conhecida mas mais antiga que a mais recente que suporto | aceitar |

E **nunca**:

```text
aceitar uma versão que não conheço, "para ser tolerante"
```

Aceitar o desconhecido seria transformar a tolerância numa falha aberta.

### Onde está implementado

| Artefacto | Leitura | Escrita | Onde |
| --- | --- | --- | --- |
| Keystore | v1 e v2 | v2 | `messenger/storage.py`, [`key_management.md`](key_management.md) |
| IPC `HELLO` | mesma major, minor diferente | a local | [`ipc_spec.md`](ipc_spec.md) §Protocolo de versão |
| Envelope | versões na lista de suporte | a mais recente | [`message_format.md`](message_format.md) §Versionamento |
| Handshake | versões na lista de suporte | a mais recente | [`handshake.md`](handshake.md) §Transcript |

Cada implementação segue a regra; o que muda é a granularidade da
versão (uma byte, um byte `major.minor`, uma constante).

---

## Regra editorial dos rótulos

Toda a documentação distingue três estados. Esta regra é **vinculativa**
para todos os documentos:

```text
IMPLEMENTADO   existe, funciona e está testado
PLANEADO       decidido, especificado, ainda sem código
CONCEITO       investigação, sem especificação formal
```

Um documento que descreva routing nodes, discovery distribuído,
Fake-IP ou multimédia está a descrever `PLANEADO` ou `CONCEITO` —
**nunca** `IMPLEMENTADO`.

Rótulo sem equivalente estrutural é proibido. Um parágrafo que
descreva comportamento do sistema e não diga em que estado está é um
parágrafo que vai divergir.

---

## Convenção de nomes

Os documentos normativos referenciam-se por nome de ficheiro sem
extensão dentro de texto corrido (`pipeline.md §K7`), e com caminho
completo quando o leitor pode não estar em `docs/`
(`docs/testing.md §Fuzzing`).

Âncoras estáveis de secção usam o cabeçalho exacto. Ao acrescentar uma
secção a um spec, actualiza [`DEV_GUIDE.md`](DEV_GUIDE.md) §6 se ela
for alvo de alteração frequente.
