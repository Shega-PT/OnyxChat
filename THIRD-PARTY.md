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

Para uma auditoria de *publish*, `cargo deny check licenses` com um
`deny.toml` versionado é o meio. **Já está feito e já é portão**:
`scripts/verificar_seguranca.sh` corre-o, em cada push, e
[`SECURITY.md`](SECURITY.md) §Cadeia de fornecimentos diz onde. Este
parágrafo dizia «Está em `docs/roadmap.md` §PLANEADO» até 2026-10-13 —
e esse rótulo estava errado: uma tarefa
declarada por fazer e já imposta em cada push convida a alguém a
reimplementá-la.

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

## Compilado de fonte, com excepção à regra

Esta secção é uma **excepção escrita**, e é exactamente o mesmo padrão
da secção de cima: uma regra, um componente nomeado, uma justificação,
e o mínimo possível fora dela. O que muda é que aqui a licença **não é
permissiva**.

| Componente | Versão | Licença | Onde | Como entra |
| --- | --- | --- | --- | --- |
| FFmpeg | 9.0.2 | **LGPL-2.1-or-later** | `tools/ffmpeg/` | compilado de fonte, comprimido com xz, **embebido no `onyxchatd`** |

### A regra que esta excepção altera

`deny.toml` recusa `GPL-2.0`, `GPL-3.0`, `LGPL-2.0`, `LGPL-2.1` e
`LGPL-3.0`, com esta justificação: *«um único crate sob GPL, AGPL,
SSPL… obriga a abrir o código derivado — comercial ou não — e torna a
licença do projecto impossível de cumprir»*.

O `deny.toml` **não foi alterado**, e essa é a decisão que importa. A
lista avalia crates do registry; o FFmpeg não é um crate, é um
executável que nós compilamos de fonte versionada — o mesmo que a
libsodium. Acrescentar `LGPL` à lista tornaria a excepção **maior do que
é**: passaria a admitir LGPL em qualquer dependência, em vez de num
componente nomeado, justificado e com uma receita à vista.

### Porquê LGPL é aceitável aqui, e GPL não

A LGPL e a GPL têm a mesma obrigação central — o derivado tem de poder
modificar e voltar a linkar — mas **a LGPL tem uma porta de saída que a
GPL não tem**: a linking dinâmico. E há uma leitura melhor ainda, que é
a que este projecto adopta:

> O `ffmpeg` **não é ligado** ao `onyxchatd`. É um executável
> separado, comprimido com xz e **embutido como dados**, e executado de
> um `memfd`. O daemon não importa símbolos seus e o FFmpeg não vê o
> código do daemon. Isto é **agregação**, não derivação: é o mesmo
> regime de um `tar.gz` que acompanha um programa, não o de uma
> biblioteca ligada.

As obrigações que restam, e como se cumprem:

| Obrigação da LGPL | Como é cumprida |
| --- | --- |
| poder modificar e voltar a linkar | `tools/ffmpeg/build.sh` **é** a receita, versionada neste repositório. Qualquer pessoa recompila o que quiser. |
| acesso ao código fonte correspondente | `tools/ffmpeg/versoes.txt` fixa a **versão exacta** (9.0.2) e o **SHA-256 da fonte**, que fica no repositório. |
| o aviso de licença acompanha o componente | A receita não o remove, e o `LICENSE` acompanha a fonte extraída. |

### A garantia não está na palavra, está no binário

Um `THIRD-PARTY.md` que diz «LGPL» é um documento. A garantia que o
projecto costuma exigir de si — *uma propriedade verificada, não uma
nota* — vem de `tools/ffmpeg/build.sh`, que no fim do build:

1. lê `License:` do `configure` e recusa se não for `LGPL*`;
2. corre `ffmpeg -buildconf` e recusa se **`--disable-gpl`** não lá
   estiver;
3. **apaga o executável** e sai com erro se qualquer uma das três falhar.

E a receita é `--disable-gpl --disable-nonfree --disable-everything`
seguido de uma lista de `--enable` **explícita**. O que não está na
lista não entra, o que faz a receita independente da versão do FFmpeg:
sem o `--disable-everything`, o que vem de origem mudaria a cada
release, e uma receita que muda sozinha não é uma receita.

### O que fica deliberadamente de fora

| Fora | Porquê |
| --- | --- |
| `libx264`, `libx265` | **GPL**. É a razão de isto ser LGPL e não GPL. |
| VP9 patentado (`libvpx` configurado com `vpx`) | **GPL**. O VP8 e o VP9 sem patents continuam disponíveis. |
| `libaom`, `libopus`, `libvorbis`, `libmp3lame` | BSD, ISC ou LGPL — entram livremente. |
| rede (`--disable-network`) | O `ffmpeg` deste projecto nunca fala com a rede. `docs/threat_model.md` §A2 é sobre um relay malicioso; um cliente HTTP dentro de um executável embebido é superfície que ninguém pediu. |
| `ffplay` e SDL2 | O `ffmpeg` de captura não reproduz. A reprodução é feita pelo receptor, e o `ffplay` que a proposta menciona é uma dependência **à parte**, com a sua própria excepção a justificar se alguma vez entrar. |

### O custo honesto desta excepção

A licença do `onyxchatd` passa a ter uma obrigação que não tinha:
quem o distribuir tem de poder oferecer a receita de compilação e a
versão da fonte. Isso é `tools/ffmpeg/`. É um custo real e é a razão
pelo qual a excepção é de **um** componente e não da categoria.

## Conclusão

**Nenhum bloqueio.** O grafo de crates é integralmente permissivo, e
o único componente vendorizado que não é é o FFmpeg — cuja excepção
está escrita e verificada acima, e cujas obrigações se cumprem por a
receita de compilação estar no repositório ao lado da fonte.

A PolyForm Noncommercial continua a ser compatível com tudo o que está
aqui. O que muda é que a distribuição passa a ter de anexar a receita e a
versão da fonte do FFmpeg — o que `tools/ffmpeg/` já é, e o que esta
secção diz em vez de o deixar implícito.

O que fica declarado: esta auditoria olha para as **licenças declaradas**
na data indicada. Uma dependência nova pode mudar isso.

É por isso que o `cargo deny` não é uma recomendação futura: corre em
cada push desde 2026-10-07, em `scripts/verificar_seguranca.sh`, contra
o `deny.toml`, e o `roadmap.md` §Segurança da cadeia de fornecimentos
regista-o como resolvido. Uma linha que dizia «recomendado no roadmap»
num documento cujo próprio roadmap já o tinha resolvido é o tipo de
divergência que ninguém lê e todos herdam.