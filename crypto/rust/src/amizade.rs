// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// amizade.rs — derivação da chave de mensagem K1
// ---------------------------------------------------------------------
// A chave que o pipeline chama «K1» é, hoje, o segredo de longo prazo
// da amizade: a mesma em todas as mensagens, para sempre. Este módulo
// deriva dela uma chave **por mensagem**, e é o que o cliente passa ao
// daemon no campo `k1` de `ENCODE`/`DECODE`.
//
// ## O que isto compra
//
// A chave de longa duração nunca atravessa a IPC nem entra na memória
// do daemon. O daemon vê uma chave por mensagem; um dump de memória, um
// core, ou `/proc/<pid>/mem` do daemon dão chaves de mensagens
// isoladas e não a chave da amizade. E como a derivação inclui a
// identidade do remetente, a chave de A→B não é a chave de B→A.
//
// ## O que isto NÃO compra — e é uma limitação, não um defeito
//
// **Não há forward secrecy.** A derivação é determinística a partir de
// duas chaves de longa duração e de um nonce público. Se `k5_proprio`
// ou `k5_par` for comprometida *depois* da conversa, todas as mensagens
// — passadas e futuras — abrem-se, porque a semente é reconstruível.
//
// Só há forward secrecy com um segredo efémero por sessão que nunca
// seja transmitido (tipo Noise IK com ephemeral). Isso é uma mudança
// de protocolo, não uma optimização: obriga a um handshake diferente.
// Está declarado em `docs/threat_model.md` §I e não é fingido aqui.
//
// ## Porque `SHA-256` antes de HKDF
//
// Porque a semente tem de ser **independente da ordem**. Os dois lados
// conhecem o mesmo par de chaves, mas cada um delas na sua posição
// («minha» e «a do par»); sem ordenação canónica, A calcularia um
// valor e B outro, e nenhuma mensagem abriria. Um SHA-256 com
// ordenação lexicográfica transforma o par num valor único, e o HKDF
// fica livre para fazer o que tem de fazer: separar contextos.
//
// ## Porque o `info` leva a identidade do remetente
//
// Sem ela, A→B e B→A com o mesmo nonce dariam a mesma chave, e um
// envelope de Alice reflectido seria um envelope válido de Bob. Com
// ela, a chave fica presa a quem enviou — o que é o mesmo motivo pelo
// qual uma assinatura vai no corpo e não numa etiqueta externa.
// =====================================================================


// ---------------------------------------------------------------------
// Constantes de domínio
// ---------------------------------------------------------------------

/// Rótulo de derivação do segredo de amizade.
///
/// O prefixo `ONXY/` e a barra são deliberados: nenhum outro rótulo do
/// projecto começa assim, e um rótulo tem de ser único **dentro de
/// todos os usos de SHA-256** que venham a existir, não só dentro deste
/// ficheiro. O `v1` é a versão do esquema — mudar o que a fórmula faz
/// é mudar o `v1`, e os dois lados têm de concordar nele.
pub const DOMINIO_AMIZADE: &[u8] = b"ONYX/AMIZADE/v1";

/// Rótulo do HKDF para a K1 derivada.
///
/// Rótulo diferente do de [`DOMINIO_AMIZADE`] porque é um papel
/// diferente: o primeiro separa a semente da amizade de qualquer outro
/// SHA-256 do projecto; este separa a K1 derivada de qualquer outro uso
/// daquela semente.
pub const INFO_K1: &[u8] = b"ONYX/K1/v1";

// ---------------------------------------------------------------------
// Derivação
// ---------------------------------------------------------------------

