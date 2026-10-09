# Modelo de Segurança

## Enquadramento

Este documento descreve as propriedades criptográficas **pretendidas**
pelo OnyxChat e, para cada uma, se está **efetivamente demonstrada ou
testada**. A distinção é obrigatória: uma propriedade pretendida que
não tem teste associado não é uma garantia — é uma intenção.

O modelo de ameaças que a sustenta está em
[`threat_model.md`](threat_model.md).

---

## Modelo de confiança

> **Todo componente de transporte é considerado não confiável.**

Isto inclui: discovery server, relay, futuros nós de routing,
infraestrutura Tor e qualquer nó intermediário — tudo o que não seja a
extremidade da comunicação.

```text
transporte
    │
    ▼
não recebe confiança criptográfica
```

A confiança existe **entre os endpoints**: identidade autenticada por
Ed25519, conteúdo cifrado fim-a-fim.

---

## As cinco áreas de segurança

O OnyxChat distingue explicitamente quatro áreas diferentes — confundir
umas com outras é a origem dos erros mais comuns em avaliação:

### 1. Segurança criptográfica (primitivas)

```text
ChaCha20-Poly1305   K1, K9
AES-256-GCM         K5
Ed25519             identidade/assinaturas
```

Avaliação: primitivas padrão, amplamente analisadas. As camadas
clássicas (K2/K3/K6/K8) **não** acrescentam garantias criptográficas
formais — são defesa em profundidade e ofuscação estrutural.

### 2. Segurança do protocolo

```text
handshake autenticado      transcript assinado (com versão)
key binding                chaves vinculadas à identidade
anti-replay                nonces de handshake + nonce1 de chat
anti-downgrade             versão dentro da região assinada
verificação pré-decifragem assinatura antes de qualquer decifra
```

### 3. Segurança da implementação

```text
memory safety              Rust no daemon e no crypto_core
parsing por comprimento    nunca delimitadores, nunca panics
bounds checking            índices validados antes de acesso
zeroization                limpeza de material sensível em memória
error handling             Result/enum — nunca unwrap em input externo
mlockall                   páginas trancadas — evita page-out para swap (F0c)
```

### 4. Segurança operacional

```text
permissões do socket       UDS 0600 + SO_PEERCRED (uid)
ficheiros                  keystore 0600
armazenamento de chaves    PBKDF2-HMAC-SHA256 + ChaCha20-Poly1305
isolamento de processo     daemon separado do cliente
swap                       /swapfile 0600 root:root; mlockall quando disponível
```

### 5. Segurança da cadeia de fornecimentos

Esta é a única das cinco cujas ameaças não chegam ao runtime: entram
**antes** de haver binário.

| Medida | Ferramenta | Bloqueia |
| --- | --- | --- |
| Segredos no histórico | `gitleaks` | padrões conhecidos |
| Advisories de Rust | `cargo audit` | `RUSTSEC` de `Cargo.lock` |
| Advisories de JavaScript | `npm audit` vs [`seguranca-excepcoes.toml`](../seguranca-excepcoes.toml) | advisories sem excepção declarada |
| Licenças | `cargo deny` com [`deny.toml`](../deny.toml) | GPL, AGPL, SSPL, OSL, EUPL, CDDL, CC-BY-SA e busl alike |
| Workflows | `zizmor` + `uses:` por SHA | injecção e acções mutáveis |

`scripts/verificar_seguranca.sh` corre as cinco, **igual** localmente e
no CI. Localmente uma ferramenta ausente é um skip anunciado; no CI, com
`--exigir`, é falha.

#### As quatro excepções que existem, e porque

`npm audit` não tem ficheiro de excepções, e as duas saídas que oferece
não servem: `--omit=dev` esconderia advisories das ferramentas que
escrevem o bundle, e `--audit-level=critical` trocaria cobertura por
silêncio. Por isso o relatório é comparado com uma lista, e cada entrada
tem uma razão verificável no código e uma data de revisão.

