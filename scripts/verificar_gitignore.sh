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
#
# ## Porque é que também verifica permissões
#
# Este script verificava o `.gitignore` e mais nada, e em 2026-10-07
# apanhou-se a falha que isso não cobre: `scripts/verificar_requisitos.sh`
# versionado sem o bit de execução, com o `USER_GUIDE.md` a mandar
# correr `./scripts/verificar_requisitos.sh`.
#
# O Git guarda um bit de permissões, e o erro só aparece no `clone` da
# outra pessoa — o pior sítio para o descobrir, porque o primeiro aviso
# é «Permissão recusada», que não menciona permissões nem o ficheiro.
# Verificar no disco não chega: o que um clone recebe é o modo do
# índice. Por isso a verificação de executáveis pede as duas coisas.
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
  # O `cargo fuzz` grava cada input que acha interessante com o nome
  # `<sha1>.bin` — **sem** o prefixo de dois dígitos que o gerador de
  # seeds usa. Um padrão `[0-9][0-9]-*.bin` accepta os dois, e o corpus
  # versionado crescia a cada execução. Medido em 2026-10-07: com o
  # padrão largo, este ficheiro era aceite por `git add`.
  "fuzz/corpus/envelope_parser/hash-sem-prefixo.bin|sim"
  "fuzz/corpus/k7_decoder/abcd1234ef567890.bin|sim"
  ".vscode/settings.json|sim"
  ".idea/workspace.xml|sim"
  "vendor/crypto/rust/src/lib.rs|sim"   # saida de `cargo vendor`
  # --- Interface (UI/) -------------------------------------------------
  # A pasta `UI/` produz a sua própria família de artefactos, e nenhum deles
  # aparece em mais lado nenhum do projecto. Sem estas linhas, `npm install`
  # no clone de outra pessoa produz `UI/node_modules/` com milhares de
  # ficheiros que o Git tenta seguir — e o `dist/` compilado, que é
  # dependente da máquina, seria comitado como se fosse fonte.
  "UI/node_modules/react/index.js|sim"
  "UI/node_modules/.package-lock.json|sim"
  "UI/dist/index.html|sim"
  "UI/dist/assets/index-abc123.js|sim"
  "UI/.vite/deps/_metadata.json|sim"
  "UI/.env|sim"
  "UI/.env.producao|sim"
  "UI/npm-debug.log|sim"
  # O alvo do Tauri (Etapa 6). Já é apanhado pelo `target/` da raiz, porque
  # um padrão sem barra inicial casa em qualquer nível — mas isso é
  # coincidência, não intenção. Declarar aqui é o que garante que continua
  # a valer se o alvo do Rust mudar de nome.
  "UI/src-tauri/target/debug/onyxchat|sim"
  "UI/src-tauri/gen/schemas/desktop-schema.json|sim"
  "UI/app/desktop/target/release/onyxchat-ui|sim"
  # A ferramenta local da interface: um Node.js completo, descompactado de
  # um pacote oficial. São ~120 MB e não têm nada de específico deste
  # projecto, e versioná-los transformaria cada actualização da interface
  # num commit de centenas de milhares de linhas.
  "UI/tools/node/bin/node|sim"
  "UI/tools/node/lib/node_modules/npm/package.json|sim"
  "UI/tools/.cache/node-v22.14.0-linux-x64.tar.xz|sim"
  "UI/tools/node-v22.14.0.tar.xz.parcial|sim"
  # A marca é um recurso **versionado** (`public/onyxchat.png`), mas o
  # `dist/` que a serve não. A distinção é o que este guard existe para
  # proteger: um ficheiro de entrada e o artefacto que sai dele não podem
  # ter o mesmo destino.
  "UI/dist/onyxchat.png|sim"
  "UI/dist/fonts/inter-400.woff2|sim"
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
  # --- Interface (UI/) -------------------------------------------------
  # `verificar_ambiente.py: ignorar`
  #
  # O bloco seguinte está isento porque **cita** a frase que esse
  # verificador proíbe, para nomear o problema. Isentar a citação é
  # melhor do que a reescrever: o sentido depende das palavras exactas.
  # Um portão que obriga a mutilar a prosa acaba desactivado.
  #
  # O `package-lock.json` é a excepção mais importante desta lista. Ele é
  # gerado, e mesmo assim tem de ser versionado: sem ele, duas pessoas
  # instalam versões diferentes das mesmas dependências e o "funciona na
  # minha máquina" volta a aparecer — agora em JavaScript. O `dist/` está
  # do lado oposto da lista de cima, e a diferença entre os dois é
  # deliberada.
  "UI/package.json|nao"
  "UI/package-lock.json|nao"
  "UI/src/main.jsx|nao"
  "UI/src/index.css|nao"
  "UI/src/App.jsx|nao"
  "UI/src/lib/shadcn.js|nao"
  "UI/src/lib/onyx/data-adapter.js|nao"
  "UI/src/components/onyx/OnyxButton.jsx|nao"
  "UI/src/components/ui/button.jsx|nao"
  "UI/vite.config.js|nao"
  "UI/tailwind.config.js|nao"
  "UI/eslint.config.js|nao"
  "UI/jsconfig.json|nao"
  "UI/components.json|nao"
  "UI/postcss.config.js|nao"
  "UI/README.md|nao"
  "UI/.gitignore|nao"
  "UI/tools/onyxchat|nao"
  "UI/tools/instalar.sh|nao"
  "UI/tools/versoes.txt|nao"
  "UI/tools/README.md|nao"
  "UI/public/fonts/inter-400.woff2|nao"
  "UI/public/fonts/inter-500.woff2|nao"
  "UI/public/fonts/inter-600.woff2|nao"
  "UI/public/fonts/jetbrains-mono-400.woff2|nao"
  "UI/public/fonts/jetbrains-mono-500.woff2|nao"
  "UI/public/onyxchat.png|nao"
  "UI/src/lib/SessaoContext.jsx|nao"
  "UI/src/lib/onyx/runtime.js|nao"
  "UI/src/lib/onyx/mock-adapter.js|nao"
  "UI/src/lib/onyx/bridge-adapter.js|nao"
  "UI/src/components/onyx/FaixaDemonstracao.jsx|nao"
  "requisitos.txt|nao"
  "scripts/verificar_requisitos.sh|nao"
  # `GUIA-REPOSITORIO.md` está em PENDENCIAS, não aqui.
  "fuzz/fuzz_targets/relay_parser.rs|nao"
  # As seeds de fuzz **têm** de estar versionadas: um clone começa com os
  # seis alvos, e sem elas o libFuzzer não ganha profundidade nos ramos
  # onde os parsers falham.
  #
  # Este caso apanhou o bug de 2026-10-07: o `.gitignore` da raiz tinha
  # `fuzz/corpus/`, que ganha sobre a re-inclusão do `fuzz/.gitignore`, e
  # as 57 seeds ficavam fora do índice sem que nada falhasse.
  "fuzz/corpus/envelope_parser/00-0100010203040506.bin|nao"
  "fuzz/corpus/handshake_parser/00-01.bin|nao"
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

