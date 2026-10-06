# Pipeline K1 → K9

## Introdução

Este documento é a **especificação normativa** do pipeline criptográfico
do OnyxChat. Define, para cada uma das nove camadas, a entrada, a chave
ou parâmetro, a operação, a saída, a função inversa, os erros, os
exemplos e os limites de tamanho — de forma que qualquer implementação
(Rust, C/C++, Lua, Python) produza exatamente os mesmos bytes.

Duas notas de enquadramento, antes das camadas:

* **Defesa em profundidade, não nove vezes mais segurança.** As nove
  camadas não se somam. O objetivo é que a observação ou comprometimento
  de uma parte do transporte não resulte automaticamente em plaintext, e
  que um atacante que obtenha uma camada intermédia continue perante as
  restantes transformações. A segurança prática do sistema resulta da
  *composição correta* + *separação de chaves* + *autenticação* +
  *proteção de transporte* + *implementação correta*.
* **Escopo atual: texto.** O pipeline opera sobre bytes UTF-8
  (`plaintext` textual). Imagens, áudio, vídeo e ficheiros arbitrários
  estão fora do escopo atual — decisão de âmbito, não deficiência. O
  pipeline não é um sistema universal de content-type.

### Parâmetros fixos

```text
K2  Cesariana ............ deslocamento +3
K3  Vigenère ............. chave "VIGENERE" (8 bytes ASCII)
K4  Transposição ......... 5 colunas, PKCS#7 com bloco de 5
K6  Playfair Adaptado .... chave "PLAYFAIR" (8 bytes ASCII), XOR
K7  Hill ................. matriz [[3,3],[2,5]] mod 256, PKCS#7 bloco 2
K8  Substituição ......... deslocamento +7
```

Estes valores estão codificados em `network/daemon_rust/src/pipeline.rs`
(`DESLOCAMENTO_K2`, `CHAVE_K3`, `CHAVE_K6`, `DESLOCAMENTO_K8`) e devem
coincidir com as referências em `crypto/python_lua/`.

> **K2 e K8 são a mesma função.** As duas são `b' = (b + k) mod 256`, com
> `k` = +3 e +7. Implementam o mesmo módulo Lua (`lua/deslocamento.lua`)
> e chamam a mesma função (`lua_camadas::deslocamento`); o que as
> distingue é o deslocamento, que é um parâmetro, não uma
> implementação. Tiveram ficheiros Lua separados até à fase G6 — com o
> corpo idêntico — e a divergência silenciosa entre os dois era o risco
> que a fase eliminou. As camadas continuam a ser distintas no
> **protocolo**: K2 e K8 ocupam posições diferentes na sequência
> K1→K9, e é isso que as define.

---

## Criptografia analógica

O OnyxChat usa o termo **criptografia analógica** com um significado
próprio e restrito:

> No contexto do OnyxChat, *criptografia analógica* refere-se a técnicas
> criptográficas ou de transformação historicamente concebidas para
> aplicação manual ou não dependente de sistemas computacionais modernos.
> O termo descreve a origem e a natureza *conceptual* da técnica, e
> **não** o formato físico dos dados durante a execução.

Em execução, **todos os dados do pipeline são digitais** (bytes). Nenhuma
camada "deixa de ser digital" por ser analógica de origem.

### Classificação das camadas

| Tipo        | Camadas                    | Algoritmo                                                    |
| ----------- | -------------------------- | ------------------------------------------------------------ |
| Analógicas  | K2, K3, K6, K8             | César, Vigenère, Playfair Adaptado Onyx, substituição         |
| Digitais    | K1, K4, K5, K7, K9         | ChaCha20-Poly1305, transposição, AES-256-GCM, Hill, ChaCha20-Poly1305 |

K8 é uma adição modular byte-a-byte e classifica-se como **analógica**
(por origem: deslocamento clássico). K4 é uma transposição — técnica
clássica — mas está implementada em C++ sobre bytes e é classificada
como **digital** por ser processamento computacional de baixo nível sem
equivalente manual prático no domínio binário. Esta fronteira é uma
*organização conceptual do projecto*, não uma fronteira criptográfica.

---

## Fluxo geral

**Cifragem (ENCODE), sempre nesta ordem:**

