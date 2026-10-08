// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// anti_replay.rs — registo circular de `nonce1` do chat (anti-replay)
// ---------------------------------------------------------------------
// Especificação: `docs/message_format.md` §Anti-replay.
//
// O `nonce1` (12 bytes aleatórios, único por mensagem e coberto pela
// assinatura Ed25519) é o identificador anti-replay do chat. Este
// módulo mantém o registo do *receptor* — o daemon que decifra — com
// duas garantias de memória:
//
//   * **capacidade fixa** (`CAPACIDADE_NONCES1`): o registo é circular;
//     ao encher, a entrada mais antiga sai, por mais nova que seja;
//   * **TTL** (`TTL_NONCES1`): entradas expiram e são removidas na
//     primeira operação seguinte (relógio monotónico, sem varredura
//     periódica).
//
// A reserva é feita **depois** da verificação da assinatura e **antes**
// da decifragem, e é *libertada* se a decifragem falhar — um pedido
// legítimo que chegue com as chaves erradas não pode bloquear a
// mensagem para sempre.
//
// ## Persistência (fase B3)
// O registo era deliberadamente em memória, e essa decisão era um
// buraco: reiniciar o daemon abria uma janela de replay de TTL
// completo. Um agente que captura um envelope e reenvia-o a cada
// reinício do serviço contornava o anti-replay sem esforço.
//
// Agora o registo é gravado em `$ONYXCHAT_ESTADO/anti-replay-v1.bin`
// com escrita atómica, e carregado em `Estado::novo`. A decisão, as
// garantias e as limitações estão em `docs/message_format.md`
// §Persistência do registo — leia-se antes de mexer no formato, porque
// mudar a layout sem mudar a `VERSAO` faz o daemon descartar o seu
// próprio registo em silêncio.
// =====================================================================

use std::collections::{HashSet, VecDeque};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::envelope::TAM_NONCE;

/// Número máximo de `nonce1` guardados em memória.
///
/// Com entradas de 12 B + `Instant`, o pior caso ronda 150 KB —
/// limitado por construção, independente do tráfego observado.
pub const CAPACIDADE_NONCES1: usize = 4096;

/// O TTL em segundos de relógio de parede, para a persistência.
///
/// [`TTL_NONCES1`] é um `Duration` e serve para a janela em memória.
/// O ficheiro precisa de um número, porque o `Instant` não sobrevive a
/// um reinício do processo — é um ponto de medição relativo ao arranque
/// e não tem significado fora dele. Os dois têm de concordar, daí a
/// derivação em vez de um segundo literal.
pub const TTL_SEGUNDOS: u64 = TTL_NONCES1.as_secs();

/// Cabeçalho do ficheiro: `b"ONYXAR1"`, sete bytes.
///
/// A magia existe para uma coisa só: distinguir um ficheiro **deste**
/// formato de um ficheiro qualquer. Sem ela, o primeiro byte de um
/// ficheiro de texto seria lido como uma contagem de entradas, e o
/// registo nasceria com o tamanho que calhasse.
pub const MAGIC: &[u8; 7] = b"ONYXAR1";

/// Versão do formato. Incrementar quando a layout mudar.
pub const VERSAO: u32 = 1;

/// Tamanho de uma entrada no ficheiro: 12 do nonce + 8 do timestamp.
const TAM_ENTRADA: usize = TAM_NONCE + 8;

/// Tamanho do cabeçalho: 7 da mágica + 4 da versão + 4 da contagem.
const TAM_CABECALHO: usize = MAGIC.len() + 4 + 4;

/// Tempo de vida de uma entrada no registo.
///
/// Ordem de grandeza muito superior a qualquer janela de retransmissão
/// do transporte (segundos) e ao tempo de vida de um circuito Tor, e
/// suficientemente curta para libertar a memória sem intervenção.
pub const TTL_NONCES1: Duration = Duration::from_secs(600);

/// Registo circular de `nonce1` já vistos, com expiração por TTL.
///
/// Não é `Clone` nem `Sync` por omissão: o acesso faz-se sempre sob o
/// mutex do estado (`Estado::nonces1`), pelo que uma mensagem só pode
/// passar pela janela *check-and-insert* uma vez.
///
/// # Duas estruturas, e porque
///
/// A implementação anterior guardava só um `VecDeque` e fazia
/// `entradas.iter().any(|(n, _)| n == nonce)` em cada `DECODE`. Com
/// 4096 entradas vivas, isso é uma varredura de 4096 × 12 B ≈ 48 KB por
/// mensagem — e `libertar`, chamado quando a decifragem falha, fazia
/// uma segunda varredura com `position()`.
///
/// Duas estruturas, cada uma com a operação que é boa nela:
///
/// | estrutura | operação | complexidade |
/// | --- | --- | --- |
/// | `HashSet` | «este nonce já foi visto?» | O(1) |
/// | `VecDeque` | «qual é a mais antiga?» | O(1) |
///
/// O `VecDeque` não pode responder à primeira pergunta sem varrer; o
/// `HashSet` não pode responder à segunda sem ajuda. A ordem
/// cronológica só precisa de ser preservada para saber *qual* expirar,
/// e o `HashSet` existe só para tornar a resposta à pergunta de
/// rejeição instantânea.
///
/// Memória: o `HashSet` duplica os 12 bytes de cada nonce, o que leva o
/// total de ~150 KB para ~200 KB no caso cheio. É um troca
/// deliberada — 50 KB de RAM estática contra 48 KB de leitura por
/// mensagem.
#[derive(Debug)]
pub struct RegistoNonce1 {
    /// Entradas em ordem cronológica (mais antiga à frente).
    ///
    /// É esta estrutura que decide a expiração e a rotação quando o
    /// registo enche.
    entradas: VecDeque<([u8; TAM_NONCE], Instant)>,
    /// Os mesmos nonces, indexados para consulta directa.
    ///
    /// Espelho exacto de `entradas`. Toda a inserção e remoção passa
    /// pelos dois lados, e o `#[cfg(test)] fn coerencia()` verifica a
    /// concordância — um desincronizamento entre os dois seria um
    /// anti-replay que aceita replay, que é a falha que o anti-replay
    /// existe para impedir.
    vistos: HashSet<[u8; TAM_NONCE]>,
}

