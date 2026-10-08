import React from 'react';
import {
  Dialogo as Dialog,
  DialogoCabecalho as DialogHeader,
  DialogoConteudo as DialogContent,
  DialogoDescricao as DialogDescription,
  DialogoTitulo as DialogTitle,
} from '@/lib/shadcn';
import { cn } from '@/lib/utils';

const SIZES = {
  sm: 'sm:max-w-[400px]',
  md: 'sm:max-w-[520px]',
  lg: 'sm:max-w-[680px]',
  xl: 'sm:max-w-[840px]',
};

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

/**
 * Modal base do OnyxChat: diálogo com cabeçalho, corpo e barra de ações.
 *
 * `footer` e `description` são opcionais de propósito — o modal de
 * confirmação passa a descrição e não passa rodapé, e o cartão de
 * identidade passa ambos. Compô-los à força de obrigatórios obrigaria a
 * invocar modal vazios só para satisfazer o compilador.
 */
export default function OnyxModal({
  open,
  onOpenChange,
  title,
  description = undefined,
  footer = undefined,
  children,
  size = 'md',
  className = undefined,
  bodyClassName = undefined,
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className={cn(
          '!gap-0 overflow-hidden rounded-lg !border !border-onyx-line !bg-onyx-surface2 !p-0 text-onyx-text shadow-2xl shadow-black/60',
          SIZES[size],
          className
        )}
      >
        <DialogHeader className="border-b border-onyx-line px-5 py-4 text-left">
          <DialogTitle className="text-[13.5px] font-medium tracking-wide text-onyx-text">{title}</DialogTitle>
          {description && (
            <DialogDescription className="mt-1.5 text-[12px] leading-relaxed text-onyx-text2">
              {description}
            </DialogDescription>
          )}
        </DialogHeader>
        <div className={cn('px-5 py-4', bodyClassName)}>{children}</div>
        {footer && (
          <div className="flex items-center justify-end gap-2 border-t border-onyx-line bg-onyx-surface px-5 py-3">
            {footer}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}