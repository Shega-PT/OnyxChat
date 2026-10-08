import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { cn } from '@/lib/utils';
import OnyxPanel, { OnyxFactGrid, OnyxRow } from '@/components/onyx/OnyxPanel';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';

/** Painel técnico genérico — usado por Rede, Segurança, Definições e Identidade. */
export default function TechnicalDetails({
  title,
  subtitle = undefined,
  rows = [],
  facts = [],
  advanced = [],
  note = undefined,
  children = undefined,
}) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  if (!title && rows.length === 0 && facts.length === 0) {
    return <OnyxEmptyState compact title="Sem informação" description="Esta vista não expõe detalhes adicionais." />;
  }

  return (
    <div>
      <OnyxPanel label={title} className="pt-4">
        {subtitle && <p className="mb-2.5 text-[11.5px] leading-relaxed text-onyx-text2">{subtitle}</p>}
        {facts.length > 0 && <OnyxFactGrid items={facts} />}
        {rows.length > 0 && (
          <div className={cn(facts.length > 0 && 'mt-3 border-t border-onyx-line pt-1')}>
            {rows.map((row) => (
              <OnyxRow key={row.label} label={row.label} value={row.value} mono={row.mono} state={row.state} />
            ))}
          </div>
        )}
        {children}
      </OnyxPanel>

      {advanced.length > 0 && (
        <OnyxPanel label="Avançado">
          <button
            type="button"
            onClick={() => setShowAdvanced((value) => !value)}
            className="flex w-full items-center gap-2 rounded-md border border-onyx-line bg-onyx-surface2 px-2.5 py-2 text-[11.5px] text-onyx-text2 transition-colors duration-150 hover:border-onyx-metallic/30 hover:bg-onyx-surface3 hover:text-onyx-text"
          >
            {showAdvanced ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
            {showAdvanced ? 'Ocultar parâmetros' : 'Mostrar parâmetros técnicos'}
          </button>
          {showAdvanced && (
            <div className="mt-2 animate-onyx-fade rounded-md border border-onyx-line bg-onyx-surface2 px-2.5 py-2">
              {advanced.map((row) => (
                <OnyxRow key={row.label} label={row.label} value={row.value} mono={row.mono ?? true} />
              ))}
            </div>
          )}
        </OnyxPanel>
      )}

      {note && (
        <OnyxPanel label="Nota">
          <p className="text-[11.5px] leading-relaxed text-onyx-text3">{note}</p>
        </OnyxPanel>
      )}
    </div>
  );
}