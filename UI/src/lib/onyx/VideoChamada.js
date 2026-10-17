// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

/**
 * `VideoChamada` — descodificar VP8 com `WebCodecs` e mostrar o vídeo.
 *
 * ## Porque é que é aqui, e não no daemon
 *
 * O daemon embebido liga o `libvpx` para **codificar**, e ligar também
 * para descodificar duplicaria o executável por uma coisa que o emissor
 * nunca precisa: um emissor nunca descodifica. O daemon transporta
 * bytes de VP8 e pronto.
 *
 * Quem descodifica é quem mostra, e quem mostra é o browser. E o
 * browser tem `VideoDecoder` de série — sem subprocesso, sem contentor
 * intermédio e sem a latência de um lote.
 *
 * ## Porque é que o IVF não serve para isto
 *
 * O IVF é um contentor de **ficheiro**: declara quantos quadros tem, e
 * o `ffmpeg` que o lê não emite nada até saber que leu todos. Num pipe
 * nunca se sabe, e o resultado é um descodificador que espera para
 * sempre.
 *
 * Isto está medido, não suposto: o mesmo IVF dá 90 imagens quando se
 * lê o ficheiro todo e 0 bytes quando se escreve um quadro e se deixa o
 * pipe aberto. O `VideoDecoder` não tem o problema porque não precisa de
 * saber quantos quadros há — descodifica cada `EncodedVideoChunk` à
 * medida que chega, e quem sabe o fim da chamada é a aplicação.
 *
 * ## O VP8 não é auto-suficiente
 *
 * Frames-I são uma imagem completa; frames-P são um delta contra os
 * anteriores. Perder um frame-P é perder a imagem até ao próximo I, e
 * por isso o descodificador tem de ser **empurrado** e não solto:
 *
 *   - um `decode()` sem o `flush()` mantém o estado entre quadros;
 *   - `flush()` esvazia o que está em buffering e sinaliza fim de stream;
 *   - `close()` fecha o descodificador e liberta-o.
 *
 * Chamar `close()` a meio e depois `configure()` outra vez é o caminho
 * suportado para recuperar de um stream que recomeça.
 *
 * ## Quando não há `WebCodecs`
 *
 * Não há. Não há polyfill, não há `MediaSource` que ajude, e um
 * `<video>` com `src` de bytes VP8 não existe — o browser não aceita
 * VP8 cru. Um browser sem `VideoDecoder` não tem vídeo, e a UI diz isso
 * em vez de mostrar um ecrã preto.
 */

import { useCallback, useEffect, useRef, useState } from 'react'

/** A largura que o `ffmpeg` escreve no cabeçalho IVF por omissão. */
const LARGURA_OMISSAO = 854

/**
 * Se o browser tem `VideoDecoder`.
 *
 * Exposto como função e não como constante de módulo para que um teste
 * possa fingir a ausência sem ter de monkey-patchear globals.
 *
 * @returns {boolean} `true` se há descodificador de vídeo.
 */
export function temWebCodecs() {
  return typeof globalThis.VideoDecoder === 'function'
}

/**
 * Erro de descodificação, com a razão que o browser deu.
 *
 * `DOMException` do `WebCodecs` não sobrevive a um `JSON.stringify` e
 * a sua mensagem é técnica demais para mostrar a quem fala. Fica a
 * razão e o código, que é o que permite diagnosticar.
 */
export class ErroVideo extends Error {
  /**
   * @param {string} mensagem O que correu mal, em português.
   * @param {string} [código] O `name` da excepção do browser.
   */
  constructor(mensagem, código = '') {
    super(mensagem)
    this.name = 'ErroVideo'
    this.código = código
  }
}

/**
 * Um descodificador de VP8, com o estado que o browser mantém.
 *
 * Não é um hook: é um objecto imperativo porque o `VideoDecoder` é
 * imperativo e embrulhá-lo em `useState` faria cada `decode()` provocar
 * um render.
 */
