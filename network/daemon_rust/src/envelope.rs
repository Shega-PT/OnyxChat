// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// envelope.rs — envelope binário da mensagem (docs/message_format.md)
// ---------------------------------------------------------------------
// Layout: `[versão:1][nonce1:12][nonce5:12][nonce9:12][ct K9: N][assinatura:64]`
//   * total = 101 + N, mínimo 117 bytes (N ≥ 16 = tag AEAD);
//   * região assinada = offset [0 .. 37+N) — versão + nonces + ciphertext;
//   * a assinatura cobre os nonces, impedindo ataques de nonce-swapping.
//
// O parsing é sempre por comprimento — nunca por delimitadores — e as
// falhas viram `ErroEnvelope` (o daemon mapeia para IPC 0x06).
// =====================================================================

use std::fmt;

use crate::erros::ErroPipeline;

/// Versão do envelope que este código **escreve**.
pub const VERSAO: u8 = 0x01;

/// Versões do envelope que este código **lê**.
///
/// É esta lista que implementa a tolerância de versões
/// (`docs/index.md` §Princípio da tolerância de versões): um envelope de
/// uma versão que aqui esteja é aceite, um de qualquer outra é recusado
/// com `VersaoDesconhecida` → IPC `0x06`.
///
/// Hoje a lista tem um só elemento — só existe a v0x01. O mecanismo é que
/// interessa: quando existir uma v0x02, acrescenta-se aqui e a leitura
/// tolerante fica a funcionar **sem alterar mais uma linha**.
///
/// ## Porquê uma lista e não `major`/`minor`
///
/// O IPC usa `major.minor` porque o envelope **não** tem eixos
/// independentes — a versão é um byte opaco que não separa alterações
/// compatíveis de incompatíveis. Inventar um `minor` para o envelope
/// seria reinterpretar o byte de versão, e o byte já está assinado e
/// documentado como opaco em `docs/message_format.md`.
///
/// ## Porquê isto não enfraquece o anti-downgrade
///
/// A versão está **dentro da região assinada** (`[0..37+N)`). Aceitar uma
/// versão que se suporta nativamente não é downgrade — é compatibilidade.
/// Um downgrade seria forçar o receptor a aceitar a v0x01 *por causa de
/// um atacante*, e a assinatura denunciaria qual foi usada.
pub const VERSOES_ACEITE: &[u8] = &[VERSAO];

/// `true` se `versao` está na lista de versões lidas.
fn versao_aceite(versao: u8) -> bool {
    VERSOES_ACEITE.contains(&versao)
}
/// Tamanho de cada nonce AEAD (K1/K5/K9).
pub const TAM_NONCE: usize = 12;
/// Tamanho fixo da assinatura Ed25519.
pub const TAM_ASSINATURA: usize = 64;
/// Cabeçalho assinável: versão (1) + três nonces (3×12) = 37 bytes.
pub const TAM_CABECALHO: usize = 1 + 3 * TAM_NONCE;
/// Envelope mínimo: 101 fixos + 16 da menor tag AEAD.
pub const TAM_MINIMO: usize = TAM_CABECALHO + 64 + 16;

/// Limite máximo do plaintext aceite pelo pipeline (IPC `ENCODE`).
///
/// Fixado em `docs/message_format.md` §Limites: mensagens acima disto
/// são rejeitadas **antes** de K1 — nunca se processa material que não
/// vai caber no envelope.
pub const MAX_PLAINTEXT: usize = 65_536;

/// Limite **mínimo** do plaintext aceite pelo pipeline (IPC `ENCODE`).
///
/// Uma mensagem tem de ter pelo menos um byte. Texto vazio é input
/// degenerado: cifrá-lo produziria um envelope de apenas padding, que a
/// decifragem aceitaria como mensagem vazia — um caminho que não
/// transmite nada e não pode ser distinguido de um ataque de
/// preenchimento.
///
/// Rejeitado com `ErroPipeline::PayloadVazio` → IPC `0x02`, e não
/// `0x09`: `0x09` diz «excede o limite», `0x02` diz «não é um pedido
/// válido», e é a segunda leitura que permite ao cliente corrigir-se.
///
/// Especificado em `docs/pipeline.md` §Limites de tamanho.
pub const MIN_PLAINTEXT: usize = 1;

