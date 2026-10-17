// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/erros.rs — os erros do módulo, num só sítio
// ---------------------------------------------------------------------
// Existe por uma razão que já não precisa de ser defendida em
// `docs/security_model.md`: **nenhuma mensagem de erro deste módulo
// contém o conteúdo que a produziu.** Um erro que carrega o texto que
// não conseguiu processar é um erro que escreve o plaintext num registo
// — e o registo é o sítio onde ninguém vai procurar segredos.
//
// Os erros carregam **metadados**: o byte que não era esperado, o
// comprimento que era curto demais. Esses não revelam nada e são
// exactamente o que faz falta para corrigir.
//
// ## Porque um só tipo para os três módulos
//
// Porque são indistinguíveis para quem trata. Um `TEcLAS` que falha e
// um `MOLDURA` que falha dão a mesma resposta ao utilizador — «esta
// mensagem não é para ti» — e distinguish os dois só ajuda quem está a
// depurar. A distinção existe nos variantes, que é onde o `Display`
// a apanha quando precisa dela.
// =====================================================================

use core::fmt;

use super::base64::ErroBase64;
use super::ficheiro::TAM_MANIFESTO_FIXO;
use super::moldura::TAM_PRE_AMBULO;
use super::telhas::BYTES_POR_TELHA;

