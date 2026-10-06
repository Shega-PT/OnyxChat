// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// handshake.rs — handshake de amizade (Etapa 7)
// ---------------------------------------------------------------------
// Implementa `docs/handshake.md` ao byte:
//
//   FRIEND_REQUEST 0x10 → versao(1)‖pub(32)‖k1(32)‖k5(32)‖k9(32)‖
//                         nonce(16)‖sig(64) = 209 bytes,
//                         sig sobre "ONYX/FRIEND/REQ"‖0x01‖…
//   FRIEND_ACCEPT  0x11 → versao(1)‖pub(32)‖k5(32)‖k9(32)‖nonce(16)‖
//                         sig(64) = 177 bytes,
//                         sig sobre "ONYX/FRIEND/ACC"‖0x01‖…
//   FRIEND_REJECT  0x12 → versao(1)‖nonce(16)‖sig(64) = 81 bytes,
//                         sig sobre "ONYX/FRIEND/REJ"‖0x01‖nonce
//
// Regras obrigatórias do spec (todas cobertas por teste):
//   1. assinatura verificada ANTES de qualquer chave ser aceite;
//   2. versão no primeiro byte do corpo e dentro do transcript
//      (anti-downgrade: trocar `0x01` por `0x02` falha a assinatura);
//   3. anti-replay: nonce repetido de um pedido → rejeitado;
//   4. rotação: novo FRIEND_REQUEST da mesma pub substitui as chaves;
//   5. sem log de conteúdo — os erros só transportam metadados.
//
// O módulo é puro (sem sockets): o `ipc.rs` chama-o com os corpos que
// chegam pelos frames P2P e guarda o resultado em `Estado`.
// =====================================================================

use std::collections::{HashSet, VecDeque};
use std::fmt;

use crypto_core::signer::{chave_publica_a_partir_da_seed, TAMANHO_CHAVE_IDENTIDADE};
use zeroize::Zeroize;

use crate::p2p::FRAME_FRIEND_ACCEPT;
use crate::p2p::FRAME_FRIEND_REJECT;
use crate::p2p::FRAME_FRIEND_REQUEST;

// ---------------------------------------------------------------------
// Constantes do protocolo
// ---------------------------------------------------------------------

/// Tamanho do nonce aleatório do handshake (16 bytes, nunca repetido).
pub const TAM_NONCE: usize = 16;
/// Tamanho da chave pública/privada Ed25519.
pub const TAM_PUB: usize = TAMANHO_CHAVE_IDENTIDADE;
/// Versão do protocolo de handshake — **primeiro byte do corpo** e
/// parte do transcript (anti-downgrade; `docs/handshake.md` §Transcript).
pub const VERSAO_PROTOCOLO: u8 = 0x01;

/// Versões do handshake que este código **lê**.
///
/// Mesma regra do envelope (`envelope::VERSOES_ACEITE`): um corpo de uma
/// versão desta lista é aceite, qualquer outra dá
/// `ErroHandshake::VersaoDesconhecida` → IPC `0x0C`. Quando existir uma
/// v0x02, acrescentar aqui é suficiente.
///
/// A versão está dentro do transcript assinado, logo aceitar uma versão
/// suportada não é downgrade — ver `docs/index.md` §Princípio da
/// tolerância de versões para a reconciliação com o anti-downgrade.
pub const VERSOES_ACEITE: &[u8] = &[VERSAO_PROTOCOLO];
/// Corpo do `FRIEND_REQUEST`: 1 + 4×32 + 16 + 64 = 209 bytes.
pub const TAM_CORPO_PEDIDO: usize = 1 + 4 * TAM_PUB + TAM_NONCE + 64;
/// Corpo do `FRIEND_ACCEPT`: 1 + 3×32 + 16 + 64 = 177 bytes.
pub const TAM_CORPO_ACEITE: usize = 1 + 3 * TAM_PUB + TAM_NONCE + 64;
/// Corpo do `FRIEND_REJECT`: 1 + 16 + 64 = 81 bytes.
pub const TAM_CORPO_RECUSA: usize = 1 + TAM_NONCE + 64;

