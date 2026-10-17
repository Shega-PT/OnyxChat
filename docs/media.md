# OnyxChat Media

## Introdução

Este documento é a **especificação normativa** do módulo `onyxmedia`: a
transformação de áudio e vídeo em **texto**, e de volta, sem que a rede,
o protocolo ou o formato do envelope saibam que existe multimedia.

Define, para cada elemento, a entrada, a operação, a saída, os limites e
os erros — de forma que qualquer implementação (Rust, Python) produza
exactamente os mesmos bytes.

### O invariante, e porque é o invariante

> **O que sai para a rede é um `FRAME_CHAT` (`0x01`) com um envelope
> normal. A rede não sabe se é texto, áudio ou vídeo.**

Isto não é uma consequência de implementação — é a **restrição de
desenho** que o resto do documento existe para satisfazer. Dela decorre
tudo:

| O que NÃO muda | Onde |
| --- | --- |
| o framing `[u32 LE][tipo:1B][corpo]` | `docs/p2p.md` §Moldura |
| o tipo `FRAME_CHAT = 0x01` | `network/daemon_rust/src/p2p.rs` |
| o relay: mailbox, retenção, TTL | `docs/relay.md` |
| o `OnyxMiniNode` e o seu firmware | `OnyxMiniNode/docs/protocolo.md` |
| `MAX_PLAINTEXT = 65 536` | `docs/pipeline.md` §Limites |
| `MINIMO_CORPO` e a sua derivação | `network/daemon_rust/src/orcamento.rs` |
| o envelope, a assinatura, K1→K9 | `docs/message_format.md`, `docs/pipeline.md` |

O que muda é uma coisa só: **qual `Chaves` entra no `ENCODE`**.

### O que um observador externo vê

Um envelope de `101 + N` bytes, com `N` opaco. Um observador vê hoje
`N` variável conforme o tamanho da mensagem de texto; com media passa a
ver `N` variável conforme o tamanho do *delta* de um frame. **A forma é
a mesma em ambos os casos** — e esta indistinguibilidade é uma propriedade
verificável, não uma intenção: ver §8.

### Os dois modos, e porque existem dois

| | Não-streaming | Streaming |
| --- | --- | --- |
| Que é | um ficheiro enviado de uma vez | uma sessão ao vivo |
| Unidade | o ficheiro inteiro | um frame de origem |
| Anel e relógio | **nenhum** | obrigatórios |
| Reproduzível depois | **sim**, quantas vezes se quiser | não — é um instante |
| Retomável | **sim** | não |
| Armazenável | **sim** | não |
| Custo | ~11× menos mensagens | — |

**A distinção é aritmética, não de gosto.** Os mesmos 10 segundos de
conteúdo a 30 fps dão 300 mensagens em streaming e 26 em não-streaming,
porque o streaming paga 161 bytes de overhead **por frame** e o
não-streaming paga-os **por cada 48 KB**. O streaming existe para a
chamada; tudo o resto vai pela outra via.

---

## 1. Derivação de chave por tipo

### 1.1 A regra

O receptor tem de saber o tipo **antes** de ter qualquer plaintext, e
esse conhecimento não pode ter origem na rede. Resolve-se tornando o
**tipo parte da chave**: a K1 é derivada por tipo, e a verificação da
etiqueta AEAD da K1 **é** o detector.

```text
semente = SHA-256(b"ONYX/AMIZADE/v1" ‖ menor(k5ₐ, k5_b) ‖ maior(k5ₐ, k5_b))

k1(TIPO) = HKDF-SHA256(
    semente,
    salt   = b"ONYX/MEDIA/" ‖ TIPO,
    info   = b"ONYX/K1/v1" ‖ pub_emissor ‖ b"ONYX/MEDIA/" ‖ TIPO,
    tamanho= 32)
```

### 1.1.1 Porque é que o `salt` não é o `nonce1`

Uma primeira versão punha `salt = nonce1`, herdado de `derivar_k1`. E
isso **não funciona**, por uma razão que só aparece em execução.

O emissor teria de cifrar **duas vezes** — uma para obter um envelope
com um `nonce1` conhecido, e outra com a chave derivada desse nonce.
Mas a segunda `ENCODE` gera um `nonce1` **novo**, e o envelope que vai
para a rede tem esse nonce novo, não o que a chave usou. O receptor
deriva a chave a partir do envelope que **recebeu** — logo, deriva com
um valor que não é o da cifra — e a mensagem **não abre**.

E a falha é do tipo mais caro que existe: **cifra bem e não abre**.
`0x05 DecifragemFalhou` não aponta para a chave, que é a pista mais
errada que se pode dar a quem depura.

Com um `salt` constante, os dois lados derivam a mesma chave a partir
do que **já sabem ambos** — o par de chaves e a identidade do
remetente — e **uma** passagem de `ENCODE` resolve. O `nonce1`
continua a ser o nonce do AEAD, que é onde tem de estar: é o que
garante que duas mensagens com a mesma chave usam nonces diferentes.

Há uma consequência de segurança, e ela é real: a K1 de media é de
longa duração dentro do par, tal como a de texto. Um par cujas chaves
tenham sido comprometidas abre media antiga. É a mesma propriedade que
o texto já tem, e está declarada em §1.4 em vez de omitida.

`TIPO` é **um byte**, e só a media o usa:

| Valor | Significado |
| --- | --- |
| `0x01` | áudio |
| `0x02` | vídeo |

**O texto não entra nesta fórmula.** Continua a usar a K1 de longa
duração que já usava, e essa é a razão de o texto continuar
**bit-a-bit igual** ao que já produzia. A alternativa — derivar também
o texto, para a função ser total — trocaria uma garantia que já temos
(zero risco de regressão no chat) por uniformidade que ninguém pediu.

Os dois espaços de chaves ficam **disjuntos**, e essa é a consequência
que interessa: uma chave de media comprometida não abre texto, e uma
chave de texto comprometida não abre media.

> **Só a K1 é derivada.** K5 e K9 entram no `ENCODE` inalteradas. A K1
> é a **primeira** camada e é AEAD: uma etiqueta errada morre ali, sem
> tocar em mais nada — logo o teste custa uma verificação Poly1305, não
> uma descifra completa.

### 1.2 A semente

`docs/key_management.md` §Chaves de comunicação já define a semente e a
derivação da K1, com vectors congelados nos dois lados
(`crypto/rust/src/amizade.rs` e `messenger/amizade.py`). O módulo de
media **não reimplementa nada disso**: acrescenta um rótulo de domínio
ao `salt` e ao `info`, e reutiliza a mesma semente.

Isto importa mais do que parece. A alternativa — um segredo próprio para
media — exigiria trocar as chaves pelo handshake, o que
`key_management.md` §Rotação diz que é uma operação com consequência
(um novo `FRIEND_REQUEST` da mesma `pub` **substitui** as chaves
antigas). O áudio e o vídeo não justificam expulsar quem já tem
conversas.

### 1.3 O teste

```text
tentativa 1: DECODE com a K1 de longa duração   → OK ⇒ texto
tentativa 2: DECODE com k1(0x01)                → OK ⇒ áudio
tentativa 3: DECODE com k1(0x02)                → OK ⇒ vídeo

nenhuma verifica → a mensagem é inválida (0x05 DecifragemFalhou)
```

O texto primeiro, porque é o caso comum: uma conversa normal custa
**uma** tentativa. O custo dos três é **três HKDF e três Poly1305** —
abaixo do limiar de medição, e abaixo do custo de um único `ENCODE` de
65 KB.

> **Um `DECODE` falhado não é um erro para o cliente.** As três
> tentativas são o mecanismo, não uma excepção: só depois de as três
> falharem é que a mensagem é declarada inválida. Um `DECODE` que devolve
> `0x05` ao primeiro teste seria indistinguishable de um envelope
> adulterado — e obrigaria a distinguir os dois por mensagem.

### 1.4 Propriedades

* **Privacidade de tipo.** O `salt` e o `info` do HKDF entram na chave,
  não no envelope. O tipo não viaja. Um observador vê `101+N` bytes
  opacos.
* **Separação de chaves.** Uma chave comprometida não abre as outras.
  É a mesma doutrina de `key_management.md` §Geração: «as chaves não são
  derivadas umas das outras — são independentes por desenho, para que uma
  compromising não leve as outras».
* **Direccionalidade preservada.** `pub_emissor` continua no `info`, pelo
  que a chave continua a ser direccional (`key_management.md` §1).
* **Falha fechada.** Um envelope cujo tipo não é nenhum dos três não é
  decifrado. Não há ramo que «tente como texto» em último recurso.
* **Longa duração declarada.** `k1(TIPO)` é de longa duração dentro do
  par, como a K1 de texto — e por §1.1.1 o `salt` **tem** de ser
  constante para que o emissor consiga cifra[r] numa passagem só. Um
  par comprometido abre media antiga. Isto é o mesmo compromisso que o
  texto já faz, com a mesma mitigação: rotacionar o par pelo handshake.
  É a limitação que impede o `salt` de ser o `nonce1`.

---

## 2. A moldura

### 2.1 Onde vive

O plaintext que o `ENCODE` recebe é `str` UTF-8 válido (exigido por
`network/daemon_rust/src/ipc.rs`). Media é, portanto, **texto**, e só
entra no pipeline depois de construída.

```text
bytes da media  →  moldura  →  base64  →  plaintext UTF-8  →  ENCODE
```

A ordem é fixa e cada passo tem uma razão:

* **delta e RLE antes do base-64.** O base-64 não tem alinhamento a
  bytes: duas linhas de dados iguais produzem caracteres diferentes. Um
  RLE sobre texto base-64 não encontraria repetições que existam. Ver
  §6.3.
* **base-64 depois da moldura.** A moldura precisa de ser legível sem
  desdobrar o payload.

### 2.2 Preâmbulo comum — 36 bytes

Texto ASCII puro, dentro do plaintext **já cifrado** — a rede não o vê.

| Offset | Tam. | Campo | Notas |
| --- | --- | --- | --- |
| 0 | 4 | `ONX1` | magia ASCII |
| 4 | 1 | `tipo` | `A` áudio · `V` vídeo |
| 5 | 1 | `versão` | `1` |
| 6 | 1 | `modo` | `F` não-streaming · `S` streaming |
| 7 | 1 | `flags` | `0` a `7` — três bits em base 2: manifesto · keyframe · fim-de-lote |
| 8 | 5 | `stream` | decimal, zeros à esquerda |
| 13 | 10 | `seq` | decimal, zeros à esquerda; **monotónico por stream** |
| 23 | 12 | `clock_ms` | `+` ou `-`, seguido de 11 algarismos |
| 35 | 1 | separador | `\n` |
| 36 | … | corpo | base-64 (§2.1) |

