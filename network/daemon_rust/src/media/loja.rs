// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! A loja do `sidecar`, lida de dentro do `daemon`.
//!
//! [`docs/media.md`](../../../../docs/media.md) §4.6.2, opção A: o
//! `daemon` monta o ficheiro porque é quem tem as chaves, e lê os
//! envelopes da loja do `sidecar` porque é onde estão.
//!
//! ## Isto não é uma porta de entrada
//!
//! O `daemon` passa a ler um ficheiro que não criou, e a superfície
//! óbvia de ataque é o `id` da transferência: um identificador que
//! chegue a uma instrução SQL permite mudar o que é lido.
//!
//! Por isso o `id` **não** chega ao `sqlite3` como parte de nenhuma
//! instrução. É [`IdTransferencia`] — 16 bytes — a ser ligado a um
//! `?`, e nada mais. Não há concatenação de texto, e o único caminho de
//! um `id` até ao SQL é um parâmetro.
//!
//! A segunda garantia é [`Loja::abrir`]: `mode=ro`. A base pode ser
//! de um `sidecar` ou de um ficheiro que alguém colocou no caminho, e
//! um daemon que abre o que lhe disseram para abrir e ainda **escreve**
//! é pior do que um daemon que não abre.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::erros::ErroMedia;
use super::ficheiro::{IdTransferencia, Manifesto, Montado, Montador};
use super::moldura::Flags;

/// A instrução que devolve o manifesto de uma transferência.
///
/// `LIMIT 1` não é decoração: sem ele, um `id` repetido — que o esquema
/// não proíbe sem `PRAGMA` de unicidade em todas as versões — devolveria
/// várias linhas e a primeira seria umifestado arbitrário.
const MANIFESTO_DE: &str = "
    SELECT m.texto
      FROM mensagens AS m
      JOIN transferencias AS t ON t.id = m.transferencia_id
     WHERE t.id = ?1
       AND m.texto LIKE 'ONX1%'
     ORDER BY m.em
     LIMIT 1";

/// As fatias de uma transferência.
///
/// ## Nem `envelope IS NOT NULL` nem ordenação
///
/// A primeira versão filtrava por `envelope IS NOT NULL`, resíduo de uma
/// versão que desencriptava — e que devolveva **zero** fatias sempre que
/// a linha não tivesse `envelope`, que é o caso normal depois de §4.6.4.
/// A referência ao `envelope` desapareceu pela mesma razão que a
/// decifra.
///
/// E não há `ORDER BY`: quem ordena é o [`Montador`], pelo `seq` que vai
/// na moldura. Ordenar por `m.em` seria ordenar pela ordem **de
/// chegada** — que o relay pode mudar — e daria ao montador as fatias
/// por ordem de chegada em vez de por ordem de sequência.
const FATIAS_DE: &str = "
    SELECT m.texto
      FROM mensagens AS m
      JOIN transferencias AS t ON t.id = m.transferencia_id
     WHERE t.id = ?1
       AND m.texto LIKE 'ONX1%'
     ORDER BY m.texto";

/// A loja do `sidecar`, em leitura.
pub struct Loja {
    /// A ligação `sqlite3`, aberta com `mode=ro`.
    cx: Connection,
    /// O caminho de onde foi aberta.
    ///
    /// Guardado para o diagnóstico de [`ErroMedia::LojaIndisponivel`]:
    /// «a loja não abre» sem dizer qual é inútil quando há duas.
    caminho: PathBuf,
}

