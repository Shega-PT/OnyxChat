# Terceiros — auditoria de licenças

Este ficheiro é o registo da auditoria de licenças de terceiros. Existe
porque **a compatibilidade da licença do projecto com as dependências é
uma condição de negócio, não uma formalidade**: se alguma dependência for
copyleft forte, ela obriga a abrir o código derivado — comercial ou não —
e a PolyForm Noncommercial passa a ser impossível de cumprir.

## Como auditar, e o que esta auditoria não cobre

A auditoria foi feita com `cargo metadata --format-version 1`, que traz
o campo `license` declarado por cada pacote, e com
`importlib.metadata` para o Python. Não foi usada o `cargo deny` porque
não está instalado, e instalar dependências no ambiente do utilizador não
é decisão do assistente.

**O que isto não cobre, e é preciso saber:**

- `cargo metadata` lê o campo `license` **declarado** pelo pacote. Não
  lê o texto da licença nem o confirma. Um pacote que declare `MIT` e
  distribua outra coisa passaria esta auditoria.
- Não valida expressões compostas (`MIT OR Apache-2.0` é tratado como
  uma categoria, não como uma escolha). Não interessa aqui, porque todas
  as expressões compostas encontradas têm MIT ou Apache-2.0 como
  alternativa.
- Não analisa a ligação dinâmica. Num binário Rust estático, a linking é
  transitiva e estática; os `.rlib` não correm em lado nenhum do
  processo.

Para uma auditoria deublish, `cargo deny check licenses` com um
`deny.toml` versionado é oigrateo. Está em `docs/roadmap.md` §PLANEADO.

## Resultado — Rust

**548 pacotes** no grafo completo (perfil com arti). **105** no perfil
leve.

**Nenhum pacote sob GPL, AGPL, SSPL, OSL, EUPL, CDDL ou CC-BY-SA.**

Distribuição:

```text
  283  MIT OR Apache-2.0
   94  MIT
   56  Apache-2.0 OR MIT
   30  MIT/Apache-2.0
   18  Unicode-3.0
    9  Unlicense OR MIT
    7  BSD-3-Clause
    6  Apache-2.0
    6  Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT
    5  ISC
    4  Zlib OR Apache-2.0 OR MIT
    4  Zlib
    2  BSD-3-Clause OR MIT OR Apache-2.0
    2  MIT OR Apache-2.0 OR LGPL-2.1-or-later     <- r-efi
    2  BSD-2-Clause OR Apache-2.0 OR MIT
    2  Apache-2.0/MIT
    2  PolyForm-Noncommercial-1.0.0               <- o projecto
   ... mais 17 variantes distintas
```

### As quatro que merecem nota

Nenhuma obriga a nada. Estão aqui porque «não há copyleft» é uma
afirmação que precisa de ser auditável, e estas são as que fariam um
leitor duvidar.

| Pacote | Versão | Licença declarada | Por que não é problema |
| --- | --- | --- | --- |
| `r-efi` | 5.3.0, 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | É uma **escolha**: `MIT` está disponível. E é uma crate de alvo UEFI — entra no grafo só para cross-compilation, não é ligada no binário Linux. |
| `priority-queue` | 2.7.0 | `LGPL-3.0-or-later OR MPL-2.0` | Idem: `MPL-2.0` está disponível, e o `or-later` não é obrigatório. |
| `option-ext` | 0.2.0 | `MPL-2.0` | Copyleft **por ficheiro**, não por programa. Obrigações só nas modificações aos *seus* ficheiros. Não os modificamos. |

Sobre o MPL-2.0, que é o único caso não dual: a licença copyleft da Mozilla
exige que, se modificares um ficheiro de um MPL, publiques essa
modificação. Usar sem modificar não transmite obrigação nenhuma ao teu
código. É o mesmo regime da LGPL-linkagem e a razão pela qual a comunidade
Rust trata MPL como compatível.

## Resultado — Python

`pyproject.toml` declara `dependencies = []`: **o projecto não tem
dependências de runtime**. Todo o Python usa a biblioteca padrão.

O `venv` de desenvolvimento tem 15 distribuições (pytest, coverage,
hypothesis e as suas dependências). Todas permissivas — MIT, BSD,
Apache-2.0 — e nenhuma é distribuída com o projecto.

## Vendorizado no repositório

| Componente | Versão | Licença | Onde |
| --- | --- | --- | --- |
| libsodium | 1.0.18 | **ISC** | `crypto/c_cpp/vendor/libsodium/` |
| Lua 5.4 | via `lua-src` | **MIT** | compilado de fonte pelo `mlua` |

Ambas permissivas, ambas com a sua licença original mantida no
repositório. A libsodium é linkada estaticamente no `onyxchatd`, o que
significa que o binário incorporates o seu código sob a ISC.

## Conclusão

**Nenhum bloqueio.** O grafo é integralmente permissivo ou copyleft
fraco-com-alternativa, e nenhum pacote vendorizado é copyleft. A
PolyForm Noncommercial é compatível com tudo o que está no repositório.

O que fica declarado: esta auditoria olha para as **licenças declaradas**
na data indicada. Uma dependência nova pode mudar isso — daí o `cargo
deny` estar recomendado no roadmap como verificação contínua em vez de
auditoria pontual.