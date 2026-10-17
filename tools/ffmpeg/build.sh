#!/usr/bin/env bash
# =====================================================================
# build.sh — compila o `ffmpeg` a partir da fonte que o `fetch.sh` trouxe
# ---------------------------------------------------------------------
# Compila com `--disable-gpl --disable-nonfree` e, no fim, **confirma
# com o próprio binário** que a configuração é a que a excepção de
# `THIRD-PARTY.md` promete. Um binário que não confirma é apagado.
#
# ## A garantia, e onde ela está
#
# A garantia de licença não está no manifesto nem neste script: está no
# binário. `ffmpeg -buildconf` imprime a configuração com que foi
# compilado, e o FFmpeg imprime também, na primeira linha de
# `ffmpeg -version`, a licença que daí resulta. Se o `--disable-gpl` não
# estiver lá, o que saiu é GPL e a excepção não o cobre.
#
# Por isso o passo final **não é opcional** e **não avisa**: apaga o
# executável e sai com erro. Um `ffmpeg` GPL num cache à espera de ser
# embebido por um `cargo build` é a forma mais provável de a excepção
# ser ultrapassada sem ninguém dar por isso.
#
# ## `--disable-everything` e a lista de `--enable`
#
# O `--disable-everything` seguido de uma lista explícita é o que faz
# isto mínimo: nada entra no binário sem que alguém o tenha pedido por
# nome. Sem ele, `--disable-gpl` sozinho deixa entrar tudo o que o
# FFmpeg traz de origem, e o que entra de origem muda entre versões —
# o que tornaria a receita dependente da versão.
#
# ## As bibliotecas que faltam
#
# `libopus`, `libvpx`, `libX11`/`libxcb` e `libpipewire` **não** estão
# em `tools/ffmpeg/`. Este script recusa com uma mensagem que diz o que
# falta e onde o obter, em vez de produzir um `ffmpeg` que compila e não
# captura. A falha alternativa — um `ffmpeg` que compila sem `x11grab`
# e que só dá erro em produção, dentro de um `x11grab` que o ffmpeg
# diz desconhecer — é exactamente o tipo de sucesso que se parece com
# uma falha e custa mais a descobrir.
# =====================================================================
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CACHE="$AQUI/cache"

# Prefixos locais desempacotados, se existirem.
#
# `VPX_LOCAL` para o `libvpx` (o codec de vídeo) e `SIS_LOCAL` para o
# resto (Opus, X11, ALSA). São prefixos em vez de instalação de sistema pelo
# mesmo motivo que o Node: `sudo` não está disponível, e um `.deb`
# desempacotado para dentro do projecto dá exactamente o mesmo
# resultado.
#
# `SIS_LOCAL` tem o `.so` desempacotado e não o link simbólico; o linker
# precisa do `-l` com o nome do ficheiro, e por isso o `prefixo/lib`
# entra no `-L` tal e qual.
#
# ## Por que isto existe
#
# `libvpx-dev` é um pacote de sistema e a instalação normal precisa de
# `sudo`. Este projecto embute dependências, e o `UI/tools/instalar.sh`
# já faz exactamente isto com o Node: descarrega, desempacota para um
# directório local, e usa a partir daí.
#
# O mesmo vale para o `libvpx`: um `.deb` descarregado e desempacotado
# em `.cache/libvpx/` dá os headers e a biblioteca estática, e o
# `--disable-x86asm` do FFmpeg casa com ela — nada disto precisa de
# privilégios.
#
# Se o directório não existir, o build continua a procurar pelo sistema
# e a recusar com a mensagem de sempre. Este caminho é uma alternativa,
# não uma obrigação.
VPX_LOCAL="$AQUI/../../.cache/libvpx"
SIS_LOCAL="$AQUI/../../.cache/syslibs"

