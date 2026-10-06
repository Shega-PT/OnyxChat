# Gestão de Chaves (Key Management)

## Princípio

> As chaves do OnyxChat **não são uma única chave global.**

Existem chaves com papéis diferentes, proprietários diferentes, ciclos
de vida diferentes. Este documento documenta, para cada uma: quem a
detém, quando é criada, quem a conhece, como chega ao outro endpoint,
como é protegida em memória e quando é destruída.

```text
Identidade
    └── Ed25519 (seed + pub)

Chaves de comunicação
    ├── K1   (par)
    ├── K5   (remetente)
    └── K9   (receptor)
```

---

## Tabela mestre

| Chave | Proprietário | Geração | Persistência | Distribuição | Utilização | Destruição |
| --- | --- | --- | --- | --- | --- | --- |
| **Ed25519 seed** | utilizador local | CSPRNG, 1× na criação da identidade | `config.keystore` cifrado, ou `config.json` se não tiver cifrado | **nunca sai do host** | assinar handshake e envelopes | apagar config |
| **Ed25519 pub** | derivada da seed | derivada | idem | handshake (`FRIEND_*`) | verificar assinaturas | — |
| **K1** | par (A+B) | CSPRNG por A no `FRIEND_REQUEST` | keystore cifrado + estado do daemon | `FRIEND_REQUEST` (assinado) | camada K1 | rotação (novo pedido) ou apagar amizade |
| **K5** | remetente (mas conhecida pelo par) | CSPRNG por cada lado | keystore cifrado | `FRIEND_*` (assinado) | camada K5 | idem |
| **K9** | receptor (mas conhecida pelo par) | CSPRNG por cada lado | keystore cifrado | `FRIEND_*` (assinado) | camada K9 | idem |
| **keystore key** | utilizador (passphrase) | PBKDF2-HMAC-SHA256 (sal aleatório) | derivada em cada desbloqueio | **nunca sai do host** | cifrar o keystore | vive em memória só durante o uso |

---

## Identidade Ed25519

* **Ciclo de vida:** criada uma única vez (`Identidade.gerar()`), usada
  para sempre (a identidade é estável — é isso que a estabiliza).
* **Pública** deriva-se da seed; a seed **nunca** trafega em claro pela
  rede e **nunca** é registada em logs.
* **No IPC:** a seed aparece como campo dos comandos `ENCODE`,
  `PEDIR_AMIZADE`, `ACEITAR_AMIZADE`, `RECUSAR_AMIZADE` e
  `CONFIRMAR_AMIZADE` — sempre dentro do UDS local autenticado por uid.
* **Armazenamento:** em `config.keystore` cifrado com PBKDF2-HMAC-SHA256
  + ChaCha20-Poly1305, com permissões `0600`. Sem `onyxchat cifrar`
  (ou `iniciar --cifrar`), fica em hex claro dentro de `config.json`
  — o `0600` cobre outro utilizador da máquina, não um backup.

## Chaves de comunicação (K1/K5/K9)

* **Geração:** CSPRNG do sistema (`crypto_core::gerar_chave`), 32 bytes
  aleatórios. As chaves do handshake **não** são derivadas umas das
  outras — são independentes por desenho, para que uma compromising
  não leve as outras.
* **Derivação da K1 por mensagem:** a K1 que entra no pipeline **pode**
  ser derivada do par (`amizade.rs`, fase I):

  ```text
  semente = SHA-256(b"ONYX/AMIZADE/v1" ‖ menor(k5ₐ, k5_b) ‖ maior(k5ₐ, k5_b))
  k1      = HKDF-SHA256(semente, salt=nonce1, info=b"ONYX/K1/v1" ‖ pub_emissor)
  ```

  A ordenação canónica das duas chaves é o que permite aos dois lados
  obterem a mesma semente; a identidade do remetente no `info` torna a
  chave direccional. Implementações em `crypto/rust/src/amizade.rs` e
  `messenger/amizade.py`, com vector congelado nas duas.

  **Sem forward secrecy** — declarada em
  [`threat_model.md`](threat_model.md) §Limitações 8. **A derivação
  ainda não está ligada ao caminho por omissão**: `ipc_client.cifrar()`
  continua a passar a K1 de longa duração. Ver `docs/roadmap.md`.
* **Distribuição:** apenas através do handshake assinado — cada chave
  que trafega está coberta pelo transcript Ed25519
  (`handshake.md` §Transcript), o que vincula a chave à identidade.
