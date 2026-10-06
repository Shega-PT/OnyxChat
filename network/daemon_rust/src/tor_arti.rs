// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// tor_arti.rs — backend Tor REAL do daemon (arti-client embutido)
// ---------------------------------------------------------------------
// Implementa `BackendTor` por cima da arti (`arti-client` 0.47):
//
//   * `arrancar`  — cria o TorClient (bootstrap *on demand*), lança o
//                   hidden service `onyxchat` e encaminha os pedidos de
//                   rendezvous (porta 80) para o servidor P2P local via
//                   `tor-hsrproxy`; devolve o `.onion`;
//   * `ligar`     — abre um stream de saída `.onion` pela arti e devolve
//                   um `TcpStream` síncrono (ponte loopback);
//   * `parar`     — desliga o proxy e o serviço (idempotente).
//
// A cobertura de linhas exclui este ficheiro
// (`--ignore-filename-regex 'tor_arti'`): a métrica de 100% mede o
// código que corre sem rede.
//
// ## Este ficheiro e `p2p.rs` são os dois pontos de rede real
//
// Uma versão anterior deste comentário dizia que este era o **único**
// ficheiro do workspace a falar com a rede real. É falso, e a
// afirmação era perigosa: `p2p.rs` também liga à rede, no modo relay
// (`LigacaoRelay::abrir` faz `TcpStream::connect` directo). A
// diferença é que essa ligação é **em clearnet** e por isso expõe o IP
// real do utilizador — o que `privacy_model.md` §8 declara como o
// preço aceitável do modo relay.
//
// Os dois pontos de rede são:
//
//   * este ficheiro: saída por circuito Tor, o operador de rede vê o
//     operador Tor;
//   * `p2p.rs` (`LigacaoRelay`): TCP directo a um relay, o IP real é
//     visível para esse relay.
//
// O guarda `tests/guarda_tor.rs` garante que **este** módulo não é
// importado por lado nenhum, o que é o que interessa: que o resto do
// daemon não possa alcançar a arti senão por `BackendTor`. Ele não
// cobre `std::net` directo — e `p2p.rs` usa-o, de propósito, no
// caminho de relay.
// =====================================================================

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arti_client::config::TorClientConfigBuilder;
use arti_client::{BootstrapBehavior, TorClient};
use safelog::DisplayRedacted as _;
use tor_hsrproxy::config::{Encapsulation, ProxyAction, ProxyPattern, ProxyRule, TargetAddr};
use tor_hsrproxy::{OnionServiceReverseProxy, ProxyConfig};
use tor_hsservice::{HsNickname, OnionServiceConfig, RunningOnionService};
use tor_rtcompat::{PreferredRuntime, SpawnExt as _, ToplevelBlockOn as _};

use crate::tor::{BackendTor, ErroTor, EstadoTor, PORTA_ONION};

/// Apelido (nickname) do hidden service — fixo para reutilizar as chaves.
const APELIDO: &str = "onyxchat";

/// Cliente arti com o runtime preferido (tokio) escolhido pela própria arti.
type ClienteArti = TorClient<PreferredRuntime>;

/// Prazo máximo à espera do endereço `.onion` depois do lançamento.
const ESPERA_ONION: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------
// Diretórios de estado da arti
// ---------------------------------------------------------------------

