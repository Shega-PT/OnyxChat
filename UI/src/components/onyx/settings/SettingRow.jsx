import React from 'react';
import { Copy, ExternalLink } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import { cn } from '@/lib/utils';
import OnyxButton, { OnyxIconButton } from '@/components/onyx/OnyxButton';
import OnyxSwitch from '@/components/onyx/OnyxSwitch';
import OnyxSelect from '@/components/onyx/OnyxSelect';
import OnyxInput from '@/components/onyx/OnyxInput';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import { copyToClipboard } from '@/lib/onyx/format';

/** Invólucro de uma linha de definição — rótulo à esquerda, controlos à direita. */
function RowShell({ label, hint = undefined, badge = undefined, children, className = undefined }) {
  return (
    <div className={cn('flex flex-wrap items-center gap-3 border-b border-onyx-line py-3 last:border-b-0', className)}>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-[12.5px] text-onyx-text">{label}</span>
          {badge && <OnyxBadge variant="outline">{badge}</OnyxBadge>}
        </div>
        {hint && <p className="mt-1 max-w-[520px] text-[11px] leading-relaxed text-onyx-text3">{hint}</p>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

/** Linha de definição — switch, select, texto, leitura ou ação. */
export default function SettingRow({ item, value, onChange, onAction }) {
  const { toast } = useToast();

  if (item.type === 'switch') {
    return (
      <RowShell label={item.label} hint={item.hint} badge={item.badge}>
        <OnyxSwitch checked={Boolean(value)} onCheckedChange={(checked) => onChange(checked)} />
      </RowShell>
    );
  }

  if (item.type === 'select') {
    return (
      <RowShell label={item.label} hint={item.hint} badge={item.badge}>
        <OnyxSelect value={value} options={item.options} onChange={(event) => onChange(event.target.value)} />
      </RowShell>
    );
  }

  if (item.type === 'text') {
    return (
      <RowShell label={item.label} hint={item.hint} badge={item.badge}>
        <OnyxInput
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className="w-[240px]"
          inputClassName="h-8 text-[12.5px]"
        />
      </RowShell>
    );
  }

  if (item.type === 'readonly') {
    return (
      <RowShell label={item.label} hint={item.hint} badge={item.badge}>
        <span className="rounded-md border border-onyx-line bg-onyx-surface3 px-2.5 py-1.5 font-mono text-[11px] tracking-wide text-onyx-text2">
          {value}
        </span>
        {item.action === 'copy' && (
          <OnyxIconButton
            aria-label="Copiar"
            onClick={() =>
              copyToClipboard(value)
                .then(() => toast({ title: 'Copiado', description: value }))
                .catch(() => toast({ title: 'Não foi possível copiar', variant: 'destructive' }))
            }
          >
            <Copy className="h-3.5 w-3.5" />
          </OnyxIconButton>
        )}
      </RowShell>
    );
  }

  return (
    <RowShell label={item.label} hint={item.hint} badge={item.badge}>
      <OnyxButton size="sm" variant={item.danger ? 'danger' : 'secondary'} onClick={() => onAction(item)}>
        <ExternalLink className="h-3.5 w-3.5" />
        {item.actionLabel || 'Executar'}
      </OnyxButton>
    </RowShell>
  );
}