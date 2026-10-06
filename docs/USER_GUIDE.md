# Guia do Utilizador — OnyxChat

> **Este é o manual do sistema.** Descreve tudo o que pode fazer, como
> funciona na prática, e o que fazer quando algo falha.
>
> Para *entender* a arquitectura: [`SYS_GUIDE.md`](SYS_GUIDE.md) ·
> Para *desenvolver*: [`DEV_GUIDE.md`](DEV_GUIDE.md)

---

## 1. Antes de começar

### 1.1 O que precisa

| Requisito | Detalhe |
| --- | --- |
| Sistema operativo | Linux (testado em Ubuntu 24.04 / Zorin OS 18) |
| Compilador Rust | `rustc` ≥ 1.98, com `cargo` |
| Compilador C/C++ | `cmake` ≥ 3.16, `g++` (para a camada K4) |
| Python | ≥ 3.10 |
| RAM | **≥ 8 GB para compilar**; ≥ 2 GB para executar |

> **Aviso sobre RAM.** O binário de produção embute Tor (arti), o que
> arrasta ~458 dependências. Compilar isso numa máquina com 4 GB ou
> menos pode esgotar a memória e fazer o sistema recorrer a disco. Se for
> o seu caso, veja [`DEV_GUIDE.md`](DEV_GUIDE.md) §1 antes de tentar.

### 1.2 Compilar

```bash
# Daemon de produção — INCLUI Tor embutido
cargo build --release --features tor-real

# Cliente Python
python3 -m venv .venv
.venv/bin/pip install -e '.[dev]'
```

O executável fica em `target/release/onyxchatd`. A CLI de utilizador
fica disponível como `onyxchat` dentro do virtualenv.

### 1.3 Os dois processos

O OnyxChat corre como **dois processos** que falam por um socket local:

```text
   onyxchat (CLI)  ──socket UDS──►  onyxchatd (daemon Rust)
   Python                        o que tem as chaves, o Tor e o pipeline
```

O daemon tem de estar a correr antes de quase qualquer comando
funcionar. A excepção é `iniciar` e `descobrir`, que não precisam dele.

---

## 2. Onde ficam as coisas

| Caminho | Conteúdo | Permissões |
| --- | --- | --- |
| `~/.config/onyxchat/config.json` | preferências; **e a seed + K5/K9 se não tiver cifrado** | `0600` |
| `~/.config/onyxchat/config.keystore` | seed + K5/K9 **cifradas**, se tiver cifrado | `0600` |
| `/tmp/onyxchat-{uid}.sock` | socket IPC com o daemon | `0600` |
| `~/.local/share/onyxchat/` | pasta de dados | — |

Todos os caminhos podem ser sobrepostos por variáveis de ambiente:

```bash
export ONYXCHAT_CONFIG=/caminho/alternativo/config.json
export ONYXCHAT_SOCKET=/caminho/alternativo/onyxchat.sock
export ONYXCHAT_TOR=nenhum          # ou: arti
```

> **A seed é a sua identidade.** Se a perder, perde a identidade. Se a
> copiar, duplicou-a. Ver §9.
>
> O `0600` protege contra outro utilizador da máquina. **Não** protege
> contra um backup do `~/.`, um `.tar` do seu directório pessoal, ou
> sincronização de cloud — a seed está lá em claro se não tiver cifrado.
> Ver §4.1.1.

---

## 3. Arrancar o daemon

```bash
# Tor real (produção) — precisa de rede
./target/release/onyxchatd --tor arti

# Sem rede (offline, para experimentar)
./target/release/onyxchatd --tor nenhum
```

Opções:

| Opção | Efeito |
| --- | --- |
| `--socket CAMINHO` | caminho do socket (por omissão, o do §2) |
| `--tor arti` | backend Tor real. **É o omissão.** |
| `--tor nenhum` | backend em memória, sem rede |
| `--version` | imprime a versão e sai |
| `--help` | imprime a ajuda e sai |

O daemon corre em primeiro plano até ser interrompido (`Ctrl-C`).

> **Se compilou sem `--features tor-real`**, `--tor arti` falha com uma
> mensagem explícita e o daemon **não arranca**. Não existe queda
> silenciosa para um modo sem Tor — ver §8.4.

---

## 4. Primeiros passos

### 4.1 Criar a identidade

```bash
onyxchat iniciar
```

