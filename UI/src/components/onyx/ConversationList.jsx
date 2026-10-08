import React from 'react';
import { Plus, Search } from 'lucide-react';
import { cn } from '@/lib/utils';
import OnyxButton, { OnyxIconButton } from '@/components/onyx/OnyxButton';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import OnyxConversationItem from '@/components/onyx/OnyxConversationItem';

const FILTERS = [
  { id: 'todas', label: 'Todas' },
  { id: 'nao-lidas', label: 'Não lidas' },
  { id: 'verificadas', label: 'Verificadas' },
];

export default function ConversationList({
  conversations = [],
  loading = false,
  selectedId = undefined,
  onSelect,
  query = '',
  onQueryChange,
  filter = 'todas',
  onFilterChange,
  onNew = undefined,
  className = undefined,
}) {
  return (
    <div className={cn('flex min-h-0 flex-col border-onyx-line bg-onyx-surface', className)}>
      <div className="shrink-0 space-y-2.5 border-b border-onyx-line px-3 py-3">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-[13px] font-medium text-onyx-text">Conversas</h2>
          <OnyxIconButton onClick={onNew} aria-label="Nova conversa">
            <Plus className="h-4 w-4" />
          </OnyxIconButton>
        </div>
        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-onyx-text3" />
          <input
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="Pesquisar conversas"
            className="h-8 w-full rounded-md border border-onyx-line bg-onyx-surface2 pl-8 pr-2.5 text-[12.5px] text-onyx-text placeholder:text-onyx-text3 transition-colors duration-150 focus:border-onyx-metallic/45 focus:bg-onyx-surface3 focus:outline-none"
          />
        </div>
        <div className="flex items-center gap-0.5">
          {FILTERS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onFilterChange(item.id)}
              className={cn(
                'h-7 rounded px-2 text-[11.5px] transition-colors duration-150',
                filter === item.id
                  ? 'bg-onyx-elevated text-onyx-text'
                  : 'text-onyx-text3 hover:bg-onyx-surface3 hover:text-onyx-text2'
              )}
            >
              {item.label}
            </button>
          ))}
          <span className="ml-auto font-mono text-[10px] tracking-wide text-onyx-text3">{conversations.length}</span>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {loading ? (
          <OnyxLoader label="A carregar conversas" />
        ) : conversations.length === 0 ? (
          <OnyxEmptyState
            compact
            title="Nenhuma conversa"
            description="Nada corresponde a este filtro. Ajusta a pesquisa ou inicia uma conversa nova."
            action={
              <OnyxButton variant="secondary" size="sm" onClick={onNew}>
                <Plus className="h-3.5 w-3.5" /> Nova conversa
              </OnyxButton>
            }
          />
        ) : (
          <div className="flex flex-col gap-0.5">
            {conversations.map((conversation) => (
              <OnyxConversationItem
                key={conversation.id}
                conversation={conversation}
                selected={conversation.id === selectedId}
                onSelect={() => onSelect(conversation.id)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}