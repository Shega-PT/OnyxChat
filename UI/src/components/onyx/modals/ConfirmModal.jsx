import React from 'react';
import OnyxModal from '@/components/onyx/OnyxModal';
import OnyxButton from '@/components/onyx/OnyxButton';

/** Confirmação de ações sensíveis (bloquear, limpar, reiniciar). */
export default function ConfirmModal({ open, onClose, payload }) {
  const danger = payload?.variant === 'danger';

  const confirm = () => {
    payload?.onConfirm?.();
    onClose();
  };

  return (
    <OnyxModal
      open={open}
      onOpenChange={(value) => !value && onClose()}
      title={payload?.title || 'Confirmar'}
      description={payload?.description}
      size="sm"
      footer={
        <>
          <OnyxButton variant="ghost" onClick={onClose}>
            Cancelar
          </OnyxButton>
          <OnyxButton variant={danger ? 'danger' : 'primary'} onClick={confirm} autoFocus>
            {payload?.confirmLabel || 'Confirmar'}
          </OnyxButton>
        </>
      }
    >
      <p className="text-[12px] leading-relaxed text-onyx-text2">
        Esta ação aplica-se imediatamente. Podes revertê-la mais tarde nas definições do contacto.
      </p>
    </OnyxModal>
  );
}