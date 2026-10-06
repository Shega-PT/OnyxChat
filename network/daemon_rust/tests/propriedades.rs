// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// propriedades.rs — property tests do daemon (Fase 3.2)
// ---------------------------------------------------------------------
// Invariântes do sistema para TODO o domínio de inputs gerados
// (Resumo §48), não apenas para exemplos fixos:
//
//   * pipeline K1→K9: roundtrip fim-a-fim e payload acima do limite
//     rejeitado antes de processar (fail-closed);
//   * envelope: qualquer corrupção de um byte é rejeitada;
//   * matriz de corrupção campo-a-campo com o erro exacto documentado
//     (versão, nonce assinado, assinatura, ciphertext);
//   * anti-replay: check-and-insert de `nonce1` (uma só passagem) e
//     libertação da reserva quando a decifragem falha por chave errada;
//   * handshake: montagem/validação roundtrip e corrupção de QUALQUER
//     byte do pedido rejeitada.
//
// O binário não é alterado; estes testes só exercitam a API pública.
// =====================================================================

use std::collections::BTreeSet;

use crypto_core::signer::chave_publica_a_partir_da_seed;
use proptest::prelude::*;

use onyxchatd::anti_replay::{RegistoNonce1, CAPACIDADE_NONCES1};
use onyxchatd::envelope;
use onyxchatd::erros::ErroPipeline;
use onyxchatd::handshake;
use onyxchatd::pipeline::{self, Chaves};

// ---------------------------------------------------------------------
// Estratégias partilhadas
// ---------------------------------------------------------------------

/// Chaves de pipeline (K1/K5/K9) arbitrárias.
fn chaves() -> impl Strategy<Value = Chaves> {
    (any::<[u8; 32]>(), any::<[u8; 32]>(), any::<[u8; 32]>()).prop_map(|(k1, k5, k9)| Chaves {
        k1,
        k5,
        k9,
    })
}

/// Nonce de 12 bytes arbitrário (formato do envelope).
fn nonce12() -> impl Strategy<Value = [u8; 12]> {
    any::<[u8; 12]>()
}

/// Texto UTF-8 **não vazio**.
///
/// A restrição existe porque o pipeline recusa plaintext vazio
/// (`MIN_PLAINTEXT = 1`, `ErroPipeline::PayloadVazio`). Usar
/// `any::<String>()` directamente faria o proptest gerar a string vazia
/// e o teste falharia com `PayloadVazio` em vez de testar o que diz
/// testar.
///
/// Isto é uma melhoria da força do teste, não uma conveniência: filtrar
/// a string vazia é o que garante que «para todos os textos» continua a
/// ser verdade depois da introdução do limite mínimo. O caso da string
/// vazia é verificado à parte, em `pipeline::tests::mensagem_vazia`.
fn texto_nao_vazio() -> impl Strategy<Value = String> {
    any::<String>().prop_filter("texto não vazio", |s| !s.is_empty())
}

// ---------------------------------------------------------------------
// Pipeline fim-a-fim
// ---------------------------------------------------------------------

