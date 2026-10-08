// =====================================================================
// Fachada tipada sobre os primitivos vendorizados do shadcn.
//
// ## O problema
//
// `src/components/ui/` é gerado pelo `shadcn add`. Os seus componentes
// embrulham o Radix em `React.forwardRef` com **desestruturação por
// descanso**:
//
//     const Botao = React.forwardRef(({ className, variant, ...props }, ref) =>
//         <button ref={ref} className={cn(base, variant, className)} {...props} />)
//
// Numa função assim, o TypeScript infere o tipo das props como o objecto
// das props *nomeadas* — `{ className, variant }` — e **perde o resto**.
// `children`, `onClick` e `aria-label` desaparecem do contrato, e o tipo
// exportado degrada para `IntrinsicAttributes & RefAttributes<any>`.
//
// O efeito é contagioso e não fica no ficheiro do shadcn: propaga-se a
// todos os nossos consumidores. Uma única linha de shadcn mal inferida
// produzia 204 erros, quase todos em código nosso, em ficheiros que não
// tinham nada de errado.
//
// ## Porque não corrigir o shadcn
//
// Três saídas foram avaliadas. A correcção dentro do shadcn resolveria o
// problema e **seria descartada no próximo `shadcn add`** — o que a torna
// enganadora: o código passaria a compilar por um motivo que não sobrevive
// a nenhuma operação de manutenção normal.
//
// `@ts-nocheck` em cada um dos 53 ficheiros esconderia os erros internos
// mas **não** impediria a inferência má de se propagar. Só tornaria o
// problema menos visível.
//
// Declarar os módulos como não tipados também não resolve: uma
// `declare module '@/components/ui/*'` é ignorada quando o caminho
// realmente resolve para um ficheiro do projecto, que é o caso.Foi
// verificado, não presumido.
//
// ## A solução
//
// Este ficheiro. Reexporta os primitivos que usamos e **fixa o tipo** de
// cada um num ponto único, deixando de propagar a inferência quebrada.
//
// O `any` abaixo é deliberado e é o único em todo o projecto. A
// alternativa — reescrever aqui as props de cada primitivo do Radix —
// seria escrever à mão um ficheiro de declarações do Radix, que envelhece
// mal e volta a passar verificações que o próprio Radix já garante. O que
// este `any` compra está escrito: **não verificamos o que é do Radix**.
// O que é nosso continua verificado em pleno.
//
// Fica num ficheiro nosso, num sítio conhecido, e sobrevive ao `shadcn
// add`.
//
// ## Regra de ouro
//
// **Nada em `src/`, fora deste ficheiro, importa directamente de
// `@/components/ui/*`.** Para acrescentar um primitivo, escreve-se
// primeiro a linha correspondente aqui. A vantagem é que a fronteira
// entre código nosso e código gerado passa a ser uma linha explícita, e
// não um hábito disperso por dezenas de ficheiros.
// =====================================================================

import { Button as BotaoRadix } from '@/components/ui/button';
import {
  Dialog as DialogoRadix,
  DialogContent as DialogoConteudoRadix,
  DialogDescription as DialogoDescricaoRadix,
  DialogHeader as DialogoCabecalhoRadix,
  DialogTitle as DialogoTituloRadix,
} from '@/components/ui/dialog';
import {
  DropdownMenu as MenuRadix,
  DropdownMenuContent as MenuConteudoRadix,
  DropdownMenuItem as MenuItemRadix,
  DropdownMenuLabel as MenuEtiquetaRadix,
  DropdownMenuSeparator as MenuSeparadorRadix,
  DropdownMenuTrigger as MenuGatilhoRadix,
} from '@/components/ui/dropdown-menu';
import { Input as EntradaRadix } from '@/components/ui/input';
import {
  InputOTP as CampoOTPRadix,
  InputOTPGroup as GrupoOTPRadix,
  InputOTPSlot as CasaOTPRadix,
} from '@/components/ui/input-otp';
import { Label as RotuloRadix } from '@/components/ui/label';
import { Separator as SeparadorRadix } from '@/components/ui/separator';
import { Switch as InterruptorRadix } from '@/components/ui/switch';
import {
  Tooltip as DicaRadix,
  TooltipContent as DicaConteudoRadix,
  TooltipProvider as DicaProvedorRadix,
  TooltipTrigger as DicaGatilhoRadix,
} from '@/components/ui/tooltip';
import { Toaster as NotificadorRadix } from '@/components/ui/toaster';
import {
  useToast as usarToastRadix,
  toast as notificarRadix,
} from '@/components/ui/use-toast';