# Prefixo limpo, sem acentos nem espaços no caminho.
#
# Por que é preciso
#
# O caminho do projecto tem acentos (`Programaçao`) e o `pkg-config` não
# lida com eles: um `prefix` com acentes quebra o `configure` com um erro
# que não menciona acentos, o que é a pior forma de falha.
#
# Por isso o passo seguinte monta as dependências desempacotadas num
# prefixo sem acentos, reescrevendo os `prefix=`/`libdir=` dos `.pc` para
# apontarem para lá. É uma gambiarra de caminho, não de conteúdo: os
# headers e as bibliotecas são os do `.deb`.
PREFIXO_LIMPO="${TMPDIR:-/tmp}/onyx-deps"

# O `pkg-config` tambem e dependencia, e tambem nao vem numa maquina
# minima — sem ele o `configure` diz `opus not found using pkg-config`,
# que parece um problema de `libopus` e e um problema do próprio
# `pkg-config`. Embutido, como o resto.
FERRAMENTAS_LOCAL="${TMPDIR:-/tmp}/onyx-deps-tools/usr/bin"
FERRAMENTAS_LIBS="${TMPDIR:-/tmp}/onyx-deps-tools/usr/lib/x86_64-linux-gnu"

# ---------------------------------------------------------------------
# As duas funções que escrevem
# ---------------------------------------------------------------------
cor() { printf '\033[%sm%s\033[0m\n' "$1" "$2"; }
erro() { cor '1;31' "erro: $1" >&2; }
ok() { cor '1;32' "$1"; }
nota() { cor '2' "$1"; }

# ---------------------------------------------------------------------
# As bandeiras — a receita, e ela é a excepção
# ---------------------------------------------------------------------

