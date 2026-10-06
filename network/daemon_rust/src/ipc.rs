// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// ipc.rs — servidor Unix Domain Socket + protocolo do daemon
// ---------------------------------------------------------------------
// Implementa `docs/ipc_spec.md`:
//   * enquadramento `[u32 LE comprimento][carga útil]` (≤ 16 MiB);
//   * sessão começa com `0x00 HELLO` (versão explícita) — pedidos
//     anteriores são rejeitados com `0x02`;
//   * autenticação local por `SO_PEERCRED`: uid ≠ do daemon → a
//     ligação é fechada **sem resposta**;
//   * pedidos `0x01 ENCODE` / `0x02 DECODE` (estadoless, puros);
//   * pedidos de rede/handshake `0x03..0x0C` sobre o `Estado` comum
//     (Etapa 7: Tor, P2P, relay e troca de chaves de amizade);
//   * socket em `$ONYXCHAT_SOCKET` ou `/tmp/onyxchat-{uid}.sock`, 0600;
//   * o daemon nunca aborta por dados do cliente — tudo vira `ERRO`
//     com código legível, sem chaves/nonces/payloads no texto.
//
// Modelo síncrono: cada ligação é atendida numa thread própria com
// múltiplos pedidos sequenciais (cliente bloqueia até à resposta). O
// `Estado` (rede + amizades) é **partilhado** por todas as ligações —
// o cliente Python abre uma ligação por pedido e o daemon mantém a
// sessão P2P entre pedidos.
//
// Disciplina de bloqueios (nunca inverte a ordem):
//   1. `rede`  — sessão Tor/P2P/handshake; só é mantido durante
//      operações de rede (a receção usa fatias e liberta entre frames);
//   2. `amigos` — identidades de amigos + nonces de handshake (curto);
//      desde G2 só guarda a `publica` de cada amigo (32 B por par), não
//      as chaves: quem precisa das chaves é o cliente, que as recebe na
//      resposta IPC e as guarda no seu keystore;
//   3. `nonces1` — anti-replay do chat, mantido durante um `DECODE`.
// =====================================================================

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::anti_replay::RegistoNonce1;
use crate::envelope::TAM_MINIMO;
use crate::orcamento;
use crate::erros::ErroPipeline;
use crate::handshake::{self, Amizade, RegistoAmigos, TAM_NONCE, TAM_PUB};
use crate::p2p::{self, ErroP2P, LigacaoP2P, ModoLigacao, PedidoLigacao, ServidorP2P};
use crate::pipeline::{self, Chaves};
use crate::tor::{BackendTor, EstadoTor};
use crypto_core::Segredo;
use zeroize::Zeroize;

/// Tamanho máximo de uma carga útil.
///
/// Não é um literal: é derivado de um orçamento de memória, e o
/// utilizador pode sobrescrevê-lo com `ONYXCHAT_MAX_PAYLOAD`. O valor
/// antigo, 16 MiB, era **255× maior** do que o maior corpo que o
/// protocolo produz (65 820 B num `DECODE`) e não tinha nenhuma
/// relação com o que o daemon sabe construir — ver
/// `crate::orcamento` para a derivação e as consequências de segurança
/// a longo prazo.
///
/// Para código que precise do limite em tempo de compilação (arrays de
/// tamanho fixo, constantes de tipo), use [`MAX_PAYLOAD`] com cuidado:
/// ele reflecte o valor por omissão, não o override. Para comparação
/// em tempo de execução, use [`limite_carga`].
pub fn limite_carga() -> usize {
    orcamento::orcaa().0
}

/// O valor por omissão, sem ler o ambiente.
///
/// Para documentação, testes e constantes. O caminho quente usa
/// [`limite_carga`].
pub const MAX_PAYLOAD: usize = orcamento::orcamento_padrao();

/// Comando HELLO: primeiro pedido da sessão — declara a versão do
/// protocolo e recebe a versão/build do daemon (`ipc_spec.md` §HELLO).
pub const CMD_HELLO: u8 = 0x00;
/// Comando ENCODE: cifrar mensagem → envelope.
pub const CMD_ENCODE: u8 = 0x01;
/// Comando DECODE: verificar assinatura e decifrar envelope.
pub const CMD_DECODE: u8 = 0x02;
/// Comando ESTADO: estado do daemon (Tor, ligação, amizades, onion).
pub const CMD_ESTADO: u8 = 0x03;
/// Comando OUVIR: arranca o backend Tor e publica o hidden service.
pub const CMD_OUVIR: u8 = 0x04;
/// Comando LIGAR: abre a ligação P2P ao par (direto ou relay).
pub const CMD_LIGAR: u8 = 0x05;
/// Comando ENVIAR: envia um envelope como frame `CHAT`.
pub const CMD_ENVIAR: u8 = 0x06;
/// Comando RECEBER: espera pelo próximo frame P2P.
pub const CMD_RECEBER: u8 = 0x07;
/// Comando FECHAR: fecha a ligação P2P (idempotente).
pub const CMD_FECHAR: u8 = 0x08;
/// Comando PEDIR_AMIZADE: monta e envia `FRIEND_REQUEST`.
pub const CMD_PEDIR_AMIZADE: u8 = 0x09;
/// Comando ACEITAR_AMIZADE: valida um pedido e envia `FRIEND_ACCEPT`.
pub const CMD_ACEITAR_AMIZADE: u8 = 0x0A;
/// Comando RECUSAR_AMIZADE: monta e envia `FRIEND_REJECT`.
pub const CMD_RECUSAR_AMIZADE: u8 = 0x0B;
/// Comando CONFIRMAR_AMIZADE: valida o `FRIEND_ACCEPT` e fecha amizade.
pub const CMD_CONFIRMAR_AMIZADE: u8 = 0x0C;

/// Estado de resposta: sucesso.
pub const ESTADO_OK: u8 = 0x00;
/// Estado de resposta: erro estruturado.
pub const ESTADO_ERRO: u8 = 0x01;

// Códigos de erro (docs/ipc_spec.md).
/// Comando fora da tabela de comandos.
pub const ERR_COMANDO_DESCONHECIDO: u8 = 0x01;
/// Corpo curto demais para o comando.
pub const ERR_PAYLOAD_MALFORMADO: u8 = 0x02;
/// Tamanho/campo de chave inválido.
pub const ERR_CHAVE_INVALIDA: u8 = 0x03;
/// Assinatura Ed25519 rejeitada (DECODE/handshake).
pub const ERR_ASSINATURA_INVALIDA: u8 = 0x04;
/// Tag AEAD inválida (DECODE).
pub const ERR_DECIFRAGEM_FALHOU: u8 = 0x05;
/// Versão/comprimento do envelope errados.
pub const ERR_ENVELOPE_INVALIDO: u8 = 0x06;
/// Falha interna AEAD (ENCODE).
pub const ERR_CIFRAGEM_FALHOU: u8 = 0x07;
/// Plaintext/corpo não é UTF-8 válido.
pub const ERR_TEXTO_INVALIDO_UTF8: u8 = 0x08;
/// Comprimento acima de MAX_PAYLOAD.
pub const ERR_PAYLOAD_GRANDE_DEMAIS: u8 = 0x09;
/// Operação de rede sem ligação ativa.
pub const ERR_NAO_LIGADO: u8 = 0x0A;
/// Backend Tor indisponível/falha no arranque.
pub const ERR_TOR_INDISPONIVEL: u8 = 0x0B;
/// Corpo/assinatura/nonce de handshake inválido.
pub const ERR_HANDSHAKE_INVALIDO: u8 = 0x0C;
/// `.onion`/ID/endereço malformado.
pub const ERR_DESTINO_INVALIDO: u8 = 0x0D;
/// `RECEBER` expirou sem frames.
pub const ERR_SEM_MENSAGEM: u8 = 0x0E;
/// Comando incoerente com o estado atual.
pub const ERR_ESTADO_INVALIDO: u8 = 0x0F;
/// Rate-limit de frames excedido (p2p::LIMITE_FRAMES).
pub const ERR_RATE_LIMIT: u8 = 0x10;
/// Falha reportada pelo relay/TURN configurado.
pub const ERR_RELAY: u8 = 0x11;
/// Versão do protocolo IPC incompatível (HELLO com versão ≠ 0x01).
pub const ERR_VERSAO_INCOMPATIVEL: u8 = 0x12;
/// `nonce1` de chat já visto no registo anti-replay (DECODE).
pub const ERR_NONCE_REPETIDO: u8 = 0x13;

