// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// interface-visual.js — a janela interna
// ---------------------------------------------------------------------
// O que este ficheiro faz: monta uma janela do GTK 4 com um `WebView` do
// WebKit dentro, e serve-a ao utilizador para avaliar a interface.
//
// ## Porque é que não é um navegador
//
// Porque `chromium`, `firefox` e `google-chrome` não são dependências
// deste projecto. Chamar `xdg-open` com um URL é chamar um comando que
// pode não existir, e a falha aparece no fim do que se queria ver. A
// janela é aqui uma `Gtk.Window` nossa, e o motor de renderização é o
// mesmo que o Tauri usaria — a diferença é que nada disto precisa de ser
// compilado.
//
// ## As três coisas que o GJS não embala bem, e o que se fez
//
// A introspecção do GJS é feita a partir de *typelibs* que descrevem
// uma versão do WebKitGTK, e o que está instalado nem sempre é essa
// versão. As consequências foram medidas, uma a uma, e são estas:
//
// 1. **Os callbacks assíncronos chegam com os argumentos errados.** Chamar
//    `get_snapshot` com um *callback* próprio entrega `(WebView, Gio.Task)`
//    em vez de `(textura, erro)`. A correcção é `Gio._promisify` sobre o
//    **protótipo**, que é o que a documentação do GJS prescreve e que
//    devolve uma promessa com o valor certo.
//
// 2. **`evaluate_javascript` é inutilizável.** Depois de promifizada
//    continua a exigir uma assinatura que o *typelib* descreve de forma
//    incompatível com a biblioteca instalada, e quando não rebenta entrega
//    argumentos que não correspondem a nada. Deixou de ser usado, e é por
//    isso que a navegação abaixo recarrega em vez de mandar o `history`.
//
// 3. **Um `JSC.Value` não se converte com `String()`.** O que funciona é
//    `to_json()`, e é o que este ficheiro usa para ler o que a página
//    devolveu.
//
// A consequência de desenho: **nada aqui depende de um callback cru.**
// Tudo o que é assíncrono passa por promessa, e tudo o que vem da página
// vem por *sinal*, que o GJS embala como se deve.
//
// ## A navegação é um carregamento, não um `pushState`
//
// Trocar de ecrã recarrega a página em vez de mandar o `history` para um
// caminho novo. É menos elegante e é a escolha certa: o `pushState` só se
// consegue a partir de dentro da página, e dentro da página só se entra
// com `evaluate_javascript`, que não funciona. Recarregar tem ainda uma
// vantagem — cada ecrã é percorrido pelo mesmo caminho pelo qual um
// utilizador chega lá, o React Router incluído, e não por um atalho que
// saltaria o que se quer estar a ver.
//
// ## O agente de página
//
// O resumo do que um ecrã mostrou chega por `postMessage` do lado do
// browser e por `script-message-received` do lado do GJS. É injectado
// como `UserScript`, portanto corre em todas as páginas sem que ninguém
// lhe peça nada: quando a página assenta, ela fala.
//
// Não se injecta nada na aplicação. O agente é do harness, vive no
// contexto de injectados do WebKit e desaparece com a janela.
// =====================================================================

imports.gi.versions.Gtk = '4.0';
imports.gi.versions.Gdk = '4.0';
imports.gi.versions.WebKit = '6.0';
const { Gtk, Gdk, WebKit, GLib, Gio } = imports.gi;

// `get_snapshot` devolve a textura por `Gio.Task`, e o GJS só embralha
// isso bem depois de promifizado. Ver a nota 1 do cabeçalho.
Gio._promisify(WebKit.WebView.prototype, 'get_snapshot');

// ---------------------------------------------------------------------
// O que o `interface-visual.py` passou
// ---------------------------------------------------------------------

/** A raiz do sidecar, sem barra e sem caminho. */
const BASE = GLib.getenv('ONYX_VISUAL_BASE') || 'http://127.0.0.1:8787';

/** `let` e não `const` porque a tecla `d` o inverte. */
let MOCK = GLib.getenv('ONYX_VISUAL_MOCK') === '1';