```text
plaintext (UTF-8)
   │
   ▼  K1  ChaCha20-Poly1305   chave do par     → ciphertext1 ‖ tag1
   ▼  K2  César +3            (Lua)            → ciphertext2
   ▼  K3  Vigenère            (Lua)            → ciphertext3
   ▼  K4  Transposição 5 col.  PKCS#7/5 (C++)  → ciphertext4
   ▼  K5  AES-256-GCM         chave remetente  → ciphertext5 ‖ tag5
   ▼  K6  Playfair Adaptado   (Lua)            → ciphertext6
   ▼  K7  Hill 2×2 mod 256    PKCS#7/2 (Rust)  → ciphertext7
   ▼  K8  Substituição +7     (Lua)            → ciphertext8
   ▼  K9  ChaCha20-Poly1305   chave receptor   → ciphertext9 ‖ tag9
   │
   ▼  envelope + assinatura Ed25519  →  message_format.md
```

**Decifragem (DECODE):** exatamente inverso, na ordem K9 → K1, com a
assinatura Ed25519 verificada **antes** de qualquer operação de
decifragem (ver `message_format.md` §Assinatura).

### AAD (dados associados)

**Todas as camadas AEAD do projeto (K1, K5, K9) usam AAD vazio.**

Isto está fixado na implementação — `crypto_aead_chacha20poly1305_ietf_encrypt(
…, ad=NULL, adlen=0, …)` em C, e `cipher.encrypt(n, mensagem)` (sem AAD)
em Rust. A autenticação de contexto (versão, nonces, identidade) não é
feita pelas camadas AEAD, mas sim:

* pela **assinatura Ed25519 do envelope** (cobre versão ‖ nonces ‖
  ciphertext), e
* pelo **transcript do handshake** (cobre identidades e versão).

Alterar isto exigeria nova versão de envelope/pipeline.

### Tags AEAD

O output de K1, K5 e K9 é `ciphertext ‖ tag(16 bytes)`, produzido pela
primitiva AEAD. **O tag faz parte do fluxo de dados**: a camada seguinte
cifra o resultado completo (incluindo o tag). É por isto que o tamanho
cresce em +16 bytes em cada uma destas camadas.

### Limites de tamanho

| Limite                | Valor            | Onde é aplicado                  |
| --------------------- | ---------------- | -------------------------------- |
| `MAX_PLAINTEXT`       | 65 536 bytes     | antes de K1 (IPC `ENCODE`)       |
| `MAX_ENVELOPE`        | 65 692 bytes     | parsing do envelope (receptor)   |
| `MIN_PLAINTEXT`       | **1 byte**       | antes de K1 |

> **`MIN_PLAINTEXT` — `IMPLEMENTADO`.** O pipeline rejeita plaintext
> vazio **à entrada**, antes de K1, com `PayloadVazio` → IPC `0x02`.
>
> O código é `0x02` e não `0x09` de propósito: `0x09` diz «excede o
> limite» e `0x02` diz «não é um pedido válido». Um cliente que receba
> `0x09` para um texto vazio não consegue corrigir-se; um que receba
> `0x02` sabe que tem de escrever alguma coisa.

Os valores derivam do formato. O crescimento **máximo** do plaintext
até ao envelope final é:

```text
K1   AEAD tag ........................ +16
K4   PKCS#7 bloco 5 (máx. 1..5) ......  +5
K5   AEAD tag ........................ +16
K7   PKCS#7 bloco 2 (máx. 1..2) ......  +2
K9   AEAD tag ........................ +16
Envelope (cabeçalho 37 + assinatura 64) +101
                                    ----------
                          sobretotal = 156 bytes

MAX_ENVELOPE = 65 536 + 156 = 65 692 bytes
```

Mensagens acima do limite são rejeitadas **antes** de processamento
(ver `docs/message_format.md` §Limites e `docs/security_model.md`).

---

## Especificação por camada

Cada camada é especificada com os mesmos nove campos. Em todos os
casos, `entrada` e `saida` são `bytes` arbitrários (0x00–0xFF).

### K1 — ChaCha20-Poly1305 (Rust)

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | `plaintext` (UTF-8)                                          |
| Chave          | `k1` de 32 bytes — chave única do par (remetente + receptor) |
| Parâmetro      | `nonce1` de 12 bytes, aleatório por mensagem (CSPRNG)        |
| Operação       | AEAD ChaCha20-Poly1305, **AAD vazio**                        |
| Saída          | `ciphertext1 ‖ tag1` = `len(entrada) + 16` bytes             |
| Inversa        | `K1⁻¹` = AEAD decrypt com a mesma chave e `nonce1`            |
| Erros          | `TamanhoDeChaveInvalido`, `TamanhoDeNonceInvalido`, `DecifragemFalhou` |
| Exemplo        | `docs/test_vectors.md` → vector K1                           |
| Limite         | `len(entrada) ≤ MAX_PLAINTEXT`                               |