/// Versão do protocolo IPC (`major`/`minor` em nibbles: high/low).
///
/// O `HELLO` carrega-a explicitamente; versões diferentes → `0x12`.
pub const VERSAO_IPC: u8 = 0x01;
/// Build do daemon devolvido no corpo `OK` do `HELLO` (UTF-8).
pub const BUILD: &str = env!("CARGO_PKG_VERSION");

/// Tamanho de cada chave/parte fixa dos pedidos (32 bytes).
pub const TAM_PARTE: usize = 32;
/// Corpo mínimo do ENCODE: k1‖k5‖k9‖seed (4×32) — mensagem pode ser vazia.
pub const MIN_CORPO_ENCODE: usize = 4 * TAM_PARTE;
/// Corpo mínimo do DECODE: k1‖k5‖k9‖pub (4×32) + envelope mínimo (117).
pub const MIN_CORPO_DECODE: usize = 4 * TAM_PARTE + TAM_MINIMO;

// ---------------------------------------------------------------------
// Caminhos de runtime
// ---------------------------------------------------------------------

/// Devolve o caminho do socket IPC.
///
/// Precedência: `$ONYXCHAT_SOCKET` (definido e não vazio) →
/// `/tmp/onyxchat-{uid}.sock` (só o utilizador do daemon liga).
pub fn caminho_socket() -> PathBuf {
    if let Ok(caminho) = std::env::var("ONYXCHAT_SOCKET") {
        if !caminho.is_empty() {
            return PathBuf::from(caminho);
        }
    }
    // `getuid` não tem efeitos laterais — chamada segura apesar do
    // `unsafe` exigido pela FFI de C.
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/onyxchat-{uid}.sock"))
}

/// Directório de estado do daemon.
///
/// Não é `/tmp`. O registo anti-replay diz quais mensagens **já foram
/// vistas** — é metainformação sobre o tráfego do utilizador, e `/tmp` é
/// legível e substituível por qualquer conta da máquina. Um atacante
/// local que consiga escrever um registo completo antes do arranque
/// reintroduz o replay que estamos a fechar.
///
/// Precedência: `$ONYXCHAT_ESTADO` → `$XDG_STATE_HOME/onyxchat` →
/// `~/.local/state/onyxchat`. A segunda e a terceira seguem a
/// especificação XDG e o mesmo caminho que o cliente Python usa para os
/// seus dados (`user/config.py`, `~/.local/share/onyxchat`).
pub fn caminho_estado() -> PathBuf {
    if let Ok(c) = std::env::var("ONYXCHAT_ESTADO") {
        if !c.is_empty() {
            return PathBuf::from(c);
        }
    }
    if let Ok(x) = std::env::var("XDG_STATE_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x).join("onyxchat");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local").join("state").join("onyxchat")
}

/// Ficheiro do registo anti-replay de `nonce1`.
pub fn caminho_anti_replay() -> PathBuf {
    caminho_estado().join("anti-replay-v1.bin")
}

// ---------------------------------------------------------------------
// Respostas
// ---------------------------------------------------------------------

/// Resposta `OK` com corpo arbitrário (envelope ou plaintext).
fn resposta_ok(corpo: &[u8]) -> Vec<u8> {
    let mut resp = Vec::with_capacity(1 + corpo.len());
    resp.push(ESTADO_OK);
    resp.extend_from_slice(corpo);
    resp
}

/// Resposta `ERRO` com código + mensagem legível UTF-8 (sem segredos).
fn resposta_erro(codigo: u8, mensagem: &str) -> Vec<u8> {
    let mut resp = Vec::with_capacity(2 + mensagem.len());
    resp.push(ESTADO_ERRO);
    resp.push(codigo);
    resp.extend_from_slice(mensagem.as_bytes());
    resp
}

/// Converte um erro de pipeline no par (código IPC, mensagem legível).
fn traduzir_erro(erro: &ErroPipeline, em_cifragem: bool) -> (u8, String) {
    (erro.para_ipc(em_cifragem), erro.to_string())
}

/// Resultado de pipeline → resposta IPC (`OK`+corpo ou `ERRO`+código).
///
/// Função pura partilhada por ENCODE e DECODE: o ramo `Err` é
/// diretamente testável mesmo quando o pipeline não falha em condições
/// normais (defensiva de produção, coberta por teste unitário).
fn resposta_de_pipeline(resultado: Result<Vec<u8>, ErroPipeline>, em_cifragem: bool) -> Vec<u8> {
    match resultado {
        Ok(dados) => resposta_ok(&dados),
        Err(erro) => {
            let (codigo, texto) = traduzir_erro(&erro, em_cifragem);
            resposta_erro(codigo, &texto)
        }
    }
}

// ---------------------------------------------------------------------
// Processamento de pedidos (função pura — testável sem sockets)
// ---------------------------------------------------------------------

/// Processa o corpo completo de um pedido e devolve a resposta.
///
/// Nunca paniqueia: qualquer corpo inválido vira `ERRO` com o código
/// documentado; o daemon só regista metadados (comando/tamanho).
///
/// `ENCODE`/`DECODE` são puros (não tocam no estado); os comandos de
/// rede/handshake (`0x03..0x0C`) operam sobre o `Estado` partilhado.
///
/// Pressupõe **sessão já aberta** (o `HELLO` obrigatório é tratado em
/// [`processar_sessao`], chamada por [`atender`]); o `HELLO` repetido é
/// idempotente e revalida a versão. Para testar o portão da sessão use
/// [`processar_sessao`].
pub fn processar_pedido(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    let Some(&comando) = corpo.first() else {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "corpo vazio");
    };
    let resto = &corpo[1..];
    match comando {
        CMD_HELLO => processar_hello(resto),
        CMD_ENCODE => processar_encode(resto),
        CMD_DECODE => processar_decode(estado, resto),
        CMD_ESTADO => processar_estado(estado, resto),
        CMD_OUVIR => processar_ouvir(estado, resto),
        CMD_LIGAR => processar_ligar(estado, resto),
        CMD_ENVIAR => processar_enviar(estado, resto),
        CMD_RECEBER => processar_receber(estado, resto),
        CMD_FECHAR => processar_fechar(estado, resto),
        CMD_PEDIR_AMIZADE => processar_pedir_amizade(estado, resto),
        CMD_ACEITAR_AMIZADE => processar_aceitar_amizade(estado, resto),
        CMD_RECUSAR_AMIZADE => processar_recusar_amizade(estado, resto),
        CMD_CONFIRMAR_AMIZADE => processar_confirmar_amizade(estado, resto),
        _ => resposta_erro(ERR_COMANDO_DESCONHECIDO, "comando desconhecido"),
    }
}

/// `HELLO` — corpo `versao(1)`; responde `OK ‖ versao ‖ build`.
///
/// `HELLO 0x00` → `versao:1B`; resposta `OK` = `versao:1B ‖ build`.
///
/// # Tolerância de versões
///
/// A versão é `major` no nibble alto e `minor` no baixo. A regra
/// (norma em `docs/index.md` §Princípio da tolerância de versões):
///
/// ```text
/// major igual   + minor diferente  →  ACEITA (compatibilidade)
/// major diferente                  →  0x12 (incompatível)
/// ```
///
/// Isto substitui a comparação por igualdade, que tratava `0x01` e
/// `0x02` como igualmente incompatíveis. A consequência prática: um
/// cliente com a versão local mais recente continua a falar com um
/// daemon com a anterior, e vice-versa. É o que permite a dois
/// utilizadores em versões diferentes do software usar o mesmo sistema.
///
/// Uma alteração de `minor` **tem de continuar compatível**: só se
/// acrescenta, nunca se muda a semântica de nada que já existia. É por
/// isso que o nibble `minor` existe — dá espaço para evoluir sem
/// partir o mundo.
///
/// Uma alteração de `major` é incompatível por definição, e é o que
/// justifica o `0x12`.
fn processar_hello(resto: &[u8]) -> Vec<u8> {
    if resto.len() != 1 {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "HELLO: esperados 2 bytes");
    }
    if !versao_compativel(resto[0], VERSAO_IPC) {
        return resposta_erro(
            ERR_VERSAO_INCOMPATIVEL,
            &format!(
                "versão do protocolo incompatível: cliente 0x{:02x}, daemon 0x{:02x} \
                 (major diferente)",
                resto[0], VERSAO_IPC
            ),
        );
    }
    // A resposta devolve a versão **do cliente**, não a do daemon: quem
    // está a ler a resposta precisa de confirmar que a *sua* versão foi
    // aceite, e não apenas saber qual é a daqui.
    let mut corpo = Vec::with_capacity(1 + BUILD.len());
    corpo.push(resto[0]);
    corpo.extend_from_slice(BUILD.as_bytes());
    resposta_ok(&corpo)
}

