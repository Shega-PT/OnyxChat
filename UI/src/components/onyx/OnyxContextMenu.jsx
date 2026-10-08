import React from 'react';
import { cn } from '@/lib/utils';
import {
  MenuContextual as RaizMenuContextual,
  MenuContextualConteudo,
  MenuContextualEtiqueta,
  MenuContextualItem,
  MenuContextualSeparador,
  MenuContextualGatilho as GatilhoMenuContextual,
} from '@/lib/shadcn';

/**
 * Estilo único das superfícies de menu (contexto e suspenso).
 *
 * Existe como função e não como constante para não ser confundido com uma
 * classe fixa: as duas famílias de menu partilham a mesma folha de estilo
 * de propósito, e mudar uma delas sem a outra produziria menus com cores
 * diferentes dentro da mesma aplicação.
 *
 * As cores usam os mesmos tokens de `dropdown-menu`, para que o menu de
 * contexto e o menu do botão pareçam o mesmo elemento em dois momentos
 * distintos.
 */
export const estiloMenu = (extra = undefined) =>
  cn(
    'w-52 rounded-md border border-onyx-line bg-onyx-surface2 p-1 text-onyx-text2 shadow-xl shadow-black/50',
    extra
  );

/** Item de menu, alinhado com o peso da barra superior. */
export const estiloItemMenu = cn(
  'flex cursor-pointer select-none items-center gap-2 rounded px-2 py-1.5 text-[12.5px] outline-none',
  'focus:bg-onyx-surface3 focus:text-onyx-text',
  'data-[disabled]:pointer-events-none data-[disabled]:opacity-45'
);

/** Rótulo de secção dentro de um menu. */
export const estiloEtiquetaMenu = cn(
  'px-2 py-1.5 font-mono text-[9.5px] uppercase tracking-[0.14em] text-onyx-text3'
);

/**
 * Item de menu destrutivo.
 *
 * Só o texto muda de cor — o fundo mantém-se o da paleta. Um fundo
 * vermelho sólido em cada menu que se abre é ruído; o texto a vermelho
 * chega para marcar a acção como destrutiva.
 */
export const estiloItemMenuDestrutivo = cn(estiloItemMenu, 'text-onyx-error focus:bg-onyx-error/10 focus:text-onyx-error');

/**
 * Contentor de menu de contexto.
 *
 * O gatilho usa `asChild` para que o menu abra **sobre a linha** e não
 * sobre um rectângulo à volta dela. É o que dá a sensação de que a linha
 * inteira é o alvo, e não um ícone dentro dela.
 *
 * Quando não há itens, devolve o filho sem invólucro nenhum. Envolver
 * cada linha num contexto vazio custaria um contexto do Radix por linha
 * sem qualquer efeito.
 *
 * `aberto` e `onFechar` são controladas pelo utilizador em vez de geridas
 * internamente, para que uma vista possa manter o menu aberto enquanto
 * confirma uma acção num diálogo por cima.
 */
export default function OnyxContextMenu({
  aberto = false,
  onFechar = undefined,
  itens = undefined,
  children,
}) {
  if (!itens) return children;

  return (
    <RaizMenuContextual open={aberto} onOpenChange={(valor) => !valor && onFechar?.()}>
      <GatilhoMenuContextual asChild>{children}</GatilhoMenuContextual>
      <MenuContextualConteudo className={estiloMenu()}>
        {itens}
      </MenuContextualConteudo>
    </RaizMenuContextual>
  );
}

export {
  MenuContextualEtiqueta,
  MenuContextualItem,
  MenuContextualSeparador,
};