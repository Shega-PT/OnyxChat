import React from 'react';
import {
  Dica as Tooltip,
  DicaConteudo as TooltipContent,
  DicaGatilho as TooltipTrigger,
  DicaProvedor as TooltipProvider,
} from '@/lib/shadcn';
import { OnyxKbd } from '@/components/onyx/OnyxBadge';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

/**
 * Dica flutuante, com atalho de teclado opcional.
 *
 * Cada dica instala o seu próprio `TooltipProvider`, o que é o que o Radix
 * exige: o componente tem de poder viver isolado, sem um fornecedor
 * montado à volta da aplicação inteira. O custo é um contexto por dica,
 * desprezável para a dúzia que a barra superior usa.
 */
export default function OnyxTooltip({
  label,
  kbd = undefined,
  side = 'top',
  children,
  delay = 240,
}) {
  return (
    <TooltipProvider delayDuration={delay}>
      <Tooltip>
        <TooltipTrigger asChild>{children}</TooltipTrigger>
        <TooltipContent
          side={side}
          className="flex items-center gap-2 rounded !border !border-onyx-line !bg-onyx-elevated px-2 py-1 text-[11px] !text-onyx-text shadow-xl shadow-black/40"
        >
          {label}
          {kbd && <OnyxKbd>{kbd}</OnyxKbd>}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}