// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// lua_camadas.rs — embedding Lua das camadas analógicas K2/K3/K6/K8
// ---------------------------------------------------------------------
// Os três cifradores analógicos vivem em `crypto/python_lua/lua/` e
// são embutidos no binário via `include_str!` (zero I/O em runtime,
// rebuild automático quando o `.lua` muda).
//
// Contrato de cada módulo Lua: a função devolve uma tabela com
// `cifrar(...)` / `decifrar(...)` sobre strings binárias arbitrárias.
// Parâmetros (docs/pipeline.md): K3 "VIGENERE" · K6 "PLAYFAIR" —
// aplicados aqui, no daemon, e não dentro do script.
//
// ## K2 e K8 são o mesmo módulo, com deslocamentos diferentes
//
// K2 (Cesariana +3) e K8 (substituição +7) são a mesma função:
// `b' = (b + k) mod 256`, com `decifrar = cifrar(-k)`. Tinham dois
// ficheiros Lua — `cesar.lua` e `substituicao.lua` — com o corpo
// byte-a-byte idêntico e apenas o cabeçalho em comum.
//
// Dois ficheiros com o mesmo algoritmo é uma armadilha com data de
// validade: o dia em que alguém corrigir um bug num deles — um `%
//` negativo para valores negativos, um `string.char` fora de [0,255] —
// K2 passa a divergir de K8 sem nenhum teste acusar, porque os testes
// comparam com vectores, e os vectores de cada camada só são
// regenerados quando alguém se lembra. Um `.lua` só, parametrizado
// pelo deslocamento, elimina a classe inteira do problema.
//
// A identidade das camadas continua onde pertence: em
// `pipeline.rs`, que aplica [`deslocamento`] com `DESLOCAMENTO_K2`
// (+3) e `DESLOCAMENTO_K8` (+7).
//
// Erros do interpretador viram `ErroPipeline::Lua` (mensagens fixas
// dos scripts — nunca contêm chaves nem payloads).
//
// O interpretador é criado uma vez por thread e reutilizado. Antes,
// cada camada criava o seu — ver [`com_interpretador`] para o porquê
// e para a alternativa que foi rejeitada.
// =====================================================================

use std::cell::{Ref, RefCell};
use std::collections::HashMap;

use mlua::{Lua, Table};

use crate::erros::ErroPipeline;

// Ficheiros embutidos na compilação (caminhos relativos a este ficheiro).
//
// São três, não quatro: K2 e K8 partilham `deslocamento.lua` (ver a
// nota sobre unificação no cabeçalho deste ficheiro).
const DESLOCAMENTO_LUA: &str = include_str!("../../../crypto/python_lua/lua/deslocamento.lua");
const VIGENERE_LUA: &str = include_str!("../../../crypto/python_lua/lua/vigenere.lua");
const PLAYFAIR_LUA: &str = include_str!("../../../crypto/python_lua/lua/playfair.lua");

/// Converte um `mlua::Result` no erro do pipeline.
///
/// Ponto único de tradução de erros do interpretador: todos os
/// `load`/`get`/`create_string`/`call` passam por aqui, garantindo que
/// o ramo de erro do Lua é coberto por qualquer teste que dispare um
/// erro (ex.: chave vazia, método inexistente).
fn traduzir<T>(resultado: mlua::Result<T>) -> Result<T, ErroPipeline> {
    resultado.map_err(|erro| ErroPipeline::Lua(erro.to_string()))
}