proptest! {
    /// `decifrar(cifrar(texto)) == texto` para qualquer texto UTF-8,
    /// chaves, seed e nonces.
    #[test]
    fn pipeline_roundtrip_para_inputs_arbitrarios(
        texto in texto_nao_vazio(),
        k in chaves(),
        seed in any::<[u8; 32]>(),
        n1 in nonce12(),
        n5 in nonce12(),
        n9 in nonce12(),
    ) {
        let pub_a = chave_publica_a_partir_da_seed(&seed);
        let (envelope, _) = pipeline::cifrar_com_nonces(
            texto.as_bytes(),
            &k,
            &seed,
            n1,
            n5,
            n9,
        )
        .expect("cifra sempre possível dentro do limite");
        let mut registo = RegistoNonce1::novo();
        prop_assert_eq!(
            pipeline::decifrar(&envelope, &k, &pub_a, &mut registo),
            Ok(texto)
        );
    }

    /// Mensagens acima de `MAX_PLAINTEXT` são rejeitadas com
    /// `PayloadGrande` — antes de qualquer alocação proporcional.
    #[test]
    fn payload_acima_do_limite_e_rejeitado(
        obtido in 65_537usize..70_000,
        k in chaves(),
        seed in any::<[u8; 32]>(),
    ) {
        let dados = vec![0u8; obtido];
        prop_assert_eq!(
            pipeline::cifrar(&dados, &k, &seed),
            Err(ErroPipeline::PayloadGrande {
                obtido,
                maximo: onyxchatd::envelope::MAX_PLAINTEXT,
            })
        );
    }

    /// Envelope truncado (prefixo curto do mínimo) é rejeitado.
    #[test]
    fn envelope_truncado_e_rejeitado(
        texto in texto_nao_vazio(),
        k in chaves(),
        seed in any::<[u8; 32]>(),
        corte in 0usize..117,
    ) {
        let pub_a = chave_publica_a_partir_da_seed(&seed);
        let (envelope, _) =
            pipeline::cifrar_com_nonces(texto.as_bytes(), &k, &seed, [7; 12], [8; 12], [9; 12])
                .expect("cifra");
        let mut registo = RegistoNonce1::novo();
        prop_assert!(
            pipeline::decifrar(&envelope[..corte], &k, &pub_a, &mut registo).is_err()
        );
    }

    /// Correr um byte de QUALQUER posição do envelope (incluindo a
    /// assinatura) faz a decifragem falhar.
    #[test]
    fn corrupcao_de_qualquer_byte_e_rejeitada(
        texto in texto_nao_vazio(),
        k in chaves(),
        seed in any::<[u8; 32]>(),
        idx in any::<usize>(),
    ) {
        let pub_a = chave_publica_a_partir_da_seed(&seed);
        let (envelope, _) = pipeline::cifrar_com_nonces(
            texto.as_bytes(),
            &k,
            &seed,
            [1; 12],
            [2; 12],
            [3; 12],
        )
        .expect("cifra");
        let mut alterado = envelope.clone();
        let pos = idx % alterado.len();
        alterado[pos] ^= 0x01;
        let mut registo = RegistoNonce1::novo();
        prop_assert!(pipeline::decifrar(&alterado, &k, &pub_a, &mut registo).is_err());
    }
}

// ---------------------------------------------------------------------
// Matriz de corrupção (docs/testing.md §Testes de corrupção)
// ---------------------------------------------------------------------
//
// O teste genérico acima prova que *qualquer* byte corrompido falha; os
// quatro casos abaixo fixam, campo a campo, o **erro exacto** que a
// matriz documenta. Só assim é que a tabela de `testing.md` fica
// verificada por código e não apenas afirmada em prosa:
//
//   versão alterada    → EnvelopeInvalido   (anti-downgrade, fase 1)
//   nonce alterado     → AssinaturaInvalida (nonce coberto pela sig.)
//   assinatura alter.  → AssinaturaInvalida (Ed25519)
//   ciphertext alter.  → AssinaturaInvalida (a sig. cobre o ciphertext)
//
// A última linha merece explicação: `regiao_assinada` é
// `versão ‖ nonce1 ‖ nonce5 ‖ nonce9 ‖ ciphertext`, logo um byte de
// ciphertext alterado nunca chega à tag AEAD — é travado pela assinatura
// na etapa 2 de `pipeline::decifrar` (verificação **antes** da
// decifragem, Resumo §21).

