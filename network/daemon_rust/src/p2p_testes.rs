// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// p2p_testes.rs — os testes de `p2p`, num ficheiro próprio
// ---------------------------------------------------------------------
// Ver a nota em `p2p.rs`, à altura do `mod testes`.
//
// A mesma razão que em `ipc.rs`: `p2p.rs` tinha 1053 linhas de produção
// e 1319 de asserções — mais de metade do ficheiro era teste. Ler a
// máquina de estados de uma ligação (directo, relay, handshake,
// anti-replay, recepção por lotes) sem atravessar um ficheiro de testes
// é a diferença entre auditar o protocolo e lê-lo.
//
// O que exige acesso privado — `ligacao_de_teste`, `TorFalso`,
// `relay_de_teste`, `ler_stderr` — impede que isto seja um `tests/`
// de integração sem tornar o interno público. Filho de `p2p`, vê o
// privado do pai sem alargar nada.
//
// Nada mudou de sítio funcional: os mesmos 55 testes.
// =====================================================================
use super::*;
use crate::tor::{EstadoTor, TorFalso};
use std::io::Cursor;
use std::net::TcpStream as StdTcpStream;
use std::sync::mpsc;
use std::thread;

// ----------------------------------------------------------------
// Utilitários de teste
// ----------------------------------------------------------------

/// Passeio de frames sobre um par de sockets loopback.
fn par_de_sockets() -> (LigacaoDireta, LigacaoDireta) {
    let servidor = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = servidor.local_addr().expect("addr").port();
    let cliente = StdTcpStream::connect(("127.0.0.1", porta)).expect("connect");
    let (lado_servidor, _) = servidor.accept().expect("accept");
    (
        LigacaoDireta::nova(cliente),
        LigacaoDireta::nova(lado_servidor),
    )
}

/// Relay mínimo que responde `OK` e regista o que recebeu.
///
/// Devolve o endpoint e um canal com os frames `ENVIAR` observados.
fn relay_de_teste() -> (String, mpsc::Receiver<Vec<u8>>) {
    let servidor = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = servidor.local_addr().expect("addr").port();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        if let Ok((mut fluxo, _)) = servidor.accept() {
            // Falha de escrita/leitura termina o atendimento (mesmo
            // comportamento do antigo `break`, sem ramo inalcançável).
            let _ = atender_relay_de_teste(&mut fluxo, &tx);
        }
    });
    (format!("127.0.0.1:{porta}"), rx)
}

/// Atende um cliente do relay de teste até ao `FECHO`/EOF.
fn atender_relay_de_teste(
    fluxo: &mut TcpStream,
    tx: &mpsc::Sender<Vec<u8>>,
) -> Result<(), ErroP2P> {
    // Subscrição: responde OK.
    let _sub = ler_frame(fluxo)?;
    escrever_frame(fluxo, RELAY_OK, &[])?;
    while let Ok(frame) = ler_frame(fluxo) {
        match frame.tipo {
            RELAY_ENVIAR => {
                let _ = tx.send(frame.corpo.clone());
                // Confirma ao emissor.
                escrever_frame(fluxo, RELAY_OK, &[])?;
            }
            RELAY_FECHO => break,
            // Encomendas especiais do teste: entregar frame.
            0x99 => escrever_frame(fluxo, RELAY_CAIXA, &frame.corpo)?,
            // 0x98 = entrega com ack tardio (OK antes do CAIXA).
            0x98 => {
                escrever_frame(fluxo, RELAY_OK, &[])?;
                escrever_frame(fluxo, RELAY_CAIXA, &frame.corpo)?;
            }
            _ => escrever_frame(fluxo, RELAY_ERRO, b"\x06comando")?,
        }
    }
    Ok(())
}

/// Servidor que entrega um `CAIXA` **antes** do `OK` da subscrição.
fn atender_caixa_prematura(fluxo: &mut TcpStream) -> Result<(), ErroP2P> {
    let _sub = ler_frame(fluxo)?; // SUBSCREVER
    let mut entrega = vec![FRAME_CHAT];
    entrega.extend_from_slice(b"prematura");
    escrever_frame(fluxo, RELAY_CAIXA, &entrega)?; // push prematuro
    escrever_frame(fluxo, RELAY_OK, &[])?;
    // Segura a ligação aberta até o cliente mandar `FECHO`/EOF.
    while ler_frame(fluxo).is_ok() {}
    Ok(())
}

/// Servidor que manda um tipo de frame desconhecido depois do `OK`.
fn atender_tipo_estranho(fluxo: &mut TcpStream) -> Result<(), ErroP2P> {
    let _sub = ler_frame(fluxo)?; // SUBSCREVER
    escrever_frame(fluxo, RELAY_OK, &[])?;
    escrever_frame(fluxo, 0x77, b"?")?;
    // Segura a ligação aberta até o cliente mandar `FECHO`/EOF.
    while ler_frame(fluxo).is_ok() {}
    Ok(())
}

// ----------------------------------------------------------------
// Frame + enquadramento
// ----------------------------------------------------------------

/// Roundtrip enquadrar → ler (com corpo vazio e não vazio).
#[test]
fn frame_roundtrip() {
    for corpo in [Vec::new(), b"Ola mundo!".to_vec(), vec![0u8; 1000]] {
        let pacote = enquadrar(FRAME_CHAT, &corpo).expect("enquadra");
        let mut cursor = Cursor::new(pacote);
        let lido = ler_frame(&mut cursor).expect("lê");
        assert_eq!(lido.tipo, FRAME_CHAT);
        assert_eq!(lido.corpo, corpo);
        assert_eq!(cursor.position() as usize, cursor.get_ref().len());
    }
}

/// Corpo acima do limite → `PayloadGrande` (0x09).
///
/// Este teste usava `[0u8; TAM_MAX_CORPO + 1]` — um array na
/// **pilha**, de 16 MiB com o limite antigo, contra um `ulimit -s`
/// de 8 MiB. O resultado não era uma falha de teste: era um
/// *stack overflow* que abortava o binário inteiro e levava
/// contigo todos os outros testes do crate.
///
/// Passa a alocar no heap (`vec!`), o que é a diferença entre
/// «o limite foi excedido» e «o processo morreu».
#[test]
fn frame_grande_rejeitado() {
    let limite = limite_corpo();
    let grande = vec![0u8; limite + 1];
    let erro = enquadrar(FRAME_CHAT, &grande).expect_err("grande");
    assert_eq!(erro.para_ipc(), 0x09);
    // A mensagem de erro tem de reportar o limite que foi aplicado,
    // senão o utilizador acrescentaria bytes para «resolver» o problema
    // errado — a lição do literal 240 vs 241 que `ipc.rs` documenta.
    assert!(
        erro.to_string().contains(&limite.to_string()),
        "erro não reporta o limite em vigor: {erro}"
    );
    // Caso de bordo: exatamente no limite é aceite.
    let no_limite = vec![0u8; limite];
    assert!(enquadrar(FRAME_CHAT, &no_limite).is_ok());
}

