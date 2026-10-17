// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

/**
 * `Chamada` — a chamada, de ponta a ponta, do botão ao som.
 *
 * ## Um só componente, e porquê
 *
 * Porque uma chamada tem **um** relógio. O vídeo e o áudio têm relógios
 * independentes no browser — o `VideoDecoder` conta em quadros e o
 * `AudioContext` em amostras de cristal — e o que os liga é o
 * `clock_ms` que o daemon carimba em cada quadro e o limitador da §8.0
 * que impede o áudio de passar à frente.
 *
 * Com dois componentes, cada um com o seu `useEffect`, os dois
 * conversos nascem em momentos diferentes e o áudio pode ficar meio
 * segundo à frente do vídeo sem que nada o diga. Com um, o relógio é
 * um.
 *
 * ## O gesto, e porque não há atalho para ele
 *
 * ## Porque o botão é preciso e não um `useEffect`
 *
 * O browser não deixa criar um `AudioContext` fora de um gesto do
 * utilizador. Sem gesto, o contexto nasce `suspended`, o `decode()`
 * corre e não produz som, **sem erro nenhum** — e o sintoma é uma
 * chamada em que se vê a imagem e não se ouve a voz, que é a falha mais
 * confusa que uma chamada pode ter.
 *
 * Não há `resume()` que resolva: `resume()` também precisa de um gesto.
 * E não há polyfill. A decisão é do browser e é a única forma de
 * respectá-la sem mentir ao utilizador sobre o que está a acontecer.
 *
 * ## O atraso, honestamente
 *
 * ```text
 *   1 frame de captura        33 ms
 * + fila do VideoDecoder     ~33 ms
 * + AudioContext             10-20 ms   §10.3.1
 * + RTT do Tor             100-500 ms   §3.5
 * ```
 *
 * O `AudioContext` e o `VideoDecoder` são os dois que se podem
 * escolher. O RTT não: §3.5 diz que é o piso e que nenhum número
 * daqui o reduz.
 *
 * ## O que este componente **não** faz
 *
 * Não escolhe quem chama, não descodifica o Opus (isso é o
 * `AudioChamada`), não desenha o quadro (isso é o `VideoChamada`), e
 * não sabe nada de envelopes. É a cola entre três coisas que já sabem
 * o seu trabalho.
 */

import { useCallback, useEffect, useRef, useState } from 'react'

import { ErroVideo, useVideoChamada } from './VideoChamada.js'
import { useAudioChamada } from './AudioChamada.js'

/**
 * A geometria de omissão, e tem de bater com `EmissorStreaming` em Python.
 *
 * 480p a 30 fps a 300 kbit/s: §3.4.2 mediu 6,3× de folga no orçamento
 * de 12,8 msg/s.
 */
const LARGURA = 854
const ALTURA = 480
const FPS = 30
const AUDIO_KBPS = 32
const VIDEO_KBPS = 300

/**
 * Os estados de uma chamada.
 *
 * São três e não dois porque «a ligar» e «ligada» são coisas
 * diferentes para quem está do outro lado: durante o `a ligar` o
 * `ffmpeg` ainda está a arrancar e ainda não há som, e dizer «ligada»
 * antes disso é uma mentira que a pessoa descobre 200 ms depois.
 */
const ESTADO = {
  PARADA: 'parada',
  A_LIGAR: 'a-ligar',
  LIGADA: 'ligada',
}

/**
 * A chamada: vídeo e áudio, com um relógio.
 *
 * @param {object} props
 * @param {object} props.adaptador O adaptador (`bridge-adapter.js`).
 * @param {(estado: string) => void} [props.aoMudarEstado] Chamado com o
 *   estado: `parada`, `a-ligar` ou `ligada`.
 * @param {(quadros: number) => void} [props.aoReceberVideo] Quantos
 *   quadros a UI já viu. Serve para mostrar que a chamada está viva,
 *   que num vídeo parado também é verdade.
 * @returns {JSX.Element} O elemento.
 */
