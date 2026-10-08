import React, { useState } from 'react';
import { AtSign, Send } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import OnyxModal from '@/components/onyx/OnyxModal';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxInput, { OnyxTextarea } from '@/components/onyx/OnyxInput';
import { dataAdapter } from '@/lib/onyx/data-adapter';

export default function AddContactModal({ open, onClose }) {
  const [identifier, setIdentifier] = useState('');
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [sending, setSending] = useState(false);
  const { toast } = useToast();

  const submit = async () => {
    const value = identifier.trim();
    if (value.length < 6) {
      setError('Introduz um identificador completo (mínimo 6 caracteres).');
      return;
    }
    setError('');
    setSending(true);
    try {
      await dataAdapter.sendContactRequest(value, message.trim());
      toast({
        title: 'Pedido enviado',
        description: 'Recebes uma notificação quando o identificador responder.',
      });
      setIdentifier('');
      setMessage('');
      onClose();
    } catch (requestError) {
      setError(requestError.message || 'Não foi possível enviar o pedido.');
    } finally {
      setSending(false);
    }
  };

  return (
    <OnyxModal
      open={open}
      onOpenChange={(value) => !value && onClose()}
      title="Adicionar contacto"
      description="Um pedido de contacto revela apenas o teu identificador e a mensagem que escreveres."
      size="md"
      footer={
        <>
          <OnyxButton variant="ghost" onClick={onClose}>
            Cancelar
          </OnyxButton>
          <OnyxButton variant="primary" onClick={submit} disabled={sending || !identifier.trim()}>
            <Send className="h-3.5 w-3.5" /> {sending ? 'A enviar…' : 'Enviar pedido'}
          </OnyxButton>
        </>
      }
    >
      <div className="space-y-4">
        <OnyxInput
          label="Identificador"
          mono
          autoFocus
          value={identifier}
          onChange={(event) => setIdentifier(event.target.value.toUpperCase())}
          placeholder="ONYX-XXXXXX-XXXX#"
          hint="Formato OnyxChat: ONYX, bloco de identidade e sufixo de verificação."
          error={error}
          trailing={<AtSign className="h-3.5 w-3.5 text-onyx-text3" />}
        />
        <OnyxTextarea
          label="Mensagem (opcional)"
          rows={3}
          value={message}
          onChange={(event) => setMessage(event.target.value)}
          placeholder="Explica quem és e porque queres ligar-te."
        />
      </div>
    </OnyxModal>
  );
}