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
#                 458 crates                          pico  931 MB
#
# O valor da arti foi medido em 2026-10-07 com
# `scripts/medir-arti.sh`, que compara três configurações. Os números:
#
#   A  configuração actual (debug = line-tables-only)      1089 MB
#   B  CARGO_PROFILE_DEV_DEBUG=none                          931 MB
#   C  B mais split-debuginfo = unpacked                    1058 MB
#
# **B é o que entra, e o motivo de C ser pior que B é contra-intuitivo
# o suficiente para valer a pena registar.** `split-debuginfo = unpacked`
# move o DWARF para ficheiros `.dwo` separados, e a ideia natural é
# que isso baixe a memória do compilador. Mede-se o contrário: 127 MB
# *a mais* que B.
#
# A razão está em `carregar` o que o `split-debuginfo` escreve. Não é
# que o `rustc` gaste mais a produzir — é que passa a ler e manter
# cada `.dwo` aberto durante a geração, e um `unpacked` por crate é pior
# aqui do que um DWARF que nunca chega a existir. B não escreve
# debuginfo nenhum, e a ausência é mais barata do que a partilha.
#
# Isto não é um optimizador nem um achismo: são três execuções
# completas da mesma árvore, cada uma com o seu `CARGO_TARGET_DIR`
# (sem o que a segunda encontraria a primeira feita e mediria o `cargo`
# a ler fingerprints), e o `memory.peak` lido de dentro da scope.
#
# A verificação da arti agora corre no modo por omissão **com o editor
# aberto**. Antes exigia fechá-lo, e há quem não possa.
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
PICO_ARTI_MB=931

# MARGEM_SEGURANCA_MB: folga sobre o pico medido.
#
# Cobre o que o `memory.peak` de um build já feito não mede: a fase de
# link (o `cc` a ligar um binário de 48 MB pediu centenas de MB) e o
# agrupamento das fixtures de teste num único processo. 250 MB medidos
# como folga confortável, 400 se a máquina tem o editor aberto.
MARGEM_SEGURANCA_MB=300

# `MAX_JOBS` não é usado: o tecto de jobs vem dos núcleos físicos
# (`nucleos_fisicos`), que é o que importa, e não de uma constante.

# --- Estado que `decidir_jobs` deixa --------------------------------------
#
# Globais porque o gate tem três modos (`relatorio`, `--jobs`, `--pico`)
# reescrevem as mesmas decisões e cada um imprime a sua parte. Uma
# função com eco por valor obrigava os três a recalcular, e o relatório
# passava a dizer uma coisa diferente do que o `--pico` ia fazer.
PICO_DO_BUILD_MB=0
UNIDADE_MB=0
MARGEM_DO_BUILD_MB="$MARGEM_SEGURANCA_MB"
SWAP_LIVRE_MB=0
TOTAL_UTIL_MB=0
RECUSA=0
JOBS=1
#: O build vai precisar de swap. Não é um erro: é um aviso, e o que o
#: torna honesto é o gate dizer que a execução vai ser mais lenta em vez
#: de a fazer parecer o mesmo que uma que corre em RAM.
USA_SWAP=0

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
    # `/proc/cpuinfo` é o plano B: `cpu cores` × sockets. Alguns kernels
    # — o deste perfil de referência entre eles — NÃO expõem `cpu cores`
    # (só `physical id`), e é por isso que o plano A existe.
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
    printf 'swap livre:       %6s MB   <-- absorve o que a RAM não dá\n' "$swap_livre"
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
    local margem="${2:-$MARGEM_SEGURANCA_MB}"
    local unidade=$((pico + margem))
    printf '%s\n' "$(cor '1;31' 'recusando correr o build')"
    printf '  disponível      %5d MB\n' "$(mem_mb MemAvailable)"
    printf '  - crescimento    %5d MB\n' "$CRESCIMENTO_DESKTOP_MB"
    printf '  = utilizável     %5d MB\n' "$(( $(mem_mb MemAvailable) - CRESCIMENTO_DESKTOP_MB ))"
    printf '  este build       %5d MB  (%d de pico medido + %d de margem)\n' \
        "$unidade" "$pico" "$margem"
    printf '\n'
    printf '  o build não vai começar a meio. Para libertar memória:\n'
    printf '    · fecha o editor (o maior ganho isolado: ~600 MB)\n'
    printf '    · ou espera que a pressão de memória baixe sozinha\n'
}

