// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// tor.rs — backend Tor do daemon (Etapa 7)
// ---------------------------------------------------------------------
// Define o traço `BackendTor` que abstrai "como se publica um hidden
// service e como se liga a outro", com duas implementações:
//
//   * `TorFalso`  — loopback puro, sem rede: usado por TODOS os testes
//                   (mantém a cobertura de 100% sem bootstrap Tor);
//   * `TorReal`   — arti-client embutido (ver `tor_arti.rs`): bootstrap,
//                   hidden service efémero e ligações .onion de saída.
//
// O resto do daemon (P2P, IPC, handshake) fala apenas com o traço, nunca
// com a arti diretamente — regra de ouro que isola o glue de rede
// (excluído da métrica de cobertura por exigir rede real).
//
// Contrato com o servidor P2P: `arrancar(porta_local)` publica um
// endereço que encaminha tráfego DE ENTRADA para a `porta_local` onde
// este nó já escuta; `ligar(destino)` devolve o stream de SAÍDA.
// =====================================================================

use std::collections::HashMap;
use std::fmt;
use std::net::TcpStream;
use std::sync::{Mutex, OnceLock};

/// Porta lógica dos hidden services OnyxChat (hsproxy → P2P local).
pub const PORTA_ONION: u16 = 80;

// ---------------------------------------------------------------------
// Estado do backend
// ---------------------------------------------------------------------

/// Estado observável do backend Tor (reportado por `ESTADO 0x03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EstadoTor {
    /// Ainda não arrancou (ou foi parado).
    #[default]
    Parado,
    /// Bootstrap em curso (visível no backend real).
    Arrancando,
    /// Hidden service publicado e pronto a receber.
    Ativo,
}

impl EstadoTor {
    /// Código reportado pelo IPC (`docs/ipc_spec.md` §`ESTADO`).
    pub fn para_ipc(self) -> u8 {
        match self {
            EstadoTor::Parado => 0x00,
            EstadoTor::Arrancando => 0x01,
            EstadoTor::Ativo => 0x02,
        }
    }
}

// ---------------------------------------------------------------------
// Erros do backend
// ---------------------------------------------------------------------

/// Falha do backend Tor — nunca transporta segredos, só metadados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroTor {
    /// Operação de rede exigiu um backend que ainda não arrancou.
    NaoArrancado,
    /// Destino sem entrada no diretório do backend (apenas `TorFalso`).
    DestinoDesconhecido(String),
    /// Falha reportada pelo backend real (bootstrap/ligação/proxy).
    Falha(String),
}

impl fmt::Display for ErroTor {
    /// Mensagem legível para a resposta IPC — sem chaves nem payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroTor::NaoArrancado => write!(f, "backend Tor ainda não arrancou"),
            ErroTor::DestinoDesconhecido(d) => write!(f, "destino desconhecido: {d}"),
            ErroTor::Falha(m) => write!(f, "backend Tor: {m}"),
        }
    }
}

impl ErroTor {
    /// Código de erro IPC correspondente (`docs/ipc_spec.md`).
    pub fn para_ipc(&self) -> u8 {
        match self {
            // Sem backend ativo → 0x0B; destino malformado → 0x0D.
            ErroTor::NaoArrancado => 0x0B,
            ErroTor::DestinoDesconhecido(_) => 0x0D,
            ErroTor::Falha(_) => 0x0B,
        }
    }
}

// ---------------------------------------------------------------------
// Traço do backend
// ---------------------------------------------------------------------

/// Operações que o daemon precisa do Tor — e apenas essas.
pub trait BackendTor: Send {
    /// Publica o hidden service encaminhando ENTRADAS para `porta_local`
    /// (o servidor P2P deste nó) e devolve o endereço `.onion`.
    ///
    /// Chamadas repetidas são idempotentes: devolvem sempre o mesmo
    /// endereço enquanto o backend estiver `Ativo`.
    fn arrancar(&mut self, porta_local: u16) -> Result<String, ErroTor>;

    /// Para o backend e limpa o estado. Idempotente.
    fn parar(&mut self) -> Result<(), ErroTor>;

    /// Estado atual (para o comando `ESTADO`).
    fn estado(&self) -> EstadoTor;

    /// Endereço `.onion` publicado, se o backend estiver `Ativo`.
    fn onion(&self) -> Option<&str>;

