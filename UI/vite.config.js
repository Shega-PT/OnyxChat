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
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
})