**Todos os campos são ASCII, e nenhum é binário.** Isto não é uma
preferência: o plaintext tem de ser UTF-8 **válido**
(`network/daemon_rust/src/ipc.rs`), e `FF` — que é o primeiro byte de
`-250` em complemento de dois — não é. Uma versão anterior desta
especificação punha campos binários e produzia plaintexts que o `ENCODE`
recusava com `0x08 TextoInvalidoUtf8`, o que aparecia como «a media não
envia» e não como «o formato está errado».

Os números são **algarismos, não bytes**. A alternativa era continuar
binário e pôr todo o preâmbulo em base-64: seriam 21 bytes em vez de 36,
e perder-se-ia a única coisa que um preâmbulo vale — ser legível. Um
`hexdump -C` mostra `ONX1V1S2046600000000007-00000000250`, e lê-se o
tipo, a versão, o modo, as flags, a stream, a sequência e o relógio sem
descodificador nenhum.

**A largura fixa** é o que dispensa um parser de texto: cada campo
começa num offset conhecido e acaba em `início + largura`. Um
`strtod` por frame, a 30 fps, seria o custo do módulo inteiro.

`stream` e `seq` são decimais e não hexadecimais porque um campo que se
lê de trás para a frente em base 16 não é o mesmo campo em base 10, e
esta spec é lida por pessoas.

`clock_ms` é `0` no modo não-streaming: não há relógio porque não há
sincronismo (§4).

### 2.3 `seq` é obrigatório, e não é opcional por conveniência

Áudio e vídeo são **mensagens `CHAT` separadas**. O relay é store-and-forward
com entrega única (`docs/relay.md` §Regras de retenção), e um caminho
por relay **pode reordenar**. Sem `seq` o receptor não distingue
«faltou o 7» de «o 7 chegou tarde» e aplica um delta sobre a referência
errada — silenciosamente.

`seq` é monotónio **por `stream`**, e só por `stream`: é o que permite
detectar a perda, a reordenação e o salto de geração, com um inteiro.

### 2.4 Corpo

* **Modo não-streaming**: `base64` de uma fatia (§4.2).
* **Modo streaming**: `base64` de um lote de telhas (§6.2).
* **Manifesto**: ver §2.5 — substitui o corpo.

### 2.5 Manifesto

O manifesto substitui o corpo e traz os parâmetros que **não mudam**
durante a vida do stream. Enviá-los uma vez em vez de os repetir em cada
unidade é o que mantém o preâmbulo em 18 bytes.

**Não-streaming** (`flags` bit 0), 17 bytes + 32 de resumo + nome:

| Tam. | Campo |
| --- | --- |
| 1 | `tipo` — `A` áudio · `V` vídeo (§2.2) |
| 4 | `total_bytes` (`u32` big-endian) |
| 4 | `total_pedacos` (`u32` big-endian) |
| 2 | `bytes_por_pedaco` (`u16`) |
| 2 | `codec` (`u16` — ver §3.3) |
| 2 | `formato` (`u16` — ver §3.3) |
| 4 | `duracao_s` (`u32`) |
| 32 | `sha256` do conteúdo montado |
| var. | `nome` em base-64, sem bytes nulos |

**O `tipo` é um campo do manifesto e não se deduz do `codec`.** É o
erro que a implementação apanhou: sem ele, o receptor lia o `codec` e
derivava o tipo de lá — e um vídeo Matroska com uma faixa de áudio Opus
dentro é `Codec::Opus`, que daria `Tipo::Audio`, e a chave seria a de
áudio para um conteúdo que é vídeo. O `tipo` é o que **escolhe a chave**
(§1), e um campo que escolhe uma chave não pode ser deduzido de um
campo que descreve bytes.

Isto é a mesma doutrina de `docs/message_format.md` §Regra 4: a versão
é o que identifica o perfil, e não se deduz do conteúdo.

**Streaming** (`flags` bit 0):

| Tam. | Campo |
| --- | --- |
| 2 | `largura` (`u16`) |
| 2 | `altura` (`u16`) |
| 2 | `fps` (`u16`) |
| 2 | `bytes_por_ms` (`u16`) — ver §5.4 |
| 2 | `codec` (`u16`) |
| 4 | `keyframe_ms` (`u32`) |

`largura` e `altura` são **múltiplos de 16** (§6.1). Uma grelha de
telhas que muda a meio da stream invalida a referência sem dar erro
— produz lixo. Daí o manifesto ser a única forma de a declarar.

E **360 não é múltiplo de 16** (dá 22,5), o que apanha toda a gente: é
o número que se escolhe para um 16:9. Um ecrã de 640×360 é rejeitado,
e a rejeição é o comportamento correcto — a última fila de telhas
ficaria a meio e o receptor reconstruiria um quadro de largura
diferente da que foi enviado. As geometrias que o sistema aceita
começam em 320×240, 640×352 e 1280×720.

---

## 3. Limites

### 3.1 O número que governa tudo

```text
MAX_PLAINTEXT            = 65 536   (docs/pipeline.md §Limites)
base64: 4 bytes por cada 3 de entrada

mínimo por unidade       = 3 × ⌊65 536 / 4⌋ = 49 152 bytes
```

**49 152 bytes é o tamanho máximo de media por envelope.** Todo o resto
da aritmética decorre daqui.

### 3.2 O que resta depois da moldura

| | Bytes |
| --- | --- |
| preâmbulo, separador incluído | 36 |
| base-64 de 49 152 bytes | 65 536 |

Excede `MAX_PLAINTEXT`. O máximo real é, portanto, o maior múltiplo de 3
cujo base-64, com 36 bytes de moldura, caiba em 65 536:

```text
(65 536 − 36) / 4 × 3 = 49 125 bytes
```

**49 125** é o número, e é **derivado** — `network/daemon_rust/src/media/
moldura.rs` calcula-o de `MAX_PLAINTEXT` e do comprimento do preâmbulo, e
os dois são constantes de tamanho fixo. Um literal aqui
continuaria o valor antigo se `MAX_PLAINTEXT` mudasse, e a falha
apareceria como `0x09 PayloadGrandeDemais` a meio de uma transferência.

Pelo mesmo motivo, **todos os offsets do preâmbulo são derivados uns dos
outros**, nunca escritos à mão. A primeira implementação escrevia-os em
dois sítios — a construção e a leitura — e divergiram em um byte no
primeiro build. Derivar não é estilo: é a diferença entre um formato que
não pode divergir e um que diverge no primeiro dia em que alguém
acrescenta um campo.

### 3.3 Códigos

| Valor | `codec` | Conteúdo | Formatos (`formato`) |
| --- | --- | --- | --- |
| `0x0001` | Opus | áudio comprimido | `0x0001` Opus · `0x0002` MP3 · `0x0003` WAV |
| `0x0002` | rawvideo gray | frames de 8 bits | `0x0010` rawvideo |
| `0x0004` | VP8 | compressão de movimento | `0x0030` IVF |
| `0x0003` | Matroska | invólido para streaming | `0x0020` MP4 · `0x0021` Matroska · `0x0022` WebM |

A separação entre `codec` e `formato` é deliberada: **o `codec` diz o que
decodificar**, o `formato` diz **o que o contentor é**. Um vídeo
não-streaming é Matroska com Opus dentro — `codec = 0x0001`,
`formato = 0x0021`. O receptor precisa das duas coisas e são coisas
diferentes.

**VP8** (`0x0004`) é o `codec` de vídeo do streaming (§8.1). E `rawvideo
gray` (`0x0002`) deixou de ser o único: era o único porque o VP8 não
estava na receita, e a medição de F9 mostrou que o `rawvideo` não cabe
(§3.4.2). Fica válido para um receptor que o encontre, porque um
manifesto `0x0002` já pode estar em circulation — mas **nenhum emissor
novo o produz**.

### 3.4 A taxa — medida, e com codec

`network/daemon_rust/src/p2p.rs` impõe `LIMITE_FRAMES = 128` por
`JANELA_RATE = 10 s` — **12,8 mensagens por segundo**, e vale para todo o
`FRAME_CHAT`.

Isto **não** impede os 30 fps. Um lote de streaming (§8.3) transporta
vários frames de origem numa só mensagem.

> **Esta secção foi reescrita duas vezes.** A primeira versão tinha uma
> tabela de taxas escrita **antes** do encoder existir, e ela estava
> errada por um factor de ~11. A segunda versão media o `rawvideo` e as
> telhas 16×16, e provou que esse caminho não fecha (§3.6). A terceira
> mede o codec que substitui os dois.

#### 3.4.1 O orçamento, e de onde vem

```text
12,8 msg/s × 49 125 bytes por envelope = 629 kbit/s no total
```

E o total é partilhado entre **áudio e vídeo** — que é o número que
manda, e que uma tabela só de vídeo esconde.

#### 3.4.2 O que cabe, medido com o codec real

`tools/ffmpeg/build.sh` compila o `ffmpeg` com `libvpx` (§7.1), e é
com ele que a medição foi feita:

| bitrate VP8 pedido | medido | + áudio Opus 32k | do orçamento |
| --- | --- | --- | --- |
| 300 kbit/s | 39 kbit/s | 71 kbit/s | **8,9×** |
| 500 kbit/s | 67 kbit/s | 99 kbit/s | **6,3×** |

**854×480 a 30 fps cabe com 6,3× de folga**, mesmo com ruído de sensor
forte (±25 níveis por pixel, que é uma câmara barata).

E o `-b:v` é um **tecto**, não um alvo: o VP8 escreve menos do que
pede quando o conteúdo o permite, e o receptor dimensiona o anel pelo
valor pedido — que é o mais alto que pode chegar.

A taxa de mensagens que 99 kbit/s exige é de **0,3 msg/s**. A excepção
de §3.4.3 não é o que torna a chamada possível; é o que a deixa
crescer sem esbarrar no tecto de texto.

#### 3.4.3 A excepção do streaming

O limite de 12,8 msg/s é de **texto**: 128 mensagens de conversa por
10 s. Uma chamada mede-se noutro sítio, e 0,9 msg/s de media convive
com 12,8 de texto sem disputa.

`p2p.rs` tem **duas janelas**, e não uma com dois tetos:

```text
JanelaRate
  ├─ contador      LIMITE_FRAMES        = 128   texto
  ├─ inicio        JANELA_RATE          = 10 s
  ├─ contador_media LIMITE_FRAMES_MEDIA =  64   media
  └─ inicio_media  JANELA_RATE          = 10 s
```

Um contador com dois tetos é um contador em que o maior tecto vence
sempre, porque a soma é um número: quem envia 512 mensagens de texto
passaria, e quem só fala passa a ter folga que nunca pediu. Duas janelas
obrigam a declarar **de que** são antes de gastar.