export class DescodificadorVp8 {
  /**
   * @param {number} largura A largura, em pixels.
   * @param {number} altura A altura, em pixels.
   * @param {(quadro: VideoFrame) => void} aoReceber Chamado por quadro
   *   descodificado. O `VideoFrame` é **transferido**, não copiado: o
   *   browser passa a ser dono dele e quem recebeu tem de o fechar.
   */
  constructor(largura, altura, aoReceber) {
    if (!temWebCodecs()) {
      throw new ErroVideo(
        'este browser não tem WebCodecs, e sem ele não há vídeo: ' +
          'o VP8 chega comprimido e não há descodificador',
      )
    }
    if (!Number.isInteger(largura) || largura <= 0) {
      throw new ErroVideo(`largura inválida: ${largura}`)
    }
    if (!Number.isInteger(altura) || altura <= 0) {
      throw new ErroVideo(`altura inválida: ${altura}`)
    }

    this._largura = largura
    this._altura = altura
    this._aoReceber = aoReceber
    this._fechado = false
    /** Os erros que o descodificador deu, para diagnóstico. */
    this.erros = []

    // O `error` é assíncrono e **não** atira excepção: um codec que
    // falha a meio de um stream não pode derrubar a chamada de quem
    // está a ver. Fica guardado, e quem quiser saber lê `erros`.
    this._descodificador = new globalThis.VideoDecoder({
      output: (quadro) => this._aoReceber(quadro),
      error: (e) => {
        this.erros.push(e)
      },
    })
    this._configurar()
  }

  _configurar() {
    this._descodificador.configure({
      codec: 'vp8',
      codedWidth: this._largura,
      codedHeight: this._altura,
      // `optimizeForLatency` desliga o buffering de reordenação.
      //
      // E é **obrigatório** numa chamada: sem isto o browser pode
      // guardar dois ou três quadros à espera de um keyframe, e a
      // imagem que aparece tem dois ou três quadros de atraso. Numa
      // chamada, dois quadros de atraso é uma conversa em que as
      // pessoas se interrompem.
      optimizeForLatency: true,
    })
  }

  /**
   * Descodifica um quadro.
   *
   * @param {Uint8Array} dados Os bytes de VP8, **sem** o contentor IVF:
   *   o `VideoDecoder` quer o quadro, não o ficheiro.
   * @param {number} timestamp O tempo do quadro, em microssegundos.
   *   É o `clock_ms` do daemon (§5.3) vezes mil, e é o que o browser
   *   usa para ordenar e para o `VideoFrame.timestamp`.
   * @returns {Promise<void>} Resolve quando o quadro foi **entregue**
   *   ao `VideoDecoder`, que é diferente de ter sido **descodificado**:
   *   a descodificação é assíncrona e o resultado vem pelo `output`.
   */
  async descodificar(dados, timestamp) {
    if (this._fechado) {
      throw new ErroVideo('o descodificador está fechado')
    }
    if (!dados || dados.byteLength === 0) {
      throw new ErroVideo('quadro de VP8 sem bytes')
    }

    // A fila de `decode()` é ilimitada se o descodificador for mais
    // lento do que a chegada. Sem este limite, uma chamada lenta
    // acumula quadros até o browser matar o processo — e o sintoma é
    // «a imagem parou e depois reiniciou», que não parece falta de
    // memória.
    const MAXIMO_EM_ESPERA = 4
    if (this._descodificador.decodeQueueSize > MAXIMO_EM_ESPERA) {
      await new Promise((resolve) => {
        this._descodificador.addEventListener('dequeue', resolve, {
          once: true,
        })
      })
    }

    try {
      this._descodificador.decode(
        new globalThis.EncodedVideoChunk({
          type: 'key',
          timestamp: timestamp * 1000,
          data: dados,
        }),
      )
    } catch (e) {
      // `decode()` atira `TypeError` se o codec não estiver
      // configurado, e `InvalidStateError` se o stream foi fechado.
      // Os dois são erros de **estado** e não de conteúdo: um quadro
      // mau não os produz, e por isso a distinção importa — um
      // reinício de stream resolve os dois.
      throw new ErroVideo(`não consegui descodificar: ${e.message}`, e.name)
    }
  }