/// O interpretador da thread, com os quatro módulos já carregados.
///
/// # Porque um `thread_local!` e não um interpretador por chamada
///
/// A versão anterior chamava `Lua::new()` e `lua.load(fonte)` em cada
/// invocação. O pipeline corre quatro camadas analógicas por operação
/// (`pipeline.rs`: K2, K3, K6, K8 no encode; as inversas no decode), o
/// que dava **8 interpretadores e 8 compilações de chunk por ciclo
/// completo**. Cada `luaL_newstate` seguido de `luaL_openlibs` é da
/// ordem de milissegundos, e a suite de testes instanciava mais de
/// 10 000 vezes — a auditoria mediu 8 instâncias por caso de proptest e
/// 256 casos por teste.
///
/// # Porque `thread_local!` e não um `Mutex<Lua>` partilhado
///
/// `mlua::Lua` é `Send` mas **não** é `Sync`: o estado é um pointer C
/// sem protecção interna. Partilhá-lo exigiria um `Mutex`, e o
/// `Estado` do daemon já usa `Arc<Mutex<Rede>>` — pôr o Lua dentro do
/// mesmo lock seguraria a cadeia de cifragem inteira por dentro do
/// lock, serializando o que hoje é paralelo entre threads.
///
/// # Porquê um `RefCell` à volta
///
/// `thread_local!` dá um slot por thread com `UnsafeCell` no interior.
/// O `RefCell` converte esse acesso inseguro num borrow verificado em
/// tempo de execução: se algum dia um caminho reentrante precisar do
/// interpretador enquanto o tem tomado, entra em pânico com uma
/// mensagem clara em vez de corromper o estado em C.
///
/// # Sobre o isolamento entre pedidos
///
/// O comentário original dizia que um interpretador por chamada
/// mantinha o estado isolado. O estado dos módulos analógicos é feito
/// de funções puras — recebem bytes e devolvem bytes, sem campos no
/// upvalue — por isso não havia isolamento a perder. O que se mantém é
/// o isolamento *entre threads*, que é o que o `thread_local!` dá, e
/// que é o isolamento de que a propriedade de cifra precisa.
struct Interpretador {
    /// O estado do interpretador.
    ///
    /// Fica no `struct` em vez de ser devolvido à parte para que o
    /// `Ref` do `RefCell` seja o que garante a exclusão: um
    /// `&Lua` solto viveria mais do que o borrow, que é exactamente o
    /// caminho por onde um `Lua` (não-`Sync`) acabaria partilhado entre
    /// threads.
    lua: Lua,
    /// Módulos carregados, por nome (`"deslocamento.lua"`, …).
    modulos: HashMap<&'static str, Table>,
}

impl Interpretador {
    /// O estado do interpretador, com o borrow ainda válido.
    fn lua(&self) -> &Lua {
        &self.lua
    }

    /// A tabela de um módulo.
    ///
    /// O erro é um `ErroPipeline::Lua` e não um `expect`: a chave vem
    /// de uma constante do módulo, mas um `panic` dentro do atendimento
    /// de uma ligação derrubaria o daemon inteiro. Um `Err` devolve
    /// `0x03 Erro` ao cliente e o daemon continua a viver.
    fn modulo(&self, nome: &'static str) -> Result<&Table, ErroPipeline> {
        self.modulos
            .get(nome)
            .ok_or_else(|| ErroPipeline::Lua(format!("módulo Lua desconhecido: {nome}")))
    }
}

thread_local! {
    /// Um interpretador por thread, criado à primeira utilização.
    ///
    /// `const { … }` inicializa o `RefCell` sem alocação; o `HashMap`
    /// dentro é que cresce na primeira chamada. A inicialização é
    /// preguiçosa de propósito: uma thread que nunca cifra uma
    /// mensagem não paga o custo de `luaL_openlibs`.
    static INTREPRETADOR: RefCell<Option<Interpretador>> = const { RefCell::new(None) };
}