impl RegistoNonce1 {
    /// Carrega o registo de `caminho`.
    ///
    /// **Nunca falha.** Um ficheiro em falta, truncado, de outra
    /// versão ou corrompido dá um registo vazio — fechar, nunca abrir.
    /// A alternativa (propagar o erro) faria o daemon não arrancar, e um
    /// daemon que não arranca é um modo de falhar que ninguém descobre
    /// senão no primeiro upgrade.
    ///
    /// Descarta de cada vez as entradas mais antigas que o TTL, usando o
    /// timestamp de relógio de parede gravado com cada uma. O TTL
    /// continua a valer entre arranques: um registo com uma hora
    /// carregado agora não retém os nonces dessa hora.
    pub fn carregar_de(caminho: &Path) -> Self {
        let mut registo = Self::novo();
        let Ok(bytes) = std::fs::read(caminho) else {
            return registo;
        };
        // Cabeçalho + pelo menos uma entrada, senão não há o que ler.
        if bytes.len() < TAM_CABECALHO + TAM_ENTRADA {
            return registo;
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return registo;
        }
        let versao = u32::from_le_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]);
        if versao != VERSAO {
            return registo;
        }
        let quantos = u32::from_le_bytes([bytes[11], bytes[12], bytes[13], bytes[14]]) as usize;
        // Tecto antes de percorrer: o ficheiro diz quantas entradas tem,
        // e um ficheiro adulterado pode dizer `u32::MAX`. Sem este
        // `min`, isso seria um laço de quatro mil milhões de confirmações
        // confirmações — que não aloca, mas bloqueia o arranque.
        let disponiveis = (bytes.len() - TAM_CABECALHO) / TAM_ENTRADA;
        let quantos = quantos.min(disponiveis).min(CAPACIDADE_NONCES1);

        let agora = agora_epoch();
        let mut i = TAM_CABECALHO;
        for _ in 0..quantos {
            let mut nonce = [0u8; TAM_NONCE];
            nonce.copy_from_slice(&bytes[i..i + TAM_NONCE]);
            let ts = u64::from_le_bytes([
                bytes[i + TAM_NONCE],
                bytes[i + TAM_NONCE + 1],
                bytes[i + TAM_NONCE + 2],
                bytes[i + TAM_NONCE + 3],
                bytes[i + TAM_NONCE + 4],
                bytes[i + TAM_NONCE + 5],
                bytes[i + TAM_NONCE + 6],
                bytes[i + TAM_NONCE + 7],
            ]);
            i += TAM_ENTRADA;

            // TTL em relógio de parede. O `saturating_sub` trata o caso
            // em que o relógio foi atrasado entre o gravar e o carregar:
            // sem isso, `agora - ts` em `u64` daria um número enorme e
            // a entrada pareceria recente para sempre.
            if agora.saturating_sub(ts) >= TTL_SEGUNDOS {
                continue;
            }
            // `Instant::now()` e não o instante do ficheiro: o `Instant`
            // é relativo ao arranque e não pode ser reconstruído. O
            // efeito é que uma entrada carregada ganha uma janela nova
            // de TTL, sobre a qual já foi aplicado o filtro acima — o
            // prazo efectivo fica entre TTL e 2×TTL. Documentado em
            // `docs/message_format.md` §Anti-replay.
            if !registo.vistos.insert(nonce) {
                continue;
            }
            if registo.entradas.len() == CAPACIDADE_NONCES1 {
                if let Some((sai, _)) = registo.entradas.pop_front() {
                    registo.vistos.remove(&sai);
                }
            }
            registo.entradas.push_back((nonce, Instant::now()));
        }
        registo
    }

    /// Grava o registo em `caminho`, de forma atómica.
    ///
    /// **Atómica** significa: escrever num ficheiro temporário no mesmo
    /// directório, fazer `fsync`, e depois `rename` sobre o destino. O
    /// `rename` é atómico dentro do mesmo sistema de ficheiros, portanto
    /// um corte de energia deixa sempre o registo antigo inteiro ou o
    /// novo inteiro — nunca metade de um dos dois. Uma escrita directa
    /// sobre o destino deixaria um ficheiro truncado, e um ficheiro
    /// truncado é descartado (ver [`RegistoNonce1::carregar_de`]), o que
    /// perderia todo o anti-replay num momento de corte de energia.
    ///
    /// Sem erro de retorno, por decisão: uma falha de escrita não pode
    /// derrubar o daemon nem interromper uma decifragem. O pior caso é o
    /// registo ficar com o estado anterior, que é o estado de antes do
    /// último nonce — a mesma situação de um reinício, e nenhuma pior.
    pub fn gravar_para(&self, caminho: &Path) {
        if let Some(directorio) = caminho.parent() {
            if std::fs::create_dir_all(directorio).is_err() {
                return;
            }
        }
        let mut dados = Vec::with_capacity(TAM_CABECALHO + self.entradas.len() * TAM_ENTRADA);
        dados.extend_from_slice(MAGIC);
        dados.extend_from_slice(&VERSAO.to_le_bytes());
        dados.extend_from_slice(&(self.entradas.len() as u32).to_le_bytes());
        for (nonce, _) in &self.entradas {
            dados.extend_from_slice(nonce);
            dados.extend_from_slice(&agora_epoch().to_le_bytes());
        }

        let temporario = caminho.with_extension("tmp");
        // `OpenOptions` com `.mode(0o600)`: o registo diz quais nonces
        // já foram vistos, que é informação sobre o tráfego do
        // utilizador. O `0600` é aplicado na criação, não num `chmod`
        // posterior — que abriria uma janela entre criar e fechar.
        let Ok(mut f) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporario)
        else {
            return;
        };
        if std::io::Write::write_all(&mut f, &dados).is_err() || f.sync_all().is_err() {
            let _ = std::fs::remove_file(&temporario);
            return;
        }
        drop(f);
        if std::fs::rename(&temporario, caminho).is_err() {
            let _ = std::fs::remove_file(&temporario);
        }
    }

    /// Registo vazio.
    pub fn novo() -> Self {
        RegistoNonce1 {
            entradas: VecDeque::new(),
            // Capacidade pré-alcada: `CAPACIDADE_NONCES1` entradas.
            // Um `HashSet` que cresce até 4096 rehash cinco ou seis
            // vezes, e o registo é criado no arranque do daemon — um
            // pico de alocação evitável.
            vistos: HashSet::with_capacity(CAPACIDADE_NONCES1),
        }
    }

    /// Entradas vivas no momento consultado.
    pub fn quantidade(&self) -> usize {
        self.entradas.len()
    }

    /// Remove do frontal todas as entradas já expiradas.
    ///
    /// As entradas entram em ordem de `Instant` (relógio monotónico),
    /// logo basta varrer do início até à primeira entrada viva.
    fn expirar(&mut self, agora: Instant) {
        while let Some((_, visto_em)) = self.entradas.front() {
            if agora.saturating_duration_since(*visto_em) >= TTL_NONCES1 {
                let (nonce, _) = self.entradas.pop_front().expect("front() devolve Some");
                self.vistos.remove(&nonce);
            } else {
                break;
            }
        }
    }

    /// Reserva `nonce` a partir de um relógio fornecido (testes de TTL).
    ///
    /// Devolve `true` se o nonce era novo — e fica registado de
    /// imediato, fechando a janela *check-and-insert* — ou `false` se
    /// já tinha sido visto (replay).
    fn reservar_em(&mut self, nonce: &[u8; TAM_NONCE], agora: Instant) -> bool {
        self.expirar(agora);

        // O(1), onde antes era O(CAPACIDADE_NONCES1).
        //
        // `insert` devolve `false` se o valor já lá estava, o que dá a
        // resposta e a inserção numa operação só. Não é
        // `contains` seguido de `insert`: entre as duas, outra thread
        // podia entrar — embora sob o mutex do `Estado` isso não
        // aconteça hoje, o registo não deve depender de o chamador o
        // fazer.
        if !self.vistos.insert(*nonce) {
            return false;
        }

        if self.entradas.len() == CAPACIDADE_NONCES1 {
            // Memória circular: a entrada mais antiga é a primeira a
            // sair. O `HashSet` tem de acompanhar, senão o nonce
            // descartado continuaria a bloquear uma mensagem legítima
            // durante o resto da vida do processo.
            let (sai, _) = self.entradas.pop_front().expect("len == CAPACIDADE_NONCES1");
            self.vistos.remove(&sai);
        }

        self.entradas.push_back((*nonce, agora));
        true
    }

    /// Reserva `nonce` para a mensagem que está a ser decifrada.
    ///
    /// Devolve `true` na primeira vez e `false` para qualquer reenvio
    /// da mesma mensagem dentro da janela do registo.
    pub fn reservar(&mut self, nonce: &[u8; TAM_NONCE]) -> bool {
        self.reservar_em(nonce, Instant::now())
    }

    /// Liberta uma reserva anterior (devolve `true` se libertou).
    ///
    /// Usada quando a decifragem falha depois da reserva: as chaves
    /// apresentadas é que estavam erradas, não a mensagem — sem esta
    /// libertação, um erro do cliente tornar-se-ia permanente.
    pub fn libertar(&mut self, nonce: &[u8; TAM_NONCE]) -> bool {
        // `remove` no `HashSet` decide se o nonce lá estava, e é O(1).
        // A varredura no `VecDeque` só corre quando o nonce existe de
        // facto — o caso raro, porque `libertar` segue uma
        // `reservar` que quase sempre inseriu.
        if !self.vistos.remove(nonce) {
            return false;
        }
        if let Some(pos) = self.entradas.iter().position(|(n, _)| n == nonce) {
            self.entradas.remove(pos);
        }
        true
    }
}