/// Domínio assinado do pedido (literal do spec, sem terminador).
pub const DOMINIO_PEDIDO: &[u8] = b"ONYX/FRIEND/REQ";
/// Domínio assinado do aceite.
pub const DOMINIO_ACEITE: &[u8] = b"ONYX/FRIEND/ACC";
/// Domínio assinado da recusa.
pub const DOMINIO_RECUSA: &[u8] = b"ONYX/FRIEND/REJ";

/// Capacidade da memória de nonces (anti-replay circular).
pub const CAPACIDADE_NONCES: usize = 256;

/// Tipo de frame do pedido (espelha `p2p::FRAME_FRIEND_REQUEST`).
pub const TIPO_PEDIDO: u8 = FRAME_FRIEND_REQUEST;
/// Tipo de frame do aceite.
pub const TIPO_ACEITE: u8 = FRAME_FRIEND_ACCEPT;
/// Tipo de frame da recusa.
pub const TIPO_RECUSA: u8 = FRAME_FRIEND_REJECT;

// ---------------------------------------------------------------------
// Erros
// ---------------------------------------------------------------------

/// Falha na validação de uma mensagem de handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroHandshake {
    /// Corpo com comprimento diferente do esperado para o tipo.
    ComprimentoInvalido { esperado: usize, obtido: usize },
    /// Primeiro byte do corpo fora de [`VERSOES_ACEITE`] (anti-downgrade).
    VersaoDesconhecida { obtida: u8 },
    /// Assinatura Ed25519 inválida ou chave pública não representável.
    AssinaturaInvalida,
    /// Nonce já visto (replay de um pedido congelado).
    NonceRepetido,
    /// O aceite não responde ao pedido (nonce divergente).
    NaoResponde,
}

impl fmt::Display for ErroHandshake {
    /// Mensagem legível — nunca inclui chaves nem nonces.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroHandshake::ComprimentoInvalido { esperado, obtido } => write!(
                f,
                "corpo de handshake inválido: esperado {esperado} bytes, obtido {obtido}"
            ),
            ErroHandshake::VersaoDesconhecida { obtida } => {
                write!(f, "versão de handshake desconhecida: 0x{obtida:02x}")
            }
            ErroHandshake::AssinaturaInvalida => write!(f, "assinatura de handshake inválida"),
            ErroHandshake::NonceRepetido => write!(f, "nonce de handshake repetido"),
            ErroHandshake::NaoResponde => {
                write!(f, "aceite não corresponde ao pedido enviado")
            }
        }
    }
}

impl ErroHandshake {
    /// Código IPC — todos os casos caem em `0x0C HandshakeInvalido`.
    pub fn para_ipc(&self) -> u8 {
        0x0C
    }
}

// ---------------------------------------------------------------------
// Estruturas decodificadas
// ---------------------------------------------------------------------

/// `FRIEND_REQUEST` validado (camadas do iniciador A).
///
/// **Zeroização (Resumo §22):** K1/K5/K9 são apagadas no descarte —
/// o pedido transita em RAM só enquanto é validado/assinado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    /// Chave pública de A (identidade).
    pub publica: [u8; TAM_PUB],
    /// K1 — escolhida por A, passa a ser partilhada.
    pub k1: [u8; 32],
    /// K5_A — camada do remetente A.
    pub k5: [u8; 32],
    /// K9_A — última camada de mensagens B → A.
    pub k9: [u8; 32],
    /// Nonce aleatório de 16 bytes do pedido.
    pub nonce: [u8; TAM_NONCE],
}

/// `FRIEND_ACCEPT` validado (camadas do receptor B).
///
/// **Zeroização (Resumo §22):** K5/K9 são apagadas no descarte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aceite {
    /// Chave pública de B (identidade).
    pub publica: [u8; TAM_PUB],
    /// K5_B — camada do remetente B.
    pub k5: [u8; 32],
    /// K9_B — última camada de mensagens A → B.
    pub k9: [u8; 32],
    /// Eco do nonce do pedido (liga o aceite ao pedido).
    pub nonce: [u8; TAM_NONCE],
}

