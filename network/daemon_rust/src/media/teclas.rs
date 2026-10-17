// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/teclas.rs — escolher a chave pelo tipo, e descobrir o tipo
// ---------------------------------------------------------------------
// A pergunta que este módulo responde é a única que o invariante de
// `docs/media.md` §1 levanta:
//
//     Como é que o receptor sabe o tipo de uma mensagem sem a rede lha
//     dizer?
//
// A resposta cabe numa frase: **o tipo está dentro da chave**, e a
// verificação da etiqueta AEAD da K1 — a primeira camada do pipeline, e
// uma tag de 16 bytes — diz qual das chaves era a certa. A rede vê
// `101 + N` bytes opacos e nunca chega a distinguir áudio de texto.
//
// ## Porque a sondagem é barata
//
// Porque a K1 é a **primeira** camada e é AEAD. Uma chave errada morre
// na verificação da etiqueta, antes de K2, K4, K5, K7, K9 e antes das
// três tags. Não é uma descifra falhada — é uma comparação de 16 bytes
// que sai a ~200 microssegundos as três.
//
// ## Porque a sondagem recebe uma closure
//
// Porque o sondar **não sabe** como é que se decifra: isso é do
// pipeline, que valida assinatura e anti-replay antes de tocar numa
// chave. Receber uma closure em vez de um `&Envelope` separa as duas
// responsabilidades e — o motivo pelo qual isto é uma escolha e não um
// atalho — torna o sondar **testável sem forjar envelopes**: os testes
// injectam um comportamento e verificam a ordem, o número de tentativas
// e a propagação do erro, que com envelopes reais exigiria três
// construções de 65 KB por caso.
// =====================================================================

use crypto_core::amizade;

use super::erros::ErroMedia;
use super::moldura::Tipo;

/// O byte do tipo de áudio no `info` do HKDF.
///
/// Função e não constante re-exportada porque [`super::moldura::Tipo`]
/// e este módulo têm de concordar no valor, e o sítio onde essa
/// concordância é imposta é a **única** definição. Se os dois escrevessem
/// `0x01` por conta própria, um dia um deles mudaria e a falha seria
/// «o áudio não abre» — que é o pior sítio para descobrir uma
/// divergência de um byte.
pub fn bytes_de_tipo_audio() -> u8 {
    amizade::TIPO_AUDIO
}

/// O byte do tipo de vídeo no `info` do HKDF.
pub fn bytes_de_tipo_video() -> u8 {
    amizade::TIPO_VIDEO
}

/// O tipo de media de que o `info` de uma chave foi derivado, ou
/// `None` se não foi nenhum.
///
/// A inversa de [`bytes_de_tipo_audio`] e [`bytes_de_tipo_video`], e
/// existe pelo mesmo motivo: quem descobre o tipo pela chave precisa de
/// o valor que a chave usou, e ir buscá-lo a outro sítio seria
/// reintroduzir a divergência que a função acima evita.
pub fn tipo_de_bytes(b: u8) -> Option<Tipo> {
    match b {
        x if x == bytes_de_tipo_audio() => Some(Tipo::Audio),
        x if x == bytes_de_tipo_video() => Some(Tipo::Video),
        _ => None,
    }
}

/// A ordem em que as chaves são testadas.
///
/// O texto vai primeiro, e por um motivo que é quase tudo o que importa
/// aqui: **o texto é o caso comum**. Uma conversa normal custa uma
/// tentativa. Se o texto fosse o último, cada mensagem de texto do
/// sistema pagaria três verificações Poly1305 para nada.
///
/// O vector é `pub` e não uma constante privada porque o cliente Python
/// tem de testar na mesma ordem — e duas listas de ordem divergentes
/// dariam um sistema que abre texto num lado e áudio no outro. Ver
/// `messenger/media.py`.
pub const ORDEM: [Tipo; 2] = [Tipo::Audio, Tipo::Video];

