// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// ipc_testes.rs — os 86 testes de `ipc`, num ficheiro próprio
// ---------------------------------------------------------------------
// Ver a nota em `ipc.rs`, à altura do `mod testes`, para o porquê.
//
// Em duas frases: ler o protocolo significava ler dois terços de
// asserções, e os testes precisam do que é privado a `crate::ipc`,
// logo têm de ser filho do módulo e não um `tests/` de integração.
//
// Nada mudou de sítio funcional: os mesmos 86 testes, as mesmas
// asserções, o mesmo número. A suite que passa antes passa depois.
// =====================================================================

use super::*;
use std::net::{TcpListener, TcpStream};

use crate::tor::ErroTor;

// ----------------------------------------------------------------
// Utilitários de teste
// ----------------------------------------------------------------

/// Sessão válida: chaves aleatórias + identidade do remetente.
fn sessao() -> (Chaves, [u8; 32], [u8; 32]) {
    let chaves = Chaves {
        k1: crypto_core::gerar_chave(),
        k5: crypto_core::gerar_chave(),
        k9: crypto_core::gerar_chave(),
    };
    let (seed, publica) = crypto_core::gerar_identidade();
    (chaves, seed, publica)
}

/// Monta o corpo ENCODE: k1‖k5‖k9‖seed‖mensagem.
fn corpo_encode(chaves: &Chaves, seed: &[u8; 32], mensagem: &[u8]) -> Vec<u8> {
    let mut corpo = vec![CMD_ENCODE];
    corpo.extend_from_slice(&chaves.k1);
    corpo.extend_from_slice(&chaves.k5);
    corpo.extend_from_slice(&chaves.k9);
    corpo.extend_from_slice(seed);
    corpo.extend_from_slice(mensagem);
    corpo
}

/// Monta o corpo DECODE: k1‖k5‖k9‖pub‖envelope.
fn corpo_decode(chaves: &Chaves, publica: &[u8; 32], envelope: &[u8]) -> Vec<u8> {
    let mut corpo = vec![CMD_DECODE];
    corpo.extend_from_slice(&chaves.k1);
    corpo.extend_from_slice(&chaves.k5);
    corpo.extend_from_slice(&chaves.k9);
    corpo.extend_from_slice(publica);
    corpo.extend_from_slice(envelope);
    corpo
}

/// Lê uma resposta enquadrada de um stream (cliente de teste).
fn ler_resposta(fluxo: &mut std::os::unix::net::UnixStream) -> std::io::Result<Vec<u8>> {
    let mut cab = [0u8; 4];
    fluxo.read_exact(&mut cab)?;
    let n = u32::from_le_bytes(cab) as usize;
    let mut corpo = vec![0u8; n];
    fluxo.read_exact(&mut corpo)?;
    Ok(corpo)
}

/// Estado isolado com backend falso (loopback, sem rede real).
fn estado_de_teste() -> Arc<Estado> {
    Arc::new(Estado::novo(Box::new(crate::tor::TorFalso::novo())))
}

/// Serializa os testes que escrevem `$ONYXCHAT_RELAY` (env global).
static TRAVA_RELAY: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ----------------------------------------------------------------
// processar_sessao — HELLO obrigatório (portão por ligação)
// ----------------------------------------------------------------

/// `HELLO` válido → `OK ‖ versão ‖ build`; sessão aberta.
#[test]
fn hello_valido_devolve_versao_e_build() {
    let estado = estado_de_teste();
    let mut hello = false;
    let r = processar_sessao(&estado, &[CMD_HELLO, VERSAO_IPC], &mut hello);
    assert_eq!(r[0], ESTADO_OK);
    assert_eq!(r[1], VERSAO_IPC, "versão ecoada");
    assert_eq!(&r[2..], BUILD.as_bytes(), "build UTF-8");
    assert!(hello, "sessão aberta");
}

/// Pedidos antes de um `HELLO` válido → `0x02`, sessão fechada.
#[test]
fn pedidos_antes_do_hello_dao_0x02() {
    let estado = estado_de_teste();
    let (chaves, seed, _publica) = sessao();
    let mut hello = false;
    for pedido in [
        corpo_encode(&chaves, &seed, b"cedo demais"),
        vec![CMD_ESTADO],
        vec![0x7F],                      // desconhecido: o portão precede a tabela
        // Major diferente: `VERSAO_IPC + 1` seria compatível
        // (diferença de minor), que é o que a tolerância exige.
        vec![CMD_HELLO, 0x10],
    ] {
        let r = processar_sessao(&estado, &pedido, &mut hello);
        assert_eq!(r[0], ESTADO_ERRO);
        assert!(!hello, "HELLO inválido não abre a sessão");
    }
    // A sessão continua fechada: o comando anterior volta a falhar.
    let r = processar_sessao(&estado, &corpo_encode(&chaves, &seed, b"x"), &mut hello);
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);

    // Corpo vazio é enquadramento (0x02) — regra anterior ao portão.
    let r = processar_sessao(&estado, b"", &mut hello);
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);

    // HELLO correto abre; os comandos passam a ser processados.
    let r = processar_sessao(&estado, &[CMD_HELLO, VERSAO_IPC], &mut hello);
    assert_eq!(r[0], ESTADO_OK);
    assert!(hello);
    let r = processar_sessao(&estado, &corpo_encode(&chaves, &seed, b"x"), &mut hello);
    assert_eq!(r[0], ESTADO_OK, "ENCODE após HELLO é aceite");
}

/// `HELLO` com corpo ≠ 1 byte → `0x02` (antes de ver a versão).
#[test]
fn hello_corpo_errado_e_0x02() {
    for corpo in [
        vec![CMD_HELLO],
        vec![CMD_HELLO, VERSAO_IPC, 0x00],
        vec![CMD_HELLO, 0x01, 0x02, 0x03],
    ] {
        let estado = estado_de_teste();
        let mut hello = false;
        let r = processar_sessao(&estado, &corpo, &mut hello);
        assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO, "corpo {corpo:?}");
        assert!(!hello);
    }
}

/// A mensagem de `ACEITAR_AMIZADE` tem de dizer o comprimento
/// **real**. A versão anterior dizia «240» quando o correcto é 241,
/// e um número errado no diagnóstico manda o utilizador acrescentar
/// bytes ao pedido para o «resolver» —_typo que o desktop é.
///
/// O teste deriva o número das constantes, para que a mensagem e a
/// validação não possam divergir de novo.
#[test]
fn aceitar_amizade_diz_o_comprimento_certo() {
    let estado = estado_de_teste();
    let esperado = TAM_PARTE + handshake::TAM_CORPO_PEDIDO;
    assert_eq!(esperado, 241, "32 (seed) + 209 (FRIEND_REQUEST)");

    // Um corpo de comprimento errado tem de ser recusado, com o
    // número correcto na mensagem.
    let curto = vec![0u8; esperado - 1];
    let r = processar_pedido(&estado, &cmd(CMD_ACEITAR_AMIZADE, &curto));
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
    let mensagem = String::from_utf8_lossy(&r[2..]).into_owned();
    assert!(
        mensagem.contains(&esperado.to_string()),
        "a mensagem tem de dizer {esperado}: {mensagem}"
    );

    // E cada código de comando tem de ter a sua própria
    // mensagem — a de ACEITAR não pode reaproveitar a de outro
    // comando, que ensinaria o utilizador a mexer no sítio errado.
    assert!(mensagem.contains("ACEITAR_AMIZADE"), "{mensagem}");
}

/// **Matriz de tolerância de versões** (`docs/index.md`
/// §Princípio da tolerância de versões).
///
/// Este é o teste que decide se um utilizador com software antigo
/// consegue trabalhar com software novo. Uma alteração de `minor`
/// tem de ser aceite; só `major` diferente é incompatível.
#[test]
fn hello_tolerante_a_minor_diferente() {
    let estado = estado_de_teste();
    for versao_cliente in 0x00u8..=0x0F {
        let mut hello = false;
        let r = processar_sessao(&estado, &[CMD_HELLO, versao_cliente], &mut hello);
        assert_eq!(
            r[0],
            ESTADO_OK,
            "minor {versao_cliente:#04x} tem de ser aceite (major {:#x})",
            major(versao_cliente)
        );
        assert!(hello, "a sessão tem de abrir");
        // A resposta confirma a versão **do cliente**, para que ele
        // saiba que a sua foi aceite. A resposta é
        // `[estado][versao][build]`, logo a versão está em `r[1]`.
        assert_eq!(
            r[1],
            versao_cliente,
            "a resposta tem de ecoar a versão do cliente"
        );
        // E tem de incluir o build, para diagnóstico de versão.
        assert!(
            r.len() > 2 && !BUILD.is_empty(),
            "a resposta tem de incluir o build"
        );
    }
}

/// `major` diferente é incompatível — para qualquer valor.
#[test]
fn hello_rejeita_major_diferente() {
    let estado = estado_de_teste();
    for major_da_cliente in 0x10u8..=0xF0 {
        let versao = major_da_cliente | (major(VERSAO_IPC) << 4);
        let mut hello = false;
        let r = processar_sessao(&estado, &[CMD_HELLO, versao], &mut hello);
        assert_eq!(
            r[1],
            ERR_VERSAO_INCOMPATIVEL,
            "major {major_da_cliente:#x} tem de ser incompatível"
        );
        assert!(!hello, "uma versão incompatível não abre sessão");
    }
}

/// A função de compatibilidade tem de reflectingir a norma, não o
/// que acontece por acaso.
#[test]
fn versao_compativel_separa_major_de_minor() {
    assert!(versao_compativel(0x01, 0x01), "idêntica");
    assert!(versao_compativel(0x00, 0x01), "mesmo major, minor diferente");
    assert!(versao_compativel(0x0F, 0x01), "mesmo major, minor extremo");
    assert!(versao_compativel(0x1F, 0x10), "major 1 = major 1");
    assert!(!versao_compativel(0x10, 0x01), "major diferente");
    assert!(!versao_compativel(0x00, 0x10), "major 0 vs major 1");
    // Os auxiliares `major`/`minor` são a decomposição do nibble.
    assert_eq!((major(0xAB), minor(0xAB)), (0x0A, 0x0B));
    assert_eq!((major(0x01), minor(0x01)), (0, 1));
}

/// `HELLO` com versão errada → `0x12 VersaoIncompativel`.
#[test]
fn hello_versao_incompativel_0x12() {
    let estado = estado_de_teste();
    let mut hello = false;
    let r = processar_sessao(&estado, &[CMD_HELLO, 0x10], &mut hello);
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_VERSAO_INCOMPATIVEL);
    // A mensagem nomeia as duas versões. Isso não viola a política de
    // erros sem vazamentos (`docs/security_model.md`): um número de
    // versão é público, não é segredo, e sem ele o utilizador não sabe
    // que software tem do outro lado.
    let mensagem = std::str::from_utf8(&r[2..]).expect("mensagem UTF-8");
    assert!(mensagem.contains("0x10"), "nomeia a versão do cliente: {mensagem}");
    assert!(mensagem.contains("0x01"), "nomeia a versão do daemon: {mensagem}");
    assert!(!hello);
}

/// `HELLO` repetido com sessão aberta é idempotente (revalida).
#[test]
fn hello_repetido_e_idempotente() {
    let estado = estado_de_teste();
    let mut hello = false;
    let r = processar_sessao(&estado, &[CMD_HELLO, VERSAO_IPC], &mut hello);
    assert_eq!(r[0], ESTADO_OK);
    let r2 = processar_sessao(&estado, &[CMD_HELLO, VERSAO_IPC], &mut hello);
    assert_eq!(r2, r, "mesma resposta");
    assert!(hello);
    // Uma versão errada a meio da sessão fecha-a de novo.
    // `0x00` tem o mesmo major e seria aceite; a incompatibilidade
    // vem do major.
    let r3 = processar_sessao(&estado, &[CMD_HELLO, 0x10], &mut hello);
    assert_eq!(r3[1], ERR_VERSAO_INCOMPATIVEL);
    assert!(!hello, "sessão volta a fechar");
}

/// `HELLO` via `processar_pedido` (função pura) mantém-se válido.
#[test]
fn hello_via_processar_pedido() {
    let estado = estado_de_teste();
    let r = processar_pedido(&estado, &[CMD_HELLO, VERSAO_IPC]);
    assert_eq!(r[0], ESTADO_OK);
    assert_eq!(r[1], VERSAO_IPC);
}

// ----------------------------------------------------------------
// SO_PEERCRED — autenticação local do par
// ----------------------------------------------------------------

/// Só o próprio uid é aceite (outros uids nunca passam).
#[test]
fn outro_uid_e_rejeitado_pelo_comparador() {
    let meu = unsafe { libc::getuid() };
    assert!(par_do_uid_proprio(meu), "próprio uid aceite");
    assert!(
        !par_do_uid_proprio(meu.wrapping_add(1)),
        "outro uid rejeitado"
    );
}

