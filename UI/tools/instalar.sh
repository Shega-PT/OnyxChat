#!/usr/bin/env bash
# =====================================================================
# instalar.sh — instala a ferramenta local em `UI/tools/node/`
# ---------------------------------------------------------------------
# Instala o Node.js declarado em `versoes.txt`, dentro do próprio
# repositório, sem privilégios e sem tocar no sistema.
#
# ## Porquê uma cópia local
#
# O Vite 8 declara `engines.node = "^20.19.0 || >=22.12.0"` e usa
# `util.styleText`, que só existe a partir do Node 20.12. Uma distribuição
# pode ter o Node 18 — e quando isso acontece o `npm run build` falha com
# um erro de módulo que **não menciona versões**:
#
#   SyntaxError: The requested module 'node:util' does not provide an
#   export named 'styleText'
#
# Duas saídas seriam possíveis: exigir que quem desenvolve instale o Node
# certo pelo gestor de pacotes do sistema, ou trazer o Node consigo.
#
# Trouxe-se consigo. A máquina de quem desenvolve não é o alvo, e um
# requisito que depende do pacote que a distribuição escolheu é um requisito que
# fica por satisfazer. Uma cópia em `UI/tools/node/` é previsível, não
# precisa de `sudo`, não entra em conflito com outro projecto, e é apagada
# com a pasta.
#
# A contrapartida é a honestidade do custo: ~120 MB descomprimidos, dentro
# do repositório, **não versionados**. É o mesmo acordo que `target/` do
# Rust ou `node_modules/` do npm — coisas que se apagam e se reconstróem.
#
# ## Confiança no artefacto
#
# O sumário SHA-256 vem de `versoes.txt`, versionado e revisto, e **não**
# de uma descarga ao nodejs.org. Ver a justificação em `versoes.txt`.
#
# A verificação é feita antes de descompactar: um pacote que não bate o
# sumário nunca chega a ser escrito no disco em `UI/tools/node/`.
#
# ## Idempotência
#
# Correr duas vezes não faz mal e não volta a descarregar. A ferramenta é
# dada por instalada se `UI/tools/node/bin/node` responder com a versão
# declarada.
#
# ## Uso
#
#   UI/tools/instalar.sh            # instala se faltar
#   UI/tools/instalar.sh --forcar   # reinstala do zero
# =====================================================================
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFESTO="$AQUI/versoes.txt"
DESTINO="$AQUI/node"
BAINAS="$AQUI/.cache"

# ---------------------------------------------------------------------
# APRS
# ---------------------------------------------------------------------
cor_verde() { printf '\033[32m%s\033[0m\n' "$1"; }
cor_amarelo() { printf '\033[33m%s\033[0m\n' "$1"; }
cor_vermelho() { printf '\033[31m%s\033[0m\n' "$1"; }
erro() { cor_vermelho "erro: $1" >&2; exit 1; }

# ---------------------------------------------------------------------
# Plataforma
# ---------------------------------------------------------------------
# `uname -m` dá `x86_64` onde o Node chama `x64`, e `aarch64` onde chama
# `arm64`. Traduzir aqui, e não no manifesto, para que o manifesto fale a
# linguagem do Node.
detectar_plataforma() {
    local sistema arqu
    sistema="$(uname -s)"
    arqu="$(uname -m)"
    case "$arqu" in
        x86_64|amd64) arqu="x64" ;;
        aarch64|arm64) arqu="arm64" ;;
        *) erro "arquitectura não suportada: $arqu" ;;
    esac
    case "$sistema" in
        Linux) printf 'linux-%s' "$arqu" ;;
        Darwin) printf 'darwin-%s' "$arqu" ;;
        *) erro "sistema não suportado: $sistema (o projecto é testado em Linux)" ;;
    esac
}

# ---------------------------------------------------------------------
# Manifesto
# ---------------------------------------------------------------------
ler_manifesto() {
    # Campo pedido; devolve o valor ou falha com mensagem.
    local chave="$1"
    local linha
    linha="$(grep -E "^[[:space:]]*${chave}[[:space:]]*=" "$MANIFESTO" | head -1)" \
        || erro "não encontrei '$chave' em $(basename "$MANIFESTO")"
    linha="${linha#*=}"
    # `sed` em vez de expansão de parâmetro: o manifesto tem espaços à
    # volta e não é código, é configuração.
    printf '%s' "$linha" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
}

