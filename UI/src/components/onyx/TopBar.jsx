import React from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { ChevronRight, LogOut, Menu, PanelRight, Plus, Search, Settings2, ShieldCheck, User } from 'lucide-react';
import {
  Menu as DropdownMenu,
  MenuConteudo as DropdownMenuContent,
  MenuEtiqueta as DropdownMenuLabel,
  MenuGatilho as DropdownMenuTrigger,
  MenuItem as DropdownMenuItem,
  MenuSeparador as DropdownMenuSeparator,
} from '@/lib/shadcn';
import { base44 } from '@/api/base44Client';
import { cn } from '@/lib/utils';
import OnyxLogo from '@/components/onyx/OnyxLogo';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxTooltip from '@/components/onyx/OnyxTooltip';
import { OnyxIconButton } from '@/components/onyx/OnyxButton';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { useAsync } from '@/lib/onyx/use-async';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { sectionLabelFor } from '@/lib/onyx/nav';

function ConnectionPill() {
  const { openModal } = useOnyxUI();
  const { data: network } = useAsync(() => dataAdapter.getNetwork(), []);

  return (
    <button
      type="button"
      onClick={() =>
        network &&
        openModal({
          type: 'info',
          payload: {
            title: 'Estado da rede',
            description: 'Ligação atual deste dispositivo ao núcleo OnyxChat.',
            rows: [
              { label: 'Modo', value: network.modeLabel },
              { label: 'Latência', value: `${network.latencyMs} ms · jitter ${network.jitterMs} ms` },
              { label: 'Transporte', value: network.transport },
              { label: 'Cifra', value: network.encryption },
              { label: 'Peers', value: `${network.peersOnline} de ${network.peersTotal} online` },
              { label: 'Relays', value: `${network.relays} disponíveis · ${network.relaysUsed} em uso` },
              { label: 'Sessão', value: network.uptime },
            ],
          },
        })
      }
      className="hidden items-center gap-2 rounded-md border border-onyx-line bg-onyx-surface2 px-2.5 py-1.5 transition-colors duration-150 hover:border-onyx-metallic/35 hover:bg-onyx-surface3 lg:inline-flex"
    >
      <span className="h-1.5 w-1.5 rounded-full bg-onyx-success" />
      <span className="text-[11px] text-onyx-text2">{network ? network.modeLabel : 'A ligar…'}</span>
      <span className="font-mono text-[10px] tracking-wide text-onyx-text3">
        {network ? `${network.latencyMs} ms` : '—'}
      </span>
    </button>
  );
}

export default function TopBar({ onOpenNav }) {
  const { identity, openModal, contextLabel, setPanelOpen } = useOnyxUI();
  const navigate = useNavigate();
  const { pathname } = useLocation();

  return (
    <header className="z-20 flex h-[60px] shrink-0 items-center gap-2 border-b border-onyx-line bg-onyx-surface px-3 md:gap-3 md:px-4">
      <OnyxIconButton className="md:hidden" onClick={onOpenNav} aria-label="Abrir navegação">
        <Menu className="h-4 w-4" />
      </OnyxIconButton>

      <Link to="/" className="flex items-center gap-2.5" aria-label="OnyxChat — início">
        <OnyxLogo size={26} />
        <span className="hidden text-[12.5px] font-medium uppercase tracking-[0.22em] text-onyx-text sm:inline">
          OnyxChat
        </span>
      </Link>

      <span className="mx-1 hidden h-5 w-px bg-onyx-line md:block" />

      <div className="hidden min-w-0 items-center gap-2 md:flex">
        <span className="onyx-label">{sectionLabelFor(pathname)}</span>
        {contextLabel && (
          <>
            <ChevronRight className="h-3 w-3 shrink-0 text-onyx-text3" />
            <span className="truncate text-[12px] text-onyx-text2">{contextLabel}</span>
          </>
        )}
      </div>

      <div className="ml-auto flex items-center gap-1.5">
        <ConnectionPill />

        <OnyxTooltip label="Pesquisar" kbd="⌘K">
          <OnyxIconButton onClick={() => openModal({ type: 'command' })} aria-label="Pesquisa rápida">
            <Search className="h-4 w-4" />
          </OnyxIconButton>
        </OnyxTooltip>

        <OnyxTooltip label="Nova conversa" kbd="⌘N">
          <OnyxIconButton onClick={() => openModal({ type: 'newConversation' })} aria-label="Nova conversa">
            <Plus className="h-4 w-4" />
          </OnyxIconButton>
        </OnyxTooltip>

        <OnyxTooltip label="Detalhes">
          <OnyxIconButton onClick={() => setPanelOpen(true)} className="xl:hidden" aria-label="Detalhes">
            <PanelRight className="h-4 w-4" />
          </OnyxIconButton>
        </OnyxTooltip>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className="ml-1 rounded-full transition-opacity duration-150 hover:opacity-85"
              aria-label="Perfil"
            >
              <OnyxAvatar seed={identity?.id || 'onyx'} name={identity?.name} size={30} status={identity?.status} />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="end"
            className="w-60 rounded-md border border-onyx-line bg-onyx-surface2 p-1 text-onyx-text2 shadow-xl shadow-black/50"
          >
            <DropdownMenuLabel className="px-2 py-2">
              <span className="block text-[12.5px] font-medium text-onyx-text">{identity?.name || '…'}</span>
              <span className="mt-0.5 block truncate font-mono text-[10px] tracking-wide text-onyx-text3">
                {identity?.identifier || ''}
              </span>
            </DropdownMenuLabel>
            <DropdownMenuSeparator className="my-1 bg-onyx-line" />
            <DropdownMenuItem
              onSelect={() => openModal({ type: 'identityCard' })}
              className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
            >
              <ShieldCheck className="h-3.5 w-3.5" /> Cartão de identidade
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() => openModal({ type: 'quickSettings' })}
              className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
            >
              <User className="h-3.5 w-3.5" /> Definições rápidas
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() => navigate('/definicoes')}
              className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
            >
              <Settings2 className="h-3.5 w-3.5" /> Definições
            </DropdownMenuItem>
            <DropdownMenuSeparator className="my-1 bg-onyx-line" />
            <DropdownMenuItem
              onSelect={() => base44.auth.logout()}
              className={cn('cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text')}
            >
              <LogOut className="h-3.5 w-3.5" /> Terminar sessão
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </header>
  );
}