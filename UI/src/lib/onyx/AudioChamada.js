// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

/**
 * `AudioChamada` — descodificar Opus e tirá-lo para uma saída de áudio.
 *
 * ## Por que o áudio não é o vídeo com outro nome
 *
 * Porque o sintoma de um atraso é **invertido**. Um quadro de vídeo
 * atrasado mostra-se com dois quadros detras — e é quase imperceptível
 * a 30 fps. Um pacote de áudio atrasado é voz que chega tarde, e quem
 * está a ouvir percebe antes de ver.
 *
 * Por isso aqui não há fila: o áudio **toca quando chega**, e o que se
 * descarta é o pacote mais antigo. Descartar áudio **ouve-se** — é um
 * estalo — e descarta-se o mais antigo precisamente porque descartar o
 * novo tira a voz a quem está a falar *agora*.
 *
 * ## O buffer, e porque é que existe
 *
 * O `AudioContext` do browser não toca o que se lhe dá no instante em
 * que chega: ele tem um relógio próprio, de cristal, e o que se
 * escreve antes da posição corrente toca com o atraso que houver. É o
 * `latencyHint`, e é por isso que o áudio de uma chamada tem
 * tipicamente 10 a 20 ms de atraso **que ninguém pode tirar**: é o
 * browser a poder começar a mixar sem estalar.
 *
 * E é esse atraso, e não o RTT, que decide se um pacote atrasado se
 * ouve. Daí a regra: o que está mais atrasado que `MAX_ATRASO_MS`
 * descarta-se.
 *
 * ## O que este módulo NÃO faz
 *
 * Não faz cancelamento de eco. Numa chamada entre duas máquinas há
 * eco da voz que acabou de falar, e o `echoCancellation` do
 * `getUserMedia` trata o do **microfone** — o da linha não. Resolver
 * isso é um filtro de eco por referência, e é trabalho que não cabe
 * aqui.
 */

import { useCallback, useEffect, useRef, useState } from 'react'

/**
 * Se o browser tem descodificação de áudio.
 *
 * Exposto como função, e não como constante de módulo, para que um
 * teste possa fingir a ausência sem monkey-patchear globals.
 *
 * @returns {boolean} `true` se há `AudioDecoder` e `AudioContext`.
 */
export function temAudioWebCodecs() {
  return (
    typeof globalThis.AudioDecoder === 'function' &&
    typeof globalThis.AudioContext === 'function'
  )
}

/**
 * Erro de áudio, com a razão que o browser deu.
 */
export class ErroAudio extends Error {
  /**
   * @param {string} mensagem O que correu mal, em português.
   * @param {string} [código] O `name` da excepção do browser.
   */
  constructor(mensagem, código = '') {
    super(mensagem)
    this.name = 'ErroAudio'
    this.código = código
  }
}

/**
 * Descodifica Opus e toca-o.
 */
export class DescodificadorOpus {
  /**
   * O atraso pedido ao `AudioContext`, em milissegundos.
   *
   * `playbackLatency` e não `interactive`: a diferença é que
   * `interactive` deixa o browser usar a melhor aproximação que
   * conseguir, e `playbackLatency` diz «usa pelo menos isto». Numa
   * chamada é o segundo que se quer — a qualidade de aproximação não
   * interessa e a latência interessa.
   */
  static ATRASO_ALVO = 60

  /**
   * Quantos pacotes podem estar à espera de tocar.
   *
   * Três, e não mais. Cada pacote de 20 ms dá 60 ms de fila — que é
   * exactamente o atraso pedido. Quatro seriam 80 ms, e um atraso que
   * cresce é pior do que um pacote perdido.
   */
  static MAXIMO_NA_FILA = 3

  /**
   * @param {(estado: object) => void} [aoMudarEstado] Chamado quando o
   *   número de pacotes na fila muda, para a UI mostrar que a chamada
   *   está viva.
   */
  constructor(aoMudarEstado) {
    if (!temAudioWebCodecs()) {
      throw new ErroAudio(
        'este browser não tem WebCodecs de áudio, e sem ele não há áudio',
      )
    }
    this._fila = []
    this._contexto = null
    this._descodificador = null
    this._proximoInicio = 0
    this._relogio = 0
    this._descartados = 0
    this._aoMudarEstado = aoMudarEstado
    this._erros = []
  }