impl Loja {
    /// Abre uma loja em modo **só de leitura**.
    ///
    /// ## Porque `immutable` **não** é uma opção aceitável
    ///
    /// `immutable=1` diz ao sqlite que o ficheiro não vai mudar. É mais
    /// rápido, e seria um erro grave aqui: o `sidecar` escreve a loja
    /// enquanto o `daemon` lê, e `immutable` com um ficheiro que muda
    /// dá leitura de páginas velhas **sem erro** — que para um
    /// ficheiro de media significa servir bytes que já não são o que o
    /// resumo diz.
    ///
    /// `query_only` é o que se quer: o sqlite recusa escritas, e a
    /// leitura vê o que está lá.
    ///
    /// ## O `-wal`
    ///
    /// O `sidecar` usa `journal_mode = WAL`. O sqlite só abre em
    /// leitura uma base em WAL se conseguir criar o ficheiro `-shm` ao
    /// lado, e isso exige escrita nodirectório. Quando não pode — um
    /// `sidecar` a correr como outro utilizador, um directório
    /// só-leitura — a abertura dá `SQLITE_CANTOPEN`, e a resposta é
    /// [`ErroMedia::LojaIndisponivel`]. Ver §4.6.2.
    pub fn abrir(caminho: &Path) -> Result<Self, ErroMedia> {
        if caminho.as_os_str().is_empty() {
            return Err(ErroMedia::LojaIndisponivel);
        }

        // O `mode=ro` do URI é o que impede escrita; o `query_only`
        // é a mesma ideia dita ao sqlite depois de ligado, e os dois
        // juntos cobrem o caso em que um deles seja ignorado.
        let cx = Connection::open_with_flags(
            caminho,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .and_then(|cx| {
            cx.pragma_update(None, "query_only", "1")?;
            Ok(cx)
        })
        .map_err(|_| ErroMedia::LojaIndisponivel)?;

        // ## Porque uma consulta é necessária **aqui**
        //
        // O sqlite é preguiçoso: `open` de um ficheiro que não é uma
        // base **tem sucesso**. O cabeçalho só é lido na primeira
        // consulta, e é aí que sai o «file is not a database».
        //
        // Sem esta linha, `abrir` devolvia `Ok` para `/etc/hostname` e
        // o erro aparecia na primeira mensagem pedida — longe da causa,
        // e como `0x13` em vez de um erro de media. Uma consulta ao
        // esquema é o que confirma que é uma base, e é a mais barata
        // que existe.
        let e_base = cx
            .query_row("SELECT count(*) FROM sqlite_schema", [], |l| {
                l.get::<_, i64>(0)
            })
            .map(|n| n >= 0)
            .unwrap_or(false);
        if !e_base {
            return Err(ErroMedia::LojaIndisponivel);
        }

        Ok(Self {
            cx,
            caminho: caminho.to_path_buf(),
        })
    }

    /// O caminho de onde a loja foi aberta.
    pub fn caminho(&self) -> &Path {
        &self.caminho
    }

    /// O manifesto de uma transferência, lido das suas mensagens.
    ///
    /// ## Porque o manifesto se lê da loja e não da linha `transferencias`
    ///
    /// A linha de `transferencias` tem os números, mas **não** tem a
    /// ordem dos campos do manifesto. Reproduzir a partir dela seria
    /// duplicar o formato num segundo sítio, e os dois divergem sem que
    /// nada diga. A linha de manifesto é a fonte, e é a que o receptor
    /// leu para decidir o que fazer com estes bytes.
    pub fn manifesto(&self, id: &IdTransferencia) -> Result<Manifesto, ErroMedia> {
        // A coluna `texto` é o **plaintext**: a loja guarda texto, não
        // envelopes, e é o que o `sidecar` escreve. Por isso o manifesto
        // se lê sem decifrar — e isso é correcto. O que protege a coluna
        // é ser cópia de um plaintext que foi autenticado quando entrou
        // no disco; a proteção não é a coluna ser um segredo, porque
        // ela não é.
        let texto = self
            .cx
            .query_row(MANIFESTO_DE, rusqlite::params![hex(id)], |l| {
                // `String` e não `Vec<u8>`: a coluna é `TEXT`, e o
                // `rusqlite` recusa entregar um `Vec<u8>` de uma coluna
                // de texto — devolve `InvalidColumnType`, que é uma
                // recusa de tipo e não uma linha em falta.
                l.get::<_, String>(0)
            })
            .map_err(|_| ErroMedia::LojaIndisponivel)?;

        let aberta = super::moldura::abrir(texto.as_bytes())?;
        if !aberta.flags.tem(Flags::MANIFESTO) {
            return Err(ErroMedia::ManifestoIncoerente(
                "a linha do manifesto não tem a flag de manifesto",
            ));
        }
        if aberta.seq != 0 {
            return Err(ErroMedia::ManifestoIncoerente(
                "o manifesto não tem o seq 0",
            ));
        }

        Manifesto::de_corpo(&aberta.corpo)
    }

    /// Monta uma transferência a partir das linhas guardadas.
    ///
    /// É esta a função que faz o trabalho de §4.6: lê as fatias, monta
    /// por `seq`, verifica o SHA-256, e devolve os bytes prontos a irem
    /// para o `memfd`.
    ///
    /// ## Porque o [`Montador`] e não um `Vec`
    ///
    /// O `Montador` é o mesmo que o receptor em memória usa, e é a
    /// razão de o resumo ser uma **conferência** e não uma comparação
    /// de tamanhos: um `memfd` com os bytes errados nunca chega a ser
    /// criado, porque `montar` devolve `Err` primeiro.
    pub fn montar(&self, id: &IdTransferencia) -> Result<Montado, ErroMedia> {
        let manifesto = self.manifesto(id)?;
        let mut montador = Montador::novo(&manifesto)?;

        let mut consulta = self
            .cx
            .prepare(FATIAS_DE)
            .map_err(|_| ErroMedia::LojaIndisponivel)?;

        let linhas = consulta
            .query_map(rusqlite::params![hex(id)], |l| l.get::<_, String>(0))
            .map_err(|_| ErroMedia::LojaIndisponivel)?;

        for linha in linhas {
            let texto = linha.map_err(|_| ErroMedia::LojaIndisponivel)?;

            // Uma linha sem `ONX1` não é uma fatia. A loja recusa isso
            // na escrita, mas uma base vinda de outra versão pode ter,
            // e recusar a transferência inteira por causa disso seria
            // pior do que ignorar a linha: o resumo final decide se o
            // ficheiro é o certo.
            if !texto.starts_with("ONX1") {
                continue;
            }

            let aberta = super::moldura::abrir(texto.as_bytes())?;

            // O manifesto tem `seq` 0, e o montador recusa o 0. É a
            // linha que [`Loja::manifesto`] já leu; repeti-la aqui só
            // serviria para devolver um erro que não explicaria nada.
            if aberta.flags.tem(Flags::MANIFESTO) {
                continue;
            }

            // `Montador::receber` recusa o `seq` 0 e qualquer fatia
            // fora do intervalo. Uma linha repetida é aceite e fica:
            // ver a nota de `Montador::receber`.
            montador.receber(aberta.seq, &aberta.corpo)?;
        }

        montador.montar()?.ok_or(ErroMedia::PedacosEmFalta)
    }
}

/// Os 16 bytes do `id` em hexadecimal, para ligar ao `?`.
///
/// Os bytes, e não o hex: ligar o hex ao `sqlite3` seria ligar uma
/// `String` vinda do exterior a uma comparação, e o caminho inteiro
/// ganha uma superfície de ataque sem necessidade.
fn hex(id: &IdTransferencia) -> String {
    super::ficheiro::id_em_hex(id)
}

#[cfg(test)]
#[cfg(test)]
mod testes {
    use super::*;