/** O directório de `--guardar`; vazio quando a janela é interactiva. */
const DESTINO = GLib.getenv('ONYX_VISUAL_GUARDAR') || '';

/** O nome do cenário, só para o índice. */
const CENARIO = GLib.getenv('ONYX_VISUAL_CENARIO') || 'vazio';

/** Os ecrãs, na ordem de `UI/src/App.jsx`, como pares `[caminho, nome]`. */
const ECRAS = JSON.parse(GLib.getenv('ONYX_VISUAL_ECRAS') || '[]');

/** O índice do ecrã que está à vista. */
let ecraActual = 0;

/**
 * O ecrã a que o relatório que acabou de chegar pertence.
 *
 * É uma variável à parte de `ecraActual` porque um relatório chega depois
 * de a navegação já ter avançado: sem isto, um relatório lento seria
 * contado no ecrã seguinte e a linha de estado mentiria sobre o que está
 * no ecrã.
 */
let ecraDoRelatorio = 0;

/** O que cada ecrã disse sobre si próprio. */
const relatorios = new Map();

// ---------------------------------------------------------------------
// O agente de página
// ---------------------------------------------------------------------

/**
 * O código que a página corre para se descrever.
 *
 * O que se pede responde à pergunta «isto funciona?»: o caminho, o
 * número de elementos, os títulos de ecrã e um excerto do texto. O
 * excerto é o que apanha a falha mais comum, que é o ecrã carregar e não
 * mostrar nada — um ecrã com quarenta elementos e zero texto é uma lista
 * de molduras vazias, e sem o texto não se distingue de um ecrã a
 * trabalhar.
 *
 * **Os erros de JavaScript são apanhados aqui e não por um sinal.** O
 * sinal `console-message` foi removido no WebKit 6.0 — foi medido: ligar
 * a ele dá «No signal 'console-message' on object 'WebKitWebView'» — e o
 * que resta, `web-process-terminated`, só fala de uma morte, não do que a
 * matou. Um ecrã em branco é a falha mais importante que este harness
 * pode encontrar, e a causa dela é quase sempre uma excepção em
 * ``render``; ouvi-la é metade do trabalho de a diagnosticar.
 *
 * @returns {string} o código a injectar.
 */
function agente() {
  return `
    const problemas = [];
    window.addEventListener('error', (evento) => {
      problemas.push(
        (evento.message || 'erro') + ' @ ' +
        (evento.filename || '?') + ':' + (evento.lineno || 0)
      );
    });
    window.addEventListener('unhandledrejection', (evento) => {
      problemas.push('promessa rejeitada: ' + evento.reason);
    });

    window.addEventListener('load', () => {
      setTimeout(() => {
        let texto;
        try {
          texto = JSON.stringify({
            caminho: location.pathname + location.search,
            titulo: document.title,
            elementos: document.querySelectorAll('*').length,
            cabecalhos: Array.from(document.querySelectorAll('h1, h2'))
              .map((n) => n.innerText.trim())
              .filter(Boolean)
              .slice(0, 8),
            texto: (document.body.innerText || '')
              .replace(/\\s+/g, ' ')
              .trim()
              .slice(0, 400),
            problemas: problemas.slice(0, 5)
          });
        } catch (erro) {
          texto = JSON.stringify({ erro: String(erro) });
        }
        try {
          window.webkit.messageHandlers.onyx.postMessage(texto);
        } catch (erro) {
          // Sem *handler* não há por onde falar. É o caso de uma página
          // que não é a aplicação, e insistir não melhora nada.
        }
      }, 1200);
    });
  `;
}

// ---------------------------------------------------------------------
// A janela
// ---------------------------------------------------------------------
// A janela
// ---------------------------------------------------------------------

// `Gtk.init()` antes de qualquer `WebView`: sem ele o processo morre com
// uma falha de segmentação, e sem aviso.
Gtk.init();

const janela = new Gtk.Window({ title: 'OnyxChat — avaliação visual' });
janela.set_default_size(1440, 900);
janela.set_decorated(false);

