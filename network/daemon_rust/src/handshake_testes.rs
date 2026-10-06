// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// handshake_testes.rs — os testes de `handshake`, num ficheiro próprio
// ---------------------------------------------------------------------
// Ver a nota em `handshake.rs`, à altura do `mod testes`.
//
// Mais apertado do que nos outros dois ficheiros: `handshake.rs` tem
// 642 linhas de produção e 537 de teste. Ganhar o ficheiro aqui vale
// menos do que a consistência — um leitor que aprendeu que os testes
// vivem em `handshake_testes.rs` não tem de se perguntar se este é a
// excepção.
//
// Os testes usam `dominio()`, `pedido_de_teste` e o `RegistoAmigos`,
// que é privado. Filho do módulo, sem alargar visibilidade.
// =====================================================================
use super::*;

// ----------------------------------------------------------------
// Utilitários
// ----------------------------------------------------------------

/// Identidade nova de teste.
fn identidade() -> ([u8; 32], [u8; 32]) {
    crypto_core::gerar_identidade()
}

/// Invariante: todos os corpos têm o tamanho exato do spec.
#[test]
fn tamanhos_do_spec() {
    assert_eq!(VERSAO_PROTOCOLO, 0x01);
    assert_eq!(TAM_CORPO_PEDIDO, 209);
    assert_eq!(TAM_CORPO_ACEITE, 177);
    assert_eq!(TAM_CORPO_RECUSA, 81);
    assert_eq!(DOMINIO_PEDIDO, b"ONYX/FRIEND/REQ");
    assert_eq!(DOMINIO_ACEITE, b"ONYX/FRIEND/ACC");
    assert_eq!(DOMINIO_RECUSA, b"ONYX/FRIEND/REJ");
    assert_eq!(TIPO_PEDIDO, 0x10);
    assert_eq!(TIPO_ACEITE, 0x11);
    assert_eq!(TIPO_RECUSA, 0x12);
    assert_eq!(TAM_NONCE, 16);
    assert_eq!(CAPACIDADE_NONCES, 256);
}

// ----------------------------------------------------------------
// FRIEND_REQUEST
// ----------------------------------------------------------------

/// Montar → validar roundtrip com todos os campos.
#[test]
fn pedido_roundtrip() {
    let (seed, publica) = identidade();
    let (esperado, corpo) = montar_pedido(&seed);
    assert_eq!(corpo.len(), TAM_CORPO_PEDIDO);
    assert_eq!(esperado.publica, publica, "pub derivada da seed");

    let lido = validar_pedido(&corpo).expect("valida");
    assert_eq!(lido, esperado, "todos os campos preservados");
    // Aleatório por construção (nunca reutiliza chaves/nonces).
    let (_, outro) = montar_pedido(&seed);
    assert_ne!(corpo, outro, "nonce/chaves aleatórios por pedido");
}

/// Corpo com tamanho errado → `ComprimentoInvalido` (0x0C).
#[test]
fn pedido_comprimentos_invalidos() {
    for tamanho in [0, 1, TAM_CORPO_PEDIDO - 1, TAM_CORPO_PEDIDO + 1] {
        let erro = validar_pedido(&vec![0u8; tamanho]).expect_err("invalido");
        assert_eq!(
            erro,
            ErroHandshake::ComprimentoInvalido {
                esperado: TAM_CORPO_PEDIDO,
                obtido: tamanho
            }
        );
        assert_eq!(erro.para_ipc(), 0x0C);
    }
}

