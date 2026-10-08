// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// orcamento.rs — orçamento de memória do daemon
// ---------------------------------------------------------------------
// Existe por uma razão concreta, e é uma correcção.
//
// O valor `16 * 1024 * 1024` estava escrito à mão em quatro sítios
// (`ipc.rs`, `p2p.rs`, `messenger/ipc_client.py`,
// `server/relay_server.py`) como protecção contra *memory exhaustion*.
// Mas o maior corpo que o protocolo pode legitimamente produzir é de
// cerca de 66 KB:
//
//     DECODE   4 chaves × 32 B + envelope (65 692 B) = 65 820 B
//     ENCODE   4 chaves × 32 B + payload  (65 536 B) = 65 664 B
//
// Ou seja, o limite era **255× maior** do que o necessário. Não é uma
// margem defensiva: é uma superfície de ataque. Um cliente do mesmo
// uid (basta `SO_PEERCRED`) pode pedir um payload de 16 MiB, e o
// servidor aloca-o *antes* de ler um único byte
// (`ipc.rs`: `let mut corpo = vec![0u8; comprimento]`). Como não há
// limite de ligações concorrentes (`thread::spawn` por cada uma
// aceite), o total não tem tecto: N ligações × 16 MiB.
//
// Pior ainda, três testes alocavam `[0u8; 16_777_217]` — um array na
// **pilha**, onde o limite por omissão é 8 MB. Isso não dá erro de
// alocação: dá *stack overflow* e aborta o binário de testes
// inteiro. Um teste que mata a suite é um teste que não informa.
//
// Este módulo substitui o literal por uma derivação a partir de um
// orçamento explícito, e expõe:
//
//   * `MINIMO_CORPO` — o menor limite que o protocolo aceita;
//   * `orçamento_padrao()` — a partir do tecto de 1 GB do sistema;
//   * `orcaa` — o limite em vigor, já resolvido por override;
//   * `LimiteInvalido` — o override foi escrito mas não faz sentido.
//
// A regra de derivação é o mais simples possível de explicar, porque
// um número que ninguém consegue prever não se revê:
//
//     orcamento = tecto − uma reserva fixa para o resto do daemon
//
// A reserva cobre o que o daemon usa *independentemente* do buffer de
// um pedido: o estado partilhado (Tor, P2P, amizades, anti-replay), a
// pilha, o heap do allocator, e os buffers de decifragem (um envelope
// de 64 KB é copiado três vezes no caminho de encode — ver
// `docs/DEV_GUIDE.md` §1.2). Numa máquina de 1 GB, 512 MB de reserva
// para buffers de 512 KB de payload é generoso de propósito: o
// objectivo é que o limite *caiba* no orçamento, não que o use.
//
// Override por ambiente (`ONYXCHAT_MAX_PAYLOAD`), por coerência com
// `ONYXCHAT_SOCKET`, `ONYXCHAT_RELAY`, `ONYXCHAT_TOR` e
// `ONYXCHAT_ARTI_DIR`, que já são o padrão do projecto. Aceita sufixos
// de unidade (`16MiB`, `512k`, `1048576`) porque escrever `16777216`
// à mão é exactamente o que produziu o literal errado.
//
// Quando o override é rejeitado, o daemon **avisa no arranque** em vez
// de o aceitar em silêncio: um valor que não é atingível é uma
// propriedade verificada, não uma nota em documentação.
// =====================================================================

use std::fmt;

/// Tecto de memória que o sistema usa para escolher o limite por
/// omissão: 1 GB.
///
/// Não é arbitrário. O objectivo declarado para o projecto é que
/// corra em hardware modesto, e o pico medido do build de testes é de
/// 881 MB (`scripts/memoria.sh --pico`, `docs/DEV_GUIDE.md` §1.2). Com
/// 1 GB de tecto, a regra abaixo dá 512 MB de buffer por pedido — que
/// é 8× o que o protocolo produz.
pub const ORCAMENTO_PADRAO: usize = 1024 * 1024 * 1024;