| Advisory | Porquê é aceite |
| --- | --- |
| `GHSA-vfj7-8cjw-p6xm` — `braces`, DoS | Só corre na **construção**, sobre os padrões glob do repositório. Sem correcção upstream. CVSS 7.5, e build-time |
| `GHSA-rj75-hqrm-r3gf` — `postcss-selector-parser` | Mesma cadeia de construção, sobre o CSS do projecto. Sem input externo |
| `GHSA-wrjc-x8rr-h8h6` — `react-router`, open redirect | O alvo de **todos** os `<Link to>` e `navigate()` é uma constante de `UI/src/lib/onyx/nav.js`; e `authReturnTo.js` rejeita `\`, `//` e o que não começa por `/` |
| `GHSA-337j-9hxr-rhxg` — `react-router`, injecção de construtor | Só `deserializeErrors()`, de SSR Hydration. A aplicação é uma SPA: `hydrateRoot` e `renderToString` não aparecem em `UI/src/` |

As duas do `react-router` desaparecem quando o 7.x entrar, que é uma
major com mudanças na API de rotas. A excepção tem data de revisão para
que essa migração não fique esquecida.

#### O que estas cinco medidas **não** fazem

Escrito aqui para não haver dúvida:

* **Não** leem o texto da licença de um crate. Leem o campo declarado e
  a expressão SPDX. Um crate que declare `MIT` e distribua GPL não é
  apanhado por nenhum scanner.
* **Não** avaliam se a correcção de um advisory é boa. Dizem que existe
  um advisory e que se decidiu o que fazer com ele.
* **Não** protectem o que o código faz depois de arrancar. Isso é fuzzing
  ([`testing.md`](testing.md) §Fuzzing) e são as fronteiras de rede
  acima.
* **Não** cobrem Python, porque não há superfície: `pyproject.toml` tem
  `dependencies = []` e quatro pacotes de desenvolvimento. Quando a
  houver, o passo aparece.

Por cima de tudo: **não há promessa de ausência de vulnerabilidades.** O
que há é a fronteira declarada em [`threat_model.md`](threat_model.md) §A
sétima fronteira, a auditoria a correr, e as excepções com as razões ao
lado. Uma promessa do contrário seria o que
[`SECURITY.md`](../SECURITY.md) diz para não fazer.

#### Onde reportar

[`SECURITY.md`](../SECURITY.md). Em resumo: issue privado ou o endereço
em `COPYRIGHT.md`, **nunca** um issue público — a janela entre a
descrição e a correcção é a janela em que o bug é explorado.

---

## Porque é que os testes correm sem optimização

Os perfis `dev` e `test` compilam com `opt-level = 0` (`DEV_GUIDE.md`
§2.1). Isto **não** enfraquece a verificação — reforça-a:

```text
overflow-checks      ON em dev/test → aritmética com overflow é apanhada
debug-assertions     ON em dev/test → invariantes internas verificadas
codegen-units = 4    menos paralelização interna → menos RAM, mesmo código
opt-level = 0        o LLVM não pode explorar comportamento indefinido
```

Ao compilar com optimização, o LLVM tem latitude para assumir que não
há *undefined behavior* e para o eliminar. Numa suite que corre
optimizada, um bug de *UB* pode **desaparecer** em vez de ser
detectado. Compilar os testes sem optimização é, para este projecto,
a escolha que **mais apanha bugs**.

A contrapartida é tempo de execução mais alto nos testes. Como o
perfil não é usado em produção, e como o custo de compilação é o factor
limitante de qualquer máquina que não tenha um `build farm`, a troca é
favorável.

> A produção compila com `--release` (`opt-level = 3`). O que é
> verificado em testes é a **lógica**; o desempenho é uma propriedade de
> outro perfil.

---

## Propriedades pretendidas vs demonstradas