# O mínimo que fica **fora** da regra, escrito uma vez e aplicado
# directamente. Cada linha diz porquê no comentário acima; o que a
# receita não disser não entra no binário.
RECEITA=(
    # --- o que é copyleft e não entra -------------------------------
    --disable-gpl            # LGPL. A excepção de THIRD-PARTY.md.
    --disable-nonfree        # Redundante depois de --disable-gpl, e fica
                             # para sobreviver a quem leia só uma linha.

    # --- o que este projecto não precisa ------------------------------
    --disable-network        # O ffmpeg nunca fala com a rede.
                             # docs/threat_model.md §A2 é sobre um relay
                             # malicioso; um cliente HTTP dentro de um
                             # executável embebido é superfície que
                             # ninguém pediu.
    --disable-debug          # Símbolos e linhas de depuração: -30% de
                             # tamanho, e o binário embebido vai num
                             # repositório.
    --disable-doc            # Não estamos a distribuir páginas de
                             # manual de um componente de terceiros.
    --disable-x86asm         # Sem `nasm` numa máquina de build, e sem
                             # sudo. Custa ~20% de velocidade ao VP8 —
                             # e este comentário dizia que era
                             # irrelevante porque num `rawvideo` quem
                             # comprimia era o browser. Deixou de ser
                             # verdade quando o vídeo passou a
                             # codificar-se aqui (F9).

    # --- o mínimo, e só o mínimo --------------------------------------
    # `--disable-everything` seguido de uma lista explícita é o que
    # torna a receita independente da versão do FFmpeg: sem ele, o que
    # entra de origem mudaria a cada release.
    --disable-everything
    --enable-ffmpeg

    # --- formatos de contentor do modo não-streaming ------------------
    # `docs/media.md` §3.3. São multiplexers internos, sem biblioteca
    # externa e sem licença a pensar neles.
    --enable-muxer=matroska
    --enable-muxer=mp4
    --enable-muxer=webm
    --enable-muxer=opus
    --enable-muxer=pcm_s16le
    --enable-muxer=rawvideo
    # O `ivf` é o contentor de saída do VP8 no streaming. E' o mínimo
    # que dá: um cabeçalho de 32 bytes mais os quadros, e é o que
    # permite a um pipe ler quadro a quadro sem demultiplexar. O `webm`
    # seria mais padrão mas traz um índice que um pipe não pode usar.
    --enable-muxer=ivf
    --enable-muxer=wav

    # --- demuxers e decoders para os contentores acima ---------------
    --enable-demuxer=matroska
    --enable-demuxer=mov
    --enable-demuxer=wav
    --enable-demuxer=pcm_s16le
    --enable-demuxer=rawvideo
    # O `ivf` e o `vp8` sao o **caminho de volta**: o receptor le o IVF
    # e precisa de descodificar. Sem o demuxer diz «Unknown input
    # format: 'ivf'» e sem o decoder nao descodifica.
    #
    # O daemon so codifica (§8.0.2: um emissor nunca descodifica), mas
    # o binario e o mesmo dos dois lados, e um `ffmpeg` que so sabe
    # codificar nao descodifica o que ele próprio escreveu.
    --enable-demuxer=ivf
    --enable-decoder=vp8
    # E o `rawvideo` a **escrever**: o receptor quer pixéis, e
    # «Automatic encoder selection failed — Default encoder for format
    # rawvideo is probably disabled» é o que sai sem isto.
    --enable-encoder=rawvideo
    # O DECODER de `rawvideo` é obrigatório, e não o ter notado
    # custou tempo: o `-f rawvideo` na entrada precisa de um decoder, e
    # sem ele o ffmpeg diz «Decoding requested, but no decoder found for:
    # rawvideo». A captura do modo de streaming passa quadros crus ao
    # encoder, e é este decoder que os cospe.
    --enable-decoder=rawvideo

    # --- protocolos ---------------------------------------------------
    # `fd` é o que faz `-f opus fd:3` funcionar (docs/media.md §7.1).
    --enable-protocol=fd
    --enable-protocol=file
    # E `pipe`: e' o caminho de **volta**. O receptor escreve o IVF num
    # pipe e le os pixeis de outro, e sem este protocolo o ffmpeg diz
    # «Protocol not found. Did you mean file:pipe:0?» — que parece um
    # erro de escrita e e um protocolo que a receita nao habilitou.
    --enable-protocol=pipe

    # --- filtros ----------------------------------------------------------
    # `scale` é o que faz a geometria: `docs/media.md` §8.1 escreve
    # `scale=W:H`, e sem ele o `ffmpeg` diz «No option name near
    # '320:240'» — que parece um erro de sintaxe e é um filtro que a
    # receita não habilitou.
    #
    # A conversão para `gray` saiu com o VP8 (F9): o codec quer YUV, e
    # um quadro de um só canal teria de ser expandido para três antes
    # de ser comprimido.
    --enable-filter=scale
    --enable-filter=format
    --enable-filter=null

    # --- o vídeo: VP8 é obrigatório -------------------------------
    # Não há codificador VP8 no FFmpeg — o `libvpx` é externo, como o
    # `libopus`, e BSD.
    #
    # ## Por que é obrigatório, e por que substitui o `rawvideo`
    #
    # O streaming enviava `rawvideo` e codificava em telhas 16×16. A
    # medição de F9 (`examples/medir_media.rs`) mostrou que esse
    # caminho não fechava: uma keyframe a 640×480 são 414 456 bytes de
    # texto contra 65 536 de `MAX_PLAINTEXT`, e o orçamento de 12,8
    # msg/s dá ~629 kB/s no total.
    #
    # O VP8 é o codec de videochamada de baixo bitrate, e resolve o
    # problema da raiz: faz compensação de movimento e codificação
    # entrópica, que é o que as telhas caseiras não faziam. 480p a
    # 30 fps cabe em 300–500 kB/s — dentro do orçamento.
    #
    # `libvpx` é BSD, como `libopus`, e por isso cabe na excepção de
    # `THIRD-PARTY.md` ao LGPL do FFmpeg.
    # O nome do encoder é `libvpx_vp8_encoder`, não `libvpx`: o
    # `--enable-encoder=libvpx` é aceite e **não corresponde a nada**,
    # e o FFmpeg avisa «Option --enable-encoder=libvpx did not match
    # anything» e segue em frente. O `--enable-libvpx` liga a
    # biblioteca; o `--enable-encoder` liga o encoder.
    --enable-encoder=libvpx_vp8
    --enable-encoder=libvpx_vp9
    --enable-encoder=libopus
    --enable-libvpx
    --enable-parser=vp8

    # --- o áudio: Opus é obrigatório ---------------------------------
    # Não há um codificador Opus nativo no FFmpeg, e sem ele o modo
    # streaming não existe. `libopus` é BSD.
    --enable-libopus

    # --- o áudio em PCM, para o caminho sem Opus ----------------------
    # `docs/media.md` §4.6: a reprodução pode receber PCM, e o
    # `pcm_s16le` é interno.
    --enable-encoder=pcm_s16le
)

