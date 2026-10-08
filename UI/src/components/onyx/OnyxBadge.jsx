import React from 'react';
import { cn } from '@/lib/utils';

// =====================================================================
// Convenção de contrato, aplicada a todos os primitivos `Onyx*`.
//
// **Toda a prop opcional recebe um valor por omissão explícito**, mesmo
// que esse valor seja `undefined`.
//
// A razão não é estética. Em JavaScript com `checkJs`, o TypeScript infere
// o tipo de uma prop desestruturada sem valor por omissão como *obrigatória*:
// não há forma de distinguir "não passada" de "não declarada" sem o valor.
// O resultado é um contrato falso — o compilador exige ao consumidor uma
// prop que o componente trata como opcional, e o `typecheck` enche-se de
// `TS2741` falsos. Dar `= undefined` não muda o comportamento em runtime
// (é o mesmo valor que a omissão produz) e corrige o contrato.
//
// A consequência prática é que a lista de props deste ficheiro é a
// definição completa e verificável do componente.
// =====================================================================

const VARIANTS = {
  neutral: 'border-onyx-line bg-onyx-surface3 text-onyx-text2',
  accent: 'border-onyx-metallic/30 bg-onyx-elevated2 text-onyx-text',
  secure: 'border-onyx-info/30 bg-onyx-info/10 text-onyx-info',
  success: 'border-onyx-success/30 bg-onyx-success/10 text-onyx-success',
  warning: 'border-onyx-warning/30 bg-onyx-warning/10 text-onyx-warning',
  error: 'border-onyx-error/30 bg-onyx-error/10 text-onyx-error',
  outline: 'border-onyx-line bg-transparent text-onyx-text3',
};

/**
 * Etiqueta compacta. `mono` força a tipografia monoespaçada, usada nos
 * valores técnicos (identificadores, latências, versões).
 */
export default function OnyxBadge({
  children,
  variant = 'neutral',
  mono = false,
  className = undefined,
  title = undefined,
}) {
  return (
    <span
      title={title}
      className={cn(
        'inline-flex shrink-0 items-center gap-1 rounded border px-1.5 py-0.5 text-[10.5px] font-medium leading-4',
        mono && 'font-mono tracking-wide',
        VARIANTS[variant] || VARIANTS.neutral,
        className
      )}
    >
      {children}
    </span>
  );
}

/** Tecla ou combinação de teclas, para dicas de atalho. */
export function OnyxKbd({ children, className = undefined }) {
  return (
    <kbd
      className={cn(
        'inline-flex h-5 items-center rounded border border-onyx-line bg-onyx-surface3 px-1.5 font-mono text-[10px] leading-none text-onyx-text3',
        className
      )}
    >
      {children}
    </kbd>
  );
}