import React from 'react';
import { ChevronDown } from 'lucide-react';
import { cn } from '@/lib/utils';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

/**
 * Lista suspensa nativa, com a seta desenhada à mão.
 *
 * `options` são cadeias simples: o valor apresentado e o valor enviado são
 * o mesmo. As definições cujas opções têm de ser objectos (por exemplo,
 * opções com um valor interno diferente do rótulo) ficam para a Etapa 7,
 * quando o conteúdo passar a vir do `config.json` do backend.
 */
export default function OnyxSelect({
  value,
  onChange,
  options = [],
  className = undefined,
  mono = false,
  disabled = false,
  id = undefined,
}) {
  return (
    <span className="relative inline-flex">
      <select
        id={id}
        value={value}
        onChange={onChange}
        disabled={disabled}
        className={cn(
          'h-9 min-w-[150px] appearance-none rounded-md border border-onyx-line bg-onyx-surface2 pl-3 pr-8 text-[13px] text-onyx-text transition-colors duration-150 focus:border-onyx-metallic/45 focus:bg-onyx-surface3 focus:outline-none focus:ring-2 focus:ring-onyx-metallic/15 disabled:opacity-50',
          mono && 'font-mono text-[12px] tracking-wide',
          className
        )}
      >
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-onyx-text3" />
    </span>
  );
}