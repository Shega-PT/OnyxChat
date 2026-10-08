import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ArrowRight, CornerDownLeft, Search } from 'lucide-react';
import {
  Dialogo as Dialog,
  DialogoConteudo as DialogContent,
  DialogoDescricao as DialogDescription,
  DialogoTitulo as DialogTitle,
} from '@/lib/shadcn';
import { cn } from '@/lib/utils';
import { OnyxKbd } from '@/components/onyx/OnyxBadge';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { useAsync } from '@/lib/onyx/use-async';
import { NAV_ITEMS, FOOTER_ITEMS } from '@/lib/onyx/nav';

/** ⌘K — pesquisa de navegação e contactos. */
export default function CommandPalette({ open, onClose }) {
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const navigate = useNavigate();

  const { data, loading } = useAsync(
    () => (open ? dataAdapter.listContacts({ query }) : Promise.resolve([])),
    [query, open]
  );

  useEffect(() => {
    if (open) {
      setQuery('');
      setActiveIndex(0);
    }
  }, [open]);

  const term = query.trim().toLowerCase();
  const sections = [...NAV_ITEMS, ...FOOTER_ITEMS].filter(
    (item) => !term || item.label.toLowerCase().includes(term)
  );
  const navItems = sections.map((item) => ({
    id: `nav-${item.to}`,
    label: item.label,
    icon: item.icon,
    run: () => {
      navigate(item.to);
      onClose();
    },
  }));
  const contactItems = (data || []).slice(0, 5).map((contact) => ({
    id: `contact-${contact.id}`,
    label: contact.name,
    sub: contact.identifier,
    seed: contact.id,
    run: () => {
      navigate('/contactos');
      onClose();
    },
  }));
  const items = [...navItems, ...contactItems];

  const handleKeyDown = (event) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setActiveIndex((index) => Math.min(index + 1, items.length - 1));
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setActiveIndex((index) => Math.max(index - 1, 0));
    } else if (event.key === 'Enter') {
      event.preventDefault();
      items[activeIndex]?.run();
    }
  };

  const renderRow = (item, index) => {
    const Icon = item.icon;
    return (
      <button
        key={item.id}
        type="button"
        onMouseEnter={() => setActiveIndex(index)}
        onClick={item.run}
        className={cn(
          'flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left transition-colors duration-150',
          activeIndex === index ? 'bg-onyx-elevated text-onyx-text' : 'text-onyx-text2 hover:bg-onyx-surface3'
        )}
      >
        <Icon className="h-3.5 w-3.5 shrink-0 text-onyx-metallic2" />
        <span className="min-w-0 flex-1 truncate text-[12.5px]">{item.label}</span>
        {item.sub && <span className="truncate font-mono text-[10px] tracking-wide text-onyx-text3">{item.sub}</span>}
        {activeIndex === index && <CornerDownLeft className="h-3 w-3 shrink-0 text-onyx-text3" />}
      </button>
    );
  };

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onClose()}>
      <DialogContent
        className="!gap-0 overflow-hidden rounded-lg !border !border-onyx-line !bg-onyx-surface2 !p-0 text-onyx-text shadow-2xl shadow-black/60 sm:max-w-[560px]"
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <DialogTitle className="sr-only">Pesquisa rápida</DialogTitle>
        <DialogDescription className="sr-only">Navegação e contactos</DialogDescription>

        <div className="flex items-center gap-2.5 border-b border-onyx-line px-3.5 py-3">
          <Search className="h-4 w-4 shrink-0 text-onyx-text3" />
          <input
            autoFocus
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActiveIndex(0);
            }}
            onKeyDown={handleKeyDown}
            placeholder="Ir para… ou pesquisar contactos"
            className="flex-1 bg-transparent text-[13px] text-onyx-text placeholder:text-onyx-text3 focus:outline-none"
          />
          <OnyxKbd>Esc</OnyxKbd>
        </div>

        <div className="max-h-[360px] overflow-y-auto p-2">
          {navItems.length > 0 && (
            <>
              <p className="onyx-label px-2.5 pb-1.5 pt-1">Navegação</p>
              {navItems.map((item, index) => renderRow(item, index))}
            </>
          )}
          {contactItems.length > 0 && (
            <>
              <p className="onyx-label px-2.5 pb-1.5 pt-3">Contactos</p>
              {contactItems.map((item, index) => renderRow(item, navItems.length + index))}
            </>
          )}
          {!loading && items.length === 0 && (
            <p className="px-2.5 py-6 text-center text-[12px] text-onyx-text3">Nada corresponde a “{query}”.</p>
          )}
        </div>

        <div className="flex items-center gap-3 border-t border-onyx-line bg-onyx-surface px-3.5 py-2">
          <span className="flex items-center gap-1.5 text-[10.5px] text-onyx-text3">
            <OnyxKbd>↑</OnyxKbd>
            <OnyxKbd>↓</OnyxKbd> navegar
          </span>
          <span className="flex items-center gap-1.5 text-[10.5px] text-onyx-text3">
            <OnyxKbd>Enter</OnyxKbd> abrir
          </span>
          <span className="ml-auto flex items-center gap-1.5 font-mono text-[10px] text-onyx-text3">
            <ArrowRight className="h-3 w-3" /> OnyxChat
          </span>
        </div>
      </DialogContent>
    </Dialog>
  );
}