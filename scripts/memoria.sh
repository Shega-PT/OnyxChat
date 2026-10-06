#!/usr/bin/env bash
# =====================================================================
# memoria.sh — gate de memória do OnyxChat
# ---------------------------------------------------------------------
# Existe porque a optimização por ajuste de `-j` não funciona.
#
# O que aconteceu em 2026-10-05, registado pelo kernel:
#
#   17:32:06  Killed process 837375 (rust-analyzer) anon-rss:679848kB
#   18:25:57  Killed process 858658 (rust-analyzer) anon-rss:446448kB
#   16:44:16  Killed process 917104 (rust-analyzer) anon-rss:1014148kB
#             snap.code...scope: Failed with result 'oom-kill'
#
# Três OOM kills, e as três vítimas foram o `rust-analyzer`, nunca o
# `rustc`. No momento do OOM das 16:44, o build era minúsculo:
# `cargo` com 26 MB e dois `rustc` com 39 e 41 MB. O que esgotou a
# máquina foi a SOMA — e o `rust-analyzer`, com 990 MB, era o elo mais
# gordo. Como o OOM killer escolhe a vítima por `oom_score`, matou o
# editor em vez do build.
#
# Duas conclusões, e são as que este script implementa:
#
#   1. `-j` não é um controlo de memória. Reduzi-lo de 4 para 2 não
#      impede que o kernel mate o editor: impede que o `rustc` seja a
#      vítima, o que não interessa. A unidade correcta é `MemAvailable`
#      menos o que o desktop já ocupa — não a RAM total.
#
#   2. O build tem de ser CONFINADO. Este script corre o cargo dentro
#      de uma scope de cgroup v2 com `MemoryMax` e `OOMPolicy=kill`.
#      Com o tecto posto, o OOM passa a ocorrer DENTRO da scope e a
#      matar o `rustc` — o `rust-analyzer`, o VSCode e o resto do
#      sistema ficam fora do alcance. É isto, e não `-j`, que impede o
#      editor de ser sacrificado.
#
# Modos de uso:
#
#   ./scripts/memoria.sh                      # relatório, não executa
#   ./scripts/memoria.sh --jobs CMD           # escolhe jobs e executa
#   ./scripts/memoria.sh --pico CMD           # executa com tecto e mede
#   ./scripts/memoria.sh --pico --  CMD       # idem, com `--` explícito
#
# O comando é executado tal como foi dado: o script não reinterpreta
# as suas flags, apenas as calcula e injecta `-j`.
#
# Referência: `docs/DEV_GUIDE.md` §1.2 e `docs/testing.md`
# §Gate de hardware.
# =====================================================================

set -o errexit
set -o nounset
set -o pipefail

# Raiz do projecto (a pasta que contém `Cargo.toml`).
RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# --- Constantes de calibração ---------------------------------------------
#
# CRESCIMENTO_DESKTOP_MB: quanto se espera que o desktop CRESÇA durante
# o build, em megabytes.
#
#   Isto é pequeno, e é o ponto que quase custou o gate inteiro.
#
#   `MemAvailable` já é, por definição, a memória que o kernel estima
#   poder entregar sem swap — depois de descontar tudo o que já está a
#   correr. Medido aqui: com o editor aberto e o `rust-analyzer`
#   removido, `MemAvailable` = 1407 MB, e é essa a fatia real
#   disponível para o build.
#
#   A primeira versão deste script subtraía 2200 MB — o RSS total do
#   editor — e concluía que havia 0 MB. Era dupla contagem: contava
#   como reservado aquilo que `MemAvailable` já tinha excluído. O
#   resultado era um gate que recusava tudo.
#
#   O que sobra para reservar é só o CRESCIMENTO: o editor a indexar
#   ficheiros enquanto o build escreve em `target/`, a cache do linker,
#   buffers de log. Centenas de MB, não gigabytes.
#
#   Para comparação, o que realmente consumia memória era o
#   `rust-analyzer` (446 MB a 1,0 GB de RSS, três OOM kills) — e esse
#   já foi removido. `MemAvailable` subiu de 548 MB para 1407 MB.
CRESCIMENTO_DESKTOP_MB=350