* **Conhecimento:** ao fim do handshake, **ambos os lados** conhecem
  K1, K5_A, K9_A, K5_B, K9_B — as chaves distinguem-se pelo *papel*,
  não por serem secretas em relação ao par.
* **Rotação:** um novo `FRIEND_REQUEST` da mesma `pub` **substitui**
  as chaves antigas. As chaves antigas são descartadas do estado do
  daemon nesse momento.
* **Persistência:** o cliente guarda-as em `config.keystore`, cifrado por
  `messenger/storage.py`; o daemon mantém-as em memória enquanto a
  amizade estiver ativa.

## Keystore cifrado

```text
passphrase ──PBKDF2-HMAC-SHA256(sal, iterações)──▶ chave de 32 B
                                                       │
v1 (legada): ONYXKS1          ‖ sal(16) ‖ nonce(12) ‖ AEAD(chave, nonce, JSON, aad=ONYXKS1)
v2 (actual): ONYXKS‖0x00‖0x02 ‖ sal(16) ‖ nonce(12) ‖ AEAD(chave, nonce, JSON, aad=magic‖sal)
permissões: 0600 (criação com O_TRUNC e chmod explícito)
```

* **AAD** do AEAD na v2 cobre o cabeçalho — **mágica e sal** — o que o
  distingue do AAD vazio do pipeline e impede que o sal seja substituído
  por outro mantendo a etiqueta válida. É uma melhoria real de
  integridade: na v1 o sal só ficava coberto por via indirecta (a chave
  derivada).
* **Comparação de etiquetas** sempre em tempo constante
  (`hmac.compare_digest`).
* A chave derivada **vive em memória apenas** durante a operação; não
  é persistida.

### Versionamento e leitura tolerante

O keystore é **versionado pela própria magic**, lida *antes* de qualquer
decifragem — necessário porque o AAD muda entre versões.

| Versão | Magic | AAD | Estado |
| --- | --- | --- | --- |
| 1 | `ONYXKS1` (7 B) | `mágica` | legada, só leitura |
| 2 | `ONYXKS` ‖ `0x00` ‖ `0x02` (8 B) | `mágica ‖ sal` | actual |

**Regra de leitura:** qualquer versão conhecida é aceite; a escrita usa
sempre a mais recente. É o que permite a dois utilizadores em versões
diferentes do software usar o mesmo sistema — princípio formalizado em
[`index.md`](index.md).

**Migração:** não há passos manuais. Carregar e gravar é a migração
completa:

```python
dados = carregar_keystore(caminho, passphrase)   # lê v1
guardar_keystore(dados, caminho, passphrase)     # escreve v2
```

### O terminador NUL, e porque existe

A v2 não se chama `ONYXKS2` mas `ONYXKS` ‖ `0x00` ‖ `0x02`.

Sem o NUL, `ONYXKS12` (versão 12) teria `ONYXKS1` — a magic da v1 — nos
seus primeiros 7 bytes, e o ficheiro seria decifrado com o AAD errado. Um
byte NUL não pode ser um dígito de versão, logo a partir de v2 nenhuma
versão futura colide com a v1 nem com outra v2+.

**Limitação declarada:** a ambiguidade com a v1 legada é **irredutível**
para um ficheiro que siga o esquema *antigo* (`ONYXKS12`). A informação
não está no ficheiro. Se acontecer, o erro é `PassphraseErrada` em vez
de «formato desconhecido» — um problema de **diagnosticabilidade**, não de
segurança: o ficheiro não pode ser aceite, porque a etiqueta AEAD não
verifica contra um AAD diferente.

---

## Proteção em memória (zeroization)

Política normativa — aplica-se sempre que tecnicamente possível:

```text
segredo ──uso──▶ zeroização ──▶ descarte
```

