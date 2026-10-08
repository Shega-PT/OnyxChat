import React, { useEffect, useState } from 'react';
import { RotateCcw, Save } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import { cn } from '@/lib/utils';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel, useDetails, useOnyxUI } from '@/lib/onyx/onyx-context';
import PageHeader from '@/components/onyx/PageHeader';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import TechnicalDetails from '@/components/onyx/details/TechnicalDetails';
import SettingRow from '@/components/onyx/settings/SettingRow';

export default function Settings() {
  const { loading, data } = useAsync(() => dataAdapter.getSettings(), []);
  const [draft, setDraft] = useState(null);
  const [baseline, setBaseline] = useState(null);
  const [activeSection, setActiveSection] = useState('geral');
  const [saving, setSaving] = useState(false);
  const { openModal } = useOnyxUI();
  const { toast } = useToast();

  useEffect(() => {
    if (!data) return;
    const clone = JSON.parse(JSON.stringify(data));
    setDraft(clone);
    setBaseline(clone);
  }, [data]);

  useContextLabel('Preferências locais', []);
  useDetails(
    <TechnicalDetails
      title="Sobre a aplicação"
      subtitle="OnyxChat · cliente de rede"
      rows={[
        { label: 'Versão', value: '4.2.0', mono: true },
        { label: 'Compilação', value: '2026.10.06', mono: true },
        { label: 'Protocolo', value: 'ONYX-SP 4.2', mono: true },
        { label: 'Idioma', value: 'Português (PT)' },
      ]}
      advanced={[
        { label: 'Pasta de dados', value: '~/onyx/state' },
        { label: 'Registo', value: '32 MB' },
        { label: 'Verificação de integridade', value: 'ok' },
      ]}
      note="As preferências são guardadas localmente e sincronizadas entre os teus dispositivos autorizados."
    />,
    []
  );

  const update = (sectionId, itemId, value) => {
    setDraft((current) =>
      current.map((section) =>
        section.id !== sectionId
          ? section
          : {
              ...section,
              items: section.items.map((item) => (item.id === itemId ? { ...item, value } : item)),
            }
      )
    );
  };

  const dirty = draft && baseline && JSON.stringify(draft) !== JSON.stringify(baseline);
  const section = draft?.find((item) => item.id === activeSection);

  const save = async () => {
    setSaving(true);
    try {
      await dataAdapter.saveSettings();
      setBaseline(JSON.parse(JSON.stringify(draft)));
      toast({ title: 'Definições guardadas', description: 'As alterações ficam ativas de imediato.' });
    } finally {
      setSaving(false);
    }
  };

  const handleAction = (item) => {
    if (item.danger) {
      openModal({
        type: 'confirm',
        payload: {
          title: item.label,
          description: 'As sessões ativas são interrompidas durante alguns segundos.',
          confirmLabel: item.actionLabel || 'Confirmar',
          variant: 'danger',
          onConfirm: () => toast({ title: 'Ação executada', description: item.label }),
        },
      });
      return;
    }
    toast({ title: 'Ação iniciada', description: item.label });
  };

  if (loading || !draft) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <PageHeader title="Definições" description="Preferências da aplicação." />
        <OnyxLoader label="A carregar definições" />
      </div>
    );
  }

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        title="Definições"
        description="Geral, aparência, privacidade, identidade, rede, segurança, notificações e avançado."
        meta={dirty ? <span className="font-mono text-[10.5px] tracking-wide text-onyx-warning">alterações por guardar</span> : null}
      />

      <div className="flex min-h-0 flex-1">
        <nav className="hidden w-[212px] shrink-0 overflow-y-auto border-r border-onyx-line bg-onyx-surface p-2 lg:block">
          {draft.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => setActiveSection(item.id)}
              className={cn(
                'flex h-9 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-[12.5px] transition-colors duration-150',
                activeSection === item.id
                  ? 'bg-onyx-elevated text-onyx-text'
                  : 'text-onyx-text2 hover:bg-onyx-surface3 hover:text-onyx-text'
              )}
            >
              <span className="truncate">{item.label}</span>
            </button>
          ))}
        </nav>

        <div className="flex min-h-0 flex-1 flex-col">
          <div className="shrink-0 overflow-x-auto border-b border-onyx-line bg-onyx-surface px-3 py-2 lg:hidden">
            <div className="flex items-center gap-1">
              {draft.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setActiveSection(item.id)}
                  className={cn(
                    'h-7 shrink-0 rounded px-2 text-[11.5px] transition-colors duration-150',
                    activeSection === item.id
                      ? 'bg-onyx-elevated text-onyx-text'
                      : 'text-onyx-text3 hover:bg-onyx-surface3 hover:text-onyx-text2'
                  )}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-5">
            <div className="mx-auto max-w-[720px]">
              <div className="rounded-lg border border-onyx-line bg-onyx-surface2 px-4 py-1">
                {section?.items.map((item) => (
                  <SettingRow
                    key={item.id}
                    item={item}
                    value={item.value}
                    onChange={(value) => update(section.id, item.id, value)}
                    onAction={handleAction}
                  />
                ))}
              </div>
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-2 border-t border-onyx-line bg-onyx-surface px-5 py-3">
            <span className="text-[11px] text-onyx-text3">
              {dirty ? 'Tens alterações por guardar nesta secção.' : 'Todas as alterações estão guardadas.'}
            </span>
            <div className="ml-auto flex items-center gap-2">
              <OnyxButton
                size="sm"
                variant="ghost"
                disabled={!dirty}
                onClick={() => setDraft(JSON.parse(JSON.stringify(baseline)))}
              >
                <RotateCcw className="h-3.5 w-3.5" /> Repor
              </OnyxButton>
              <OnyxButton size="sm" variant="primary" disabled={!dirty || saving} onClick={save}>
                <Save className="h-3.5 w-3.5" /> {saving ? 'A guardar…' : 'Guardar alterações'}
              </OnyxButton>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}