# PICO_POR_BUILD_MB: pico medido de RAM para UM job, por perfil de
# workload. Não são estimativas: são leituras de `memory.peak` feitas
# dentro da scope de cgroup, com o invólucro `memoria-envolver.sh`.
#
# Máquina de medição: i3-5005U, 2 núcleos físicos / 4 threads, 3,8 GB de
# RAM, `opt-level = 0`, `debug = line-tables-only`, `-j1`.
#
#   PERFIL_LEVE   cargo test --workspace --no-default-features ...
#                 103 crates, suite completa         pico  547 MB
#                                               (medido três vezes em
#                                               condições diferentes: 314,
#                                               384 e 547 MB — o pico real
#                                               depende do que mudou, por
#                                               isso se toma o maior)
#   PERFIL_ARTI   cargo check -p onyxchatd --features tor-real
#                 458 crates                          pico 1156 MB
#
# A diferença entre os dois é que a árvore da arti não beneficiou das
# correcções do perfil leve: `mlua` e `lua-src` são também compilados
# com `--features tor-real`, mas o pico da arti é dominado pelos crates
# pesados de streaming (tokio, rustls, zstd, icu), que não mudaram.
#
# Por que dois valores e não uma fórmula sobre o número de crates: o
# pico NÃO cresce linearmente com os crates. Nos 103 crates do workspace
# leve o crate mais pesado manda; acrescentar os 355 da arti acrescenta
# dependências de streaming (tokio, rustls, zstd, icu) que pesam mais,
# mas nenhuma o bastante para dobrar. Um `pico = crates * X` erraria por
# factor de três em ambos os extremos — e o gate ficaria ou demasiado
# permissivo ou recusaria builds que cabem.
#
# Estes números são de uma máquina. Em outra, o comando `--medir`
# mede e o valor deve ser substituído.
#
# O valor de `PICO_LEVE_MB` foi revisto em 2026-10-05, depois de três
# correcções que só depois de aplicadasSe podem medir:
#
#   * `MAX_PAYLOAD` deixou de ser 16 MiB e passou a ser derivado de um
#     orçamento (`network/daemon_rust/src/orcamento.rs`). Três testes
#     alocavam `[0u8; 16 MiB]` na **pilha**, contra um `ulimit -s` de
#     8 MB: o binário de testes abortava em vez de reportar a falha;
#   * `lua_camadas` passou a reutilizar um interpretador por thread em
#     vez de instanciar 8 por ciclo completo do pipeline;
#   * `lua-src` deixou de herdar `opt-level = 0` do perfil de
#     desenvolvimento.
#
# A medição é do build a partir do zero com a feature `docs`
# (`cargo test --no-run`), lida de dentro da scope pelo invólucro.
PICO_LEVE_MB=547
PICO_ARTI_MB=1156

# MARGEM_SEGURANCA_MB: folga sobre o pico medido.
#
# Cobre o que o `memory.peak` de um build já feito não mede: a fase de
# link (o `cc` a ligar um binário de 48 MB pediu centenas de MB) e o
# agrupamento das fixtures de teste num único processo. 250 MB medidos
# como folga confortável, 400 se a máquina tem o editor aberto.
MARGEM_SEGURANCA_MB=300

# `MAX_JOBS` não é usado: o tecto de jobs vem dos núcleos físicos
# (`nucleos_fisicos`), que é o que importa, e não de uma constante.

# --- Instrumentação --------------------------------------------------------

PASSOU=0
FALHOU=0

# Só escreve cor se a saída for um terminal — em log de CI, os códigos
# ANSI são lixo.
cor() {
    if [[ -t 1 ]]; then
        printf '\033[%sm%s\033[0m' "$1" "$2"
    else
        printf '%s' "$2"
    fi
}

erro() { printf '%s %s\n' "$(cor '1;31' 'erro:')" "$1" >&2; }
aviso() { printf '%s %s\n' "$(cor '1;33' 'aviso:')" "$1" >&2; }
info() { printf '%s %s\n' "$(cor '1;36' '·')" "$1"; }

# --- Leitura da memória ----------------------------------------------------

# Lê um campo de /proc/meminfo em kilobytes e devolve megabytes.
#
# `MemAvailable` e não `MemFree`: `MemFree` ignora o page cache, que o
# kernel pode recuperar sem trocar. `MemAvailable` é a estimativa do
# próprio kernel de quanto se pode obter sem swap — que é exactamente
# a pergunta que importa aqui.
mem_mb() {
    local chave="$1"
    awk -v k="$chave" '
        $1 == k":" { printf "%d", ($2 / 1024); exit }
    ' /proc/meminfo
}