/// Comprimento zero e comprimento acima do limite no stream.
#[test]
fn cabecalhos_invalidos() {
    // comprimento = 0 → ComprimentoZero (0x02).
    let mut cursor = Cursor::new(vec![0u8, 0, 0, 0, 0x01]);
    assert_eq!(
        ler_frame(&mut cursor).expect_err("zero"),
        ErroP2P::ComprimentoZero
    );
    // comprimento = 1 + 16 MiB + 1 → PayloadGrande (0x09).
    let gigante = (1 + TAM_MAX_CORPO + 1) as u32;
    let mut pacote = gigante.to_le_bytes().to_vec();
    pacote.push(FRAME_CHAT);
    let erro = ler_frame(&mut Cursor::new(pacote)).expect_err("gigante");
    assert_eq!(erro.para_ipc(), 0x09);
}

/// EOF no início → `Fechada`; EOF a meio → `Desalinhado`.
#[test]
fn eof_limpo_e_truncado() {
    let erro = ler_frame(&mut Cursor::new(Vec::new())).expect_err("vazio");
    assert_eq!(erro, ErroP2P::Fechada);
    assert_eq!(erro.para_ipc(), 0x0A);

    // Cabeçalho de 2 bytes: lê o primeiro, falha no resto.
    let erro = ler_frame(&mut Cursor::new(vec![4u8, 0])).expect_err("trunc");
    assert_eq!(erro, ErroP2P::Desalinhado);
    assert_eq!(erro.para_ipc(), 0x0A);

    // Cabeçalho completo mas corpo truncado.
    let erro = ler_frame(&mut Cursor::new(vec![5u8, 0, 0, 0, 0x01])).expect_err("sem corpo");
    assert_eq!(erro, ErroP2P::Desalinhado);
}