/// Diretórios (estado, cache) usados pela arti neste daemon, já criados.
///
/// Ordem: `ONYXCHAT_ARTI_DIR` → `$XDG_DATA_HOME/onyxchat` →
/// `$HOME/.local/share/onyxchat` → `$TMPDIR/onyxchat-arti`.
///
/// A árvore é criada com modo 0700: guardamos aqui as chaves dos hidden
/// services e o `fs_mistrust` da arti recusa diretórios legíveis por
/// outros (`create_dir_all` daria 0755 e o cliente recusaria arrancar).
fn dirs_arti() -> Result<(PathBuf, PathBuf), ErroTor> {
    let base = std::env::var_os("ONYXCHAT_ARTI_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_DATA_HOME").map(|raiz| PathBuf::from(raiz).join("onyxchat"))
        })
        .or_else(|| {
            std::env::var_os("HOME").map(|raiz| PathBuf::from(raiz).join(".local/share/onyxchat"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("onyxchat-arti"));
    let estado_dir = base.join("state");
    let cache_dir = base.join("cache");
    for diretorio in [&base, &estado_dir, &cache_dir] {
        criar_dir_privado(diretorio)?;
    }
    Ok((estado_dir, cache_dir))
}

/// Cria o diretório (e os pais) e força-lhe o modo 0700.
fn criar_dir_privado(diretorio: &std::path::Path) -> Result<(), ErroTor> {
    std::fs::create_dir_all(diretorio).map_err(de_io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(diretorio, std::fs::Permissions::from_mode(0o700))
            .map_err(de_io)?;
    }
    Ok(())
}

/// Converte um erro de I/O do sistema em `ErroTor` legível (sem payload).
fn de_io(erro: std::io::Error) -> ErroTor {
    ErroTor::Falha(erro.to_string())
}

/// Junta um erro com toda a sua cadeia de causas numa só linha.
///
/// A arti envolve o erro real (`fs_mistrust` → `tor-persist` → …) e o
/// `Display` de topo esconde-o; sem a cadeia um problema de permissões
/// de disco aparece só como "Error while trying to access persistent
/// state", sem dizer qual diretório nem porquê.
fn cadeia(erro: &(dyn std::error::Error + 'static)) -> String {
    let mut texto = erro.to_string();
    let mut causa = erro.source();
    while let Some(c) = causa {
        texto.push_str(" ← ");
        texto.push_str(&c.to_string());
        causa = c.source();
    }
    texto
}

// ---------------------------------------------------------------------
// TorReal
// ---------------------------------------------------------------------

/// Backend Tor real (arti) — bootstrap sob procura, hidden service
/// efémero com identidade persistente e ligações de saída `.onion`.
pub struct TorReal {
    /// Cliente arti (criado uma vez; o bootstrap é *on demand*).
    cliente: Arc<ClienteArti>,
    /// Serviço lançado — mantido vivo enquanto estiver a escuta.
    servico: Option<Arc<RunningOnionService>>,
    /// Proxy reverso dos rendezvous → servidor P2P local.
    proxy: Option<Arc<OnionServiceReverseProxy>>,
    /// Endereço `.onion` publicado (`None` = parado).
    onion: Option<String>,
    /// Estado corrente (para o comando `ESTADO`).
    estado: EstadoTor,
}

impl TorReal {
    /// Cria o cliente arti **sem** bootstrap (rede toca só em `arrancar`
    /// ou `ligar`), com estado em disco próprio do OnyxChat.
    ///
    /// Falhas de sistema de ficheiros/da configuração viram
    /// `ErroTor::Falha` — o daemon nunca aborta por isto.
    pub fn novo() -> Result<Self, ErroTor> {
        // O fornecedor do rustls é `ring` (único ativo, vindo do
        // Cargo.toml). A arti entrega o rustls sem provider, por isso
        // instalamos aqui — antes de qualquer handshake. Idempotente:
        // se já estiver instalado, segue em frente.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let (estado_dir, cache_dir) = dirs_arti()?;

        let construtor = TorClientConfigBuilder::from_directories(&estado_dir, &cache_dir);
        let config = construtor
            .build()
            .map_err(|erro| ErroTor::Falha(format!("configuração da arti: {}", cadeia(&erro))))?;

        // O runtime é DOSSIÊ do backend: o daemon é código síncrono, por
        // isso nunca existe um runtime ambiental onde a arti se pudesse
        // apoiar (`TorClient::builder()` chamaria `PreferredRuntime::current()`
        // e panicaria fora de contexto). Criamo-lo nós e entregamo-lo à
        // arti — `block_on` posterior é sempre feito a partir da thread
        // principal, fora de qualquer runtime.
        let runtime = PreferredRuntime::create()
            .map_err(|erro| ErroTor::Falha(format!("runtime: {}", cadeia(&erro))))?;

        let cliente = TorClient::with_runtime(runtime)
            .config(config)
            // Bootstrap só quando necessário: criar o daemon é barato e
            // offline; a rede entra em `arrancar`/`ligar`.
            .bootstrap_behavior(BootstrapBehavior::OnDemand)
            .create_unbootstrapped()
            .map_err(|erro| ErroTor::Falha(format!("criação do cliente Tor: {}", cadeia(&erro))))?;

        Ok(TorReal {
            cliente,
            servico: None,
            proxy: None,
            onion: None,
            estado: EstadoTor::Parado,
        })
    }

    /// Publica o serviço: lança o hidden service, liga o proxy reverso
    /// (porta `PORTA_ONION` → `porta_local`) e espera o `.onion`.
    fn publicar(&mut self, porta_local: u16) -> Result<String, ErroTor> {
        let apelido = HsNickname::new(APELIDO.to_string())
            .map_err(|_| ErroTor::Falha("apelido de serviço inválido".into()))?;

        let mut construtor = OnionServiceConfig::builder();
        construtor.nickname(apelido.clone());
        let config = construtor
            .build()
            .map_err(|erro| ErroTor::Falha(format!("config do serviço: {erro}")))?;

        let (servico, pedidos) = self
            .cliente
            .launch_onion_service(config)
            .map_err(|erro| ErroTor::Falha(format!("lançamento do serviço: {}", cadeia(&erro))))?
            .ok_or_else(|| ErroTor::Falha("serviço Tor desativado na configuração".into()))?;

        // Proxy reverso: tudo o que entra no hidden service (porta 80)
        // segue para o servidor P2P local já em escuta.
        let alvo = TargetAddr::Inet(SocketAddr::from(([127, 0, 0, 1], porta_local)));
        let regra = ProxyRule::new(
            ProxyPattern::one_port(PORTA_ONION)
                .map_err(|erro| ErroTor::Falha(format!("regra de porta: {erro}")))?,
            ProxyAction::Forward(Encapsulation::Simple, alvo),
        );
        let mut proxy_construtor = ProxyConfig::builder();
        proxy_construtor.set_proxy_ports(vec![regra]);
        let proxy_config = proxy_construtor
            .build()
            .map_err(|erro| ErroTor::Falha(format!("config do proxy: {erro}")))?;
        let proxy = OnionServiceReverseProxy::new(proxy_config);

        // Tarefa que encaminha os rendezvous enquanto o serviço viver.
        let runtime = self.cliente.runtime().clone();
        let em_curso = runtime.clone();
        let proxy_tarefa = Arc::clone(&proxy);
        let apelido_tarefa = apelido.clone();
        runtime
            .spawn(async move {
                let _ = proxy_tarefa
                    .handle_requests(em_curso, apelido_tarefa, Box::pin(pedidos))
                    .await;
            })
            .map_err(|erro| ErroTor::Falha(format!("tarefa do proxy: {erro}")))?;

        let endereco = esperar_onion(&servico)?;
        self.servico = Some(servico);
        self.proxy = Some(proxy);
        Ok(endereco)
    }

    /// Abre a ligação de SAÍDA ao `destino` e devolve um `TcpStream`
    /// síncrono (ponte loopback entre a arti e o código do P2P).
    fn ponte(&self, destino: &str) -> Result<TcpStream, ErroTor> {
        let alvo = if destino.contains(':') {
            destino.to_string()
        } else {
            // Endereços `.onion` não trazem porta: o hidden service
            // OnyxChat sempre escuta em `PORTA_ONION`.
            format!("{destino}:{PORTA_ONION}")
        };
        let cliente = Arc::clone(&self.cliente);
        let runtime = self.cliente.runtime().clone();

        // 1. Escuta local + stream Tor (o bootstrap acontece aqui).
        let (ouvinte, mut fluxo, endereco) = runtime.block_on(async move {
            let ouvinte = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .map_err(de_io)?;
            let endereco = ouvinte.local_addr().map_err(de_io)?;
            let fluxo = cliente
                .connect(alvo.as_str())
                .await
                .map_err(|erro| ErroTor::Falha(format!("ligação .onion: {}", cadeia(&erro))))?;
            Ok::<_, ErroTor>((ouvinte, fluxo, endereco))
        })?;

        // 2. Tarefa que bombeia bytes entre o socket local e a arti.
        runtime
            .spawn(async move {
                if let Ok((mut local, _)) = ouvinte.accept().await {
                    let _ = tokio::io::copy_bidirectional(&mut local, &mut fluxo).await;
                }
            })
            .map_err(|erro| ErroTor::Falha(format!("tarefa da ponte: {erro}")))?;

        // 3. O `connect` completo já está na fila do kernel: devolve já.
        TcpStream::connect(endereco).map_err(de_io)
    }
}

/// Espera que o endereço `.onion` fique disponível no serviço.
fn esperar_onion(servico: &Arc<RunningOnionService>) -> Result<String, ErroTor> {
    let limite = Instant::now() + ESPERA_ONION;
    loop {
        if let Some(hsid) = servico.onion_address() {
            return Ok(hsid.display_unredacted().to_string());
        }
        if Instant::now() >= limite {
            return Err(ErroTor::Falha(
                "tempo esgotado à espera do endereço .onion".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

impl BackendTor for TorReal {
    fn arrancar(&mut self, porta_local: u16) -> Result<String, ErroTor> {
        // Idempotente: o mesmo backend nunca muda de endereço.
        if let Some(onion) = &self.onion {
            return Ok(onion.clone());
        }
        self.estado = EstadoTor::Arrancando;
        match self.publicar(porta_local) {
            Ok(onion) => {
                self.onion = Some(onion.clone());
                self.estado = EstadoTor::Ativo;
                Ok(onion)
            }
            Err(erro) => {
                self.estado = EstadoTor::Parado;
                Err(erro)
            }
        }
    }

    fn parar(&mut self) -> Result<(), ErroTor> {
        if let Some(proxy) = self.proxy.take() {
            proxy.shutdown();
        }
        // Soltar o serviço faz a arti desligar o hidden service.
        self.servico = None;
        self.onion = None;
        self.estado = EstadoTor::Parado;
        Ok(())
    }

    fn estado(&self) -> EstadoTor {
        self.estado
    }

    fn onion(&self) -> Option<&str> {
        self.onion.as_deref()
    }

    fn ligar(&mut self, destino: &str) -> Result<TcpStream, ErroTor> {
        // Espelha o contrato do `TorFalso`: um nó que ainda não publicou
        // o próprio hidden service não abre ligações de saída.
        if self.estado != EstadoTor::Ativo {
            return Err(ErroTor::NaoArrancado);
        }
        self.ponte(destino)
    }
}
