// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React from 'react';
import {
  Ban,
  Bell,
  BellOff,
  Check,
  Copy,
  Fingerprint,
  Pin,
  PinOff,
  ShieldCheck,
  Trash2,
} from 'lucide-react';
import {
  estiloEtiquetaMenu,
  estiloItemMenu,
  estiloItemMenuDestrutivo,
  MenuContextualEtiqueta,
  MenuContextualItem,
  MenuContextualSeparador,
} from '@/components/onyx/OnyxContextMenu';

/**
 * Itens do menu de contexto de uma linha de conversa.
 *
 * Todas as acções aqui disponíveis também existem noutro sítio — o menu
 * suspenso do cabeçalho da conversa tem as mesmas. Mantêm-se as duas vias
 * porque o menu de contexto não é alcançável por teclado de forma fiável, e
 * um gesto que só existe com o botão direito deixa de existir para quem não
 * usa o rato.
 *
 * As acções são passadas como funções pela vista e não implementadas aqui:
 * este componente sabe desenhar o item, não sabe o que é "marcar como
 * lida". É a mesma separação que existe entre `MessageComposer`, que
 * desenha o campo, e `Conversations`, que decide o que enviar.
 *
 * Todas recebem a **conversa inteira**, e não o `id`. As três que já o
 * recebiam — ver identidade, copiar, limpar, bloquear — precisavam dele
 * por si; as três restantes passaram a precisar porque «fixar» e
 * «silenciar» são inversões, e inverter exige saber o estado actual.
 * Passar só o `id` obrigaria a vista a manter uma segunda fonte de
 * verdade sobre o que está fixado — que é exactamente o tipo de estado
 * duplicado que diverge.
 *
 * @param {object} props
 * @param {object} props.conversation Conversa, já preparada pelo adaptador.
 * @param {(conversation: object) => void} props.onMarcarLida
 * @param {(conversation: object) => void} props.onSilenciar
 * @param {(conversation: object) => void} props.onFixar
 * @param {(conversation: object) => void} props.onVerIdentidade
 * @param {(conversation: object) => void} props.onCopiarIdentificador
 * @param {(conversation: object) => void} props.onLimparHistorico
 * @param {(conversation: object) => void} props.onBloquear
 */
export default function ConversationContextItems({
  conversation,
  onMarcarLida,
  onSilenciar,
  onFixar,
  onVerIdentidade,
  onCopiarIdentificador,
  onLimparHistorico,
  onBloquear,
}) {
  const temNaoLidas = conversation.unread > 0;

  return (
    <>
      <MenuContextualEtiqueta className={estiloEtiquetaMenu}>{conversation.title}</MenuContextualEtiqueta>
      <MenuContextualSeparador className="my-1 bg-onyx-line" />

      {temNaoLidas && (
        <MenuContextualItem className={estiloItemMenu} onSelect={() => onMarcarLida(conversation)}>
          <Check className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
          Marcar como lida
          <span className="ml-auto font-mono text-[10px] text-onyx-text3">{conversation.unread}</span>
        </MenuContextualItem>
      )}

      <MenuContextualItem className={estiloItemMenu} onSelect={() => onSilenciar(conversation)}>
        {conversation.muted ? (
          <Bell className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        ) : (
          <BellOff className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        )}
        {conversation.muted ? 'Reativar notificações' : 'Silenciar'}
      </MenuContextualItem>

      <MenuContextualItem className={estiloItemMenu} onSelect={() => onFixar(conversation)}>
        {conversation.pinned ? (
          <PinOff className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        ) : (
          <Pin className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        )}
        {conversation.pinned ? 'Desafixar' : 'Fixar no topo'}
      </MenuContextualItem>

      <MenuContextualItem className={estiloItemMenu} onSelect={() => onVerIdentidade(conversation)}>
        <Fingerprint className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        Ver identidade
      </MenuContextualItem>

      <MenuContextualItem className={estiloItemMenu} onSelect={() => onCopiarIdentificador(conversation)}>
        <Copy className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        Copiar identificador
      </MenuContextualItem>

      {/*
        Uma sessão verificada é informação, não acção. Aparece aqui
        desactivada para que o menu não mude de forma consoante a conversa:
        um item que ora existe e ora não treina o utilizador a procurar
        onde está a acção.
      */}
      {conversation.verified && (
        <>
          <MenuContextualSeparador className="my-1 bg-onyx-line" />
          <MenuContextualItem className={estiloItemMenu} disabled>
            <ShieldCheck className="h-3.5 w-3.5 shrink-0 text-onyx-info" />
            Sessão verificada fora do canal
          </MenuContextualItem>
        </>
      )}

      <MenuContextualSeparador className="my-1 bg-onyx-line" />

      <MenuContextualItem className={estiloItemMenu} onSelect={() => onLimparHistorico(conversation)}>
        <Trash2 className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        Limpar histórico local
      </MenuContextualItem>

      <MenuContextualItem className={estiloItemMenuDestrutivo} onSelect={() => onBloquear(conversation)}>
        {conversation.contact?.blocked ? (
          <Check className="h-3.5 w-3.5 shrink-0" />
        ) : (
          <Ban className="h-3.5 w-3.5 shrink-0" />
        )}
        {conversation.contact?.blocked ? 'Desbloquear' : 'Bloquear'}
      </MenuContextualItem>
    </>
  );
}