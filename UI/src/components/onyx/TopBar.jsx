// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React, { useCallback, useState } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { ChevronRight, Download, Lock, Menu, PanelRight, Plus, Search, Settings2, ShieldCheck, User } from 'lucide-react';
import {
  Menu as DropdownMenu,
  MenuConteudo as DropdownMenuContent,
  MenuEtiqueta as DropdownMenuLabel,
  MenuGatilho as DropdownMenuTrigger,
  MenuItem as DropdownMenuItem,
  MenuSeparador as DropdownMenuSeparator,
} from '@/lib/shadcn';
import OnyxLogo from '@/components/onyx/OnyxLogo';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxTooltip from '@/components/onyx/OnyxTooltip';
import { OnyxIconButton } from '@/components/onyx/OnyxButton';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { useSessao } from '@/lib/SessaoContext';
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
  const { estado, trancar, exportar } = useSessao();
  const [ocupado, setOcupado] = useState(false);

  /**
   * Tranca a sessão e manda para o ecrã de entrada.
   *
   * O `setOcupado` existe para o caso em que o `lockAccount` falha: sem
   * ele, o item fica activo e uma segunda tentativa parece não ter
   * acontecido. Com ele, o menu mostra que a operação está em curso, e
   * é o `SessaoContext` — não este ficheiro — que decide o estado final.
   */
  const aoTrancar = useCallback(async () => {
    setOcupado(true);
    try {
      await trancar();
      navigate('/entrar');
    } finally {
      setOcupado(false);
    }
  }, [trancar, navigate]);

  /**
   * Descarrega a cópia de segurança.
   *
   * O ficheiro que chega do backend é **base64 de um keystore cifrado**,
   * e é isso que vai para o disco — sem descodificar, sem abrir, sem
   * olhar. Uma cópia que a interface decifrasse para mostrar o seu
   * conteúdo seria um ficheiro novo a nascer em memória num sítio onde
   * ninguém pediu que estivesse.
   */
  const aoExportar = useCallback(async () => {
    setOcupado(true);
    try {
      const resultado = await exportar();
      if (resultado.erro !== null || resultado.ficheiro === null) {
        openModal({
          type: 'info',
          payload: {
            title: 'Não foi possível exportar',
            description: resultado.erro ?? 'O sidecar devolveu uma cópia vazia.',
          },
        });
        return;
      }
      const bruto = Uint8Array.from(
        atob(resultado.ficheiro.conteudo),
        (c) => c.charCodeAt(0),
      );
      const url = URL.createObjectURL(new Blob([bruto], { type: 'application/octet-stream' }));
      const ligacao = document.createElement('a');
      ligacao.href = url;
      ligacao.download = 'onyxchat-conta.keystore';
      ligacao.click();
      // O object URL mantém o blob vivo até ser revogado; sem isto, a
      // memória fica com a cópia até alguém abrir o inspector.
      URL.revokeObjectURL(url);
    } finally {
      setOcupado(false);
    }
  }, [exportar, openModal]);

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
            {/*
              O menu de perfil leva acções de **identidade**, não acções
              de sessão. «Sair da conta» não existe neste projecto: apagar a
              identidade de alguém porque fechou a janela é uma decisão que
              a interface nunca toma por si, e o subcomando que faria isso
              (`user/cli.py`) é explícito sobre não o fazer.

              O que fica são as duas acções que uma identidade tem:
              trancar e levar a cópia de segurança. Ambas já existem no
              backend — `POST /api/conta/trancar` e
              `GET /api/conta/exportar` — e ambas passam por
              `SessaoContext`, que é quem sabe se a conta existe.

              Este item já-prometia «Bloquear» e «Exportar chave» e não
              fazia nada: abria um modal a dizer que chegariam mais tarde,
              quando já estavam prontas. A lição é a do `Recuperar.jsx` — um
              botão que existe e não faz nada é pior do que um botão que
              não existe, porque o primeiro promete.
            */}
            {estado !== 'sem-conta' && (
              <DropdownMenuItem
                onSelect={aoTrancar}
                disabled={ocupado}
                className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text disabled:cursor-not-allowed disabled:opacity-50"
              >
                <Lock className="h-3.5 w-3.5" /> Bloquear
              </DropdownMenuItem>
            )}
            {estado === 'aberta' && (
              <DropdownMenuItem
                onSelect={aoExportar}
                disabled={ocupado}
                className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text disabled:cursor-not-allowed disabled:opacity-50"
              >
                <Download className="h-3.5 w-3.5" /> Exportar cópia de segurança
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </header>
  );
}