/// `true` se duas versões `major.minor` são compatíveis.
///
/// Compatível = **o `major` é igual**. O `minor` pode diferir: só o
/// `major` é um contrato de incompatibilidade.
///
/// Esta função é a norma executável da tolerância de versões, e é
/// testada directamente — é a decisão que decide se um utilizador com
/// software antigo consegue trabalhar com software novo.
fn versao_compativel(versao: u8, suporte: u8) -> bool {
    (versao >> 4) == (suporte >> 4)
}

/// Versão local, para testes e para mensagens.
#[cfg(test)]
fn major(v: u8) -> u8 {
    v >> 4
}

/// Versão local, para testes e para mensagens.
#[cfg(test)]
fn minor(v: u8) -> u8 {
    v & 0x0F
}

/// Portão de sessão por ligação: `HELLO` obrigatório antes de tudo.
///
/// * corpo vazio → `0x02` (regra de enquadramento, antes do portão);
/// * `HELLO` válido → abre a sessão (`*hello_feito = true`);
/// * `HELLO` inválido → `0x02`/`0x12` e a sessão **não** abre — o
///   próximo comando continua a ser rejeitado com `0x02`;
/// * qualquer outro comando sem sessão → `0x02 PayloadMalformado`;
/// * `HELLO` repetido com sessão aberta → idempotente (revalida).
///
/// Chamada por [`atender`]; os testes de protocolo puro usam
/// [`processar_pedido`] como sessão já aberta.
pub fn processar_sessao(estado: &Estado, corpo: &[u8], hello_feito: &mut bool) -> Vec<u8> {
    let Some(&comando) = corpo.first() else {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "corpo vazio");
    };
    if comando == CMD_HELLO {
        let resposta = processar_hello(&corpo[1..]);
        *hello_feito = resposta[0] == ESTADO_OK;
        return resposta;
    }
    if !*hello_feito {
        return resposta_erro(
            ERR_PAYLOAD_MALFORMADO,
            "HELLO obrigatório antes de qualquer comando",
        );
    }
    processar_pedido(estado, corpo)
}

/// As quatro partes fixas de 32 B de um pedido: k1, k5, k9, seed/pub.
type PartesFixas<'a> = (&'a [u8], &'a [u8], &'a [u8], &'a [u8]);

/// Extrai as quatro partes fixas (32 B) de um corpo de pedido.
///
/// Devolve `None` se o corpo for curto demais → `PayloadMalformado`.
fn partes_fixas(pedido: &[u8]) -> Option<PartesFixas<'_>> {
    if pedido.len() < MIN_CORPO_ENCODE {
        return None;
    }
    Some((
        &pedido[0..32],
        &pedido[32..64],
        &pedido[64..96],
        &pedido[96..128],
    ))
}

/// `ENCODE` — valida corpo/UTF-8 e cifra o pipeline completo.
fn processar_encode(pedido: &[u8]) -> Vec<u8> {
    let Some((k1, k5, k9, seed)) = partes_fixas(pedido) else {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "ENCODE: corpo curto");
    };
    let mensagem = &pedido[MIN_CORPO_ENCODE..];
    // Limite ANTES de UTF-8 e de cifrar: rejeição barata e cedo
    // (`docs/ipc_spec.md` §Validação — campo acima do limite → 0x09).
    if mensagem.len() > crate::envelope::MAX_PLAINTEXT {
        return resposta_erro(ERR_PAYLOAD_GRANDE_DEMAIS, "mensagem acima de MAX_PLAINTEXT");
    }
    if std::str::from_utf8(mensagem).is_err() {
        // Nunca registar o conteúdo — apenas o facto de não ser UTF-8.
        return resposta_erro(ERR_TEXTO_INVALIDO_UTF8, "mensagem não é UTF-8 válido");
    }
    let chaves = Chaves {
        k1: k1.try_into().expect("32B validados"),
        k5: k5.try_into().expect("32B validados"),
        k9: k9.try_into().expect("32B validados"),
    };
    // Seed privada em `Segredo`: zeroiza no descarte, em todos os
    // ramos (Resumo §22 — `docs/key_management.md`).
    let seed = Segredo::novo(seed.try_into().expect("32B validados"));
    resposta_de_pipeline(pipeline::cifrar(mensagem, &chaves, seed.como_bytes()), true)
}

/// `DECODE` — valida corpo e envelope, verifica assinatura e decifra.
///
/// O registo anti-replay de `nonce1` é do `Estado` partilhado e fica
/// bloqueado durante a decifragem: a janela *check-and-insert* tem de
/// ser atómica para que dois reenvios simultâneos não passem ambos.
fn processar_decode(estado: &Estado, pedido: &[u8]) -> Vec<u8> {
    // Comprimento mínimo do corpo: chaves (4×32) + envelope mínimo
    // (117). Abaixo disto não há envelope parseável — `0x02` barato,
    // antes de tocar no pipeline (`MIN_CORPO_DECODE`).
    if pedido.len() < MIN_CORPO_DECODE {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "DECODE: corpo curto");
    }
    let Some((k1, k5, k9, publica)) = partes_fixas(pedido) else {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "DECODE: corpo curto");
    };
    // O limite máximo (MAX_ENVELOPE), a versão e o anti-replay são
    // validados dentro do pipeline (`PayloadGrande` → 0x09,
    // `EnvelopeInvalido` → 0x06, `NonceRepetido` → 0x13) — validação
    // única, sem duplicação.
    let envelope = &pedido[MIN_CORPO_ENCODE..];
    let chaves = Chaves {
        k1: k1.try_into().expect("32B validados"),
        k5: k5.try_into().expect("32B validados"),
        k9: k9.try_into().expect("32B validados"),
    };
    let publica: [u8; 32] = publica.try_into().expect("32B validados");
    let resultado = {
        let mut registo = bloquear(&estado.nonces1);
        let r = pipeline::decifrar(envelope, &chaves, &publica, &mut registo)
            .map(|t| t.into_bytes());
        // Persiste **depois** da decifragem e ainda dentro do mesmo
        // lock, por duas razões:
        //
        //   * o `registo` reflecte o resultado real, incluindo a
        //     `libertar()` que o pipeline faz quando a decifragem
        //     falha por chaves erradas — gravar antes escreveria uma
        //     reserva que o próprio pedido libertou a seguir;
        //   * nenhum outro `DECODE` pode correr entre a alteração em
        //     memória e a escrita em disco, portanto o ficheiro nunca
        //     fica com um estado que o processo já não tem.
        registo.gravar_para(&caminho_anti_replay());
        r
    };
    resposta_de_pipeline(resultado, false)
}

// ---------------------------------------------------------------------
// Estado do daemon (Etapa 7) — partilhado por todas as ligações IPC
// ---------------------------------------------------------------------

/// Pedido de handshake enviado e ainda por confirmar (`PEDIR_AMIZADE`).
#[derive(Debug, Clone)]
struct PedidoPendente {
    /// Identidade esperada do par (dada pelo cliente).
    publica: [u8; TAM_PUB],
    /// Corpo `FRIEND_REQUEST` enviado (necessário ao `CONFIRMAR`).
    corpo: Vec<u8>,
}

impl PedidoPendente {
    /// Apaga o corpo que guarda K1/K5/K9 em claro (209 B).
    ///
    /// Separado do `Drop` para ser testável (o compilador proíbe
    /// chamar `Drop::drop` explicitamente); o `Drop` delega aqui.
    fn limpar(&mut self) {
        self.corpo.zeroize();
    }
}

impl Drop for PedidoPendente {
    /// Delega em [`PedidoPendente::limpar`] no descarte (substituição,
    /// confirmação ou fim do processo; Resumo §22).
    fn drop(&mut self) {
        self.limpar();
    }
}

/// Sessão de rede: backend Tor, escuta P2P, ligação e handshake.
struct Rede {
    /// Backend Tor (real ou falso) que resolve/publica endereços.
    backend: Box<dyn BackendTor>,
    /// `true` depois de um `OUVIR` bem-sucedido (escuta publicada).
    a_escuta: bool,
    /// Endpoint do relay por omissão (`$ONYXCHAT_RELAY`).
    endpoint_padrao: Option<String>,
    /// Ligação P2P ativa ao par (direta ou via relay).
    ligacao: Option<LigacaoP2P>,
    /// Último `FRIEND_REQUEST` enviado, por confirmar.
    pedido: Option<PedidoPendente>,
}

