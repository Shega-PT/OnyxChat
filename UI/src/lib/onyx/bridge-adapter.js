// =====================================================================
// bridge-adapter.js — implementação real do adaptador
// ---------------------------------------------------------------------
// Habla com o sidecar local em `127.0.0.1`, que por sua vez fala com o
// daemon `onyxchatd` pelo socket Unix.
//
// ## O que este adaptador faz e o que ele não faz
//
// Fala com o sidecar em `127.0.0.1:8787`, que é quem sabe do daemon e da
// loja. Cada função abaixo é um caminho e um corpo — nada mais.
//
// ## A diferença entre «vazio» e «sem dados»
//
// A alternativa a um sidecar real seria devolver objectos vazios ou de
// exemplo, e é precisamente essa a alternativa que este projecto recusa.
// Uma vista que recebe `[]` e um `OnyxEmptyState` a dizer «Nenhuma
// conversa» comunica «não tens conversas»; o que é verdade, com o daemon
// em baixo, é «não há fonte de dados». São mensagens diferentes, e a
// pessoa age de forma diferente em cada uma.
//
// Por isso o sidecar responde `503` quando o daemon não responde, e este
// adaptador transforma isso em `PonteIndisponivel` — que a casca da
// aplicação mostra como estado próprio, legível, e que não se confunde com
// «vazio».
//
// ## Contrato
//
// As assinaturas têm de ser exactamente as de `mock-adapter.js`. É isso que
// permite à fachada trocar de implementação sem tocar numa vista.
//
// A cadeia completa é:
//
//   vista → dataAdapter → bridgeAdapter → sidecar → messenger/ipc_client → onyxchatd
//                                        │
//                                        └→ loja (sqlite, o sidecar é dono)
//
// ## Erros
//
// Três, e a distinção é o que permite à interface reagir bem:
//
//   `PonteIndisponivel` — o sidecar não respondeu, ou respondeu algo que
//     não é JSON. A pessoa reinicia o programa.
//   `ErroDoSidecar`      — o sidecar respondeu com um erro. O `detalhe` é
//     o que a pessoa lê, e não é opcional.
//   `SyntaxError` do JSON — não chega a chegar aqui; é convertido logo.
//
// Nenhum dos três é engolido. Um adaptador que devolvesse `[]` em vez de
// falhar transformaria «o programa não arrancou» em «não tens
// conversas», e essa é a falha que custa a confiança numa interface.
// =====================================================================

/**
 * Erro de sidecar ausente ou inacessível.
 *
 * Distinto de qualquer outro erro de propósito: uma falha de rede ou um
 * erro do daemon produziriam a mesma excepção com outra informação, e
 * confundir "não há sidecar" com "o sidecar falhou" levaria a mostrar a
 * mensagem errada.
 */
export class PonteIndisponivel extends Error {
  constructor(detalhe) {
    super(
      `O backend local não está disponível${detalhe ? `: ${detalhe}` : '.'} ` +
        'O sidecar corre em 127.0.0.1 e é a única forma de falar com o ' +
        'daemon; sem ele, ligue o modo de demonstração.'
    );
    this.name = 'PonteIndisponivel';
  }
}

/** Endereço do sidecar. Fica aqui para não estar repetido em cada função. */
const BASE = 'http://127.0.0.1:8787';

/**
 * O tempo limite de um pedido.
 *
 * A conta é a operação mais lenta que há: `conta.criar` deriva a chave do
 * keystore com PBKDF2 a 200 mil iterações, e um portátil modesto demora
 * segundos. Um limite de dez segundos dá folga larga sem que um sidecar
 * que morreu fique a pedir resposta durante um minuto.
 */
const LIMITE_MS = 10000;

/**
 * Executa um pedido ao sidecar.
 *
 * É o **único** sítio que conhece o endereço, o tempo limite e o formato
 * do erro. Cada função deste adaptador limita-se a dizer o caminho e o
 * corpo, o que é o que permite mudar o endereço num sítio só.
 *
 * @param {string} caminho Caminho relativo, com barra inicial.
 * @param {RequestInit} [opcoes]
 * @returns {Promise<any>}
 */