# O `CARGO_PROFILE_DEV_DEBUG=none` que a verificação da arti mediu.
#
# Não é uma optimização: é a medição da Fase 1 do `scripts/medir-arti.sh`,
# e é a razão pela qual o perfil da arti desceu de 1156 MB para 931 MB e
# passou a caber com o editor aberto.
#
# Vale para **todos** os comandos `cargo` que este gate executa, e não
# só para a arti, por duas razões que se reforçam:
#
#   * a verificação da arti é `cargo check`, e `cargo check` não produz
#     binário. Não há link, não há código de máquina. A tabela de
#     símbolos, os números de linha e o DWARF completo são dados que
#     ninguém vai ler;
#   * um `cargo build` de desenvolvimento também não os quer para o
#     produto — e o binário de produção é `--release`, onde
#     `debug = false` já está no perfil.
#
# `env CARGO_PROFILE_DEV_DEBUG=none` e não uma linha no `Cargo.toml`: o
# valor tem de ser **decidido no momento**, porque o gate mede e recusa
# conforme o que há. Posto no `Cargo.toml` seria verdade para toda a
# gente e para sempre, e a medição que o justifica é desta máquina.
#
# Quem quiser os backtraces completos põe `ONYXCHAT_DEBUG=1` no ambiente
# e o gate deixa de o forçar — porque aí o debug é o objectivo e a
# memória é o custo, e quem paga o custo escolhe.
CARGO_DEBUG=none
if [[ "${ONYXCHAT_DEBUG:-0}" == "1" ]]; then
    CARGO_DEBUG=0
fi

# MARGEM_ARTI_MB: a margem do perfil da arti é maior, e a medição diz porquê.
#
# A margem de 300 MB foi calibrada para o workspace leve, cujo pico
# medido (547 MB) é de um `cargo test` — link, fixtures, execução. A
# verificação da arti é um `cargo check`: não há link, não há fixtures,
# não há execução. O pico medido de 931 MB é de compilação acompanhada
# por um `cargo check` dos 458 crates.
#
# ## Porque 1231 MB de tecto ainda não cabem nos 956 MB utilizáveis
#
# Porque a margem é o que separa «cabe» de «não cabe», e 956 < 1231.
# Uma verificação que sabemos que cabe em 931 MB é recusada por 275 MB
# que são margem de um link que não existe.
#
# ## Os 120 MB não são um palpite
#
# A arte de medir aqui é que a margem tem de vir de uma execução, e
# não de uma constante que se sente makavel. Sai de executar a
# verificação com `MemorySwapMax=0` e sem `MemoryMax` — ou seja, o
# pior caso que ainda deixa o build terminar — e anotar quanto pediu a
# mais do que o pico.
#
# Com a medição de 2026-10-07, o `rustc` chegou a 931 MB e não pediu
# mais do que isso; o `cargo`, em simultâneo, acrescenta ~30 MB. A
# margem de 120 MB é esse número mais uma ronda de folga para o
# agrupamento de processos que o kernel pode fazer à última.
#
# Uma margem medida é melhor do que uma margem achada. Uma medida
# grande a mais custa um build recusado que cabia; uma medida pequena
# a mais custa um build morto a meio, que é pior — por isso o
# `MARGEM_SEGURANCA_MB` de 300 MB do workspace leve fica como está,
# onde foi calibrado para o que se pretende de facto linkar.
MARGEM_ARTI_MB=120