/// Assinatura adulterada ou de outra identidade → rejeição.
#[test]
fn pedido_assinatura_invalida() {
    let (seed, _) = identidade();
    let (_, mut corpo) = montar_pedido(&seed);

    // Bit-flip na assinatura (últimos 64 bytes).
    let ultimo = corpo.len() - 1;
    corpo[ultimo] ^= 0x01;
    assert_eq!(
        validar_pedido(&corpo).expect_err("adulterado"),
        ErroHandshake::AssinaturaInvalida
    );

    // Chave pública trocada por outra identidade (offset 1: após a
    // versão — trocar o byte de versão testar-se-ia em `downgrade`).
    let (_, corpo) = montar_pedido(&seed);
    let (outra_seed, _) = identidade();
    let outra_pub = chave_publica_a_partir_da_seed(&outra_seed);
    let mut corpo2 = corpo;
    corpo2[1..33].copy_from_slice(&outra_pub);
    assert_eq!(
        validar_pedido(&corpo2).expect_err("pub trocada"),
        ErroHandshake::AssinaturaInvalida
    );

    // Corpo de tamanho certo com a versão correta mas lixo total.
    let mut lixo = vec![0xFFu8; TAM_CORPO_PEDIDO];
    lixo[0] = VERSAO_PROTOCOLO;
    assert_eq!(
        validar_pedido(&lixo).expect_err("lixo"),
        ErroHandshake::AssinaturaInvalida
    );
    assert!(ErroHandshake::AssinaturaInvalida
        .to_string()
        .contains("assinatura"));
}

/// Versão ≠ `0x01` → `VersaoDesconhecida`, antes da assinatura.
///
/// É o anti-downgrade do handshake: trocar a versão invalida a
/// assinatura, mas a rejeição acontece logo no primeiro byte.
#[test]
fn pedido_downgrade_de_versao() {
    for versao in [0x00u8, 0x02, 0x10, 0xFF] {
        let (seed, _) = identidade();
        let (_, mut corpo) = montar_pedido(&seed);
        corpo[0] = versao;
        assert_eq!(
            validar_pedido(&corpo).expect_err("versão trocada"),
            ErroHandshake::VersaoDesconhecida { obtida: versao },
            "versão 0x{versao:02x}"
        );
    }
    // Mensagem legível e estável (sem chaves/nonces).
    assert_eq!(
        ErroHandshake::VersaoDesconhecida { obtida: 0x02 }.to_string(),
        "versão de handshake desconhecida: 0x02"
    );
    assert_eq!(
        ErroHandshake::VersaoDesconhecida { obtida: 0x02 }.para_ipc(),
        0x0C
    );
}

// ----------------------------------------------------------------
// FRIEND_ACCEPT
// ----------------------------------------------------------------

/// Montar → validar roundtrip do aceite.
#[test]
fn aceite_roundtrip() {
    let (seed_b, pub_b) = identidade();
    let nonce = gerar_nonce();
    let k5 = crypto_core::gerar_chave();
    let k9 = crypto_core::gerar_chave();

    let (esperado, corpo) = montar_aceite(&seed_b, &k5, &k9, &nonce);
    assert_eq!(corpo.len(), TAM_CORPO_ACEITE);
    assert_eq!(esperado.publica, pub_b);
    assert_eq!(esperado.nonce, nonce, "eco do nonce do pedido");

    let lido = validar_aceite(&corpo).expect("valida");
    assert_eq!(lido, esperado);
}

/// Comprimento/assinatura inválidos no aceite → 0x0C.
#[test]
fn aceite_invalidos() {
    let (seed_b, _) = identidade();
    let (_, corpo) = montar_aceite(&seed_b, &[2u8; 32], &[3u8; 32], &[4u8; 16]);

    // Comprimento errado.
    let erro = validar_aceite(&corpo[..100]).expect_err("curto");
    assert_eq!(
        erro,
        ErroHandshake::ComprimentoInvalido {
            esperado: TAM_CORPO_ACEITE,
            obtido: 100
        }
    );

    // Assinatura adulterada (último byte do corpo).
    let mut adulterado = corpo.clone();
    let ultimo = adulterado.len() - 1;
    adulterado[ultimo] ^= 0x80;
    assert_eq!(
        validar_aceite(&adulterado).expect_err("sig"),
        ErroHandshake::AssinaturaInvalida
    );
    // Versão trocada → rejeição de downgrade (antes da assinatura).
    let mut downgrade = corpo.clone();
    downgrade[0] = 0x02;
    assert_eq!(
        validar_aceite(&downgrade).expect_err("versão"),
        ErroHandshake::VersaoDesconhecida { obtida: 0x02 }
    );
    // Lixo total de tamanho certo (com a versão correta).
    let mut lixo = vec![0xFFu8; TAM_CORPO_ACEITE];
    lixo[0] = VERSAO_PROTOCOLO;
    assert_eq!(
        validar_aceite(&lixo).expect_err("lixo"),
        ErroHandshake::AssinaturaInvalida
    );
}