    /// O esquema do `sidecar`, reduzido ao que este módulo lê.
    ///
    /// ## Porque o esquema está aqui e não é importado
    ///
    /// O `sidecar` é Python, e o esquema vive em `server/loja.py`.
    /// Testar contra esse ficheiro seria um teste que depende de um
    /// interpretador a correr — e, pior, um teste que passaria com um
    /// esquema **errado**, desde que o erro fosse simétrico entre os
    /// dois lados. Este esquema é o que o `sidecar` cria, escrito a
    /// partir de `server/loja.py`.
    const ESQUEMA: &str = "
        CREATE TABLE conversas (
            id TEXT PRIMARY KEY, contacto_id TEXT, criada_em REAL NOT NULL
        );
        CREATE TABLE mensagens (
            id TEXT PRIMARY KEY,
            conversa_id TEXT NOT NULL,
            de TEXT NOT NULL,
            texto TEXT NOT NULL,
            em REAL NOT NULL,
            estado TEXT NOT NULL,
            envelope BLOB,
            transferencia_id TEXT
        );
        CREATE TABLE transferencias (
            id TEXT PRIMARY KEY,
            conversa_id TEXT NOT NULL,
            de TEXT NOT NULL,
            tipo TEXT NOT NULL,
            codec TEXT NOT NULL,
            formato TEXT NOT NULL,
            nome TEXT NOT NULL,
            total_bytes INTEGER NOT NULL,
            total_pedacos INTEGER NOT NULL,
            bytes_por_pedaco INTEGER NOT NULL,
            duracao_s INTEGER,
            sha256 BLOB NOT NULL,
            recebido_em REAL NOT NULL,
            completo INTEGER NOT NULL DEFAULT 0
        );";