### K2 — César (Lua)

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | output de K1                                                 |
| Chave          | — (parâmetro público fixo)                                   |
| Parâmetro      | `deslocamento = +3`                                          |
| Operação       | `b' = (b + 3) mod 256` para cada byte                        |
| Saída          | mesmo comprimento da entrada                                 |
| Inversa        | `b = (b' − 3) mod 256` (`decifrar`, sinal negativo)          |
| Erros          | `ErroPipeline::Lua` apenas se o interpretador falhar; entrada vazia → saída vazia |
| Exemplo        | `"ABCDE"` +3 → `"DEFGH"`                                     |
| Limite         | sem limite próprio (herda o do pipeline)                     |

### K3 — Vigenère (Lua)

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | output de K2                                                 |
| Chave          | `"VIGENERE"` (8 bytes ASCII)                                 |
| Parâmetro      | índice periódico `i mod 8`                                   |
| Operação       | `b' = (b + chave[i mod 8]) mod 256`                          |
| Saída          | mesmo comprimento da entrada                                 |
| Inversa        | `b = (b' − chave[i mod 8]) mod 256`                          |
| Erros          | chave vazia → `ErroPipeline::Lua` com mensagem fixa          |
| Exemplo        | `"Hello!"` com `"VIGENERE"` → `9eaeb3b1bd66`                 |
| Limite         | herda o do pipeline                                          |

### K4 — Transposição de 5 colunas (C++)

**Ordem exata das operações (normativa):**