E o que isto **não** abre: nada. O `FRAME_CHAT` continua a ser um
`FRAME_CHAT`, o envelope continua o mesmo, e a cifra continua a mesma.
A excepção é um contador — não é um caminho novo para a rede.

#### 3.4.4 A origem, e porque é declarada e não deduzida

`FRAME_CHAT` é o mesmo tipo para texto e para media, e a distinção só
aparece no plaintext **depois de decifrado**. E o `p2p.rs` não
decifra nada, por desenho.

Logo a origem viaja no comando:

```text
ENVIAR 0x06
  origem: 1 B   0x00 texto, 0x01 media
  corpo:  n B   o envelope, como antes
```

Declarar e não deduzir é a propriedade: deduzir exigiria decifrar no
caminho de escrita, e essa é exactamente a que o projecto não quer
perder.

### 3.5 O tecto que não é nosso

O atraso de apresentação do streaming tem um piso que nenhuma linha
deste documento reduz: **o RTT do Tor**, tipicamente 100–500 ms. A 30 fps
isso são 3 a 15 frames de atraso. Um circuito Tor que se reconstrói
(1–3 s) produz um congelamento, mitigado por keyframe em até 2 s (§8.5)
e nunca eliminado.

Dizer isto aqui, e não na conclusão, é o que faz a diferença entre um
tecto e uma promessa.

### 3.6 O achado que a medição de F9 mudou

> **RESOLVIDO por fração.** Isto não é um afinar de números: são duas
> decisões já tomadas na spec que eram **incompatíveis entre si**, e o
> código escrito até F8 seguia as duas.

Uma **keyframe** é o delta contra um quadro zero, e por isso leva todas
as telhas com variação. O custo sai de três constantes que já estavam
decididas:

```text
MAX_PLAINTEXT        65 536 bytes   §3.1
base-64              × 1,3333       §2
moldura              + 36 bytes      §2
telha com variação   2 + 1 + 256    §6.2
```

```text
(65 536 − 36) / 1,3333 / 259 = 189 telhas
```

| geometria | telhas | keyframe em texto | |
| --- | --- | --- | --- |
| 640×480 (§3.1) | 1 200 | 414 456 | **6,3× acima** |
| 256×192 | 192 | 66 340 | não cabe, por 1,2× |
| 256×176 | 176 | 60 936 | cabe |

**Não existia largura de ecrã útil em que a keyframe coubesse.** A
640 de largura cabiam 4 fileiras de telhas. E sem keyframe não há
recuperação (§8.5), e sem recuperação o streaming é uma imagem que se
parte na primeira perda e não volta.

#### 3.6.1 A resposta: keyframe fracionada, como a §4

A resposta **não** é baixar a resolução. Uma keyframe que não cabe não
é uma keyframe que se mande pequena — é uma keyframe que não existe.

É a mesma resposta que a §4 dá aos ficheiros: **fraccionar**. Uma
keyframe passa a ser um **lote** de N unidades de ≤ 189 telhas, e o
receptor volta a ter a imagem completa ao fim da última.

```text
640×480 = 1 200 telhas
        ÷ 189 por unidade
        = 7 unidades

unidade 0   telhas   0..188
unidade 1   telhas 189..377
...
unidade 6   telhas 1131..1199   flag FIM_DE_LOTE
```

O cabeçalho de cada unidade são 12 bytes:

```text
base_seq    4 B   §8.5: igual ao seq da unidade
largura     2 B
altura      2 B
n_unidades  2 B
indice      2 B
```

E a geometria vai **em todas** as unidades, não só na primeira. Podia
ir só na primeira e poupava 4 bytes por unidade; vai em todas porque
uma keyframe é o mecanismo de recuperação, e o receptor que precisa
dela pode ser um que esteve dez minutos sem ver nada — e um receptor às
cegas não tem geometria para inferir.

#### 3.6.2 O que fica igual, e porque é que não podia ser diferente

**O `base_seq` não muda.** A unidade `k` do lote leva
`seq = s + k` e `base_seq = s + k`. Isto mantém exacta a invariante de
§8.5 — «`base_seq == seq` é uma keyframe» — e mantém a cadeia contínua
sem excepções: a delta que vem depois do lote leva `base_seq` igual ao
`seq` da última unidade.

Por isso [`Receptor`](ressincronizacao) não precisou de uma regra nova:
`pode_aplicar` vê `base_seq == seq` e aceita, como sempre fez.

#### 3.6.3 O que a fracção NÃO é

**Não é aplicar unidade a unidade.** Uma keyframe é o delta contra um
quadro **zero** e o receptor aplica a **soma** do delta a um quadro
zero. Concatenar os bytes das telhas e aplicar uma vez dá a imagem
certa; aplicar unidade a unidade contra o zero daria sempre a última
telha de cada unidade.

**Não é contar telhas para saber se chegou tudo.** Um delta de
keyframe **não** tem todas as telhas: `diferenca` salta as que não
mudam, e contra o quadro zero essas são as que são pretas. Exigir
«`recebidas == total`» é exigir que a imagem tenha preto onde o ecrã
tem preto, e o primeiro teste de ponta-a-ponta desceu a imagem com um
quadrante em preto.

O que é verificado são os **índices**, e é mais forte: têm de crescer
estritamente e sem buracos. Contar telhas não distingue «a telha 7
veio» de «a telha 7 não veio mas a 8 sim», e a segunda imagem é a
primeira com um buraco negro do tamanho de um quadrante.

#### 3.6.4 O custo, dito honestamente

| | |
| --- | --- |
| Overhead por unidade de lote | 12 bytes contra 4 |
| Unidades por keyframe a 640×480 | 7 |
| Recuperação de uma perda | 7 janelas ≈ 140 ms a 30 fps |
| Custo de uma perda no meio de uma keyframe | a keyframe perde-se e espera-se a próxima |

E **7 unidades de keyframe somam-se às 15 msg/s do baseline** (§3.4.2),
que já está acima do limite. A keyframe espontânea de §8.5, que é
«1 por 2 s», acrescenta 3,5 msg/s nesse segundo. Ver §3.4.2.

## 4. Modo não-streaming

### 4.1 Porque é o caminho por omissão

Porque para tudo o que não seja uma chamada, o modo não-streaming é
**objectivamente melhor**: menos mensagens, sem anel, sem relógio,
retomável e armazenável (§Introdução).

### 4.2 Protocolo

```text
1. MANIFESTO   1 mensagem, flags bit 0
2. FATIAS      N mensagens, uma por pedaço
3. SHA-256     o receptor só marca «disponível» quando bate certo
```

Cada fatia é um `FRAME_CHAT` normal. Nenhum tipo novo, nenhum opcode de
rede.

### 4.3 Retomar, e porque não há retransmissão

`docs/relay.md` §Regras de retenção: *entrega única — sem duplicados*,
*TTL 300 s*, e `docs/roadmap.md` regista que não há
retransmissão. **Uma fatia perdida não volta sozinha** e a transferência
fica pendurada para sempre se nada a pedir.

A solução mais barata possível é a que já é a norma num messenger: **o
receptor responde com uma mensagem de texto**.

```text
A → B   MANIFESTO (id = 9f3a…, fatias = 204, sha256 = …)
B → A   texto normal: "9f3a: faltam 7 42 118"
A → B   as três fatias
```

O que responde é uma pessoa a dizer o que falta. É um `FRAME_CHAT`
normal, um envelope normal, e a rede não distingue esta resposta de
qualquer outra mensagem.

O receptor guarda os `pedaco` já recebidos e só reenvia os que faltam, por
isso a retoma não reenvia o que já chegou.

### 4.4 Armazenamento

O sidecar guarda **envelopes, não texto claro** — é o que `server/loja.py`
já declara e já faz. Portanto guardar media encriptada **não precisa de
criptografia nova**:

```text
plano de dados  →  ENCODE  →  envelope (ciphertext)  →  loja
                                                    guarda-se o envelope
```

A loja nunca tem uma chave de media e nunca precisa de uma: guarda
ciphertext, tal como já guarda o das mensagens.

Cada linha de fatia guarda o `id` da transferência a que pertence, e é
isso que faz duas coisas. A **conta** dá o número certo mesmo numa
conversa com duas transferências e texto pelo meio — filtrar por
`envelope IS NOT NULL` contaria as mensagens de texto encriptado.
E a **eliminação** sabe o que é seu: apagar uma transferência apaga as
suas fatias e nada mais, porque o filtro é `transferencia_id = ?` e não
`conversa_id AND envelope IS NOT NULL`.

O texto fica, por omissão, com `transferencia_id` a `NULL` — e é esse
`NULL` que o protege.

### 4.5 Eliminação

> **Se quem envia elimina, quem recebeu mantém.**

Isto é **verdadeiro por construção**, e não por uma política. Cada
participante tem as suas linhas, o seu `id`, o seu ficheiro. Um `DELETE`
de um lado **não chega ao outro porque nunca houve canal de propagação**
— e, pelo modelo de ameaça, não deve haver: um `DELETE` propagado seria
uma capacidade de destruição remota.

É a mesma propriedade do texto: apagar uma mensagem da tua máquina não
apaga a cópia que o outro já tinha.

### 4.6 Reproduzir

Na RAM, e só na RAM (§7.4): desencriptar, concatenar em memória,
`memfd_create`, `fexecve`. **O ficheiro nunca toca o disco** — nem
temporário, nem apagado. `ls /proc/<pid>/fd` do leitor de media não
mostra nenhum ficheiro com nome: mostra `/memfd:onyx-media`.

#### 4.6.0 Qual é o selo, e qual **não** é

O descritor leva `F_SEAL_SEAL | F_SEAL_GROW`, e a escolha merece
explicação porque a intuitive é errada:

| Selo | O que proíbe | Aqui |
| --- | --- | --- |
| `F_SEAL_SHRINK` | `ftruncate` para **menor** | não |
| `F_SEAL_GROW` | `write` que **aumenta**, e `mmap` para lá do fim | **sim** |
| `F_SEAL_SEAL` | mais selos, e `mmap` para lá do fim | **sim** |
| `F_SEAL_WRITE` | abrir para escrever | **jamais** |

O que protege é `F_SEAL_GROW`: sem ele, quem herda o descritor
acrescenta bytes ao fim e transforma o vídeo num ficheiro maior do que
o que o SHA-256 diz. E o `F_SEAL_SEAL` vai junto para que não possam
ser acrescentados mais selos.

`F_SEAL_WRITE` é o único que proíbe escrever por inteiro, e é
precisamente o que **não** pode ser usado: o leitor de media tem de ler
o ficheiro.