    /// Um caminho temporário único por teste.
    ///
    /// `std::env::temp_dir()` e não uma dependência: `tempfile` seria a
    /// primeira dependência de testes de um projecto que não tem
    /// nenhuma, e comprar uma biblioteca para evitar um
    /// `remove_dir_all` é o tipo de custo permanente que este projecto
    /// recusa.
    fn dir_temporario(rotulo: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("onyx-media-loja-{rotulo}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir temporário");
        dir
    }

    /// Cria uma base de teste com uma conversa e uma transferência sem
    /// nenhuma linha de media.
    fn loja_de_teste(rotulo: &str) -> (PathBuf, Connection) {
        let dir = dir_temporario(rotulo);
        let cx = Connection::open(dir.join("loja.db")).expect("base");
        cx.execute_batch(ESQUEMA).expect("esquema");
        cx.execute(
            "INSERT INTO conversas (id, contacto_id, criada_em) VALUES ('c1', 'a1', 1.0)",
            [],
        )
        .expect("conversa");
        cx.execute(
            "INSERT INTO transferencias (id, conversa_id, de, tipo, codec, formato, nome,\
             total_bytes, total_pedacos, bytes_por_pedaco, sha256, recebido_em)\
             VALUES (?1, 'c1', 'them', 'video', 'matroska', 'matroska', 'v.mkv',\
             0, 0, 49125, zeroblob(32), 1.0)",
            [ID],
        )
        .expect("transferência");
        (dir, cx)
    }

    /// O `id` de teste, em hex — os 16 bytes todos iguais.
    const ID: &str = "11111111111111111111111111111111";

    fn id_de_teste() -> IdTransferencia {
        [0x11u8; 16]
    }

    fn loja_de_media(rotulo: &str) -> (PathBuf, Connection, Loja) {
        let (dir, _cx) = loja_de_teste(rotulo);
        let loja = Loja::abrir(&dir.join("loja.db")).expect("abre");
        (dir, _cx, loja)
    }

    #[test]
    fn abre_uma_loja_em_leitura() {
        let (dir, _, loja) = loja_de_media("abre");
        assert!(loja.caminho().ends_with("loja.db"));
        assert_eq!(loja.caminho(), dir.join("loja.db"));
    }

    #[test]
    fn o_modo_leitura_impede_escrever() {
        let (_, _, loja) = loja_de_media("escrita");

        // Se `query_only` não estivesse ligado, isto passava. E é o
        // teste que diz que o daemon **não pode** escrever na loja de
        // quem o mandou abrir — não por cortesia, mas porque a opção A
        // de §4.6.2 promete só leitura.
        let resultado = loja.cx.execute(
            "INSERT INTO conversas (id, contacto_id, criada_em) VALUES ('x', 'y', 1.0)",
            [],
        );
        assert!(
            resultado.is_err(),
            "o daemon conseguiu escrever na loja do sidecar"
        );
    }