/// Devolve as duas chaves em ordem lexicográfica.
///
/// Existe como função — e não inline na [`semente`] — para que o teste
/// possa afirmar o mesmo ordenamento sem reimplementar a comparação. Um
/// teste que reimplementa o que verifica está a verificar a sua própria
/// cópia; este usa a de produção.
fn ordenar<'a>(a: &'a [u8; 32], b: &'a [u8; 32]) -> (&'a [u8; 32], &'a [u8; 32]) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// O segredo de longo prazo da amizade: `SHA-256(domínio ‖ k₁ ‖ k₂)`.
///
/// As duas chaves entram em **ordem lexicográfica**, não na ordem em que
/// o chamador as passou. É o que torna o valor único para o par: Alice
/// passa `(&k5_alice, &k5_bob)` e Bob passa `(&k5_bob, &k5_alice)`, e
/// ambos têm de obter a mesma semente.
///
/// A comparação é feita sobre os arrays completos de 32 bytes, com a
/// comparação de arrays do Rust (`Ord` para `[u8; N]` é
/// lexicográfica). Não é uma escolha arbitrária: qualquer ordenação
/// total serve, desde que as duas implementações usem a mesma, e a
/// lexicográfica é a única que não precisa de um código extra no
/// Python.
pub fn semente(k_a: &[u8; 32], k_b: &[u8; 32]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let (primeira, segunda) = ordenar(k_a, k_b);

    let mut hash = Sha256::new();
    hash.update(DOMINIO_AMIZADE);
    hash.update(primeira);
    hash.update(segunda);
    hash.finalize().into()
}

/// O `info` do HKDF: rótulo seguido da identidade do remetente.
///
/// Fica em [`info`] e não inline em [`derivar_k1`] porque os testes
/// precisam de fixar a sequência de bytes, e um valor inline só é
/// verificável a partir de dentro.
pub fn info(pub_emissor: &[u8; 32]) -> Vec<u8> {
    let mut saida = Vec::with_capacity(INFO_K1.len() + 32);
    saida.extend_from_slice(INFO_K1);
    saida.extend_from_slice(pub_emissor);
    saida
}

/// Deriva a K1 de uma mensagem: `HKDF-SHA256(semente, nonce1, info)`.
/// 
/// `salt` é o `nonce1` do envelope — o valor que o AEAD vai usar, e que
/// por isso está preso a esta mensagem. Passá-lo como `salt` do HKDF
/// (e não como parte do `info`) é o que a RFC 5869 recomenda: o `salt`
/// é um valor não secreto que o derivador pode usar para(randomizar) a
/// extracção, e o `nonce1` é exactamente isso.
///
/// Não há forward secrecy e não se finge que há: a operação é
/// determinística em função de duas chaves de longa duração e de um
/// nonce público. Ver a nota no cabeçalho deste ficheiro e
/// `docs/threat_model.md` §I.
pub fn derivar_k1(
    k_emissor: &[u8; 32],
    k_recetor: &[u8; 32],
    pub_emissor: &[u8; 32],
    nonce1: &[u8; 12],
) -> [u8; 32] {
    let segredo = semente(k_emissor, k_recetor);
    let contexto = info(pub_emissor);

    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(nonce1), &segredo);
    let mut saida = [0u8; 32];
    // `expand` só falha se o `info` for maior que 255×hash_len — 48
    // bytes contra 8160, e o comprimento de saída está em 32, que é
    // menos que o limite de 255. O `.expect` é, portanto, a sério
    // inalcançável: um `unwrap` aqui seria um pânico em código de
    // derivação, e um `Err` que se propaga seria um caminho que nunca
    // acontece tratado como se happenesse.
    hk.expand(&contexto, &mut saida)
        .expect("info de 48 bytes e saída de 32: dentro dos limites do HKDF");
    saida
}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
use super::*;

/// Converte 64 caracteres hex num array de 32 bytes.
fn hex32(texto: &str) -> [u8; 32] {
    assert_eq!(texto.len(), 64, "esperava 64 caracteres hex");
    let mut saida = [0u8; 32];
    for (i, byte) in saida.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&texto[i * 2..i * 2 + 2], 16)
            .unwrap_or_else(|_| panic!("hex inválido no vector"));
    }
    saida
}