# Bibliotecas **externas** que a receita pede, e a mensagem para quem não
# as tem. Separadas da receita porque não são opcionais da nossa
# vontade — são o que falta à máquina, e a mensagem tem de dizer o
# pacote de cada distribuição em vez de um nome de biblioteca.
BIBLIOTECAS_EXTERNAS=(
    "libopus|Opus (áudio)|libopus-dev"
    "libvpx|VP8 (vídeo, codec de videochamada)|libvpx-dev"
    "libX11|x11grab (ecrã em X11)|libx11-dev libxcb1-dev"
    "libpipewire|pipewiregrab (ecrã em Wayland)|libpipewire-0.3-dev"
    "libasound|alsa (microfone)|libasound2-dev"
)

# ---------------------------------------------------------------------
# O que falta
# ---------------------------------------------------------------------

tem_header() { [ -f "/usr/include/$1" ] || [ -f "/usr/local/include/$1" ]; }

# O mesmo header, mas num prefixo local desempacotado.
# `local` não é portátil e este script corre em `sh` nalguns sistemas.
# Os dois argumentos são lidos com `${1}` e `${2}` directamente.
tem_header_derivado() {
    [ -f "${1}/usr/include/${2}" ]
}

# Monta as dependencias desempacotadas num prefixo sem acentos.
#
# E o passo que faz o `pkg-config` funcionar: um `.pc` de um `.deb` diz
# `prefix=/usr`, que aqui nao existe, e o `configure` morre a testar
# `libopus` sem dizer porquê. Reescrever `prefix` e `libdir` para o
# prefixo local e o que resolve — e e uma gambiarra de caminho, nao de
# conteudo: os headers e as bibliotecas sao os do `.deb`.
montar_prefixo_limpo() {
    local destino="$1"; shift
    local pfx
    rm -rf "$destino"
    mkdir -p "$destino/usr/lib/x86_64-linux-gnu/pkgconfig"
    for pfx in "$@"; do
        [ -d "$pfx" ] || continue
        # Acumula: nao se apaga o destino a cada prefixo, porque os
        # prefixos sao varios (`libvpx` num, o resto noutro) e um
        # `rm -rf` no ciclo apagava o que o anterior tinha copiado.
        [ -d "$pfx/usr/include" ] && cp -rL "$pfx/usr/include/." "$destino/usr/include/" 2>/dev/null || true
        [ -d "$pfx/usr/lib/x86_64-linux-gnu/pkgconfig" ] \
            && cp "$pfx/usr/lib/x86_64-linux-gnu/pkgconfig/"*.pc "$destino/usr/lib/x86_64-linux-gnu/pkgconfig/" 2>/dev/null || true
        cp "$pfx"/usr/lib/x86_64-linux-gnu/*.a "$destino/usr/lib/x86_64-linux-gnu/" 2>/dev/null || true
        cp "$pfx"/usr/lib/x86_64-linux-gnu/*.so* "$destino/usr/lib/x86_64-linux-gnu/" 2>/dev/null || true
    done

    # Todos os `.pc` apontam para o prefixo novo, nao para `/usr` de onde
    # vieram. Sem isto o `configure` procura em `/usr` e nao os ve.
    sed -i "s|^prefix=.*|prefix=$destino/usr|; s|^libdir=.*|libdir=$destino/usr/lib/x86_64-linux-gnu|" \
        "$destino/usr/lib/x86_64-linux-gnu/pkgconfig/"*.pc 2>/dev/null || true

    # O `opus.pc` do Ubuntu: `Cflags` so serve `<opus.h>` (o FFmpeg usa
    # `<opus/opus.h>`) e `Libs` nao declara `-lm` (o `libopus.a` usa
    # `sqrtf`). Sem estas duas correccoes o `configure` falha com
    # «opus not found using pkg-config», que nao menciona headers nem
    # matematica.
    local opuspc="$destino/usr/lib/x86_64-linux-gnu/pkgconfig/opus.pc"
    if [ -f "$opuspc" ]; then
        sed -i "s|^Cflags: -I\${includedir}/opus|Cflags: -I\${includedir}/opus -I\${includedir}|" "$opuspc"
        grep -q '^Libs:.*-lm' "$opuspc" \
            || sed -i "s|^Libs:.*|Libs: -L\${libdir} -lopus -lm|" "$opuspc"
    fi

    # `vpx.pc` precisa de `-lm` tambem: o `libvpx.a` usa funcoes
    # matematicas, e o `configure` liga o encoder com `-lvpx -lm`.
    local vpxpc="$destino/usr/lib/x86_64-linux-gnu/pkgconfig/vpx.pc"
    if [ -f "$vpxpc" ]; then
        grep -q '^Libs:.*-lm' "$vpxpc" \
            || sed -i "s|^Libs:.*|Libs: -L\${libdir} -lvpx -lm -lpthread|" "$vpxpc"
    fi
    [ -f "$opuspc" ]
}

# O prefixo limpo e montado ANTES das verificacoes: e ele que tem os
# headers, e sem ele a deteccao diria que estao a faltar.
montar_prefixo_limpo "$PREFIXO_LIMPO" "$SIS_LOCAL" "$VPX_LOCAL" || true
if [ -d "$PREFIXO_LIMPO" ]; then
    tem_header_derivado "$PREFIXO_LIMPO" vpx/vpx_encoder.h && tem_vpx=1
    tem_header_derivado "$PREFIXO_LIMPO" opus/opus.h && tem_opus=1
    tem_header_derivado "$PREFIXO_LIMPO" X11/Xlib.h && tem_x11=1
    tem_header_derivado "$PREFIXO_LIMPO" alsa/asoundlib.h && tem_alsa=1
    nota "prefixo limpo: $PREFIXO_LIMPO"
fi

faltam=()
# `libopus` e `libasound` são necessities para o streaming funcionar.
# `libX11` e `libpipewire` são **alternativas** entre si: com uma das
# duas há captura de ecrã. É por isso que o teste abaixo é «pelo menos
# uma» e não «todas».
# `libopus` e `libvpx` sao **necessities**: sem audio nao ha chamada, e
# sem VP8 o video nao cabe no orcamento (F9). `libX11` e `libpipewire`
# sao alternativas entre si: com uma das duas ha captura de ecra.
tem_opus=0
tem_vpx=0
tem_x11=0
tem_pipewire=0

(tem_header "opus/opus.h" || tem_header_derivado "$SIS_LOCAL" opus/opus.h) && tem_opus=1
if tem_header_derivado "$VPX_LOCAL" vpx/vpx_encoder.h || tem_header "vpx/vpx_encoder.h"; then
    tem_vpx=1
fi
(tem_header "X11/Xlib.h" || tem_header_derivado "$SIS_LOCAL" X11/Xlib.h) && tem_x11=1
tem_header "pipewire-0.3/pipewire/pipewire.h" && tem_pipewire=1

[ "$tem_opus" -eq 1 ] || faltam+=("libopus-dev — Opus é o áudio de docs/media.md §7.1, e não há codificador Opus nativo")
[ "$tem_vpx" -eq 1 ] || faltam+=("libvpx-dev — VP8 é o vídeo de docs/media.md §8. Sem ele o ffmpeg não tem codec de vídeo, e o rawvideo não cabe no orçamento de mensagens (F9)")
if [ "$tem_x11" -eq 0 ] && [ "$tem_pipewire" -eq 0 ]; then
    faltam+=("libx11-dev + libxcb1-dev, ou libpipewire-0.3-dev — sem um deles o ffmpeg compila mas não captura o ecrã")
fi
if ! tem_header "alsa/asoundlib.h" && [ "${tem_alsa:-0}" -eq 0 ]; then
    faltam+=("libasound2-dev — a entrada de áudio")
fi

if [ "${#faltam[@]}" -gt 0 ]; then
    erro "faltam bibliotecas para compilar o ffmpeg que este projecto usa:"
    printf '  - %s\n' "${faltam[@]}" >&2
    cat >&2 <<'AVISO'

  Deliberado: recusar aqui é melhor do que compilar e falhar em
  produção. Um ffmpeg sem x11grab é um binário que passa nos testes de
  compilação e dá erro no primeiro MEDIA_INICIAR, dentro de um daemon
  que já arrancou.

  Alternativas que não exigem nada disto: manter o modo degradado
  (docs/media.md §Dependências) e o texto continua a funcionar.

AVISO
    exit 1
fi
ok "bibliotecas presentes"

# ---------------------------------------------------------------------
# O que compilar
# ---------------------------------------------------------------------

TARBAL=$(ls "$CACHE"/ffmpeg-*.tar.xz 2>/dev/null | head -1) || {
    erro "não há fonte em cache. Corre tools/ffmpeg/fetch.sh primeiro."
    exit 1
}
[ -n "$TARBAL" ] || { erro "não há fonte em cache. Corre tools/ffmpeg/fetch.sh primeiro."; exit 1; }

OBRA="$CACHE/obras"
nota "a extrair em ${OBRA##*/}/"
rm -rf "$OBRA"
mkdir -p "$OBRA"
tar -xJf "$TARBAL" -C "$OBRA" --strip-components=1

