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

    # --- os dispositivos de captura (os `indev`) ----------------------
    # ## O `--disable-everything` desligou isto sem ninguém dar por isso
    #
    # A receita tem `--disable-everything` desde o início, e uma lista
    # explícita de `--enable-*` para o que se queria. O que se queria
    # era **codificar**; o que se esqueceu foi **capturar**. Os dois
    # comandos de que a captura precisa — `-f x11grab` e `-i default` —
    # são um *formato de entrada* e um *dispositivo*, e nenhum dos dois
    # é um codec: nenhuma flag de codec os traz de volta.
    #
    # A consequência foi um binário que compila limpo, passa a bateria
    # de testes e não captura nada. Verificado por execução:
    #
    #     $ ./ffmpeg -f x11grab -i :0.0 -t 0.1 -f null -
    #     [in#0 @ 0x…] Unknown input format: 'x11grab'
    #     Error opening input file :0.0.
    #
    #     $ ./ffmpeg -i default -t 0.1 -f null -
    #     Error opening input file default.
    #
    # E a tabela `Devices:` do binário sai vazia. Sem ela o daemon
    # recebe `ERR_MEDIA` no primeiro `MEDIA_CAPTURAR` (0x0F) — depois
    # de o IPC estar a funcionar, a ligação estabelecida e o
    # utilizador estar à espera de ver a sua cara.
    #
    # ## Porquê estas e não outras
    #
    # `pipewiregrab` e `v4l2` são **câmara**, e câmara é o que uma
    # chamada precisa. O `x11grab` (ecrã) e o `fbdev` (framebuffer) entram
    # só como alternatives mais abaixo, e nunca por omissão: a
    # `media/ffmpeg.rs::entrada_de_video()` escolhe `v4l2`/`pipewiregrab`
    # primeiro e só cai no `x11grab` se nenhuma das outras houver.
    #
