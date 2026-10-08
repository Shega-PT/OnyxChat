import React, { useEffect, useState } from 'react';
import { MessageSquare, Plus, Search, UserPlus } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { usarToast as useToast } from '@/lib/shadcn';
import { cn } from '@/lib/utils';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel, useDetails, useOnyxUI } from '@/lib/onyx/onyx-context';
import PageHeader from '@/components/onyx/PageHeader';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxContactItem from '@/components/onyx/OnyxContactItem';
import OnyxContextMenu from '@/components/onyx/OnyxContextMenu';
import ContactContextItems from '@/components/onyx/ContactContextItems';
import { copyToClipboard } from '@/lib/onyx/format';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import IdentityDetails from '@/components/onyx/details/IdentityDetails';
import { OnyxFactGrid, OnyxRow } from '@/components/onyx/OnyxPanel';
import { formatDate, formatRelative, shortFingerprint } from '@/lib/onyx/format';

export default function Contacts() {
  const [query, setQuery] = useState('');
  const [selectedId, setSelectedId] = useState(null);
  const [mobileView, setMobileView] = useState('lista');
  const { loading, data } = useAsync(() => dataAdapter.listContacts({ query }), [query]);
  const { openModal } = useOnyxUI();
  const { toast } = useToast();
  const navigate = useNavigate();

  const contacts = data || [];

  useEffect(() => {
    if (!contacts.length) {
      setSelectedId(null);
      return;
    }
    if (!selectedId || !contacts.some((contact) => contact.id === selectedId)) setSelectedId(contacts[0].id);
  }, [contacts, selectedId]);

  const selected = contacts.find((contact) => contact.id === selectedId) || null;

  /**
   * Copia o identificador de um contacto a partir do menu de contexto.
   *
   * Fica aqui, e não dentro de `ContactContextItems`, pelo mesmo motivo que
   * a acção equivalente está em `ConversationList`: o componente de menu
   * desenha o item, a vista decide o que ele faz.
   */
  const copiarIdentificador = (contact) => {
    const valor = contact.identifier || contact.id;
    copyToClipboard(valor)
      .then(() => toast({ title: 'Identificador copiado', description: valor }))
      .catch(() => toast({ title: 'Não foi possível copiar', variant: 'destructive' }));
  };

  useContextLabel(selected?.name || null, [selected?.name]);
  useDetails(<IdentityDetails person={selected} kind="contact" />, [selected?.id]);

  return (
    <div className="flex min-w-0 flex-1">
      <div
        className={cn(
          'flex min-h-0 w-full flex-col border-onyx-line bg-onyx-surface lg:flex lg:w-[320px] lg:border-r',
          mobileView === 'detalhe' && 'hidden lg:flex'
        )}
      >
        <PageHeader
          title="Contactos"
          description={`${contacts.length} identificadores em cache`}
          actions={
            <OnyxButton size="sm" variant="secondary" onClick={() => openModal({ type: 'addContact' })}>
              <Plus className="h-3.5 w-3.5" /> Adicionar
            </OnyxButton>
          }
        />
        <div className="shrink-0 border-b border-onyx-line px-3 py-2.5">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-onyx-text3" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Nome, identificador ou etiqueta"
              className="h-8 w-full rounded-md border border-onyx-line bg-onyx-surface2 pl-8 pr-2.5 text-[12.5px] text-onyx-text placeholder:text-onyx-text3 transition-colors duration-150 focus:border-onyx-metallic/45 focus:bg-onyx-surface3 focus:outline-none"
            />
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {loading ? (
            <OnyxLoader label="A carregar contactos" />
          ) : contacts.length === 0 ? (
            <OnyxEmptyState
              compact
              title="Nenhum contacto"
              description="Nada corresponde a esta pesquisa. Podes descobrir identificadores novos."
              action={
                <OnyxButton size="sm" variant="secondary" onClick={() => navigate('/descobrir')}>
                  Descobrir identificador
                </OnyxButton>
              }
            />
          ) : (
            <div className="flex flex-col gap-0.5">
              {contacts.map((contact) => (
                <OnyxContextMenu
                  key={contact.id}
                  itens={
                    <ContactContextItems
                      contact={contact}
                      onAbrirConversa={(c) => {
                        setSelectedId(c.id);
                        navigate('/');
                      }}
                      onVerIdentidade={(c) => openModal({ type: 'identityCard', payload: { contact: c } })}
                      onCopiarIdentificador={copiarIdentificador}
                      onPedirVerificacao={(c) =>
                        toast({
                          title: 'Pedido de verificação enviado',
                          description: `Compara a impressão digital de ${c.name} por um canal separado.`,
                        })
                      }
                      onBloquear={(c) =>
                        openModal({
                          type: 'confirm',
                          payload: {
                            title: 'Bloquear contacto',
                            description: `${c.name} deixa de poder enviar pedidos nem mensagens.`,
                            confirmLabel: 'Bloquear',
                            variant: 'danger',
                            onConfirm: () =>
                              toast({ title: 'Contacto bloqueado', description: c.name }),
                          },
                        })
                      }
                    />
                  }
                >
                  <OnyxContactItem
                    contact={contact}
                    selected={contact.id === selectedId}
                    onSelect={() => {
                      setSelectedId(contact.id);
                      setMobileView('detalhe');
                    }}
                  />
                </OnyxContextMenu>
              ))}
            </div>
          )}
        </div>
      </div>

      <div
        className={cn(
          'flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto bg-onyx-bg lg:flex',
          mobileView === 'lista' && 'hidden lg:flex'
        )}
      >
        {!selected ? (
          <div className="flex flex-1 items-center justify-center">
            <OnyxEmptyState
              title="Seleciona um contacto"
              description="Aqui vês a identidade completa, as verificações e o histórico de sessões."
            />
          </div>
        ) : (
          <>
            <div className="relative overflow-hidden border-b border-onyx-line bg-onyx-surface px-5 py-5">
              <div className="onyx-rings pointer-events-none absolute -right-24 -top-24 h-72 w-72 rounded-full" />
              <div className="relative flex flex-wrap items-center gap-4">
                <OnyxAvatar seed={selected.id} name={selected.name} size={72} status={selected.status} />
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <h2 className="text-[15px] font-medium text-onyx-text">{selected.name}</h2>
                    {selected.verified ? (
                      <OnyxBadge variant="secure">verificado</OnyxBadge>
                    ) : (
                      <OnyxBadge variant="warning">por verificar</OnyxBadge>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => openModal({ type: 'identityCard', payload: { contact: selected } })}
                    className="mt-1 font-mono text-[10.5px] tracking-wide text-onyx-text3 transition-colors hover:text-onyx-text2"
                  >
                    {selected.identifier}
                  </button>
                  <div className="mt-2 flex items-center gap-3">
                    <OnyxStatus status={selected.status} size="sm" />
                    <span className="font-mono text-[10px] tracking-wide text-onyx-text3">
                      visto {formatRelative(selected.lastSeen)}
                    </span>
                  </div>
                </div>
                <div className="ml-auto flex flex-wrap items-center gap-2">
                  <OnyxButton size="sm" variant="primary" onClick={() => navigate('/')}>
                    <MessageSquare className="h-3.5 w-3.5" /> Abrir conversa
                  </OnyxButton>
                  <OnyxButton size="sm" variant="secondary" onClick={() => openModal({ type: 'identityCard', payload: { contact: selected } })}>
                    <UserPlus className="h-3.5 w-3.5" /> Cartão
                  </OnyxButton>
                </div>
              </div>
            </div>

            <div className="space-y-4 p-5">
              <section className="rounded-lg border border-onyx-line bg-onyx-surface2 p-4">
                <p className="onyx-label mb-3">Informação associada</p>
                <OnyxFactGrid
                  items={[
                    { label: 'Função', value: selected.role || '—' },
                    { label: 'Adicionado em', value: formatDate(selected.addedAt, { day: '2-digit', month: 'short', year: 'numeric' }) },
                    { label: 'Contactos comuns', value: selected.mutual ?? 0 },
                    { label: 'Etiquetas', value: selected.tags?.length ? selected.tags.join(' · ') : '—' },
                  ]}
                />
                <div className="mt-3 border-t border-onyx-line pt-1">
                  <OnyxRow label="Impressão digital" value={shortFingerprint(selected.fingerprint)} mono />
                  <OnyxRow label="Nota" value={selected.note || '—'} />
                </div>
              </section>

              <section className="rounded-lg border border-onyx-line bg-onyx-surface2 p-4">
                <p className="onyx-label mb-3">Verificações e sessões</p>
                <OnyxRow label="Estado da verificação" value={selected.verified ? 'Confirmada' : 'Pendente'} state={selected.verified ? 'ok' : 'warn'} />
                <OnyxRow label="Sessões registadas" value={selected.mutual ? `${selected.mutual + 2} nos últimos 30 dias` : 'Nenhuma'} />
                <OnyxRow label="Rotação de chaves" value={selected.verified ? 'Alinhada com o nó local' : 'Aguardando confirmação'} state={selected.verified ? 'ok' : 'info'} />
              </section>

              <div className="flex flex-wrap items-center gap-2">
                <OnyxButton
                  size="sm"
                  variant="secondary"
                  onClick={() =>
                    toast({ title: 'Pedido de verificação enviado', description: `Compara a impressão digital de ${selected.name}.` })
                  }
                >
                  Pedir verificação
                </OnyxButton>
                <OnyxButton size="sm" variant="ghost" onClick={() => navigate('/descobrir')}>
                  Descobrir outro identificador
                </OnyxButton>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}