> A primeira versão punha só `F_SEAL_SEAL`, com um comentário que dizia
> que impedia o ficheiro de crescer. **Não impedia** — um `write(2)`
> normal passa, e o kernel deixa o ficheiro ir de 6 para 9 bytes.
> Encontrado por um teste, não por leitura da documentação: o teste
> escreve depois do selo e esperava `EPERM`.
>
> A lição geral: `F_SEAL_SEAL` soa a «selado, não mexas» e não é. O
> que sela é `F_SEAL_GROW` e `F_SEAL_WRITE`, e são coisas diferentes.

#### 4.6.0b O cursor fica no fim

O `write` deixa a posição no fim, e um leitor que **herde** o descritor
começa aí: a primeira `read` devolveria zero bytes, e o ficheiro pareceria
vazio.

Não é um bug em produção — o daemon passa o descritor por
`execve("/proc/self/fd/N")`, e o filho abre o caminho, que começa na
posição 0. Mas é uma armadilha para quem testar o descritor directamente,
e por isso está escrita aqui e não só no código.

#### 4.6.1 A superfície IPC

O `sidecar` **não** monta ficheiros. Montar exige as chaves da
amizade, e o sidecar não as tem — nem deve. Quem tem as chaves é o
`messenger`, e quem tem o `memfd` é o `daemon`.

Por isso a reprodução é um `MEDIA_REPRODUZIR` e não uma rota HTTP:

| Comando | Valor | Corpo do pedido | Resposta `OK` |
| --- | --- | --- | --- |
| `MEDIA_INICIAR` | `0x0D` | `caminho_loja: u16 LE ‖ UTF-8` | vazio |
| `MEDIA_ESTADO` | `0x0E` | vazio | `loja: 1B ‖ ffmpeg: 1B ‖ leitor: 1B` |
| `MEDIA_CAPTURAR` | `0x0F` | `largura: u16 LE ‖ altura: u16 LE ‖ fps: u16 LE ‖ audio_kbps: u16 LE` | `largura ‖ altura`, `u16 LE` |

A resposta é a geometria **efectiva** e não a pedida: com `scale` e
`format=gray`, o `ffmpeg` pode arredondar, e devolver o que se pediu
seria um contrato que mente.

> **Não há `MEDIA_PARAR`, e é de propósito.** A captura pertence à
> ligação IPC que a pediu: quando o `sidecar` fecha, ela morre. Um
> comando de paragem exigiria um `pid` guardado no daemon para poder
> falar de «aquela» captura — e um `pid` guardado é um estado que
> sobrevive à coisa que o justificava. Um `ffmpeg` vivo depois de o
> `sidecar` fechar seria um `x11grab` a ler o ecrã de uma pessoa sem que
> ninguém consuma.
| `MEDIA_REPRODUZIR` | `0x10` | `transferencia: 16 bytes hex ‖ leitor: u8` | `pid: u32 LE` |

`MEDIA_REPRODUZIR` **não leva chaves**, e a razão merece estar escrita
porque a primeira versão levava `k1`, `k5`, `k9`, `pub` e `seed` (§
4.6.3, que já não existe). Ver §4.6.4.

#### 4.6.1a A resposta **não** é um `fd`, e porquê

Uma primeira versão desta tabela dizia que `MEDIA_REPRODUZIR` devolvia
um descritor. Não pode, e a razão é de transporte:

**Um número de descritor só significa alguma coisa no processo que o
tem.** Para o `sidecar` receber o `fd` do `daemon` seria preciso
`SCM_RIGHTS` — dados anexos no socket — e isso muda o enquadramento de
`ipc.rs`, que é a base de todo o IPC local e de todos os seus testes.

Por isso o `daemon` é quem **lança** o leitor, e a resposta é o `pid`:

* o `memfd` sobrevive ao `exec` porque foi criado **sem**
  `MFD_CLOEXEC` — não há `pre_exec` a correr antes do `exec` num
  processo com várias threads, que é onde um erro é um `SIGSEGV` em
  vez de um `errno`;
* o `daemon` **não** desaparece: é `fork` + `exec` dentro do filho, e o
  daemon continua a responder a IPC enquanto o áudio toca. Um
  `execvp` sem `fork` substituiria o daemon, e a aplicação deixava de
  funcionar enquanto se ouvia um áudio;
* o leitor herda **só** o descritor do ficheiro. `Command` fecha o
  resto, e portanto o leitor de media não recebe o socket do IPC.

A alternativa — o `daemon` monta e o `sidecar` lança — é mais elegante
no papel e, na prática, exige o mesmo `SCM_RIGHTS`. O ganho seria
**quem** executa o programa, e isso não compensa mexer no transporte de
um protocolo já testado.

E é a mesma operação que o `ffmpeg` da captura, com outro binário.

**O `fd` é a resposta, e não um caminho.** É o mesmo acordo de §7.5: o
daemon entrega um descritor selado e o leitor herda-o. A alternativa —
escrever em `/tmp` e devolver o caminho — seria um ficheiro que qualquer
outro utilizador da máquina pode abrir.

`MEDIA_INICIAR` existe porque o `daemon` e a loja do `sidecar` são
processos diferentes, e o daemon precisa da loja para ler os envelopes.
O caminho vem **do sidecar**, e nunca do cliente: um caminho que o
cliente escolhesse seria o cliente a dizer ao daemon que ficheiro ler.

> **`0x0D`–`0x10` já estavam reservados.** Nenhum comando novo, nenhum
> `FRAME` novo na rede. O que muda é o que o daemon **faz** com os
> bytes — e isso é exactamente o que a derivação por tipo esconde.

#### 4.6.2 Opção A: o daemon lê a loja do sidecar

Escolhida sobre as alternativas, e a razão é a mesma da §7.4: **o
ficheiro nunca toca o disco**.

* **Opção B — devolver os envelopes ao `sidecar`**, que monta num
  `memfd` seu e passa o `fd`. Exige que os envelopes atravessem o IPC
  para ir e voltar, e duplica a montagem em dois lados.
* **Opção C — o `messenger` monta e guarda num `memfd` próprio**, e o
  leitor herda-o. Evita a loja, mas obriga a manter um segundo
  armazenamento de envelopes que é exactamente o que §4.4 diz para não
  duplicar.

A opção A tem um custo, e é real: **o daemon passa a confiar num
ficheiro do sidecar**. Mitigações, e nenhuma delas chega a «confiar no sidecar»:

1. o caminho vem do `sidecar`, tem de ser absoluto e existe — verificado
   na abertura;
2. o esquema é lido em modo `readonly` (`file:…?mode=ro`) — um
   `sqlite3` que escrevesse num ficheiro de media seria pior do que o
   que estamos a evitar;
3. nenhuma consulta que o daemon faça aceita uma constante vinda do
   exterior — os identificadores são hex de tamanho fixo, validados
   antes de chegar ao `sqlite3`;
4. se o ficheiro não for uma base Onyx, a resposta é um erro de media,
   não um crash.

O ponto (3) é o que torna isto aceitável: **a superfície de ataque é o
identificador, e o identificador é hex de 32 caracteres validado antes
de tocar em SQL.** Não há concatenação de cadeia vinda do exterior, e
todos os `?` são parâmetros ligados.

#### 4.6.4 Porque `MEDIA_REPRODUZIR` não leva chaves

A primeira versão levava `k1`, `k5`, `k9`, `pub_remetente` e `seed`, e
montava o ficheiro **desencriptando** cada `envelope` guardado. Isso
está errado, e a falha é silenciosa:

**O `nonce1` de cada fatia já foi consumido no registo anti-replay** no
momento em que a mensagem chegou. Uma segunda decifra do mesmo envelope
é exactamente o ataque que o registo existe para travar — a resposta
correta seria `0x13 NonceRepetido`, para todas as fatias, sempre.

Desactivar o registo para a montagem seria trocar uma garantia real
pela conveniência de não ter de a implementar duas vezes.

O que se perde ao ler o `texto`? Nada. A loja guarda **as duas**
colunas: `texto` é o plaintext da moldura, `envelope` é o ciphertext. E
o `texto` já é plaintext de cada mensagem — é o que a interface mostra.

E a integridade continua a vir de onde vinha: o **SHA-256 do manifesto
é conferido sobre os bytes montados**, e é essa conferência que impede
que o `memfd` incorrecto chegue a ser criado.

A consequência prática é boa: o `daemon` **não precisa da amizade para
reproduzir**, e o pedido de reprodução não transporta nenhum segredo.
Um pedido que leva chaves é um pedido em que um erro de comprimento é
um vazamento — e este não tem por onde vazar.

## 5. Relógio

### 5.1 O invariante

```text
RelogioMedia { media_ms: i64, bytes_por_ms: u32 }
```

O clock é **tempo de media**, monotónico, e **nunca** tempo de parede.
Um `Instant` do sistema não entra nesta aritmética: entra apenas para
decidir *quando presenting*, e essa decisão é §5.5.

### 5.2 Quem avança, e com quê

* **Áudio** avança o relógio mestre:
  ```text
  media_ms += bytes_recebidos / bytes_por_ms
  ```
* **Vídeo** avança o relógio de vídeo por `1000 / fps` por frame
  processado.

O áudio é o mestre porque o seu ritmo é **constante por construção** e o
do vídeo não: o `fps` varia, a taxa de bits de áudio não.

### 5.3 `bytes_por_ms` é declarado, não presumido

A regra `16 bytes/ms` é exacta para PCM cru a 128 kbps. E PCM cru a
128 kbps é s16le mono a 8 kHz — qualidade telefónica. Para Opus a 128
kbps os 16 são *nominais*, não exactos: os frames Opus duram
2,5/5/10/20/40/60 ms e cada um leva uma quantidade variável de bytes.

Daí a regra ser generalizada e **declarada uma vez** no manifesto:

```text
bytes_por_ms = bitrate_bps / 8000
```

| Transporte | `bitrate_bps` | `bytes_por_ms` |
| --- | --- | --- |
| PCM s16le mono 48 kHz | 768 000 | 96 |
| PCM s16le mono 8 kHz | 128 000 | 16 |
| Opus CBR 32 kHz | 32 000 | 4 |

O emissor **tem** de ligar `-vbr off` (§8.1). Com VBR o valor declarado é
uma média e o relógio acumula desvio.

### 5.4 A verificação cruzada

`clock_ms` viaja no preâmbulo de cada unidade (§2.2). É a
**verificação cruzada** do clock por contagem:

```text
|clock_ms declarado − clock contado| > 5 ms  →  stream dessincronizada
```

