import React, { useState } from 'react';
import { ArrowRight, Compass, Search, UserPlus } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import { cn } from '@/lib/utils';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel, useDetails, useOnyxUI } from '@/lib/onyx/onyx-context';
import PageHeader from '@/components/onyx/PageHeader';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import IdentityDetails from '@/components/onyx/details/IdentityDetails';
import TechnicalDetails from '@/components/onyx/details/TechnicalDetails';
import { OnyxFactGrid } from '@/components/onyx/OnyxPanel';

export default function Discover() {
  const [identifier, setIdentifier] = useState('');
  const [result, setResult] = useState(null);
  const [status, setStatus] = useState('idle');
  const { data: suggestion } = useAsync(() => dataAdapter.getDiscoverySuggestion(), []);
  const { openModal } = useOnyxUI();
  const { toast } = useToast();

  const person = result?.contact || null;

  useContextLabel(status === 'found' && person ? person.name : null, [status, person?.name]);

  useDetails(
    status === 'found' && person ? (
      <IdentityDetails person={person} kind="request" />
    ) : (
      <TechnicalDetails
        title="Descoberta de identidade"
        subtitle="A pesquisa é local e só consulta a rede quando confirmas."
        rows={[
          { label: 'Consulta de diretório', value: 'Sob pedido explícito' },
          { label: 'Dados enviados', value: 'Apenas o identificador' },
          { label: 'Resultado em cache', value: '10 minutos' },
        ]}
        advanced={[
          { label: 'Protocolo', value: 'ONYX-DISCOVERY 4.2' },
          { label: 'Resolução', value: 'DHT + relays autorizados' },
          { label: 'Registo local', value: 'Nenhum' },
        ]}
        note="Nunca é revelado o teu nome, estado ou lista de contactos durante a pesquisa."
      />
    ),
    [status, person?.id]
  );

  const search = async () => {
    if (!identifier.trim()) return;
    setStatus('searching');
    setResult(null);
    const response = await dataAdapter.discover(identifier);
    setResult(response);
    setStatus(response.status);
  };

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader title="Descobrir" description="Procura um identificador na rede OnyxChat." />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="relative mx-auto flex w-full max-w-[560px] flex-col items-center px-5 py-10">
          <div className="onyx-rings pointer-events-none absolute left-1/2 top-0 h-[420px] w-[420px] -translate-x-1/2 rounded-full" />

          <div className="relative w-full rounded-lg border border-onyx-line bg-onyx-surface2 p-5">
            <div className="flex items-center gap-2">
              <Compass className="h-4 w-4 text-onyx-metallic2" />
              <span className="onyx-label">Pesquisa de identidade</span>
            </div>
            <p className="mt-2.5 text-[12px] leading-relaxed text-onyx-text2">
              Introduz um identificador completo. Nada é enviado até premires Descobrir.
            </p>

            <form
              className="mt-4"
              onSubmit={(event) => {
                event.preventDefault();
                search();
              }}
            >
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-onyx-text3" />
                <input
                  value={identifier}
                  onChange={(event) => setIdentifier(event.target.value.toUpperCase())}
                  placeholder="ONYX-XXXXXX-XXXX#"
                  className="h-11 w-full rounded-md border border-onyx-line bg-onyx-surface pl-9 pr-3 font-mono text-[13px] tracking-wide text-onyx-text placeholder:text-onyx-text3 transition-colors duration-150 focus:border-onyx-metallic/45 focus:ring-2 focus:ring-onyx-metallic/10 focus:outline-none"
                />
              </div>
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <OnyxButton type="submit" variant="primary" disabled={!identifier.trim() || status === 'searching'}>
                  <ArrowRight className="h-3.5 w-3.5" /> Descobrir
                </OnyxButton>
                {suggestion && (
                  <button
                    type="button"
                    onClick={() => setIdentifier(suggestion.identifier)}
                    className="font-mono text-[10.5px] tracking-wide text-onyx-text3 transition-colors hover:text-onyx-text2"
                  >
                    exemplo: {suggestion.identifier}
                  </button>
                )}
              </div>
            </form>
          </div>

          <div className="relative mt-4 w-full">
            {status === 'idle' && (
              <p className="rounded-lg border border-dashed border-onyx-line px-4 py-6 text-center text-[11.5px] leading-relaxed text-onyx-text3">
                O identificador tem três blocos: o prefixo <span className="font-mono text-onyx-text2">ONYX</span>, o bloco
                de identidade e o sufixo de verificação.
              </p>
            )}

            {status === 'searching' && (
              <div className="rounded-lg border border-onyx-line bg-onyx-surface2">
                <OnyxLoader label="A procurar na rede" />
              </div>
            )}

            {status === 'invalid' && (
              <div className="rounded-lg border border-onyx-warning/30 bg-onyx-warning/5 px-4 py-3">
                <p className="text-[12px] text-onyx-warning">{result?.message || 'Identificador inválido.'}</p>
              </div>
            )}

            {status === 'not_found' && (
              <div className="rounded-lg border border-onyx-line bg-onyx-surface2 px-4 py-6 text-center">
                <p className="text-[12.5px] font-medium text-onyx-text">Identificador não encontrado</p>
                <p className="mx-auto mt-1.5 max-w-[380px] text-[11.5px] leading-relaxed text-onyx-text3">
                  Confirma cada bloco do identificador. Se continuar a falhar, a chave pode ter sido rotacionada.
                </p>
              </div>
            )}

            {status === 'found' && person && (
              <div className="animate-onyx-scale-in overflow-hidden rounded-lg border border-onyx-line bg-onyx-surface2">
                <div className="flex flex-wrap items-center gap-4 border-b border-onyx-line px-4 py-4">
                  <OnyxAvatar seed={person.id} name={person.name} size={52} status={person.status} />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <h2 className="text-[13.5px] font-medium text-onyx-text">{person.name}</h2>
                      {person.verified ? (
                        <OnyxBadge variant="secure">verificado</OnyxBadge>
                      ) : (
                        <OnyxBadge variant="warning">por verificar</OnyxBadge>
                      )}
                    </div>
                    <p className="mt-0.5 font-mono text-[10.5px] tracking-wide text-onyx-text3">{person.identifier}</p>
                    <OnyxStatus status={person.status} size="sm" className="mt-1.5" />
                  </div>
                  <div className="ml-auto flex items-center gap-2">
                    <OnyxButton size="sm" variant="secondary" onClick={() => openModal({ type: 'identityCard', payload: { contact: person } })}>
                      Ver identidade
                    </OnyxButton>
                    <OnyxButton
                      size="sm"
                      variant="primary"
                      onClick={() => toast({ title: 'Pedido enviado', description: `Aguardas resposta de ${person.name}.` })}
                    >
                      <UserPlus className="h-3.5 w-3.5" /> Pedir contacto
                    </OnyxButton>
                  </div>
                </div>
                <div className={cn('px-4 py-4')}>
                  <OnyxFactGrid
                    items={[
                      { label: 'Contactos comuns', value: person.mutual ?? 0 },
                      { label: 'Função', value: person.role || '—' },
                      { label: 'Etiquetas', value: person.tags?.length ? person.tags.join(' · ') : '—' },
                      { label: 'Nota', value: person.note || '—' },
                    ]}
                  />
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}