/// Amizade estabelecida — o que cada lado guarda ao fim do handshake.
///
/// **Zeroização (Resumo §22):** as cinco chaves (K1 + K5/K9 de cada
/// lado) são apagadas no descarte — inclui a rotação (substituição por
/// uma nova amizade) e o fim do processo (`docs/key_management.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Amizade {
    /// Identidade do par (fixa; rotações mudam só as chaves).
    pub publica: [u8; TAM_PUB],
    /// K1 partilhada pelo par.
    pub k1: [u8; 32],
    /// K5 própria (camada de envio deste lado).
    pub k5_proprio: [u8; 32],
    /// K9 própria (última camada de receção deste lado).
    pub k9_proprio: [u8; 32],
    /// K5 do par (camada de envio do par).
    pub k5_par: [u8; 32],
    /// K9 do par (última camada para o par abrir).
    pub k9_par: [u8; 32],
}

impl Drop for Pedido {
    /// Apaga K1/K5/K9 no descarte (identidade e nonce são públicos).
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.k1.zeroize();
        self.k5.zeroize();
        self.k9.zeroize();
    }
}

impl Drop for Aceite {
    /// Apaga K5/K9 no descarte (identidade e nonce são públicos).
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.k5.zeroize();
        self.k9.zeroize();
    }
}

impl Drop for Amizade {
    /// Apaga as cinco chaves no descarte (a `publica` é identidade
    /// pública e não requer limpeza).
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.k1.zeroize();
        self.k5_proprio.zeroize();
        self.k9_proprio.zeroize();
        self.k5_par.zeroize();
        self.k9_par.zeroize();
    }
}

// ---------------------------------------------------------------------
// Helpers puros
// ---------------------------------------------------------------------

/// Gera um nonce de handshake com o CSPRNG do sistema.
pub fn gerar_nonce() -> [u8; TAM_NONCE] {
    let mut nonce = [0u8; TAM_NONCE];
    crypto_core::rng::preencher(&mut nonce);
    nonce
}

/// Valida o comprimento exato de um corpo de handshake.
fn exigir_comprimento(corpo: &[u8], esperado: usize) -> Result<(), ErroHandshake> {
    if corpo.len() != esperado {
        return Err(ErroHandshake::ComprimentoInvalido {
            esperado,
            obtido: corpo.len(),
        });
    }
    Ok(())
}

/// Exige que o primeiro byte do corpo esteja em [`VERSOES_ACEITE`].
///
/// Corre sempre **depois** do comprimento (necessário para existir o
/// byte) e **antes** da assinatura: é a etapa mais barata e rejeita cedo
/// qualquer tentativa de downgrade (`docs/handshake.md` §Validação).
///
/// Aceita-se qualquer versão da lista, não apenas a que escrevemos — é a
/// tolerância de versões. A lista tem um elemento hoje.
fn exigir_versao(corpo: &[u8]) -> Result<(), ErroHandshake> {
    if !VERSOES_ACEITE.contains(&corpo[0]) {
        return Err(ErroHandshake::VersaoDesconhecida { obtida: corpo[0] });
    }
    Ok(())
}

/// Verifica a assinatura de um corpo contra a `publica` indicada.
fn verificar(
    publica: &[u8; TAM_PUB],
    regiao: &[u8],
    assinatura: &[u8],
) -> Result<(), ErroHandshake> {
    crypto_core::verify_signature(regiao, assinatura, publica)
        .map_err(|_| ErroHandshake::AssinaturaInvalida)
}

// ---------------------------------------------------------------------
// FRIEND_REQUEST
// ---------------------------------------------------------------------

/// Monta um `FRIEND_REQUEST` assinado: gera K1/K5/K9/nonce e devolve
/// as chaves do iniciador mais o corpo pronto a enviar.
///
/// Infallível: a seed é uma chave válida e assinar um corpo de tamanho
/// fixo nunca falha — por isso não devolve `Result`.
pub fn montar_pedido(seed: &[u8; TAM_PUB]) -> (Pedido, Vec<u8>) {
    montar_pedido_com(
        seed,
        gerar_nonce(),
        crypto_core::gerar_chave(),
        crypto_core::gerar_chave(),
        crypto_core::gerar_chave(),
    )
}

/// Idem [`montar_pedido`] com nonce e camadas fornecidos — variante
/// determinística para os vetores oficiais (`tests/vectors/`), que
/// exigem bytes exactamente reprodutíveis (Resumo §25).
pub fn montar_pedido_com(
    seed: &[u8; TAM_PUB],
    nonce: [u8; TAM_NONCE],
    k1: [u8; 32],
    k5: [u8; 32],
    k9: [u8; 32],
) -> (Pedido, Vec<u8>) {
    let pedido = Pedido {
        publica: chave_publica_a_partir_da_seed(seed),
        k1,
        k5,
        k9,
        nonce,
    };
    let corpo = assinar_pedido(&pedido, seed);
    (pedido, corpo)
}