/// Estado completo do daemon visto pelo protocolo IPC.
///
/// Os espelhos (`tor`, `ligado`, `onion`) são atualizados sempre que a
/// `rede` muda, para que `ESTADO` responda mesmo enquanto outra thread
/// espera um frame em `RECEBER`.
pub struct Estado {
    /// Sessão de rede (bloqueio de ordem 1 — nunca invertido).
    rede: Arc<Mutex<Rede>>,
    /// Identidades dos amigos + memória de nonces (bloqueio de ordem 2).
    ///
    /// Sem chaves, desde G2: o registo responde a «quantos amigos há?» e
    /// «esta identidade é amiga?» — e a mais nada. Ver
    /// [`RegistoAmigos`](crate::handshake::RegistoAmigos).
    amigos: Mutex<RegistoAmigos>,
    /// Espelho de `backend.estado()` para `ESTADO` (0x00/0x01/0x02).
    tor: AtomicU8,
    /// Espelho de "existe ligação ativa" para `ESTADO`.
    ligado: Arc<AtomicBool>,
    /// Espelho do `.onion` publicado (vazio = não escuta).
    onion: Mutex<Option<String>>,
    /// Registo anti-replay de `nonce1` do chat (bloqueio de ordem 3).
    ///
    /// É o registo do *receptor* (`message_format.md` §Anti-replay);
    /// segura-se durante a decifragem de um `DECODE` para que a janela
    /// *check-and-insert* seja atómica. Serializar as decifragens é
    /// irrelevante num daemon local (uma mensagem por mensagem).
    nonces1: Mutex<RegistoNonce1>,
}

/// Bloqueia um mutex do estado (o daemon nunca paniqueia com ele
/// detido, por isso o envenenamento é um invariante defensivo).
fn bloquear<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("mutex do estado nunca envenenado")
}

impl Estado {
    /// Estado novo com o `backend` indicado (TorFalso nos testes).
    pub fn novo(backend: Box<dyn BackendTor>) -> Self {
        Estado {
            rede: Arc::new(Mutex::new(Rede {
                backend,
                a_escuta: false,
                endpoint_padrao: std::env::var("ONYXCHAT_RELAY")
                    .ok()
                    .filter(|e| !e.is_empty()),
                ligacao: None,
                pedido: None,
            })),
            amigos: Mutex::new(RegistoAmigos::novo()),
            tor: AtomicU8::new(EstadoTor::Parado.para_ipc()),
            ligado: Arc::new(AtomicBool::new(false)),
            onion: Mutex::new(None),
            // Carrega de disco em vez de nascer vazio. Um registo que
            // nascesse vazio tornaria o anti-replay inútil durante toda a
            // vida do processo — e um reinício é precisamente a janela
            // em que um envelope capturado volta a passar.
            nonces1: Mutex::new(RegistoNonce1::carregar_de(&caminho_anti_replay())),
        }
    }

    /// Estado Tor reportado por `ESTADO` (código IPC 0/1/2).
    pub fn tor(&self) -> u8 {
        self.tor.load(Ordering::Relaxed)
    }

    /// `true` se existir ligação P2P ativa.
    pub fn ligado(&self) -> bool {
        self.ligado.load(Ordering::Relaxed)
    }

    /// `.onion` publicado (se o backend estiver a escuta).
    pub fn onion(&self) -> Option<String> {
        bloquear(&self.onion).clone()
    }

    /// Número de amizades registadas.
    pub fn amigos(&self) -> usize {
        bloquear(&self.amigos).quantidade()
    }

    /// Publica o hidden service encaminhando entradas para o servidor
    /// P2P local e arranca a thread que aceita os pares.
    ///
    /// Idempotente: repetir `OUVIR` devolve o mesmo endereço.
    fn ouvir(&self) -> Result<String, (u8, String)> {
        self.ouvir_em("127.0.0.1", 0)
    }

    /// `ouvir` vinculado a um endpoint TCP concreto (o `bind` pode
    /// falhar — endereço ocupado, sem rotas, sem ficheiros).
    fn ouvir_em(&self, endereco: &str, porta: u16) -> Result<String, (u8, String)> {
        let mut rede = bloquear(&self.rede);
        if rede.a_escuta {
            return Ok(bloquear(&self.onion).clone().unwrap_or_default());
        }
        let servidor =
            ServidorP2P::abrir(endereco, porta).map_err(|e| (e.para_ipc(), e.to_string()))?;
        let porta = servidor.porta();
        let onion = rede
            .backend
            .arrancar(porta)
            .map_err(|e| (e.para_ipc(), e.to_string()))?;
        rede.a_escuta = true;
        let estado_tor = rede.backend.estado().para_ipc();
        drop(rede);

        self.tor.store(estado_tor, Ordering::Relaxed);
        *bloquear(&self.onion) = Some(onion.clone());
        self.iniciar_aceitacao(servidor);
        Ok(onion)
    }

    /// Thread que aceita as ligações de entrada do hidden service.
    ///
    /// A mais recente substitui a ligação corrente (o par reconectou);
    /// o `TcpListener` vive na thread — o bloqueio da `rede` nunca é
    /// mantido durante um `accept` bloqueante.
    fn iniciar_aceitacao(&self, servidor: ServidorP2P) {
        let rede = Arc::clone(&self.rede);
        let ligado = Arc::clone(&self.ligado);
        thread::spawn(move || {
            // O ciclo termina quando o listener morre (nunca esperado:
            // a thread aceitante desaparece sem derrubar o daemon).
            while let Ok(aceite) = servidor.aceitar() {
                let mut rede_bloqueada = bloquear(&rede);
                rede_bloqueada.ligacao = Some(LigacaoP2P::Direta(Box::new(aceite)));
                // Sob o mesmo bloqueio: qualquer `RECEBER` que use esta
                // ligação vê também o `ligado` já actualizado.
                ligado.store(true, Ordering::Relaxed);
            }
        });
    }

    /// Limpa a ligação se o erro indicar que o stream já não serve.
    ///
    /// * `na_rececao = false` (envio): o frame nunca chegou a ser
    ///   escrito — o stream continua íntegro;
    /// * `na_rececao = true`: um comprimento acima do limite deixa o
    ///   stream **desalinhado** (o corpo anunciado nunca é lido, e
    ///   lê-lo seria vetor de memória) — a ligação morre, tal como no
    ///   relay (`relay.md` §Transporte e enquadramento). O código
    ///   continua a ser `0x09 PayloadGrandeDemais`.
    fn limpar_se_morta(erro: &ErroP2P, na_rececao: bool) -> bool {
        matches!(
            erro,
            ErroP2P::Fechada | ErroP2P::Io(_) | ErroP2P::Desalinhado
        ) || (na_rececao && matches!(erro, ErroP2P::PayloadGrande { .. }))
    }
}

// ---------------------------------------------------------------------
// Comandos de rede (docs/ipc_spec.md §Comandos de rede)
// ---------------------------------------------------------------------

/// `ESTADO 0x03` → `tor:1B ‖ ligado:1B ‖ amigos:u16 LE ‖ onion UTF-8`.
fn processar_estado(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    if !corpo.is_empty() {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "ESTADO: corpo tem de ser vazio");
    }
    let amigos = u16::try_from(estado.amigos()).unwrap_or(u16::MAX);
    let onion = estado.onion().unwrap_or_default();
    let mut resposta = Vec::with_capacity(4 + onion.len());
    resposta.push(estado.tor());
    resposta.push(u8::from(estado.ligado()));
    resposta.extend_from_slice(&amigos.to_le_bytes());
    resposta.extend_from_slice(onion.as_bytes());
    resposta_ok(&resposta)
}

/// `OUVIR 0x04` → publica o hidden service e devolve o `.onion`.
fn processar_ouvir(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    if !corpo.is_empty() {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "OUVIR: corpo tem de ser vazio");
    }
    match estado.ouvir() {
        Ok(onion) => resposta_ok(onion.as_bytes()),
        Err((codigo, mensagem)) => resposta_erro(codigo, &mensagem),
    }
}

/// Extrai uma cadeia UTF-8 com comprimento `u16 LE` à frente.
///
/// Corpo curto, comprimento exagerado ou UTF-8 inválido → `None`
/// (o chamador responde `PayloadMalformado`).
fn tirar_cadeia(dados: &[u8]) -> Option<(String, &[u8])> {
    let (&baixo, resto) = dados.split_first()?;
    let (&alto, resto) = resto.split_first()?;
    let comprimento = usize::from(u16::from_le_bytes([baixo, alto]));
    if resto.len() < comprimento {
        return None;
    }
    let (cadeia, resto) = resto.split_at(comprimento);
    let cadeia = std::str::from_utf8(cadeia).ok()?.to_string();
    Some((cadeia, resto))
}

