// Utilitários de formatação e identidade visual do OnyxChat.

export const LOCALE = 'pt-PT';

export function formatTime(iso) {
  return new Date(iso).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' });
}

function startOfDay(date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

export function formatDayLabel(iso) {
  const date = new Date(iso);
  const diff = Math.round((startOfDay(new Date()) - startOfDay(date)) / 86400000);
  if (diff === 0) return 'Hoje';
  if (diff === 1) return 'Ontem';
  if (diff < 7) return date.toLocaleDateString(LOCALE, { weekday: 'long' });
  return date.toLocaleDateString(LOCALE, { day: '2-digit', month: 'short' });
}

export function formatRelative(iso) {
  if (!iso) return '';
  const minutes = Math.floor((Date.now() - new Date(iso).getTime()) / 60000);
  if (minutes < 1) return 'agora';
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h`;
  const days = Math.floor(hours / 24);
  if (days === 1) return 'ontem';
  if (days < 7) return `${days} d`;
  return new Date(iso).toLocaleDateString(LOCALE, { day: '2-digit', month: '2-digit', year: '2-digit' });
}

/**
 * Formata uma data ISO em português europeu.
 *
 * `options` é declarado como `Intl.DateTimeFormatOptions` através de
 * JSDoc porque, sem essa anotação, o TypeScript não consegue distinguir
 * um objecto de opções válido de um qualquer objecto, e a chamada a
 * `toLocaleDateString` deixa de ter sobrecarga que sirva.
 *
 * @param {string} iso Data em ISO 8601.
 * @param {Intl.DateTimeFormatOptions} [options]
 * @returns {string}
 */
export function formatDate(iso, options = { day: '2-digit', month: 'long', year: 'numeric' }) {
  if (!iso) return '';
  return new Date(iso).toLocaleDateString(LOCALE, options);
}

export function truncate(text, max = 72) {
  if (!text) return '';
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

export function hashSeed(seed = '') {
  let hash = 2166136261;
  for (let i = 0; i < seed.length; i += 1) {
    hash ^= seed.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return Math.abs(hash);
}

export function initials(name = '') {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0] || '')
    .join('')
    .toUpperCase();
}

const SYMBOLS = '!@#$%&*';

/** Identificador público no formato do logo: ONYX-XXXXXX-!X0%X0# */
export function identifierFor(id = '') {
  const hash = hashSeed(id);
  const alphabet = 'ABCDEFGHJKLMNPQRSTUVWXYZ0123456789';
  let tail = '';
  for (let i = 0; i < 4; i += 1) {
    const value = (hash >> (i * 3)) % alphabet.length;
    tail += i % 2 === 0 ? SYMBOLS[value % SYMBOLS.length] : alphabet[value];
  }
  return `ONYX-${id.toUpperCase()}-${tail}#`;
}

/** Impressão digital determinística a partir do identificador (formato AA:BB:…). */
export function fingerprintFor(id = '') {
  const alphabet = '0123456789ABCDEF';
  let hash = hashSeed(id) || 1;
  const pairs = [];
  for (let i = 0; i < 16; i += 1) {
    hash = (hash * 1103515245 + 12345) & 0x7fffffff;
    const byte = hash % 256;
    pairs.push(`${alphabet[(byte >> 4) & 15]}${alphabet[byte & 15]}`);
  }
  return pairs.join(':');
}

export function shortFingerprint(id = '') {
  const full = fingerprintFor(id);
  const parts = full.split(':');
  return `${parts.slice(0, 3).join(':')}:…:${parts.slice(-2).join(':')}`;
}

export function copyToClipboard(text) {
  if (typeof navigator !== 'undefined' && navigator.clipboard) return navigator.clipboard.writeText(text);
  return Promise.reject(new Error('Área de transferência indisponível'));
}