# Número de núcleos físicos (não threads lógicas).
#
# `nproc` conta threads. O `rustc` não beneficia de hyper-threading:
# cada thread adicional é um `rustc` a mais em paralelo, ou seja mais
# RAM, e não mais trabalho. Por isso se conta o que há em `cpu cores`.
nucleos_fisicos() {
    # sysfs é a fonte mais fiável: `/sys/devices/system/cpu/cpuN/topology/`
    # expõe `core_id` e `physical_package_id` por CPU lógica. Contar os
    # pares (package, core) distintos dá o número de núcleos físicos,
    # que é o que interessa — `nproc` conta threads e o `rustc` não
    # beneficia de hyper-threading: cada thread extra é um `rustc` a
    # mais em paralelo, ou seja mais RAM, não mais trabalho.
    #
    # `/proc/cpuinfo` é o plano B: `cpu cores` × sockets. Nesta máquina
    # o kernel NÃO expõe `cpu cores` (só `physical id`), o que é a
    # razão de o plano A existir.
    local n
    n="$(
        for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
            local pkg core
            pkg="$(cat "$cpu/topology/physical_package_id" 2>/dev/null || echo '?')"
            core="$(cat "$cpu/topology/core_id" 2>/dev/null || echo '?')"
            [[ -n "$core" ]] && printf '%s:%s\n' "$pkg" "$core"
        done | sort -u | wc -l
    )"

    if [[ -z "$n" ]] || [[ "$n" == "0" ]]; then
        local cores
        cores="$(awk -F: '/^cpu cores/ { gsub(/ /,"",$2); print $2; exit }' /proc/cpuinfo 2>/dev/null || true)"
        if [[ -n "$cores" ]]; then
            n="$cores"
        else
            # Último recurso: threads. Sobrestimar aqui só torna o gate
            # mais conservador, que é o erro seguro.
            n="$(nproc 2>/dev/null || echo 1)"
        fi
    fi

    printf '%s' "$n"
}

# --- Relatório -------------------------------------------------------------

relatorio() {
    local total avail swap_livre nucleos
    local pico="${1:-$PICO_LEVE_MB}"
    total="$(mem_mb MemTotal)"
    avail="$(mem_mb MemAvailable)"
    swap_livre="$(mem_mb SwapFree)"
    nucleos="$(nucleos_fisicos)"

    local util=$((avail - CRESCIMENTO_DESKTOP_MB))
    if (( util < 0 )); then util=0; fi

    printf '\n%s\n' "$(cor '1;1' '── estado de memória ──')"
    printf 'RAM total:        %6s MB\n' "$total"
    printf 'disponível:       %6s MB   <-- o que o kernel diz que dá\n' "$avail"
    printf 'swap livre:       %6s MB   (lento; não conta como memória útil)\n' "$swap_livre"
    printf 'núcleos físicos:  %6s   (threads lógicas: %s)\n' "$nucleos" "$(nproc)"

    printf '\n%s\n' "$(cor '1;1' '── balanço ──')"
    printf 'utilizável:       %6s MB   (disponível − %s de crescimento)\n' \
        "$util" "$CRESCIMENTO_DESKTOP_MB"
    printf 'pico deste build: %6s MB   (medido, não estimado)\n' "$pico"
}

# Recusa o build, e diz exactamente o que falta.
#
# Recusar em vez de começar é deliberado. Um build que sabemos que vai
# ser morto pelo tecto a meio não poupa nada: gasta minutos de CPU,
# deixa `target/` inconsistente e, no pior caso, dá a impressão de que
# a máquina está avariada. Dizer «fecha o editor» poupa o tempo do
# utilizador.
recusar_build() {
    local pico="$1"
    local unidade=$((pico + MARGEM_SEGURANCA_MB))
    printf '%s\n' "$(cor '1;31' 'recusando correr o build')"
    printf '  disponível      %5d MB\n' "$(mem_mb MemAvailable)"
    printf '  - crescimento    %5d MB\n' "$CRESCIMENTO_DESKTOP_MB"
    printf '  = utilizável     %5d MB\n' "$(( $(mem_mb MemAvailable) - CRESCIMENTO_DESKTOP_MB ))"
    printf '  este build       %5d MB  (%d de pico medido + %d de margem)\n' \
        "$unidade" "$pico" "$MARGEM_SEGURANCA_MB"
    printf '\n'
    printf '  o build não vai começar a meio. Para libertar memória:\n'
    printf '    · fecha o editor (o maior ganho isolado: ~600 MB)\n'
    printf '    · ou espera que a pressão de memória baixe sozinha\n'
}