/// `LIGAR 0x05` → `modo:1B ‖ len_endpoint ‖ endpoint ‖ len_destino ‖
/// destino ‖ pub_propria(32) ‖ pub_par(32)`.
fn processar_ligar(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    let Some((&codigo, resto)) = corpo.split_first() else {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "LIGAR: corpo curto");
    };
    let modo = match ModoLigacao::de_ipc(codigo) {
        Ok(modo) => modo,
        Err(erro) => return resposta_erro(erro.para_ipc(), &erro.to_string()),
    };
    let Some((endpoint, resto)) = tirar_cadeia(resto) else {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "LIGAR: endpoint malformado");
    };
    let Some((destino, resto)) = tirar_cadeia(resto) else {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "LIGAR: destino malformado");
    };
    if resto.len() != 2 * TAM_PARTE {
        return resposta_erro(
            ERR_PAYLOAD_MALFORMADO,
            "LIGAR: faltam as chaves públicas (64 bytes)",
        );
    }
    let pub_propria: [u8; 32] = resto[..32].try_into().expect("32B validados");
    let pub_par: [u8; 32] = resto[32..].try_into().expect("32B validados");

    // O destino tem de ser coerente com o modo, e a coerência é
    // verificada **antes** de abrir qualquer socket: um pedido mal
    // formado não pode custar uma ligação de rede.
    let destino_para_o_modo = match validar_destino(modo, &destino, &pub_par) {
        Ok(ok) => ok,
        Err(codigo) => return resposta_erro(codigo.0, codigo.1),
    };

    let mut rede = bloquear(&estado.rede);
    if rede.ligacao.is_some() {
        return resposta_erro(
            ERR_ESTADO_INVALIDO,
            "já existe ligação ativa — use FECHAR primeiro",
        );
    }
    let pedido = PedidoLigacao {
        modo,
        destino: destino_para_o_modo,
        endpoint_relay: if endpoint.is_empty() {
            rede.endpoint_padrao.clone()
        } else {
            Some(endpoint)
        },
        caixa_propria: p2p::caixa_de_pub(&pub_propria),
    };
    let caixa_par = p2p::caixa_de_pub(&pub_par);
    match p2p::abrir_ligacao(&mut *rede.backend, &pedido, caixa_par) {
        Ok(ligacao) => {
            rede.ligacao = Some(ligacao);
            drop(rede);
            estado.ligado.store(true, Ordering::Relaxed);
            resposta_ok(&[])
        }
        Err(erro) => resposta_erro(erro.para_ipc(), &erro.to_string()),
    }
}

/// Erro de validação de `destino`: código IPC e mensagem pronta.
///
/// Tupla em vez de `ErroP2P` porque `DestinoInvalido` aqui não vem da
/// camada P2P — vem da validação do pedido, antes de tocar na rede.
type ErroDestino = (u8, &'static str);

/// Valida `destino` contra o `modo` escolhido e a `pub_par` do corpo.
///
/// Devolve o `destino` a usar. Em modo `relay` o valor é **sempre**
/// canónico (a pública em hex minúsculo), para que não haja duas formas
/// de descrever o mesmo par dentro do processo.
///
/// A regra (normativa em `docs/ipc_spec.md` §`destino`):
///
/// * **directo** — exige `xxxx.onion`. O daemon **não** tem cliente de
///   discovery; resolver um ID hex para um `.onion` é responsabilidade
///   do cliente Python, antes de emitir `LIGAR`. Um ID hex aqui é
///   `0x0D`, não uma tentativa de resolver.
/// * **relay** — aceita `xxxx.onion` **ou** o ID hex da `pub_par`. A
///   mailbox é derivada da `pub_par` do corpo; o ID hex é redundante por
///   construção e a validação existe para detectar um pedido
///   incoerente (um `destino` que aponte para outro par), que de outro
///   modo entregaria a mailbox errada sem qualquer erro.
fn validar_destino(
    modo: ModoLigacao,
    destino: &str,
    pub_par: &[u8; 32],
) -> Result<String, ErroDestino> {
    // Reconhece um endereço `.onion` **completo**: 56 caracteres de
    // base32 (a-z, 2-7) seguidos de ".onion".
    //
    // A validação é deliberadamente estrita. Um `.onion` sem etiqueta, ou
    // com caracteres fora do base32, não é um endereço — e aceitar
    // "qualquer coisa que acabe em .onion" transformaria uma validação
    // em decorado. Em modo relay o valor é ignorado, mas um `destino`
    // malformado que passa a validação é um pedido incoerente que
    // ninguém notou.
    let parece_onion = {
        let etiqueta = destino.strip_suffix(".onion");
        match etiqueta {
            Some(etiqueta) => {
                etiqueta.len() == 56
                    && etiqueta.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() && b != b'0' && b != b'1'
                    })
            }
            None => false,
        }
    };
    let hex_da_pub = p2p::hex_de_pub(pub_par);

    match modo {
        ModoLigacao::Direto => {
            if destino.is_empty() {
                return Err((
                    ERR_DESTINO_INVALIDO,
                    "modo direto exige destino (.onion)",
                ));
            }
            if !parece_onion {
                // Não tentamos resolver: o daemon não tem discovery.
                // A mensagem diz isso, para o utilizador saber que a
                // correcção é resolver no cliente e não no daemon.
                return Err((
                    ERR_DESTINO_INVALIDO,
                    "modo directo exige um endereço .onion (resolva o ID \
                     no cliente, via discovery, e passe o .onion)",
                ));
            }
            Ok(destino.to_string())
        }
        ModoLigacao::Relay => {
            // Vazio é legítimo: significa «a mailbox da pub do corpo».
            if destino.is_empty() {
                return Ok(String::new());
            }
            if parece_onion {
                // Aceito mas normalizado: a mailbox vem sempre do
                // corpo, nunca do `destino`.
                return Ok(destino.to_string());
            }
            match p2p::pub_de_hex(destino) {
                Some(publica) if &publica == pub_par => Ok(hex_da_pub),
                Some(_) => Err((
                    ERR_DESTINO_INVALIDO,
                    "modo relay: o ID hex de destino não corresponde à \
                     pública do par informada no corpo",
                )),
                None => Err((
                    ERR_DESTINO_INVALIDO,
                    "modo relay: destino inválido (esperado .onion ou ID \
                     hex de 64 caracteres da pública do par)",
                )),
            }
        }
    }
}

/// `ENVIAR 0x06` → frame `CHAT 0x01` com o envelope por corpo.
fn processar_enviar(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    let mut rede = bloquear(&estado.rede);
    let Some(ligacao) = rede.ligacao.as_mut() else {
        return resposta_erro(ERR_NAO_LIGADO, "sem ligação ativa ao par");
    };
    match ligacao.enviar(p2p::FRAME_CHAT, corpo) {
        Ok(()) => resposta_ok(&[]),
        Err(erro) => {
            let morta = Estado::limpar_se_morta(&erro, false);
            if morta {
                rede.ligacao = None;
            }
            drop(rede);
            if morta {
                estado.ligado.store(false, Ordering::Relaxed);
            }
            resposta_erro(erro.para_ipc(), &erro.to_string())
        }
    }
}

/// `RECEBER 0x07` → `timeout_ms:u32 LE` → `tipo:1B ‖ corpo` do frame.
///
/// A espera é feita em fatias (`p2p::FATIA_ESPERA`): entre frames o
/// bloqueio da `rede` é libertado, permitindo `ENVIAR`/`FECHAR`/
/// `ESTADO` em paralelo com uma receção longa.
fn processar_receber(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    let Ok([b0, b1, b2, b3]) = <[u8; 4]>::try_from(corpo) else {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "RECEBER: timeout_ms em falta");
    };
    let timeout_ms = u32::from_le_bytes([b0, b1, b2, b3]);
    let prazo =
        (timeout_ms != 0).then(|| Instant::now() + Duration::from_millis(u64::from(timeout_ms)));

    loop {
        let restante = prazo.map(|prazo| prazo.saturating_duration_since(Instant::now()));
        if restante == Some(Duration::ZERO) {
            return resposta_erro(ERR_SEM_MENSAGEM, "nenhum frame dentro do tempo");
        }
        // Piso: uma espera a meio de frame nunca expira com o prazo
        // do cliente (mataria o stream com `Desalinhado`).
        let espera_frame = restante;
        let resultado = {
            let mut rede = bloquear(&estado.rede);
            match rede.ligacao.as_mut() {
                None => return resposta_erro(ERR_NAO_LIGADO, "sem ligação ativa ao par"),
                Some(ligacao) => ligacao.receber_por_lotes(espera_frame, p2p::FATIA_ESPERA),
            }
        };
        match resultado {
            Ok(frame) => {
                let mut resposta = Vec::with_capacity(1 + frame.corpo.len());
                resposta.push(frame.tipo);
                resposta.extend_from_slice(&frame.corpo);
                return resposta_ok(&resposta);
            }
            // Nenhum byte: solta a `rede` e volta ao topo, onde o prazo
            // é reavaliado (`restante == ZERO` → 0x0E na linha 554).
            Err(ErroP2P::TempoEsgotado) => {}
            Err(erro) => {
                let morta = Estado::limpar_se_morta(&erro, true);
                if morta {
                    let mut rede = bloquear(&estado.rede);
                    rede.ligacao = None;
                    drop(rede);
                    estado.ligado.store(false, Ordering::Relaxed);
                }
                return resposta_erro(erro.para_ipc(), &erro.to_string());
            }
        }
    }
}