// ----------------------------------------------------------------
// FRIEND_REJECT
// ----------------------------------------------------------------

/// Recusa válida: comprimento, nonce ecoado e assinatura da pub B.
#[test]
fn recusa_valida_e_nonce_divergente() {
    let (seed_b, pub_b) = identidade();
    let nonce = gerar_nonce();
    let corpo = montar_recusa(&seed_b, &nonce);
    assert_eq!(corpo.len(), TAM_CORPO_RECUSA);

    validar_recusa(&corpo, &nonce).expect("nonce certo");
    validar_recusa_com(&corpo, &nonce, &pub_b).expect("assinatura B");

    // Nonce divergente → NaoResponde.
    let outro = gerar_nonce();
    assert_eq!(
        validar_recusa(&corpo, &outro).expect_err("nonce errado"),
        ErroHandshake::NaoResponde
    );
    // Comprimento errado → ComprimentoInvalido.
    assert_eq!(
        validar_recusa(&corpo[..10], &nonce).expect_err("curto"),
        ErroHandshake::ComprimentoInvalido {
            esperado: TAM_CORPO_RECUSA,
            obtido: 10
        }
    );
    // Assinatura errada (outra pub) → AssinaturaInvalida.
    let (_, outra_pub) = identidade();
    assert_eq!(
        validar_recusa_com(&corpo, &nonce, &outra_pub).expect_err("pub errada"),
        ErroHandshake::AssinaturaInvalida
    );
    // Corpo com nonce certo mas assinatura destruída → só a
    // assinatura pode falhar (o nonce já ecoa). A destruição começa
    // no offset 17: versão e nonce têm de ficar intactos.
    let mut lixo = corpo.clone();
    lixo[17..].fill(0);
    assert_eq!(
        validar_recusa_com(&lixo, &nonce, &pub_b).expect_err("lixo"),
        ErroHandshake::AssinaturaInvalida
    );
    // Versão trocada → VersaoDesconhecida, antes do nonce/assinatura.
    let mut downgrade = corpo.clone();
    downgrade[0] = 0x02;
    assert_eq!(
        validar_recusa(&downgrade, &nonce).expect_err("versão"),
        ErroHandshake::VersaoDesconhecida { obtida: 0x02 }
    );
}

// ----------------------------------------------------------------
// Fluxos completos
// ----------------------------------------------------------------

/// Fluxo completo A → B → A: pedido, aceite e amizades espelhadas.
#[test]
fn fluxo_completo_de_amizade() {
    let (seed_a, pub_a) = identidade();
    let (seed_b, pub_b) = identidade();

    // A monta e envia o pedido.
    let (pedido_a, corpo_pedido) = montar_pedido(&seed_a);

    // B valida e aceita: gera chaves e monta o aceite.
    let pedido_validado = validar_pedido(&corpo_pedido).expect("B valida");
    let (amizade_b, corpo_aceite) = aceitar_pedido(&seed_b, &pedido_validado);
    assert_eq!(amizade_b.publica, pub_a);
    assert_eq!(amizade_b.k1, pedido_a.k1, "K1 vem do pedido");
    assert_ne!(amizade_b.k5_proprio, pedido_a.k5, "K5_B ≠ K5_A");

    // A confirma: o aceite tem de ecoar o nosso nonce.
    let amizade_a =
        confirmar_aceite(&seed_a, &corpo_pedido, &corpo_aceite).expect("A confirma");
    assert_eq!(amizade_a.publica, pub_b);
    assert_eq!(amizade_a.k1, amizade_b.k1, "K1 comum");
    assert_eq!(amizade_a.k5_proprio, pedido_a.k5, "K5_A própria");
    assert_eq!(amizade_a.k9_proprio, pedido_a.k9, "K9_A própria");
    assert_eq!(amizade_a.k5_par, amizade_b.k5_proprio, "espelho");
    assert_eq!(amizade_a.k9_par, amizade_b.k9_proprio, "espelho");
    assert_eq!(amizade_b.k5_par, amizade_a.k5_proprio, "espelho B");
    assert_eq!(amizade_b.k9_par, amizade_a.k9_proprio, "espelho B");
}

