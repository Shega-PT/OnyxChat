#!/usr/bin/env bash
# =====================================================================
# verificar_requisitos.sh — confirma e instala o que falta
# ---------------------------------------------------------------------
# Lê `requisitos.txt` e diz o que falta na máquina. Instala o que for
# seguro instalar sozinho e explica, com o comando exacto, o que não é.
#
# ## Modos
#
#   verificar_requisitos.sh              só relata, não muda nada
#   verificar_requisitos.sh instalar     instala o que não precisa de privilégios
#   verificar_requisitos.sh instalar tudo  instala também os pacotes de sistema, com confirmação
#
# O modo por omissão é o que não muda nada, e é o único seguro correr em
# automação.
#
# ## Porque é que nem tudo é instalado automaticamente
#
# Porque há três categorias, e automaticar as três seria irresponsável:
#
#   1. **Sem privilégios** — o Node local (`UI/tools/instalar.sh`) e as
#      dependências Python do virtualenv. Não tocam no sistema, não
#      precisam de `sudo`, e não há nada que Installsr em parte.
#      Automático.
#
#   2. **Gestor de pacotes** — `cmake`, `g++`, `pkg-config`. Instalar
#      estes exige `sudo` e escolhe entre `apt`, `dnf`, `pacman` ou
#      `zypper` conforme a distribuição. **Ficam de fora do automático**,
#      porque é uma alteração ao sistema que o utilizador não pediu, e
#      porque o script recusa travar a este ponto por causa do `sudo`.
#
#   3. **Fora do alcance de um script** — a memória RAM. Não se instala
#      RAM. Mede-se e avisa-se.
#
# ## A diferença entre os dois portões
#
# Isto não é o guard `verificar_gitignore.sh`. Esse responde a "o
# `.gitignore` está certo?" e tem de falhar com força, porque é uma
# condição binária sobre o repositório. Este responde a "o que falta
# aqui?" e é uma ferramenta de trabalho: não toca no sistema, e dizer
# que falta uma dependência é o resultado normal, não uma emergência.
#
# ## Da lista ao código
#
# O formato de `requisitos.txt` é de cinco campos separados por `|`. Se
# alguém acrescentar uma dependência à lista e o script não souber
# verificá-la, o verificador **diz isso** em vez de a ignorar em
# silêncio — a mesma lição de `collectar_fontes`, em
# `network/daemon_rust/tests/guarda_tor.rs`: uma lista escrita à mão que
# fica desactualizada é pior do que não existir, porque ninguém sabe que
# deixou de cobrir alguma coisa.
# =====================================================================
set -uo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ" || exit 1
LISTA="$RAIZ/requisitos.txt"

MODO="${1:-verificar}"
NOME="${0##*/}"
SILENCIOSO=0

cor_verde()  { printf '\033[32m%s\033[0m\n' "$1"; }
cor_amarelo() { printf '\033[33m%s\033[0m\n' "$1"; }
cor_vermelho() { printf '\033[31m%s\033[0m\n' "$1"; }
cor_info()   { printf '\033[36m%s\033[0m\n' "$1"; }

case "$MODO" in
    verificar) ;;
    instalar) ;;
    instalar-tudo) ;;
    -h|--help|ajuda)
        sed -n '3,32p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) cor_vermelho "modo desconhecido: $MODO (use `verificar`, `instalar` ou `instalar tudo`)"; exit 2 ;;
esac

[ -f "$LISTA" ] || { cor_vermelho "erro: falta $LISTA"; exit 1; }

FALTAM=()
AVISOS=()
FALTAM_OPCIONAIS=()
INSTALADOS=()
DESCONHECIDAS=()

# O nível de uma chave, lido da lista. Serve para não repetir o `awk` em
# cada passagem e para isolar as opcionais.
nivel_de() {
    awk -F'|' -v k="$1" '
        $1 ~ "^[[:space:]]*" k "[[:space:]]*$" {
            gsub(/^[ \t]+|[ \t]+$/, "", $4); print $4; exit
        }' "$LISTA"
}

minimo_de() {
    awk -F'|' -v k="$1" '
        $1 ~ "^[[:space:]]*" k "[[:space:]]*$" {
            gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit
        }' "$LISTA"
}

