import React from 'react';
import { Copy, Download, Share2, ShieldCheck } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel, useDetails, useOnyxUI } from '@/lib/onyx/onyx-context';
import PageHeader from '@/components/onyx/PageHeader';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import OnyxRings from '@/components/onyx/OnyxRings';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import TechnicalDetails from '@/components/onyx/details/TechnicalDetails';
import { OnyxFactGrid, OnyxRow } from '@/components/onyx/OnyxPanel';
import { copyToClipboard, formatDate, shortFingerprint } from '@/lib/onyx/format';

export default function Identity() {
  const { loading, data } = useAsync(() => dataAdapter.getIdentity(), []);
  const identity = data;
  const { openModal } = useOnyxUI();
  const { toast } = useToast();

  useContextLabel(identity?.name || null, [identity?.name]);
  useDetails(
    identity ? (
      <TechnicalDetails
        title="Registo de identidade"
        subtitle="Dados que compõem o identificador público."
        rows={[
          { label: 'Bloco de identidade', value: identity.id, mono: true },
          { label: 'Criado em', value: formatDate(identity.createdAt) },
          { label: 'Região', value: identity.region },
          { label: 'Dispositivos', value: identity.devices, mono: true },
          { label: 'Contactos', value: `${identity.verifiedContacts}/${identity.totalContacts} verificados` },
        ]}
        advanced={[
          { label: 'Impressão', value: identity.fingerprint },
          { label: 'Algoritmo', value: identity.keyAlgorithm },
          { label: 'Sessões hoje', value: identity.sessionsToday },
        ]}
        note="O identificador só é partilhado quando o decides. Nunca é publicado automaticamente na rede."
      />
    ) : null,
    [identity?.id]
  );

  if (loading || !identity) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <PageHeader title="Identidade" description="A tua representação na rede OnyxChat." />
        <OnyxLoader label="A carregar identidade" />
      </div>
    );
  }

  const lines = identity.identifier.split('-');

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        title="Identidade"
        description="Cartão público, chaves associadas e verificações."
        meta={
          <OnyxBadge variant="secure" mono>
            {identity.role}
          </OnyxBadge>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        <div className="mx-auto flex max-w-[820px] flex-col gap-4">
          <section className="relative overflow-hidden rounded-lg border border-onyx-line bg-onyx-surface2">
            <OnyxRings size={620} opacity={0.42} className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2" />
            <div className="relative flex flex-col items-center px-6 py-10 text-center">
              <span className="onyx-label">Identidade Onyx</span>
              <OnyxAvatar seed={identity.id} name={identity.name} size={104} status={identity.status} className="mt-6" />
              <h1 className="mt-5 text-[19px] font-medium tracking-tight text-onyx-text">{identity.name}</h1>
              <OnyxStatus status={identity.status} size="sm" className="mt-2" />

              <div className="mt-6 w-full max-w-[420px] rounded-md border border-onyx-line bg-onyx-surface px-5 py-4">
                {lines.map((line, index) => (
                  <p
                    key={index}
                    className={
                      index === 0
                        ? 'font-mono text-[10px] uppercase tracking-[0.36em] text-onyx-text3'
                        : 'font-mono text-[17px] tracking-[0.2em] text-onyx-text'
                    }
                  >
                    {line}
                  </p>
                ))}
              </div>

              <div className="mt-6 flex flex-wrap items-center justify-center gap-2">
                <OnyxButton
                  size="sm"
                  variant="primary"
                  onClick={() =>
                    copyToClipboard(identity.identifier)
                      .then(() => toast({ title: 'Identificador copiado', description: identity.identifier }))
                      .catch(() => toast({ title: 'Não foi possível copiar', variant: 'destructive' }))
                  }
                >
                  <Copy className="h-3.5 w-3.5" /> Copiar identificador
                </OnyxButton>
                <OnyxButton size="sm" variant="secondary" onClick={() => openModal({ type: 'identityCard' })}>
                  <ShieldCheck className="h-3.5 w-3.5" /> Cartão de identidade
                </OnyxButton>
                <OnyxButton
                  size="sm"
                  variant="secondary"
                  onClick={() => toast({ title: 'Ligação de partilha criada', description: 'Válida durante 24 horas.' })}
                >
                  <Share2 className="h-3.5 w-3.5" /> Partilhar
                </OnyxButton>
                <OnyxButton
                  size="sm"
                  variant="ghost"
                  onClick={() => toast({ title: 'Exportação preparada', description: 'Ficheiro cifrado com a tua frase de segurança.' })}
                >
                  <Download className="h-3.5 w-3.5" /> Exportar chave
                </OnyxButton>
              </div>
            </div>
          </section>

          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <section className="rounded-lg border border-onyx-line bg-onyx-surface2 p-4">
              <p className="onyx-label mb-3">Informação associada</p>
              <OnyxFactGrid
                items={[
                  { label: 'Bloco de identidade', value: identity.id, mono: true },
                  { label: 'Criado em', value: formatDate(identity.createdAt, { day: '2-digit', month: 'short', year: 'numeric' }) },
                  { label: 'Região', value: identity.region },
                  { label: 'Dispositivos', value: `${identity.devices} autorizados` },
                ]}
              />
              <div className="mt-3 border-t border-onyx-line pt-1">
                <OnyxRow label="Impressão digital" value={shortFingerprint(identity.fingerprint)} mono />
                <OnyxRow label="Algoritmo" value={identity.keyAlgorithm} mono />
              </div>
            </section>

            <section className="rounded-lg border border-onyx-line bg-onyx-surface2 p-4">
              <p className="onyx-label mb-3">Verificações</p>
              <OnyxRow
                label="Contactos verificados"
                value={`${identity.verifiedContacts} de ${identity.totalContacts}`}
                state="ok"
              />
              <OnyxRow label="Sessões hoje" value={identity.sessionsToday} mono />
              <OnyxRow label="Estado da chave" value="Ativa · rotação agendada" state="ok" />
              <OnyxRow label="Cópia de segurança" value="Há 41 dias" state="warn" />
              <div className="mt-3 flex flex-wrap gap-2">
                <OnyxButton size="sm" variant="secondary" onClick={() => toast({ title: 'Verificação agendada', description: 'Vais ser notificado quando o contacto confirmar.' })}>
                  Pedir verificação
                </OnyxButton>
              </div>
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}