| Propriedade | Pretendida | Demonstrada/testada |
| --- | --- | --- |
| Confidencialidade do conteúdo | ✔ | ✔ tags AEAD falham com chave errada; testes de corrupção |
| Integridade do conteúdo | ✔ | ✔ adulteração de ciphertext → rejeição |
| Autenticidade do remetente | ✔ | ✔ assinatura Ed25519; `chave_publica_de_outro` |
| Binding identidade ↔ sessão | ✔ | ✔ transcript assinado inclui pub + chaves |
| Anti-replay (handshake) | ✔ | ✔ `nonce_visto` / `NonceRepetido` |
| Anti-replay (chat) | ✔ | ✔ registo de `nonce1` |
| Anti-downgrade | ✔ | ✔ versão na região assinada; versão no transcript |
| Verificação antes da decifragem | ✔ | ✔ testes que isolam `AssinaturaInvalida` antes de K9 |
| Separação de chaves | ✔ | ✔ chaves distintas por papel (K1/K5/K9) |
| K1 derivada por mensagem | ✔ | ✔ `SHA-256(domínio ‖ k₅ₐ ‖ k₅_b)` ordenado + HKDF-SHA256; simetria, direcção e sensibilidade a cada entrada em property tests |
| Forward secrecy | ✘ | **não** — declarado em [`threat_model.md`](threat_model.md) §Limitações 8. A semente é reconstruível a partir de duas chaves de longa duração |
| Paridade da derivação Rust↔Python | ✔ | ✔ vector congelado nas duas pontas + teste de guarda `derivacao_da_amizade_bate_entre_rust_e_python` |
| Zeroização de segredos | parcial | ✔ Rust: `Segredo` com `Drop` zeroize · ✔ C: `onyx_limpar` (= `sodium_memzero`) nos caminhos de erro de K4/K9 · ✘ os caminhos de sucesso e os buffers de entrada não são limpos, por pertencerem ao chamador |
| Protecção contra page-out | ✔ | **parcial** — `mlockall` é `PLANEADO` (F0c); swapfile `0600` já é do ambiente, não do projecto |
| Isolamento do backend Tor | ✔ | ✔ teste de guarda: `arti::` só em `tor_arti.rs` |
| Falha explícita sem Tor | ✔ | ✔ `construir_backend` devolve `Err` sem a feature — não degrada em silêncio |
| Paridade entre linguagens | ✔ | ✔ test vectors partilhados (`testing.md`) |
| Cobertura de linhas 100% | ✔ | ✔ llvm-cov/gcovr/coverage.py |
| **Ausência total de metadados** | **não pretendida** | ver `privacy_model.md` |
| **Segurança global vs observador total** | **não pretendida** | depende do Tor |
| **Resistência a root / cold-boot** | **não pretendida** | ver `threat_model.md` §A10 |

---

## Defesa em profundidade (K1 → K9)

As nove camadas **não** se somam:

```text
SEGURANÇA ≠ segurança(K1) + segurança(K2) + … + segurança(K9)

SEGURANÇA prática =
      composição correta
    + separação de chaves
    + autenticação (Ed25519 + tags AEAD)
    + proteção de transporte (Tor)
    + implementação correta
```

O objetivo é que a observação ou comprometimento de uma parte do
transporte não resulte automaticamente em plaintext, e que um atacante
que obtenha uma camada intermédia continue perante as restantes
transformações.

---

## Cobertura de linhas ≠ segurança

```text
100% line coverage  ≠  100% seguro
```

A cobertura demonstra que determinadas linhas foram **executadas** por
testes. Não demonstra:

* correção criptográfica;
* resistência a ataques;
* segurança do protocolo;
* ausência de bugs lógicos.

Por isso, além da cobertura, o projeto mantém: test vectors,
property tests, testes de corrupção e fuzzing (`testing.md`).

---

## Segurança das camadas — separação de papéis

```text
autenticação de identidade .... Ed25519 (handshake + envelope)
confidencialidade mútua ....... K1 (chave do par)
confidencialidade de origem ... K5 (chave do remetente)
confidencialidade de destino .. K9 (chave do receptor)
ofuscação estrutural .......... K2, K3, K4, K6, K7, K8
```

A separação importa: comprometer K5 não revela a chave do par, nem do
receptor. As chaves nunca são "uma única chave global"
(`key_management.md`).

---

## Propriedades de implementação exigidas

1. **Nenhum campo crítico alterável sem invalidar a assinatura.**
2. **Nenhum input externo provoca panic, crash ou alocação ilimitada.**
3. **Nenhuma mensagem de erro vaza segredos.**
4. **Rejeição segura** em todos os caminhos de erro (nunca plaintext
   parcial).
5. **Limites aplicados antes do processamento.**
6. **Falha explícita, nunca degradação silenciosa** — um binário sem uma
   capacidade que a operação exige recusa a operação, não finge
   tê-la.
7. **Nenhum caminho de erro devolve plaintext parcial** — incluindo
   padding inválido e tags AEAD erradas.

Todas estão cobertas por testes (`testing.md` §Categorias), excepto
onde indicado como `PLANEADO`.