| Material | Onde vive | Limpeza |
| --- | --- | --- |
| segredo de longa duração da amizade | **cliente Python** (`config.keystore`) | nunca entra no daemon |
| K1 derivada por mensagem | cliente Python, depois IPC | `drop` de Python é best-effort — ver §Limitação declarada |
| chaves K1/K5/K9 (não derivadas) | Rust daemon (`Chaves`; não é `Copy`) | `Drop` com `zeroize` |
| chaves de handshake | Rust daemon (`Pedido`, `Aceite`, `Amizade`) | `Drop` com `zeroize` |
| corpo `FRIEND_REQUEST` pendente | Rust daemon (`PedidoPendente`) | `zeroize` no descarte e na limpeza |
| seed Ed25519 | Rust daemon, cliente Python | `Segredo`/zeroize (Rust); best-effort (Python) |
| buffers de pedido/resposta IPC | Rust daemon (`atender`) | `zeroize` após o uso |
| nonces | efémeros por mensagem | não requer (públicos no envelope) |
| transcripts temporários do handshake | `Vec<u8>` locais (`regiao`) | `zeroize` após assinar/verificar |
| buffers de saída C (K4/K9) | heap, alocado pelo chamador | `onyx_limpar` (= `sodium_memzero`) nos caminhos de erro |
| keystore key (Python) | memória do processo | best-effort: `bytearray` limpo após uso |
| chaves em disco | `config.keystore` (cifrado) | PBKDF2 + ChaCha20-Poly1305 + `0600` |
| chaves em disco **sem** `cifrar` | `config.json` | hex claro + `0600` — **não** resiste a um backup do `$HOME` |
| passphrase | memória do processo | `getpass` desliga o eco; nunca em `argv` |

**Limitação declarada (Python):** o CPython não garantiria a ausência
de cópias internas em `str`/`bytes` imutáveis. A limpeza em Python é
*best-effort* e está documentada como tal — a garantia forte de
zeroização existe no **Rust** e no **C**.

**Implementação:** `crypto/rust/src/secret.rs` (`Segredo` com
`Zeroize` no `Drop`); no daemon, `Drop` com `zeroize` em
`pipeline.rs` (`Chaves`), `handshake.rs` (`Pedido`, `Aceite`,
`Amizade`) e `ipc.rs` (`PedidoPendente` + buffers de `atender`), com
as seeds guardadas em `Segredo`.

### O C tinha zeroização escrita e nenhuma feita

Até à fase F, este documento afirmava que `sodium_memzero` era chamado
em `crypto/c_cpp/k9_chacha.c`. **Não era.** Uma busca por
`memzero|memset|explicit_bzero` em `crypto/c_cpp/` não devolvia
nenhuma ocorrência — só a *declaração* no header do libsodium vendored.
A garantia estava escrita e não implementada, que é o estado mais
perigoso: um leitor que confie no documento deixa de olhar.

O que a fase F fez, e o que está implementado:

| Onde | O quê | Porquê |
| --- | --- | --- |
| `onyx_crypto.h` | declara `onyx_limpar` | a garantia passa a ser parte da API, não um comentário |
| `k9_chacha.c` | implementa `onyx_limpar` sobre `sodium_memzero` | o ficheiro que já inclui `sodium.h`; a C++ chama pela declaração |
| `transposition.cpp` | limpa a saída nos **dois** ramos de `ONYX_ERR_PAD` | a K4 escreve a des-transposição e só depois valida o padding |
| `k9_chacha.c` | limpa a saída em `ONYX_ERR_AUTH` | defesa em profundidade: o contrato não depende de um detalhe do libsodium |
| `ffi_c.rs` | declara `onyx_limpar` e expõe `limpar` | um ponto de entrada em Rust, sem `sodium.h` do lado do daemon |

O contrato passou a ser **«devolveu erro, não há saída utilizável»**, e
é verificado dos dois lados: os testes C (`test_transposition.cpp`)
observam o buffer depois de um erro, os testes Rust
(`ffi_c::tests::k4_rejeitado_nao_deixa_saida_utilizavel` e
`k9_rejeitado_…`) fazem o mesmo pela fronteira da FFI, e o guard
`limpeza_de_memoria_esta_chamada` falha se a função deixar de estar
declarada, implementada ou chamada nos dois ramos.

**O que continua por limpar, e é uma escolha:**

* **Os caminhos de sucesso não são limpos.** A saída pertence ao
  chamador — apagá-la seria apagar o plaintext que ele acabou de pedir.
* **Os buffers de entrada não são limpos.** Pertencem a quem os passou;
  em Rust são `Vec` com `zeroize` ou `Drop`, do lado de lá.
* **O que a C escreve é limpo; o que o compilador optimizou não é
  mensurável.** `sodium_memzero` é opaca ao optimizador, o que é o
  melhor que se consegue sem `mlockall` — e `mlockall` é uma troca
  diferente, tratada em §Exposição a swap.

> **Nota sobre `explicit_bzero`.** Não é usado. O libsodium está
> linkado estaticamente em `crypto/c_cpp/vendor/libsodium/lib/libsodium.a`
> e exporta `sodium_memzero` (`nm` confirma `T sodium_memzero`), pelo
> que a alternativa da libc não se justifica. Se um dia o vendor
> desaparecer e o CMake caia no libsodium do sistema, `sodium_memzero`
> continua lá — é a mesma biblioteca.

