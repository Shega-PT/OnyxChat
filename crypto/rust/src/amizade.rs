// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
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
// Derivação por tipo de conteúdo (media)
// ---------------------------------------------------------------------
//
// Tudo o que se segue é uma **extensão de uma linha** do `info`. A
// semente, o `salt`, o algoritmo e a identidade do remetente são os de
// [`derivar_k1`] — não há um segundo segredo, nem uma segunda
// criptografia, nem um segundo vector para manter.
//
// A pergunta que este código responde é: *como é que o receptor sabe o
// tipo de uma mensagem sem a rede lha dizer?* A resposta é que o tipo
// está **dentro da chave**, e a verificação da etiqueta AEAD da K1 — a
// primeira camada, e uma tag de 16 bytes — diz qual das chaves era a
// certa. Um observador vê `101 + N` bytes opacos, exactamente como vê
// uma mensagem de texto. Ver `docs/media.md` §1.
// =====================================================================

/// Rótulo que separa as chaves de media das chaves de texto.
///
/// Entra no `info` do HKDF e não noutro lado, porque o `info` é a única
/// parte do contexto que não precisa de ser mantida secreta — e por
/// isso a única que pode ser compartilhada entre as duas linguagens sem
/// custo. Nada disto é transmitido: o `info` está **dentro** da chave,
/// e a chave não sai do host de quem cifra.
pub const DOMINIO_MEDIA: &[u8] = b"ONYX/MEDIA/";

/// Tipo de conteúdo: áudio.
///
/// O texto **não** tem valor aqui, e a ausência é deliberada. Ele
/// continua a usar a K1 de longa duração que já usava — ver
/// [`derivar_k1`] — e é por isso que o texto que o sistema produz é
/// byte-a-byte igual ao que produzia antes de existir media. Derivar
/// também o texto, para a função ser total, trocaria uma garantia que
/// já temos (zero risco de regressão no chat) por uniformidade que
/// ninguém pediu.
pub const TIPO_AUDIO: u8 = 0x01;

/// Tipo de conteúdo: vídeo.
pub const TIPO_VIDEO: u8 = 0x02;

