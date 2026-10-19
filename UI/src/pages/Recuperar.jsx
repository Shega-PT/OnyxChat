// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// Recuperar.jsx — o que acontece quando a frase se perdeu
// ---------------------------------------------------------------------
// Este é o ecrã mais difícil do projecto, porque não há resposta boa.
//
// ## O que este programa **não** tem
//
// Um servidor para o qual mandar um correio. Um número de telefone. Uma
// pergunta secreta guardada. Um contacto de recuperação. Um ficheiro em
// que o utilizador possa confirmar «esta sou eu».
//
// Cada um deles resolveria o problema. Nenhum existe, por desenho: um
// sistema de mensagens que não pede correio a ninguém é uma das razões
// de existir.
//
// ## O que este ecrã faz, então
//
// Três coisas, e as três são verdade:
//
//   1. **Tentar de novo.** A causa mais comum é o teclado em modo
//      maiúsculas, uma cedilha perdida ou um espaço a mais no fim.
//   2. **Repor de uma cópia de segurança.** É a única recuperação que o
//      projecto consegue oferecer, e depende inteiramente de a pessoa ter
//      feito uma cópia. A interface não finge que isto é automático.
//   3. **Recomeçar.** Cria uma identidade **nova**. Para os contactos
//      existentes, isso é outra pessoa: o identificador muda, e o
//      caminho de quem o tinha guardado deixa de funcionar. O ecrã diz
//      isso antes do botão, e não depois.
//
// ## Porque o terceiro caminho é uma porta e não uma nota de rodapé
//
// Porque alguém cuja conta está fechada precisa de uma saída, e uma saída
// que esconde a consequência é pior do que nenhuma. Quem recomeçar sabe
// o que está a perder; quem não souber descobre-o quando os contactos
// desaparecerem, e a essa hora já não há nada a fazer.
//
// A conta antiga **não** é apagada por omissão: apagá-la junto com a
// recomeçada seria o pior dos dois — e o botão que o faz é separado, e
// pede o nome outra vez.
// =====================================================================