/// As chaves K1 de media para um envelope, por tipo.
///
/// `k_texto` entra só para ficar explícito o que **não** se faz: o texto
/// não é derivado. Devolve-se tal e qual, e quem o usar não tem de ir
/// ler a derivação para saber que é a chave de longa duração.
///
/// Devolve um par e não um slice indexado porque são **dois** e porque
/// um slice indexado por um `Tipo` que pode ser inválido seria
/// exactamente o acesso fora dos limites que este módulo existe para
/// evitar.
pub fn derivadas(
    k_texto: &[u8; 32],
    k_emissor: &[u8; 32],
    k_recetor: &[u8; 32],
    publica_emissor: &[u8; 32],
) -> Result<ChavesMedia, ErroMedia> {
    let audio =
        amizade::derivar_k1_por_tipo(k_emissor, k_recetor, publica_emissor, amizade::TIPO_AUDIO)
            .ok_or(ErroMedia::TipoSemChave(amizade::TIPO_AUDIO))?;
    let video =
        amizade::derivar_k1_por_tipo(k_emissor, k_recetor, publica_emissor, amizade::TIPO_VIDEO)
            .ok_or(ErroMedia::TipoSemChave(amizade::TIPO_VIDEO))?;

    Ok(ChavesMedia {
        texto: *k_texto,
        audio,
        video,
    })
}

/// As três chaves que um envelope pode ter sido cifrado com.
///
/// `texto` não é derivada — é a chave de longa duração, igual à que o
/// sistema usava antes de existir media. Ver [`derivadas`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChavesMedia {
    /// A chave de longa duração, para texto.
    pub texto: [u8; 32],
    /// A chave derivada de áudio.
    pub audio: [u8; 32],
    /// A chave derivada de vídeo.
    pub video: [u8; 32],
}

impl ChavesMedia {
    /// A chave de um tipo de media.
    ///
    /// Devolve `None` para um tipo que não tem chave derivada — e o
    /// único tipo com chave derivada que existe é o que não é texto.
    /// Passar [`Tipo`] aqui não é um erro porque o tipo pode vir de
    /// bytes não validados, e o `Option` é mais barato de verificar do
    /// que um pânico.
    pub fn de_media(&self, tipo: Tipo) -> Option<[u8; 32]> {
        match tipo {
            Tipo::Audio => Some(self.audio),
            Tipo::Video => Some(self.video),
        }
    }

    /// As três chaves na ordem em que [`sondar_com`] as tenta.
    ///
    /// O texto vem primeiro **por uma razão que não é de derivação**:
    /// ele não tem chave derivada nenhuma. A lista de derivação
    /// ([`ORDEM`]) tem dois elementos; esta tem três, e a diferença é
    /// exactamente o caso comum do sistema.
    ///
    /// Devolve um `Vec` e não um array de tamanho fixo para que uma
    /// chave acrescentada amanhã apareça aqui sozinha em vez de não
    /// compilar.
    pub fn por_ordem(&self) -> Vec<([u8; 32], Genero)> {
        let mut saida = vec![(self.texto, Genero::Texto)];
        for tipo in ORDEM {
            let chave = self.de_media(tipo).expect("ORDEM só tem tipos com chave");
            saida.push((chave, Genero::Media(tipo)));
        }
        saida
    }
}

/// O resultado de uma sondagem, com o texto distinguível.
///
/// Existe como tipo próprio, e não como um [`Tipo`], porque **o texto
/// não é um `Tipo`**: `Tipo` em [`super::moldura`] só tem áudio e
/// vídeo. Acrescentar-lhe uma variante `Texto` seria dizer que o texto
/// é uma moldura — e não é: a moldura é o que a media tem **a mais**.
///
/// A consequência é boa: quem recebe [`Genero::Texto`] sabe, sem
/// interpretar mais nada, que não há moldura para abrir. É uma
/// distinção que o `Option` de um quarto valor do enum tornaria
/// ambígua.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genero {
    /// Texto normal. Não tem moldura.
    Texto,
    /// Media, com o seu tipo.
    Media(Tipo),
}