Imprime a sua chave pública (64 caracteres hex, 32 bytes):

```text
identidade criada: 3f9a…  (64 hex)
```

Isto cria `config.json` com `0600`, contendo a seed Ed25519 e as chaves
K5 e K9 próprias. **A seed nunca é impressa.**

Se já existir uma configuração, o comando **recusa** substituí-la:

```bash
onyxchat iniciar --forcar        # substitui — perde a identidade actual
```

### 4.1.1 Cifrar a identidade (recomendado)

Por omissão a seed fica **em claro** no `config.json`. Com `--cifrar`, ela
e as chaves vão para `config.keystore`, cifradas com
PBKDF2-HMAC-SHA256 + ChaCha20-Poly1305, e o `config.json` fica só com as
preferências.

```bash
# interactivo — pede a passphrase duas vezes
onyxchat iniciar --cifrar

# sem passphrase na linha de comandos nem no histórico do shell
echo 'a minha passphrase' > ~/.pass-onyxchat
chmod 600 ~/.pass-onyxchat
onyxchat iniciar --passphrase-ficheiro ~/.pass-onyxchat
rm ~/.pass-onyxchat

# uso programático (o ambiente é legível por tudo o que executa)
export ONYXCHAT_PASSPHRASE='a minha passphrase'
onyxchat iniciar --cifrar
```

Já tem uma identidade em claro? Migre-a:

```bash
onyxchat cifrar            # irreversível
```

Depois de cifrar, qualquer subcomando que abra a configuração pede a
passphrase:

```bash
onyxchat identidade
passphrase:
```

> **A passphrase não é guardada em lado nenhum.** Perdi-la é perder a
> identidade — não há recuperação. Use uma passphrase que consiga
> lembrar, não a mais forte que consiga inventar.
>
> Porquê `--passphrase-ficheiro` e não `--passphrase`? Um argumento de
> linha de comandos é legível por qualquer utilizador da máquina em
> `/proc/<pid>/cmdline` (modo `0444`) e fica no histórico do shell.

### 4.2 Ver a sua identidade

```bash
onyxchat identidade
```

Imprime `pub: <64 hex>`. Esta é a informação que dá a quem quer falar
 consigo. **Não é secreta.**

### 4.3 Arrancar o daemon noutro terminal

```bash
./target/release/onyxchatd --tor arti
```

### 4.4 Publicar o seu endereço

```bash
onyxchat ouvir
```

Faz o daemon publicar um hidden service e imprime o endereço `.onion`:

```text
abcdefghijklmnop…xyz.onion
```

Este endereço **muda a cada execução** (é efémero). Quem quiser falar
 consigo precisa deste valor *agora*, ou registá-lo no discovery.

### 4.5 Descobrir o endereço de alguém

```bash
onyxchat descobrir 3f9a…        # ID hex da pessoa
```

Imprime o `.onion` registado, ou falha com código 1 e a mensagem
`<id>: desconhecido`.

O servidor de discovery guarda apenas pares `ID → .onion` com TTL
efémero (300 s por omissão). **Não guarda mensagens** e não é um
servidor central de comunicação.

Para correr o seu próprio, o `__main__` do módulo sobe o servidor em
`127.0.0.1:8789` (porta fixa, sem opções de linha de comando):

```bash
python3 -m server.discovery_server
onyxchat descobrir 3f9a… --servidor http://127.0.0.1:8789
```

Para uma configuração diferente (interface, porta, partilha entre
instâncias) chame `server.discovery_server.criar_servidor(host, porta)`
directamente — ver [`DEV_GUIDE.md`](DEV_GUIDE.md) §6.

---

## 5. Estabelecer uma amizade

Este é o passo mais elaborado. Exige três excepções e um transporte, e
**não há ainda um comando que automatize o fluxo completo**. É feito em
quatro passos, com hex a passar entre as partes.

### 5.1 O que cada chave significa

Antes do fluxo, o vocabulário. Cada lado tem:

| Chave | De quem | Para que serve |
| --- | --- | --- |
| `k1` | do par | cifra mútua (camada K1) |
| `k5_proprio` / `k5_par` | de cada um | chave de origem (camada K5) |
| `k9_proprio` / `k9_par` | de cada um | chave de destino (camada K9) |
| `publica` | Ed25519 | identidade, assina tudo |