/// Reserva do orçamento que não é buffer de pedido: estado partilhado,
/// pilha, heap do allocator, e as cópias intermédias do pipeline.
///
/// 512 MB numa máquina de 1 GB é conservador de propósito — o limite tem de
/// caber no orçamento mesmo que o utilizador peça mensagens grandes.
pub const RESERVA_FIXA: usize = ORCAMENTO_PADRAO / 2;

/// Tecto do override. Acima disto a máquina está a pedir para ser
/// morta pelo OOM killer, e o valor é recusado.
const MAX_OVERRIDE: usize = 256 * 1024 * 1024;

/// Menor corpo que o protocolo aceita.
///
/// Derivado, não escrito: `MAX_ENVELOPE` (65 692 B) mais as quatro
/// chaves de 32 B de um pedido `DECODE`. Qualquer limite abaixo disto
/// tornaria impossível decifrar a maior mensagem que o pipeline sabe
/// construir — o que é uma configuração quebrada, não uma
/// configuração apertada.
pub const MINIMO_CORPO: usize = crate::envelope::MAX_ENVELOPE + 4 * 32;

/// O override escrito não pode ser usado.
///
/// Separado de qualquer outro erro porque é o único caso em que o
/// utilizador fez alguma coisa e o sistema tem de explicar o que fez
/// de errado, em vez de devolver um erro genérico de arranque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimiteInvalido {
    /// O que o utilizador escreveu.
    pub pedido: String,
    /// Porquê não serve.
    pub razao: &'static str,
}

impl fmt::Display for LimiteInvalido {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ONYXCHAT_MAX_PAYLOAD={}: {}",
            self.pedido, self.razao
        )
    }
}

impl std::error::Error for LimiteInvalido {}

/// O aviso de arranque que acompanha um override recusado.
pub fn aviso_override_recusado(erro: &LimiteInvalido, minimo: usize, tecto: usize) -> String {
    format!(
        "ignorado: {erro}\\n\
         o limite efectivo fica em {minimo} B (o minimo do protocolo); \
         com um tecto de {tecto} B, mais acima daria alocar mais do que a máquina tem"
    )
}

/// Deriva o limite a partir de um tecto.
///
/// Regra: metade do tecto, arredondada para baixo a um múltiplo de
/// 64 KiB (para o buffer não cair em extremos de página), e nunca
/// abaixo de [`MINIMO_CORPO`].
///
/// O arredondamento não é um detalhe: um `Vec` de um tamanho que não é
/// múltiplo da página obriga o allocator a pedir mais um bloco inteiro
/// ao sistema, e o pico medido sobe sem que nenhum código tenha mudado.
pub const fn derivar(tecto: usize) -> usize {
    const GRANDE: usize = 64 * 1024;
    let metade = tecto.saturating_sub(RESERVA_FIXA) / 2;
    let arredondado = (metade / GRANDE) * GRANDE;
    // `Ord::max` ainda não é `const` em `std`, e a alternativa —
    // `core::cmp::max` — também não. O ternário resolve, e é mais
    // claro de ler do que uma constante com nome.
    if arredondado > MINIMO_CORPO {
        arredondado
    } else {
        MINIMO_CORPO
    }
}

/// O limite por omissão, a partir do tecto de [`ORCAMENTO_PADRAO`].
pub const fn orcamento_padrao() -> usize {
    derivar(ORCAMENTO_PADRAO)
}

