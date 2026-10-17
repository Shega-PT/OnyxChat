// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React, { useCallback, useState } from 'react';
import { FileAudio, FileVideo, Loader2, Play } from 'lucide-react';
import { usarToast as useToast } from '@/lib/shadcn';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { cn } from '@/lib/utils';
import { formatBytes } from '@/lib/onyx/format';

/**
 * Uma transferência de media, mostrada **uma vez**.
 *
 * ## Porque este componente existe separado
 *
 * Uma transferência de 10 MB são 205 linhas na loja — um manifesto e
 * 204 fatias — e cada uma delas é uma mensagem com `mediaId`. Se a lista
 * as desenhasse uma a uma, a pessoa veria 205 bolhas de `ONX1V1F…` e
 * nenhum vídeo. `MessageList` agrupa por `mediaId` e desenha uma aqui.
 *
 * ## E o que a pessoa vê enquanto chega
 *
 * A barra de progresso e o «a chegar». Uma transferência incompleta é
 * informação: escondê-la faz a pessoa concluir que o vídeo não foi
 * enviado, e o que aconteceu é que 42 de 204 fatias já chegaram.
 */
export default function MediaMessage({ transferencia, estadoMedia, outgoing = false }) {
  const { toast } = useToast();
  const [aTocar, setATocar] = useState(false);

  // `estadoMedia` vem de cima e **não** é perguntado aqui: ele descreve
  // a máquina — há `mpv`, o daemon tem a loja — e não o ficheiro. Três
  // transferências na mesma conversa eram três perguntas ao daemon para
  // a mesma resposta, e cada uma é uma ida ao IPC.

  const reproduzir = useCallback(async () => {
    setATocar(true);
    try {
      await dataAdapter.playTransfer(transferencia.id);
    } catch (falha) {
      toast({
        title: 'Não foi possível reproduzir',
        description: falha?.message ?? 'O ficheiro não pôde ser montado.',
        variant: 'destructive',
      });
    } finally {
      setATocar(false);
    }
  }, [transferencia.id, toast]);

  const completo = Boolean(transferencia.completo);
  const Icon = transferencia.tipo === 'audio' ? FileAudio : FileVideo;
  const recebidas = transferencia.recebidasPedacos ?? 0;
  const total = transferencia.totalPedacos || 0;

  return (
    <div
      className={cn(
        'flex w-full',
        outgoing ? 'justify-end' : 'justify-start'
      )}
    >
      <div
        className={cn(
          'w-[min(320px,78%)] rounded-md border px-3 py-2.5',
          outgoing
            ? 'border-onyx-elevated2/70 bg-onyx-elevated2'
            : 'border-onyx-line bg-onyx-surface3'
        )}
      >
        <div className="flex items-center gap-2.5">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-onyx-elevated/60">
            <Icon className="h-4 w-4 text-onyx-metallic2" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] font-medium text-onyx-text">
              {transferencia.nome}
            </p>
            <p className="text-[11px] text-onyx-text3">
              {transferencia.tipo === 'audio' ? 'Áudio' : 'Vídeo'} ·{' '}
              {formatBytes(transferencia.totalBytes)}
              {transferencia.duracaoS ? ` · ${transferencia.duracaoS}s` : ''}
            </p>
          </div>
          <button
            type="button"
            onClick={reproduzir}
            disabled={!completo || aTocar || !estadoMedia?.disponivel}
            aria-label={completo ? `Reproduzir ${transferencia.nome}` : 'Ainda a chegar'}
            className={cn(
              'flex h-8 w-8 shrink-0 items-center justify-center rounded-full border transition-colors',
              completo && estadoMedia?.disponivel
                ? 'border-onyx-accent/50 text-onyx-accent hover:bg-onyx-accent/10'
                : 'cursor-not-allowed border-onyx-line text-onyx-text3/50'
            )}
          >
            {aTocar ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Play className="h-4 w-4" />
            )}
          </button>
        </div>

        {/* A barra só existe a meio. Um ficheiro completo não precisa de
            uma barra a 100%, e mostrá-la seria ruído. */}
        {!completo && (
          <div className="mt-2">
            <div
              className="h-1 overflow-hidden rounded-full bg-onyx-elevated"
              role="progressbar"
              aria-valuenow={recebidas}
              aria-valuemin={0}
              aria-valuemax={total || 1}
            >
              <div
                className="h-full rounded-full bg-onyx-accent/70 transition-[width]"
                style={{
                  width: total ? `${Math.min(100, (recebidas / total) * 100)}%` : '0%',
                }}
              />
            </div>
            <p className="mt-1 font-mono text-[10px] text-onyx-text3">
              a chegar · {recebidas} de {total}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}