/**
 * Substitui a inferência quebrada do primitivo por um contrato verdadeiro.
 *
 * @param {unknown} Componente primitivo vendorizado, já importado.
 * @returns {import('react').FunctionComponent<any>}
 */
const semTipos = (Componente) =>
  /** @type {import('react').FunctionComponent<any>} */ (Componente);

// ---------------------------------------------------------------------
// Reexportações com o tipo corrigido.
// ---------------------------------------------------------------------

/** Botão genérico. Só a autenticação da Base44 o usa; sai na Etapa 2. */
export const Botao = semTipos(BotaoRadix);
/** Campo de texto simples. */
export const Entrada = semTipos(EntradaRadix);
/** Rótulo de campo. */
export const Rotulo = semTipos(RotuloRadix);
/** Divisória horizontal. */
export const Separador = semTipos(SeparadorRadix);
/** Interruptor binário, por baixo de `OnyxSwitch`. */
export const Interruptor = semTipos(InterruptorRadix);

/** Diálogo modal. */
export const Dialogo = semTipos(DialogoRadix);
/** Superfície do diálogo. */
export const DialogoConteudo = semTipos(DialogoConteudoRadix);
/** Cabeçalho do diálogo. */
export const DialogoCabecalho = semTipos(DialogoCabecalhoRadix);
/** Título do diálogo. */
export const DialogoTitulo = semTipos(DialogoTituloRadix);
/** Descrição do diálogo — obrigatória para a acessibilidade do Radix. */
export const DialogoDescricao = semTipos(DialogoDescricaoRadix);

/** Menu suspenso. */
export const Menu = semTipos(MenuRadix);
/** Superfície do menu. */
export const MenuConteudo = semTipos(MenuConteudoRadix);
/** Item do menu. */
export const MenuItem = semTipos(MenuItemRadix);
/** Rótulo de secção dentro do menu. */
export const MenuEtiqueta = semTipos(MenuEtiquetaRadix);
/** Divisória dentro do menu. */
export const MenuSeparador = semTipos(MenuSeparadorRadix);
/** Elemento que abre o menu. */
export const MenuGatilho = semTipos(MenuGatilhoRadix);

/** Dica flutuante. */
export const Dica = semTipos(DicaRadix);
/** Superfície da dica. */
export const DicaConteudo = semTipos(DicaConteudoRadix);
/** Provedor de contexto das dicas. */
export const DicaProvedor = semTipos(DicaProvedorRadix);
/** Elemento que abre a dica. */
export const DicaGatilho = semTipos(DicaGatilhoRadix);

/** Fila de notificações efémeras. */
export const Notificador = semTipos(NotificadorRadix);

/**
 * Campo de entrada de código, um algarismo por casa.
 *
 * Só a autenticação da Base44 o usa (o passo de verificação por OTP do
 * registo); sai na Etapa 2, com o resto desse fluxo.
 */
export const CampoOTP = semTipos(CampoOTPRadix);
/** Agrupa as casas do campo OTP. */
export const GrupoOTP = semTipos(GrupoOTPRadix);
/** Uma casa do campo OTP. */
export const CasaOTP = semTipos(CasaOTPRadix);

/**
 * Notificações efémeras — gancho e função directa.
 *
 * O shadcn expõe as duas coisas a partir do mesmo módulo porque partilham o
 * mesmo estado. Aqui ficam com nomes distintos porque em português uma
 * importing `toast` e uma importing `useToast` lê-se como duas coisas
 * diferentes quando não o são.
 */
export const usarToast = usarToastRadix;
export const notificar = notificarRadix;