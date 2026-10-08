import React from 'react';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import CommandPalette from '@/components/onyx/modals/CommandPalette';
import NewConversationModal from '@/components/onyx/modals/NewConversationModal';
import AddContactModal from '@/components/onyx/modals/AddContactModal';
import IdentityCardModal from '@/components/onyx/modals/IdentityCardModal';
import QuickSettingsModal from '@/components/onyx/modals/QuickSettingsModal';
import InfoModal from '@/components/onyx/modals/InfoModal';
import ConfirmModal from '@/components/onyx/modals/ConfirmModal';

/** Registo único dos modais da aplicação. */
export default function ModalHost() {
  const { modal, closeModal } = useOnyxUI();
  const type = modal?.type;

  return (
    <>
      <CommandPalette open={type === 'command'} onClose={closeModal} />
      <NewConversationModal open={type === 'newConversation'} onClose={closeModal} />
      <AddContactModal open={type === 'addContact'} onClose={closeModal} />
      <IdentityCardModal open={type === 'identityCard'} onClose={closeModal} payload={modal?.payload} />
      <QuickSettingsModal open={type === 'quickSettings'} onClose={closeModal} />
      <InfoModal open={type === 'info'} onClose={closeModal} payload={modal?.payload} />
      <ConfirmModal open={type === 'confirm'} onClose={closeModal} payload={modal?.payload} />
    </>
  );
}