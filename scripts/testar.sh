#!/usr/bin/env bash
# =====================================================================
# testar.sh — a matriz de testes do OnyxChat num comando
# ---------------------------------------------------------------------
# Existe porque os comandos correctos **não são os óbvios**:
#
#   * sem `--no-default-features`, compila-se a árvore da arti — 458 das
#     548 crates. É a razão de o build travar em máquinas com 4 GB;
#   * sem `--features docs`, os testes que leem os vectores oficiais não
#     compilam de todo;
#   * e nenhum passo `cargo` corre sem passar por `scripts/memoria.sh`,
#     que o confine numa scope de cgroup com tecto de memória;
#   * `-j` não é preciso: o gate de `scripts/memoria.sh` calcula-o a
#     partir da memória disponível, e `.cargo/config.toml` põe
#     `jobs = 1` como piso para quem chamar o cargo directamente.
#
# Chamar directamente `cargo test --workspace` produz um build sem
# tecto de memória — e é exactamente assim que o VSCode foi fechado
# três vezes por OOM. Este script é o caminho que não faz isso.
#
# Uso:
#   ./scripts/testar.sh              # matriz completa (1–5)
#   ./scripts/testar.sh --cobertura  # acrescenta as coberturas (6–7)
#   ./scripts/testar.sh --rapido     # só Rust + Python, sem C/E2E
#
# Referência: `docs/DEV_GUIDE.md` §3 e `docs/testing.md` §Como correr.
# =====================================================================

set -o errexit
set -o nounset
set -o pipefail

# Raiz do projecto (a pasta que contém `Cargo.toml`).
RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"

# --- Configuração -----------------------------------------------------------

# Flags de build. `--no-default-features` é o ponto crítico.
#
# `docs` dá acesso ao `serde_json` de que a renderização dos vectores
# precisa, sem o que `tests/vetores.rs` não compila.
CARGO_FLAGS=(--no-default-features --features docs)

# Onde está o virtualenv Python. Se não existir, os passos Python são
# saltados com um aviso em vez de falharem — porque testar o Rust num
# ambiente sem Python instalado é uma escolha legítima.
VENV="$RAIZ/.venv"
PYTEST="$VENV/bin/pytest"
COVERAGE="$VENV/bin/coverage"

# Directório de build do C/C++.
BUILD_CPP="$RAIZ/crypto/c_cpp/build"

# --- Gate de memória --------------------------------------------------------
#
# Nenhum passo `cargo` corre directamente: corre por `memoria.sh
# --pico`, que o põe dentro de uma scope de cgroup v2 com `MemoryMax` e
# `OOMPolicy=kill`.
#
# A razão é o que aconteceu em 2026-10-05. O kernel registou três OOM
# kills, e as três vítimas foram o `rust-analyzer` (446 MB a 1,0 GB de
# RSS), nunca o `rustc`. No momento do OOM das 16:44 o build era
# minúsculo — `cargo` a 26 MB e dois `rustc` a 39 e 41 MB. O que
# esgotou a máquina foi a soma, e o editor era o elo mais gordo.
#
# Sem tecto, o kernel escolhe a vítima pelo `oom_score`, e isso não é
# negociável do lado do build. Com tecto, o OOM ocorre dentro da scope
# e mata o `rustc`. É esta a diferença entre «o build falhou» e «o
# VSCode fechou».
MEMORIA="$RAIZ/scripts/memoria.sh"

# `cargo` com tecto de memória e medição do pico.
#
# A recusa do gate **não** é uma falha da matriz. Um build que não cabe
# é uma decisão informada, e contá-la como falha mistura duas coisas:
# «o código está errado» e «a máquina não tem memória». O que interessa
# ao utilizador é a diferença.
# `cargo_tectado` é um invólucro: recebe o comando **completo**, com o
# `cargo` à frente. Passar só o sub-comando (`test`, `build`, `check`)
# faz o invólucro procurar `test` no `$PATH` — que existe, como
# `/usr/bin/test` — e falha com um erro de sintaxe que não tem nada a ver
# com o projecto.
#
# Foi o que acontecia nas três chamadas antes desta correcção: a matriz
# corria `/usr/bin/test --workspace`, `/usr/bin/build` e `/usr/bin/check`,
# os três saíam mal, e o gate reportava «✓ passou» porque não propagava o
# código de saída. Ou seja: a matriz vinha a dar verde sem executar
# um único teste Rust. O que durante semanas pareceu «a suite passa» era
# uma suite que nunca correu.
cargo_tectado() {
    if [[ ! -x "$MEMORIA" ]]; then
        printf '%s %s\n' "$(cor '1;31' 'erro:')" \
            "scripts/memoria.sh não encontrado ou não executável" >&2
        printf '  %s\n' \
            "sem o gate, o build corre sem tecto e o kernel pode matar o editor" >&2
        printf '  %s\n' "regenerar com: chmod +x scripts/memoria.sh" >&2
        FALHOU=$((FALHOU + 1))
        AVISOS+=("gate de memória indisponível")
        return 2
    fi

    local saida codigo
    saida="$("$MEMORIA" --pico -- "$@" 2>&1)"
    codigo=$?
    printf '%s\n' "$saida"

    if (( codigo == 0 )); then
        PASSOU=$((PASSOU + 1))
        return 0
    fi

    # O gate recusa (código 1) sem sequer começar o build. Distingue-se
    # por mensagem: `recusando correr o build` é o gate, qualquer outra
    # coisa é uma falha a sério do comando.
    if printf '%s' "$saida" | grep -q 'recusando correr o build'; then
        SALTADOS=$((SALTADOS + 1))
        return 0
    fi

    # Só `passo` regista a falha. `cargo_tectado` devolvia o código e
    # deixava o registo para o chamador — mas também o fazia, e cada
    # falha aparecia duas vezes no sumário, com `FALHOU` a contar o
    # dobro. Um sumário que diz «4 falhados» quando houve 2 ensina a
    # não ler o sumário.
    return "$codigo"
}

