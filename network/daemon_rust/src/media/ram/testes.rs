// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Testes do `memfd` de media.
//!
//! O que tentam estragar, por ordem: devolver um descritor que não pode
//! ser lido (e a falha aparecer só no leitor), devolver um descritor
//! **legível por outros**, e devolver um descritor que possa crescer
//! depois de devolvido.

use super::*;

#[test]
fn o_conteudo_volta_intacto() {
    let conteudo: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", &conteudo).expect("memfd");

    assert_eq!(ficheiro.tamanho(), conteudo.len());
    assert_eq!(ficheiro.ler_de_volta().expect("le"), conteudo);
}

#[test]
fn um_ficheiro_vazio_e_legal() {
    // Um áudio mais curto do que um quadro dá zero bytes. `write` com
    // comprimento zero devolve 0 sem escrever, e uma implementação que
    // tratasse `escrito == 0` como falha recusaria um ficheiro
    // perfectly válido.
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", b"").expect("memfd");
    assert_eq!(ficheiro.tamanho(), 0);
    assert!(ficheiro.ler_de_volta().expect("le").is_empty());
}

#[test]
fn o_conteudo_nao_cresce_apos_o_selo() {
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", b"origem").expect("memfd");
    let fd = ficheiro.fd();

    // A propriedade de §4.6: depois de devolvido, ninguém acrescenta
    // bytes. Sem o selo, um leitor podia transformar o vídeo num
    // ficheiro maior do que o que o SHA-256 diz.
    // Ao fim, para a escrita **aumentar** o ficheiro. Uma escrita no
    // meio não prova nada: reescrever bytes que já lá estão é
    // permitido, e era o que o `F_SEAL_SEAL` deixava passar.
    assert_eq!(
        unsafe { libc::lseek(fd, 0, libc::SEEK_END) },
        6,
        "o cursor devia estar no fim"
    );
    let n = unsafe { libc::write(fd, b"\x00\x00\x00".as_ptr().cast(), 3) };
    assert!(
        n < 0,
        "o descritor aceitou escrita que aumentaria o ficheiro"
    );
    assert_eq!(
        ultimo_erro(),
        libc::EPERM,
        "o motivo tem de ser EPERM, para se saber que foi o selo"
    );

    // E o conteúdo original está intacto.
    assert_eq!(ficheiro.ler_de_volta().expect("le"), b"origem");
}

#[test]
fn o_conteudo_esta_no_caminho_do_descritor() {
    // Um `memfd` não pode ser "aberto" por outro processo sem herdar o
    // descritor, e é isso que se está a testar: o conteúdo é legível
    // pelo caminho `/proc/self/fd/N`, que é o que o leitor de media faz.
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", b"conteudo").expect("memfd");
    let caminho = format!("/proc/self/fd/{}", ficheiro.fd());

    let lido = std::fs::read(&caminho).expect("lê pelo caminho do descritor");
    assert_eq!(lido, b"conteudo");
}

#[test]
fn as_permissoes_sao_de_dono() {
    use std::os::unix::fs::PermissionsExt as _;

    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", b"privado").expect("memfd");
    let caminho = format!("/proc/self/fd/{}", ficheiro.fd());

    let modo = std::fs::metadata(&caminho)
        .expect("metadados")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(modo, 0o600, "o media ficou legível por outros");
}

#[test]
fn um_nome_com_bytes_nulos_da_erro_e_nao_panico() {
    // O nome vem de um nome de ficheiro, e esse nome pode vir de fora.
    // Um `.expect()` aqui seria um pânico por causa de um `str`.
    assert!(matches!(
        FicheiroEmRam::de_conteudo("onyx\0media", b"x"),
        Err(ErroMedia::NomeInvalido)
    ));
}

#[test]
fn o_nome_aparece_no_proc() {
    // Quem olha para `/proc/self/fd` vê o nome. Escolher «onyx-media» em
    // vez do `id` da transferência é uma decisão: o nome não diz que
    // vídeo é, e um `id` no nome de um descritor seria metadado de quem
    // viu o quê.
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", b"x").expect("memfd");
    let alvo = std::fs::read_link(format!("/proc/self/fd/{}", ficheiro.fd())).expect("link");
    assert!(
        alvo.to_string_lossy().starts_with("/memfd:onyx-media"),
        "nome inesperado: {alvo:?}"
    );
}

#[test]
fn o_descritor_herdado_continua_a_funcionar() {
    // O caminho de produção: o leitor herda o descritor no `exec`, e o
    // cursor fica onde o `write` o deixou.
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", b"herdado").expect("memfd");
    let fd = ficheiro.para_herdar();

    // O cursor fica **no fim**, deixado pelo `write`. Um leitor que
    // herda o descritor tem de o reposicionar — e é por isso que o
    // daemon passa o descritor por `execve("/proc/self/fd/N")`: o
    // filho abre o caminho e começa na posição 0. Este teste usa o
    // `lseek` que um leitor directo teria de fazer.
    assert_eq!(unsafe { libc::lseek(fd, 0, libc::SEEK_SET) }, 0);

    let mut lido = [0u8; 7];
    let n = unsafe { libc::read(fd, lido.as_mut_ptr().cast(), lido.len()) };
    assert_eq!(n, 7);
    assert_eq!(&lido, b"herdado");

    // Uma segunda leitura devolve 0, e não um pânico nem bytes a mais.
    let n = unsafe { libc::read(fd, lido.as_mut_ptr().cast(), lido.len()) };
    assert_eq!(n, 0, "o cursor devia estar no fim");

    fechar(fd);
}

#[test]
fn o_pread_ignora_a_posicao() {
    // `pread` não mexe na posição, e é por isso que `ler_de_volta` é
    // correcta sem um `File` que tomasse posse do descritor.
    let ficheiro = FicheiroEmRam::de_conteudo("onyx-media", b"abcdef").expect("memfd");
    assert_eq!(ficheiro.ler_de_volta().expect("le"), b"abcdef");
    assert_eq!(
        ficheiro.ler_de_volta().expect("le"),
        b"abcdef",
        "a segunda leitura"
    );
    fechar(ficheiro.fd());
}