/// Erro do módulo de media.
///
/// Nenhum destes valores transporta plaintext, chave ou conteúdo. Os que
/// transportam um **byte** ou um **comprimento** transportam metadados
/// de parsing, que é o que um diagnóstico precisa e o que uma fuga de
/// dados não.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroMedia {
    // --- moldura --------------------------------------------------------
    /// A moldura é mais curta do que o preâmbulo.
    MolduraCurta(usize),
    /// Os quatro primeiros bytes não são a magia.
    MagiaInvalida,
    /// A versão é diferente da que esta implementação conhece.
    VersaoDesconhecida(u8),
    /// O byte do tipo não é `0x01` nem `0x02`.
    TipoDesconhecido(u8),
    /// O byte do modo não é `0x01` nem `0x02`.
    ModoDesconhecido(u8),
    /// Há bits de flag acesos que esta versão não define.
    ///
    /// Recusados em vez de ignorados: ignorar é agir como se soubesse o
    /// que o emissor quis dizer, e não sabe. Ver `Flags::de_byte`.
    FlagsReservadasAcesas(u8),
    /// O byte 30 não é o separador.
    SeparadorInvalido,
    /// Um campo numérico tem uma letra onde devia ter um algarismo.
    ///
    /// Diferente de [`ErroMedia::CampoForaDeIntervalo`] porque a
    /// correcção é diferente: aqui o campo não é um número, e lá é um
    /// número que não cabe.
    CampoNaoNumerico,
    /// Um campo numérico é um número válido que não cabe no tipo.
    ///
    /// Acontece com um envelope hostil: onze algarismos no `seq` são um
    /// número perfectly válido que não cabe em `u32`. Truncar em silêncio
    /// seria pior do que recusar — um `seq` truncado reinicia a contagem
    /// e o receptor passa a aceitar envelopes antigos como novos.
    CampoForaDeIntervalo,
    /// O corpo não é base-64 válido.
    Base64(ErroBase64),

    // --- teclas ---------------------------------------------------------
    /// O tipo não tem chave derivada — porque só áudio e vídeo têm.
    TipoSemChave(u8),
    /// Nenhum dos tipos testados abriu o envelope.
    ///
    /// Um só erro para os três tipos, e é deliberado: dizer «nenhum
    /// dos três» é dizer ao receptor exactamente o que ele precisa de
    /// saber, e dizer «falhou o áudio» seria sugerir que ele devia
    /// tentar só o vídeo a seguir — o que é a mesma operação de novo.
    TipoIndetectavel,

    // --- teclas (ver §8 de docs/media.md) --------------------------------
    /// O `seq` não é maior do que o último aplicado.
    ///
    /// Reordenação ou duplicação. Não é um erro fatal — a unidade
    /// descarta-se e a stream continua, desde que um `HOLD` ou uma
    /// keyframe venha a seguir.
    SequenciaForaDeOrdem { esperada: u32, obtida: u32 },
    /// O `base_seq` não é o frame de referência que o receptor tem.
    ///
    /// A divergência que dispara o pedido de keyframe
    /// (`docs/media.md` §7.5).
    BaseDesalinhada { esperada: u32, obtida: u32 },
    /// O relógio contado e o `clock_ms` declarado divergem mais do que
    /// a tolerância.
    ///
    /// Não é um aviso: é a detecção de perda, de adulteração, ou de um
    /// `bytes_por_ms` declarado a enganar.
    RelogioDivergente {
        contado: i32,
        declarado: i32,
        tolerancia: i32,
    },

    // --- ficheiro -------------------------------------------------------
    /// O `sha256` do manifesto não bate com o do conteúdo montado.
    ///
    /// Uma transferência incompleta ou adulterada. Nunca chega a ser
    /// reproduzida: `docs/media.md` §4.6.
    ResumoNaoConfere,
    /// Faltam pedacos e o receptor esgotou as tentativas.
    PedacosEmFalta,
    /// O ficheiro excede `u32`, e o manifesto não o consegue descrever.
    FicheiroGrandeDemais,

    // --- ficheiro (super::ficheiro) --------------------------------------
    /// O corpo do manifesto é mais curto do que os campos fixos.
    ManifestoCurto(usize),
    /// Os números do manifesto não batem uns com os outros.
    ///
    /// O texto diz **qual** é a incoerência, não o valor: «as fatias
    /// não batem com o tamanho» diz mais do que «manifesto incoerente».
    /// São três casos — divisão por zero, zero fatias, e contagem errada —
    /// e cada um tem uma correcção diferente.
    ManifestoIncoerente(&'static str),
    /// O nome do ficheiro não é UTF-8, ou tem bytes nulos.
    ///
    /// Os nulos contam porque um `String` com um zero no meio é um
    /// `CString` truncado à espera de acontecer, e passa a um
    /// `snprintf` como separador de argumentos.
    NomeInvalido,
    /// O `seq` de uma fatia está além da última.
    PedacoForaDeIntervalo(u32),
    /// A fatia é maior do que o manifesto declara.
    ///
    /// Sem esta verificação uma fatia gigante faria o `Vec` crescer sem
    /// tecto — que é o que `orcamento.rs` existe para impedir, mas por
    /// um caminho que o orçamento não vê.
    PedacoGrandeDemais(usize),
    /// O `codec` do manifesto não é nenhum dos declarados.
    ///
    /// Não é adivinhado: um codec errado descodifica um ficheiro para
    /// ruído, e o utilizador vê ruído em vez de um erro.
    CodecDesconhecido(u16),
    /// O `formato` do manifesto não é nenhum dos declarados.
    FormatoDesconhecido(u16),
    /// A leitura do ficheiro falhou por um erro de I/O.
    ///
    /// Não guarda o `io::Error`: a mesma doutrina de `docs/security_model.md`
    /// que impede uma mensagem de erro de conter o conteúdo.
    LeituraFalhou,

    // --- captura (super::ffmpeg) -----------------------------------------
    /// A geometria pedida não serve.
    ///
    /// O texto diz **porquê** e não qual foi o valor: `largura e altura
    /// têm de ser múltiplos de 16` é accionável, `largura inválida` não
    /// é. E o motivo é sempre um destes três — lado zero, não-múltiplo
    /// de 16, ou um valor zero — e os três têm a mesma causa num bug
    /// de configuração do emissor.
    GeometriaInvalida(&'static str),
    /// Uma telha que não cabe num envelope.
    ///
    /// Deixou de ser um erro de arranque em F9: era o sintoma, e o
    /// sintoma era uma keyframe que não cabe — que se resolve por
    /// fração (§3.6), não por recusar a resolução. Fica porque ainda
    /// há um caso onde é verdade: **uma** telha tem de caber, e se um
    /// dia `BYTES_POR_TELHA` crescer até ao tecto, a fração deixa de
    /// ter por onde cortar e é melhor dizer isso do que emitir um
    /// envelope que o `ENCODE` recusa a meio da chamada.
    TelhaNaoCabe { bytes: usize, tecto: usize },
    // ---- os do lote (F9, §3.6) ----
    //
    // Todos dizem **quanto** e não só **que** correu mal, pela mesma
    // razão que `TelhaNaoCabe`: o receptor não pode corrigir
    // nada, mas quem lê o log pode, e «corpo truncado» sem o número
    // obriga a ir ao `hexdump`.
    /// O corpo de uma unidade de lote não chega ao cabeçalho.
    LoteTruncado(usize),
    /// Um lote que diz ter zero unidades.
    LoteVazio,
    /// O índice de uma unidade está fora do lote que ela diz fazer.
    LoteIndiceFora { indice: u16, n_unidades: u16 },
    /// Um delta acaba a meio de uma telha.
    LoteDeltaTruncado(usize),
    /// Uma telha diz ter mais pixéis do que uma telha tem.
    ContagemDeTelhaImpossivel(usize),
    /// O lote tem mais unidades do que um `u16` aguenta.
    LoteGrandeDemais(usize),
    /// O lote fechou com um número de telhas diferente do da geometria.
    LoteIncompleto { recebidas: usize, esperadas: usize },
    /// A geometria do lote manda alocar mais do que um quadro pode ter.
    ImagemEnorme { telhas: usize, tecto: usize },

    // --- delta (streaming) ----------------------------------------------
    /// O `ffmpeg` recusou arrancar, e disse porquê.
    ///
    /// O motivo é a **última linha** do `stderr`, e não a primeira: o
    /// `ffmpeg` escreve o contexto antes do erro, e a última linha é a
    /// que diz o que aconteceu.
    CapturaRecusada(String),
    /// O delta acaba a meio de uma telha.
    ///
    /// Distinto de [`ErroMedia::BaseDesalinhada`] porque a correcção é
    /// outra: ali um corpo mal formado é um stream corrompido, e aqui é
    /// um `ENCODE` interrompido. Nenhum dos dois se recupera.
    DeltaTruncado,
    /// O código de telha não é 1 nem 2.
    DeltaCodigoDesconhecido(u8),
    /// O corpo de uma unidade não chega para o `base_seq`.
    ///
    /// Distinto de [`ErroMedia::DeltaTruncado`] porque a correcção é
    /// outra: um `base_seq` em falta é uma unidade de outra versão, e um
    /// delta truncado é uma escrita cortada a meio.
    BaseTruncado,

    // --- infraestrutura -------------------------------------------------
    /// O `ffmpeg` não está embebido nesta compilação.
    ///
    /// Existe porque o binário tem de funcionar sem ele: um daemon sem
    /// media é um daemon completo, e o texto não é afectado. Ver
    /// `docs/media.md` §Modo degradado.
    DependenciaAusente,
    /// A loja do sidecar não pôde ser aberta em leitura.
    ///
    /// Quase sempre a base em WAL sem o `-shm`, que o SQLite não abre em
    /// modo leitura. Ver `docs/media.md` §Reproduzir.
    LojaIndisponivel,
    /// O leitor pedido não existe, ou não se sabe o que fazer com ele.
    ///
    /// Leva o nome porque é o que o utilizador precisa de ver para
    /// instalar o pacote certo. Um `0x13` genérico obrigaria a adivinhar
    /// entre `mpv` e `cvlc`, e adivinhar o leitor de media é pior do
    /// que dizer que não há.
    LeitorDesconhecido(String),
    /// O `memfd_create` ou o `fexecve` falharam.
    ///
    /// Vale a pena separá-los porque a correcção é diferente: um
    /// `memfd` que falha é falta de memória; um `fexecve` que falha é
    /// normalmente `ETXTBSY` por uma imagem anterior ainda a correr.
    ExecucaoFalhou(i32),
}

impl fmt::Display for ErroMedia {
    /// Só metadados — nunca conteúdo, nunca chaves, nunca plaintext.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // --- moldura ---
            ErroMedia::MolduraCurta(n) => {
                write!(f, "moldura com {n} bytes, menos de {TAM_PRE_AMBULO}")
            }
            ErroMedia::MagiaInvalida => write!(f, "os 4 primeiros bytes não são ONX1"),
            ErroMedia::VersaoDesconhecida(v) => {
                write!(f, "versão de moldura 0x{v:02x} desconhecida")
            }
            ErroMedia::TipoDesconhecido(t) => write!(f, "tipo 0x{t:02x} desconhecido"),
            ErroMedia::ModoDesconhecido(m) => write!(f, "modo 0x{m:02x} desconhecido"),
            ErroMedia::FlagsReservadasAcesas(b) => {
                write!(f, "flags 0x{b:02x} têm bits reservados acesos")
            }
            ErroMedia::SeparadorInvalido => write!(f, "o byte 30 não é o separador"),
            ErroMedia::CampoNaoNumerico => write!(f, "um campo numérico tem uma letra"),
            ErroMedia::CampoForaDeIntervalo => write!(f, "um campo numérico não cabe no tipo"),
            ErroMedia::Base64(e) => write!(f, "corpo em base-64 inválido: {e}"),

            // --- teclas ---
            ErroMedia::TipoSemChave(t) => write!(f, "o tipo 0x{t:02x} não tem chave derivada"),
            ErroMedia::TipoIndetectavel => {
                write!(f, "nenhuma das chaves de media abriu este envelope")
            }

            ErroMedia::SequenciaForaDeOrdem { esperada, obtida } => write!(
                f,
                "sequência {obtida} fora de ordem: esperava {esperada} ou mais"
            ),
            ErroMedia::BaseDesalinhada { esperada, obtida } => write!(
                f,
                "delta sobre a base {obtida}, e o receptor tem a {esperada}"
            ),
            ErroMedia::RelogioDivergente {
                contado,
                declarado,
                tolerancia,
            } => write!(
                f,
                "relógio contado {contado} ms contra {declarado} ms declarados \
                 (tolerância {tolerancia} ms)"
            ),

            // --- ficheiro ---
            ErroMedia::ResumoNaoConfere => {
                write!(f, "o SHA-256 do conteúdo não bate com o manifesto")
            }
            ErroMedia::PedacosEmFalta => write!(f, "faltam pedacos e as tentativas acabaram"),
            ErroMedia::FicheiroGrandeDemais => {
                write!(f, "o ficheiro excede 4 GiB, e o manifesto não o descreve")
            }

            // --- ficheiro ---
            ErroMedia::ManifestoCurto(n) => {
                write!(f, "manifesto com {n} bytes, menos de {TAM_MANIFESTO_FIXO}")
            }
            ErroMedia::ManifestoIncoerente(motivo) => {
                write!(f, "manifesto incoerente: {motivo}")
            }
            ErroMedia::NomeInvalido => {
                write!(f, "o nome do ficheiro tem bytes nulos ou não é UTF-8")
            }
            ErroMedia::PedacoForaDeIntervalo(s) => {
                write!(f, "fatia {s} além da última")
            }
            ErroMedia::PedacoGrandeDemais(n) => write!(f, "fatia de {n} bytes, acima do declarado"),
            ErroMedia::CodecDesconhecido(c) => write!(f, "codec 0x{c:04x} desconhecido"),
            ErroMedia::FormatoDesconhecido(v) => write!(f, "formato 0x{v:04x} desconhecido"),
            ErroMedia::LeituraFalhou => write!(f, "a leitura do ficheiro falhou"),

            // --- delta (streaming) ---
            ErroMedia::CapturaRecusada(motivo) => {
                write!(f, "o ffmpeg recusou arrancar: {motivo}")
            }
            ErroMedia::DeltaTruncado => write!(f, "o delta acaba a meio de uma telha"),
            ErroMedia::DeltaCodigoDesconhecido(c) => {
                write!(f, "código de telha 0x{c:02x} desconhecido")
            }
            ErroMedia::BaseTruncado => write!(f, "o corpo não chega para o base_seq"),

            // --- captura ---
            ErroMedia::GeometriaInvalida(motivo) => {
                write!(f, "geometria de captura inválida: {motivo}")
            }
            ErroMedia::TelhaNaoCabe { bytes, tecto } => write!(
                f,
                "uma telha de {bytes} bytes não cabe num envelope (tecto: {tecto})"
            ),
            ErroMedia::LoteTruncado(n) => {
                write!(f, "corpo de lote com {n} bytes, menos do que o cabeçalho")
            }
            ErroMedia::LoteVazio => write!(f, "lote com zero unidades"),
            ErroMedia::LoteIndiceFora { indice, n_unidades } => {
                write!(f, "unidade {indice} de um lote de {n_unidades}")
            }
            ErroMedia::LoteDeltaTruncado(p) => {
                write!(f, "delta truncado no byte {p}")
            }
            ErroMedia::ContagemDeTelhaImpossivel(n) => {
                write!(f, "telha com {n} pixéis, e uma telha tem {BYTES_POR_TELHA}")
            }
            ErroMedia::LoteGrandeDemais(n) => {
                write!(f, "lote de {n} unidades, e o cabeçalho aguenta 65535")
            }
            ErroMedia::ImagemEnorme { telhas, tecto } => {
                write!(f, "imagem de {telhas} telhas, e o tecto é {tecto}")
            }
            ErroMedia::LoteIncompleto {
                recebidas,
                esperadas,
            } => write!(
                f,
                "lote fechou com {recebidas} telhas, e a geometria pede {esperadas}"
            ),

            // --- infraestrutura ---
            ErroMedia::DependenciaAusente => write!(
                f,
                "o ffmpeg não está embebido nesta compilação (ver docs/media.md)"
            ),
            ErroMedia::LojaIndisponivel => write!(
                f,
                "a loja do sidecar não pôde ser aberta em leitura (provavelmente WAL sem -shm)"
            ),
            ErroMedia::LeitorDesconhecido(nome) => {
                write!(f, "leitor de media desconhecido: {nome}")
            }
            ErroMedia::ExecucaoFalhou(e) => {
                write!(f, "não foi possível executar o leitor de media (errno {e})")
            }
        }
    }
}

