// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

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

/**
 * Encontra a conversa de demonstração pelo identificador.
 *
 * @param {string} conversationId
 * @returns {object|undefined}
 */
function acharConversa(conversationId) {
  return mock.conversations.find((c) => c.id === conversationId);
}

/**
 * Encontra o contacto de uma conversa.
 *
 * @param {object} conversation
 * @returns {object|undefined}
 */
function acharContacto(conversation) {
  return conversation?.contactId
    ? mock.contacts.find((c) => c.id === conversation.contactId)
    : undefined;
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

  /**
   * Marca a conversa e as suas mensagens recebidas como lidas.
   *
   * As mensagens da conversa são as `messages`, e cada uma tem `unread`.
   * O zero vai para a conversa e para as mensagens **recebidas** — as
   * enviadas por nós nunca tiveram nada por ler, e limpá-las faria a
   * lista mostrar menos do que a pessoa escreveu. Esta é a mesma razão
   * que está no comentário de `marcar_lida` em `server/loja.py`.
   */
  async markConversationRead(conversationId) {
    await delay(140);
    const conversation = acharConversa(conversationId);
    if (!conversation) throw new Error('Não existe essa conversa.');
    conversation.unread = 0;
    conversation.messages = conversation.messages.map((message) =>
      message.from === 'them' ? { ...message, unread: false } : message
    );
    return { ok: true };
  },

  /** Fixa ou desfixa. Vê `bridge-adapter.pinConversation`. */
  async pinConversation(conversationId, pinned) {
    await delay(140);
    const conversation = acharConversa(conversationId);
    if (!conversation) throw new Error('Não existe essa conversa.');
    conversation.pinned = Boolean(pinned);
    return { ok: true };
  },

  /** Silencia ou dessilencia. Vê `bridge-adapter.muteConversation`. */
  async muteConversation(conversationId, muted) {
    await delay(140);
    const conversation = acharConversa(conversationId);
    if (!conversation) throw new Error('Não existe essa conversa.');
    conversation.muted = Boolean(muted);
    return { ok: true };
  },

  /**
   * Apaga as mensagens, e deixa a conversa.
   *
   * Deixar `messages` com um único item impede `lastMessageOf` de
   * rebentar num `.text` de `undefined` — e a lista continuaria a
   * desenhar, vazia, que é o que se vê depois de limpar.
   */
  async clearHistory(conversationId) {
    await delay(180);
    const conversation = acharConversa(conversationId);
    if (!conversation) throw new Error('Não existe essa conversa.');
    conversation.messages = [{ from: 'system', text: '', at: conversation.messages[0]?.at }];
    conversation.lastMessage = '';
    conversation.unread = 0;
    return { ok: true };
  },

  /**
   * Bloqueia ou desbloqueia o contacto.
   *
   * O bloqueio é do **contacto**, não da conversa: quem se bloqueia é a
   * pessoa, e é o contacto que passa a recusar pedidos. A conversa
   * continua a ser a mesma — silenciar é que é por conversa.
   */
  async blockContact(contactId, blocked) {
    await delay(160);
    const contact = mock.contacts.find((c) => c.id === contactId);
    if (!contact) throw new Error('Não existe esse contacto.');
    contact.blocked = Boolean(blocked);
    return { ok: true };
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
   * Devolve os **três valores do backend** — `sem-conta`, `trancada`,
   * `desbloqueada` — e não um quarto inventado aqui. Esta devia devolver
   * `registada`, que não existe em lado nenhum do outro lado do adaptador,
   * e o `SessaoContext` tratava tudo o que não fosse `sem-conta` como
   * `trancada`: uma coincidência que funcionava por acidente e que
   * deixaria de funcionar à primeira alteração.
   *
   * Uma conta de demonstração existe e está trancada: `entrarConta()`
   * exige a frase, e `FRASE_DEMONSTRACAO` é a frase. Dizer
   * `desbloqueada` sem que a sessão esteja aberta seria o inverso do
   * defeito que este método corrigia.
   *
   * @returns {Promise<{estado: string, demonstracao: boolean}>}
   */
  async getAccountState() {
    await delay(120);
    const conta = contaDemo.lerConta();
    return { estado: conta ? 'trancada' : 'sem-conta', demonstracao: true };
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

  /**
   * Devolve a cópia de segurança, **em base64** como o backend.
   *
   * Esta devolveia `conteudo` já desserializado e um `nomeFicheiro`, o
   * que a tornava incompatível com `bridge-adapter.exportAccount()`: o
   * backend devolve base64 de um ficheiro binário, porque o corpo de uma
   * resposta tem de ser JSON. Duas formas para um método é um contrato
   * que não é contrato — e a versão que diverge sempre é a de
   * demonstração, que é justamente a que ninguém exercita a sério.
   *
   * A demonstração passa a passar pelo mesmo caminho, base64 e
   * descodificação incluídas. É mais uma operação, e é a mesma que o
   * `Recuperar.jsx` faz com uma cópia real.
   *
   * @returns {Promise<{conteudo: string, modo: string}>}
   */
  async exportAccount() {
    await delay(300);
    const conta = contaDemo.lerConta();
    if (!conta) throw new Error('Ainda não existe conta para exportar.');
    return {
      conteudo: btoa(unescape(encodeURIComponent(JSON.stringify(conta)))),
      modo: '0600',
    };
  },

  /**
   * Repõe uma conta a partir de uma cópia.
   *
   * O `conteudo` chega em base64, pelo mesmo caminho de `exportAccount`.
   * Descodificar antes de decidir o que fazer com a frase é o que faz
   * esta função e `bridge-adapter.restoreAccount()` receberem a mesma
   * coisa: um ficheiro não legível é `400`, e uma frase errada é um erro
   * de frase. Confundir os dois seria fazer a pessoa recomeçar à procura
   * do ficheiro errado.
   *
   * @param {{conteudo: string, frase: string}} dados
   */
  async restoreAccount({ conteudo, frase }) {
    await delay(420);
    let conta;
    try {
      conta = JSON.parse(decodeURIComponent(escape(atob(conteudo))));
    } catch {
      throw new Error('a cópia não é um ficheiro válido');
    }
    return contaDemo.reporConta({ conta, frase });
  },

  async sendContactRequest(identifier, message) {
    await delay(520);
    return { identifier, message, sentAt: new Date().toISOString() };
  },
};