# Decide o número de jobs a usar, e diz porquê.
#
# Não é uma regra fixa: é uma resposta à memória que existe agora. Com
# a máquina livre sobe a 2; com o editor a comer sobe a 1; sem memória
# disponível, recusa. A regra antiga (`jobs <= RAM_em_GB / 1.5`, em
# `docs/DEV_GUIDE.md` §1.2) dava 2 nesta máquina e 2 foi o que matou o
# editor — porque a RAM total não diz nada sobre quanto sobra.
# Decide quantos jobs, dado o pico medido do workload.
#
# Um job é o piso. O segundo só entra se a máquina tiver a sobra para
# dois, e só há jobs para correr se a máquina tiver núcleos.
#
# Recusar, e não começar, é deliberado: um build que sabemos que vai ser
# morto pelo tecto a meio não poupa nada — gasta minutos de CPU e deixa
# `target/` inconsistente.
decidir_jobs() {
    local pico="$1"
    local avail nucleos util
    avail="$(mem_mb MemAvailable)"
    nucleos="$(nucleos_fisicos)"
    util=$((avail - CRESCIMENTO_DESKTOP_MB))
    if (( util < 0 )); then util=0; fi

    local unidade=$((pico + MARGEM_SEGURANCA_MB))
    PICO_DO_BUILD_MB="$pico"
    UNIDADE_MB="$unidade"

    if (( util < unidade )); then
        RECUSA=1
        JOBS=0
        return
    fi

    RECUSA=0
    if (( nucleos >= 2 && util >= 2 * unidade )); then
        JOBS=2
    else
        JOBS=1
    fi
}

# Detecta de que perfil é um comando, olhando para as features.
#
# A distinção importa porque os dois workloads têm picos muito
# diferentes (547 MB contra 1156 MB) e decidir com o valor errado é
# errar por 275 MB — ou recusar um build que cabe, ou aceitar um que não
# cabe.
perfil_do_comando() {
    local arg
    for arg in "$@"; do
        case "$arg" in
            --features=*|--features)
                case "$arg" in
                    *tor-real*) printf 'arti'; return ;;
                esac
                ;;
            *tor-real*) printf 'arti'; return ;;
        esac
    done
    printf 'leve'
}

# Falha se a scope não pôde ser criada, ou se as propriedades não
# foram aceites. Verificado nesta máquina que cgroup v2 + systemd 255
# aceitam MemoryHigh/MemoryMax/MemorySwapMax/OOMPolicy, mas uma máquina
# sem `systemd` a correr, ou sem cgroup v2, não tem onde-meter o
# tecto — e nesse caso o gate tem de dizer isso em vez de fingir que
# protegeu alguma coisa.
scope_suportada() {
    local stat
    stat="$(stat -fc %T /sys/fs/cgroup 2>/dev/null || true)"
    [[ "$stat" == "cgroup2fs" ]] || return 1
    command -v systemd-run >/dev/null 2>&1 || return 1
    return 0
}

# --- Execução confinada ----------------------------------------------------