/// `SO_PEERCRED` de um par real devolve o nosso uid (fiação).
#[test]
fn uid_do_par_do_socket_e_o_atual() {
    let dir = std::env::temp_dir().join(format!("onyx-uid-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let socket = dir.join("s.sock");
    let l = prender(&socket).expect("bind");
    let cliente = std::os::unix::net::UnixStream::connect(&socket).expect("liga");
    let (par, _) = l.accept().expect("accept");
    let meu = unsafe { libc::getuid() };
    assert_eq!(uid_do_par(&par).expect("SO_PEERCRED do lado servidor"), meu);
    assert_eq!(
        uid_do_par(&cliente).expect("SO_PEERCRED do lado cliente"),
        meu
    );
    assert!(par_do_uid_proprio(uid_do_par(&par).expect("uid")));
    let _ = fs::remove_dir_all(&dir);
}

// ----------------------------------------------------------------
// processar_pedido — protocolo puro
// ----------------------------------------------------------------

/// Política de erros (Resumo §56): nenhuma resposta de erro ecoa
/// os bytes do pedido — sem chaves, nonces, plaintext ou dados
/// arbitrários do cliente, só códigos e comprimentos.
#[test]
fn erros_de_ipc_nunca_ecoam_os_dados_do_pedido() {
    let estado = estado_de_teste();
    let marcador = [0xABu8; 16];
    // Comandos que falham rápido com corpo de 16 bytes (o RECEBER
    // é excluído: com timeout poderia bloquear o teste).
    let comandos = [
        CMD_ENCODE,
        CMD_DECODE,
        CMD_ESTADO,
        CMD_OUVIR,
        CMD_LIGAR,
        CMD_ENVIAR,
        CMD_FECHAR,
        CMD_PEDIR_AMIZADE,
        CMD_ACEITAR_AMIZADE,
        CMD_RECUSAR_AMIZADE,
        CMD_CONFIRMAR_AMIZADE,
        0x7F, // desconhecido
    ];
    for comando in comandos {
        let mut pedido = vec![comando];
        pedido.extend_from_slice(&marcador);
        let r = processar_pedido(&estado, &pedido);
        assert_eq!(r[0], ESTADO_ERRO, "comando {comando:#04x} devia falhar");
        assert!(
            !r.windows(marcador.len()).any(|j| j == marcador),
            "resposta ao {comando:#04x} ecoou dados do pedido: {r:02x?}"
        );
    }
    // HELLO com o marcador no corpo (também rejeitado).
    let mut hello = vec![CMD_HELLO, 0x02];
    hello.extend_from_slice(&marcador);
    let r = processar_pedido(&estado, &hello);
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO, "corpo ≠ 1 byte");
    assert!(
        !r.windows(marcador.len()).any(|j| j == marcador),
        "erro do HELLO ecoou dados do pedido: {r:02x?}"
    );
}

/// ENCODE→DECODE round-trip via processar_pedido (protocolo puro).
#[test]
fn encode_decode_roundtrip_puro() {
    let estado = estado_de_teste();
    let (chaves, seed, publica) = sessao();
    let resposta = processar_pedido(
        &estado,
        &corpo_encode(&chaves, &seed, "Olá mundo!".as_bytes()),
    );
    assert_eq!(resposta[0], ESTADO_OK);
    assert!(resposta.len() > TAM_MINIMO);

    let pedido = corpo_decode(&chaves, &publica, &resposta[1..]);
    let texto = processar_pedido(&estado, &pedido);
    assert_eq!(texto[0], ESTADO_OK);
    assert_eq!(&texto[1..], "Olá mundo!".as_bytes());
}

/// Corpo vazio → PayloadMalformado (0x02).
#[test]
fn corpo_vazio() {
    let estado = estado_de_teste();
    let r = processar_pedido(&estado, b"");
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
}

/// Comando desconhecido → 0x01.
#[test]
fn comando_desconhecido() {
    let estado = estado_de_teste();
    let r = processar_pedido(&estado, &[0x7F, 1, 2, 3]);
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_COMANDO_DESCONHECIDO);
    // Mensagem legível presente.
    assert!(String::from_utf8_lossy(&r[2..]).contains("desconhecido"));
}

/// ENCODE com corpo curto → 0x02.
#[test]
fn encode_corpo_curto() {
    let estado = estado_de_teste();
    let mut corpo = vec![CMD_ENCODE];
    corpo.extend_from_slice(&[0u8; 10]); // corpo de 10 B < 128 B
    let r = processar_pedido(&estado, &corpo);
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
}

/// DECODE com corpo curto → 0x02.
#[test]
fn decode_corpo_curto() {
    let estado = estado_de_teste();
    let mut corpo = vec![CMD_DECODE];
    corpo.extend_from_slice(&[0u8; 10]);
    let r = processar_pedido(&estado, &corpo);
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
}

/// Mensagem não-UTF-8 em ENCODE → 0x08.
#[test]
fn encode_mensagem_nao_utf8() {
    let estado = estado_de_teste();
    let (chaves, seed, _) = sessao();
    let r = processar_pedido(&estado, &corpo_encode(&chaves, &seed, &[0xC3, 0x28]));
    assert_eq!(r[1], ERR_TEXTO_INVALIDO_UTF8);
}

/// Mensagem acima de `MAX_PLAINTEXT` em ENCODE → 0x09, antes de
/// UTF-8 e antes de cifrar qualquer camada.
#[test]
fn encode_mensagem_acima_do_limite() {
    let estado = estado_de_teste();
    let (chaves, seed, _) = sessao();
    let grande = vec![0x41u8; crate::envelope::MAX_PLAINTEXT + 1];
    let r = processar_pedido(&estado, &corpo_encode(&chaves, &seed, &grande));
    assert_eq!(r[1], ERR_PAYLOAD_GRANDE_DEMAIS);
    // No limite exato continua a ser aceite (round-trip completo).
    let no_limite = vec![0x42u8; crate::envelope::MAX_PLAINTEXT];
    let enc = processar_pedido(&estado, &corpo_encode(&chaves, &seed, &no_limite));
    assert_eq!(enc[0], ESTADO_OK);
}

/// Envelope acima de `MAX_ENVELOPE` em DECODE → 0x09, antes de
/// verificar assinatura ou decifrar.
#[test]
fn decode_envelope_acima_do_limite() {
    let estado = estado_de_teste();
    let (chaves, _, publica) = sessao();
    let excedente = vec![0x01u8; crate::envelope::MAX_ENVELOPE + 1];
    let r = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &excedente));
    assert_eq!(r[1], ERR_PAYLOAD_GRANDE_DEMAIS);
}

/// DECODE com corpo exatamente um byte abaixo do mínimo → 0x02
/// (fronteira de `MIN_CORPO_DECODE`: 4×32 + 117 − 1).
#[test]
fn decode_corpo_um_byte_abaixo_do_minimo() {
    let estado = estado_de_teste();
    let (chaves, _, publica) = sessao();
    let r = processar_pedido(
        &estado,
        &corpo_decode(
            &chaves,
            &publica,
            &[0x01u8; crate::envelope::TAM_MINIMO - 1],
        ),
    );
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
}

/// Reenvio do mesmo envelope → `0x13 NonceRepetido`; mensagens
/// distintas (nonces novos) continuam a ser aceites.
#[test]
fn decode_nonce_repetido() {
    let estado = estado_de_teste();
    let (chaves, seed, publica) = sessao();
    let enc = processar_pedido(&estado, &corpo_encode(&chaves, &seed, b"uma vez"));
    assert_eq!(enc[0], ESTADO_OK);

    let pedido = corpo_decode(&chaves, &publica, &enc[1..]);
    let primeira = processar_pedido(&estado, &pedido);
    assert_eq!(primeira[0], ESTADO_OK, "primeira entrega aceite");
    assert_eq!(&primeira[1..], b"uma vez");

    // O registo é do `Estado` partilhado: reenvio via novo pedido
    // (e, portanto, outra ligação) continua rejeitado.
    let replay = processar_pedido(&estado, &pedido);
    assert_eq!(replay[0], ESTADO_ERRO);
    assert_eq!(replay[1], ERR_NONCE_REPETIDO);

    // Envelope novo da mesma mensagem → nonces distintos → aceite.
    let outro = processar_pedido(&estado, &corpo_encode(&chaves, &seed, b"uma vez"));
    assert_eq!(outro[0], ESTADO_OK);
    assert_ne!(&outro[1..], &enc[1..], "nonces por mensagem nunca repetem");
    let segunda = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &outro[1..]));
    assert_eq!(segunda[0], ESTADO_OK, "mensagem nova aceite");
}

/// Com assinatura inválida o registo não avança — um forjador não
/// consegue encher o anti-replay nem bloquear mensagens legítimas.
#[test]
fn decode_assinatura_invalida_nao_regista() {
    let estado = estado_de_teste();
    let (chaves, seed, publica) = sessao();
    let enc = processar_pedido(&estado, &corpo_encode(&chaves, &seed, b"integra"));
    let mut env = enc[1..].to_vec();
    let ultimo = env.len() - 1;
    env[ultimo] ^= 0x01;
    let r = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &env));
    assert_eq!(r[1], ERR_ASSINATURA_INVALIDA);
    // A versão autêntica continua aceite.
    let ok = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &enc[1..]));
    assert_eq!(ok[0], ESTADO_OK);
}

/// DECODE com corpo abaixo de `MIN_CORPO_DECODE` → 0x02.
/// (Envelope "curto" via IPC colapsa no mínimo do corpo: 128 + 117;
/// o caminho `EnvelopeInvalido`/0x06 continua testado ao nível do
/// pipeline e pela versão inválida, abaixo.)
#[test]
fn decode_envelope_curto() {
    let estado = estado_de_teste();
    let (chaves, _, publica) = sessao();
    let r = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &[0x01u8; 40]));
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
}

/// Assinatura inválida em DECODE → 0x04.
#[test]
fn decode_assinatura_invalida() {
    let estado = estado_de_teste();
    let (chaves, seed, publica) = sessao();
    let enc = processar_pedido(&estado, &corpo_encode(&chaves, &seed, b"m"));
    assert_eq!(enc[0], ESTADO_OK);
    let mut env = enc[1..].to_vec();
    let ultimo = env.len() - 1;
    env[ultimo] ^= 0x01;
    let r = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &env));
    assert_eq!(r[1], ERR_ASSINATURA_INVALIDA);
}

/// Chave K9 errada em DECODE → 0x05 (tag inválida).
#[test]
fn decode_chave_errada() {
    let estado = estado_de_teste();
    let (chaves, seed, publica) = sessao();
    let enc = processar_pedido(&estado, &corpo_encode(&chaves, &seed, b"m"));
    let erradas = Chaves {
        k9: crypto_core::gerar_chave(),
        ..chaves
    };
    let r = processar_pedido(&estado, &corpo_decode(&erradas, &publica, &enc[1..]));
    assert_eq!(r[1], ERR_DECIFRAGEM_FALHOU);
}

/// Ciphertext adulterado re-assinado → 0x05 na decifragem K9.
#[test]
fn decode_ciphertext_adulterado() {
    let estado = estado_de_teste();
    let (chaves, seed, publica) = sessao();
    let enc = processar_pedido(&estado, &corpo_encode(&chaves, &seed, b"m"));
    let mut env = enc[1..].to_vec();
    // Atacante com a seed (teste) altera o ciphertext e re-assina.
    env[40] ^= 0x01;
    let regiao: Vec<u8> = env[..env.len() - 64].to_vec();
    let nova = crypto_core::sign_message(&regiao, &seed);
    let inicio = env.len() - 64;
    env[inicio..].copy_from_slice(&nova);
    let r = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &env));
    assert_eq!(r[1], ERR_DECIFRAGEM_FALHOU);
}

/// DECODE de plaintext não-UTF-8 → 0x08.
///
/// Um remetente que viole o contrato UTF-8 produz um envelope
/// válido (assinatura OK) cujo K1 final não é texto — o receptor
/// recebe `TextoInvalidoUtf8` e nunca um crash.
#[test]
fn decode_texto_invalido_utf8() {
    let estado = estado_de_teste();
    let (chaves, seed, publica) = sessao();
    // Cifra bytes arbitrários diretamente pelo pipeline (o ENCODE
    // do IPC bloqueia isto cedo; aqui isola-se o ramo do DECODE).
    let envelope = pipeline::cifrar(&[0xFF, 0xFE, 0xC3], &chaves, &seed).expect("cifra");
    let r = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &envelope));
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_TEXTO_INVALIDO_UTF8);
}