/// `FECHAR 0x08` → fecha a ligação P2P (idempotente, sempre `OK`).
fn processar_fechar(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    if !corpo.is_empty() {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "FECHAR: corpo tem de ser vazio");
    }
    let antiga = {
        let mut rede = bloquear(&estado.rede);
        rede.ligacao.take()
    };
    estado.ligado.store(false, Ordering::Relaxed);
    match antiga {
        Some(LigacaoP2P::Relay(relay)) => relay.fechar(),
        Some(LigacaoP2P::Direta(_)) | None => {}
    }
    resposta_ok(&[])
}

// ---------------------------------------------------------------------
// Comandos de handshake (docs/ipc_spec.md §Comandos de handshake)
// ---------------------------------------------------------------------

/// Serializa as chaves de uma amizade: `k1‖k5_p‖k9_p‖k5_par‖k9_par‖pub`.
fn corpo_da_amizade(amizade: &Amizade) -> [u8; 6 * TAM_PARTE] {
    let mut saida = [0u8; 6 * TAM_PARTE];
    saida[0..32].copy_from_slice(&amizade.k1);
    saida[32..64].copy_from_slice(&amizade.k5_proprio);
    saida[64..96].copy_from_slice(&amizade.k9_proprio);
    saida[96..128].copy_from_slice(&amizade.k5_par);
    saida[128..160].copy_from_slice(&amizade.k9_par);
    saida[160..192].copy_from_slice(&amizade.publica);
    saida
}

/// `PEDIR_AMIZADE 0x09` → `seed(32) ‖ pub_dest(32)`; envia o pedido
/// e devolve `k1‖k5‖k9 ‖ request_corpo(209)`.
fn processar_pedir_amizade(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    if corpo.len() != 2 * TAM_PARTE {
        return resposta_erro(ERR_PAYLOAD_MALFORMADO, "PEDIR_AMIZADE: esperados 64 bytes");
    }
    let seed = Segredo::novo(corpo[..32].try_into().expect("32B validados"));
    let pub_dest: [u8; TAM_PUB] = corpo[32..].try_into().expect("32B validados");

    let mut rede = bloquear(&estado.rede);
    let Some(ligacao) = rede.ligacao.as_mut() else {
        return resposta_erro(ERR_NAO_LIGADO, "sem ligação ativa ao par");
    };
    // `montar_pedido` é infallível (assinatura sobre corpo fixo).
    let (pedido, mut corpo_pedido) = handshake::montar_pedido(seed.como_bytes());
    // Resposta construída antes do commit — não depende do envio.
    let mut resposta = Vec::with_capacity(3 * TAM_PARTE + handshake::TAM_CORPO_PEDIDO);
    resposta.extend_from_slice(&pedido.k1);
    resposta.extend_from_slice(&pedido.k5);
    resposta.extend_from_slice(&pedido.k9);
    resposta.extend_from_slice(&corpo_pedido);
    let mut resposta = resposta_ok(&resposta);
    if let Err(erro) = ligacao.enviar(handshake::TIPO_PEDIDO, &corpo_pedido) {
        let morta = Estado::limpar_se_morta(&erro, false);
        if morta {
            rede.ligacao = None;
        }
        drop(rede);
        if morta {
            estado.ligado.store(false, Ordering::Relaxed);
        }
        // Nada foi guardado: apaga corpo e resposta (ambos com chaves).
        corpo_pedido.zeroize();
        resposta.zeroize();
        return resposta_erro(erro.para_ipc(), &erro.to_string());
    }
    rede.pedido = Some(PedidoPendente {
        publica: pub_dest,
        corpo: corpo_pedido,
    });
    drop(rede);
    resposta
}

/// `ACEITAR_AMIZADE 0x0A` → `seed(32) ‖ request_corpo(209)`; valida
/// assinatura + anti-replay, envia o aceite e devolve
/// `k1‖k5_b‖k9_b‖k5_a‖k9_a‖pub_a ‖ accept_corpo(177)`.
fn processar_aceitar_amizade(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    if corpo.len() != TAM_PARTE + handshake::TAM_CORPO_PEDIDO {
        // O número vem das constantes, não de um literal: 240 era um
        // literal desactualizado (o correcto é 32 + 209 = 241), e uma
        // mensagem de diagnóstico que diz o número errado faz o
        // utilizador acrescentar bytes ao pedido para a «resolver».
        let esperado = TAM_PARTE + handshake::TAM_CORPO_PEDIDO;
        return resposta_erro(
            ERR_PAYLOAD_MALFORMADO,
            &format!("ACEITAR_AMIZADE: esperados {esperado} bytes"),
        );
    }
    let seed = Segredo::novo(corpo[..32].try_into().expect("32B validados"));
    let corpo_pedido = &corpo[32..];

    // 1. Validação da mensagem (assinatura) — antes de qualquer chave.
    let pedido = match handshake::validar_pedido(corpo_pedido) {
        Ok(pedido) => pedido,
        Err(erro) => return resposta_erro(erro.para_ipc(), &erro.to_string()),
    };
    // 2. Anti-replay: nonce já visto → rejeita sem guardar nada.
    if bloquear(&estado.amigos).nonce_visto(&pedido.nonce) {
        return resposta_erro(ERR_HANDSHAKE_INVALIDO, "nonce de handshake repetido");
    }
    // 3. Precisamos de ligação para entregar o `FRIEND_ACCEPT`.
    let mut rede = bloquear(&estado.rede);
    let Some(ligacao) = rede.ligacao.as_mut() else {
        return resposta_erro(ERR_NAO_LIGADO, "sem ligação ativa ao par");
    };
    // 4. Aceite: infallível porque o pedido já foi validado no passo 1.
    let (amizade, mut corpo_aceite) = handshake::aceitar_pedido(seed.como_bytes(), &pedido);
    let envio = ligacao.enviar(handshake::TIPO_ACEITE, &corpo_aceite);
    if let Err(erro) = envio {
        let morta = Estado::limpar_se_morta(&erro, false);
        if morta {
            rede.ligacao = None;
        }
        drop(rede);
        if morta {
            estado.ligado.store(false, Ordering::Relaxed);
        }
        // Nada foi guardado: o corpo do aceite leva K5/K9 (§22).
        corpo_aceite.zeroize();
        return resposta_erro(erro.para_ipc(), &erro.to_string());
    }
    drop(rede);

    // 4. Commit: nonce anti-replay + amizade (com rotação de chaves).
    //
    // Só a identidade vai para o registo: as chaves saem do daemon na
    // resposta IPC, e o cliente é quem as guarda a partir daí. O
    // `Clone` desapareceu com isso — `publica` é `Copy`, e clonar uma
    // `Amizade` para a deitar fora era trabalho que não havia.
    {
        let mut amigos = bloquear(&estado.amigos);
        amigos.guardar_nonce(pedido.nonce);
        amigos.guardar(amizade.publica);
    }

    let mut resposta = Vec::with_capacity(6 * TAM_PARTE + handshake::TAM_CORPO_ACEITE);
    resposta.extend_from_slice(&corpo_da_amizade(&amizade));
    resposta.extend_from_slice(&corpo_aceite);
    corpo_aceite.zeroize();
    resposta_ok(&resposta)
}

/// `RECUSAR_AMIZADE 0x0B` → `seed(32) ‖ nonce(16)`; envia a recusa
/// assinada e devolve `reject_corpo(81)`.
fn processar_recusar_amizade(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    if corpo.len() != TAM_PARTE + TAM_NONCE {
        return resposta_erro(
            ERR_PAYLOAD_MALFORMADO,
            "RECUSAR_AMIZADE: esperados 48 bytes",
        );
    }
    let seed = Segredo::novo(corpo[..32].try_into().expect("32B validados"));
    let nonce: [u8; TAM_NONCE] = corpo[32..].try_into().expect("16B validados");

    let mut rede = bloquear(&estado.rede);
    let Some(ligacao) = rede.ligacao.as_mut() else {
        return resposta_erro(ERR_NAO_LIGADO, "sem ligação ativa ao par");
    };
    let corpo_recusa = handshake::montar_recusa(seed.como_bytes(), &nonce);
    if let Err(erro) = ligacao.enviar(handshake::TIPO_RECUSA, &corpo_recusa) {
        let morta = Estado::limpar_se_morta(&erro, false);
        if morta {
            rede.ligacao = None;
        }
        drop(rede);
        if morta {
            estado.ligado.store(false, Ordering::Relaxed);
        }
        return resposta_erro(erro.para_ipc(), &erro.to_string());
    }
    resposta_ok(&corpo_recusa)
}

