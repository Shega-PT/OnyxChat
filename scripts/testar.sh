#!/usr/bin/env bash
# =====================================================================
# testar.sh — a matriz de testes do OnyxChat num comando
# ---------------------------------------------------------------------
# Existe porque os comandos correctos **não são os óbvios**:
#
#   * sem `--no-default-features`, compila-se a árvore da arti — 458 das
#     548 crs. É a razão de o build travar em máquinas com 4 GB;
#   * sem `--features docs`, os testes que leem os vectores oficiais não
#     compilam de todo;
#   * e nenhum passo `cargo` corre sem passar por `scripts/memoria.sh`,
#     que o confine numa scope de cgroup com tecto de memória;
#   * `-j` não é preciso: o g de `scripts/memoria.sh` calcula-o a
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
#   ./scripts/testar.sh --fuzz       # acrescenta os 6 alvos de fuzzing
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

# --- G de memória --------------------------------------------------------
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
# A recusa do g **não** é uma falha da matriz. Um build que não cabe
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
# os três saíam mal, e o g reportava «✓ passou» porque não propagava o
# código de saída. Ou seja: a matriz vinha a dar verde sem executar
# um único teste Rust. O que durante semanas pareceu «a suite passa» era
# uma suite que nunca correu.
cargo_tectado() {
    # A saída de CI entra **antes** da verificação do g: num runner
    # não há editor a proteger, e o g — que existe para proteger o
    # editor — não tem o que proteger.
    sem_tecado && { "$@"; return $?; }
    if [[ ! -x "$MEMORIA" ]]; then
        printf '%s %s\n' "$(cor '1;31' 'erro:')" \
            "scripts/memoria.sh não encontrado ou não executável" >&2
        printf '  %s\n' \
            "sem o g, o build corre sem tecto e o kernel pode matar o editor" >&2
        printf '  %s\n' "regenerar com: chmod +x scripts/memoria.sh" >&2
        FALHOU=$((FALHOU + 1))
        AVISOS+=("g de memória indisponível")
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

    # O g recusa (código 1) sem sequer começar o build. Distingue-se
    # por mensagem: `recusando correr o build` é o g, qualquer outra
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

# O Python também vai com tecto, pelo mesmo motivo.
#
# O g de `memoria.sh` tem dois perfis medidos e os dois são de `cargo`
# (103 crs, 547 MB; a arti, 931 MB). Aplicá-los ao `pytest` seria
# inventar: o `pytest` mede-se — pico de **71 MB** para a suite completa
# com cobertura, em `docs/testing.md` §G de hardware.
#
# Sem tecto, o passo 3 era o único da matriz fora do `MemoryMax`. É o que
# aconteceu em 2026-10-07: a suite Python completa foi lançada de uma vez
# sem confinamento, e o editor foi junto. Não era o `pytest` a estourar —
# medido, pede 89 MB — era não haver `MemoryMax` à volta.
#
# O pico **de referência** da suite Python. É o pior observado, não uma
# média nem um número redondo — ver `docs/testing.md` §Execuções
# registadas, onde estão os cinco valores medidos em 2026-10-07 (71, 74,
# 75, 78 e 89 MB; os últimos, com os testes de auditores a correr cada
# um o seu auditor num subprocesso).
#
# O tecto é este número + 300 de margem. Uma referência **abaixo** do
# que a máquina pede é um tecto calculado às cegas: o `pytest` pedia
# 89 MB com o cgroup posto para 71 + 300, e quando acrescentar dois
# auditores levou o pico acima dos 100 MB, o tecto deixou de ser folga
# e passou a ser o limite. É a razão pela qual se mede e não se estima.
PICO_PYTEST_MB=89

python_tectado() {
    sem_tecado && { "$@"; return $?; }
    if [[ ! -x "$MEMORIA" ]]; then
        printf '%s %s\n' "$(cor '1;31m' 'erro:')" \
            "scripts/memoria.sh não encontrado ou não executável" >&2
        return 2
    fi

    local tecto=$(( PICO_PYTEST_MB + 300 ))
    local ficheiro_pico
    ficheiro_pico="$(mktemp -t onyxchat-pico-pytest.XXXXXX)"

    # A mesma verificação que `memoria.sh` faz, e com a mesma regra: sem
    # cgroup v2 não há onde pôr o tecto, e dizer que o passo está
    # protegido sem o estar é pior do que não o proteger.
    local stat
    stat="$(stat -fc %T /sys/fs/cgroup 2>/dev/null || true)"
    if [[ "$stat" != "cgroup2fs" ]] || ! command -v systemd-run >/dev/null 2>&1; then
        rm -f "$ficheiro_pico"
        printf '%s %s\n' "$(cor '1;31' 'erro:')" \
            "sem cgroup v2 ou sem systemd-run — o pytest fica por medir" >&2
        printf '%s\n' \
            "  este passo corre a suite Python inteira; sem tecto, um" >&2
        printf '%s\n' \
            "  pico inesperado leva o editor com ele, que foi o que aconteceu." >&2
        return 2
    fi

    printf '→ pytest com tecto de %d MB (pico medido %d MB)\n' "$tecto" "$PICO_PYTEST_MB"

    local codigo=0
    ONYXCHAT_PICO="$ficheiro_pico" ONYXCHAT_TECTO="$tecto" \
        systemd-run --user --scope \
        -p MemoryHigh=$(( tecto * 80 / 100 ))M \
        -p MemoryMax="${tecto}M" \
        -p MemorySwapMax=0 \
        -p OOMPolicy=kill \
        -- env ONYXCHAT_PICO="$ficheiro_pico" ONYXCHAT_TECTO="$tecto" \
        "$RAIZ/scripts/memoria-envolver.sh" "$@" || codigo=$?

    local pico
    pico="$(head -n1 "$ficheiro_pico" 2>/dev/null || printf '0')"
    rm -f "$ficheiro_pico"
    printf '→ pico do pytest: %s MB de %d MB\n' "$pico" "$tecto"
    return "$codigo"
}

# O fuzzing vai com tecto, e o tecto é **medido**.
#
# Medido em 2026-10-07 no perfil de referência descrito em
# `docs/DEV_GUIDE.md` §1.2 (2 núcleos, 3,8 GB de RAM, `MemAvailable`
# de ~1350 MB com um editor aberto):
#
#   cargo +nightly fuzz build <alvo>      pico 770 MB   (uma vez, partilhado
#                                                        pelos seis alvos)
#   cargo +nightly fuzz run   <alvo>      pico  33–117 MB
#
# O build é o passo caro: compila o harness em release com debuginfo, e
# `crypto_core` e `mlua` por baixo. Os seis alvos partilham esse build,
# por isso são **um de cada vez** — o libFuzzer é multi-threaded por
# defeito (`-workers`/`-jobs`), e seis motores concorrentes a hundred MB
# cada um é a receita para repetir o que a arti fez.
#
# `cargo fuzz` sai ≠ 0 quando encontra um crash, e escreve o input em
# `fuzz/artifacts/`. O `-max_total_time` limita a wall-clock de cada
# alvo para que a matriz não fique presa; um smoke não é uma sessão de
# fuzzing, e `docs/testing.md` §Fuzzing diz o que uma sessão é.
PICO_FUZZ_BUILD_MB=770
PICO_FUZZ_RUN_MB=117
FUZZ_RUNS=2000
FUZZ_SEGUNDOS=120
FUZZ_ALVOS=(envelope_parser handshake_parser ipc_parser k4_decoder k7_decoder relay_parser)

# `nightly_tectado <tecto_mb> CMD…`
#
# O g de `memoria.sh` injecta `-j` e escolhe o perfil pelas features do
# comando; o `cargo fuzz` não tem features nem `-j` que interessem, e o
# alvo não faz parte do workspace. Confinar à mão é o que dá, e o
# tecto é o que `medir` mediu acima.
nightly_tectado() {
    sem_tecado && { "$@"; return $?; }
    local tecto="$1"; shift
    local ficheiro_pico
    ficheiro_pico="$(mktemp -t onyxchat-pico-fuzz.XXXXXX)"
    local codigo=0
    systemd-run --user --scope --quiet \
        -p "MemoryHigh=$((tecto * 80 / 100))M" \
        -p "MemoryMax=${tecto}M" \
        -p "MemorySwapMax=${tecto}M" \
        -p OOMPolicy=kill \
        -- env ONYXCHAT_PICO="$ficheiro_pico" ONYXCHAT_TECTO="$tecto" \
            "$RAIZ/scripts/memoria-envolver.sh" "$@" || codigo=$?
    local pico
    pico="$(head -n1 "$ficheiro_pico" 2>/dev/null || printf '0')"
    rm -f "$ficheiro_pico"
    if (( codigo != 0 )); then
        printf '  %s\n' "$(cor '1;33' "pico ${pico} MB de ${tecto} — o comando devolveu ${codigo}")"
    else
        printf '  %s\n' "$(cor '1;32' "pico ${pico} MB de ${tecto}")"
    fi
    return "$codigo"
}

# --- Instrumentação ---------------------------------------------------------

PASSOU=0
FALHOU=0
#: Passos que o g não deixou começar por falta de memória. Não são
#: falhas — são trabalho adiado, e o sumário diz quantos.
SALTADOS=0
AVISOS=()

# ## Porque é que existe uma saída, e porque é que ela é barulhenta
#
# O tecto de cgroup existe para uma coisa: proteger o editor de quem
# está a correr os testes com ele aberto. Numa máquina de 4 GB isso é a
# diferença entre um pico de RAM e um OOM que mata a sessão de trabalho.
#
# Num runner do GitHub Actions não há editor ao lado, e a máquina é uma
# VM só para aquele job — o tecto não protege nada e só introduz uma
# dependência de `systemd-run --user` que falha de formas difíceis de
# diagnosticar. Por isso `ONYXCHAT_SEM_CGROUP=1` corre o mesmo comando
# **sem** tecto.
#
# E diz isso, em cada passo, com o número do que deixou de estar
# medido. Um portão que deixa de medir em silêncio diria «✓» e quem lesse
# o relatório pensaria que a medição continua a acontecer. É
# exactamente o que `ONYXCHAT_TECTO` evita.
sem_tecado() {
    if [[ "${ONYXCHAT_SEM_CGROUP:-0}" != "1" ]]; then
        return 1
    fi
    printf '%s %s\n' "$(cor '1;33' '!')" \
        'sem tecto de memória (ONYXCHAT_SEM_CGROUP=1) — este passo não é medido'
    return 0
}

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
    --cobertura) COM_COBERTURA=1; RAPIDO=0; FUZZ=0 ;;
    --fuzz)      COM_COBERTURA=0; RAPIDO=0; FUZZ=1 ;;
    --rapido)    COM_COBERTURA=0; RAPIDO=1; FUZZ=0 ;;
    completo|"") COM_COBERTURA=0; RAPIDO=0; FUZZ=0 ;;
    -h|--help)
        sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *)
        printf 'opção desconhecida: %s (use --cobertura, --fuzz, --rapido ou --help)\n' "$MODO"
        exit 2
        ;;