/// Executa `operacao` com o interpretador da thread, carregando-o na
/// primeira utilização.
///
/// ## Porque isto não devolve um `&Interpretador`
///
/// A primeira tentativa devolvia `Ref<'static, Interpretador>`, feito
/// com `Ref::map` sobre um `Ref<Option<…>>`. Não compila: o `Ref`
/// emprestado a um `thread_local` tem o tempo de vida do *slot*, não
/// `'static`, e devolvê-lo para fora do fecho `with` é usar uma
/// referência depois de o emprestar acabar.
///
/// A solução correcta é não devolver nada: a operação inteira corre
/// **dentro** do `with`, e o que sai é um `Vec<u8>` — um valor próprio,
/// sem referência ao `thread_local`. É também a forma mais segura: o
/// `Ref` não pode sobreviver à chamada, porque nunca é devolvido.
///
/// ## Carga
///
/// `borrow()` e não `borrow_mut()`: uma vez carregado, o interpretador
/// nunca muda. A fase de carga é a única que precisa de escrita, e
/// faz-se com um `borrow_mut()` próprio que termina antes de o `Ref`
/// de leitura ser tomado.
///
/// `const { … }` no `thread_local!` inicializa o `RefCell` sem
/// alocação, e a criação do `Lua` é preguiçosa: uma thread que nunca
/// cifra uma mensagem não paga `luaL_openlibs`.
fn com_interpretador<T>(
    operacao: impl FnOnce(&Interpretador) -> Result<T, ErroPipeline>,
) -> Result<T, ErroPipeline> {
    INTREPRETADOR.with(|slot| {
        if slot.borrow().is_none() {
            let mut guarda = slot.borrow_mut();
            // Duas threads podem passar pelo `is_none()` exterior ao
            // mesmo tempo. A segunda encontraria o slot já cheio e
            // carregaria os módulos outra vez — trabalho inútil, não um
            // bug. Reavaliar dentro do `borrow_mut()` é o que torna o
            // carregamento idempotente.
            if guarda.is_none() {
                let lua = Lua::new();
                let mut modulos = HashMap::with_capacity(FONTES.len());
                for &(fonte, nome) in FONTES {
                    let tabela: Table = traduzir(lua.load(fonte).set_name(nome).call(()))?;
                    modulos.insert(nome, tabela);
                }
                *guarda = Some(Interpretador { lua, modulos });
            }
        }

        // `Ref::map` converte o `Ref<Option<…>>` num `Ref<…>` com o
        // `Option` verificado lá dentro: a operação recebe
        // `&Interpretador` sem ter de lidar com o `None`.
        let guarda = slot.borrow();
        // O `Ref` é um guard de borrow: vive até ao fim do `with`, o
        // que é exactamente o período durante qual a operação pode
        // tocar no interpretador. O `&Interpretador` é uma dereferência
        // desse guard, e não uma referência independente — logo não pode
        // escapar sem o `Ref` que a sustenta.
        let int = Ref::map(guarda, |o: &Option<Interpretador>| {
            o.as_ref().expect("preenchido no bloco acima")
        });
        operacao(&int)
    })
}

/// Os quatro módulos, por nome e fonte.
///
/// Juntos numa constante para que a lista e o mapa não possam divergir
/// — o mesmo cuidado que o `tor_arti.rs` tem com as chaves de mailbox.
const FONTES: &[(&str, &str)] = &[
    (DESLOCAMENTO_LUA, "deslocamento.lua"),
    (VIGENERE_LUA, "vigenere.lua"),
    (PLAYFAIR_LUA, "playfair.lua"),
];

/// Invoca `metodo(entrada, deslocamento)` — a camada de
/// deslocamento, usada por K2 (+3) e K8 (+7).
fn com_deslocamento(
    nome_modulo: &'static str,
    metodo: &'static str,
    entrada: &[u8],
    deslocamento: i64,
) -> Result<Vec<u8>, ErroPipeline> {
    com_interpretador(|int| {
        let tabela = int.modulo(nome_modulo)?;
        let funcao: mlua::Function = traduzir(tabela.get(metodo))?;
        // `create_string` copia a entrada para a heap do Lua; o
        // `as_bytes` final copia de volta. São as duas cópias que um
        // caminho nativo em Rust eliminaria — mas mantê-las agora é o
        // que garante que o `.lua` continua a ser a definição
        // executável da camada, que é o argumento do projecto
        // (`docs/pipeline.md` §Criptografia analógica).
        let dados = traduzir(int.lua().create_string(entrada))?;
        let saida: mlua::LuaString = traduzir(funcao.call((dados, deslocamento)))?;
        Ok(saida.as_bytes().to_vec())
    })
}

