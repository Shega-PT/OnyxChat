import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { dataAdapter } from './data-adapter';

const OnyxContext = createContext(null);

export function OnyxUIProvider({ children }) {
  const [identity, setIdentity] = useState(null);
  const [counts, setCounts] = useState({ unread: 0, requests: 0, contacts: 0, relays: 0, peersOnline: 0, peersTotal: 0 });
  const [details, setDetails] = useState(null);
  const [panelOpen, setPanelOpen] = useState(false);
  const [modal, setModal] = useState(null);
  const [contextLabel, setContextLabel] = useState(null);

  const refreshCounts = useCallback(() => {
    dataAdapter.getCounts().then(setCounts);
  }, []);

  useEffect(() => {
    let active = true;
    dataAdapter.getIdentity().then((value) => {
      if (active) setIdentity(value);
    });
    dataAdapter.getCounts().then((value) => {
      if (active) setCounts(value);
    });
    return () => {
      active = false;
    };
  }, []);

  const openModal = useCallback((next) => setModal(next), []);
  const closeModal = useCallback(() => setModal(null), []);

  const value = useMemo(
    () => ({
      identity,
      counts,
      refreshCounts,
      details,
      setDetails,
      panelOpen,
      setPanelOpen,
      modal,
      openModal,
      closeModal,
      contextLabel,
      setContextLabel,
    }),
    [identity, counts, refreshCounts, details, panelOpen, modal, openModal, closeModal, contextLabel]
  );

  return <OnyxContext.Provider value={value}>{children}</OnyxContext.Provider>;
}

export function useOnyxUI() {
  const context = useContext(OnyxContext);
  if (!context) throw new Error('useOnyxUI tem de ser usado dentro de OnyxUIProvider');
  return context;
}

/** Publica conteúdo no painel de detalhes enquanto a vista estiver montada. */
export function useDetails(node, deps = []) {
  const { setDetails } = useOnyxUI();
  useEffect(() => {
    setDetails(node);
    return () => setDetails(null);
  }, deps);
}

/** Publica o contexto atual (elemento selecionado) na barra superior. */
export function useContextLabel(label, deps = []) {
  const { setContextLabel } = useOnyxUI();
  useEffect(() => {
    setContextLabel(label || null);
    return () => setContextLabel(null);
  }, deps);
}