/// Validar um pedido adulterado (pub trocada) → assinatura falha.
///
/// A validação é o passo obrigatório antes de `aceitar_pedido` —
/// daí viver fora da função infallível.
#[test]
fn validar_pedido_rejeita_pedido_adulterado() {
    let (seed_a, _) = identidade();
    let (_, corpo) = montar_pedido(&seed_a);
    let (seed_atacante, _) = identidade();
    // O atacante re-assina um pedido com a pub dele → inválido.
    // (Offset 1: o byte 0 é a versão, validada antes da assinatura.)
    let mut corpo2 = corpo.clone();
    let atacante_pub = chave_publica_a_partir_da_seed(&seed_atacante);
    corpo2[1..33].copy_from_slice(&atacante_pub);
    assert_eq!(
        validar_pedido(&corpo2),
        Err(ErroHandshake::AssinaturaInvalida)
    );
    // O pedido original continua válido (controle positivo).
    assert!(validar_pedido(&corpo).is_ok());
}

/// `confirmar_aceite` rejeita pedido não nosso, nonce trocado e lixo.
#[test]
fn confirmacao_com_falhas() {
    let (seed_a, _) = identidade();
    let (seed_b, _) = identidade();
    let (_pedido, corpo_pedido) = montar_pedido(&seed_a);
    let pedido_validado = validar_pedido(&corpo_pedido).expect("valida");
    let (_amizade, corpo_aceite) = aceitar_pedido(&seed_b, &pedido_validado);

    // Pedido de outra identidade (o corpo não está assinado pela
    // nossa seed) → rejeitado antes de sequer olhar ao aceite.
    let (outra_seed, _) = identidade();
    assert_eq!(
        confirmar_aceite(&outra_seed, &corpo_pedido, &corpo_aceite)
            .expect_err("pedido não nosso"),
        ErroHandshake::AssinaturaInvalida
    );

    // Aceite de B assinado corretamente, mas fechado sobre OUTRO
    // nonce: a assinatura é boa, logo só o eco do nonce falha.
    let k5_b = crypto_core::gerar_chave();
    let k9_b = crypto_core::gerar_chave();
    let (_aceite, aceite_de_outro) = montar_aceite(&seed_b, &k5_b, &k9_b, &[7u8; 16]);
    assert_eq!(
        confirmar_aceite(&seed_a, &corpo_pedido, &aceite_de_outro).expect_err("nonce trocado"),
        ErroHandshake::NaoResponde
    );

    // Corpo de aceite curto → comprimento inválido.
    assert!(matches!(
        confirmar_aceite(&seed_a, &corpo_pedido, &corpo_aceite[..10]),
        Err(ErroHandshake::ComprimentoInvalido { .. })
    ));
}

// ----------------------------------------------------------------
// Registo de amizades + anti-replay
// ----------------------------------------------------------------

/// O registo guarda **quantas** amizades há e **quais** as identidades
/// — e nada mais.
///
/// Regressão de G2: o registo guardava a `Amizade` inteira (192 bytes
/// por par: `publica` + cinco chaves de 32 B) e só alguma coisa lia
/// essas chaves — os testes. O runtime conta e confirma presenças, e
/// nunca volta a buscar uma chave: quem precisa das chaves recebe-as
/// na resposta IPC do handshake e é o cliente que as guarda.
#[test]
fn registo_conta_e_distingue_identidades() {
    let mut registo = RegistoAmigos::novo();
    assert_eq!(registo.quantidade(), 0);
    assert!(!registo.tem(&[1u8; 32]), "registo vazio não tem ninguém");

    assert!(!registo.guardar([1u8; 32]), "primeira não substitui");
    assert_eq!(registo.quantidade(), 1);
    assert!(registo.tem(&[1u8; 32]));
    assert!(!registo.tem(&[2u8; 32]), "pub desconhecida continua ausente");

    // Rotação de chaves com a **mesma** identidade: a contagem não
    // cresce. É a mesma pessoa, não uma amiga nova.
    assert!(registo.guardar([1u8; 32]), "segunda substitui");
    assert_eq!(registo.quantidade(), 1, "uma amizade por identidade");

    assert!(!registo.guardar([2u8; 32]));
    assert_eq!(registo.quantidade(), 2);
    assert!(registo.tem(&[1u8; 32]) && registo.tem(&[2u8; 32]));
}