cd "$OBRA"

# ---------------------------------------------------------------------
# Compilar
# ---------------------------------------------------------------------

JOBS="$(nproc 2>/dev/null || echo 2)"
nota "a configurar com ${#RECEITA[@]} bandeiras"
nota "a compilar com -j$JOBS — a medir, não a estimar"

# O prefixo local, quando existe, entra no `PKG_CONFIG_PATH` e no
# `CPPFLAGS`/`LDFLAGS`. O FFmpeg encontra o `libvpx` por `pkg-config`,
# e um prefixo que o `pkg-config` não vê é um prefixo que não conta.
#
# O `-L` vem **antes** do caminho de sistema por uma razão que só se
# percebe depois: o `libvpx9` de runtime está instalado aqui, e sem o
# `-L` o linker encontraria a de sistema e o `configure` confirmaria uma
# configuração que depois não liga.
if [ -d "$PREFIXO_LIMPO" ]; then
    export PKG_CONFIG_PATH="$PREFIXO_LIMPO/usr/lib/x86_64-linux-gnu/pkgconfig:${PKG_CONFIG_PATH:-}"
    export CPPFLAGS="-I$PREFIXO_LIMPO/usr/include ${CPPFLAGS:-}"
    export LDFLAGS="-L$PREFIXO_LIMPO/usr/lib/x86_64-linux-gnu ${LDFLAGS:-}"
    nota "a compilar com o prefixo limpo: $PREFIXO_LIMPO"
