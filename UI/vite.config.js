// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import { fileURLToPath, URL } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// =====================================================================
// vite.config.js
// ---------------------------------------------------------------------
// O plugin de integração que vinha com o projecto original foi removido
// na Etapa 2. Fazia três coisas, e nenhuma sobrevive:
//
//   1. **Proxy de `/api`** para um backend hospedado. Não há
//      backend hospedado, e o serviço local — o sidecar da Etapa 5 —
//      escuta em `127.0.0.1` numa porta fixa e serve a sua própria origem,
//      o que dispensa proxy.
//   2. **Injecção de variáveis de ambiente lidas do servidor**. Não há
//      identificador de aplicação nem versão de funções.
//   3. **Agente de edição visual**, que ligava a página a um construtor
//      externo e emitia eventos de substituição de imagem. Em vez disso,
//      a interface passa a ser editada no código.
//
// O que fica é a configuração mínima do Vite com o plugin do React.
//
// ## Divisão do código de produção
//
// Ainda não há divisão. O pacote tem ~650 kB numa só peça, e o Vite avisa
// disso — com razão. A divisão por vista entra na Etapa 6, com a casca de
// desktop: `manualChunks` agruparia a casca e o fornecedor separadamente
// de cada rota, e só faz sentido decidir isso quando souber que há uma
// janela de aplicação e não um servidor.
//
// Repartido já agora por raio de `dias` seria arbitrar o tamanho de
// pedidos sem saber o padrão de acesso — e com um único utilizador, o
// padrão de acesso é a aplicação inteira.
// =====================================================================
// ## O alias `@/`
//
// Este alias **não é nosso historicamente**: vinha com o plugin, que o
// declarava em nome de quem o usasse. Removido o plugin, o build
// passou a falhar com `Rolldown failed to resolve import "@/App.jsx"`.
//
// Declarar aqui é a correção certa, e não um remendo. Um alias que vem de
// um pacote de terceiros é um alias que desaparece quando esse pacote
// muda de versão ou sai, e o erro que produz não menciona o pacote —
// menciona o teu próprio ficheiro.
//
// O mesmo mapeamento já está escrito em `jsconfig.json` (`paths`) e em
// `components.json` (`aliases`), para o editor e para o shadcn. São três
// declarações do mesmo facto, em três ferramentas diferentes, e é
// inevitável: nenhuma delas lê as outras duas.
// =====================================================================
// O aviso de licença no pacote construído
// ---------------------------------------------------------------------
// `dist/` é o que sai da máquina: é o que o sidecar serve em
// `127.0.0.1`, e é o que alguém copia para um disco externo. A PolyForm
// Noncommercial §Notices exige que quem recebe «a copy of any part»
// leve os termos **e** as linhas `Required Notice:` que o titular
// forneceu. Sem isto, o ficheiro que mais viaja do projecto não diz de
// onde veio.
//
// ## `postBanner` e não `banner`
//
// A diferença entre os dois é a minificação, e não é um detalhe. O
// `banner` é aplicado **antes** de o `renderChunk` correr e antes de a
// minificação eliminar comentários; o `postBanner` é aplicado **depois**
// dela. Num comentário de bloco normal, o passo de remoção de comentários
// leva-o consigo — e um aviso que desaparece da versão construída é
// pior do que nenhum, porque continua a estar no código-fonte e ninguém
// repara.
//
// ## Porquê uma constante e não um literal
//
// O texto vem de `AVISO_LICENCA`, e não de uma linha escrita aqui. A
// PolyForm pede «plain-text lines beginning with `Required Notice:`» —
//
// com esses dois pontos e essas palavras exactas. Reescrever a frase
// numa paráfrase em português cumpre a intenção e falha o requisito, e a
// diferença entre as duas coisas não aparece em nenhum teste a não ser
// este.
//
// O mesmo texto vive no cabeçalho dos ficheiros de fonte e aqui. São
// duas ocorrências porque não há um sítio único de onde ambas lêem: o
// cabeçalho tem de estar no ficheiro para_polyForm o acompanhar, e o
// pacote não é um ficheiro de fonte. A verificação que impede as duas de
// divergirem é `licenca_e_metadados_sao_coerentes`, em
// `network/daemon_rust/tests/guarda_tor.rs`.
// =====================================================================

/**
 * O aviso que acompanha o pacote construído.
 *
 * Uma linha só, e não uma frase partida em várias. A PolyForm pede
 * «plain-text lines beginning with `Required Notice:`» — linhas, e cada
 * linha tem de começar pelo rótulo. O aviso aqui escrito com o URL
 * numa segunda linha produzia um ficheiro com uma linha que começa
 * como deve ser e outra que é continuação, e ninguém saberia dizer se
 * isso cumpre ou não. Na dúvida, o formato é o dos ficheiros de
 * fonte: uma linha, com tudo nela.
 *
 * @type {string}
 */
const AVISO_LICENCA = [
  '/*!',
  ' * Required Notice: ShegaPT / Carlos Alberto da Silva Brito — ' +
    'PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>',
  ' *',
  ' * Licença base do projecto: PolyForm Noncommercial 1.0.0.',
  ' * Não é open source pela definição da OSI. Uso comercial',
  ' * exige autorização do titular — ver COMMERCIAL-LICENSE.md.',
  ' */',
].join('\n');

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  build: {
    rollupOptions: {
      output: {
        // `postBanner`, nunca `banner`: ver a nota acima. A PolyForm
        // chama ao requisito «plain-text lines beginning with
        // `Required Notice:`», e uma linha que só existe antes da
        // minificação não acompanha o ficheiro que é servido.
        postBanner: AVISO_LICENCA,
      },
    },
  },
})