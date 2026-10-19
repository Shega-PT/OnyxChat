// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

/**
 * `Chamada` — a chamada, de ponta a ponta, do botão ao som.
 *
 * ## E o que mudou, e porquê
 *
 * A versão anterior deste componente dependia de três APIs de browser
 * que o renderizador **não garante**:
 *
 * | API | Onde | O que acontece sem ela |
 * |---|---|---|
 * | `VideoDecoder` | `VideoChamada.js` | a imagem não aparece — e o erro é um ecrã preto |
 * | `AudioDecoder` | `AudioChamada.js` | o som não aparece — e o erro é silêncio |
 * | `AudioContext` | `AudioChamada.js` | o áudio fica `suspended` — e o erro é silêncio |
 *
 * O renderizador é um **WebKitGTK 6** (`scripts/interface-visual.js`),
 * e o WebKitGTK não garante nenhuma delas. A primeira versão do
 * `VideoChamada.js` dizia-o no seu próprio cabeçalho: «um browser sem
 * `VideoDecoder` não tem vídeo».
 *
 * Por isso o áudio e o vídeo **mudaram de dono**:
 *
 * * o **vídeo** é descodificado no daemon, que tem o `ffmpeg` com o
 *   decoder de VP8, e chega à interface como **JPEG** — que qualquer
 *   motor de renderização mostra num `<img>` sem descodificar nada;
 * * o **áudio** é descodificado e reproduzido no daemon, que o escreve
 *   no dispositivo de som. A interface nunca vê o áudio.
 *
 * E o que a interface faz, então, é mostrar imagens e dizer ao daemon
 * o que tocar. O `AudioContext` desapareceu, e com ele a restrição de
 * «só dentro de um gesto» — que era a fonte da metade dos problemas
 * desta componente.
 *
 * ## Os estados
 *
 * São **cinco**, e não três, porque uma chamada tem mais momentos do
 * que «ligada» e «parada»:
 *
 * ```text
 * parada → a-tocar → a-esperar → ligada → terminada
 * ```
 *
 * E cada um existe porque sem ele a pessoa não sabe o que está a
 * acontecer. O `a-esperar` — a pessoa recebeu um convite e ainda não
 * respondeu — é o que impede que alguém fique a falar para o vazio
 * sem perceber que o outro lado não ouviu.
 *
 * ## E o lip sync
 *
 * O `clock_ms` que viaja com cada quadro e com cada pacote de áudio é
 * o que liga os dois. E a decisão — apresentar, apresentar já, ou
 * descartar — é a regra dos três cenários de `docs/media.md` §5.5.
 *
 * A regra está no **daemon**, e não aqui: o daemon tem os dois
 * relógios, e a interface tem um só e não sabe a que momento pertence.
 *
 * ## E o que este componente **não** faz
 *
 * Não escolhe quem chama — isso é do `TopBar`, que sabe a quem é a
 * conversa. Não sabe nada de envelopes, de chaves ou de Tor. E não
 * decide o que é um quadro válido: isso é do `ffmpeg`, que o diz.
 */

import { useCallback, useEffect, useRef, useState } from 'react'


/**
 * O que correu mal com o media.
 *
 * E uma classe própria, e não um `Error` nu, porque a interface mostra
 * a mensagem e o `name` é o que a distingue de um erro de rede — que é
 * uma coisa com outra leitura.
 *
 * ## E porque vive aqui
 *
 * Porque o `VideoChamada.js` — onde vivia — deixou de existir: a
 * descodificação passou para o daemon, e o que a interface faz do vídeo
 * é mostrar um JPEG. A classe ficou, e mudar de sítio é o que evita
 * deixar um ficheiro morto que ainda é importado.
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
 * A geometria, e tem de bater com `EmissorStreaming` em Python.
 *
 * 480p a 30 fps a 300 kbit/s: §3.4.2 mediu 6,3× de folga no orçamento
 * de mensagens.
 */
const LARGURA = 854
const ALTURA = 480
const FPS = 30
const AUDIO_KBPS = 32
const VIDEO_KBPS = 300

/**
 * Os estados de uma chamada.
 *
 * E um `Object.freeze`, porque um estado que muda de valor é um estado
 * que se compara por identidade noutro sítio e dá sempre diferente.
 */
export const ESTADO = Object.freeze({
  /** Não há chamada. */
  PARADA: 'parada',
  /** Tocou, e o outro lado ainda não respondeu. */
  A_TOCAR: 'a-tocar',
  /** O outro lado tocou, e esta pessoa ainda não respondeu. */
  A_ESPERAR: 'a-esperar',
  /** A chamada está a decorrer. */
  LIGADA: 'ligada',
  /** A chamada acabou. */
  TERMINADA: 'terminada',
})

/**
 * O que a interface mostra, por estado.
 *
 * E um mapa e não um `switch` dentro do componente, porque o mesmo
 * estado aparece em mais do que um sítio — o ecrã e o botão — e dois
 * `switch` divergem.
 */