export default function Chamada({ adaptador, aoMudarEstado, aoReceberVideo }) {
  const { canvasRef, erro: erroVideo, descodificar: descodificarVideo } =
    useVideoChamada(LARGURA, ALTURA)
  const { descodificar: descodificarAudio, acordar, estado: estadoAudio, erro: erroAudio } =
    useAudioChamada()

  const [estado, definirEstado] = useState(ESTADO.PARADA)
  const [erro, definirErro] = useState(null)
  const [quadros, definirQuadros] = useState(0)

  // Um `ref` e não estado: o `AbortController` tem de sobreviver a
  // re-renders, e um `useState` recriava o pedido a cada render — o que
  // com 30 fps por segundo é 30 leituras de quadros a mais por segundo.
  const pararRef = useRef(null)

  const parar = useCallback(() => {
    if (pararRef.current) {
      pararRef.current.abort()
      pararRef.current = null
    }
  }, [])

  useEffect(() => {
    if (aoMudarEstado) {
      aoMudarEstado(estado)
    }
    if (aoReceberVideo && estado === ESTADO.LIGADA) {
      aoReceberVideo(quadros)
    }
  }, [estado, quadros, aoMudarEstado, aoReceberVideo])

  const terminar = useCallback(
    async (adaptadorActual) => {
      parar()
      try {
        await adaptadorActual.stopVideoStream()
      } catch {
        // A chamada já acabou; uma falha a fechá-la é ruído.
      }
    },
    [parar],
  )

  /**
   * Liga a chamada. **Chamar dentro de um gesto do utilizador.**
   *
   * @returns {Promise<void>} Resolve quando a chamada está ligada, ou
   *   quando falhou — a falha vai para o `erro` e não é atirada.
   */
  const ligar = useCallback(async () => {
    if (estado !== ESTADO.PARADA) {
      return
    }
    definirEstado(ESTADO.A_LIGAR)
    definirErro(null)

    // O áudio primeiro, e **dentro do gesto**: é o único momento em
    // que o browser deixa criar um `AudioContext`. Se este passo
    // falhar, a captura nem é ligada — um `ffmpeg` a ler o ecrã sem
    // ninguém a ouvir é pior do que nenhum.
    const acordou = await acordar()
    if (!acordou) {
      definirErro(
        new ErroVideo(
          'o browser não deixou ligar o áudio: uma chamada só começa ' +
            'a partir de um clique',
        ),
      )
      definirEstado(ESTADO.PARADA)
      return
    }

    const controlador = new AbortController()
    pararRef.current = controlador
    let vivo = true

    try {
      await adaptador.startVideoStream(
        LARGURA,
        ALTURA,
        FPS,
        AUDIO_KBPS,
        VIDEO_KBPS,
      )
    } catch (e) {
      definirErro(new ErroVideo(`a captura não arrancou: ${e.message}`))
      definirEstado(ESTADO.PARADA)
      return
    }

    definirEstado(ESTADO.LIGADA)

    // A leitura é um laço, e o laço pára por três motivos: a chamada
    // acaba, o componente morre, ou quem liga desliga. Não há
    // temporizador — `readVideoFrame` bloqueia até haver quadro, e é
    // esse bloqueio que põe o ritmo no `ffmpeg` e não numa
    // temporização que ninguém calibrou.
    const correr = async () => {
      while (vivo && !controlador.signal.aborted) {
        let quadro
        try {
          quadro = await adaptador.readVideoFrame()
        } catch (e) {
          if (vivo) {
            definirErro(new ErroVideo(`a leitura falhou: ${e.message}`))
          }
          return
        }

        // `null` é o fim: o `ffmpeg` morreu. Não é um erro, e pintar o
        // ecrã de vermelho no momento em que alguém pousa o telefone
        // seria a pior das respostas.
        if (!quadro) {
          break
        }

        try {
          const vp8 = await deBase64(quadro.vp8)
          await descodificarVideo(vp8, quadro.clockMs)
          if (vivo) {
            definirQuadros((n) => n + 1)
          }
        } catch (e) {
          if (vivo) {
            definirErro(e)
          }
          return
        }
      }

      if (vivo) {
        await terminar(adaptador)
        definirEstado(ESTADO.PARADA)
      }
    }

    const correrAudio = async () => {
      while (vivo && !controlador.signal.aborted) {
        let pacote
        try {
          pacote = await adaptador.readAudioFrame()
        } catch (e) {
          if (vivo) {
            definirErro(new ErroVideo(`a leitura do áudio falhou: ${e.message}`))
          }
          return
        }
        if (!pacote) {
          return
        }
        try {
          await descodificarAudio(pacote)
        } catch (e) {
          if (vivo) {
            definirErro(e)
          }
          return
        }
      }
    }

    // Dois laços e não um: o `readVideoFrame` bloqueia até haver
    // quadro e o `readAudioFrame` até haver pacote. Um laço que
    // esperasse pelos dois nunca sairia de espera — e os dois
    // O bloqueio é o que mantém cada um ao seu ritmo.
    correrAudio()
    correr()
  }, [adaptador, acordar, descodificarAudio, descodificarVideo, estado, terminar])

  const desligar = useCallback(async () => {
    await terminar(adaptador)
    definirEstado(ESTADO.PARADA)
  }, [adaptador, terminar])

  const erroFinal = erro ?? erroVideo ?? erroAudio
  if (erroFinal) {
    return (
      <div role="alert" className="onyx-chamada--erro">
        <p>{erroFinal.message}</p>
        <button type="button" onClick={() => definirErro(null)}>
          Fechar
        </button>
      </div>
    )
  }

  if (estado === ESTADO.PARADA) {
    return (
      <div className="onyx-chamada--parada">
        {/* `onClick` e não `onPointerDown`: o browser conta um clique
            como gesto de utilizador e nem todos os eventos de ponteiro
            contam — um `touchstart` num browser sem `{passive: false}`
            pode ser comido. */}
        <button type="button" onClick={ligar}>
          Ligar chamada
        </button>
      </div>
    )
  }

  return (
    <div className="onyx-chamada">
      <canvas
        ref={canvasRef}
        width={LARGURA}
        height={ALTURA}
        aria-label="Vídeo da chamada"
      />
      <button type="button" onClick={desligar}>
        Desligar
      </button>
      {estadoAudio.naFila > 0 ? (
        <span className="onyx-chamada--fila">
          áudio: {estadoAudio.naFila} em espera
        </span>
      ) : null}
    </div>
  )
}