/// O vector conhecido da semente, congelado como constante nomeada.
///
/// Nomeada e não embutida no `assert_eq!` por uma razão prática: o guard
/// `derivacao_da_amizade_bate_entre_rust_e_python` lê este valor por
/// nome para o comparar com o Python. Um literal solto dentro de uma
/// asserção obriga o guard a procurar pelo valor — e procurar pelo
/// valor encontra primeiro a cópia na docstring. Nomeada, não há
/// ambiguidade.
const VECTOR_SEMENTE: &str = "27e15a94059c937034b10823b479f30ef84c732075a95588541e443575d443bc";

/// O vector conhecido da K1 derivada, congelado como constante nomeada.
///
/// Mesmo motivo que [`VECTOR_SEMENTE`], e calculado pelo Python (ver o
/// docstring de `vector_conhecido_da_derivacao`).
const VECTOR_K1: &str = "f01f37511c8daa9cb81be7faf296f43b6e78271516cdba0403d3108d7218a664";

/// Matriz de quatro valores distintos: as chaves dos dois lados e as
/// identidades dos dois lados. Distintas para que qualquer troca de
/// argumentos altere o resultado, e não o esconda.
const KA: [u8; 32] = [0x11; 32];
const KB: [u8; 32] = [0x22; 32];
const PUB_A: [u8; 32] = [0xAA; 32];
const PUB_B: [u8; 32] = [0xBB; 32];
const NONCE: [u8; 12] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C];

/// **A simetria que faz a coisa funcionar.** A e B conhecem o mesmo
/// par de chaves, cada um na sua posição. A derivação tem de dar o
/// mesmo valor para `(&KA, &KB)` e para `(&KB, &KA)`.
///
/// Se esta asserção falhar, nada mais importa: as mensagens não
/// abrem. É a razão de existir da ordenação canónica, e é o primeiro
/// teste porque é o que falha de forma mais silenciosa — o código
/// compila, os vectores de uma camada passam, e a falha só aparece
/// como mensagens indecifráveis em produção.
#[test]
fn derivacao_e_simetrica_na_chave() {
    let de_a = derivar_k1(&KA, &KB, &PUB_A, &NONCE);
    let de_b = derivar_k1(&KB, &KA, &PUB_A, &NONCE);
    assert_eq!(
        de_a, de_b,
        "A e B têm de obter a mesma chave — a ordenação canónica falhou"
    );
}

/// A simetria acima é **apenas** na ordem das chaves. A identidade do
/// remetente entra pelo `info` do HKDF, e isso tem de tornar a
/// derivação direccional: a chave de A→B não é a chave de B→A.
///
/// Sem esta propriedade, um envelope de Alice reflectido para Bob com o
/// mesmo nonce abriria como se fosse de Bob. Com ela, a chave está
/// presa ao emissor.
#[test]
fn derivacao_e_direccional() {
    let a_para_b = derivar_k1(&KA, &KB, &PUB_A, &NONCE);
    let b_para_a = derivar_k1(&KB, &KA, &PUB_B, &NONCE);
    assert_ne!(
        a_para_b, b_para_a,
        "A→B e B→A não podem partilhar chave: é a barreira contra reflexão"
    );
}

/// O nonce é o que separa as mensagens. Duas mensagens com o mesmo
/// par de chaves e nonces diferentes têm de ter chaves diferentes.
#[test]
fn nonce_diferente_da_chave_diferente() {
    let mut outro = NONCE;
    outro[0] ^= 0x01;

    let a = derivar_k1(&KA, &KB, &PUB_A, &NONCE);
    let b = derivar_k1(&KA, &KB, &PUB_A, &outro);
    assert_ne!(a, b, "o nonce não está a entrar na derivação");
}

