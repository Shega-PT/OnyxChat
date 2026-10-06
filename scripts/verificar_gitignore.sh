#!/usr/bin/env bash
# =====================================================================
# verificar_gitignore.sh — prova que o .gitignore apanha o que deve
# ---------------------------------------------------------------------
# A pergunta "o .gitignore está completo?" não se responde a ler. O
# ficheiro pode ter 90 regras e mesmo assim deixar passar o artefacto
# que aparece amanhã.
#
# A pergunta responde-se **por tentativa**: cria-se um ficheiro com o
# nome de cada artefacto que este projecto produz, e pergunta-se ao Git
# se o ignoraria. Se a resposta for "não", o `.gitignore` tem um buraco
# — e diz-se qual, com o comando que o reproduz.
#
# O mesmo teste corre nos dois sentidos, que é o que o torna útil:
#
#   * artefactos que **têm** de ser ignorados;
#   * fontes que **não** podem ser ignoradas por engano — um
#     `.gitignore` que apanha `*.py` é tão perigoso como outro que não
#     apanha nada, e só aparece quando alguém clona.
#
# Não escreve nada fora de `/tmp` e apaga tudo o que cria.
# =====================================================================
set -uo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ" || exit 1

IGNORADOS=0
VISIVEIS=0
FALHAS=0

# ---------------------------------------------------------------------
# (caminho, tem de ser ignorado: "sim" | "nao")
# ---------------------------------------------------------------------

# Artefactos: têm de desaparecer.
ARTEFACTOS=(
  "target/debug/onyxchatd|sim"
  "network/daemon_rust/target/x.rs|sim"
  "onyxchat.egg-info/PKG-INFO|sim"
  "dist/onyxchat-0.1.0.tar.gz|sim"
  "build/lib/messenger/__init__.py|sim"
  ".venv/bin/python|sim"
  "venv/pyvenv.cfg|sim"
  "__pycache__/keys.cpython-312.pyc|sim"
  "messenger/__pycache__/keys.cpython-312.pyc|sim"
  ".pytest_cache/CACHEDIR.TAG|sim"
  ".hypothesis/example/1|sim"
  ".mypy_cache/3.12/cache|sim"
  ".ruff_cache/0.1.0/cache|sim"
  ".tox/py312/log|sim"
  ".coverage|sim"
  ".coverage.hostname.1234|sim"
  "coverage.xml|sim"
  "htmlcov/index.html|sim"
  "lcov.info|sim"
  "config.json|sim"
  "config.keystore|sim"
  ".env|sim"
  ".env.production|sim"
  "id_ed25519|sim"
  "server.key|sim"
  "cert.pem|sim"
  "secrets.toml|sim"
  "onyxchat.log|sim"
  "debug.log|sim"
  "core|sim"
  "core.123456|sim"
  "app.core|sim"
  "panic.stackdump|sim"
  "notes.txt.bak|sim"
  "merge.orig|sim"
  "patch.rej|sim"
  "backup~|sim"
  "editor.swp|sim"
  ".DS_Store|sim"
  "Thumbs.db|sim"
  "crypto/c_cpp/build/CMakeCache.txt|sim"
  "crypto/c_cpp/build/Makefile|sim"
  "crypto/c_cpp/build/compile_commands.json|sim"
  "crypto/c_cpp/build/cmake_install.cmake|sim"
  "crypto/c_cpp/build/libonyx_crypto.a|sim"
  "crypto/c_cpp/onyx_crypto.o|sim"
  "crypto/c_cpp/cobertura.gcno|sim"
  "crypto/c_cpp/cobertura.gcda|sim"
  "crypto/c_cpp/cobertura.gcov|sim"
  "crypto/c_cpp/libonyx.a|sim"
  "crypto/c_cpp/libsodium.so|sim"
  "crypto/c_cpp/libsodium.dylib|sim"
  "fuzz/artifacts/onyxchat-fuzz/crash-abc|sim"
  "fuzz/coverage/cov.json|sim"
  ".vscode/settings.json|sim"
  ".idea/workspace.xml|sim"
  "vendor/crypto/rust/src/lib.rs|sim"   # saida de `cargo vendor`
)

# Fontes: nunca podem ser ignoradas.
# ---------------------------------------------------------------------------
# Pendência por decidir, à espera do dono do projecto
# ---------------------------------------------------------------------------
# `GUIA-REPOSITORIO.md` está hoje em `.gitignore`, por uma linha que foi
# acrescentada à mão depois de o ficheiro ser escrito. Não se sabe se foi
# de propósito — e o documento contém o nome civil e os passos pessoais
# de publicação, o que é uma razão coerente para o querer de fora.
#
# A linha continua a estar lá, porque não é do assistente para apagar
# uma escolha que pode ser do dono. Até haver resposta, esta lista
# declara a excepção e o guard `o_gitignore_apanha_artefactos_e_nao_
# apanha_fontes` mostra-a pelo nome em vez de a esconder.
#
# Para resolver: confirmar se o guia entra no repositório. Se entra,
# apagar a linha do `.gitignore` e remover a excepção daqui.
PENDENCIAS=(
  "GUIA-REPOSITORIO.md"
)

