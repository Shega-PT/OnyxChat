// =====================================================================
// SessaoContext.jsx — o estado de acesso da aplicação
// ---------------------------------------------------------------------
// Duas coisas, e uma terceiro que não existe: **quem** é a pessoa, **se**
// a conta está trancada, e nada mais.
//
// ## Porque é que não há `isLoadingAuth`
//
// O contexto anterior tinha três bandeiras de carregamento e uma de erro,
// todas precisadas por uma consulta de rede. Aqui o estado vem de um
// ficheiro local, que ou responde de imediato ou responde «não há conta»
// — não há um terceiro estado intermédio para representar. Guardar
// `carregando` para um caso que não existe é a forma de um estado ficar
// obsoleto sem ninguém dar por isso.
//
// ## Os três estados, e porque são três
//
//   * `sem-conta`   — não há ficheiro. Mostra-se o registo.
//   * `trancada`    — há ficheiro, ninguém escreveu a frase. Mostra-se a
//                    entrada.
//   * `aberta`      — há sessão. Mostra-se a casca.
//
// Uma interface que tratasse os dois primeiros como «não posso entrar«
// obrigaria a pessoa a distinguir, no ecrã, entre «criar uma conta» e
// «abrir a que já tem». São operações opostas — a segunda pode perder
// dados, a primeira não — e o erro entre elas custa uma conta.
//
// ## O que NÃO é uma sessão
//
// Não há token, não há expiração, e não há renovação. O que existe é a
// chave em memória e o facto de a pessoa estar a olhar para a aplicação.
// Um contexto que guardasse um token renovável seria uma tradução fiel de
// um modelo que o projecto não tem.
//
// E há uma consequência que vale a pena dizer: **fechar a janela
// bloqueia a conta.** Não há «continuar onde parou» porque não há lado
// nenhum onde «parou» estar guardado. A alternativa — guardar a chave em
// `sessionStorage` — trocaria a protecção da frase de segurança por um
// detalhe de implementação do navegador.
// =====================================================================

import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { estaEmDemonstracao } from '@/lib/onyx/runtime';

const SessaoContext = createContext(undefined);

/**
 * Traduz os erros do adaptador em algo que uma pessoa possa ler.
 *
 * O adaptador devolve `Error` com texto em português, e um ecrã que
 * mostrasse `error.message` de uma excepção desconhecida mostraria
 * «Failed to fetch» ao lado de uma caixa de password — que é exactamente
 * o que acontece quando não há tradução.
 *
 * @param {unknown} erro
 * @returns {string} uma frase para mostrar.
 */
function mensagemDe(erro) {
  if (erro instanceof Error && erro.message) return erro.message;
  return 'Não foi possível falar com o programa local. Tente outra vez.';
}

/**
 * Fornece o estado de acesso a toda a aplicação.
 *
 * @param {object} props
 * @param {React.ReactNode} props.children
 */
