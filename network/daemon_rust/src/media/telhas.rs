// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Telhas 16×16, delta e RLE — tudo em **binário**, antes do base-64.
//!
//! [`docs/media.md`](../../../../docs/media.md) §6.2 e §6.3.
//!
//! ## A ordem, e porque é a única
//!
//! ```text
//! delta → RLE → base-64 → ENCODE
//! ```
//!
//! Cada passo é irredutível, e inverter qualquer um deles estraga o
//! seguinte:
//!
//! * **Telhas antes de linhas.** Com análise por linha, um scroll de 1
//!   pixel altera **todas** as linhas e o delta explode para 100%. Com
//!   telhas 16×16 altera **uma fila de telhas** — que é a razão pela qual
//!   H.264, VP9 e AV1 usam macroblocos. O ecrã desliza-se; a telha é a
//!   unidade que sobrevive ao deslocamento.
//!
//! * **RLE antes do base-64.** O base-64 **não tem alinhamento a
//!   bytes**: duas linhas de pixels iguais produzem caracteres
//!   diferentes. Um RLE depois do base-64 encontraria repetições que
//!   não existem, e o `HOLD` (§8.4) — que depende de a unidade não ter
//!   nada a enviar — nunca activaria.
//!
//! ## O que sai
//!
//! Três casos, e a escolha entre eles é o que dá o tamanho final:
//!
//! | Caso | Bytes | Quando |
//! | --- | --- | --- |
//! | inalterada | 2 (o índice) | a telha é igual à anterior |
//! | chapada | 2 + 1 | todos os pixels iguais |
//! | com variação | 2 + 1 + 256 | o resto |
//!
//! ## E a reconstrução
//!
//! [`Telhas::aplicar`] é o inverso exacto, e a propriedade que se testa
//! é `aplicar(diferenca(f1, f2), f1) == f2` para quadris arbitrários.
//! Sem isso, um receptor que dessincroniza descobre-se por ver cores
//! trocadas — a falha mais silenciosa que há.

/// O lado de uma telha, em pixels.
pub const LADO: usize = 16;

/// Os bytes de uma telha.
pub const BYTES_POR_TELHA: usize = LADO * LADO;

/// O número de bytes que um índice de telha ocupa no delta.
///
/// Dois, e não um: o número de telhas de um ecrã é `(W/16) × (H/16)`, e
/// um ecrã de 2560 de largura são 160 colunas — que não cabem em um
/// byte. Com dois, cabem 65536 telhas, e um índice de 16 bits é o
/// suficiente para qualquer ecrã que caiba no `u32` de largura.
pub const BYTES_INDICE: usize = 2;

/// A diferença entre dois quadros, por telha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    /// A telha não mudou. Só o índice viaja.
    Igual,
    /// Todos os 256 pixels iguais a `símbolo`.
    Chapada(u8),
    /// A telha inteira, quantizada.
    Variacao([u8; BYTES_POR_TELHA]),
}

impl Delta {
    /// O código interno, que vai no corpo do delta.
    pub fn codigo(&self) -> u8 {
        match self {
            Delta::Igual => 0,
            Delta::Chapada(_) => 1,
            Delta::Variacao(_) => 2,
        }
    }

    /// O **corpo** da diferença, sem o código.
    ///
    /// O código vai à parte porque o receptor precisa dele **antes** de
    /// saber quantos bytes ler a seguir — é o que dispensa um campo de
    /// comprimento no stream.
    pub fn corpo(&self) -> &[u8] {
        match self {
            Delta::Igual => &[],
            Delta::Chapada(s) => std::slice::from_ref(s),
            Delta::Variacao(p) => p.as_slice(),
        }
    }
}