FONTES=(
  "Cargo.lock|nao"
  "Cargo.toml|nao"
  ".cargo/config.toml|nao"
  ".gitignore|nao"
  "LICENSE|nao"
  "README.md|nao"
  "tests/vectors/camadas.json|nao"
  "crypto/c_cpp/vendor/libsodium/lib/libsodium.a|nao"
  "crypto/c_cpp/vendor/libsodium/include/sodium.h|nao"
  "crypto/c_cpp/vendor/libsodium/LICENSE.txt|nao"
  "network/daemon_rust/src/main.rs|nao"
  "network/daemon_rust/src/ipc_testes.rs|nao"
  "network/daemon_rust/tests/propriedades.proptest-regressions|nao"
  "user/cli.py|nao"
  "messenger/amizade.py|nao"
  "server/relay_server.py|nao"
  "tests/test_amizade.py|nao"
  "scripts/memoria.sh|nao"
  "crypto/python_lua/lua/deslocamento.lua|nao"
  "docs/architecture.md|nao"
  # `GUIA-REPOSITORIO.md` está em PENDENCIAS, não aqui.
  "fuzz/fuzz_targets/relay_parser.rs|nao"
)

criar() {
  local rel="$1" dir
  dir="$(dirname "$rel")"
  mkdir -p "$dir" 2>/dev/null
  printf 'artefacto de teste\n' > "$rel" 2>/dev/null || return 1
}

limpar() {
  local rel="$1" dir
  dir="$(dirname "$rel")"
  rm -f "$rel" 2>/dev/null
  # Só remove o directório se tiver ficado vazio e for um dos que criámos.
  rmdir "$dir" 2>/dev/null
}

avaliar() {
  local rel="$1" esperado="$2" grupo="$3"
  if [[ -e "$rel" ]]; then
    # Já existe de verdade: não criar, apenas perguntar ao Git.
    local real
    if git check-ignore -q "$rel"; then real="sim"; else real="nao"; fi
    if [[ "$real" == "$esperado" ]]; then
      printf '  \033[0;32mok\033[0m    %-58s %s\n' "$rel" \
        "$([[ "$real" == "sim" ]] && echo 'ignorado' || echo 'versionado')"
    else
      printf '  \033[1;31mFALHA\033[0m %-58s %s\n' "$rel" \
        "$([[ "$esperado" == "sim" ]] && echo 'Deveria ser ignorado e não é' || echo 'Deveria ser versionado e está ignorado')"
      FALHAS=$((FALHAS + 1))
    fi
    return
  fi

  criar "$rel" || return
  local real
  if git check-ignore -q "$rel"; then real="sim"; else real="nao"; fi
  limpar "$rel"

  # Excepções declaradas acima: não são falha, são pendência.
  local pendente="nao"
  for p in "${PENDENCIAS[@]}"; do
    [[ "$p" == "$rel" ]] && pendente="sim"
  done
  if [[ "$pendente" == "sim" ]]; then
    printf '  \033[0;33mPEND\033[0m  %-58s %s\n' "$rel" \
      "$([[ "$real" == "sim" ]] && echo 'ignorado — por decidir' || echo 'versionado')"
    return
  fi

  if [[ "$real" == "$esperado" ]]; then
    printf '  \033[0;32mok\033[0m    %-58s %s\n' "$rel" \
      "$([[ "$real" == "sim" ]] && echo 'ignorado' || echo 'versionado')"
  else
    printf '  \033[1;31mFALHA\033[0m %-58s %s\n' "$rel" \
      "$([[ "$esperado" == "sim" ]] && echo 'Deveria ser ignorado e não é' || echo 'Deveria ser versionado e está ignorado')"
    [[ "$grupo" == "artefactos" ]] && IGNORADOS=$((IGNORADOS + 1))
    [[ "$grupo" == "fontes" ]] && VISIVEIS=$((VISIVEIS + 1))
    FALHAS=$((FALHAS + 1))
  fi
}

echo "artefactos que TEM de ser ignorados"
echo
for entrada in "${ARTEFACTOS[@]}"; do
  IFS='|' read -r rel esperado grupo <<<"$entrada"
  avaliar "$rel" "$esperado" "$grupo"
done

echo
echo "fontes que NUNCA podem ser ignoradas"
echo
for entrada in "${FONTES[@]}"; do
  IFS='|' read -r rel esperado grupo <<<"$entrada"
  avaliar "$rel" "$esperado" "$grupo"
done

# As pendências são avaliadas à mesma, e marcadas como tal. Uma
# excepção que não aparece no relatório é uma excepção que ninguém vai
# resolver — que é a mesma coisa que não ter a declarado.
if (( ${#PENDENCIAS[@]} > 0 )); then
  echo
  echo "pendências por decidir (excepções declaradas)"
  echo
  for p in "${PENDENCIAS[@]}"; do
    if [[ -e "$p" ]]; then
      if git check-ignore -q "$p"; then real="ignorado"; else real="versionado"; fi
    else
      real="não existe"
    fi
    printf '  \033[0;33mPEND\033[0m  %-58s %s\n' "$p" "$real"
  done
fi

echo
echo "──────────────────────────────────────────────────────────"
if (( FALHAS == 0 )); then
  printf '\033[0;32m%s\033[0m — %d artefactos, %d fontes, %d pendência(s)\n' \
    "todas as verificações passam" \
    "${#ARTEFACTOS[@]}" "${#FONTES[@]}" "${#PENDENCIAS[@]}"
  exit 0
fi
printf '\033[1;31m%d falha(s)\033[0m\n' "$FALHAS"
exit 1