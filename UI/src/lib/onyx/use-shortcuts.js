// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import { useEffect, useRef } from 'react';

/** Atalhos globais: ⌘/Ctrl+K pesquisa, ⌘/Ctrl+N nova conversa, Escape fecha. */
export function useShortcuts({ onSearch, onNewConversation, onEscape }) {
  const handlers = useRef({ onSearch, onNewConversation, onEscape });
  handlers.current = { onSearch, onNewConversation, onEscape };

  useEffect(() => {
    const handleKeyDown = (event) => {
      const modifier = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();

      if (modifier && key === 'k') {
        event.preventDefault();
        handlers.current.onSearch?.();
        return;
      }
      if (modifier && key === 'n') {
        event.preventDefault();
        handlers.current.onNewConversation?.();
        return;
      }
      if (event.key === 'Escape') handlers.current.onEscape?.();
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);
}