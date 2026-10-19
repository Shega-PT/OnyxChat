// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

/**
 * `Armazenamento` — o que esta conta tem guardado.
 *
 * ## A pergunta
 *
 * «O que tenho guardado?» — e a resposta é a lista de tudo, de
 * todas as conversas.
 *
 * E a pergunta é **por conta**, e não por conversa. As transferências de
 * uma conversa são uma coisa — `GET /conversas/{id}/transferencias` — e
 * o armazenamento é outra. São perguntas diferentes, e a interface tem
 * de as responder separadamente.
 *
 * ## E a reabertura
 *
 * Cada linha tem um botão que **abre** o ficheiro, e é a mesma
 * `MEDIA_REPRODUZIR` que toca um áudio: o conteúdo é montado em RAM
 * (`memfd`), selado, e lançado no leitor do sistema.
 *
 * E é a única via de abrir, porque é a única que não põe o conteúdo
 * num ficheiro em disco — e o conteúdo de uma conversa privada é
 * precisamente o que não deve estar num disco que outro processo lê.
 *
 * ## E o que esta página **não** mostra
 *
 * O **caminho** do ficheiro. Não há caminho: o conteúdo vive na base
 * de dados e é montado em memória quando é preciso. Uma lista de
 * caminhos seria informação que não existe e não faz falta.
 */

import { useCallback, useState } from 'react';
import { useAsync } from '@/lib/onyx/use-async';
import { useContextLabel } from '@/lib/onyx/onyx-context';
import { dataAdapter } from '@/lib/onyx/data-adapter';
import { usarToast as useToast } from '@/lib/shadcn';
import { FileText, FolderOpen, Music, Video, Loader2 } from 'lucide-react';
import PageHeader from '@/components/onyx/PageHeader';
import OnyxBadge from '@/components/onyx/OnyxBadge';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxCard from '@/components/onyx/OnyxCard';
import OnyxEmptyState from '@/components/onyx/OnyxEmptyState';
import OnyxLoader from '@/components/onyx/OnyxLoader';
import { formatRelative } from '@/lib/onyx/format';

/**
 * O ícone de um ficheiro, pelo que a transferência é.
 *
 * E pelo **formato**, e não pela extensão: a extensão é o que o
 * emissor escolheu, e o formato é o que o sistema sabe descrever. Um
 * `.dat` que é um vídeo é um vídeo.
 *
 * @param {string} formato
 * @returns {JSX.Element} O ícone.
 */
function IconeDe(formato) {
  if (formato === 'binario') {
    return <FileText className="h-4 w-4" />;
  }
  if (formato === 'ogg' || formato === 'mp3' || formato === 'wav') {
    return <Music className="h-4 w-4" />;
  }
  if (formato === 'mp4' || formato === 'matroska' || formato === 'webm') {
    return <Video className="h-4 w-4" />;
  }
  return <FileText className="h-4 w-4" />;
}

/**
 * O tamanho de um ficheiro, em texto.
 *
 * @param {number} bytes
 * @returns {string}
 */
function tamanhoDe(bytes) {
  if (!bytes) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * A lista de transferências guardadas.
 *
 * E um componente à parte porque o `return` do `Armazenamento` ficava
 * com três níveis de JSX aninhado, e o analisador não aguentava.
 *
 * @param {object} props
 * @param {object[]} props.transferencias
 * @param {string | null} props.aAbrir O id que está a abrir.
 * @param {(t: object) => void} props.aoAbrir
 * @returns {JSX.Element} O elemento.
 */
function ListaDeTransferencias({ transferencias, aAbrir, aoAbrir }) {
  if (transferencias.length === 0) {
    return (
      <OnyxEmptyState
        title="Nada guardado"
        description="Os ficheiros e medias que te enviarem aparecem aqui, e podem ser abertos a qualquer momento."
      />
    );
  }
  return (
    <div className="grid gap-2">
      {transferencias.map((transferencia) => (
        <OnyxCard key={transferencia.id} className="flex items-center gap-3 px-3 py-2.5">
          <span className="text-onyx-text3">
            <IconeDe formato={transferencia.formato} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[12.5px] text-onyx-text">{transferencia.nome}</div>
            <div className="mt-0.5 flex items-center gap-2 text-[10.5px] text-onyx-text3">
              <span>{tamanhoDe(transferencia.totalBytes)}</span>
              {transferencia.contacto ? <span>· {transferencia.contacto}</span> : null}
              {transferencia.recebidoEm ? (
                <span>· {formatRelative(transferencia.recebidoEm)}</span>
              ) : null}
              {transferencia.completo ? null : (
                <OnyxBadge variant="warning">incompleto</OnyxBadge>
              )}
            </div>
          </div>
          <OnyxButton
            variant="ghost"
            size="sm"
            onClick={() => aoAbrir(transferencia)}
            disabled={!transferencia.completo || aAbrir === transferencia.id}
            aria-label="Abrir ficheiro"
          >
            {aAbrir === transferencia.id ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <FolderOpen className="h-3.5 w-3.5" />
            )}
            Abrir
          </OnyxButton>
        </OnyxCard>
      ))}
    </div>
  );
}

export default function Armazenamento() {
  const { toast } = useToast();
  const { loading, data, reload } = useAsync(() => dataAdapter.listStorage(), []);
  const [aAbrir, definirAAbrir] = useState(null);

  const transferencias = data?.transferencias ?? [];
  const total = transferencias.reduce((soma, t) => soma + (t.totalBytes ?? 0), 0);

  useContextLabel(
    transferencias.length ? `${transferencias.length} guardados` : null,
    [transferencias.length],
  );

  /**
   * Abre um ficheiro guardado.
   *
   * E a `MEDIA_REPRODUZIR` — o conteúdo é montado em RAM e lançado no
   * leitor do sistema. E **não** uma leitura directa do ficheiro, que
   * não existe: o conteúdo vive na base de dados.
   *
   * @param {object} transferencia
   */
  const abrir = useCallback(
    async (transferencia) => {
      definirAAbrir(transferencia.id);
      try {
        await dataAdapter.playTransfer(transferencia.id);
        toast({
          title: 'A abrir',
          description: transferencia.nome,
        });
      } catch (erro) {
        toast({
          title: 'Não foi possível abrir',
          description: erro?.message ?? String(erro),
        });
      } finally {
        definirAAbrir(null);
      }
    },
    [toast],
  );

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-onyx-bg">
      <PageHeader
        title="Armazenamento"
        description="Tudo o que esta conta tem guardado, de todas as conversas."
        meta={
          transferencias.length ? (
            <OnyxBadge variant="neutral">
              {transferencias.length} · {tamanhoDe(total)}
            </OnyxBadge>
          ) : null
        }
        actions={
          <OnyxButton variant="ghost" size="sm" onClick={reload} disabled={loading}>
            {loading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <FolderOpen className="h-3.5 w-3.5" />
            )}
            Actualizar
          </OnyxButton>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        {loading ? (
          <OnyxLoader />
        ) : (
          <ListaDeTransferencias
            transferencias={transferencias}
            aAbrir={aAbrir}
            aoAbrir={abrir}
          />
        )}
      </div>
    </div>
  );
}
