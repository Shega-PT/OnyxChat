// =====================================================================
// conta-demo.js — a conta da demonstração
// ---------------------------------------------------------------------
// O que esta demonstração faz de diferente de uma conta real: **nada,
// e é esse o objectivo**. Os ecrãs de registo e entrada têm de poder ser
// avaliados, mostrados e testados, e para isso precisam de uma conta que
// exista e aceite uma frase.
//
// A frase que aceitamos é `demo`, e a verificação é uma comparação de
// texto. Isto **não** é criptografia e não pretende ser: o keystore real
// é `messenger/storage.py`, com ChaCha20-Poly1305 e PBKDF2 a 200 mil
// iterações, e a interface fala com ele através do sidecar da Etapa 5.
//
// A linha que separa as duas coisas é esta: `bridge-adapter.js` não
// conhece este ficheiro, e `data-adapter.js` escolhe entre os dois
// adaptadores por um interruptor visível. Quem está a ler a demonstração
// sabe que está a ler a demonstração, porque a faixa está no ecrã.
//
// ## Porque é que a conta de demonstração é derivada, não inventada
//
// `mock-data.js` deriva as identidades com `messenger/identidade.py` e
// um sal à vista, e é por isso que a conta de aqui não pode ter um
// identificador escrito à mão: passaria a ser o único valor da interface
// que não corresponde a nada.
//
// ## Onde fica guardada
//
// Em `localStorage`, e **é preciso** que fique. A conta de demonstração
// que desaparecesse ao recarregar a página faria o ecrã de entrada
// aparecer sempre, e quem estivesse a avaliar os ecrãs nunca veria o
// estado desbloqueado. Num produto real a chave é um ficheiro cifrado em
// disco; aqui é uma chave de demonstração que qualquer pessoa com
// acesso à máquina e ao código lê.
// =====================================================================

import { identity as mockIdentity } from './mock-data';

/** Chave de `localStorage` onde fica a conta de demonstração. */
const CHAVE_CONTA = 'onyx:conta:demo';

/** A frase que esta demonstração aceita. */
export const FRASE_DEMONSTRACAO = 'demo';

/** Comprimento mínimo, espelhando `messenger.conta.FRASE_MINIMA`. */
const COMPRIMENTO_MINIMO = 12;

/** Entropia mínima, espelhando `messenger.conta.ENTROPIA_MINIMA`. */
const ENTROPIA_MINIMA = 60;

/**
 * Os conjuntos de caracteres, por classe.
 *
 * Copiados de `messenger/conta.py`. Viver nos dois lados é uma duplicação
 * — e é uma duplicação que vale a pena, porque a alternativa seria o
 * browser fazer um pedido ao servidor só para perguntar «esta frase é
 * forte?», e o endereço de rede de quem escreve a frase é a última coisa
 * que se quer contactar antes de a conta existir.
 */
const CLASSES = {
  minusculas: 'abcdefghijklmnopqrstuvwxyz',
  maiusculas: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  digitos: '0123456789',
  simbolos: '!@$%^&*()-_=+[]{};:,.<>?/\\|`~',
};

/**
 * Estima a entropia de uma frase, da mesma forma que o servidor.
 *
 * Entropia empírica do multiconjunto de caracteres, multiplicada pelo
 * comprimento. É um **limite superior** do que a frase vale, e mede
 * variedade — não adivinhação. Uma frase de palavras ditas sai com um
 * número alto e continua fraca, e o texto do ecrã diz isso.
 *
 * @param {string} frase
 * @returns {number} bits estimados; `0` para a frase vazia.
 */
export function estimarEntropia(frase) {
  if (!frase) return 0;

  const contagens = new Map();
  for (const caractere of frase) contagens.set(caractere, (contagens.get(caractere) || 0) + 1);

  let entropia = 0;
  for (const n of contagens.values()) {
    const p = n / frase.length;
    entropia -= p * Math.log2(p);
  }
  return entropia * frase.length;
}

/**
 * Avalia a força de uma frase de segurança.
 *
 * Devolve o mesmo par que o servidor devolve — a mesma ordem de
 * verificações — para que o texto do ecrã não mude de regra
 * conforme a origem. A ordem é comprimento, depois variedade: uma frase
 * curta **e** repetitiva tem de ser mandada alongar primeiro, senão a
 * pessoa alonga-a e é recusada outra vez pela mesma razão.
 *
 * @param {string} frase
 * @returns {{ forte: boolean, comprimento: number, bits: number, problema: string }}
 */
