import React from 'react';
import { cn } from '@/lib/utils';
import OnyxModal from '@/components/onyx/OnyxModal';
import OnyxButton from '@/components/onyx/OnyxButton';

/** Modal informativo genérico — detalhes técnicos e mensagens do sistema. */
export default function InfoModal({ open, onClose, payload }) {
  const rows = payload?.rows || [];

  return (
    <OnyxModal
      open={open}
      onOpenChange={(value) => !value && onClose()}
      title={payload?.title || 'Informação'}
      description={payload?.description}
      footer={
        <OnyxButton variant="secondary" onClick={onClose}>
          Fechar
        </OnyxButton>
      }
      size="md"
    >
      {rows.length > 0 && (
        <div className="divide-y divide-onyx-line overflow-hidden rounded-md border border-onyx-line bg-onyx-surface">
          {rows.map((row) => (
            <div key={row.label} className="flex items-start justify-between gap-4 px-3 py-2">
              <span className="text-[11.5px] leading-5 text-onyx-text3">{row.label}</span>
              <span
                className={cn(
                  'max-w-[62%] break-words text-right text-[12px] leading-5 text-onyx-text',
                  row.mono && 'font-mono text-[10.5px] leading-5 tracking-wide text-onyx-text2'
                )}
              >
                {row.value}
              </span>
            </div>
          ))}
        </div>
      )}
      {payload?.note && (
        <p className="mt-3 rounded-md border border-onyx-info/25 bg-onyx-info/5 px-3 py-2 text-[11.5px] leading-relaxed text-onyx-info">
          {payload.note}
        </p>
      )}
    </OnyxModal>
  );
}