proptest! {
    /// Byte de versão ≠ `0x01` → `EnvelopeInvalido`, rejeitado pelo
    /// parser **antes** de qualquer operação com chaves (anti-downgrade:
    /// um atacante não troca a versão do envelope em trânsito).
    #[test]
    fn alterar_versao_e_rejeitado(
        texto in texto_nao_vazio(),
        k in chaves(),
        seed in any::<[u8; 32]>(),
        outra_versao in any::<u8>(),
    ) {
        let pub_a = chave_publica_a_partir_da_seed(&seed);
        let (envelope, _) = pipeline::cifrar_com_nonces(
            texto.as_bytes(),
            &k,
            &seed,
            [1; 12],
            [2; 12],
            [3; 12],
        )
        .expect("cifra");
        // 1/256 das estratégias manteria a versão correcta — rejeita-se
        // o caso em vez de o fulsificar como falha do produto.
        prop_assume!(outra_versao != envelope[0], "byte de versão distinto");
        let mut alterado = envelope.clone();
        alterado[0] = outra_versao;
        let mut registo = RegistoNonce1::novo();
        prop_assert!(
            matches!(
                pipeline::decifrar(&alterado, &k, &pub_a, &mut registo),
                Err(ErroPipeline::EnvelopeInvalido(_))
            ),
            "versão trocada tem de cair em EnvelopeInvalido"
        );
        // Nada ficou reservado: o parser falhou antes do anti-replay.
        prop_assert_eq!(registo.quantidade(), 0, "sem reserva após rejeição");
    }

    /// Qualquer byte do campo `nonce1` (offset 1..13) →
    /// `AssinaturaInvalida`. O `nonce1` está coberto pela assinatura,
    /// pelo que um replay não consegue trocá-lo por um nonce novo sem
    /// invalidar Ed25519 (anti-replay ligado à autenticidade).
    #[test]
    fn alterar_nonce_assinado_e_rejeitado(
        texto in texto_nao_vazio(),
        k in chaves(),
        seed in any::<[u8; 32]>(),
        idx in any::<usize>(),
    ) {
        let pub_a = chave_publica_a_partir_da_seed(&seed);
        let (envelope, _) = pipeline::cifrar_com_nonces(
            texto.as_bytes(),
            &k,
            &seed,
            [1; 12],
            [2; 12],
            [3; 12],
        )
        .expect("cifra");
        let mut alterado = envelope.clone();
        // Offset 0 é a versão; 1..13 é `nonce1` (TAM_NONCE = 12).
        let pos = 1 + idx % 12;
        alterado[pos] ^= 0x01;
        let mut registo = RegistoNonce1::novo();
        prop_assert!(
            matches!(
                pipeline::decifrar(&alterado, &k, &pub_a, &mut registo),
                Err(ErroPipeline::AssinaturaInvalida)
            ),
            "nonce1 adulterado tem de cair em AssinaturaInvalida"
        );
        // A assinatura falhou ANTES da reserva — o nonce1 do atacante
        // nunca chega a entrar no registo circular.
        prop_assert_eq!(registo.quantidade(), 0, "sem reserva após assinatura inválida");
    }

    /// Byte da assinatura Ed25519 (últimos 64) → `AssinaturaInvalida`.
    #[test]
    fn alterar_assinatura_e_rejeitado(
        texto in texto_nao_vazio(),
        k in chaves(),
        seed in any::<[u8; 32]>(),
        idx in any::<usize>(),
    ) {
        let pub_a = chave_publica_a_partir_da_seed(&seed);
        let (envelope, _) = pipeline::cifrar_com_nonces(
            texto.as_bytes(),
            &k,
            &seed,
            [1; 12],
            [2; 12],
            [3; 12],
        )
        .expect("cifra");
        let mut alterado = envelope.clone();
        // A assinatura é o bloco final de TAM_ASSINATURA = 64 bytes.
        let inicio_sig = alterado.len() - 64;
        let pos = inicio_sig + idx % 64;
        alterado[pos] ^= 0x01;
        let mut registo = RegistoNonce1::novo();
        prop_assert!(
            matches!(
                pipeline::decifrar(&alterado, &k, &pub_a, &mut registo),
                Err(ErroPipeline::AssinaturaInvalida)
            ),
            "assinatura adulterada tem de cair em AssinaturaInvalida"
        );
    }

    /// Byte do ciphertext (região 37 .. fim-64) → `AssinaturaInvalida`.
    ///
    /// A assinatura cobre o ciphertext, portanto a adulteração morre na
    /// verificação de Ed25519 — a tag AEAD nem é alcançada. Isto é o
    /// comportamento documentado em `docs/message_format.md` e é o que
    /// impede troca de payload entre mensagens assinadas.
    #[test]
    fn alterar_ciphertext_e_rejeitado(
        texto in texto_nao_vazio(),
        k in chaves(),
        seed in any::<[u8; 32]>(),
        idx in any::<usize>(),
    ) {
        let pub_a = chave_publica_a_partir_da_seed(&seed);
        let (envelope, _) = pipeline::cifrar_com_nonces(
            texto.as_bytes(),
            &k,
            &seed,
            [1; 12],
            [2; 12],
            [3; 12],
        )
        .expect("cifra");
        let mut alterado = envelope.clone();
        // Cabeçalho: 1 (versão) + 3×12 (nonces) = 37 bytes; assinatura:
        // 64 finais. O ciphertext ocupa o intervalo entre os dois.
        let fim_ct = alterado.len() - 64;
        prop_assume!(fim_ct > 37, "há ciphertext para corromper");
        let pos = 37 + idx % (fim_ct - 37);
        alterado[pos] ^= 0x01;
        let mut registo = RegistoNonce1::novo();
        prop_assert!(
            matches!(
                pipeline::decifrar(&alterado, &k, &pub_a, &mut registo),
                Err(ErroPipeline::AssinaturaInvalida)
            ),
            "ciphertext adulterado tem de cair em AssinaturaInvalida"
        );
        prop_assert_eq!(registo.quantidade(), 0, "sem reserva após assinatura inválida");
    }
}