/// `guardar` recebe uma identidade, não uma amizade.
///
/// Regressão de G2: o registo guardava a `Amizade` inteira — 192
/// bytes por par (a `publica` mais cinco chaves de 32 B) — e o custo
/// por amizade Vivia no *tipo do valor*, não numa linha de código
/// que dissesse «isto custa 192 bytes». Regressão possível: alguém
/// reintroduz as chaves no registo e o consumo sobe seis vezes sem
/// diff nenhum que aparente a mudança.
///
/// Este teste não mede bytes: fixa a **assinatura** de `guardar`.
/// Se alguém lhe passar a passar uma `Amizade`, isto deixa de
/// compilar — que é o aviso mais barato possível.
#[test]
fn registo_guarda_apenas_a_identidade() {
    let guarda: fn(&mut RegistoAmigos, [u8; TAM_PUB]) -> bool = RegistoAmigos::guardar;
    let mut registo = RegistoAmigos::novo();
    assert!(!guarda(&mut registo, [7u8; 32]));
    assert_eq!(registo.quantidade(), 1);
}

/// Anti-replay: nonces únicos detetados, memória circular limitada.
#[test]
fn anti_replay_circular() {
    let mut registo = RegistoAmigos::novo();
    let nonce = gerar_nonce();
    assert!(!registo.nonce_visto(&nonce));
    registo.guardar_nonce(nonce);
    assert!(registo.nonce_visto(&nonce), "replay detetado");
    assert!(!registo.nonce_visto(&gerar_nonce()), "nonce novo aceite");

    // Enche a memória para lá da capacidade: os mais antigos saem.
    for i in 0..CAPACIDADE_NONCES {
        registo.guardar_nonce([i as u8; 16]);
    }
    // O primeiro nonce (guardado no início) já saiu da janela.
    assert!(!registo.nonce_visto(&nonce), "expirou da janela");
    // Os mais recentes continuam lá.
    assert!(registo.nonce_visto(&[255u8; 16]));
}

// ----------------------------------------------------------------
// Erros
// ----------------------------------------------------------------

/// `Display` de todos os variantes — estável e sem segredos.
#[test]
fn display_dos_erros() {
    assert_eq!(
        ErroHandshake::ComprimentoInvalido {
            esperado: 209,
            obtido: 10
        }
        .to_string(),
        "corpo de handshake inválido: esperado 209 bytes, obtido 10"
    );
    assert_eq!(
        ErroHandshake::VersaoDesconhecida { obtida: 0x02 }.to_string(),
        "versão de handshake desconhecida: 0x02"
    );
    assert_eq!(
        ErroHandshake::AssinaturaInvalida.to_string(),
        "assinatura de handshake inválida"
    );
    assert_eq!(
        ErroHandshake::NonceRepetido.to_string(),
        "nonce de handshake repetido"
    );
    assert_eq!(
        ErroHandshake::NaoResponde.to_string(),
        "aceite não corresponde ao pedido enviado"
    );
    // Todos os variantes caem no mesmo código IPC.
    for erro in [
        ErroHandshake::VersaoDesconhecida { obtida: 0x02 },
        ErroHandshake::AssinaturaInvalida,
        ErroHandshake::NonceRepetido,
        ErroHandshake::NaoResponde,
    ] {
        assert_eq!(erro.para_ipc(), 0x0C);
    }
}

