import React from 'react';
import { AtSign, Ban, Check, Copy, Server, ShieldCheck, UserPlus } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import OnyxPanel, { OnyxFactGrid, OnyxRow } from '@/components/onyx/OnyxPanel';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { copyToClipboard, formatDate, formatRelative, shortFingerprint } from '@/lib/onyx/format';

/** Detalhes de uma pessoa (contacto, pedido ou resultado de pesquisa). */
export default function IdentityDetails({ person, kind = 'contact' }) {
  const { openModal } = useOnyxUI();
  const { toast } = useToast();

  if (!person) return null;

  const copyIdentifier = () => {
    copyToClipboard(person.identifier || person.id)
      .then(() => toast({ title: 'Identificador copiado', description: person.identifier || person.id }))
      .catch(() => toast({ title: 'Não foi possível copiar', variant: 'destructive' }));
  };

  return (
    <div>
      <div className="relative overflow-hidden border-b border-onyx-line px-3.5 py-4">
        <div className="onyx-rings pointer-events-none absolute -left-14 -top-14 h-44 w-44 rounded-full" />
        <div className="relative flex flex-col items-center text-center">
          <OnyxAvatar seed={person.id} name={person.name} size={60} status={person.status} />
          <h3 className="mt-3 text-[13.5px] font-medium text-onyx-text">{person.name}</h3>
          <button
            type="button"
            onClick={copyIdentifier}
            className="mt-1 inline-flex max-w-full items-center gap-1.5 rounded px-1.5 py-0.5 font-mono text-[10.5px] tracking-wide text-onyx-text3 transition-colors hover:bg-onyx-surface3 hover:text-onyx-text2"
          >
            <span className="truncate">{person.identifier || person.id}</span>
            <Copy className="h-3 w-3 shrink-0" />
          </button>
          <div className="mt-2.5 flex flex-wrap items-center justify-center gap-2">
            <OnyxStatus status={person.status} size="sm" />
            {person.service && (
              <OnyxBadge>
                <Server className="h-3 w-3" /> serviço
              </OnyxBadge>
            )}
            {person.verified ? (
              <OnyxBadge variant="secure">
                <ShieldCheck className="h-3 w-3" /> verificado
              </OnyxBadge>
            ) : (
              <OnyxBadge variant="warning">por verificar</OnyxBadge>
            )}
          </div>
        </div>
      </div>

      <OnyxPanel label="Identidade">
        <OnyxFactGrid
          items={[
            { label: 'Contactos comuns', value: person.mutual ?? 0 },
            { label: 'Função', value: person.role || '—' },
            { label: 'Adicionado em', value: kind === 'contact' ? formatDate(person.addedAt, { day: '2-digit', month: 'short', year: 'numeric' }) : '—' },
            { label: 'Última atividade', value: formatRelative(person.lastSeen || person.at) },
          ]}
        />
        <div className="mt-3 border-t border-onyx-line pt-1">
          <OnyxRow
            label="Impressão digital"
            value={shortFingerprint(person.fingerprint)}
            mono
            action={
              <Copy
                className="h-3.5 w-3.5 shrink-0 cursor-pointer text-onyx-text3 hover:text-onyx-text"
                onClick={copyIdentifier}
              />
            }
          />
          {person.note && <OnyxRow label="Nota" value={person.note} />}
        </div>
      </OnyxPanel>

      <OnyxPanel label="Ações">
        <div className="space-y-2">
          <OnyxButton
            className="w-full"
            size="sm"
            variant="primary"
            onClick={() =>
              toast({
                title: kind === 'request' ? 'Pedido aceite' : 'Pedido enviado',
                description: kind === 'request' ? `${person.name} foi adicionado aos contactos.` : `Aguardas resposta de ${person.name}.`,
              })
            }
          >
            {kind === 'request' ? <Check className="h-3.5 w-3.5" /> : <UserPlus className="h-3.5 w-3.5" />}
            {kind === 'request' ? 'Aceitar pedido' : 'Enviar pedido de contacto'}
          </OnyxButton>
          <OnyxButton
            className="w-full"
            size="sm"
            variant="secondary"
            onClick={() => openModal({ type: 'identityCard', payload: { contact: person } })}
          >
            <AtSign className="h-3.5 w-3.5" /> Ver cartão de identidade
          </OnyxButton>
          <OnyxButton
            className="w-full"
            size="sm"
            variant="danger"
            onClick={() =>
              openModal({
                type: 'confirm',
                payload: {
                  title: 'Bloquear identificador',
                  description: `${person.name} não poderá voltar a enviar pedidos nem mensagens.`,
                  confirmLabel: 'Bloquear',
                  variant: 'danger',
                  onConfirm: () => toast({ title: 'Identificador bloqueado', description: person.name }),
                },
              })
            }
          >
            <Ban className="h-3.5 w-3.5" /> Bloquear
          </OnyxButton>
        </div>
      </OnyxPanel>
    </div>
  );
}