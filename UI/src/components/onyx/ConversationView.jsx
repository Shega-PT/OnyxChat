import React from 'react';
import { usarToast as useToast } from '@/lib/shadcn';
import { Bell, BellOff, ChevronLeft, Lock, MoreHorizontal, ShieldCheck, Trash2, User, Ban } from 'lucide-react';
import {
  Menu as DropdownMenu,
  MenuConteudo as DropdownMenuContent,
  MenuGatilho as DropdownMenuTrigger,
  MenuItem as DropdownMenuItem,
  MenuSeparador as DropdownMenuSeparator,
} from '@/lib/shadcn';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton, { OnyxIconButton } from '@/components/onyx/OnyxButton';
import OnyxTooltip from '@/components/onyx/OnyxTooltip';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import MessageList from '@/components/onyx/MessageList';
import MessageComposer from '@/components/onyx/MessageComposer';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { truncate } from '@/lib/onyx/format';
import { cn } from '@/lib/utils';

export default function ConversationView({
  conversation,
  contact = undefined,
  onSend,
  onBack = undefined,
  className = undefined,
}) {
  const { openModal } = useOnyxUI();
  const { toast } = useToast();

  if (!conversation) {
    return (
      <div className={cn('flex min-w-0 flex-1 items-center justify-center bg-onyx-bg', className)}>
        <OnyxEmptyState
          title="Nenhuma conversa selecionada"
          description="Escolhe uma conversa na lista para veres as mensagens. Todas as sessões são cifradas ponta a ponta."
          hint="⌘K pesquisa · ⌘N nova conversa"
        />
      </div>
    );
  }

  const isGroup = !contact;
  const title = contact ? contact.name : conversation.title;

  const openSessionInfo = () => {
    openModal({
      type: 'info',
      payload: {
        title: 'Estado da sessão',
        description: `Sessão com ${title}. Cifra negociada no handshake e chaves renovadas a cada rotação.`,
        rows: [
          { label: 'Rota', value: conversation.route === 'direto' ? 'Ligação direta' : 'Via relay' },
          { label: 'Latência', value: `${conversation.latency} ms` },
          { label: 'Transporte', value: 'QUIC · UDP' },
          { label: 'Cifra', value: 'X25519 · AES-256-GCM' },
          { label: 'Impressão digital', value: contact?.fingerprint || '—', mono: true },
        ],
        note: 'Compara a impressão digital por um canal separado antes de confiar nesta sessão.',
      },
    });
  };

  return (
    <div className={cn('flex min-w-0 flex-1 flex-col bg-onyx-bg', className)}>
      <header className="flex h-[60px] shrink-0 items-center gap-3 border-b border-onyx-line bg-onyx-surface px-3 md:px-4">
        <OnyxIconButton className="lg:hidden" onClick={onBack} aria-label="Voltar à lista">
          <ChevronLeft className="h-4 w-4" />
        </OnyxIconButton>
        <OnyxAvatar
          seed={contact ? contact.id : conversation.avatarSeed}
          name={title}
          size={38}
          status={contact ? contact.status : undefined}
        />
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="truncate text-[13.5px] font-medium text-onyx-text">{title}</h2>
            {conversation.verified && (
              <OnyxBadge variant="secure" title="Sessão verificada fora do canal">
                <ShieldCheck className="h-3 w-3" />
                verificada
              </OnyxBadge>
            )}
            {isGroup && <OnyxBadge>{conversation.members} membros</OnyxBadge>}
          </div>
          <div className="mt-0.5 flex items-center gap-2">
            {contact ? <OnyxStatus status={contact.status} size="sm" /> : <OnyxStatus status="secure" size="sm" label="Grupo cifrado" />}
            <span className="hidden truncate font-mono text-[10px] tracking-wide text-onyx-text3 sm:inline">
              {contact ? contact.identifier : `salas/${conversation.id}`}
            </span>
          </div>
        </div>

        <div className="ml-auto flex items-center gap-1">
          <OnyxTooltip label="Verificar sessão">
            <OnyxIconButton onClick={openSessionInfo} aria-label="Verificar sessão">
              <ShieldCheck className="h-4 w-4" />
            </OnyxIconButton>
          </OnyxTooltip>
          <OnyxTooltip label={conversation.muted ? 'Reativar notificações' : 'Silenciar conversa'}>
            <OnyxIconButton
              onClick={() =>
                toast({
                  title: conversation.muted ? 'Notificações reativadas' : 'Conversa silenciada',
                  description: truncate(title, 40),
                })
              }
              aria-label="Silenciar"
            >
              {conversation.muted ? <BellOff className="h-4 w-4" /> : <Bell className="h-4 w-4" />}
            </OnyxIconButton>
          </OnyxTooltip>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <OnyxIconButton aria-label="Mais ações">
                <MoreHorizontal className="h-4 w-4" />
              </OnyxIconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              className="w-52 rounded-md border border-onyx-line bg-onyx-surface2 p-1 text-onyx-text2 shadow-xl shadow-black/50"
            >
              <DropdownMenuItem
                onSelect={openSessionInfo}
                className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
              >
                <Lock className="h-3.5 w-3.5" /> Detalhes da cifra
              </DropdownMenuItem>
              <DropdownMenuItem
                onSelect={() => openModal({ type: 'identityCard', payload: { contact } })}
                disabled={!contact}
                className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
              >
                <User className="h-3.5 w-3.5" /> Ver identidade
              </DropdownMenuItem>
              <DropdownMenuItem
                onSelect={() =>
                  toast({ title: 'Histórico local limpo', description: 'As mensagens no servidor mantêm-se intactas.' })
                }
                className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
              >
                <Trash2 className="h-3.5 w-3.5" /> Limpar histórico local
              </DropdownMenuItem>
              <DropdownMenuSeparator className="my-1 bg-onyx-line" />
              <DropdownMenuItem
                onSelect={() =>
                  openModal({
                    type: 'confirm',
                    payload: {
                      title: 'Bloquear contacto',
                      description: `${title} deixa de poder enviar pedidos ou mensagens. Podes desbloquear em Segurança.`,
                      confirmLabel: 'Bloquear',
                      variant: 'danger',
                      onConfirm: () => toast({ title: 'Contacto bloqueado', description: truncate(title, 40) }),
                    },
                  })
                }
                className="cursor-pointer gap-2 rounded text-[12.5px] text-onyx-error focus:bg-onyx-error/10 focus:text-onyx-error"
              >
                <Ban className="h-3.5 w-3.5" /> Bloquear
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>

      <MessageList messages={conversation.messages} isGroup={isGroup} />

      <div className="mx-auto flex w-full max-w-[860px] items-center gap-2 px-4 pb-1 pt-2">
        <OnyxButton variant="ghost" size="sm" onClick={openSessionInfo} className="text-onyx-text3">
          <Lock className="h-3.5 w-3.5" />
          {conversation.route === 'direto' ? 'Ligação direta' : 'Via relay'} · {conversation.latency} ms
        </OnyxButton>
      </div>

      <MessageComposer onSend={onSend} recipientName={contact ? contact.name : title} route={conversation.route} />
    </div>
  );
}