O clock por contagem é o primário — é o que avança sem esperar pelo
manifesto — e o `clock_ms` declarado é o que confirma. Divergir não é
um incómodo: é a detecção de perda, de adulteração, ou de um
`bytes_por_ms` declarado a enganar. Seis bytes por unidade compram três
certezas que de outra forma só apareceriam como áudio corrido.

### 5.5 A regra dos três cenários

```text
atraso = relogio_video_ms − relogio_audio_ms

atraso >  +LIMITE (120 ms)  →  DESCARTAR o frame
                              e SOMAR o tempo do frame ao clock de vídeo
|atraso| ≤ LIMITE          →  APRESENTAR          (sincronia)
atraso <  −LIMITE          →  APRESENTAR já
```

**O caso `LIMITE > TempoVideo` não tem um `sleep` no caminho normal, e
esta é a correcção mais importante deste documento.** O clock é tempo de
media, não tempo de parede. Tor tem TCP por baixo, portanto **atrasa
muito e perde pouco** — o problema real é atraso, não perda. Dormir
localmente converte um problema de rede num problema de renderização: o
frame chega, o utilizador vê um congelamento, e nada melhorou.

`nanosleep` só é legítimo como anti-burst — quando o emissor é mais
rápido que o fio e a fila cresce sem limite — e mesmo aí tem de ser
limitado a ~5 ms por iteração com `Instant::now()` a medir o que resta.
Um `sleep` de um milissegundo num laço de apresentação é uma taxa fixa
de 1000 fps que ninguém pediu.

### 5.6 Porquê 120 ms

Três vezes o piso do Tor (≈40 ms de ida e volta em condições
favoráveis). Abaixo disso o jitter normal do Tor passa a ser reportado
como perda de sincronização e o receptor deita fora frames que ainda
chegavam a tempo. Acima disso, «sincronia» passa a ser «sincronia com
um atraso de dois segundos».

---

## 6. Codificação de vídeo

### 6.1 `rawvideo gray`

O quadro que o `ffmpeg` emite é **luma de 8 bits por pixel**, um byte por
pixel:

```text
largura × altura × 1 byte
```

`-pix_fmt gray` é 1/3 do RGB e, para partilha de ecrã — que é quase
sempre texto e interface, não fotografia — luma-only é visualmente quase
sem perdas. `largura` e `altura` são **múltiplos de 16**, obrigatoriamente
(§2.5) — e note-se que 360 não conta (22,5), o que torna 640×360
inválido apesar de ser o 16:9 mais natural.

### 6.1b Uma telha **não** são 256 bytes contíguos

São 16 linhas de 16 bytes, com passo de `largura`. Escrever `fill()`
sobre 256 bytes seguidos — que é o que a aritmética sugere — muda 16
linhas de pixels de 16 sítios diferentes.

A primeira implementação de [`media/telhas.rs`] fazia exactamente isso,
e o resultado era subtil e terrível: uma telha comparava-se com a faixa
de pixels à sua volta, o delta marcava como alteradas todas as telhas de
uma linha sempre que uma mudava, e **uma mudança contaminava as seis
seguintes**. O receptor reconstruía o mesmo quadro — a identidade
passava — e viajava o dobro dos bytes.

> É a mesma armadilha que a escrita tem. Por isso tanto a comparação
> (`de_telha`) como a aplicação (`aplicar`) são linha a linha, e por isso
> o teste escolhe uma telha **do meio** de uma linha com vizinhas: com a
> telha 0 o bug não aparece.

### 6.2 Telhas 16×16

```text
frame (W×H cinza)
  └─► telhas 16×16 → grelha de W/16 × H/16
        ├─ telha inalterada  → índice apenas (2 bytes)
        ├─ telha chapada     → símbolo + 1 byte
        └─ telha com variação→ símbolo + 256 bytes quantizados
```

**Porquê telhas e não linhas.** Com análise por linha, um scroll de 1
pixel altera **todas** as linhas e o delta explode para 100%. Com telhas
16×16, altera **uma fila de telhas**. É a mesma razão pela qual
H.264, VP9 e AV1 usam macroblocos: é a unidade que sobrevive a
deslocamentos, e o ecrã é o que se desloca.

Quantização a dois níveis (chapada / não-chapada) é o degrau seguinte se
a banda apertar: reduz a telha com variação de 256 para muito menos.

### 6.3 O delta corre em binário, nunca depois do base-64

Repetido porque é a inversão mais fácil de cometer: **base-64 não tem
alinhamento a bytes**. Duas linhas de pixels iguais produzem caracteres
base-64 diferentes. Um RLE aplicado depois do base-64 não encontraria
repetições que existem, e o `HOLD` (§8.4) — que depende de a unidade
não ter nada a enviar — nunca activaria.

A ordem é **delta → RLE → base-64 → ENCODE**, e cada passo é
irredutível.

---

## 7. Dependências embutidas

### 7.1 A regra

O `ffmpeg` **não** é um binário pré-compilado de terceiros. É a fonte
do FFmpeg com sumário fixado no repositório, compilada por uma receita
que também está no repositório, comprimida com xz e embebida no
`onyxchatd`.

Isto não é uma preferência de estilo. A primeira versão apontava para um
executável pronto, e o que se encontrou é que **os builds estáticos de
`ffmpeg` mais fáceis de obter são GPL v3** — e o `deny.toml` recusa GPL
porque uma dependência copyleft torna a licença do projecto impossível
de cumprir. Um binário de terceiros de 80 MB, de origem não
verificável, seria uma renúncia a qualquer garantia.

A garantia que o substitui:

| Onde | O quê |
| --- | --- |
| `tools/ffmpeg/versoes.txt` | A versão exacta (9.0.2) e o SHA-256 da fonte |
| `tools/ffmpeg/fetch.sh` | Descarrega a fonte e **confirma o sumário antes de descompactar** |
| `tools/ffmpeg/build.sh` | Compila com uma receita fixa e **verifica a licença pelo binário** |
| `network/daemon_rust/build.rs` | Embete o executável; **escreve sempre** um ficheiro, mesmo vazio |
| `media/ffmpeg_embutido.rs` | Executa-o de um `memfd`, sem tocar o disco |
| `tests/media_embutido.rs` | Prova que arranca e que **declara `--disable-gpl`** |

### 7.2 A receita, e o que fica de fora

```text
--disable-gpl  --disable-nonfree  --disable-network  --disable-debug
--disable-doc  --disable-x86asm   --disable-everything
--enable-ffmpeg
--enable-muxer=…  --enable-demuxer=…  --enable-protocol=fd,file
--enable-libopus  --enable-encoder=libopus
--enable-libvpx   --enable-encoder=libvpx_vp8,libvpx_vp9
--enable-parser=vp8  --enable-decoder=rawvideo
```

E `--enable-encoder=libvpx_vp8`, **não** `--enable-encoder=libvpx`:
o segundo é aceite, não corresponde a nada, e o FFmpeg avisa
`Option --enable-encoder=libvpx did not match anything` e segue em
frente. O `--enable-libvpx` liga a biblioteca; o `--enable-encoder`
liga o encoder.

`--disable-everything` seguido de uma lista de `--enable` **explícita**
é o que torna a receita independente da versão: sem ele, o que o FFmpeg
traz de origem mudaria a cada release, e uma receita que muda sozinha
não é uma receita.

| Fora | Porquê |
| --- | --- |
| `libx264`, `libx265` | **GPL.** É a razão de isto ser LGPL e não GPL. |
| VP9 patentado | **GPL.** O VP8 e o VP9 sem patents continuam disponíveis. |
| rede | O `ffmpeg` nunca fala com a rede. `docs/threat_model.md` §A2 é sobre um relay malicioso; um cliente HTTP dentro de um executável embebido é superfície que ninguém pediu. |
| `ffplay` e SDL2 | O `ffmpeg` de captura não reproduz. A reprodução é do receptor. |

### 7.3 A excepção de licença, e onde está escrita

A receita deriva **LGPL-2.1-or-later**, que o `deny.toml` também
recusa. A excepção está escrita em `THIRD-PARTY.md`
§Componentes compilados de fonte, ao lado da `libsodium` — o mesmo
padrão: uma regra, um componente nomeado, uma justificação, e o
mínimo possível fora dela.

O `deny.toml` **não foi alterado**, e essa é a decisão que importa. A
lista avalia crates do registry; o FFmpeg não é um crate. Acrescentar
`LGPL` à lista transformaria a excepção de *um* componente nomeado numa
excepção da **categoria** — passaria a valer para qualquer dependência
LGPL que alguém acrescentasse depois.

### 7.4 Porque `memfd_create` e não um ficheiro temporário

| | Ficheiro temporário | `memfd_create` |
| --- | --- | --- |
| Onde está | `/tmp`, com nome | **em lado nenhum** — não tem nome |
| Depois do `unlink` | o inode fica até o último descritor fechar | não há nome para apagar |
| Quem o vê | qualquer utilizador da máquina | quem tiver o descritor, e o `F_SEAL_SEAL` torna-o imutável |
| Custo | paginado sob pressão | é memória anonima |

O terceiro ponto é o que o `docs/threat_model.md` §A8 descreve: outro
utilizador da mesma máquina. Um `/tmp` com um executável de 80 MB
dentro é um ficheiro executável que qualquer outro utilizador pode ver,
copiar e executar.

O custo desta escolha é o do canto inferior direito: um `memfd` de
80 MB **é** memória anonima. Um ficheiro em disco seria paginado sob
pressão de memória; este não tem para onde ir. É a mesma troca que
`crypto_core::paginas` faz em sentido inverso — ali tranca-se para o
segredo não ir para swap, aqui aceita-se que vá porque o conteúdo é um
programa público.

### 7.5 O que foi medido

Números de uma compilação real, medidos em 2026-10-16 (FFmpeg 9.0.2,
`-j4`, máquina com 4 GB de RAM):

| | |
| --- | --- |
| | conjunto mínimo | **com `libvpx`** |
| --- | --- | --- |
| Tempo da compilação | 1 min 59 s | **2 min 47 s** |
| Executável | 3,1 MB | **7,3 MB** |
| Comprimido com `xz -9e` | 1,0 MB | **2,4 MB** |

A receita com `libvpx` é a que está em `tools/ffmpeg/build.sh`.

> **Uma versão anterior desta secção previa «dezenas de MB» e «dez
> minutos».** Nenhum dos dois: o `libvpx` liga só os encoders
> *externos* e não entra a biblioteca inteira, porque `--disable-everything`
> corta o resto. O executável cresce 2,4× e a compilação 1,4×.

E `--disable-x86asm` custa ~20 % de velocidade ao VP8. O comentário
da receita dizia que era irrelevante porque num `rawvideo` quem
comprimia era o browser — e deixou de ser verdade quando o vídeo
passou a codificar-se aqui.

### 7.6 O modo degradado