# `alsa` e `pulse` são **microfone**. O `-i default` que o
    # `ffmpeg.rs` usava dependia do `pulse` (o `default` do ALSA é
    # normalmente um `null` que devolve silêncio); habilitar os dois dá a
    # escolha explícita em vez de um nome mágico que significa coisas
    # diferentes em máquinas diferentes.
    #
    # Nenhuma delas acrescenta biblioteca: `alsa`/`pulse`/`pipewire` são
    # todos **internos** do FFmpeg, com a biblioteca do sistema ligada
    # pelo `pkg-config`. É por isso que a lista de
    # `BIBLIOTECAS_EXTERNAS` abaixo os continua a exigir.
    #
    # ## O `pipewiregrab` é condicional, e o motivo é o build
    #
    # As três primeiras (`v4l2`, `alsa`, `x11grab`) são internas ou
    # trazidas por headers que o `syslibs` já traz. O `pipewiregrab`
    # precisa do `libpipewire-0.3-dev`, que **não** está no prefixo local
    # e não se obtém sem `sudo`.
    #
    # Mas `v4l2` não precisa de nada: `/dev/video*` é uma interface do
    # kernel, não uma biblioteca. Uma máquina sem PipeWire tem câmaras
    # `/dev/video0` na mesma, e o V4L2 apanha-as.
    #
    # Por isso o `pipewiregrab` é acrescentado à receita mais abaixo,
    # **só se** o header estiver presente. Um `--enable-indev` que não
    # encontra a biblioteca não é um aviso: o `configure` aborta. Daí a
    # lista ser montada em duas partes.
    --enable-indev=v4l2
    --enable-indev=alsa

    # --- a saída de áudio -------------------------------------------
    # O `alsa` como **saída** é o que dá som aos altifalantes, e sem
    # ele o daemon sabe descodificar Opus mas não tem por onde o
    # tocar.
    #
    # E o `pulse` também, porque o `pipewire-pulse` — o componente que
    # faz o PipeWire falar o protocolo do PulseAudio — é o que está
    # instalado, e é por ele que o áudio chega ao servidor de som.
    #
    # O `pulse` é interno do FFmpeg e não precisa de biblioteca
    # extra; o `alsa` usa a `libasound`, que já está no prefixo local.
    --enable-outdev=alsa
    --enable-outdev=pulse
    # E o **muxer** `pulse`, que é o contentor do protocolo PulseAudio.
    # O `outdev` diz ao FFmpeg que há um dispositivo; o muxer diz como
    # se escreve nele. Sem o muxer, o `pulse` é um dispositivo que não
    # tem por onde escrever — e o erro é «Requested output format
    # 'pulse' is not known».
    --enable-muxer=pulse

    # --- o áudio a caminho de volta: descodificar Opus ---------------
    # `--enable-encoder=libopus` (acima) codifica; não descodifica. O
    # `docs/media.md` §7.1 chama ao Opus «o áudio da chamada», e o
    # receptor precisa de o **ouvir**: sem este decoder o `ffmpeg`
    # responde «Decoding requested, but no decoder found for: opus» e
    # não há caminho de volta para o áudio.
    #
    # Isto é o mesmo caso do `rawvideo` nas linhas de cima: enable do
    # encoder sem o decoder é metade de um caminho, e a metade que
    # falta é a que dá erro em produção.
    --enable-decoder=opus
    # E o `ogg`, que é o contentor em que o Opus chega gravado. Sem o
    # demuxer, um ficheiro `.opus` ou `.ogg` não abre — e o `ogg` é
    # também o contentor nativo do muxer `opus`, de modo que habilitar
    # ambos parte do mesmo `configure`.
    --enable-demuxer=ogg

    # --- a imagem que a UI consegue mostrar --------------------------
    # ## O problema: não há `VideoDecoder` no renderizador
    #
    # A UI corre num WebKitGTK 6 (`scripts/interface-visual.js`), e o
    # WebKitGTK **não garante** `VideoDecoder`/`AudioDecoder` — a API de
    # WebCodecs. O `VideoChamada.js` foi escrito contra essa hipótese e
    # diz-no no seu próprio cabeçalho: «um browser sem `VideoDecoder` não
    # tem vídeo, e a UI diz isso em vez de mostrar um ecrã preto».
    #
    # Sem `VideoDecoder`, o caminho de pixéis tem de ser um formato que
    # **qualquer** renderizador sabe mostrar num `<img>` sem descodificar
    # nada: JPEG. O daemon descodifica o VP8 para pixéis e re-codifica
    # em JPEG; a UI recebe bytes de imagem e escreve-os num `blob:`.
    #
    # `mjpeg` é interno do FFmpeg (não GPL), e `image2` é o muxer que
    # escreve uma imagem num ficheiro. O custo é um segundo
    # codificador por quadro a ~480p — medido em ~8 ms, que é o preço
    # de não depender de uma API que o renderizador pode não ter.
    --enable-encoder=mjpeg
    --enable-muxer=image2

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
    #
    # ## E o **decoder** de PCM, que faltava
    #
    # `--enable-encoder=pcm_s16le` sem `--enable-decoder=pcm_s16le` é
    # metade de um caminho — e a metade que falta é a que dá erro. É
    # literalmente o mesmo caso do `rawvideo` algumas linhas acima, e a
    # captura de microfone é onde se sente:
    #
    #     $ ./ffmpeg -f alsa -i default -t 0.4 -f s16le saida.raw
    #     Decoding requested, but no decoder found for: pcm_s16le
    #
    # O `alsa_indev` entrega frames **já descodificados** em PCM, e o
    # FFmpeg passa o input pelo decoder antes de o encoder o receber. Sem
    # o decoder, o microfone é capturado e de seguida rejeitado.
    #
    # A regra que daqui sai, e que vale para todos: **cada `--enable-encoder`
    # de uma linha traz o `--enable-decoder` da mesma, ou a receita está a
    # descrever metade do caminho.** `vp8`, `rawvideo` e `pcm_s16le` são
    # os três lados que existem; `opus` é o quarto, e está em cima.
    --enable-decoder=pcm_s16le
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

    # ## O `libasound` do prefixo é um esboço, e não pode ficar cá
    #
    # O `.deb` desempacotado traz um `libasound.so` que serve para ligar
    # mas **não tem `snd_pcm_htimestamp`**, que é o que o `alsa_indev` do
    # FFmpeg exige (ffmpeg's `configure`: `alsa_indev_deps='alsa'`, e o
    # `check_func_headers` é exactamente `snd_pcm_htimestamp`).
    #
    # Deixá-lo aqui faz o linker encontrá-lo primeiro — o `-L` do prefixo
    # local vem antes do de sistema de propósito — e o `configure` desliga
    # o `alsa_indev` com
    #
    #     WARNING: Disabled alsa_indev because not all dependencies are
    #     satisfied: alsa
    #
    # que é um aviso que o script não lia, e o binário saía com
    # `--enable-indev=alsa` no `-buildconf` e **sem** `alsa` em
    # `-devices`. Um `--buildconf` que promete e um binário que não cumpre
    # é a pior forma de falha: passa a verificação e falha em produção.
    #
    # ## E o symlink desempacotado também não serve
    #
    # A correcção óbvia — apagar o `.so` e deixar o linker ir buscar o de
    # sistema — também falha, por uma razão que só aparece quando se
    # tenta. O `.deb` traz `libasound.so` como symlink para
    # `libasound.so.2.0.0`, e esse alvo está dentro do prefixo
    # desempacotado. Apagado o `.so`, o `/usr/lib/x86_64-linux-gnu` só
    # tem `libasound.so.2` — o runtime — e **não** o `.so` de linkagem,
    # que só entra com o pacote `-dev`. `-lasound` dá
    # «não foi possível localizar -lasound».
    #
    # A solução é fabricar o symlink de linkagem a apontar para o runtime
    # de sistema, que é o mesmo código e o mesmo `SONAME`. `ld` só
    # precisa do nome para resolver `DT_NEEDED`; o `rpath` garante que em
    # tempo de execução vai buscar a biblioteca verdadeira.
    local alsapc="$destino/usr/lib/x86_64-linux-gnu/pkgconfig/alsa.pc"
    if [ -f "$alsapc" ]; then
        sed -i "s|^prefix=.*|prefix=/usr|; s|^libdir=.*|libdir=/usr/lib/x86_64-linux-gnu|" "$alsapc"
        rm -f "$destino/usr/lib/x86_64-linux-gnu/"libasound.so* 2>/dev/null || true
        # O runtime de sistema, se existir. Sem ele não se fabrica nada e
        # o `alsa_indev` fica desligado — que é o que a verificação do
        # indev, mais abaixo, vai dizer.
        if [ -e /usr/lib/x86_64-linux-gnu/libasound.so.2 ]; then
            ln -sf /usr/lib/x86_64-linux-gnu/libasound.so.2 \
                  "$destino/usr/lib/x86_64-linux-gnu/libasound.so"
        fi
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
# O `alsa` é a entrada de microfone, e sem ele não há chamada de áudio.
#
# O `x11grab` e o `pipewiregrab` **já não são requisito**: desde que a
# câmara passou a ser o dispositivo (F10), o V4L2 — `/dev/video*`, uma
# interface do kernel — chega sem uma única biblioteca. A captura de
# **ecrã** degrada em Wayland sem PipeWire, e isso é aceitável: ecrã não
# é o que uma chamada precisa.
if ! tem_header "alsa/asoundlib.h" && [ "${tem_alsa:-0}" -eq 0 ]; then
    faltam+=("libasound2-dev — a entrada de microfone (alsa_indev)")