# Devolve "<sha256> <ficheiro>" para a plataforma dada.
ler_pacote() {
    local plataforma="$1"
    local linha
    linha="$(grep -E "^[[:space:]]*${plataforma}[[:space:]]*\|" "$MANIFESTO" | head -1)" \
        || erro "não há pacote para '$plataforma' em $(basename "$MANIFESTO")"
    printf '%s' "$linha" | awk -F'|' '{
        gsub(/^[ \t]+|[ \t]+$/, "", $2)
        gsub(/^[ \t]+|[ \t]+$/, "", $3)
        print $2 " " $3
    }'
}

# ---------------------------------------------------------------------
# Verificação do binário já instalado
# ---------------------------------------------------------------------
versao_instalada() {
    [ -x "$DESTINO/bin/node" ] || return 1
    "$DESTINO/bin/node" --version 2>/dev/null || return 1
}

# ---------------------------------------------------------------------
# Soma de verificação
# ---------------------------------------------------------------------
# Prefere `sha256sum` e cai para `shasum -a 256` (macOS). As duas calculam o
# mesmo valor em linguagens diferentes, e o macOS não traz a primeira.
soma256() {
    local ficheiro="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$ficheiro" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$ficheiro" | awk '{print $1}'
    else
        erro "nem `sha256sum` nem `shasum` estão disponíveis — não dá para verificar o pacote"
    fi
}

# ---------------------------------------------------------------------
# Descompactar
# ---------------------------------------------------------------------
# `tar` com `--strip-components=1` para que `node/bin/node` fique
# directamente em `node/`, sem o directorio da versão pelo meio. Sem isso o
# resto do código teria de saber o nome da directório, que muda a cada
# actualização.
descompactar() {
    local pacote="$1"
    mkdir -p "$DESTINO"
    tar -xf "$pacote" -C "$DESTINO" --strip-components=1
}

# ---------------------------------------------------------------------
# Programa principal
# ---------------------------------------------------------------------
principal() {
    local forcar="${1:-}"
    [ -f "$MANIFESTO" ] || erro "falta $MANIFESTO"

    local versao_desejada
    versao_desejada="$(ler_manifesto node)"

    local actual=""
    if [ "$forcar" != "--forcar" ]; then
        actual="$(versao_instalada || true)"
    fi

    if [ -n "$actual" ] && [ "$actual" = "v${versao_desejada}" ]; then
        cor_verde "Node local já instalado: ${actual} ($(basename "$DESTINO"))"
        return 0
    fi

    if [ -n "$actual" ]; then
        cor_amarelo "Node local na versão ${actual}; o manifesto pede v${versao_desejada}"
    fi

    local plataforma pacote soma ficheiro
    plataforma="$(detectar_plataforma)"
    read -r soma ficheiro <<<"$(ler_pacote "$plataforma")"

    [ -n "$soma" ] && [ -n "$ficheiro" ] || erro "leitura do manifesto falhou para '$plataforma'"
    [ "${#soma}" -eq 64 ] || erro "o sumário de '$plataforma' não tem 64 dígitos: '$soma'"

    local url="https://nodejs.org/dist/v${versao_desejada}/${ficheiro}"
    mkdir -p "$BAINAS"
    local caminho="${BAINAS}/${ficheiro}"

    if [ -f "$caminho" ]; then
        cor_amarelo "reaproveitando o pacote já descarregado"
    else
        printf 'a descarregar %s\n' "$url"
        if command -v curl >/dev/null 2>&1; then
            curl -sSfL --proto '=https' --tlsv1.2 -o "${caminho}.parcial" "$url" \
                || erro "descarga falhou"
        elif command -v wget >/dev/null 2>&1; then
            wget -q -O "${caminho}.parcial" "$url" || erro "descarga falhou"
        else
            erro "nem `curl` nem `wget` estão disponíveis"
        fi
        mv "${caminho}.parcial" "$caminho"
    fi

    local obtida
    obtida="$(soma256 "$caminho")"
    if [ "$obtida" != "$soma" ]; then
        rm -f "$caminho"
        erro "a soma não bate.
  esperada: $soma
  obtida:   $obtida
O pacote foi apagado e não foi descompactado. Ou a descarga foi
corrompida em trânsito, ou o manifesto está desactualizado."
    fi
    cor_verde "soma SHA-256 correcta"

    printf 'a descompactar para %s\n' "$(basename "$DESTINO")"
    rm -rf "$DESTINO"
    descompactar "$caminho"

    local instalada
    instalada="$(versao_instalada)" \
        || erro "a ferramenta foi descompactada mas o binário não corre"
    [ "$instalada" = "v${versao_desejada}" ] \
        || erro "versão descompactada inesperada: ${instalada} (esperava v${versao_desejada})"

    cor_verde "Node local pronto: ${instalada}"
    printf 'use-o por  %s/onyxchat <comando>\n' "$(basename "$AQUI")"
}

principal "$@"