# O estado real de um ficheiro tem **três** valores, e não dois.
#
# `git check-ignore` só responde a uma pergunta: «isto é ignorado?». Um
# ficheiro que não é ignorado pode estar versionado **ou** simplesmente
# por versionar — e a diferença é o que separa um `git clone` com as
# sementes de fuzzing de um `git clone` sem elas.
#
# Dizer «versionado» ao que só não está ignorado foi o que escondeu, em
# 2026-10-07, que as 50 sementes do corpus estavam na árvore e não no
# índice. O portão passava, o relatório dizia «ok», e ninguém versionou
# nada.
estado_de() {
  local rel="$1"
  if git check-ignore -q "$rel"; then echo "ignorado"
  elif git ls-files --error-unmatch -- "$rel" >/dev/null 2>&1; then echo "versionado"
  else echo "POR VERSIONAR"
  fi
}

avaliar() {
  local rel="$1" esperado="$2" grupo="$3"
  if [[ -e "$rel" ]]; then
    # Já existe de verdade: não criar, apenas perguntar ao Git.
    local real
    if git check-ignore -q "$rel"; then real="sim"; else real="nao"; fi
    if [[ "$real" == "$esperado" ]]; then
      printf '  \033[0;32mok\033[0m    %-58s %s\n' "$rel" \
        "$(estado_de "$rel")"
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
      "$(estado_de "$rel") — por decidir"
    return
  fi

  if [[ "$real" == "$esperado" ]]; then
    printf '  \033[0;32mok\033[0m    %-58s %s\n' "$rel" \
      "$(estado_de "$rel")"
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
      real="$(estado_de "$p")"
    else
      real="não existe"
    fi
    printf '  \033[0;33mPEND\033[0m  %-58s %s\n' "$p" "$real"
  done
fi

# A terceira pergunta, e a que faltava: das fontes que o `.gitignore`
# deixa visíveis, quais é que **continuam fora do índice**?
#
# Sem esta secção, «não ignorado» contava como «versionado» e o portão
# passava com 50 sementes de fuzzing por versionar. Um clone fazia-se sem
# elas, e a primeira sessão de fuzzing de quem clonasse começava do
# zero — sem que nada tivesse falhado.
echo
echo "fontes visíveis que ainda NÃO estão no índice"
echo
POR_VERSIONAR=0
while IFS= read -r rel; do
    [[ -n "$rel" ]] || continue
    POR_VERSIONAR=$((POR_VERSIONAR + 1))
    printf '  \033[0;33mPEND\033[0m  %s\n' "$rel"
done < <(git ls-files --others --exclude-standard)
if (( POR_VERSIONAR == 0 )); then
    printf '  \033[0;32mok\033[0m    nenhuma\n'
fi

echo
echo "scripts que TEM de ser executáveis"
echo
#
# ## Porque é que isto está aqui
#
# Em 2026-10-07, `scripts/verificar_requisitos.sh` estava no índice sem
# o bit de execução, e o `docs/USER_GUIDE.md` manda correr
#
#     scripts/verificar_requisitos.sh
#
# que responde «Permissão recusada». Nenhum verificador apanha isto: o
# Git só guarda um bit de permissões, e essa verificação só acontece no
# `clone` da outra pessoa — o pior sítio para a descobrir, porque o
# primeiro aviso é um erro que não menciona permissões.
#
# O que se verifica é o bit de execução do ficheiro **no disco**, não o
# modo `100755` que o Git indexia: o objectivo é que `./script.sh` funcione
# a partir de um clone, e num clone o Git escreve o que está no índice.
# Por isso a verificação pede as duas coisas — bit no disco e modo `100755`
# no índice — e cada linha diz o que o script é para o utilizador.
verificar_execucavel() {
  local rel="$1" razao="$2"

  if [[ ! -e "$rel" ]]; then
    printf '  \033[1;31mFALHA\033[0m %-58s %s\n' "$rel" "não existe"
    FALHAS=$((FALHAS + 1))
    return
  fi

  local disco="nao" indice="nao"
  [[ -x "$rel" ]] && disco="sim"
  # O modo no índice é o que um clone recebe. Um ficheiro sem entries
  # no índice (ainda não adicionado) devolve vazio, que conta como "não
  # versionado" — e é esse o estado que dá a falha.
  local modo
  modo="$(git ls-files -s -- "$rel" | awk '{print $1}')"
  [[ "$modo" == "100755" ]] && indice="sim"

  if [[ "$disco" == "sim" && "$indice" == "sim" ]]; then
    printf '  \033[0;32mok\033[0m    %-58s %s\n' "$rel" "executável"
  elif [[ "$disco" != "sim" ]]; then
    printf '  \033[1;31mFALHA\033[0m %-58s %s\n' "$rel" \
      "sem bit de execução no disco — $razao"
    FALHAS=$((FALHAS + 1))
  else
    printf '  \033[1;31mFALHA\033[0m %-58s %s\n' "$rel" \
      "exECUTável no disco mas 100644 no índice — um clone perde o bit"
    FALHAS=$((FALHAS + 1))
  fi
}

verificar_execucavel "scripts/memoria.sh" \
  "o gate de memória; sem ele o build corre sem tecto e o kernel mata o editor"
verificar_execucavel "scripts/memoria-envolver.sh" \
  "o invólucro de medição que o gate corre dentro da scope"
verificar_execucavel "scripts/testar.sh" \
  "a matriz de testes; é o único caminho que confina os passos"
verificar_execucavel "scripts/verificar_gitignore.sh" \
  "este próprio verificador"
verificar_execucavel "scripts/verificar_requisitos.sh" \
  "USER_GUIDE.md §1.1.1 manda correr ./scripts/verificar_requisitos.sh"
# `verificar_seguranca.sh` entrava no `testar.sh` e no workflow
# `seguranca.yml` como `./scripts/verificar_seguranca.sh`, e não estava
# nesta lista. Um clone que lhe tirasse o bit transformava a auditoria de
# segurança num `Permission denied` — e o portão de permissões do
# `portoes.yml`, que também o esquecia, dizia «ok». Um ficheiro que dois
# workflows executam directamente tem de estar na lista; não estar é o
# ficheiro desaparecer sem que nada diga que era preciso.
#
# A razão vai **sem** crases porque o `printf` recebe-a em aspas duplas:
# uma crase dentro de aspas duplas é substituição de comandos, e o shell
# ia tentar correr `testar.sh` como um programa. Foi o que aconteceu à
# primeira versão desta linha, e o aviso saiu dizer
# «testar.sh: comando não encontrado» no meio de um portão que passava.
verificar_execucavel "scripts/verificar_seguranca.sh" \
  "a auditoria de segurança; testar.sh e o workflow seguranca.yml correm-na directamente"
verificar_execucavel "scripts/medir-arti.sh" \
  "mede o pico da verificação da arti; sem o bit mede-se com o comando errado"
verificar_execucavel "UI/tools/instalar.sh" \
  "instala o Node local com o SHA-256 fixado"
verificar_execucavel "UI/tools/onyxchat" \
  "verificar / instalar as dependências da interface"

echo
echo "──────────────────────────────────────────────────────────"
if (( FALHAS == 0 )); then
  # O `9` é a contagem de `verificar_execucavel` acima. Está escrito à mão
  # em vez de contado, e um número fixo que mente é pior do que um número
  # que se mantém: a linha de summary é o que se lê quando se quer saber o
  # que o portão cobriu, e «8» quando a lista tem nove lê-se como «falhou
  # um e não percebo qual». Para contar em vez de escrever, o `9` passa a
  # ser `${#EXECUTAVEIS[@]}` e cada `verificar_execucavel` acrescenta o
  # nome — que é a forma de isto deixar de ser um número que alguém tem de
  # lembrar de actualizar.
  printf '\033[0;32m%s\033[0m — %d artefactos, %d fontes, %d executáveis, %d pendência(s), %d por versionar\n' \
    "todas as verificações passam" \
    "${#ARTEFACTOS[@]}" "${#FONTES[@]}" 9 "${#PENDENCIAS[@]}" "$POR_VERSIONAR"
  exit 0
fi
printf '\033[1;31m%d falha(s)\033[0m\n' "$FALHAS"
exit 1