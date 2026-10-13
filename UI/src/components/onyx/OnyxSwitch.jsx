// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React from 'react';
import { Interruptor as Switch } from '@/lib/shadcn';
import { cn } from '@/lib/utils';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

/**
 * Interruptor, sobre o primitivo do Radix.
 *
 * Os modificadores `!` são necessários porque o componente do Radix define
 * as suas próprias cores com `!important` em variantes de estado; sem os
 * sobrescrever aqui, a paleta Onyx não chegaria ao ecrã.
 */
export default function OnyxSwitch({ className = undefined, ...props }) {
  return (
    <Switch
      className={cn(
        'shrink-0 !border !border-onyx-line data-[state=checked]:!border-onyx-metallic/40 data-[state=checked]:!bg-onyx-elevated2 data-[state=unchecked]:!bg-onyx-surface3 [&>span]:!bg-onyx-metallic2',
        className
      )}
      {...props}
    />
  );
}