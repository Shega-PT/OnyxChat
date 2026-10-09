# OnyxChat — interface

Interface do OnyxChat: React + Vite, escrita em JavaScript, sem
dependência de ningún serviço externo em tempo de execução.

> **Estado actual: Etapas 1 a 5 concluídas.** Há conta local, com a frase
> de segurança a proteger um ficheiro cifrado, e ecrãs de registo, entrada
> e recuperação. A aplicação **não contacta nenhum servidor ao arrancar** —
> nem para arrancar, nem em modo de demonstração.
>
> As páginas **vêm** pelo `dataAdapter`, e ele escolhe: com o interruptor
> de demonstração ligado responde o `mock-adapter`; desligado
> (`?mock=0`), responde o `bridge-adapter`, que fala com o `sidecar` em
> `127.0.0.1`, que por sua vez fala com o `onyxchatd` pelo socket Unix.
>
> A predefinição é demonstração — e é uma decisão, não uma inadvertência:
> quem não sabe o que está a ver continua a ver uma demonstração rotulada
> como tal, e quem sabe desliga o interruptor e vê o backend verdadeiro.
>
> O que **falta** é a Etapa 7: cada ecrã tratar o backend como fonte
> principal e não como hipótese, e mostrar o que o sidecar devolveu em vez
> do que o adaptador de demonstração inventou. Está registada em
> [`../docs/roadmap.md`](../docs/roadmap.md).
>
> O que falta e o que vem a seguir está escrito abaixo sem atenuantes.

---

## Requisitos

| Requisito | Versão | Porquê |
| --- | --- | --- |
| **Node.js** | `^20.19.0` ou `>=22.12.0` | declarado em `package.json` |

O requisito não é arbitrário: o Vite 8.3.3 declara
`engines: { node: "^20.19.0 || >=22.12.0" }` e usa `util.styleText`, que só
existe a partir do Node 20.12. **Com o Node 18 o `npm run build` falha com
um erro que não menciona versões** (`The requested module 'node:util' does
not provide an export named 'styleText'`), e `npm install` deixa o
`rolldown` sem a ligação nativa da plataforma.

### A ferramenta local

O Node é **trazido dentro do repositório**, em `UI/tools/node/`, em vez de
exigir que o instale pelo gestor de pacotes do sistema:

```bash
UI/tools/onyxchat instalar      # Node local, sem sudo
UI/tools/onyxchat instalar-ui   # o anterior + as dependências
UI/tools/onyxchat verificar     # o que falta
```

São ~120 MB, **não versionados**. O pacote vem de nodejs.org e a soma
SHA-256 é conferida contra `UI/tools/versoes.txt` — que é versionado, e
não descarregado do mesmo sítio de onde veio o pacote. A razão está em
[`tools/README.md`](tools/README.md).

`onyxchat` não falha se a cópia local não existir e o sistema já tiver um
Node bom: usa-o e avisa. Se o do sistema for antigo, recusa-se a arrancar.

## Comandos

Atalho para o dia-a-dia:

```bash
UI/tools/onyxchat dev           # servidor de desenvolvimento
UI/tools/onyxchat build         # compila para dist/
UI/tools/onyxchat ver           # lint + tipos + construção
```

Ou directamente, **depois** de a ferramenta estar instalada:

```bash
npm install       # instala as dependências
npm run dev       # servidor de desenvolvimento
npm run build     # compila para dist/
npm run preview   # serve dist/ para inspeção
```

Portões de verificação — **os três têm de passar**:

```bash
npm run lint          # ESLint, avisos incluídos
npm run lint:quiet    # só erros — é este o portão
npm run typecheck     # TypeScript sobre JavaScript (checkJs)
npm run build         # Vite
```

As dependências de sistema do projecto inteiro — Node incluído — estão em
[`../requisitos.txt`](../requisitos.txt) e são verificadas por
`../scripts/verificar_requisitos.sh`.

### Avisos que ficam

