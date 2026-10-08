// =====================================================================
// mock-adapter.js — implementação de demonstração do adaptador
// ---------------------------------------------------------------------
// Serve dados fictícios, com atrasos simulados que imitam uma rede.
//
// ## O que é fictício, e é preciso dizê-lo
//
// Estes dados **não correspondem ao backend**. Não são uma versão
// simplificada: são coisas que o OnyxChat não tem.
//
// * Latência, jitter, throughput e uptime: o daemon `onyxchatd` não mede
//   nenhum deles. O `IPC` que o Python fala com ele tem treze comandos e
//   nenhum devolve uma medição de desempenho.
// * Um grupo de conversa (`c3`, seis membros): o protocolo é estritamente
//   1:1, com **uma** ligação activa por daemon. Não há salas.
// * Dispositivos, score de segurança, datas de rotação de chave, cópias
//   de segurança: nada disto existe no modelo de chaves, que é uma
//   `Identidade` com uma `seed` Ed25519 e duas chaves simétricas.
// * `ONYX-SP 4.2`, `QUIC · UDP`, `X25519`, `DHT + relays autorizados`:
//   nenhum destes existe. O transporte é TCP sobre Tor; a cifra é
//   Ed25519 com ChaCha20-Poly1305 e AES-256-GCM; e a "descoberta" é um
//   mapa HTTP com tempo de vida, não uma DHT.
//
// Existe porque a interface precisa de ser mostrada e desenhada sem o
// backend ligado, e porque desenhar contra dados inventados é mais rápido
// do que desenhar contra o backend real — que ainda nem existe como
// superfície HTTP (Etapa 5).
//
// Enquanto isto estiver ligado, a barra de aviso da casca está no ecrã.
//
// ## Contrato
//
// As assinaturas são o contrato com as vistas. Este ficheiro e
// `bridge-adapter.js` têm de as cumprir exactamente igual: é isso que
// permite trocar de implementação sem tocar numa única vista.
// =====================================================================

import { identificadoresCoincidem } from './format';
import * as mock from './mock-data';
import * as contaDemo from './conta-demo';

/**
 * Atraso simulado, para que os estados de carregamento sejam vistos.
 *
 * Não é um enfeite: sem ele, `OnyxLoader` e `OnyxEmptyState` apareceriam
 * durante menos de um fotograma e não haveria forma de os avaliar numa
 * demonstração, nem de os testar.
 *
 * @param {number} [ms]
 * @returns {Promise<void>}
 */
const delay = (ms = 280) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Devolve a pessoa tal como está, porque a identidade já lá vem.
 *
 * Antes, este passo calculava o identificador a partir de `id`. Agora
 * `identifier` e `fingerprint` são **dados**: vêm calculados uma vez — em
 * `messenger/identidade.py`, com SHA-256 e um sal — e guardados. Uma
 * demonstração que inventa o valor a cada leitura não demonstraria o
 * sistema real, e o valor inventado seria um resumo de 32 bits, que é
 * exactamente o que a Etapa 3 veio substituir.
 *
 * A função mantém-se porque o `contact` aninhado dentro de uma conversa
 * continua a precisar de vir montado, e porque o contrato com as vistas
 * não mudou.
 *
 * @param {object} person
 * @returns {object}
 */
const decoratePerson = (person) => ({ ...person });

const lastMessageOf = (conversation) => conversation.messages[conversation.messages.length - 1];

function buildConversation(conversation) {
  const contact = conversation.contactId
    ? mock.contacts.find((c) => c.id === conversation.contactId)
    : null;
  const last = lastMessageOf(conversation);
  return {
    ...conversation,
    contact: contact ? decoratePerson(contact) : null,
    title: contact ? contact.name : conversation.title,
    lastMessage: last.text,
    lastFromMe: last.from === 'me',
    lastAuthor: last.author || null,
    lastAt: last.at,
  };
}

