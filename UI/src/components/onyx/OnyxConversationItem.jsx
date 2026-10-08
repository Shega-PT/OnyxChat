import React from 'react';
import { BellOff, ShieldCheck, Users } from 'lucide-react';
import { cn } from '@/lib/utils';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import { formatRelative, truncate } from '@/lib/onyx/format';

/** Linha de conversa compacta com estados selecionado, não selecionado e novo. */
export default function OnyxConversationItem({ conversation, selected, onSelect }) {
  const isGroup = !conversation.contact;
  const unread = conversation.unread > 0;
  const preview = `${conversation.lastFromMe ? 'Tu: ' : ''}${conversation.lastMessage}`;

  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        'flex w-full items-start gap-2.5 rounded-md border border-transparent px-2.5 py-2 text-left transition-colors duration-150',
        selected ? 'border-onyx-line bg-onyx-elevated/60' : 'hover:bg-onyx-surface3/70'
      )}
    >
      <OnyxAvatar
        seed={conversation.contact ? conversation.contact.id : conversation.avatarSeed}
        name={conversation.title}
        size={36}
        status={isGroup ? undefined : conversation.contact.status}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span className={cn('truncate text-[12.5px] text-onyx-text', unread ? 'font-semibold' : 'font-medium')}>
            {conversation.title}
          </span>
          {isGroup && <Users className="h-3 w-3 shrink-0 text-onyx-text3" />}
          {conversation.verified && !isGroup && (
            <ShieldCheck className="h-3.5 w-3.5 shrink-0 text-onyx-info/80" aria-label="Sessão verificada" />
          )}
          {conversation.pinned && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-onyx-metallic/70" />}
          <span className="ml-auto shrink-0 font-mono text-[10px] tracking-wide text-onyx-text3">
            {formatRelative(conversation.lastAt)}
          </span>
        </div>
        <div className="mt-0.5 flex items-center gap-1.5">
          <span className={cn('truncate text-[11.5px]', unread ? 'text-onyx-text2' : 'text-onyx-text3')}>
            {truncate(preview, 60)}
          </span>
          {conversation.muted && <BellOff className="h-3 w-3 shrink-0 text-onyx-text3" />}
          {unread && (
            <span className="ml-auto shrink-0 rounded border border-onyx-info/30 bg-onyx-info/15 px-1.5 font-mono text-[10px] leading-4 text-onyx-info">
              {conversation.unread}
            </span>
          )}
        </div>
      </div>
    </button>
  );
}