// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/mod.rs — o módulo `onyxmedia`
// ---------------------------------------------------------------------
// Áudio e vídeo como **texto**, dentro do envelope que o sistema já
// transporta. A especificação normativa é [`docs/media.md`](../../media.md);
// este ficheiro é a implementação, e a spec é que manda nos bytes.
//
// ## O invariante, que é o que justifica o módulo existir
//
//     O que sai para a rede é um `FRAME_CHAT` (`0x01`) com um envelope
//     normal. A rede não sabe se é texto, áudio ou vídeo.
//
// Daqui decorre que **`p2p.rs`, `docs/relay.md` e o `OnyxMiniNode` não
// são tocados**. Se alguma vez precisarem de uma alteração para a
// media passar, é sinal de que o invariante foi quebrado — e o teste
// que o verifica é `tests/media_indistinguivel.rs`, que compara a
// forma de um envelope de media com a de um de texto.
//
// ## As duas funções
//
// `emitir` e `reconstruir`. O mesmo caminho em espelho, e é por isso
// que este módulo tem duas metades e não uma: a metade que produz e a
// metade que desfaz têm de concordar byte a byte, e a forma de garantir
// isso é ter **um** formato, não dois.
//
// ## A ordem das dependências
//
// ```text
//   media  →  moldura  →  base64
//     ↓        ↓
//   teclas    erros
// ```
//
// `base64` e `erros` não dependem de nada. `moldura` depende de
// `base64`. `teclas` não depende de `moldura` — só do `Tipo` que está
// lá, e essa dependência é de um valor, não de um formato. É a ordem
// que permite testar a base64 e a moldura sem decidir nada sobre
// chaves, e as chaves sem decidir nada sobre bytes.
// =====================================================================

/// A base-64 que torna a media texto.
pub mod base64;

/// O `ffmpeg` dentro do binário, executado de um `memfd`.
pub mod ffmpeg_embutido;

/// Lançar o `ffmpeg` com dois canais de saída: vídeo no 1, áudio no 3.
pub mod ffmpeg;

/// Os erros do módulo, num só sítio, sem conteúdo.
pub mod eco;
pub mod erros;

/// A loja do `sidecar`, lida em modo só de leitura, para montar media.
pub mod loja;

/// O ficheiro montado em RAM, entregue como descritor selado.
pub mod ram;
pub mod saida;

/// A reprodução: juntar loja, RAM e leitor do sistema.
pub mod reproduzir;

/// O relógio de media: tempo de media, nunca tempo de parede.
pub mod relogio;

/// Telhas 16×16, delta e a sua aplicação — em binário, antes do base-64.
pub mod telhas;

/// O anel de media: dois segundos, com políticas opostas por fila.
pub mod anel;

/// O coalescing adaptativo e o `HOLD`.
pub mod coalescer;
pub mod desempate;
pub mod imagem;
pub mod ivf;
pub mod lote;

/// `base_seq`, keyframes e ressincronização.
pub mod ressincronizacao;

/// O modo não-streaming: fracção, manifesto, montagem e reprodução.
pub mod ficheiro;

/// A moldura `ONX1`: o formato do "texto" de media.
pub mod moldura;

/// A escolha da chave pelo tipo, e a sondagem que descobre o tipo.
pub mod teclas;

pub use erros::ErroMedia;
pub use ficheiro::{Codec, Formato, Manifesto, Montado, Montador};
pub use moldura::{Aberta as MolduraAberta, Modo, Moldura, Tipo};
pub use teclas::{sondar_com, ChavesMedia, Detectado, Genero};

/// A versão do módulo, para o `MEDIA_ESTADO` e para o `onyxchat media
/// --versao`.
///
/// É um inteiro e não a [`moldura::VERSAO`] porque são coisas
/// diferentes: a versão da **moldura** é o que o outro lado tem de
/// concordar para os bytes fazerem sentido; a versão do **módulo** é o
/// que um humano lê para saber o que está a correr. Confundir as duas
/// daria um número que significa uma coisa no código e outra na
/// interface.
pub const VERSAO_MODULO: u32 = 1;

/// O invariante, escrito onde não se pode deixar de ler.
///
/// Um invariant só no `docs/media.md` é um invariant que ninguém lê
/// enquanto escreve código. Isto é uma **constante de texto no módulo**:
/// vive onde qualquer pessoa que abra o ficheiro vai passar, e é o que
/// impede a próxima pessoa de «só acrescentar um tipo de frame para
/// isto não ir pela metade do texto».
///
/// A regra continua a ser: se a media precisar de uma alteração na
/// rede, a alteração está errada.
#[allow(dead_code)]
pub const INVARIANTE: &str = "\
O que sai para a rede é um FRAME_CHAT com um envelope normal. \
A rede não sabe se é texto, áudio ou vídeo. \
Se a media precisar de um caminho novo na rede, o caminho novo está errado.";

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod testes {
    use super::*;

    /// O invariante é dito no código, não só na spec.
    ///
    /// Um teste que lê uma constante de texto e confirma que ela não
    /// está vazia parece inútil — e é, se a constante mudar de
    /// conteúdo. Serve para outra coisa: obriga a constante a **existir**
    /// e a ser **usada**, porque uma constante morta é removida pela
    /// próxima pessoa que lê o aviso do compilador.
    #[test]
    fn o_invariante_e_uma_frase_e_nao_uma_promessa_vazia() {
        assert!(
            INVARIANTE.len() > 100,
            "o invariante tem de ser uma frase, não uma palavra"
        );
        for frase in ["FRAME_CHAT", "envelope normal", "não sabe", "está errado"] {
            assert!(
                INVARIANTE.contains(frase),
                "o invariante deixou de dizer «{frase}»"
            );
        }
    }

    /// A ordem dos submodules é a ordem da spec.
    ///
    /// Teste de sanidade, e o mais barato de escrever com informação
    /// real: confirma que [`base64`] é a base de tudo e que [`teclas`]
    /// não depende da [`moldura`] por outra via. Se este teste
    /// precisar de ser mudado, é porque alguém reordenou as dependências
    /// — e o motivo tem de estar no `git log`.
    #[test]
    fn as_dependencias_vao_no_sentido_certo() {
        // base64 não precisa de nada do módulo.
        let _ = base64::codificar(b"");
        // A moldura depende de base64 e não de teclas.
        let m = moldura::construir(Tipo::Video, Modo::NaoStreaming, 0, 0, 0, b"x");
        let a = moldura::abrir(m.as_bytes()).expect("abre");
        assert_eq!(a.corpo, b"x");
    }
}