/// Sobretotal máximo do ciphertext em relação ao plaintext:
///
/// ```text
/// K1 tag +16 · K4 PKCS#7(5) +5 · K5 tag +16 · K7 PKCS#7(2) +2
/// K9 tag +16 · envelope (37 cabeçalho + 64 assinatura) +101  = 156
/// ```
///
/// Qualquer envelope maior que `MAX_PLAINTEXT + 156` é impossível de
/// produzir por esta implementação — tratá-lo como `Grande` é portanto
/// fail-closed e impede alocações proporcionais a comprimento não
/// validado (`docs/security_model.md` §Propriedades de implementação).
pub const SOBRETOTAL_MAXIMO: usize = 156;

/// Limite máximo de um envelope completo (receptor).
pub const MAX_ENVELOPE: usize = MAX_PLAINTEXT + SOBRETOTAL_MAXIMO;

/// Erros de parsing do envelope (mapeados para IPC `0x06`, exceto
/// `Grande` que mapeia para `0x09 PayloadGrandeDemais`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroEnvelope {
    /// Comprimento total abaixo do mínimo de 117 bytes.
    Curto { obtido: usize },
    /// Comprimento total acima de `MAX_ENVELOPE` (65 692 bytes).
    Grande { obtido: usize },
    /// Byte de versão fora de [`VERSOES_ACEITE`].
    VersaoDesconhecida { obtida: u8 },
}

impl fmt::Display for ErroEnvelope {
    /// Mensagem legível sem expor o conteúdo do envelope.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroEnvelope::Curto { obtido } => {
                write!(f, "envelope curto: {obtido} bytes (mínimo {TAM_MINIMO})")
            }
            ErroEnvelope::Grande { obtido } => {
                write!(
                    f,
                    "envelope demasiado grande: {obtido} bytes (máximo {MAX_ENVELOPE})"
                )
            }            ErroEnvelope::VersaoDesconhecida { obtida } => {
                write!(f, "versão de envelope desconhecida: 0x{obtida:02x}")
            }
        }
    }
}

/// Envelope decifrável: os três nonces + o ciphertext final de K9.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// Nonce de K1 (ChaCha20-Poly1305, chave do par).
    pub nonce1: [u8; TAM_NONCE],
    /// Nonce de K5 (AES-256-GCM, chave do remetente).
    pub nonce5: [u8; TAM_NONCE],
    /// Nonce de K9 (ChaCha20-Poly1305, chave do receptor).
    pub nonce9: [u8; TAM_NONCE],
    /// Ciphertext final K9 (N ≥ 16; inclui a tag Poly1305).
    pub ciphertext: Vec<u8>,
}

impl Envelope {
    /// Constrói um envelope novo a partir dos nonces e ciphertext.
    pub fn novo(
        nonce1: [u8; TAM_NONCE],
        nonce5: [u8; TAM_NONCE],
        nonce9: [u8; TAM_NONCE],
        ciphertext: Vec<u8>,
    ) -> Self {
        Envelope {
            nonce1,
            nonce5,
            nonce9,
            ciphertext,
        }
    }

    /// Região assinada: `versão ‖ nonce1 ‖ nonce5 ‖ nonce9 ‖ ciphertext`.
    ///
    /// A capacidade reservada inclui a assinatura, e isso não é
    /// cosmético: [`Envelope::fechar_regiao`] acrescenta os 64 bytes da
    /// assinatura a **esta** `Vec`. Sem a reserva, o `extend_from_slice`
    /// final não caberia e o `realloc` copiaria o envelope inteiro —
    /// numa mensagem de 64 KiB, mais uma cópia de tudo, e um pico de
    /// memória que sobe no pior momento possível.
    ///
    /// A capacidade extra é invisível para quem só assina: [`len`] é o
    /// que a [`crypto_core::sign_message`] vê.
    pub fn regiao_assinada(&self) -> Vec<u8> {
        let mut regiao = Vec::with_capacity(
            TAM_CABECALHO + self.ciphertext.len() + TAM_ASSINATURA,
        );
        regiao.push(VERSAO);
        regiao.extend_from_slice(&self.nonce1);
        regiao.extend_from_slice(&self.nonce5);
        regiao.extend_from_slice(&self.nonce9);
        regiao.extend_from_slice(&self.ciphertext);
        debug_assert!(
            regiao.capacity() >= regiao.len() + TAM_ASSINATURA,
            "a região tem de vir com espaço para a assinatura"
        );
        regiao
    }