/// `montar_recusa` produz corpo válido para o nosso próprio nonce.
#[test]
fn recusa_self_check() {
    let (seed, pub_propria) = identidade();
    let nonce = gerar_nonce();
    let corpo = montar_recusa(&seed, &nonce);
    validar_recusa_com(&corpo, &nonce, &pub_propria).expect("auto-valida");
}

// ----------------------------------------------------------------
// Zeroização (Resumo §22 — `docs/key_management.md`)
// ----------------------------------------------------------------

/// `Pedido` apaga K1/K5/K9 no descarte; nonce/publica ficam.
#[test]
fn pedido_zeroiza_chaves_no_drop() {
    use std::mem::ManuallyDrop;
    let mut pedido = ManuallyDrop::new(Pedido {
        publica: [0x11; TAM_PUB],
        k1: [0xA1; 32],
        k5: [0xA5; 32],
        k9: [0xA9; 32],
        nonce: [0x33; TAM_NONCE],
    });
    unsafe { std::ptr::drop_in_place(&mut *pedido) };
    assert_eq!(pedido.k1, [0u8; 32], "K1 apagada");
    assert_eq!(pedido.k5, [0u8; 32], "K5 apagada");
    assert_eq!(pedido.k9, [0u8; 32], "K9 apagada");
    assert_eq!(pedido.publica, [0x11u8; TAM_PUB], "publica é pública");
    assert_eq!(pedido.nonce, [0x33u8; TAM_NONCE], "nonce é público");
}

/// `Aceite` apaga K5/K9 no descarte; nonce/publica ficam.
#[test]
fn aceite_zeroiza_chaves_no_drop() {
    use std::mem::ManuallyDrop;
    let mut aceite = ManuallyDrop::new(Aceite {
        publica: [0x22; TAM_PUB],
        k5: [0xB5; 32],
        k9: [0xB9; 32],
        nonce: [0x44; TAM_NONCE],
    });
    unsafe { std::ptr::drop_in_place(&mut *aceite) };
    assert_eq!(aceite.k5, [0u8; 32], "K5 apagada");
    assert_eq!(aceite.k9, [0u8; 32], "K9 apagada");
    assert_eq!(aceite.publica, [0x22u8; TAM_PUB], "publica é pública");
    assert_eq!(aceite.nonce, [0x44u8; TAM_NONCE], "nonce é público");
}

/// `Amizade` apaga as cinco chaves no descarte (guarda longa).
#[test]
fn amizade_zeroiza_chaves_no_drop() {
    use std::mem::ManuallyDrop;
    let mut amizade = ManuallyDrop::new(Amizade {
        publica: [0x55; TAM_PUB],
        k1: [0xC1; 32],
        k5_proprio: [0xC5; 32],
        k9_proprio: [0xC9; 32],
        k5_par: [0xD5; 32],
        k9_par: [0xD9; 32],
    });
    unsafe { std::ptr::drop_in_place(&mut *amizade) };
    assert_eq!(amizade.k1, [0u8; 32], "K1 apagada");
    assert_eq!(amizade.k5_proprio, [0u8; 32], "K5 própria apagada");
    assert_eq!(amizade.k9_proprio, [0u8; 32], "K9 própria apagada");
    assert_eq!(amizade.k5_par, [0u8; 32], "K5 do par apagada");
    assert_eq!(amizade.k9_par, [0u8; 32], "K9 do par apagada");
    assert_eq!(amizade.publica, [0x55u8; TAM_PUB], "publica fica");
}

/// Uma `Amizade` clonada zera-se também no seu próprio descarte
/// (cada instância é dona das suas cópias).
#[test]
fn clone_de_amizade_zeroiza_independente() {
    let original = Amizade {
        publica: [0x66; TAM_PUB],
        k1: [0xE1; 32],
        k5_proprio: [0xE5; 32],
        k9_proprio: [0xE9; 32],
        k5_par: [0xF5; 32],
        k9_par: [0xF9; 32],
    };
    let copia = original.clone();
    assert_eq!(copia.k1, original.k1, "clone preserva o conteúdo");
    drop(original);
    assert_eq!(copia.k1, [0xE1; 32], "o clone sobrevive intacto");
}