/// Região assinada do pedido + assinatura → corpo de 209 bytes.
///
/// `TRANSCRIPT = "ONYX/FRIEND/REQ" ‖ 0x01 ‖ pub ‖ k1 ‖ k5 ‖ k9 ‖ nonce`
/// e o corpo é `0x01 ‖ mesmos campos ‖ sig` — ou seja, o transcript é
/// `domínio ‖ corpo[..145]`, montagem canónica sem duplicação.
fn assinar_pedido(pedido: &Pedido, seed: &[u8; TAM_PUB]) -> Vec<u8> {
    let fim_campos = 1 + 4 * TAM_PUB + TAM_NONCE; // 145 (versão + campos)
    let mut regiao = Vec::with_capacity(DOMINIO_PEDIDO.len() + fim_campos);
    regiao.extend_from_slice(DOMINIO_PEDIDO);
    regiao.push(VERSAO_PROTOCOLO);
    regiao.extend_from_slice(&pedido.publica);
    regiao.extend_from_slice(&pedido.k1);
    regiao.extend_from_slice(&pedido.k5);
    regiao.extend_from_slice(&pedido.k9);
    regiao.extend_from_slice(&pedido.nonce);
    let assinatura = crypto_core::sign_message(&regiao, seed);
    // Transcript temporário com chaves: apaga após assinar (§22).
    regiao.zeroize();

    let mut corpo = Vec::with_capacity(TAM_CORPO_PEDIDO);
    corpo.push(VERSAO_PROTOCOLO);
    corpo.extend_from_slice(&pedido.publica);
    corpo.extend_from_slice(&pedido.k1);
    corpo.extend_from_slice(&pedido.k5);
    corpo.extend_from_slice(&pedido.k9);
    corpo.extend_from_slice(&pedido.nonce);
    debug_assert_eq!(corpo.len(), fim_campos, "campos canónicos");
    corpo.extend_from_slice(&assinatura);
    corpo
}

/// Valida o corpo de um `FRIEND_REQUEST` (comprimento + versão +
/// assinatura), por essa ordem.
///
/// # Erros
/// - `ComprimentoInvalido` — ≠ 209 bytes;
/// - `VersaoDesconhecida` — primeiro byte ≠ `0x01` (anti-downgrade);
/// - `AssinaturaInvalida` — assinatura de A não valida com `pub_A`.
pub fn validar_pedido(corpo: &[u8]) -> Result<Pedido, ErroHandshake> {
    exigir_comprimento(corpo, TAM_CORPO_PEDIDO)?;
    exigir_versao(corpo)?;
    let fim_campos = 1 + 4 * TAM_PUB + TAM_NONCE;
    let pedido = Pedido {
        publica: corpo[1..33].try_into().expect("32B validados"),
        k1: corpo[33..65].try_into().expect("32B validados"),
        k5: corpo[65..97].try_into().expect("32B validados"),
        k9: corpo[97..129].try_into().expect("32B validados"),
        nonce: corpo[129..145].try_into().expect("16B validados"),
    };
    let mut regiao = Vec::with_capacity(DOMINIO_PEDIDO.len() + fim_campos);
    regiao.extend_from_slice(DOMINIO_PEDIDO);
    regiao.extend_from_slice(&corpo[..fim_campos]);
    let ok = verificar(&pedido.publica, &regiao, &corpo[fim_campos..]);
    // Transcript temporário com chaves: apaga em ambos os ramos (§22).
    regiao.zeroize();
    ok?;
    Ok(pedido)
}

// ---------------------------------------------------------------------
// FRIEND_ACCEPT
// ---------------------------------------------------------------------

