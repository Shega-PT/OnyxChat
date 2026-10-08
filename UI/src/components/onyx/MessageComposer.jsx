import React, { useEffect, useRef, useState } from 'react';
import { Lock, Send } from 'lucide-react';
import { cn } from '@/lib/utils';
import { OnyxKbd } from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';

export default function MessageComposer({
  onSend,
  recipientName = undefined,
  route = 'direto',
  disabled = false,
}) {
  const [value, setValue] = useState('');
  const [focused, setFocused] = useState(false);
  const textareaRef = useRef(null);

  useEffect(() => {
    const element = textareaRef.current;
    if (!element) return;
    element.style.height = 'auto';
    element.style.height = `${Math.min(element.scrollHeight, 140)}px`;
  }, [value]);

  const submit = () => {
    const text = value.trim();
    if (!text || disabled) return;
    onSend(text);
    setValue('');
  };

  const handleKeyDown = (event) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  };

  return (
    <div className="shrink-0 border-t border-onyx-line bg-onyx-surface px-3 pb-2.5 pt-3">
      <div
        className={cn(
          'flex items-end gap-2 rounded-lg border bg-onyx-surface2 px-2.5 py-2 transition-colors duration-150',
          focused ? 'border-onyx-metallic/40 ring-2 ring-onyx-metallic/10' : 'border-onyx-line'
        )}
      >
        <textarea
          ref={textareaRef}
          rows={1}
          value={value}
          disabled={disabled}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          placeholder={recipientName ? `Mensagem cifrada para ${recipientName}` : 'Mensagem cifrada'}
          className="max-h-[140px] min-h-[26px] flex-1 resize-none bg-transparent py-1 text-[13.5px] leading-[1.5] text-onyx-text placeholder:text-onyx-text3 focus:outline-none disabled:opacity-50"
        />
        <OnyxButton
          variant="primary"
          size="icon"
          onClick={submit}
          disabled={!value.trim() || disabled}
          aria-label="Enviar mensagem"
        >
          <Send className="h-4 w-4" />
        </OnyxButton>
      </div>
      <div className="mt-2 flex items-center gap-3 px-1">
        <span className="flex items-center gap-1.5">
          <Lock className="h-3 w-3 text-onyx-info/70" />
          <span className="text-[10.5px] text-onyx-text3">
            Cifrado ponta a ponta · {route === 'direto' ? 'rota direta' : 'via relay'}
          </span>
        </span>
        <span className="ml-auto hidden items-center gap-2 text-[10.5px] text-onyx-text3 sm:flex">
          <OnyxKbd>Enter</OnyxKbd> enviar
          <OnyxKbd>Shift + Enter</OnyxKbd> nova linha
        </span>
      </div>
    </div>
  );
}