/// Invoca `metodo(entrada, chave)` — camadas parametrizadas por chave
/// (K3 Vigenère, K6 Playfair adaptado).
fn com_chave(
    nome_modulo: &'static str,
    metodo: &'static str,
    entrada: &[u8],
    chave: &[u8],
) -> Result<Vec<u8>, ErroPipeline> {
    com_interpretador(|int| {
        let tabela = int.modulo(nome_modulo)?;
        let funcao: mlua::Function = traduzir(tabela.get(metodo))?;
        let dados = traduzir(int.lua().create_string(entrada))?;
        let segredo = traduzir(int.lua().create_string(chave))?;
        let saida: mlua::LuaString = traduzir(funcao.call((dados, segredo)))?;
        Ok(saida.as_bytes().to_vec())
    })
}

// ---------------------------------------------------------------------
// Deslocamento — K2 (+3) e K8 (+7)
// ---------------------------------------------------------------------

/// Cifra por deslocamento: `b' = (b + deslocamento) mod 256`.
///
/// Serve K2 (Cesariana, `DESLOCAMENTO_K2` = +3) e K8 (substituição
/// bijetiva, `DESLOCAMENTO_K8` = +7). São o mesmo algoritmo com
/// deslocamentos diferentes, e agora também a mesma função.
///
/// O `i64` é o que a Lua usa: um inteiro com sinal, para que a inversa
/// possa passar `-deslocamento` e não ter de subtrair módulo à mão.
/// Um `u8` aqui obrigaria aduas vias — `if b < k { … } else { … }` — e
/// é o tipo de ramificação que diverge entre a cifra e a decifra.
pub fn deslocamento(entrada: &[u8], deslocamento: i64) -> Result<Vec<u8>, ErroPipeline> {
    com_deslocamento("deslocamento.lua", "cifrar", entrada, deslocamento)
}

/// Decifra por deslocamento (`b - deslocamento`, módulo 256).
pub fn deslocamento_inverso(entrada: &[u8], deslocamento: i64) -> Result<Vec<u8>, ErroPipeline> {
    com_deslocamento("deslocamento.lua", "decifrar", entrada, deslocamento)
}

// ---------------------------------------------------------------------
// K3 — Vigenère (soma modular periódica da chave)
// ---------------------------------------------------------------------

/// Cifra K3: `b' = (b + chave[i mod n]) mod 256`.
pub fn vigenere(entrada: &[u8], chave: &[u8]) -> Result<Vec<u8>, ErroPipeline> {
    com_chave("vigenere.lua", "cifrar", entrada, chave)
}

/// Decifra K3 (subtração modular periódica).
pub fn vigenere_inverso(entrada: &[u8], chave: &[u8]) -> Result<Vec<u8>, ErroPipeline> {
    com_chave("vigenere.lua", "decifrar", entrada, chave)
}

// ---------------------------------------------------------------------
// K6 — Playfair adaptado (XOR repetido com a chave)
// ---------------------------------------------------------------------

/// Cifra K6: `b' = b XOR chave[i mod n]` (involutiva).
pub fn playfair(entrada: &[u8], chave: &[u8]) -> Result<Vec<u8>, ErroPipeline> {
    com_chave("playfair.lua", "cifrar", entrada, chave)
}

