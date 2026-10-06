// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// p2p.rs — ligações ponto a ponto do OnyxChat (Etapa 7)
// ---------------------------------------------------------------------
// Implementa o plano aprovado em Fase 3:
//
//   * enquadramento `[u32 LE comprimento][tipo:1B][corpo]` — o mesmo
//     formato do handshake (`docs/handshake.md`) e do relay
//     (`docs/relay.md`). O limite do corpo é derivado de um orçamento
//     de memória, não um literal (`crate::orcamento`);
//   * servidor P2P local (aceita os streams vindos do hidden service ou
//     do loopback do `TorFalso`) e cliente de saída via `BackendTor`;
//   * **fallback relay/TURN**: ligação direta primeiro; se o backend
//     falhar (ou o modo for relay) e houver endpoint configurado, abre
//     `LigacaoRelay` — rendezvous por mailbox `SHA-256(domínio‖pub)`;
//   * rate-limit por ligação (`LIMITE_FRAMES` por janela) e fecho limpo.
//
// ## Duas rotas de saída, e porque a segunda é mais exposta
//
// Este ficheiro é, com `tor_arti.rs`, um dos dois pontos onde o daemon
// fala com a rede real. A diferença entre eles é a que o projecto
// assume em `privacy_model.md` §8:
//
//   * `BackendTor::ligar` sai por um circuito Tor. O operador de rede
//     vê o operador Tor, e nada mais.
//
//   * [`LigacaoRelay`] abre um TCP **direto** a um relay alcançável.
//     O IP real do utilizador é visível para esse relay e para quem
//     estiver no caminho.
//
// É uma escolha, não um acidente: quando dois hidden services não se
// alcançam, há três opções — não comunicar, comunicar por um relay em
// clearnet, ou comunicar por um relay que também é um hidden service
// (que `privacy_model.md` §8 promete e que a implementação actual não
// suporta: `Endpoint::validar` aceita `*.onion`, mas o TCP directo a
// um `.onion` não resolve sem a arti, e por isso um endpoint `.onion`
// não funciona).
//
// O que este módulo garante é que a segunda rota é **declarada**: o
// endpoint é validado antes de qualquer I/O, e `abrir_com_aviso` escreve
// um aviso antes de ligar. O que não pode garantir — e o nenhum
// software pode — é que o relay não veja o IP. É para isso que a
// escolha de usar relay tem de ser do utilizador.
//
// Design: tudo o que é parseável é função pura/genericamente testável
// (`ler_frame`/`escrever_frame` sobre `Read`/`Write`); a rede real só
// aparece nos tipos de socket, cobertos por pares loopback nos testes.
// =====================================================================

use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::tor::{BackendTor, ErroTor};

// ---------------------------------------------------------------------
// Constantes de frame (tipos OnyxChat)
// ---------------------------------------------------------------------

/// Tamanho máximo do **corpo** de um frame.
///
/// Derivado do mesmo orçamento que o IPC (`crate::orcamento`) e não um
/// literal: o valor antigo, 16 MiB, era 255× maior do que o maior
/// envelope que o pipeline produz, e três testes o alocavam como array
/// na **pilha** — 16 MB contra um limite de 8 MB, o que abortava o
/// binário de testes em vez de reportar uma falha.
///
/// Para o valor em tempo de execução (que o utilizador pode
/// sobrescrever com `ONYXCHAT_MAX_PAYLOAD`), ver [`limite_corpo`].
pub const TAM_MAX_CORPO: usize = crate::orcamento::orcamento_padrao();

/// O limite de corpo de frame em vigor, já resolvido por override.
///
/// Os guards de [`enquadrar`] e de [`ler_frame`] usam este, e não a
/// constante: um limite que o código não cumpre é uma falsa promessa,
/// e o `Display` de [`ErroP2P`] reporta-o ao cliente — que ficaria a
/// ver um número que o daemon não aplica.
pub fn limite_corpo() -> usize {
    crate::orcamento::orcaa().0
}

/// Frame de chat: o corpo é um envelope K1→K9 completo.
pub const FRAME_CHAT: u8 = 0x01;
/// `FRIEND_REQUEST` — corpo de 209 B (`docs/handshake.md`).
pub const FRAME_FRIEND_REQUEST: u8 = 0x10;
/// `FRIEND_ACCEPT` — corpo de 177 B.
pub const FRAME_FRIEND_ACCEPT: u8 = 0x11;
/// `FRIEND_REJECT` — corpo de 81 B.
pub const FRAME_FRIEND_REJECT: u8 = 0x12;
/// `PING` de keepalive (corpo vazio).
pub const FRAME_PING: u8 = 0x20;
/// `PONG` de keepalive (corpo vazio).
pub const FRAME_PONG: u8 = 0x21;

/// Janela e teto do rate-limit por ligação (proteção anti-flood).
pub const LIMITE_FRAMES: u32 = 128;
/// Duração da janela do rate-limit.
pub const JANELA_RATE: Duration = Duration::from_secs(10);

/// Fatia máxima de espera pelo **primeiro byte** de um frame numa
/// receção faseada (`receber_por_lotes`).
///
/// Passada a fatia sem qualquer byte, o chamador (IPC) pode libertar o
/// bloqueio do estado — nenhum frame está a meio, por isso é seguro
/// deixar outros comandos correrem. Assim que chega o primeiro byte, o
/// resto do frame é lido com a espera total pedida (o stream nunca é
/// abandonado a meio de um frame por causa da fatia).
pub const FATIA_ESPERA: Duration = Duration::from_millis(500);