> Note: `k1` é **simétrica e comum**. Já `k5` e `k9` são distintas por
> lado — K5 protege a origem, K9 protege o destino. Comprometer a K5 de
> alguém não revela a K9. Ver [`SYS_GUIDE.md`](SYS_GUIDE.md) §5.4.

### 5.2 Passo 1 — A pede amizade a B

A executa A, localmente:

```bash
onyxchat pedir-amizade <pub_de_B>            # seed lida do config.json
onyxchat pedir-amizade <pub_de_B> --seed HEX # seed explícita
```

Imprime quatro linhas:

```text
k1=<64 hex>
k5=<64 hex>
k9=<64 hex>
corpo=<418 hex>          # corpo FRIEND_REQUEST, assinado por A
```

A envia `corpo` a B por qualquer meio — e-mail, mensagem, à mão. É um
blob opaco assinado; não contém plaintext.

> **Omitir `--seed` é o caminho seguro.** A seed em `argv` fica
> visível a qualquer utilizador da máquina em `/proc/<pid>/cmdline`
> (modo `0444`) e no histórico do shell. Sem `--seed`, a CLI lê-a de
> `config.json`, que é `0600`. A flag só é necessária quando o daemon
> a correr tem outra identidade — e aí a seed de que precisa também é
> a do utilizador, não a do daemon.
>
> E repare no que é impresso: `k1`, `k5` e `k9` são **chaves de sessão
> completas**, em claro no stdout. São necessárias para o resto do
> handshake, mas quem ler esse terminal fica com elas.

### 5.3 Passo 2 — B aceita

B, localmente, com o `corpo` que recebeu de A:

```bash
onyxchat aceitar-amizade <corpo_de_A>              # seed do config
```

Imprime:

```text
k1=<64 hex>
k5_proprio=<64 hex>
k9_proprio=<64 hex>
k5_par=<64 hex>          # a K5 de A, extraída do pedido
k9_par=<64 hex>          # a K9 de A
publica=<64 hex>         # a pública de A
corpo=<354 hex>          # corpo FRIEND_ACCEPT, assinado por B
```

### 5.4 Passo 3 — B devolve a chave a A

B envia o `corpo` (FRIEND_ACCEPT) a A, pelo mesmo meio.

### 5.5 Passo 4 — A confirma e fecha a amizade

A, localmente:

```bash
onyxchat confirmar-amizade <corpo_de_B>            # seed do config
```

Imprime as seis chaves finais:

```text
k1=<64 hex>
k5_proprio=<64 hex>       # K5 de A
k9_proprio=<64 hex>       # K9 de A
k5_par=<64 hex>           # K5 de B
k9_par=<64 hex>           # K9 de B
publica=<64 hex>          # pública de B
```

**Guarde estas seis chaves.** São de que precisa para cifrar e decifrar
com B.

### 5.6 Recusar

Se B não quiser:

```bash
onyxchat recusar-amizade <nonce_de_16_bytes_em_hex> # seed do config
```

O `nonce` é o nonce do `FRIEND_REQUEST` recebido (16 bytes hex).

### 5.7 O que o handshake garante

```text
autenticidade     ambos os lados assinam (Ed25519)
vinculação        as chaves do par ficam vinculadas às identidades
anti-replay       o nonce do handshake não pode ser repetido
anti-downgrade    a versão do protocolo está dentro da assinatura
```

O que **não** garante: que alguém do outro lado é quem diz ser. Se a
pública de B foi copiada, A está a falar com quem tem essa chave. A
verificação de identidade é humana (por canal fora de banda) — não há
certificados.

Norma completa: [`handshake.md`](handshake.md).

---

## 6. Enviar e receber mensagens

### 6.1 Cifrar (A → envelope)

```bash
onyxchat enviar "olá" \
  --k1 <k1_hex> \
  --k9-par <k9_de_B_em_hex>
```

Imprime o envelope em hex (muitas linhas numa terminal estreita — use
`> ficheiro.txt` ou `| xxd -r -p`).

Cada mensagem usa **nonces aleatórios**, portanto cifrar a mesma frase
duas vezes dá envelopes **diferentes**. Isso é o comportamento correcto.

### 6.2 Decifrar

```bash
onyxchat receber <envelope_em_hex> \
  --k1 <k1_hex> \
  --k5-par <k5_de_A_em_hex> \
  --pub-par <pub_de_A_em_hex>
```