const vista = new WebKit.WebView();
vista.set_vexpand(true);
vista.set_hexpand(true);

// A barra de estado é uma faixa dentro da janela e não uma
// `Gtk.HeaderBar` de título, e a diferença não é de aspecto.
//
// Com `decorated: false` e uma `HeaderBar` em `set_titlebar`, o GTK
// nunca chega a dar-lhe uma `allocation` — não há moldura que a peça —
// e cada paint do cabeçalho produz `Trying to snapshot GtkHeaderBar
// without a current allocation`. É um aviso, não uma falha, mas sai no
// `stderr` a cada ecrã e tapa a saída do harness, que é o que se quer
// ler. Uma faixa dentro da janela é desenhada como tudo o resto.
const faixa = new Gtk.Box({
  orientation: Gtk.Orientation.HORIZONTAL,
  spacing: 12,
  margin_start: 12,
  margin_end: 12,
  margin_top: 6,
  margin_bottom: 6,
});

const estado = new Gtk.Label({
  hexpand: true,
  xalign: 0,
  ellipsize: 3,
  css_classes: ['dim-label'],
});

const marca = new Gtk.Label({
  label: 'OnyxChat',
  css_classes: ['heading'],
});

faixa.append(marca);
faixa.append(estado);

const caixa = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL });
caixa.append(faixa);
caixa.append(vista);

janela.set_child(caixa);
janela.present();

// ---------------------------------------------------------------------
// A ponte com a página
// ---------------------------------------------------------------------

const gestor = vista.get_user_content_manager();
gestor.register_script_message_handler('onyx', null);
gestor.add_script(new WebKit.UserScript(
  agente(), WebKit.UserContentInjectedFrames.ALL_FRAMES, null, null, null
));

gestor.connect('script-message-received', (_g, valor) => {
  let situacao;

  // `to_json` e não `toString`: ver a nota 3 do cabeçalho. O que ele
  // devolve é o valor em JSON, e como a página manda uma cadeia, o
  // resultado é uma cadeia dentro de outra — daí a segunda leitura.
  // Aceitar as duas formas evita que uma alteração no agente, de cadeia
  // para objecto, parta a leitura em silêncio e passe a reportar
  // «zero elementos» em todos os ecrãs.
  try {
    let bruto = String(valor.to_json(0));
    try {
      const primeira = JSON.parse(bruto);
      bruto = typeof primeira === 'string' ? primeira : bruto;
    } catch (erro) {
      // Já vinha em forma de objecto; segue como está.
    }
    situacao = JSON.parse(bruto);
  } catch (erro) {
    situacao = { erro: String(erro) };
  }

  relatorios.set(ecraDoRelatorio, situacao);
  mostrar(ecraDoRelatorio);
});

// ---------------------------------------------------------------------
// A navegação
// ---------------------------------------------------------------------

/**
 * O URL de um ecrã, com o interruptor de demonstração pelo meio.
 *
 * O parâetro vai sempre no URL e nunca em `localStorage` porque é
 * efémero por natureza — recarregar devolve ao que estava definido, e
 * é isso que se quer de uma ferramenta de avaliação.
 *
 * @param {number} indice o índice do ecrã.
 * @returns {string} o URL a carregar.
 */
function urlDe(indice) {
  const ecra = ECRAS[indice];
  const caminho = ecra ? ecra[0] : '/';
  const sep = BASE.indexOf('?') === -1 ? '?' : '&';
  return `${BASE}${caminho}${sep}mock=${MOCK ? '1' : '0'}`;
}

/** Vai para um ecrã, recarregando. */
function irPara(indice) {
  if (ECRAS.length === 0) {
    estado.set_text('nenhum ecrã lido de UI/src/App.jsx');
    return;
  }
  ecraActual = ((indice % ECRAS.length) + ECRAS.length) % ECRAS.length;
  ecraDoRelatorio = ecraActual;
  mostrar(ecraActual);
  vista.load_uri(urlDe(ecraActual));
}