`npm run lint` mostra dez avisos e zero erros. Nenhum é um defeito de
código: são todos a mesma família, e todos se resolvem quando a camada de
dados deixar de ser o adaptador de demonstração.

| Onde | Aviso | Porque fica |
| --- | --- | --- |
| `lib/onyx/use-async.js:25` | `useEffect` recebe um array de dependências com expansão (`[run, ...deps]`) | O gancho aceita dependências por argumento, o que impede a verificação estática. Substitui-se quando o carregamento passar a React Query (Etapa 5). |
| `lib/onyx/onyx-context.jsx:67,76` | `useDetails` e `useContextLabel` recebem dependências por argumento | O contrato é "republicar um nó React sempre que as dependências mudam"; a regra não tem como avaliar isso. |
| `pages/Contacts.jsx:33`, `pages/Conversations.jsx:19` | `data \|\| []` dentro das dependências produz um array novo em cada render | Desaparece com React Query, que mantém identidade estável para o mesmo estado. |
| `pages/OAuthConsent.jsx:75,116` | variáveis não usadas | **Resolvido na Etapa 2**: o ficheiro saiu com o resto da integração hospedada. |

Não foram silenciados. Um aviso desligado porque inconvenientemente é pior
do que um aviso visível, porque a regra deixa de ter utilidade no ponto
em que ia ser útil.

---

## Modo de demonstração

A interface mostra **dados fictícios** por omissão, e diz isso no ecrã.

### Porquê

Porque o backend não tem uma superfície que um browser possa usar, e
porque a demonstração é o que permite desenhar e mostrar a interface sem
ele. É exactamente por isso que é perigoso: os dados de demonstração
incluem coisas que o OnyxChat **não tem** — latências, dispositivos, um
score de segurança, um grupo de conversa, o protocolo `ONYX-SP 4.2`.

### O aviso

Enquanto a demonstração está ligada, uma faixa aparece **acima da barra
superior**, e não pode ser dispensada. O único botão é *desligar*.

Um aviso dispensável é um aviso que se dispensa, e a consequência de
dispensar este seria alguém mostrar a interface a outra pessoa e essa
pessoa sair a crer que o OnyxChat tem 92 pontos de segurança.

### As três formas de o controlar

| Onde | Prioridade | Quando usar |
| --- | --- | --- |
| **Definições → Demonstração** | 1 | a decisão do utilizador |
| `?mock=1` / `?mock=0` na ligação | 2 | uma demonstração pontual; sobrepõe sem gravar |
| `VITE_ONYX_MOCK` | 3 | a predefinição de uma cópia de demonstração |

Sem qualquer uma, a demonstração **está ligada**. Uma instalação nova a
mostrar-se vazia pareceria estar avariada.

Mudar o interruptor recarrega a aplicação. A escolha da implementação
acontece na leitura de cada chamada ao adaptador; sem recarregar, metade
da aplicação continuaria a falar da demonstração — pior do que estar
inteiramente num dos dois lados.