// ---------------------------------------------------------------------
// Anti-replay (Resumo §35)
// ---------------------------------------------------------------------

proptest! {
    /// `reservar` aceita cada `nonce1` uma vez só; repetir devolve
    /// `false` e não duplica entradas no registo.
    #[test]
    fn anti_replay_so_passa_uma_vez(
        nonces in prop::collection::vec(any::<[u8; 12]>(), 0..100),
    ) {
        let mut registo = RegistoNonce1::novo();
        for nonce in &nonces {
            prop_assert!(registo.reservar(nonce), "primeira passagem aceite");
            prop_assert!(!registo.reservar(nonce), "reenvio rejeitado");
        }
        let esperado = nonces.iter().copied().collect::<BTreeSet<_>>();
        prop_assert_eq!(registo.quantidade(), esperado.len());
        prop_assert!(registo.quantidade() <= CAPACIDADE_NONCES1);
    }

    /// Uma decifragem falhada (chave errada) liberta a reserva, de
    /// forma que a mensagem autêntica possa ser processada depois.
    #[test]
    fn falha_de_chave_liberta_a_reserva(
        texto in texto_nao_vazio(),
        k in chaves(),
        k_errada in chaves(),
        seed in any::<[u8; 32]>(),
        n1 in nonce12(),
    ) {
        prop_assume!(k_errada.k9 != k.k9, "K9 distinta");
        let pub_a = chave_publica_a_partir_da_seed(&seed);
        let (envelope, _) = pipeline::cifrar_com_nonces(
            texto.as_bytes(),
            &k,
            &seed,
            n1,
            [4; 12],
            [5; 12],
        )
        .expect("cifra");
        let mut registo = RegistoNonce1::novo();
        // Chave errada → falha de decifragem → reserva libertada.
        prop_assert!(pipeline::decifrar(&envelope, &k_errada, &pub_a, &mut registo).is_err());
        prop_assert_eq!(registo.quantidade(), 0, "reserva libertada");
        // Agora a mensagem autêntica é aceite (nonce1 ainda livre).
        prop_assert_eq!(
            pipeline::decifrar(&envelope, &k, &pub_a, &mut registo),
            Ok(texto)
        );
    }
}

// ---------------------------------------------------------------------
// Handshake (REQUEST/ACCEPT/REJECT)
// ---------------------------------------------------------------------