/// `escrever_frame` produz exatamente o enquadramento esperado.
#[test]
fn escrever_frame_enquadramento_exato() {
    let mut saida = Vec::new();
    escrever_frame(&mut saida, FRAME_PING, &[]).expect("escreve");
    assert_eq!(saida, vec![1u8, 0, 0, 0, FRAME_PING]);
    // Falha de escrita (só escrita) → Io.
    struct EscritorFalho;
    impl Write for EscritorFalho {
        fn write(&mut self, _b: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "tapado"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // O `flush` da impl nunca é alcançado pela falha de `write`;
    // invoca-o à mão para o ramo `Ok(())` não ficar por cobrir.
    let mut escritor = EscritorFalho;
    assert!(Write::flush(&mut escritor).is_ok());
    let erro = escrever_frame(&mut escritor, FRAME_CHAT, b"x").expect_err("falha");
    assert_eq!(erro.para_ipc(), 0x0A);
    assert!(erro.to_string().contains("I/O"));
}

// ----------------------------------------------------------------
// Identidade/relay helpers
// ----------------------------------------------------------------

/// Vectores conhecidos de `caixa_de_pub` (contrato docs/relay.md).
#[test]
fn vectores_da_caixa() {
    let zeros = caixa_de_pub(&[0u8; 32]);
    assert_eq!(
        zeros,
        [
            0xa3, 0xb5, 0x53, 0x75, 0x1a, 0xf2, 0x6f, 0x59, 0xa3, 0x09, 0x94, 0xe0, 0x9b, 0xc6,
            0x88, 0xa1, 0xfd, 0x5f, 0xcf, 0x70, 0xcc, 0x40, 0x20, 0xc8, 0x79, 0xb0, 0xff, 0xd8,
            0x68, 0xfc, 0x51, 0xa8,
        ]
    );
    let sequencial: [u8; 32] = std::array::from_fn(|i| i as u8);
    let outra = caixa_de_pub(&sequencial);
    assert_ne!(outra, zeros, "hashes distintos para pubs distintas");
    assert_eq!(
        outra[0], 0xa2,
        "vector conhecido (00..1f) — trava o domínio de derivação"
    );
}

/// `pub_de_hex` aceita 64 hex e rejeita o resto.
#[test]
fn hex_de_pub() {
    let hex = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    let publica = pub_de_hex(hex).expect("hex válido");
    assert_eq!(publica[0], 0x00);
    assert_eq!(publica[31], 0x1f);
    // Caixa alta também é aceite (hex é case-insensitive).
    assert_eq!(pub_de_hex(&hex.to_uppercase()), Some(publica));
    // Rejeições: tamanho, caracteres e vazio.
    assert_eq!(pub_de_hex(""), None);
    assert_eq!(pub_de_hex(&"00".repeat(31)), None);
    assert_eq!(pub_de_hex(&"zz".repeat(32)), None);
    assert_eq!(pub_de_hex(&(hex.to_string() + "00")), None);
}

/// Modo de ligação: 0x00/0x01 válidos, resto rejeitado.
#[test]
fn modo_de_ligacao() {
    assert_eq!(ModoLigacao::de_ipc(0x00), Ok(ModoLigacao::Direto));
    assert_eq!(ModoLigacao::de_ipc(0x01), Ok(ModoLigacao::Relay));
    let erro = ModoLigacao::de_ipc(0x02).expect_err("inválido");
    assert_eq!(erro.para_ipc(), 0x0D);
}

/// Erros `0x81` do relay: com/sem código e com mensagem.
#[test]
fn descrever_erros_do_relay() {
    assert_eq!(descrever_erro_relay(&[]), "erro sem código");
    assert_eq!(descrever_erro_relay(&[0x04]), "erro 0x04");
    assert_eq!(descrever_erro_relay(&[0x05, b'o', b'k']), "erro 0x05: ok");
    // Mensagem não-UTF-8 nunca derruba o parser.
    //
    // O `\u{FFFD}` está escrito como escape e não como o carácter
    // literal: é o que `String::from_utf8_lossy` produz para um byte
    // inválido, e a asserção tem de ser exacta — mas escrever o
    // carácter no ficheiro punha um símbolo de outro sistema de escrita
    // no código, e a auditoria de alfabeto passava a accuse um ficheiro
    // que está correcto. O escape diz a mesma coisa sem o problema.
    assert_eq!(descrever_erro_relay(&[0x01, 0xFF]), "erro 0x01: \u{FFFD}");
}

// ----------------------------------------------------------------
// Rate-limit
// ----------------------------------------------------------------

/// Acima de `LIMITE_FRAMES` na mesma janela → `RateLimit` (0x10).
#[test]
fn rate_limit_por_ligacao() {
    let (mut a, mut b) = par_de_sockets();
    for i in 0..LIMITE_FRAMES {
        a.enviar(FRAME_CHAT, &[i as u8; 8]).expect("envia");
        let r = b.receber(None).expect("recebe");
        assert_eq!(r.tipo, FRAME_CHAT);
    }
    let erro = a.enviar(FRAME_CHAT, b"demais").expect_err("rate-limit");
    assert_eq!(erro, ErroP2P::RateLimit);
    assert_eq!(erro.para_ipc(), 0x10);
    assert!(erro.to_string().contains("rate-limit"));
}

// ----------------------------------------------------------------
// Servidor + ligações diretas
// ----------------------------------------------------------------

/// Servidor aceita, exchange de frames e EOF limpo.
#[test]
fn servidor_e_troca_de_frames() {
    let servidor = ServidorP2P::abrir("127.0.0.1", 0).expect("abre");
    assert!(servidor.porta() > 0);

    let mut cliente = StdTcpStream::connect(("127.0.0.1", servidor.porta())).expect("liga");
    let mut lado_servidor = servidor.aceitar().expect("aceita");

    // Cliente → servidor.
    use std::io::Write as _;
    cliente
        .write_all(&enquadrar(FRAME_FRIEND_REQUEST, b"pedido").expect("q"))
        .expect("envia");
    let frame = lado_servidor.receber(None).expect("recebe");
    assert_eq!(frame.tipo, FRAME_FRIEND_REQUEST);
    assert_eq!(frame.corpo, b"pedido");
    assert!(lado_servidor.endereco_local().is_some());

    // Servidor → cliente (keepalive roundtrip).
    cliente
        .write_all(&enquadrar(FRAME_PONG, &[]).expect("q"))
        .expect("escreve pong");
    let pong = lado_servidor.receber(None).expect("lê pong");
    assert_eq!(pong.tipo, FRAME_PONG);
    lado_servidor.enviar(FRAME_CHAT, b"r").expect("responde");

    use std::io::Read as _;
    let esperado = enquadrar(FRAME_CHAT, b"r").expect("enquadra");
    let mut buf = vec![0u8; esperado.len()];
    cliente.read_exact(&mut buf).expect("lê");
    assert_eq!(buf, esperado);
}

/// Timeout sem dados → `TempoEsgotado` (0x0E) e ligação útil.
#[test]
fn timeout_de_recebimento() {
    let (mut a, _b) = par_de_sockets();
    let erro = a
        .receber(Some(Duration::from_millis(50)))
        .expect_err("expira");
    assert_eq!(erro, ErroP2P::TempoEsgotado);
    assert_eq!(erro.para_ipc(), 0x0E);
    assert!(erro.to_string().contains("expirou"));
    // A ligação continua utilizável: frames seguintes chegam.
    a.enviar(FRAME_PING, b"ainda vivo").expect("envia");
    // (o outro lado só lê mais tarde — aqui valida-se o caminho puro)
}

/// PING/PONG via `ping()` — keepalive com resposta esperada.
#[test]
fn ping_com_pong() {
    let (mut a, mut b) = par_de_sockets();
    // B responde PONG assim que lê PING.
    let esperador = thread::spawn(move || {
        let recebido = b.receber(None).expect("lê ping");
        assert_eq!(recebido.tipo, FRAME_PING);
        b.enviar(FRAME_PONG, &[]).expect("pong");
        b
    });
    a.ping().expect("ping ok");
    drop(esperador.join().expect("thread"));
}

/// Resposta que não é PONG → erro legível (0x11).
#[test]
fn ping_com_resposta_errada() {
    let (mut a, mut b) = par_de_sockets();
    let esperador = thread::spawn(move || {
        let _ = b.receber(None).expect("lê");
        b.enviar(FRAME_CHAT, b"nao e pong").expect("responde");
        b
    });
    let erro = a.ping().expect_err("tem de falhar");
    assert_eq!(erro.para_ipc(), 0x11);
    assert!(erro.to_string().contains("0x01"));
    drop(esperador.join().expect("thread"));
}

// ----------------------------------------------------------------
// Relay
// ----------------------------------------------------------------

/// Subscrição + envio com confirmação no relay de teste.
#[test]
fn relay_subscreve_e_envia() {
    let (endpoint, recebidos) = relay_de_teste();
    let caixa_propria = caixa_de_pub(&[1u8; 32]);
    let caixa_par = caixa_de_pub(&[2u8; 32]);
    let mut ligacao = LigacaoRelay::abrir(&endpoint, caixa_propria, caixa_par).expect("abre");

    ligacao.enviar(FRAME_CHAT, b"envelope").expect("envia");
    let pacote = recebidos.recv().expect("relay viu");
    // mailbox(32) ‖ tipo ‖ corpo.
    assert_eq!(&pacote[..32], &caixa_par);
    assert_eq!(pacote[32], FRAME_CHAT);
    assert_eq!(&pacote[33..], b"envelope");

    // Fecho limpo envia FECHO (o servidor de teste termina).
    ligacao.fechar();
}

/// `CAIXA` entregue em push chega ao chamador como frame interno.
#[test]
fn relay_recebe_push() {
    let (endpoint, _recebidos) = relay_de_teste();
    let mut ligacao = LigacaoRelay::abrir(
        &endpoint,
        caixa_de_pub(&[3u8; 32]),
        caixa_de_pub(&[4u8; 32]),
    )
    .expect("abre");

    // Encomenda ao stub: entregar FRAME_CHAT "ola". O corpo do
    // CAIXA é `tipo ‖ corpo` (o relay já removeu a mailbox).
    let mut entrega = vec![FRAME_CHAT];
    entrega.extend_from_slice(b"ola");
    escrever_frame(&mut ligacao.fluxo, 0x99, &entrega).expect("pedido");
    ligacao
        .fluxo
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    let caixa = ler_frame(&mut ligacao.fluxo).expect("CAIXA");
    assert_eq!(caixa.tipo, RELAY_CAIXA);
    ligacao.enfileirar_caixa(caixa).expect("enfileira");

    let frame = ligacao.receber(None).expect("frame interno");
    assert_eq!(frame.tipo, FRAME_CHAT);
    assert_eq!(frame.corpo, b"ola");
    // Fila vazia → espera com timeout devolve TempoEsgotado.
    let erro = ligacao
        .receber(Some(Duration::from_millis(30)))
        .expect_err("vazio");
    assert_eq!(erro, ErroP2P::TempoEsgotado);
    ligacao.fechar();
}

/// Um `OK` a caminho de um `CAIXA` (ack tardio) é ignorado.
#[test]
fn relay_ignora_ack_tardio() {
    let (endpoint, _recebidos) = relay_de_teste();
    let mut ligacao = LigacaoRelay::abrir(
        &endpoint,
        caixa_de_pub(&[9u8; 32]),
        caixa_de_pub(&[10u8; 32]),
    )
    .expect("abre");
    let mut entrega = vec![FRAME_CHAT];
    entrega.extend_from_slice(b"com ack");
    escrever_frame(&mut ligacao.fluxo, 0x98, &entrega).expect("pedido");
    // O `receber` lê OK (continua) e devolve o CAIXA seguinte.
    let frame = ligacao
        .receber(Some(Duration::from_secs(2)))
        .expect("frame");
    assert_eq!(frame.tipo, FRAME_CHAT);
    assert_eq!(frame.corpo, b"com ack");
    ligacao.fechar();
}

/// Erro `0x81` chegado durante um `receber` é propagado.
#[test]
fn relay_erro_durante_recebimento() {
    let (endpoint, _recebidos) = relay_de_teste();
    let mut ligacao = LigacaoRelay::abrir(
        &endpoint,
        caixa_de_pub(&[11u8; 32]),
        caixa_de_pub(&[12u8; 32]),
    )
    .expect("abre");
    // Comando desconhecido → o stub responde 0x81; lemos à mão e
    // confirmamos que o corpo se descreve como erro do relay.
    escrever_frame(&mut ligacao.fluxo, 0x55, b"x").expect("manda");
    let frame = ler_frame(&mut ligacao.fluxo).expect("resposta");
    assert_eq!(frame.tipo, RELAY_ERRO);
    let texto = descrever_erro_relay(&frame.corpo);
    assert!(texto.starts_with("erro 0x"), "{texto}");
    ligacao.fechar();
}

/// Um `CAIXA` sem corpo é rejeitado (nunca vira frame vazio).
#[test]
fn relay_caixa_vazia_rejeitada() {
    let (endpoint, _rx) = relay_de_teste();
    let mut ligacao = LigacaoRelay::abrir(
        &endpoint,
        caixa_de_pub(&[5u8; 32]),
        caixa_de_pub(&[6u8; 32]),
    )
    .expect("abre");
    let erro = ligacao
        .enfileirar_caixa(Frame::novo(RELAY_CAIXA, Vec::new()))
        .expect_err("vazio");
    assert_eq!(erro, ErroP2P::ComprimentoZero);
    assert_eq!(erro.para_ipc(), 0x02);
    ligacao.fechar();
}

/// Corpo `ENVIAR` acima do limite → PayloadGrande (caminho raro).
#[test]
fn relay_envio_gigante() {
    let (endpoint, _rx) = relay_de_teste();
    let mut ligacao = LigacaoRelay::abrir(
        &endpoint,
        caixa_de_pub(&[7u8; 32]),
        caixa_de_pub(&[8u8; 32]),
    )
    .expect("abre");
    // Heap, não pilha: ver o comentário de `frame_grande_rejeitado`.
    let gigante = vec![0u8; limite_corpo()];
    let erro = ligacao
        .enviar(FRAME_CHAT, &gigante)
        .expect_err("grande demais");
    assert_eq!(erro.para_ipc(), 0x09);
    ligacao.fechar();
}

/// Erros do relay: resposta `0x81` e frame inesperado.
#[test]
fn relay_erros_reportados() {
    // Servidor que responde 0x81 logo à subscricao.
    let servidor = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = servidor.local_addr().expect("addr").port();
    thread::spawn(move || {
        let (mut fluxo, _) = servidor.accept().expect("accept");
        let _ = ler_frame(&mut fluxo).expect("SUBSCREVER");
        let _ = escrever_frame(&mut fluxo, RELAY_ERRO, &[0x04, b'n', b'g']);
    });
    let erro = LigacaoRelay::abrir(&format!("127.0.0.1:{porta}"), [0u8; 32], [1u8; 32])
        .expect_err("erro do relay");
    assert_eq!(erro.para_ipc(), 0x11);
    assert!(erro.to_string().contains("erro 0x04: ng"), "{erro}");
}

/// Frame de tipo desconhecido do relay → `Relay(...)`.
#[test]
fn relay_frame_inesperado() {
    let servidor = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = servidor.local_addr().expect("addr").port();
    thread::spawn(move || {
        let (mut fluxo, _) = servidor.accept().expect("accept");
        let _ = ler_frame(&mut fluxo).expect("SUBSCREVER");
        let _ = escrever_frame(&mut fluxo, 0x77, b"?");
    });
    let erro = LigacaoRelay::abrir(&format!("127.0.0.1:{porta}"), [0u8; 32], [1u8; 32])
        .expect_err("tipo errado");
    assert!(erro.to_string().contains("0x77"), "{erro}");
    assert_eq!(erro.para_ipc(), 0x11);
}

/// Relay inacessível → `Io` (0x0A) na abertura.
#[test]
fn relay_indisponivel() {
    // Porta reservada e fechada logo a seguir (não escuta).
    let reserva = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = reserva.local_addr().expect("addr").port();
    drop(reserva);
    let erro = LigacaoRelay::abrir(&format!("127.0.0.1:{porta}"), [0u8; 32], [1u8; 32])
        .expect_err("sem relay");
    assert_eq!(erro.para_ipc(), 0x0A);
}

// ----------------------------------------------------------------
// abertura com fallback
// ----------------------------------------------------------------

/// Pedido direto com par registado → ligação direta estabelecida.
#[test]
fn abrir_direta_com_sucesso() {
    let servidor = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = servidor.local_addr().expect("addr").port();
    let mut dono = TorFalso::novo();
    let onion = dono.arrancar(porta).expect("publica");

    let mut backend = TorFalso::novo();
    backend.arrancar(0).expect("A");

    let pedido = PedidoLigacao {
        modo: ModoLigacao::Direto,
        destino: onion,
        endpoint_relay: None,
        caixa_propria: [9u8; 32],
    };
    let mut ligacao = abrir_ligacao(&mut backend, &pedido, [10u8; 32]).expect("abre");
    assert!(matches!(ligacao, LigacaoP2P::Direta(_)));

    // O par aceita e troca frames.
    let (lado_b, _) = servidor.accept().expect("accept");
    let mut b = LigacaoDireta::nova(lado_b);
    ligacao.enviar(FRAME_CHAT, b"ola").expect("envia");
    let f = b.receber(None).expect("recebe");
    assert_eq!(f.corpo, b"ola");
    dono.parar().expect("limpa");
    backend.parar().expect("limpa");
}

/// Direta falha sem relay → erro original do backend (0x0D/0x0B).
#[test]
fn abrir_direta_falha_sem_relay() {
    let mut backend = TorFalso::novo();
    backend.arrancar(1).expect("A");
    let pedido = PedidoLigacao {
        modo: ModoLigacao::Direto,
        destino: "desconhecido.onion".into(),
        endpoint_relay: None,
        caixa_propria: [0u8; 32],
    };
    let erro = abrir_ligacao(&mut backend, &pedido, [0u8; 32]).expect_err("falha");
    assert_eq!(erro.para_ipc(), 0x0D, "{erro}");
    backend.parar().expect("limpa");
}

/// Direta falha → fallback para o relay configurado.
#[test]
fn abrir_direta_com_fallback_relay() {
    let (endpoint, _rx) = relay_de_teste();
    let mut backend = TorFalso::novo();
    backend.arrancar(2).expect("A");
    let pedido = PedidoLigacao {
        modo: ModoLigacao::Direto,
        destino: "desconhecido.onion".into(),
        endpoint_relay: Some(endpoint),
        caixa_propria: [0u8; 32],
    };
    let ligacao = abrir_ligacao(&mut backend, &pedido, [1u8; 32]).expect("fallback");
    assert!(matches!(ligacao, LigacaoP2P::Relay(_)));
    backend.parar().expect("limpa");
}

/// Modo relay: happy path e exigências de endpoint.
#[test]
fn abrir_modo_relay() {
    let (endpoint, _rx) = relay_de_teste();
    let mut backend = TorFalso::novo();
    backend.arrancar(3).expect("A");

    // Sem endpoint → SemRelay (0x11).
    let sem_endpoint = PedidoLigacao {
        modo: ModoLigacao::Relay,
        destino: "ab".repeat(32),
        endpoint_relay: None,
        caixa_propria: [0u8; 32],
    };
    let erro = abrir_ligacao(&mut backend, &sem_endpoint, [0u8; 32]).expect_err("sem relay");
    assert_eq!(erro, ErroP2P::SemRelay);
    assert_eq!(erro.para_ipc(), 0x11);

    // Com endpoint → sessão aberta.
    let com_endpoint = PedidoLigacao {
        modo: ModoLigacao::Relay,
        destino: "cd".repeat(32),
        endpoint_relay: Some(endpoint),
        caixa_propria: [0u8; 32],
    };
    let mut ligacao = abrir_ligacao(&mut backend, &com_endpoint, [2u8; 32]).expect("relay");
    assert!(matches!(ligacao, LigacaoP2P::Relay(_)));
    ligacao.enviar(FRAME_CHAT, b"x").expect("envia");
    backend.parar().expect("limpa");
}

/// Fecho do relay via enum `LigacaoP2P` (caminho delegado).
#[test]
fn fecho_do_relay_pelo_enum() {
    let (endpoint, _rx) = relay_de_teste();
    let mut backend = TorFalso::novo();
    backend.arrancar(4).expect("A");
    let pedido = PedidoLigacao {
        modo: ModoLigacao::Relay,
        destino: "ef".repeat(32),
        endpoint_relay: Some(endpoint),
        caixa_propria: [0u8; 32],
    };
    let ligacao = abrir_ligacao(&mut backend, &pedido, [3u8; 32]).expect("relay");
    assert!(matches!(ligacao, LigacaoP2P::Relay(_)), "esperava relay");
    if let LigacaoP2P::Relay(relay) = ligacao {
        relay.fechar();
    }
    backend.parar().expect("limpa");
}

/// Backend parado + modo direto → `Tor(NaoArrancado)` (0x0B).
#[test]
fn abrir_sem_backend_arrancado() {
    let mut backend = TorFalso::novo();
    let pedido = PedidoLigacao {
        modo: ModoLigacao::Direto,
        destino: "x.onion".into(),
        endpoint_relay: None,
        caixa_propria: [0u8; 32],
    };
    let erro = abrir_ligacao(&mut backend, &pedido, [0u8; 32]).expect_err("parado");
    assert_eq!(erro.para_ipc(), 0x0B);
    assert!(matches!(erro, ErroP2P::Tor(ErroTor::NaoArrancado)));
}

// ----------------------------------------------------------------
// Contrato de constantes e estados
// ----------------------------------------------------------------

/// Tabela de tipos/constantes — documentação executável.
#[test]
fn constantes_do_protocolo_p2p() {
    assert_eq!(FRAME_CHAT, 0x01);
    assert_eq!(FRAME_FRIEND_REQUEST, 0x10);
    assert_eq!(FRAME_FRIEND_ACCEPT, 0x11);
    assert_eq!(FRAME_FRIEND_REJECT, 0x12);
    assert_eq!(FRAME_PING, 0x20);
    assert_eq!(FRAME_PONG, 0x21);
    assert_eq!(RELAY_SUBSCREVER, 0x01);
    assert_eq!(RELAY_ENVIAR, 0x02);
    assert_eq!(RELAY_FECHO, 0x03);
    assert_eq!(RELAY_ERRO, 0x81);
    assert_eq!(RELAY_OK, 0x83);
    assert_eq!(RELAY_CAIXA, 0x84);
    assert_eq!(DOMINIO_RELAY, b"ONYX/RELAY/v1");
    // O limite deixou de ser um literal. Este teste existe agora
    // para fixar a DERIVAÇÃO, não o número: se `MAX_ENVELOPE`
    // crescer, a constante tem de mudar e este teste tem de dizer
    // qual é o novo valor, em vez de aceitar qualquer um.
    assert_eq!(
        TAM_MAX_CORPO,
        crate::orcamento::orcamento_padrao(),
        "TAM_MAX_CORPO tem de ser o valor por omissão do orçamento"
    );
    assert_eq!(LIMITE_FRAMES, 128);
    assert_eq!(JANELA_RATE, Duration::from_secs(10));
    assert_eq!(TIMEOUT_ACK_RELAY, Duration::from_secs(5));
}

/// `Display` de todos os variantes — sem dados sensíveis.
#[test]
fn display_de_todos_os_erros() {
    let io = io::Error::new(io::ErrorKind::ConnectionRefused, "recusada");
    assert_eq!(
        ErroP2P::Io(io.to_string()).to_string(),
        "I/O na ligação: recusada"
    );
    assert_eq!(ErroP2P::Fechada.to_string(), "ligação fechada pelo par");
    assert_eq!(
        ErroP2P::TempoEsgotado.to_string(),
        "espera de frame expirou"
    );
    assert_eq!(ErroP2P::Desalinhado.to_string(), "frame truncado no stream");
    // O número vem do limite em vigor, não de um literal. Com o
    // literal 16777216 no teste, o teste fixava o valor errado em vez
    // de fixar o formato — e passaria a falhar sozinho quando o
    // orçamento mudasse, sem dizer o que devia mudar.
    assert_eq!(
        ErroP2P::PayloadGrande { obtido: 5 }.to_string(),
        format!("frame acima de {} bytes: 5", limite_corpo())
    );
    assert_eq!(
        ErroP2P::ComprimentoZero.to_string(),
        "frame com comprimento zero"
    );
    assert_eq!(
        ErroP2P::RateLimit.to_string(),
        "rate-limit de frames excedido"
    );
    assert_eq!(
        ErroP2P::DestinoInvalido("modo").to_string(),
        "destino inválido: modo"
    );
    assert_eq!(ErroP2P::Relay("x".into()).to_string(), "relay: x");
    assert_eq!(
        ErroP2P::SemRelay.to_string(),
        "modo relay sem endpoint configurado"
    );
    assert_eq!(ErroP2P::SemLigacao.to_string(), "sem ligação ativa ao par");
    assert_eq!(ErroP2P::SemLigacao.para_ipc(), 0x0A);
    assert_eq!(
        ErroP2P::Tor(ErroTor::NaoArrancado).to_string(),
        "backend Tor ainda não arrancou"
    );
    // Conversão implícita ErroTor → ErroP2P.
    let convertido: ErroP2P = ErroTor::NaoArrancado.into();
    assert_eq!(convertido.para_ipc(), 0x0B);
    // O estado do backend reporta 0/1/2 conforme o IPC.
    assert_eq!(EstadoTor::Arrancando.para_ipc(), 0x01);
}

// ----------------------------------------------------------------
// Receção faseada (liberta o bloqueio do estado entre frames)
// ----------------------------------------------------------------

/// Direta: sem dados → `TempoEsgotado` (sinal para soltar o lock);
/// com frame → devolvido normalmente.
#[test]
fn rececao_faseada_direta() {
    let (mut a, mut b) = par_de_sockets();

    // Nenhum byte dentro da fatia: erro de "ainda nada".
    let erro = a
        .receber_por_lotes(None, Duration::from_millis(40))
        .expect_err("vazio");
    assert_eq!(erro, ErroP2P::TempoEsgotado);

    // Frame completo chega → devolvido pela fase de leitura.
    b.enviar(FRAME_CHAT, b"ola").expect("envia");
    let frame = a
        .receber_por_lotes(Some(Duration::from_secs(2)), FATIA_ESPERA)
        .expect("frame");
    assert_eq!(frame.tipo, FRAME_CHAT);
    assert_eq!(frame.corpo, b"ola");
}

/// Direta: se o primeiro byte chegar, o resto do frame é esperado
/// com a espera TOTAL — a fatia nunca trunca um frame a meio.
#[test]
fn rececao_faseada_nao_abandona_frame_a_meio() {
    use std::io::Write as _;
    let servidor = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = servidor.local_addr().expect("addr").port();
    let mut cliente = StdTcpStream::connect(("127.0.0.1", porta)).expect("connect");
    let (lado_servidor, _) = servidor.accept().expect("accept");
    let mut recebedor = LigacaoDireta::nova(lado_servidor);

    let pacote = enquadrar(FRAME_CHAT, b"ola").expect("enquadra");
    // Primeiro pedaço imediato; o resto só passa 150 ms depois —
    // acima da fatia de 100 ms usada neste teste.
    cliente.write_all(&pacote[..2]).expect("1.º pedaço");
    thread::sleep(Duration::from_millis(150));
    cliente.write_all(&pacote[2..]).expect("resto");

    let frame = recebedor
        .receber_por_lotes(Some(Duration::from_secs(2)), Duration::from_millis(100))
        .expect("frame completo");
    assert_eq!(frame.tipo, FRAME_CHAT);
    assert_eq!(frame.corpo, b"ola");
}

/// Direta: `espera` menor que a fatia manda (a espera total é a
/// que limita a espera pelo primeiro byte).
#[test]
fn rececao_faseada_respeita_espera_menor() {
    let (mut a, _b) = par_de_sockets();
    let inicio = Instant::now();
    let erro = a
        .receber_por_lotes(Some(Duration::from_millis(50)), Duration::from_secs(30))
        .expect_err("expira");
    assert_eq!(erro, ErroP2P::TempoEsgotado);
    assert!(inicio.elapsed() < Duration::from_secs(5), "usou a fatia");
}

/// Relay: fila já com frames devolve de imediato; sem nada na
/// fila → `TempoEsgotado`; com `CAIXA` chegado → frame interno.
#[test]
fn rececao_faseada_relay() {
    let (endpoint, _rx) = relay_de_teste();
    let mut ligacao = LigacaoRelay::abrir(
        &endpoint,
        caixa_de_pub(&[13u8; 32]),
        caixa_de_pub(&[14u8; 32]),
    )
    .expect("abre");

    // (a) frame já enfileirado → devolvido sem tocar no socket.
    let mut previo = vec![FRAME_PING];
    previo.extend_from_slice(b"fila");
    escrever_frame(&mut ligacao.fluxo, 0x99, &previo).expect("fila");
    ligacao
        .fluxo
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    let caixa = ler_frame(&mut ligacao.fluxo).expect("CAIXA");
    ligacao.enfileirar_caixa(caixa).expect("enfileira");
    let frame = ligacao
        .receber_por_lotes(None, FATIA_ESPERA)
        .expect("da fila");
    assert_eq!(frame.corpo, b"fila");

    // (b) fila vazia e relay em silêncio → sinal para soltar o lock.
    let erro = ligacao
        .receber_por_lotes(None, Duration::from_millis(40))
        .expect_err("vazio");
    assert_eq!(erro, ErroP2P::TempoEsgotado);

    // (c) entrega em push → frame interno devolvido.
    let mut entrega = vec![FRAME_CHAT];
    entrega.extend_from_slice(b"push");
    escrever_frame(&mut ligacao.fluxo, 0x99, &entrega).expect("entrega");
    let frame = ligacao
        .receber_por_lotes(Some(Duration::from_secs(2)), FATIA_ESPERA)
        .expect("push");
    assert_eq!(frame.tipo, FRAME_CHAT);
    assert_eq!(frame.corpo, b"push");
    ligacao.fechar();
}

/// O enum delega a receção faseada para os dois transportes.
#[test]
fn rececao_faseada_pelo_enum() {
    // Transporte direto.
    let (a, mut b) = par_de_sockets();
    let mut direta = LigacaoP2P::Direta(Box::new(a));
    let erro = direta
        .receber_por_lotes(None, Duration::from_millis(30))
        .expect_err("vazio");
    assert_eq!(erro, ErroP2P::TempoEsgotado);
    b.enviar(FRAME_PONG, b"x").expect("envia");
    let frame = direta
        .receber_por_lotes(Some(Duration::from_secs(2)), FATIA_ESPERA)
        .expect("frame");
    assert_eq!(frame.tipo, FRAME_PONG);
    drop(direta);

    // Transporte relay.
    let (endpoint, _rx) = relay_de_teste();
    let mut backend = TorFalso::novo();
    backend.arrancar(5).expect("A");
    let pedido = PedidoLigacao {
        modo: ModoLigacao::Relay,
        destino: "11".repeat(32),
        endpoint_relay: Some(endpoint),
        caixa_propria: [0u8; 32],
    };
    let mut relay = abrir_ligacao(&mut backend, &pedido, [1u8; 32]).expect("relay");
    let erro = relay
        .receber_por_lotes(None, Duration::from_millis(30))
        .expect_err("vazio");
    assert_eq!(erro, ErroP2P::TempoEsgotado);
    backend.parar().expect("limpa");
}

/// `de_io_meio_frame`: timeout a meio é desalinhamento; qualquer
/// outro erro de I/O mantém-se como `Io`.
#[test]
fn de_io_meio_frame_traduz_erro() {
    let timeout = io::Error::new(io::ErrorKind::TimedOut, "passou o prazo");
    assert_eq!(de_io_meio_frame(timeout), ErroP2P::Desalinhado);
    let bloqueio = io::Error::new(io::ErrorKind::WouldBlock, "ocupado");
    assert_eq!(de_io_meio_frame(bloqueio), ErroP2P::Desalinhado);
    let eof = io::Error::new(io::ErrorKind::UnexpectedEof, "cortado");
    assert_eq!(de_io_meio_frame(eof), ErroP2P::Desalinhado);
    let outro = io::Error::new(io::ErrorKind::BrokenPipe, "partida");
    assert!(
        matches!(de_io_meio_frame(outro), ErroP2P::Io(_)),
        "erros de fundo continuam a ser Io"
    );
}

/// `esperar_primeiro_byte` com um `Read` que falha por fundo
/// (não timeout) propaga `Io` em vez de devolver `Ok(false)`.
#[test]
fn esperar_primeiro_byte_com_erro_de_io() {
    struct LeitorQueFalha;
    impl Read for LeitorQueFalha {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "sem acesso",
            ))
        }
    }
    let mut leitor = LeitorQueFalha;
    let mut cabecalho = [0u8; 4];
    let erro = esperar_primeiro_byte(&mut leitor, &mut cabecalho).expect_err("erro");
    assert!(matches!(erro, ErroP2P::Io(_)), "{erro:?}");
}

