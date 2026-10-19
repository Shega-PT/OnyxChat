// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React, { useCallback, useEffect, useState } from 'react';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { usarToast as useToast } from '@/lib/shadcn';
import { Bell, BellOff, ChevronLeft, Lock, MoreHorizontal, Phone, PhoneOff, ShieldCheck, Trash2, User, Ban, Video, VideoOff } from 'lucide-react';
import {
  Menu as DropdownMenu,
  MenuConteudo as DropdownMenuContent,
  MenuGatilho as DropdownMenuTrigger,
  MenuItem as DropdownMenuItem,
  MenuSeparador as DropdownMenuSeparator,
} from '@/lib/shadcn';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton, { OnyxIconButton } from '@/components/onyx/OnyxButton';
import OnyxTooltip from '@/components/onyx/OnyxTooltip';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import MessageList from '@/components/onyx/MessageList';
import MessageComposer from '@/components/onyx/MessageComposer';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { truncate } from '@/lib/onyx/format';
import { cn } from '@/lib/utils';

export default function ConversationView({
  conversation,
  contact = undefined,
  onSend,
  onSendFile = undefined,
  onBack = undefined,
  className = undefined,
}) {
  const { openModal } = useOnyxUI();
  const { toast } = useToast();
  const [transferencias, setTransferencias] = useState([]);
  const [estadoMedia, setEstadoMedia] = useState(null);

  // O estado da chamada, e quem a pessoa está a chamar.
  //
  // E **aqui** que vive a escolha de quem chama, e não no `TopBar`: o
  // `TopBar` é a barra global, e uma chamada é sempre a **uma** pessoa
  // — que é a pessoa com quem se está a falar nesta conversa.
  //
  // E os dois botões são separados porque uma chamada de áudio e uma
  // de vídeo são coisas diferentes: a de áudio não precisa de câmara,
  // e quem está a receber um convite de áudio não quer ver a cara de
  // quem liga.
  const [chamada, definirChamada] = useState(null);


  // As transferências são buscadas à parte das mensagens porque são
  // uma **lista**, e a conversa traz as linhas. As duas coisas estão
  // ligadas pelo `mediaId`, e é essa ligação que o `MessageList` usa
  // para desenhar 205 linhas como uma.
  //
  // E é uma busca por conversa, não por linha: `GET …/transferencias`
  // devolve a lista inteira, e pedir o detalhe por linha seriam 205
  // pedidos para a mesma informação.
  useEffect(() => {
    if (!conversation?.id) {
      setTransferencias([]);
      return undefined;
    }
    let vivo = true;
    dataAdapter
      .listTransfers(conversation.id)
      .then((lista) => {
        if (vivo) setTransferencias(lista);
      })
      .catch(() => {
        // Sem transferências, a conversa é uma conversa de texto. A
        // loja pode estar a uma versão anterior, ou o sidecar pode não
        // conhecer a rota — e nenhuma das duas é motivo para esconder
        // as mensagens.
        if (vivo) setTransferencias([]);
      });
    return () => {
      vivo = false;
    };
  }, [conversation?.id]);

  // O estado de media é uma **pergunta sobre a máquina**, não sobre o
  // ficheiro: há `mpv` instalado, o daemon tem a loja apontada. Por isso
  // vive aqui e não em cada transferência — três transferências na mesma
  // conversa eram três idas ao daemon para a mesma resposta, e cada uma
  // delas é uma ida ao IPC.
  useEffect(() => {
    let vivo = true;
    dataAdapter
      .getMediaState()
      .then((e) => {
        if (vivo) setEstadoMedia(e);
      })
      .catch(() => {
        // Um sidecar sem media não é um sidecar partido: o botão fica
        // indisponível e o resto da conversa fica como estava.
        if (vivo) setEstadoMedia({ disponivel: false, motivo: 'indisponível' });
      });
    return () => {
      vivo = false;
    };
  }, []);

  /**
   * Liga uma chamada a quem está nesta conversa.
   *
   * @param {boolean} comVideo `true` para vídeo, `false` para áudio.
   */
  const aoChamar = useCallback(
    (comVideo) => {
      if (!contact) {
        toast({
          title: 'Não se pode chamar um grupo',
          description: 'Uma chamada é sempre a uma pessoa.',
        });
        return;
      }
      definirChamada({ estado: 'a-tocar', comVideo, contacto: contact.id });
      toast({
        title: comVideo ? 'A chamar com vídeo…' : 'A chamar…',
        description: truncate(contact ? contact.name : conversation.title, 40),
      });
    },
    [contact, conversation.title, toast],
  );

  /**
   * Termina a chamada, se houver.
   */
  const aoDesligar = useCallback(() => {
    if (!chamada) {
      return;
    }
    definirChamada(null);
    toast({ title: 'Chamada terminada' });
  }, [chamada, toast]);

  if (!conversation) {
    return (
      <div className={cn('flex min-w-0 flex-1 items-center justify-center bg-onyx-bg', className)}>
        <OnyxEmptyState
          title="Nenhuma conversa selecionada"
          description="Escolhe uma conversa na lista para veres as mensagens. Todas as sessões são cifradas ponta a ponta."
          hint="⌘K pesquisa · ⌘N nova conversa"
        />
      </div>
    );
  }

  const isGroup = !contact;
  const title = contact ? contact.name : conversation.title;


  const openSessionInfo = () => {
    openModal({
      type: 'info',
      payload: {
        title: 'Estado da sessão',
        description: `Sessão com ${title}. Cifra negociada no handshake e chaves renovadas a cada rotação.`,
        rows: [
          { label: 'Rota', value: conversation.route === 'direto' ? 'Ligação direta' : 'Via relay' },
          { label: 'Latência', value: `${conversation.latency} ms` },
          { label: 'Transporte', value: 'QUIC · UDP' },
          { label: 'Cifra', value: 'X25519 · AES-256-GCM' },
          { label: 'Impressão digital', value: contact?.fingerprint || '—', mono: true },
        ],
        note: 'Compara a impressão digital por um canal separado antes de confiar nesta sessão.',
      },
    });
  };

  return (
    <div className={cn('flex min-w-0 flex-1 flex-col bg-onyx-bg', className)}>
      <header className="flex h-[60px] shrink-0 items-center gap-3 border-b border-onyx-line bg-onyx-surface px-3 md:px-4">
        <OnyxIconButton className="lg:hidden" onClick={onBack} aria-label="Voltar à lista">
          <ChevronLeft className="h-4 w-4" />
        </OnyxIconButton>
        <OnyxAvatar
          seed={contact ? contact.id : conversation.avatarSeed}
          name={title}
          size={38}
          status={contact ? contact.status : undefined}
        />
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="truncate text-[13.5px] font-medium text-onyx-text">{title}</h2>
            {conversation.verified && (
              <OnyxBadge variant="secure" title="Sessão verificada fora do canal">
                <ShieldCheck className="h-3 w-3" />
                verificada
              </OnyxBadge>
            )}
            {isGroup && <OnyxBadge>{conversation.members} membros</OnyxBadge>}
          </div>
          <div className="mt-0.5 flex items-center gap-2">
            {contact ? <OnyxStatus status={contact.status} size="sm" /> : <OnyxStatus status="secure" size="sm" label="Grupo cifrado" />}
            <span className="hidden truncate font-mono text-[10px] tracking-wide text-onyx-text3 sm:inline">
              {contact ? contact.identifier : `salas/${conversation.id}`}
            </span>
          </div>
        </div>

        <div className="ml-auto flex items-center gap-1">
          {/* Os botões de chamada, e são dois porque áudio e vídeo são
              coisas diferentes. A chamada de áudio não precisa de câmara
              — e quem recebe um convite de áudio não quer ver a cara de
              quem liga. */}
          {chamada ? (
            <OnyxTooltip label="Desligar">
              <OnyxIconButton onClick={aoDesligar} aria-label="Desligar">
                {chamada.comVideo ? (
                  <VideoOff className="h-4 w-4" />
                ) : (
                  <PhoneOff className="h-4 w-4" />
                )}
              </OnyxIconButton>
            </OnyxTooltip>
          ) : (
            <>
              <OnyxTooltip label="Chamada de áudio">
                <OnyxIconButton
                  onClick={() => aoChamar(false)}
                  aria-label="Chamada de áudio"
                  disabled={isGroup}
                >
                  <Phone className="h-4 w-4" />
                </OnyxIconButton>
              </OnyxTooltip>
              <OnyxTooltip label="Chamada de vídeo">
                <OnyxIconButton
                  onClick={() => aoChamar(true)}
                  aria-label="Chamada de vídeo"
                  disabled={isGroup}
                >
                  <Video className="h-4 w-4" />
                </OnyxIconButton>
              </OnyxTooltip>
            </>
          )}
          <OnyxTooltip label="Verificar sessão">
            <OnyxIconButton onClick={openSessionInfo} aria-label="Verificar sessão">
              <ShieldCheck className="h-4 w-4" />
            </OnyxIconButton>
          </OnyxTooltip>
          <OnyxTooltip label={conversation.muted ? 'Reativar notificações' : 'Silenciar conversa'}>
            <OnyxIconButton
              onClick={() =>
                toast({
                  title: conversation.muted ? 'Notificações reativadas' : 'Conversa silenciada',
                  description: truncate(title, 40),
                })
              }
              aria-label="Silenciar"
            >
              {conversation.muted ? <BellOff className="h-4 w-4" /> : <Bell className="h-4 w-4" />}
            </OnyxIconButton>
          </OnyxTooltip>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <OnyxIconButton aria-label="Mais ações">
                <MoreHorizontal className="h-4 w-4" />
              </OnyxIconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              className="w-52 rounded-md border border-onyx-line bg-onyx-surface2 p-1 text-onyx-text2 shadow-xl shadow-black/50"
            >
              <DropdownMenuItem
                onSelect={openSessionInfo}
                className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
              >
                <Lock className="h-3.5 w-3.5" /> Detalhes da cifra
              </DropdownMenuItem>
              <DropdownMenuItem
                onSelect={() => openModal({ type: 'identityCard', payload: { contact } })}
                disabled={!contact}
                className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
              >
                <User className="h-3.5 w-3.5" /> Ver identidade
              </DropdownMenuItem>
              <DropdownMenuItem
                onSelect={() =>
                  toast({ title: 'Histórico local limpo', description: 'As mensagens no servidor mantêm-se intactas.' })
                }
                className="cursor-pointer gap-2 rounded text-[12.5px] focus:bg-onyx-surface3 focus:text-onyx-text"
              >
                <Trash2 className="h-3.5 w-3.5" /> Limpar histórico local
              </DropdownMenuItem>
              <DropdownMenuSeparator className="my-1 bg-onyx-line" />
              <DropdownMenuItem
                onSelect={() =>
                  openModal({
                    type: 'confirm',
                    payload: {
                      title: 'Bloquear contacto',
                      description: `${title} deixa de poder enviar pedidos ou mensagens. Podes desbloquear em Segurança.`,
                      confirmLabel: 'Bloquear',
                      variant: 'danger',
                      onConfirm: () => toast({ title: 'Contacto bloqueado', description: truncate(title, 40) }),
                    },
                  })
                }
                className="cursor-pointer gap-2 rounded text-[12.5px] text-onyx-error focus:bg-onyx-error/10 focus:text-onyx-error"
              >
                <Ban className="h-3.5 w-3.5" /> Bloquear
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>

      <MessageList
        messages={conversation.messages}
        transferencias={transferencias}
        estadoMedia={estadoMedia}
        isGroup={isGroup}
      />

      <div className="mx-auto flex w-full max-w-[860px] items-center gap-2 px-4 pb-1 pt-2">
        <OnyxButton variant="ghost" size="sm" onClick={openSessionInfo} className="text-onyx-text3">
          <Lock className="h-3.5 w-3.5" />
          {conversation.route === 'direto' ? 'Ligação direta' : 'Via relay'} · {conversation.latency} ms
        </OnyxButton>
      </div>

      <MessageComposer
        onSend={onSend}
        onSendFile={onSendFile}
        recipientName={contact ? contact.name : title}
        route={conversation.route}
      />
    </div>
  );
}