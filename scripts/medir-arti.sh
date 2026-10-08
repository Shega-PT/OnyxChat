#!/usr/bin/env bash
# =====================================================================
# medir-arti.sh — mede o pico de RAM da verificação da arti
# ---------------------------------------------------------------------
# Existe para responder a uma pergunta que não se responde com
# estimativas: **é possível verificar `tor_arti.rs` nesta máquina com o
# editor aberto?**
#
# ## O problema
#
# `cargo check -p onyxchatd --features tor-real` tem um pico medido de
# 1156 MB (ver `docs/testing.md` §Gate de hardware). Com o editor aberto
# há ~1280 MB utilizáveis e o gate aceita 1 job de 847 MB — o build da
# arti é recusado.
#
# Fechar o editor resolve o número, mas deixa ninguém a acompanhar. E
# a resposta «espera» é pior. Portanto a pergunta tem de se resolver
# dentro da janela que há.
#
# ## O que este script mede
#
# Corre o mesmo comando três vezes, mudando o que muda a memória do
# `rustc`, e devolve o pico de cada uma:
#
#   A  a configuração actual (`debug = "line-tables-only"`)
#   B  `CARGO_PROFILE_DEV_DEBUG=none`
#   C  B mais `CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=unpacked`
#
# ## Porque é que B e C podem lowering
#
# Um `cargo check` **não produz binário**. Não há link, não há código de
# máquina, não há execução. O que produz é metadata (`.rmeta`), e a
# metadata não precisa de tabela de símbolos, números de linha nem DWARF
# completo.
#
# E `split-debuginfo = "unpacked"` move o que resta de debug para
# ficheiros `.dwo` separados, tirando-o do processo do compilador. É a
# razão de o pico do `rustc` cair sem que o código compilado mude: não
# muda — só muda onde a informação fica escrita.
#
# Se B e C chegarem abaixo do que está disponível, a verificação da arti
# passa a correr no modo por omissão com o editor aberto, e o passo 5 da
# matriz deixa de ser recusado.
#
# ## As três leituras são independentes
#
# Cada configuração corre com `CARGO_TARGET_DIR` própria. Sem isso a
# segunda medição encontraria a primeira já feita e o `rustc` nem
# correria — o "pico" seria o do `cargo` a ler fingerprints, que é a
# medição de `/usr/bin/true`, não a de um build.
#
# ## O tecto
#
# Cada medição corre dentro de uma scope de cgroup com `MemoryMax` e
# `OOMPolicy=kill`, como qualquer build. `MemorySwapMax=0` nestas
# medições, deliberadamente: a pergunta da Fase 1 é «o `rustc` cabe em
# RAM?», e deixar o swap responder por ele mediria outra coisa. O swap
# é a Fase 3, e é uma decisão separada.
#
# Referência: `docs/DEV_GUIDE.md`, secção 1.2 e `docs/testing.md` §Gate de
# hardware.
# =====================================================================

set -o errexit
set -o nounset
set -o pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"

# --- Configuração ----------------------------------------------------------

#: O comando medido. É o passo 5 de `scripts/testar.sh`, e é o único
#: comando do projecto que compila `tor_arti.rs`. O `-j1` é acrescentado
#: mais abaixo, antes das medições — as três têm de usar o mesmo
#: paralelismo para os picos serem comparáveis.
COMANDO=(cargo check -p onyxchatd --features tor-real)

#: Tecto de cada medição. Generoso de propósito: aqui não se quer que o
#: build morra, quer-se saber **quanto** pediria. A recusa por memória é
#: assunto do gate, não deste script.
Tecto_MB="${ONYXCHAT_TECTO_MEDIDA:-1400}"

NOME="${0##*/}"

cor() {
    if [[ -t 1 ]]; then printf '\033[%sm%s\033[0m' "$1" "$2"
    else printf '%s' "$2"; fi
}

erro() {
    printf '%s %s\n' "$(cor '1;31' 'erro:')" "$*" >&2
}

# --- Pré-condições ---------------------------------------------------------

[[ -d "$RAIZ/.cargo" ]] || { erro "isto não é a raiz do projecto"; exit 2; }
command -v cargo >/dev/null 2>&1 || { erro "cargo não encontrado"; exit 2; }

# A scope é onde vive o `MemoryMax`. Sem cgroup v2 não há tecto, e
# medir sem tecto é a forma mais rápida de matar o editor — que é
# exactamente o que este script existe para evitar.
if [[ "$(stat -fc %T /sys/fs/cgroup 2>/dev/null || true)" != "cgroup2fs" ]] \
   || ! command -v systemd-run >/dev/null 2>&1; then
    erro "sem cgroup v2 ou sem systemd-run — não há onde pôr o tecto"
    printf '  %s\n' "este script mede builds; sem tecto, um pico alto leva" >&2
    printf '%s\n' "o editor com ele, e a medição seria a de um OOM." >&2
    exit 1
fi

