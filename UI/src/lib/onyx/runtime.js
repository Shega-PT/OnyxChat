// =====================================================================
// runtime.js — decisões de ambiente, lidas uma vez e num sítio só
// ---------------------------------------------------------------------
// O interruptor de demonstração e a única fonte de verdade sobre se a
// aplicação está a mostrar dados fictícios.
//
// ## Porque é que isto existe em vez de um `import.meta.env` espalhado
//
// O problema que resolve não é técnico, é de confiança.
//
// O OnyxChat mostra, em modo de demonstração, coisas que **não existem no
// backend**: latências, jitter, throughput, três dispositivos, um score
// de segurança, grupos de conversa, um protocolo chamado `ONYX-SP 4.2`.
// Nenhuma delas é real. O modo de demonstração existe porque a interface
// precisa de ser mostrada sem o backend ligado — e é exactamente por isso
// que é perigoso.
//
// Sem um sinalizador, alguém demonstra a interface a outra pessoa e essa
// pessoa sai a crer que o OnyxChat tem essas coisas. Com um sinalizador
// visível e sempre ligado, ninguém pode confundir. A implementação que
// existia antes desta regra tinha 92 pontos de segurança e um protocolo
// chamado `ONYX-SP 4.2`; ambos eram fictícios, e nada na interface dizia
// que eram.
//
// ## As três fontes, por ordem de prioridade
//
//   1. **O interruptor em Definições** — decisão do utilizador, guardada
//      em `localStorage`. É a que ganha: quem primeiramente liga e desliga
//      a demonstração está a falar com quem lhe vai mostrar.
//   2. **`?mock=1` ou `?mock=0` na ligação** — para uma demonstração
//      pontual sem tocar nas definições, e para testes automatizados.
//      Sobrepõe-se à definição guardada **e não a altera**: recarregar a
//      página volta ao que estava definido.
//   3. **`VITE_ONYX_MOCK`** — a predefinição de construção. Serve para
//      uma cópia de demonstração da aplicação, em que o estado normal é
//      a demonstração.
//
// A ordem não é arbitrária: a fonte mais específica é a mais forte, e a
// ligação é mais específica do que uma definição guardada — mas é
// efémera, e é por isso que não a grava.
//
// ## Porque é lido uma vez e não a cada chamada
//
// `estaEmDemonstracao()` lê o valor e devolve um booleano. Não observa
// alterações: mudar o interruptor em Definições recarrega a aplicação.
//
// A alternativa — um subscritor — seria mais elegante e produziria dois
// estados que podem divergir: um booleano lido por uma vista e o valor
// actual lido por outra, durante o mesmo instante. Um recarregamento é um
// custo de 200 ms que elimina uma classe inteira de bugs.
// =====================================================================

/** Chave do interruptor em `localStorage`. */
const CHAVE_DEMONSTRACAO = 'onyx:demonstracao';

/** Prefixo do parâmetro de ligação que sobrepõe o interruptor. */
const PARAMETRO_DEMONSTRACAO = 'mock';

/**
 * Lê o parâmetro `?mock=` da ligação.
 *
 * Devolve `null` quando não há parâmetro, e um booleano quando há. O valor
 * por omissão de um booleano seria `false`, e `false` é uma resposta
 * válida — sem o `null` não se distinguiria «não disse nada» de «disse
 * que não», e a diferença decide se a definição guardada se aplica.
 *
 * @returns {boolean | null}
 */
function parametroDaLigacao() {
  if (typeof window === 'undefined') return null;

  const bruto = new URLSearchParams(window.location.search).get(PARAMETRO_DEMONSTRACAO);
  if (bruto === null) return null;

  // Qualquer valor que não seja uma negação explita conta como ligado.
  // A razão: quem escreve `?mock` na barra de endereços quer a
  // demonstração, e tratar `?mock=` como desligado seria surpreendente.
  return bruto !== '0' && bruto.toLowerCase() !== 'false';
}

/**
 * Lê o interruptor guardado em Definições.
 *
 * A ausência de chave significa «demonstração ligada». É a predefinição de
 * uma aplicação que ainda não tem backend: sem ela, uma instalação nova
 * abriria vazia e pareceria estar avariada.
 *
 * @returns {boolean | null} `null` quando nunca foi definido.
 */
function definicaoGuardada() {
  if (typeof localStorage === 'undefined') return null;

  try {
    const bruto = localStorage.getItem(CHAVE_DEMONSTRACAO);
    if (bruto === null) return null;
    return bruto === '1';
  } catch {
    // `localStorage` lança em modo privado de alguns navegadores, e em
    // contextos onde as cookies estão bloqueadas. Uma falha de
    // armazenamento não pode impedir a aplicação de arrancar, e o pior
    // que acontece é cair na predefinição.
    return null;
  }
}

/**
 * Está a aplicação em modo de demonstração?
 *
 * @returns {boolean}
 */
export function estaEmDemonstracao() {
  const daLigacao = parametroDaLigacao();
  if (daLigacao !== null) return daLigacao;

  const guardada = definicaoGuardada();
  if (guardada !== null) return guardada;

  // Predefinição de construção. Ausente, conta como demonstração — pelas
  // razões de `definicaoGuardada`.
  const deConstrucao = import.meta.env.VITE_ONYX_MOCK;
  if (deConstrucao !== undefined) return deConstrucao !== '0' && deConstrucao !== 'false';

  return true;
}

/**
 * Guarda a definição de demonstração e devolve o valor que passa a vigorar.
 *
 * Recarrega a aplicação para que o adaptador e as vistas voltem a ser
 * construídos com a implementação certa. Sem o recarregamento, metade da
 * aplicação continuaria a falar do adaptador antigo — que é a forma mais
 * confusa de um interruptor falhar.
 *
 * @param {boolean} valor
 * @returns {boolean} O valor que passou a vigorar.
 */
export function definirDemonstracao(valor) {
  const novo = Boolean(valor);
  if (typeof localStorage !== 'undefined') {
    try {
      localStorage.setItem(CHAVE_DEMONSTRACAO, novo ? '1' : '0');
    } catch {
      // Ver `definicaoGuardada`. Sem persistência, o interruptor funciona
      // até ao fim da sessão.
    }
  }

  // O parâmetro de ligação sobrepõe-se à definição guardada; limpá-lo é o
  // que faz o interruptor passar a valer depois de o gravar.
  if (typeof window !== 'undefined' && window.location) {
    const url = new URL(window.location.href);
    if (url.searchParams.has(PARAMETRO_DEMONSTRACAO)) {
      url.searchParams.delete(PARAMETRO_DEMONSTRACAO);
      window.history.replaceState({}, '', url);
    }
  }

  window.location.reload();
  return novo;
}

/**
 * A definição está a ser forçada pelo parâmetro de ligação?
 *
 * Serve para a interface de Definições mostrar o interruptor como
 * «temporário»: alterá-lo não muda nada enquanto o parâmetro estiver lá, e
 * dizer o contrário seria mentir sobre o estado do interruptor.
 *
 * @returns {boolean}
 */
export function demonstracaoEstaForcada() {
  return parametroDaLigacao() !== null;
}