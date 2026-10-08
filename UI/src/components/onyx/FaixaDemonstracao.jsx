import React from 'react';
import { FlaskConical, X } from 'lucide-react';
import { definirDemonstracao } from '@/lib/onyx/runtime';
import { cn } from '@/lib/utils';
import { OnyxIconButton } from '@/components/onyx/OnyxButton';

/**
 * Faixa de aviso de demonstração.
 *
 * Aparece no topo da aplicação, acima de tudo, sempre que os dados são
 * fictícios. Não há forma de a fechar sem desligar a demonstração.
 *
 * ## Porque é uma faixa e não um aviso
 *
 * Um aviso que se pode dispensar é um aviso que se dispensa. E a
 * consequência de dispensar este seria exactamente a que este ficheiro
 * existe para evitar: alguém mostra a interface a outra pessoa, essa
 * pessoa vê um score de segurança de 92 e um protocolo `ONYX-SP 4.2`, e sai
 * a crer que o OnyxChat os tem. Não tem — nenhum dos dois existe.
 *
 * Por isso a faixa não tem botão de fechar. O único botão é **desligar a
 * demonstração**, que recarrega a aplicação e a deixa a mostrar o estado
 * verdadeiro: a bridge, que até à Etapa 5 recusa com uma explicação.
 *
 * ## A cor
 *
 * `bg-onyx-warning` — a cor de aviso da paleta, não uma cor nova. A faixa
 * usa a mesma linguagem visual de um estado de aviso em qualquer outro
 * sítio, e não um inválido que tencionasse gritar por ser mais importante.
 */
export default function FaixaDemonstracao({ className = undefined }) {
  return (
    <div
      // `role="status"` em vez de `role="alert"`: não é uma mensagem de
      // erro que interrompe o que se está a fazer, é uma condição
      // permanente. `alert` interromperia a leitura aos leitores de ecrã a
      // cada navegação, que é o oposto do pretendido.
      role="status"
      className={cn(
        'flex h-8 shrink-0 items-center gap-2 border-b border-onyx-warning/40 bg-onyx-warning/12 px-3',
        'font-mono text-[10.5px] tracking-wide text-onyx-warning',
        className
      )}
    >
      <FlaskConical className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      <span className="min-w-0 truncate">
        Modo de demonstração — os dados são fictícios e não vêm do OnyxChat.
      </span>
      <span className="hidden truncate text-onyx-text3 md:inline">
        Latências, dispositivos e o score de segurança não existem.
      </span>
      <div className="ml-auto flex shrink-0 items-center gap-1">
        <button
          type="button"
          onClick={() => definirDemonstracao(false)}
          className="rounded px-2 py-0.5 text-onyx-text2 underline-offset-2 transition-colors duration-150 hover:text-onyx-text hover:underline"
        >
          Desligar
        </button>
        <OnyxIconButton
          onClick={() => definirDemonstracao(false)}
          aria-label="Desligar o modo de demonstração"
          className="h-6 w-6"
        >
          <X className="h-3 w-3" />
        </OnyxIconButton>
      </div>
    </div>
  );
}