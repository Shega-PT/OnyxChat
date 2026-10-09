# Roadmap — estado do projecto

Este documento **descreve o estado**, não compromete datas. Serve para
que uma ideia futura nunca seja confundida com funcionalidade existente.

Rótulos usados:

```text
IMPLEMENTADO  →  existe, funciona e está testado
PLANEADO      →  decidido como próximo passo, especificado, ainda sem código
CONCEITO      →  investigação, sem especificação formal
```

> **Regra de sincronização.** Este documento é actualizado no **fim** de
> cada fase de trabalho, depois de o código aterrar — nunca antes. Um
> `PLANEADO` que vira `IMPLEMENTADO` é um rótulo que reflecte a
> realidade, não uma intenção.

---

## IMPLEMENTADO

### Núcleo criptográfico

| Item | Onde |
| --- | --- |
| Pipeline K1→K9 completo | [`pipeline.md`](pipeline.md), `pipeline.rs` |
| Envelope binário com assinatura | [`message_format.md`](message_format.md), `envelope.rs` |
| Handshake de amizade assinado (com versão) | [`handshake.md`](handshake.md), `handshake.rs` |
| Anti-replay (handshake + chat por `nonce1`) | `handshake.rs`, `anti_replay.rs` |
| Anti-downgrade (versão assinada) | envelope + transcript |
| Identidade Ed25519 | `crypto/rust/src/signer.rs` |
| Gestão de chaves e zeroização | [`key_management.md`](key_management.md), `secret.rs` |

### Rede e processos

| Item | Onde |
| --- | --- |
| Daemon Rust (`onyxchatd`) | `network/daemon_rust` |
| Tor embutido (`BackendTor` + arti) | `tor.rs`, `tor_arti.rs` |
| Falha explícita quando `--tor arti` sem a feature | `main.rs`, `construir_backend` |
| P2P directo (hidden service ↔ hidden service) | `p2p.rs` |
| Transporte por relay opcional | [`relay.md`](relay.md), `relay_server.py` |
| Discovery `ID → .onion` com TTL | [`discovery.md`](discovery.md), `discovery_server.py` |
| IPC local UDS (autenticado por uid) | [`ipc_spec.md`](ipc_spec.md), `ipc.rs` |
| Cliente Python (mensageiro, keystore cifrado) | `messenger/`, `user/` |
| Interface React completa (sistema de design, 11 ecrãs, 45 componentes) | [`../UI/README.md`](../UI/README.md) |
| Modo de demonstração com aviso visível e interruptor | [`../UI/README.md`](../UI/README.md) §Modo de demonstração |

### Qualidade

| Item | Onde |
| --- | --- |
| Testes unitários, integração e E2E | `tests/`, `#[cfg(test)]`, CTest |
| Test vectors multi-linguagem | [`test_vectors.md`](test_vectors.md), `tests/vectors/*.json` |
| Property tests (proptest/hypothesis) | [`testing.md`](testing.md) |
| Fuzzing dos parsers | `fuzz/` |
| Cobertura de linhas 100% (3 linguagens) | [`testing.md`](testing.md) §Cobertura |

### Documentação

| Item | Onde |
| --- | --- |
| Guias: sistema, utilizador, desenvolvedor | `SYS_GUIDE.md`, `USER_GUIDE.md`, `DEV_GUIDE.md` |
| Índice e ordem de leitura | [`index.md`](index.md) |
| Estrutura técnica | [`architecture.md`](architecture.md) |
| Modelos de ameaça/segurança/privacidade | [`threat_model.md`](threat_model.md), [`security_model.md`](security_model.md), [`privacy_model.md`](privacy_model.md) |
| Especificações de protocolo | `pipeline.md`, `message_format.md`, `handshake.md`, `ipc_spec.md`, `p2p.md`, `relay.md`, `discovery.md` |

---

### Itens que atravessaram fases

Itens que estavam especificados e passaram a ter código. Cada linha
diz o que ficou por fazer, porque é essa a parte que interessa a quem
ler o PLANEADO a seguir.

