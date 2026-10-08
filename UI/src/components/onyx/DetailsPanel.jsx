import React from 'react';
import { X } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { OnyxIconButton } from '@/components/onyx/OnyxButton';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';

/** Painel contextual direito — recebe conteúdo da vista ativa. */
export default function DetailsPanel({ className }) {
  const { details, setPanelOpen } = useOnyxUI();

  return (
    <aside className={cn('w-[320px] shrink-0 flex-col border-l border-onyx-line bg-onyx-surface', className)}>
      <div className="flex h-[60px] shrink-0 items-center justify-between border-b border-onyx-line px-3.5">
        <span className="onyx-label">Detalhes</span>
        <OnyxIconButton onClick={() => setPanelOpen(false)} aria-label="Fechar detalhes">
          <X className="h-4 w-4" />
        </OnyxIconButton>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {details || (
          <OnyxEmptyState
            compact
            title="Sem seleção"
            description="Seleciona uma conversa, contacto ou pedido para ver aqui informação técnica contextual."
          />
        )}
      </div>
    </aside>
  );
}