    /// Abre uma ligação de SAÍDA ao `destino` (`.onion` ou ID).
    fn ligar(&mut self, destino: &str) -> Result<TcpStream, ErroTor>;
}

// ---------------------------------------------------------------------
// TorFalso — loopback puro (testes e modo sem rede)
// ---------------------------------------------------------------------

/// Diretório partilhado `onion → porta loopback` do `TorFalso`.
///
/// Como todos os "pares" de um teste correm no mesmo processo, um
/// diretório global basta para que A encontre a porta do B — é o
/// substituto da rede, sem qualquer fio de rede real.
fn registro() -> &'static Mutex<HashMap<String, u16>> {
    static REGISTRO: OnceLock<Mutex<HashMap<String, u16>>> = OnceLock::new();
    REGISTRO.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Backend falso: gera um `.onion` plausível e resolve-o para uma porta
/// de loopback registada por outro `TorFalso` no mesmo processo.
#[derive(Debug, Default)]
pub struct TorFalso {
    /// Endereço publicado por `arrancar` (None = parado).
    onion: Option<String>,
    /// Porta local registada neste backend (para limpeza em `parar`).
    porta_local: Option<u16>,
    /// Estado corrente.
    estado: EstadoTor,
}

impl TorFalso {
    /// Cria um backend falso já parado.
    pub fn novo() -> Self {
        Self::default()
    }
}

/// Codificação base32 (RFC 4648) em minúsculas e sem padding — o
/// formato dos endereços `.onion` v3 (35 bytes → 56 caracteres).
fn para_base32(dados: &[u8]) -> String {
    const ALFABETO: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut saida = String::with_capacity(dados.len().div_ceil(5) * 8);
    let mut acumulado: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in dados {
        acumulado = (acumulado << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            saida.push(ALFABETO[((acumulado >> bits) & 0x1F) as usize] as char);
        }
    }
    // Sobras: alinha à esquerda e completa o último grupo de 5 bits.
    if bits > 0 {
        saida.push(ALFABETO[((acumulado << (5 - bits)) & 0x1F) as usize] as char);
    }
    saida
}

impl BackendTor for TorFalso {
    fn arrancar(&mut self, porta_local: u16) -> Result<String, ErroTor> {
        // Idempotente: o mesmo backend nunca muda de endereço.
        if let Some(onion) = &self.onion {
            return Ok(onion.clone());
        }
        // 35 bytes aleatórios → base32 = 56 chars (igual a um v3 real).
        let mut semeadura = [0u8; 35];
        crypto_core::rng::preencher(&mut semeadura);
        let endereco = format!("{}.onion", para_base32(&semeadura));
        registro()
            .lock()
            .expect("mutex do registro nunca envenenado")
            .insert(endereco.clone(), porta_local);
        self.onion = Some(endereco.clone());
        self.porta_local = Some(porta_local);
        self.estado = EstadoTor::Ativo;
        Ok(endereco)
    }

