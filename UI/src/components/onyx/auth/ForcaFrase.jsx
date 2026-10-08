// =====================================================================
// ForcaFrase.jsx — o medidor de força da frase de segurança
// ---------------------------------------------------------------------
// Duas barras e um número, e não uma lista de regras.
//
// ## Porque isto não é uma lista de requisitos
//
// A forma comum — «pelo menos 8 caracteres, uma maiúscula, um número, um
// símbolo» — é uma má regra por dois lados ao mesmo tempo. Fixa o
// comprimento e deixa passar `Passw0rd!`, que um dicionário abre em
// segundos; e recusa `cavalos lentos numa mare`, que é ordens de grandeza
// mais forte, só por não ter um ponto de interrogação.
//
// A regra de `messenger/conta.py` é outra: comprimento **e** variedade,
// medida por entropia empírica dos caracteres. Este componente limita-se a
// mostrar o que essa regra diz.
//
// ## Porque é que o texto por baixo é comprido
//
// «Esta frase tem cerca de 40 bits.» é um número. Quem lê fica com a
// impressão de que sabe o que isso significa, e não sabe: não sabe que um
// dicionário faz mil milhões de tentativas em segundos, nem que
// `correct horse battery staple` dá mais de cem bits nesta fórmula e
// continua a ser fraca.
//
// A alternativa — mostrar o número e nada mais — é o que torna as barras de
// força um exercício de conformidade em vez de segurança. Dizer «não
// faltam caracteres: falta variedade» a quem escreveu `aaaaaaaaaaaaaaaa` é
// mais útil do que uma barra a trinta por cento.
//
// ## Porque mede ao escrever e não ao submeter
//
// Porque o objectivo é ajudar, e um erro que só aparece depois de carregar
// no botão já fez a pessoa esperar. A medição é uma multiplicação por
// carácter — não há motivo para a adiar.
// =====================================================================

import React from 'react';
import { cn } from '@/lib/utils';
import { avaliarFrase } from '@/lib/onyx/conta-demo';

/**
 * Os degraus da leitura, do mais baixo ao mais alto.
 *
 * Os rótulos não são «fraco», «médio», «forte». Dizer «forte» a quem
 * escreveu uma frase de vinte e dois caracteres e trinta e cinco bits seria
 * mentira — e é por isso que existe um degrau intermédio com um rótulo
 * incômodo: o número está abaixo do que a conta aceita, e o texto diz isso.
 */
const DEGRAUS = [
  { minimo: 24, rotulo: 'Curta', cor: 'bg-onyx-error/70' },
  { minimo: 45, rotulo: 'Variada, mas quase curta', cor: 'bg-onyx-warning/70' },
  { minimo: 60, rotulo: 'Aceite', cor: 'bg-onyx-success/60' },
  { minimo: 90, rotulo: 'Forte', cor: 'bg-onyx-success' },
];

/** Onde a barra satura. Acima disto não muda nada para quem a vê. */
const SATURACAO = 90;

/**
 * A barra e a legenda da força da frase.
 *
 * Recebe o valor e mostra; não é um campo. O campo é do ecrã que o usa, e
 * duplicar o `<input>` aqui daria dois controlos com o mesmo nome e dois
 * estados a divergir — que é o modo mais rápido de um ecrã de registo
 * mostrar uma força e submeter outra.
 *
 * @param {object} props
 * @param {string} props.frase
 * @param {string} [props.className]
 */
export default function ForcaFrase({ frase, className = undefined }) {
  const { comprimento, bits, forte, problema } = avaliarFrase(frase);

  // Nada escrito, nada mostrado. Uma barra a zero por omissão é decoração.
  if (comprimento === 0) return null;

  const percentagem = Math.min(100, Math.round((bits / SATURACAO) * 100));
  const degrau = [...DEGRAUS].reverse().find((d) => bits >= d.minimo);

  return (
    <div className={cn('mt-2', className)}>
      <div className="h-1 w-full overflow-hidden rounded-full bg-onyx-surface3">
        <div
          className={cn(
            'h-full rounded-full transition-all duration-200',
            degrau ? degrau.cor : 'bg-onyx-error/70'
          )}
          style={{ width: `${percentagem}%` }}
          // O número é o que a barra mede; o texto é o que a pessoa
          // precisa. Uma barra sem texto é um enigma animado.
          role="progressbar"
          aria-valuenow={Math.round(bits)}
          aria-valuemin={0}
          aria-valuemax={SATURACAO}
          aria-label={`Força estimada: ${degrau ? degrau.rotulo : 'insuficiente'}, ${Math.round(bits)} bits`}
        />
      </div>

      <p className="mt-1.5 flex items-baseline justify-between gap-3 text-[11px]">
        <span className={forte ? 'text-onyx-success' : 'text-onyx-text3'}>
          {comprimento} caracteres · {degrau ? degrau.rotulo : 'Insuficiente'}
        </span>
        <span className="shrink-0 font-mono text-onyx-text3">{Math.round(bits)} bits</span>
      </p>

      {/*
        A ressalva aparece sempre que há texto, e não só quando a frase é
        fraca. Quem veja «145 bits» e leia «145 bits» e não mais vai supor
        que está protegida; o que vem a seguir é o que separa «a conta
        aceita» de «está protegido».
      */}
      <p className="mt-1 text-[11px] leading-relaxed text-onyx-text3">
        {problema ||
          'O número mede variedade de caracteres, não resistência a um dicionário. Palavras ditas dão um número alto e continuam a ser adivinháveis.'}
      </p>
    </div>
  );
}