/// Monta um `FRIEND_ACCEPT` (B) ecoando o `nonce` do pedido de A.
///
/// A `k1` não viaja aqui: ambos já a partilham via `FRIEND_REQUEST`.
pub fn montar_aceite(
    seed: &[u8; TAM_PUB],
    k5: &[u8; 32],
    k9: &[u8; 32],
    nonce: &[u8; TAM_NONCE],
) -> (Aceite, Vec<u8>) {
    let aceite = Aceite {
        publica: chave_publica_a_partir_da_seed(seed),
        k5: *k5,
        k9: *k9,
        nonce: *nonce,
    };
    let fim_campos = 1 + 3 * TAM_PUB + TAM_NONCE; // 113
    let mut regiao = Vec::with_capacity(DOMINIO_ACEITE.len() + fim_campos);
    regiao.extend_from_slice(DOMINIO_ACEITE);
    regiao.push(VERSAO_PROTOCOLO);
    regiao.extend_from_slice(&aceite.publica);
    regiao.extend_from_slice(&aceite.k5);
    regiao.extend_from_slice(&aceite.k9);
    regiao.extend_from_slice(&aceite.nonce);
    let assinatura = crypto_core::sign_message(&regiao, seed);
    // Transcript temporário com chaves: apaga após assinar (§22).
    regiao.zeroize();

    let mut corpo = Vec::with_capacity(TAM_CORPO_ACEITE);
    corpo.push(VERSAO_PROTOCOLO);
    corpo.extend_from_slice(&aceite.publica);
    corpo.extend_from_slice(&aceite.k5);
    corpo.extend_from_slice(&aceite.k9);
    corpo.extend_from_slice(&aceite.nonce);
    debug_assert_eq!(corpo.len(), fim_campos, "campos canónicos");
    corpo.extend_from_slice(&assinatura);
    (aceite, corpo)
}

/// Valida o corpo de um `FRIEND_ACCEPT` (comprimento + versão +
/// assinatura), por essa ordem.
pub fn validar_aceite(corpo: &[u8]) -> Result<Aceite, ErroHandshake> {
    exigir_comprimento(corpo, TAM_CORPO_ACEITE)?;
    exigir_versao(corpo)?;
    let fim_campos = 1 + 3 * TAM_PUB + TAM_NONCE;
    let aceite = Aceite {
        publica: corpo[1..33].try_into().expect("32B validados"),
        k5: corpo[33..65].try_into().expect("32B validados"),
        k9: corpo[65..97].try_into().expect("32B validados"),
        nonce: corpo[97..113].try_into().expect("16B validados"),
    };
    let mut regiao = Vec::with_capacity(DOMINIO_ACEITE.len() + fim_campos);
    regiao.extend_from_slice(DOMINIO_ACEITE);
    regiao.extend_from_slice(&corpo[..fim_campos]);
    let ok = verificar(&aceite.publica, &regiao, &corpo[fim_campos..]);
    // Transcript temporário com chaves: apaga em ambos os ramos (§22).
    regiao.zeroize();
    ok?;
    Ok(aceite)
}

// ---------------------------------------------------------------------
// FRIEND_REJECT
// ---------------------------------------------------------------------

/// Monta uma `FRIEND_REJECT` (B) sobre o `nonce` do pedido rejeitado.
pub fn montar_recusa(seed: &[u8; TAM_PUB], nonce: &[u8; TAM_NONCE]) -> Vec<u8> {
    let mut regiao = Vec::with_capacity(DOMINIO_RECUSA.len() + 1 + TAM_NONCE);
    regiao.extend_from_slice(DOMINIO_RECUSA);
    regiao.push(VERSAO_PROTOCOLO);
    regiao.extend_from_slice(nonce);
    let assinatura = crypto_core::sign_message(&regiao, seed);

    let mut corpo = Vec::with_capacity(TAM_CORPO_RECUSA);
    corpo.push(VERSAO_PROTOCOLO);
    corpo.extend_from_slice(nonce);
    corpo.extend_from_slice(&assinatura);
    corpo
}

