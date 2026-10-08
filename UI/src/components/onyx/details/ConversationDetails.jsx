import React from 'react';
import { Ban, Bell, BellOff, ShieldCheck, Tag, User } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import OnyxPanel, { OnyxRow } from '@/components/onyx/OnyxPanel';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { formatRelative, shortFingerprint } from '@/lib/onyx/format';

/** Detalhes da conversa selecionada: identidade, sessão, verificação. */
export default function ConversationDetails({ conversation, contact }) {
  const { openModal } = useOnyxUI();
  const { toast } = useToast();

  if (!conversation) {
    return (
      <OnyxEmptyState compact title="Sem seleção" description="Escolhe uma conversa para ver a identidade e o estado da sessão." />
    );
  }

  const title = contact ? contact.name : conversation.title;

  return (
    <div>
      <div className="relative overflow-hidden border-b border-onyx-line px-3.5 py-4">
        <div className="onyx-rings pointer-events-none absolute -right-16 -top-16 h-48 w-48 rounded-full" />
        <div className="relative flex flex-col items-center text-center">
          <OnyxAvatar
            seed={contact ? contact.id : conversation.avatarSeed}
            name={title}
            size={64}
            status={contact ? contact.status : undefined}
          />
          <h3 className="mt-3 text-[13.5px] font-medium text-onyx-text">{title}</h3>
          <p className="mt-1 max-w-full truncate font-mono text-[10.5px] tracking-wide text-onyx-text3">
            {contact ? contact.identifier : `salas/${conversation.id}`}
          </p>
          <div className="mt-2.5 flex items-center gap-2">
            {contact ? <OnyxStatus status={contact.status} size="sm" /> : <OnyxStatus status="secure" size="sm" label="Grupo cifrado" />}
            {conversation.verified && (
              <OnyxBadge variant="secure">
                <ShieldCheck className="h-3 w-3" /> verificada
              </OnyxBadge>
            )}
          </div>
          <div className="mt-3.5 flex w-full items-center justify-center gap-2">
            <OnyxButton size="sm" variant="secondary" onClick={() => openModal({ type: 'identityCard', payload: { contact } })} disabled={!contact}>
              <User className="h-3.5 w-3.5" /> Identidade
            </OnyxButton>
            <OnyxButton
              size="sm"
              variant="secondary"
              onClick={() =>
                toast({
                  title: conversation.muted ? 'Notificações reativadas' : 'Conversa silenciada',
                  description: title,
                })
              }
            >
              {conversation.muted ? <Bell className="h-3.5 w-3.5" /> : <BellOff className="h-3.5 w-3.5" />}
              {conversation.muted ? 'Reativar' : 'Silenciar'}
            </OnyxButton>
          </div>
        </div>
      </div>

      <OnyxPanel label="Sessão">
        <OnyxRow label="Rota" value={conversation.route === 'direto' ? 'Ligação direta' : 'Via relay'} />
        <OnyxRow label="Latência" value={`${conversation.latency} ms`} mono />
        <OnyxRow label="Transporte" value="QUIC · UDP" mono />
        <OnyxRow label="Cifra" value="X25519 · AES-256-GCM" mono />
        <OnyxRow label="Mensagens" value={conversation.messages.length} mono />
        <OnyxRow label="Última atividade" value={formatRelative(conversation.lastAt)} />
      </OnyxPanel>

      <OnyxPanel label="Verificação">
        <div className="rounded-md border border-onyx-line bg-onyx-surface2 p-2.5">
          <div className="flex items-center gap-2">
            {conversation.verified ? (
              <ShieldCheck className="h-3.5 w-3.5 text-onyx-info" />
            ) : (
              <ShieldCheck className="h-3.5 w-3.5 text-onyx-warning" />
            )}
            <span className="text-[11.5px] text-onyx-text2">
              {conversation.verified ? 'Impressão digital confirmada' : 'Ainda não verificada'}
            </span>
          </div>
          <p className="mt-2 break-all font-mono text-[10.5px] leading-relaxed text-onyx-text3">
            {contact ? shortFingerprint(contact.fingerprint) : '—'}
          </p>
        </div>
        <OnyxButton
          className="mt-2.5 w-full"
          size="sm"
          variant="outline"
          onClick={() =>
            openModal({
              type: 'info',
              payload: {
                title: 'Verificar sessão',
                description: 'Confirma a impressão digital por um canal separado (chamada, encontro presencial ou outro dispositivo).',
                rows: [
                  { label: 'Contacto', value: title },
                  { label: 'Impressão digital', value: contact?.fingerprint || '—', mono: true },
                  { label: 'Estado atual', value: conversation.verified ? 'Verificada' : 'Não verificada' },
                ],
                note: 'Se a impressão digital não coincidir, encerra a sessão e bloqueia o contacto.',
              },
            })
          }
        >
          <ShieldCheck className="h-3.5 w-3.5" /> Comparar impressão digital
        </OnyxButton>
      </OnyxPanel>

      {contact && contact.tags?.length > 0 && (
        <OnyxPanel label="Etiquetas">
          <div className="flex flex-wrap items-center gap-1.5">
            <Tag className="h-3.5 w-3.5 text-onyx-text3" />
            {contact.tags.map((tag) => (
              <OnyxBadge key={tag}>{tag}</OnyxBadge>
            ))}
          </div>
        </OnyxPanel>
      )}

      <OnyxPanel label="Zona sensível">
        <OnyxButton
          className="w-full"
          size="sm"
          variant="danger"
          onClick={() =>
            openModal({
              type: 'confirm',
              payload: {
                title: 'Bloquear contacto',
                description: `${title} deixa de poder enviar pedidos ou mensagens. As chaves desta sessão são descartadas.`,
                confirmLabel: 'Bloquear',
                variant: 'danger',
                onConfirm: () => toast({ title: 'Contacto bloqueado', description: title }),
              },
            })
          }
        >
          <Ban className="h-3.5 w-3.5" /> Bloquear contacto
        </OnyxButton>
      </OnyxPanel>
    </div>
  );
}