/// Envelope com versão inválida (≥117 bytes) → 0x06 via pipeline.
#[test]
fn decode_versao_invalida() {
    let estado = estado_de_teste();
    let (chaves, seed, publica) = sessao();
    let enc = processar_pedido(&estado, &corpo_encode(&chaves, &seed, b"m"));
    let mut env = enc[1..].to_vec();
    env[0] = 0x09;
    let r = processar_pedido(&estado, &corpo_decode(&chaves, &publica, &env));
    assert_eq!(r[1], ERR_ENVELOPE_INVALIDO);
}

/// O ramo `Err` da tradução de resultado cobre os códigos IPC.
#[test]
fn resposta_de_pipeline_erro() {
    // Falha de cifragem (ENCODE) → 0x07.
    let r = resposta_de_pipeline(Err(ErroPipeline::CifragemFalhou("interna".into())), true);
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_CIFRAGEM_FALHOU);
    assert_eq!(&r[2..], b"cifragem falhou: interna");
    // Assinatura (DECODE) → 0x04.
    let r = resposta_de_pipeline(Err(ErroPipeline::AssinaturaInvalida), false);
    assert_eq!(r[1], ERR_ASSINATURA_INVALIDA);
    // Erro Lua em decifragem → 0x05.
    let r = resposta_de_pipeline(Err(ErroPipeline::Lua("chave vazia".into())), false);
    assert_eq!(r[1], ERR_DECIFRAGEM_FALHOU);
    // Erro Lua em cifragem → 0x07.
    let r = resposta_de_pipeline(Err(ErroPipeline::Lua("chave vazia".into())), true);
    assert_eq!(r[1], ERR_CIFRAGEM_FALHOU);
    // Chave inválida → 0x03.
    let r = resposta_de_pipeline(
        Err(ErroPipeline::ChaveInvalida {
            esperado: 32,
            obtido: 4,
        }),
        true,
    );
    assert_eq!(r[1], ERR_CHAVE_INVALIDA);
    // Envelope inválido → 0x06.
    let r = resposta_de_pipeline(Err(ErroPipeline::EnvelopeInvalido("curto".into())), false);
    assert_eq!(r[1], ERR_ENVELOPE_INVALIDO);
    // Sucesso também passa por aqui (estrutura única).
    let r = resposta_de_pipeline(Ok(b"dado".to_vec()), true);
    assert_eq!(r[0], ESTADO_OK);
    assert_eq!(&r[1..], b"dado");
}

/// Todas as respostas de erro terminam com mensagem UTF-8 legível.
#[test]
fn respostas_de_erro_sao_legiveis() {
    let estado = estado_de_teste();
    let casos: Vec<Vec<u8>> = vec![
        processar_pedido(&estado, b""),
        processar_pedido(&estado, &[0x55]),
        processar_pedido(&estado, &[CMD_ENCODE]),
        processar_pedido(&estado, &[CMD_DECODE]),
    ];
    for r in casos {
        assert_eq!(r[0], ESTADO_ERRO);
        assert!(r.len() > 2);
        assert!(std::str::from_utf8(&r[2..]).is_ok());
    }
}

// ----------------------------------------------------------------
// caminho_socket + prender
// ----------------------------------------------------------------

/// Caminho por omissão usa o uid; a env var sobrepõe-se.
#[test]
fn caminho_do_socket() {
    // Sem env: /tmp/onyxchat-{uid}.sock
    std::env::remove_var("ONYXCHAT_SOCKET");
    let padrao = caminho_socket();
    let uid = unsafe { libc::getuid() };
    assert_eq!(padrao, PathBuf::from(format!("/tmp/onyxchat-{uid}.sock")));

    // Com env vazia: continua a usar o padrão.
    std::env::set_var("ONYXCHAT_SOCKET", "");
    assert_eq!(caminho_socket(), padrao);

    // Com env definida: caminho livre do teste.
    std::env::set_var("ONYXCHAT_SOCKET", "/tmp/onyxchat-teste.sock");
    assert_eq!(caminho_socket(), PathBuf::from("/tmp/onyxchat-teste.sock"));
    std::env::remove_var("ONYXCHAT_SOCKET");
}