Imprime o texto em claro.

> **A assinatura é verificada localmente, antes de pedir a decifragem ao
> daemon.** Um envelope com assinatura inválida é rejeitado sem que
> qualquer cifra seja rodada. Se vir `onyxchat: assinatura inválida`, o
> envelope **não veio de quem diz vir** — ou foi alterado no caminho.

### 6.3 Limites

```text
máximo de texto em claro .... 65 536 bytes (64 KiB)
envelope mínimo ............. 117 bytes
envelope máximo ............. 65 692 bytes
```

Acima do máximo o daemon **rejeita antes de processar**. Não há
truncamento silencioso.

---

## 7. Comunicação P2P

### 7.1 Ligar

```bash
# Modo directo: o Tor gere o hidden service de cada extremidade
onyxchat ligar <onion_ou_id_hex> <pub_propria> <pub_do_par> \
  --modo direto

# Modo relay: via um servidor de transporte cego
onyxchat ligar <id_hex_do_par> <pub_propria> <pub_do_par> \
  --modo relay --endpoint 127.0.0.1:8788
```

Os argumentos posicionais são, por ordem: `destino`, `pub_propria`,
`pub_par`.

| `--modo` | O que exige | O que vê o outro lado |
| --- | --- | --- |
| `direto` | um `.onion` válido; Tor a funcionar | o seu hidden service |
| `relay` | `--endpoint` do relay; ID hex do par | a sua mailbox derivada da pub |

Sem `--endpoint` em modo relay, o daemon usa o endpoint configurado por
omissão (se existir), ou `$ONYXCHAT_RELAY`.

> **O modo relay é uma ligação directa, e o daemon avisa.**
> Ligar a um relay alcançável é um TCP sem Tor: o relay — e quem estiver
> no caminho — vê o seu IP real. Por isso o daemon valida o `--endpoint`
> antes de tentar ligar e escreve no `stderr`, **antes** da ligação:
>
> ```text
> onyxchatd: ligação em CLEARNET ao relay 203.0.113.7:8788 — o IP real é observável por este relay (privacy_model.md §3)
> ```
>
> Se não vir esta linha, a ligação não saiu em clearnet. O aviso nunca
> contém chaves, e a sua escrita não pode derrubar o daemon. Para não
> expor o IP, use `--modo direto` por `.onion`.

> **Só se pode ter uma ligação activa de cada vez.** Uma segunda tentativa
> falha com `0x0F`. Feche a anterior primeiro.

### 7.2 Enviar pela ligação

```bash
onyxchat enviar-p2p <envelope_em_hex>
```

Imprime `enviado (N bytes)`.

### 7.3 Receber pela ligação

```bash
onyxchat receber-p2p --timeout-ms 5000
```

Imprime `<tipo:2 hex> <corpo_em_hex>`:

```text
01 3f9a…          # frame 0x01 = CHAT, corpo é o envelope
```

O `--timeout-ms 0` espera indefinidamente. Sem a opção, 5000 ms.

### 7.4 Fechar

```bash
onyxchat fechar
```

Idempotente: fechá-lo duas vezes não dá erro.

### 7.5 Ver o estado

```bash
onyxchat estado
```

```text
tor: ativo | ligado: sim | amigos: 1 | onion: abcdef…onion
```

---

## 8. Diagnóstico

### 8.1 O daemon não está a correr

```text
onyxchat: <erro de socket>
```

Solução: arrancar o daemon (§3). Confirme o caminho com
`echo $ONYXCHAT_SOCKET`.

### 8.2 Tabela de erros do IPC

O daemon responde com um código de estado. Estes são os que pode ver:

| Código | Nome | O que significa | O que fazer |
| --- | --- | --- | --- |
| `0x01` | `ComandoDesconhecido` | comando inexistente | versão de cliente ≠ daemon |
| `0x02` | `PayloadMalformado` | corpo truncado ou malformado | normalmente hex incompleto colado |
| `0x04` | `AssinaturaInvalida` | assinatura não confere | **envelope alterado ou de outra pessoa** |
| `0x05` | `DecifragemFalhou` | tag AEAD não bateu | chave errada, ou ciphertext alterado |
| `0x06` | `EnvelopeInvalido` | formato/length errado | envelope incompleto |
| `0x08` | `TextoInvalidoUtf8` | plaintext não é UTF-8 válido | conteúdo corrompido |
| `0x09` | `PayloadGrandeDemais` | acima do limite | mensagem > 64 KiB |
| `0x0A` | `NaoLigado` | não há ligação P2P | `ligar` primeiro |
| `0x0B` | `TorIndisponivel` | Tor não arrancou | ver §3, rede, permissões |
| `0x0C` | `HandshakeInvalido` | corpo de handshake inválido | tamanho ou assinatura errados |
| `0x0D` | `DestinoInvalido` | `.onion` ou ID inválido | o par não está publicado |
| `0x0F` | `EstadoInvalido` | operação impossível no estado actual | há já uma ligação aberta |
| `0x10` | `RateLimit` | flood detetado | mais de 128 frames em 10 s |
| `0x11` | `Relay` | o relay recusou | ver erros de relay abaixo |
| `0x12` | `VersaoIncompativel` | `HELLO` com versão diferente | cliente e daemon dessincronizados |
| `0x13` | `NonceRepetido` | mensagem já vista | **replay** — ou reenvio seu |

### 8.3 Erros do relay

| Código | Significado |
| --- | --- |
| `0x81` `ERRO` | erro genérico do relay (código + mensagem) |
| `0x83` `OK` | operação aceite |
| `0x84` `CAIXA` | chegou um envelope pendente |

Códigos dentro de `0x81`: `0x02` mailbox inválida, `0x03` payload grande
demais, `0x04` caixa cheia (64 pendentes), `0x05` rate limit (32
`ENVIAR`/s), `0x06` comando desconhecido.

### 8.4 Se o daemon não arranca

```text
onyxchatd: backend arti indisponível: binário compilado sem a feature
`tor-real` (use `--tor nenhum` ou reconstrua com features por omissão)
```

Compilou sem Tor. Duas saídas:

```bash
onyxchatd --tor nenhum        # modo offline, sem Tor
cargo build --release --features tor-real    # reconstruir com Tor
```

**Isto é propositado.** Um binário sem Tor **recusa** arrancar em modo
Tor, para nunca parecer que tem protecção que não tem. Ver
[`DEV_GUIDE.md`](DEV_GUIDE.md) §2.3.

### 8.5 Se o `pedir-amizade` dá erro de assinatura

Quase sempre é o seed em hexadecimal mal colado. O seed tem de ter
**exatamente 64 caracteres hex** (32 bytes). Um carácter a mais ou a
menos produz `0x0C`.

---

## 9. Segurança prática: o que deve saber

### 9.1 Protegido

```text
conteúdo           cifrado fim-a-fim; o transporte não tem as chaves
identidade         assinatura Ed25519 liga a mensagem a uma identidade
replay             nonces de handshake e de chat rejeitam reenvio
downgrade          a versão está dentro da região assinada
```

### 9.2 NÃO protegido — leia antes de confiar em alguém

```text
metadados          tamanho, frequência e timing são observáveis
IP real            não é exposto no modo directo; é visível num relay
                    em clearnet
identidade Onyx    estável e CORRELACIONÁVEL entre sessões
confidencialidade  não há ocultação de conteúdo no pipeline de K1..K9
                    contra quem tenha a seed — as chaves são o sistema
seed em disco      o `0600` não protege contra um backup, um `.tar` do
                    directório pessoal, ou sincronização de cloud.
                    `onyxchat cifrar` resolve (§4.1.1)
linha de comandos  `--k1`, `--k5-par` e `--k9-par` ficam visíveis em
                    `/proc/<pid>/cmdline` (modo 0444) e no histórico.
                    `--seed` deixou de ser obrigatório (ver §5.2)
```

As duas últimas linhas são limitações reais do **cliente**, não do
protocolo: o daemon nunca põe segredos em `argv`, e a seed no
`config.json` é um problema de armazenamento que a cifragem resolve.
Para um par, prefira os comandos que **não** levam a seed:

```bash
onyxchat pedir-amizade <pub_de_B>      # lê a seed do config
```

As chaves de sessão (`--k1`, `--k5-par`, `--k9-par`) continuam
obrigatórias em `argv`, e o comando imprime `k1`/`k5`/`k9` no stdout.
São chaves de **uma sessão**, não a identidade — mas são secretas, e
ambos os sitios as expõem ao mesmo tipo de observador.

Ver §9.3 para o resto.