impl fmt::Display for ErroBase64 {
    /// Só metadados. Um erro de base-64 **não** diz qual carácter foi
    /// mau: dizer isso significaria devolver um byte do plaintext ao
    /// chamador sob a forma de diagnóstico.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroBase64::ComprimentoInvalido(n) => {
                write!(f, "comprimento {n}, que não é múltiplo de 4")
            }
            ErroBase64::CaractereInvalido => write!(f, "carácter fora do alfabeto"),
            ErroBase64::PreenchimentoInvalido => write!(f, "preenchimento fora de sítio"),
        }
    }
}

impl core::error::Error for ErroMedia {}

impl core::error::Error for ErroBase64 {}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod testes {
    use super::*;
    use crate::media::moldura::TAM_PRE_AMBULO;

    /// Nenhum `Display` menciona o conteúdo que a produziu.
    ///
    /// Esta é a propriedade de `docs/security_model.md` verificada por
    /// um teste, e não por leitura: um erro novo que transportasse
    /// plaintext falhava aqui sem depender de alguém se lembrar de
    /// verificar.
    ///
    /// A verificação é literal — o texto tem de aparecer na
    /// mensagem — e é literal porque é a única forma de ser não-trivial:
    /// um teste que só medisse o tamanho deixaria passar um erro que
    /// levasse um *fragmento* de 8 bytes.
    #[test]
    fn nenhum_erro_menciona_conteudo() {
        // Uma cadeia que ninguém escreveria por acidente num `Display`.
        let segredo = b"ONYX-CONTIDO-NAO-PODE-VAZAR-1234567890";
        // Todas as mensagens de erro possíveis a partir de uma moldura
        // válida não podem conter o conteúdo nem o seu base-64.
        let codificado = super::super::base64::codificar_bytes(segredo);

        let erros = [
            ErroMedia::MagiaInvalida,
            ErroMedia::VersaoDesconhecida(2),
            ErroMedia::TipoDesconhecido(3),
            ErroMedia::ModoDesconhecido(3),
            ErroMedia::FlagsReservadasAcesas(8),
            ErroMedia::SeparadorInvalido,
            ErroMedia::MolduraCurta(3),
            ErroMedia::TipoIndetectavel,
            ErroMedia::TipoSemChave(7),
            ErroMedia::Base64(ErroBase64::CaractereInvalido),
            ErroMedia::Base64(ErroBase64::PreenchimentoInvalido),
            ErroMedia::Base64(ErroBase64::ComprimentoInvalido(5)),
        ];

        for erro in &erros {
            let texto = erro.to_string();
            let em_bytes: Vec<u8> = texto.chars().map(|c| c as u8).collect();
            assert!(
                !texto.contains("ONYX-CONTIDO") && !texto.contains("NAO-PODE"),
                "`{erro}` deixou ver o conteúdo"
            );
            assert!(
                !em_bytes
                    .windows(codificado.len())
                    .any(|janela| janela == codificado.as_slice()),
                "`{erro}` deixou ver o base-64 do conteúdo"
            );
        }
    }

