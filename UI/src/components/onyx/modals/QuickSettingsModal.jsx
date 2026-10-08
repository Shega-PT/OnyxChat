import React, { useState } from 'react';
import { Bell, Eye, Route, Signal } from 'lucide-react';
import OnyxModal from '@/components/onyx/OnyxModal';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxSwitch from '@/components/onyx/OnyxSwitch';
import { usarToast as useToast } from '@/lib/shadcn';

const OPTIONS = [
  { id: 'notifications', icon: Bell, label: 'Notificações de mensagens', value: true },
  { id: 'readReceipts', icon: Eye, label: 'Mostrar estado de leitura', value: true },
  { id: 'direct', icon: Route, label: 'Preferir ligação direta', value: true },
  { id: 'dataSaver', icon: Signal, label: 'Poupança de dados', value: false },
];

/** Definições rápidas — os interruptores mais usados, sem sair da conversa. */
export default function QuickSettingsModal({ open, onClose }) {
  const [values, setValues] = useState(() =>
    OPTIONS.reduce((accumulator, option) => ({ ...accumulator, [option.id]: option.value }), {})
  );
  const { toast } = useToast();

  const toggle = (id) => setValues((current) => ({ ...current, [id]: !current[id] }));

  return (
    <OnyxModal
      open={open}
      onOpenChange={(value) => !value && onClose()}
      title="Definições rápidas"
      description="Ajustes imediatos de privacidade e ligação."
      size="sm"
      footer={
        <OnyxButton
          variant="primary"
          onClick={() => {
            toast({ title: 'Definições rápidas aplicadas' });
            onClose();
          }}
        >
          Aplicar
        </OnyxButton>
      }
    >
      <div className="space-y-1">
        {OPTIONS.map((option) => (
          <div
            key={option.id}
            className="flex items-center gap-3 rounded-md px-2 py-2 transition-colors duration-150 hover:bg-onyx-surface3"
          >
            <option.icon className="h-4 w-4 shrink-0 text-onyx-text3" />
            <span className="flex-1 text-[12.5px] text-onyx-text2">{option.label}</span>
            <OnyxSwitch checked={values[option.id]} onCheckedChange={() => toggle(option.id)} />
          </div>
        ))}
      </div>
    </OnyxModal>
  );
}