/** @type {import('./data-adapter').AdaptadorDados} */
export const mockAdapter = {
  async getIdentity() {
    await delay(240);
    return { ...mock.identity };
  },

  async getCounts() {
    await delay(120);
    return {
      unread: mock.conversations.reduce((total, c) => total + c.unread, 0),
      conversations: mock.conversations.length,
      requests: mock.requests.length,
      contacts: mock.contacts.length,
      peersOnline: mock.network.peersOnline,
      peersTotal: mock.network.peersTotal,
      relays: mock.network.relays,
    };
  },

  async listConversations({ query = '', filter = 'todas' } = {}) {
    await delay(query ? 180 : 360);
    const term = query.trim().toLowerCase();
    return mock.conversations
      .map(buildConversation)
      .filter((c) => {
        if (filter === 'nao-lidas' && c.unread === 0) return false;
        if (filter === 'verificadas' && !c.verified) return false;
        if (!term) return true;
        const haystack = [c.title, c.contact?.identifier, c.contact?.id, c.lastMessage]
          .filter(Boolean)
          .join(' ')
          .toLowerCase();
        return haystack.includes(term);
      })
      .sort((a, b) => {
        // Fixadas primeiro, e dentro de cada grupo da mais recente para a
        // mais antiga. A comparação é por `.getTime()` e não por subtracção
        // de objectos `Date`: a subtracção funciona em runtime (o operador
        // converte para número) mas não é válida segundo o tipo, e
        // silenciá-la seria deixar uma operação sem regra.
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
        return new Date(b.lastAt).getTime() - new Date(a.lastAt).getTime();
      });
  },

  async listContacts({ query = '' } = {}) {
    await delay(query ? 160 : 320);
    const term = query.trim().toLowerCase();
    return mock.contacts
      .map(decoratePerson)
      .filter((c) =>
        !term
          ? true
          : [c.name, c.identifier, c.note, c.role, ...(c.tags || [])]
              .filter(Boolean)
              .join(' ')
              .toLowerCase()
              .includes(term)
      )
      .sort((a, b) => a.name.localeCompare(b.name, 'pt'));
  },

  async listRequests({ includeBlocked = false } = {}) {
    await delay(280);
    return mock.requests.map((request) => ({
      ...request,
      blocked: false,
      handled: includeBlocked ? undefined : undefined,
    }));
  },

  async decideRequest(requestId, decision) {
    await delay(220);
    return { requestId, decision, decidedAt: new Date().toISOString() };
  },

  async getNetwork() {
    await delay(300);
    return mock.network;
  },

  async getSecurity() {
    await delay(300);
    return mock.security;
  },

  async getSettings() {
    await delay(260);
    return mock.settingsSections;
  },

  async saveSettings() {
    await delay(420);
    return { ok: true, savedAt: new Date().toISOString() };
  },

  /** Exemplo de identificador válido, para servirem de sugestão na vista Descobrir. */
  async getDiscoverySuggestion() {
    await delay(80);
    const first = mock.contacts[0];
    return { identifier: first.identifier, name: first.name };
  },

  async discover(rawIdentifier) {
    await delay(760);
    const value = rawIdentifier.trim().replace(/\s+/g, '').toUpperCase();
    if (value.length < 6) {
      return { status: 'invalid', message: 'O identificador tem de ter pelo menos 6 caracteres.' };
    }
    // A pesquisa aceita o identificador completo e o bloco de
    // identidade isolado. Aceitar só o completo faria a pessoa ter de
    // escrever os dezoito caracteres para escrever seis.
    const bloco = value.replace(/^ONYX-/, '').split('-')[0];
    const found = mock.contacts.find((c) => {
      if (identificadoresCoincidem(value, c.identifier)) return true;
      if (value === c.id.toUpperCase()) return true;
      return c.identifier.includes(bloco);
    });
    if (found) return { status: 'found', contact: decoratePerson(found) };
    return { status: 'not_found' };
  },

  async sendMessage(conversationId, text) {
    await delay(160);
    return {
      id: `${conversationId}-${Date.now()}`,
      from: 'me',
      text,
      at: new Date().toISOString(),
      state: 'sent',
    };
  },

  /**
   * O estado da conta, para a interface decidir entre registo e entrada.
   *
   * Devolve sempre `{ estado: 'registada' }` assim que existe conta: a
   * demonstração não tem uma barreira entre o registo e a casca, porque
   * não há chave nenhuma para desbloquear. Dizer «bloqueada» aqui seria
   * obrigar a interface a mostrar um ecrã de entrada para o qual não há
   * segredo — e quem lesse esse ecrã ficaria à espera de uma palavra que
   * não existe em lado nenhum.
   */
  async getAccountState() {
    await delay(120);
    const conta = contaDemo.lerConta();
    return { estado: conta ? 'registada' : 'sem-conta', demonstracao: true };
  },

  /** Cria a conta de demonstração. */
  async registerAccount(dados) {
    await delay(520);
    return contaDemo.criarConta(dados);
  },

  /** Abre a conta de demonstração. */
  async unlockAccount(dados) {
    await delay(420);
    return contaDemo.entrarConta(dados);
  },

  /**
   * Tranca a sessão.
   *
   * A conta continua escrita — quem recarregar a página volta a entrar.
   * Trancar sem apagar é o que a palavra «sair» quer dizer.
   */
  async lockAccount() {
    await delay(160);
  },

  /** Devolve a cópia de segurança, como texto para descarregar. */
  async exportAccount() {
    await delay(300);
    const conta = contaDemo.lerConta();
    if (!conta) throw new Error('Ainda não existe conta para exportar.');
    return { conteudo: conta, nomeFicheiro: 'conta-demo.json' };
  },

  /** Repõe uma conta a partir de uma cópia. */
  async restoreAccount({ conteudo }) {
    await delay(420);
    if (!contaDemo.lerConta()) throw new Error('Ainda não existe conta para substituir.');
    return contaDemo.entrarConta({ frase: conteudo.frase });
  },

  async sendContactRequest(identifier, message) {
    await delay(520);
    return { identifier, message, sentAt: new Date().toISOString() };
  },
};