  _arrancarContexto() {
    if (this._contexto) {
      return this._contexto
    }
    const Contexto = globalThis.AudioContext || globalThis.webkitAudioContext
    this._contexto = new Contexto({
      latencyHint: DescodificadorOpus.ATRASO_ALVO / 1000,
    })
    this._proximoInicio = this._contexto.currentTime + this._ATRASO_EM_SEGUNDOS()
    return this._contexto
  }

  _ATRASO_EM_SEGUNDOS() {
    return DescodificadorOpus.ATRASO_ALVO / 1000
  }

  _arrancarDescodificador(taxaAmostragem) {
    if (this._descodificador) {
      return this._descodificador
    }
    this._descodificador = new globalThis.AudioDecoder({
      // Opus chega com a taxa dentro de cada pacote, e o
      // `AudioDecoder` aceita-a no `configure` uma vez.
      output: (dados) => this._aoReceber(dados),
      error: (e) => {
        this._erros.push(e)
      },
    })
    this._descodificador.configure({
      codec: 'opus',
      sampleRate: taxaAmostragem,
      numberOfChannels: 1,
    })
    return this._descodificador
  }

  _aoReceber(dados) {
    const taxa = dados.sampleRate
    const canal = dados.getChannelData(0)
    // Copia antes de fechar: o `AudioData` é de quem o criou e
    // desvanece a seguir, e usar os `Float32Array` dele depois daria
    // silêncio sem nenhum erro.
    const copia = new Float32Array(canal)
    dados.close()

    this._fila.push({ samples: copia, taxa })

    // Descarte do **antigo**, nunca do recente: descartar o recente
    // tirava a voz a quem está a falar agora, e isso ouve-se.
    while (this._fila.length > DescodificadorOpus.MAXIMO_NA_FILA) {
      this._fila.shift()
      this._descartados += 1
    }

    this._tocar()
    this._avisar()
  }

  _tocar() {
    const contexto = this._contexto
    if (!contexto || this._fila.length === 0) {
      return
    }

    const agora = contexto.currentTime
    const pacote = this._fila[0]
    const buffer = contexto.createBuffer(1, pacote.samples.length, pacote.taxa)
    buffer.copyToChannel(pacote.samples, 0)

    // O pacote entra na fila **a partir de agora**, e nunca antes: um
    // `startTime` no passado toca imediatamente e uma rajada de pacotes
    // toca todos em cima uns dos outros — que é um estalo, não voz.
    const inicio = Math.max(agora, this._proximoInicio)
    const fonte = contexto.createBufferSource()
    fonte.buffer = buffer
    fonte.connect(contexto.destination)
    fonte.start(inicio)

    this._proximoInicio = inicio + buffer.duration
    this._fila.shift()
  }

  _avisar() {
    if (this._aoMudarEstado) {
      this._aoMudarEstado({
        naFila: this._fila.length,
        descartados: this._descartados,
      })
    }
  }

  /**
   * Descodifica um pacote de Opus.
   *
   * @param {Uint8Array} dados O pacote, cru.
   * @param {number} taxaAmostragem A taxa em Hz, se souber. `0` deixa o
   *   browser decidir a partir do próprio pacote.
   * @returns {Promise<void>} Resolve quando o pacote foi entregue ao
   *   `AudioDecoder`.
   */
  async descodificar(dados, taxaAmostragem = 0) {
    if (!dados || dados.byteLength === 0) {
      return
    }
    // O relógio do receptor. O áudio **é** o relógio (§5.3), e este é
    // o contador que diz quanto tempo passou desde o primeiro pacote.
    this._relogio += 20

    this._arrancarContexto()
    const taxa = taxaAmostragem > 0 ? taxaAmostragem : 48000
    const descodificador = this._arrancarDescodificador(taxa)

    // A fila do `AudioDecoder`, e não a do `AudioContext`: o browser
    // descodifica mais depressa do que a rede entrega, e uma fila sem
    // limite aqui é um `AudioData` por pacote à espera de ser fechado.
    if (descodificador.decodeQueueSize > 4) {
      await new Promise((resolve) => {
        descodificador.addEventListener('dequeue', resolve, { once: true })
      })
    }

    try {
      descodificador.decode(
        new globalThis.EncodedAudioChunk({
          type: 'key',
          timestamp: this._relogio * 1000,
          data: dados,
        }),
      )
    } catch (e) {
      throw new ErroAudio(`não consegui descodificar o áudio: ${e.message}`, e.name)
    }
  }

