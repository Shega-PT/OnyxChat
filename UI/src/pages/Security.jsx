import React, { useState } from 'react';
import { AlertTriangle, CheckCircle2, ChevronDown, ChevronRight, Cpu, Download, Info, RefreshCw, ShieldAlert, ShieldCheck } from 'lucide-react';
import { cn } from '@/lib/utils';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel, useDetails, useOnyxUI } from '@/lib/onyx/onyx-context';
import { usarToast as useToast } from '@/lib/shadcn';
import PageHeader from '@/components/onyx/PageHeader';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxCard from '@/components/onyx/OnyxCard';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import TechnicalDetails from '@/components/onyx/details/TechnicalDetails';
import { OnyxRow } from '@/components/onyx/OnyxPanel';
import { formatDate, formatRelative } from '@/lib/onyx/format';

const CHECK_ICON = { ok: CheckCircle2, warn: AlertTriangle, error: ShieldAlert };
const CHECK_COLOR = { ok: 'text-onyx-success', warn: 'text-onyx-warning', error: 'text-onyx-error' };

function ScoreRing({ score }) {
  const radius = 44;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (score / 100) * circumference;

  return (
    <svg width="120" height="120" viewBox="0 0 120 120" className="-rotate-90">
      <circle cx="60" cy="60" r={radius} fill="none" stroke="hsl(var(--elevated-1))" strokeWidth="3" />
      <circle
        cx="60"
        cy="60"
        r={radius}
        fill="none"
        stroke="hsl(var(--metallic-2))"
        strokeWidth="3"
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={offset}
      />
      <g transform="rotate(90 60 60)">
        <text x="60" y="58" textAnchor="middle" className="fill-onyx-text font-mono" style={{ fontSize: 26 }}>
          {score}
        </text>
        <text x="60" y="76" textAnchor="middle" className="fill-onyx-text3" style={{ fontSize: 9, letterSpacing: 1.6 }}>
          SCORE
        </text>
      </g>
    </svg>
  );
}

