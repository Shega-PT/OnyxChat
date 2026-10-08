import React from 'react';
import { Plus, Search } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import { cn } from '@/lib/utils';
import OnyxButton, { OnyxIconButton } from '@/components/onyx/OnyxButton';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import OnyxConversationItem from '@/components/onyx/OnyxConversationItem';
import OnyxContextMenu from '@/components/onyx/OnyxContextMenu';
import ConversationContextItems from '@/components/onyx/ConversationContextItems';
import { copyToClipboard } from '@/lib/onyx/format';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

const FILTERS = [
  { id: 'todas', label: 'Todas' },
  { id: 'nao-lidas', label: 'Não lidas' },
  { id: 'verificadas', label: 'Verificadas' },
];

/** Nada a fazer — usado enquanto as acções do menu ainda não escrevem. */
const semAccao = () => {};

/**
 * Painel lateral da lista de conversas.
 *
 * Os itens do menu de contexto são montados **aqui** e não dentro de
 * `OnyxConversationItem`: o item desenha-se, o menu decide-se. Assim a
 * linha continua sem saber nada sobre o que é uma conversa, e a vista
 * continua a ser a dona das suas acções.
 *
 * A maioria das acções ainda não escreve em lado nenhum — pertencem ao
 * adaptador, que ganha implementação real na Etapa 5 e na demonstração
 * altera só o estado local. Estão isoladas em props opcionais com
 * alternativa inerte, para que a lista não dependa de a vista saber quais
 * estão implementadas.
 */
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
  onMarcarLida = semAccao,
  onSilenciar = semAccao,
  onFixar = semAccao,
  onVerIdentidade = semAccao,
  onLimparHistorico = semAccao,
  onBloquear = semAccao,
  className = undefined,
}) {
  const { toast } = useToast();

  /**
   * Copia o identificador da conversa, anunciando o resultado nos dois
   * sentidos. Esta é a única acção do menu que **está** implementada: não
   * precisa de backend, e serve de referência para o que as outras vão
   * precisar de fazer.
   */
  const copiarIdentificador = (conversation) => {
    const valor = conversation.contact?.identifier || conversation.id;
    copyToClipboard(valor)
      .then(() => toast({ title: 'Identificador copiado', description: valor }))
      .catch(() => toast({ title: 'Não foi possível copiar', variant: 'destructive' }));
  };

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
              <OnyxContextMenu
                key={conversation.id}
                itens={
                  <ConversationContextItems
                    conversation={conversation}
                    onMarcarLida={onMarcarLida}
                    onSilenciar={onSilenciar}
                    onFixar={onFixar}
                    onVerIdentidade={onVerIdentidade}
                    onCopiarIdentificador={copiarIdentificador}
                    onLimparHistorico={onLimparHistorico}
                    onBloquear={onBloquear}
                  />
                }
              >
                <OnyxConversationItem
                  conversation={conversation}
                  selected={conversation.id === selectedId}
                  onSelect={() => onSelect(conversation.id)}
                  indisponivel={Boolean(conversation.indisponivel)}
                />
              </OnyxContextMenu>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}