/// O número máximo de telhas que cabem numa **unidade**.
///
/// ## O tecto que a medição encontrou
///
/// [`docs/media.md`](../../../../docs/media.md) §3.4 foi escrito **antes**
/// do encoder existir, e a tabela de taxas nunca viu este número. Ele
/// sai de três constantes que já estavam decididas:
///
/// ```text
/// MAX_PLAINTEXT  65 536 bytes   (o mesmo do texto)
/// base-64        × 1,3333
/// moldura        + 36 bytes
/// telha com variação  2 + 1 + 256 = 259 bytes
/// ```
///
/// ```text
/// (65 536 − 36) / 1,3333 / 259 = 189 telhas
/// ```
///
/// Uma **keyframe** é o delta contra um quadro zero, e por isso leva
/// **todas** as telhas. A 640×480 são 1 200 telhas: 414 400 bytes de
/// texto, contra 65 536. **Não cabe**, 6,3× acima.
///
/// Isto não é um problema de optimização: é uma incompatibilidade
/// entre duas decisões que já estavam tomadas. Sem keyframe não há
/// recuperação (§8.5), e sem recuperação o streaming é uma imagem que
/// se parte sozinha na primeira perda.
///
/// A geometria que cabe: 189 telhas de 16×16 dão 3024×3024 px no
/// quadrado, ou 640×448 se se preferir a largura de um ecrã — que é o
/// que [`Coalescer::novo`] exige.
pub const MAX_TELHAS_POR_UNIDADE: usize = 189;

/// Os bytes de texto de uma unidade com `telhas` todas com variação.
///
/// A função existe para que o tecto seja um **cálculo** e não um literal
/// repetido em três sítios — e um literal repetido divergiria do
/// `MAX_PLAINTEXT` no dia em que alguém o mexesse.
pub fn texto_de_uma_unidade(telhas: usize) -> usize {
    let binarios = telhas * (BYTES_INDICE + 1 + BYTES_POR_TELHA);
    // O base-64 cresce de 4 em 3, com `ceil` porque um resto de 1 byte
    // ocupa 2 caracteres.
    let b64 = binarios.div_ceil(3) * 4;
    b64 + super::moldura::TAM_PRE_AMBULO
}

/// A diferença entre dois quadros, telha a telha.
///
/// Não guarda o quadro anterior: devolve os bytes do delta, e quem
/// chama guarda o que precisa. Um `Vec` de 205 fatias de vídeo seria o
/// dobro da memória sem nenhum ganho.
#[derive(Debug, Clone)]
pub struct Telhas<'a> {
    /// O quadro de referência.
    pub referencia: &'a [u8],
    /// A largura do quadro, em pixels.
    pub largura: usize,
    /// A altura do quadro, em pixels.
    pub altura: usize,
}

impl<'a> Telhas<'a> {
    /// Prepara a comparação entre dois quadros da mesma geometria.
    ///
    /// ## Porque os tamanhos têm de bater
    ///
    /// Uma telha lida para fora do quadro leria memória de outro sítio e
    /// escreveria o que está lá num pixel do ecrã do receptor. E mesmo
    /// que o valor estivesse a zero, o quadro montado teria a largura
    /// errada e o resultado seria um deslize que se acumula a cada
    /// keyframe — a falha mais difícil de ver que existe num codec.
    pub fn nova(
        referencia: &'a [u8],
        largura: usize,
        altura: usize,
    ) -> Result<Self, super::erros::ErroMedia> {
        if largura % LADO != 0 || altura % LADO != 0 {
            return Err(super::erros::ErroMedia::GeometriaInvalida(
                "largura e altura têm de ser múltiplos de 16",
            ));
        }
        if referencia.len() != largura * altura {
            return Err(super::erros::ErroMedia::GeometriaInvalida(
                "o quadro não tem o tamanho da geometria",
            ));
        }
        Ok(Self {
            referencia,
            largura,
            altura,
        })
    }

    /// As dimensões da grelha de telhas.
    pub fn grelha(&self) -> (usize, usize) {
        (self.largura / LADO, self.altura / LADO)
    }

    /// O número de telhas.
    pub fn quantas(&self) -> usize {
        let (colunas, linhas) = self.grelha();
        colunas * linhas
    }