/// Valida o comprimento e a versão da `FRIEND_REJECT` e confirma que
/// ecoa o `nonce_esperado` do pedido que enviamos (a assinatura é
/// verificada por `validar_recusa_com`, pois a recusa não traz `pub`
/// própria).
///
/// ## Porque é que isto não é código morto
///
/// Foi candidato a remoção em G4: nenhum caminho de produção do daemon
/// lhe chama. E não lhe chama por desenho, não por esquecimento — o
/// daemon é transporte, não protocolo. `processar_receber` devolve o
/// frame ao cliente tal e qual, e é o **cliente** que decide se aquela
/// `FRIEND_REJECT` é legítima.
///
/// Isto é a estrutura certa: um validador que corre no cliente não pode
/// ser contornado por um daemon que o ignorasse, e o fuzz alvo
/// (`fuzz_targets/handshake_parser.rs`) exercita-o como parser sem
/// reliance na rede. O que o daemon **não** faz é aceitar uma recusa
/// para fechar um pedido sem a validar, porque não fecha pedidos: o
/// pedido pendente vive em `rede.pedido` e é o cliente que decide o que
/// lhe acontece.
///
/// # Erros
/// - `ComprimentoInvalido` — ≠ 81 bytes;
/// - `VersaoDesconhecida` — primeiro byte ≠ `0x01`;
/// - `NaoResponde` — nonce divergente do pedido.
pub fn validar_recusa(corpo: &[u8], nonce_esperado: &[u8; TAM_NONCE]) -> Result<(), ErroHandshake> {
    exigir_comprimento(corpo, TAM_CORPO_RECUSA)?;
    exigir_versao(corpo)?;
    let nonce: [u8; TAM_NONCE] = corpo[1..17].try_into().expect("16B validados");
    if &nonce != nonce_esperado {
        return Err(ErroHandshake::NaoResponde);
    }
    Ok(())
}

/// Idem `validar_recusa`, verificando também a assinatura contra a
/// `publica` de quem rejeitou (normalmente o destinatário do pedido).
///
/// # Erros
/// - os de `validar_recusa`;
/// - `AssinaturaInvalida` — assinatura que não corresponde à `publica`.
pub fn validar_recusa_com(
    corpo: &[u8],
    nonce: &[u8; TAM_NONCE],
    publica: &[u8; TAM_PUB],
) -> Result<(), ErroHandshake> {
    validar_recusa(corpo, nonce)?;
    let mut regiao = Vec::with_capacity(DOMINIO_RECUSA.len() + 1 + TAM_NONCE);
    regiao.extend_from_slice(DOMINIO_RECUSA);
    regiao.extend_from_slice(&corpo[..17]);
    verificar(publica, &regiao, &corpo[17..])
}

// ---------------------------------------------------------------------
// Fluxos completos (usados pelo IPC)
// ---------------------------------------------------------------------

/// **B**: aceita um pedido de A **já validado** — gera as suas chaves
/// (K5_B/K9_B), monta o `ACCEPT` e devolve a amizade a guardar.
///
/// A validação (comprimento + assinatura) é da responsabilidade do
/// chamador (`validar_pedido`), tal como o anti-replay
/// (`RegistoAmigos`): esta função é infallível por construção.
pub fn aceitar_pedido(seed: &[u8; TAM_PUB], pedido: &Pedido) -> (Amizade, Vec<u8>) {
    aceitar_pedido_com(
        seed,
        pedido,
        crypto_core::gerar_chave(),
        crypto_core::gerar_chave(),
    )
}

/// Idem [`aceitar_pedido`] com as camadas de B fornecidas — variante
/// determinística para os vetores oficiais (Resumo §25).
pub fn aceitar_pedido_com(
    seed: &[u8; TAM_PUB],
    pedido: &Pedido,
    k5_b: [u8; 32],
    k9_b: [u8; 32],
) -> (Amizade, Vec<u8>) {
    let (_aceite, corpo_aceite) = montar_aceite(seed, &k5_b, &k9_b, &pedido.nonce);
    let amizade = Amizade {
        publica: pedido.publica,
        k1: pedido.k1,
        k5_proprio: k5_b,
        k9_proprio: k9_b,
        k5_par: pedido.k5,
        k9_par: pedido.k9,
    };
    (amizade, corpo_aceite)
}

/// **A**: confirma o `ACCEPT` de B — prova que o pedido era nosso (assinado
/// pela nossa seed), que o nonce ecoa e fecha a amizade.
pub fn confirmar_aceite(
    seed: &[u8; TAM_PUB],
    corpo_pedido: &[u8],
    corpo_aceite: &[u8],
) -> Result<Amizade, ErroHandshake> {
    let pedido = validar_pedido(corpo_pedido)?;
    // O pedido tem de estar assinado pela nossa seed (é o que enviamos).
    let nossa_pub = chave_publica_a_partir_da_seed(seed);
    if pedido.publica != nossa_pub {
        return Err(ErroHandshake::AssinaturaInvalida);
    }
    let aceite = validar_aceite(corpo_aceite)?;
    if aceite.nonce != pedido.nonce {
        return Err(ErroHandshake::NaoResponde);
    }
    Ok(Amizade {
        publica: aceite.publica,
        k1: pedido.k1,
        k5_proprio: pedido.k5,
        k9_proprio: pedido.k9,
        k5_par: aceite.k5,
        k9_par: aceite.k9,
    })
}