    fn parar(&mut self) -> Result<(), ErroTor> {
        if let Some(onion) = self.onion.take() {
            registro()
                .lock()
                .expect("mutex do registro nunca envenenado")
                .remove(&onion);
        }
        self.porta_local = None;
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
        if self.estado != EstadoTor::Ativo {
            return Err(ErroTor::NaoArrancado);
        }
        // Resolução "DNS" falsa: procura a porta registada pelo par.
        let porta = registro()
            .lock()
            .expect("mutex do registro nunca envenenado")
            .get(destino)
            .copied()
            .ok_or_else(|| ErroTor::DestinoDesconhecido(destino.to_string()))?;
        // Não existe par → `ConnectionRefused` propaga-se como Falha? Não:
        // o erro de TCP é do chamador (p2p), aqui só resolvemos o destino.
        TcpStream::connect(("127.0.0.1", porta))
            .map_err(|erro| ErroTor::Falha(format!("ligação recusada em porta {porta}: {erro}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----------------------------------------------------------------
    // Base32 — formato dos endereços .onion
    // ----------------------------------------------------------------

    /// Vectores RFC 4648 (secção 10) — trancam o codificador.
    #[test]
    fn base32_vectores_rfc4648() {
        assert_eq!(para_base32(b""), "");
        assert_eq!(para_base32(b"f"), "my");
        assert_eq!(para_base32(b"fo"), "mzxq");
        assert_eq!(para_base32(b"foo"), "mzxw6");
        assert_eq!(para_base32(b"foob"), "mzxw6yq");
        assert_eq!(para_base32(b"fooba"), "mzxw6ytb");
        assert_eq!(para_base32(b"foobar"), "mzxw6ytboi");
        // Casos de bordo: bytes de saturação e alinhamento por bits.
        // 2 bytes = 16 bits → 4 dígitos (o 16.º bit encaixa sozinho).
        assert_eq!(para_base32(&[0xFF, 0xFF]), "777q");
        assert_eq!(para_base32(&[0x00, 0x01, 0x02]), "aaaqe");
        // Múltiplo de 5 bytes → sem resto, 8 dígitos todos a saturação.
        assert_eq!(para_base32(&[0xFF; 5]), "77777777");
        // 35 bytes (tamanho de um onion v3) → 56 dígitos.
        assert_eq!(para_base32(&[0u8; 35]).len(), 56);
    }

    /// Um onion falso tem exatamente a forma de um v3 real.
    #[test]
    fn onion_falso_tem_formato_v3() {
        let mut backend = TorFalso::novo();
        let onion = backend.arrancar(9000).expect("arranca");
        // 56 caracteres base32 + ".onion" = 62 no total.
        assert_eq!(onion.len(), 62);
        assert!(onion.ends_with(".onion"));
        let raiz = &onion[..56];
        assert!(
            raiz.chars()
                .all(|c| c.is_ascii_lowercase() || ("234567".contains(c))),
            "fora do alfabeto base32: {raiz}"
        );
        assert_ne!(raiz, para_base32(&[0u8; 35])); // aleatório ≠ zeros
    }

    // ----------------------------------------------------------------
    // Ciclo de vida do TorFalso
    // ----------------------------------------------------------------

    /// `arrancar` publica, é idempotente e `parar` limpa tudo.
    #[test]
    fn ciclo_de_vida_do_tor_falso() {
        let mut backend = TorFalso::novo();
        assert_eq!(backend.estado(), EstadoTor::Parado);
        assert_eq!(backend.onion(), None);

        let primeira = backend.arrancar(9101).expect("arranca");
        assert_eq!(backend.estado(), EstadoTor::Ativo);
        assert_eq!(backend.onion(), Some(primeira.as_str()));

        // Idempotência: segunda chamada devolve o mesmo endereço.
        assert_eq!(backend.arrancar(9999).expect("2.ª"), primeira);
        assert_eq!(backend.porta_local, Some(9101), "porta não é regravada");

        // Parar volta ao estado inicial e limpa o registro.
        backend.parar().expect("para");
        assert_eq!(backend.estado(), EstadoTor::Parado);
        assert_eq!(backend.onion(), None);
        assert_eq!(backend.porta_local, None);
        // Parar sem nunca ter arrancado também é seguro (idempotente).
        backend.parar().expect("2.ª paragem");
        assert_eq!(backend.estado(), EstadoTor::Parado);
    }

    /// Ligar sem arrancar → `NaoArrancado` (0x0B).
    #[test]
    fn ligar_sem_arrancar_rejeitado() {
        let mut backend = TorFalso::novo();
        let erro = backend.ligar("abc.onion").expect_err("tem de falhar");
        assert_eq!(erro, ErroTor::NaoArrancado);
        assert_eq!(erro.para_ipc(), 0x0B);
    }

    /// Destino sem registro → `DestinoDesconhecido` (0x0D).
    #[test]
    fn ligar_destino_desconhecido() {
        let mut backend = TorFalso::novo();
        backend.arrancar(9102).expect("arranca");
        let erro = backend
            .ligar("inexistente.onion")
            .expect_err("sem registro");
        assert_eq!(
            erro,
            ErroTor::DestinoDesconhecido("inexistente.onion".into())
        );
        assert_eq!(erro.para_ipc(), 0x0D);
        assert!(erro.to_string().contains("inexistente.onion"));
    }

    /// Duas instâncias falam entre si: A resolve a porta de B e liga.
    #[test]
    fn ligacao_entre_dois_backends_falsos() {
        // Servidor de teste (proxy do P2P de B) em loopback — a porta só
        // é conhecida depois do bind, por isso B arranca já com ela.
        let servidor = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let porta_real = servidor.local_addr().expect("addr").port();
        let mut b = TorFalso::novo();
        let onion_b = b.arrancar(porta_real).expect("B arranca");

        // A liga ao endereço de B (resolvido pelo registro partilhado).
        let mut a = TorFalso::novo();
        a.arrancar(9104).expect("A arranca");
        let mut fluxo = a.ligar(&onion_b).expect("liga a B");

        // Handshake TCP local: B aceita, A escreve, B lê.
        let (mut lado_b, _) = servidor.accept().expect("accept");
        use std::io::{Read, Write};
        fluxo.write_all(b"ping").expect("escreve");
        let mut buf = [0u8; 4];
        lado_b.read_exact(&mut buf).expect("lê");
        assert_eq!(&buf, b"ping");
        b.parar().expect("B fecha");
        a.parar().expect("A fecha");
    }

    /// Porta registada sem servidor → erro de TCP vira `Falha` (0x0B).
    #[test]
    fn ligacao_recusada_vira_falha() {
        // Porta efémera sem escuta (reservamo-la e fechámo-la).
        let servidor = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let porta_morta = servidor.local_addr().expect("addr").port();
        drop(servidor);

        let mut dono = TorFalso::novo();
        let onion = dono.arrancar(porta_morta).expect("arranca");
        let mut a = TorFalso::novo();
        a.arrancar(9105).expect("A");
        let erro = a.ligar(&onion).expect_err("porta morta");
        assert!(matches!(erro, ErroTor::Falha(_)), "{erro:?}");
        assert_eq!(erro.para_ipc(), 0x0B);
        assert!(erro.to_string().contains("recusada"));
        dono.parar().expect("limpa");
        a.parar().expect("limpa");
    }

    // ----------------------------------------------------------------
    // Contrato partilhado (estado/erros)
    // ----------------------------------------------------------------

    /// Mapeamento IPC de `EstadoTor` e `ErroTor` — tabela completa.
    #[test]
    fn mapeamentos_de_estado_e_erro() {
        assert_eq!(EstadoTor::Parado.para_ipc(), 0x00);
        assert_eq!(EstadoTor::Arrancando.para_ipc(), 0x01);
        assert_eq!(EstadoTor::Ativo.para_ipc(), 0x02);

        assert_eq!(ErroTor::NaoArrancado.para_ipc(), 0x0B);
        assert_eq!(ErroTor::Falha("x".into()).para_ipc(), 0x0B);
        assert_eq!(ErroTor::DestinoDesconhecido("y".into()).para_ipc(), 0x0D);

        assert_eq!(
            ErroTor::NaoArrancado.to_string(),
            "backend Tor ainda não arrancou"
        );
        assert_eq!(
            ErroTor::Falha("bootstrap".into()).to_string(),
            "backend Tor: bootstrap"
        );
    }

    /// O traço aceita qualquer `BackendTor` (sanidade do objecto dinâmico).
    #[test]
    fn traco_aceita_backend_falso() {
        let mut backend: Box<dyn BackendTor> = Box::new(TorFalso::novo());
        let endereco = backend.arrancar(9106).expect("arranca");
        assert!(endereco.ends_with(".onion"));
        assert_eq!(backend.estado(), EstadoTor::Ativo);
        backend.parar().expect("para");
        assert_eq!(backend.estado(), EstadoTor::Parado);
    }

    /// Portas registadas por backends não interferem entre si.
    #[test]
    fn registro_e_isolado_por_endereco() {
        let mut um = TorFalso::novo();
        let mut dois = TorFalso::novo();
        let onion_um = um.arrancar(9201).expect("1");
        let onion_dois = dois.arrancar(9202).expect("2");
        assert_ne!(onion_um, onion_dois, "onions têm de ser únicos");

        // Cada um resolve a porta que publicou.
        let mut cliente = TorFalso::novo();
        cliente.arrancar(9203).expect("cliente");
        // Sanity: os endpoints registados são distintos.
        let mapa = registro().lock().expect("lock");
        assert_eq!(mapa.get(&onion_um), Some(&9201));
        assert_eq!(mapa.get(&onion_dois), Some(&9202));
        drop(mapa);
        um.parar().expect("limpa");
        dois.parar().expect("limpa");
        cliente.parar().expect("limpa");
    }
}