/// `CONFIRMAR_AMIZADE 0x0C` → `seed(32) ‖ accept_corpo(177)`; valida
/// o aceite contra o pedido enviado e devolve
/// `k1‖k5_a‖k9_a‖k5_b‖k9_b‖pub_b` (192).
fn processar_confirmar_amizade(estado: &Estado, corpo: &[u8]) -> Vec<u8> {
    if corpo.len() != TAM_PARTE + handshake::TAM_CORPO_ACEITE {
        return resposta_erro(
            ERR_PAYLOAD_MALFORMADO,
            "CONFIRMAR_AMIZADE: esperados 209 bytes",
        );
    }
    let seed = Segredo::novo(corpo[..32].try_into().expect("32B validados"));
    let corpo_aceite = &corpo[32..];

    let pendente = {
        let rede = bloquear(&estado.rede);
        match &rede.pedido {
            Some(pendente) => pendente.clone(),
            None => {
                return resposta_erro(
                    ERR_ESTADO_INVALIDO,
                    "sem FRIEND_REQUEST pendente — use PEDIR_AMIZADE",
                )
            }
        }
    };
    let amizade =
        match handshake::confirmar_aceite(seed.como_bytes(), &pendente.corpo, corpo_aceite) {
            Ok(amizade) => amizade,
            Err(erro) => return resposta_erro(erro.para_ipc(), &erro.to_string()),
        };
    // O aceite tem de vir da identidade que o cliente indicou.
    if amizade.publica != pendente.publica {
        return resposta_erro(
            ERR_HANDSHAKE_INVALIDO,
            "aceite não corresponde à identidade esperada",
        );
    }
    {
        let mut rede = bloquear(&estado.rede);
        rede.pedido = None;
    }
    // Registar a identidade antes de serializar. Um `guardar` que
    // falhasse não há: só pode rodar uma amizade já existente, e
    // confirmar duas vezes a mesma amizade é o caso normal (o cliente
    // reenvia o `CONFIRMAR_AMIZADE` depois de um timeout) — inofensivo,
    // porque a contagem não muda e as chaves não voltam a ser geradas.
    bloquear(&estado.amigos).guardar(amizade.publica);
    resposta_ok(&corpo_da_amizade(&amizade))
}

// ---------------------------------------------------------------------
// Enquadramento + ciclo de atendimento
// ---------------------------------------------------------------------

/// Lê um pedido completo (`u32 LE` + payload) do stream.
///
/// EOF no início de um enquadramento (cliente fechou) ou payload acima
/// do limite devolve `Ok(None)` → o atendedor termina/erro.
///
/// O limite vem de [`limite_carga`], não de [`MAX_PAYLOAD`]: o valor
/// por omissão é uma constante de compilação, mas o utilizador pode
/// alterá-lo por ambiente, e um limite que o daemon não cumpre é uma
/// falsa promessa.
fn ler_pedido(fluxo: &mut std::os::unix::net::UnixStream) -> io::Result<Option<Vec<u8>>> {
    let mut cabecalho = [0u8; 4];
    fluxo.read_exact(&mut cabecalho)?;
    let comprimento = u32::from_le_bytes(cabecalho) as usize;

    // O comprimento é validado ANTES de qualquer alocação.
    //
    // A versão anterior testava o mesmo, mas o limite era 16 MiB — e o
    // `vec![0u8; comprimento]` seguinte reservava essa quantidade antes
    // de o cliente enviar um único byte. Como `thread::spawn` aceitava
    // ligações sem limite (ver [`servidor`]), um cliente do mesmo uid
    // podia abrir N ligações e reservar N × 16 MiB sem nunca escrever
    // nada. Bastava reenviar o cabeçalho de 4 bytes.
    //
    // Com o limite derivado de um orçamento de 1 GB, o pior caso por
    // ligação desceu para ~512 MB e o número de ligações deixou de
    // ser ilimitado. A ordem — validar, depois alocar — é o que fecha
    // o resto: um limite correcto não protege se a alocação acontece
    // antes da validação.
    if comprimento > limite_carga() {
        return Ok(None);
    }

    let mut corpo = vec![0u8; comprimento];
    fluxo.read_exact(&mut corpo)?;
    Ok(Some(corpo))
}

/// Escreve uma resposta com o enquadramento por comprimento.
fn escrever_resposta(fluxo: &mut std::os::unix::net::UnixStream, corpo: &[u8]) -> io::Result<()> {
    // Toda a resposta cabe em `u32` (pedido ≤ 16 MiB + margem fixa).
    let comprimento = u32::try_from(corpo.len()).expect("resposta < 4 GiB");
    fluxo.write_all(&comprimento.to_le_bytes())?;
    fluxo.write_all(corpo)?;
    fluxo.flush()
}

/// Atende uma ligação até EOF: vários pedidos síncronos em sequência.
///
/// O **primeiro** pedido de cada ligação tem de ser um `HELLO` válido
/// ([`processar_sessao`]); pedidos anteriores recebem `0x02`.
///
/// Erros de I/O terminam a ligação silenciosamente (o cliente também
/// desapareceu); o daemon nunca aborta por dados do cliente.
pub fn atender(mut fluxo: std::os::unix::net::UnixStream, estado: Arc<Estado>) -> io::Result<()> {
    // Sessão por ligação: fecha no fim do atendimento (não partilhada).
    let mut hello_feito = false;
    loop {
        let Some(mut corpo) = ler_pedido(&mut fluxo)? else {
            // Payload acima do limite: responde 0x09 e fecha.
            let resposta =
                resposta_erro(ERR_PAYLOAD_GRANDE_DEMAIS, "enquadramento acima de 16 MiB");
            escrever_resposta(&mut fluxo, &resposta)?;
            return Ok(());
        };
        let mut resposta = processar_sessao(&estado, &corpo, &mut hello_feito);
        escrever_resposta(&mut fluxo, &resposta)?;
        // O pedido pode conter seed/chaves (ENCODE/ENVIAR/handshake) e
        // a resposta entrega chaves (PEDIR/ACEITAR/CONFIRMAR) — ambos
        // são apagados após o uso (Resumo §22).
        resposta.zeroize();
        corpo.zeroize();
    }
}

// ---------------------------------------------------------------------
// Binding do socket
// ---------------------------------------------------------------------

/// Cria/renova o socket de escuta com permissões 0600.
///
/// * socket morto (ligação recusada) → é removido e recriado;
/// * outro daemon ativo → `AddrInUse` (nunca partilhar o socket);
/// * ficheiro não-socket existente → também removido (stale).
pub fn prender(caminho: &Path) -> io::Result<std::os::unix::net::UnixListener> {
    if caminho.exists() {
        match std::os::unix::net::UnixStream::connect(caminho) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "outro daemon já ativo neste socket",
                ));
            }
            Err(_) => fs::remove_file(caminho)?,
        }
    }
    if let Some(pai) = caminho.parent() {
        fs::create_dir_all(pai)?;
    }
    let listener = std::os::unix::net::UnixListener::bind(caminho)?;
    // Só o utilizador do daemon liga/escuta (enquadramento de privacidade).
    let permissoes = fs::Permissions::from_mode(0o600);
    fs::set_permissions(caminho, permissoes)?;
    Ok(listener)
}

/// `SO_PEERCRED` — verdadeiro se o par pertence ao próprio utilizador.
///
/// Comparador puro (testável sem sockets) usado por
/// [`servir_com_paragem`] antes de atender: outro uid → a ligação é
/// descartada **sem resposta** (`ipc_spec.md` §Transporte). O caminho
/// do socket já é `0600`; isto é a defesa em profundidade que impede o
/// IPC de ser fronteira de confiança ambígua (Resumo §31).
fn par_do_uid_proprio(uid_par: u32) -> bool {
    // `getuid` não tem efeitos laterais — chamada segura apesar do `unsafe`.
    uid_par == unsafe { libc::getuid() }
}

