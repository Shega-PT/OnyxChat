import React from 'react';
import { BellOff, ShieldCheck, Users } from 'lucide-react';
import { cn } from '@/lib/utils';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import { formatRelative, truncate } from '@/lib/onyx/format';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.
//
// ## Estados visuais da linha
//
// O prompt que originou esta interface pede cinco estados numa linha de
// lista. Todos estão implementados, e a distinção entre eles importa
// porque comunicam coisas diferentes:
//
//   * **não seleccionado** — `hover:bg-onyx-surface3/70`. Sem marcação.
//   * **seleccionado**    — fundo `bg-onyx-elevated/60` e contorno.
//   * **novo**             — `unread > 0`: nome em `font-semibold`,
//                            pré-visualização mais clara, contador.
//   * **vazio**            — fora desta linha: é o `OnyxEmptyState` que
//                            substitui a lista inteira quando não há nada.
//   * **indisponível**     — `indisponivel`: ver abaixo.
//
// `indisponivel` é o estado que faltava. Não é "sem mensagens": é a
// situação em que a fila existe mas o par não pode ser alcançado agora —
// sem ligação activa, ou com o par bloqueado. Distingui-lo de "vazio" é o
// que impede que o utilizador conclua que alguém lhe apagou a conversa.
// Visualmente desce a saturação de toda a linha e troca o indicador de
// estado por um traço, para que a diferença se leia sem ler texto.

/**
 * Linha de conversa compacta.
 *
 * `onSelect` é um handler de clique, não uma função que recebe o
 * identificador: quem constrói a linha é que fecha sobre o `id`. Dar-lhe
 * a assinatura `(id) => …` seria mais elegante, mas o React passaria
 * aqui o evento de rato e o `id` nunca chegaria ao sítio certo — que é o
 * que acontecia antes de este ficheiro ser tipado.
 *
 * @param {object} props
 * @param {object} props.conversation Conversa, já preparada pelo adaptador.
 * @param {boolean} [props.selected]
 * @param {() => void} [props.onSelect]
 * @param {boolean} [props.indisponivel] Par inalcançável neste momento.
 */
export default function OnyxConversationItem({
  conversation,
  selected = false,
  onSelect,
  indisponivel = false,
}) {
  const isGroup = !conversation.contact;
  const unread = conversation.unread > 0 && !indisponivel;

  // Um par indisponível não tem Activity a mostrar: a data do último
  // contacto deixaria de ser verdadeira. Preferimos o rótulo do estado.
  const previsualizacao = indisponivel
    ? conversation.indisponivelMotivo || 'Sem ligação a este par'
    : `${conversation.lastFromMe ? 'Tu: ' : ''}${conversation.lastMessage}`;

  const Component = onSelect ? 'button' : 'div';

  return (
    <Component
      type={onSelect ? 'button' : undefined}
      onClick={onSelect}
      aria-disabled={indisponivel || undefined}
      className={cn(
        'flex w-full items-start gap-2.5 rounded-md border px-2.5 py-2 text-left transition-colors duration-150',
        selected ? 'border-onyx-line bg-onyx-elevated/60' : 'hover:bg-onyx-surface3/70',
        indisponivel && 'opacity-50'
      )}
    >
      <OnyxAvatar
        seed={conversation.contact ? conversation.contact.id : conversation.avatarSeed}
        name={conversation.title}
        size={36}
        status={indisponivel ? undefined : isGroup ? undefined : conversation.contact.status}
        flat={indisponivel}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span
            className={cn(
              'truncate text-[12.5px] text-onyx-text',
              unread ? 'font-semibold' : 'font-medium'
            )}
          >
            {conversation.title}
          </span>
          {isGroup && <Users className="h-3 w-3 shrink-0 text-onyx-text3" />}
          {conversation.verified && !isGroup && (
            <ShieldCheck className="h-3.5 w-3.5 shrink-0 text-onyx-info/80" aria-label="Sessão verificada" />
          )}
          {conversation.pinned && (
            <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-onyx-metallic/70" />
          )}
          <span className="ml-auto shrink-0 font-mono text-[10px] tracking-wide text-onyx-text3">
            {indisponivel ? <span className="text-onyx-warning">indisponível</span> : formatRelative(conversation.lastAt)}
          </span>
        </div>
        <div className="mt-0.5 flex items-center gap-1.5">
          <span className={cn('truncate text-[11.5px]', unread ? 'text-onyx-text2' : 'text-onyx-text3')}>
            {truncate(previsualizacao, 60)}
          </span>
          {conversation.muted && <BellOff className="h-3 w-3 shrink-0 text-onyx-text3" />}
          {unread && (
            <span className="ml-auto shrink-0 rounded border border-onyx-info/30 bg-onyx-info/15 px-1.5 font-mono text-[10px] leading-4 text-onyx-info">
              {conversation.unread}
            </span>
          )}
        </div>
      </div>
    </Component>
  );
}