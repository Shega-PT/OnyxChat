// =====================================================================
// Registar.jsx — criar a conta local
// ---------------------------------------------------------------------
// Três campos, e o terceiro é o mais importante: a frase de segurança
// vai ser a **única** forma de abrir a conta outra vez.
//
// ## Porque é que a confirmação da frase não é um «repita a palavra-passe»
//
// Que duas vezes é uma verificação de escrita. Que uma frase duas vezes é
// uma verificação de **memória**: quem escreve duas vezes a mesma frase
// está a confirmar que a sabe de cor, e quem a copiou do gestor de
// palavras-passe não está a confirmar nada. A segunda caixa não é
// redundante — é a diferença entre uma conta que se abre amanhã e uma
// que se perdeu.
//
// ## O que este ecrã promete e não cumpre
//
// Promete que a frase protege a conta. Não promete que se possa recuperar.
//
// Não há correio, não há SMS, não há pergunta secreta. Este programa não
// tem servidor para o qual mandar uma resposta, e um ecrã que pedisse um
// endereço estaria a prometer uma recuperação que não existe. A única
// cópia são os ficheiros de que a pessoa foi avisada mais abaixo.
//
// ## Porquê o aviso de «esta frase é a conta»
//
// Porque é verdade e porque é o que se esquece. Quem escolhe uma frase
// sem pensar sabe que a pode repetir; quem escolhe uma frase por
// esquecimento esquece que a frase tem de ser lembrada por si e não por
// um programa. O aviso não substitui a boa frase — só diz o que a boa
// frase custa.
// =====================================================================

import React, { useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import AuthLayout from '@/components/AuthLayout';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxInput from '@/components/onyx/OnyxInput';
import OnyxLogo from '@/components/onyx/OnyxLogo';
import ForcaFrase from '@/components/onyx/auth/ForcaFrase';
import { useSessao } from '@/lib/SessaoContext';
import { avaliarFrase } from '@/lib/onyx/conta-demo';
import { estaEmDemonstracao } from '@/lib/onyx/runtime';

/**
 * Ecrã de registo.
 *
 * Não recebe utilizador, nome ou identidade da navegação: quem regista
 * numa demonstração não é a pessoa que está a ler, e quem chega aqui a
 * partir de um ecrã de entrada não trouxe perfil nenhum. O ecrã tem três
 * campos e mais nenhum pressuposto.
 */
export default function Registar() {
  const navegar = useNavigate();
  const { registar } = useSessao();

  const [utilizador, setUtilizador] = useState('');
  const [frase, setFrase] = useState('');
  const [confirmacao, setConfirmacao] = useState('');
  const [erro, setErro] = useState('');
  const [ocupado, setOcupado] = useState(false);

  // A força é lida a cada escrita, e é a mesma que o ecrã precisa para
  // decidir se o botão pode ser carregado. Calculá-la em dois sítios daria
  // duas respostas que já podem divergir.
  const forca = useMemo(() => avaliarFrase(frase), [frase]);

  /** Se o formulário pode ser submetido agora. */
  const pronto =
    utilizador.trim().length > 0 && forca.forte && confirmacao.length > 0 && !ocupado;

  /**
   * O que impede o envio vai no campo, não numa caixa em cima.
   *
   * Uma caixa de erro diz que houve um problema; o campo diz o que
   * escrever. Quem lê a mensagem primeiro — o botão — precisa de saber o
   * que fazer, e «a frase não é aceite» sem dizer qual das duas regras
   * falhou não diz.
   */
  const problemaConfirmacao =
    confirmacao.length > 0 && confirmacao !== frase
      ? 'As duas frases não são iguais.'
      : undefined;
  const problemaNome = utilizador.length > 0 && utilizador.trim().length === 0 ? 'Escreva o seu nome.' : undefined;

  async function submeter(evento) {
    evento.preventDefault();
    if (!pronto) return;

    setOcupado(true);
    setErro('');

    const { erro: falha } = await registar({ utilizador: utilizador.trim(), frase });
    if (falha) {
      setErro(falha);
      setOcupado(false);
      return;
    }

    // A conta ficou aberta: vai para a casca. Não há caminho de volta que
    // faça sentido — voltar ao registo agora mostraria uma conta que já
    // existe.
    navegar('/', { replace: true });
  }

  return (
    <AuthLayout
      icon={OnyxLogo}
      title="Criar a sua conta"
      subtitle="Tudo fica neste computador. Não há servidor, não há correio, não há contas em lado nenhum."
      footer={
        <span>
          Já tem conta?{' '}
          <Link to="/entrar" className="text-onyx-metallic hover:underline">
            Entrar
          </Link>
        </span>
      }
    >
      <form onSubmit={submeter} className="space-y-4" noValidate>
        <OnyxInput
          label="Como se chama"
          value={utilizador}
          onChange={(e) => setUtilizador(e.target.value)}
          error={problemaNome}
          hint="É este nome que os seus contactos veem. Pode mudá-lo depois."
          autoComplete="username"
          autoFocus
          maxLength={64}
        />

        <div>
          <OnyxInput
            label="Frase de segurança"
            type="password"
            value={frase}
            onChange={(e) => setFrase(e.target.value)}
            hint="Palavras que só você saiba. Não é uma palavra-passe."
            autoComplete="new-password"
            maxLength={256}
          />
          <ForcaFrase frase={frase} />
        </div>

        <OnyxInput
          label="Repita a frase"
          type="password"
          value={confirmacao}
          onChange={(e) => setConfirmacao(e.target.value)}
          error={problemaConfirmacao}
          hint="De memória, não a copiar. É a única verificação de que a vai lembrar."
          autoComplete="new-password"
          maxLength={256}
        />

        {/*
          O aviso vai **antes** do botão, e não depois de a conta existir.
          Depois de a conta existir já não há nada a fazer com a
          informação; antes, ainda há.

          E vai sempre, mesmo com a frase forte. A frase pode ser aceite e
          ser uma data de nascimento — e é para esse caso que o texto diz
          o que uma frase é.
        */}
        <div className="rounded-md border border-onyx-warning/30 bg-onyx-warning/[0.06] p-3 text-[11.5px] leading-relaxed text-onyx-text2">
          <p className="onyx-label mb-1 text-onyx-warning">Leia isto antes de continuar</p>
          <p>
            A frase de segurança é a <strong>única</strong> forma de abrir esta conta outra
            vez. Este programa não tem servidor, por isso não há correio para lhe enviar um
            código, nem pergunta secreta para lhe fazer: se a esquecer, a conta fecha-se para
            sempre.
          </p>
          <p className="mt-1.5">
            Escolha palavras que <strong>você</strong> se lembre: uma frase de uma
            conversa, a rua onde cresceu, a letra de uma música. Não a data de nascimento, e
            não a mesma frase que usa noutro sítio.
          </p>
          {estaEmDemonstracao() && (
            <p className="mt-1.5 text-onyx-text3">
              Está em modo de demonstração: a frase é aceite sem cifrar nada.{' '}
              <code className="font-mono">demo</code> também serve.
            </p>
          )}
        </div>

        {erro && (
          <p role="alert" className="rounded-md border border-onyx-error/40 bg-onyx-error/[0.06] px-3 py-2 text-[12px] text-onyx-error">
            {erro}
          </p>
        )}

        <OnyxButton type="submit" variant="primary" className="w-full" disabled={!pronto || ocupado}>
          {ocupado ? 'A criar…' : 'Criar conta'}
        </OnyxButton>
      </form>
    </AuthLayout>
  );
}