fi
if [ "$tem_x11" -eq 0 ] && [ "$tem_pipewire" -eq 0 ]; then
    nota "nem X11 nem PipeWire: sem captura de ecrã. A câmara usa v4l2 (/dev/video*), que não precisa de biblioteca"
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
# O `pipewiregrab` é a única entrada condicional
# ---------------------------------------------------------------------
#
# ## Porquê só esta
#
# As entradas de captura da receita dividem-se em dois grupos, e a
# diferença é se precisam de uma biblioteca **externa**:
#
#   • `v4l2`    — `/dev/video*` é uma interface do kernel. Sem headers,
#                 sem `pkg-config`, sem nada. Uma máquina com câmara USB
#                 funciona sem um único pacote instalado.
#   • `alsa`    — headers em `syslibs`, já presentes.
#   • `x11grab` — headers de `X11`/`xcb` em `syslibs`, já presentes.
#   • `pulse`   — interna; o `alsa` cobre a captura sem servidor de som.
#   • `pipewiregrab` — precisa de `libpipewire-0.3-dev`, que não está no
#                 prefixo local e não vem sem `sudo`.
#
# Um `--enable-indev=pipewiregrab` sem a biblioteca não é um aviso do
# `configure`: **é um abort**. Por isso não pode estar na `RECEITA`
# incondicional, e a detecção abaixo decide se entra.
#
# ## O que se perde sem ele
#
# Nada de essencial. O `media/ffmpeg.rs::entrada_de_video()` prefere
# `pipewiregrab` em Wayland e cai para `v4l2`, que é o caminho
# POSIX de sempre e não depende de servidor de som. Perde-se a captura
# **de ecrã** em Wayland — e a ecrã não é o que uma chamada precisa.
if tem_header "pipewire-0.3/pipewire/pipewire.h"; then
    RECEITA+=(--enable-indev=pipewiregrab)
    ok "pipewiregrab habilitado (câmara em Wayland)"