/// Todos os ramos de `esperar_primeiro_byte` percorridos pela mesma
/// instanciação: fechado, byte disponível, expirou e erro de fundo.
#[test]
fn esperar_primeiro_byte_todos_os_ramos() {
    enum Passo {
        Byte(u8),
        Erro(io::ErrorKind),
    }
    struct LeitorProgramado(Vec<Passo>);
    impl Read for LeitorProgramado {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            match self.0.first() {
                Some(Passo::Byte(byte)) => {
                    buffer[0] = *byte;
                    self.0.remove(0);
                    Ok(1)
                }
                Some(Passo::Erro(fundo)) => {
                    let fundo = *fundo;
                    self.0.remove(0);
                    Err(io::Error::new(fundo, "fundo"))
                }
                None => Ok(0),
            }
        }
    }

    // Fecho limpo → `Fechada`.
    let mut cabecalho = [0u8; 4];
    let mut leitor = LeitorProgramado(vec![]);
    let erro = esperar_primeiro_byte(&mut leitor, &mut cabecalho).expect_err("fechada");
    assert!(matches!(erro, ErroP2P::Fechada), "{erro:?}");

    // Byte disponível → há frame a meio.
    let mut leitor = LeitorProgramado(vec![Passo::Byte(7)]);
    assert_eq!(esperar_primeiro_byte(&mut leitor, &mut cabecalho), Ok(true));
    assert_eq!(cabecalho[0], 7, "o byte ficou no cabeçalho");

    // Expirou sem byte → `Ok(false)` (o chamador liberta o bloqueio).
    let mut leitor = LeitorProgramado(vec![Passo::Erro(io::ErrorKind::WouldBlock)]);
    assert_eq!(
        esperar_primeiro_byte(&mut leitor, &mut cabecalho),
        Ok(false)
    );

    // Erro de fundo → `Io`, nunca `Ok(false)`.
    let mut leitor = LeitorProgramado(vec![Passo::Erro(io::ErrorKind::ConnectionReset)]);
    let erro = esperar_primeiro_byte(&mut leitor, &mut cabecalho).expect_err("fundo");
    assert!(matches!(erro, ErroP2P::Io(_)), "{erro:?}");
}