# --- Instrumentação ---------------------------------------------------------

PASSOU=0
FALHOU=0
#: Passos que o gate não deixou começar por falta de memória. Não são
#: falhas — são trabalho adiado, e o sumário diz quantos.
SALTADOS=0
AVISOS=()

cor() {
    # Só cor se a saída for um terminal: em log de CI, os códigos ANSI
    # são lixo.
    if [[ -t 1 ]]; then
        printf '\033[%sm%s\033[0m' "$1" "$2"
    else
        printf '%s' "$2"
    fi
}

titulo() {
    printf '\n%s\n' "$(cor '1;36' "══ $* ══")"
}

# `passo <descrição> <comando…>`
#
# Correr o comando, reportar e CONTAR. Um passo que falha não aborta o
# script: o objectivo de correr a matriz é saber o estado de *tudo*, e
# abortar no primeiro problema esconde os restantes.
passo() {
    local descricao="$1"; shift
    printf '%s %s\n' "$(cor '1;33' '→')" "$descricao"
    # `cargo_tectado` usa este nome quando precisa de registar uma falha.
    CMD_NOME_ATUAL="$descricao"
    if "$@"; then
        printf '%s %s\n' "$(cor '1;32' '✓')" "$descricao"
        PASSOU=$((PASSOU + 1))
    else
        printf '%s %s\n' "$(cor '1;31' '✗')" "$descricao"
        FALHOU=$((FALHOU + 1))
        AVISOS+=("$descricao")
    fi
}

saltar() {
    printf '%s %s — %s\n' "$(cor '1;33' '→')" "$1" "$(cor '1;33' "saltado: $2")"
}

# --- Pré-condições ----------------------------------------------------------

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf '%s falta: %s\n' "$(cor '1;31' 'erro:')" "$1"
        printf '  %s\n' "$2"
        exit 2
    fi
}

require cargo "Instale Rust: https://rustup.rs"

# --- Opções -----------------------------------------------------------------

MODO="${1:-completo}"
case "$MODO" in
    --cobertura) COM_COBERTURA=1; RAPIDO=0 ;;
    --rapido)    COM_COBERTURA=0; RAPIDO=1 ;;
    completo|"") COM_COBERTURA=0; RAPIDO=0 ;;
    -h|--help)
        sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *)
        printf 'opção desconhecida: %s (use --cobertura, --rapido ou --help)\n' "$MODO"
        exit 2
        ;;
esac

printf '%s\n' "$(cor '1;1' 'OnyxChat — matriz de testes')"
printf 'raiz:  %s\n' "$RAIZ"
printf 'modo:  %s\n' "$MODO"

# --- 1. Rust: unit + integração + property tests ---------------------------

titulo "1. Rust — workspace (103 crates, sem arti)"
passo "cargo test --workspace ${CARGO_FLAGS[*]}" \
    cargo_tectado cargo test --workspace "${CARGO_FLAGS[@]}"

# --- 2. C/C++: K4, K9, vectors --------------------------------------------

if [[ "$RAPIDO" -eq 0 ]]; then
    titulo "2. C/C++ — CTest"
    if ! command -v cmake >/dev/null 2>&1; then
        saltar "C/C++" "cmake não encontrado"
    else
        passo "cmake configure" \
            cmake -S crypto/c_cpp -B "$BUILD_CPP"
        passo "cmake build" \
            cmake --build "$BUILD_CPP"
        passo "ctest" \
            ctest --test-dir "$BUILD_CPP" --output-on-failure
    fi
else
    saltar "C/C++" "modo rápido"
fi

# --- 3. Python: pytest + cobertura ----------------------------------------

titulo "3. Python — pytest"
if [[ ! -x "$PYTEST" ]]; then
    saltar "Python" "sem virtualenv em .venv (correr: python3 -m venv .venv && .venv/bin/pip install -e '.[dev]')"
