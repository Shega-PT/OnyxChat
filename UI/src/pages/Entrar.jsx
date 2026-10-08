// =====================================================================
// Entrar.jsx — abrir a conta local
// ---------------------------------------------------------------------
// Um campo, e ele é a chave.
//
// ## Porque este ecrã não tem «não me lembro»
//
// Teria, se houvesse o que fazer. Este programa não tem servidor para
// enviar um código, não tem pergunta secreta guardada, e não tem
// contacto de ninguém a quem perguntar. Um botão «não me lembro» que
// abre um ecrã de recuperação sem opções reais é a forma mais rápida de
// gastar a confiança de quem o carregou.
//
// O que há, em vez disso, é um link para a cópia de segurança — que é a
// única forma de recuperar que existe, e que depende de a pessoa ter
// feito uma cópia. A ligação está escrita como está: quem não fez cópia
// não vai encontrar o que esperava.
//
// ## Porque o campo não é «palavra-passe»
//
// Porque não é uma palavra-passe. Não há nada contra que comparar: a
// frase é a chave com que o ficheiro é cifrado, e é a etiqueta AEAD que
// diz se a chave é a certa. Chamar-lhe palavra-passe faz alguém escolher
// uma password de oito caracteres com maiúscula e número.
//
// ## Porque a mensagem de erro é igual para tudo
//
// «Frase errada» e «o ficheiro está corrompido» produzem a mesma
// mensagem. Se produzissem mensagens diferentes, quem testasse uma a uma
// observaria a diferença e teria um oráculo. Ver `tests/test_conta.py`,
// que fixa essa indistinguibilidade com um ficheiro adulterado de
// propósito.
// =====================================================================

import React, { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import AuthLayout from '@/components/AuthLayout';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxInput from '@/components/onyx/OnyxInput';
import OnyxLogo from '@/components/onyx/OnyxLogo';
import { useSessao } from '@/lib/SessaoContext';
import { estaEmDemonstracao } from '@/lib/onyx/runtime';

/**
 * Ecrã de entrada.
 *
 * Quando a conta está trancada, a identidade é lida **depois** de abrir.
 * Não antes: enquanto a sessão está fechada, a identidade não é o que
 * decide o que se mostra, e lê-la antes pediria ao servidor um dado que
 * a pessoa ainda não provou ter o direito de ver.
 */
export default function Entrar() {
  const navegar = useNavigate();
  const { entrar } = useSessao();

  const [frase, setFrase] = useState('');
  const [erro, setErro] = useState('');
  const [ocupado, setOcupado] = useState(false);

  async function submeter(evento) {
    evento.preventDefault();
    if (frase.length === 0 || ocupado) return;

    setOcupado(true);
    setErro('');

    const { erro: falha } = await entrar({ frase });
    if (falha) {
      // A frase fica no campo. Limpar o campo depois de um erro obriga a
      // reescrevê-la, e a taxa de erro num campo de texto é alta enough
      // para que isso seja uma injustiça.
      setErro(falha);
      setOcupado(false);
      return;
    }

    navegar('/', { replace: true });
  }

  return (
    <AuthLayout
      icon={OnyxLogo}
      title="OnyxChat"
      subtitle="Escreva a frase de segurança para abrir a conta."
      footer={
        <span>
          <Link to="/recuperar" className="text-onyx-metallic hover:underline">
            Não tenho a frase
          </Link>
        </span>
      }
    >
      <form onSubmit={submeter} className="space-y-4" noValidate>
        <OnyxInput
          label="Frase de segurança"
          type="password"
          value={frase}
          onChange={(e) => setFrase(e.target.value)}
          hint="A mesma que escreveu ao criar a conta. Sem pontos, sem maiúsculas postas."
          autoComplete="current-password"
          autoFocus
          maxLength={256}
        />

        {erro && (
          <p
            role="alert"
            className="rounded-md border border-onyx-error/40 bg-onyx-error/[0.06] px-3 py-2 text-[12px] text-onyx-error"
          >
            {erro}
          </p>
        )}

        <OnyxButton type="submit" variant="primary" className="w-full" disabled={frase.length === 0 || ocupado}>
          {ocupado ? 'A abrir…' : 'Abrir conta'}
        </OnyxButton>

        {/*
          O aviso de que fechar a janela tranca a conta está **debixo** do
          botão, e não em cima: quem já sabe disto não o vai ler, e quem
          não sabe precisa de o ler depois de ver que a conta abriu.

          E é verdade sem ser um aviso genérico de «a sua sessão expira» —
          o mecanismo é concreto, e a pessoa pode verificá-lo.
        */}
        <p className="border-t border-onyx-line pt-3 text-[11px] leading-relaxed text-onyx-text3">
          Fechar esta janela volta a trancar a conta. Não há sessão que dure entre arranques: a
          chave só existe enquanto a aplicação está aberta, e só depois de escrever a frase.
        </p>

        {estaEmDemonstracao() && (
          <p className="text-[11px] text-onyx-text3">
            Demonstração: <code className="font-mono">demo</code> abre qualquer conta.
          </p>
        )}
      </form>
    </AuthLayout>
  );
}