/// Uma janela já expirada é reiniciada no próprio `registar`
/// (contador volta a 1, sem esbarrar no teto).
#[test]
fn janela_de_rate_reinicia() {
    let mut janela = JanelaRate {
        contador: LIMITE_FRAMES,
        inicio: Instant::now() - JANELA_RATE - Duration::from_millis(1),
    };
    janela.registar().expect("janela nova");
    assert_eq!(janela.contador, 1, "contagem reiniciada");
}

/// Um `CAIXA` que chega antes do `OK` da subscrição é enfileirado
/// (o `esperar_ok` não o descarta) e chega ao chamador.
#[test]
fn relay_caixa_prematura_durante_a_subscricao() {
    let servidor = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = servidor.local_addr().expect("addr").port();
    thread::spawn(move || {
        if let Ok((mut fluxo, _)) = servidor.accept() {
            let _ = atender_caixa_prematura(&mut fluxo);
        }
    });
    let mut ligacao =
        LigacaoRelay::abrir(&format!("127.0.0.1:{porta}"), [4u8; 32], [5u8; 32]).expect("abre");
    let frame = ligacao
        .receber(Some(Duration::from_secs(2)))
        .expect("frame prematuro");
    assert_eq!(frame.tipo, FRAME_CHAT);
    assert_eq!(frame.corpo, b"prematura");
    ligacao.fechar();
}