esac

printf '%s\n' "$(cor '1;1' 'OnyxChat — matriz de testes')"
printf 'raiz:  %s\n' "$RAIZ"
printf 'modo:  %s\n' "$MODO"

# --- 1. Rust: unit + integração + property tests ---------------------------

titulo "1. Rust — workspace (103 crs, sem arti)"
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
        passo "pytest" python_tectado "$PYTEST" -q
    else
        # `fail_under = 100` está em pyproject.toml: o pytest sai != 0
        # se a cobertura descer abaixo de 100%.
        passo "pytest --cov" python_tectado "$PYTEST" --cov
    fi
fi

# --- 3b. O mapa da estrutura b com a árvore? ----------------------------

# `Estrutura.txt` vive **fora** do repositório, e por isso nada no
# `pytest` o confirmava: um mapa desatualizado/listava sete ficheiros que
# não existiam durante meses sem que nada desse sinal. Este passo é o
# que dá o sinal.
#
# Vai antes dos testes Python porque é instantâneo e não precisa de
# virtualenv — se o mapa estiver errado, é bom sabê-lo antes de gastar
# RAM a correr a matriz.
titulo "3b. Estrutura — o mapa b com a árvore?"
passo "verificar_estrutura.py" \
    "$RAIZ/scripts/verificar_estrutura.py"