# ---------------------------------------------------------------------
# Comparação de versões
# ---------------------------------------------------------------------
# Função simples que devolve 0 se a versão mais antiga B basta. Compara
# componente a componente e para na primeira diferença. Não pretende ser
# um `dpkg --compare-versions`: só tem de responder a "1.98 chega para
# 1.99?" e "22.14.0 chega para 20.19?", que é o que a lista pede.
versao_basta() {
    local obtida="$1" minima="$2" i
    [ "$minima" = "-" ] && return 0
    local -a A B
    IFS='.' read -r -a A <<<"${obtida#v}"
    IFS='.' read -r -a B <<<"$minima"
    for i in 0 1 2; do
        local x="${A[i]:-0}" y="${B[i]:-0}"
        # Um componente ausente conta como zero: "22" contra "22.14" dá
        # 22 == 22 e depois 0 < 14, que é a leitura correcta de que 22 não
        # chega a 22.14.
        x="${x%%[!0-9]*}"
        y="${y%%[!0-9]*}"
        [ -z "$x" ] && x=0
        [ -z "$y" ] && y=0
        [ "$x" -gt "$y" ] 2>/dev/null && return 0
        [ "$x" -lt "$y" ] 2>/dev/null && return 1
    done
    return 0
}

# ---------------------------------------------------------------------
# Detecção de versões, por chave
# ---------------------------------------------------------------------
# Devolve a versão detectada em `stdout`, ou nada se a ferramenta não
# existir. O código de saída é **sempre 0** para uma chave que o script
# conhece, e só é 1 para uma chave que ele não sabe verificar.
#
# A distinção importa. Uma versão anterior devolvia o código do `grep`
# final do pipeline, que é não-zero quando a ferramenta não existe — e uma
# ferramenta ausente era reportada como "chave desconhecida", que é uma
# mensagem completamente diferente e errada: o script sabia verificar o
# `pkg-config`, apenas não o encontrou.
detectar() {
    local saida
    case "$1" in
        bash)        saida="$(bash --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)" ;;
        git)         saida="$(git --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)" ;;
        python)      saida="$(python3 --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)" ;;
        rustc)       saida="$(rustc --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)" ;;
        cmake)       saida="$(cmake --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)" ;;
        gpp)         saida="$(g++ --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)" ;;
        pkg-config)  saida="$(pkg-config --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' | head -1)" ;;
        virtualenv)  [ -d "$RAIZ/.venv" ] && saida="presente" || saida="" ;;
        pytest)      if [ -x "$RAIZ/.venv/bin/pytest" ]; then
                         saida="$("$RAIZ/.venv/bin/pytest" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)"
                     else saida=""; fi ;;
        libssl-dev)  [ -e /usr/include/openssl/ssl.h ] && saida="presente" || saida="" ;;
        node)
            # A verificação segue a mesma ordem que a instalação: o Node
            # local primeiro, o do sistema depois. Divergir das duas daria
            # um "falta" que não é verdade.
            if [ -x "$RAIZ/UI/tools/node/bin/node" ]; then
                saida="$("$RAIZ/UI/tools/node/bin/node" --version 2>/dev/null | sed 's/^v//')"
            elif command -v node >/dev/null 2>&1; then
                saida="$(node --version 2>/dev/null | sed 's/^v//')"
            else
                saida=""
            fi
            ;;
        memoria)
            # `free` escreve o cabeçalho em português nesta máquina
            # (`total usada livre`) e em inglês noutra (`total used free`),
            # e a linha de dados começa por `Mem.:` em algumas versões do
            # procps e por `Mem:` noutras. Em vez de adivinhar a posição de
            # cada campo, toma-se a primeira linha que começa por `Mem` e
            # lê-se o segundo campo, que é o total em bytes em todas.
            if command -v free >/dev/null 2>&1; then
                saida="$(LC_ALL=C free -b 2>/dev/null | awk '/^Mem/ { if ($2 ~ /^[0-9]+$/) { printf "%.1f\n", $2/1073741824; exit } }')"
            elif command -v sysctl >/dev/null 2>&1; then
                saida="$(sysctl -n hw.memsize 2>/dev/null | awk '{ printf "%.1f\n", $1/1073741824 }')"
            else
                saida=""
            fi
            ;;
        *) return 1 ;;
    esac
    printf '%s' "$saida"
    return 0
}

