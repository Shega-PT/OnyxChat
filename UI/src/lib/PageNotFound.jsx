import { Link } from 'react-router-dom';
import OnyxRings from '@/components/onyx/OnyxRings';

/**
 * Página de caminho não encontrado.
 *
 * ## Porque foi reescrita
 *
 * A versão anterior vinha de um esqueleto gerado automaticamente: texto em
 * inglês, cores `slate-*` — que não existem na paleta do projecto —, um
 * botão com um SVG desenhado à mão e uma nota que só aparecia a
 * administradores, a dizer «a IA pode ainda não ter implementado esta
 * página».
 *
 * Nenhuma dessas coisas sobrevive. A nota do administrador dependia de
 * uma chamada de autenticação remota e do campo `role` de uma entidade
 * que já não existe;
 * e uma página de erro que fala inglês, num produto cujo público é
 * português, é um defeito por si só.
 *
 * O desenho segue o mesmo caminho dos estados vazios — anéis concêntricos
 * ao fundo — para que o utilizador não sinta que saiu da aplicação.
 */
export default function PageNotFound() {
  return (
    <div className="relative flex h-screen w-full items-center justify-center overflow-hidden bg-onyx-bg text-onyx-text">
      {/*
        Os anéis são posicionados para transbordarem: um círculo com o
        mesmo diâmetro que o ecrã pareceria um alvo. Saem do ecrã para
        parecerem parte de algo maior, que é a leitura que a linguagem
        gráfica do logótipo pretende.
      */}
      <OnyxRings
        size={720}
        opacity={0.35}
        className="pointer-events-none absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2"
      />

      <div className="relative flex max-w-[420px] flex-col items-center px-5 text-center">
        <p
          className="font-mono text-[11px] uppercase tracking-[0.36em] text-onyx-text3"
          aria-hidden="true"
        >
          Erro
        </p>
        <p className="mt-4 font-mono text-[64px] leading-none tracking-wide text-onyx-metallic/50">
          404
        </p>

        <h1 className="mt-6 text-[15px] font-medium text-onyx-text">
          Esta página não existe
        </h1>
        <p className="mt-2.5 text-[12.5px] leading-relaxed text-onyx-text2">
          O endereço não corresponde a nenhuma vista da aplicação. Se seguiste um
          ligação para aqui, ele aponta para uma página que mudou ou que nunca
          chegou a existir.
        </p>

        <div className="mt-7 flex flex-wrap items-center justify-center gap-2">
          <Link
            to="/"
            className="inline-flex h-8 items-center rounded-md border border-onyx-metallic/30 bg-onyx-elevated2 px-3 text-[12.5px] text-onyx-text transition-colors duration-150 hover:border-onyx-metallic/45"
          >
            Voltar às conversas
          </Link>
          <Link
            to="/definicoes"
            className="inline-flex h-8 items-center rounded-md border border-onyx-line bg-onyx-surface2 px-3 text-[12.5px] text-onyx-text2 transition-colors duration-150 hover:bg-onyx-surface3 hover:text-onyx-text"
          >
            Definições
          </Link>
        </div>

        <p className="mt-8 font-mono text-[10px] tracking-wide text-onyx-text3">
          OnyxChat
        </p>
      </div>
    </div>
  );
}