# --- Memória disponível ----------------------------------------------------
#
# O número que decide se um pico serve. `-350` é o mesmo crescimento de
# desktop que `scripts/memoria.sh` assume, e pela mesma razão: o editor
# cresce enquanto o build corre, e um tecto que ignore isso mata o
# editor no fim de um build que estava a ir bem.
MEM_TOTAL_MB=$(awk '/MemTotal:/ { print int($2/1024) }' /proc/meminfo)
MEM_DISP_MB=$(awk '/MemAvailable:/ { print int($2/1024) }' /proc/meminfo)
MEM_UTIL_MB=$(( MEM_DISP_MB - 350 ))
(( MEM_UTIL_MB < 0 )) && MEM_UTIL_MB=0
SWAP_LIVRE_MB=$(awk '/SwapFree:/ { print int($2/1024) }' /proc/meminfo)

printf '%s\n' "$(cor '1;1' 'OnyxChat — pico da verificação da arti')"
printf 'raiz:  %s\n' "$RAIZ"
printf 'comando: %s\n' "${COMANDO[*]}"
echo
printf 'RAM total:      %5d MB\n' "$MEM_TOTAL_MB"
printf 'disponível:     %5d MB\n' "$MEM_DISP_MB"
printf 'utilizável:     %5d MB   (disponível − 350 de crescimento)\n' "$MEM_UTIL_MB"
printf 'swap livre:     %5d MB   (não usado nestas medições)\n' "$SWAP_LIVRE_MB"
printf 'tecto por medição: %d MB\n' "$Tecto_MB"
echo

# --- Uma medição -----------------------------------------------------------
#
# `mede <rótulo> <descrição> [VAR=VAL ...]`
#
# O `VAR=VAL` vai para o ambiente do `cargo`, e é o que diferencia as
# três configurações. Passá-los por cima de `env` em vez de os pôr no
# comando mantém o `cargo` intacto: o que muda é o perfil, não o que se
# pede ao cargo.
mede() {
    local rotulo="$1" descricao="$2"; shift 2

    local alvo
    alvo="$(mktemp -d -t onyxchat-arti.XXXXXX)"
    local pico_file
    pico_file="$(mktemp -t onyxchat-pico.XXXXXX)"

    printf '\n%s\n' "$(cor '1;33' "→ $rotulo — $descricao")"
    printf '  target: %s\n' "$alvo"

    local inicio
    inicio=$(date +%s)

    # As três camadas de `env` são distintas e a ordem não se inverte:
    #
    #   env (externo)   o que o *invólucro* precisa — `ONYXCHAT_PICO`.
    #                    Tem de vir antes do invólucro: o invólucro
    #                    executa literalmente o que recebe e não lê o
    #                    ambiente por si.
    #   env (interno)   o que o *cargo* precisa — perfil e paralelismo.
    #                    Tem de vir antes do `cargo`, e depois do
    #                    invólucro, que é quem o executa.
    #   "${COMANDO[@]}"  o cargo e as suas flags. Uma flag dentro de um
    #                    `env` é lida como nome de programa, e o
    #                    resultado é código 127 com uma mensagem que não
    #                    menciona o que correu mal.
    #
    # Os dois primeiros `env` podiam ser um só; são dois porque medem
    # coisas diferentes, e um `env` que faz as duas coisas escondia qual
    # das duas estava errada quando o `cargo` não arrancava.
    local codigo=0
    systemd-run --user --scope --quiet \
        -p "MemoryHigh=$(( Tecto_MB * 80 / 100 ))M" \
        -p "MemoryMax=${Tecto_MB}M" \
        -p MemorySwapMax=0 \
        -p OOMPolicy=kill \
        -- env ONYXCHAT_PICO="$pico_file" ONYXCHAT_TECTO="$Tecto_MB" \
            "$RAIZ/scripts/memoria-envolver.sh" \
            env CARGO_TARGET_DIR="$alvo" CARGO_BUILD_JOBS=1 "$@" \
            "${COMANDO[@]}" || codigo=$?

    local fim
    fim=$(date +%s)

    local pico=0
    if [[ -s "$pico_file" ]]; then
        pico="$(head -n1 "$pico_file")"
    fi
    rm -f "$pico_file"

    # Pico zero com código zero é um `cargo` que não chegou a correr —
    # e uma medição de 0 MB seria lida como «cabe», que é a peor
    # leitura possível. Tratar o zero como «não medido».
    local pico_valido=1
    if [[ "$pico" == "0" ]]; then
        pico_valido=0
    fi

    # O `target` é descartado de propósito. Um pico medido tem de ser
    # medido a compilar, e deixar 30 GB de incremental para trás para
    # não voltar a medir nunca é uma escolha que se faz aqui.
    rm -rf "$alvo"

    local segs=$(( fim - inicio ))
    if (( codigo != 0 )); then
        printf '%s  pico %d MB em %d s — build falhado (código %d)\n' \
            "$(cor '1;31' '✗')" "$pico" "$segs" "$codigo"
    elif (( pico_valido == 0 )); then
        printf '%s  pico não medido em %d s — o invólucro não escreveu\n' \
            "$(cor '1;31' '✗')" "$segs"
        printf '  %s\n' "isto é uma falha do script, não um build rápido." >&2
    else
        printf '%s  pico %d MB em %d s\n' "$(cor '1;32' '✓')" "$pico" "$segs"
    fi

    # A conta que decide: cabe no que há, com o editor aberto?
    if (( codigo == 0 && pico_valido == 1 )); then
        if (( pico <= MEM_UTIL_MB )); then
            printf '%s\n' \
                "$(cor '1;32' '  cabe') — ${pico} MB de ${MEM_UTIL_MB} MB utilizáveis"
        else
            printf '%s\n' \
                "$(cor '1;33' '  não cabe') — ${pico} MB contra ${MEM_UTIL_MB} MB utilizáveis"
        fi
    fi

    PICOS["$rotulo"]=$pico
    [[ $(( pico_valido == 1 )) -eq 1 ]] && MEDIU=1 || return 0
}