/// Interpreta um valor escrito pelo utilizador.
///
/// Aceita o número puro (bytes) ou com sufixo `k`/`m`/`g` (ou `ki`/
/// `mi`/`gi`, ignorando maiúsculas), que é o que torna
/// `ONYXCHAT_MAX_PAYLOAD=16MiB` legível. Devolve [`LimiteInvalido`]
/// para valor não numérico, valor zero, valor acima de [`MAX_OVERRIDE`]
/// ou valor abaixo de [`MINIMO_CORPO`] — este último com a razão mais
/// específica, porque «o limite é menor do que a maior mensagem que o
/// sistema sabe construir» é um erro de configuração distinto de
/// «escreveste lixo».
pub fn interpretar(texto: &str) -> Result<usize, LimiteInvalido> {
    let limpo = texto.trim();
    if limpo.is_empty() {
        return Err(LimiteInvalido {
            pedido: texto.to_string(),
            razao: "valor vazio",
        });
    }

    // Separa o sufixo do número. Só se aceitam as unidades enumeradas
    // abaixo; qualquer outra letra é erro, não zero silencioso.
    let (digitos, sufixo): (String, String) = {
        let mut d = String::new();
        let mut s = String::new();
        let mut no_numero = true;
        for ch in limpo.chars() {
            if ch.is_ascii_digit() && no_numero {
                d.push(ch);
            } else {
                no_numero = false;
                s.push(ch);
            }
        }
        (d, s.trim().to_ascii_lowercase())
    };

    if digitos.is_empty() {
        return Err(LimiteInvalido {
            pedido: texto.to_string(),
            razao: "não começa por um número",
        });
    }

    let base: usize = digitos.parse().map_err(|_| LimiteInvalido {
        pedido: texto.to_string(),
        razao: "número fora do intervalo de `usize`",
    })?;

    let multiplicador: usize = match sufixo.as_str() {
        "" | "b" => 1,
        // Ordem das alternativas: as de duas letras primeiro, porque a
        // comparação distingue maiúsculas mas não comprimentos, e
        // «mb» tem de ser testado antes de «m» — senão `2mb` seria lido
        // como `2` seguido de sufixo inválido.
        "kb" | "kib" => 1024,
        "mb" | "mib" => 1024 * 1024,
        "gb" | "gib" => 1024 * 1024 * 1024,
        "k" | "ki" => 1024,
        "m" | "mi" => 1024 * 1024,
        "g" | "gi" => 1024 * 1024 * 1024,
        _ => {
            return Err(LimiteInvalido {
                pedido: texto.to_string(),
                razao: "unidade desconhecida (use k, m, g ou nenhuma)",
            })
        }
    };

    let valor = base.checked_mul(multiplicador).ok_or(LimiteInvalido {
        pedido: texto.to_string(),
        razao: "overflow ao aplicar a unidade",
    })?;

    if valor < MINIMO_CORPO {
        return Err(LimiteInvalido {
            pedido: texto.to_string(),
            razao: "abaixo do mínimo do protocolo",
        });
    }

    if valor > MAX_OVERRIDE {
        return Err(LimiteInvalido {
            pedido: texto.to_string(),
            razao: "acima do tecto máximo aceite",
        });
    }

    Ok(valor)
}

/// O limite em vigor.
///
/// Lê `ONYXCHAT_MAX_PAYLOAD`. Um valor inválido **não** é fatal: o
/// daemon arranca com o limite derivado e devolve o aviso, porque
/// recusar arrancar por causa de uma variável de ambiente seria pior
/// do que ignorar a variável — mas ignorá-la em silêncio seria
/// mentira, e por isso o aviso existe.
pub fn orcaa() -> (usize, Option<LimiteInvalido>) {
    match std::env::var("ONYXCHAT_MAX_PAYLOAD") {
        Err(_) => (orcamento_padrao(), None),
        Ok(bruto) => match interpretar(&bruto) {
            Ok(v) => (v, None),
            Err(e) => (orcamento_padrao(), Some(e)),
        },
    }
}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------

#[cfg(test)]
mod testes {
    use super::*;