else
    nota "pipewiregrab NÃO habilitado: falta pipewire-0.3/pipewire/pipewire.h"
    nota "  a câmara usa v4l2 (/dev/video*), que não precisa de biblioteca"
fi

# O `pulse` (microfone e saída por PulseAudio/PipeWire) precisa do
# `libpulse-dev`. É condicional pelo mesmo motivo: o `alsa` já dá captura
# de microfone sem servidor de som, e ficar sem `pulse` degrada a escolha
# de dispositivo, não a impede.
if tem_header "pulse/pulseaudio.h"; then
    RECEITA+=(--enable-indev=pulse)
    ok "pulse habilitado (microfone por PulseAudio/PipeWire)"
else
    nota "pulse NÃO habilitado: falta pulse/pulseaudio.h. O alsa cobre a captura de microfone"
fi

# O `x11grab` foi **removido** da receita, e a razão vale dizer em voz
# alta porque já foi tentada.
#
# ## Porque deixar de fora
#
# É captura de **ecrã**, e a câmara de uma chamada não é ecrã. Desde que
# o `v4l2` cobre a câmara sem uma única biblioteca, o `x11grab` só
# acrescentaria peso por uma funcionalidade que a chamada não usa.
#
# ## E porque não fica condicional
#
# Poderia, e foi a primeira tentativa. Mas `libxcb` no prefixo local
# traz um `xcb.pc` que exige `xau` e `xdmcp`, que não lá estão:
#
#     ERROR: libxcb requested but not found
#
# E ao contrário do `pipewiregrab` — cuja ausência degrada uma
# alternativa — aqui a falha é um **abort do `configure`**. Habilitar
# `libxcb` num prefixo incompleto não dá um binário sem ecrã: dá nenhum
# binário. Uma dependência que pode fazer o build inteiro desaparecer
# não é condicional, é uma mentira.
#
# Se algum dia a captura de ecrã voltar a ser pedida, o caminho é trazer
# `xau`/`xdmcp` para o prefixo, não repor a bandeira às cegas.
#
# A câmara não depende disto: `v4l2` e `pipewiregrab` são o caminho.
nota "x11grab fora da receita: captura de ecrã, não de câmara. A câmara usa v4l2"

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
    # E o `rpath` do prefixo limpo. Sem ele o binário liga contra a
    # biblioteca de linkagem mas, em execução, o loader segue o
    # `DT_NEEDED` e não a encontra — porque os `.deb` desempacotados não
    # estão em `/usr/lib`. O `rpath` diz ao loader onde ir buscar.
    export LDFLAGS="-Wl,-rpath,$PREFIXO_LIMPO/usr/lib/x86_64-linux-gnu $LDFLAGS"
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