export default function Security() {
  const { loading, data, reload } = useAsync(() => dataAdapter.getSecurity(), []);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const { openModal } = useOnyxUI();
  const { toast } = useToast();
  const security = data;

  useContextLabel(security ? `Nível ${security.level}` : null, [security?.level]);
  useDetails(
    security ? (
      <TechnicalDetails
        title="Chave de identidade"
        subtitle="A chave privada nunca sai deste dispositivo."
        rows={[
          { label: 'Algoritmo', value: security.key.algorithm, mono: true },
          { label: 'Rotação', value: formatDate(security.key.rotatedAt, { day: '2-digit', month: 'short', year: 'numeric' }) },
          { label: 'Próxima', value: formatDate(security.key.nextRotation, { day: '2-digit', month: 'short', year: 'numeric' }) },
          { label: 'Cópia', value: formatDate(security.key.backupAt, { day: '2-digit', month: 'short', year: 'numeric' }), state: 'warn' },
        ]}
        advanced={[
          { label: 'Impressão', value: security.key.fingerprint },
          { label: 'Dispositivos', value: `${security.devices.length} autorizados` },
          { label: 'Alertas', value: `${security.alerts.length} registados` },
        ]}
        note="Se exportares a chave, guarda o ficheiro cifrado num suporte separado."
      />
    ) : null,
    [security?.level]
  );

  if (loading || !security) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <PageHeader title="Segurança" description="Estado criptográfico da tua identidade." />
        <OnyxLoader label="A verificar segurança" />
      </div>
    );
  }

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        title="Segurança"
        description="Resumo do estado da identidade, dispositivos e sessões."
        actions={
          <OnyxButton size="sm" variant="secondary" onClick={reload}>
            <RefreshCw className="h-3.5 w-3.5" /> Rever agora
          </OnyxButton>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        <div className="mx-auto flex max-w-[980px] flex-col gap-4">
          <div className="flex flex-wrap items-center gap-6 rounded-lg border border-onyx-line bg-onyx-surface2 p-5">
            <ScoreRing score={security.score} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <ShieldCheck className="h-4 w-4 text-onyx-info" />
                <h2 className="text-[14px] font-medium text-onyx-text">Nível {security.level.toLowerCase()}</h2>
              </div>
              <p className="mt-2 max-w-[560px] text-[12px] leading-relaxed text-onyx-text2">
                A tua identidade está protegida por chave local e todas as sessões usam cifra negociada. Há duas
                recomendações por resolver: a cópia de segurança da chave e a exposição do identificador.
              </p>
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <OnyxBadge variant="success">{security.checks.filter((check) => check.state === 'ok').length} verificações ok</OnyxBadge>
                <OnyxBadge variant="warning">
                  {security.checks.filter((check) => check.state === 'warn').length} recomendações
                </OnyxBadge>
                <OnyxBadge mono>{security.devices.length} dispositivos</OnyxBadge>
              </div>
            </div>
          </div>

          <OnyxCard title="Verificações" subtitle="Estado de cada camada de proteção" dense>
            <div className="divide-y divide-onyx-line">
              {security.checks.map((check) => {
                const Icon = CHECK_ICON[check.state] || Info;
                return (
                  <button
                    key={check.id}
                    type="button"
                    onClick={() =>
                      openModal({
                        type: 'info',
                        payload: {
                          title: check.label,
                          description: check.detail,
                          rows: [
                            { label: 'Estado', value: check.state === 'ok' ? 'Verificado' : 'Recomendação' },
                            { label: 'Camada', value: check.id },
                          ],
                          note: check.state === 'warn' ? 'Resolve esta recomendação para subir o nível de segurança.' : undefined,
                        },
                      })
                    }
                    className="flex w-full items-center gap-3 px-1 py-2.5 text-left transition-colors duration-150 hover:bg-onyx-surface3/60"
                  >
                    <Icon className={cn('h-4 w-4 shrink-0', CHECK_COLOR[check.state])} />
                    <div className="min-w-0 flex-1">
                      <p className="text-[12.5px] text-onyx-text">{check.label}</p>
                      <p className="mt-0.5 truncate text-[11px] text-onyx-text3">{check.detail}</p>
                    </div>
                    <OnyxBadge variant={check.state === 'ok' ? 'success' : 'warning'} mono>
                      {check.state}
                    </OnyxBadge>
                  </button>
                );
              })}
            </div>
          </OnyxCard>

          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <OnyxCard title="Dispositivos autorizados" subtitle="Cada dispositivo tem a sua própria chave" dense>
              <div className="divide-y divide-onyx-line">
                {security.devices.map((device) => (
                  <div key={device.id} className="flex items-center gap-3 px-1 py-2.5">
                    <Cpu className="h-4 w-4 shrink-0 text-onyx-metallic2" />
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <p className="text-[12.5px] text-onyx-text">{device.label}</p>
                        {device.current && <OnyxBadge variant="secure">este dispositivo</OnyxBadge>}
                      </div>
                      <p className="mt-0.5 font-mono text-[10px] tracking-wide text-onyx-text3">
                        {device.os} · {formatRelative(device.at)}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </OnyxCard>

            <OnyxCard title="Alertas" subtitle="Atividade recente da identidade" dense>
              <div className="divide-y divide-onyx-line">
                {security.alerts.map((alert) => (
                  <div key={alert.id} className="flex items-center gap-3 px-1 py-2.5">
                    <span
                      className={cn(
                        'h-1.5 w-1.5 shrink-0 rounded-full',
                        alert.level === 'warn' ? 'bg-onyx-warning' : alert.level === 'ok' ? 'bg-onyx-success' : 'bg-onyx-info'
                      )}
                    />
                    <p className="min-w-0 flex-1 text-[12px] text-onyx-text2">{alert.text}</p>
                    <span className="shrink-0 font-mono text-[10px] tracking-wide text-onyx-text3">
                      {formatRelative(alert.at)}
                    </span>
                  </div>
                ))}
              </div>
            </OnyxCard>
          </div>

          <OnyxCard title="Área avançada" subtitle="Parâmetros técnicos da chave e das sessões" dense>
            <button
              type="button"
              onClick={() => setShowAdvanced((value) => !value)}
              className="flex w-full items-center gap-2 rounded-md border border-onyx-line bg-onyx-surface px-3 py-2 text-[11.5px] text-onyx-text2 transition-colors duration-150 hover:border-onyx-metallic/30 hover:text-onyx-text"
            >
              {showAdvanced ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
              {showAdvanced ? 'Ocultar parâmetros' : 'Mostrar parâmetros técnicos'}
            </button>
            {showAdvanced && (
              <div className="mt-3 animate-onyx-fade rounded-md border border-onyx-line bg-onyx-surface px-3 py-2">
                <OnyxRow label="Algoritmo" value={security.key.algorithm} mono />
                <OnyxRow label="Impressão digital" value={security.key.fingerprint} mono />
                <OnyxRow label="Rotacionada em" value={formatDate(security.key.rotatedAt)} />
                <OnyxRow label="Próxima rotação" value={formatDate(security.key.nextRotation)} />
                <OnyxRow label="Cópia de segurança" value={formatDate(security.key.backupAt)} state="warn" />
                <div className="mt-3 flex flex-wrap gap-2">
                  <OnyxButton
                    size="sm"
                    variant="secondary"
                    onClick={() => toast({ title: 'Exportação preparada', description: 'Ficheiro cifrado com a tua frase de segurança.' })}
                  >
                    <Download className="h-3.5 w-3.5" /> Exportar chave
                  </OnyxButton>
                  <OnyxButton
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      openModal({
                        type: 'confirm',
                        payload: {
                          title: 'Rotacionar chave agora',
                          description: 'São geradas chaves novas. As sessões ativas são renovadas automaticamente.',
                          confirmLabel: 'Rotacionar',
                          onConfirm: () => toast({ title: 'Rotação iniciada', description: 'As sessões renovam nos próximos minutos.' }),
                        },
                      })
                    }
                  >
                    Rotacionar chave
                  </OnyxButton>
                </div>
              </div>
            )}
          </OnyxCard>
        </div>
      </div>
    </div>
  );
}