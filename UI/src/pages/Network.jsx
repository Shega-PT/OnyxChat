import React from 'react';
import { Activity, Gauge, Globe, RefreshCw, Server, Wifi, Zap } from 'lucide-react';
import { cn } from '@/lib/utils';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel, useDetails } from '@/lib/onyx/onyx-context';
import PageHeader from '@/components/onyx/PageHeader';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxCard from '@/components/onyx/OnyxCard';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import TechnicalDetails from '@/components/onyx/details/TechnicalDetails';
import { formatRelative } from '@/lib/onyx/format';

const LEVEL_COLOR = { ok: 'bg-onyx-success', warn: 'bg-onyx-warning', info: 'bg-onyx-info' };

function MetricBar({ value, max }) {
  const percentage = Math.min(100, Math.round((value / max) * 100));
  return (
    <div className="mt-2 h-1 w-full overflow-hidden rounded-full bg-onyx-surface3">
      <div className="h-full rounded-full bg-onyx-metallic2/70" style={{ width: `${percentage}%` }} />
    </div>
  );
}

export default function Network() {
  const { loading, data, reload } = useAsync(() => dataAdapter.getNetwork(), []);
  const network = data;

  useContextLabel(network ? network.modeLabel : null, [network?.modeLabel]);
  useDetails(
    network ? (
      <TechnicalDetails
        title="Nó local"
        subtitle={`${network.nodes.length} nós visíveis nesta sessão.`}
        rows={[
          { label: 'Modo', value: network.modeLabel },
          { label: 'Transporte', value: network.transport, mono: true },
          { label: 'Cifra', value: network.encryption, mono: true },
          { label: 'Latência', value: `${network.latencyMs} ms`, mono: true },
          { label: 'Jitter', value: `${network.jitterMs} ms`, mono: true },
          { label: 'Sessão', value: network.uptime },
        ]}
        advanced={[
          { label: 'Protocolo', value: network.protocol },
          { label: 'Porta', value: 'UDP 443' },
          { label: 'Descoberta', value: 'DHT + relays autorizados' },
          { label: 'Relays em uso', value: `${network.relaysUsed} de ${network.relays}` },
        ]}
        note="Nenhum pacote é transmitido em claro, mesmo quando a rota passa por um relay de terceiros."
      />
    ) : null,
    [network?.latencyMs]
  );

  if (loading || !network) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <PageHeader title="Rede" description="Estado da ligação e conectividade do nó local." />
        <OnyxLoader label="A medir ligação" />
      </div>
    );
  }

  const metrics = [
    { icon: Wifi, label: 'Estado da ligação', value: network.modeLabel, meta: network.transport, badge: 'ok' },
    { icon: Server, label: 'Peers', value: `${network.peersOnline}/${network.peersTotal}`, meta: 'nós alcançáveis', badge: 'info' },
    { icon: Globe, label: 'Relays', value: `${network.relays}`, meta: `${network.relaysUsed} em uso`, badge: 'info' },
    { icon: Gauge, label: 'Latência', value: `${network.latencyMs} ms`, meta: `jitter ${network.jitterMs} ms`, bar: network.latencyMs, max: 120 },
    { icon: Zap, label: 'Tráfego', value: network.throughput, meta: 'média dos últimos 5 min' },
    { icon: Activity, label: 'Sincronização', value: network.sync.state, meta: `há ${formatRelative(network.sync.lastAt)}`, badge: 'ok' },
  ];

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        title="Rede"
        description="Estado da ligação, nós alcançáveis e sincronização."
        meta={<OnyxStatus status="online" size="sm" label="Operacional" />}
        actions={
          <OnyxButton size="sm" variant="secondary" onClick={reload}>
            <RefreshCw className="h-3.5 w-3.5" /> Atualizar
          </OnyxButton>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        <div className="mx-auto flex max-w-[980px] flex-col gap-4">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
            {metrics.map((metric) => (
              <div
                key={metric.label}
                className="rounded-lg border border-onyx-line bg-onyx-surface2 p-3.5 transition-colors duration-150 hover:border-onyx-metallic/25"
              >
                <div className="flex items-center gap-2">
                  <metric.icon className="h-3.5 w-3.5 text-onyx-metallic2" />
                  <span className="onyx-label">{metric.label}</span>
                  {metric.badge && (
                    <span className={cn('ml-auto h-1.5 w-1.5 rounded-full', LEVEL_COLOR[metric.badge])} />
                  )}
                </div>
                <p className="mt-3 font-mono text-[17px] tracking-wide text-onyx-text">{metric.value}</p>
                <p className="mt-0.5 text-[11px] text-onyx-text3">{metric.meta}</p>
                {metric.bar !== undefined && <MetricBar value={metric.bar} max={metric.max} />}
              </div>
            ))}
          </div>

          <OnyxCard title="Nós visíveis" subtitle="Rotas preferidas e latência medida" dense>
            <div className="divide-y divide-onyx-line">
              {network.nodes.map((node) => (
                <div key={node.id} className="flex flex-wrap items-center gap-3 px-1 py-2.5">
                  <OnyxStatus status={node.status} showLabel={false} />
                  <div className="min-w-0">
                    <p className="truncate text-[12.5px] text-onyx-text">{node.label}</p>
                    <p className="font-mono text-[10px] tracking-wide text-onyx-text3">{node.id}</p>
                  </div>
                  <span className="ml-auto font-mono text-[10.5px] tracking-wide text-onyx-text3">{node.region}</span>
                  <OnyxBadge variant={node.route === 'direto' ? 'secure' : 'neutral'} mono>
                    {node.route}
                  </OnyxBadge>
                  <span className="w-14 text-right font-mono text-[11px] tracking-wide text-onyx-text2">
                    {node.latency} ms
                  </span>
                </div>
              ))}
            </div>
          </OnyxCard>

          <OnyxCard title="Eventos recentes" subtitle="Registo técnico do nó local" dense>
            <div className="divide-y divide-onyx-line">
              {network.events.map((event) => (
                <div key={event.text} className="flex items-center gap-3 px-1 py-2.5">
                  <span className={cn('h-1.5 w-1.5 shrink-0 rounded-full', LEVEL_COLOR[event.level])} />
                  <p className="min-w-0 flex-1 truncate text-[12px] text-onyx-text2">{event.text}</p>
                  <span className="shrink-0 font-mono text-[10px] tracking-wide text-onyx-text3">
                    {formatRelative(event.at)}
                  </span>
                </div>
              ))}
            </div>
          </OnyxCard>

          <div className="flex flex-wrap items-center gap-2 pb-2">
            <OnyxButton size="sm" variant="secondary" onClick={() => reload()}>
              <RefreshCw className="h-3.5 w-3.5" /> Recalcular rotas
            </OnyxButton>
            <span className="font-mono text-[10.5px] tracking-wide text-onyx-text3">
              protocolo {network.protocol} · cifra {network.encryption}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}