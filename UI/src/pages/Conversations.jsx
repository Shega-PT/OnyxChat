// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React, { useCallback, useEffect, useState } from 'react';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { usarToast as useToast } from '@/lib/shadcn';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel, useDetails, useOnyxUI } from '@/lib/onyx/onyx-context';
import ConversationList from '@/components/onyx/ConversationList';
import ConversationView from '@/components/onyx/ConversationView';
import ConversationDetails from '@/components/onyx/details/ConversationDetails';
import { cn } from '@/lib/utils';

export default function Conversations() {
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('todas');
  const [selectedId, setSelectedId] = useState(null);
  const [sentMessages, setSentMessages] = useState({});
  const [mobileView, setMobileView] = useState('lista');

  const { loading, data, reload } = useAsync(
    () => dataAdapter.listConversations({ query, filter }),
    [query, filter]
  );
  const { openModal } = useOnyxUI();
  const { toast } = useToast();
  const conversations = data || [];

  useEffect(() => {
    if (!conversations.length) return;
    if (!selectedId || !conversations.some((conversation) => conversation.id === selectedId)) {
      setSelectedId(conversations[0].id);
    }
  }, [conversations, selectedId]);

  const selected = conversations.find((conversation) => conversation.id === selectedId) || null;
  const contact = selected?.contact || null;
  const messages = selected ? [...selected.messages, ...(sentMessages[selected.id] || [])] : [];
  const active = selected ? { ...selected, messages } : null;

  useContextLabel(selected?.title || null, [selected?.title]);
  useDetails(<ConversationDetails conversation={selected} contact={contact} />, [selected?.id]);

  const handleSend = async (text) => {
    const message = await dataAdapter.sendMessage(selected.id, text);
    const conversationId = selected.id;
    setSentMessages((current) => ({
      ...current,
      [conversationId]: [...(current[conversationId] || []), message],
    }));
    setTimeout(() => {
      setSentMessages((current) => ({
        ...current,
        [conversationId]: (current[conversationId] || []).map((item) =>
          item.id === message.id ? { ...item, state: 'delivered' } : item
        ),
      }));
    }, 900);
  };

  // -----------------------------------------------------------------
  // As acções que escrevem
  // -----------------------------------------------------------------
  //
  // Até 2026-10-13 estas seis existiam no menu de contexto e não faziam
  // nada: `ConversationList` recebia-as por `props` com uma alternativa
  // inerte (`semAccao`) e ninguém as passava. O botão estava desenhado,
  // o rótulo estava escrito, e o gesto não tinha efeito nenhum.
  //
  // Todas seguem a mesma forma — chamar o adaptador, recarregar, dizer o
  // que aconteceu — e o motivo de ser sempre esta sequência, e não uma
  //actualização local do estado, é que **a resposta do servidor é a
  // verdade**. Uma lista que «se» marcava como lida e recarregasse logo a
  // seguir piscava mesmo quando o servidor não tinha gravado nada, e a
  // pessoa via o estado dela desfazer-se sem explicação.

  /**
   * Corre uma acção e anuncia o resultado nos dois sentidos.
   *
   * @param {() => Promise<object>} accao
   * @param {{titulo: string, erro: string}} rotulos
   */
  const correr = useCallback(
    async (accao, { titulo, erro }) => {
      try {
        await accao();
        await reload();
        toast({ title: titulo });
      } catch (falha) {
        toast({ title: erro, description: falha?.message, variant: 'destructive' });
      }
    },
    [reload, toast]
  );

  const marcarLida = useCallback(
    (conversationId) =>
      correr(() => dataAdapter.markConversationRead(conversationId), {
        titulo: 'Conversa marcada como lida',
        erro: 'Não foi possível marcar como lida',
      }),
    [correr]
  );

  /**
   * Fixar e silenciar recebem o valor **invertido** do estado actual.
   *
   * O menu diz «Fixar» numa conversa solta e «Desfixar» numa fixa, mas o
   * que chega ao adaptador é a intenção — o valor de destino — e não a
   * palavra do botão. Passar o valor actual transformaria o rótulo num
   * espelho e o botão passaria a fazer o contrário do que promete.
   */
  const fixar = useCallback(
    (conversation) =>
      correr(() => dataAdapter.pinConversation(conversation.id, !conversation.pinned), {
        titulo: conversation.pinned ? 'Conversa desfixada' : 'Conversa fixada',
        erro: 'Não foi possível mudar a fixação',
      }),
    [correr]
  );

  const silenciar = useCallback(
    (conversation) =>
      correr(() => dataAdapter.muteConversation(conversation.id, !conversation.muted), {
        titulo: conversation.muted ? 'Conversa dessilenciada' : 'Conversa silenciada',
        erro: 'Não foi possível silenciar',
      }),
    [correr]
  );

  /**
   * Limpar o histórico pede confirmação antes de escrever.
   *
   * É a única das seis que apaga. As outras cinco invertem um estado que
   * a segunda pressão desfaz; esta não tem volta dentro do ecrã, e um
   * botão que apaga mensagens sem perguntar é um botão de quem não leu
   * o rótulo. A confirmação vai pelo modal que a interface já tem, e não
   * por `window.confirm` — que é um diálogo do browser dentro de uma
   * aplicação com sistema de design, e não se parece com nada do resto.
   */
  const limparHistorico = useCallback(
    (conversation) =>
      openModal({
        type: 'confirm',
        payload: {
          title: 'Limpar histórico',
          description: `As mensagens de ${conversation.title} serão apagadas. A conversa e o contacto ficam.`,
          confirmLabel: 'Limpar',
          variant: 'danger',
          onConfirm: () =>
            correr(() => dataAdapter.clearHistory(conversation.id), {
              titulo: 'Histórico limpo',
              erro: 'Não foi possível limpar o histórico',
            }),
        },
      }),
    [correr, openModal]
  );

  const bloquear = useCallback(
    (conversation) => {
      if (!conversation.contactId) {
        toast({
          title: 'Esta conversa não tem contacto',
          description: 'Não há quem bloquear.',
          variant: 'destructive',
        });
        return;
      }
      const contact = conversation.contact;
      const jaBloqueado = Boolean(contact?.blocked);
      return openModal({
        type: 'confirm',
        payload: {
          title: jaBloqueado ? 'Desbloquear' : 'Bloquear',
          description: jaBloqueado
            ? `${conversation.title} volta a poder enviar mensagens.`
            : `${conversation.title} deixa de poder enviar mensagens, e os pedidos dele deixam de aparecer.`,
          confirmLabel: jaBloqueado ? 'Desbloquear' : 'Bloquear',
          variant: jaBloqueado ? undefined : 'danger',
          onConfirm: () =>
            correr(() => dataAdapter.blockContact(conversation.contactId, !jaBloqueado), {
              titulo: jaBloqueado ? 'Contacto desbloqueado' : 'Contacto bloqueado',
              erro: 'Não foi possível mudar o bloqueio',
            }),
        },
      });
    },
    [correr, openModal, toast]
  );

  /** Ver a identidade: não escreve, e por isso não recarrega. */
  const verIdentidade = useCallback(
    (conversation) => {
      const alvo = conversation.contact || conversation;
      openModal({ type: 'identityCard', payload: { contact: alvo } });
    },
    [openModal]
  );

  return (
    <div className="flex min-w-0 flex-1">
      <ConversationList
        className={cn('w-full lg:flex lg:w-[300px] lg:border-r', mobileView === 'conversa' && 'hidden lg:flex')}
        conversations={conversations}
        loading={loading}
        selectedId={selectedId}
        onSelect={(id) => {
          setSelectedId(id);
          setMobileView('conversa');
        }}
        query={query}
        onQueryChange={setQuery}
        filter={filter}
        onFilterChange={setFilter}
        onNew={() => openModal({ type: 'newConversation' })}
        onMarcarLida={marcarLida}
        onSilenciar={silenciar}
        onFixar={fixar}
        onVerIdentidade={verIdentidade}
        onLimparHistorico={limparHistorico}
        onBloquear={bloquear}
      />
      <ConversationView
        className={cn(mobileView === 'lista' && 'hidden lg:flex')}
        conversation={active}
        contact={contact}
        onSend={handleSend}
        onBack={() => setMobileView('lista')}
      />
    </div>
  );
}