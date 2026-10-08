import React from 'react';
import { Ban, Copy, Fingerprint, MessageSquare, ShieldCheck, UserPlus } from 'lucide-react';
import {
  estiloEtiquetaMenu,
  estiloItemMenu,
  estiloItemMenuDestrutivo,
  MenuContextualEtiqueta,
  MenuContextualItem,
  MenuContextualSeparador,
} from '@/components/onyx/OnyxContextMenu';

/**
 * Itens do menu de contexto de uma linha de contacto.
 *
 * Reflete o mesmo princípio de `ConversationContextItems`: estas acções
 * duplicam de propósito as que já existem na vista de detalhe do contacto.
 * O menu de contexto do Radix só é alcançável por teclado com a tecla de
 * menu ou Shift+F10, e o caminho principal tem de continuar a ser o clique
 * simples.
 *
 * @param {object} props
 * @param {object} props.contact Contacto, já preparado pelo adaptador.
 * @param {(contact: object) => void} props.onAbrirConversa
 * @param {(contact: object) => void} props.onVerIdentidade
 * @param {(contact: object) => void} props.onCopiarIdentificador
 * @param {(contact: object) => void} props.onPedirVerificacao
 * @param {(contact: object) => void} props.onBloquear
 */
export default function ContactContextItems({
  contact,
  onAbrirConversa,
  onVerIdentidade,
  onCopiarIdentificador,
  onPedirVerificacao,
  onBloquear,
}) {
  return (
    <>
      <MenuContextualEtiqueta className={estiloEtiquetaMenu}>{contact.name}</MenuContextualEtiqueta>
      <MenuContextualSeparador className="my-1 bg-onyx-line" />

      <MenuContextualItem className={estiloItemMenu} onSelect={() => onAbrirConversa(contact)}>
        <MessageSquare className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        Abrir conversa
      </MenuContextualItem>

      <MenuContextualItem className={estiloItemMenu} onSelect={() => onVerIdentidade(contact)}>
        <Fingerprint className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        Ver identidade
      </MenuContextualItem>

      <MenuContextualItem className={estiloItemMenu} onSelect={() => onCopiarIdentificador(contact)}>
        <Copy className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
        Copiar identificador
      </MenuContextualItem>

      {/*
        A verificação só faz sentido pedir a quem ainda não a fez. Um item
        desactivado seria mais consistente com o menu de conversas, mas aqui
        a ausência comunica melhor: "pedir verificação a alguém já
        verificado" não é uma acção, é um erro de quem clica.
      */}
      {!contact.verified && (
        <MenuContextualItem className={estiloItemMenu} onSelect={() => onPedirVerificacao(contact)}>
          <UserPlus className="h-3.5 w-3.5 shrink-0 text-onyx-text3" />
          Pedir verificação
        </MenuContextualItem>
      )}

      {contact.verified && (
        <MenuContextualItem className={estiloItemMenu} disabled>
          <ShieldCheck className="h-3.5 w-3.5 shrink-0 text-onyx-info" />
          Verificado fora do canal
        </MenuContextualItem>
      )}

      <MenuContextualSeparador className="my-1 bg-onyx-line" />

      <MenuContextualItem className={estiloItemMenuDestrutivo} onSelect={() => onBloquear(contact)}>
        <Ban className="h-3.5 w-3.5 shrink-0" />
        Bloquear
      </MenuContextualItem>
    </>
  );
}