/// `consumir_resposta_relay` com `ERRO` e com um tipo desconhecido
/// do relay → ambos viram `Relay(...)` legível.
#[test]
fn relay_consumir_erro_e_tipo_desconhecido() {
    // (a) ERRO do relay durante o recebimento.
    let (endpoint, _rx) = relay_de_teste();
    let mut ligacao = LigacaoRelay::abrir(&endpoint, [0u8; 32], [1u8; 32]).expect("abre");
    escrever_frame(&mut ligacao.fluxo, 0x55, b"x").expect("manda");
    let erro = ligacao
        .receber(Some(Duration::from_secs(2)))
        .expect_err("erro do relay");
    assert!(erro.to_string().contains("erro 0x06"), "{erro}");
    ligacao.fechar();

    // (b) Tipo de frame desconhecido logo após a subscricao.
    let servidor = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = servidor.local_addr().expect("addr").port();
    thread::spawn(move || {
        if let Ok((mut fluxo, _)) = servidor.accept() {
            let _ = atender_tipo_estranho(&mut fluxo);
        }
    });
    let mut ligacao =
        LigacaoRelay::abrir(&format!("127.0.0.1:{porta}"), [2u8; 32], [3u8; 32]).expect("abre");
    let erro = ligacao
        .receber(Some(Duration::from_secs(2)))
        .expect_err("tipo desconhecido");
    assert!(erro.to_string().contains("0x77"), "{erro}");
    ligacao.fechar();
}