/// Lê o uid do processo par via `SO_PEERCRED` (Linux).
///
/// `None` se o `getsockopt` falhar — o chamador trata como "não
/// autenticado" e fecha a ligação (falha para o lado seguro).
fn uid_do_par(fluxo: &std::os::unix::net::UnixStream) -> Option<u32> {
    use std::os::unix::io::AsRawFd;
    let mut credenciais: libc::ucred = unsafe { std::mem::zeroed() };
    let mut comprimento = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // `as_raw_fd` devolve um fd vivo enquanto o `fluxo` existir — a
    // chamada é síncrona e não guarda o descritor.
    let resultado = unsafe {
        libc::getsockopt(
            fluxo.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credenciais as *mut libc::ucred).cast::<libc::c_void>(),
            &mut comprimento,
        )
    };
    (resultado == 0).then_some(credenciais.uid)
}

/// Número máximo de ligações atendidas em simultâneo.
///
/// Uma thread por ligação, e cada thread pode reservar até
/// [`limite_carga`] bytes (`ler_pedido` aloca o buffer do payload). Sem
/// limite, o total não tem tecto: um cliente do mesmo uid — que é tudo o
/// que `SO_PEERCRED` exige — abre N ligações, cada uma a reservar um
/// buffer do limite, e o daemon é morto pelo OOM killer.
///
/// 32 é generoso para o uso real. O cliente Python abre uma ligação
/// por pedido (`docs/ipc_spec.md`), e os testes E2E usam duas a três
/// em simultâneo. Um limite alto deixaria passar o caso legítimo sem
/// deixar passar o ataque, e um baixo criaria um risco de negação de
/// serviço — que é o mesmo problema pelo outro lado.
///
/// `ONYXCHAT_LIGACOES_MAX` sobrescreve, pela mesma razão que
/// `ONYXCHAT_MAX_PAYLOAD`: quem conhece o seu hardware decide.
pub const LIGACOES_MAX: usize = 32;

/// Lê o limite de ligações, com override de ambiente.
fn limite_ligacoes() -> usize {
    match std::env::var("ONYXCHAT_LIGACOES_MAX")
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some(v) if !v.is_empty() => {
            // Um valor mal formado é ignorado e cai no omissão, em vez
            // de impedir o daemon de arrancar: o limite é uma
            // protecção, não uma pré-condição de funcionamento.
            v.parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .unwrap_or(LIGACOES_MAX)
        }
        _ => LIGACOES_MAX,
    }
}

/// Contagem de vagas para os atendimentos em curso.
///
/// Um `Semaphore` da biblioteca padrão seria a escolha óbvia, e é o que
/// quase toda a gente escreve primeiro. O que está aqui é um
/// `Mutex` + `Condvar` porque o daemon também é servidor Unix — atende
/// o mesmo socket em modo bloqueante — e o `Mutex` já está no lugar
/// para o resto do estado. Um semáforo separado seria um segundo
/// mecanismo de sincronização para proteger a mesma coisa.
///
/// A alternativa atómica (`AtomicUsize` com espera activa por
/// *spin*) foi descartada: consome um núcleo inteiro por thread à
/// espera, e este daemon serve um utilizador só.
struct Vagas {
    /// Contagem de vagas livres, com a variável de condição que a
    /// sinaliza.
    ///
    /// O par fica num único `Arc` — e não dois `Arc` separados — para
    /// que o guarda e a estrutura partilhem exactamente o mesmo
    /// objecto. Com dois, um `Condvar` de uma vida e um `Mutex` de
    /// outra seria um bug de sincronização a attendre por alguém.
    interior: Arc<(Mutex<usize>, Condvar)>,
}

impl Vagas {
    fn novas(n: usize) -> Self {
        Vagas {
            interior: Arc::new((Mutex::new(n), Condvar::new())),
        }
    }

    /// Espera por uma vaga e devolve um guarda que a devolve ao cair.
    ///
    /// O guarda segura o `Arc` do interior, não um índice: quando a
    /// thread termina e o guarda cai, a contagem sobe. A thread
    /// continua a poder atender sem parar.
    fn adquirir(&self) -> Guarda {
        let (lock, cv) = &*self.interior;
        let mut livres = lock.lock().unwrap_or_else(|e| e.into_inner());
        while *livres == 0 {
            livres = cv.wait(livres).unwrap_or_else(|e| e.into_inner());
        }
        *livres -= 1;
        Guarda(Arc::clone(&self.interior))
    }
}

/// Devolve a vaga ao cair. Não faz mais nada.
struct Guarda(Arc<(Mutex<usize>, Condvar)>);

impl Drop for Guarda {
    fn drop(&mut self) {
        let (lock, cv) = &*self.0;
        // Um `Mutex` envenenado significaria que um atendimento entrou
        // em pânico com o lock tomado. Panicking outra vez aqui, no
        // `Drop`, aborta o processo — por isso recupera-se o lock
        // envenenado em vez de propagar.
        let mut livres = lock.lock().unwrap_or_else(|e| e.into_inner());
        *livres += 1;
        // `notify_one` e não `notify_all`: só uma thread ficou com uma
        // vaga nova, e acordar todas as restantes seria trabalho
        // inútil em cada fim de atendimento.
        cv.notify_one();
    }
}

/// Ciclo principal de aceitação; `paragem` (testes) encerra o ciclo.
///
/// Em produção (`paragem = None`) o ciclo é infinito e só sai por
/// erro de `accept` propagado ao chamador. O `estado` é partilhado por
/// todas as ligações (a sessão P2P sobrevive a clientes que fecham).
pub fn servir_com_paragem(
    caminho: &Path,
    paragem: Option<Arc<AtomicBool>>,
    estado: Arc<Estado>,
) -> io::Result<()> {
    let listener = prender(caminho)?;
    // Uma permissão por thread em atendimento, em vez de uma por
    // ligação aceite.
    //
    // O `Semaphore` conta threads *vivas*, não Threads criadas: quando o
    // atendimento termina e a thread morre, a permissão é devolvida
    // automaticamente ao cair do `Arc`. Sem isto, a aceitação continuava
    // a criar threads sem limite e o `Semaphore` só serviria para
    // atrasar o problema.
    //
    // O que se perde, ao atingir o limite, é a aceitação de uma
    // ligação — não uma resposta de erro. O socket continua no backlog
    // do kernel e é servido assim que uma thread terminate. Para o
    // cliente é um atraso; para o atacante, nada.
    let vagas = Vagas::novas(limite_ligacoes());
    loop {
        if paragem.as_ref().is_some_and(|p| p.load(Ordering::Relaxed)) {
            return Ok(());
        }
        let (fluxo, _endereco) = listener.accept()?;
        // Autenticação local (SO_PEERCRED): uid ≠ do daemon → a
        // ligação é fechada imediatamente, sem resposta nenhuma.
        // (A inexistência de resposta é a resposta.)
        match uid_do_par(&fluxo) {
            Some(uid) if par_do_uid_proprio(uid) => {}
            _ => continue, // `fluxo` descartado aqui → fecho silencioso.
        }
// Espera por uma vaga. Bloquear aqui, e não antes, é
        // deliberado: a autenticação por uid é o filtro que deve
        // aplicar-se a toda a gente, mesmo a quem vai ser recusado
        // depois por falta de vaga.
        let vaga = vagas.adquirir();
        // Uma thread por ligação: pedidos síncronos, múltiplas ligações.
        let cliente = Arc::clone(&estado);
        thread::spawn(move || {
            // A vaga vive até ao fim do atendimento; quando a thread
            // morre, o guarda cai e devolve-a.
            let _vaga = vaga;
            let _ = atender(fluxo, cliente);
        });
    }
}

/// Ponto de entrada do servidor (sem paragem — ciclo infinito).
pub fn servir(caminho: &Path, estado: Arc<Estado>) -> io::Result<()> {
    servir_com_paragem(caminho, None, estado)
}

// =====================================================================
// Testes — `ipc_testes.rs`
// ---------------------------------------------------------------------
// Os 86 testes desta unidade estão num ficheiro próprio. O porquê
// completo está no topo desse ficheiro; em resumo: `ipc.rs` tinha 1445
// linhas de produção e 2009 de asserções, e a segunda metade barulhava
// a leitura do protocolo.
//
// Este módulo é **filho** de `ipc`, não um `tests/` de integração. Os
// testes precisam do que é privado — `processar_pedido`, `bloquear`,
// `Estado::novo` — e um módulo de integração só vê `pub`. Sendo filho,
// vê o privado do pai sem que uma única coisa deixe de ser privada —
// por isso `#[path]` e não `mod ipc_testes` no `lib.rs`.
#[cfg(test)]
#[path = "ipc_testes.rs"]
mod testes;
