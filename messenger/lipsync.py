# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""lipsync.py — manter a voz e a cara no mesmo instante.

Uma chamada tem dois canais que chegam ao mesmo tempo e por caminhos
diferentes: o áudio vai em pacotes de Opus, o vídeo vai em quadros de
VP8, e **nenhum dos dois sabe quando o outro vai chegar**. O que os
alinha é o relógio que viaja com cada um deles — o `clock_ms` de
``docs/media.md`` §5.3.

Este módulo é a implementação Python da §5.5: a regra dos três
cenários, com o desvio medido e a decisão que sai dele.

## O invariante

    O áudio é o relógio mestre. O vídeo é apresentado em relação a ele.

E a razão é a da §5.2: o ritmo do áudio é **constante por
construção** — 48 kHz, 160 amostras por bloco, sempre o mesmo — e o do
vídeo não: o `fps` varia, o `ffmpeg` pode entregar dois quadros
seguidos e depois nenhum, e a taxa de bits muda com a cena.

## E porque isto **não** dorme

Porque §5.5 é explícita: dormir localmente converte um problema de
rede num problema de renderização. O quadro chega, a pessoa vê um
congelamento, e nada melhorou.

Tor tem TCP por baixo: **atrasa muito e perde pouco**. O atraso é o
caso comum e é tratado por quem apresenta; a perda é o caso raro e é
tratada pelo descarte.

## E o que o módulo faz, em três linhas

1. Recebe o `clock_ms` do áudio e do vídeo.
2. Calcula o desvio entre eles.
3. Devolve uma **decisão**: apresentar, apresentar já, ou descartar.