  /**
   * Quantos pacotes estão à espera de tocar.
   *
   * @returns {number} A profundidade da fila.
   */
  get fila() {
    return this._fila.length
  }

  /**
   * Quantos pacotes foram descartados por atraso.
   *
   * @returns {number} O total.
   */
  get descartados() {
    return this._descartados
  }

  /**
   * Fecha o descodificador e o contexto de áudio.
   *
   * @returns {Promise<void>} Resolve quando o browser confirma.
   */
  async fechar() {
    const descodificador = this._descodificador
    const contexto = this._contexto
    this._descodificador = null
    this._contexto = null
    this._fila = []

    if (descodificador) {
      try {
        await descodificador.flush()
      } catch {
        // Já morreu; o `close` resolve.
      }
      try {
        descodificador.close()
      } catch {
        // Já estava fechado.
      }
    }
    if (contexto && contexto.state !== 'closed') {
      try {
        await contexto.close()
      } catch {
        // Já estava fechado.
      }
    }
  }
}

/**
 * O gancho de React: um descodificador de áudio vivo enquanto o
 * componente estiver montado.
 *
 * ## Porque descodificar e tocar, e não só descodificar
 *
 * Porque o `AudioDecoder` sozinho não faz som: produz `AudioData`, e
 * é o `AudioContext` que o toca. São dois objectos com dois
 * ciclo de vida, e o `AudioContext` tem de ser criado **dentro** de um
 * gesto do utilizador — o browser recusa um `AudioContext` que não
 * teve um gesto, e recusa com um `NotAllowedError` que não diz o que
 * fazer.
 *
 * Por isso o gancho é criado no primeiro `descodificar` e não no
 * `useEffect`: o `AudioContext` nasce dentro de uma chamada que já vem
 * de um gesto.
 *
 * @returns {{descodificar: function, estado: object, erro: Error | null}}
 *   A função para entregar um pacote, o estado corrente, e o erro.
 */
export function useAudioChamada() {
  const [erro, definirErro] = useState(null)
  const [estado, definirEstado] = useState({ naFila: 0, descartados: 0 })
  const descodificadorRef = useRef(null)
  const contextoRef = useRef(null)

  const garantirContexto = useCallback(() => {
    if (contextoRef.current) {
      return contextoRef.current
    }
    const Contexto = globalThis.AudioContext || globalThis.webkitAudioContext
    const contexto = new Contexto({
      latencyHint: DescodificadorOpus.ATRASO_ALVO / 1000,
    })
    contextoRef.current = contexto
    descodificadorRef.current = new DescodificadorOpus(definirEstado)
    return contexto
  }, [])

  /**
   * Chamar **dentro de um gesto** do utilizador: um clique, um toque.
   *
   * É a única forma de o browser deixar criar um `AudioContext`, e não
   * é um detalhe de implementação: sem um gesto anterior, o contexto
   * nasce `suspended` e o `decode()` não produz som nenhum — sem erro,
   * sem aviso, e uma chamada muda que não se ouve.
   *
   * Se o contexto já estiver `running`, isto não faz nada. Se estiver
   * `suspended` — que acontece depois de o browser o suspender, e é o
   * que ele faz quando o separador deixa de estar à frente —,
   * `resume()` é o que o acorda, e também precisa de um gesto.
   */
  const acordar = useCallback(async () => {
    const contexto = garantirContexto()
    if (contexto.state === 'suspended') {
      await contexto.resume()
    }
    return contexto.state === 'running'
  }, [garantirContexto])

  const descodificar = useCallback(
    async (dados, taxaAmostragem = 0) => {
      try {
        if (!descodificadorRef.current) {
          definirErro(
            new ErroAudio(
              'o áudio não foi accordado: uma chamada só começa com um gesto',
            ),
          )
          return
        }
        await descodificadorRef.current.descodificar(dados, taxaAmostragem)
      } catch (e) {
        definirErro(e instanceof ErroAudio ? e : new ErroAudio(String(e)))
      }
    },
    [],
  )

  useEffect(() => {
    return () => {
      if (descodificadorRef.current) {
        descodificadorRef.current.fechar()
        descodificadorRef.current = null
      }
      contextoRef.current = null
    }
  }, [])

  return { descodificar, acordar, estado, erro }
}