```text
plaintext
   │  1) PKCS#7 com bloco de 5   ← ANTES da transposição
   ▼
buffer com pad
   │  2) transposição (escrita linha-a-linha, leitura coluna-a-coluna)
   ▼
ciphertext4
```

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | output de K3                                                 |
| Chave          | — (estrutura fixa)                                           |
| Parâmetro      | `5 colunas`                                                  |
| Operação       | ver fórmulas abaixo                                          |
| Saída          | sempre múltiplo de 5 e **estrictamente maior** que a entrada (PKCS#7 acrescenta 1..5 bytes, mesmo quando `len % 5 == 0`) |
| Inversa        | leitura inversa + remoção do PKCS#7                          |
| Erros          | `ONYX_ERR_ARG` (ponteiro nulo), `ONYX_ERR_CAP` (buffer), `ONYX_ERR_TAMANHO` (entrada ≠ múltiplo de 5 ou vazia), `ONYX_ERR_PAD` (padding inválido) |
| Exemplo        | `docs/test_vectors.md` → vector K4                           |
| Limite         | herda o do pipeline                                          |

**Fórmulas (normativas):**

```text
total   = ⌊len/5⌋+1 × 5          # PKCS#7: acrescenta 1..5 bytes
pad     = total − len             # valor do byte de padding (1..5)
linhas  = total / 5

# Cifragem — escrita coluna-a-coluna a partir do buffer com pad:
#   para cada coluna c ∈ [0,5), para cada linha l ∈ [0,linhas):
saida[c · linhas + l] = buffer[l · 5 + c]

# Decifragem — inverso:
buffer[l · 5 + c] = entrada[c · linhas + l]      # para c, l
depois: remove os últimos `pad` bytes (validando que todos == pad)
```

*Comprimento 0:* a cifragem **rejeita** entrada vazia
(`ONYX_ERR_TAMANHO`) — não existe sentido em transpor nada, e o
resultado seria um bloco de 5 bytes de padding puro que a decifragem
aceitaria como mensagem vazia. A decifragem rejeita igualmente
`len == 0`.

> **Estado: `IMPLEMENTADO`.** A cifragem devolve `ONYX_ERR_TAMANHO`
> para `len == 0`, e `onyx_k4_encrypt_len(0)` devolve **0** como sinal
> (a função não tem canal de erro, e 0 nunca é uma capacidade válida).
>
> **Onde está cada defesa.** K4 **aceita** decifrar 5 bytes de padding
> puro como plaintext vazio — isso é PKCS#7 correcto e obrigatório; se
> recusasse, não conseguiria decifrar nada cuja última camada tivesse
> escolhido o bloco completo. A rejeição do plaintext vazio está no
> **pipeline**. Funciona porque a ordem das camadas protecte: a saída de
> K4 só existe depois de K1, que é AEAD, logo um atacante sem as chaves
> não consegue injectir bytes em K4 sem forjar a tag. E um emissor
> legítimo já não cria a mensagem vazia, porque o pipeline a recusa.

*Comprimento ímpar/comoquer:* não há restrição de paridade em K4; o
padding PKCS#7 trata qualquer comprimento.

### K5 — AES-256-GCM (Rust)

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | output de K4                                                 |
| Chave          | `k5` de 32 bytes — chave única do **remetente**              |
| Parâmetro      | `nonce5` de 12 bytes, aleatório por mensagem                 |
| Operação       | AEAD AES-256-GCM, **AAD vazio**                              |
| Saída          | `ciphertext5 ‖ tag5` = `len(entrada) + 16` bytes             |
| Inversa        | AEAD decrypt com `nonce5`                                    |
| Erros          | `TamanhoDeChaveInvalido`, `TamanhoDeNonceInvalido`, `DecifragemFalhou` |
| Exemplo        | `docs/test_vectors.md` → vector K5                           |
| Limite         | herda o do pipeline                                          |

### K6 — Playfair Adaptado Onyx (Lua)

**Designação:** *Playfair Adaptado Onyx*. É uma **variante própria** do
projeto, **não** a cifra de Playfair histórica. Não existe grelha 5×5,
não existem bigramas, não existe digrama `XX`, não existem regras de
linha/coluna. O nome "adaptado" é obrigatório em qualquer referência.

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | output de K5                                                 |
| Chave          | `"PLAYFAIR"` (8 bytes ASCII)                                 |
| Parâmetro      | —                                                            |
| Operação       | `b' = b XOR chave[i mod 8]` (XOR periódico sobre todo o buffer) |
| Saída          | mesmo comprimento da entrada                                 |
| Inversa        | idêntica à cifragem — XOR é involutivo (`decifrar` = `cifrar`) |
| Erros          | chave vazia → `ErroPipeline::Lua` com mensagem fixa          |
| Exemplo        | `"AB"` com `"PLAYFAIR"` → `110e`                             |
| Limite         | herda o do pipeline                                          |

**Preparação do texto:** **nenhuma**. Não há normalização, minúsculas,
remoção de espaços nem preenchimento. **Agrupamento:** **nenhum** — o
algoritmo opera byte-a-byte, não sobre pares.

### K7 — Hill 2×2 em Z/256Z (Rust)

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | output de K6                                                 |
| Chave          | matriz fixa `[[3,3],[2,5]]`                                  |
| Parâmetro      | módulo `256`, bloco de `2` bytes, PKCS#7 com bloco de 2      |
| Operação       | ver fórmulas abaixo                                          |
| Saída          | sempre comprimento par; amplia 1..2 bytes (PKCS#7)           |
| Inversa        | multiplicação por `[[29,85],[142,171]]` + remoção do padding  |
| Erros          | `BlocoIncompleto` (comprimento ímpar no input da decifragem), `PaddingInvalido` (n∉1..2, bytes inconsistentes, buffer vazio) |
| Exemplo        | `"AB"` → `89cc0c0e`                                          |
| Limite         | herda o do pipeline                                          |

**Invertibilidade (documentada e verificada):**

```text
matriz M  = [[3, 3], [2, 5]]
det(M)    = 3×5 − 3×2 = 9
gcd(9, 256) = 1   →  det invertível mod 256
9⁻¹ mod 256 = 57
M⁻¹       = 57 × adj(M) mod 256 = [[29, 85], [142, 171]]
```

**Fórmulas (normativas), por bloco `[a, b]`:**

```text
cifra:   [3a + 3b,  2a + 5b]  mod 256
decifra: [29a + 85b, 142a + 171b] mod 256
```

**Comprimento ímpar:** a cifragem aplica PKCS#7 de bloco 2 *antes* da
multiplicação, pelo que o buffer fica sempre par (o padding resolve o
caso ímpar). A decifragem **rejeita** comprimento ímpar com
`BlocoIncompleto` — é matematicamente impossível que uma saída válida
de K7 tenha comprimento ímpar.

```text
pad_pkcs7(n): falta = 2 − (n mod 2)      # 1..2 bytes, sempre
              n = 0  → acrescenta [2, 2]  # bloco inteiro
              n par  → acrescenta [2, 2]
              n ímpar→ acrescenta [1]
```

### K8 — Substituição (Lua)

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | output de K7                                                 |
| Chave          | — (parâmetro público fixo)                                   |
| Parâmetro      | `deslocamento = +7`                                          |
| Operação       | `b' = (b + 7) mod 256`                                       |
| Saída          | mesmo comprimento da entrada                                 |
| Inversa        | `b = (b' − 7) mod 256`                                       |
| Erros          | nenhum próprio (transformação total)                         |
| Exemplo        | `"AB"` +7 → `"HI"`                                           |
| Limite         | herda o do pipeline                                          |

A adição modular é bijetiva para qualquer deslocamento: cada byte tem
exatamente uma imagem e uma pré-imagem.

### K9 — ChaCha20-Poly1305 (C/libsodium)

| Campo          | Definição                                                    |
| -------------- | ------------------------------------------------------------ |
| Entrada        | output de K8                                                 |
| Chave          | `k9` de 32 bytes — chave única do **receptor**               |
| Parâmetro      | `nonce9` de 12 bytes, aleatório por mensagem                 |
| Operação       | AEAD ChaCha20-Poly1305 (libsodium), **AAD vazio**            |
| Saída          | `ciphertext9 ‖ tag9` = `len(entrada) + 16` bytes — é o `N` do envelope |
| Inversa        | AEAD decrypt com `nonce9` (falha com `ONYX_ERR_AUTH` se a tag não conferir) |
| Erros          | `ONYX_ERR_ARG`, `ONYX_ERR_TAMANHO` (`len < 16`), `ONYX_ERR_AUTH` |
| Exemplo        | `docs/test_vectors.md` → vector K9                           |
| Limite         | herda o do pipeline                                          |

**Duas implementações:** a de **produção** é a C/libsodium (usada pelo
daemon via FFI); a em Rust (`crypto_core::aead_chacha`) é o **espelho de
referência**, usado para validação e testes cruzados. São duas funções
distintas com papéis distintos — não são opções intermutáveis em
produção. Ver `docs/testing.md`.

---

## Assinaturas digitais (fora do pipeline)

* **Algoritmo:** Ed25519, assinatura de 64 bytes.
* **Posição:** bloco final do envelope.
* **Região assinada:** `versão ‖ nonce1 ‖ nonce5 ‖ nonce9 ‖ ciphertext K9`
  (todos os bytes anteriores à assinatura).
* **Ordem de operação no receptor:** validar formato → **verificar
  assinatura** → só então iniciar a decifragem K9⁻¹ → … → K1⁻¹.
* A assinatura **não** cifra nada e **não** faz parte das nove camadas;
  dá autenticidade e origem.

---

## Erros por camada

| Camada | Erro                                  | Semântica                         |
| ------ | ------------------------------------- | --------------------------------- |
| K1/K5  | `TamanhoDeChaveInvalido`              | chave ≠ 32 bytes                 |
| K1/K5  | `TamanhoDeNonceInvalido`              | nonce ≠ 12 bytes                 |
| K1/K5  | `DecifragemFalhou`                    | tag AEAD inválida                |
| K2/K3/K6/K8 | `ErroPipeline::Lua`             | falha do interpretador (mensagem fixa) |
| K3/K6  | `Lua("…chave inválida…")`             | chave vazia rejeitada pelo script |
| K4     | `ONYX_ERR_ARG` / `ONYX_ERR_CAP`       | argumento/buffer inválidos        |
| K4     | `ONYX_ERR_TAMANHO`                    | entrada vazia ou ≠ múltiplo de 5 (decifragem: `IMPLEMENTADO` · cifragem: `PLANEADO` F2.2) |
| K4     | `ONYX_ERR_PAD`                        | PKCS#7 inválido na decifragem     |
| K7     | `BlocoIncompleto`                     | comprimento ímpar                 |
| K7     | `PaddingInvalido`                     | PKCS#7 inválido ou ausente        |
| K9     | `ONYX_ERR_AUTH`                       | tag Poly1305 inválida             |
| global | `EnvelopeInvalido`                    | envelope curto / versão desconhecida |
| global | `AssinaturaInvalida`                  | Ed25519 falha (antes de decifrar) |
| global | `TextoInvalidoUtf8`                   | K1 devolveu bytes que não são UTF-8 |

Nenhuma mensagem de erro contém chaves, nonces, plaintext ou conteúdo
de mensagens — política de erros definida em `docs/security_model.md`.

---

## Test vectors

Os exemplos reproduzíveis (chaves, nonces e outputs completos por
camada) estão em [`test_vectors.md`](test_vectors.md), com os dados
máquina em `tests/vectors/*.json`, consumidos por **todas** as
implementações (Rust, C, Lua, Python). O exemplo histórico com chaves
truncadas foi removido por não ser verificável.

---

## Observações finais

* Nonces são aleatórios por mensagem, gerados unicamente pelo CSPRNG —
  nunca por contador persistente e nunca reutilizados.
* A assinatura é sempre verificada antes da decifragem.
* Logging de erros apenas com metadados (códigos e tamanhos), nunca com
  payloads.
* Os testes obrigam: tags inválidas, padding incorreto, assinaturas
  falsas, versão adulterada, truncagem — todos com **rejeição segura**
  (nunca crash nem plaintext parcial).