```text
UI/
├─ index.html                 # sem recursos externos
├─ vite.config.js             # plugin do React + alias `@/`
├─ tailwind.config.js         # tokens da paleta `onyx.*` e animações
├─ jsconfig.json              # âmbito da verificação de tipos
├─ eslint.config.js           # âmbito do lint
├─ public/
│  ├─ fonts/                  # Inter e JetBrains Mono, servidas localmente
│  └─ onyxchat.png            # ícone
├─ tools/                     # Node local — ver tools/README.md
└─ src/
   ├─ main.jsx                # ponto de entrada
   ├─ App.jsx                 # rotas
   ├─ index.css               # paleta OnyxChat e tipografia
   ├─ lib/
   │  ├─ shadcn.js            # fachada tipada sobre o código vendorizado
   │  ├─ SessaoContext.jsx    # estado de acesso (conta, identidade, demonstração)
   │  ├─ authReturnTo.js      # validador de redireccionamento pós-sessão
   │  ├─ PageNotFound.jsx     # 404 na paleta do projecto
   │  ├─ query-client.js      # React Query
   │  ├─ utils.js             # `cn()`
   │  └─ onyx/
   │     ├─ data-adapter.js   # A costura — ver abaixo
   │     ├─ mock-adapter.js   # implementação de demonstração
   │     ├─ bridge-adapter.js # 22 métodos sobre o sidecar (Etapa 5)
   │     ├─ runtime.js        # interruptor de demonstração
   │     ├─ mock-data.js      # Os dados fictícios
   │     ├─ onyx-context.jsx  # Estado partilhado da casca
   │     ├─ use-async.js      # Carregamento
   │     ├─ use-shortcuts.js  # ⌘K, ⌘N, Escape
   │     ├─ format.js         # Datas, truncagem, avatares
   │     └─ nav.js            # Rotas da barra lateral
   ├─ components/
   │  ├─ onyx/                # 45 componentes do sistema de design
   │  ├─ AuthLayout.jsx       # invólucro dos ecrãs de acesso
   │  └─ ui/                  # ⚠ 8 ficheiros do shadcn (7 componentes + 1 gancho)
   └─ pages/                  # Os 11 ecrãs
```

**Os ecrãs de acesso estão aqui, e são nossos.** `Entrar.jsx`,
`Registar.jsx` e `Recuperar.jsx` construíram-se na Etapa 4 sobre o
`AuthLayout`, que é nosso. O que da versão hospedada foi removido — e
removido a sério — são `Login`, `Register`, `ForgotPassword`,
`ResetPassword` e o ecrã de consentimento OAuth: verificação por correio
com código, OAuth do Google e rotas `/api/apps/{id}/mcp/*` pressupõem um
servidor, e este projecto não tem. A *recuperação* de conta resolve-se por
eliminação — tentar de novo, repor de uma cópia, recomeçar — e o ecrã diz
o que cada uma dessas coisas custa.

`AuthLayout.jsx`, o invólucro visual, **foi preservado** e é nosso. A Etapa 4
reconstruiu os ecrãs sobre ele, e construiu-os com os primitivos `Onyx*` e
não com os do shadcn — por isso `button`, `input`, `label` e `input-otp`
saíram do pacote.

---

## Camada de dados

Toda a interface lê e escreve através de um único módulo:
`src/lib/onyx/data-adapter.js`.

```text
páginas  →  dataAdapter  →  mockAdapter     (demonstração)
páginas  →  dataAdapter  →  bridgeAdapter   (backend real, Etapa 5)
```

Nenhuma página conhece a origem dos dados, e nenhuma escreve no adaptador.

**O contrato está escrito por extenso**, no `data-adapter.js`: são catorze
funções com as suas assinaturas. Os dois adaptadores são verificados
contra ele, o que significa que acrescentar uma função a um e esquecer o
outro dá erro de compilação — e não um erro em produção quando uma vista
tenta chamar a que falta.

### `bridge-adapter.js` — o que ainda recusa

São **22 métodos** implementados contra o `sidecar`. `PonteIndisponivel`
fica para três casos, e são os três legítimos:

1. o utilizador está em modo de demonstração, e o `dataAdapter` nunca
   chegou a chamar este adaptador;
2. o sidecar não respondeu dentro do limite — 3 segundos;
3. o pedido ao sidecar foi recusado, e aí é `ErroDoSidecar`, que traz o
   código HTTP e o detalhe que o sidecar devolveu.

A alternativa ao `PonteIndisponivel` seria devolver `[]`, e uma vista a
mostrar «Nenhuma conversa» comunica *não tens conversas*. O que é verdade
quando o sidecar não responde é *não há fonte de dados*. São mensagens
diferentes, e a segunda é a honesta.

### Código vendorizado