    /// A diferença de uma telha.
    ///
    /// `novo` é o quadro **inteiro**, não uma fatia da telha. A
    /// assinatura com uma fatia de 256 bytes é a tentação natural — e é
    /// impossível de implementar, porque uma telha não é 256 bytes
    /// contíguos (ver a nota no corpo).
    pub fn de_telha(&self, indice: usize, novo: &[u8]) -> Delta {
        debug_assert!(
            indice < self.quantas(),
            "índice de telha fora da grelha: {indice} de {}",
            self.quantas()
        );
        debug_assert_eq!(
            novo.len(),
            self.largura * self.altura,
            "o quadro novo não tem o tamanho da geometria"
        );

        let inicio = self.inicio_da_telha(indice);

        // ## A telha **não** são 256 bytes contíguos
        //
        // São 16 linhas de 16 bytes, com passo de `largura`. A primeira
        // versão comparava `&referencia[inicio..inicio+256]`, que lê 256
        // bytes **seguidos** — e com 640 de largura isso é a telha
        // inteira de uma vez só quando `largura == 16`.
        //
        // O efeito era subtel e terrível: uma telha comparava-se com a
        // faixa de pixels à sua volta, e o delta marcava como alteradas
        // todas as telhas de uma linha sempre que uma mudava. O
        // receptor reconstruía o mesmo quadro — a identidade passava —
        // mas viajava o dobro dos bytes, e uma mudança numa telha
        // contaminava as seis seguintes.
        //
        // Por isso a comparação é linha a linha, e é por isso que o
        // teste escolhe uma telha do **meio** de uma linha com
        // vizinhas: com a telha 0 o bug não aparece.
        let mut igual = true;
        let mut chapado = true;
        // `novo[inicio]` e **não** `novo[0]`: desde que `de_telha` passou
        // a receber o quadro inteiro, `novo[0]` é o pixel do canto
        // superior esquerdo do ecrã — e não o primeiro pixel da telha.
        // Lê-lo dava um símbolo errado em toda a telha menos na primeira,
        // e nenhuma telha era reconhecida como chapada.
        let primeiro = novo[inicio];
        let mut payload = [0u8; BYTES_POR_TELHA];

        for linha in 0..LADO {
            let off = inicio + linha * self.largura;
            let antes = &self.referencia[off..off + LADO];
            let agora = &novo[off..off + LADO];

            if antes != agora {
                igual = false;
            }
            let destino = &mut payload[linha * LADO..linha * LADO + LADO];
            destino.copy_from_slice(agora);
            if agora.iter().any(|&p| p != primeiro) {
                chapado = false;
            }
        }

        if igual {
            return Delta::Igual;
        }
        if chapado {
            return Delta::Chapada(primeiro);
        }
        Delta::Variacao(payload)
    }

    /// O primeiro byte de uma telha, dentro do quadro.
    ///
    /// ## Porque isto não é `índice * 256`
    ///
    /// A primeira versão escrevia `(lin * LADO + col) * LADO`, que trata
    /// o quadro como se as linhas tivessem **16** bytes de passo. Têm
    /// `largura`. Com 640 de largura, a diferença entre as duas formulae
    /// é de 39 KB por telha a partir da segunda linha — e o resultado
    /// era uma imagem que se dessincronizava sem nenhum erro, que é a
    /// falha mais cara que um codec pode ter.
    ///
    /// O teste de identidade apanhou-o: reconstruía um quadro de lixo.
    fn inicio_da_telha(&self, indice: usize) -> usize {
        let (colunas, _) = self.grelha();
        let (col, lin) = (indice % colunas, indice / colunas);
        lin * LADO * self.largura + col * LADO
    }