# ---------------------------------------------------------------------
# Instalação sem privilégios
# ---------------------------------------------------------------------
instalar_auto() {
    case "$1" in
        node)
            [ -x "$RAIZ/UI/tools/instalar.sh" ] || return 1
            "$RAIZ/UI/tools/instalar.sh" >/dev/null 2>&1
            ;;
        virtualenv|pytest)
            command -v python3 >/dev/null 2>&1 || return 1
            if [ ! -d "$RAIZ/.venv" ]; then
                python3 -m venv "$RAIZ/.venv" || return 1
            fi
            "$RAIZ/.venv/bin/pip" install --quiet --upgrade pip >/dev/null 2>&1
            # `.[dev]` traz pytest, coverage e hypothesis. São os extras
            # que `pyproject.toml` declara; reinstalar o projecto em modo
            # editável é o que os torna disponíveis dentro do virtualenv.
            "$RAIZ/.venv/bin/pip" install --quiet -e "$RAIZ[dev]" >/dev/null 2>&1
            ;;
        bash|git|python|rustc) return 1 ;;
        *) return 1 ;;
    esac
}

# ---------------------------------------------------------------------
# Gestor de pacotes
# ---------------------------------------------------------------------
# Devolve a linha de comando de instalação para a distribuição. A
# função é deliberadamente pequena: duas linhas por gestor em vez de uma
# tabela gigante, porque a alternativa é uma tabela que ninguém actualiza
# e que fica a dar o pacote errado à pessoa errada.
comando_gestor() {
    case "$1" in
        memoria)
            # Não há pacote que instale memória. Dizer "instale a
            # dependência" aqui seria mentira, e a parte útil da resposta —
            # o que fazer quando não há RAM suficiente — está no guia, não
            # numa linha de comando.
            printf 'não se instala — ver docs/USER_GUIDE.md §1.1 (RAM) e docs/DEV_GUIDE.md §1'
            ;;
        cmake|gpp|pkg-config|libssl-dev)
            if command -v apt-get >/dev/null 2>&1; then
                local pacote="$1"
                [ "$1" = "gpp" ] && pacote="g++"
                printf 'sudo apt-get install -y %s' "$pacote"
            elif command -v dnf >/dev/null 2>&1; then
                local p="$1"; [ "$1" = "gpp" ] && p="gcc-c++"
                printf 'sudo dnf install -y %s' "$p"
            elif command -v pacman >/dev/null 2>&1; then
                local p="$1"; [ "$1" = "gpp" ] && p="gcc"
                printf 'sudo pacman -S --needed %s' "$p"
            elif command -v zypper >/dev/null 2>&1; then
                printf 'sudo zypper install -y %s' "$1"
            else
                printf ''
            fi
            ;;
        rustc) printf 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable' ;;
        *)     printf '' ;;
    esac
}