export const TEXTO_ESTADO = Object.freeze({
  [ESTADO.PARADA]: '',
  [ESTADO.A_TOCAR]: 'A tocar…',
  [ESTADO.A_ESPERAR]: 'Chamada a entrar…',
  [ESTADO.LIGADA]: 'A chamar',
  [ESTADO.TERMINADA]: 'Chamada terminada',
})

/**
 * Converte base-64 em bytes.
 *
 * E o `atob` e não o `fetch('data:...')` que a versão anterior usava.
 * O `fetch` de um `data:` URL é uma **operação assíncrona** que
 * bloqueia o laço de leitura por um instante de cada vez — e a 30
 * quadros por segundo isso são 30 paragens por segundo num caminho que
 * devia ser só uma leitura.
 *
 * @param {string} base64
 * @returns {Uint8Array}
 */
function deBase64(base64) {
  const binario = atob(base64)
  const bytes = new Uint8Array(binario.length)
  for (let i = 0; i < binario.length; i += 1) {
    bytes[i] = binario.charCodeAt(i)
  }
  return bytes
}

/**
 * Converte bytes num `blob:` URL, para um `<img>`.
 *
 * E um `blob:` e não um `data:` URL porque o `data:` embute a imagem
 * inteira no atributo `src` — e a 30 quadros por segundo, a 30 KB por
 * quadro, são 900 KB de base-64 por segundo a entrar no DOM e a sair
 * dele.
 *
 * @param {Uint8Array} bytes
 * @returns {string}
 */
function paraBlobUrl(bytes) {
  return URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }))
}

/**
 * A chamada: o que a interface faz, e o que pede ao daemon.
 *
 * @param {object} props
 * @param {object} props.adaptador O adaptador (`bridge-adapter.js`).
 * @param {(estado: string) => void} [props.aoMudarEstado] Chamado com o
 *   estado da chamada.
 * @param {(quadros: number) => void} [props.aoReceberVideo] Quantos
 *   quadros a UI já viu.
 * @param {(imagem: string | null) => void} [props.aoReceberImagem] A
 *   imagem mais recente, como um `blob:` URL. O componente **não**
 *   mostra a imagem: quem mostra é quem tem o `<img>`, e passar a URL
 *   é o que deixa a decisão de a mostrar a quem tem o contexto.
 * @param {(sessao: string | null) => void} [props.aoMudarSessao] A
 *   sessão da chamada, que é o que amarra o convite ao media.
 * @param {(accoes: object) => void} [props.aoMudarAccoes] Chamado com
 *   as acções da chamada — `ligar`, `atender`, `recusar`,
 *   `entrouChamada` e `terminar`. É por aqui que quem tem os botões
 *   chega às acções.
 * @returns {JSX.Element} O elemento.
 */