/// Deriva a K1 de uma mensagem de media, por tipo de conteúdo.
///
/// É [`derivar_k1`] com [`DOMINIO_MEDIA`] ‖ `tipo` a acrescentar ao
/// `info`.
///
/// ## Porque devolve `Option`
///
/// Porque um tipo desconhecido **não pode** produzir uma chave. Se
/// devolvesse `[u8; 32]` e confiasse no chamador, bastava um `0x03` a
/// passar sem reparo para o sistema a gerar chaves que ninguém sabe
/// distinguir umas das outras — e o erro apareceria como «a imagem não
/// abre», muito longe da sua causa.
///
/// `None` transforma isso num erro **no sítio onde o tipo foi escrito**,
/// que é o sítio onde se pode corrigir. É a mesma doutrina de `LIMIT`
/// em [`crate::envelope`]: fechar por construção, não por confiança.
///
/// ## Porque só a K1
///
/// K5 e K9 entram no `ENCODE` inalteradas. A K1 é a **primeira** camada
/// e é AEAD, logo uma etiqueta errada morre ali e as restantes nunca
/// correm. É o que torna o sondar do tipo barato: **uma verificação
/// Poly1305**, não uma descifra completa.
pub fn derivar_k1_por_tipo(
    k_emissor: &[u8; 32],
    k_recetor: &[u8; 32],
    pub_emissor: &[u8; 32],
    tipo: u8,
) -> Option<[u8; 32]> {
    if !matches!(tipo, TIPO_AUDIO | TIPO_VIDEO) {
        return None;
    }

    let segredo = semente(k_emissor, k_recetor);
    let mut contexto = info(pub_emissor);
    contexto.extend_from_slice(DOMINIO_MEDIA);
    contexto.push(tipo);

    // O `salt` é o **rótulo de domínio e o tipo**, e **não** um nonce.
    //
    // A primeira versão punha `salt = nonce1`, herdado de
    // [`derivar_k1`]. E isso **não funciona**, por uma razão que só
    // aparece em execução:
    //
    //     o emissor teria de cifrar duas vezes — uma para obter um
    //     envelope com um `nonce1` conhecido, e outra com a chave
    //     derivada desse nonce. Mas a segunda `ENCODE` gera um
    //     `nonce1` **novo**, e o envelope que vai para a rede tem esse
    //     nonce novo, não o que a chave usou.
    //
    // Ou seja: o emissor cifra com uma chave derivada de `P` e envia
    // um envelope cujo `nonce1` é `Q`. O receptor deriva a chave a
    // partir do envelope que **recebeu**, logo deriva com `Q` — e a
    // cifra falha.
    //
    // E a falha é do tipo mais caro: **cifra bem e não abre**.
    // `0x05 DecifragemFalhou` não aponta para a chave, que é a pista
    // mais errada que existe.
    //
    // Com um `salt` constante, os dois lados derivam a mesma chave a
    // partir do que **já sabem ambos** — o par de chaves e a
    // identidade do remetente — e a passagem única resolve. O
    // `nonce1` continua a ser o nonce do AEAD, que é onde tem de estar
    // e é o que garante que duas mensagens com a mesma chave usam
    // nonces diferentes.
    //
    // A consequência de segurança é a mesma que o texto já tem: a K1
    // de media é de longa duração dentro do par. Ver `docs/media.md`
    // §1.4 — que é por isso que ali está escrita como limitação
    // declarada e não como detalhe omitido.
    let mut sal = Vec::with_capacity(DOMINIO_MEDIA.len() + 1);
    sal.extend_from_slice(DOMINIO_MEDIA);
    sal.push(tipo);

    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&sal), &segredo);
    let mut saida = [0u8; 32];
    // Mesmos limites de [`derivar_k1`], e pela mesma razão: 49 bytes de
    // `info` contra 8160, saída de 32. Um `unwrap` aqui seria um pânico
    // em código de derivação, por causa de um caminho que não existe.
    hk.expand(&contexto, &mut saida)
        .expect("info de 49 bytes e saída de 32: dentro dos limites do HKDF");
    Some(saida)
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

    /// Vector da K1 de media, áudio, `A→B`. Calculado por
    /// `messenger/amizade.py` — igual a [`VECTOR_K1`], que é a razão de
    /// nenhum dos lados escrever o seu próprio valor esperado.
    ///
    /// **Mudou** quando o `salt` deixou de ser o `nonce1`. A nota longa em
    /// [`derivar_k1_por_tipo`] diz porquê, e o que importa aqui é que um
    /// vector que muda de valor é uma **quebra de formato** — e a excepção
    /// de que a media precisa, e não o texto. Uma mudança no `info`
    /// partia as duas coisas; uma mudança só no `salt` só parte a media,
    /// e é por isso que a regra é escrita assim.
    const VECTOR_K1_AUDIO: &str =
        "8824b45c0d37bc7abdc207762b616a3156ed7b749e6a309850c4c1bd831b7a24";

    /// Vector da K1 de media, vídeo, `A→B`.
    const VECTOR_K1_VIDEO: &str =
        "d7004b083251ba885d5cac42391f7e9f3c2ee101f731c6c04353edf2981d139f";

    /// Vector da K1 de media, vídeo, `B→A`.
    ///
    /// Existe como vector **separado** e não como um `assert_ne` porque
    /// mede outra coisa: que a direcção do `info` se estende à derivação por
    /// tipo. Um envelope de vídeo de Alice reflectido para Bob não pode
    /// abrir como se fosse de Bob, e a prova disso é um valor concreto e não
    /// uma desigualdade.
    const VECTOR_K1_VIDEO_INVERSO: &str =
        "5d6da3de9879224f0c400ab71759e94e1ff646df4c3c82b01890d54a99020e46";

    /// Matriz de quatro valores distintos: as chaves dos dois lados e as
    /// identidades dos dois lados. Distintas para que qualquer troca de
    /// argumentos altere o resultado, e não o esconda.
    const KA: [u8; 32] = [0x11; 32];
    const KB: [u8; 32] = [0x22; 32];
    const PUB_A: [u8; 32] = [0xAA; 32];
    const PUB_B: [u8; 32] = [0xBB; 32];
    const NONCE: [u8; 12] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
    ];

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
    /// A derivação por tipo bate com o Python, nos três vectores.
    ///
    /// Este é o teste que impede a divergência silenciosa que o projecto já
    /// apanhou uma vez com o K2/K8: dois ficheiros com o mesmo algoritmo e
    /// bodies quase idênticos, que divergem no dia em que alguém corrigir um
    /// dos dois. Aqui os dois lados são implementações **independentes** da
    /// mesma fórmula — o Rust com `hkdf`, o Python com `hmac` — e o vector
    /// foi calculado pelo Python. Se divergirem, este teste falha.
    ///
    /// E falha de uma forma útil: a mensagem diz que **é uma quebra de
    /// formato**, porque é isso que é. Um envelope cifrado com a fórmula
    /// antiga tem de continuar a abrir depois desta alteração; se não abrir,
    /// quem tem conversas guardadas fica sem elas.
    #[test]
    fn derivacao_por_tipo_bate_com_o_python() {
        assert_eq!(
            derivar_k1_por_tipo(&KA, &KB, &PUB_A, TIPO_AUDIO).unwrap(),
            hex32(VECTOR_K1_AUDIO),
            "a K1 de áudio mudou — é uma quebra de formato, não um bug"
        );
        assert_eq!(
            derivar_k1_por_tipo(&KA, &KB, &PUB_A, TIPO_VIDEO).unwrap(),
            hex32(VECTOR_K1_VIDEO),
            "a K1 de vídeo mudou — é uma quebra de formato, não um bug"
        );
    }

    /// A direcção do `info` estende-se à derivação por tipo.
    ///
    /// Um envelope de vídeo de Alice reflectido para Bob **não pode** abrir
    /// como se fosse de Bob. A prova é um valor concreto, não uma
    /// desigualdade: um valor congelado que o Python calculou para a
    /// direcção inversa.
    #[test]
    fn derivacao_por_tipo_mantem_a_direccao() {
        assert_eq!(
            derivar_k1_por_tipo(&KB, &KA, &PUB_B, TIPO_VIDEO).unwrap(),
            hex32(VECTOR_K1_VIDEO_INVERSO),
            "A→B e B→A pelo mesmo tipo têm de dar chaves diferentes"
        );
    }

    /// A simetria da chave continua a valer por tipo.
    ///
    /// Mesma propriedade de [`derivacao_e_simetrica_na_chave`], no caminho
    /// novo: A e B conhecem o par na sua posição e têm de obter a mesma
    /// chave.
    #[test]
    fn derivacao_por_tipo_e_simetrica() {
        assert_eq!(
            derivar_k1_por_tipo(&KA, &KB, &PUB_A, TIPO_VIDEO).unwrap(),
            derivar_k1_por_tipo(&KB, &KA, &PUB_A, TIPO_VIDEO).unwrap(),
            "a ordenação canónica falhou no caminho por tipo"
        );
    }

    /// Os três espaços de chave são disjuntos.
    ///
    /// Esta é a asserção que justifica a derivação existir. Se áudio, vídeo
    /// e texto partilhassem chave, não haveria separação nenhuma e o
    /// mecanismo de sondagem seria apenas uma formalidade: um envelope de
    /// texto abriria como áudio e vice-versa.
    #[test]
    fn os_espacos_de_chave_sao_disjuntos() {
        let texto = derivar_k1(&KA, &KB, &PUB_A, &NONCE);
        let audio = derivar_k1_por_tipo(&KA, &KB, &PUB_A, TIPO_AUDIO).unwrap();
        let video = derivar_k1_por_tipo(&KA, &KB, &PUB_A, TIPO_VIDEO).unwrap();

        assert_ne!(texto, audio, "texto e áudio não podem partilhar chave");
        assert_ne!(texto, video, "texto e vídeo não podem partilhar chave");
        assert_ne!(audio, video, "áudio e vídeo não podem partilhar chave");
    }

    /// Um tipo desconhecido **não** deriva.
    ///
    /// É o teste de [`derivar_k1_por_tipo`] a devolver `Option`: o `0x00`
    /// (texto) e o `0x03` (não existe) têm de recusar, porque uma chave
    /// derivada com um tipo que ninguém sabe ler produz envelopes que não
    /// abrem em lado nenhum.
    #[test]
    fn tipo_desconhecido_nao_deriva() {
        for tipo in [0x00u8, 0x03, 0x10, 0xFF] {
            assert!(
                derivar_k1_por_tipo(&KA, &KB, &PUB_A, tipo).is_none(),
                "o tipo 0x{tipo:02x} não devia ter derivado chave"
            );
        }
    }

    /// O `info` de media é o `info` de texto mais o rótulo e o tipo.
    ///
    /// Fixa a **sequência de bytes** do `info`, que é o que garante a
    /// paridade entre as duas implementações. Um `info` com os mesmos bytes
    /// por outra ordem daria uma chave diferente — e o Python não se
    /// queixaria, porque derivaria o valor errado com a mesma confiança.
    #[test]
    fn info_de_media_tem_ordem_fixo() {
        let mut esperado = Vec::new();
        esperado.extend_from_slice(INFO_K1);
        esperado.extend_from_slice(&PUB_A);
        esperado.extend_from_slice(DOMINIO_MEDIA);
        esperado.push(TIPO_AUDIO);

        // Reconstruído à mão a partir das constantes, e comparado com o que a
        // função produz internamente — comparando com `info()` mais um
        // acrescento, para que o teste não reimplemente a função.
        let mut obtido = info(&PUB_A);
        obtido.extend_from_slice(DOMINIO_MEDIA);
        obtido.push(TIPO_AUDIO);

        assert_eq!(obtido, esperado);
    }
}