proptest! {
    /// Montar → validar roundtrips em ambos os lados; a recusa valida
    /// contra a pública de B; e QUALQUER corrupção de um byte do pedido
    /// é rejeitada.
    #[test]
    fn handshake_roundtrip_e_corrupcao_rejeitada(
        seed_a in any::<[u8; 32]>(),
        seed_b in any::<[u8; 32]>(),
        k1 in any::<[u8; 32]>(),
        k5 in any::<[u8; 32]>(),
        k9 in any::<[u8; 32]>(),
        k5_b in any::<[u8; 32]>(),
        k9_b in any::<[u8; 32]>(),
        nonce in any::<[u8; 16]>(),
        idx in any::<usize>(),
    ) {
        let pub_a = chave_publica_a_partir_da_seed(&seed_a);
        let pub_b = chave_publica_a_partir_da_seed(&seed_b);

        // A monta e B valida o pedido.
        let (pedido, request) = handshake::montar_pedido_com(&seed_a, nonce, k1, k5, k9);
        // Compara por referência: `pedido` continua a ser emprestado na
        // aceitação abaixo (um `prop_assert_eq!` por valor moveria o
        // struct e rebentaria com E0382).
        prop_assert_eq!(
            &handshake::validar_pedido(&request).expect("pedido válido"),
            &pedido
        );

        // B aceita; A confirma; as amizades cruzam-se corretamente.
        let (amizade_b, aceite) = handshake::aceitar_pedido_com(&seed_b, &pedido, k5_b, k9_b);
        let amizade_a =
            handshake::confirmar_aceite(&seed_a, &request, &aceite).expect("A confirma o ACCEPT");
        prop_assert_eq!(amizade_a.publica, pub_b);
        prop_assert_eq!(amizade_a.k1, k1);
        prop_assert_eq!(amizade_a.k5_proprio, k5);
        prop_assert_eq!(amizade_a.k5_par, k5_b);
        prop_assert_eq!(amizade_a.k9_par, k9_b);
        prop_assert_eq!(amizade_b.publica, pub_a);
        prop_assert_eq!(amizade_b.k5_proprio, k5_b);
        prop_assert_eq!(amizade_b.k5_par, k5);

        // Recusa: valida contra a pública de B e ecoa o nonce.
        let recusa = handshake::montar_recusa(&seed_b, &nonce);
        prop_assert!(
            handshake::validar_recusa_com(&recusa, &nonce, &pub_b).is_ok(),
            "recusa válida"
        );

        // Corrupção: um flip de bit em qualquer byte do pedido falha.
        let mut alterado = request.clone();
        let pos = idx % alterado.len();
        alterado[pos] ^= 0x01;
        prop_assert!(handshake::validar_pedido(&alterado).is_err());
    }
}

// =====================================================================
// Tolerância de versões nos formatos de rede (F3.3)
// =====================================================================
//
// O princípio (`docs/index.md` §Princípio da tolerância de versões):
// aceitar as versões que se suportam nativamente, recusar as restantes.
//
// Estes testes não verificam que «a v0x01 é aceite» — isso é trivial.
// Verificam as duas propriedades que importam:
//
//   1. A **lista** `VERSOES_ACEITE` é o mecanismo, e a versão que
//      escrevemos está nela. Se amanhã alguém trocar a escrita para uma
//      v0x02 sem a acrescentar à lista, estes testes falham.
//   2. Uma versão **fora** da lista é recusada com o erro certo, e a
//      recusa acontece **antes** de qualquer decifragem (para o
//      envelope) ou de qualquer verificação de assinatura (para o
//      handshake).

/// O envelope escreve uma versão que está na lista de leitura.
///
/// Sem esta propriedade, acrescentar uma v2 à lista e escrever a v1 (ou
/// vice-versa) partiria o sistema: escreveria envelopes que não lê.
#[test]
fn envelope_escreve_uma_versao_que_lê() {
    assert!(
        envelope::VERSOES_ACEITE.contains(&envelope::VERSAO),
        "a versão escrita ({:#04x}) tem de estar na lista de leitura {:?}",
        envelope::VERSAO,
        envelope::VERSOES_ACEITE
    );
}

