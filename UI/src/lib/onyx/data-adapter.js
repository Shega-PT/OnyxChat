// =====================================================================
// data-adapter.js — a costura de dados
// ---------------------------------------------------------------------
// Toda a interface lê e escreve por aqui. Nenhuma página conhece a origem
// dos dados, e nenhuma escreve no adaptador.
//
//   páginas  →  dataAdapter  →  mockAdapter    (demonstração)
//   páginas  →  dataAdapter  →  bridgeAdapter  (backend real, Etapa 5)
//
// ## Porque a escolha acontece aqui e não nas páginas
//
// Porque é a única forma de o interruptor de demonstração ser um interruptor
// e não uma condicional espalhada. Se cada vista perguntasse "estou em
// demonstração?", haveria uma resposta errada em cada vista que o
// perguntasse — e uma vista que não o pergunte mostra os dados errados sem
// ninguém dar por isso.
//
// Escolher aqui também significa que trocar o backend real é escrever um
// ficheiro. Nenhuma vista muda.
//
// ## O contrato
//
// As assinaturas de `mockAdapter` e `bridgeAdapter` são idênticas às que
// aqui se exports. A fachada não as reescreve uma a uma — seria uma
// segunda lista para manter em vez de uma, e divergiria. Encaminha por
// delegação, o que faz do objecto exportado um proxy com as mesmas
// propriedades.
//
// A consequência é que `dataAdapter.sendMessage(...)` não existe como
// função no código: existe como acesso a uma propriedade do alvo. É
// intencional, e o custo é uma linha a mais na consola.
// =====================================================================

import { estaEmDemonstracao } from './runtime';
import { mockAdapter } from './mock-adapter';
import { bridgeAdapter } from './bridge-adapter';
import { PonteIndisponivel } from './bridge-adapter';

/**
 * O contrato de dados.
 *
 * É escrito aqui, e não inferido. Duas razões, ambas da mesma natureza:
 *
 *  1. **`dataAdapter` é um `Proxy`.** O TypeScript infere o tipo de
 *     `new Proxy({}, ...)` como `{}`, e cada uma das catorze funções
 *     aparece como «não existe» ao consumidor. Um `@type` em cima da
 *     exportação resolve — e o que se escreve nesse `@type` não pode ser
 *     um palpite.
 *
 *  2. **O contrato precisa de existir por escrito.** As vistas conhecem
 *     estas catorze funções e nada mais. Escrevê-las aqui transforma a
 *     frase «as assinaturas têm de ser idênticas às de `mockAdapter`» numa
 *     verificação: acrescentar uma função a um adaptador e esquecer a outra
 *     passa a dar erro de tipo, e não a dar um erro em produção quando a
 *     vista chama a função que falta.
 *
 * Os tipos de retorno são propositadamente vagos. São dados de
 * demonstração hoje e dados de um sidecar amanhã; escrevê-los com
 * precisão seria inventar um formato que ainda não existe, e a
 * verificação que deles-resultaria seria falsa.
 *
 * @typedef {object} AdaptadorDados
 * @property {() => Promise<object|null>} getIdentity
 * @property {() => Promise<object>} getCounts
 * @property {(opcoes?: {query?: string, filter?: string}) => Promise<object[]>} listConversations
 * @property {(opcoes?: {query?: string}) => Promise<object[]>} listContacts
 * @property {(opcoes?: {includeBlocked?: boolean}) => Promise<object[]>} listRequests
 * @property {(id: string, decisao: string) => Promise<object>} decideRequest
 * @property {() => Promise<object|null>} getNetwork
 * @property {() => Promise<object|null>} getSecurity
 * @property {() => Promise<object[]>} getSettings
 * @property {(definicoes?: object) => Promise<object>} saveSettings
 * @property {() => Promise<{identifier: string, name?: string}|null>} getDiscoverySuggestion
 * @property {(identificador: string) => Promise<object>} discover
 * @property {(conversationId: string, texto: string) => Promise<object>} sendMessage
 * @property {(identificador: string, mensagem: string) => Promise<object>} sendContactRequest
 * @property {() => Promise<{estado: string}>} getAccountState
 * @property {(dados: {utilizador: string, frase: string}) => Promise<object>} registerAccount
 * @property {(dados: {frase: string}) => Promise<object>} unlockAccount
 * @property {() => Promise<void>} lockAccount
 * @property {() => Promise<object>} exportAccount
 * @property {(dados: {conteudo: object, frase: string}) => Promise<object>} restoreAccount
 */

/**
 * Escolhe a implementação conforme o interruptor.
 *
 * Lida a cada acesso em vez de uma vez no carregamento do módulo, para que
 * uma mudança do interruptor não exija reiniciar o servidor de
 * desenvolvimento. O custo é uma leitura de `localStorage` por chamada,
 * que é de ordem microsegundos e não aparece ao lado de um pedido HTTP.
 *
 * @returns {AdaptadorDados}
 */
function alvo() {
  return /** @type {AdaptadorDados} */ (estaEmDemonstracao() ? mockAdapter : bridgeAdapter);
}

/**
 * Adaptador de dados, escolhido pelo interruptor de demonstração.
 *
 * Não há substituto de ambiente para isto, de propósito. Um ambiente não
 * pode desligar um sinalizador que existe para proteger o utilizador: a
 * pergunta «esta aplicação é uma demonstração?» é respondida por quem a
 * usa, não por quem a construiu.
 *
 * @type {AdaptadorDados}
 */
export const dataAdapter = /** @type {AdaptadorDados} */ (
  new Proxy(
    {},
    {
      get(_destino, propriedade) {
        return alvo()[/** @type {keyof AdaptadorDados} */ (propriedade)];
      },
    }
  )
);

/**
 * A implementação real está disponível?
 *
 * Vale `false` quando a demonstração está ligada, e `true` quando o
 * adaptador real está escolhido — **o que não significa que o sidecar
 * esteja a correr**. Para saber isso é preciso tentar um pedido, e tentar
 * um pedido em cada desenho da casca seria um pedido por segundo.
 *
 * A distinção é preservada no texto para a vista mostrar a mensagem certa:
 * «a mostrar dados de demonstração» e «o backend não respondeu» são
 * estados diferentes e o utilizador age de forma diferente em cada um.
 *
 * @returns {boolean}
 */
export function temDadosReais() {
  return !estaEmDemonstracao();
}

/**
 * O erro de backend ausente, reexportado para as vistas.
 *
 * Reexportar em vez de importar de `bridge-adapter` nas vistas evita que
 * elas conheçam a existência das implementações — que é o que a fachada
 * existe para esconder.
 */
export { PonteIndisponivel };