### 9.3 Chaves: como não as perder

```text
seed Ed25519  → é a identidade. Guarde-o como guarda uma chave privada.
               sem backup = identidade perdida para sempre
config.json   → 0600. Se NÃO tiver cifrado, contém a seed em claro
config.keystore → 0600, cifrado com PBKDF2 + ChaCha20-Poly1305.
                  só existe depois de `onyxchat cifrar` (ou `--cifrar`)
```

O `config.keystore` e o `config.json` são um par: o segundo aponta
para o primeiro, e o primeiro é inútil sem a passphrase. Guarde-os
juntos, e guarde a passphrase **fora** dessedirectório.

Uma cópia de segurança tem de ser dos dois ficheiros, e só é
legível com a passphrase:

```bash
tar czf onyxchat-backup.tgz -C ~/.config/onyxchat config.json config.keystore
```

Nenhuma chave sai do seu host por outro caminho que não seja o
handshake assinado. Ver [`key_management.md`](key_management.md).

---

## 10. Perguntas frequentes

**P: A minha identidade expõe o meu nome?**
R: Não directamente. Mas a identidade Onyx é **estável**: se alguém
guardar a sua pública e a associar a si, todas as sessões futuras são
associáveis. É o custo de uma identidade verificável. Para
desassociação, use uma identidade nova por interlocutor — o que
implica gerir mais identidades.

**P: Posso falar com alguém sem Tor?**
R: Pode, em `--tor nenhum` — mas é um loopback em memória, útil para
experimentar, não para conversar. E não há garantia de anonimato sem Tor.

**P: Posso enviar uma mensagem vazia?**
R: Não, e é deliberado. O pipeline recusa texto vazio à entrada e
devolve o erro `0x02` («pedido malformado»), não `0x09` («excede o
limite») — para que saiba que a correcção é escrever alguma coisa.

A razão: cifrar texto vazio produziria um envelope com um bloco de
padding e nada mais, que o receptor leria como mensagem vazia. Um
envelope que não transporta nada e não se distingue de um ataque de
preenchimento não é uma mensagem. Ver [`pipeline.md`](pipeline.md)
§Limites de tamanho.

**P: O relay guarda as minhas mensagens?**
R: Não tem as chaves, portanto não consegue lê-las. Guarda cada
envelope até 300 s numa caixa derivada de `SHA-256("ONYX/RELAY/v1" ‖
pub)`, no máximo 64 por caixa. Passado o TTL, deixa de existir.

**P: O discovery sabe com quem falo?**
R: Sabe **quem consulta que ID** e **quem se regista com que `.onion`**.
Não vê mensagens, chaves, nem quem fala com quem — uma consulta
indica intenção de contacto, não comunicação. Ver
[`discovery.md`](discovery.md).

**P: Perdi o `config.json`. Posso recuperá-lo?**
R: Não. A identidade deriva da seed. Sem o ficheiro, sem identidade.
Faça backup cifrado se a identidade lhe importa.

**P: Como é que o sistema escala?**
R: Cada instância é cliente e servidor, portanto **não há servidor de
mensagens para escalar**. O que escala mal é o discovery e o relay, que
são centralizados por desenho pragmático. Routing descentralizado é
`PLANEADO` — ver [`roadmap.md`](roadmap.md).

---

## 11. Referência rápida

```bash
onyxchat iniciar                      # criar identidade
onyxchat identidade                   # ver a minha pública
onyxchat ouvir                        # publicar o meu .onion
onyxchat descobrir ID                 # resolver ID → .onion

onyxchat pedir-amizade PUB              # passo 1 do handshake
onyxchat aceitar-amizade CORPO          # passo 2 (B)
onyxchat confirmar-amizade CORPO        # passo 4 (A)
onyxchat recusar-amizade NONCE           # recusar

onyxchat enviar TEXTO --k1 K --k9-par K9        # cifrar
onyxchat receber ENVELOPE --k1 K --k5-par K5 --pub-par PUB  # decifrar

onyxchat estado                      # ver o estado
onyxchat ligar DESTINO PUBPROPRIA PUBPAR --modo directo
onyxchat enviar-p2p ENVELOPE
onyxchat receber-p2p --timeout-ms 5000
onyxchat fechar

onyxchat relay --host 0.0.0.0 --porta 8788        # correr um relay
python3 -m server.discovery_server                 # correr discovery
```