async function pedir(caminho, opcoes = {}) {
  const controlador = new AbortController();
  // O temporizador é sempre limpo, mesmo quando o pedido vai a tempo.
  // Deixá-lo pendente mantém o controlador vivo e o `fetch` nunca
  // termina — que é um vazamento que só aparece sob carga.
  const temporizador = setTimeout(() => controlador.abort(), LIMITE_MS);

  try {
    const resposta = await fetch(`${BASE}${caminho}`, {
      ...opcoes,
      signal: controlador.signal,
      headers: { Accept: 'application/json', ...(opcoes.headers || {}) },
    });

    const bruto = await resposta.text();
    let corpo = null;
    try {
      corpo = bruto ? JSON.parse(bruto) : null;
    } catch {
      // Uma resposta que não é JSON é um sinal de que o sidecar não é
      // o que se supõe que seja. Deixar o JSON inválido passar daria à
      // interface um `undefined` silencioso em vez de um erro visível.
      throw new PonteIndisponivel(
        `o sidecar devolveu algo que não é JSON em ${caminho}: ${bruto.slice(0, 80)}`
      );
    }

    if (!resposta.ok) {
      // O corpo de erro do sidecar traz `erro`, `codigo` e `detalhe`. O
      // `detalhe` é o que a pessoa lê — «a frase de segurança não
      // cumpre os mínimos» — e não é opcional: sem ele, um erro de
      // validação diz apenas «pedido malformado».
      throw new ErroDoSidecar(corpo?.erro || `HTTP ${resposta.status}`, resposta.status, corpo?.detalhe || '');
    }

    return corpo;
  } catch (erro) {
    if (erro instanceof PonteIndisponivel || erro instanceof ErroDoSidecar) throw erro;
    if (erro.name === 'AbortError') {
      throw new PonteIndisponivel(`sem resposta do sidecar em ${LIMITE_MS / 1000}s`);
    }
    // A negição de rede do browser não diz *porquê*. `Failed to fetch`
    // é o que a pessoa veria, e não diz nada de útil.
    throw new PonteIndisponivel(erro instanceof Error ? erro.message : String(erro));
  } finally {
    clearTimeout(temporizador);
  }
}

/**
 * Erro devolvido pelo sidecar, com o código e o detalhe.
 *
 * Distinto de `PonteIndisponivel` porque o que se faz a seguir é
 * diferente: uma indisponibilidade é «o programa não está a correr» e a
 * pessoa reinicia; um erro do sidecar é «o pedido está errado» e a pessoa
 * corrige o que escreveu.
 */
export class ErroDoSidecar extends Error {
  constructor(mensagem, codigo, detalhe) {
    super(detalhe ? `${mensagem}: ${detalhe}` : mensagem);
    this.name = 'ErroDoSidecar';
    this.codigo = codigo;
    this.detalhe = detalhe;
  }
}

/**
 * Adaptador real — os 22 métodos contra o `sidecar` de `127.0.0.1`.
 *
 * `PonteIndisponivel` fica para o sidecar que não respondeu dentro do
 * limite; os pedidos que chegam ao servidor e são recusados dão
 * `ErroDoSidecar`, que traz o código HTTP e o detalhe.
 *
 * Uma versão anterior deste ficheiro dizia que «todas as funções
 * recusavam até a Etapa 5». Já não era verdade, e uma mensagem de erro
 * que aponta para um estado ultrapassado faz quem depura perder tempo a
 * procurar a Etapa 5 num projecto que já passou dela.
 */
