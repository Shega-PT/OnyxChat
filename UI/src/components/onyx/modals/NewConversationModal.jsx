import React, { useState } from 'react';
import { Search } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { usarToast as useToast } from '@/lib/shadcn';
import OnyxModal from '@/components/onyx/OnyxModal';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import OnyxContactItem from '@/components/onyx/OnyxContactItem';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';

export default function NewConversationModal({ open, onClose }) {
  const [query, setQuery] = useState('');
  const [selectedId, setSelectedId] = useState(null);
  const { data, loading } = useAsync(
    () => (open ? dataAdapter.listContacts({ query }) : Promise.resolve([])),
    [query, open]
  );
  const { toast } = useToast();
  const navigate = useNavigate();

  const contacts = data || [];
  const selected = contacts.find((contact) => contact.id === selectedId) || null;

  return (
    <OnyxModal
      open={open}
      onOpenChange={(value) => !value && onClose()}
      title="Nova conversa"
      description="Escolhe um contacto verificado. A sessão é negociada antes da primeira mensagem."
      size="md"
      footer={
        <>
          <OnyxButton variant="ghost" onClick={onClose}>
            Cancelar
          </OnyxButton>
          <OnyxButton
            variant="primary"
            disabled={!selected}
            onClick={() => {
              toast({ title: 'Sessão iniciada', description: `Conversa com ${selected.name}.` });
              onClose();
              setSelectedId(null);
              navigate('/');
            }}
          >
            Iniciar conversa
          </OnyxButton>
        </>
      }
    >
      <div className="relative">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-onyx-text3" />
        <input
          autoFocus
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Pesquisar por nome, identificador ou etiqueta"
          className="h-9 w-full rounded-md border border-onyx-line bg-onyx-surface2 pl-8 pr-2.5 text-[12.5px] text-onyx-text placeholder:text-onyx-text3 transition-colors duration-150 focus:border-onyx-metallic/45 focus:bg-onyx-surface3 focus:outline-none"
        />
      </div>

      <div className="mt-3 max-h-[320px] overflow-y-auto">
        {loading ? (
          <OnyxLoader label="A carregar contactos" size={32} />
        ) : contacts.length === 0 ? (
          <OnyxEmptyState
            compact
            title="Sem resultados"
            description="Nenhum contacto corresponde a esta pesquisa. Usa Descobrir para encontrar identificadores novos."
          />
        ) : (
          <div className="flex flex-col gap-0.5">
            {contacts.map((contact) => (
              <OnyxContactItem
                key={contact.id}
                contact={contact}
                selected={contact.id === selectedId}
                onSelect={() => setSelectedId(contact.id)}
              />
            ))}
          </div>
        )}
      </div>
    </OnyxModal>
  );
}