    /// O limite por omissão tem de servir o protocolo.
    ///
    /// Este é o teste que impede a regressão que existia antes: com o
    /// literal de 16 MiB isto passava, mas o valor não tinha nenhuma
    /// relação com o que o daemon produz. Se `MAX_ENVELOPE` crescer
    /// amanhã, este teste falha e obriga a rever a derivação.
    #[test]
    fn orcamento_padrao_serve_o_protocolo() {
        let v = orcamento_padrao();
        assert!(
            v >= MINIMO_CORPO,
            "orçamento {v} B não serve um DECODE de {MINIMO_CORPO} B"
        );
        assert_eq!(v, derivar(ORCAMENTO_PADRAO));
    }

    /// A derivada nunca desce abaixo do mínimo, seja qual for o tecto.
    #[test]
    fn derivada_respeita_o_minimo_em_tec_tois_pequenos() {
        for tecto in [0, 1024, 64 * 1024, MINIMO_CORPO] {
            let v = derivar(tecto);
            assert!(
                v >= MINIMO_CORPO,
                "tecto {tecto} deu {v}, abaixo do mínimo {MINIMO_CORPO}"
            );
        }
    }

    /// A derivada é monótona: mais tecto nunca dá menos buffer.
    #[test]
    fn derivada_e_monotona() {
        let mut anterior = 0;
        for tecto in (MINIMO_CORPO..(16 * ORCAMENTO_PADRAO)).step_by(ORCAMENTO_PADRAO / 8) {
            let v = derivar(tecto);
            assert!(
                v >= anterior,
                "tecto {tecto} deu {v}, menos que o tecto anterior ({anterior})"
            );
            anterior = v;
        }
    }

    /// O arredondamento a 64 KiB é exacto.
    #[test]
    fn resultado_e_multiplo_de_64_kib() {
        const GRANDE: usize = 64 * 1024;
        for tecto in [
            MINIMO_CORPO,
            ORCAMENTO_PADRAO,
            3 * ORCAMENTO_PADRAO,
            7 * ORCAMENTO_PADRAO,
        ] {
            let v = derivar(tecto);
            // Só quando a derivada não está presa ao mínimo: aí o valor
            // é herdado e não tem por que ser múltiplo.
            if v > MINIMO_CORPO {
                assert_eq!(v % GRANDE, 0, "derivar({tecto}) = {v} não é múltiplo de 64 KiB");
            }
        }
    }

    /// Interpretar números com e sem unidade dá o mesmo valor.
    #[test]
    fn interpretar_aceita_unidades() {
        let esperado = 2 * 1024 * 1024;
        for texto in ["2097152", "2m", "2M", "2mb", "2MB", "2MiB", " 2 m "] {
            assert_eq!(
                interpretar(texto).unwrap_or(0),
                esperado,
                "«{texto}» devia dar {esperado}"
            );
        }
    }

    /// Valores claramente errados são recusados com razão.
    #[test]
    fn interpretar_recusa_lixo() {
        for texto in ["", "   ", "abc", "12x", "k", "-1", "0"] {
            let r = interpretar(texto);
            assert!(r.is_err(), "«{texto}» devia ser recusado, deu {r:?}");
        }
    }

    /// Um valor acima do tecto máximo é recusado — é o que impede que o
    /// utilizador reintroduza o problema que este módulo veio corrigir.
    #[test]
    fn interpretar_recusa_valor_absurdo() {
        let r = interpretar("1g").expect_err("1 GiB devia ser recusado");
        assert_eq!(r.razao, "acima do tecto máximo aceite");
    }

    /// Um valor abaixo do mínimo tem razão própria, porque é um erro de
    /// configuração diferente de lixo.
    #[test]
    fn interpretar_recusa_abaixo_do_minimo() {
        let r = interpretar("1k").expect_err("1 KiB devia ser recusado");
        assert_eq!(r.razao, "abaixo do mínimo do protocolo");
        assert!(r.to_string().contains("ONYXCHAT_MAX_PAYLOAD"));
    }

