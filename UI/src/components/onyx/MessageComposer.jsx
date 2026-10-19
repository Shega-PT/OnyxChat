// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import React, { useEffect, useRef, useState } from 'react';
import { Lock, Paperclip, Send } from 'lucide-react';
import { cn } from '@/lib/utils';
import { OnyxKbd } from '@/components/onyx/OnyxBadge';
import OnyxButton, { OnyxIconButton } from '@/components/onyx/OnyxButton';

export default function MessageComposer({
  onSend,
  onSendFile = undefined,
  onErro = undefined,
  recipientName = undefined,
  route = 'direto',
  disabled = false,
}) {
  const [value, setValue] = useState('');
  const [focused, setFocused] = useState(false);
  const [ficheiro, setFicheiro] = useState(null);
  const textareaRef = useRef(null);
  const ficheiroRef = useRef(null);

  useEffect(() => {
    const element = textareaRef.current;
    if (!element) return;
    element.style.height = 'auto';
    element.style.height = `${Math.min(element.scrollHeight, 140)}px`;
  }, [value]);

  const submit = () => {
    const text = value.trim();
    if (!text || disabled) return;
    onSend(text);
    setValue('');
  };

  /**
   * Escolhe um ficheiro e envia-o.
   *
   * E a acção que traz o envio de ficheiro: antes, o sistema só sabia enviar
   * media — áudio e vídeo — porque o emissor de transferências só
   * aceitava extensões de media.
   *
   * E o ficheiro é lido **inteiro** e enviado em base-64, que é o que a
   * rota aceita. O custo é 33 % de tamanho, e é o mesmo que a media já
   * paga.
   */
  const enviarFicheiro = async (evento) => {
    const escolhido = evento.target.files?.[0];
    if (!escolhido || !onSendFile) return;
    // O valor do `input` é limpo para que escolher o **mesmo** ficheiro
    // outra vez dispare o evento — sem isto, a segunda escolha do mesmo
    // ficheiro não faz nada e parece um bug.
    evento.target.value = '';
    setFicheiro(escolhido);
    try {
      const conteudo = await lerComoBase64(escolhido);
      await onSendFile(escolhido.name, conteudo);
    } catch (erro) {
      // A falha vai para quem chamou. O `MessageComposer` não tem um
      // canal de erro próprio, e a mensagem que a pessoa vê quando o
      // ficheiro não vai é do mesmo sítio que a da mensagem que não
      // vai.
      setFicheiro(null);
      if (onErro) {
        onErro(erro);
      }
    }
  };

  const handleKeyDown = (event) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  };

  return (
    <div className="shrink-0 border-t border-onyx-line bg-onyx-surface px-3 pb-2.5 pt-3">
      <div
        className={cn(
          'flex items-end gap-2 rounded-lg border bg-onyx-surface2 px-2.5 py-2 transition-colors duration-150',
          focused ? 'border-onyx-metallic/40 ring-2 ring-onyx-metallic/10' : 'border-onyx-line'
        )}
      >
        {onSendFile ? (
          <>
            <input
              ref={ficheiroRef}
              type="file"
              className="hidden"
              onChange={enviarFicheiro}
              aria-label="Escolher ficheiro"
            />
            <OnyxIconButton
              variant="ghost"
              size="icon"
              onClick={() => ficheiroRef.current?.click()}
              disabled={disabled}
              aria-label="Enviar ficheiro"
            >
              <Paperclip className="h-4 w-4" />
            </OnyxIconButton>
          </>
        ) : null}
        <textarea
          ref={textareaRef}
          rows={1}
          value={value}
          disabled={disabled}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          placeholder={recipientName ? `Mensagem cifrada para ${recipientName}` : 'Mensagem cifrada'}
          className="max-h-[140px] min-h-[26px] flex-1 resize-none bg-transparent py-1 text-[13.5px] leading-[1.5] text-onyx-text placeholder:text-onyx-text3 focus:outline-none disabled:opacity-50"
        />
        <OnyxButton
          variant="primary"
          size="icon"
          onClick={submit}
          disabled={!value.trim() || disabled}
          aria-label="Enviar mensagem"
        >
          <Send className="h-4 w-4" />
        </OnyxButton>
      </div>
      {ficheiro ? (
        <div className="mt-1.5 px-1 text-[10.5px] text-onyx-text3">A enviar {ficheiro.name}…</div>
      ) : null}
      <div className="mt-2 flex items-center gap-3 px-1">
        <span className="flex items-center gap-1.5">
          <Lock className="h-3 w-3 text-onyx-info/70" />
          <span className="text-[10.5px] text-onyx-text3">
            Cifrado ponta a ponta · {route === 'direto' ? 'rota direta' : 'via relay'}
          </span>
        </span>
        <span className="ml-auto hidden items-center gap-2 text-[10.5px] text-onyx-text3 sm:flex">
          <OnyxKbd>Enter</OnyxKbd> enviar
          <OnyxKbd>Shift + Enter</OnyxKbd> nova linha
        </span>
      </div>
    </div>
  );
}

/**
 * Lê um ficheiro e devolve o conteúdo em base-64.
 *
 * E o `FileReader` e não `arrayBuffer()`, porque o `arrayBuffer` é uma
 * promessa e a interface tem de **esperar** por ela — e o `FileReader`
 * com uma promessa por fora é a mesma coisa com mais código.
 *
 * E o resultado é base-64 e não um `ArrayBuffer`, porque a rota o
 * quer em base-64 e convertê-lo no `fetch` custaria o mesmo trabalho
 * noutro sítio.
 *
 * @param {File} ficheiro
 * @returns {Promise<string>} O conteúdo em base-64.
 */
function lerComoBase64(ficheiro) {
  return new Promise((resolver, rejeitar) => {
    const leitor = new FileReader();
    leitor.onerror = () => rejeitar(leitor.error ?? new Error('a leitura falhou'));
    leitor.onload = () => resolver(String(leitor.result ?? ''));
    leitor.readAsDataURL(ficheiro);
  })
    .then((dados) => {
      // O `readAsDataURL` dá `data:<tipo>;base64,<conteúdo>`, e o que
      // a rota quer é só o conteúdo.
      const virgula = dados.indexOf(',');
      return virgula >= 0 ? dados.slice(virgula + 1) : dados;
    });
}