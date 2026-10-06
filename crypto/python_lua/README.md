# crypto/python_lua — camadas analógicas (K2, K3, K6, K8)

## Porque existe este nome de pasta

O nome `python_lua` é um resquício histórico. A pasta continha uma
implementação de referência em Python (`python/`) **e** a de produção em
Lua; a de Python foi removida em 2026-10-05 por não ter caller — ver
§Porque só há Lua abaixo.

O que fica é a implementação de produção:

```text
lua/   →  implementação de PRODUÇÃO
          embutida no daemon Rust (mlua) e executada em cada ENCODE/DECODE
```

## Porque só há Lua

A implementação de referência em Python foi removida. Tinha 199 linhas,
39 statements, 100% de cobertura e **nenhum caller em produção**:
`messenger/pipeline.py` delega toda a cifra ao daemon por IPC, e o
carregador de plugins (`messenger/plugins_loader.py`) não era chamado
por nada. Em concreto, estava a ser medida pelo `fail_under = 100` de
`pyproject.toml` e a pesar na métrica de qualidade sem proteger nada.

O que substitui a função de referência são os **vectores oficiais** em
`tests/vectors/*.json`: são gerados pelo daemon (`gerar_vetores`) e
consumidos por três linguagens (`vetores.rs`, `test_vectors.c`,
`tests/test_vectors.py`), com o documento `docs/test_vectors.md`
verificado por teste. Um literal de vector repetido à mão num segundo
ficheiro Python não é uma garantia mais forte — é mais uma coisa que
pode divergir em silêncio.

## Camadas cobertas

| Camada | Algoritmo | Ficheiro | Parâmetro de produção |
| --- | --- | --- | --- |
| K2 | César | `lua/deslocamento.lua` | deslocamento `+3` |
| K3 | Vigenère | `lua/vigenere.lua` | chave `"VIGENERE"` |
| K6 | Playfair Adaptado Onyx | `lua/playfair.lua` | chave `"PLAYFAIR"` |
| K8 | Substituição | `lua/deslocamento.lua` | deslocamento `+7` |

K2 e K8 são a mesma função com deslocamentos diferentes: `b' = (b + k)
mod 256`. Vivem no mesmo ficheiro desde a fase G6 — antes eram
`cesar.lua` e `substituicao.lua`, com o corpo byte-a-byte idêntico, e
dois ficheiros que só podiam divergir. 

As constantes de produção estão fixadas em
`network/daemon_rust/src/pipeline.rs` (`DESLOCAMENTO_K2`, `CHAVE_K3`,
`CHAVE_K6`, `DESLOCAMENTO_K8`) e devem coincidir com os valores acima.
A especificação normativa (entrada, saída, erros, inversa, exemplos) é
[`docs/pipeline.md`](../../docs/pipeline.md).

## Contrato comum das quatro camadas

* Operam sobre **bytes arbitrários** (0x00–0xFF), não sobre alfabetos —
  compatíveis com o output binário de K1/K5.
* São **transformações totais e bijectivas**: `decifrar(cifrar(x)) == x`
  para qualquer `x`, sem exceções e sem erros próprios (exceto chave
  vazia em K3/K6).
* **Não acrescentam garantias criptográficas formais** — são defesa em
  profundidade e ofuscação estrutural (`docs/security_model.md` §1).

## Paridade entre implementações

Os testes obrigam a que Lua e Python produzam **os mesmos bytes** para
os mesmos inputs (`docs/testing.md` §Interoperabilidade), e os vectores
oficiais em `tests/vectors/*.json` são consumidos por ambas
(`docs/test_vectors.md`). Se alterar qualquer camada, altere as duas
implementações e regenere os vectores:

```bash
cargo run -p onyxchatd --example gerar_vetores
.venv/bin/pytest tests/test_vetores_python.py
```

## Papel de cada linguagem — resumo

```text
Lua     = execução real no daemon (produção)
Python  = referência, testes e plugins (validação)
Rust    = orqueção do pipeline + primitivas digitais (K1/K5/K7/K9)
C/C++   = K4 (transposição) + K9 produção (libsodium)
```