`src/components/ui/` tem **8** primitivos (eram 53), gerados pelo
`shadcn add`, e **não é código nosso**. Os seus componentes embrulham o
Radix com `React.forwardRef` e desestruturação por descanso, e o
TypeScript infere mal o tipo das suas props — o erro propaga-se a todos
os nossos consumidores.

A correcção está em `src/lib/shadcn.js`: uma fachada que fixa o tipo de
cada primitivo num único lugar. **Regra: nada em `src/` importa
directamente de `@/components/ui/*` — só de `@/lib/shadcn`.** Para
acrescentar um primitivo, escreve-se a linha na fachada primeiro.

Os ficheiros vendorizados levam `// @ts-nocheck` na primeira linha, o que
os retira do programa verificado sem lhes tocar no resto — um
`shadcn add` reescreve o ficheiro e o resto sobrevive.

---

## O que a interface mostra e o que o backend sabe

Esta é a parte que exige atenção antes de mostrar a interface a alguém.

O backend OnyxChat **não tem** API HTTP pública, nem WebSocket, nem
serviço que um browser possa usar de fora. A interface de cliente do
`onyxchatd` é um *socket Unix* binário (`docs/ipc_spec.md`), e um browser
não abre sockets Unix.

A **Etapa 5** resolveu isso sem expor nada: `server/sidecar.py` é um
servidor HTTP em `127.0.0.1` que fala com o socket Unix, e é o único
processo que faz essa tradução. Serve 22 rotas, não escuta em outra
interface, e `server/origem.py` decide quem tem o direito de lhe falar.

E ainda assim, parte do que a interface mostra **não tem correspondência
no backend**:

| O que a interface mostra | O que o backend tem |
| --- | --- |
| Conversas com `unread`, `pinned`, `muted`, `lastAt` | `server/loja.py`, via `GET /conversas` e `GET /conversas/{id}`; o histórico em `mensagens` e `POST /mensagens` |
| Contactos com `verified`, `note`, `pinned`, `muted` | `server/loja.py`, via `GET /contactos`. **`role`, `tags` e `mutual` não existem** — `ESTADO.amigos` do daemon é um `u16`, e o único comando de listar devolve um nome, não um perfil |
| Conversa de grupo | impossível: o protocolo é estritamente 1:1, uma ligação activa por daemon |
| `ONYX-SP 4.2` | não existe. São IPC `0x01`, envelope `0x01`, handshake `0x01` |
| `QUIC · UDP`, `X25519` | TCP sobre Tor; Ed25519 + ChaCha20-Poly1305 (K1, K9) + AES-256-GCM (K5) |
| `DHT + relays autorizados` | um mapa HTTP com TTL (`server/discovery_server.py`) |
| Score de segurança 92, 3 dispositivos, datas de rotação | nada |
| Latência, jitter, throughput, uptime | nada |

A última linha desta tabela — latência, jitter, throughput — não é um
buraco do sidecar: **não é recolhida por ninguém**. O `sqlite3` da loja
não guarda tempos, e um número de latência que ninguém mede não se
mostra.

**O que muda é a Etapa 7**, e nada disto depende dela: o sidecar tem as
22 rotas e o `bridge-adapter` sabe chamá-las. O que falta é cada ecrã
usar o backend como fonte principal, e mostrar o que ele devolveu.

### Identidade

O identificador `ONYX-XXXXXX-…#` e a impressão digital `AA:BB:…` são
derivados em `messenger/identidade.py`, sobre SHA-256, com um sal de 16
bytes gerado por conta:

```
identificador = SHA-256("ONYX/ID/v1" ‖ sal ‖ NFKC(nome).casefold)
impressão     = SHA-256("ONYX/FP/v1" ‖ identificador)[0:16]
```

O formato visível mantém-se: `ONYX-`, seis caracteres, `-`, quatro, `#`.
O que mudou foi o que está por baixo.