/// Uma versão **fora** da lista é recusada — e antes de decifrar.
#[test]
fn envelope_recusa_versao_fora_da_lista() {
    for versao in 0u8..=255 {
        if envelope::VERSOES_ACEITE.contains(&versao) {
            continue;
        }
        // Um envelope mínimo bem formado, com a versão trocada. Se
        // qualquer outra validação corresse primeiro, o erro seria outro.
        let mut envelope = envelope_minimo_valido();
        envelope[0] = versao;

        match envelope::Envelope::abrir(&envelope) {
            Err(envelope::ErroEnvelope::VersaoDesconhecida { obtida }) => {
                assert_eq!(obtida, versao, "o erro tem de reportar a versão recebida");
            }
            Err(outro) => panic!(
                "versão {versao:#04x} deu {outro:?}, não VersaoDesconhecida — \
                 a validação está pela ordem errada"
            ),
            Ok(_) => panic!("versão {versao:#04x} foi aceite e não devia"),
        }
    }
}

/// Uma versão **dentro** da lista é aceite — o round-trip tem de
/// funcionar para todas elas.
#[test]
fn envelope_aceita_toda_a_lista() {
    for &versao in envelope::VERSOES_ACEITE {
        let mut envelope = envelope_minimo_valido();
        envelope[0] = versao;
        envelope::Envelope::abrir(&envelope)
            .unwrap_or_else(|e| panic!("versão {versao:#04x} da lista foi recusada: {e:?}"));
    }
}

/// O handshake escreve uma versão que está na lista de leitura.
#[test]
fn handshake_escreve_uma_versao_que_lê() {
    assert!(
        handshake::VERSOES_ACEITE.contains(&handshake::VERSAO_PROTOCOLO),
        "a versão escrita tem de estar na lista de leitura"
    );
}

/// Um corpo de handshake com versão fora da lista é recusado.
#[test]
fn handshake_recusa_versao_fora_da_lista() {
    for versao in 0u8..=255 {
        if handshake::VERSOES_ACEITE.contains(&versao) {
            continue;
        }
        // Um `FRIEND_REQUEST` real, com a versão trocada. Como a versão
        // entra no transcript, trocar o byte também invalida a
        // assinatura — mas a versão é validada **primeiro**, que é o
        // que se está a testar.
        let mut corpo = pedido_valido();
        corpo[0] = versao;

        match handshake::validar_pedido(&corpo) {
            Err(handshake::ErroHandshake::VersaoDesconhecida { obtida }) => {
                assert_eq!(obtida, versao);
            }
            Err(outro) => panic!("versão {versao:#04x} deu {outro:?}"),
            Ok(_) => panic!("versão {versao:#04x} foi aceite e não devia"),
        }
    }
}

/// Envelope mínimo bem formado: cabeçalho com nonces zero e 16 bytes de
/// ciphertext. A assinatura não é verificada por `abrir` (é só separada),
/// por isso zero serve.
fn envelope_minimo_valido() -> Vec<u8> {
    let mut e = vec![envelope::VERSAO];
    e.extend_from_slice(&[0u8; 3 * envelope::TAM_NONCE]);
    // O ciphertext ocupa o resto até ao mínimo. O comprimento é calculado
    // em runtime e aplicado com `resize`/`extend_from_slice`, porque um
    // array de tamanho `[u8; expr]` exige uma expressão **constante** e
    // `e.len()` não o é.
    let assinatura = envelope::TAM_ASSINATURA;
    let ciphertext = envelope::TAM_MINIMO - e.len() - assinatura;
    e.resize(e.len() + ciphertext, 0u8);
    e.resize(envelope::TAM_MINIMO, 0u8);
    assert_eq!(e.len(), envelope::TAM_MINIMO);
    e
}

/// `FRIEND_REQUEST` válido com chaves e assinatura reais.
fn pedido_valido() -> Vec<u8> {
    // `montar_pedido` gera nonce e chaves internamente e devolve
    // `(Pedido, corpo)`. Não precisa da pública: é derivável da seed.
    let seed = [0x11u8; handshake::TAM_PUB];
    let (_pedido, corpo) = handshake::montar_pedido(&seed);
    corpo
}