Sem `tools/ffmpeg/cache/ffmpeg.xz`, o `build.rs` escreve um ficheiro de
**zero bytes** e emite um aviso de compilação. Daí `disponivel()` é
`false`, `MEDIA_INICIAR` devolve `0x1F DependenciaAusente`, e **o
texto funciona exactamente como antes**.

Isto não é uma conveniência: é o que permite construir o daemon numa
máquina que não tem as bibliotecas de captura, o que é a maioria das
máquinas de desenvolvimento.

## 8. Streaming

### 8.0 O áudio e o vídeo andam ao mesmo ritmo

`ffmpeg` escreve o vídeo no descritor 1 e o áudio no descritor 3, e
**não os acopla**. Cada um chega à taxa dele.

Feitas as contas, os dois duram o mesmo tempo: em 10 s o vídeo oferece
10 000 ms e o áudio também. A diferença é de 1 % — o arredondamento de
33,3 ms para 33 ms.

O que existe é **acúmulo**: o áudio chega em rajadas, e se o emissor o
escreve assim que o tem, ele ganha corrida numa rajada e perde noutra.
O buffer do receptor enche e alguém tem de o descartar.

E aqui a assimetria decide tudo:

| | descartar custa |
| --- | --- |
| áudio | um estalo, e a voz volta atrasada — **ouve-se** |
| vídeo | um quadro que o seguinte torna irrelevante (§8.2) — **não se ouve** |

Por isso é o áudio que se protege, e por isso `media/desempate.rs`
mede os dois em **tempo** e não em contagem. 48 kHz e 30 fps não têm
unidades em comum.

A regra é uma janela de acúmulo de **200 ms**:

```text
áudio acumulado  ≤  vídeo visto + 200 ms   →  emite
áudio acumulado  >  vídeo visto + 200 ms   →  espera pelo vídeo
```

200 ms é a janela de jitter de uma chamada normal. Abaixo disso corta-se
áudio que ia chegar; acima disso a voz atrasa-se. E **quando a janela
transborda não se descarta nada**: o áudio espera. Um emissor lento
custa menos do que uma voz entrecortada.

> **Uma versão anterior desta secção dizia que o áudio chegava «1,67×
> mais rápido» e que o receptor descartava «4 em cada 5» amostras.**
> Não é verdade, e a conta errada vem de comparar 20 ms de áudio com
> 33 ms de vídeo como se um fosse contagem de unidades e o outro tempo.
> Com o limitador, uma chamada de 10 s emite as 500 unidades de áudio
> das 500 que o `ffmpeg` produz.

#### 8.0.1 O receptor: ler o IVF

O `ffmpeg` escreve **IVF** no descritor 1, e o daemon tem de saber
onde acaba um quadro para o enviar como uma unidade.

```text
cabeçalho, 32 bytes
  DKIF(4) versão(1) cabeçalho_extra(1) reservado(2) fourcc(4)
  largura(2) altura(2) fps(4) n_quadros(4) reservado(8)

cada quadro
  tamanho:u32le   timestamp:u64le   payload
```

**O `n_quadros` do cabeçalho mente, e vale sempre 1.** O `ffmpeg`
escreve o cabeçalho antes de saber quantos quadros vai produzir. Um
leitor que confiasse nele leria um quadro e achava que a chamada tinha
acabado — e o sintoma seria «a chamada mostra um frame de dez em
dez», que não parece um bug de contagem.

**O byte 5 é o cabeçalho *extra*, e é 0.** Um IVF tem os 32 bytes todos
no cabeçalho fixo; pôr 32 diz ao `ffmpeg` que há mais 32 bytes a ler.
O sintoma é `Invalid frame rate`, porque o desvio de dois bytes atira
o `fps` para o sítio errado.

**O `fourcc` é validado.** IVF também transporta VP9 e AV1, e este
daemon só tem VP8. Aceitar um IVF de outro codec daria um ecrã preto
sem nenhum sinal.

**Um quadro truncado não é o fim do stream.** O `ffmpeg` morreu a meio
de escrever é diferente de ter terminado.

#### 8.0.1.1 Porquê o IVF **não** é usado para descodificar

Este é o ponto que custou mais tempo a perceber, e é **medido**, não
suposto.

O IVF é um contentor de **ficheiro**: declara quantos quadros tem. E o
`ffmpeg` que o lê **não emite nada** até saber que leu todos — porque
um frame-P depende dos anteriores e não sabe se virá o seguinte.

| | resultado |
| --- | --- |
| IVF completo por um pipe | **90 imagens** |
| um quadro, pipe aberto | **0 bytes** |
| `-avioflags direct` | 0 bytes |
| `-fflags nobuffer` | 0 bytes |

Num pipe nunca se sabe quando acaba, e por isso um `ffmpeg` a ler IVF
de um pipe **não mostra nada**.

E daí a escolha: **quem descodifica é a UI, com `WebCodecs`**. Não é
uma preferência de front-end:

* O `VideoDecoder` descodifica cada `EncodedVideoChunk` à medida que
  chega, e **não precisa de saber quantos há**. O problema do IVF
  não existe para ele.
* `optimizeForLatency` desliga o buffering de reordenação. Sem isso o
  browser guarda dois ou três quadros à espera de um keyframe, e a
  imagem que aparece tem dois ou três quadros de atraso — numa
  chamada, isso é uma conversa em que as pessoas se interrompem.
* Não há subprocesso, não há contentor intermédio, e não há latência
  de lote.

E o daemon não descodifica porque **nunca precisa de o fazer**: um
emissor não descodifica. Ligar o `libvpx` para descodificar também
duplicaria o executável por uma coisa que só o receptor usa.

#### 8.0.2 O que o daemon **não** faz

O daemon **não decodifica**. Transporta bytes de VP8 de um lado ao
outro, e quem os decodifica é o receptor, que tem o `ffmpeg` completo.

E isto não é uma poupança de espaço: o `build.sh` liga `libvpx` para
codificar, e ligar também para decodicar duplicaria o executável por
uma coisa que **o emissor não precisa**. Um emissor nunca descodifica.

O quadro chega ao anel em bytes comprimidos, e o anel conta **unidades**
e não microssegundos — porque um quadro de VP8 tem comprimento
variável, e «contar microssegundos de vídeo» não teria base nenhuma.

### 8.1 Captura: `ffmpeg` e o segundo canal

O `ffmpeg` escreve o vídeo no descritor 1 e o áudio no descritor 3. O
descritor 3 é criado pelo pai antes do `exec` e sobrevive à substituição
da imagem do processo:

```text
ffmpeg -f x11grab -video_size WxH -framerate F -i :0.0
       -filter:v scale=W:H
       -c:v libvpx -b:v Bk -deadline realtime -cpu-used 8 -g 60
       -map 0:v -f ivf -                         → fd 1  (vídeo)
       -i default
       -map 1:a -c:a libopus -b:a Bk -vbr off
              -f opus fd:3                        → fd 3  (áudio)
```

* `-vbr off` é **obrigatório** (§5.3).
* `-deadline realtime` e `-cpu-used 8` são o que distingue uma
  codificação de chamada de uma de ficheiro. Sem eles o VP8 procura um
  bitstream melhor do que a latência permite, e o `ffmpeg` acumula
  atraso — que numa chamada é indistinguível de rede lenta.
* O contentor é **IVF**, não WebM: um cabeçalho de 32 bytes e os
  quadros a seguir. O WebM traz um índice, e um índice num pipe não
  serve para nada.
* **`format=gray` saiu.** O VP8 quer YUV; um quadro de um só canal
  teria de ser expandido para três antes de ser comprimido, e
  reconstruído para um só depois.
* `fd:3` é a forma documentada de escrever num descritor numerado.
* O `dup2` em `pre_exec` tem de falhar alto: se falhar em silêncio, o fd
  3 é qualquer outra coisa e o áudio vai para o sítio errado sem erro
  nenhum. Daí `dup2` com verificação e `_exit(127)`.

#### 8.1.1 O `stderr` é lido, e o arranque é verificado

O `stderr` do `ffmpeg` vai para um pipe, e o que está dentro do pipe
importa:

* **Se ninguém o lê**, o buffer (64 kB) enche e o `ffmpeg` **bloqueia
  para sempre** à espera de um leitor que não existe. A captura fica
  pendurada e nada está errado do ponto de vista de ninguém.
* **Se ninguém o lê e o `ffmpeg` falha** — ecrã não encontrado, formato
  não suportado — `MEDIA_CAPTURAR` respondia `OK`. A pessoa via «a
  capturar» e não acontecia nada, sem pista.

Por isso há uma thread que consome o pipe e guarda as **últimas 8
linhas**, e [`lancar`] espera até 60 ms para o filho dizer que não
arrancou. O custo é 60 ms numa acção que acontece uma vez por captura, e
o que se recebe em troca é a mensagem do próprio `ffmpeg`.

> A espera de arranque **não** é um `sleep` arbitrário. A primeira
> versão regressou `None` no primeiro passo porque `try_wait` devolve
> `Ok(None)` enquanto o filho vive, e não quando morreu — a verificação
> ficou a fazer nada sem dar erro. E a segunda vez que o mesmo trocador
> caiu foi um `return` dentro do laço que saía no passo 1 de 30.

E a última linha do `stderr` é a que vale a pena mostrar: o `ffmpeg`
escreve o contexto antes do erro.

#### 8.1.2 Sem `MEDIA_PARAR`, e porquê

A captura pertence à **ligação IPC** que a pediu. Quando o `sidecar`
fecha, morre.

Um comando de paragem exigiria um `pid` guardado no daemon para poder
falar de «aquela» captura — e um `pid` guardado é um estado que
sobrevive à coisa que o justificava. Um `ffmpeg` vivo depois de o
`sidecar` fechar seria um `x11grab` a ler o ecrã de uma pessoa sem que
ninguém consuma.

E uma segunda captura não substitui a primeira: seria uma segunda
leitura do mesmo ecrã, a competir com a primeira, e quem pediu a
primeira ficaria sem som sem saber porquê.

**Wayland**: `-f x11grab` só funciona com XWayland. A detecção é
`$XDG_SESSION_TYPE`; sem XWayland o emissor usa `pipewiregrab`
(ffmpeg ≥ 6.1) ou recusa com um erro que diz qual.

### 8.2 O anel, orçado em microssegundos

O anel **não** é contado em frames. Uma keyframe tem dezenas de
kilobytes e um delta tem um; um anel contado em frames não tem memória
previsível, e é a memória que estoura primeiro.

```text
capacidade = 2 s de tempo de media
```

Duas filas com **políticas opostas**, e esta é a parte que um anel único
não consegue ter:

| Fila | Política | Porquê |
| --- | --- | --- |
| áudio | **bloqueia, nunca descarta** | perder áudio parte o relógio mestre (§5.1) e deixa o vídeo a apresentar sem som — pior que silêncio |
| vídeo | **drop-oldest** | descartar vídeo é a operação correcta |

Um descarte de vídeo é um **evento de divergência**: marca a próxima
unidade como keyframe (§8.5).

> **O anel não decide a keyframe.** Regista o descarte — com o `seq` e o
> tempo — e quem emite decide. Quem decide tem o `seq`, o relógio e o
> coalescer à vista; quem regista só tem uma fila.

E o áudio: §8.2 diz «bloqueia, nunca descarta», e isso é o que acontece
no **caminho normal** — `cabe_audio()` pergunta antes, e um `false` faz o
emissor esperar. O que acontece se não couber na mesma é a pergunta que
a spec não faz em voz alta, e a resposta é descartar o **mais antigo**:
crescer sem limite estoura a memória do daemon, e um daemon que morre é
pior do que alguém que ouve dois segundos de áudio antigo. Descartar o
áudio **novo** tiraria a voz a quem está a falar agora.

Um anel **não** é só isto: `cabe_video()` é o espelho para o vídeo, e
perguntar depois de empurrar já é tarde.

### 8.3 Coalescing adaptativo

```text
delta pequeno (< 8 KB)  →  acumular até 20 ms, depois emitir
delta grande            →  emitir já, um frame por mensagem
0x10 RateLimit recebido →  emitir já, até a janela expirar
```

### 8.3.1 «Acumular» é guardar o **último quadro**, não concatenar deltas

A primeira leitura de §8.3 é acumular os **deltas** e mandá-los juntos.
Está errado, e um contador chega:

```text
referência   telha 5 = 7
quadro 1     telha 5 = 200    → delta 1 escreve 200
quadro 2     telha 5 = 7      → delta 2 não escreve nada
```

Concatenados e aplicados à mesma referência, o receptor fica com
`200`. O quadro 2 tem `7`. O receptor mostra uma cor que **não está no
ecrã** de quem emite — e sem nenhum sinal, porque o envelope e o
`SHA` estão correctos.

Por isso o coalescer guarda **um** quadro e calcula o delta no momento
de emitir. Os quadros intermédios não são codificados: é uma perda, e
§8.2 diz que perder vídeo é a operação correcta.

### 8.3.2 A janela tem um início, e é para isso que serve um campo

A janela de 20 ms mede-se desde quando **abriu**, e não desde o quadro
anterior. A primeira versão media desde o quadro anterior, e uma stream
de deltas pequenos **nunca** fechava a janela: cada quadro reiniciava a
espera, a soma nunca chegava a 20 ms, e o coalescer acumulava para
sempre — uma fila sem limite, que é exactamente o que §8.2 proíbe ao
anel.

O `seq` e o `clock_ms` são os do **quadro que sai**; o `iniciou_ms` é
interno e serve para fechar a janela.

### 8.3.3 8 KB é 31 telhas, e a medição é um `memcmp`

Decidir se um delta é pequeno exige saber o tamanho dele, e codificá-lo
a 30 fps seria pagar o encode inteiro para decidir se o encode se paga.
O que se mede é o **número de telhas alteradas** — que é o mesmo dado de
que o delta precisa — e o limiar é `8 KB / 259 = 31`.

`259` são os bytes de uma telha com variação: 2 do índice, 1 do código,
256 do corpo.

O `RateLimit` **já é um sinal de feedback**: `p2p.rs` já o produz e o
módulo já o recebe. Nenhum canal de controlo novo é preciso para saber
que se está a saturar — o mecanismo de protecção contra flood do
protocolo **é** o sensor de congestão do media.

### 8.4 `HOLD`

Uma unidade cujo delta é vazio transporta **só o `base_seq` e a
moldura**: 40 bytes de texto em vez de um frame inteiro.

E um `HOLD` **não move a referência**: se movesse, o delta seguinte seria
calculado contra um quadro que nunca foi enviado. Mas o `base_seq`
**presente-se sempre** — é ele que diz ao receptor que a cadeia continua
e que não há delta a aplicar. Um `HOLD` sem `base_seq` seria uma
unidade que o receptor não pode nem aplicar nem descartar com
confiança.

Com ecrã estático isso é ~600 bytes/s em vez de 5,8 KB/s. E a peça não
passa pelo pipeline: é um `HOLD` dentro de um lote que **já está
autenticado** pela etiqueta AEAD do envelope. Não custa uma assinatura
nova — herda a que já existe.

### 8.5 `base_seq` e keyframes

```text
unidade de vídeo  →  base_seq = o frame de referência a que esta
                     unidade aplica os seus deltas
receptor          →  se base_seq ≠ último aplicado: descarta a unidade
                     e pede uma keyframe
emissor           →  emite keyframe (frame completo), 1 por 2 s
```

Isto é o que torna o modo streaming recuperável. Sem isto, uma única
unidade perdida deixa a imagem **permanentemente** corrompida, porque
cada delta é relativo ao anterior e não existe forma de saber que a
base se perdeu.

> **Isto não está cumprido, e a medição de F9 mostrou porquê.** Uma
> keyframe «frame completo» são todas as telhas, e isso **não cabe num
> envelope** em lado nenhum com largura de ecrã: a 640×480 são 6,3×
> acima do `MAX_PLAINTEXT`. Ver §3.6, que traz os números e as três
> saídas. O `base_seq`, o pedido de keyframe e a detecção de perda estão
> implementados e testados; o que falta é a **keyframe em si**, que
> precisa de uma decisão de arquitectura antes de ser escrita.

#### 8.5.1 Onde vive o `base_seq`, e porque é declarado

No **corpo** da unidade de streaming, e não no preâmbulo: o preâmbulo é
de largura fixa e partilhado com o não-streaming (§2.2).

```text
corpo de uma unidade de vídeo = [base_seq: u32 BE][delta]
```

Quatro bytes por unidade, que é 5 % do orçamento de §3.4. E o `base_seq`
**nunca falta**, nem num `HOLD`: é ele que diz ao receptor que a cadeia
continua.

O receptor **poderia** detectar a perda contando o `seq`. Declarar é
melhor por duas razões:

1. Uma **keyframe** muda a referência sem seguir a cadeia. Sem
   `base_seq`, o receptor teria de adivinhar que a unidade `N` é
   auto-suficiente — e adivinhar é o que produz uma imagem corrompida que
   ninguém percebe.
2. Um erro de numeração no emissor passaria despercebido na detecção por
   lacuna, e apareceria como «o ecrã do meu interlocutor está a tremer».

#### 8.5.2 A invariante

```text
base_seq == seq  →  a unidade é auto-suficiente (keyframes)
```

Uma keyframe é o delta contra um quadro **todo a zero**, com o próprio
`seq` como base. O receptor aplica-a sem condição e passa a esperar por
`seq + 1`. Não há instrução separada: a auto-suficiência **é** a
instrução, e é verificável sem o receptor saber a regra.

#### 8.5.3 Os três gatilhos

Todos locais e todos gratuitos:

1. **Um descarte no anel** (§8.2) — já sabemos que perdemos.
2. **Pedido explícito do receptor** — uma **mensagem de texto**, pela
   mesma razão que a retoma de §4.3: atravessa o relay sem canal novo.
3. **A cadência** — 2 s desde a última keyframe espontânea.

```text
9f3a: keyframe na stream 7
```

E o pedido **não leva** o `seq` do receptor. A primeira versão mandava, e
escrevia `0` tanto para «ainda não apliquei nada» como para «apliquei a
unidade 0» — o round-trip perdia a distinção, e um campo que se perde ao
ir e voltar é um campo que não está. O emissor não o usa: a decisão é
«manda uma keyframe», não «manda a partir da unidade 42».

> **O `keyframe_ms` do manifesto passou a dentro.** A primeira versão
> dizia que a cadência vinha de §2.5; a implementação tem uma constante
> de 2 s, e um campo declarado que ninguém escreve é um campo que a
> interface mostra e nada produz.

#### 8.5.4 «Limite de 1 por 2 s» — a leitura que se implementa

Os três gatilhos mandam uma keyframe **já**, e a cadência é o piso das
que não têm causa.

A leitura alternativa — um limite absoluto de 1 por 2 s — faria uma
keyframe pedida por um descarte esperar até 2 s, e a imagem ficava
corrompida esse tempo todo. §8.5 diz que isto «é o que torna o modo
streaming recuperável», e uma recuperação que demora 2 s não é
recuperação: é um congelamento diferente.

E uma keyframe **por descarte** não conta para a cadência. Contar faria
a seguinte esperar mais 2 s — depois de uma perda, que é exactamente
quando a segunda perda dói mais.

#### 8.5.5 O teste que justifica o módulo

`o_emissor_e_o_receptor_fecham_o_ciclo_com_uma_perda`: três segundos de
media a 30 fps, a **segunda** unidade a perder-se no caminho, e a
confirmação de que o receptor

* **descarta** as unidades seguintes até uma keyframe — e o teste falha
  se alguma delas for aplicada sem ser keyframe, porque isso seria
  aplicar bytes contra uma base que não tem;
* volta a sincronizar;
* fica com **o mesmo ecrã** que o emissor.

Três segundos de propósito: com menos, não há cadência depois da perda
— e quem manda a keyframe é quem sabe que perdeu, e no caminho de rede
ninguém sabe.

---

## 8.6 A regra dos três cenários, e o sentido do atraso

```text
atraso = video_ms − audio_ms
```

`atraso > 0` é o **vídeo à frente do áudio**, e é o que se **descarta**:
um quadro que já vai 200 ms à frente do áudio está a antecipar o que o
áudio ainda não disse. `atraso < 0` é o vídeo atrasado, e apresenta-se
**já**.

A confusão entre os dois sentidos é fácil e o primeiro conjunto de testes
caiu nela. Vale a pena deixar escrito: quem **atrasa** é o áudio, que
chega tarde; o vídeo, que não pode atrasar-se a si próprio, passa à
frente dele.

E o descarte **avança o relógio de vídeo** em §5.5. Sem esse segundo
passo, o vídeo fica cada vez mais à frente e cada frame seguinte também
é descartado — uma imagem morta em vez de uma que volta.

### 8.7 O que o relógio **não** tem

Nem `sleep`, nem `Instant`, nem `Duration`. `media/relogio.rs` é
aritmética sobre números, e isso é o que torna cada propriedade de §5
testável sem nenhum ecrã.

O teste que o fixa lê o **ficheiro** e falha se encontrar
`thread::sleep` ou `Instant::now` no código — a mesma técnica do teste
do `id` em §4.6.2. O que não se pode afirmar por execução, afirma-se por
leitura da fonte.