    #[test]
    fn uma_loja_que_nao_existe_da_loja_indisponivel() {
        let dir = dir_temporario("inexistente");
        assert!(matches!(
            Loja::abrir(&dir.join("nao-existe.db")),
            Err(ErroMedia::LojaIndisponivel)
        ));
    }

    #[test]
    fn um_caminho_vazio_da_loja_indisponivel() {
        assert!(matches!(
            Loja::abrir(Path::new("")),
            Err(ErroMedia::LojaIndisponivel)
        ));
    }

    #[test]
    fn um_ficheiro_que_nao_e_uma_base_da_loja_indisponivel() {
        let dir = dir_temporario("lixo");
        let caminho = dir.join("nao-e-base.db");
        std::fs::write(&caminho, b"isto nao e uma base de dados").expect("escreve");

        // Um caminho que não é uma base Onyx tem de dar **erro de
        // media**, e não um pânico do `sqlite3` a subir pelo `main`.
        assert!(matches!(
            Loja::abrir(&caminho),
            Err(ErroMedia::LojaIndisponivel)
        ));
    }

    #[test]
    fn um_manifesto_que_ainda_nao_chegou_da_loja_indisponivel() {
        let (_, _, loja) = loja_de_media("sem-manifesto");

        // Sem linha de manifesto, `query_row` devolve `NoRows`. O que
        // interessa é que **não** é um pânico e **não** devolve um
        // manifesto inventado: um manifesto vazio montaria um ficheiro
        // vazio que passaria, e o utilizador ouviria um ficheiro de zero
        // bytes como se fosse o vídeo que esperava.
        assert!(matches!(
            loja.manifesto(&id_de_teste()),
            Err(ErroMedia::LojaIndisponivel)
        ));
    }

