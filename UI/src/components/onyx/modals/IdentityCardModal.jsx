import React, { useState } from 'react';
import { Copy, Download, Fingerprint, Lock } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import OnyxModal from '@/components/onyx/OnyxModal';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { copyToClipboard, formatDate } from '@/lib/onyx/format';

/** Cartão de identidade — representação visual forte, identificador em monospace. */
export default function IdentityCardModal({ open, onClose, payload }) {
  const { identity } = useOnyxUI();
  const { toast } = useToast();
  const [showFingerprint, setShowFingerprint] = useState(false);

  const person = payload?.contact || identity;
  if (!person) return null;

  const own = !payload?.contact;
  const identifier = person.identifier || '';
  const lines = identifier.split('-');
  const fingerprint = person.fingerprint || '';

  const copy = (value, label) => {
    copyToClipboard(value)
      .then(() => toast({ title: `${label} copiada`, description: value }))
      .catch(() => toast({ title: 'Não foi possível copiar', variant: 'destructive' }));
  };

  return (
    <OnyxModal
      open={open}
      onOpenChange={(value) => !value && onClose()}
      title={own ? 'O meu cartão de identidade' : `Cartão de ${person.name}`}
      description="Partilha apenas o identificador. A impressão digital confirma-se sempre por um canal separado."
      size="md"
      footer={
        <>
          <OnyxButton variant="ghost" onClick={onClose}>
            Fechar
          </OnyxButton>
          {own && (
            <OnyxButton
              variant="secondary"
              onClick={() => toast({ title: 'Exportação preparada', description: 'Ficheiro cifrado com a tua frase de segurança.' })}
            >
              <Download className="h-3.5 w-3.5" /> Exportar
            </OnyxButton>
          )}
          <OnyxButton variant="primary" onClick={() => copy(identifier, 'Identificador')}>
            <Copy className="h-3.5 w-3.5" /> Copiar identificador
          </OnyxButton>
        </>
      }
    >
      <div className="relative overflow-hidden rounded-lg border border-onyx-line bg-onyx-surface px-6 py-6">
        <div className="onyx-rings pointer-events-none absolute left-1/2 top-1/2 h-[360px] w-[360px] -translate-x-1/2 -translate-y-1/2 rounded-full" />
        <div className="relative flex flex-col items-center">
          <div className="flex w-full items-center justify-between">
            <span className="onyx-label">Identidade Onyx</span>
            <OnyxBadge variant={own ? 'secure' : 'neutral'} mono>
              {own ? 'primária' : person.service ? 'serviço' : 'contacto'}
            </OnyxBadge>
          </div>

          <OnyxAvatar seed={person.id} name={person.name} size={88} status={person.status} className="mt-5" />

          <h3 className="mt-4 text-[15px] font-medium text-onyx-text">{person.name}</h3>
          <OnyxStatus status={person.status} size="sm" className="mt-1.5" />

          <div className="mt-5 w-full rounded-md border border-onyx-line bg-onyx-surface2 px-4 py-3 text-center">
            {lines.map((line, index) => (
              <p
                key={index}
                className={
                  index === 0
                    ? 'font-mono text-[10px] uppercase tracking-[0.34em] text-onyx-text3'
                    : 'font-mono text-[15px] tracking-[0.16em] text-onyx-text'
                }
              >
                {line}
              </p>
            ))}
          </div>

          <div className="mt-4 grid w-full grid-cols-2 gap-x-4 gap-y-3 text-left">
            <div className="min-w-0">
              <p className="onyx-label mb-1.5">Criado em</p>
              <p className="text-[12px] text-onyx-text">{formatDate(person.createdAt || person.addedAt)}</p>
            </div>
            <div className="min-w-0">
              <p className="onyx-label mb-1.5">{own ? 'Região' : 'Contactos comuns'}</p>
              <p className="text-[12px] text-onyx-text">{own ? person.region : person.mutual ?? 0}</p>
            </div>
            <div className="col-span-2 min-w-0">
              <p className="onyx-label mb-1.5">Impressão digital</p>
              <button
                type="button"
                onClick={() => setShowFingerprint((value) => !value)}
                className="flex w-full items-center gap-2 rounded-md border border-onyx-line bg-onyx-surface2 px-2.5 py-2 text-left transition-colors duration-150 hover:border-onyx-metallic/30 hover:bg-onyx-surface3"
              >
                <Fingerprint className="h-3.5 w-3.5 shrink-0 text-onyx-metallic2" />
                <span className="flex-1 break-all font-mono text-[10.5px] leading-relaxed tracking-wide text-onyx-text2">
                  {showFingerprint ? fingerprint : `${fingerprint.slice(0, 11)}:••:••:••:••:••:••:${fingerprint.slice(-5)}`}
                </span>
                <Lock className="h-3 w-3 shrink-0 text-onyx-text3" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </OnyxModal>
  );
}