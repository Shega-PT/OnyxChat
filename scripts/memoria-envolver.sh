#!/usr/bin/env bash
# =====================================================================
# memoria-envolver.sh — invólucro de medição do gate de memória
# ---------------------------------------------------------------------
# Não se invoca directamente. É o que `scripts/memoria.sh --pico`
# corre DENTRO da scope de cgroup, em vez do comando do utilizador.
#
# Existe por uma razão concreta: o pico de memória (`memory.peak`) só
# pode ser lido de dentro da scope. A scope é transitória e
# desaparece quando o comando termina, e o caminho resolve-se por
# `/proc/self/cgroup` — que, lido de fora, dá a scope de quem chamou,
# não a scope do build. Ler de fora mede a coisa errada.
#
# O invólucro faz três coisas, por esta ordem:
#
#   1. imprime o valor inicial de `memory.peak`, para que um
#      `memory.peak` já contaminado por fases anteriores da scope não
#      conte para o build;
#   2. executa o comando, propagando o código de saída e os sinais;
#   3. grava o pico observado num ficheiro, para o gate o ler depois.
#
# Variáveis de ambiente (postas por `memoria.sh`):
#
#   ONYXCHAT_PICO    ficheiro onde gravar o pico, em MB
#   ONYXCHAT_TECTO   tecto em MB, só para diagnóstico
# =====================================================================

set -o errexit
set -o nounset
set -o pipefail

PICO_FILE="${ONYXCHAT_PICO:?ONYXCHAT_PICO por definir}"
TECTO="${ONYXCHAT_TECTO:-0}"

# Resolve a scope em que este processo vive. Dentro da scope, este é o
# caminho correcto — é a própria scope, não a de quem invocou.
scope_atual() {
    awk -F: '$1 == "0" { print $3 }' /proc/self/cgroup
}

# Pico em MB. `memory.peak` existe no cgroup v2 desde o kernel 5.19;
# em kernels mais antigos devolve 0 em vez de falhar, e o gate trata
# 0 como "não medido".
pico_mb() {
    local dir="$1"
    local bytes
    bytes="$(cat "/sys/fs/cgroup${dir}/memory.peak" 2>/dev/null || true)"
    if [[ -z "$bytes" ]]; then
        printf '0'
        return
    fi
    awk -v b="$bytes" 'BEGIN { printf "%d", (b / 1048576) }'
}

SCOPE_DIR="$(scope_atual)"

# `memory.peak` é acumulativo desde a criação da scope, e só pode ser
# reposto a zero escrevendo nele. Sem o repôr, uma scope nova começa com
# o zero — mas o `systemd-run --scope` cria a scope e depois executa o
# comando, e se o `memory.peak` tiver sido herdado de uma execução
# anterior (o que acontece quando o `systemd` reusa o nome transitório),
# o valor inicial não é zero.
#
# Escrever 0 é seguro: o kernel só aceita a escrita se o novo valor for
# **igual ou superior** ao actual, ou se não houver processo na cgroup.
# Como estamos a executá-lo, a escrita é tipicamente recusada — daí o
# guard `|| true`, e daí o valor inicial servir de base de comparação.
pico_actual="$(pico_mb "$SCOPE_DIR")"
echo 0 > "/sys/fs/cgroup${SCOPE_DIR}/memory.peak" 2>/dev/null || true

# O pico a partir de que medimos: o que a scope consumia **depois** de
# qualquer herança.
PICO_INICIAL="$(pico_mb "$SCOPE_DIR")"
if (( PICO_INICIAL > pico_actual )); then
    # O repôr não pegou (a escrita foi recusada pelo kernel). Nesses
    # casos medimos a diferença, não o absoluto.
    PICO_INICIAL="$pico_actual"
fi

# --- O comando do utilizador ----------------------------------------------

if (( $# == 0 )); then
    printf 'memoria-envolver.sh: nenhum comando\n' >&2
    exit 2
fi

codigo=0
"$@" || codigo=$?

# --- A medição ------------------------------------------------------------

PICO_FINAL="$(pico_mb "$SCOPE_DIR")"

# Reportar o valor absoluto quando a scope começou a zero é o caso
# normal e o que interessa: é o pico do build. A subtração só é
# necessária quando o repôr falhou e há herança a remover.
if (( PICO_INICIAL > pico_actual )); then
    PICO_FINAL=$(( PICO_FINAL > pico_actual ? PICO_FINAL - pico_actual : 0 ))
fi

printf '%d\n' "$PICO_FINAL" > "$PICO_FILE"

# Para o log do gate, em stderr, para não se confundir com a saída do
# próprio build (que o utilizador quer ver limpa).
if (( codigo != 0 )); then
    printf 'onyxchat: build terminou com código %d, pico %d MB de um tecto de %d MB\n' \
        "$codigo" "$PICO_FINAL" "$TECTO" >&2
fi

exit "$codigo"