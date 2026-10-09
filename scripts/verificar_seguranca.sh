#!/usr/bin/env bash
# =====================================================================
# verificar_seguranca.sh — a superfície de ataque que não é código
# ---------------------------------------------------------------------
# Corre as cinco verificações de segurança que dependem de ferramentas
# externas, e que portanto não podem viver num verificador em Python:
#
#   segredos   gitleaks    chaves e tokens no histórico
#   avisos     cargo audit  advisories do RustSec em `Cargo.lock`
#   avisos     npm audit    advisories do registry em `package-lock.json`
#   licenças   cargo deny   PolyForm Noncommercial ⇒ copyleft proibido
#   workflows  zizmor      injecção e acções não fixadas por conteúdo
#
# ## Porquê um script e não cinco chamadas no workflow
#
# Porque é a mesma razão que fez a matriz correr `testar.sh` em vez de
# uma versão paralela escrita no YAML: se o CI fizesse a sua própria
# auditoria, haveria duas verdades, e a segunda não sería testada. Este
# script corre **igual** localmente e no CI — é o que faz uma decisão
# tomada aqui valer para lá.
#
# ## `--exigir`, e porquê
#
# Localmente, uma ferramenta ausente é um **skip anunciado** com o
# comando de instalação. No CI, é uma **falha**.
#
# A regra do projecto é que um portão que salta em silêncio é pior do que
# não existir: o `verificar_comentarios.py` devolvia lista vazia para
# ficheiros JavaScript — porque não conhecia o sufixo, e não por não haver
# nada a reportar — e a interface inteira estava por verificar sem que a
# saída dissesse nada. Um skip que não se anuncia é o mesmo erro com
# outro nome.
#
# ## O que NÃO está aqui, e porquê
#
# **Python.** `pyproject.toml` tem `dependencies = []` e quatro extras de
# desenvolvimento. Um analisador Python auditaria `pytest`, `coverage`,
# `pytest-cov` e `hypothesis` — quatro pacotes. Deixar de fora é uma
# decisão, e fica escrita em `docs/security_model.md` §4 para que não
# pareça um esquecimento. Quando o projecto ganhar dependências de
# execução, o passo aparece.
#
# **SBOM e assinatura de artefactos.** Úteis a partir do momento em que
# há distribuição. Ver `docs/security_model.md` §4.
# =====================================================================

set -uo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ" || exit 2

# `1` = cada ferramenta ausente é falha. O CI passa-o; a máquina de
# quem desenvolve não, porque nem toda a gente tem `cargo-deny` instalado.
EXIGIR=0
case "${1:-}" in
    --exigir) EXIGIR=1 ;;
    "")       ;;
    -h|--help)
        sed -n '2,45p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *)
        printf 'opção desconhecida: %s (use --exigir ou --help)\n' "$1" >&2
        exit 2
        ;;
esac

PASSOU=0
FALHOU=0
SALTADOS=0
AVISOS=()

# --- Cor ------------------------------------------------------------------

if [[ -t 1 ]]; then
    C_OK=$'\033[0;32m'; C_MAL=$'\033[0;31m'; C_AV=$'\033[0;33m'
    C_TIT=$'\033[1m'; C_FIM=$'\033[0m'
else
    C_OK=""; C_MAL=""; C_AV=""; C_TIT=""; C_FIM=""
fi

titulo() { printf '\n%s%s%s\n' "$C_TIT" "$1" "$C_FIM"; }

verificacao() {
    # `verificacao <descrição> <ferramenta> <instalação> <comando…>`
    #
    # Quatro posições, porque o quarto caso — a ferramenta ausente — é o
    # que decide o código de saída e o resumo não pode depender de ele.
    local descricao="$1" ferramenta="$2" instalar="$3"; shift 3

    if ! command -v "$ferramenta" >/dev/null 2>&1; then
        if (( EXIGIR )); then
            printf '  %s✗%s %-38s %s\n' "$C_MAL" "$C_FIM" "$descricao" \
                "$ferramenta não instalado — falha exigida pelo CI"
            printf '    instalar com: %s\n' "$instalar"
            FALHOU=$(( FALHOU + 1 ))
            AVISOS+=("$descricao: $ferramenta ausente")
        else
            printf '  %s→%s %-38s %ssaltado%s (falta %s)\n' \
                "$C_AV" "$C_FIM" "$descricao" "$C_AV" "$C_FIM" "$ferramenta"
            printf '    instalar com: %s\n' "$instalar"
            SALTADOS=$(( SALTADOS + 1 ))
        fi
        return 0
    fi

    printf '  %s→%s %s\n' "$C_AV" "$C_FIM" "$descricao"
    if "$@"; then
        printf '  %s✓%s %s\n' "$C_OK" "$C_FIM" "$descricao"
        PASSOU=$(( PASSOU + 1 ))
    else
        printf '  %s✗%s %s\n' "$C_MAL" "$C_FIM" "$descricao"
        FALHOU=$(( FALHOU + 1 ))
        AVISOS+=("$descricao")
    fi
    return 0
}

# =====================================================================
# 1. Segredos no histórico
# =====================================================================
#
# O repositório tem material criptográfico **de propósito**: vectores de
# teste, chaves de fixture, nonces. Um scanner confirma que nenhum deles
# é uma chave real — que é uma coisa que o olho não sabe fazer e que um
# ficheiro de vectores, por definição, não pode provar sozinho.
#
# `--redact` porque o relatório é lido no log do CI, e um segredo
# encontrado num log é um segredo Finding it twice.

