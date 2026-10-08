import React, { useState } from 'react';
import { Outlet } from 'react-router-dom';
import { OnyxUIProvider, useOnyxUI } from '@/lib/onyx/onyx-context';
import { useSessao } from '@/lib/SessaoContext';
import { useShortcuts } from '@/lib/onyx/use-shortcuts';
import TopBar from '@/components/onyx/TopBar';
import Sidebar from '@/components/onyx/Sidebar';
import DetailsPanel from '@/components/onyx/DetailsPanel';
import MobileDrawer from '@/components/onyx/MobileDrawer';
import ModalHost from '@/components/onyx/modals/ModalHost';
import FaixaDemonstracao from '@/components/onyx/FaixaDemonstracao';

function Shell() {
  const { modal, openModal, closeModal, panelOpen, setPanelOpen } = useOnyxUI();
  const { demonstracao } = useSessao();
  const [navOpen, setNavOpen] = useState(false);

  useShortcuts({
    onSearch: () => openModal({ type: 'command' }),
    onNewConversation: () => openModal({ type: 'newConversation' }),
    onEscape: () => {
      if (modal) {
        closeModal();
        return;
      }
      setPanelOpen(false);
      setNavOpen(false);
    },
  });

  return (
    <div className="flex h-screen w-full flex-col overflow-hidden bg-onyx-bg text-onyx-text">
      {/*
        A faixa fica **acima** da barra superior, não dentro dela. A barra
        é o elemento que o utilizador aprende a ignorar; uma faixa acima
        dela nunca é confundida com o conteúdo da aplicação.
      */}
      {demonstracao && <FaixaDemonstracao />}

      <TopBar onOpenNav={() => setNavOpen(true)} />

      <div className="relative flex min-h-0 flex-1">
        <Sidebar className="hidden md:flex" />

        <main className="flex min-w-0 flex-1">
          <Outlet />
        </main>

        <DetailsPanel className="hidden xl:flex" />

        {navOpen && (
          <MobileDrawer side="left" label="Navegação" onClose={() => setNavOpen(false)}>
            <Sidebar className="flex w-[288px]" onNavigate={() => setNavOpen(false)} />
          </MobileDrawer>
        )}

        {panelOpen && (
          <MobileDrawer side="right" label="Detalhes" onClose={() => setPanelOpen(false)}>
            <DetailsPanel className="flex w-[324px]" />
          </MobileDrawer>
        )}
      </div>

      <ModalHost />
    </div>
  );
}

export default function AppShell() {
  return (
    <OnyxUIProvider>
      <Shell />
    </OnyxUIProvider>
  );
}