# Os rótulos do roadmap não custam RAM, e um `PLANEADO` sem
# especificação é uma promessa que o código não pode cumprir. Vai
# depois da estrutura porque as duas coisas são sobre documentos que
# descrevem o código, e antes do `pytest` porque é instantâneo.
passo "verificar_roadmap.py" \
    "$RAIZ/scripts/verificar_roadmap.py"

# A documentação não pode falar da máquina de quem a escreve: um pico de
# RAM é propriedade do build, e a máquina só decide se cabe. Este passo
# lê cerca de 240 ficheiros e não compila nada.
passo "verificar_ambiente.py" \
    "$RAIZ/scripts/verificar_ambiente.py"

# A superfície de ataque que não é código. Vai depois dos portões de
# texto porque é a única que precisa de rede e de ferramentas externas,
# e vai sem `--exigir`: localmente, uma ferramenta ausente é um skip
# anunciado. No CI, com `--exigir`, é falha.
passo "verificar_seguranca.sh" \
    "$RAIZ/scripts/verificar_seguranca.sh"

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
    # a tudo. Duas falhas empilhadas: o g mentia, e por baixo do g
    # havia um passo partido. Corrigir uma sem ver a outra teria
    # trocado um verde falso por um vermelho falso.
    passo "cargo build (binário para o E2E)" \
        cargo_tectado cargo build "${CARGO_FLAGS[@]}"
    # `test_e2e_sidecar.py` entra aqui desde 2026-10-07: é o único
    # ficheiro que arranca os três (sidecar, loja e daemon) ao mesmo
    # tempo, e portanto o único que apanha o desacordo entre eles.
    passo "pytest E2E (daemon, rede, sidecar)" \
        python_tectado "$PYTEST" tests/test_e2e_daemon.py \
        tests/test_e2e_rede.py tests/test_e2e_sidecar.py -q