E o resto do módulo precisa do resto em `media.rs`:
`RelogioMedia::ms_por_frame` arredonda `1000 / fps` para **cima**, e um
`fps` de 2000 dá `0` em divisão inteira. A resposta é 1: um frame que não
avança o relógio é um frame que o relógio não consegue ordenar.

## 9. Propriedades verificáveis

Cada uma destas é uma afirmação que um teste pode confirmar ou refutar.

| # | Propriedade | Onde se verifica |
| --- | --- | --- |
| P1 | Um envelope de media tem a **mesma forma** que um de texto: `101+N` opaco | teste estrutural sobre o envelope |
| P2 | O tipo não é recuperável do envelope sem as chaves | teste de negação |
| P3 | Nenhum tipo de frame novo na rede | `p2p.rs` sem alterações |
| P4 | O plaintext do `ENCODE` é UTF-8 válido | `ipc.rs` já o exige; teste |
| P5 | `moldura(base64(decodificar(base64(x)))) == x`, para todo o `x` | property test |
| P6 | A soma das fatias monta exactamente o `sha256` do manifesto | teste de transferência |
| P7 | Uma transferência incompleta **nunca** chega a ser reproduzida | teste |
| P8 | O binário não embebido não tem `MEDIA_INICIAR` funcional, e **o texto não é afectado** | teste de modo degradado |
| P9 | O anel nunca ultrapassa o orçamento em microssegundos | property test |
| P10 | Descarte de áudio é impossível por construção | teste de invariante |

---

## 10. Estado

Este documento é **normativo** e `PLANEADO`. As fases constam de
`docs/roadmap.md`.

### 10.1 O que está medido, e o que a medição mudou

`network/daemon_rust/examples/medir_media.rs` corre o encoder real com
um ecrã sintético de gradiente e mede tamanho de unidade, taxa de
mensagens e velocidade de codificar/reconstruir. Foi ele que reescreveu
§3.4 e que produziu o achado de §3.6.

| | |
| --- | --- |
| Codificar | 0,188 ms por quadro de 640×480 |
| Reconstruir | 0,182 ms por quadro |
| Tecto de uma unidade | 189 telhas (~256×176) |
| Taxa a 30 fps | 15 msg/s parado; 29,9 msg/s acima do limiar |

Duas conclusões, e as duas contrariam o que este documento dizia antes:

1. **A keyframe não cabe** em lado nenhum com largura de ecrã (§3.6).
   O código recusa a geometria à construção em vez de gerar um envelope
   que o `ENCODE` rejeitaria a meio.
2. **A taxa não cabe** a 30 fps, mesmo na menor geometria (§3.4.2). Não
   há percentagem de alteração a 30 fps dentro do limite de 12,8 msg/s.

O encoder e a máquina estão 100× fora do problema. O problema é o
tamanho da mensagem contra `MAX_PLAINTEXT`, e **é uma decisão de
arquitectura, não de afinação**.

### 10.2 O que está implementado

Tudo o que não depende das duas decisões acima: derivação, moldura,
manifesto e fracção (§4), relógio, telhas, anel, coalescer, ressincronização,
`base_seq`, detecção e pedido de keyframe, `MAX_TELHAS_POR_UNIDADE` e a
recusa de geometria.

### 10.3 O circuito, ponta a ponta

```text
ffmpeg (VP8, 480p, 30 fps)
  → MEDIA_LER 0x11            o daemon lê um quadro do IVF
  → media_ler()              Python: seq ‖ clock_ms ‖ VP8
  → EmissorStreaming         moldura `streaming`, base-64
  → ENCODE + ENVIAR          cifrado, com origem `media`
  → [a rede, indistinguível de conversa]
  → DECODE                   o receptor descifra
  → ReceptorStreaming        separa texto, áudio e vídeo
  → ChamadaRede              o laço que lê a ligação e separa
  → VideoDecoder             WebCodecs descodifica
  → canvas                   a imagem
```

Nenhum passo disto é um caminho novo para a rede: o vídeo é um `CHAT`
como o texto, e o que o torna vídeo é estar comprimido.

| peça | onde | estado |
| --- | --- | --- |
| `ffmpeg` com VP8 a 480p | `tools/ffmpeg/build.sh` | feito, medido |
| ler o IVF do pipe | `media/ivf.rs` | feito, 14 testes |
| `MEDIA_CAPTURAR` / `MEDIA_LER` | `ipc.rs` | feito, testes IPC |
| `Captura` com `video_kbps` | `media/ffmpeg.rs` | feito |
| `media_ler()` | `ipc_client.py` | feito |
| `EmissorStreaming` | `media.py` | feito, 3 testes |
| `ReceptorStreaming` | `media.py` | feito, 4 testes |
| `ChamadaRede` (o laço) | `media.py` | feito, 4 testes |
| `MEDIA_AUDIO` + `media_audio()` | `ipc.rs`, `ipc_client.py` | feito |
| `LimitadorAudio` (a §8.0) | `media.py` | feito, 6 testes |
| `um_audio()` / `um_audio_distinto()` | `media.py` | feito |
| `AudioChamada` (`WebCodecs`) | `UI/…/AudioChamada.js` | feito |
| `readAudioFrame` no adaptador e na rota | `bridge-adapter.js`, `rotas.py` | feito, 4 testes |
| `VideoChamada` (`WebCodecs`) | `UI/…/VideoChamada.js` | feito |
| `Chamada` (vídeo + áudio, um relógio) | `UI/…/Chamada.jsx` | feito |
| três métodos nos dois adaptadores | `bridge-adapter.js` + `mock-adapter.js` | feito |
| três rotas de streaming | `server/rotas.py` | feito, 8 testes |

### 10.3.1 O áudio não é o vídeo com outro nome

Porque o sintoma de um atraso é **invertido**. Um quadro de vídeo
atrasado mostra-se com dois quadros de detras, e é quase
imperceptível a 30 fps. Um pacote de áudio atrasado é voz que chega
tarde, e quem está a ouvir percebe antes de ver.

Daí as duas decisões do `AudioChamada`:

| | vídeo | áudio |
| --- | --- | --- |
| fila | até ao `dequeue` | **três pacotes** |
| atraso | `optimizeForLatency` | `playbackLatency` de 60 ms |
| quando está atrasado | mostra mesmo assim | **descarta o antigo** |

Os 60 ms não são um ajuste: é o menor atraso que o `AudioContext` pode
dar sem estalar, e é por isso que uma chamada tem 10 a 20 ms de atraso
que **ninguém pode tirar**. É o browser a poder começar a mixar.

E o descarte é sempre do **antigo**, nunca do recente — descartar o
recente tirava a voz a quem está a falar *agora*, e isso ouve-se.

#### 10.3.1.1 O gesto, e porque não há atalho

O browser não deixa criar um `AudioContext` fora de um gesto do
utilizador. Sem gesto, o contexto nasce `suspended`, o `decode()` corre
e não produz som, **sem erro nenhum** — e o sintoma é uma chamada em
que se vê a imagem e não se ouve a voz, que é a falha mais confusa que
uma chamada pode ter.

Não há `resume()` que resolva: `resume()` também precisa de um gesto.
E não há polyfill. A decisão é do browser e é a única forma de a
respectar sem mentir ao utilizador sobre o que está a acontecer.

Por isso o botão «Ligar chamada» do `Chamada.jsx` **é** o gesto, e o
áudio é acordado **antes** da captura: se o browser recusar, nenhum
`ffmpeg` é lançado, porque um `ffmpeg` a ler o ecrã sem ninguém a
ouvir é pior do que nenhum.

#### 10.3.1.2 O que a chamada não faz

* **Cancelamento de eco.** Numa chamada entre duas máquinas há o eco
  da voz que acabou de falar, e o `echoCancellation` do
  `getUserMedia` trata o do **microfone** — o da linha não. Isso é um
  filtro de eco por referência e não cabe aqui.
* **Sincronização labial.** O `VideoDecoder` e o `AudioContext` têm
  relógios independentes, e alinhar os dois é trabalho a sério.
* **Escolher quem chama.** Isto não é uma centralita.

### 10.3.2 A sondagem é o que faz o sistema funcionar

Numa chamada o mesmo `stream` leva três coisas ao mesmo tempo, e elas
chegam **misturadas** — cada uma pelo `ENVIAR`, que é `CHAT` como o
texto:

```text
"olá"        texto
<opus>       áudio, 10 msg/s
<vp8>        vídeo, 30 msg/s
```

Quem separa é quem **descifra**: a K1 de cada tipo é derivada com o
tipo no `info` do HKDF, e por isso só a chave certa abre o envelope
certo. A ordem é texto, áudio, vídeo — e o texto primeiro porque é o
caso comum, o que faz uma conversa normal custar **uma** verificação.

Sem isto um quadro de vídeo seria testado com a chave de texto,
falharia na Poly1305, e a tentativa seguinte seria com a de áudio: três
verificações por quadro, a 30 fps, e a possibilidade de uma delas
abrir o envelope errado.

E o que a sondagem **não** engole: uma falha de rede sobe. Só o
`0x05 DecifragemFalhou` é «esta chave não bate»; uma rede que caiu é
uma falha, e engoli-la seria tentar as três chaves com o daemon
avariado.


* **O `clock_ms` do áudio na rede.** O pacote de Opus **é** o relógio
  (§5.3) e a rota não lhe carimba nada — mas o receptor precisa de
  alguma coisa com que ordenar o áudio no tempo, e hoje só o vídeo
  tem. Sem isso, o áudio sabe quando chegou e não sabe a que instante
  do vídeo pertence.
* **O `AudioContext` dentro de um gesto.** Resolvido no `Chamada.jsx`:
  o botão «Ligar chamada» é o gesto, e o áudio é acordado **antes** da
  captura — se o browser recusar, nenhum `ffmpeg` é lançado, porque um
  `ffmpeg` a ler o ecrã sem ninguém a ouvir é pior do que nenhum.
* **Ligar a chamada a uma amizade.** O `ChamadaRede` é o laço de
  **quem ouve**: recebe um `Frame`, separa, e devolve o que é
  vídeo. Falta o simétrico de quem **emite** — o objecto que
  decide, quando um `ENVIAR` é uma chamada e quando é conversa.

O que **não** está decidido e é proibido descrever como decidido:

* Negociação de qualidade adaptativa pelo receptor (`QSET`) — investigada,
  não especificada.
* Codecs de vídeo além de VP8 no streaming.
* Streaming através do relay — a semântica store-and-forward de
  `docs/relay.md` é estruturalmente incompatível com tempo real, e nada
  aqui a altera.
* `MEDIA_PARAR` — não existe, e §8.1.2 diz porquê.