    /// A diferença do quadro inteiro, em bytes.
    ///
    /// ```text
    /// [índice:2][código:1][corpo:0|1|256]   por telha que mudou
    /// ```
    ///
    /// Uma telha inalterada **não** escreve nada — nem sequer o índice.
    /// É essa a propriedade que faz o scroll de 1 pixel custar uma fila
    /// de telhas em vez do quadro inteiro, e é o que §6.2 chama «a
    /// unidade que sobrevive ao deslocamento».
    ///
    /// O índice vai **sempre** primeiro, mesmo quando a telha mudou
    /// pouco: sem ele o receptor não saberia qual telha escrever, e
    /// adivinhar por posição só funcionaria se nunca se perdesse uma.
    pub fn diferenca(&self, novo: &[u8]) -> Vec<u8> {
        debug_assert_eq!(
            novo.len(),
            self.largura * self.altura,
            "o quadro novo não tem o tamanho da geometria"
        );
        let mut saida = Vec::with_capacity(self.quantas() * 8);
        let (colunas, _) = self.grelha();

        for linha in 0..self.altura / LADO {
            for col in 0..colunas {
                let indice = linha * colunas + col;
                let d = self.de_telha(indice, novo);
                if matches!(d, Delta::Igual) {
                    continue;
                }
                saida.extend_from_slice(&(indice as u16).to_be_bytes());
                saida.push(d.codigo());
                saida.extend_from_slice(d.corpo());
            }
        }

        saida
    }
}

/// Aplica um delta a um quadro de referência, in-place.
///
/// ## E o `&mut` do receptor
///
/// A assinatura é `&mut [u8]` e não `-> Vec<u8>` porque o receptor
/// mantém **um** quadro e reescreve as telhas que chegaram. Alocar um
/// novo quadro por unidade seria 360 KB por frame a 30 fps — 10 MB/s de
/// lixo numa máquina que também está a cifrar.
///
/// E a mesma função serve para uma keyframe, em que o delta traz todas
/// as telhas: é por isso que não há um caminho separado para
///keyframes, e sim `diferenca` contra um quadro vazio.
pub fn aplicar(
    delta: &[u8],
    referencia: &mut [u8],
    largura: usize,
) -> Result<(), super::erros::ErroMedia> {
    let colunas = largura / LADO;
    let mut i = 0usize;

    while i + BYTES_INDICE + 1 <= delta.len() {
        let indice = u16::from_be_bytes([delta[i], delta[i + 1]]) as usize;
        i += BYTES_INDICE;

        let codigo = delta[i];
        i += 1;

        let (col, lin) = (indice % colunas, indice / colunas);
        // O mesmo passo de `largura` que em `Telhas::inicio_da_telha`. Se
        // um dos dois mudar e o outro não, o emissor e o receptor leem
        // telhas diferentes — e a imagem fica corrida sem erro.
        let inicio = lin * LADO * largura + col * LADO;

        // E `get_mut` **por linha**, não sobre 256 bytes: é o que
        // impede escrever fora da grelha. Um índice de telha maior do
        // que a grelha daria uma linha inválida e a escrita seria
        // recusada — em vez de escrever memória de outro sítio.
        let fora = || super::erros::ErroMedia::PedacoForaDeIntervalo(indice as u32);

        match codigo {
            1 => {
                let simbolo = *delta.get(i).ok_or(super::erros::ErroMedia::DeltaTruncado)?;
                i += 1;
                for linha in 0..LADO {
                    let off = inicio + linha * largura;
                    referencia
                        .get_mut(off..off + LADO)
                        .ok_or_else(fora)?
                        .fill(simbolo);
                }
            }
            2 => {
                let corpo = delta
                    .get(i..i + BYTES_POR_TELHA)
                    .ok_or(super::erros::ErroMedia::DeltaTruncado)?;
                for linha in 0..LADO {
                    let off = inicio + linha * largura;
                    let destino = referencia.get_mut(off..off + LADO).ok_or_else(fora)?;
                    destino.copy_from_slice(&corpo[linha * LADO..linha * LADO + LADO]);
                }
                i += BYTES_POR_TELHA;
            }
            outro => {
                return Err(super::erros::ErroMedia::DeltaCodigoDesconhecido(outro));
            }
        }
    }

    if i != delta.len() {
        return Err(super::erros::ErroMedia::DeltaTruncado);
    }
    Ok(())
}

#[cfg(test)]
mod testes;