else
    if [[ "$RAPIDO" -eq 1 ]]; then
        passo "pytest" "$PYTEST" -q
    else
        # `fail_under = 100` está em pyproject.toml: o pytest sai != 0
        # se a cobertura descer abaixo de 100%.
        passo "pytest --cov" "$PYTEST" --cov
    fi
fi

# --- 4. Binário real: subprocess -------------------------------------------

if [[ "$RAPIDO" -eq 0 ]] && [[ -x "$PYTEST" ]]; then
    titulo "4. Binário real — daemon_binario + E2E"
    # O `cargo` no início não é redundante: `cargo_tectado` é um
    # invólucro, não uma abreviatura de `cargo`. Sem ele, o invólucro
    # executava `build` como comando do sistema e o sistema devolvia
    # 127.
    #
    # Este bug esteve lá desde sempre e nunca se viu, porque
    # `memoria.sh` não propagava o código de saída e reportava «✓ passou»
    # a tudo. Duas falhas empilhadas: o gate mentia, e por baixo do gate
    # havia um passo partido. Corrigir uma sem ver a outra teria
    # trocado um verde falso por um vermelho falso.
    passo "cargo build (binário para o E2E)" \
        cargo_tectado cargo build "${CARGO_FLAGS[@]}"
    passo "pytest test_e2e_daemon.py test_e2e_rede.py" \
        "$PYTEST" tests/test_e2e_daemon.py tests/test_e2e_rede.py -q
else
    saltar "E2E" "modo rápido ou sem virtualenv"
fi

# --- 5. Verificação de que tor_arti.rs continua a compilar ----------------
#
# `tor-real` não é feature de omissão, pelo que o ciclo de testes não a
# compila. Este passo é o que garante que o ficheiro que **toca a rede
# real** continua a compilar — sem pagar a árvore completa da arti a cada
# execução.

# Este é o passo que provocou os OOM kills: 498 fingerprints da arti
# foram escritos às 16:43 e o OOM ocorreu às 16:44. A verificação
# continua no modo por omissão, mas dentro de uma scope com tecto — o
# `rustc` morre, o editor não.
titulo "5. Tor — verificação de compilação de tor_arti.rs (458 crates, com tecto)"
passo "cargo check --features tor-real" \
    cargo_tectado cargo check -p onyxchatd --features tor-real

# --- 6–7. Coberturas (opcional) --------------------------------------------

if [[ "$COM_COBERTURA" -eq 1 ]]; then
    titulo "6. Cobertura Rust (llvm-cov)"
    if command -v cargo-llvm-cov >/dev/null 2>&1; then
        passo "cargo llvm-cov (tor_arti.rs excluído)" \
            cargo_tectado cargo llvm-cov --workspace "${CARGO_FLAGS[@]}" \
                --ignore-filename-regex 'tor_arti' --summary-only
    else
        saltar "Cobertura Rust" "cargo-llvm-cov não instalado (cargo install cargo-llvm-cov)"
    fi

    titulo "7. Cobertura C/C++ (gcovr)"
    if [[ -x "$VENV/bin/gcovr" ]] && command -v cmake >/dev/null 2>&1; then
        passo "rebuild com instrumentação gcov" \
            cmake -S crypto/c_cpp -B "$BUILD_CPP" -DONYX_COVERAGE=ON
        passo "cmake build" \
            cmake --build "$BUILD_CPP"
        passo "ctest" \
            ctest --test-dir "$BUILD_CPP" --output-on-failure
        passo "gcovr" \
            "$VENV/bin/gcovr" --root . \
                --filter 'crypto/c_cpp/(k9_chacha|transposition)\.(c|cpp)' \
                --exclude 'tests/' --exclude 'build/' "$BUILD_CPP"
    else
        saltar "Cobertura C/C++" "gcovr ou cmake em falta"
    fi
fi

# --- Sumário ----------------------------------------------------------------

printf '\n%s\n' "$(cor '1;1' '── sumário ──')"
printf '%s passos ok, %s falhados' "$PASSOU" "$FALHOU"
if (( SALTADOS > 0 )); then
    printf ', %s saltados por falta de memória' "$SALTADOS"
fi
printf '\n'

if (( SALTADOS > 0 )); then
    printf '\n%s\n' "$(cor '1;33' 'passos que o gate não deixou começar:')"
    printf '  %s\n' "sem memória suficiente para um build com tecto."
    printf '  %s\n' "Fecha o editor e repete — o gate não arrisca o teu sistema."
fi

if [[ "$FALHOU" -gt 0 ]]; then
    printf '\n%s\n' "$(cor '1;31' 'passos que falharam:')"
    for a in "${AVISOS[@]}"; do
        printf '  %s\n' "$a"
    done
    printf '\n%s\n' "$(cor '1;31' 'matriz FALHOU')"
    exit 1
fi

printf '%s\n' "$(cor '1;32' 'matriz completa: tudo passou')"