export default function Chamada({
  adaptador,
  aoMudarEstado,
  aoReceberVideo,
  aoReceberImagem,
  aoMudarSessao,
  aoMudarAccoes,
}) {
  const [estado, definirEstado] = useState(ESTADO.PARADA)
  const [erro, definirErro] = useState(null)
  const [quadros, definirQuadros] = useState(0)
  const [imagem, definirImagem] = useState(null)

  // Um `ref` e não estado: o `AbortController` tem de sobreviver a
  // re-renders, e um `useState` recriava o pedido a cada render — o que
  // com 30 fps por segundo são 30 leituras de quadros a mais por
  // segundo.
  const pararRef = useRef(null)
  // E a `URL` da imagem, para a revogar. Uma `blob:` URL que não é
  // revogada fica na memória do renderer até ao fim da sessão, e a 30
  // por segundo são 30 imagens por segundo a acumular.
  const urlImagemRef = useRef(null)

  const parar = useCallback(() => {
    if (pararRef.current) {
      pararRef.current.abort()
      pararRef.current = null
    }
    if (urlImagemRef.current) {
      URL.revokeObjectURL(urlImagemRef.current)
      urlImagemRef.current = null
    }
    definirImagem(null)
  }, [])

  useEffect(() => {
    if (aoMudarEstado) {
      aoMudarEstado(estado)
    }
    if (aoReceberVideo && estado === ESTADO.LIGADA) {
      aoReceberVideo(quadros)
    }
    if (aoReceberImagem) {
      aoReceberImagem(imagem)
    }
  }, [estado, quadros, imagem, aoMudarEstado, aoReceberVideo, aoReceberImagem])

  const terminar = useCallback(
    async (adaptadorActual) => {
      parar()
      try {
        await adaptadorActual.stopVideoStream()
      } catch {
        // A chamada já acabou; uma falha a fechá-la é ruído.
      }
      definirEstado(ESTADO.TERMINADA)
    },
    [parar],
  )

  /**
   * Liga a captura local.
   *
   * @param {object} adaptadorActual
   * @returns {Promise<boolean>} Se a captura arrancou.
   */
  const ligarCaptura = useCallback(async (adaptadorActual) => {
    try {
      await adaptadorActual.startVideoStream(
        LARGURA,
        ALTURA,
        FPS,
        AUDIO_KBPS,
        VIDEO_KBPS,
      )
      return true
    } catch (e) {
      definirErro(new ErroVideo(`a captura não arrancou: ${e.message}`))
      return false
    }
  }, [])

  /**
   * Liga a chamada. **Chamar dentro de um gesto do utilizador.**
   *
   * @returns {Promise<void>} Resolve quando a chamada está ligada, ou
   *   quando falhou — a falha vai para o `erro` e não é atirada.
   */
  const ligar = useCallback(async () => {
    if (estado !== ESTADO.PARADA && estado !== ESTADO.TERMINADA) {
      return
    }
    definirEstado(ESTADO.A_TOCAR)
    definirErro(null)

    if (!(await ligarCaptura(adaptador))) {
      definirEstado(ESTADO.PARADA)
      return
    }

    const controlador = new AbortController()
    pararRef.current = controlador
    let vivo = true

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
          // E a A descodificação acontece **no daemon**. O VP8 chega
          // cru e sai JPEG, e a interface escreve num `<img>` — que
          // qualquer motor de renderização mostra sem descodificar
          // nada.
          const imagemDecodificada = await adaptador.decodeVideoImage(
            quadro.vp8,
            LARGURA,
            ALTURA,
          )
          if (!vivo) {
            return
          }

          if (urlImagemRef.current) {
            URL.revokeObjectURL(urlImagemRef.current)
          }
          const url = paraBlobUrl(deBase64(imagemDecodificada.jpeg))
          urlImagemRef.current = url
          definirImagem(url)
          definirQuadros((n) => n + 1)
        } catch (e) {
          if (vivo) {
            // Uma imagem que não descodificou **não** é o fim da
            // chamada. É um quadro perdido — e perdê-lo é o que o
            // descarte do lip sync faz de propósito. O laço continua.
            definirErro(new ErroVideo(`um quadro não descodificou: ${e.message}`))
          }
        }
      }

      if (vivo) {
        await terminar(adaptador)
      }
    }

    correr()
  }, [estado, adaptador, ligarCaptura, terminar])

  /**
   * Atende uma chamada que está a entrar.
   *
   * @param {string} sessao A sessão do convite.
   * @returns {Promise<void>}
   */
  const atender = useCallback(
    async (sessao) => {
      if (estado !== ESTADO.A_ESPERAR) {
        return
      }
      definirEstado(ESTADO.LIGADA)
      if (aoMudarSessao) {
        aoMudarSessao(sessao)
      }
    },
    [estado, aoMudarSessao],
  )

  /**
   * Recusa uma chamada que está a entrar.
   *
   * @returns {Promise<void>}
   */
  const recusar = useCallback(async () => {
    definirEstado(ESTADO.TERMINADA)
    parar()
  }, [parar])

  /**
   * Marca que uma chamada está a entrar.
   *
   * @param {string} sessao A sessão do convite.
   */
  const entrouChamada = useCallback(
    (sessao) => {
      if (estado !== ESTADO.PARADA) {
        return
      }
      definirEstado(ESTADO.A_ESPERAR)
      if (aoMudarSessao) {
        aoMudarSessao(sessao)
      }
    },
    [estado, aoMudarSessao],
  )

  // A limpeza: o `AbortController` e a `blob:` URL.
  useEffect(() => () => parar(), [parar])

  // As acções da chamada, para quem tem o contexto.
  //
  // E um callback e não o retorno do componente, porque o `Chamada` é
  // usado dentro de um `ChamadaView` que **é** quem tem os botões — e
  // um `ref` seria a única forma de o pai chamar estas funções, e um
  // `ref` para um `useCallback` é a forma mais chata de o resolver.
  //
  // E o `aoMudarAccoes` é chamado **sempre** que as acções mudam, e
  // não só no primeiro render: o `ligar` depende do `estado`, e sem
  // isto o pai ficaria com a função do primeiro render — a que não
  // ligava porque o estado era `parada` mas o closure era de outro
  // momento.
  useEffect(() => {
    if (aoMudarAccoes) {
      aoMudarAccoes({ ligar, atender, recusar, entrouChamada, terminar })
    }
  }, [ligar, atender, recusar, entrouChamada, terminar, aoMudarAccoes])

  return (
    <div
      className="onyx-chamada"
      data-estado={estado}
      data-quadros={quadros}
      data-tem-erro={erro ? 'sim' : 'nao'}
    >
      {erro ? <p className="onyx-chamada-erro">{erro.message}</p> : null}
      {aoMudarEstado ? null : <span>{TEXTO_ESTADO[estado]}</span>}
    </div>
  )
}