/// O que uma sondagem bem sucedida devolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detectado {
    /// Se é texto, ou media de que tipo.
    pub genero: Genero,
    /// O plaintext.
    ///
    /// `Vec<u8>` e não `String`: quem chama decide se é texto (e valida
    /// UTF-8) ou são bytes de media, e validar aqui obrigaria a
    /// duplicar uma decisão que o [`super::moldura`] já sabe tomar.
    pub texto: Vec<u8>,
}

/// Descobre o tipo de um envelope, tentando as chaves por ordem.
///
/// `tentar` recebe uma chave e devolve o plaintext se a etiqueta da K1
/// verificar. É a **única** coisa que esta função sabe fazer, e é
/// deliberado: a validação da assinatura e do anti-replay é do
/// pipeline, e uma sondagem que as saltasse seria uma sondagem que
/// desceria a dados não autenticados.
///
/// ## Porquê o texto primeiro
///
/// A ordem de [`ORDEM`] é áudio, vídeo — porque a derivação só tem
/// esses dois. Mas o **texto não usa uma chave derivada**: usa a chave
/// de longa duração, e por isso é testado **primeiro**, à mão, antes de
/// [`ORDEM`]. Uma conversa normal custa uma verificação Poly1305.
///
/// ## Porquê não há um quarto teste
///
/// Porque não há um quarto tipo. Um envelope que não abra com nenhuma
/// das três é inválido, e devolvê-lo como tal é o que permite a
/// assinatura ser a fronteira: se uma mensagem não tem assinatura
/// válida, nenhuma chave a abre e o resultado é o mesmo de sempre.
pub fn sondar_com<F>(chaves: &ChavesMedia, mut tentar: F) -> Result<Detectado, ErroMedia>
where
    F: FnMut(&[u8; 32]) -> Option<Vec<u8>>,
{
    // 1. Texto — o caso comum, e a única chave que não é derivada.
    if let Some(texto) = tentar(&chaves.texto) {
        return Ok(Detectado {
            genero: Genero::Texto,
            texto,
        });
    }

    // 2 e 3. Media, pela ordem declarada.
    for tipo in ORDEM {
        let chave = chaves
            .de_media(tipo)
            .ok_or(ErroMedia::TipoSemChave(tipo.tipo_derivado()))?;
        if let Some(texto) = tentar(&chave) {
            return Ok(Detectado {
                genero: Genero::Media(tipo),
                texto,
            });
        }
    }

    Err(ErroMedia::TipoIndetectavel)
}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod testes {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Chaves de teste, todas distintas e todas com 32 bytes.
    fn chaves_de_teste() -> ChavesMedia {
        ChavesMedia {
            texto: [0x00; 32],
            audio: [0xAA; 32],
            video: [0xBB; 32],
        }
    }

    /// Uma sondagem que só abre com a chave de vídeo, e **regista** as
    /// tentativas.
    ///
    /// O registo é o que permite verificar a **ordem** e não só o
    /// resultado: uma sondagem que tentasse áudio antes do texto
    /// devolveria o tipo certo e o teste passaria na mesma.
    ///
    /// Devolve a closure e o registo, partilhados.
    ///
    /// ## Porque `Rc<RefCell<…>>` e não `RefCell<…>`
    ///
    /// Porque [`RefCell::clone`] faz **cópia profunda** do valor
    /// interior — clona o `Vec` inteiro no momento do clone. Não
    /// partilha a célula: partilha a *bandeira* de empréstimo e nada
    /// mais. A primeira versão deste helper clonava, escrevia na cópia,
    /// e o teste via um registo vazio — que é o tipo de falha em que
    /// nada dá erro e a asserção parece estar a testar a coisa errada.
    ///
    /// `Rc<RefCell<Vec<_>>>` dá a partilha que o nome sugere. E o
    /// `Rc` é justificado: o registo é um artefacto de teste, vive
    /// numa só thread (`RUST_TEST_THREADS = 1`, em
    /// `.cargo/config.toml`), e um `Arc` seria uma promessa de
    /// sincronização que nada aqui precisa.
    fn sonda<'a>(
        chaves: &'a ChavesMedia,
    ) -> (
        Rc<RefCell<Vec<[u8; 32]>>>,
        impl FnMut(&[u8; 32]) -> Option<Vec<u8>> + 'a,
    ) {
        let feitas: Rc<RefCell<Vec<[u8; 32]>>> = Rc::new(RefCell::new(Vec::new()));
        let registo = Rc::clone(&feitas);
        let fechar = move |chave: &[u8; 32]| {
            registo.borrow_mut().push(*chave);
            // Abre com a chave de vídeo, e só com ela.
            if chave == &chaves.video {
                Some(b"aberto".to_vec())
            } else {
                None
            }
        };
        (feitas, fechar)
    }

    /// A sondagem devolve o género certo, com o plaintext certo.
    ///
    /// A sonda de teste abre só com a chave de vídeo, logo o resultado
    /// tem de ser `Media(Video)`. O teste falha se alguém trocar o tipo
    /// devolvido pelo que **abriu** — que é a confusão que a derivação
    /// por tipo existe para eliminar.
    #[test]
    fn a_sondagem_devolve_o_genero_certo() {
        let c = chaves_de_teste();
        let (feitas, sonda) = sonda(&c);
        let r = sondar_com(&c, sonda).expect("a sonda abre");
        assert_eq!(r.genero, Genero::Media(Tipo::Video));
        assert_eq!(r.texto, b"aberto");
        // E a ordem é texto, áudio, vídeo — o que este helper existe
        // para mostrar.
        assert_eq!(feitas.borrow().len(), 3, "deviam ser três tentativas");
        assert_eq!(feitas.borrow().last(), Some(&c.video));
    }

    /// O texto sai como `Genero::Texto` e não como um `Tipo` qualquer.
    ///
    /// A razão de existir é [`super::moldura::abrir`]: quem recebe
    /// `Texto` não tem moldura para abrir, e este teste fixa que o
    /// sistema **diz isso** em vez de devolver um tipo de media que
    /// depois não abre.
    #[test]
    fn o_texto_sai_como_genero_texto() {
        let c = chaves_de_teste();
        let r = sondar_com(&c, |chave| {
            (chave == &c.texto).then(|| b"sou texto".to_vec())
        })
        .expect("abre com o texto");
        assert_eq!(r.genero, Genero::Texto);
        assert_eq!(r.texto, b"sou texto");
    }

    /// O texto é a **primeira** tentativa.
    ///
    /// É o teste que vale mais desta lista. Uma conversa normal — que é
    /// a totalidade das mensagens que o sistema transporta hoje — tem de
    /// custar **uma** verificação, e não três.
    #[test]
    fn o_texto_e_a_primeira_tentativa() {
        let c = chaves_de_teste();
        let mut tentativas: Vec<[u8; 32]> = Vec::new();
        let mut tentar = |chave: &[u8; 32]| {
            tentativas.push(*chave);
            if chave == &c.texto {
                Some(b"sou texto".to_vec())
            } else {
                None
            }
        };

        sondar_com(&c, &mut tentar).expect("a sonda abre com o texto");

        assert_eq!(
            tentativas,
            vec![c.texto],
            "o texto tem de ser a única tentativa: custou {}",
            tentativas.len()
        );
    }

    /// O texto custa uma tentativa; a media custa três.
    ///
    /// O número é o orçamento, e escrevê-lo no teste é o que impede uma
    /// alteração futura de o mudar em silêncio.
    #[test]
    fn o_numero_de_tentativas_depende_do_tipo() {
        let c = chaves_de_teste();

        let mut n_texto = 0usize;
        {
            let mut t = |chave: &[u8; 32]| {
                n_texto += 1;
                if chave == &c.texto {
                    Some(vec![1])
                } else {
                    None
                }
            };
            sondar_com(&c, &mut t).expect("abre");
        }
        assert_eq!(n_texto, 1, "o texto não pode custar mais que uma tentativa");

        let mut n_media = 0usize;
        {
            let mut t = |chave: &[u8; 32]| {
                n_media += 1;
                if chave == &c.video {
                    Some(vec![1])
                } else {
                    None
                }
            };
            sondar_com(&c, &mut t).expect("abre");
        }
        assert_eq!(n_media, 3, "a media custa texto + áudio + vídeo");
    }

    /// Áudio é tentado **antes** de vídeo.
    ///
    /// A ordem de [`ORDEM`] é áudio, vídeo, e este teste existe para que
    /// inverter a constante não mude só um número: mude o número de
    /// Poly1305 de cada conversa.
    #[test]
    fn o_audio_e_tentado_antes_do_video() {
        let c = chaves_de_teste();
        let mut tentativas: Vec<[u8; 32]> = Vec::new();
        let mut t = |chave: &[u8; 32]| {
            tentativas.push(*chave);
            if chave == &c.audio {
                Some(vec![1])
            } else {
                None
            }
        };
        sondar_com(&c, &mut t).expect("abre com áudio");

        assert_eq!(
            tentativas,
            vec![c.texto, c.audio],
            "a ordem é texto, áudio, e nada mais"
        );
    }

    /// Nenhuma das três chaves a abre ⇒ tipo indetectável.
    ///
    /// E não um quarto erro: o receptor precisa de saber que a mensagem
    /// não é dele, e não qual dos testes falhou.
    #[test]
    fn nenhuma_chave_abre_da_erro_unico() {
        let c = chaves_de_teste();
        let r = sondar_com(&c, |_| None);
        assert_eq!(r, Err(ErroMedia::TipoIndetectavel));
    }

    /// A sondagem **para** na primeira que abre.
    ///
    /// Um teste que continuasse a tentar depois de abrir seria apenas
    /// mais lento; um que verificasse o resultado errado também
    /// devolveria um tipo plausível. Este fixa que o texto ganha ao
    /// áudio mesmo quando as duas chaves abririam.
    #[test]
    fn a_sondagem_para_na_primeira_que_abre() {
        let c = chaves_de_teste();
        let mut tentativas = 0usize;
        let mut t = |chave: &[u8; 32]| {
            tentativas += 1;
            // Abre com todas — o que devolve tem de ser o texto, e o
            // número de tentativas tem de ser 1.
            Some(vec![if *chave == c.texto { 0 } else { 1 }])
        };
        let r = sondar_com(&c, &mut t).expect("abre");
        assert_eq!(tentativas, 1, "continuou a tentar depois de abrir");
        assert_eq!(r.texto, vec![0], "devolveu o plaintext de outra chave");
    }

    /// Uma chave de media nunca é a chave de texto.
    ///
    /// Se esta asserção falhar, o mecanismo de sondagem é uma
    /// formalidade: um envelope de texto abriria como áudio.
    #[test]
    fn os_espacos_de_chave_nao_se_intersectam() {
        let c = chaves_de_teste();
        assert_ne!(c.texto, c.audio);
        assert_ne!(c.texto, c.video);
        assert_ne!(c.audio, c.video);
    }

    /// As chaves derivadas batem com a fórmula, e as de texto não são
    /// derivadas.
    ///
    /// O teste confirma que [`derivadas`] passa o `k_texto` **tal e
    /// qual** — a propriedade de `docs/media.md` §1.2 que garante que o
    /// texto continua byte-a-byte igual ao que produzia antes.
    #[test]
    fn o_texto_nao_e_derivado() {
        let k_emissor = [0x11u8; 32];
        let k_recetor = [0x22u8; 32];
        let publica = [0xAAu8; 32];
        let k_texto = [0x55u8; 32];

        let c = derivadas(&k_texto, &k_emissor, &k_recetor, &publica).expect("deriva");

        assert_eq!(c.texto, k_texto, "a chave de texto foi mexida");
        assert_eq!(
            c.audio,
            amizade::derivar_k1_por_tipo(&k_emissor, &k_recetor, &publica, amizade::TIPO_AUDIO)
                .unwrap(),
        );
        assert_eq!(
            c.video,
            amizade::derivar_k1_por_tipo(&k_emissor, &k_recetor, &publica, amizade::TIPO_VIDEO)
                .unwrap(),
        );
    }
}