**A interface não deriva nada.** `lib/onyx/format.js` já não calcula
identidades: lê as que recebe, como leria de um sidecar. Derivar no
browser seria possível (`crypto.subtle.digest`), e seria pior por três
razões — a operação é assíncrona, a fórmula passaria a existir em dois
sítios que não se verificam um ao outro, e correria no browser de quem lê
o código em vez de correr uma vez no registo.

`tests/test_identidade_interface.py` é a verificação: recalcula cada valor
a partir do nome que está **no próprio ficheiro de demonstração** e
compara com o que a interface mostra. Os valores em `mock-data.js` são
derivados a sério, com um sal fixo à vista; um número inventado que
«parecesse» uma impressão ensinaria a interface a apresentar algo que não
verifica nada.

Vectores congelados em `tests/vectors/identidade.json`, legíveis em
[`docs/test_vectors.md`](../docs/test_vectors.md) §Identidade. O servidor
de descoberta aceita o alfabeto completo do identificador — sem isso,
rejeitava o identificador que o próprio projecto produz.

### Acesso

O portão de acesso existe, e bloqueia a **identidade**: sem a frase de
segurança não se lê a seed Ed25519, e sem a seed não há assinatura nem
cifra de nada. Não bloqueia uma sessão de rede, porque não há sessão de
rede — não há token, não há expiração, e fechar a janela tranca a conta.

| Ecrã | Ficheiro | O que faz |
| --- | --- | --- |
| Registar | `pages/Registar.jsx` | cria a conta, e mostra a identidade logo a seguir |
| Entrar | `pages/Entrar.jsx` | abre a conta com a frase de segurança |
| Recuperar | `pages/Recuperar.jsx` | três saídas reais, e o custo de cada uma escrito antes do botão |

**Em modo de demonstração o portão não existe.** Não há identidade a
proteger — os dados são fictícios e a faixa está no ecrã — e pedir uma
palavra para ver conteúdo inventado seria absurdo. Os três ecrãs continuam
mesmo alcançáveis em `/entrar`, `/registar` e `/recuperar`: um ecrã que
ninguém vê nunca é corrigido.

#### O que não existe, e é uma decisão

Não há recuperação por correio, por SMS nem por pergunta secreta. Este
programa não tem servidor para o qual mandar uma resposta, e um ecrã que
pedisse um endereço de correio estaria a prometer o que o sistema não pode
fazer.

O que o ecrã de recuperação oferece é o que existe: tentar de novo, repor
de uma cópia de segurança, ou recomeçar — e o texto diz, **antes** do
botão, que recomeçar cria outra pessoa e que o caminho guardado por
outras deixa de funcionar.

Ajustes ao `backend` estão em [`../docs/conta.md`](../docs/conta.md).

#### Onde está a frase

`lib/onyx/conta-demo.js` guarda a conta de demonstração em `localStorage`
e aceita `demo`. **Não é criptografia** e o ficheiro diz isso: a conta
real é `messenger/storage.py`, com ChaCha20-Poly1305 e PBKDF2 a 200 mil
iterações, e a interface fala com ela pelo sidecar da Etapa 5.

A identidade da demonstração **não** é inventada: vem de `mock-data.js`,
derivada a sério em `messenger/identidade.py`. A interface não tem o sal
nem a fórmula, e é essa a propriedade que a Etapa 3 estabeleceu.

#### A medidor de força

`components/onyx/auth/ForcaFrase.jsx` mostra comprimento e bits estimados,
e diz por baixo que o número mede variedade de caracteres e **não**
resistência a um dicionário. Não há regra de «uma maiúscula, um número, um
símbolo»: `quatro cavalos lentos numa mare` dá 113 bits e `Passw0rd!` é
recusado.

---

## Etapas