else
    saltar "E2E" "modo rápido ou sem virtualenv"
fi

# --- 5. Verificação de que tor_arti.rs continua a compilar ----------------
#
# `tor-real` não é feature de omissão, pelo que o ciclo de testes não a
# compila. Este passo é o que garante que o ficheiro que **toca a rede
# real** continua a compilar — sem pagar a árvore completa da arti a cada
# execução.

# O passo que provocou os OOM kills: 498 fingerprints da arti
# foram escritos e, no minuto seguinte, o OOM ocorreu. A verificação
# continua no modo por omissão, mas dentro de uma scope com tecto — o
# `rustc` morre, o editor não.
#
# O que mudou em 2026-10-07: o passo deixou de ser recusado em máquinas
# de 3,8 GB com o editor aberto. O pico medido desceu de 1156 MB para
# 931 MB (`CARGO_PROFILE_DEV_DEBUG=none`, que o g põe a todos os
# comandos `cargo`), e o g passou a contar RAM e swap em tectos
# separados. Medições em `docs/testing.md` §G de hardware.
#
# `ONYXCHAT_PULAR_TOR=1` salta-o, e **diz que saltou**. É o que o job
# de pull request usa: o passo compila 458 crs da arti e é o mais
# lento da matriz, e a única coisa que apanha é uma regressão num
# ficheiro. O mesmo passo corre no job agendado, e em `workflow_dispatch`,
# onde o tempo se paga.
#
# Saltar em silêncio seria o oposto do que este script faz em todo o
# resto: `saltar()` imprime sempre a razão.
if [[ "${ONYXCHAT_PULAR_TOR:-0}" == "1" ]]; then
    saltar "5. Tor (tor_arti.rs)" \
        "ONYXCHAT_PULAR_TOR=1 — corre no job agendado e no manual"