/// Piso da 2.ª fase de uma receção faseada: a continuação de um frame
/// **já começado** nunca expira antes disto.
///
/// Um prazo curto do cliente (`RECEBER timeout_ms`) limita a espera
/// pelo primeiro byte, mas jamais mata o stream a meio de um frame
/// (isso deixaria a ligação desalinhada — `ErroP2P::Desalinhado`).
pub const PISO_LEITURA: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------
// Constantes do protocolo de relay (docs/relay.md)
// ---------------------------------------------------------------------

/// Cliente → relay: subscrever mailbox.
pub const RELAY_SUBSCREVER: u8 = 0x01;
/// Cliente → relay: depositar frame na mailbox do par.
pub const RELAY_ENVIAR: u8 = 0x02;
/// Cliente → relay: fecho de sessão.
pub const RELAY_FECHO: u8 = 0x03;
/// Relay → cliente: erro estruturado (`código ‖ mensagem`).
pub const RELAY_ERRO: u8 = 0x81;
/// Relay → cliente: confirmação (`OK`).
pub const RELAY_OK: u8 = 0x83;
/// Relay → cliente: *push* de um frame depositado na nossa mailbox.
pub const RELAY_CAIXA: u8 = 0x84;

/// Domínio de derivação dos mailboxes (contrato com `docs/relay.md`).
pub const DOMINIO_RELAY: &[u8] = b"ONYX/RELAY/v1";

/// Espera máxima por um `OK` do relay (o relay local responde já.
pub const TIMEOUT_ACK_RELAY: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------
// Erros P2P
// ---------------------------------------------------------------------

/// Falha numa ligação/quadro — nunca transporta payloads nem chaves.
///
/// O erro de I/O guarda a mensagem (não o `io::Error`) para permitir
/// `PartialEq` nos testes e manter a resposta IPC sem dados internos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroP2P {
    /// Erro de I/O do socket (inclui ligação recusada).
    Io(String),
    /// O par fechou a ligação no limite de um frame (EOF limpo).
    Fechada,
    /// Timeout sem qualquer byte de frame — a ligação continua útil.
    TempoEsgotado,
    /// Timeout/EOF a meio de frame — o stream ficou desalinhado.
    Desalinhado,
    /// Corpo acima de [`limite_corpo`].
    PayloadGrande { obtido: usize },
    /// Comprimento de frame igual a zero.
    ComprimentoZero,
    /// Mais de `LIMITE_FRAMES` enviados na janela corrente.
    RateLimit,
    /// Endereço/destino fora do formato exigido pelo modo escolhido.
    DestinoInvalido(&'static str),
    /// Erro reportado pelo relay (`0x81`) ou frame inesperado.
    Relay(String),
    /// Modo relay pedido mas nenhum endpoint configurado.
    SemRelay,
    /// Operação de rede sem ligação ativa ao par.
    SemLigacao,
    /// Falha do backend Tor (traduzida para o código IPC dela).
    Tor(ErroTor),
}

impl fmt::Display for ErroP2P {
    /// Mensagem legível — apenas metadados, nunca conteúdo.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroP2P::Io(e) => write!(f, "I/O na ligação: {e}"),
            ErroP2P::Fechada => write!(f, "ligação fechada pelo par"),
            ErroP2P::TempoEsgotado => write!(f, "espera de frame expirou"),
            ErroP2P::Desalinhado => write!(f, "frame truncado no stream"),
            ErroP2P::PayloadGrande { obtido } => {
                write!(
                    f,
                    "frame acima de {} bytes: {obtido}",
                    limite_corpo()
                )
            }
            ErroP2P::ComprimentoZero => write!(f, "frame com comprimento zero"),
            ErroP2P::RateLimit => write!(f, "rate-limit de frames excedido"),
            ErroP2P::DestinoInvalido(m) => write!(f, "destino inválido: {m}"),
            ErroP2P::Relay(m) => write!(f, "relay: {m}"),
            ErroP2P::SemRelay => write!(f, "modo relay sem endpoint configurado"),
            ErroP2P::SemLigacao => write!(f, "sem ligação ativa ao par"),
            ErroP2P::Tor(e) => write!(f, "{e}"),
        }
    }
}

impl ErroP2P {
    /// Código de erro IPC (`docs/ipc_spec.md`).
    pub fn para_ipc(&self) -> u8 {
        match self {
            ErroP2P::Io(_) | ErroP2P::Fechada | ErroP2P::Desalinhado | ErroP2P::SemLigacao => 0x0A,
            ErroP2P::TempoEsgotado => 0x0E,
            ErroP2P::PayloadGrande { .. } => 0x09,
            ErroP2P::ComprimentoZero => 0x02,
            ErroP2P::RateLimit => 0x10,
            ErroP2P::DestinoInvalido(_) => 0x0D,
            ErroP2P::Relay(_) | ErroP2P::SemRelay => 0x11,
            ErroP2P::Tor(e) => e.para_ipc(),
        }
    }
}

impl From<ErroTor> for ErroP2P {
    /// Propaga um erro do backend Tor para o mundo P2P.
    fn from(erro: ErroTor) -> Self {
        ErroP2P::Tor(erro)
    }
}

/// Traduz erros de `Read`/`Write` para a semântica P2P.
fn de_io(erro: io::Error) -> ErroP2P {
    match erro.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => ErroP2P::TempoEsgotado,
        _ => ErroP2P::Io(erro.to_string()),
    }
}

/// Idem, mas um timeout **a meio** de um frame é desalinhamento.
fn de_io_meio_frame(erro: io::Error) -> ErroP2P {
    match erro.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => ErroP2P::Desalinhado,
        io::ErrorKind::UnexpectedEof => ErroP2P::Desalinhado,
        _ => ErroP2P::Io(erro.to_string()),
    }
}