// ---------------------------------------------------------------------
// Registo de amizades + anti-replay
// ---------------------------------------------------------------------

/// Estado das amizades do daemon: uma por `pub` (com rotação) e a
/// memória circular de nonces vista (anti-replay de pedidos).
#[derive(Debug, Default)]
pub struct RegistoAmigos {
    /// Identidades dos pares com amizade estabelecida.
    ///
    /// Um `HashSet` de chaves, não um `HashMap` de amizades. A diferença
    /// é 32 bytes por par em vez de 192: o registo tem de responder a
    /// duas perguntas — *quantas* amizades há e *esta* identidade é uma
    /// amiga? — e nenhuma delas é sobre chaves.
    ///
    /// As chaves (K1, K5 e K9 de cada lado) **não** são guardadas aqui
    /// porque nunca foram lidas aqui. O runtime só contava e confirmava
    /// presenças; quem precisa das chaves é o cliente, que as recebe na
    /// resposta IPC do `ACEITAR_AMIZADE`/`CONFIRMAR_AMIZADE` e as guarda
    /// no seu `config.keystore` (`user/config.py` §formato v2).
    ///
    /// A consequência é que o registo é agora **imune** a uma clase de
    /// bug: com as chaves dentro, um `Clone` acidental, um `Debug`
    /// derivado ou um dump do `Estado` expunha material simétrico em
    /// memória que ninguém usa. Ver `registo_guarda_apenas_a_identidade`.
    publicas: HashSet<[u8; TAM_PUB]>,
    /// Nonces de pedidos já processados (ordem de chegada).
    nonces: VecDeque<[u8; TAM_NONCE]>,
}

impl RegistoAmigos {
    /// Registo vazio.
    pub fn novo() -> Self {
        Self::default()
    }

    /// `true` se o `nonce` já foi visto (replay).
    ///
    /// `VecDeque::contains` percorre o deque — `O(n)`, com `n` limitado
    /// por [`CAPACIDADE_NONCES`]. É a mesma complexidade de antes; a
    /// diferença é que deixou de haver um predicado escrito à mão que
    /// pode divergir do `==` que o resto do código assume.
    pub fn nonce_visto(&self, nonce: &[u8; TAM_NONCE]) -> bool {
        self.nonces.contains(nonce)
    }

    /// Regista um nonce; a memória é circular (`CAPACIDADE_NONCES`).
    pub fn guardar_nonce(&mut self, nonce: [u8; TAM_NONCE]) {
        if self.nonces.len() == CAPACIDADE_NONCES {
            self.nonces.pop_front();
        }
        self.nonces.push_back(nonce);
    }

    /// Regista a identidade de um par; devolve `true` se já era amiga.
    ///
    /// O valor de retorno é o sinal de **rotação**: aceitar um segundo
    /// pedido do mesmo par renova as chaves, e `guardar` precisa de dizer
    /// isso. O `bool` já existia com esta semântica; muda o argumento,
    /// de uma `Amizade` completa para a `publica` que a identifica.
    pub fn guardar(&mut self, publica: [u8; TAM_PUB]) -> bool {
        !self.publicas.insert(publica)
    }

    /// `true` se a `publica` tem amizade estabelecida.
    pub fn tem(&self, publica: &[u8; TAM_PUB]) -> bool {
        self.publicas.contains(publica)
    }

    /// Número de amizades ativas (reportado por `ESTADO`).
    pub fn quantidade(&self) -> usize {
        self.publicas.len()
    }
}

// =====================================================================
// Testes — `handshake_testes.rs`
// ---------------------------------------------------------------------
// Mesma decisão que em `ipc.rs` e `p2p.rs`: ficheiro próprio, filho do
// módulo, para ler o protocolo sem a segunda metade ser teste.
#[cfg(test)]
#[path = "handshake_testes.rs"]
mod testes;