fi

# O `pkg-config` tem de existir **antes** do `configure`, e com a
# `libpkgconf` no `LD_LIBRARY_PATH`: o binário é o `pkgconf`, o nome que
# o `configure` chama é `pkg-config`, e a biblioteca não vem por ombre.
if [ -x "$FERRAMENTAS_LOCAL/pkgconf" ]; then
    export PATH="$FERRAMENTAS_LOCAL:$PATH"
    export LD_LIBRARY_PATH="$FERRAMENTAS_LIBS:${LD_LIBRARY_PATH:-}"
    ln -sf pkgconf "$FERRAMENTAS_LOCAL/pkg-config" 2>/dev/null || true
    nota "pkg-config embutido: $(command -v pkg-config || echo 'NÃO ENCONTRADO')"
fi

if ! ./configure "${RECEITA[@]}"; then
    erro "o configure falhou."
    nota "o log está em $OBRA/ffbuild/config.log"
    exit 1
fi

# O `configure` imprime a licença, e é a primeira verificação de três.
# As outras duas são o `-buildconf` e o `-version` abaixo; as três
# leem o mesmo facto de três maneiras, e é redundância a propósito: um
# único ponto de verificação é um ponto de falha silenciosa.
# A licença vem de `config.h`, não do `config.log`.
#
# O `configure` escreve `License: LGPL...` no terminal, e o `config.log`
# **não tem** essa linha. Ler o log dava vazio e o script saía com
# «o configure não derivou LGPL: «»» depois de o `configure` ter
# derivado LGPL e o ter dito no ecrã.
CONFIG_LICENCA="$(sed -n 's/^#define FFMPEG_LICENSE *//p' config.h | tr -d '"')"
case "$CONFIG_LICENCA" in
    LGPL*) ok "configure: $CONFIG_LICENCA" ;;
    *) erro "o configure não derivou LGPL: «$CONFIG_LICENCA»"; exit 1 ;;
