import { Notificador as Toaster } from '@/lib/shadcn';
import { BrowserRouter as Router, Navigate, Route, Routes } from 'react-router-dom';
import ScrollToTop from '@/components/ScrollToTop';
import { SessaoProvider, useSessao } from '@/lib/SessaoContext';
import PageNotFound from '@/lib/PageNotFound';
import AppShell from '@/components/onyx/AppShell';
import Conversations from '@/pages/Conversations';
import Contacts from '@/pages/Contacts';
import Requests from '@/pages/Requests';
import Discover from '@/pages/Discover';
import Network from '@/pages/Network';
import Security from '@/pages/Security';
import Identity from '@/pages/Identity';
import Settings from '@/pages/Settings';
import Entrar from '@/pages/Entrar';
import Registar from '@/pages/Registar';
import Recuperar from '@/pages/Recuperar';

/**
 * Raiz da aplicação.
 *
 * ## O portão, e o que ele não é
 *
 * Havia, e bloqueava o arranque até uma chamada a um serviço hospedado
 * responder. Depois reenviava para um ecrã de sessão com um token de um
 * fornecedor externo — um estado em que a aplicação não faz nada enquanto
 * espera por um serviço que pode não estar na máquina.
 *
 * O portão de agora é o inverso disso, e a diferença está no que bloqueia:
 *
 *   * **bloqueia a identidade.** Sem a frase de segurança não se lê a
 *     seed Ed25519, e sem a seed não há assinatura nem cifra de nada.
 *   * **não espera por rede.** O estado vem de um ficheiro local. Entre
 *     «ainda não perguntei» e «perguntei» não passa um ecrã em branco: há
 *     um estado `desconhecido` que diz exactamente isso.
 *   * **não tem token.** Não há sessão a expirar, e portanto não há
 *     «a sua sessão expirou» — a sessão acaba quando a janela fecha.
 *
 * ## Porque a demonstração não passa por aqui
 *
 * Porque em modo de demonstração não há identidade a proteger: os dados
 * são fictícios e a faixa de aviso está no ecrã. Um portão de acesso por
 * cima de uma demonstração seria pedir a alguém que se lembre de uma
 * palavra para ver conteúdo inventado.
 *
 * A consequência é que os ecrãs de acesso **continuam alcançáveis** em
 * `/entrar`, `/registar` e `/recuperar`, mesmo com o portão desligado.
 * Sem isso nunca seriam vistos, e um ecrã que ninguém vê nunca é
 * corrigido.
 *
 * ## Estrutura
 *
 * Um único nível de rotas, todas dentro da casca. Não há grupos nem
 * variantes enquanto não houver condições de acesso distintas: uma
 * estrutura de rotas que antecipa permissões que não existem é uma
 * estrutura que se reescreve inteira quando elas chegarem.
 */
function Aplicacao() {
  const { estado, demonstracao } = useSessao();

  // Em demonstração o portão não existe. Ver acima.
  if (!demonstracao && (estado === 'sem-conta' || estado === 'trancada')) {
    return (
      <Routes>
        <Route path="/registar" element={<Registar />} />
        <Route path="/entrar" element={<Entrar />} />
        <Route path="/recuperar" element={<Recuperar />} />
        {/*
          Tudo o resto vai para o ecrã que faz falta: quem não tem conta
          é mandado para o registo, quem tem vai para a entrada.

          `replace` porque o endereço anterior deixa de fazer sentido. Deixá-lo
          na história é o que faz a pessoa carregar em «voltar» e voltar a
          ver um ecrã de acesso depois de já ter entrado.
        */}
        <Route
          path="*"
          element={<Navigate to={estado === 'sem-conta' ? '/registar' : '/entrar'} replace />}
        />
      </Routes>
    );
  }

  // `null` é «ainda não sei». A casca é mostrada assim mesmo, com o que
  // houver: mostrar um ecrã de espera antes de cada arranque seria pior do
  // que a alternativa, que é a casca com o estado vazio durante uma
  // passagem. E um `desconhecido` que resultasse de uma falha de rede
  // **não** pode virar ecrã de acesso — uma falha de rede não é uma
  // decisão de segurança.
  return (
    <Routes>
      {/*
        Três ecrãs que existem em todos os estados, mesmo com o portão
        desligado. Ver «Porque a demonstração não passa por aqui».
      */}
      <Route path="/entrar" element={<Entrar />} />
      <Route path="/registar" element={<Registar />} />
      <Route path="/recuperar" element={<Recuperar />} />

      <Route element={<AppShell />}>
        <Route path="/" element={<Conversations />} />
        <Route path="/contactos" element={<Contacts />} />
        <Route path="/pedidos" element={<Requests />} />
        <Route path="/descobrir" element={<Discover />} />
        <Route path="/rede" element={<Network />} />
        <Route path="/seguranca" element={<Security />} />
        <Route path="/identidade" element={<Identity />} />
        <Route path="/definicoes" element={<Settings />} />
      </Route>
      <Route path="*" element={<PageNotFound />} />
    </Routes>
  );
}

export default function App() {
  return (
    // `SessaoProvider` por fora de tudo — o portão de acesso e a faixa
    // de demonstração leem o mesmo estado, e quem decide o que mostrar
    // tem de estar acima de quem mostra.
    <SessaoProvider>
      <Router>
        <ScrollToTop />
        <Aplicacao />
      </Router>
      {/*
        O `Toaster` fica **fora** do `Router` por uma razão que não é
        estética: é o único elemento que sobrevive a uma troca de rota, e
        quem o puser dentro da casca perderia as notificações a cada
        navegação — o utilizador carrega em «enviar» e não vê nada
        acontecer.
      */}
      <Toaster />
    </SessaoProvider>
  );
}