export function avaliarFrase(frase) {
  const comprimento = frase.length;
  const bits = estimarEntropia(frase);

  let problema = '';
  if (comprimento === 0) {
    problema = 'Escreva a frase de segurança.';
  } else if (comprimento < COMPRIMENTO_MINIMO) {
    problema = `Faltam ${COMPRIMENTO_MINIMO - comprimento} caracteres.`;
  } else if (bits < ENTROPIA_MINIMA) {
    problema = 'Não faltam caracteres: falta variedade. Use palavras, ou uma frase que só você conheça.';
  }

  return { forte: problema === '', comprimento, bits, problema };
}

/**
 * Lê a conta de demonstração, se existir.
 *
 * @returns {object | null}
 */
export function lerConta() {
  try {
    const bruto = localStorage.getItem(CHAVE_CONTA);
    return bruto === null ? null : JSON.parse(bruto);
  } catch {
    // `localStorage` lança em modo privado de alguns navegadores, e o
    // JSON pode estar corrompido por uma escrita a meio. Nenhum dos dois
    // impede a aplicação de arrancar: o pior é a conta de demonstração
    // não estar lá.
    return null;
  }
}

/**
 * Grava a conta de demonstração.
 *
 * @param {object} conta
 */
function gravar(conta) {
  try {
    localStorage.setItem(CHAVE_CONTA, JSON.stringify(conta));
  } catch {
    // Ver `lerConta`. Sem persistência, a conta dura a sessão.
  }
}

/** Apaga a conta de demonstração. */
export function apagarConta() {
  try {
    localStorage.removeItem(CHAVE_CONTA);
  } catch {
    // Ver `lerConta`.
  }
}

/**
 * Cria uma conta de demonstração.
 *
 * O identificador vem de `mock-data.js`, derivado a sério. A
 * demonstração **não** calcula identidade: seria uma segunda fórmula, e
 * a segunda fórmula é exactamente o que a Etapa 3 veio substituir.
 *
 * @param {{ utilizador: string, frase: string }} dados
 * @returns {object} a conta criada.
 */
export function criarConta({ utilizador, frase }) {
  const nome = utilizador.trim();
  if (!nome) throw new Error('O nome não pode ficar vazio.');

  const forca = avaliarFrase(frase);
  if (!forca.forte) throw new Error(forca.problema);

  // A demonstração não repete aderivação; **reutiliza** a identidade que
  // `mock-data.js` já derivou. Uma conta de demonstração com a mesma
  // pessoa de `mock.identity` é o que faz o ecrã de entrada levar à
  // casca com a identidade que a pessoa já conhecia.
  const base = {
    utilizador: nome,
    identificador: identidadeDemonstracao().identifier,
    impressao: identidadeDemonstracao().fingerprint,
    criadaEm: new Date().toISOString(),
    demonstracao: true,
  };

  gravar({ ...base, frase });
  return base;
}

/**
 * Abre a conta de demonstração.
 *
 * @param {{ frase: string }} dados
 * @returns {object} a conta aberta.
 * @throws {Error} se não houver conta, ou se a frase não bater.
 */
export function entrarConta({ frase }) {
  const guardada = lerConta();
  if (!guardada) throw new Error('Ainda não existe conta neste navegador.');

  if (frase !== guardada.frase && frase !== FRASE_DEMONSTRACAO) {
    throw new Error('A frase de segurança não bate.');
  }

  // A frase nunca é devolvida. `apagarConta` precisa dela, e guardá-la
  // fora do objecto da conta evita que um `console.log` da sessão
  // a imprima junto com o resto.
  const { frase: _omitida, ...conta } = guardada;
  return conta;
}

/**
 * A identidade de demonstração, derivada e com a forma certa.
 *
 * Vem de `mock-data.js`, e não é inventada aqui — a interface nem sequer
 * tem o sal nem a fórmula, e é essa a propriedade que a Etapa 3
 * estabeleceu.
 *
 * @returns {{ identifier: string, fingerprint: string }}
 */
function identidadeDemonstracao() {
  return { identifier: mockIdentity.identifier, fingerprint: mockIdentity.fingerprint };
}