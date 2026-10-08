import React from 'react';
import { cn } from '@/lib/utils';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

/**
 * Bloco de secção para o painel de detalhes.
 *
 * O cabeçalho só é desenhado se houver rótulo ou acção: um `header` vazio
 * deixaria um espaço morto de 2,5 remote em cada bloco.
 */
export default function OnyxPanel({
  label = undefined,
  action = undefined,
  children,
  className = undefined,
}) {
  return (
    <section className={cn('border-b border-onyx-line px-3.5 py-3.5 last:border-b-0', className)}>
      {(label || action) && (
        <header className="mb-2.5 flex items-center justify-between gap-2">
          <span className="onyx-label">{label}</span>
          {action}
        </header>
      )}
      {children}
    </section>
  );
}

const STATE_DOT = {
  ok: 'bg-onyx-success',
  warn: 'bg-onyx-warning',
  error: 'bg-onyx-error',
  info: 'bg-onyx-info',
};

/**
 * Linha rótulo/valor, com ponto de estado e acção opcional à direita.
 *
 * `value` e `children` são alternativas: o primeiro serve para texto
 * simples e o segundo para conteúdo composto. A prioridade é de `value`,
 * o que evita ter de envolver texto simples num elemento.
 */
export function OnyxRow({
  label,
  value = undefined,
  mono = false,
  state = undefined,
  action = undefined,
  className = undefined,
  children = undefined,
}) {
  return (
    <div className={cn('flex items-start justify-between gap-3 py-1.5', className)}>
      <span className="min-w-0 flex-1 text-[11.5px] leading-5 text-onyx-text3">{label}</span>
      <span className="flex min-w-0 items-center gap-2">
        {state && STATE_DOT[state] && <span className={cn('h-1.5 w-1.5 shrink-0 rounded-full', STATE_DOT[state])} />}
        {(value || children) && (
          <span
            className={cn(
              'truncate text-right text-[12px] leading-5 text-onyx-text2',
              mono && 'font-mono text-[11px] tracking-wide'
            )}
          >
            {value || children}
          </span>
        )}
        {action}
      </span>
    </div>
  );
}

/** Grelha de factos em duas colunas, usada nos cartões de identidade. */
export function OnyxFactGrid({ items = [], className = undefined }) {
  return (
    <div className={cn('grid grid-cols-2 gap-x-4 gap-y-3', className)}>
      {items.filter(Boolean).map((item) => (
        <div key={item.label} className="min-w-0">
          <p className="onyx-label mb-1.5">{item.label}</p>
          <p className={cn('truncate text-[12px] text-onyx-text', item.mono && 'font-mono text-[11px] tracking-wide')}>
            {item.value}
          </p>
        </div>
      ))}
    </div>
  );
}