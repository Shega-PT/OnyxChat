#!/usr/bin/env bash
# =====================================================================
# fetch.sh — traz a FONTE do `ffmpeg` declarado em `versoes.txt`
# ---------------------------------------------------------------------
# Deixa o tarball da fonte em `cache/` depois de conferir o SHA-256.
# Não compila: compilar é o trabalho do `build.sh`, e a separação é
# deliberada — descarregar é preciso uma vez, compilar é preciso de
# propósito.
#
# ## O que este script já não faz, e porquê
#
# A primeira versão descarregava um **executável** pronto e recusava a
# licença antes de escrever. Era a garantia errada: um binário de
# terceiros tem de ser **confiado**, e a confiança a um binário de 80 MB
# de origem não verificável não é um acto técnico — é uma renúncia.
#
# Hoje o que se fixa é o **código-fonte**, com sumário no repositório,
# e a garantia passa a ser a receita de compilação, que está no mesmo
# repositório e é do `build.sh` verificada com `ffmpeg -buildconf`. É a
# mesma diferença que o projecto já faz entre um crate que declara uma
# licença e um crate cujo código se lê.
#
# ## E a licença?
#
# `THIRD-PARTY.md` §Componentes compilados de fonte tem a excepção, com
# justificação, ao lado da `libsodium`. Aqui não há lista de licenças
# proibidas porque **já não há um artefacto de terceiros a aceitar**:
# há fonte, que o `deny.toml` não avalia, e uma receita que o
# `build.sh` aplica e depois **confirma**.
#
# ## Uso
#
#   tools/ffmpeg/fetch.sh              # traz a fonte
#   tools/ffmpeg/fetch.sh --verificar  # só confere o que já está em cache
#   tools/ffmpeg/fetch.sh --forcar     # redescarrega
# =====================================================================
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFESTO="$AQUI/versoes.txt"
CACHE="$AQUI/cache"

# ---------------------------------------------------------------------
# As duas funções que escrevem
# ---------------------------------------------------------------------

# Emite o código ANSI indicado.
cor() { printf '\033[%sm%s\033[0m\n' "$1" "$2"; }

erro() { cor '1;31' "erro: $1" >&2; }
aviso() { cor '1;33' "aviso: $1" >&2; }
ok() { cor '1;32' "$1"; }
nota() { cor '2' "$1"; }

# ---------------------------------------------------------------------
# Espaços nas pontas
# ---------------------------------------------------------------------

# Remove os espaços à esquerda e à direita, e **só** esses.
#
# Existe porque `${v%% }` remove um espaço — o `%%` casa com o sufixo
# mais longo que casa com o padrão, e o padrão é um espaço, logo casa
# com um. Um manifesto alinhado com pipes fica assim:
#
#     chave="${linha%%|*}"   →  "ffmpeg     "
#     chave="${chave%% }"    →  "ffmpeg    "   (um a menos)
#
# E a alternativa `${v// /}` corromperia um URL com `%20`.
trim() {
    printf '%s' "$1" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
}

# ---------------------------------------------------------------------
# O manifesto
# ---------------------------------------------------------------------

# A entrada `ffmpeg = <versão> | <url> | <sha256> | <versão>`, ou nada.
entrada() {
    local linha
    while IFS= read -r linha; do
        case "$linha" in
            \#*|'') continue ;;
        esac
        local chave
        chave="$(trim "${linha%%=*}")"
        if [ "$chave" = "ffmpeg" ]; then
            printf '%s\n' "${linha#*=}"
            return 0
        fi
    done < "$MANIFESTO"
    return 1
}

sha256_de() { sha256sum "$1" | cut -d' ' -f1; }

# ---------------------------------------------------------------------
# Argumentos
# ---------------------------------------------------------------------

SO_VERIFICAR=0
FORCAR=0
while [ $# -gt 0 ]; do
    case "$1" in
        --verificar) SO_VERIFICAR=1; shift ;;
        --forcar) FORCAR=1; shift ;;
        -h|--help) sed -n '2,32p' "$0"; exit 0 ;;
        *) erro "opção desconhecida: $1"; exit 2 ;;
    esac
done

[ -f "$MANIFESTO" ] || { erro "falta $MANIFESTO"; exit 1; }
mkdir -p "$CACHE"

ENTRADA="$(entrada || true)"
if [ -z "$ENTRADA" ]; then
    erro "versoes.txt não declara a fonte do ffmpeg."
    nota "formato: ffmpeg = <versão> | <url> | <sha256> | <versão>"
    exit 1
fi

OLDIFS="$IFS"
IFS='|'
# shellcheck disable=SC2086
set -- $ENTRADA
IFS="$OLDIFS"

VERSAO="$(trim "${1-}")"
URL="$(trim "${2-}")"
ESPERADO="$(trim "${3-}")"

[ -n "$URL" ] && [ -n "$ESPERADO" ] || {
    erro "a entrada está incompleta."
    nota "formato: ffmpeg = <versão> | <url> | <sha256> | <versão>"
    exit 1
}

FICHEIRO="$CACHE/${URL##*/}"

# ---------------------------------------------------------------------
# O download
# ---------------------------------------------------------------------

if [ "$SO_VERIFICAR" -eq 1 ]; then
    [ -f "$FICHEIRO" ] || { erro "não está em cache: ${FICHEIRO##*/}"; exit 1; }
elif [ -f "$FICHEIRO" ] && [ "$FORCAR" -eq 0 ]; then
    nota "já está em cache: ${FICHEIRO##*/}"
else
    nota "a descarregar ffmpeg $VERSAO — 12 MB, pode demorar"
    TEMP="$CACHE/.$RANDOM.em-parte"
    # `--fail` para que um 404 não deixe um ficheiro de HTML que passa a
    # ser verificado como se fosse a fonte.
    curl --fail --location --silent --show-error --max-time 900 \
        --output "$TEMP" "$URL" || {
        rm -f "$TEMP"
        erro "o download falhou."
        exit 1
    }
    mv "$TEMP" "$FICHEIRO"
fi

# ---------------------------------------------------------------------
# O sumário, antes de descompactar
# ---------------------------------------------------------------------

ACTUAL="$(sha256_de "$FICHEIRO")"
if [ "$ACTUAL" != "$ESPERADO" ]; then
    erro "o SHA-256 da fonte não bate."
    nota  "esperado: $ESPERADO"
    nota  "obtido:   $ACTUAL"
    cat >&2 <<'ERRO'

  A fonte nunca é descompactada: a verificação é antes, e é a razão de
  um tarball adulterado não chegar a ser escrito em tools/ffmpeg/cache/.

  Se o valor esperado está errado, ele foi calculado por quem compila,
  não copiado do fornecedor — o FFmpeg não publica sumários sidecar.
  Ver tools/ffmpeg/versoes.txt.

ERRO
    exit 1
fi
ok "fonte confirmada: $VERSAO"

# ---------------------------------------------------------------------
# O passo seguinte
# ---------------------------------------------------------------------

nota "em cache/${FICHEIRO##*/}"
nota "agora: tools/ffmpeg/build.sh"
nota "      que a compila com --disable-gpl e confirma a licença com"
nota "      ffmpeg -buildconf. Ver THIRD-PARTY.md §Componentes compilados"
nota "      de fonte, e deny.toml para a regra que esta excepção altera."