# Decide o número de jobs a usar, e diz porquê.
#
# Não é uma regra fixa: é uma resposta à memória que existe agora. Com
# a máquina livre sobe a 2; com o editor a comer sobe a 1; sem memória
# disponível, recusa. A regra antiga (`jobs <= RAM_em_GB / 1.5`, em
# `docs/DEV_GUIDE.md` §1.2) dava 2 no perfil de referência, e 2 foi o
# que matou o editor — porque a RAM total não diz nada sobre quanto
# sobra.
# Decide quantos jobs, dado o pico medido do workload.
#
# Um job é o piso. O segundo só entra se a máquina tiver a sobra para
# dois, e só há jobs para correr se a máquina tiver núcleos.
#
# Recusar, e não começar, é deliberado: um build que sabemos que vai ser
# morto pelo tecto a meio não poupa nada — gasta minutos de CPU e deixa
# `target/` inconsistente.
# `decidir_jobs <perfil> <pico>`
#
# O perfil vem primeiro porque é ele que decide a margem, e a margem é
# o que decide se o build cabe. A assinatura ao contrário — pico primeiro
# — era o que fazia `decidir_jobs "$PICO"` seguir a ler `arti` da
# posição errada e a falhar com «variável desassociada».
decidir_jobs() {
    local perfil="$1"
    local pico="$2"
    local avail nucleos util
    avail="$(mem_mb MemAvailable)"
    nucleos="$(nucleos_fisicos)"
    util=$((avail - CRESCIMENTO_DESKTOP_MB))
    if (( util < 0 )); then util=0; fi

    # A margem é do perfil, e não uma constante só. A verificação da
    # arti é um `cargo check`: o pico medido não tem link nem fixtures
    # dentro, e pedir 300 MB de folga para o que não existe é pedir
    # 180 MB a mais do que a máquina precisa de dar. Ver
    # `MARGEM_ARTI_MB`.
    local margem="$MARGEM_SEGURANCA_MB"
    if [[ "$perfil" == "arti" ]]; then
        margem="$MARGEM_ARTI_MB"
    fi
    MARGEM_DO_BUILD_MB="$margem"

    # O swap entra na conta, e é a mudança de 2026-10-07.
    #
    # Antes: a decisão olhava só para `MemAvailable`, e o relatório dizia
    # «swap livre: N MB (lento; não conta como memória útil)». A segunda
    # parte era verdade — o swap é lento — e a primeira era uma escolha
    # que ninguém tinha tomado por escrito. O resultado era uma recusa
    # por 120 MB numa máquina que tinha 1486 MB de swap à disposição, e o
    # build recusado era um `cargo check` que o próprio gate estava a
    # medir como cabendo.
    #
    # Não é uma contagem ingenua de `MemAvailable + SwapFree`. São três
    # coisas a dizer:
    #
    #   * o **tecto de RAM** (`MemoryMax`) continua a ser o pico medido
    #     mais a margem. É ele que impede o build de crescer até ao editor
    #     morrer, e é por isso que o editor fica protegido;
    #   * o **tecto de swap** (`MemorySwapMax`) é o que está livre, e é
    #     a parte que o build usa quando a RAM chega ao `MemoryMax`;
    #   * a **decisão** olha para os dois juntos. Se o pico couber em
    #     RAM, o swap não é tocado.
    #
    # Ou seja: o swap não é uma licença para usar mais RAM. É o que
    # permite um pico de 931 MB passar num `MemoryMax` de 931 MB, com a
    # diferença a ir para swap em vez de para o `rust-analyzer`.
    local swap_livre
    swap_livre="$(mem_mb SwapFree)"
    local total_util=$(( util + swap_livre ))

    local unidade=$((pico + margem))
    PICO_DO_BUILD_MB="$pico"
    UNIDADE_MB="$unidade"
    SWAP_LIVRE_MB="$swap_livre"
    TOTAL_UTIL_MB="$total_util"

    if (( util < unidade )); then
        # Não cabe em RAM. Cabe no total?
        if (( total_util < unidade )); then
            RECUSA=1
            JOBS=0
            return
        fi
        # Cabe, mas vai precisar de swap. É uma execução mais lenta, e
        # a honestidade é dizer isso em vez de o esconder: um build que
        # escreve 120 MB em disco pode sentir-se.
        RECUSA=0
        USA_SWAP=1
    else
        RECUSA=0
        USA_SWAP=0
    fi

    if (( nucleos >= 2 && util >= 2 * unidade )); then
        JOBS=2
    else
        JOBS=1
    fi
}