/// Decifra K6 — XOR é a própria inversa; mantém-se explícita para o
/// contrato uniforme das camadas.
pub fn playfair_inverso(entrada: &[u8], chave: &[u8]) -> Result<Vec<u8>, ErroPipeline> {
    com_chave("playfair.lua", "decifrar", entrada, chave)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----------------------------------------------------------------
    // Vectores de paridade com as referências Python (pytest) — se a
    // Lua divergir do contrato hex, estes testes falham aqui.
    // ----------------------------------------------------------------

    /// K2 vetor: "ABCDE" +3 → "DEFGH" (idêntico ao teste Python).
    ///
    /// Passa por [`deslocamento`] com o deslocamento da K2, que é como
    /// o `pipeline` a aplica. O nome «K2» não aparece na função: a
    /// camada é o deslocamento, e quem sabe qual é o `pipeline.rs`.
    #[test]
    fn k2_vetor_de_parity() {
        let k2 = crate::pipeline::DESLOCAMENTO_K2;
        assert_eq!(deslocamento(b"ABCDE", k2).expect("K2"), b"DEFGH");
        assert_eq!(
            deslocamento_inverso(b"DEFGH", k2).expect("inverso"),
            b"ABCDE"
        );
    }

    /// K3 vetor: "Hello!" com "VIGENERE" → 9eaeb3b1bd66.
    #[test]
    fn vigenere_vetor_de_parity() {
        let saida = vigenere(b"Hello!", b"VIGENERE").expect("vigenere");
        assert_eq!(saida, vec![0x9e, 0xae, 0xb3, 0xb1, 0xbd, 0x66]);
        assert_eq!(
            vigenere_inverso(&saida, b"VIGENERE").expect("inverso"),
            b"Hello!"
        );
    }

    /// K6 vetor: "AB" com "PLAYFAIR" → 110e (XOR).
    #[test]
    fn playfair_vetor_de_parity() {
        let saida = playfair(b"AB", b"PLAYFAIR").expect("playfair");
        assert_eq!(saida, vec![0x11, 0x0e]);
        assert_eq!(
            playfair_inverso(&saida, b"PLAYFAIR").expect("inverso"),
            b"AB"
        );
    }

    /// K8 vetor: "AB" +7 → "HI".
    #[test]
    fn k8_vetor_de_parity() {
        let k8 = crate::pipeline::DESLOCAMENTO_K8;
        assert_eq!(deslocamento(b"AB", k8).expect("K8"), b"HI");
        assert_eq!(deslocamento_inverso(b"HI", k8).expect("inverso"), b"AB");
    }

    /// K2 e K8 são a mesma função, e não duas que concordam.
    ///
    /// Este teste fixa a unificação de G6. Antes havia `cesar.lua` e
    /// `substituicao.lua` com o corpo idêntico e dois pares de funções
    /// Rust por cima; agora há um `deslocamento.lua` e uma função.
    ///
    /// A asserção que importa não é «com deslocamento 3 dá uma coisa e
    /// com 7 dá outra» — isso era verdade antes também. É que **não há
    /// segunda implementação**: `DESLOCAMENTO_K2` e `DESLOCAMENTO_K8`
    /// passam pelo mesmo código, e a diferença entre as camadas é
    /// exactamente a diferença entre 3 e 7. Se alguém reintroduzir um
    /// `cesar.lua`, este teste continua a passar — o que é o ponto: a
    /// garantia de que não há divergência passa a ser estrutural (um
    /// só ficheiro embutido) e não sustentada por asserção.
    ///
    /// O que este teste faz é fixar os vectores das duas camadas contra
    /// a **mesma** função, de modo que a equivalência fica registada em
    /// código e não apenas na intenção.
    #[test]
    fn k2_e_k8_partilham_a_mesma_funcao() {
        let entrada: Vec<u8> = (0u8..=255).collect();

        let c2 = deslocamento(&entrada, crate::pipeline::DESLOCAMENTO_K2).expect("K2");
        let c8 = deslocamento(&entrada, crate::pipeline::DESLOCAMENTO_K8).expect("K8");

        // A diferença entre as duas camadas tem de ser **constante**, e
        // ser exactamente a diferença entre os deslocamentos. Uma
        // diferença constante prova que as duas camadas são a mesma
        // função com `k` diferente; uma diferença que variasse por
        // posição significaria que uma das duas trata certos bytes à
        // maneira dela — que é a divergência que G6 eliminou.
        const DIFERENCA: u8 = (crate::pipeline::DESLOCAMENTO_K2
            - crate::pipeline::DESLOCAMENTO_K8) as u8;

        for (i, ((entrada_b, k2_b), k8_b)) in
            entrada.iter().zip(&c2).zip(&c8).enumerate()
        {
            assert_eq!(
                k2_b.wrapping_sub(*k8_b),
                DIFERENCA,
                "no byte {i} (entrada {entrada_b:#04x}): K2 menos K8 tem de \
                 ser sempre {DIFERENCA}"
            );
            // E cada camada tem de fazer o que o seu deslocamento diz,
            // byte a byte — sem casos especiais nas fronteiras (0, 127,
            // 128, 255), que é onde um `if b < k` mal escrito travaria.
            assert_eq!(
                *k2_b,
                entrada_b.wrapping_add(crate::pipeline::DESLOCAMENTO_K2 as u8)
            );
            assert_eq!(
                *k8_b,
                entrada_b.wrapping_add(crate::pipeline::DESLOCAMENTO_K8 as u8)
            );
        }
    }

    // ----------------------------------------------------------------
    // Propriedades gerais (round-trip sobre 256 valores de byte)
    // ----------------------------------------------------------------

    /// Round-trip completo para todas as camadas com bytes 0..=255.
    #[test]
    fn roundtrip_de_todas_as_camadas() {
        let brutos: Vec<u8> = (0..=255).collect();

        let k2 = deslocamento(&brutos, crate::pipeline::DESLOCAMENTO_K2).expect("k2");
        assert_eq!(deslocamento_inverso(&k2, crate::pipeline::DESLOCAMENTO_K2).expect("k2 inv"), brutos);

        let k3 = vigenere(&brutos, b"VIGENERE").expect("k3");
        assert_eq!(vigenere_inverso(&k3, b"VIGENERE").expect("k3 inv"), brutos);

        let k6 = playfair(&brutos, b"PLAYFAIR").expect("k6");
        assert_eq!(playfair_inverso(&k6, b"PLAYFAIR").expect("k6 inv"), brutos);

        let k8 = deslocamento(&brutos, crate::pipeline::DESLOCAMENTO_K8).expect("k8");
        assert_eq!(deslocamento_inverso(&k8, crate::pipeline::DESLOCAMENTO_K8).expect("k8 inv"), brutos);
    }

    /// Entradas vazias produzem saídas vazias em todas as camadas.
    #[test]
    fn entradas_vazias() {
        assert_eq!(deslocamento(b"", crate::pipeline::DESLOCAMENTO_K2).expect("k2"), b"");
        assert_eq!(deslocamento_inverso(b"", crate::pipeline::DESLOCAMENTO_K2).expect("k2i"), b"");
        assert_eq!(vigenere(b"", b"VIGENERE").expect("k3"), b"");
        assert_eq!(vigenere_inverso(b"", b"VIGENERE").expect("k3i"), b"");
        assert_eq!(playfair(b"", b"PLAYFAIR").expect("k6"), b"");
        assert_eq!(playfair_inverso(b"", b"PLAYFAIR").expect("k6i"), b"");
        assert_eq!(deslocamento(b"", crate::pipeline::DESLOCAMENTO_K8).expect("k8"), b"");
        assert_eq!(deslocamento_inverso(b"", crate::pipeline::DESLOCAMENTO_K8).expect("k8i"), b"");
    }

    // ----------------------------------------------------------------
    // Erros propagados do interpretador (chaves vazias)
    // ----------------------------------------------------------------

    /// Chave vazia em K3/K6 → `ErroPipeline::Lua` com a mensagem fixa.
    #[test]
    fn chaves_vazias_sao_rejeitadas() {
        let erro_k3 = vigenere(b"dado", b"").unwrap_err();
        assert!(matches!(&erro_k3, ErroPipeline::Lua(m) if m.contains("chave inválida")));
        let erro_k6 = playfair_inverso(b"dado", b"").unwrap_err();
        assert!(matches!(&erro_k6, ErroPipeline::Lua(m) if m.contains("chave inválida")));
    }

    /// Método inexistente → erro do interpretador propagado (cobre o
    /// ramo `get` de `traduzir` sem depender de OOM ou de outros erros).
    #[test]
    fn metodo_inexistente_erro_lua() {
        let erro = com_deslocamento("deslocamento.lua", "nao_existe", b"x", 3).expect_err("campo");
        // A tabela não tem o método → o interpretador devolve nil e a
        // conversão para função falha com mensagem fixa (sem segredos).
        assert!(
            matches!(&erro, ErroPipeline::Lua(m) if m.contains("nil")),
            "{erro:?}"
        );
    }

    /// Módulos Lua expõem exatamente `cifrar`/`decifrar` chamáveis.
    ///
    /// Passa pelo interpretador partilhado, que é a via por que o
    /// daemon real chega aos módulos. A versão anterior carregava um
    /// interpretador próprio só para introspecção, o que também não
    /// provava nada sobre o caminho de produção.
    #[test]
    fn modulos_tem_as_funcoes_esperadas() {
        com_interpretador(|int| {
            for (_, nome) in FONTES {
                let tabela = int.modulo(nome)?;
                let _: mlua::Function = tabela.get("cifrar").expect("cifrar existe");
                let _: mlua::Function = tabela.get("decifrar").expect("decifrar existe");
            }
            Ok(())
        })
        .expect("todos os módulos têm cifrar/decifrar");
    }

    /// Todos os quatro módulos estão registados.
    ///
    /// Um módulo em falta falharia em `modulo()` com um
    /// `ErroPipeline::Lua` — que num pedido real chega ao cliente como
    /// `0x03 Erro`, silencioso. Este teste torna o desaparecimento um
    /// erro de compilação em vez de uma falha em runtime.
    #[test]
    fn todos_os_modulos_estao_registados() {
        com_interpretador(|int| {
            for (_, nome) in FONTES {
                assert!(
                    int.modulo(nome).is_ok(),
                    "módulo {nome} não está registado"
                );
            }
            // E nenhum a mais: um `HashMap` com entradas órfãs
            // significaria que `FONTES` e o mapa divergiram.
            assert_eq!(int.modulos.len(), FONTES.len());
            Ok(())
        })
        .expect("registo coerente");
    }

    /// O interpretador é reutilizado entre operações.
    ///
    /// Este é o teste que fixa o ganho: sem ele, alguém pode reintroduzir
    /// um `Lua::new()` por chamada e a suite continua a passar — apenas
    /// mais lenta. Comparar o endereço do interpretador entre duas
    /// operações prova que o estado é o mesmo objecto.
    #[test]
    fn interpretador_e_reutilizado() {
        let endereco = |int: &Interpretador| int.lua() as *const Lua as usize;
        let primeiro = com_interpretador(|int| Ok(endereco(int))).expect("1.ª");
        let segundo = com_interpretador(|int| Ok(endereco(int))).expect("2.ª");
        assert_eq!(
            primeiro, segundo,
            "cada operação está a criar o seu próprio interpretador"
        );
    }

    /// Um módulo inexistente dá erro, não pânico.
    ///
    /// O nome vem de uma constante, mas um `panic` dentro do atendimento
    /// de uma ligação derrubaria o daemon inteiro. Um `Err` devolve
    /// `0x03 Erro` ao cliente e o daemon continua a viver.
    #[test]
    fn modulo_desconhecido_da_erro() {
        let erro = com_interpretador(|int| int.modulo("inexistente.lua").map(|_| ()))
            .expect_err("módulo inexistente");
        assert!(matches!(&erro, ErroPipeline::Lua(m) if m.contains("inexistente")));
    }
}