// ---------------------------------------------------------------------
// Frame + enquadramento (puro, genérico sobre Read/Write)
// ---------------------------------------------------------------------

/// Um frame completo: tipo de 1 byte + corpo opaco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Tipo (ver `FRAME_*`/`RELAY_*`).
    pub tipo: u8,
    /// Corpo (0..=[`limite_corpo`] bytes).
    pub corpo: Vec<u8>,
}

impl Frame {
    /// Constrói um frame.
    pub fn novo(tipo: u8, corpo: Vec<u8>) -> Self {
        Frame { tipo, corpo }
    }
}

/// Serializa `[tipo][corpo]` com o cabeçalho de comprimento.
///
/// Comprimento = `1 + corpo.len()` (inclui o tipo), tal como no relay.
pub fn enquadrar(tipo: u8, corpo: &[u8]) -> Result<Vec<u8>, ErroP2P> {
    if corpo.len() > limite_corpo() {
        return Err(ErroP2P::PayloadGrande {
            obtido: corpo.len(),
        });
    }
    // Invariante: o guarda acima limita o corpo ao tecto em vigor
    // (16 MiB « u32::MAX), logo a conversão nunca falha.
    let comprimento =
        u32::try_from(1 + corpo.len()).expect("corpo limitado por `limite_corpo`");
    let mut pacote = Vec::with_capacity(4 + 1 + corpo.len());
    pacote.extend_from_slice(&comprimento.to_le_bytes());
    pacote.push(tipo);
    pacote.extend_from_slice(corpo);
    Ok(pacote)
}

/// Lê exatamente `n` bytes; timeout/EOF no meio → `Desalinhado`.
fn ler_exato<R: Read>(leitor: &mut R, n: usize) -> Result<Vec<u8>, ErroP2P> {
    let mut buffer = vec![0u8; n];
    leitor.read_exact(&mut buffer).map_err(de_io_meio_frame)?;
    Ok(buffer)
}

/// Lê um frame completo do stream.
///
/// * EOF **antes** de qualquer byte → `Fechada` (par saiu limpo);
/// * timeout antes de qualquer byte → `TempoEsgotado` (ligação útil);
/// * EOF/timeout a meio → `Desalinhado` (fechar a ligação);
/// * comprimento 0 → `ComprimentoZero`; acima do limite → `PayloadGrande`.
pub fn ler_frame<R: Read>(leitor: &mut R) -> Result<Frame, ErroP2P> {
    // Primeiro byte distingue "sem dados" de "frame incompleto".
    let mut cabecalho = [0u8; 4];
    match leitor.read(&mut cabecalho[..1]) {
        Ok(0) => return Err(ErroP2P::Fechada),
        Ok(_) => {}
        Err(erro) => return Err(de_io(erro)),
    }
    ler_apos_primeiro_byte(leitor, cabecalho)
}

/// Continua a leitura depois de lido o primeiro byte do comprimento.
///
/// Separação que permite ao IPC esperar o primeiro byte em fatia curta
/// (sem nunca abandonar um frame já começado — ver `FATIA_ESPERA`).
fn ler_apos_primeiro_byte<R: Read>(
    leitor: &mut R,
    mut cabecalho: [u8; 4],
) -> Result<Frame, ErroP2P> {
    // Restantes 3 bytes do comprimento (LE) — EOF/timeout a meio
    // significa frame truncado (o stream deixa de ser fiável).
    let resto = ler_exato(leitor, 3)?;
    cabecalho[1..4].copy_from_slice(&resto);
    let comprimento = u32::from_le_bytes(cabecalho) as usize;
    if comprimento == 0 {
        return Err(ErroP2P::ComprimentoZero);
    }
    if comprimento > 1 + limite_corpo() {
        return Err(ErroP2P::PayloadGrande {
            obtido: comprimento,
        });
    }
    let tipo = ler_exato(leitor, 1)?.remove(0);
    let corpo = ler_exato(leitor, comprimento - 1)?;
    Ok(Frame::novo(tipo, corpo))
}

/// Espera da continuação de um frame (2.ª fase): honra o prazo
/// pedido, mas nunca abaixo de `PISO_LEITURA`.
fn espera_de_leitura(espera: Option<Duration>) -> Option<Duration> {
    espera.map(|total| total.max(PISO_LEITURA))
}

/// Espera o primeiro byte do cabeçalho com a fatia indicada.
///
/// `Ok(false)` = expirou sem qualquer byte (o chamador pode libertar o
/// bloqueio do estado); `Ok(true)` = há um frame a meio, continua-se.
fn esperar_primeiro_byte<R: Read>(
    leitor: &mut R,
    cabecalho: &mut [u8; 4],
) -> Result<bool, ErroP2P> {
    match leitor.read(&mut cabecalho[..1]) {
        Ok(0) => Err(ErroP2P::Fechada),
        Ok(_) => Ok(true),
        Err(erro) => match erro.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Ok(false),
            _ => Err(de_io(erro)),
        },
    }
}

/// Escreve um frame com o enquadramento por comprimento.
pub fn escrever_frame<W: Write>(escritor: &mut W, tipo: u8, corpo: &[u8]) -> Result<(), ErroP2P> {
    let pacote = enquadrar(tipo, corpo)?;
    escritor.write_all(&pacote).map_err(de_io)?;
    escritor.flush().map_err(de_io)
}

// ---------------------------------------------------------------------
// Helpers de identidade/relay
// ---------------------------------------------------------------------