    /// Junta a assinatura Ed25519 à região assinada → envelope final.
    pub fn fechar(&self, assinatura: &[u8; TAM_ASSINATURA]) -> Vec<u8> {
        Self::fechar_regiao(self.regiao_assinada(), assinatura)
    }

    /// Junta a assinatura a uma região **já construída**.
    ///
    /// O caminho de produção é este, e não [`Envelope::fechar`]: o
    /// pipeline precisa de `regiao_assinada` para assinar, e pedir a
    /// região outra vez a `fechar` construía o mesmo envelope duas
    /// vezes. Com um payload no limite (`MAX_PLAINTEXT`), eram três
    /// cópias inteiras do ciphertext — `c9.clone()`, a região para
    /// assinar, e a região dentro de `fechar` — onde duas chegam.
    ///
    /// Consome a `Vec` recebida em vez de a copiar, e devolve a mesma
    /// alocação com a assinatura no fim. Como [`regiao_assinada`] reserva
    /// [`TAM_ASSINATURA`] bytes, o `extend` não realoca.
    pub fn fechar_regiao(
        mut regiao: Vec<u8>,
        assinatura: &[u8; TAM_ASSINATURA],
    ) -> Vec<u8> {
        debug_assert!(
            regiao.len() >= TAM_CABECALHO,
            "uma região assinada sem cabeçalho não é um envelope"
        );
        regiao.extend_from_slice(assinatura);
        regiao
    }

    /// Faz o parsing do envelope completo e separa a assinatura.
    ///
    /// A ordem das verificações é normativa (`docs/message_format.md`
    /// §Regras de parsing): mínimo → máximo → versão.
    ///
    /// # Erros
    /// - `Curto` — menos de 117 bytes;
    /// - `Grande` — mais de `MAX_ENVELOPE` (65 692 bytes);
    /// - `VersaoDesconhecida` — o primeiro byte não está em
    ///   [`VERSOES_ACEITE`].
    pub fn abrir(envelope: &[u8]) -> Result<(Envelope, [u8; TAM_ASSINATURA]), ErroEnvelope> {
        if envelope.len() < TAM_MINIMO {
            return Err(ErroEnvelope::Curto {
                obtido: envelope.len(),
            });
        }
        // Limite máximo ANTES de qualquer alocação proporcional ao
        // comprimento (proteção contra *memory exhaustion*).
        if envelope.len() > MAX_ENVELOPE {
            return Err(ErroEnvelope::Grande {
                obtido: envelope.len(),
            });
        }
        // Versão: aceita-se qualquer uma de `VERSOES_ACEITE`, não apenas
        // a que escrevemos. A lista tem um elemento hoje; o mecanismo é o
        // que permite a coexistência de versões quando a houver.
        if !versao_aceite(envelope[0]) {
            return Err(ErroEnvelope::VersaoDesconhecida {
                obtida: envelope[0],
            });
        }
        // Fatias fixas: o comprimento mínimo garante que todos os
        // offsets abaixo são válidos (análise por comprimento).
        let mut nonce1 = [0u8; TAM_NONCE];
        let mut nonce5 = [0u8; TAM_NONCE];
        let mut nonce9 = [0u8; TAM_NONCE];
        nonce1.copy_from_slice(&envelope[1..13]);
        nonce5.copy_from_slice(&envelope[13..25]);
        nonce9.copy_from_slice(&envelope[25..37]);
        let fim = envelope.len() - TAM_ASSINATURA;
        let mut assinatura = [0u8; TAM_ASSINATURA];
        assinatura.copy_from_slice(&envelope[fim..]);
        Ok((
            Envelope::novo(
                nonce1,
                nonce5,
                nonce9,
                envelope[TAM_CABECALHO..fim].to_vec(),
            ),
            assinatura,
        ))
    }
}

