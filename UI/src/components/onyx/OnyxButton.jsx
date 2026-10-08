import React from 'react';
import { cn } from '@/lib/utils';

const VARIANTS = {
  primary:
    'border border-onyx-metallic/30 bg-onyx-elevated2 text-onyx-text hover:border-onyx-metallic/45 hover:bg-onyx-elevated',
  secondary:
    'border border-onyx-line bg-onyx-surface2 text-onyx-text2 hover:bg-onyx-surface3 hover:text-onyx-text',
  outline: 'border border-onyx-line bg-transparent text-onyx-text hover:border-onyx-metallic/40 hover:bg-onyx-surface2',
  ghost: 'border border-transparent text-onyx-text2 hover:bg-onyx-surface3 hover:text-onyx-text',
  danger: 'border border-onyx-error/35 bg-onyx-error/10 text-onyx-error hover:bg-onyx-error/20',
};

const SIZES = {
  sm: 'h-7 gap-1.5 px-2.5 text-[12px]',
  md: 'h-9 gap-2 px-3.5 text-[13px]',
  lg: 'h-10 gap-2 px-4 text-[13.5px]',
  icon: 'h-8 w-8 justify-center',
};

/**
 * Classes do botão só de ícone.
 *
 * Exportadas para que uma vista possa reutilizá-las sem instanciar o
 * componente — usado pelos painéis de detalhe e pela barra superior, que
 * precisam do mesmo peso visual sem um handler associado.
 */
export const iconButtonClass =
  'inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-onyx-text3 transition-colors duration-150 hover:bg-onyx-surface3 hover:text-onyx-text disabled:pointer-events-none disabled:opacity-40 focus-visible:ring-2 focus-visible:ring-onyx-metallic/25';

// Nota sobre `forwardRef`: este componente **não** o usa, e essa remoção é
// deliberada. Com `React.forwardRef` e desestruturação por descanso, o
// TypeScript infere o tipo das props como o objecto das props nomeadas
// (`{ className, type }`) e perde por completo o resto — `children`,
// `onClick` e `aria-label` deixam de existir no contrato, e o consumidor
// recebe erros em todas as chamadas. Verificou-se que **nenhum** consumidor
// passa `ref` a estes botões: o único `ref` do projecto que chega a um
// elemento nativo é o `textarea` do compositor de mensagens e o `div` da
// lista, ambos criados dentro dos seus próprios componentes. O `forwardRef`
// era, portanto, peso morto que só servia para degradar a verificação de
// tipos.

/**
 * Contrato das props partilhado pelos dois botões.
 *
 * @typedef {object} PropsBotao
 * @property {'primary'|'secondary'|'outline'|'ghost'|'danger'} [variant]
 * @property {'sm'|'md'|'lg'|'icon'} [size]
 * @property {'button'|'submit'|'reset'} [type]
 * @property {string} [className]
 */

/**
 * Botão só de ícone, quadrado, para acções de barra e cabeçalhos.
 *
 * @param {PropsBotao & Record<string, unknown>} props
 */
export function OnyxIconButton({ className = undefined, type = 'button', ...props }) {
  return <button type={type} className={cn(iconButtonClass, className)} {...props} />;
}

/**
 * Botão principal da aplicação, com variantes e quatro tamanhos.
 *
 * @param {PropsBotao & Record<string, unknown>} props
 */
export function OnyxButton({ variant = 'secondary', size = 'md', type = 'button', className = undefined, ...props }) {
  return (
    <button
      type={type}
      className={cn(
        'inline-flex items-center rounded-md font-medium transition-colors duration-150 disabled:pointer-events-none disabled:opacity-45 focus-visible:ring-2 focus-visible:ring-onyx-metallic/25',
        VARIANTS[variant],
        SIZES[size],
        className
      )}
      {...props}
    />
  );
}

export default OnyxButton;