/// Deriva o mailbox do relay a partir de uma chave pública Ed25519.
///
/// `mailbox = SHA-256("ONYX/RELAY/v1" ‖ pub)` — contrato de
/// `docs/relay.md`; o relay nunca vê a `pub`, só o hash.
pub fn caixa_de_pub(publica: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(DOMINIO_RELAY);
    hasher.update(publica);
    hasher.finalize().into()
}

/// Converte 64 caracteres hex em `pub(32)` (ID de utilizador).
///
/// Rejeita tamanhos/alfabetos inválidos → caller decide o erro.
///
/// Aceita maiúsculas e minúsculas: um ID colado por um utilizador pode
/// vir em qualquer caixa. O valor canónico para comparação é sempre o
/// de [`hex_de_pub`].
pub fn pub_de_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut publica = [0u8; 32];
    // Cada par de dígitos hexadecimais forma um byte: alto ‖ baixo.
    // O charset e o comprimento já foram validados acima, por isso o
    // `expect` é uma invariante (não há ramo que possa falhar).
    for (posicao, destino) in publica.iter_mut().enumerate() {
        let par = &hex[2 * posicao..2 * posicao + 2];
        *destino = u8::from_str_radix(par, 16).expect("par hex validado acima");
    }
    Some(publica)
}

/// Forma canónica de `pub(32)` em hex: minúsculas, sem separadores.
///
/// Existe para que o ID do par tenha **uma só representação** dentro do
/// processo. Duas formas de descrever o mesmo par num sistema onde o
/// ID é a identidade são duas identidades potenciais — uma fonte clássica
/// de comparação falhada.
pub fn hex_de_pub(publica: &[u8; 32]) -> String {
    let mut saida = String::with_capacity(64);
    for byte in publica {
        // `write!` a um `String` nunca falha; `format!` por byte seria 64
        // alocações.
        use std::fmt::Write as _;
        // SAFETY de formatação: `{:02x}` de um `u8` produz sempre 2
        // caracteres hexadecimais válidos.
        let _ = write!(saida, "{byte:02x}");
    }
    saida
}

/// Contador de frames por janela — partilhado por direta e relay.
#[derive(Debug)]
struct JanelaRate {
    /// Frames enviados na janela corrente.
    contador: u32,
    /// Início da janela corrente.
    inicio: Instant,
}

impl JanelaRate {
    /// Janela nova, vazia.
    fn nova() -> Self {
        JanelaRate {
            contador: 0,
            inicio: Instant::now(),
        }
    }