| Etapa | Objectivo | Estado |
| --- | --- | --- |
| 1 | Desbloquear a construção, corrigir o âmbito das verificações, alinhar a interface com o prompt | **concluída** |
| 2 | Remover a integração com o serviço hospedado; interruptor de demonstração com aviso visível | **concluída** |
| 3 | Identidade sobre SHA-256, com vectores cruzados | **concluída** |
| 4 | Credenciais locais cifradas e ecrãs de acesso, sem serviço externo | **concluída** |
| 5 | Sidecar Python em `127.0.0.1`: HTTP sobre o socket Unix | **concluída** |
| 6 | Casca de desktop (Tauri) | `PLANEADO` |
| 7 | Cada página alimentada pelo backend, e não pelo adaptador de demonstração | `PLANEADO` |

> **A Etapa 5 ficou com duas decisões que valem a pena saber.**
>
> **Não há WebSocket.** Nenhum consumidor do contrato o pede, e um
> WebSocket meio implementado é pior do que nenhum: `/api/eventos`
> responde `501` com uma mensagem que diz o que não está implementado e
> porquê. Inventar um protocolo de eventos para depois o cliente
> desligar é trabalho que ninguém pediu.
>
> **A loja é do sidecar, não do daemon.** `ESTADO.amigos` do
> `onyxchatd` é um `u16` e não existe comando para listar conversas. O
> que o sidecar guarda em `sqlite3` são as conversas e os contactos da
> **interface**; as chaves de amizade continuam a ser do daemon, e
> escrevê-las na loja seria guardar segredo em dois sítios — que é como
> um deles fica desatualizado.

## Padrões

**Toda a prop opcional de um primitivo `Onyx*` recebe um valor por omissão
explícito**, mesmo que esse valor seja `undefined`. Sem ele o TypeScript
infere a prop como obrigatória e o contrato passa a ser falso — foi o que
produzia 354 erros de verificação. A convenção está justificada em
`components/onyx/OnyxBadge.jsx`.

**Texto em português europeu**, com revisão pelo
`scripts/verificar_portugues.py` da raiz do repositório. A interface
está coberta desde 2026-10-07: o auditor varre também `{js, jsx, mjs,
cjs}`.

As 236 ocorrências medidas em 2026-10-03 eram quase todas falsos
positivos — `export default` e classes do Tailwind como `min-h-screen`.
Delas, **duas eram reais** e estão corrigidas: um «gerador de `senhas`»
num comentário de `pages/Registar.jsx`, e um rótulo em `lib/onyx/nav.js`
que o próprio comentário do ficheiro explicava.

### Porque o filtro é sintaxe, e não uma lista de palavras

O auditor não mantém uma lista de palavras que «saem» da verificação —
mantém padrões que removem **o que é sintaxe**: o atributo `className`
inteiro, e as famílias de utilitários do Tailwind. A lista cresceria com
cada biblioteca que entrasse no projecto; o padrão não.

A primeira versão filtrava por `PascalCase`, que é a convenção de
componentes React. **Apanhava o texto dentro dos nomes**: um componente
chamado `SenhaDoUtilizador` levava consigo `senha` e `utilizador` dentro
do nome, e o verificador deixava de reportar. Um verificador que engole texto não é um
verificador, é um buraco — e `tests/test_verificar_portugues.py` existe
para isso não voltar a acontecer sem que ninguém veja.

### Um falso positivo que se aceita

A forma `arquivo` é reportada, e é português correcto — «`arquivo` notarial»
devia passar. Silenciá-la exigiria filtrar pela forma — plural,
artigo — e quatro padrões foram testados: apanhavam o americanismo («o
`arquivo` foi apagado») ou o português correcto («o `arquivo` de
`arquivo`»), nunca só um.

A escolha é feita a favor de **ver o americanismo**. Um verificador que
aceita a palavra toda deixa de reportar o calão mais comum do português de
informática, e um verificador que não reporta é indistinguível de um que
funciona. O custo é o rótulo «Arquivadas» ser adjectivo, e isso é o
motivo de ele ser adjectivo.

**Nenhum recurso externo.** Nem fontes, nem ícones, nem chamadas de rede
ao arrancar. Verificável no `dist/`: `grep -E 'src="https?://' dist/index.html`
não devolve nada.