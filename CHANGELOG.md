# Registo de alterações — OnyxChat

Todas as alterações relevantes deste projecto estão registadas aqui.

O formato segue [Keep a Changelog](https://keepachangelog.com/pt/1.1.0/),
e o versionamento é [SemVer](https://semver.org/lang/pt/). O que o
projecto acrescenta à norma é a exigência de que **toda a linha aqui
escrita seja verificável no código**. Uma entrada que descreva
funcionalidade sem dizer onde está é uma promessa, e um `PLANEADO`
registado como feito custa mais caro do que um item em falta: quem o lê
constrói por cima dele e só tarde descobre que não havia lá nada.

---

## [Não publicado]

Trabalhos que ainda não têm versão. A secção não existia até haver o
que pôr nela — um `CHANGELOG` com uma secção vazia é um `CHANGELOG` que
convida a encher de promessas, e o que está aqui é o que está feito.

### Adicionado — relay de ESP32

| Item | Onde |
| --- | --- |
| Firmware de relay para ESP32 DEVKIT V1, ligado como *submodule* | [`OnyxMiniNode/`](https://github.com/Shega-PT/OnyxMiniNode) — repositório próprio, **MIT** |
| Implementação do protocolo de [`docs/relay.md`](docs/relay.md) byte a byte | `src/frame.cpp`, `src/sessao.cpp`, `src/caixa.cpp` do submodule |
| SHA-256 com o **acelerador de hardware** do ESP32, via mbedTLS | `src/mailbox.cpp` §Onde está o acelerador |
| 55 casos de teste de host, sem placa e sem ESP-IDF | `pio test -e native` no submodule |
| CI próprio do submodule: compilar, testar, mapa, higiene | `.github/workflows/portoes.yml` do submodule |

### Adicionado — regra que decide a licença por componente

| Item | Onde |
| --- | --- |
| «O que tem segredos é PolyForm. O que é transporte cego é livre.» | [`docs/architecture.md`](docs/architecture.md) §A licença segue a fronteira |
| A regra aplicada ao `OnyxMiniNode`, com a aritmética da RAM | [`COPYRIGHT.md`](COPYRIGHT.md) §O `OnyxMiniNode` tem licença diferente |
| O submodule na tabela dos três: discovery, relay, routing | [`docs/architecture.md`](docs/architecture.md) §Discovery ≠ Routing |
| O que o OnyxMiniNode **não** resolve, item a item | [`docs/roadmap.md`](docs/roadmap.md) §O que o OnyxMiniNode não resolve |
| As implementações do protocolo e os seus limites | [`docs/relay.md`](docs/relay.md) §Quem implementa este protocolo |

### Adicionado — o que este repositório aprendeu com o submodule

Três exclusões que só a existência de um repositório com licença
diferente tornou necessárias, e cada uma com um teste:

| Item | Onde |
| --- | --- |
| `ARVORE_INDEPENDENTE`: o guard de licença não desce a uma árvore MIT | `network/daemon_rust/tests/guarda_tor.rs`, com dois testes |
| `IGNORADOS_ARVORE`: a contagem de ficheiros não inclui os de outro repositório | `scripts/verificar_estrutura.py` — 2 905, e não 2 917 |
| `CAMINHOS_IGNORADOS`: os auditores de texto não varrem outro repositório | `scripts/verificar_ambiente.py` |
| `submodules: false` declarado nos quatro `checkout` | `.github/workflows/` — o CI não vê o firmware, e é isso que se quer |

---

## [0.1.0] — 2026-10-13

- **Autor:** ShegaPT (pseudónimo)
- **Titular dos direitos de autor:** Carlos Alberto da Silva Brito
- **Licença:** PolyForm Noncommercial 1.0.0 — uso comercial exige
  autorização do titular, ver [`COMMERCIAL-LICENSE.md`](COMMERCIAL-LICENSE.md)
- **Estado:** primeira versão. Ainda **não publicada** — o repositório
  não tem etiqueta de versão, e os cinco manifestos (`Cargo.toml`,
  `Cargo.toml` do `crypto_core`, `pyproject.toml`, `UI/package.json` e
  `docs/test_vectors.md`) concordam em `0.1.0`.

> Esta entrada cobre **tudo** desde o primeiro commit, em 2026-10-06.
> Não há entradas anteriores porque nunca houve uma versão
> anterior — o primeiro commit já continha o sistema completo, e o que
> se seguiu foi Consertá-lo e não construí-lo.

### Adicionado — núcleo criptográfico

| Item | Onde |
| --- | --- |
| Pipeline híbrido K1→K9, com parâmetros documentados | `crypto/rust/`, [`pipeline.md`](docs/pipeline.md) |
| Envelope binário assinado, com região autenticada | `message_format.md`, `envelope.rs` |
| Handshake de amizade assinado, com versão e anti-replay | `handshake.md`, `handshake.rs` |
| Identidade Ed25519 e gestão de chaves com zeroização | `signer.rs`, `secret.rs`, [`key_management.md`](docs/key_management.md) |
| `mlockall(MCL_CURRENT\|MCL_FUTURE)` no arranque, com degradação explícita | `paginas.rs`, `main.rs` |
| Rendimento do registo anti-replay entre reinícios | `anti_replay.rs` |

### Adicionado — rede e processos

| Item | Onde |
| --- | --- |
| Daemon Rust (`onyxchatd`) | `network/daemon_rust/` |
| Tor embutido (`BackendTor` + arti), com falha explícita sem a *feature* | `tor.rs`, `tor_arti.rs` |
| P2P directo e transporte opcional por relay | [`p2p.md`](docs/p2p.md), [`relay.md`](docs/relay.md) |
| Discovery `ID → .onion` com TTL | [`discovery.md`](docs/discovery.md) |
| IPC local sobre UDS, autenticado por `SO_PEERCRED` | [`ipc.md`](docs/ipc.md) |
| Sidecar HTTP em `127.0.0.1` sobre o socket Unix | `server/`, [`sidecar.md`](docs/sidecar.md) |

### Adicionado — interface

| Item | Onde |
| --- | --- |
| Onze ecrãs, quarenta e cinco componentes, sistema de design | `UI/` |
| Conta local cifrada com frase de segurança, sem serviço externo | [`conta.md`](docs/conta.md) |
| Modo de demonstração com aviso visível e interruptor | [`UI/README.md`](UI/README.md) |

### Adicionado — as acções do menu de contexto

Seis botões estavam desenhados e não escreviam. As colunas `fixada`,
`silenciada` e `bloqueado` estavam no esquema SQLite desde o início e
eram **lidas sem nunca serem escritas** — a interface mostrava três
estados que nada produzia.

- cinco métodos de escrita em `server/loja.py` — `fixar_conversa`,
  `silenciar_conversa`, `limpar_historico`, `bloquear_contacto` e a
  Already-existente `marcar_lida`, agora exposta por rota;
- cinco rotas novas em `server/rotas.py`, todas `POST` (a rota
  `/conversas/{id}/lida` já existia desde a Etapa 5 sem que nada a
  chamasse);
- cinco métodos em cada um dos dois adaptadores, mais a declaração no
  contrato `data-adapter.js` — **25** em cada, e o número passa a ser
  conferido por `scripts/verificar_estrutura.py`;
- `Conversations.jsx` passou a decidir o que cada botão faz.

Três invariantes ficaram por escrito: as acções recebem a conversa
inteira e não o `id` (porque «fixar» e «silenciar» são inversões, e
inverter exige saber o estado actual); o valor de destino vai explícito
no corpo em vez de o servidor alternar, e um corpo sem o campo é `400`
em vez de valer por `false`; limpar o histórico e bloquear passam por
confirmação, porque as outras quatro invertem um estado que a segunda
pressão desfaz.

### Adicionado — qualidade e verificação

| Item | Detalhe |
| --- | --- |
| Testes | 407 em Rust, 812 em Python, 3 em C/C++ |
| Cobertura de linhas | 100% em Python e C/C++; **99,38%** em Rust |
| Test vectors multi-linguagem, gerados e verificados | [`test_vectors.md`](docs/test_vectors.md) |
| Property tests e fuzzing dos parsers | [`testing.md`](docs/testing.md) |
| Portões de entrega | [`scripts/testar.sh`](scripts/testar.sh) — quinze passos |
| Auditoria da cadeia de fornecimentos | segredos, advisories de Rust e de JavaScript, licenças, e os próprios *workflows* |

### Corrigido — documentação que afirmava o contrário

Sete documentos diziam que algo não existia quando existia. Um `PLANEADO`
que já tem código é pior do que um item em falta, porque **não dá erro**:
o Markdown continua bem formado e o leitor vê uma linha igual às outras.

| Alegação falsa | Onde estava | A verdade |
| --- | --- | --- |
| `mlockall` por fazer | quatro documentos | implementado desde a fase F |
| `pub_de_hex()` é código morto | [`ipc.md`](docs/ipc.md) | chamado em `ipc.rs:879` |
| anti-replay só em memória | [`message_format.md`](docs/message_format.md) | persiste, com `fsync` e `rename` |
| não há cliente de discovery em `messenger/` | [`discovery.md`](docs/discovery.md) | `messenger/descoberta.py` |
| zeroização em C por fazer | [`DEV_GUIDE.md`](docs/DEV_GUIDE.md) | `sodium_memzero` em `k9_chacha.c` |
| K4 «cifragem: `PLANEADO`» | [`pipeline.md`](docs/pipeline.md) | já validado em `transposition.cpp` |
| `cargo deny` por instalar | [`THIRD-PARTY.md`](THIRD-PARTY.md) | é portão desde a auditoria |

A tabela do `IMPLEMENTADO` no `roadmap.md` dizia «cobertura 100% em três
linguagens». A verdade é 99,38% em Rust, com seis das quarenta e cinco
linhas em falta alcançáveis e por testar — declaradas uma a uma em
[`testing.md`](docs/testing.md), porque declarar as restantes inatingíveis
seria a mesma mentira.

### Corrigido — texto que mentia a quem usava o programa

- `Settings.jsx`, no ramo **real** (não de demonstração), dizia «a ponte
  que o serve ainda não existe — chega na Etapa 5». A Etapa 5 está
  concluída desde havia meses.
- `TopBar.jsx` prometia «Bloquear» e «Exportar chave» da Etapa 4, que
  já estava concluída, e afirmava «o OnyxChat não tem contas». Passou a
  ligar as acções reais: trancar a sessão e exportar a cópia cifrada.
- `Recuperar.jsx` dizia que «a Etapa 5 põe aqui o sidecar real».

Ao ligar essas acções apareceu um **defeito real** que a leitura não
teria encontrado: a cópia de segurança era exportada em base64 e o ecrã de
recuperação fazia-lhe `JSON.parse`, que falha sempre. O fluxo de restauro
nunca funcionou. Corrigido, e a demonstração passou a passar pelo mesmo
caminho — uma demonstração que exercita um caminho diferente do real
valida esse caminho, não o que importa.

### Adicionado — `docs/sidecar.md`

Três ficheiros de código apontavam para este documento e ele não
existia: `server/sidecar.py`, `server/origem.py` e
`tests/test_origem.py` — este último com uma premissa que o exige
exister, porque `ONYX_ORIGENS` deixar de estar documentado faria a
excepção desaparecer **em silêncio**.

### Corrigido — consistência estrutural

- **Contagens erradas em três sítios.** O contrato da interface tinha
  «22 métodos», «catorze funções» e «vinte e uma funções» — o número
  real nunca foi nenhum dos três, e o 22 era a contagem das **rotas**,
  copiada para a outra. A contagem passou a ser conferida pelo portão.
- **Colisão de nomes.** «Etapa 7» significava três coisas diferentes: a
  interface, a rede, e uma terceira em `test_e2e_sidecar.py`. A rede
  passou a escrever «etapa de rede», sem número.
- **`docs/sidecar.md` inexistente** — ver acima.
- **Defeitos menores:** uma tabela que dizia «cinco áreas» sobre quatro,
  duas linhas duplicadas, um passo duplicado, um vector de testes
  omitido, e remissões para secções que não falavam do que se lhes
  perguntava.

### Documentação

Vinte e um documentos normativos e de modelo, com a distinção
`IMPLEMENTADO` / `PLANEADO` / `CONCEITO` como regra editorial:
[`roadmap.md`](docs/roadmap.md) §Como se decide.

### Registado — o que fica por fazer

Doze itens `PLANEADO`, todos com secção normativa, e seis `CONCEITO`.
Três são de segurança e estão declarados em
[`threat_model.md`](docs/threat_model.md):

- ligar a K1 derivada ao caminho por omissão — hoje `ipc_client.cifrar()`
  passa a K1 de longa duração, **sem forward secrecy**;
- mover as chaves de sessão para fora de `argv` e do stdout;
- levar o relay a `.onion`, que hoje abre um TCP directo.

O maior é o primeiro, e é uma decisão de migeração e não um defeito:
ligá-lo quebra a compatibilidade entre clientes de versões diferentes.


### Corrigido — o aviso de licença e o guard que o fiscaliza

O `Required Notice:` apontava para `…/Shega-PT/OnixChat`, com um «i» a
mais, em **108 ficheiros** versionados. O remoto real — o único sítio onde
o endereço existe sem ser escrito por alguém — é `…/OnyxChat`. Corrigido
nos 108, e a verificação do endereço acrescentada ao guard para que não
divirja outra vez.

O aviso faltava nos **91 ficheiros `.js`/`.jsx`** da interface: o guard
recolhia `.rs .py .c .cpp .h` e via 108 ficheiros de aviso e zero dos 91
ficheiros JavaScript, que são um terço do código. Acrescentados, e a lista
de extensões do guard alargada.

E o aviso passou a acompanhar **o pacote construído** — `UI/dist/`, que é
a parte que sai da máquina. A PolyForm fala de quem recebe «uma cópia de
qualquer parte»; a parte que sai não são os ficheiros de origem. Posto
em `vite.config.js` num `postBanner` — nunca num `banner`, que a
minificação levaria sem deixar rasto — e verificado pelo job da interface
no CI.

Três alterações ao guard, todas com prova de que detectam:

| Antes | Agora |
| --- | --- |
| `tests/` e `fuzz/` fora de tudo | fora do **aviso** (não são distribuídos), dentro da contagem de **marcadores** |
| o endereço nunca era conferido | conferido, em todos os ficheiros de código |
| o `dist/` não estava excluído | excluído — o guard lê o disco, e `dist/assets/` tem `.js` |

O guard tinha um ponto cego que valia mais que os outros: **excluía-se a
si próprio**. `guarda_tor.rs:1` carregava um marcador por resolver na
primeira linha, invisível ao teste que mora nesse ficheiro, durante
meses. O guard dava verde e o erro estava no seu próprio cabeçalho.

### Corrigido — duas constantes mortas nos auditores

`DOCUMENTOS_ESTRANGEIROS` em `verificar_frases.py` e `GRAMATICA` em
`verificar_portugues.py` não eram consultadas por nada.

A primeira era a mais grave: continha `README`, `LICENSE` e `CHANGELOG`,
que são os três documentos que existem **e que estão em português**.
Tê-la ligado era pôr os três documentos mais lidos do projecto fora da
auditoria de português. Foi apagada em vez de ligada.

A segunda eram oito regras de concordância nunca aplicadas, com defeitos
visíveis sem correr nada — uma delas invertida em relação ao objectivo,
outra com uma alternativa repetida duas vezes. Também apagada.

Ambas têm agora um teste que falha se voltarem.

---

## Versões

Formato baseado em [SemVer](https://semver.org/lang/pt/), com o
significado de cada parte:

| Parte | Significado aqui |
| --- | --- |
| **MAIOR** | formato de disco, de protocolo ou de interface incompatível com o que está instalado |
| **MENOR** | funcionalidade nova, compatível com o que está instalado |
| **REVISÃO** | correcção de defeito, sem alteração de formato |

Enquanto a major for **0**, o projecto declara a si próprio que é uma
arquitectura em construção e não um produto. [`SYS_GUIDE.md`](docs/SYS_GUIDE.md)
§1.2 diz o mesmo noutro idioma: «é uma base de arquitectura com
especificações formais e testes de paridade».

> ## Onde está esta versão
>
> O repositório não tem etiqueta de versão. A referência aponta ao
> repositório e não a uma *release*, porque uma release ainda não
> existe — e um link para uma página que devolve 404 é a mesma promessa
> que não se cumpre que este documento passou o dia inteiro a corrigir.
>
> Quando publicares, `git tag v0.1.0` torna a linha abaixo verdadeira e
> a referência passa a apontar para a *release*.

[0.1.0]: https://github.com/Shega-PT/OnyxChat