# --- As três configurações -------------------------------------------------

#: Os picos, por rótulo. Preenchidos por `mede` e lidos no sumário.
declare -A PICOS=()
#: Houve alguma medição válida? Sem isto o sumário recommendaria um
#: número que ninguém mediu.
MEDIU=0

# `-j1` em todas: as três medições têm de correr com o mesmo
# paralelismo, e uma medição com 2 jobs não se compara com uma com 1.
#
# O `-j1` entra como flag do `cargo` e não como `CARGO_BUILD_JOBS` no
# ambiente: o invólucro executa o que recebe como argumentos, e um
# `CARGO_BUILD_JOBS=1` solto seria lido como nome de comando.
COMANDO=(cargo check -p onyxchatd --features tor-real)

mede "A" "configuração actual (debug = line-tables-only)"

mede "B" "CARGO_PROFILE_DEV_DEBUG=none" "CARGO_PROFILE_DEV_DEBUG=none"

mede "C" "B mais CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=unpacked" \
    "CARGO_PROFILE_DEV_DEBUG=none" \
    "CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=unpacked"

# --- Sumário ---------------------------------------------------------------
#
# O sumário diz o que fazer, não só o que aconteceu. Um pico medido sem
# uma recomendação é um número; a mesma coisa com «esta serve» é uma
# decisão.
printf '\n%s\n' "$(cor '1;1' '── sumário ──')"
printf 'utilizável com o editor aberto: %d MB\n' "$MEM_UTIL_MB"
echo

if (( MEDIU == 0 )); then
    printf '%s\n' "$(cor '1;31' 'nenhuma medição foi válida')"
    printf '%s\n' "Não há número para Recommendar. Nenhum valor de"
    printf '%s\n' "PICO_ARTI_MB deve mudar com esta execução."
    exit 1
fi

printf '%s\n' "$(cor '1;1' 'picos ──')"
for rotulo in A B C; do
    pico="${PICOS[$rotulo]:-0}"
    [[ "$pico" == "0" ]] && continue
    if (( pico <= MEM_UTIL_MB )); then
        printf '  %s  %5d MB  %s\n' "$rotulo" "$pico" "$(cor '1;32' 'cabe')"
    else
        printf '  %s  %5d MB  %s\n' "$rotulo" "$pico" "$(cor '1;33' 'não cabe')"
    fi
done
echo

# O menor pico que cabe é o que entra no gate. «Menor» e não «o último»
# porque a ordem A→B→C é crescente em agressividade, e o que
# interessa é a configuração mais barata que serve — não a mais
# agressiva que calhou servir.
MELHOR=0
for rotulo in A B C; do
    pico="${PICOS[$rotulo]:-0}"
    [[ "$pico" == "0" ]] && continue
    (( pico <= MEM_UTIL_MB )) || continue
    if (( MELHOR == 0 || pico < MELHOR )); then
        MELHOR=$pico
        MELHOR_ROTULO="$rotulo"
    fi
done

if (( MELHOR > 0 )); then
    printf '%s\n' "$(cor '1;32' "configuração $MELHOR_ROTULO serve") — pico ${MELHOR} MB"
    printf '\n%s\n' "Actualizar scripts/memoria.sh:"
    printf '  PICO_ARTI_MB=%d        # era 1156\n' "$MELHOR"
    echo
    printf '%s\n' "Ver docs/DEV_GUIDE.md secao 1.2 para onde isto entra."
else
    printf '%s\n' "$(cor '1;33' 'nenhuma configuração cabe') —Flags não chegam."
    printf '\n%s\n' "O passo seguinte é o gate contar RAM + swap no tecto total."
    printf '%s\n' "Ver scripts/memoria.sh, secao decidir_jobs, e docs/DEV_GUIDE.md secao 1.2."
fi