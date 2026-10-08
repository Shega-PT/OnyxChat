import React, { useEffect, useState } from 'react';
import { dataAdapter } from '@/lib/onyx/data-adapter';
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

  const { loading, data } = useAsync(() => dataAdapter.listConversations({ query, filter }), [query, filter]);
  const { openModal } = useOnyxUI();
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