  /**
   * Fecha o descodificador e liberta-o.
   *
   * Idempotente. Um `close()` que levanta numa segunda chamada é pior
   * do que um que não levanta: quem fecha está a limpar, e um erro ao
   * limpar é ruído.
   *
   * @returns {Promise<void>} Resolve quando o browser confirma.
   */
  async fechar() {
    if (this._fechado) {
      return
    }
    this._fechado = true
    try {
      // `flush()` antes de `close()`: esvazia o que está em buffering
      // para que o último quadro chegue a ser desenhado. Sem isto a
      // chamada morre com o último meio segundo por ver.
      await this._descodificador.flush()
    } catch {
      // Um `flush()` que falha é um descodificador já morto, e o
      // `close()` abaixo resolve-o na mesma.
    }
    try {
      this._descodificador.close()
    } catch {
      // Já estava fechado. Idempotente, como se prometeu.
    }
  }

  /**
   * O número de quadros à espera de descodificação.
   *
   * @returns {number} A profundidade da fila.
   */
  get fila() {
    return this._descodificador ? this._descodificador.decodeQueueSize : 0
  }
}

/**
 * Descarte de um `VideoFrame` que já foi desenhado.
 *
 * O `VideoFrame` segura memória de GPU, e um `VideoFrame` por quadro a
 * 30 fps são 30 alocações de memória de vídeo por segundo. Um
 * `requestAnimationFrame` sem isto acaba por falhar a alocar, e o
 * sintoma é «o vídeo funciona durante um minuto e depois congela».
 *
 * @param {VideoFrame | null} quadro O quadro, se houver.
 */
export function soltarQuadro(quadro) {
  if (quadro && typeof quadro.close === 'function') {
    quadro.close()
  }
}

/**
 * Escreve um `VideoFrame` num `canvas` e solta-o.
 *
 * @param {HTMLCanvasElement} canvas O canvas onde desenhar.
 * @param {VideoFrame} quadro O quadro a desenhar.
 * @returns {boolean} `true` se desenhou, `false` se as dimensões não
 *   batem — o que acontece quando o emissor mudou a resolução a meio.
 */
export function desenhar(canvas, quadro) {
  const largura = canvas.width
  const altura = canvas.height
  if (quadro.displayWidth !== largura || quadro.displayHeight !== altura) {
    return false
  }
  const ctx = canvas.getContext('2d', { alpha: false })
  if (!ctx) {
    return false
  }
  ctx.drawImage(quadro, 0, 0, largura, altura)
  soltarQuadro(quadro)
  return true
}

/**
 * O gancho de React: um descodificador vivo enquanto o componente
 * estiver montado.
 *
 * ## Porque o descodificador não é estado
 *
 * Porque `VideoDecoder` é imperativo e cada `decode()` provoca um
 * `output`. Se o descodificador estivesse em `useState`, cada quadro
 * seria um render, e a 30 fps seriam 30 renders por segundo por causa
 * de uma imagem que o canvas desenha sozinho.
 *
 * A troca: o estado é **o erro**, não o descodificador. O componente
 * re-renderiza quando há um erro a mostrar, e não quando há um quadro.
 *
 * @returns {{canvasRef: object, erro: Error | null, descodificar: function, largura: number, altura: number}}
 *   O `ref` do canvas, o erro corrente (ou `null`), e a função para
 *   empurrar um quadro.
 */
export function useVideoChamada(largura = LARGURA_OMISSAO, altura = 480) {
  const canvasRef = useRef(null)
  const descodificadorRef = useRef(null)
  const [erro, definirErro] = useState(null)

  useEffect(() => {
    if (!temWebCodecs()) {
      definirErro(
        new ErroVideo(
          'este browser não tem WebCodecs e não descodifica VP8',
        ),
      )
      return undefined
    }

    let vivo = true
    const descodificador = new DescodificadorVp8(
      largura,
      altura,
      (quadro) => {
        const canvas = canvasRef.current
        if (!canvas || !vivo) {
          soltarQuadro(quadro)
          return
        }
        desenhar(canvas, quadro)
      },
    )
    descodificadorRef.current = descodificador

    return () => {
      vivo = false
      descodificador.fechar()
      descodificadorRef.current = null
    }
  }, [largura, altura])

  const descodificar = useCallback(async (dados, clockMs) => {
    const d = descodificadorRef.current
    if (!d) {
      throw new ErroVideo('o descodificador ainda não arrancou')
    }
    try {
      await d.descodificar(dados, clockMs)
    } catch (e) {
      definirErro(e instanceof ErroVideo ? e : new ErroVideo(String(e)))
    }
  }, [])

  return { canvasRef, erro, descodificar, largura, altura }
}