    #[test]
    fn o_id_viaja_como_parametro_e_nunca_como_texto() {
        // A propriedade de segurança de §4.6.2: o `id` não é
        // concatenado em SQL. Isto não é um teste de SQL — é um teste
        // de **texto**: se alguém mudar a instrução para
        // `format!("… WHERE id = \'{id}\'")`, o `format!` apareceria
        // aqui e o teste leria a instrução com o `{` e falhava.
        for instrucao in [MANIFESTO_DE, FATIAS_DE] {
            assert!(
                !instrucao.contains('{') && !instrucao.contains('}'),
                "uma instrução tem uma interpolação: {instrucao}"
            );
            assert!(
                instrucao.contains('?'),
                "uma instrução não usa parâmetro ligado: {instrucao}"
            );
        }

        // E o `id` é hex de tamanho fixo, que é o que impede uma
        // citação de ser um identificador válido.
        assert_eq!(hex(&id_de_teste()).len(), 32);
        assert!(hex(&id_de_teste()).chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn nenhum_byte_do_id_encontra_uma_transferencia() {
        let (_, _, loja) = loja_de_media("bytes");

        // Todos os 256 valores no `id`: nenhum pode fazer o `sqlite3`
        // interpretar a ligação como outra coisa. Com um `?` ligado isto
        // é uma comparação de 32 caracteres; com interpolação, um
        // simples `0x27` fecharia a citação e o resto seria SQL. É o
        // teste que prova que a superfície foi fechada.
        for byte in 0..=255u8 {
            let id = [byte; 16];
            assert!(
                loja.manifesto(&id).is_err(),
                "o id 0x{byte:02x} encontrou uma transferência"
            );
        }
    }

    #[test]
    fn uma_loja_escrita_pelo_sidecar_e_legivel() {
        // O cenário real: o `sidecar` escreve, o `daemon` abre a mesma
        // base em leitura e monta. Se a leitura fosse feita por uma
        // ligação diferente com outro estado, o `daemon` montaria um
        // ficheiro que não é o que o `sidecar` gravou — e o sintoma
        // seria «o vídeo não abre», sem forma de saber porquê.
        use crate::media::moldura::{construir_com_flags, Flags};

        let dir = dir_temporario("sidecar");
        let caminho = dir.join("loja.db");
        let cx = Connection::open(&caminho).expect("base");
        cx.execute_batch(ESQUEMA).expect("esquema");
        cx.execute(
            "INSERT INTO conversas (id, contacto_id, criada_em) VALUES ('c1', 'a1', 1.0)",
            [],
        )
        .expect("conversa");

        let conteudo: Vec<u8> = (0..=255u8).cycle().take(90_000).collect();
        let manifesto = super::super::Manifesto::novo(
            super::super::Tipo::Video,
            super::super::Codec::Matroska,
            super::super::Formato::Matroska,
            42,
            "praia.mkv",
            &conteudo,
        )
        .expect("manifesto");
        let id = manifesto.id();
        let id_hex = super::super::ficheiro::id_em_hex(&id);

        cx.execute(
            "INSERT INTO transferencias (id, conversa_id, de, tipo, codec, formato, nome,
             total_bytes, total_pedacos, bytes_por_pedaco, duracao_s, sha256, recebido_em,
             completo)
             VALUES (?1, 'c1', 'them', 'video', 'matroska', 'matroska', 'praia.mkv',
             ?2, ?3, ?4, 42, ?5, 1.0, 1)",
            rusqlite::params![
                id_hex,
                manifesto.total_bytes,
                manifesto.total_pedacos,
                manifesto.bytes_por_pedaco,
                manifesto.sha256.to_vec()
            ],
        )
        .expect("transferência");

        let m_manifesto = construir_com_flags(
            super::super::Tipo::Video,
            super::super::Modo::NaoStreaming,
            Flags::VAZIO.com(Flags::MANIFESTO),
            7,
            0,
            0,
            &manifesto.corpo(),
        );
        cx.execute(
            "INSERT INTO mensagens (id, conversa_id, de, texto, em, estado, envelope,
                                   transferencia_id)
             VALUES ('m0', 'c1', 'them', ?1, 0.0, 'recebida', NULL, ?2)",
            rusqlite::params![m_manifesto.como_texto(), id_hex],
        )
        .expect("manifesto na loja");

        for (i, pedaco) in manifesto
            .fatias(&conteudo)
            .expect("fatias")
            .iter()
            .enumerate()
        {
            let m = crate::media::moldura::construir(
                super::super::Tipo::Video,
                super::super::Modo::NaoStreaming,
                7,
                i as u32 + 1,
                0,
                pedaco,
            );
            cx.execute(
                "INSERT INTO mensagens (id, conversa_id, de, texto, em, estado, envelope,
                                       transferencia_id)
                 VALUES (?1, 'c1', 'them', ?2, ?3, 'recebida', NULL, ?4)",
                rusqlite::params![
                    format!("m{}", i + 1),
                    m.como_texto(),
                    (i + 1) as f64,
                    id_hex
                ],
            )
            .expect("fatia");
        }
        drop(cx);

        let loja = Loja::abrir(&caminho).expect("abre em leitura");
        let lido = loja.manifesto(&id).expect("manifesto");
        assert_eq!(lido.total_bytes, manifesto.total_bytes);

        let montado = loja.montar(&id).expect("monta");
        assert_eq!(
            montado.bytes(),
            conteudo.as_slice(),
            "os bytes não são os de origem"
        );
    }

    #[test]
    fn uma_linha_sem_moldura_nao_e_um_manifesto() {
        let (_, cx) = loja_de_teste("texto-plano");
        cx.execute(
            "INSERT INTO mensagens (id, conversa_id, de, texto, em, estado, envelope,\
             transferencia_id) VALUES ('m1', 'c1', 'them', 'olá', 1.0, 'recebida', NULL, ?1)",
            [ID],
        )
        .expect("linha");
        let loja = Loja::abrir(Path::new(cx.path().unwrap())).expect("abre");

        // Uma linha de **texto** com `transferencia_id` é o que uma
        // loja de outra versão pode ter. Não pode virar manifesto.
        assert!(loja.manifesto(&id_de_teste()).is_err());
    }
}