# Corre CMD dentro de uma scope de cgroup v2 com tecto de memória, e
# devolve o pico de RAM efectivamente consumido.
#
# A propriedade decisiva é `OOMPolicy=kill`: quando a scope atinge o
# `MemoryMax`, o kernel mata um processo DENTRO dela. Como o `cargo` e
# os `rustc` são os únicos nessa scope, o que morre é o build. O
# editor está noutra scope e não entra na contagem.
#
# `MemoryHigh` abaixo de `MemoryMax` é intencional: o reclaim começa
# em `MemoryHigh`, o que é reversível, em vez de na alocação, o que
# mata. `MemorySwapMax` limita o swap da scope — um build que enche
# 2 GB de swap deixa a máquina inteira lenta, mesmo que não morra.
#
# Verificado nesta máquina: cgroup v2, systemd 255, e o caminho da
# scope resolve-se de dentro dela via /proc/self/cgroup.
executar_confinado() {
    local tecto="$1"
    shift

    # O pico tem de ser lido DE DENTRO da scope, e não de fora.
    #
    # A primeira versão resolvia o caminho por `/proc/self/cgroup` antes
    # do `systemd-run` e lia `memory.peak` a seguir — o que media a
    # scope do opencode, não a scope do build. Dava 1130 MB para
    # `/usr/bin/true`, que não consome nada. A leitura tem de ocorrer
    # enquanto o processo ainda está dentro da scope.
    #
    # Por isso o comando é envolvido: o invólucro imprime o pico para
    # um ficheiro antes de sair, e quem o invoca lê o ficheiro depois.
    local ficheiro_pico
    ficheiro_pico="$(mktemp -t onyxchat-pico.XXXXXX)"

    local inv="$RAIZ/scripts/memoria-envolver.sh"
    local codigo=0
    ONYXCHAT_PICO="$ficheiro_pico" ONYXCHAT_TECTO="$tecto" \
        systemd-run --user --scope --quiet \
            -p "MemoryHigh=$((tecto * 80 / 100))M" \
            -p "MemoryMax=${tecto}M" \
            -p "MemorySwapMax=${tecto}M" \
            -p OOMPolicy=kill \
            -- "$inv" "$@" || codigo=$?

    # Lê o pico que o invólucro escreveu. Se não houver ficheiro, o
    # invólucro não chegou a correr — tipicamente porque a scope não
    # pôde ser criada.
    PICO_MB=0
    if [[ -s "$ficheiro_pico" ]]; then
        PICO_MB="$(head -n1 "$ficheiro_pico")"
    fi
    rm -f "$ficheiro_pico"

    # `return` explícito e obrigatório. Sem ele, o estado de saída da
    # função é o da última instrução executada — a atribuição acima, que
    # é sempre 0. O chamador fazia `RETORNO=$?`, que lia 0, e o gate
    # reportava «✓ passou» com o cargo a devolver 101.
    #
    # Isto não era teórico: um teste de guarda a falhar passou pelo gate
    # como verde, porque o gate media memória, não.testes.
    return "$codigo"
}

# --- Análise dos argumentos ------------------------------------------------

MODO=relatorio
CMD=()

while (( $# > 0 )); do
    case "$1" in
        --pico)
            MODO=pico
            shift
            # Um `--` explícito separa as flags do script das do
            # comando. Sem ele, o primeiro argumento que não seja uma
            # opção do gate é o início do comando.
            if [[ "${1:-}" == "--" ]]; then
                shift
                CMD=("$@")
                break
            fi
            ;;
        --jobs)
            MODO=jobs
            shift
            if [[ "${1:-}" == "--" ]]; then
                shift
                CMD=("$@")
                break
            fi
            ;;
        -h|--help)
            sed -n '2,45p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        --)
            shift
            CMD=("$@")
            break
            ;;
        *)
            CMD=("$@")
            break
            ;;
    esac
done

if [[ "${#CMD[@]}" -eq 0 && "$MODO" != "relatorio" ]]; then
    erro "modo --${MODO} sem comando"
    printf 'uso: %s [--pico|--jobs] CMD\n' "$(basename "$0")" >&2
    exit 2
fi

# --- Relatório (por omissão) ----------------------------------------------
#
# Correr sem nenhum argumento é o caso normal: imprime o estado da
# memória e a decisão que o gate tomaria, sem construir nada. É o que
# se quer ao abrir um terminal e perguntar «posso compilar agora?».

if [[ "$MODO" == "relatorio" ]]; then
    PERFIL="$(perfil_do_comando "${CMD[@]+"${CMD[@]}"}")"
    if [[ "$PERFIL" == "arti" ]]; then
        PICO="$PICO_ARTI_MB"
    else
        PICO="$PICO_LEVE_MB"
    fi

    relatorio "$PICO"
    decidir_jobs "$PICO"
    echo

    local_disponivel="$(mem_mb MemAvailable)"
    local_util=$(( local_disponivel - CRESCIMENTO_DESKTOP_MB ))

    if [[ "$PERFIL" == "arti" ]]; then
        printf 'perfil: árvore da arti, 458 crates (%d MB de pico medido)\n' "$PICO"
    else
        printf 'perfil: workspace leve, 103 crates (%d MB de pico medido)\n' "$PICO"
    fi

    if (( RECUSA )); then
        printf '%s\n' "$(cor '1;31' 'nem um job cabe')"
        printf '  este build exige %d MB (%d de pico + %d de margem)\n' \
            "$UNIDADE_MB" "$PICO" "$MARGEM_SEGURANCA_MB"
        printf '  fecha o editor, ou espera a pressão de memória baixar\n'
    else
        printf '%s %d job%s de %s núcleos físicos\n' \
            "$(cor '1;32' 'jobs =')" "$JOBS" \
            "$( ((JOBS == 1)) && echo '' || echo 's')" "$(nucleos_fisicos)"
        printf '  %d MB disponíveis − %d MB de crescimento = %d MB utilizáveis\n' \
            "$local_disponivel" "$CRESCIMENTO_DESKTOP_MB" "$local_util"
        printf '  cada job exige %d MB → cabem %d\n' \
            "$UNIDADE_MB" "$(( local_util / UNIDADE_MB ))"
    fi
    echo
    exit 0