/** Escreve a linha de estado do ecrã indicado. */
function mostrar(indice) {
  const ecra = ECRAS[indice];
  if (!ecra) return;

  const modo = MOCK ? 'demonstração' : 'backend real';
  const situacao = relatorios.get(indice);
  const prefixo = `${indice + 1}. ${ecra[1] || ecra[0]} — ${modo}`;

  if (!situacao) {
    estado.set_text(`${prefixo} — a carregar…`);
    return;
  }

  const cabecalhos = (situacao.cabecalhos || []).join(' · ');
  if (situacao.erro) {
    estado.set_text(`${prefixo} — erro: ${situacao.erro}`);
    return;
  }

  estado.set_text(
    `${prefixo} — ${situacao.elementos} elementos` +
      (cabecalhos ? ` — ${cabecalhos}` : '')
  );
}

// ---------------------------------------------------------------------
// As capturas
// ---------------------------------------------------------------------

/**
 * Guardar o que está desenhado num PNG.
 *
 * `SnapshotRegion.VISIBLE` e não `FULL_DOCUMENT`: o que se avalia é o
 * ecrã, e uma página inteira numa janela de 1440 pixels sai com uma
 * escala que não corresponde ao que o utilizador está a ver. O que está
 * fora da janela existe, e obtém-se maximizando a janela.
 *
 * @param {string} caminho onde escrever.
 * @returns {Promise<void>} resolvido quando o ficheiro está escrito.
 */
async function guardarPng(caminho) {
  const textura = await vista.get_snapshot(
    WebKit.SnapshotRegion.VISIBLE,
    WebKit.SnapshotOptions.NONE,
    new Gio.Cancellable()
  );
  textura.save_to_png(caminho);
}

// ---------------------------------------------------------------------
// O modo `--guardar`
// ---------------------------------------------------------------------

/** Prepara o directório de destino, uma vez só. */
function prepararDestino() {
  const pasta = Gio.File.new_for_path(DESTINO);
  if (!pasta.query_exists(null)) {
    pasta.make_directory_with_parents(null);
  }
}

/**
 * Espera que um ecrã se anuncie.
 *
 * @param {number} indice o ecrã à espera.
 * @param {number} milissegundos o limite.
 * @returns {Promise<void>} resolvido quando chega ou quando expira.
 */
function esperarRelatorio(indice, milissegundos) {
  return new Promise((resolve) => {
    if (relatorios.has(indice)) {
      resolve();
      return;
    }
    const inicio = GLib.get_monotonic_time();
    GLib.timeout_add(GLib.PRIORITY_DEFAULT, 120, () => {
      const passado = (GLib.get_monotonic_time() - inicio) / 1000;
      if (relatorios.has(indice) || passado > milissegundos) {
        resolve();
        return GLib.SOURCE_REMOVE;
      }
      return GLib.SOURCE_CONTINUE;
    });
  });
}

/**
 * Percorre os ecrãs todos, guarda um PNG de cada e escreve o índice.
 *
 * Não abre janela: é o modo de comparar duas execuções, e uma janela a
 * piscar por detrás de um ciclo de capturas não ajuda ninguém.
 *
 * @returns {Promise<void>} resolvido quando as capturas acabaram.
 */
async function exportar() {
  prepararDestino();
  const linha = [
    `cenário: ${CENARIO}`,
    `interruptor: mock=${MOCK ? '1' : '0'}`,
    `base: ${BASE}`,
    `ecrãs: ${ECRAS.length}`,
    '',
  ];

  for (let i = 0; i < ECRAS.length; i++) {
    ecraActual = i;
    ecraDoRelatorio = i;
    mostrar(i);
    vista.load_uri(urlDe(i));

    await esperarRelatorio(i, 25000);

    const nome = `${String(i + 1).padStart(2, '0')}-${ECRAS[i][1]}`;
    await guardarPng(`${DESTINO}/${nome}.png`);

    const situacao = relatorios.get(i) || {};
    linha.push(`${nome}.png — ${situacao.elementos || '?'} elementos`);
    linha.push(`  ${(situacao.cabecalhos || []).join(' · ') || '(sem cabeçalho)'}`);
    for (const problema of situacao.problemas || []) {
      linha.push(`  ERRO: ${problema}`);
    }
    linha.push(`  ${(situacao.texto || '').slice(0, 300)}`);
    linha.push('');

    const erros = (situacao.problemas || []).length;
    print(
      `${nome} — ${situacao.elementos || '?'} elementos` +
        (erros ? ` — ${erros} ERRO(S): ${situacao.problemas[0]}` : '')
    );
  }

  GLib.file_set_contents(`${DESTINO}/indice.txt`, linha.join('\n'));
  print(`guardados ${ECRAS.length} ecrãs em ${DESTINO}`);
}