/// Trocar uma das chaves muda a chave derivada.
#[test]
fn chave_diferente_da_chave_diferente() {
    let mut outra = KA;
    outra[31] ^= 0x01;

    assert_ne!(
        derivar_k1(&KA, &KB, &PUB_A, &NONCE),
        derivar_k1(&outra, &KB, &PUB_A, &NONCE),
        "uma chave diferente tem de dar uma K1 diferente"
    );
    assert_ne!(
        derivar_k1(&KA, &KB, &PUB_A, &NONCE),
        derivar_k1(&KA, &outra, &PUB_A, &NONCE),
        "a ordem canónica não pode esconder uma troca de chave"
    );
}

/// A saída não pode ser nenhuma das entradas, nem zero.
///
/// Um derivador que devolva a entrada funciona nos testes com vectores
/// conhecidos e é umaolla completa em produção. A verificação é barata
/// e apanha a classe de erro inteira.
#[test]
fn saida_nao_e_uma_das_entradas() {
    let derivada = derivar_k1(&KA, &KB, &PUB_A, &NONCE);

    assert_ne!(derivada, KA, "a K1 derivada é a chave de A");
    assert_ne!(derivada, KB, "a K1 derivada é a chave de B");
    assert_ne!(derivada, PUB_A, "a K1 derivada é a identidade do remetente");
    assert_ne!(derivada, [0u8; 32], "a K1 derivada são 32 bytes de zero");
    assert_ne!(
        derivada[..28],
        [0u8; 28],
        "os primeiros 28 bytes estão a zeros — nem uma chaveAES normal \
         começa assim, quanto mais uma derivada"
    );
}

/// A semente é o SHA-256 do domínio com as chaves **ordenadas**.
///
/// Este teste existe para fixar a ordem de byte exata, que é o que
/// torna a derivação interoperável entre o Rust e o Python. Se a
/// sequência mudar, os dois lados param de falar ao mesmo tempo — e o
/// teste tem de falhar em vez de deixar isso para o campo.
#[test]
fn semente_e_o_sha256_do_dominio_com_as_chaves_ordenadas() {
    use sha2::{Digest, Sha256};

    let (primeira, segunda) = ordenar(&KA, &KB);

    let mut esperado = Sha256::new();
    esperado.update(DOMINIO_AMIZADE);
    esperado.update(primeira);
    esperado.update(segunda);
    let want: [u8; 32] = esperado.finalize().into();

    assert_eq!(semente(&KA, &KB), want);
    assert_eq!(semente(&KB, &KA), want, "a semente é independente da ordem");

    // E o valor congelado, que é o que o Python partilha.
    assert_eq!(
        semente(&KA, &KB),
        hex32(VECTOR_SEMENTE),
        "a semente mudou — é uma quebra de formato, não um bug"
    );
}

/// O vector conhecido da derivação completa.
///
/// É o que garante que o cliente em Python e o daemon em Rust derivam
/// a mesma chave. Um teste de paridade que recalcula a `expected` com a
/// mesma fórmula não prova nada entre linguagens; um vector congelado
/// prova. Se este valor mudar, é uma quebra de protocolo e o ficheiro
/// de vectores tem de ser regenerado com o mesmo passo.
#[test]
fn vector_conhecido_da_derivacao() {
    let derivada = derivar_k1(&KA, &KB, &PUB_A, &NONCE);
    assert_eq!(
        derivada,
        hex32(VECTOR_K1),
        "a derivação mudou — é uma quebra de formato, não um bug"
    );
}

/// O `info` do HKDF leva o rótulo e a identidade do remetente.
///
/// Fixa a sequência de byte, pelo mesmo motivo do vector: é o que
/// garante a paridade entre implementações.
#[test]
fn info_do_hkdf_tem_ordem_fixo() {
    let mut esperado = Vec::new();
    esperado.extend_from_slice(INFO_K1);
    esperado.extend_from_slice(&PUB_A);

    assert_eq!(info(&PUB_A), esperado);
}

/// Trocar a identidade do remetente muda o `info` e, portanto, a chave.
#[test]
fn info_reacciona_a_identidade() {
    assert_ne!(info(&PUB_A), info(&PUB_B));
}
}
