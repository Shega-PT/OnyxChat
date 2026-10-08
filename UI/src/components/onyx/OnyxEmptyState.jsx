import React from 'react';
import { cn } from '@/lib/utils';
import OnyxLogo from '@/components/onyx/OnyxLogo';
import OnyxRings from '@/components/onyx/OnyxRings';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

/**
 * Estado vazio, na linguagem gráfica do OnyxChat: anéis concêntricos ao
 * fundo e o logótipo centrado, nunca um ícone genérico.
 *
 * `compact` reduz o tamanho total para caber dentro de um painel lateral
 * ou de uma lista; a versão completa usa mais respiro vertical.
 */
export default function OnyxEmptyState({
  title,
  description = undefined,
  action = undefined,
  compact = false,
  hint = undefined,
  className = undefined,
}) {
  return (
    <div
      className={cn(
        'relative flex flex-col items-center justify-center overflow-hidden text-center',
        compact ? 'px-6 py-10' : 'px-8 py-16',
        className
      )}
    >
      <OnyxRings
        size={compact ? 260 : 360}
        opacity={0.4}
        className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2"
      />
      <OnyxLogo size={compact ? 40 : 52} className="relative" />
      <h3 className="relative mt-5 text-[13.5px] font-medium text-onyx-text">{title}</h3>
      {description && (
        <p className="relative mt-2 max-w-[300px] text-[12px] leading-relaxed text-onyx-text2">{description}</p>
      )}
      {action && <div className="relative mt-6">{action}</div>}
      {hint && <p className="relative mt-5 font-mono text-[10.5px] tracking-wide text-onyx-text3">{hint}</p>}
    </div>
  );
}