/// Socket morto (ficheiro regular) é removido ao fazer bind.
#[test]
fn bind_remove_stale() {
    let dir = std::env::temp_dir().join(format!("onyx-stale-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let socket = dir.join("d.sock");
    fs::write(&socket, b"lixo").expect("stale file");
    let _listener = prender(&socket).expect("prende após remover stale");
    assert!(socket.exists());
    // Permssões 0600.
    let modo = fs::metadata(&socket).expect("meta").permissions().mode();
    assert_eq!(modo & 0o777, 0o600);
    let _ = fs::remove_dir_all(&dir);
}

/// Caminho vazio: sem pai para criar → falha de bind propagada.
#[test]
fn bind_caminho_vazio() {
    let erro = prender(Path::new("")).expect_err("bind de caminho vazio");
    assert_eq!(erro.kind(), io::ErrorKind::NotFound);
}

/// Pai do socket é um ficheiro → `create_dir_all` falha e propaga.
#[test]
fn bind_pai_e_ficheiro() {
    let dir = std::env::temp_dir().join(format!("onyx-pai-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let ficheiro = dir.join("ficheiro");
    fs::write(&ficheiro, b"x").expect("ficheiro");
    let erro = prender(&ficheiro.join("d.sock")).expect_err("pai não é dir");
    // `create_dir_all` sobre um ficheiro existente devolve sempre
    // EEXIST (o caminho existe mas não é diretório).
    assert_eq!(erro.kind(), io::ErrorKind::AlreadyExists, "{erro:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// Outro daemon ativo (socket ligável) → AddrInUse.
#[test]
fn bind_com_daemon_ativo() {
    let dir = std::env::temp_dir().join(format!("onyx-ativo-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let socket = dir.join("d.sock");
    let _listener = prender(&socket).expect("primeiro bind");
    let erro = prender(&socket).expect_err("segundo bind tem de falhar");
    assert_eq!(erro.kind(), io::ErrorKind::AddrInUse);
    let _ = fs::remove_dir_all(&dir);
}

/// Diretório inexistente é criado pelo `prender`.
#[test]
fn bind_cria_diretorio() {
    let dir = std::env::temp_dir().join(format!("onyx-novo-{}/sub", std::process::id()));
    let socket = dir.join("d.sock");
    let _listener = prender(&socket).expect("cria dir + socket");
    assert!(socket.exists());
    let _ = fs::remove_dir_all(dir.parent().expect("pai"));
}

// ----------------------------------------------------------------
// Servidor real: ligação, vários pedidos, EOF e payload gigante
// ----------------------------------------------------------------

/// Sobe o servidor, faz ENCODE+DECODE reais e termina via paragem.
#[test]
fn servidor_atende_ligacao_real() {
    let dir = std::env::temp_dir().join(format!("onyx-ipc-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let socket = dir.join("s.sock");
    let caminho = socket.clone();

    let paragem = Arc::new(AtomicBool::new(false));
    let paragem_h = Arc::clone(&paragem);
    let estado = estado_de_teste();
    let servidor = thread::spawn(move || servir_com_paragem(&caminho, Some(paragem_h), estado));

    // Espera o socket aparecer (bind feito pela thread).
    for _ in 0..200 {
        if socket.exists() {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }

    let mut cliente = std::os::unix::net::UnixStream::connect(&socket).expect("liga");
    let (chaves, seed, publica) = sessao();

    // Pedido 0: HELLO obrigatório primeiro (versão ‖ build).
    let mut hello = Vec::new();
    hello.extend_from_slice(&2u32.to_le_bytes());
    hello.extend_from_slice(&[CMD_HELLO, VERSAO_IPC]);
    cliente.write_all(&hello).expect("escreve HELLO");
    let resp0 = ler_resposta(&mut cliente).expect("lê HELLO");
    assert_eq!(resp0[0], ESTADO_OK);
    assert_eq!(resp0[1], VERSAO_IPC);
    assert_eq!(&resp0[2..], BUILD.as_bytes());

    // Pedido 1: ENCODE (já com sessão aberta).
    let enc = corpo_encode(&chaves, &seed, b"primeiro");
    let mut pedido = Vec::new();
    pedido.extend_from_slice(&(enc.len() as u32).to_le_bytes());
    pedido.extend_from_slice(&enc);
    cliente.write_all(&pedido).expect("escreve enquadramento");
    let resp = ler_resposta(&mut cliente).expect("lê resposta");
    assert_eq!(resp[0], ESTADO_OK);

    // Pedido 2: DECODE na mesma ligação (várias trocas ✓).
    let dec = corpo_decode(&chaves, &publica, &resp[1..]);
    let mut pedido2 = Vec::new();
    pedido2.extend_from_slice(&(dec.len() as u32).to_le_bytes());
    pedido2.extend_from_slice(&dec);
    cliente.write_all(&pedido2).expect("escreve 2");
    let resp2 = ler_resposta(&mut cliente).expect("lê 2");
    assert_eq!(resp2[0], ESTADO_OK);
    assert_eq!(&resp2[1..], b"primeiro");

    // EOF: fecha a ligação; o servidor termina a thread da ligação.
    drop(cliente);

    // LIGAÇÃO 2: payload gigante (header-only) → 0x09 e fecho.
    let mut cliente2 = std::os::unix::net::UnixStream::connect(&socket).expect("liga 2");
    // Usa o limite em vigor, não a constante: o guard de
    // `ler_pedido` compara com `limite_carga()`, e um teste que
    // mandasse `MAX_PAYLOAD + 1` passaria a ter razão diferente
    // se o utilizador tivesse posto um override.
    let gigante = (limite_carga() as u32) + 1;
    cliente2.write_all(&gigante.to_le_bytes()).expect("header");
    let resp3 = ler_resposta(&mut cliente2).expect("lê 0x09");
    assert_eq!(resp3[0], ESTADO_ERRO);
    assert_eq!(resp3[1], ERR_PAYLOAD_GRANDE_DEMAIS);

    // LIGAÇÃO 3: body-only (comprimento zero) → 0x02 corpo vazio.
    let mut cliente3 = std::os::unix::net::UnixStream::connect(&socket).expect("liga 3");
    cliente3.write_all(&0u32.to_le_bytes()).expect("len 0");
    let resp4 = ler_resposta(&mut cliente3).expect("lê 0x02");
    assert_eq!(resp4[1], ERR_PAYLOAD_MALFORMADO);

    // Paragem: sinaliza e liga uma vez para acordar um `accept`
    // pendente; se o servidor já terminou, a ligação falha e ignora.
    paragem.store(true, Ordering::Relaxed);
    let _ = std::os::unix::net::UnixStream::connect(&socket);
    servidor
        .join()
        .expect("thread do servidor não panique")
        .expect("servidor termina com Ok");

    let _ = fs::remove_dir_all(&dir);
}

/// `servir` (sem paragem) propaga erro de `prender` ao chamador.
#[test]
fn servir_sem_paragem_propaga_erro() {
    let dir = std::env::temp_dir().join(format!("onyx-erro-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    // Caminho = diretório existente: a ligação falha (não é socket)
    // e `remove_file` sobre diretório devolve erro → propagado.
    let erro = servir(&dir, estado_de_teste()).expect_err("bind tem de falhar");
    assert_eq!(erro.kind(), io::ErrorKind::IsADirectory);
    let _ = fs::remove_dir_all(&dir);
}

/// Leitura de enquadramento: EOF a meio devolve erro de I/O.
#[test]
fn ler_pedido_eof_imediatamente() {
    let dir = std::env::temp_dir().join(format!("onyx-eof-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let socket = dir.join("e.sock");
    let _l = prender(&socket).expect("bind");
    let cliente = std::os::unix::net::UnixStream::connect(&socket).expect("liga");
    drop(cliente); // EOF imediato
                   // O accept no servidor devolve a ligação; atender falha com EOF.
    let (fluxo, _) = _l.accept().expect("accept");
    let resultado = atender(fluxo, estado_de_teste());
    assert!(resultado.is_err());
    let _ = fs::remove_dir_all(&dir);
}

/// Escrita de resposta com enquadramento correto (unitária).
#[test]
fn escrever_resposta_enquadramento() {
    let dir = std::env::temp_dir().join(format!("onyx-wr-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let socket = dir.join("w.sock");
    let l = prender(&socket).expect("bind");
    let mut cliente = std::os::unix::net::UnixStream::connect(&socket).expect("liga");
    let (mut servidor, _) = l.accept().expect("accept");

    escrever_resposta(&mut servidor, &[0x00, 0xAA, 0xBB]).expect("escreve");
    let mut cab = [0u8; 4];
    cliente.read_exact(&mut cab).expect("lê header");
    assert_eq!(u32::from_le_bytes(cab), 3);
    let mut corpo = [0u8; 3];
    cliente.read_exact(&mut corpo).expect("lê corpo");
    assert_eq!(&corpo, &[0x00, 0xAA, 0xBB]);
    let _ = fs::remove_dir_all(&dir);
}

/// Contrato das constantes mínimas (documentação executável).
#[test]
fn constantes_do_protocolo() {
    // `MAX_PAYLOAD` deixou de ser um literal escrito à mão; é o
    // valor por omissão derivado de um orçamento. Este teste fixa a
    // ligação entre os dois, que é o que impede alguém de voltar a
    // escrever um número sem relação com o que o daemon produz.
    assert_eq!(
        MAX_PAYLOAD,
        crate::orcamento::orcamento_padrao(),
        "MAX_PAYLOAD tem de reflectir o orçamento por omissão"
    );
    assert!(
        MAX_PAYLOAD >= orcamento::MINIMO_CORPO,
        "o limite por omissão não sirve um DECODE de {} B",
        orcamento::MINIMO_CORPO
    );
    assert_eq!(CMD_ENCODE, 0x01);
    assert_eq!(CMD_DECODE, 0x02);
    assert_eq!(ESTADO_OK, 0x00);
    assert_eq!(ESTADO_ERRO, 0x01);
    assert_eq!(MIN_CORPO_ENCODE, 128);
    assert_eq!(MIN_CORPO_DECODE, 128 + TAM_MINIMO);
    // 0x01..0x09 consecutivos conforme docs/ipc_spec.md.
    assert_eq!(ERR_COMANDO_DESCONHECIDO, 0x01);
    assert_eq!(ERR_PAYLOAD_MALFORMADO, 0x02);
    assert_eq!(ERR_CHAVE_INVALIDA, 0x03);
    assert_eq!(ERR_ASSINATURA_INVALIDA, 0x04);
    assert_eq!(ERR_DECIFRAGEM_FALHOU, 0x05);
    assert_eq!(ERR_ENVELOPE_INVALIDO, 0x06);
    assert_eq!(ERR_CIFRAGEM_FALHOU, 0x07);
    assert_eq!(ERR_TEXTO_INVALIDO_UTF8, 0x08);
    assert_eq!(ERR_PAYLOAD_GRANDE_DEMAIS, 0x09);
    assert_eq!(ERR_NAO_LIGADO, 0x0A);
    assert_eq!(ERR_TOR_INDISPONIVEL, 0x0B);
    assert_eq!(ERR_HANDSHAKE_INVALIDO, 0x0C);
    assert_eq!(ERR_DESTINO_INVALIDO, 0x0D);
    assert_eq!(ERR_SEM_MENSAGEM, 0x0E);
    assert_eq!(ERR_ESTADO_INVALIDO, 0x0F);
    assert_eq!(ERR_RATE_LIMIT, 0x10);
    assert_eq!(ERR_RELAY, 0x11);
    assert_eq!(ERR_NONCE_REPETIDO, 0x13);
    assert_eq!(TAM_PARTE, 32);
}

// ----------------------------------------------------------------
// Utilitários da Etapa 7 (rede + handshake)
// ----------------------------------------------------------------

/// Os quatro valores devolvidos por `par_ligado`.
type ParDeNos = (
    Arc<Estado>,
    Arc<Estado>,
    ([u8; 32], [u8; 32]),
    ([u8; 32], [u8; 32]),
);

/// Backend Tor que falha sempre — cobre o caminho de erro do OUVIR.
struct BackendQueFalha;

impl BackendTor for BackendQueFalha {
    fn arrancar(&mut self, _porta_local: u16) -> Result<String, ErroTor> {
        Err(ErroTor::Falha("bootstrap falhou".into()))
    }
    fn parar(&mut self) -> Result<(), ErroTor> {
        Ok(())
    }
    fn estado(&self) -> EstadoTor {
        EstadoTor::Parado
    }
    fn onion(&self) -> Option<&str> {
        None
    }
    fn ligar(&mut self, _destino: &str) -> Result<TcpStream, ErroTor> {
        Err(ErroTor::NaoArrancado)
    }
}

/// Monta um corpo de comando: `comando ‖ resto`.
fn cmd(comando: u8, resto: &[u8]) -> Vec<u8> {
    let mut corpo = vec![comando];
    corpo.extend_from_slice(resto);
    corpo
}

/// Resto do corpo do `LIGAR 0x05` (sem o byte de comando, que
/// `cmd` acrescenta) conforme docs/ipc_spec.md.
/// Endereço `.onion` **bem formado** (56 caracteres base32).
///
/// A validação de `destino` é estrita de propósito, por isso os
/// testes de `LIGAR` têm de usar endereços reais em forma — um
/// `.onion` curto seria recusado antes de chegar ao `TorFalso`, e o
/// teste deixaria de estar a testar o que dice ser.
const ONION_DE_TESTE: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.onion";

/// `.onion` bem formado mas **não registado** no `TorFalso` — o que
/// exercita o `DestinoDesconhecido` (0x0D) vindo da camada Tor, e não
/// da validação de formato.
const ONION_DESCONHECIDO: &str =
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.onion";

fn corpo_ligar(
    modo: u8,
    endpoint: &str,
    destino: &str,
    propria: &[u8; 32],
    par: &[u8; 32],
) -> Vec<u8> {
    let mut corpo = vec![modo];
    corpo.extend_from_slice(&(endpoint.len() as u16).to_le_bytes());
    corpo.extend_from_slice(endpoint.as_bytes());
    corpo.extend_from_slice(&(destino.len() as u16).to_le_bytes());
    corpo.extend_from_slice(destino.as_bytes());
    corpo.extend_from_slice(propria);
    corpo.extend_from_slice(par);
    corpo
}

/// Publica o hidden service do nó e devolve o `.onion`.
fn ouvir(estado: &Estado) -> String {
    let resposta = processar_pedido(estado, &cmd(CMD_OUVIR, &[]));
    assert_eq!(resposta[0], ESTADO_OK, "OUVIR falhou");
    String::from_utf8(resposta[1..].to_vec()).expect("onion UTF-8")
}

/// Liga `a` (cliente) ao hidden service de `b` — modo direto — e
/// espera a aceitação do lado `b` (a thread do servidor corre à
/// parte; sem esta espera os testes correriam contra uma ligação
/// ainda não instalada).
fn ligar_ate(a: &Estado, b: &Estado, onion_b: &str, pub_a: &[u8; 32], pub_b: &[u8; 32]) {
    let corpo = corpo_ligar(0x00, "", onion_b, pub_a, pub_b);
    let resposta = processar_pedido(a, &cmd(CMD_LIGAR, &corpo));
    let diagnostico =
        String::from_utf8_lossy(resposta.get(2..).unwrap_or_default()).into_owned();
    assert_eq!(resposta[0], ESTADO_OK, "LIGAR falhou: {diagnostico}");
    esperar_ligacao(b);
}

/// Espera até 2 s pela ligação instalada pela thread aceitante.
fn esperar_ligacao(estado: &Estado) {
    let mut instalada = false;
    for _ in 0..400 {
        if estado.ligado() {
            instalada = true;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(instalada, "a ligação do par não chegou em 2 s");
}

/// `a` fecha e `b` confirma o EOF: a ligação morta é limpa.
fn fechar_e_confirmar(a: &Estado, b: &Estado) {
    assert_eq!(processar_pedido(a, &cmd(CMD_FECHAR, &[]))[0], ESTADO_OK);
    assert!(!a.ligado());
    let r = receber_frame(b, 3_000);
    let diagnostico = String::from_utf8_lossy(r.get(2..).unwrap_or_default()).into_owned();
    assert_eq!(r[0], ESTADO_ERRO, "{diagnostico}");
    assert!(diagnostico.contains("fechada"), "{diagnostico}");
    assert!(!b.ligado(), "a ligação morta foi limpa");
}

/// `RECEBER` com o `timeout_ms` indicado (`0` = esperar para sempre).
fn receber_frame(estado: &Estado, timeout_ms: u32) -> Vec<u8> {
    processar_pedido(estado, &cmd(CMD_RECEBER, &timeout_ms.to_le_bytes()))
}

/// Repete `tentativa` até o envio falhar de facto com a ligação
/// já morta.
///
/// O código IPC `0x0A` é comum a "sem ligação" e a "ligação
/// fechada", por isso distingue-se pela mensagem: o teste só
/// termina quando o *envio* é que falhou (o ramo coberto).
fn falha_de_envio(mut tentativa: impl FnMut() -> Vec<u8>) -> Vec<u8> {
    let mut ultima = Vec::new();
    let mut texto = String::new();
    for _ in 0..8 {
        ultima = tentativa();
        texto = String::from_utf8_lossy(ultima.get(2..).unwrap_or_default()).into_owned();
        if ultima[0] == ESTADO_ERRO && !texto.contains("sem ligação ativa") {
            return ultima;
        }
    }
    // Nunca deve chegar aqui: o 8.º envio já falhava.
    assert!(
        !texto.contains("sem ligação ativa"),
        "o envio nunca falhou com a ligação fechada"
    );
    ultima
}

/// Deserializa um corpo de amizade `k1‖k5_p‖k9_p‖k5_par‖k9_par‖pub`.
///
/// Espelha [`corpo_da_amizade`] e existe para os testes afirmarem
/// sobre **o que o cliente recebe**, e não sobre o que o daemon
/// guarda por dentro. Desde G2 o registo só guarda identidades, e as
/// chaves existem em RAM no daemon apenas durante a construção da
/// resposta — pelo que «as chaves espelham-se» deixou de ser
/// verificável no registo, e passou a ser verificável na fronteira,
/// que é onde interessa.
fn amizade_de_corpo(corpo: &[u8]) -> Option<Amizade> {
    if corpo.len() != 6 * TAM_PARTE {
        return None;
    }
    let parte = |i: usize| {
        let mut chave = [0u8; 32];
        chave.copy_from_slice(&corpo[i * 32..(i + 1) * 32]);
        chave
    };
    Some(Amizade {
        k1: parte(0),
        k5_proprio: parte(1),
        k9_proprio: parte(2),
        k5_par: parte(3),
        k9_par: parte(4),
        publica: parte(5),
    })
}

/// Dois nós com hidden service publicado e ligados entre si.
///
/// Devolve `(a, b, (seed_a, pub_a), (seed_b, pub_b))`.
fn par_ligado() -> ParDeNos {
    let a = estado_de_teste();
    let b = estado_de_teste();
    let onion_b = ouvir(&b);
    // O `TorFalso` exige bootstrap no cliente (como o backend real).
    ouvir(&a);
    let (seed_a, pub_a) = crypto_core::gerar_identidade();
    let (seed_b, pub_b) = crypto_core::gerar_identidade();
    ligar_ate(&a, &b, &onion_b, &pub_a, &pub_b);
    (a, b, (seed_a, pub_a), (seed_b, pub_b))
}

/// Relay TCP mínimo: responde `OK` a cada comando e termina no
/// `FECHO`/EOF — suficiente para exercitar o transporte relay.
fn relay_falso() -> String {
    let servidor = TcpListener::bind("127.0.0.1:0").expect("relay");
    let porta = servidor.local_addr().expect("addr").port();
    thread::spawn(move || {
        while let Ok((mut fluxo, _)) = servidor.accept() {
            // Erro de stream (EOF/ligação morta) só termina o loop.
            let _ = atender_relay_falso(&mut fluxo);
        }
    });
    format!("127.0.0.1:{porta}")
}

/// Responde `OK` a cada comando do relay falso até ao `FECHO`/EOF.
fn atender_relay_falso(fluxo: &mut TcpStream) -> io::Result<()> {
    loop {
        let mut cabecalho = [0u8; 4];
        fluxo.read_exact(&mut cabecalho)?;
        let mut corpo = vec![0u8; u32::from_le_bytes(cabecalho) as usize];
        fluxo.read_exact(&mut corpo)?;
        if corpo.first() == Some(&p2p::RELAY_FECHO) {
            return Ok(());
        }
        let ok = [1u8, 0, 0, 0, p2p::RELAY_OK];
        fluxo.write_all(&ok)?;
    }
}

// ----------------------------------------------------------------
// ESTADO / OUVIR
// ----------------------------------------------------------------

/// Estado inicial: Tor parado, sem ligação, sem amigos, sem onion.
#[test]
fn estado_inicial_do_daemon() {
    let estado = estado_de_teste();
    let r = processar_pedido(&estado, &cmd(CMD_ESTADO, &[]));
    assert_eq!(r[0], ESTADO_OK);
    assert_eq!(&r[1..], &[0x00, 0x00, 0x00, 0x00]);
    assert_eq!(estado.tor(), 0x00);
    assert!(!estado.ligado());
    assert_eq!(estado.amigos(), 0);
    assert_eq!(estado.onion(), None);
}

/// `ESTADO` com corpo → 0x02 (só comandos sem corpo).
#[test]
fn estado_com_corpo_rejeitado() {
    let estado = estado_de_teste();
    let r = processar_pedido(&estado, &cmd(CMD_ESTADO, &[0xFF]));
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
}

/// `OUVIR` publica o onion, é idempotente e o `ESTADO` reflecte-o.
#[test]
fn ouvir_publica_onion_e_idempotente() {
    let estado = estado_de_teste();
    let onion = ouvir(&estado);
    assert_eq!(onion.len(), 62, "onion v3 = 56 + .onion");
    assert_eq!(estado.tor(), 0x02);

    // Idempotente: o mesmo endereço e a mesma escuta.
    assert_eq!(ouvir(&estado), onion);

    let r = processar_pedido(&estado, &cmd(CMD_ESTADO, &[]));
    assert_eq!(r[0], ESTADO_OK);
    assert_eq!(r[1], 0x02, "tor ativo");
    assert_eq!(r[2], 0x00, "ainda sem ligação de saída");
    assert_eq!(&r[5..], onion.as_bytes());
    assert_eq!(estado.onion().as_deref(), Some(onion.as_str()));
}

/// `OUVIR` com corpo → 0x02.
#[test]
fn ouvir_com_corpo_rejeitado() {
    let estado = estado_de_teste();
    let r = processar_pedido(&estado, &cmd(CMD_OUVIR, &[0]));
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
    assert_eq!(estado.tor(), 0x00, "nada foi publicado");
}

/// Falha do backend no arranque → 0x0B e o estado fica intacto.
#[test]
fn ouvir_com_backend_a_falhar() {
    let estado = Arc::new(Estado::novo(Box::new(BackendQueFalha)));
    let r = processar_pedido(&estado, &cmd(CMD_OUVIR, &[]));
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_TOR_INDISPONIVEL);
    assert_eq!(estado.tor(), 0x00);
    assert!(!bloquear(&estado.rede).a_escuta);
}

/// Falha de `bind` do servidor P2P → 0x0A, sem escuta nem onion.
#[test]
fn ouvir_com_porta_ocupada() {
    let ocupada = TcpListener::bind(("127.0.0.1", 0)).expect("porta ocupável");
    let porta = ocupada.local_addr().expect("endereço local").port();
    let estado = estado_de_teste();
    let (codigo, mensagem) = estado
        .ouvir_em("127.0.0.1", porta)
        .expect_err("a porta já está vinculada");
    assert_eq!(codigo, ERR_NAO_LIGADO, "{mensagem}");
    assert!(!mensagem.is_empty());
    assert_eq!(estado.tor(), 0x00, "nada foi publicado");
    assert!(!bloquear(&estado.rede).a_escuta);
}

/// Superfície completa do backend que falha (fora do `OUVIR`).
#[test]
fn superficie_do_backend_que_falha() {
    let mut backend = BackendQueFalha;
    assert!(backend.arrancar(9050).is_err(), "arranque falha");
    assert!(backend.ligar("abc.onion").is_err(), "ligação falha");
    assert_eq!(backend.estado(), EstadoTor::Parado);
    assert_eq!(backend.onion(), None, "nunca publicou onion");
    assert!(backend.parar().is_ok(), "parar é indolor");
}

// ----------------------------------------------------------------
// LIGAR
// ----------------------------------------------------------------

/// Todos os corpos malformados do `LIGAR` → 0x02/0x0D.
#[test]
fn ligar_corpos_invalidos() {
    let estado = estado_de_teste();

    let casos: &[(Vec<u8>, u8)] = &[
        // corpo curto (só o byte do modo em falta → nem isso)
        (cmd(CMD_LIGAR, &[]), ERR_PAYLOAD_MALFORMADO),
        // modo desconhecido
        (cmd(CMD_LIGAR, &[0x07]), ERR_DESTINO_INVALIDO),
        // endpoint com comprimento acima do resto
        (cmd(CMD_LIGAR, &[0x00, 0xFF, 0xFF]), ERR_PAYLOAD_MALFORMADO),
        // destino em falta depois de um endpoint vazio
        (cmd(CMD_LIGAR, &[0x00, 0x00, 0x00]), ERR_PAYLOAD_MALFORMADO),
        // chaves públicas incompletas (faltam os 64 bytes)
        (
            cmd(CMD_LIGAR, &[0x00, 0x00, 0x00, 0x00, 0x00]),
            ERR_PAYLOAD_MALFORMADO,
        ),
    ];
    for (corpo, esperado) in casos {
        let r = processar_pedido(&estado, corpo);
        assert_eq!(r[0], ESTADO_ERRO, "{corpo:?}");
        assert_eq!(r[1], *esperado, "{corpo:?}");
    }
}

/// Modo direto exige destino (`.onion`) → 0x0D.
#[test]
fn ligar_exige_destino_no_modo_direto() {
    let estado = estado_de_teste();
    ouvir(&estado);
    let (pub_a, pub_b) = ([1u8; 32], [2u8; 32]);
    let corpo = corpo_ligar(0x00, "", "", &pub_a, &pub_b);
    let r = processar_pedido(&estado, &cmd(CMD_LIGAR, &corpo));
    assert_eq!(r[1], ERR_DESTINO_INVALIDO);
}

/// Sem bootstrap no cliente (sem OUVIR) → 0x0B.
#[test]
fn ligar_sem_bootstrap_do_cliente() {
    let estado = estado_de_teste();
    let corpo = corpo_ligar(0x00, "", ONION_DE_TESTE, &[1u8; 32], &[2u8; 32]);
    let r = processar_pedido(&estado, &cmd(CMD_LIGAR, &corpo));
    let diagnostico = String::from_utf8_lossy(r.get(2..).unwrap_or_default()).into_owned();
    assert_eq!(r[1], ERR_TOR_INDISPONIVEL, "{diagnostico}");
}

/// Destino inexistente no diretório → 0x0D.
#[test]
fn ligar_destino_desconhecido() {
    let estado = estado_de_teste();
    ouvir(&estado);
    // Endereço bem formado mas não registado: o 0x0D tem de vir do
    // `TorFalso`, não da validação de formato.
    let corpo = corpo_ligar(0x00, "", ONION_DESCONHECIDO, &[1u8; 32], &[2u8; 32]);
    let r = processar_pedido(&estado, &cmd(CMD_LIGAR, &corpo));
    assert_eq!(r[1], ERR_DESTINO_INVALIDO);
    assert!(!estado.ligado());
}

/// Modo relay sem endpoint configurado → 0x11.
#[test]
fn ligar_relay_sem_endpoint() {
    let _trava = TRAVA_RELAY.lock().expect("trava do relay nunca envenenada");
    std::env::remove_var("ONYXCHAT_RELAY");
    let estado = estado_de_teste();
    ouvir(&estado);
    // `destino` em relay tem de ser o ID hex da `pub_par` do corpo.
    let corpo = corpo_ligar(0x01, "", &p2p::hex_de_pub(&[2u8; 32]), &[1u8; 32], &[2u8; 32]);
    let r = processar_pedido(&estado, &cmd(CMD_LIGAR, &corpo));
    assert_eq!(r[1], ERR_RELAY, "{}", String::from_utf8_lossy(&r[2..]));
}

/// `$ONYXCHAT_RELAY` com valor alimenta o `endpoint_padrao` do
/// estado (o filtro descarta apenas a cadeia vazia).
#[test]
fn endpoint_relay_do_ambiente() {
    let _trava = TRAVA_RELAY.lock().expect("trava do relay nunca envenenada");

    std::env::set_var("ONYXCHAT_RELAY", "127.0.0.1:9050");
    let estado = estado_de_teste();
    std::env::remove_var("ONYXCHAT_RELAY");
    assert_eq!(
        bloquear(&estado.rede).endpoint_padrao.as_deref(),
        Some("127.0.0.1:9050")
    );

    // Vazio conta como ausente.
    std::env::set_var("ONYXCHAT_RELAY", "");
    let sem_endpoint = estado_de_teste();
    std::env::remove_var("ONYXCHAT_RELAY");
    assert_eq!(bloquear(&sem_endpoint.rede).endpoint_padrao, None);
}

/// Uma segunda `LIGAR` com ligação viva → 0x0F.
#[test]
fn ligar_duas_vezes_rejeitado() {
    let (a, b, (_sa, pa), (_sb, pb)) = par_ligado();
    let onion_b = ouvir(&b);
    let corpo = corpo_ligar(0x00, "", &onion_b, &pa, &pb);
    let r = processar_pedido(&a, &cmd(CMD_LIGAR, &corpo));
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_ESTADO_INVALIDO);
}

/// Transporte relay completo contra um relay TCP falso.
#[test]
fn ligacao_relay_contra_falso_relay() {
    let endpoint = relay_falso();
    let estado = estado_de_teste();
    ouvir(&estado);
    // `destino` tem de corresponder à `pub_par` do corpo, senão a
    // validação rejeita com `0x0D` antes de tocar na rede.
    let destino = p2p::hex_de_pub(&[2u8; 32]);
    let corpo = corpo_ligar(0x01, &endpoint, &destino, &[1u8; 32], &[2u8; 32]);
    let r = processar_pedido(&estado, &cmd(CMD_LIGAR, &corpo));
    assert_eq!(r[0], ESTADO_OK, "{}", String::from_utf8_lossy(&r[2..]));
    assert!(estado.ligado());

    // Enviar espera o `OK` do relay; FECHAR emite o `FECHO`.
    assert_eq!(
        processar_pedido(&estado, &cmd(CMD_ENVIAR, b"envelope"))[0],
        ESTADO_OK
    );
    assert_eq!(
        processar_pedido(&estado, &cmd(CMD_FECHAR, &[]))[0],
        ESTADO_OK
    );
    assert!(!estado.ligado());
}

// ----------------------------------------------------------------
// Matriz do `destino` (F2.1) — `docs/ipc_spec.md` §`destino`
// ----------------------------------------------------------------
//
// O objectivo é provar três coisas distintas:
//   1. modo directo aceita `.onion` e **recusa** ID hex (o daemon não
//      tem discovery — a resolução é do cliente);
//   2. modo relay aceita ID hex **coerente** com `pub_par`, e recusa
//      ID hex de outro par;
//   3. a recusa acontece **antes** de abrir socket.

/// Em modo directo, um `.onion` bem formado é aceite.
#[test]
fn destino_onion_e_aceite_em_modo_directo() {
    let _trava = TRAVA_RELAY.lock().expect("trava do relay nunca envenenada");
    std::env::remove_var("ONYXCHAT_RELAY");
    let pub_par = [7u8; 32];
    let estado = estado_de_teste();
    // Regista o endereço para que o `TorFalso` o resolva.
    ouvir(&estado);
    let r = validar_destino(ModoLigacao::Direto, ONION_DE_TESTE, &pub_par)
        .expect("onion aceite");
    assert_eq!(r, ONION_DE_TESTE, "em directo o destino passa tal e qual");
}

/// Em modo directo, um ID hex é recusado com `0x0D` — e a mensagem
/// diz que a resolução é do cliente, para o utilizador saber onde
/// actuar.
#[test]
fn destino_id_hex_e_recusado_em_modo_directo() {
    let pub_par = [7u8; 32];
    let id = p2p::hex_de_pub(&pub_par);
    let (codigo, mensagem) = validar_destino(ModoLigacao::Direto, &id, &pub_par)
        .expect_err("id hex não pode ser resolvido pelo daemon");
    assert_eq!(codigo, ERR_DESTINO_INVALIDO);
    assert!(
        mensagem.contains("cliente"),
        "a mensagem tem de indicar onde resolver: {mensagem}"
    );
}

/// Destino vazio em modo directo: `0x0D` com a mensagem original.
#[test]
fn destino_vazio_e_recusado_em_modo_directo() {
    let (codigo, _) = validar_destino(ModoLigacao::Direto, "", &[7u8; 32])
        .expect_err("modo directo exige destino");
    assert_eq!(codigo, ERR_DESTINO_INVALIDO);
}

/// Em modo relay, o ID hex **coerente** com `pub_par` é aceite e
/// canonificado (minúsculas).
#[test]
fn destino_id_hex_coerente_e_aceite_em_modo_relay() {
    let pub_par = [0xABu8; 32];
    let id = p2p::hex_de_pub(&pub_par);
    let canonico = validar_destino(ModoLigacao::Relay, &id, &pub_par)
        .expect("id coerente aceite");
    assert_eq!(canonico, id, "o valor canónico é o hex minúsculo da pub");
}

/// Um ID hex em **maiúsculas** é aceite e canonicalizado — o
/// utilizador pode colar em qualquer caixa.
#[test]
fn destino_id_hex_maiusculas_e_canonicalizado() {
    let pub_par = [0xABu8; 32];
    let id = p2p::hex_de_pub(&pub_par).to_uppercase();
    let canonico = validar_destino(ModoLigacao::Relay, &id, &pub_par)
        .expect("maiúsculas são o mesmo ID");
    assert_eq!(canonico, p2p::hex_de_pub(&pub_par));
    assert!(
        canonico.chars().all(|c| !c.is_ascii_uppercase()),
        "a forma canónica tem de ser minúscula: {canonico}"
    );
}

/// **A propriedade que importa:** um ID hex que aponte para outro par
/// é `0x0D`, não uma entrega silenciosa na mailbox errada.
#[test]
fn destino_id_hex_de_outro_par_e_recusado() {
    let pub_par = [1u8; 32];
    let outro = [2u8; 32];
    let (codigo, mensagem) =
        validar_destino(ModoLigacao::Relay, &p2p::hex_de_pub(&outro), &pub_par)
            .expect_err("id de outro par tem de ser recusado");
    assert_eq!(codigo, ERR_DESTINO_INVALIDO);
    assert!(
        mensagem.contains("não corresponde"),
        "a mensagem tem de explicar a divergência: {mensagem}"
    );
}

/// Destino em relay que não é nem `.onion` nem hex: `0x0D` com as
/// duas formas aceites.
#[test]
fn destino_invalido_em_modo_relay() {
    let pub_par = [1u8; 32];
    for mau in ["caixa-do-par", "abc", &"zz".repeat(32), "onion", ".onion"] {
        let erro = validar_destino(ModoLigacao::Relay, mau, &pub_par)
            .err()
            .unwrap_or_else(|| panic!("{mau:?} tem de ser recusado"));
        assert_eq!(
            erro.0,
            ERR_DESTINO_INVALIDO,
            "{mau:?} tem de dar 0x0D, não {:?}",
            erro.0
        );
    }
    // `.onion` continua a ser aceito em relay (é um destino válido
    // para o modo directo e não deve ser rejeitado aqui).
    assert!(
        validar_destino(ModoLigacao::Relay, ONION_DE_TESTE, &pub_par).is_ok(),
        ".onion é aceite em relay"
    );
    // Vazio significa «a mailbox da pub do corpo».
    assert_eq!(
        validar_destino(ModoLigacao::Relay, "", &pub_par).expect("vazio legítimo"),
        String::new()
    );
}

/// A validação corre **antes** de qualquer socket: um pedido com
/// destino incoerente não abre ligação nem toca no `TorFalso`.
#[test]
fn destino_incoerente_nao_abre_ligacao() {
    let _trava = TRAVA_RELAY.lock().expect("trava do relay nunca envenenada");
    std::env::remove_var("ONYXCHAT_RELAY");
    let pub_propria = [1u8; 32];
    let pub_par = [2u8; 32];
    let destino_de_outro = p2p::hex_de_pub(&[9u8; 32]);
    let estado = estado_de_teste();
    let r = processar_pedido(
        &estado,
        &cmd(
            CMD_LIGAR,
            &corpo_ligar(0x01, "", &destino_de_outro, &pub_propria, &pub_par),
        ),
    );
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(
        r[1],
        ERR_DESTINO_INVALIDO,
        "{}",
        String::from_utf8_lossy(&r[2..])
    );
    assert!(
        !estado.ligado(),
        "um pedido recusado não pode deixar o daemon ligado"
    );
}

// ----------------------------------------------------------------
// ENVIAR / RECEBER / FECHAR
// ----------------------------------------------------------------
/// Todos os comandos de rede exigem ligação ativa (0x0A/0x0F).
#[test]
fn comandos_de_rede_sem_ligacao() {
    let estado = estado_de_teste();
    let (seed, _pub) = crypto_core::gerar_identidade();
    let (_, corpo_pedido) = handshake::montar_pedido(&seed);

    let mut aceitar = vec![CMD_ACEITAR_AMIZADE];
    aceitar.extend_from_slice(&seed);
    aceitar.extend_from_slice(&corpo_pedido);

    let casos: &[(Vec<u8>, u8)] = &[
        (cmd(CMD_ENVIAR, b"envelope"), ERR_NAO_LIGADO),
        (cmd(CMD_RECEBER, &0u32.to_le_bytes()), ERR_NAO_LIGADO),
        (cmd(CMD_PEDIR_AMIZADE, &[0u8; 64]), ERR_NAO_LIGADO),
        (cmd(CMD_RECUSAR_AMIZADE, &[0u8; 48]), ERR_NAO_LIGADO),
        (aceitar, ERR_NAO_LIGADO),
        (cmd(CMD_CONFIRMAR_AMIZADE, &[0u8; 209]), ERR_ESTADO_INVALIDO),
    ];
    for (corpo, esperado) in casos {
        let r = processar_pedido(&estado, corpo);
        assert_eq!(r[0], ESTADO_ERRO, "{corpo:?}");
        assert_eq!(r[1], *esperado, "{corpo:?}");
    }

    // FECHAR sem ligação continua a ser idempotente → OK.
    assert_eq!(
        processar_pedido(&estado, &cmd(CMD_FECHAR, &[]))[0],
        ESTADO_OK
    );
    // …mas com corpo → 0x02.
    assert_eq!(
        processar_pedido(&estado, &cmd(CMD_FECHAR, &[0]))[1],
        ERR_PAYLOAD_MALFORMADO
    );
}

/// `RECEBER` com corpo de tamanho errado → 0x02.
#[test]
fn receber_timeout_malformado() {
    let estado = estado_de_teste();
    let r = processar_pedido(&estado, &cmd(CMD_RECEBER, &[1, 2, 3]));
    assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO);
}

/// `RECEBER` ligado mas sem frames → 0x0E dentro do prazo.
#[test]
fn receber_expira_sem_frames() {
    let (a, b, (_sa, _pa), (_sb, _pb)) = par_ligado();
    let inicio = Instant::now();
    let r = receber_frame(&b, 700);
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_SEM_MENSAGEM);
    let decorrido = inicio.elapsed();
    assert!(
        decorrido < Duration::from_secs(3),
        "o prazo do cliente foi ultrapassado: {decorrido:?}"
    );
    // A ligação continua útil depois de um timeout.
    assert!(b.ligado());
    assert_eq!(processar_pedido(&a, &cmd(CMD_ESTADO, &[]))[2], 0x01);
}

/// Mensagem A → B: `ENVIAR`/`RECEBER` com `ESTADO` a ambos os lados.
#[test]
fn fluxo_de_chat_entre_dois_nos() {
    let (a, b, (_sa, _pa), (_sb, _pb)) = par_ligado();

    // ESTADO = tor ‖ ligado ‖ amigos:16 ‖ onion.
    let antes = processar_pedido(&b, &cmd(CMD_ESTADO, &[]));
    assert_eq!(antes[1], 0x02, "B publicou o hidden service");
    assert_eq!(antes[2], 0x01, "B aceitou a ligação de A");
    assert_eq!(u16::from_le_bytes([antes[3], antes[4]]), 0);

    let r = processar_pedido(&a, &cmd(CMD_ENVIAR, b"envelope-qualquer"));
    assert_eq!(r[0], ESTADO_OK);

    // `timeout_ms = 0` → espera indefinida (cobre o prazo infinito).
    let frame = receber_frame(&b, 0);
    let diagnostico = String::from_utf8_lossy(frame.get(2..).unwrap_or_default()).into_owned();
    assert_eq!(frame[0], ESTADO_OK, "{diagnostico}");
    assert_eq!(frame[1], p2p::FRAME_CHAT);
    assert_eq!(&frame[2..], b"envelope-qualquer");

    let depois = processar_pedido(&a, &cmd(CMD_ESTADO, &[]));
    assert_eq!(depois[1], 0x02, "A também está a escuta");
    assert_eq!(depois[2], 0x01, "A tem ligação ativa");
}

/// O fecho do par chega como EOF: `RECEBER`/`ENVIAR` limparam tudo.
#[test]
fn fecho_do_par_mata_a_ligacao() {
    let (a, b, (_sa, _pa), (_sb, _pb)) = par_ligado();

    // A fecha → B vê EOF ao tentar ler (caminho "morta" do RECEBER).
    fechar_e_confirmar(&a, &b);
    // E já não há ligação nenhuma.
    assert_eq!(
        processar_pedido(&b, &cmd(CMD_ENVIAR, b"x"))[1],
        ERR_NAO_LIGADO
    );
}

/// Depois de `FECHAR`, uma nova `LIGAR` volta a ligar os nós.
#[test]
fn fechar_e_volver_a_ligar() {
    let (a, b, (_sa, pa), (_sb, pb)) = par_ligado();
    fechar_e_confirmar(&a, &b);

    ligar_ate(&a, &b, &ouvir(&b), &pa, &pb);
    assert!(a.ligado());
    assert_eq!(
        processar_pedido(&a, &cmd(CMD_ENVIAR, b"de-volta"))[0],
        ESTADO_OK
    );
    let frame = receber_frame(&b, 3_000);
    assert_eq!(&frame[2..], b"de-volta");
}

/// O ramo de erro do `ENVIAR` com ligação já morta limpa o estado.
#[test]
fn enviar_com_ligacao_morta() {
    let (a, b, (_sa, pa), (_sb, pb)) = par_ligado();
    let mut tentativa = || processar_pedido(&b, &cmd(CMD_ENVIAR, b"lixo"));

    // Com a ligação viva: oito envios seguidos sem falhar — só
    // assim se percorre o desfecho "desiste ao 8.º".
    let viva = falha_de_envio(&mut tentativa);
    let diagnostico = String::from_utf8_lossy(viva.get(2..).unwrap_or_default()).into_owned();
    assert_eq!(viva[0], ESTADO_OK, "{diagnostico}");

    // Depois de `FECHAR`, a mesma tentativa acaba por falhar.
    assert_eq!(processar_pedido(&a, &cmd(CMD_FECHAR, &[]))[0], ESTADO_OK);

    // O 1.º write pode ainda ser aceite pelo kernel; o 2.º falha.
    let erro = falha_de_envio(&mut tentativa);
    let diagnostico = String::from_utf8_lossy(erro.get(2..).unwrap_or_default()).into_owned();
    assert_eq!(erro[1], ERR_NAO_LIGADO, "{diagnostico}");
    assert!(!b.ligado(), "a ligação morta foi limpa");
    // O nó A pode reconectar sem limpar nada à mão.
    ligar_ate(&a, &b, &ouvir(&b), &pa, &pb);
    assert!(a.ligado());
}

// ----------------------------------------------------------------
// Handshake (0x09..0x0C)
// ----------------------------------------------------------------

/// Fluxo completo A → B → A pelo protocolo IPC.
#[test]
fn handshake_completo_entre_a_e_b() {
    let (a, b, (seed_a, pub_a), (seed_b, pub_b)) = par_ligado();

    // A pede amizade a B.
    let mut pedir = vec![CMD_PEDIR_AMIZADE];
    pedir.extend_from_slice(&seed_a);
    pedir.extend_from_slice(&pub_b);
    let r = processar_pedido(&a, &pedir);
    assert_eq!(r[0], ESTADO_OK, "{}", String::from_utf8_lossy(&r[2..]));
    assert_eq!(r.len(), 1 + 3 * TAM_PARTE + handshake::TAM_CORPO_PEDIDO);
    let corpo_pedido = r[1 + 3 * TAM_PARTE..].to_vec();

    // B recebe o FRIEND_REQUEST.
    let frame = receber_frame(&b, 3_000);
    assert_eq!(frame[0], ESTADO_OK);
    assert_eq!(frame[1], handshake::TIPO_PEDIDO);
    assert_eq!(&frame[2..], &corpo_pedido[..]);
    assert_eq!(b.amigos(), 0, "nada guardado antes do aceite");

    // B aceita: valida assinatura + anti-replay e envia o ACCEPT.
    let mut aceitar = vec![CMD_ACEITAR_AMIZADE];
    aceitar.extend_from_slice(&seed_b);
    aceitar.extend_from_slice(&corpo_pedido);
    let r_b = processar_pedido(&b, &aceitar);
    let r = &r_b;
    assert_eq!(r[0], ESTADO_OK, "{}", String::from_utf8_lossy(&r[2..]));
    assert_eq!(r.len(), 1 + 6 * TAM_PARTE + handshake::TAM_CORPO_ACEITE);
    let corpo_aceite = r[1 + 6 * TAM_PARTE..].to_vec();
    assert_eq!(b.amigos(), 1);

    // A recebe o FRIEND_ACCEPT.
    let frame = receber_frame(&a, 3_000);
    assert_eq!(frame[0], ESTADO_OK);
    assert_eq!(frame[1], handshake::TIPO_ACEITE);
    assert_eq!(&frame[2..], &corpo_aceite[..]);

    // A confirma e fecha a amizade dos dois lados.
    let mut confirmar = vec![CMD_CONFIRMAR_AMIZADE];
    confirmar.extend_from_slice(&seed_a);
    confirmar.extend_from_slice(&corpo_aceite);
    let r_a = processar_pedido(&a, &confirmar);
    let r = &r_a;
    assert_eq!(r[0], ESTADO_OK, "{}", String::from_utf8_lossy(&r[2..]));
    assert_eq!(r.len(), 1 + 6 * TAM_PARTE);

    // Espelhos das chaves partilhadas, vistos do lado do cliente.
    // O corpo vem **depois** do byte de estado; cortá-lo a partir do
    // zero desalinharia as chaves em um byte e as comparações
    // passariam a comparar coisas diferentes.
    let am_b = amizade_de_corpo(&r_b[1..1 + 6 * TAM_PARTE]).expect("corpo do B");
    let am_a = amizade_de_corpo(&r_a[1..1 + 6 * TAM_PARTE]).expect("corpo do A");
    assert_eq!(am_a.publica, pub_b);
    assert_eq!(am_b.publica, pub_a);
    // O registo guardou as identidades — e só elas.
    assert!(bloquear(&a.amigos).tem(&pub_b), "A regista B");
    assert!(bloquear(&b.amigos).tem(&pub_a), "B regista A");
    assert_eq!(am_a.k1, am_b.k1, "K1 comum");
    assert_eq!(am_a.k5_par, am_b.k5_proprio, "espelhos K5");
    assert_eq!(am_a.k9_par, am_b.k9_proprio, "espelhos K9");
    assert_eq!(am_a.k5_proprio, am_b.k5_par);
    assert_eq!(am_a.k9_proprio, am_b.k9_par);

    // ESTADO reflecte a amizade.
    let e = processar_pedido(&a, &cmd(CMD_ESTADO, &[]));
    assert_eq!(u16::from_le_bytes([e[3], e[4]]), 1);

    // Replay do pedido → nonce repetido (0x0C) e nada muda.
    let r = processar_pedido(&b, &aceitar);
    assert_eq!(r[1], ERR_HANDSHAKE_INVALIDO);
    assert_eq!(b.amigos(), 1);

    // Repetir o CONFIRMAR → já não há pedido pendente (0x0F).
    let r = processar_pedido(&a, &confirmar);
    assert_eq!(r[1], ERR_ESTADO_INVALIDO);
    assert_eq!(a.amigos(), 1);
}

/// Comprimentos errados em todos os comandos de handshake → 0x02.
#[test]
fn handshake_corpos_comprimento_errado() {
    let estado = estado_de_teste();
    let casos: &[(u8, Vec<u8>)] = &[
        (CMD_PEDIR_AMIZADE, vec![0u8; 63]),
        (CMD_ACEITAR_AMIZADE, vec![0u8; 240]),
        (CMD_RECUSAR_AMIZADE, vec![0u8; 47]),
        (CMD_CONFIRMAR_AMIZADE, vec![0u8; 208]),
    ];
    for (comando, corpo) in casos {
        let r = processar_pedido(&estado, &cmd(*comando, corpo));
        assert_eq!(r[0], ESTADO_ERRO, "comando 0x{comando:02x}");
        assert_eq!(r[1], ERR_PAYLOAD_MALFORMADO, "comando 0x{comando:02x}");
    }

    // Comprimento certo mas corpo lixo → 0x0C (a versão nula do
    // corpo de handshake é rejeitada antes da assinatura).
    let r = processar_pedido(&estado, &cmd(CMD_ACEITAR_AMIZADE, &[0u8; 241]));
    assert_eq!(r[1], ERR_HANDSHAKE_INVALIDO);
}

/// `ACEITAR` com pedido válido mas sem ligação → 0x0A (nada muda).
#[test]
fn aceitar_pedido_valido_sem_ligacao() {
    let estado = estado_de_teste();
    let (seed_a, _pub_a) = crypto_core::gerar_identidade();
    let (seed_b, _pub_b) = crypto_core::gerar_identidade();
    let (_, corpo_pedido) = handshake::montar_pedido(&seed_a);

    let mut corpo = vec![CMD_ACEITAR_AMIZADE];
    corpo.extend_from_slice(&seed_b);
    corpo.extend_from_slice(&corpo_pedido);
    let r = processar_pedido(&estado, &corpo);
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_NAO_LIGADO);
    assert_eq!(estado.amigos(), 0, "sem entrega não há amizade");
}

/// `CONFIRMAR` com o aceite de outra identidade → 0x0C.
#[test]
fn confirmar_aceite_de_identidade_errada() {
    let (a, b, (seed_a, _pa), (seed_b, _pb)) = par_ligado();
    let (seed_c, _pc) = crypto_core::gerar_identidade();

    let mut pedir = vec![CMD_PEDIR_AMIZADE];
    pedir.extend_from_slice(&seed_a);
    pedir.extend_from_slice(&_pb);
    assert_eq!(processar_pedido(&a, &pedir)[0], ESTADO_OK);

    // C aceita no lugar de B: o nonce ecoa, mas a identidade não.
    let frame = receber_frame(&b, 3_000);
    let corpo_pedido = &frame[2..];
    let nonce: [u8; TAM_NONCE] = corpo_pedido[129..145].try_into().expect("nonce");
    let k5 = crypto_core::gerar_chave();
    let k9 = crypto_core::gerar_chave();
    let (_aceite, corpo_aceite) = handshake::montar_aceite(&seed_c, &k5, &k9, &nonce);

    let mut confirmar = vec![CMD_CONFIRMAR_AMIZADE];
    confirmar.extend_from_slice(&seed_a);
    confirmar.extend_from_slice(&corpo_aceite);
    let r = processar_pedido(&a, &confirmar);
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_HANDSHAKE_INVALIDO);
    assert_eq!(a.amigos(), 0, "nada guardado com a identidade errada");
    let _ = seed_b;
}

/// Aceite que não responde ao pedido (nonce errado) → o ramo de
/// erro de `confirmar_aceite` devolve 0x0C/0x10 sem tocar na lista.
#[test]
fn confirmar_aceite_sem_resposta_ao_nonce() {
    let (a, b, (seed_a, _pa), (seed_b, _pb)) = par_ligado();

    let mut pedir = vec![CMD_PEDIR_AMIZADE];
    pedir.extend_from_slice(&seed_a);
    pedir.extend_from_slice(&_pb);
    assert_eq!(processar_pedido(&a, &pedir)[0], ESTADO_OK);

    // B aceita, mas ecoa um nonce que nunca foi pedido.
    let frame = receber_frame(&b, 3_000);
    let _nonce = &frame[2 + 129..2 + 145];
    let k5 = crypto_core::gerar_chave();
    let k9 = crypto_core::gerar_chave();
    let (_aceite, corpo_aceite) =
        handshake::montar_aceite(&seed_b, &k5, &k9, &[9u8; TAM_NONCE]);

    let mut confirmar = vec![CMD_CONFIRMAR_AMIZADE];
    confirmar.extend_from_slice(&seed_a);
    confirmar.extend_from_slice(&corpo_aceite);
    let r = processar_pedido(&a, &confirmar);
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_HANDSHAKE_INVALIDO);
    assert_eq!(a.amigos(), 0, "nada guardado sem resposta ao nonce");
}

/// `RECUSAR` envia um `FRIEND_REJECT` assinado pelo nonce pedido.
#[test]
fn recusa_de_amizade_envia_reject() {
    let (a, b, (seed_a, _pa), (seed_b, _pb)) = par_ligado();

    let mut pedir = vec![CMD_PEDIR_AMIZADE];
    pedir.extend_from_slice(&seed_a);
    pedir.extend_from_slice(&_pb);
    assert_eq!(processar_pedido(&a, &pedir)[0], ESTADO_OK);

    let frame = receber_frame(&b, 3_000);
    let nonce = &frame[2 + 129..2 + 145];

    let mut recusar = vec![CMD_RECUSAR_AMIZADE];
    recusar.extend_from_slice(&seed_b);
    recusar.extend_from_slice(nonce);
    let r = processar_pedido(&b, &recusar);
    assert_eq!(r[0], ESTADO_OK, "{}", String::from_utf8_lossy(&r[2..]));
    assert_eq!(r.len(), 1 + handshake::TAM_CORPO_RECUSA);

    let recebido = receber_frame(&a, 3_000);
    assert_eq!(recebido[1], handshake::TIPO_RECUSA);
    assert_eq!(&recebido[2..], &r[1..]);
    assert_eq!(a.amigos(), 0);
    assert_eq!(b.amigos(), 0);
}

/// O ramo de erro do envio em `PEDIR_AMIZADE` limpa a ligação.
#[test]
fn pedir_amizade_com_ligacao_morta() {
    let (a, b, (seed_a, pub_a), (_sb, pub_b)) = par_ligado();
    assert_eq!(processar_pedido(&a, &cmd(CMD_FECHAR, &[]))[0], ESTADO_OK);

    let erro = falha_de_envio(|| {
        let mut corpo = vec![CMD_PEDIR_AMIZADE];
        corpo.extend_from_slice(&seed_a);
        corpo.extend_from_slice(&pub_b);
        processar_pedido(&b, &corpo)
    });
    assert_eq!(erro[1], ERR_NAO_LIGADO);
    assert!(!b.ligado());
    ligar_ate(&a, &b, &ouvir(&b), &pub_a, &pub_b);
}

/// O ramo de erro do envio em `ACEITAR_AMIZADE` (pedido novo a
/// cada tentativa, para nunca esbarrar no anti-replay).
#[test]
fn aceitar_amizade_com_ligacao_morta() {
    let (a, b, (_sa, _pa), (seed_b, _pb)) = par_ligado();
    assert_eq!(processar_pedido(&a, &cmd(CMD_FECHAR, &[]))[0], ESTADO_OK);

    // Identidades novas por tentativa → nonces novos (o anti-replay
    // nunca dispara antes de o envio falhar).
    let erro = falha_de_envio(|| {
        let (seed_x, _pub_x) = crypto_core::gerar_identidade();
        let (_, corpo_pedido) = handshake::montar_pedido(&seed_x);
        let mut corpo = vec![CMD_ACEITAR_AMIZADE];
        corpo.extend_from_slice(&seed_b);
        corpo.extend_from_slice(&corpo_pedido);
        processar_pedido(&b, &corpo)
    });
    assert_eq!(erro[1], ERR_NAO_LIGADO);
    assert!(!b.ligado());
}

/// O ramo de erro do envio em `RECUSAR_AMIZADE` limpa a ligação.
#[test]
fn recusar_amizade_com_ligacao_morta() {
    let (a, b, (_sa, _pa), (seed_b, _pb)) = par_ligado();
    assert_eq!(processar_pedido(&a, &cmd(CMD_FECHAR, &[]))[0], ESTADO_OK);

    let erro = falha_de_envio(|| {
        let mut corpo = vec![CMD_RECUSAR_AMIZADE];
        corpo.extend_from_slice(&seed_b);
        corpo.extend_from_slice(&[7u8; TAM_NONCE]);
        processar_pedido(&b, &corpo)
    });
    assert_eq!(erro[1], ERR_NAO_LIGADO);
    assert!(!b.ligado());
}

/// `falha_de_envio` só se rende ao fim de 8 tentativas: aqui
/// nenhuma responde com erro, logo a última resposta devolve-se.
#[test]
fn falha_de_envio_renuncia_as_oito_tentativas() {
    let mut tentativas = 0usize;
    let resposta = falha_de_envio(|| {
        tentativas += 1;
        vec![ESTADO_OK, 0x00, b'o', b'k']
    });
    assert_eq!(tentativas, 8, "o limite de tentativas é 8");
    assert_eq!(resposta[0], ESTADO_OK, "devolve a última resposta");
}

// ----------------------------------------------------------------
// Classificação de erros
// ----------------------------------------------------------------

/// `limpar_se_morta` só limpa quando o stream deixou de servir.
#[test]
fn limpar_se_morta_classifica_erros() {
    assert!(Estado::limpar_se_morta(&ErroP2P::Fechada, true));
    assert!(Estado::limpar_se_morta(
        &ErroP2P::Io("broken pipe".into()),
        true
    ));
    assert!(Estado::limpar_se_morta(&ErroP2P::Desalinhado, true));
    assert!(!Estado::limpar_se_morta(&ErroP2P::TempoEsgotado, true));
    assert!(!Estado::limpar_se_morta(&ErroP2P::RateLimit, true));
    assert!(!Estado::limpar_se_morta(&ErroP2P::ComprimentoZero, true));
    assert!(!Estado::limpar_se_morta(&ErroP2P::SemLigacao, true));
    assert!(!Estado::limpar_se_morta(&ErroP2P::SemRelay, true));
    // Comprimento acima do limite: no envio o stream fica íntegro
    // (nada foi escrito); na receção o stream fica desalinhado e a
    // ligação morre (o corpo anunciado nunca é lido).
    let gigante = ErroP2P::PayloadGrande {
        obtido: 0xFFFF_FFFF,
    };
    assert!(!Estado::limpar_se_morta(&gigante, false), "envio não mata");
    assert!(Estado::limpar_se_morta(&gigante, true), "receção mata");
}

/// Comprimento acima do limite chegado por receção → `0x09` **e**
/// a ligação é derrubada (stream desalinhado; o corpo anunciado
/// nunca é lido — regra da fronteira P2P, `docs/p2p.md`).
#[test]
fn receber_comprimento_gigante_derruba_a_ligacao() {
    let estado = estado_de_teste();
    // Par TCP loopback: um lado é a nossa ligação P2P, o outro é
    // o par que escreve um cabeçalho malicioso (sem corpo).
    let ouvinte = TcpListener::bind("127.0.0.1:0").expect("bind");
    let endereco = ouvinte.local_addr().expect("addr");
    let nosso = TcpStream::connect(endereco).expect("liga");
    let (par, _) = ouvinte.accept().expect("accept");
    {
        let mut rede = bloquear(&estado.rede);
        rede.ligacao = Some(p2p::LigacaoP2P::Direta(Box::new(p2p::LigacaoDireta::nova(
            nosso,
        ))));
    }
    estado.ligado.store(true, Ordering::Relaxed);

    let gigante = (1u32 + p2p::TAM_MAX_CORPO as u32 + 1).to_le_bytes();
    let mut par = par;
    par.write_all(&gigante).expect("escreve cabeçalho");

    let r = processar_receber(&estado, &2_000u32.to_le_bytes());
    assert_eq!(r[0], ESTADO_ERRO);
    assert_eq!(r[1], ERR_PAYLOAD_GRANDE_DEMAIS, "0x09 PayloadGrandeDemais");
    assert!(
        bloquear(&estado.rede).ligacao.is_none(),
        "a ligação desalinhada foi derrubada"
    );
    assert!(!estado.ligado.load(Ordering::Relaxed), "espelho ligado = 0");
}

// ----------------------------------------------------------------
// Zeroização (Resumo §22 — `docs/key_management.md`)
// ----------------------------------------------------------------

/// `PedidoPendente::limpar` (usado pelo `Drop`) apaga o corpo de
/// 209 B que transporta K1/K5/K9; a identidade pública fica.
#[test]
fn pedido_pendente_zeroiza_corpo() {
    let mut pendente = PedidoPendente {
        publica: [0x77; TAM_PUB],
        corpo: vec![0xAB; handshake::TAM_CORPO_PEDIDO],
    };
    pendente.limpar();
    assert!(
        pendente.corpo.iter().all(|&b| b == 0),
        "corpo de 209 B (com chaves) apagado"
    );
    assert_eq!(pendente.publica, [0x77u8; TAM_PUB], "publica fica");
    }

    /// O registo anti-replay sobrevive a um `Estado::novo`.
    ///
    /// Este é o teste de integração da fase B3, e é o que fecha o buraco
    /// de ponta a ponta. `anti_replay::tests::o_registo_sobrevive_a_um_
    /// reinicio` prova que o *registo* persiste; este prova que o
    /// *daemon* o carrega. São coisas diferentes, e a segunda é a que
    /// interessa: um registo que persiste num ficheiro que ninguém lê é
    /// a mesma implementação de antes, com mais código.
    ///
    /// `ONYXCHAT_ESTADO` é apontado para um directório temporário porque
    /// o caminho de estado é resolvido em `Estado::novo` — sem isto, o
    /// teste escreveria no estado real do utilizador.
    #[test]
    fn o_registo_anti_replay_sobrevive_a_um_reinicio_do_daemon() {
        let dir = std::env::temp_dir().join(format!("onyxchat-estado-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("directório temporário");
        // `set_var` é inseguro em Rust 2024; o binário de teste é
        // single-threaded neste ponto do caso, e o aviso do compilador
        // está documentado aqui em vez de silenciado.
        #[allow(unused_unsafe)]
        unsafe { std::env::set_var("ONYXCHAT_ESTADO", &dir) };

        let caminho = caminho_anti_replay();
        assert!(
            caminho.starts_with(&dir),
            "o caminho de estado não seguiu ONYXCHAT_ESTADO: {:?}",
            caminho
        );

        // Instância 1: um nonce entra no registo.
        {
            let estado = estado_de_teste();
            let mut registo = bloquear(&estado.nonces1);
            let nonce = [0x2Bu8; 12];
            assert!(registo.reservar(&nonce), "a primeira vez tem de passar");
            registo.gravar_para(&caminho_anti_replay());
        }

        // Instância 2: recria o estado do zero, como um arranque.
        let estado2 = estado_de_teste();
        {
            let mut registo = bloquear(&estado2.nonces1);
            assert_eq!(
                registo.quantidade(),
                1,
                "o novo Estado não carregou o registo de disco"
            );
            assert!(
                !registo.reservar(&[0x2Bu8; 12]),
                "REPLAY ACEITE depois do reinício do daemon"
            );
        }

        unsafe { std::env::remove_var("ONYXCHAT_ESTADO") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `ONYXCHAT_ESTADO` tem precedência sobre o XDG.
    ///
    /// O caminho de estado decide onde vive a informação de tráfego. Um
    /// teste (ou um utilizador com vários perfis) precisa de o apontar
    /// para outro sítio, e essa precedência é o que permite — sem código
    /// nem recompilação.
    #[test]
    fn o_caminho_de_estado_segue_o_ambiente() {
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var("ONYXCHAT_ESTADO", "/tmp/onyxchat-teste-estado");
            assert_eq!(
                caminho_estado(),
                std::path::PathBuf::from("/tmp/onyxchat-teste-estado")
            );
            std::env::remove_var("ONYXCHAT_ESTADO");
        }
    }

    /// A cadeia de precedência do caminho de estado, um degrau de cada vez.
    ///
    /// O teste acima fixa o primeiro degrau (`ONYXCHAT_ESTADO`) e pára
    /// aí. Os dois seguintes — `$XDG_STATE_HOME/onyxchat` e
    /// `$HOME/.local/state/onyxchat` — eram linhas que nenhum teste
    /// executava, e são a parte da cadeia que decide **onde o ficheiro
    /// de anti-replay é criado** numa máquina normal, em que ninguém
    /// põe `ONYXCHAT_ESTADO`.
    ///
    /// Um degrau que não funciona tem uma consequência silenciosa: o
    /// registo vai parar ao sítio errado, e o anti-replay parece não
    /// funcionar porque cada arranque lê um ficheiro diferente.
    #[test]
    fn o_caminho_de_estado_desce_a_cadeia_de_precedencia() {
        #[allow(unused_unsafe)]
        unsafe {
            let guardar = |nome: &str| std::env::var(nome).ok();

            let estado_anterior = guardar("ONYXCHAT_ESTADO");
            let xdg_anterior = guardar("XDG_STATE_HOME");
            let home_anterior = guardar("HOME");

            // (a) `ONYXCHAT_ESTADO` ganha a tudo.
            std::env::set_var("ONYXCHAT_ESTADO", "/tmp/onyxchat-estado-prioritario");
            std::env::set_var("XDG_STATE_HOME", "/tmp/xdg-secundario");
            assert_eq!(
                caminho_estado(),
                std::path::PathBuf::from("/tmp/onyxchat-estado-prioritario")
            );

            // (b) sem o primeiro, o XDG decide — e `onyxchat` é acrescentado.
            std::env::remove_var("ONYXCHAT_ESTADO");
            assert_eq!(
                caminho_estado(),
                std::path::PathBuf::from("/tmp/xdg-secundario").join("onyxchat")
            );

            // (c) uma variável presente mas **vazia** não decide.
            //
            // `ONYXCHAT_ESTADO=""` é o caso que o primeiro `if !c.is_empty()`
            // trata, e sem ele o caminho seria `""` — que o registo
            // abriria como ficheiro sem directório, na raiz do
            // sistema de ficheiros. `EACCES` para quem não é root, e uma
            // falha que não menciona a variável que a causou.
            std::env::remove_var("XDG_STATE_HOME");
            std::env::set_var("ONYXCHAT_ESTADO", "");
            std::env::set_var("HOME", "/home/testador");
            assert_eq!(
                caminho_estado(),
                std::path::PathBuf::from("/home/testador")
                    .join(".local")
                    .join("state")
                    .join("onyxchat"),
                "ONYXCHAT_ESTADO vazia não pode decidir o caminho"
            );

            // (d) `XDG_STATE_HOME` vazia também não decide.
            //
            // `XDG_STATE_HOME=""` é o degrau intermédio da mesma cadeia,
            // e a especificação XDG diz que uma variável definida como
            // vazia conta como não definida. Sem este `if`, o caminho
            // seria `onyxchat` relativo ao directório de trabalho — que
            // muda de sítio conforme quem lançou o daemon.
            std::env::set_var("ONYXCHAT_ESTADO", "");
            std::env::set_var("XDG_STATE_HOME", "");
            assert_eq!(
                caminho_estado(),
                std::path::PathBuf::from("/home/testador")
                    .join(".local")
                    .join("state")
                    .join("onyxchat"),
                "XDG_STATE_HOME vazia não pode decidir o caminho"
            );
            assert_ne!(
                caminho_estado(),
                std::path::PathBuf::from("onyxchat"),
                "uma variável vazia não pode produzir um caminho relativo"
            );

            // (e) sem nenhum dos dois, o HOME decide.
            std::env::remove_var("XDG_STATE_HOME");
            std::env::remove_var("ONYXCHAT_ESTADO");
            std::env::set_var("HOME", "/home/testador");
            assert_eq!(
                caminho_estado(),
                std::path::PathBuf::from("/home/testador")
                    .join(".local")
                    .join("state")
                    .join("onyxchat")
            );

            // Restaurar o ambiente: um teste que muda `HOME` e não o
            // devolve muda a máquina de quem vem a seguir.
            for (nome, valor) in [
                ("ONYXCHAT_ESTADO", estado_anterior),
                ("XDG_STATE_HOME", xdg_anterior),
                ("HOME", home_anterior),
            ] {
                match valor {
                    Some(v) => std::env::set_var(nome, v),
                    None => std::env::remove_var(nome),
                }
            }
        }
    }

    /// `ONYXCHAT_LIGACOES_MAX` é lido, validado e recusado sem crashar.
    ///
    /// O valor é uma protecção — o tecto de atendimentos em curso — e
    /// não uma pré-condição. Um valor mal formado tem de cair no
    /// omissão e deixar o daemon arrancar, porque o alternativa é não
    /// arrancar por causa de uma variável de ambiente, que é uma falha
    /// pior do que a limitação que ela queria ajustar.
    #[test]
    fn o_limite_de_ligacoes_segue_o_ambiente() {
        #[allow(unused_unsafe)]
        unsafe {
            let anterior = std::env::var("ONYXCHAT_LIGACOES_MAX").ok();

            // Sem variável: o omissão.
            std::env::remove_var("ONYXCHAT_LIGACOES_MAX");
            assert_eq!(limite_ligacoes(), LIGACOES_MAX);

            // Valor válido — e com espaços à volta, que o `trim` trata.
            std::env::set_var("ONYXCHAT_LIGACOES_MAX", "  7  ");
            assert_eq!(limite_ligacoes(), 7);

            // Valor mal formado: o omissão, não um panic.
            std::env::set_var("ONYXCHAT_LIGACOES_MAX", "muitos");
            assert_eq!(limite_ligacoes(), LIGACOES_MAX);

            // Zero: recusado pelo `.filter(|n| *n > 0)`.
            //
            // Um tecto de zero seria um daemon que não atende ninguém,
            // e é o valor que um utilizador obtém de uma variável que
            // ficou a zero por engano. Cai no omissão.
            std::env::set_var("ONYXCHAT_LIGACOES_MAX", "0");
            assert_eq!(limite_ligacoes(), LIGACOES_MAX);

            // Negativo não é `usize`: o parse falha, e o omissão fica.
            std::env::set_var("ONYXCHAT_LIGACOES_MAX", "-1");
            assert_eq!(limite_ligacoes(), LIGACOES_MAX);

            // Variável vazia: o `Some(v) if !v.is_empty()` não casa, e o
            // omissão fica.
            std::env::set_var("ONYXCHAT_LIGACOES_MAX", "");
            assert_eq!(limite_ligacoes(), LIGACOES_MAX);

            match anterior {
                Some(v) => std::env::set_var("ONYXCHAT_LIGACOES_MAX", v),
                None => std::env::remove_var("ONYXCHAT_LIGACOES_MAX"),
            }
        }
    }