import React, { useRef, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import AuthLayout from '@/components/AuthLayout';
import OnyxButton from '@/components/onyx/OnyxButton';
import OnyxInput from '@/components/onyx/OnyxInput';
import OnyxLogo from '@/components/onyx/OnyxLogo';
import OnyxModal from '@/components/onyx/OnyxModal';
import { useSessao } from '@/lib/SessaoContext';
import { estaEmDemonstracao } from '@/lib/onyx/runtime';

/**
 * Ecrã de recuperação.
 *
 * Não tenta ser rápido. Cada opção explica o que faz antes de a fazer, e
 * a única que pode destruir alguma coisa pede confirmação por escrito.
 */
export default function Recuperar() {
  const navegar = useNavigate();
  const { conta, restaurar } = useSessao();
  const campoFicheiro = useRef(null);

  const [erro, setErro] = useState('');
  const [sucesso, setSucesso] = useState('');
  const [ocupado, setOcupado] = useState(false);
  const [confirmar, setConfirmar] = useState(false);
  const [nomeParaApagar, setNomeParaApagar] = useState('');
  const [fraseCopia, setFraseCopia] = useState('');

  /**
   * Lê o ficheiro escolhido e repõe a conta.
   *
   * O ficheiro é lido pelo navegador e vem como texto. Nenhum caminho
   * atravessa a rede nem chega ao servidor — mandar o caminho de um
   * ficheiro para um servidor é dar-lhe o poder de ler um ficheiro que a
   * pessoa não escolheu.
   *
   * O texto passa **cru**, sem `JSON.parse`. A cópia que passou
   * a exportar é base64 de um keystore binário — o corpo de uma resposta
   * HTTP tem de ser JSON, e o ficheiro não — e fazer-lhe `JSON.parse`
   * era reprovar sempre com «não é uma cópia de conta», mesmo com a
   * cópia certa à frente. Era uma falha silenciosa do ponto mais
   * sensible do programa: a pessoavia uma recusa que não se parecia com
   * um ficheiro estragado, e não com uma conta que ela acabara de
   * exportar.
   *
   * A validação fica do lado do adaptador, que é quem sabe a diferença
   * entre «isto não é uma conta» e «a frase não bate». Duas mensagens
   * distintas para duas causas distintas, e a pessoa sabe se deve
   * procurar outro ficheiro ou outra frase.
   *
   * O gatilho é o `change` do input, e não um botão de submissão: o
   * botão só abre o selector de ficheiros, e um formulário sem botão de
   * submissão nunca é submetido. Com um `onSubmit` a espera de um evento
   * que não chega, a opção de recuperação parecia existir e não fazia
   * nada — que é a pior das falhas, porque é silenciosa.
   *
   * @param {React.ChangeEvent<HTMLInputElement>} evento
   */
  async function repor(evento) {
    const ficheiro = evento.target.files?.[0];
    // Esvazia-se o campo para que escolher o **mesmo** ficheiro outra
    // vez dispare o `change`. Sem isto, a segunda tentativa não emite
    // evento nenhum e o botão parece partido.
    evento.target.value = '';
    if (!ficheiro) return;

    setErro('');
    setSucesso('');
    setOcupado(true);

    const conteudo = (await ficheiro.text()).trim();
    if (!conteudo) {
      setErro('O ficheiro escolhido está vazio.');
      setOcupado(false);
      return;
    }

    const { erro: falha } = await restaurar({ conteudo, frase: fraseCopia });
    if (falha) {
      setErro(falha);
    } else {
      setSucesso('A conta foi reposta. Escreva a frase para abrir.');
      navegar('/entrar');
    }
    setOcupado(false);
  }

  const nomeDaConta = conta?.utilizador ?? 'a conta';
  const nomeConfere = nomeParaApagar.trim().toLowerCase() === nomeDaConta.toLowerCase();

  return (
    <AuthLayout
      icon={OnyxLogo}
      title="Não tem a frase?"
      subtitle="Este programa não tem servidor, por isso não há código para lhe enviar."
      footer={
        <span>
          <Link to="/entrar" className="text-onyx-metallic hover:underline">
            Voltar a entrar
          </Link>
        </span>
      }
    >
      <div className="space-y-5 text-[13px] leading-relaxed text-onyx-text2">
        {/* ---------------------------------------------------------- 1 */}
        <section>
          <h2 className="onyx-label mb-1.5 text-onyx-text">Tentar de novo</h2>
          <p className="text-onyx-text3">
            Antes de mais: o teclado pode estar em maiúsculas, e a frase pode ter ganho um
            espaço no fim quando foi copiada de outro sítio. Vale a pena tentar duas ou três
            vezes com cuidado — esta é a resposta certa na maioria das vezes.
          </p>
          <OnyxButton variant="secondary" className="mt-2" onClick={() => navegar('/entrar')}>
            Tentar outra vez
          </OnyxButton>
        </section>

        {/* ---------------------------------------------------------- 2 */}
        <section className="border-t border-onyx-line pt-4">
          <h2 className="onyx-label mb-1.5 text-onyx-text">Repor de uma cópia</h2>
          <p className="mb-3 text-onyx-text3">
            Se exportou a conta em Definições, o ficheiro que ficou é suficiente — e continua
            cifrado, com a mesma frase. Sem uma cópia, esta opção não existe.
          </p>

          <div className="space-y-3">
            <OnyxInput
              label="Frase com que a cópia foi cifrada"
              type="password"
              value={fraseCopia}
              onChange={(e) => setFraseCopia(e.target.value)}
              autoComplete="off"
              maxLength={256}
            />
            <input
              ref={campoFicheiro}
              type="file"
              accept=".keystore,.json,application/json"
              className="sr-only"
              onChange={repor}
            />
            <OnyxButton type="button" variant="secondary" onClick={() => campoFicheiro.current?.click()}>
              Escolher a cópia
            </OnyxButton>
          </div>

          {sucesso && (
            <p role="status" className="mt-2 text-[12px] text-onyx-success">
              {sucesso}
            </p>
          )}
          {erro && (
            <p role="alert" className="mt-2 text-[12px] text-onyx-error">
              {erro}
            </p>
          )}
        </section>

        {/* ---------------------------------------------------------- 3 */}
        <section className="border-t border-onyx-line pt-4">
          <h2 className="onyx-label mb-1.5 text-onyx-warning">Recomeçar</h2>
          <p className="mb-3 text-onyx-text3">
            Isto cria uma <strong>identidade nova</strong>. O identificador muda, e as pessoas
            que o guardaram deixaram de o conseguir abrir. Não há como desfazer, e a conta
            antiga <strong>não é apagada</strong> — passa a estar órfã neste computador, e
            só se pode limpar à mão.
          </p>
          <OnyxButton variant="danger" onClick={() => setConfirmar(true)}>
            Quero recomeçar
          </OnyxButton>
        </section>

        {estaEmDemonstracao() && (
          <p className="border-t border-onyx-line pt-4 text-[11px] text-onyx-text3">
            Demonstração: nada disto toca em ficheiros. A conta de demonstração é um valor em{' '}
            <code className="font-mono">localStorage</code> que qualquer pessoa com acesso a
            este computador e ao código lê. Fora da demonstração, a conta está num keystore
            cifrado com a frase de segurança.
          </p>
        )}
      </div>

      {/*
        O modal de confirmação exige escrever o nome. É a única forma de
        ter a certeza de que quem recomeça leu o que está a perder: um
        botão com «tem a certeza?» é um botão que se carrega sem ler.
      */}
      <OnyxModal
        open={confirmar}
        onOpenChange={setConfirmar}
        title="Recomeçar mesmo?"
        description={`Isto cria uma identidade nova. A conta «${nomeDaConta}» continua no disco, mas já não a vai conseguir abrir.`}
        footer={
          <>
            <OnyxButton variant="ghost" onClick={() => setConfirmar(false)}>
              Cancelar
            </OnyxButton>
            <OnyxButton
              variant="danger"
              disabled={!nomeConfere}
              onClick={() => {
                setConfirmar(false);
                navegar('/registar', { replace: true });
              }}
            >
              Criar identidade nova
            </OnyxButton>
          </>
        }
      >
        <OnyxInput
          label={`Escreva «${nomeDaConta}» para confirmar`}
          value={nomeParaApagar}
          onChange={(e) => setNomeParaApagar(e.target.value)}
          autoFocus
        />
        <p className="mt-2 text-[11px] text-onyx-text3">
          Se escrevesse o nome de outra pessoa, a interface não o diria — não há contas alheias
          para confundir. O que se pede é atenção, não autenticação.
        </p>
      </OnyxModal>
    </AuthLayout>
  );
}