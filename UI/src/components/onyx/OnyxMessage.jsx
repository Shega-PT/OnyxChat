import React from 'react';
import { Check, CheckCheck, Clock } from 'lucide-react';
import { cn } from '@/lib/utils';
import { formatTime } from '@/lib/onyx/format';

function MessageState({ state }) {
  if (state === 'pending') return <Clock className="h-3 w-3 text-onyx-text3" />;
  if (state === 'sent') return <Check className="h-3 w-3 text-onyx-text3" />;
  if (state === 'delivered') return <CheckCheck className="h-3 w-3 text-onyx-text3" />;
  return <CheckCheck className="h-3 w-3 text-onyx-info" />;
}

/** Mensagem compacta — superfícies distintas para recebida e enviada. */
export default function OnyxMessage({ message, showAuthor = false }) {
  const outgoing = message.from === 'me';

  return (
    <div className={cn('flex w-full animate-onyx-fade-up', outgoing ? 'justify-end' : 'justify-start')}>
      <div
        className={cn(
          'max-w-[min(560px,78%)] rounded-md border px-3 py-2',
          outgoing ? 'border-onyx-elevated2/70 bg-onyx-elevated2' : 'border-onyx-line bg-onyx-surface3'
        )}
      >
        {showAuthor && !outgoing && message.author && (
          <p className="mb-1 text-[10.5px] font-medium uppercase tracking-[0.12em] text-onyx-metallic2">
            {message.author}
          </p>
        )}
        <p className="whitespace-pre-wrap break-words text-[13px] leading-[1.55] text-onyx-text">{message.text}</p>
        <div className={cn('mt-1 flex items-center gap-1.5', outgoing ? 'justify-end' : 'justify-start')}>
          <span className="font-mono text-[10px] tracking-wide text-onyx-text3">{formatTime(message.at)}</span>
          {outgoing && <MessageState state={message.state} />}
        </div>
      </div>
    </div>
  );
}