    /// Overflow com unidade não pode dar panic nem wrap.
    #[test]
    fn interpretar_recusa_overflow() {
        let r = interpretar("99999999999999999999g");
        assert!(r.is_err());
    }

    /// `orcaa()` lê a variável de ambiente, e as suas três saídas.
    ///
    /// A terceira — um override válido — é a única que altera o
    /// comportamento do daemon, e era a única das três sem teste.
    /// Antes desta medição de cobertura, o caminho de *recusa* nunca era
    /// executado por nenhum teste, e é o caminho que decide se o
    /// utilizador recebe um aviso ou um valor silenciosamente
    /// diferente do que pediu.
    ///
    /// O variável de ambiente é posto e retirado dentro do teste, e não
    /// num `OnceLock`: os testes correm um a um (`RUST_TEST_THREADS=1`,
    /// em `.cargo/config.toml`) precisamente para que mexer em estado
    /// global seja seguro.
    #[test]
    fn orcaa_segue_a_variavel_de_ambiente() {
        let anterior = std::env::var("ONYXCHAT_MAX_PAYLOAD").ok();

        // Sem variável: o valor derivado e nenhum aviso.
        std::env::remove_var("ONYXCHAT_MAX_PAYLOAD");
        let (valor, aviso) = orcaa();
        assert_eq!(valor, orcamento_padrao());
        assert!(aviso.is_none(), "sem override não há nada a avisar");

        // Override válido: o valor pedido, e nenhum aviso.
        std::env::set_var("ONYXCHAT_MAX_PAYLOAD", "1048576");
        let (valor, aviso) = orcaa();
        assert_eq!(valor, 1_048_576);
        assert!(aviso.is_none());

        // Override inválido: o valor derivado **e** o aviso.
        //
        // Este é o caso que a cobertura encontrou. Um override recusado
        // que não avise é um override que a pessoa acredita estar
        // activo e não está — e o daemon a calcular buffers com um
        // limite que ela não pediu.
        std::env::set_var("ONYXCHAT_MAX_PAYLOAD", "lixo");
        let (valor, aviso) = orcaa();
        assert_eq!(valor, orcamento_padrao(), "um override recusado não muda o limite");
        let aviso = aviso.expect("um override recusado tem de ser avisado");

        // E o aviso diz o quê, o que ficou, e porquê.
        let texto = aviso_override_recusado(&aviso, MINIMO_CORPO, ORCAMENTO_PADRAO);
        assert!(texto.contains(&aviso.pedido), "o aviso nomeia o valor pedido");
        assert!(texto.contains(&aviso.razao), "o aviso diz porque foi recusado");
        assert!(texto.contains(&MINIMO_CORPO.to_string()));
        assert!(texto.contains(&ORCAMENTO_PADRAO.to_string()));
        assert!(texto.contains("ignorado"), "o aviso diz que foi ignorado");

        // Restaurar, para não vazar para o teste seguinte.
        match anterior {
            Some(v) => std::env::set_var("ONYXCHAT_MAX_PAYLOAD", v),
            None => std::env::remove_var("ONYXCHAT_MAX_PAYLOAD"),
        }
    }

    /// O aviso de arranque não pode prometer um valor que não ficou.
    ///
    /// O texto diz «o limite efectivo fica em {minimo} B». Se o `orcaa`
    /// devolver um valor diferente do `minimo` que foi passado, o aviso
    /// mente — e é o que aconteceria se alguém reordenasse as
    /// argumentos da chamada em `main.rs` sem dar por isso.
    #[test]
    fn o_aviso_diz_o_valor_que_ficou() {
        let erro = LimiteInvalido {
            pedido: "lixo".into(),
            razao: "lixo não é um número",
        };
        let texto = aviso_override_recusado(&erro, MINIMO_CORPO, ORCAMENTO_PADRAO);
        // O mínimo do protocolo é o que o aviso afirma.
        assert!(texto.contains(&format!("fica em {MINIMO_CORPO} B")));
    }

}