impl From<ErroEnvelope> for ErroPipeline {
    /// Conversão para o erro unificado do pipeline.
    ///
    /// `Grande` vira `PayloadGrande` (IPC `0x09`) — é um limite de
    /// tamanho, não um envelope malformado; as restantes variantes
    /// mantêm `EnvelopeInvalido` (IPC `0x06`).
    fn from(erro: ErroEnvelope) -> Self {
        match erro {
            ErroEnvelope::Grande { obtido } => ErroPipeline::PayloadGrande {
                obtido,
                maximo: MAX_ENVELOPE,
            },
            outro => ErroPipeline::EnvelopeInvalido(outro.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Envelope de teste com `tamanho` bytes de ciphertext.
    fn envelope_de_teste(tamanho: usize) -> Envelope {
        Envelope::novo([1u8; 12], [2u8; 12], [3u8; 12], vec![0x5Cu8; tamanho])
    }

    /// `fechar_regiao(regiao, assinatura)` dá exactamente o mesmo
    /// envelope que `fechar(assinatura)`, byte a byte.
    ///
    /// São dois caminhos para a mesma saída — um que reconstrói a região
    /// e outro que a reaproveita. Se divergirem, a assinatura passa a
    /// ser verificada sobre bytes que o receptor não tem, e o erro
    /// aparece como «envelope corrompido» em produção, longe daqui.
    /// Por isso a igualdade é byte a byte e não por campo.
    #[test]
    fn fechar_regiao_equivale_a_fechar() {
        for tamanho in [0usize, 1, 31, 32, 33, 4096] {
            let env = envelope_de_teste(tamanho);
            let assinatura = [0xA5u8; TAM_ASSINATURA];

            let por_regiao = Envelope::fechar_regiao(env.regiao_assinada(), &assinatura);
            let por_fechar = env.fechar(&assinatura);

            assert_eq!(
                por_regiao, por_fechar,
                "divergem com {tamanho} bytes de ciphertext"
            );
            assert_eq!(por_regiao.len(), TAM_CABECALHO + tamanho + TAM_ASSINATURA);
        }
    }

    /// A região assinada já vem com espaço para a assinatura.
    ///
    /// É o invariante que torna [`Envelope::fechar_regiao`] uma
    /// operação sem realocação. Se `regiao_assinada` voltar a reservar
    /// só o exacto, o `extend_from_slice` da assinatura realoca e copia
    /// o envelope inteiro — e nada mais avisa: o resultado continua
    /// correcto, só fica mais lento e mais guloso, o pior tipo de
    /// regressão.
    #[test]
    fn regiao_assinada_reserva_a_assinatura() {
        for tamanho in [0usize, 1, 4096] {
            let regiao = envelope_de_teste(tamanho).regiao_assinada();
            assert_eq!(regiao.len(), TAM_CABECALHO + tamanho);
            assert!(
                regiao.capacity() - regiao.len() >= TAM_ASSINATURA,
                "faltam {} bytes para a assinatura (ciphertext de {tamanho})",
                TAM_ASSINATURA - (regiao.capacity() - regiao.len())
            );
        }
    }

    /// `fechar_regiao` **não** realoca: a `Vec` devolvida é a mesma
    /// alocação que a recebida, com a assinatura no fim.
    ///
    /// Compara o endereço base antes e depois. Se o `realloc` realocar
    /// para outro sítio, o endereço muda e o teste falla; se o
    /// alocador decidir devolver o mesmo endereço num `realloc`, o teste
    /// passa — nunca falha por engano, que é o que interessa numa prova
    /// de alocação.
    #[test]
    fn fechar_regiao_reaproveita_a_alocacao() {
        let env = envelope_de_teste(4096);
        let regiao = env.regiao_assinada();
        let antes = regiao.as_ptr();
        let capacidade_antes = regiao.capacity();

        let fechado = Envelope::fechar_regiao(regiao, &[0x5Au8; TAM_ASSINATURA]);

        assert_eq!(
            fechado.as_ptr(),
            antes,
            "a alocação mudou: houve cópia do envelope"
        );
        assert_eq!(
            fechado.capacity(),
            capacidade_antes,
            "a capacidade mudou: a região não reservava a assinatura"
        );
        assert_eq!(
            &fechado[fechado.len() - TAM_ASSINATURA..],
            &[0x5Au8; TAM_ASSINATURA][..]
        );
    }

    /// Envelope round-trip: montar → região assinada → fechar → abrir.
    #[test]
    fn roundtrip_do_envelope() {
        let env = Envelope::novo([1u8; 12], [2u8; 12], [3u8; 12], vec![9u8; 40]);
        let mut fechado = env.fechar(&[7u8; TAM_ASSINATURA]);
        // Total = 101 fixos + N(40) — a assinatura está incluída.
        assert_eq!(fechado.len(), 101 + 40);

        let (aberto, sig) = Envelope::abrir(&fechado).expect("abre");
        assert_eq!(aberto, env);
        assert_eq!(sig, [7u8; TAM_ASSINATURA]);

        // A região assinada termina exatamente onde a assinatura começa.
        let regiao = env.regiao_assinada();
        assert_eq!(regiao.len(), 37 + 40);
        assert_eq!(&fechado[..regiao.len()], &regiao[..]);
        // Assinatura é o bloco final de 64 bytes.
        assert_eq!(&fechado[regiao.len()..], &[7u8; TAM_ASSINATURA][..]);
        // `fechar` não pode ser reaproveitado sem revalidar.
        let ultimo = fechado.split_off(regiao.len());
        assert_eq!(ultimo.len(), TAM_ASSINATURA);
    }

    /// Comprimento abaixo de 117 → `Curto`.
    #[test]
    fn envelope_curto() {
        let erro = Envelope::abrir(&[VERSAO; TAM_MINIMO - 1]).unwrap_err();
        assert_eq!(
            erro,
            ErroEnvelope::Curto {
                obtido: TAM_MINIMO - 1
            }
        );
        // A conversão para o erro de pipeline preserva a semântica 0x06.
        let pipeline = ErroPipeline::from(erro);
        assert!(matches!(pipeline, ErroPipeline::EnvelopeInvalido(_)));
    }

    /// Versão do formato desconhecida → `VersaoDesconhecida`.
    #[test]
    fn versao_desconhecida() {
        let mut pacote = vec![0x02u8; TAM_MINIMO];
        pacote[0] = 0x02;
        let erro = Envelope::abrir(&pacote).unwrap_err();
        assert_eq!(erro, ErroEnvelope::VersaoDesconhecida { obtida: 0x02 });
        let pipeline = ErroPipeline::from(erro);
        assert!(matches!(pipeline, ErroPipeline::EnvelopeInvalido(_)));
    }

    /// Comprimento exatamente no mínimo (N = 16) é aceite.
    #[test]
    fn envelope_no_minimo() {
        let env = Envelope::novo([0u8; 12], [0u8; 12], [0u8; 12], vec![0u8; 16]);
        let fechado = env.fechar(&[0u8; TAM_ASSINATURA]);
        assert_eq!(fechado.len(), TAM_MINIMO);
        let (aberto, _) = Envelope::abrir(&fechado).expect("mínimo válido");
        assert_eq!(aberto.ciphertext.len(), 16);
    }

    /// `Display` dos erros: estável e sem conteúdo do envelope.
    ///
    /// Cobre os **três** braços: um `Display` incompleto é uma
    /// mensagem de diagnóstico que falta ao utilizador no momento em
    /// que mais precisa dela.
    #[test]
    fn display_dos_erros() {
        assert_eq!(
            ErroEnvelope::Curto { obtido: 10 }.to_string(),
            "envelope curto: 10 bytes (mínimo 117)"
        );
        assert_eq!(
            ErroEnvelope::Grande {
                obtido: MAX_ENVELOPE + 1
            }
            .to_string(),
            format!("envelope demasiado grande: {} bytes (máximo {MAX_ENVELOPE})", MAX_ENVELOPE + 1)
        );
        assert_eq!(
            ErroEnvelope::VersaoDesconhecida { obtida: 0x99 }.to_string(),
            "versão de envelope desconhecida: 0x99"
        );
    }

    /// O texto de erro **nunca** pode incluir o conteúdo do envelope —
    /// é a política de erros de `docs/security_model.md`. Verificado
    /// nas três variantes, porque um erro de formatação novo pode
    /// reintroduzi-lo.
    #[test]
    fn display_nao_vaza_conteudo() {
        let erros = [
            ErroEnvelope::Curto { obtido: 10 },
            ErroEnvelope::Grande {
                obtido: MAX_ENVELOPE + 1,
            },
            ErroEnvelope::VersaoDesconhecida { obtida: 0x99 },
        ];
        for erro in erros {
            let texto = erro.to_string();
            // Só Sizes e bytes de versão são legítimos; nada de hex.
            assert!(
                !texto.contains("a1b2c3") && !texto.to_lowercase().contains("nonce"),
                "erro com conteúdo de envelope: {texto}"
            );
            assert!(texto.len() < 120, "erro demasiado verboso: {texto}");
        }
    }
}