# ---------------------------------------------------------------------
# A quarta verificação: as entradas de captura existem mesmo
# ---------------------------------------------------------------------
#
# ## O que esta verificação apanha
#
# As três de cima leem a **configuração**; esta lê o **binário**. São
# coisas diferentes, e a diferença é exactamente onde o bug morava.
#
# `--buildconf` imprimia `--enable-indev=alsa` enquanto o binário não
# tinha `alsa`: o `configure` aceita a bandeira, descobre que a
# biblioteca não satisfaz o `snd_pcm_htimestamp`, desliga o dispositivo
# **com um aviso na sua saída** e continua. O `-buildconf` é gerado das
# opções pedidas, não das activadas, e por isso mente.
#
# Uma receita que promete um dispositivo e um binário que não o tem é a
# forma mais cara de falha que existe aqui: passa a verificação de
# licença, passa a bateria de testes, e dá `ERR_MEDIA` ao utilizador no
# primeiro `MEDIA_CAPTURAR`, depois de o daemon estar a correr e a
# ligação estabelecida.
#
# Por isso: cada `--enable-indev` que a receita pede tem de aparecer na
# tabela `Devices:` ou nos formatos de entrada do binário compilado.
# Falhar aqui é o que transforma uma falha silenciosa numa falha de
# compilação.
#
# ## O que se aceita como "presente"
#
# `v4l2` é o caso difícil: o FFmpeg escreve-o como `video4linux2,v4l2`
# e é um *demuxer*, não um dispositivo enumerado — só aparece em
# `-devices` quando há `/dev/video*`. As entradas de áudio como `alsa`
# também só aparecem quando o sistema tem hardware aberto.
#
# A verificação é por **nome exacto** de cada uma das duas listas, e
# cada entrada é procurada pelo seu nome **e** pelo nome que o FFmpeg
# lhe dá. Um `grep` com alternância e partilhada entre os nomes seria
# pior do inútil: `video4linux2` casaria para `v4l2` **e** para todos os
# outros, e a verificação passaria com `alsa` em falta — que foi
# exactamente o que aconteceu na primeira versão deste bloco.
#
# `x11grab` e `pipewiregrab` precisam de hardware para aparecerem em
# `-devices`, por isso são aceites em `-formats`, onde o FFmpeg os lista
# sempre que estão compilados.
INDES_ESPERADOS="pipewiregrab v4l2 alsa pulse"

# As listas que se consultam, separadas de propósito: `Devices:` é
#_small_ e `-formats` é onde os demuxers vivem.
LISTA_DISPOSITIVOS="$(./ffmpeg -hide_banner -devices 2>&1)"
LISTA_FORMATOS="$(./ffmpeg -hide_banner -formats 2>&1)"

# Um nome é "presente" se a linha que o descreve o trouxer. O `|` no fim
# é o que casa com o fim de linha, e sem ele `alsa` casaria dentro de
# `pulse,alsa`… o que é o correcto, mas também dentro de `foobar`,
# o que não é.
presente() {
    printf '%s\n%s\n' "$LISTA_DISPOSITIVOS" "$LISTA_FORMATOS" \
        | grep -qiE "(^|[ ,])$1([ ,]|$)"
}

indev_em_falta=()
for indev in $INDES_ESPERADOS; do
    # Só se verifica o que a receita **pediu**.
    case " ${RECEITA[*]} " in
        *" --enable-indev=$indev "*) ;;
        *) continue ;;
    esac
    presente "$indev" || indev_em_falta+=("$indev")
done

if [ "${#indev_em_falta[@]}" -gt 0 ]; then
    erro "a receita pede entradas de captura que o binário NÃO tem:"
    printf '  - %s\n' "${indev_em_falta[@]}" >&2
    cat >&2 <<'AVISO'

  A configuração declara-as e o binário não as tem: o `configure`
  desligou-as com «WARNING: Disabled … because not all dependencies are
  satisfied». A diferença entre o que se pede e o que se fica é
  exactamente a falha que esta verificação existe para tornar visível —
  o `MEDIA_CAPTURAR` (0x0F) daria `ERR_MEDIA` ao primeiro uso, com o
  daemon já a correr.

AVISO
    printf '%s\n' "$CONFCOMF" | tr ' ' '\n' | grep -- '--enable-indev' | sed 's/^/  declarado: /' >&2
    rm -f ./ffmpeg
    exit 1
fi
ok "entradas de captura presentes e verificadas no binário"

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