    /// Regista um envio; acima do teto devolve `RateLimit`.
    fn registar(&mut self) -> Result<(), ErroP2P> {
        if self.inicio.elapsed() >= JANELA_RATE {
            self.inicio = Instant::now();
            self.contador = 0;
        }
        self.contador += 1;
        if self.contador > LIMITE_FRAMES {
            return Err(ErroP2P::RateLimit);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Modo de ligação
// ---------------------------------------------------------------------

/// Modo pedido em `LIGAR 0x05`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoLigacao {
    /// Direto: backend Tor (hidden service do par).
    Direto,
    /// Via relay/TURN configurado.
    Relay,
}

impl ModoLigacao {
    /// Decodifica o byte do IPC: `0x00` direto, `0x01` relay.
    pub fn de_ipc(codigo: u8) -> Result<Self, ErroP2P> {
        match codigo {
            0x00 => Ok(ModoLigacao::Direto),
            0x01 => Ok(ModoLigacao::Relay),
            _ => Err(ErroP2P::DestinoInvalido(
                "modo deve ser 0x00 (direto) ou 0x01 (relay)",
            )),
        }
    }
}

// ---------------------------------------------------------------------
// Servidor P2P
// ---------------------------------------------------------------------

/// Listener TCP local onde entram os streams do hidden service (via
/// hsproxy) ou do loopback do `TorFalso`.
#[derive(Debug)]
pub struct ServidorP2P {
    /// Socket de escuta.
    interlocutor: TcpListener,
    /// Porta efetivamente vinculada (para o backend publicar).
    porta: u16,
}

impl ServidorP2P {
    /// Abre o servidor na `porta` (`0` = efémera) de `endereco`.
    pub fn abrir(endereco: &str, porta: u16) -> Result<Self, ErroP2P> {
        let interlocutor = TcpListener::bind((endereco, porta)).map_err(de_io)?;
        let porta = interlocutor.local_addr().map_err(de_io)?.port();
        Ok(ServidorP2P {
            interlocutor,
            porta,
        })
    }

    /// Porta vinculada (o backend Tor encaminha para aqui).
    pub fn porta(&self) -> u16 {
        self.porta
    }

    /// Aceita a próxima ligação de entrada como ligação direta.
    pub fn aceitar(&self) -> Result<LigacaoDireta, ErroP2P> {
        let (fluxo, _origem) = self.interlocutor.accept().map_err(de_io)?;
        Ok(LigacaoDireta::nova(fluxo))
    }
}

// ---------------------------------------------------------------------
// Ligação direta
// ---------------------------------------------------------------------

/// Ligação direta ao par (TCP/Tor) com rate-limit e timeouts.
#[derive(Debug)]
pub struct LigacaoDireta {
    /// Stream subjacente.
    fluxo: TcpStream,
    /// Contador de frames enviados (rate-limit).
    janela: JanelaRate,
}

impl LigacaoDireta {
    /// Envolve um stream já estabelecido.
    pub fn nova(fluxo: TcpStream) -> Self {
        LigacaoDireta {
            fluxo,
            janela: JanelaRate::nova(),
        }
    }

    /// Porta local/remote — só para diagnóstico nos testes.
    pub fn endereco_local(&self) -> Option<String> {
        self.fluxo.local_addr().ok().map(|a| a.to_string())
    }

    /// Envia um frame aplicando o rate-limit da ligação.
    pub fn enviar(&mut self, tipo: u8, corpo: &[u8]) -> Result<(), ErroP2P> {
        self.janela.registar()?;
        escrever_frame(&mut self.fluxo, tipo, corpo)
    }

    /// Lê o próximo frame; `espera = None` bloqueia indefinidamente.
    pub fn receber(&mut self, espera: Option<Duration>) -> Result<Frame, ErroP2P> {
        self.fluxo.set_read_timeout(espera).map_err(de_io)?;
        ler_frame(&mut self.fluxo)
    }

    /// Receção **faseada** para o IPC partilhar o estado entre frames.
    ///
    /// Passo 1 espera no máximo `fatia` pelo primeiro byte: se nada
    /// chegar devolve `TempoEsgotado` (nenhum frame está a meio — é
    /// seguro o chamador libertar o bloqueio e tentar de novo). Assim
    /// que há um primeiro byte, o resto do frame é lido já com a
    /// espera total `espera` (com piso `PISO_LEITURA` — nunca se larga
    /// um frame truncado).
    pub fn receber_por_lotes(
        &mut self,
        espera: Option<Duration>,
        fatia: Duration,
    ) -> Result<Frame, ErroP2P> {
        let fatia_efetiva = match espera {
            Some(total) if total < fatia => total,
            _ => fatia,
        };
        self.fluxo
            .set_read_timeout(Some(fatia_efetiva))
            .map_err(de_io)?;
        let mut cabecalho = [0u8; 4];
        if !esperar_primeiro_byte(&mut self.fluxo, &mut cabecalho)? {
            return Err(ErroP2P::TempoEsgotado);
        }
        self.fluxo
            .set_read_timeout(espera_de_leitura(espera))
            .map_err(de_io)?;
        ler_apos_primeiro_byte(&mut self.fluxo, cabecalho)
    }

    /// Envia um frame e lê a resposta — utilitário de keepalive.
    pub fn ping(&mut self) -> Result<(), ErroP2P> {
        self.enviar(FRAME_PING, &[])?;
        let resposta = self.receber(None)?;
        if resposta.tipo != FRAME_PONG {
            return Err(ErroP2P::Relay(format!(
                "resposta a PING inesperada: 0x{:02x}",
                resposta.tipo
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Ligação via relay/TURN
// ---------------------------------------------------------------------

/// Sessão com o relay: subscritos à própria mailbox e enviamos para a
/// do par; o relay faz *push* dos frames recebidos (`0x84 CAIXA`).
#[derive(Debug)]
pub struct LigacaoRelay {
    /// Socket ligado ao relay.
    fluxo: TcpStream,
    /// Mailbox do par (destino dos `ENVIAR`).
    caixa_par: [u8; 32],
    /// Frames já entregues pelo relay e ainda por consumir.
    pendentes: VecDeque<Frame>,
    /// Contador de frames enviados (rate-limit).
    janela: JanelaRate,
}

/// Partes de um `endpoint` de relay, já validadas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint<'a> {
    /// Hostname ou endereço IP, sem porta.
    pub host: &'a str,
    /// Porta, 1–65535.
    pub porta: u16,
}

impl<'a> Endpoint<'a> {
    /// Valida `host:porta` — o formato que o `relay_server.py` escuta.
    ///
    /// # Porque isto existe
    ///
    /// O `endpoint` chegava a [`LigacaoRelay::abrir`] como uma `&str`
    /// sem qualquer validação, e ia directo para
    /// `TcpStream::connect`. Isso tem duas consequências, e a segunda é
    /// a grave:
    ///
    ///   * um `endpoint` malformado não dava erro de validação mas um
    ///     `io::Error` de resolução, que é indistinguível de "o relay
    ///     está em baixo" — o diagnóstico certo e o errado dão a mesma
    ///     coisa;
    ///
    ///   * `TcpStream::connect(&str)` resolve o argumento com
    ///     `ToSocketAddrs`, que chama `getaddrinfo` para um hostname. Um
    ///     `endpoint` que não seja um IP, enviaria uma **query DNS real**
    ///     ao resolver do sistema — fora do Tor, num sistema que se
    ///     Shibboleth compromete, e a revelar que há uma tentativa de
    ///     ligação a este nome de domínio.
    ///
    /// A validação é estrita por desenho: aceita exactamente o que o
    /// relay escuta, e recusa o resto. Um `endpoint` que não passa aqui
    /// não é um endpoint malformado — é um vector.
    ///
    /// # O que é aceite
    ///
    /// `hostname:porta` ou `ip:porta`, com o host não vazio e a porta
    /// em 1–65535. Recusa-se:
    ///
    ///   * falta de `:` — `TcpStream::connect` aceitaria um hostname
    ///     sem porta (e usaria a porta do serviço por omissão, o que
    ///     transforma uma omissão em ligação inesperada);
    ///   * porta vazia, não numérica, zero ou acima de 65535;
    ///   * mais do que um `:` — um endereço IPv6 literal tem
    ///     `[::1]:porta`, e aceitar `:` a mais abriria a porta a
    ///     endereços que o utilizador não escreveu;
    ///   * host vazio (`:8080`), que liga a `0.0.0.0` em alguns
    ///     sistemas e a `localhost` noutros;
    ///   * qualquer `..`, `/` ou `\`, que num `endpoint` vindo do
    ///     cliente pode ser tentativa de alcançar um caminho de
    ///     ficheiro em vez de uma porta de rede.
    pub fn validar(texto: &'a str) -> Result<Self, ErroP2P> {
        let Some((host, porta)) = texto.rsplit_once(':') else {
            return Err(ErroP2P::DestinoInvalido(
                "endpoint sem porta (esperado host:porta)",
            ));
        };
        // Uma `:` a mais só é legítima dentro de `[...]`, como no IPv6
        // literal `[::1]:9050`. Fora de parênteses rectos é lixo.
        if host.is_empty() {
            return Err(ErroP2P::DestinoInvalido("endpoint sem host"));
        }
        if host.contains('/') || host.contains('\\') || host.contains("..") {
            return Err(ErroP2P::DestinoInvalido(
                "endpoint com caminho ou `..` (esperado host:porta)",
            ));
        }
        if porta.is_empty() {
            return Err(ErroP2P::DestinoInvalido("endpoint sem porta"));
        }
        // `parse::<u16>` já recusa não-numéricos e valores acima de
        // 65535; resta o zero, que não é uma porta válida.
        let porta = porta
            .parse::<u16>()
            .map_err(|_| ErroP2P::DestinoInvalido("porta do endpoint não é um número"))?;
        if porta == 0 {
            return Err(ErroP2P::DestinoInvalido("porta do endpoint é zero"));
        }
        Ok(Endpoint { host, porta })
    }

    /// O `endpoint` normalizado, para `TcpStream::connect`.
    ///
    /// Reconstruído a partir das partes validadas, e não devolvido a
    /// `texto`: se a validação aceitar uma forma e a ligação usar
    /// outra, deixa de ser a mesma coisa que foi validada.
    pub fn normalizado(&self) -> String {
        format!("{}:{}", self.host, self.porta)
    }
}

/// O `stderr` do processo, como `Write`.
///
/// Existe para que os testes possam injectar um writer e a produção use
/// o destino real, sem `eprintln!` — que panica se a escrita falhar, e
/// um aviso de segurança não pode ser a causa de uma queda.
fn stderr() -> impl Write {
    std::io::stderr()
}

impl LigacaoRelay {
    /// Liga ao `endpoint` (`host:porta`) e subscreve a `caixa_propria`.
    ///
    /// ## Isto liga em clearnet, e é de propósito
    ///
    /// Ao contrário do modo directo (que passa por `BackendTor` e sai
    /// por um circuito Tor), aqui a ligação é um TCP directo ao relay.
    /// O IP real do utilizador é observável por esse relay e por quem
    /// estiver entre os dois. É a contrapartida de ter um relay quando
    /// a ligação directa não funciona, e `privacy_model.md` §8 a
    /// declara.
    ///
    /// Não há forma de isto ser invisível: um relay alcançável
    /// directamente só pode ser alcançado directamente. O que este
    /// código garante é que o caminho é **declarado** — o endpoint é
    /// validado, e [`abrir_com_aviso`] escreve um aviso antes de ligar.
    pub fn abrir(
        endpoint: &str,
        caixa_propria: [u8; 32],
        caixa_par: [u8; 32],
    ) -> Result<Self, ErroP2P> {
        // A validação está em `abrir_aviso_com`, que é quem liga — é
        // lá que a ordem (validar, avisar, ligar) está fixada, e é
        // coberta por `abrir_recusa_endpoint_invalido_sem_i_o`.
        Self::abrir_com_aviso(endpoint, caixa_propria, caixa_par)
    }

    /// Liga a um endpoint **já validado**, avisando que sai em
    /// clearnet.
    ///
    /// Separado de [`LigacaoRelay::abrir`] para que o aviso fique
    /// **antes** da ligação, e não só no caminho de erro: se o aviso
    /// dependesse da ligação, nunca apareceria quando a ligação
    /// corre bem — que é o caso que importa.
    ///
    /// O destino entra no aviso porque é o que o operador do relay vê.
    /// O relay nunca vê chaves (`docs/relay.md` §1), por isso o texto
    /// não pode incluir material nenhum.
    pub fn abrir_com_aviso(
        endpoint: &str,
        caixa_propria: [u8; 32],
        caixa_par: [u8; 32],
    ) -> Result<Self, ErroP2P> {
        Self::abrir_aviso_com(endpoint, caixa_propria, caixa_par, &mut stderr())
    }

    /// Liga ao endpoint, escrevendo o aviso em `aviso`.
    ///
    /// O aviso vai para um `&mut dyn Write` injectado em vez de ir
    /// directamente para o `stderr` por três razões:
    ///
    ///   * o `stderr` é o descritor 2 do processo de teste, partilhado
    ///     com todos os testes que correm em paralelo. Capturá-lo
    ///     exigiria redireccionar o descritor inteiro, o que não é
    ///     determinístico quando a suite corre com N threads;
    ///   * `eprintln!` **apanica** se a escrita falhar. Num disco cheio
    ///     ou com o stderr fechado, um aviso de segurança não pode
    ///     derrubar o daemon;
    ///   * o texto do aviso passa a ser verificável: o teste afirma
    ///     sobre uma `String`, não sobre o que "chamou ter lido".
    ///
    /// A produção usa `stderr()`, que é exactamente o mesmo destino de
    /// antes — o que muda é que a falha de escrita deixou de ser fatal.
    fn abrir_aviso_com(
        endpoint: &str,
        caixa_propria: [u8; 32],
        caixa_par: [u8; 32],
        aviso: &mut dyn Write,
    ) -> Result<Self, ErroP2P> {
        // Valida antes de qualquer I/O **e antes do aviso**, e o `host`
        // e a `porta` do aviso são os do `endpoint` validado — nunca os
        // que o chamador mandou. Um aviso que anuncie um destino
        // diferente do que se vai usar seria pior do que nenhum aviso.
        let validado = Endpoint::validar(endpoint)?;
        let host = validado.host;
        let porta = validado.porta;
        let endpoint = validado.normalizado();

        // A validação acontece **aqui**, e não em [`LigacaoRelay::abrir`],
        // porque é esta a função que abre a ligação: um `abrir` que
        // validasse e um `abrir_aviso_com` que não fossem a mesma
        // ordem dariam um caminho sem validação.
        //
        // E o aviso vem **depois** de validar. Na primeira versão
        // escrevia-se o aviso primeiro, e o aviso de um endpoint
        // inválido saía antes do erro que o rejeita — o que diz ao
        // utilizador «vai sair em clearnet» sobre uma ligação que não
        // vai acontecer.
        let _ = writeln!(
            aviso,
            "onyxchatd: ligação em CLEARNET ao relay {host}:{porta} — \
             o IP real é observável por este relay (privacy_model.md §3)"
        );

        let fluxo = TcpStream::connect(endpoint).map_err(de_io)?;
        let mut ligacao = LigacaoRelay {
            fluxo,
            caixa_par,
            pendentes: VecDeque::new(),
            janela: JanelaRate::nova(),
        };
        ligacao.comando(RELAY_SUBSCREVER, &caixa_propria)?;
        Ok(ligacao)
    }

    /// Envia um comando e espera o `OK` correspondente.
    fn comando(&mut self, tipo: u8, corpo: &[u8]) -> Result<(), ErroP2P> {
        escrever_frame(&mut self.fluxo, tipo, corpo)?;
        self.esperar_ok()
    }

    /// Espera um `OK`, enfileirando `CAIXA` que chegue em simultâneo.
    fn esperar_ok(&mut self) -> Result<(), ErroP2P> {
        self.fluxo
            .set_read_timeout(Some(TIMEOUT_ACK_RELAY))
            .map_err(de_io)?;
        loop {
            let frame = ler_frame(&mut self.fluxo)?;
            match frame.tipo {
                RELAY_OK => return Ok(()),
                RELAY_ERRO => return Err(ErroP2P::Relay(descrever_erro_relay(&frame.corpo))),
                RELAY_CAIXA => self.enfileirar_caixa(frame)?,
                outro => {
                    return Err(ErroP2P::Relay(format!(
                        "frame do relay inesperado: 0x{outro:02x}"
                    )))
                }
            }
        }
    }

    /// Converte `0x84 CAIXA` num frame interno (`tipo ‖ corpo`).
    fn enfileirar_caixa(&mut self, frame: Frame) -> Result<(), ErroP2P> {
        let (tipo, resto) = frame.corpo.split_first().ok_or(ErroP2P::ComprimentoZero)?;
        self.pendentes.push_back(Frame::novo(*tipo, resto.to_vec()));
        Ok(())
    }

    /// Publica um frame na mailbox do par e espera confirmação.
    pub fn enviar(&mut self, tipo: u8, corpo: &[u8]) -> Result<(), ErroP2P> {
        self.janela.registar()?;
        // ENVIAR = mailbox(32) ‖ tipo(1) ‖ corpo — o relay não vê mais.
        let mut pacote = Vec::with_capacity(33 + corpo.len());
        pacote.extend_from_slice(&self.caixa_par);
        pacote.push(tipo);
        pacote.extend_from_slice(corpo);
        if pacote.len() > limite_corpo() {
            return Err(ErroP2P::PayloadGrande {
                obtido: pacote.len(),
            });
        }
        self.comando(RELAY_ENVIAR, &pacote)
    }

    /// Devolve o próximo frame depositado na nossa mailbox.
    pub fn receber(&mut self, espera: Option<Duration>) -> Result<Frame, ErroP2P> {
        if let Some(frame) = self.pendentes.pop_front() {
            return Ok(frame);
        }
        self.fluxo.set_read_timeout(espera).map_err(de_io)?;
        let frame = ler_frame(&mut self.fluxo)?;
        self.consumir_resposta_relay(frame)
    }

    /// Receção faseada (ver `LigacaoDireta::receber_por_lotes`): se o
    /// relay não mandar nada dentro da fatia, devolve `TempoEsgotado`
    /// antes de o chamador ter de manter o bloqueio do estado.
    pub fn receber_por_lotes(
        &mut self,
        espera: Option<Duration>,
        fatia: Duration,
    ) -> Result<Frame, ErroP2P> {
        if let Some(frame) = self.pendentes.pop_front() {
            return Ok(frame);
        }
        let fatia_efetiva = match espera {
            Some(total) if total < fatia => total,
            _ => fatia,
        };
        self.fluxo
            .set_read_timeout(Some(fatia_efetiva))
            .map_err(de_io)?;
        let mut cabecalho = [0u8; 4];
        if !esperar_primeiro_byte(&mut self.fluxo, &mut cabecalho)? {
            return Err(ErroP2P::TempoEsgotado);
        }
        self.fluxo
            .set_read_timeout(espera_de_leitura(espera))
            .map_err(de_io)?;
        let frame = ler_apos_primeiro_byte(&mut self.fluxo, cabecalho)?;
        self.consumir_resposta_relay(frame)
    }

    /// Processa um frame acabado de ler do relay (OK/CAIXA/ERRO) até
    /// devolver o frame interno que o chamador pediu.
    fn consumir_resposta_relay(&mut self, mut frame: Frame) -> Result<Frame, ErroP2P> {
        loop {
            match frame.tipo {
                RELAY_CAIXA => {
                    // Corpo vazio devolve `ComprimentoZero` (nunca
                    // empilha) — logo há sempre um frame para devolver.
                    self.enfileirar_caixa(frame)?;
                    let interno = self
                        .pendentes
                        .pop_front()
                        .expect("enfileirar_caixa empurra sempre um frame");
                    return Ok(interno);
                }
                // ack tardio de um envio já confirmado — ignora.
                RELAY_OK => frame = ler_frame(&mut self.fluxo)?,
                RELAY_ERRO => return Err(ErroP2P::Relay(descrever_erro_relay(&frame.corpo))),
                outro => {
                    return Err(ErroP2P::Relay(format!(
                        "frame do relay inesperado: 0x{outro:02x}"
                    )))
                }
            }
        }
    }

    /// Fecha a sessão enviando `FECHO` (best-effort) e soltando o socket.
    pub fn fechar(mut self) {
        let _ = escrever_frame(&mut self.fluxo, RELAY_FECHO, &[]);
    }
}

/// Formata o corpo `0x81` (`código:1B ‖ mensagem`) em texto legível.
fn descrever_erro_relay(corpo: &[u8]) -> String {
    match corpo.split_first() {
        None => "erro sem código".into(),
        Some((codigo, resto)) => {
            let texto = String::from_utf8_lossy(resto);
            if texto.is_empty() {
                format!("erro 0x{codigo:02x}")
            } else {
                format!("erro 0x{codigo:02x}: {texto}")
            }
        }
    }
}

// ---------------------------------------------------------------------
// Abertura de ligação com fallback
// ---------------------------------------------------------------------

/// Pedido de ligação recolhido do comando `LIGAR 0x05`.
#[derive(Debug, Clone)]
pub struct PedidoLigacao {
    /// Modo escolhido (direto/relay).
    pub modo: ModoLigacao,
    /// Endereço do par: `.onion` (direto) ou ID hex (relay).
    pub destino: String,
    /// Endpoint do relay (`host:porta`), se configurado.
    pub endpoint_relay: Option<String>,
    /// Mailbox própria (para subscrever no relay).
    pub caixa_propria: [u8; 32],
}

/// Abre a ligação ao par com a semântica de fallback aprovada:
///
/// * **direto** → tenta o backend; se falhar e houver relay, usa-o;
/// * **relay** → exige endpoint e ID hex do par (mailbox derivada).
pub fn abrir_ligacao(
    backend: &mut dyn BackendTor,
    pedido: &PedidoLigacao,
    caixa_par: [u8; 32],
) -> Result<LigacaoP2P, ErroP2P> {
    if pedido.modo == ModoLigacao::Relay {
        let endpoint = pedido.endpoint_relay.as_deref().ok_or(ErroP2P::SemRelay)?;
        return Ok(LigacaoP2P::Relay(Box::new(LigacaoRelay::abrir(
            endpoint,
            pedido.caixa_propria,
            caixa_par,
        )?)));
    }
    // Modo direto: o backend resolve `.onion`/registro falso.
    match backend.ligar(&pedido.destino) {
        Ok(fluxo) => Ok(LigacaoP2P::Direta(Box::new(LigacaoDireta::nova(fluxo)))),
        Err(erro_direta) => match pedido.endpoint_relay.as_deref() {
            Some(endpoint) => Ok(LigacaoP2P::Relay(Box::new(LigacaoRelay::abrir(
                endpoint,
                pedido.caixa_propria,
                caixa_par,
            )?))),
            // Sem relay configurado → propaga o erro original do backend.
            None => Err(ErroP2P::Tor(erro_direta)),
        },
    }
}

// ---------------------------------------------------------------------
// Ligação P2P (união direta/relay) — o que o IPC manipula
// ---------------------------------------------------------------------

/// Ligação ativa ao par, num dos dois transportes.
#[derive(Debug)]
pub enum LigacaoP2P {
    /// Transporte direto (TCP/Tor).
    Direta(Box<LigacaoDireta>),
    /// Transporte via relay/TURN.
    Relay(Box<LigacaoRelay>),
}

impl LigacaoP2P {
    /// Envia um frame no transporte ativo.
    pub fn enviar(&mut self, tipo: u8, corpo: &[u8]) -> Result<(), ErroP2P> {
        match self {
            LigacaoP2P::Direta(l) => l.enviar(tipo, corpo),
            LigacaoP2P::Relay(l) => l.enviar(tipo, corpo),
        }
    }

    /// Recebe o próximo frame no transporte ativo.
    pub fn receber(&mut self, espera: Option<Duration>) -> Result<Frame, ErroP2P> {
        match self {
            LigacaoP2P::Direta(l) => l.receber(espera),
            LigacaoP2P::Relay(l) => l.receber(espera),
        }
    }

    /// Receção faseada no transporte ativo (`TempoEsgotado` = nenhum
    /// byte — o IPC pode libertar o bloqueio do estado).
    pub fn receber_por_lotes(
        &mut self,
        espera: Option<Duration>,
        fatia: Duration,
    ) -> Result<Frame, ErroP2P> {
        match self {
            LigacaoP2P::Direta(l) => l.receber_por_lotes(espera, fatia),
            LigacaoP2P::Relay(l) => l.receber_por_lotes(espera, fatia),
        }
    }
}

// =====================================================================
// Testes — `p2p_testes.rs`
// ---------------------------------------------------------------------
// Mesma decisão que em `ipc.rs`: os testes ficam num ficheiro próprio,
// como **filho** do módulo (para poderem tocar no que é privado) e não como
// integração (que só vê `pub`). O porquê completo está em
// `p2p_testes.rs`.
#[cfg(test)]
#[path = "p2p_testes.rs"]
mod testes;
