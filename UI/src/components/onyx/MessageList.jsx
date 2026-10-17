// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React, { useEffect, useRef } from 'react';
import MediaMessage from '@/components/onyx/MediaMessage';
import OnyxMessage from '@/components/onyx/OnyxMessage';
import { formatDayLabel } from '@/lib/onyx/format';

function DaySeparator({ label }) {
  return (
    <div className="my-3 flex items-center gap-3">
      <span className="h-px flex-1 bg-onyx-line" />
      <span className="onyx-label">{label}</span>
      <span className="h-px flex-1 bg-onyx-line" />
    </div>
  );
}

/**
 * Uma linha da lista: ou texto, ou uma transferência completa.
 *
 * ## Porquê isto tem de existir
 *
 * Uma transferência de 10 MB são 205 linhas na loja — um manifesto e
 * 204 fatias — todas com o mesmo `mediaId`. Desenhá-las uma a uma daria
 * 205 bolhas de `ONX1V1F…`, e nenhum vídeo. É o mesmo motivo de o
 * `transferencia_id` existir na base: a transferência é uma coisa, e as
 * fatias são como ela chega.
 *
 * ## E cada transferência fica na posição da sua última linha
 *
 * Que é o sítio onde a pessoa espera que a conversa continue. Colocá-la
 * na posição do manifesto faria um vídeo de 10 MB empurrar para baixo
 * tudo o que foi dito a seguir — e o vídeo chega *depois*, que é
 * quando a pessoa está a falar de outra coisa.
 *
 * ## E o que fica sem par
 *
 * Uma linha com `mediaId` e sem transferência correspondente não é
 * descartada: é mostrada como texto. A loja recusa essa combinação na
 * escrita, mas uma base vinda de outra versão pode tê-la, e esconder
 * mensagens é pior do que mostrar uma bolha estranha.
 *
 * ## Duas passagens e não uma
 *
 * A primeira marca o **último** índice de cada `mediaId`; a segunda
 * decide o que desenhar. Numa só, com um `Set` de «já vi», a
 * transferência apareceria na posição do **manifesto** — que é o
 * primeiro a chegar, e portanto a última linha a ser corrigida.
 *
 * E é duas passagens em vez de uma com `indexOf` e `some` dentro do
 * laço porque isso era O(n²): numa conversa com três transferências de
 * 10 MB são 615 linhas, e 615² comparações a cada render.
 */
function agruparPorTransferencia(messages, transferencias) {
  const porId = new Map(transferencias.map((t) => [t.id, t]));

  const ultimoDe = new Map();
  messages.forEach((mensagem, i) => {
    const id = mensagem.mediaId;
    if (id && porId.has(id)) ultimoDe.set(id, i);
  });

  const saida = [];
  messages.forEach((mensagem, i) => {
    const id = mensagem.mediaId;
    if (id && porId.has(id)) {
      if (ultimoDe.get(id) === i) saida.push({ mensagem, transferencia: porId.get(id) });
      return;
    }
    saida.push({ mensagem, transferencia: null });
  });

  return saida;
}

export default function MessageList({
  messages = [],
  transferencias = [],
  estadoMedia = null,
  isGroup = false,
}) {
  const containerRef = useRef(null);

  useEffect(() => {
    const element = containerRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [messages.length]);

  const linhas = agruparPorTransferencia(messages, transferencias);
  let lastDay = null;

  return (
    <div ref={containerRef} className="min-h-0 flex-1 overflow-y-auto px-4 py-2">
      <div className="mx-auto flex max-w-[860px] flex-col gap-1.5">
        {linhas.map(({ mensagem, transferencia }) => {
          const day = formatDayLabel(mensagem.at);
          const showDay = day !== lastDay;
          lastDay = day;
          return (
            <React.Fragment key={transferencia ? `t:${transferencia.id}` : mensagem.id}>
              {showDay && <DaySeparator label={day} />}
              {transferencia ? (
                <MediaMessage
                  transferencia={transferencia}
                  estadoMedia={estadoMedia}
                  outgoing={mensagem.from === 'me'}
                />
              ) : (
                <OnyxMessage message={mensagem} showAuthor={isGroup} />
              )}
            </React.Fragment>
          );
        })}
        <div className="h-2" />
      </div>
    </div>
  );
}