/// Modo `relay` com endpoint morto → o `?` da abertura propaga-se.
#[test]
fn abrir_ligacao_relay_indisponivel() {
    let reserva = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = reserva.local_addr().expect("addr").port();
    drop(reserva);
    let mut backend = TorFalso::novo();
    backend.arrancar(5).expect("A");
    let pedido = PedidoLigacao {
        modo: ModoLigacao::Relay,
        destino: "11".repeat(32),
        endpoint_relay: Some(format!("127.0.0.1:{porta}")),
        caixa_propria: [0u8; 32],
    };
    let erro = abrir_ligacao(&mut backend, &pedido, [1u8; 32]).expect_err("sem relay");
    assert_eq!(erro.para_ipc(), 0x0A);
    backend.parar().expect("limpa");
}

/// Direto falha (backend parado) e o `?` do fallback para o relay
/// também falha → o erro do relay é o que se propaga.
#[test]
fn abrir_ligacao_fallback_relay_indisponivel() {
    let reserva = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let porta = reserva.local_addr().expect("addr").port();
    drop(reserva);
    let mut backend = TorFalso::novo(); // nunca arrancou
    let pedido = PedidoLigacao {
        modo: ModoLigacao::Direto,
        destino: "abc.onion".to_string(),
        endpoint_relay: Some(format!("127.0.0.1:{porta}")),
        caixa_propria: [0u8; 32],
    };
    let erro = abrir_ligacao(&mut backend, &pedido, [1u8; 32]).expect_err("sem relay");
    assert_eq!(erro.para_ipc(), 0x0A);
}

/// `LigacaoP2P::receber` delega nos dois transportes.
#[test]
fn receber_pelo_enum() {
    // Transporte direto.
    let (a, mut b) = par_de_sockets();
    let mut direta = LigacaoP2P::Direta(Box::new(a));
    b.enviar(FRAME_CHAT, b"ida").expect("envia");
    let frame = direta
        .receber(Some(Duration::from_secs(2)))
        .expect("frame direto");
    assert_eq!(frame.tipo, FRAME_CHAT);
    assert_eq!(frame.corpo, b"ida");
    drop(direta);

    // Transporte relay.
    let (endpoint, _rx) = relay_de_teste();
    let mut interna = LigacaoRelay::abrir(&endpoint, [0u8; 32], [1u8; 32]).expect("abre");
    let mut entrega = vec![FRAME_CHAT];
    entrega.extend_from_slice(b"volta");
    escrever_frame(&mut interna.fluxo, 0x99, &entrega).expect("entrega");
    let mut relay = LigacaoP2P::Relay(Box::new(interna));
    let frame = relay
        .receber(Some(Duration::from_secs(2)))
        .expect("frame relay");
    assert_eq!(frame.tipo, FRAME_CHAT);
    assert_eq!(frame.corpo, b"volta");
}

/// Frame::novo constrói o par tipo/corpo.
#[test]
fn frame_novo() {
    let frame = Frame::novo(0x05, vec![9, 9]);
    assert_eq!(frame.tipo, 0x05);
    assert_eq!(frame.corpo, vec![9, 9]);
}

// -----------------------------------------------------------------
// Endpoint do relay — validação (Fase E)
// -----------------------------------------------------------------
//
// Estes testes são a razão de `Endpoint::validar` existir. Sem eles,
// um endpoint malformado daria um `io::Error` de resolução — o
// mesmo sintoma de "relay em baixo" — e uma query DNS real sairia
// para fora do Tor sem que ninguém soubesse.

