import React from 'react';
import { X } from 'lucide-react';
import { cn } from '@/lib/utils';
import { OnyxIconButton } from '@/components/onyx/OnyxButton';

/** Gaveta lateral usada em ecrãs estreitos para a navegação e para os detalhes. */
export default function MobileDrawer({ side = 'left', onClose, children, label = 'Painel' }) {
  return (
    <div className="fixed inset-0 z-50 flex">
      <button
        type="button"
        aria-label="Fechar painel"
        onClick={onClose}
        className="absolute inset-0 animate-onyx-fade bg-black/60 backdrop-blur-[1px]"
      />
      <div
        className={cn(
          'relative z-10 flex h-full animate-onyx-scale-in flex-col border-onyx-line bg-onyx-surface shadow-2xl shadow-black/60',
          side === 'left' ? 'mr-auto border-r' : 'ml-auto border-l'
        )}
      >
        <div className="flex items-center justify-between border-b border-onyx-line px-3 py-2">
          <span className="onyx-label">{label}</span>
          <OnyxIconButton onClick={onClose} aria-label="Fechar painel">
            <X className="h-4 w-4" />
          </OnyxIconButton>
        </div>
        <div className="flex min-h-0 flex-1">{children}</div>
      </div>
    </div>
  );
}