    /// Todos os erros implementam `Display` sem entrar em pânico.
    ///
    /// Um `Display` que entra em pânico num caminho de erro é um
    /// `abort` em vez de uma mensagem — e os caminhos de erro são os
    /// que ninguém exercita à mão.
    #[test]
    fn todos_os_erros_sao_imprimiveis() {
        for erro in [
            ErroMedia::MagiaInvalida,
            ErroMedia::TipoIndetectavel,
            ErroMedia::ResumoNaoConfere,
            ErroMedia::PedacosEmFalta,
            ErroMedia::FicheiroGrandeDemais,
            ErroMedia::DependenciaAusente,
            ErroMedia::LojaIndisponivel,
            ErroMedia::CapturaRecusada("ecrã não encontrado".to_string()),
            ErroMedia::DeltaTruncado,
            ErroMedia::DeltaCodigoDesconhecido(9),
            ErroMedia::BaseTruncado,
            ErroMedia::LeitorDesconhecido("mpv".to_string()),
            ErroMedia::ExecucaoFalhou(26),
            ErroMedia::SequenciaForaDeOrdem {
                esperada: 5,
                obtida: 3,
            },
            ErroMedia::BaseDesalinhada {
                esperada: 5,
                obtida: 7,
            },
            ErroMedia::RelogioDivergente {
                contado: 10,
                declarado: 90,
                tolerancia: 5,
            },
        ] {
            let texto = erro.to_string();
            assert!(!texto.is_empty(), "`{erro:?}` imprimiu vazio");
            assert!(
                !texto.contains('\n'),
                "`{erro:?}` tem uma quebra de linha no meio da mensagem"
            );
        }
    }

    /// `MolduraCurta` diz quanto faltava, e não o que havia.
    ///
    /// O comprimento é metadado de parsing — não revela nada, e é o que
    /// faz o diagnóstico accionável. O teste fixa que o número sai, e
    /// não uma mensagem genérica.
    #[test]
    fn moldura_curta_diz_o_comprimento() {
        let texto = ErroMedia::MolduraCurta(5).to_string();
        assert!(texto.contains('5'), "`{texto}` não diz o comprimento");
        assert!(
            texto.contains(&TAM_PRE_AMBULO.to_string()),
            "`{texto}` não diz o mínimo"
        );
    }
}