| Item | Onde | Nota |
| --- | --- | --- |
| `destino` por ID hex: validação no relay e resolução `ID → .onion` no cliente | [`ipc_spec.md`](ipc_spec.md) §`destino`, [`discovery.md`](discovery.md) §Implementação | **resolvido.** `ipc.rs` compara `pub_de_hex(destino)` com a pública do par e devolve `ERR_DESTINO_INVALIDO` quando não coincide; `messenger/descoberta.py` aceita ID hex ou `.onion` v3 e rejeita o resto. `pub_de_hex()` **não** é código morto, ao contrário do que este roadmap afirmava até 2026-10-07: é `ipc.rs` que a chama nesse caminho |
| Integração contínua | [`DEV_GUIDE.md`](DEV_GUIDE.md) §3.6, [`testing.md`](testing.md) §Os portões de entrega | **resolvido em 2026-10-07.** `.github/workflows/portoes.yml` corre os oito portões em cada push e `verificacao.yml` corre a mesma matriz de `testar.sh`, mais os jobs de Tor e de fuzzing agendados e o job da interface. O CI **não** mede memória — os tectos são uma verdade local, medida no cgroup — e corre com `ONYXCHAT_SEM_CGROUP=1`, que o `testar.sh` declara em cada passo. O Node vem de `UI/tools/onyxchat instalar`, com o SHA-256 fixado em `UI/tools/versoes.txt`, e não de `setup-node`. **Os workflows ainda não foram executados**: só o YAML e a existência dos comandos estão verificados |
| Segurança da cadeia de fornecimentos | [`security_model.md`](security_model.md) §5, [`../SECURITY.md`](../SECURITY.md), [`threat_model.md`](threat_model.md) §A sétima fronteira | **resolvido em 2026-10-07.** `scripts/verificar_seguranca.sh` corre cinco camadas — segredos (`gitleaks`), advisories de Rust (`cargo audit`) e de JavaScript (`npm audit` contra `seguranca-excepcoes.toml`), licenças (`cargo deny` com `deny.toml`) e os próprios workflows (`zizmor`) — **com o mesmo script** localmente e no CI, e com `--exigir` a transformar uma ferramenta ausente em falha. À primeira execução encontrou **4 advisories reais**, todos com razão verificável no código e data de revisão. O `tailwindcss-animate` estava em `dependencies` sendo um plugin de compilação. E os `uses:` dos workflows, que eu tinha escrito com tag mutável contra a regra do próprio projecto, passaram a SHA de commit |
| Auditoria integral da documentação | [`testing.md`](testing.md) §Os portões de entrega, [`DEV_GUIDE.md`](DEV_GUIDE.md) §1.2 | **resolvido em 2026-10-07.** **Máquina:** 15 ocorrências da locução proibida, em documentos e comentários, reescritas — um pico de RAM é propriedade do *build*, e a máquina só decide se cabe. Imposto por `verificar_ambiente.py`, com 18 testes e isenção em bloco para a única citação da frase proibida. **Verdades:** `UI/README.md` afirmava «Etapa 4 de 7» com a Etapa 5 marcada como concluída 340 linhas abaixo, e «`bridge-adapter.js` ainda recusa» com 22 métodos implementados; corrigidos, com a Etapa 7 explicitamente `PLANEADO`. **Contagens:** o verificador de estrutura passou a conferir a aritmética dos documentos, e apanhou quatro números errados. **`.gitignore`:** `node_modules/` na raiz não estava ignorado — um `npm install` por engano versionava 14 000 ficheiros |
| Rótulos `PLANEADO`/`CONCEITO` conferidos por [`verificar_roadmap.py`](../scripts/verificar_roadmap.py) | **resolvido em 2026-10-07.** A regra de uma linha — «a coluna «Especificado em» está vazia? então é `CONCEITO`» — passou a ser §Como se decide, e um verificador com 10 testes escreve o erro de volta para o apanhar. Um rótulo errado não dá sintaxe inválida, que é por isso que ninguém o apanhava à mão |
| Índice de ficheiros | `../Estrutura.txt`, conferido por `scripts/verificar_estrutura.py` — **resolvido em 2026-10-07.** O mapa vivia fora do repositório e listava **sete ficheiros que não existiam** (quatro referências Python, `cesar.lua`, `substituicao.lua`, `plugins_loader.py`), além de omitir `scripts/`, `fuzz/`, a interface e `messenger/conta.py`. Reconciliado com a árvore e agora verificado: 167 afirmações de existência e 3 de ausência, com 10 testes em [`../tests/test_verificar_estrutura.py`](../tests/test_verificar_estrutura.py) |
| Ecrãs de acesso locais (entrar, registar, recuperar) | [`../UI/README.md`](../UI/README.md) §Acesso, [`conta.md`](conta.md) | **resolvido na Etapa 4.** `messenger/conta.py` guarda a identidade Ed25519 num keystore cifrado, com a frase de segurança a servir de chave; os ecrãs foram reconstruídos sobre o `AuthLayout` nosso. **Não há recuperação por correio** porque não há servidor — o ecrã de recuperação oferece tentar de novo, repor de uma cópia e recomeçar, e diz o que cada uma custa. **ligado na Etapa 5**: os pontos de entrada `/api/conta/*` do sidecar chamam o `messenger/conta.py`, e `user/cli.py` tem o subcomando `sidecar` que abre a conta antes de servir |
| Ligar a interface ao daemon | [`../UI/README.md`](../UI/README.md) §Etapas | **resolvido na Etapa 5.** `server/sidecar.py` é um servidor HTTP em `127.0.0.1` que fala com o `onyxchatd` pelo socket Unix e serve a interface construída; `UI/src/lib/onyx/bridge-adapter.js` é o único sítio que conhece o endereço e o formato do erro. **Sem WebSocket**: `/api/eventos` responde `501` com o motivo, porque nenhum consumidor do contrato o pede. **Resta** a Etapa 7 — cada página alimentada pelo backend, e não pelo adaptador de demonstração |
| Loja de conversas e contactos | [`../UI/README.md`](../UI/README.md) §Etapas | **resolvido na Etapa 5.** `server/loja.py` é a dona: `sqlite3` da biblioteca padrão, com escrita que não perde tudo se a energia falhar. `ESTADO.amigos` do daemon é um `u16` e não existe comando para listar conversas — o que a loja guarda é o que a **interface** mostra, e as chaves de amizade continuam a ser do daemon, para não haver segredo em dois sítios |
| Persistir as chaves de amizade | [`handshake.md`](handshake.md) §Ciclo de vida, [`key_management.md`](key_management.md), [`conta.md`](conta.md) | **resolvido em 2026-10-07.** `messenger/conta.py` grava as 192 bytes de `ChavesAmizade` no keystore da conta, cifradas com a frase de segurança, e `onyxchat amigos` lista-as. A alternativa — a loja `sqlite3` do sidecar — foi descartada por pôr material que só o daemon devia ter num ficheiro que ninguém abre com uma frase. `aceitar-amizade` e `confirmar-amizade` imprimem **e** guardam |
| Identidade pública sobre SHA-256 | [`../UI/README.md`](../UI/README.md) §Identidade, [`test_vectors.md`](test_vectors.md) §Identidade | **resolvido na Etapa 3.** `messenger/identidade.py` deriva o Onyx ID e a impressão de 128 bits sobre SHA-256 com sal por conta; a interface lê-os em vez de os derivar, e `tests/test_identidade_interface.py` reconfere cada valor. O servidor de descoberta passou a aceitar o alfabeto completo do identificador — antes rejeitava o que o projecto produz. **Resta** ligar a impressão à chave Ed25519 real (Etapa 7) — o registo de conta, com sal por conta, ficou feito na Etapa 4 em [`conta.md`](conta.md) |
| Auditar o português no código JavaScript | [`../UI/README.md`](../UI/README.md) §Padrões, `tests/test_verificar_portugues.py` | **resolvido em 2026-10-07.** Os quatro auditores de texto (`verificar_portugues`, `verificar_frases`, `verificar_comentarios`, `verificar_alfabeto`) cobrem agora `{js, jsx, mjs, cjs}`. Das 236 ocorrências medidas, **duas eram reais** e estão corrigidas. O filtro passou a remover **sintaxe** — o atributo `className` e as famílias de utilitários do Tailwind — em vez de uma lista de palavras, porque a primeira versão filtrava por `PascalCase` e, com isso, deixava de reportar o texto dentro de `SenhaDoUtilizador`. Há **testes** para os dois auditores: um verde que não apanha nada é indistinguível de um verificador que engole texto |