---

## Exposição a swap e page-out

> **A zeroização só protege as páginas que estão em RAM.** Esta secção
> declara o que acontece quando uma página de material sensível é
> descartada para swap, e o que é feito para evitar que isso aconteça.

### O problema

Quando o kernel precisa de memória, pode escrever páginas anónimas
(páginas de heap, `mmap`, pilha) numa área de swap. Se a página
contiver uma chave, **a chave existe agora fora do processo**, num
dispositivo persistente.

`Segredo` faz `zeroize` no `Drop`. Se a página já foi para swap, o
`zeroize` só reescreve a cópia em RAM — **a cópia no swap não é
tocada**. A memória sensível sobrevive, e sobrevive a desligar o
processo.

### O que já está mitigado no ambiente de execução

| Mitigação | Efeito |
| --- | --- |
| `/swapfile` com modo `0600 root:root` | um atacante **não-root** da mesma máquina (A8) não consegue ler a swap |
| `kernel.yama.ptrace_scope = 1` | restringe `/proc/<pid>/mem` entre utilizadores distintos |
| `zeroize` no `Drop` (Rust e C) | a cópia em RAM é limpa no descarte |

### O que **não** está mitigado

| Exposição | Quem está exposto |
| --- | --- |
| `root` no mesmo host | lê `/swapfile` e `/proc/*/mem` sem restrição |
| forense de disco após desligar | o swapfile é um ficheiro comum; em SSD, o wear-leveling impede zeroização fiável |
| *cold-boot attack* / RAM física | chaves em RAM são extraíveis sem desligar |
| dumps de núcleo (`coredump`) | o dump pode conter o espaço de memória do processo |

Estes casos estão **fora do âmbito de A8** e são declarados
explicitamente como **A10** em [`threat_model.md`](threat_model.md).

### `PLANEADO` (F0c) — `mlockall`

O daemon passará a trancar as suas páginas em RAM no arranque:

```text
mlockall(MCL_CURRENT | MCL_FUTURE)
```

* `MCL_CURRENT` tranca o que já está mapeado; `MCL_FUTURE` manda o
  kernel trancar as páginas das atribuições seguintes.
* Se `RLIMIT_MEMLOCK` for insuficiente, a chamada falha com `ENOMEM`.
  O daemon **degrada graciosamente** — arranca, regista um aviso, e
  prossegue sem a protecção. Um `mlock` falhado **nunca** impede o
  arranque: perder a protecção de swap é pior do que não arrancar.
* Trancar tudo numa máquina com pouca RAM pode falhar por pressão; por
  isso o alvo é trancar o processo inteiro e aceitar a degradação, em
  vez de tentar trancar chave a chave (o que seria subjectivo e
  dependente do momento de alocação).

`RLIMIT_MEMLOCK` por omissão em sistemas Linux varia bastante (de
64 KB a ~478 KB em algumas distribuições) — verificar com `ulimit -l`.
O OnyxChat precisa de muito menos do que o total do processo, mas o
valor por omissão pode ser insuficiente; nesse caso subir com
`sudo prlimit --memlock=unlimited --pid $(pidof onyxchatd)`.

> **Isto é defesa em profundidade, não uma garantia.** `mlockall`
> protege contra page-out. Não protege contra `root`, forense de disco
> nem *cold-boot*. Nenhuma chave em RAM está segura contra quem tem
> acesso físico ao sistema.

---

## Destruição

| Evento | O que é destruído |
| --- | --- |
| rotação de amizade (novo `FRIEND_REQUEST`) | chaves K1/K5/K9 antigas do estado do daemon |
| apagar amizade | entrada do keystore correspondente |
| fim de processo | `Segredo` é zeroizado no `Drop` |
| expirar TTL do anti-replay | nonces antigos saem da memória circular |

**Não existe** rotação automática periódica de chaves — a rotação é
sempre um evento explícito (novo handshake).

---

## Regras de implementação

1. Nenhuma chave é copiada para logs, erros ou mensagens de estado.
2. Nenhuma chave trafega fora do handshake e do UDS local.
3. Toda a chave que trafega está coberta por assinatura Ed25519.
4. Toda a chave em memória no Rust passa por `Segredo`/zeroize.
5. O keystore nunca é lido sem passphrase.
