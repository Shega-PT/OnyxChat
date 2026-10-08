import React, { useEffect, useState } from 'react';
import { Ban, Check, Clock, X } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import { cn } from '@/lib/utils';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel, useDetails, useOnyxUI } from '@/lib/onyx/onyx-context';
import PageHeader from '@/components/onyx/PageHeader';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import IdentityDetails from '@/components/onyx/details/IdentityDetails';
import { formatRelative } from '@/lib/onyx/format';

const TABS = [
  { id: 'pendentes', label: 'Pendentes' },
  { id: 'historico', label: 'Histórico' },
];

export default function Requests() {
  const { loading, data, reload } = useAsync(() => dataAdapter.listRequests(), []);
  const [decisions, setDecisions] = useState({});
  const [tab, setTab] = useState('pendentes');
  const [focusedId, setFocusedId] = useState(null);
  const { openModal, refreshCounts } = useOnyxUI();
  const { toast } = useToast();

  const requests = (data || []).map((request) => ({ ...request, decision: decisions[request.id] || null }));
  const pending = requests.filter((request) => !request.decision);
  const history = requests.filter((request) => request.decision);
  const visible = tab === 'pendentes' ? pending : history;

  useEffect(() => {
    if (!visible.length) {
      setFocusedId(null);
      return;
    }
    if (!focusedId || !visible.some((request) => request.id === focusedId)) setFocusedId(visible[0].id);
  }, [visible, focusedId]);

  const focused = requests.find((request) => request.id === focusedId) || null;

  useContextLabel(focused?.name || null, [focused?.name]);
  useDetails(<IdentityDetails person={focused} kind="request" />, [focused?.id]);

  const decide = async (request, decision) => {
    await dataAdapter.decideRequest(request.id, decision);
    setDecisions((current) => ({ ...current, [request.id]: decision }));
    refreshCounts();
    toast({
      title: decision === 'aceite' ? 'Pedido aceite' : decision === 'recusado' ? 'Pedido recusado' : 'Identificador bloqueado',
      description: request.name,
    });
  };

  const renderCard = (request) => {
    const focusedCard = request.id === focusedId;
    return (
      <article
        key={request.id}
        onMouseEnter={() => setFocusedId(request.id)}
        className={cn(
          'rounded-lg border bg-onyx-surface2 p-3.5 transition-colors duration-150',
          focusedCard ? 'border-onyx-metallic/30 bg-onyx-surface3/70' : 'border-onyx-line'
        )}
      >
        <div className="flex items-start gap-3">
          <OnyxAvatar seed={request.personId} name={request.name} size={38} status={request.status} />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-[12.5px] font-medium text-onyx-text">{request.name}</span>
              {request.service && <OnyxBadge>serviço</OnyxBadge>}
              <span className="ml-auto font-mono text-[10px] tracking-wide text-onyx-text3">
                {formatRelative(request.at)}
              </span>
            </div>
            <p className="mt-0.5 truncate font-mono text-[10.5px] tracking-wide text-onyx-text3">{request.identifier}</p>
            {request.message && (
              <p className="mt-2 rounded-md border border-onyx-line bg-onyx-surface px-2.5 py-2 text-[12px] leading-relaxed text-onyx-text2">
                {request.message}
              </p>
            )}
            <div className="mt-2.5 flex flex-wrap items-center gap-2">
              <span className="flex items-center gap-1.5 font-mono text-[10px] tracking-wide text-onyx-text3">
                <Clock className="h-3 w-3" /> {request.mutual} contactos comuns
              </span>
              <div className="ml-auto flex items-center gap-1.5">
                {request.decision ? (
                  <OnyxBadge variant={request.decision === 'aceite' ? 'success' : request.decision === 'recusado' ? 'neutral' : 'error'}>
                    {request.decision}
                  </OnyxBadge>
                ) : (
                  <>
                    <OnyxButton size="sm" variant="primary" onClick={() => decide(request, 'aceite')}>
                      <Check className="h-3.5 w-3.5" /> Aceitar
                    </OnyxButton>
                    <OnyxButton size="sm" variant="secondary" onClick={() => decide(request, 'recusado')}>
                      <X className="h-3.5 w-3.5" /> Recusar
                    </OnyxButton>
                    <OnyxButton
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        openModal({
                          type: 'confirm',
                          payload: {
                            title: 'Bloquear identificador',
                            description: `${request.name} fica impedido de voltar a enviar pedidos.`,
                            confirmLabel: 'Bloquear',
                            variant: 'danger',
                            onConfirm: () => decide(request, 'bloqueado'),
                          },
                        })
                      }
                      aria-label="Bloquear"
                    >
                      <Ban className="h-3.5 w-3.5" />
                    </OnyxButton>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>
      </article>
    );
  };

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        title="Pedidos"
        description="Pedidos de contacto pendentes de decisão."
        meta={
          <>
            <OnyxBadge variant={pending.length ? 'warning' : 'success'} mono>
              {pending.length} pendentes
            </OnyxBadge>
            <OnyxBadge mono>{history.length} decididos</OnyxBadge>
          </>
        }
        actions={
          <div className="flex items-center gap-0.5">
            {TABS.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => setTab(item.id)}
                className={cn(
                  'h-7 rounded px-2 text-[11.5px] transition-colors duration-150',
                  tab === item.id ? 'bg-onyx-elevated text-onyx-text' : 'text-onyx-text3 hover:bg-onyx-surface3 hover:text-onyx-text2'
                )}
              >
                {item.label}
              </button>
            ))}
          </div>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        {loading ? (
          <OnyxLoader label="A carregar pedidos" />
        ) : visible.length === 0 ? (
          <OnyxEmptyState
            title={tab === 'pendentes' ? 'Sem pedidos pendentes' : 'Ainda sem histórico'}
            description={
              tab === 'pendentes'
                ? 'Quando alguém enviar um pedido de contacto, aparece aqui com o identificador e as ligações em comum.'
                : 'Os pedidos que aceitares, recusares ou bloqueares ficam registados nesta lista.'
            }
            action={
              tab === 'pendentes' ? (
                <OnyxButton size="sm" variant="secondary" onClick={reload}>
                  Verificar novamente
                </OnyxButton>
              ) : null
            }
            hint={tab === 'pendentes' ? 'Nada é aceite sem a tua confirmação' : undefined}
          />
        ) : (
          <div className="mx-auto flex max-w-[720px] flex-col gap-2.5">{visible.map(renderCard)}</div>
        )}
      </div>
    </div>
  );
}