# Detecta de que perfil é um comando, olhando para as features.
#
# A distinção importa porque os dois workloads têm picos muito
# diferentes (547 MB contra 931 MB) e decidir com o valor errado é
# errar por 384 MB — ou recusar um build que cabe, ou aceitar um que
# não cabe.
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
# foram aceites. Verificado com cgroup v2 + systemd 255, que aceitam
# MemoryHigh/MemoryMax/MemorySwapMax/OOMPolicy, mas um sistema sem
# `systemd` a correr, ou sem cgroup v2, não tem onde meter o
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
# Verificado com cgroup v2 e systemd 255: o caminho da scope resolve-se
# de dentro dela via `/proc/self/cgroup`.
# `executar_confinado <tecto_ram> <tecto_swap> CMD…`
#
# Os dois tectos são separados porque medem coisas diferentes, e
# conflacioná-los era o que impedia a verificação da arti de correr:
# `MemoryMax` é o que protege o editor e não sobe; `MemorySwapMax` é o
# que dá a folga em disco.
executar_confinado() {
    local tecto="$1"
    local tecto_swap="$2"
    shift 2

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
            -p "MemorySwapMax=${tecto_swap}M" \
            -p OOMPolicy=kill \
            -- "$inv" env "CARGO_PROFILE_DEV_DEBUG=$CARGO_DEBUG" "$@" \
            || codigo=$?

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
    decidir_jobs "$PERFIL" "$PICO"
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
            "$UNIDADE_MB" "$PICO" "$MARGEM_DO_BUILD_MB"
        printf '  em RAM: %d MB utilizáveis, em swap: %d MB livres\n' \
            "$local_util" "$SWAP_LIVRE_MB"
        printf '  falta %d MB, e o swap já não chega\n' \
            "$(( UNIDADE_MB - local_util - SWAP_LIVRE_MB ))"
        printf '  fecha o editor, ou espera a pressão de memória baixar\n'
    else
        printf '%s %d job%s de %s núcleos físicos\n' \
            "$(cor '1;32' 'jobs =')" "$JOBS" \
            "$( ((JOBS == 1)) && echo '' || echo 's')" "$(nucleos_fisicos)"
        printf '  %d MB disponíveis − %d MB de crescimento = %d MB utilizáveis\n' \
            "$local_disponivel" "$CRESCIMENTO_DESKTOP_MB" "$local_util"
        printf '  cada job exige %d MB → cabem %d\n' \
            "$UNIDADE_MB" "$(( local_util / UNIDADE_MB ))"
        # A decisão que o swap tomou, dita no mesmo sítio que as outras.
        if (( USA_SWAP == 1 )); then
            printf '%s\n' \
                "$(cor '1;33' '  e o que não cabe em RAM vai para swap')" \
                "$(cor '1;33' "  (${SWAP_LIVRE_MB} MB livres). A execução é mais lenta.")"
        fi
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
    decidir_jobs "$PERFIL" "$PICO"
    echo

    if (( RECUSA )); then
        recusar_build "$PICO" "$MARGEM_DO_BUILD_MB"
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
decidir_jobs "$PERFIL" "$PICO"
echo

if (( RECUSA )); then
    recusar_build "$PICO" "$MARGEM_DO_BUILD_MB"
    exit 1
fi

# Tecto de RAM = o pico medido, mais a margem do perfil, mais uma
# unidade por cada job acima do primeiro. O primeiro job não acrescenta
# nada, porque é ele que dá nome ao pico medido.
util=$(( $(mem_mb MemAvailable) - CRESCIMENTO_DESKTOP_MB ))
TECTO_MB=$(( PICO + MARGEM_DO_BUILD_MB + (JOBS - 1) * PICO ))
if (( TECTO_MB < UNIDADE_MB )); then
    TECTO_MB="$UNIDADE_MB"
fi

# O tecto de swap é uma decisão à parte, e é o que fecha o caso em que
# o pico não cabe em RAM.
#
# Dois valores, e a diferença entre eles é o ponto:
#
#   * `MemoryMax = TECTO_MB` — o que o build pode ter em RAM. É o
#     tecto que protege o editor: se o build o tocar, morre o build.
#     **Não sobe** com o swap disponível, porque subir aqui seria
#     deixar o build crescer até ao editor e confiar em que o OOM killer
#     Escolhe bem — que é a coisa que este gate existe para não
#     depender.
#   * `MemorySwapMax` — o que o build pode escrever em
#     disco. É este que absorve a diferença quando o `MemoryMax` é
#     atingido sem o build morrer.
#
# O limite do swap é o que está livre **menos uma reserva**, e a reserva
# decorre de uma medição a 256 MB: é o que o kernel precisa para mover páginas sem
# deadlock quando a RAM e o swap estão ambos no limite. Sem a reserva, uma
# máquina com o swap cheio tem um problema pior do que um build lento.
SWAP_RESERVA_MB=256
TECTO_SWAP_MB=$(( SWAP_LIVRE_MB - SWAP_RESERVA_MB ))
if (( TECTO_SWAP_MB < 0 )); then
    TECTO_SWAP_MB=0
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

printf '%s RAM %d MB (MemHigh %d MB, MemMax %d MB) + swap %d MB, OOMPolicy=kill\n' \
    "$(cor '1;33' '→')" "$TECTO_MB" "$(( TECTO_MB * 80 / 100 ))" \
    "$TECTO_MB" "$TECTO_SWAP_MB"
printf '%s %s\n' "$(cor '1;32' '→')" "${CMD[*]}"

# Dizer que o build vai usar swap é parte do que o gate reporta.
#
# Uma execução que escreve em disco não é a mesma que uma execução em
# RAM, e o que muda é o tempo: o `docs/testing.md` mede ~9 min para a
# verificação da arti com tudo em RAM, e o swap em disco acrescenta
# latência a cada página que passa lá. Um gate que aceitasse o swap em
# silêncio reportaria um tempo que ninguém vai ver.
if (( USA_SWAP == 1 )); then
    printf '%s\n' "$(cor '1;33' "  vai usar swap: o pico de ${PICO} MB não cabe nos ${util} MB utilizáveis,")"
    printf '%s\n' "$(cor '1;33' '  e o que sobra vai para disco. A execução é mais lenta.')"
fi
echo

JOBS="$JOBS" executar_confinado "$TECTO_MB" "$TECTO_SWAP_MB" "$@"
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