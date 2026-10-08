import React from 'react';
import { cn } from '@/lib/utils';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

/**
 * Cartão de conteúdo com cabeçalho opcional.
 *
 * O cabeçalho só aparece se houver título ou acções, pelo mesmo motivo do
 * `OnyxPanel`. `dense` reduz o enchimento interior de 4 para 3 remote,
 * usado nos cartões que contêm listas de linhas.
 */
export default function OnyxCard({
  title,
  subtitle = undefined,
  badge = undefined,
  actions = undefined,
  children,
  dense = false,
  className = undefined,
}) {
  return (
    <section className={cn('overflow-hidden rounded-lg border border-onyx-line bg-onyx-surface2', className)}>
      {(title || actions) && (
        <header className="flex items-start justify-between gap-3 border-b border-onyx-line px-4 py-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-[12.5px] font-medium text-onyx-text">{title}</h3>
              {badge}
            </div>
            {subtitle && <p className="mt-1 text-[11px] leading-relaxed text-onyx-text3">{subtitle}</p>}
          </div>
          {actions && <div className="flex shrink-0 items-center gap-1.5">{actions}</div>}
        </header>
      )}
      <div className={cn(dense ? 'p-3' : 'p-4')}>{children}</div>
    </section>
  );
}