export function SessaoProvider({ children }) {
  // `null` é «ainda não sei». Não é `false`: a diferença entre «não há
  // conta» e «ainda não perguntei» é a diferença entre mostrar o registo
  // e mostrar um ecrã em branco durante uma passagem.
  const [estado, setEstado] = useState(null);
  const [conta, setConta] = useState(null);
  const [identidade, setIdentidade] = useState(null);

  /**
   * Pergunta ao adaptador em que estado está a conta.
   *
   * O guarda `activo` existe porque o componente pode ser desmontado
   * enquanto a leitura está a caminho — quem muda de rota nos primeiros
   * milissegundos veria um aviso «actualizar a componente» no consola do
   * React. É um erro de corrida real, e o padrão mais fiável contra ele
   * é montar o guarda num `useEffect` e não dentro da função assíncrona.
   */
  useEffect(() => {
    let activo = true;

    dataAdapter
      .getAccountState()
      .then((resposta) => {
        if (!activo) return;
        setEstado(resposta?.estado === 'sem-conta' ? 'sem-conta' : 'trancada');
      })
      .catch(() => {
        // Sem backend, a honestidade é não saber. Um ecrã de acesso que
        // aparece porque o servidor não respondeu é pior do que a
        // aplicação aberta e sem dados: uma falha de rede não deve
        // parecer uma decisão de segurança.
        if (activo) setEstado('desconhecido');
      });

    return () => {
      activo = false;
    };
  }, []);

  /**
   * Lê a identidade, para a casca ter o que mostrar.
   *
   * Corre ao lado da pergunta pela conta, e não depende do resultado
   * dela: quem está em modo de demonstração tem identidade sem ter conta,
   * e obrigar a interface a esperar por uma conta que não existe faria a
   * faixa de aviso aparecer com atraso.
   */
  const carregarIdentidade = useCallback(async () => {
    try {
      const valor = await dataAdapter.getIdentity();
      setIdentidade(valor ?? null);
    } catch {
      // A identidade é o melhor esforço. Sem ela, as vistas mostram o
      // estado vazio, que é honesto. Engolir a excepção é deliberado: quem
      // decide o que fazer com ela é quem chama.
      setIdentidade(null);
    }
  }, []);

  useEffect(() => {
    carregarIdentidade();
  }, [carregarIdentidade]);

  /**
   * Cria a conta e abre a sessão.
   *
   * @param {{utilizador: string, frase: string}} dados
   * @returns {Promise<{erro: string}>} `{}` em caso de sucesso.
   */
  const registar = useCallback(async (dados) => {
    try {
      const criada = await dataAdapter.registerAccount(dados);
      setConta(criada);
      setEstado('aberta');
      await carregarIdentidade();
      return {};
    } catch (erro) {
      return { erro: mensagemDe(erro) };
    }
  }, [carregarIdentidade]);

  /**
   * Abre a sessão com a frase de segurança.
   *
   * @param {{frase: string}} dados
   * @returns {Promise<{erro: string}>} `{}` em caso de sucesso.
   */
  const entrar = useCallback(async (dados) => {
    try {
      const aberta = await dataAdapter.unlockAccount(dados);
      setConta(aberta);
      setEstado('aberta');
      await carregarIdentidade();
      return {};
    } catch (erro) {
      return { erro: mensagemDe(erro) };
    }
  }, [carregarIdentidade]);

  /**
   * Tranca a sessão.
   *
   * Não apaga nada. `trancar` e «sair da conta» são coisas diferentes, e
   * a segunda não existe neste projecto: apagar a identidade de alguém
   * porque fechou a janela seria uma decisão que a interface nunca toma
   * por si.
   */
  const trancar = useCallback(async () => {
    try {
      await dataAdapter.lockAccount();
    } catch {
      // Falhar a trancar não pode deixar a pessoa com a sessão aberta sem
      // saber: o estado local é a fonte, e o servidor é quem diz.
    }
    setConta(null);
    setEstado('trancada');
  }, []);

  /**
   * Exporta a cópia de segurança.
   *
   * @returns {Promise<{erro: string}|{ficheiro: object}>}
   */
  const exportar = useCallback(async () => {
    try {
      const ficheiro = await dataAdapter.exportAccount();
      return { ficheiro };
    } catch (erro) {
      return { erro: mensagemDe(erro) };
    }
  }, []);

  /**
   * Repõe a conta a partir de uma cópia de segurança.
   *
   * O conteúdo vem como objecto já lido, e não como caminho: quem
   * escolheu o ficheiro foi o navegador, e mandar um caminho para o
   * servidor seria dar-lhe o poder de ler um ficheiro que a pessoa não
   * escolheu.
   *
   * Não abre a sessão. Repor e abrir são duas operações separadas porque
   * as duas falham por razões diferentes — uma cópia pode estar boa e a
   * frase errada, e nesse caso quem recomeçou quer ver a conta outra vez
   * fechada e não ser atirado para a casca.
   *
   * @param {{conteudo: object, frase: string}} dados
   * @returns {Promise<{erro: string}>} `{}` em caso de sucesso.
   */
  const restaurar = useCallback(async (dados) => {
    try {
      await dataAdapter.restoreAccount(dados);
      return {};
    } catch (erro) {
      return { erro: mensagemDe(erro) };
    }
  }, []);

  const valor = useMemo(
    () => ({
      estado,
      conta,
      identidade,
      demonstracao: estaEmDemonstracao(),
      registar,
      entrar,
      trancar,
      exportar,
      restaurar,
      carregarIdentidade,
    }),
    [estado, conta, identidade, registar, entrar, trancar, exportar, restaurar, carregarIdentidade]
  );

  return <SessaoContext.Provider value={valor}>{children}</SessaoContext.Provider>;
}

/**
 * Lê o estado de acesso.
 *
 * @returns {{
 *   estado: 'desconhecido' | 'sem-conta' | 'trancada' | 'aberta' | null,
 *   conta: object | null,
 *   identidade: object | null,
 *   demonstracao: boolean,
 *   registar: (dados: {utilizador: string, frase: string}) => Promise<{erro?: string}>,
 *   entrar: (dados: {frase: string}) => Promise<{erro?: string}>,
 *   trancar: () => Promise<void>,
 *   exportar: () => Promise<{erro?: string} | {ficheiro: object}>,
 *   restaurar: (dados: {conteudo: object, frase: string}) => Promise<{erro?: string}>,
 *   carregarIdentidade: () => Promise<void>,
 * }}
 */
export function useSessao() {
  const contexto = useContext(SessaoContext);
  if (!contexto) {
    throw new Error('useSessao tem de ser usado dentro de SessaoProvider');
  }
  return contexto;
}