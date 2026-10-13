// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React from 'react';
import { cn } from '@/lib/utils';

/** Cabeçalho consistente para todas as vistas principais. */
export default function PageHeader({
  title,
  description = undefined,
  meta = undefined,
  actions = undefined,
  className = undefined,
}) {
  return (
    <header
      className={cn(
        'flex min-h-[60px] flex-wrap items-center gap-3 border-b border-onyx-line bg-onyx-surface px-4 py-3',
        className
      )}
    >
      <div className="min-w-0">
        <h1 className="text-[13.5px] font-medium text-onyx-text">{title}</h1>
        {description && <p className="mt-0.5 text-[11.5px] text-onyx-text3">{description}</p>}
      </div>
      {meta && <div className="flex items-center gap-2">{meta}</div>}
      {actions && <div className="ml-auto flex items-center gap-2">{actions}</div>}
    </header>
  );
}