fi

# --- Modo --jobs: escolhe o paralelismo e executa --------------------------

if [[ "$MODO" == "jobs" ]]; then
    PERFIL="$(perfil_do_comando "${CMD[@]+"${CMD[@]}"}")"
    if [[ "$PERFIL" == "arti" ]]; then
        PICO="$PICO_ARTI_MB"
    else
        PICO="$PICO_LEVE_MB"
    fi

    relatorio "$PICO"
    decidir_jobs "$PICO"
    echo

    if (( RECUSA )); then
        recusar_build "$PICO"
        exit 1
    fi

    printf '%s %s (%d job(s))\n\n' "$(cor '1;32' '→')" "${CMD[*]}" "$JOBS"
    JOBS="$JOBS" "$@"
    exit $?
fi

# --- Modo --pico: executa com tecto e mede o pico --------------------------

PERFIL="$(perfil_do_comando "${CMD[@]+"${CMD[@]}"}")"
if [[ "$PERFIL" == "arti" ]]; then
    PICO="$PICO_ARTI_MB"
else
    PICO="$PICO_LEVE_MB"
fi

relatorio "$PICO"
decidir_jobs "$PICO"
echo

if (( RECUSA )); then
    recusar_build "$PICO"
    exit 1
fi

# Tecto = o que sobra, menos a margem, mais uma unidade por job acima do
# primeiro. A margem é o que o linker e as fixtures pedem.
# Tecto = o pico medido, mais a margem, mais uma unidade por cada job
# acima do primeiro. O primeiro job não acrescenta nada, porque é ele
# que dá nome ao pico medido.
util=$(( $(mem_mb MemAvailable) - CRESCIMENTO_DESKTOP_MB ))
TECTO_MB=$(( PICO + MARGEM_SEGURANCA_MB + (JOBS - 1) * PICO ))
if (( TECTO_MB < UNIDADE_MB )); then
    TECTO_MB="$UNIDADE_MB"
fi

# Sem cgroup v2 não há onde pôr o tecto, e o modo --pico seria uma
# promessa vazia. Dizer isso é melhor do que correr o build a achar
# que está protegido.
if ! scope_suportada; then
    printf '%s\n' "$(cor '1;31' 'erro:')" "sem cgroup v2 ou sem systemd-run — não há como confinar o build"
    printf '  este gate existe para que o OOM mate o build e não o editor.\n'
    printf '  sem ele, o kernel escolhe a vítima pelo oom_score, e o editor perde.\n'
    printf ' Alternativa: correr com --jobs %d, que é mais fraco mas sempre funciona.\n' "$JOBS"
    exit 1
fi

printf '%s tecto de %d MB (MemHigh %d MB, MemMax %d MB, OOMPolicy=kill)\n' \
    "$(cor '1;33' '→')" "$TECTO_MB" "$(( TECTO_MB * 80 / 100 ))" "$TECTO_MB"
printf '%s %s\n\n' "$(cor '1;32' '→')" "${CMD[*]}"

JOBS="$JOBS" executar_confinado "$TECTO_MB" "$@"
RETORNO=$?

echo
if (( RETORNO == 0 )); then
    printf '%s %s\n' "$(cor '1;32' '✓')" "passou (pico ${PICO_MB} MB de ${TECTO_MB} MB)"
    PASSOU=$((PASSOU + 1))
elif (( RETORNO == 137 )); then
    # 137 = 128 + 9 = SIGKILL. É a scope a ser morta pelo `MemoryMax`,
    # que é exactamente o que este script existe para fazer: o build
    # morre, o editor não.
    printf '%s %s\n' "$(cor '1;31' '✗')" "morto pelo tecto de memória (pico ${PICO_MB} MB de ${TECTO_MB} MB)"
    printf '  o build foi sacrificado, o editor não — este é o comportamento pretendido\n'
    printf '  para caber: fecha o editor, ou reduz o número de jobs, ou divide o build\n'
    FALHOU=$((FALHOU + 1))
else
    printf '%s %s (código %d, pico %d MB)\n' "$(cor '1;31' '✗')" "falhou" "$RETORNO" "$PICO_MB"
    FALHOU=$((FALHOU + 1))
fi

exit "$RETORNO"