/// `host:porta` bem formado é aceite.
#[test]
fn endpoint_aceita_host_e_porta() {
    let e = Endpoint::validar("127.0.0.1:9050").expect("aceita");
    assert_eq!(e.host, "127.0.0.1");
    assert_eq!(e.porta, 9050);
    // `normalizado` é o que vai ao `connect`, e tem de ser
    // reconstruído a partir das partes validadas.
    assert_eq!(e.normalizado(), "127.0.0.1:9050");
}

/// Portas nos extremos são válidas.
#[test]
fn endpoint_aceita_portas_limite() {
    assert_eq!(Endpoint::validar("h:1").expect("1").porta, 1);
    assert_eq!(Endpoint::validar("h:65535").expect("65535").porta, 65535);
}

/// Sem porta, com porta vazia, não numérica, zero ou fora do
/// intervalo — todos recusados, com `DestinoInvalido`.
///
/// O caso "sem porta" é o que mais importa: `TcpStream::connect`
/// aceitaria um hostname sem porta e usaria a do serviço por
/// omissão, o que transformaria uma omissão numa ligação que o
/// utilizador não pediu.
#[test]
fn endpoint_recusa_portas_invalidas() {
    for texto in [
        "127.0.0.1",       // sem porta
        "127.0.0.1:",      // porta vazia
        "127.0.0.1:abc",   // não numérica
        "127.0.0.1:0",     // zero
        "127.0.0.1:65536", // acima do máximo
        ":9050",           // sem host
    ] {
        let erro = Endpoint::validar(texto).expect_err(&format!("«{texto}» devia ser recusado"));
        assert_eq!(
            erro.para_ipc(),
            0x0D,
            "«{texto}» deu o código errado"
        );
        assert!(matches!(erro, ErroP2P::DestinoInvalido(_)));
    }
}

/// Caminhos e `..` são recusados.
///
/// O `endpoint` vem do cliente, pelo IPC. Um valor com `/` ou `..`
/// é uma tentativa de alcançar um caminho em vez de uma porta — e
/// mesmo que `TcpStream::connect` não obedecesse, a validação
/// não deve passar o que não parece um endpoint.
#[test]
fn endpoint_recusa_caminhos() {
    for texto in [
        "localhost:8080/../etc",
        "localhost/../x:80",
        "C:\\x:80",
        "host..x:80",
    ] {
        assert!(
            Endpoint::validar(texto).is_err(),
            "«{texto}» devia ser recusado"
        );
    }
}

/// O aviso de clearnet é escrito **antes** de ligar, e identifica o
/// relay que vai observar o IP real.
///
/// Isto é o que torna o custo do relay *declarado*. Se o aviso
/// dependesse da ligação, nunca apareceria no caso que interessa —
/// o caso em que a ligação tem sucesso.
///
/// O writer é injectado (`abrir_aviso_com`), o que evita depender
/// de capturar o descritor 2 do processo — partilhado com a suite
/// paralela e portanto não determinístico.
#[test]
fn aviso_de_clearnet_anuncia_o_destino_antes_de_ligar() {
    let (endpoint, _rx) = relay_de_teste();
    // A porta é o que torna a asserção específica: `CLEARNET` e
    // `privacy_model`, sós por si, não provariam quase nada.
    let porta = endpoint.rsplit(':').next().expect("tem porta").to_string();

    let mut aviso = Vec::new();
    LigacaoRelay::abrir_aviso_com(&endpoint, [1u8; 32], [2u8; 32], &mut aviso)
    .expect("a ligação em loopback tem de abrir")
    .fechar();

    let aviso = String::from_utf8_lossy(&aviso);
    assert!(
        aviso.contains("CLEARNET"),
        "o aviso tem de dizer que a ligação não vai por Tor:\n{aviso}"
    );
    assert!(
        aviso.contains(&porta),
        "o aviso tem de identificar o relay:\n{aviso}"
    );
    assert!(
        aviso.contains("privacy_model"),
        "o aviso tem de remeter para onde a ameaça está declarada:\n{aviso}"
    );
}

/// O aviso **não** contém material criptográfico.
///
/// O relay vê a mailbox (derivada da pública) e o ciphertext, nunca
/// chaves. Se um dia o aviso incluir a chave de sessão para "ajudar"
/// a depurar, passaria a ser o ponto mais fácil de furar. Este
/// teste existe para o dia em que alguém tentar.
#[test]
fn aviso_de_clearnet_nao_contem_chaves() {
    let (endpoint, _rx) = relay_de_teste();
    let mut aviso = Vec::new();
    LigacaoRelay::abrir_aviso_com(&endpoint, [0xAB; 32], [0xCD; 32], &mut aviso)
    .expect("abre")
    .fechar();

    let aviso = String::from_utf8_lossy(&aviso).to_lowercase();
    let chave_ab = "ab".repeat(32);
    let chave_cd = "cd".repeat(32);
    assert!(
        !aviso.contains(&chave_ab) && !aviso.contains(&chave_cd),
        "o aviso de segurança está a vazar chaves:\n{aviso}"
    );
}

/// Um endpoint inválido **não** produz aviso de clearnet.
///
/// O aviso é sobre uma ligação que vai acontecer. Uma validação que
/// falha não liga a lado nenhum, e avisar «vai sair em clearnet»
/// antes de dizer «o endpoint é inválido» seria exactamente o
/// contrário de ser claro.
#[test]
fn endpoint_invalido_nao_produz_aviso() {
    let mut aviso = Vec::new();
    let erro = LigacaoRelay::abrir_aviso_com("sem-porta", [1u8; 32], [2u8; 32], &mut aviso)
    .expect_err("endpoint sem porta tem de ser recusado");

    assert!(matches!(erro, ErroP2P::DestinoInvalido(_)));
    assert!(
        aviso.is_empty(),
        "não devia haver aviso numa validação que falha: {:?}",
        String::from_utf8_lossy(&aviso)
    );
}

/// Um writer que falha **não** derruba a ligação.
///
/// `eprintln!` entra em pânico quando a escrita falha. Um aviso de
/// segurança que derruba o daemon num disco cheio troca uma falha
/// visível por uma queda — e a queda perde o que o daemon estava a
/// fazer, que é o que interessa ao utilizador.
#[test]
fn aviso_que_falha_nao_derruba_a_ligacao() {
    /// Escreve sempre com erro — um disco cheio, um pipe fechado.
    struct EscritorQueFalha;

    impl Write for EscritorQueFalha {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::StorageFull, "sem espaço"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let (endpoint, _rx) = relay_de_teste();
    let r = LigacaoRelay::abrir_aviso_com(&endpoint, [1u8; 32], [2u8; 32], &mut EscritorQueFalha);
    r.expect("uma falha no aviso não pode impedir a ligação").fechar();
}

/// `abrir` com endpoint inválido falha **antes** de qualquer I/O.
///
/// Se validasse depois, um endpoint malformado tentaria resolver
/// via DNS — e a query sairia do sistema.
#[test]
fn abrir_recusa_endpoint_invalido_sem_i_o() {
    let erro = LigacaoRelay::abrir("sem-porta", [1u8; 32], [2u8; 32])
        .expect_err("endpoint sem porta tem de ser recusado");
    assert!(matches!(erro, ErroP2P::DestinoInvalido(_)));
    assert_eq!(erro.para_ipc(), 0x0D);
}
