import React from 'react';
import { Server, ShieldCheck } from 'lucide-react';
import { cn } from '@/lib/utils';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import { truncate } from '@/lib/onyx/format';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

/**
 * Linha de contacto compacta — usada em Contactos, Pedidos e Nova conversa.
 *
 * Sem `onSelect` o componente desenha um `div` em vez de um `button`:
 * nas listas puramente informativas não há acção a anunciar, e um botão
 * sem comportamento seria um alvo falso para o teclado e para os leitores
 * de ecrã.
 *
 * `meta` e `actions` são slots de conteúdo. Passados eles, a nota do
 * contacto e o indicador de estado deixam de ser desenhados — são as três
 * formas alternativas da mesma linha, não camadas empilhadas.
 */
export default function OnyxContactItem({
  contact,
  selected = false,
  onSelect = undefined,
  meta = undefined,
  actions = undefined,
  disabled = false,
}) {
  const Component = onSelect ? 'button' : 'div';

  return (
    <Component
      type={onSelect ? 'button' : undefined}
      onClick={onSelect}
      disabled={disabled}
      className={cn(
        'flex w-full items-start gap-2.5 rounded-md border border-transparent px-2.5 py-2 text-left transition-colors duration-150',
        onSelect && (selected ? 'border-onyx-line bg-onyx-elevated/60' : 'hover:bg-onyx-surface3/70'),
        disabled && 'cursor-not-allowed opacity-45'
      )}
    >
      <OnyxAvatar seed={contact.id} name={contact.name} size={34} status={contact.status} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span className="truncate text-[12.5px] font-medium text-onyx-text">{contact.name}</span>
          {contact.service && <Server className="h-3 w-3 shrink-0 text-onyx-text3" aria-label="Nó de serviço" />}
          {contact.verified && <ShieldCheck className="h-3.5 w-3.5 shrink-0 text-onyx-info/80" aria-label="Verificado" />}
        </div>
        <div className="mt-0.5 flex items-center gap-2">
          <span className="truncate font-mono text-[10.5px] tracking-wide text-onyx-text3">
            {contact.identifier || contact.id}
          </span>
        </div>
        {meta ? (
          <div className="mt-1 flex items-center gap-2">{meta}</div>
        ) : (
          contact.note && (
            <p className="mt-0.5 truncate text-[11.5px] text-onyx-text3">{truncate(contact.note, 52)}</p>
          )
        )}
      </div>
      {actions ? (
        <div className="flex shrink-0 items-center gap-1">{actions}</div>
      ) : (
        <OnyxStatus status={contact.status} showLabel={false} className="mt-1" />
      )}
    </Component>
  );
}