esac

if ! make -j"$JOBS"; then
    erro "a compilação falhou."
    exit 1
fi

# ---------------------------------------------------------------------
# A confirmação, pelo próprio binário
# ---------------------------------------------------------------------

[ -x ./ffmpeg ] || { erro "o make terminou mas não produziu ./ffmpeg"; exit 1; }

# A segunda verificação: a configuração que o binário declara. Sem
# `--disable-gpl` aqui, o que saiu é GPL — e a excepção de
# `THIRD-PARTY.md` não o cobre.
CONFCOMF="$(./ffmpeg -hide_banner -buildconf 2>&1)"
case "$CONFCOMF" in
    *--disable-gpl*) ok "buildconf: --disable-gpl presente" ;;
    *)
        erro "o binário NÃO declara --disable-gpl. A excepção não o cobre."
        printf '%s\n' "$CONFCOMF" | sed 's/^/  /' >&2
        rm -f ./ffmpeg
        exit 1
        ;;
esac

# A terceira: a licença que o binário resolve a partir da
# configuração. As duas coisas podem divergir — a segunda olha para a
# receita, a terceira para o resultado — e é a terceira que interessa.
# A linha da configuração é a **terceira** do `-version`, não a
# primeira: a primeira é `ffmpeg version`, a segunda é `built with`.
# O `sed -n '1s/...'` pegava na primeira e devolvia vazio — e um
# `case` com glob `*--disable-gpl*` casa vazio, por isso o erro
# aparecia só quando a configuração estava mesmo errada.
VERSAO_LICENCA="$(./ffmpeg -hide_banner -version 2>&1 | sed -n 's/^configuration: //p' | head -1)"
case "$VERSAO_LICENCA" in
    *--disable-gpl*) : ;;
    *) erro "a configuração embutida não tem --disable-gpl: $VERSAO_LICENCA"; exit 1 ;;
esac

TAM=$(stat -c%s ./ffmpeg)
ok "ffmpeg compilado: $((TAM / 1024)) KiB, LGPL-2.1-or-later"

# ---------------------------------------------------------------------
# O executável vai para o sítio que o build.rs lê
# ---------------------------------------------------------------------
xz -9e -T0 -c ./ffmpeg > "$CACHE/ffmpeg.xz"
ok "comprimido em cache/ffmpeg.xz ($(( $(stat -c%s "$CACHE/ffmpeg.xz") / 1024 )) KiB)"

rm -rf "$OBRA"

nota "agora: cargo build -p onyxchatd"
nota "      o build.rs embete cache/ffmpeg.xz. Para não o reembeber a"
nota "      cada build, apaga-o depois: rm cache/ffmpeg.xz"