/**
 * Converte base-64 em `Uint8Array`.
 *
 * **Não** usa `atob`: `atob` dá uma `string`, e passar os bytes por
 * uma string e voltar a convertê-los custa o dobro e passa por um
 * caminho onde um byte acima de 0x7F já mudou. `fetch` com um `data:`
 * URL faz a descodificação em C++ do browser.
 *
 * @param {string} base64 O texto em base-64.
 * @returns {Promise<Uint8Array>} Os bytes.
 */
async function deBase64(base64) {
  const resposta = await fetch(`data:application/octet-stream;base64,${base64}`)
  return new Uint8Array(await resposta.arrayBuffer())
}

/**
 * Exporta o áudio a um descodificador, para uso fora do componente.
 *
 * Existe para o caso de o áudio vir por um caminho que não é o
 * `readVideoFrame` — um `MediaStream` do browser, ou um teste. Sem
 * isto, quem quiser dar áudio a este componente tem de saber os
 * detalhes do `AudioContext`.
 *
 * @param {object} descodificador O que `useAudioChamada` devolve.
 * @param {Uint8Array} dados O pacote de Opus.
 * @param {number} [taxa] A taxa, se souber.
 * @returns {Promise<void>} Resolve quando foi entregue.
 */
export async function darAudio(descodificador, dados, taxa = 0) {
  await descodificador.descodificar(dados, taxa)
}