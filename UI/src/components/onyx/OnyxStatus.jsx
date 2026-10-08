import React from 'react';
import { cn } from '@/lib/utils';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`:
// é a mesma convenção que em todos os primitivos `Onyx*`.

/**
 * Estados possíveis. `pulse` marca os estados que estão a mudar — é o
 * único sinal animado, para que a animação signifique sempre "em
 * movimento" e nunca seja decoração.
 */
const STATES = {
  online: { label: 'Online', dot: 'bg-onyx-success', pulse: true },
  away: { label: 'Ausente', dot: 'bg-onyx-warning' },
  busy: { label: 'Ocupado', dot: 'bg-onyx-error' },
  offline: { label: 'Offline', dot: 'bg-onyx-metallic' },
  secure: { label: 'Cifrado', dot: 'bg-onyx-info' },
  syncing: { label: 'A sincronizar', dot: 'bg-onyx-info', pulse: true },
};

/**
 * Indicador de estado: ponto com halo pulsante opcional e rótulo.
 *
 * `label` sobrepõe o rótulo por omissão do estado; `showLabel={false}`
 * deixa só o ponto, para listas densas.
 */
export default function OnyxStatus({
  status = 'offline',
  label = undefined,
  showLabel = true,
  className = undefined,
  size = 'md',
}) {
  // Um estado desconhecido degrada para `offline` em vez de partir: um
  // valor novo vindo do backend tem de ser visível como "desligado",
  // não tem de fazer o componente devolver `undefined`.
  const state = STATES[status] || STATES.offline;
  const dotSize = size === 'sm' ? 'h-1.5 w-1.5' : 'h-2 w-2';

  return (
    <span className={cn('inline-flex items-center gap-1.5', className)}>
      <span className="relative inline-flex">
        <span className={cn('rounded-full', dotSize, state.dot)} />
        {state.pulse && (
          <span className={cn('absolute inset-0 rounded-full opacity-40 animate-onyx-pulse', state.dot)} />
        )}
      </span>
      {showLabel && (
        <span className={cn('text-onyx-text2', size === 'sm' ? 'text-[10.5px]' : 'text-[11.5px]')}>
          {label || state.label}
        </span>
      )}
    </span>
  );
}