---

## PLANEADO

Decidido como próximo passo, especificado nos documentos normativos,
**ainda sem implementação**:

| Item | Especificado em | Nota |
| --- | --- | --- |

| Semear os corpora de fuzzing a partir dos vectores | [`testing.md`](testing.md) §Fuzzing | os 6 corpora têm **50 sementes versionadas** e o que o `cargo fuzz` gera é ignorado; o que falta é semear a partir de `tests/vectors/*.json` em vez de bytes arbitrários, para o corpus nascer de inputs que o projecto já sabe serem Interesting |
| `mlockall(MCL_CURRENT\|MCL_FUTURE)` no arranque do daemon | [`key_management.md`](key_management.md) §Exposição a swap, [`threat_model.md`](threat_model.md) §A10 | fecha a lacuna entre zeroização e page-out |
| Routing nodes (nós de transporte entre instâncias) | [`SYS_GUIDE.md`](SYS_GUIDE.md) §10.4 | preparação em [`architecture.md`](architecture.md). **Não confundir com as rotas HTTP**: `server/rotas.py` e o `routing` de `server/origem.py` são a tabela de rotas do sidecar, e já existem. Routing nodes aqui são nós **P2P**, e não existem |
| Rede de transporte descentralizada | [`SYS_GUIDE.md`](SYS_GUIDE.md) §10.4 | depende de routing nodes |
| Discovery descentralizado / distribuído | [`discovery.md`](discovery.md) §Componente substituível | substitui o servidor único |
| Alimentar cada página da interface pelo backend, e não pelo adaptador de demonstração | [`../UI/README.md`](../UI/README.md) §Etapas | o sidecar tem os pontos de entrada e `bridge-adapter.js` sabe falar com eles; falta a página pedir os dados em vez de os inventar. **É a diferença entre uma demonstração e um produto** |
| Ligar a impressão de identidade à chave Ed25519 real | [`../UI/README.md`](../UI/README.md) §Identidade, [`conta.md`](conta.md) | `messenger/identidade.py` deriva a impressão sobre SHA-256 com sal por conta; falta ligá-la ao par de chaves que a conta guarda, para a impressão passar a ser verificável por terceiro |
| **Ligar a K1 derivada ao caminho por omissão** (fase I) | [`key_management.md`](key_management.md) §Chaves de comunicação, [`threat_model.md`](threat_model.md) §Limitações 8 | a derivação está implementada e testada nas duas linguagens, mas `ipc_client.cifrar()` continua a passar a K1 de longa duração. **É uma decisão, não um bug**: clientes velhos e novos não se entendem, e o que se perde sem forward secrecy é a major parte do benefício declarado |