/** @type {import('./data-adapter').AdaptadorDados} */
export const bridgeAdapter = {
  /** @returns {Promise<object>} */
  async getIdentity() {
    return pedir('/api/identidade');
  },

  /** @returns {Promise<object>} */
  async getCounts() {
    return pedir('/api/contagens');
  },

  /** @returns {Promise<object[]>} */
  async listConversations({ query = '', filter = 'todas' } = {}) {
    return pedir(`/api/conversas?pesquisa=${encodeURIComponent(query)}&filtro=${filter}`);
  },

  /** @returns {Promise<object[]>} */
  async listContacts({ query = '' } = {}) {
    return pedir(`/api/contactos?pesquisa=${encodeURIComponent(query)}`);
  },

  /** @returns {Promise<object[]>} */
  async listRequests({ includeBlocked = false } = {}) {
    return pedir(`/api/pedidos?incluirBloqueados=${includeBlocked}`);
  },

  /** @returns {Promise<object>} */
  async decideRequest(requestId, decision) {
    return pedir(`/api/pedidos/${encodeURIComponent(requestId)}`, {
      method: 'POST',
      body: JSON.stringify({ decisao: decision }),
    });
  },

  /** @returns {Promise<object>} */
  async getNetwork() {
    return pedir('/api/estado');
  },

  /** @returns {Promise<object>} */
  async getSecurity() {
    return pedir('/api/seguranca');
  },

  /** @returns {Promise<object[]>} */
  async getSettings() {
    return pedir('/api/definicoes');
  },

  /** @returns {Promise<object>} */
  async saveSettings(definicoes) {
    return pedir('/api/definicoes', {
      method: 'PUT',
      body: JSON.stringify(definicoes),
    });
  },

  /** @returns {Promise<object>} */
  async getDiscoverySuggestion() {
    return pedir('/api/descoberta/sugestao');
  },

  /** @returns {Promise<object>} */
  async discover(identificador) {
    return pedir(`/api/descoberta?identificador=${encodeURIComponent(identificador)}`);
  },

  /** @returns {Promise<object>} */
  async sendMessage(conversationId, text) {
    return pedir('/api/mensagens', {
      method: 'POST',
      body: JSON.stringify({ conversa: conversationId, texto: text }),
    });
  },

  /** @returns {Promise<object>} */
  async sendContactRequest(identifier, message) {
    return pedir('/api/pedidos', {
      method: 'POST',
      body: JSON.stringify({ identificador: identifier, mensagem: message }),
    });
  },
  /**
   * O estado da conta: `sem-conta`, `trancada` ou `desbloqueada`.
   *
   * Três estados, e não dois. Uma interface que só soubesse «existe conta»
   * ou «não existe conta» não teria como mostrar a diferença entre uma
   * conta fechada e uma sessão aberta — e as duas levam a acções opostas:
   * na primeira há uma frase para escrever, na segunda há uma porta aberta.
   */
  async getAccountState() {
    return pedir('/api/conta/estado');
  },

  /**
   * Cria a conta.
   *
   * A frase de segurança vai no corpo do pedido e é a chave do keystore do
   * lado do Python. Nunca vai num cabeçalho, nunca vai na ligação, e nunca
   * é guardada por este ficheiro.
   */
  async registerAccount({ utilizador, frase }) {
    return pedir('/api/conta', {
      method: 'POST',
      body: JSON.stringify({ utilizador, frase }),
    });
  },

  /** Abre a conta com a frase de segurança. */
  async unlockAccount({ frase }) {
    return pedir('/api/conta/desbloquear', {
      method: 'POST',
      body: JSON.stringify({ frase }),
    });
  },

  /**
   * Tranca a sessão sem apagar nada.
   *
   * Não apaga a conta nem a chave: quem voltar a abrir a aplicação escreve
   * a mesma frase e entra. O que «sair» significa é parar de usar, e não
   * destruir.
   */
  async lockAccount() {
    await pedir('/api/conta/trancar', { method: 'POST' });
  },

  /**
   * Exporta a cópia de segurança.
   *
   * Devolve um ficheiro **já cifrado**, com o modo `0600` no lado do
   * Python. A interface descarrega-o e não sabe o que está lá dentro — que
   * é o que permite que a cópia vá para um disco externo sem se transformar
   * num segredo novo.
   */
  async exportAccount() {
    return pedir('/api/conta/exportar');
  },

  /**
   * Repõe a conta a partir de uma cópia.
   *
   * O conteúdo chega como texto, não como caminho: quem escolhe o ficheiro
   * é o navegador, e mandar um caminho para o servidor seria dar-lhe o
   * poder de ler um ficheiro que a pessoa não escolheu.
   */
  async restoreAccount({ conteudo, frase }) {
    return pedir('/api/conta/restaurar', {
      method: 'POST',
      body: JSON.stringify({ conteudo, frase }),
    });
  },

};

/**
 * `getNetwork` chama `pedir`, que lança. A referência directa mantém a
 * assinatura visível para quem lê o contrato; o `void` impede que o
 * invocador espere um valor que nunca chega.
 */
void pedir;