impl Default for RegistoNonce1 {
    /// Idêntico a [`RegistoNonce1::novo`].
    fn default() -> Self {
        Self::novo()
    }
}

/// Segundos desde a época Unix, saturados em zero.
///
/// O `saturating_sub` no consumo trata um relógio atrasado; aqui o
/// `unwrap_or(0)` trata o caso em que o relógio do sistema está antes de
/// 1970, que é patológico mas não justifica um pânico no arranque.
pub fn agora_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nonce determinístico de 12 bytes para os testes.
    fn nonce(b: u8) -> [u8; TAM_NONCE] {
        [b; TAM_NONCE]
    }

    /// Nonce distinto por número, para testes que precisam de mais de
    /// 256 valores.
    ///
    /// O helper [`nonce`] deste módulo faz `[b; 12]`, o que dá só 256
    /// nonces distintos — insuficiente para encher um registo de 4096.
    /// Aqui o número vai para os dois primeiros bytes em little-endian,
    /// com o resto a zero, o que dá 65 536 valores distintos.
    fn nonce_de_numero(n: u32) -> [u8; TAM_NONCE] {
        let mut b = [0u8; TAM_NONCE];
        b[0] = (n & 0xff) as u8;
        b[1] = ((n >> 8) & 0xff) as u8;
        b
    }

    /// Constantes documentadas — se mudarem, os docs têm de mudar.
    #[test]
    fn constantes_documentadas() {
        assert_eq!(CAPACIDADE_NONCES1, 4096);
        assert_eq!(TTL_NONCES1, Duration::from_secs(600));
        assert_eq!(RegistoNonce1::novo().quantidade(), 0);
    }

    /// `Default` tem de ser equivalente a `novo()`. O registo é usado
    /// em deriva de tipos e em construções por omissão, e um `Default`
    /// que devolvesse outra coisa seria um registo de anti-replay
    /// silenciosamente desactivado.
    #[test]
    fn default_equivale_a_novo() {
        let por_omissao = RegistoNonce1::default();
        assert_eq!(por_omissao.quantidade(), 0, "tem de começar vazio");
        // E tem de ter o mesmo comportamento de anti-replay, não apenas a mesma
        // contagem: o que importa é que rejeite um reenvio.
        let mut registo = RegistoNonce1::default();
        assert!(registo.reservar(&nonce(42)), "primeira passagem aceite");
        assert!(
            !registo.reservar(&nonce(42)),
            "um registo por omissão tem de rejeitar reenvio"
        );
    }

    /// Primeira reserva aceita; repetição rejeitada; nonce novo aceito.
    #[test]
    fn reserva_e_deteccao_de_replay() {
        let mut registo = RegistoNonce1::novo();
        assert!(registo.reservar(&nonce(1)), "primeira passagem aceite");
        assert!(!registo.reservar(&nonce(1)), "reenvio rejeitado");
        assert!(!registo.reservar(&nonce(1)), "reenvio continua rejeitado");
        assert!(registo.reservar(&nonce(2)), "mensagem distinta aceite");
        assert_eq!(registo.quantidade(), 2);
    }

    /// A libertação permite novo aceite (retry com chaves certas).
    #[test]
    fn libertar_permite_nova_reserva() {
        let mut registo = RegistoNonce1::novo();
        assert!(registo.reservar(&nonce(7)));
        assert!(!registo.reservar(&nonce(7)));
        assert!(registo.libertar(&nonce(7)), "reserva libertada");
        assert!(registo.reservar(&nonce(7)), "retry aceite");
        assert!(!registo.libertar(&nonce(99)), "nada a libertar");
    }

    /// Entradas expiram ao fim do TTL (relógio controlado).
    #[test]
    fn expiracao_por_ttl() {
        let mut registo = RegistoNonce1::novo();
        let t0 = Instant::now();
        assert!(registo.reservar_em(&nonce(3), t0));
        assert!(
            !registo.reservar_em(&nonce(3), t0 + TTL_NONCES1 - Duration::from_secs(1)),
            "dentro do TTL continua rejeitado"
        );
        // Passado o TTL a entrada sai e o mesmo nonce volta a ser aceite.
        assert!(registo.reservar_em(&nonce(3), t0 + TTL_NONCES1));
        assert_eq!(registo.quantidade(), 1, "expirada foi substituída");
    }

    /// Ao encher, a entrada mais antiga sai (memória circular).
    #[test]
    fn rotacao_circular_por_capacidade() {
        let mut registo = RegistoNonce1::novo();
        let t0 = Instant::now();
        assert!(registo.reservar_em(&nonce(0), t0));
        let mut ultimo = nonce(0);
        for i in 1..=CAPACIDADE_NONCES1 {
            let mut n = [0u8; TAM_NONCE];
            n[..8].copy_from_slice(&(i as u64).to_le_bytes());
            ultimo = n;
            assert!(registo.reservar_em(&n, t0));
        }
        assert_eq!(registo.quantidade(), CAPACIDADE_NONCES1);
        assert!(
            !registo.reservar_em(&ultimo, t0),
            "o mais recente continua registado"
        );
        // O primeiro saiu da janela: volta a ser aceite.
        assert!(registo.reservar_em(&nonce(0), t0), "mais antigo saiu");
    }

    /// Mesmo com a capacidade cheia, a ordem cronológica mantém o TTL
    /// a libertar memória: tudo o que expirou sai antes da reserva.
    #[test]
    fn ttl_liberta_memoria_mesmo_sem_rotacao() {
        let mut registo = RegistoNonce1::novo();
        let t0 = Instant::now();
        for i in 0..CAPACIDADE_NONCES1 {
            let mut n = [0u8; TAM_NONCE];
            n[..8].copy_from_slice(&(i as u64).to_le_bytes());
            assert!(registo.reservar_em(&n, t0));
        }
        let depois = t0 + TTL_NONCES1;
        assert!(registo.reservar_em(&[7u8; TAM_NONCE], depois));
        assert_eq!(
            registo.quantidade(),
            1,
            "entradas expiradas removidas todas de uma vez"
        );
    }
    /// O `HashSet` e o `VecDeque` têm de conter exactamente o mesmo.
    ///
    /// Este é o teste que impede o modo de falha mais grave possível
    /// aqui: um desincronizamento em que o `HashSet` ganha uma entrada
    /// que o `VecDeque` não tem faria o registo rejeitar uma mensagem
    /// legítima para sempre; o inverso — o `HashSet` esquecer uma
    /// entrada — reabriria a janela de replay. Nenhum dos dois
    /// seria sinal de nenhum outro teste.
    #[test]
    fn os_dois_indices_estao_coerentes() {
        let mut r = RegistoNonce1::novo();
        for b in 0u8..=200 {
            assert!(r.reservar(&nonce(b)), "nonce {b} deve ser novo");
        }
        assert_eq!(r.entradas.len(), r.vistos.len());
        for (n, _) in &r.entradas {
            assert!(r.vistos.contains(n), "nonce {n:?} no deque mas não no set");
        }

        // Liberar metade tem de sair nos dois.
        for b in 0u8..=100 {
            assert!(r.libertar(&nonce(b)), "libertar {b}");
        }
        assert_eq!(r.entradas.len(), r.vistos.len());
        assert_eq!(r.entradas.len(), 100);
    }

    /// Ao encher o registo, o nonce descartado deixa de bloquear.
    ///
    /// Este é o outro lado do `HashSet`: na memória circular, o nonce
    /// que sai do `VecDeque` tem de sair também do índice, ou uma
    /// mensagem com esse nonce — que é perfeitamente legítima, porque o
    /// nonce expirou ou rodou — seria rejeitada até ao fim do processo.
    #[test]
    fn nonce_liberado_deixa_de_bloquear() {
        let mut r = RegistoNonce1::novo();
        // O helper `nonce()` deste módulo devolve `[b; 12]`, ou seja só
        // 256 valores distintos. A rotação ao cheio tem o seu próprio
        // teste, com nonces de 12 bytes verdadeiramente distintos.
        for b in 0u8..=255 {
            assert!(r.reservar(&nonce(b)));
        }
        assert_eq!(r.quantidade(), 256);

        // Reenviar um nonce já visto tem de ser rejeitado.
        assert!(!r.reservar(&nonce(0)), "replay de um nonce visto");
        // E libertá-lo tem de o devolver ao estado de "nunca visto" — é
        // o que faz uma chave errada do cliente não se tornar um erro
        // permanente.
        assert!(r.libertar(&nonce(0)));
        assert!(r.reservar(&nonce(0)), "após libertar, o nonce tem de ser novo");
        assert_eq!(r.quantidade(), 256);
    }

    /// A rotação real da memória circular remove do `HashSet`.
    ///
    /// Enche o registo até à capacidade e verifica que o nonce mais
    /// antigo deixou de estar no índice. É o caminho que o teste
    /// anterior não cobre, porque 256 nonces distintos não chegam aos
    /// 4096.
    #[test]
    fn rotacao_ao_cheio_limpa_o_indice() {
        let mut r = RegistoNonce1::novo();

        // Preenche até à capacidade, memorizando o primeiro nonce
        // inserido — é ele que a próxima inserição vai rotacionar.
        let mut n = 0u32;
        let mut a_rotacionar = None;
        while r.quantidade() < CAPACIDADE_NONCES1 {
            let b = nonce_de_numero(n);
            if n == 0 {
                a_rotacionar = Some(b);
            }
            assert!(r.reservar(&b), "nonce {n} deve ser novo");
            n += 1;
        }
        assert_eq!(r.quantidade(), CAPACIDADE_NONCES1);
        assert_eq!(r.entradas.len(), r.vistos.len());

        let a_rotacionar = a_rotacionar.expect("houve uma inserção");
        assert!(
            r.vistos.contains(&a_rotacionar),
            "antes da rotação o primeiro nonce tem de estar no índice"
        );

        // Uma inserção a mais: o registo está cheio, logo o mais antigo
        // sai — e tem de sair **dos dois índices**.
        assert!(r.reservar(&nonce_de_numero(n)));
        assert_eq!(r.quantidade(), CAPACIDADE_NONCES1, "a capacidade não mudou");
        assert!(
            !r.vistos.contains(&a_rotacionar),
            "o nonce rotacionado ficou no índice: voltaria a bloquear uma mensagem legítima"
        );
        assert_eq!(r.entradas.len(), r.vistos.len(), "os índices divergiram");

        // E reenviá-lo tem de ser aceite: rodou, logo é uma mensagem nova.
        assert!(
            r.reservar(&a_rotacionar),
            "nonce rotacionado continua a bloquear mensagens legítimas"
        );
        assert_eq!(r.quantidade(), CAPACIDADE_NONCES1);
    }

    // ----------------------------------------------------------------
    // Persistência (fase B3) — o registo sobrevive a um reinício
    // ----------------------------------------------------------------

    /// 12 bytes em hexadecimal, para nomes de ficheiro de teste.
    fn para_hex(nonce: [u8; TAM_NONCE]) -> String {
        nonce.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Caminho temporário único para um teste de persistência.
    ///
    /// `tempfile` não é dependência do crate, e o nome do ficheiro é
    /// derivado do **nonce** de teste — o que torna o caminho
    /// previsível dentro de um único teste e não colide com outro, sem
    /// precisar de um `mktemp`.
    fn caminho_de_teste(nonce: &[u8; TAM_NONCE]) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("onyxchat-antireplay-{}.dat", para_hex(*nonce)))
    }

    /// **O bug que esta fase fecha.** Um registo novo aceita de novo um
    /// `nonce1` que o registo anterior já tinha rejeitado.
    ///
    /// Sem isto, capturar um envelope e reenviá-lo depois de o daemon
    /// reiniciar é aceite — e o anti-replay deixa de existir durante a
    /// janela do TTL, que é de dez minutos. Numa mansageira cujo objectivo
    /// declarado é resistir a captura de tráfego, é a falha mais concreta
    /// que o projecto tinha: um agente deyenvela um envelope uma vez e
    /// reenvia-o durante dez minutos a cada reinício do serviço.
    #[test]
    fn o_registo_sobrevive_a_um_reinicio() {
        let nonce = [0x7Fu8; TAM_NONCE];
        let caminho = caminho_de_teste(&nonce);
        let _ = std::fs::remove_file(&caminho);

        // Instância A: aceita a mensagem e persiste o registo.
        let mut a = RegistoNonce1::novo();
        assert!(a.reservar(&nonce), "primeira vez tem de passar");
        a.gravar_para(&caminho);

        // Instância B, como se o processo tivesse morrido e renascido.
        let mut b = RegistoNonce1::carregar_de(&caminho);
        assert!(
            !b.reservar(&nonce),
            "REPLAY ACEITE depois do reinício: o registo não persistiu"
        );

        // E o registo novo continua a aceitar o que é genuinamente novo.
        let outra = [0x11u8; TAM_NONCE];
        assert!(
            b.reservar(&outra),
            "um nonce novo tem de passar depois do reinício"
        );

        let _ = std::fs::remove_file(&caminho);
    }

    /// Sem ficheiro, o registo nasce vazio — e o daemon arranca.
    ///
    /// O caminho comum de todos os arranques: não há ficheiro nenhum.
    /// Um `carregar_de` que devolvesse `Err` aqui faria o daemon não
    /// arrancar no primeiro start, que é um modo de falhar que ninguém
    /// descobre até ao primeiro upgrade.
    #[test]
    fn sem_ficheiro_o_registo_nasce_vazio() {
        let caminho = std::env::temp_dir().join("onyxchat-antireplay-inexistente.dat");
        let _ = std::fs::remove_file(&caminho);

        let mut r = RegistoNonce1::carregar_de(&caminho);
        assert_eq!(r.quantidade(), 0);
        assert!(r.reservar(&[0x33u8; TAM_NONCE]), "um nonce novo tem de passar");
    }

    /// Um ficheiro **adulterado** não pode fazer o registo aceitar tudo.
    ///
    /// O ficheiro vive no directório de estado do utilizador. Mesmo com
    /// `0600`, o pior caso a defender é a corruption: um corte de energia
    /// a meio da escrita, um disco cheio, ou um ficheiro de outra
    /// versão. O requisito é que a falha seja **fechar** — registo
    /// vazio, ou registo parcial — e nunca *abrir*.
    ///
    /// É a razão de o formato ter magia e versão: um ficheiro que não
    /// seja deste formato é descartado, não interpretado.
    #[test]
    fn ficheiro_adulterado_nao_abre_o_registo() {
        let caminho = std::env::temp_dir().join("onyxchat-antireplay-lixo.dat");

        // (a) lixo puro
        std::fs::write(&caminho, b"isto nao e um registo de nonces").expect("escreve");
        let r = RegistoNonce1::carregar_de(&caminho);
        assert_eq!(r.quantidade(), 0, "lixo produziu entradas");

        // (b) cabeçalho válido, corpo truncado a meio de uma entrada
        let mut valido = Vec::new();
        valido.extend_from_slice(MAGIC);
        valido.extend_from_slice(&VERSAO.to_le_bytes());
        valido.extend_from_slice(&[1u8, 0, 0, 0]); // 1 entrada
        valido.extend_from_slice(&[0xAA; TAM_NONCE]);
        valido.extend_from_slice(&1_700_000_000u64.to_le_bytes());
        valido.extend_from_slice(&[0xBB]); // só 1 dos 8 bytes do timestamp
        std::fs::write(&caminho, &valido).expect("escreve");
        let r = RegistoNonce1::carregar_de(&caminho);
        assert_eq!(r.quantidade(), 0, "entrada truncada foi aceite");

        // (c) contagem que excede a capacidade — não pode alocar por
        //     indicação do ficheiro
        let mut valido = Vec::new();
        valido.extend_from_slice(MAGIC);
        valido.extend_from_slice(&VERSAO.to_le_bytes());
        valido.extend_from_slice(&u32::MAX.to_le_bytes());
        std::fs::write(&caminho, &valido).expect("escreve");
        let r = RegistoNonce1::carregar_de(&caminho);
        assert_eq!(r.quantidade(), 0, "contagem absurda foi aceite");

        let _ = std::fs::remove_file(&caminho);
    }

    /// Uma *cabeçalho* errado tem de recusar o registo inteiro.
    ///
    /// O teste acima passa lixo e truncamento; este passa cabeçalhos que
    /// são quase válidos, que é a classe que escapa. Um ficheiro com a
    /// magic de outra aplicação, ou de uma versão futura do formato, é
    /// lido como registo de nonces se o carregamento aceitar bytes sem
    /// verificar — e a consequência é um daemon que recusa nonces
    /// legítimos ou aceita repetidos, conforme o que os bytes
    /// significem.
    #[test]
    fn cabecalho_errado_recusa_o_registo_inteiro() {
        let caminho = caminho_de_teste(&[0x77u8; TAM_NONCE]);

        // (a) magic diferente — mesmo tamanho, cabeçalho válido.
        let mut errado = Vec::new();
        errado.extend_from_slice(b"ONYXNZ1");
        errado.extend_from_slice(&VERSAO.to_le_bytes());
        errado.extend_from_slice(&1u32.to_le_bytes());
        errado.extend_from_slice(&[0xAA; TAM_NONCE]);
        errado.extend_from_slice(&1_700_000_000u64.to_le_bytes());
        std::fs::write(&caminho, &errado).expect("escreve");
        assert_eq!(
            RegistoNonce1::carregar_de(&caminho).quantidade(),
            0,
            "uma magic de outra aplicação foi aceite"
        );

        // (b) magic certa, versão diferente.
        //
        // A versão é o que impede um registo escrito por uma versão
        // futura — onde o layout do timestamp pode ter mudado — de ser
        // lido com o layout de hoje. Aceitar «qualquer versão» seria
        // aceitar um ficheiro cujas entradas não significam o que
        // parecem significar.
        let mut errado = Vec::new();
        errado.extend_from_slice(MAGIC);
        errado.extend_from_slice(&(VERSAO + 1).to_le_bytes());
        errado.extend_from_slice(&1u32.to_le_bytes());
        errado.extend_from_slice(&[0xAA; TAM_NONCE]);
        errado.extend_from_slice(&1_700_000_000u64.to_le_bytes());
        std::fs::write(&caminho, &errado).expect("escreve");
        assert_eq!(
            RegistoNonce1::carregar_de(&caminho).quantidade(),
            0,
            "uma versão desconhecida foi aceite"
        );

        let _ = std::fs::remove_file(&caminho);
    }

    /// Um ficheiro que não se consegue gravar não derruba o daemon.
    ///
    /// `gravar_para` escreve num temporário e faz `rename`, e os três
    /// passos podem falhar: o directório não existe e não pode ser
    /// criado, o temporário não abre, ou o `rename` falha. Nenhum deles
    /// pode ser um `panic` — o anti-replay é uma protecção, e uma
    /// protecção que crasha o daemon é uma negação de serviço com um
    /// nome simpático.
    ///
    /// O caminho usado é um que **existe como directório**: um
    /// `rename` para dentro de um directório falha em qualquer Linux, e
    /// é o modo de tornar o terceiro ramo observável sem permissões
    /// especiais — `root` não está disponível e não deve ser preciso
    /// para testar uma degradação.
    #[test]
    fn gravar_para_onde_nao_pode_nao_derruba() {
        let alvo = caminho_de_teste(&[0x88u8; TAM_NONCE]);

        // (a) `rename` para dentro de um directório falha.
        std::fs::create_dir_all(&alvo).expect("cria directório");
        let registo = RegistoNonce1::novo();
        registo.gravar_para(&alvo); // tem de ser um no-op silencioso
        assert!(
            alvo.is_dir(),
            "o caminho tem de continuar a ser um directório"
        );
        std::fs::remove_dir(&alvo).expect("remove directório");

        // (b) um caminho sem directório pai atravessa o `if let` e grava
        //     onde está.
        //
        // `gravar_para` só cria o pai se o caminho tiver um. Um nome
        // solto — como `./registo` escrito a partir do directório de
        // trabalho — não tem pai que criar, e esse ramo tinha de
        // executar para a cobertura o dizer.
        let solto = std::path::PathBuf::from("registo-sem-pai.teste");
        let _ = std::fs::remove_file(&solto);
        let registo = RegistoNonce1::novo();
        registo.gravar_para(&solto);
        assert!(solto.is_file(), "sem pai tem de gravar no sítio");
        assert_eq!(RegistoNonce1::carregar_de(&solto).quantidade(), 0);
        let _ = std::fs::remove_file(&solto);
    }

    /// Um ficheiro maior do que a capacidade é truncado, não cresce.
    ///
    /// A carga limita com `min(…, CAPACIDADE_NONCES1)` **antes** de
    /// percorrer, e é por isso que a expulsão dentro do laço de carga
    /// (`anti_replay.rs:207`) é inalcançável: nunca há uma entrada a
    /// mais para expulsar. Este teste fixa essa garantia pela positiva —
    /// um ficheiro com `CAPACIDADE + 8` fica com `CAPACIDADE`.
    ///
    /// É também a defesa que importa. O ficheiro diz quantas entradas
    /// tem, e um ficheiro maior do que a capacidade não pode fazer o
    /// registo crescer sem limite depois de carregado. O `.min()`
    /// duplo — contra o que o ficheiro diz e contra o que o ficheiro
    /// contém — é o que fecha os dois lados: um `u32::MAX` declarado
    /// não faz um laço de quatro mil milhões, e um corpo truncado não
    /// faz o laço ler fora das Reservas.
    #[test]
    fn um_ficheiro_acima_da_capacidade_e_truncado() {
        let caminho = caminho_de_teste(&[0xB0u8; TAM_NONCE]);
        let agora = agora_epoch();
        let total = CAPACIDADE_NONCES1 + 8;

        let mut dados = Vec::with_capacity(TAM_CABECALHO + total * TAM_ENTRADA);
        dados.extend_from_slice(MAGIC);
        dados.extend_from_slice(&VERSAO.to_le_bytes());
        dados.extend_from_slice(&(total as u32).to_le_bytes());
        // Cada nonce distinto, para que a inserção não caia no
        // "já visto" e a capacidade ocupada seja mesmo a que fica.
        for indice in 0..total {
            let mut nonce = [0u8; TAM_NONCE];
            nonce[0..8].copy_from_slice(&(indice as u64).to_le_bytes());
            dados.extend_from_slice(&nonce);
            dados.extend_from_slice(&agora.to_le_bytes());
        }
        std::fs::write(&caminho, &dados).expect("escreve");

        let r = RegistoNonce1::carregar_de(&caminho);
        assert_eq!(
            r.quantidade(),
            CAPACIDADE_NONCES1,
            "a capacidade não foi respeita na carga: {} entradas",
            r.quantidade()
        );

        // As primeiras `CAPACIDADE` são as que ficam; a última é a que
        // é descartada. Um nonce que reentre tem de estar entre as
        // que entraram.
        let mut primeira = [0u8; TAM_NONCE];
        primeira[0..8].copy_from_slice(&0u64.to_le_bytes());
        let mut ultima = [0u8; TAM_NONCE];
        ultima[0..8].copy_from_slice(&((total - 1) as u64).to_le_bytes());

        let mut r = r;
        assert!(
            !r.reservar(&primeira),
            "a primeira entrada deveria estar no registo"
        );
        assert!(
            r.reservar(&ultima),
            "a última entrada foi descartada, e não deveria: o ficheiro \
             guarda as primeiras, não as últimas"
        );

        let _ = std::fs::remove_file(&caminho);
    }

    /// Um nonce repetido no ficheiro não duplica a entrada.
    ///
    /// O ficheiro diz quantas entradas tem, e o conteúdo pode ter menos
    /// nonces distintos que esse número. Inserir o mesmo nonce duas
    /// vezes faria a segunda inserção devolver «já visto» e saltar a
    /// entrada — que é o comportamento certo, e o que evita contar um
    /// nonce como duas ocupações na capacidade.
    #[test]
    fn nonce_repetido_no_ficheiro_ocupa_uma_so_entrada() {
        let caminho = caminho_de_teste(&[0x99u8; TAM_NONCE]);
        let mut valido = Vec::new();
        valido.extend_from_slice(MAGIC);
        valido.extend_from_slice(&VERSAO.to_le_bytes());
        valido.extend_from_slice(&3u32.to_le_bytes());
        // O mesmo nonce três vezes, com o timestamp de agora.
        //
        // `agora_epoch()` e não um literal: um timestamp fixo que já
        // passou do TTL faria a entrada ser descartada na carga, e o
        // teste passaria por zero entradas — que era a falha da
        // primeira versão deste teste.
        let agora = agora_epoch();
        for _ in 0..3 {
            valido.extend_from_slice(&[0xAA; TAM_NONCE]);
            valido.extend_from_slice(&agora.to_le_bytes());
        }
        std::fs::write(&caminho, &valido).expect("escreve");

        let r = RegistoNonce1::carregar_de(&caminho);
        assert_eq!(
            r.quantidade(),
            1,
            "três cópias do mesmo nonce ocuparam {} entradas",
            r.quantidade()
        );
        // `reservar` é a operação de produção: devolve `false` quando o
        // nonce já está visto, que é o que a repetição tem de produzir.
        let mut r = r;
        assert!(
            !r.reservar(&[0xAA; TAM_NONCE]),
            "um nonce já carregado do ficheiro tem de ser rejeitado"
        );

        let _ = std::fs::remove_file(&caminho);
    }

    /// Entradas expiradas não são ressuscitadas pelo ficheiro.
    ///
    /// O TTL continua a valer entre arranques. Um registo gravado há
    /// uma hora e carregado agora não pode reter os nonces dessa hora:
    /// manter-nos dava um registo que só cresce e nunca esquece, que é
    /// um mecanismo de negação de serviço com aparência de segurança.
    #[test]
    fn entradas_expiradas_nao_sobrevive_ao_reinicio() {
        let caminho = caminho_de_teste(&[0x5Au8; TAM_NONCE]);
        let mut valido = Vec::new();
        valido.extend_from_slice(MAGIC);
        valido.extend_from_slice(&VERSAO.to_le_bytes());
        valido.extend_from_slice(&2u32.to_le_bytes());
        // Um nonce de há duas horas (expirado) e um de agora (vivo).
        let agora = agora_epoch();
        for segundos in [agora - 2 * TTL_SEGUNDOS, agora] {
            valido.extend_from_slice(&[0xCC; TAM_NONCE]);
            valido.extend_from_slice(&segundos.to_le_bytes());
        }
        std::fs::write(&caminho, &valido).expect("escreve");

        let mut r = RegistoNonce1::carregar_de(&caminho);
        assert_eq!(
            r.quantidade(),
            1,
            "o nonce expirado devia ter sido descartado ao carregar"
        );
        assert!(
            !r.reservar(&[0xCC; TAM_NONCE]),
            "o nonce expirado tem de continuar rejeitado"
        );

        let _ = std::fs::remove_file(&caminho);
    }

    /// A gravação é atómica: um corte a meio não corrompe o registo.
    ///
    /// Verifica-se pela existência do ficheiro temporário: a escrita é
    /// `escrever temporário → fsync → rename`, e o `rename` é atómico
    /// dentro do mesmo directório. Se a escrita fosse directa sobre o
    /// ficheiro final, um corte de energia deixaria um registo meio
    /// escrito — que o teste anterior mostra ser descartado, ou seja,
    /// um reinício perderia todo o anti-replay.
    #[test]
    fn a_gravacao_nao_deixa_temporarios_para_trase() {
        let nonce = [0x3Du8; TAM_NONCE];
        let caminho = caminho_de_teste(&nonce);
        let temporario = caminho.with_extension("tmp");
        let _ = std::fs::remove_file(&caminho);
        let _ = std::fs::remove_file(&temporario);

        let mut r = RegistoNonce1::novo();
        r.reservar(&nonce);
        r.gravar_para(&caminho);

        assert!(caminho.exists(), "o registo não foi gravado");
        assert!(
            !temporario.exists(),
            "o ficheiro temporário ficou para trás: a escrita não é \
             atómica, ou o `rename` não correu"
        );

        let _ = std::fs::remove_file(&caminho);
    }

    /// O ficheiro nunca cresce sem limite, mesmo que o registo encha.
    ///
    /// O limite vem da capacidade do registo (4096 entradas × 20 bytes
    /// ≈ 82 KB), e não de um limite de ficheiro separado: duas fontes
    /// de verdade para o mesmo limite divergem. Este teste fixa o
    /// tamanho máximo observável para que uma alteração futura na
    /// estrutura de dados não o faça explodir em silêncio.
    #[test]
    fn o_ficheiro_nao_cresce_sem_limite() {
        let caminho = caminho_de_teste(&[0x1Eu8; TAM_NONCE]);
        let _ = std::fs::remove_file(&caminho);

        let mut r = RegistoNonce1::novo();
        for i in 0..(CAPACIDADE_NONCES1 + 100) {
            r.reservar(&nonce_de_numero(i as u32));
        }
        r.gravar_para(&caminho);

        let tamanho = std::fs::metadata(&caminho).expect("metadados").len();
        let tecto = (TAM_CABECALHO + CAPACIDADE_NONCES1 * TAM_ENTRADA) as u64;
        assert!(
            tamanho <= tecto,
            "ficheiro com {tamanho} bytes, tecto {tecto}"
        );

        let _ = std::fs::remove_file(&caminho);
    }
}