else
    titulo "5. Tor — verificação de compilação de tor_arti.rs (458 crs, com tecto)"
    passo "cargo check --features tor-real" \
        cargo_tectado cargo check -p onyxchatd --features tor-real
fi

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

# --- Fuzzing (opcional) ----------------------------------------------------
#
# Os seis alvos de `docs/testing.md` §Fuzzing, um de cada vez. Fora do
# modo por omissão porque precisa de **nightly** e porque um smoke de
# 2000 execuções por alvo não substitui uma sessão: é uma verificação de
# que o alvo ainda compila e ainda corre, não uma procura de bugs.
#
# O que esta etapa apanha de verdade não são os crashes que um smoke
# encontra — 2000 execuções não chegam para isso. É o facto de o alvo
# deixar de compilar, ou de o `cargo fuzz` deixar de correr, que
# acontece quando uma API muda e ninguém repara porque ninguém o
# constrói.
if [[ "$FUZZ" -eq 1 ]]; then
    titulo "Fuzzing — 6 alvos, um de cada vez (${FUZZ_RUNS} execuções cada)"
    if ! rustup toolchain list 2>/dev/null | grep -q '^nightly'; then
        saltar "Fuzzing" "nightly não instalada (rustup toolchain install nightly)"
    else
        # `cargo fuzz build` aceita **um** alvo por invocação — passar a
        # lista inteira dá `Usage: cargo-fuzz build` e código 2, que foi
        # o que a primeira versão deste passo fez.
        #
        # O build de cada alvo compila o harness e as dependências
        # (`crypto_core`, `mlua`), e o cargo cacheia entre invocações: só
        # o primeiro paga o pico de 770 MB, os seguintes são quase
        # incrementais. É por isso que compilar todos não «desperdiça»
        # 6×770 MB.
        for alvo in "${FUZZ_ALVOS[@]}"; do
            passo "build ${alvo} (nightly, tecto de ${PICO_FUZZ_BUILD_MB} MB)" \
                nightly_tectado "$PICO_FUZZ_BUILD_MB" \
                    env CARGO_BUILD_JOBS=1 cargo +nightly fuzz build "$alvo"
        done

        # Os runs só começam se todos os alvos compilarem. Um alvo que
        # deixou de compilar é um alvo que não está a ser testado, e
        # continuar a correr os outros daria um verde que esconde o
        # buraco.
        if (( FALHOU == 0 )); then
            for alvo in "${FUZZ_ALVOS[@]}"; do
                passo "fuzz ${alvo} (${FUZZ_RUNS} execuções)" \
                    nightly_tectado "$PICO_FUZZ_RUN_MB" \
                        cargo +nightly fuzz run "$alvo" -- \
                            "-runs=${FUZZ_RUNS}" "-max_total_time=${FUZZ_SEGUNDOS}"
            done
        else
            saltar "runs de fuzzing" "um alvo não compilou — nenhum run foi feito"
        fi

        # Um crash escreve um ficheiro em `fuzz/artifacts/`. Um
        # artefacto deixado por uma sessão antiga é um bug já corrigido, e
        # o `libFuzzer` passaria a reprocessá-lo para sempre — por isso o
        # aviso diz onde estão, em vez de os apagar em silêncio.
        if [[ -d fuzz/artifacts/onyxchat-fuzz ]] \
           && compgen -G "fuzz/artifacts/onyxchat-fuzz/*" >/dev/null; then
            printf '\n%s\n' "$(cor '1;33' 'artefactos de fuzzing por tratar:')"
            ls -1 fuzz/artifacts/onyxchat-fuzz/ | sed 's/^/  /'
            printf '  %s\n' "cada um é um input que provocou um crash — ver docs/testing.md §Fuzzing"
        fi
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
    printf '\n%s\n' "$(cor '1;33' 'passos que o g não deixou começar:')"
    printf '  %s\n' "sem memória suficiente para um build com tecto."
    printf '  %s\n' "Fecha o editor e repete — o g não arrisca o teu sistema."
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
