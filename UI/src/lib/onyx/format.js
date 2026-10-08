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

// =====================================================================
// Semente de avatar
// ---------------------------------------------------------------------
// `sementeDeAvatar` é **cosmética**. Decide que sectors do desenho
// geométrico ficam mais claros, e nada mais.
//
// Não é a impressão digital, não é o identificador, e **não tem valor
// criptográfico nenhum**. A semente é de 32 bits e vem de um FNV-1a; duas
// sementes diferentes dão o mesmo desenho com probabilidade de um em
// quatro mil milhões, e um atacante que veja dois avatares iguais sabe
// pouco mais do que já sabia.
//
// Está aqui, e não numa função de resumo de identidade, porque a mudança
// de nome importa: um nome genérico era lido como sendo o resumo de uma
// identidade. `sementeDeAvatar` diz o que é, e o comentário acima diz o
// que não é.
//
// ## Porque é que não se usa a impressão digital
//
// Porque a impressão tem 128 bits e é lida como uma identidade por toda a
// interface. Reutilizá-la para pintar um avataria seria fazer com que
// material de identidade aparecesse em cada canto da aplicação, para um
// efeito que não o exige. Além disso, `sementeDeAvatar` tem de ser
// **síncrono**:
// um avatar desenha-se durante o render, e `crypto.subtle.digest` é
// assíncrono por definição. A diferença não é um detalhe de implementação,
// é o que mantém o componente dentro do orçamento de um fotograma.
//
// ## Porque é preciso ser determinístico
//
// O mesmo contacto tem de dar o mesmo avatar em todas as vistas e em
// todos os arranques. Uma semente que mudasse a cada desenho faria o
// avatar «respirar», e uma interface onde o avatar de alguém muda
// sobe quando se olha para ele chama a atenção para o avatar em vez da
// pessoa.
// =====================================================================

/**
 * Semente de 32 bits para o desenho do avatar.
 *
 * FNV-1a. Não é uma escolha de segurança — ver o bloco de comentário
 * acima.
 *
 * @param {string} [seed]
 * @returns {number} Inteiro sem sinal de 32 bits.
 */
export function sementeDeAvatar(seed = '') {
  let hash = 2166136261;
  for (let i = 0; i < seed.length; i += 1) {
    hash ^= seed.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  // `Math.abs` devolve `-2^31` intacto — `Math.abs(-2147483648)` é
  // `2147483648`, que já não cabe num inteiro de 32 bits com sinal. A
  // máscara devolve o valor ao intervalo com sinal, e o `>> 0` seguinte
  // em `OnyxAvatar` volta a tratá-lo como não negativo. Sem isto, uma
  // semente específica produz um desenho com sectores invertidos.
  return (hash >>> 0) & 0x7fffffff;
}

/**
 * Iniciais de um nome, para quando um avatar textual é preferível.
 *
 * @param {string} [name]
 * @returns {string}
 */
export function initials(name = '') {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0] || '')
    .join('')
    .toUpperCase();
}

// =====================================================================
// Identidade: o que já vem calculado
// ---------------------------------------------------------------------
// As funções `identifierFor` e `fingerprintFor` **saíram** na Etapa 3.
//
// Derivavam um resumo de 32 bits e um gerador linear congruente, e a
// impressão assim obtida era apresentada como se verificasse algo. Não
// verificava: um LCG sobre 32 bits não tem resistência a colisões, e a
// impressão existe precisamente para dizer que dois identificadores são
// diferentes.
//
// Agora a interface **lê** os dois valores, que são derivados uma vez —
// no registo, em `messenger/identidade.py`, com SHA-256 e um sal por
// conta — e depois persistidos. É assim que o sistema real funciona, e
// é a única forma de a impressão ser um valor e não um número.
// =====================================================================

/**
 * Encurta uma impressão digital para caber numa linha estreita.
 *
 * É uma função de **apresentação**: recebe a impressão já calculada e
 * mostra os três primeiros e os dois últimos octetos. Não calcula nada,
 * e não pode — a impressão de quem se está a ver vem do adaptador.
 *
 * @param {string} [fingerprint] Impressão em `AA:BB:…`.
 * @returns {string} `AA:BB:CC:…:YY:ZZ`, ou o que já vier se não tiver
 *   a forma esperada.
 */
export function shortFingerprint(fingerprint = '') {
  if (!fingerprint) return '';
  const parts = fingerprint.split(':');
  // Uma impressão tem dezasseis octetos. Qualquer coisa mais curta ou
  // mais longa não é uma impressão e devolve-se como veio — mostra-la
  // cortada seria pior do que mostrar o valor inteiro.
  if (parts.length < 6) return fingerprint;
  return `${parts.slice(0, 3).join(':')}:…:${parts.slice(-2).join(':')}`;
}

/**
 * Compara dois identificadores Onyx sem distinção de maiúsculas e
 * minúsculas.
 *
 * Existe porque a pesquisa aceita o que a pessoa escreve, e o que a
 * pessoa escreve raramente é o que está gravado. Comparar texto
 * directamente recusaria metade das pesquisas honestas.
 *
 * @param {string} [valor] O que a pessoa escreveu.
 * @param {string} [gravado] O identificador completo.
 * @returns {boolean}
 */
export function identificadoresCoincidem(valor, gravado) {
  if (!valor || !gravado) return false;
  return gravado.toUpperCase() === String(valor).trim().toUpperCase();
}

export function copyToClipboard(text) {
  if (typeof navigator !== 'undefined' && navigator.clipboard) return navigator.clipboard.writeText(text);
  return Promise.reject(new Error('Área de transferência indisponível'));
}