E nada mais. Quem apresenta é outra pessoa — o anel, o `ffmpeg` da
saída — e um `sleep` aqui seria um `sleep` no sítio errado.
"""

from __future__ import annotations

from dataclasses import dataclass

__all__ = [
    "LIMITE_MS",
    "Decisao",
    "SessaoLipSync",
]


#: O desvio máximo, em milissegundos, que ainda é sincronia.
#:
#: ## E porque 120
#:
#: Três vezes o piso do Tor — uns 40 ms de ida e volta em condições
#: favoráveis. Abaixo disso, o jitter normal do Tor seria reportado
#: como perda de sincronização, e o receptor deitaria fora quadros que
#: ainda chegavam a tempo.
#:
#: Acima disso, «sincronia» passa a ser «sincronia com dois segundos
#: de atraso», que é o que a pessoa ouve como eco.
LIMITE_MS = 120


class Decisao:
    """O que fazer com um quadro de vídeo, dado o desvio.

    ## E porque são três e não um booleano

    Porque «descartar» e «apresentar já» são **decisões opostas** que
    um único booleano não distingue. E as duas são reais: um quadro
    atrasado descarta-se, e um quadro adiantado apresenta-se já porque
    esperar por ele só gastaria tempo.

    E porque o valor é uma constante e não um enum: o módulo é
    pequeno e o valor compara-se com ``==`` em três sítios. Um enum
    daria ``Decisao.DESCARTAR`` — que é o mesmo número com mais
    caracteres.
    """

    #: O desvio está dentro do limite: o quadro apresenta-se com o
    #: áudio que o acompanha.
    SINCRONIZAR = 0

    #: O vídeo está **adiantado** em relação ao áudio: o quadro é de
    #: uma imagem que ainda não soou, e apresenta-se já.
    APRESENTAR_JA = 1

    #: O vídeo está **atrasado** em relação ao áudio: o quadro é de uma
    #: imagem que já passou, e descarta-se.
    DESCARTAR = 2

    #: Os nomes, para o diagnóstico e para os logs.
    NOMES = {
        SINCRONIZAR: "sincronizar",
        APRESENTAR_JA: "apresentar já",
        DESCARTAR: "descartar",
    }


@dataclass
class SessaoLipSync:
    """A sessão de sincronia de uma chamada.

    ## O que guarda

    O `clock_ms` do **último** áudio e do **último** vídeo. E o
    `clock_ms` do vídeo **que foi apresentado**, que é o que permite
    saber se um quadro se segue ao anterior ou se há um buraco.

    ## E porque o áudio é o mestre

    Porque §5.2: o ritmo do áudio é constante por construção, e o do
    vídeo não. Um quadro que chegou fora de ordem é um quadro de uma
    imagem passada, e o áudio é a única coisa que diz que tempo já
    passou.

    ## E o que a sessão **não** faz

    Não dorme, não bloqueia, e não guarda áudio. Ela **decide**; quem
    apresenta é o anel e quem guarda o áudio é o receptor de media.
    """

    #: O `clock_ms` do último áudio recebido, ou `None`.
    audio_ms: int | None = None

    #: O `clock_ms` do último vídeo recebido, ou `None`.
    video_ms: int | None = None

    #: O `clock_ms` do último vídeo **apresentado**, ou `None`.
    video_apresentado_ms: int | None = None

    #: Quantos quadros foram descartados.
    descartados: int = 0

    #: Quantos quadros foram apresentados.
    apresentados: int = 0

    def registar_audio(self, clock_ms: int) -> None:
        """Regista o tempo de um pacote de áudio.

        E o mestre. O valor é o `clock_ms` que veio no cabeçalho do
        pacote — o **contado** pelo emissor, e não o contado à receção,
        que seria o tempo de chegada e não o de captura.
        """
        self.audio_ms = clock_ms

    def registar_video(self, clock_ms: int) -> None:
        """Regista o tempo de um quadro de vídeo."""
        self.video_ms = clock_ms

    def desvio_ms(self) -> int | None:
        """O desvio entre o vídeo e o áudio, em milissegundos.

        E `video_ms − audio_ms`, que é a fórmula da §5.5. E `None` quando
        ainda não há os dois — que é o estado normal nos primeiros
        instantes de uma chamada, e que a §5.5 resolve com
        «apresentar já».
        """
        if self.audio_ms is None or self.video_ms is None:
            return None
        return self.video_ms - self.audio_ms

    def decidir(self, clock_ms: int) -> int:
        """Decide o que fazer com um quadro, e actualiza os contadores.

        :param clock_ms: o tempo do quadro, do cabeçalho que veio com
            ele.
        :returns: um valor de :class:`Decisao`.

        ## E a regra, e os três casos

        ```text
        atraso = video_ms − audio_ms
        atraso >  +LIMITE  →  descartar
        |atraso| ≤ LIMITE  →  sincronizar
        atraso <  −LIMITE  →  apresentar já
        ```

        ## E o caso do quadro **adiantado**
        #
        # `atraso < −LIMITE` é o vídeo à **frente** do áudio: o quadro é
        # de uma imagem que ainda não soou. Descartá-lo seria perder
        # a imagem; apresentá-lo já é o certo, porque o áudio que
        # falta vem a seguir.
        #
        # E é o caso que o `sleep` resolveria mal: esperar pelo áudio
        # significava acumular atraso de vídeo, e o atraso acumula.
        """
        self.registar_video(clock_ms)

        desvio = self.desvio_ms()
        if desvio is None:
            # Ainda não há áudio: o quadro é o primeiro, e apresentá-lo
            # é o que a §5.5 manda.
            self.apresentados += 1
            self.video_apresentado_ms = clock_ms
            return Decisao.APRESENTAR_JA

        if desvio > LIMITE_MS:
            # O quadro é de uma imagem que já passou. Descartar é o
            # que a §5.5 manda, e a perda conta.
            self.descartados += 1
            return Decisao.DESCARTAR

        if desvio < -LIMITE_MS:
            # O quadro está adiantado: apresentá-lo já.
            self.apresentados += 1
            self.video_apresentado_ms = clock_ms
            return Decisao.APRESENTAR_JA

        self.apresentados += 1
        self.video_apresentado_ms = clock_ms
        return Decisao.SINCRONIZAR

    def desvio_atual_ms(self) -> int | None:
        """O desvio neste momento, ou `None` se ainda não há os dois.

        E o que a interface mostra, e é a mesma conta de
        [`desvio_ms`](SessaoLipSync.desvio_ms) — mas sem o registo, para
        que quem pergunta não mude o estado.
        """
        return self.desvio_ms()

    def estatisticas(self) -> dict[str, int]:
        """Os contadores, para o diagnóstico.

        E um `dict` e não campos soltos porque quem mostra isto é a
        interface, e a interface quer um objecto que possa mostrar
        como quiser.
        """
        return {
            "apresentados": self.apresentados,
            "descartados": self.descartados,
        }