---

## CONCEITO

Investigação — **sem especificação formal**, não deve ser descrito
como funcionalidade:

| Item | Nota |
| --- | --- |
| Fake-IP (camada adicional de anonimização) | exigiria análise completa do modelo de privacidade |
| Pipelines multimédia (áudio, imagem, vídeo) | o pipeline actual é textual, por decisão de âmbito |
| Suporte a ficheiros arbitrários | idem |
| Novos pipelines por tipo de conteúdo | idem |
| Mitigação activa de tráfego (padding, temporização) | declarada em [`privacy_model.md`](privacy_model.md) §4; não planeada |
| Identificadores de algoritmo por camada (negociação de pipeline) | **investigado, não especificado.** Reclassificado de `PLANEADO` para `CONCEITO` em 2026-10-07. A linha vivia na tabela do `PLANEADO` com «Especificado em: —», e a definição do projecto para `PLANEADO` é «decidido, **especificado**, ainda sem implementação»: com a coluna do especificado vazia, a linha era `CONCEITO` desde o princípio. `docs/message_format.md` §Regra 4 diz que o byte de versão identifica um **perfil fixo** (K1..K9 com os parâmetros de `pipeline.md`) e que um segundo perfil receberia `0x02` — o que descreve o *custo*, não a convenção. O que falta para ser `PLANEADO` é a parte difícil: o formato do identificador, quem o escolhe, e o que um receptor faz quando não conhece o perfil — e essa resposta muda a política de leitura/escrita de versões de `index.md` §Princípio da tolerância de versões |

---

## Estado actual vs futuro — regra editorial

Toda a documentação do projecto distingue estes três rótulos. Se um
documento descrever routing nodes, discovery distribuído, Fake-IP ou
multimédia, está a descrever **PLANEADO** ou **CONCEITO** — nunca
**IMPLEMENTADO**.

Um parágrafo que descreva comportamento do sistema sem dizer em que
estado está é um parágrafo que vai divergir do código. Rótulo sem
equivalente estrutural é proibido.

### Como se decide, e o teste de uma linha

A distinção entre `PLANEADO` e `CONCEITO` **não é de grau de certeza** —
é de especificação. `PLANEADO` exige três coisas ao mesmo tempo: uma
decisão tomada, uma especificação escrita, e ausência de código.

O teste de uma linha, e é o que vale:

```text
a tabela do PLANEADO tem uma coluna «Especificado em»
→ se aponta para um documento normativo, é PLANEADO
→ se está vazia, é CONCEITO, porque nada foi especificado
```

A linha dos identificadores de algoritmo por camada vivia no `PLANEADO`
com essa coluna a `—` desde que o roadmap foi escrito. Não havia código
(não havia), não havia decisão registada, e não havia especificação: o
`docs/message_format.md` §Regra 4 descreve o **custo** de um segundo
perfil — «recebe nova versão, ex.: `0x02`» — que é consequência, não
convenção. Um documento que descreve o custo não especifica o formato.

Os três itens de rede que ficam `PLANEADO` — routing nodes, transporte
descentralizado, discovery descentralizado — têm todos secção normativa
que os descreve. É essa diferença, e não a proximidade com o código, que
separa as duas tabelas.