# ---------------------------------------------------------------------
# Percorrer a lista
# ---------------------------------------------------------------------
# Lê a lista linha a linha. O IFS é posto a barra para o `read` partir
# pelos separadores reais em vez de por espaços — os campos contêm
# espaços (`4.2  | bash --version | ...`) e um `IFS=' '` partia-os ao
# contrário.
linha=0
while IFS='|' read -r chave minima verificar nivel instalar; do
    linha=$((linha + 1))
    chave="$(printf '%s' "$chave"   | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    minima="$(printf '%s' "$minima" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    verificar="$(printf '%s' "$verificar" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    nivel="$(printf '%s' "$nivel"    | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    instalar="$(printf '%s' "$instalar" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"

    [ -z "$chave" ] && continue
    case "$chave" in \#*) continue ;; esac
    case "$minima" in \#*) continue ;; esac

    obtida="$(detectar "$chave" 2>/dev/null)" || {
        DESCONHECIDAS+=("$chave (linha $linha)")
        continue
    }

    if [ -z "$obtida" ] || ! versao_basta "$obtida" "$minima"; then
        # O campo `minima` é "-" para tudo o que não tem versão: nesses
        # casos basta existir.
        if [ "$minima" = "-" ] && [ -n "$obtida" ]; then
            printf '  %-12s %-18s %s\n' "$chave" "$obtida" "$( [ "$nivel" = "opcional" ] && echo 'opcional' || echo 'presente')"
            continue
        fi

        # As opcionais não entram em `FALTAM`: não travam o projecto e
        # contá-las como falta faria o resumo final dizer "N dependências
        # em falta" quando o que falta é, por exemplo, o `pkg-config`.
        if [ "$nivel" = "opcional" ]; then
            FALTAM_OPCIONAIS+=("$chave")
            printf '  %-12s %-18s %s\n' "$chave" "${obtida:-ausente}" 'opcional — não trava'
            continue
        fi

        FALTAM+=("$chave")
        printf '  %-12s %-18s %s\n' "$chave" "${obtida:-ausente}" "falta (>= $minima)"
        continue
    fi

    printf '  %-12s %-18s %s\n' "$chave" "$obtida" "$( [ "$nivel" = "opcional" ] && echo 'opcional' || echo 'presente')"
done < <(grep -v '^[[:space:]]*#' "$LISTA" | grep -v '^[[:space:]]*$')

# ---------------------------------------------------------------------
# Instalação
# ---------------------------------------------------------------------
# Só corre no modo `instalar`. Os itens de gestor de pacotes ficam de
# fora de propósito: precisam de `sudo`, e o que este script recusa é
# fazer alterações ao sistema sem o utilizador as ter pedido.
faltam_sistema=()
if [ "$MODO" = "instalar" ] || [ "$MODO" = "instalar-tudo" ]; then
    printf '\n%s\n' '--- a instalar o que não precisa de privilégios ---'
    for chave in "${FALTAM[@]:-}"; do
        [ -z "$chave" ] && continue
        instalar="$(awk -F'|' -v k="$chave" '
            $1 ~ "^[[:space:]]*" k "[[:space:]]*$" {
                gsub(/^[ \t]+|[ \t]+$/, "", $5); print $5; exit
            }' "$LISTA")"
        if [ "$instalar" = "auto" ]; then
            printf '  %s… ' "$chave"
            if instalar_auto "$chave"; then
                cor_verde 'instalado'
                INSTALADOS+=("$chave")
            else
                cor_vermelho 'falhou'
            fi
        else
            faltam_sistema+=("$chave")
        fi
    done
fi

# ---------------------------------------------------------------------
# Resultado
# ---------------------------------------------------------------------
# Recalcular depois da instalação, senão a lista do topo mente.
#
# A leitura vai para um array novo em vez de crescer `FALTAM` dentro do
# `for` que a percorre: percorrer um array enquanto se lhe acrescentam
# elementos tem comportamento dependente da implementação do bash, e o
# resultado seria uma lista de "ainda em falta" que varia entre máquinas.
restam=()
for chave in "${FALTAM[@]:-}"; do
    [ -z "$chave" ] && continue
    obtida="$(detectar "$chave" 2>/dev/null)"
    minima="$(minimo_de "$chave")"
    if [ -z "$obtida" ] || ! versao_basta "$obtida" "$minima"; then
        restam+=("$chave")
    fi
done
FALTAM=("${restam[@]:-}")

if [ "${#DESCONHECIDAS[@]}" -gt 0 ]; then
    printf '\n%s\n' '--- chaves que este script não sabe verificar ---'
    for chave in "${DESCONHECIDAS[@]}"; do
        cor_amarelo "  $chave"
    done
    cor_amarelo 'acrescenta-se o caso em `detectar`, ou tira-se a linha da lista.'
fi

if [ "${#FALTAM_OPCIONAIS[@]}" -gt 0 ]; then
    printf '\n%s\n' '--- opcionais em falta (não travam o projecto) ---'
    for chave in "${FALTAM_OPCIONAIS[@]}"; do
        printf '  %-12s %s\n' "$chave" "$(comando_gestor "$chave")"
    done
fi

printf '\n'
if [ "${#FALTAM[@]}" -eq 0 ]; then
    cor_verde 'todas as dependências obrigatórias estão presentes'
else
    cor_vermelho "${#FALTAM[@]} dependência(s) em falta"
    for chave in "${FALTAM[@]}"; do
        comando="$(comando_gestor "$chave")"
        if [ -n "$comando" ]; then
            printf '  %-12s %s\n' "$chave" "$comando"
        elif [ "$chave" = "node" ]; then
            # O Node é auto-instalável mas só por um caminho próprio: a
            # cópia local. Dizer "sem instalação automática" seria falso.
            printf '  %-12s %s\n' "$chave" "UI/tools/onyxchat instalar   (cópia local, sem privilégios)"
        else
            printf '  %-12s (sem instalação automática — ver requisitos.txt)\n' "$chave"
        fi
    done
    printf '\n%s\n' '  As dependências de gestor de pacotes não são instaladas'
    printf '%s\n'  '  automaticamente: precisam de sudo e alteram o sistema.'
fi

if [ "$MODO" != "verificar" ] && [ "${#INSTALADOS[@]}" -gt 0 ]; then
    printf '\n%s\n' '--- instalado ---'
    for chave in "${INSTALADOS[@]}"; do printf '  %s\n' "$chave"; done
fi

[ "${#FALTAM[@]}" -eq 0 ] && exit 0
exit 1