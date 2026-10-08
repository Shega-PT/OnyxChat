// Adaptador de dados do OnyxChat.
//
// Ponto único de integração:
//   UI  →  dataAdapter  →  dados reais
//
// Enquanto os dados forem fictícios, este módulo lê mock-data.js.
// Para ligar dados reais, substitui apenas o corpo destas funções
// (mantendo as assinaturas) — nenhuma vista precisa de ser alterada.

import { identifierFor } from './format';
import * as mock from './mock-data';

const delay = (ms = 280) => new Promise((resolve) => setTimeout(resolve, ms));

const decoratePerson = (person) => ({ ...person, identifier: identifierFor(person.id) });

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

export const dataAdapter = {
  async getIdentity() {
    await delay(240);
    return { ...mock.identity, identifier: identifierFor(mock.identity.id) };
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
      identifier: identifierFor(request.personId),
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
    return { identifier: identifierFor(first.id), name: first.name };
  },

  async discover(rawIdentifier) {
    await delay(760);
    const value = rawIdentifier.trim().replace(/\s+/g, '').toUpperCase();
    if (value.length < 6) {
      return { status: 'invalid', message: 'O identificador tem de ter pelo menos 6 caracteres.' };
    }
    const found = mock.contacts.find((c) => {
      const identifier = identifierFor(c.id).toUpperCase();
      return value === identifier || value === c.id.toUpperCase() || identifier.includes(value);
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

  async sendContactRequest(identifier, message) {
    await delay(520);
    return { identifier, message, sentAt: new Date().toISOString() };
  },
};