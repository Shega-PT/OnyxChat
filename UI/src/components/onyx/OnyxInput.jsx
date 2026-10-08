import React from 'react';
import { cn } from '@/lib/utils';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

const baseField =
  'w-full rounded-md border bg-onyx-surface2 px-3 text-[13.5px] text-onyx-text placeholder:text-onyx-text3 transition-colors duration-150 focus:border-onyx-metallic/45 focus:bg-onyx-surface3 focus:outline-none focus:ring-2 focus:ring-onyx-metallic/15 disabled:opacity-50';

/**
 * Invólucro comum ao campo e à área de texto: rótulo, dica, erro.
 *
 * A dica some quando há erro — mostrar as duas coisas ao mesmo tempo
 * compete pela mesma linha e nenhum dos dois textos fica legível.
 */
function FieldShell({ label = undefined, hint = undefined, error = undefined, className = undefined, children }) {
  return (
    <label className={cn('block', className)}>
      {label && <span className="onyx-label mb-1.5 block">{label}</span>}
      {children}
      {hint && !error && <span className="mt-1.5 block text-[11px] leading-relaxed text-onyx-text3">{hint}</span>}
      {error && <span className="mt-1.5 block text-[11px] text-onyx-error">{error}</span>}
    </label>
  );
}

/**
 * Campo de texto de uma linha.
 *
 * `trailing` recebe um elemento sobreposto à direita (um ícone), e o
 * campo ganha o preenchimento correspondente para o texto não ficar por
 * baixo dele.
 */
export default function OnyxInput({
  label = undefined,
  hint = undefined,
  error = undefined,
  mono = false,
  className = undefined,
  inputClassName = undefined,
  trailing = undefined,
  ...props
}) {
  return (
    <FieldShell label={label} hint={hint} error={error} className={className}>
      <span className="relative block">
        <input
          className={cn(
            baseField,
            'h-9',
            mono && 'font-mono text-[12px] tracking-wide',
            error ? 'border-onyx-error/60' : 'border-onyx-line',
            trailing && 'pr-10',
            inputClassName
          )}
          {...props}
        />
        {trailing && <span className="absolute right-1.5 top-1/2 -translate-y-1/2">{trailing}</span>}
      </span>
    </FieldShell>
  );
}

/** Área de texto de várias linhas, com a mesma estrutura de `OnyxInput`. */
export function OnyxTextarea({
  label = undefined,
  hint = undefined,
  error = undefined,
  className = undefined,
  rows = 3,
  ...props
}) {
  return (
    <FieldShell label={label} hint={hint} error={error} className={className}>
      <textarea
        rows={rows}
        className={cn(baseField, 'resize-none py-2 leading-relaxed', error ? 'border-onyx-error/60' : 'border-onyx-line')}
        {...props}
      />
    </FieldShell>
  );
}