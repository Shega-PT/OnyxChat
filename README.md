# OnyxChat

Mensageiro **end-to-end, peer-to-peer, com Tor embutido**, que protege
cada mensagem com um **pipeline híbrido de 9 camadas** (digitais +
analógicas) e assinaturas Ed25519 de estabilização de identidade.

Não é «um mensageiro que usa Tor» — é uma **arquitetura de comunicação
P2P privada** em que cada instância é, ao mesmo tempo, cliente e
servidor. Ver [`docs/SYS_GUIDE.md`](docs/SYS_GUIDE.md).

---

## Por onde começar

| Quero… | Ler |
| --- | --- |
| usar o sistema | [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md) |
| perceber como está organizado | [`docs/architecture.md`](docs/architecture.md) |
| compreender a arquitectura e as decisões | [`docs/SYS_GUIDE.md`](docs/SYS_GUIDE.md) |
| desenvolver | [`docs/DEV_GUIDE.md`](docs/DEV_GUIDE.md) |
| saber o que é prometido (e o que não é) | [`docs/threat_model.md`](docs/threat_model.md) |
| ver o estado actual | [`docs/roadmap.md`](docs/roadmap.md) |
| o índice completo | [`docs/index.md`](docs/index.md) |

---

## Compilar

```bash
# daemon de produção — inclui Tor (458 crates)
./scripts/memoria.sh --pico -- cargo build --release --features tor-real

# cliente Python
python3 -m venv .venv && .venv/bin/pip install -e '.[dev]'

# testes (matriz das 3 linguagens; leve — sem arti)
./scripts/testar.sh
```

> **RAM.** Todos os comandos Rust vão através de
> [`scripts/memoria.sh`](scripts/memoria.sh), que põe o build dentro de
> uma *scope* de cgroup com tecto de memória. Sem esse tecto, o kernel
> escolhe a vítima do OOM pelo `oom_score` — e o processo escolhido é o
> de maior `oom_score`, não o que usa mais memória. Num registo de três
> execuções sem tecto, a vítima foi sempre um editor de linguagem com
> ~1 GB, com o `cargo` a 26 MB e o `rustc` a 39 MB ao lado.
>
> Picos medidos: **547 MB** para o workspace leve (103 crates) e
> **931 MB** para a árvore da arti (458 crates). Os limites de payload
> são derivados de um orçamento de 1 GB e sobrescrevíveis com
> `ONYXCHAT_MAX_PAYLOAD`. Detalhes e a tabela de OOM em
> [`docs/DEV_GUIDE.md`](docs/DEV_GUIDE.md) §1.2.

---

## Pipeline K1 → K9

```text
plaintext
  │ K1  ChaCha20-Poly1305 (chave do par)        [Rust]
  │ K2  Cesariana +3                            [Lua]
  │ K3  Vigenère "VIGENERE"                     [Lua]
  │ K4  Transposição 5 colunas (PKCS#7)         [C++]
  │ K5  AES-256-GCM (chave do remetente)        [Rust]
  │ K6  Playfair Adaptado Onyx (XOR "PLAYFAIR") [Lua]
  │ K7  Hill [[3,3],[2,5]] mod 256              [Rust]
  │ K8  Substituição b+7 mod 256                [Lua]
  │ K9  ChaCha20-Poly1305 (chave do receptor)   [C/libsodium]
  ▼
envelope = versão ‖ nonces ‖ ciphertext ‖ assinatura Ed25519
```

As nove camadas são **defesa em profundidade**, não «nove vezes mais
segurança». A assinatura é **sempre verificada antes de decifrar**; os
nonces são aleatórios por mensagem. Norma completa em
[`docs/pipeline.md`](docs/pipeline.md).

---

## Estado

`IMPLEMENTADO`: pipeline K1→K9, envelope assinado, handshake com versão
autenticada, anti-replay e anti-downgrade, identidade Ed25519, daemon
Rust, Tor embutido, P2P directo, relay opcional, discovery
`ID → .onion`, IPC autenticado por uid, cliente Python com keystore
cifrado, test vectors multi-linguagem, property tests, fuzzing,
especificações e modelos.

`PLANEADO`: routing nodes, rede de transporte descentralizada, discovery
descentralizado. `CONCEITO`: Fake-IP, multimédia, mitigação de metadados,
identificadores de algoritmo por camada.

> **Cobertura de linhas não é 100% em Rust — é 99,38%** (medido em
> 2026-10-07). Das 45 linhas em falta, 6 são alcançáveis e estão por
> testar; as outras 39 estão declaradas uma a uma, com a razão, em
> [`docs/testing.md`](docs/testing.md) §Cobertura. Python e C/C++ estão
> a 100%. A cobertura mostra execução, não correcção. Ver
> [`docs/security_model.md`](docs/security_model.md).

---

## Aviso sobre privacidade

> Cifrar conteúdo **não** elimina metadados (tamanho, frequência,
> timing). Em modo relay sobre clearnet, os IPs de ligação são visíveis
> ao relay. A identidade Onyx é estável e correlacionável.
>
> O que está protegido e o que não está, com precisão:
> [`docs/privacy_model.md`](docs/privacy_model.md).

---

## Licença

**PolyForm Noncommercial 1.0.0** (SPDX: `PolyForm-Noncommercial-1.0.0`).
**Não é uma licença open source pela definição da OSI.**

### O que podes fazer sem pedir nada

Usar, alterar e redistribuir, para fins **não-comerciais**:

* **uso pessoal** — estudo,investigação, experimentação, projectos
  pessoais;
* **ensino e investigação** — instituições de ensino, investigação
  pública, e também organizações caritativas ou de saúde pública, **independentemente
  da origem do financiamento**.

Um fork não comercial é legal e não precisa de autorização.

### O que precisa de autorização do titular

Qualquer uso **comercial**: vender, oferecer como serviço, ou integrar o
código num produto pago. Isso é autorizado por um documento separado,
`COMMERCIAL-LICENSE.md`, emitido por quem consta de `AUTHORIZED.md`.
Envia um *issue* ou escreve para o endereço em `COPYRIGHT.md`.

### Vale a pena saber

**Não é open source.** Se procuras uma licença OSI — GPL, AGPL, Apache —
não a encontras aqui. É source-available, deliberadamente.

### Terceiros

O libsodium vendorizado (ISC) e o Lua (MIT) mantêm as suas licenças.
Todas as dependências são permissivas — nenhuma copyleft. Ver a secção
**Licenças de terceiros** do `LICENSE`.