// ---------------------------------------------------------------------
// As teclas
// ---------------------------------------------------------------------

/**
 * O número de uma tecla `1`–`9`, ou `null`.
 *
 * @param {number} chaveval o valor da tecla.
 * @returns {number|null} o índice do ecrã.
 */
function teclaDeNumero(chaveval) {
  for (let n = 1; n <= 9; n++) {
    if (chaveval === Gdk[`KEY_${n}`]) return n - 1;
  }
  return null;
}

const teclado = new Gtk.EventControllerKey();
janela.add_controller(teclado);

teclado.connect('key-pressed', (_c, chaveval, _codigo, mascara) => {
  // O teste dos modificadores é o que impede que um `Ctrl-C` seja lido
  // como a tecla `3`: sem ele, o harness saltava de ecrã em vez de sair,
  // e saía de si quando a reacção seria fechar.
  if ((mascara & Gtk.accelerator_get_default_mod_mask()) !== 0) {
    return Gdk.EVENT_PROPAGATE;
  }

  switch (chaveval) {
    case Gdk.KEY_q:
    case Gdk.KEY_Escape:
      janela.close();
      return Gdk.EVENT_STOP;

    case Gdk.KEY_d: {
      // Alterna entre demonstração e backend real sem recarregar o
      // processo. É a comparação que se está a fazer, e tê-la num toque
      // é o que a torna rápida.
      MOCK = !MOCK;
      estado.set_text(
        `a alternar para ${MOCK ? 'demonstração' : 'backend real'}…`
      );
      vista.load_uri(urlDe(ecraActual));
      return Gdk.EVENT_STOP;
    }

    case Gdk.KEY_s: {
      if (!DESTINO) {
        estado.set_text('sem --guardar não há directório para onde escrever');
        return Gdk.EVENT_STOP;
      }
      prepararDestino();
      const destino = `${DESTINO}/manual-${ECRAS[ecraActual][1]}.png`;
      guardarPng(destino)
        .then(() => estado.set_text(`guardado em ${destino}`))
        .catch((erro) => estado.set_text(`falhou: ${erro}`));
      return Gdk.EVENT_STOP;
    }

    default:
      break;
  }

  const numero = teclaDeNumero(chaveval);
  if (numero !== null && numero < ECRAS.length) {
    irPara(numero);
    return Gdk.EVENT_STOP;
  }

  return Gdk.EVENT_PROPAGATE;
});

// ---------------------------------------------------------------------
// O arranque
// ---------------------------------------------------------------------

const ciclo = new GLib.MainLoop(null, false);

janela.connect('close-request', () => {
  ciclo.quit();
  return Gdk.EVENT_STOP;
});

if (DESTINO) {
  // Um instante antes de exportar. A janela foi desenhada no arranque e
  // o GTK ainda não lhe deu uma `allocation`, e uma captura tirada nesse
  // intervalo sai sem a barra de título e com um aviso na consola.
  GLib.timeout_add(GLib.PRIORITY_DEFAULT, 600, () => {
    exportar()
      .then(() => ciclo.quit())
      .catch((erro) => {
        printerr(`exportar falhou: ${erro}`);
        ciclo.quit();
      });
    return GLib.SOURCE_REMOVE;
  });
} else {
  irPara(0);
}

ciclo.run();