titulo "1. Segredos no histórico"
verificacao "gitleaks" gitleaks \
    "cargo install gitleaks" \
    gitleaks detect --source . --redact --no-banner --exit-code 1

# =====================================================================
# 2. Advisories de Rust
# =====================================================================
#
# 548 pacotes em `Cargo.lock`, quase todos puxados pela arti. É a maior
# superfície de dependências do projecto e a que mais vale vigiar.
#
# As excepções vivem em `.cargo/audit.toml`, e cada uma tem razão e
# data de revisão. Um CI que fica sempre vermelho ensina-se a ignorar
# é pior do que um CI sem auditoria — a excepção datada é o que evita
# essa lição.

titulo "2. Advisories de Rust (RustSec)"
verificacao "cargo audit" cargo-audit \
    "cargo install cargo-audit" \
    cargo audit

# =====================================================================
# 3. Advisories de JavaScript
# =====================================================================
#
# A interface tem 16 dependências de execução — React, Radix, TanStack
# Query — e 17 de desenvolvimento, incluindo o Vite, que é uma cadeia de
# construção de código. Não se usa `--omit=dev`: um `vite` com advisory
# escreve código que o projecto distribui.
#
# Sai pelo mesmo caminho do `onyxchat` — o Node local, com SHA-256
# fixado em `UI/tools/versoes.txt`. Um `npm` do sistema seria um segundo
# toolchain, e um segundo toolchain é uma segunda coisa para manter.

# O `npm audit` não tem ficheiro de excepções próprio, e o `--omit=dev`
# não serve: um `vite` com advisory escreve o código que o projecto
# distribui. Por isso o relatório é comparado com `seguranca-excepcoes.toml`
# por `scripts/verificar_npm_auditoria.py`, que falha no que não estiver
# aceite — e imprime o que foi aceite, com a razão e a data.
if command -v node >/dev/null 2>&1 || [[ -x UI/tools/node/bin/node ]]; then
    verificacao "npm audit" npm \
        "UI/tools/onyxchat instalar" \
        python3 "$RAIZ/scripts/verificar_npm_auditoria.py"
else
    printf '  %s→%s npm audit                                %ssaltado%s (sem node)\n' \
        "$C_AV" "$C_FIM" "$C_AV" "$C_FIM"
    printf '    o Node é instalado por: UI/tools/onyxchat instalar\n'
    SALTADOS=$(( SALTADOS + 1 ))
fi

# =====================================================================
# 4. Licenças
# =====================================================================
#
# ## Porque é que isto é a camada mais específica deste projecto
#
# A licença do projecto é **PolyForm Noncommercial**, e a compatibilidade
# com as dependências é uma condição de negócio, não uma formalidade: um
# único crate GPL obriga a abrir o código derivado — comercial ou não — e
# torna a licença do projecto impossível de cumprir.
#
# `THIRD-PARTY.md` confirma que não há GPL, AGPL, SSPL, OSL, EUPL, CDDL
# nem CC-BY-SA. Aquilo foi uma auditoria manual, feita uma vez. O
# `cargo deny` passa a ser a **regra permanente**, e fecha uma lacuna que
# o próprio `THIRD-PARTY.md` declara: o `cargo metadata` não avalia
# expressões SPDX compostas — `MIT OR Apache-2.0` é tratado como texto
# livre. `cargo deny` avalia-as.

titulo "4. Licenças (PolyForm Noncommercial)"
verificacao "cargo deny — licenças" cargo-deny \
    "cargo install cargo-deny" \
    cargo deny --workspace --all-features check licenses

# =====================================================================
# 5. Os próprios workflows
# =====================================================================
#
# O `zizmor` audita `.github/workflows/` em busca de injecção através de
# `${{ github.event.* }}` e de acções não fixadas por conteúdo.
#
# A segunda é a que pesa aqui. `UI/tools/versoes.txt` recusa
# deliberadamente `SHASUMS256.txt` descarregado do nodejs.org, porque
# verificar um pacote contra quem o entrega não é verificar nada. Usar
# `actions/checkout@v4` — uma tag que pode ser movida — é exactamente o
# mesmo argumento, na mesma máquina, contra o mesmo projecto.
#
# Por isso o `zizmor` é a regra que mantém a honestidade do que já está
# escrito, e não uma ferramenta a mais.

titulo "5. Workflows do GitHub Actions"
verificacao "zizmor" zizmor \
    "pipx install zizmor" \
    zizmor --offline --no-progress .

# --- Sumário --------------------------------------------------------------

printf '\n%s%s%s\n' "$C_TIT" "$(printf '─%.0s' {1..66})" "$C_FIM"

if (( ${#AVISOS[@]} > 0 )); then
    printf '%s%ssem verificação:%s\n' "$C_AV" "$C_TIT" "$C_FIM"
    for a in "${AVISOS[@]}"; do printf '  - %s\n' "$a"; done
fi

printf '%d verificação(ões) ok, %d falhada(s), %d saltada(s)\n' \
    "$PASSOU" "$FALHOU" "$SALTADOS"

if (( FALHOU > 0 )); then
    printf '\n%sSegurança: FALHA%s\n' "$C_MAL" "$C_FIM"
    exit 1
fi
printf '\n%sSegurança: ok%s\n' "$C_OK" "$C_FIM"
exit 0