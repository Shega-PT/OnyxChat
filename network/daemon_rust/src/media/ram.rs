// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

//! Um ficheiro montado em RAM, entregue como descritor.
//!
//! [`docs/media.md`](../../../../docs/media.md) §4.6: o ficheiro de
//! media **nunca toca o disco**. Nem temporário, nem apagado, nem
//! «limpo a seguir».
//!
//! ## Porque um descritor e não um caminho
//!
//! A alternativa — escrever em `/tmp` e devolver o caminho — tem três
//! problemas, e o primeiro basta:
//!
//! 1. O conteúdo aparece em `ls /tmp`, legível por qualquer outro
//!    utilizador da máquina. O media de uma pessoa não é público.
//! 2. O caminho volta a ser um caminho, e um caminho pode ser
//!    substituído entre o momento em que o daemon o diz e o momento em
//!    que o leitor o abre.
//! 3. «Apagar no fim» é um `unlink` que alguém pode não executar.
//!
//! Com um `memfd` não há nome, não há directório e não há conteúdo a
//! reverter: o ficheiro vive no espaço do processo e acaba quando o
//! descritor fecha.
//!
//! ## Porque este módulo não está em [`super::ffmpeg_embutido`]
//!
//! Porque os dois `memfd` fazem coisas **opostas**, e juntá-los seria
//! um módulo sobre um mecanismo com duas semânticas. O do `ffmpeg` é
//! selado com `F_SEAL_SEAL` porque é um executável, e ninguém pode
//! escrever depois. O de media **não** pode levar `F_SEAL_WRITE` — um
//! leitor que não consegue ler não serve de nada — e por isso só leva
//! os selos de **tamanho**.
//!
//! ## Sem [`Drop`]
//!
//! O descritor é **transferido** para quem o pediu, e fechá-lo aqui
//! seria fechá-lo antes de uso. Um descritor que fica por caminho numa
//! falha é fechado à mão, e [`criar`] é o único sítio onde isso
//! acontece.

use std::os::unix::io::RawFd;

use super::erros::ErroMedia;

/// `MFD_ALLOW_SEALING`.
///
/// Pedido porque o selo de tamanho precisa dele: um descritor que não
/// permite selar rejeita `F_ADD_SEALS` com `EBADF`, que é um erro de
/// permissões a dizer que o problema era o argumento.
const PERMITE_SELAR: libc::c_uint = 0x0002;

/// `F_SEAL_SEAL | F_SEAL_GROW` — o conteúdo não pode crescer depois de
/// devolvido.
///
/// É o que protege a propriedade que interessa: um leitor que herde o
/// descritor não pode acrescentar bytes ao fim, e portanto não pode
/// transformar o vídeo num ficheiro maior do que o que o SHA-256 diz.
///
/// ## Porque `F_SEAL_GROW` e não `F_SEAL_SEAL`
///
/// A primeira versão deste código punha `F_SEAL_SEAL` e escrevia, no
/// comentário, que impedia o ficheiro de crescer. **Não impede.** O que
/// o `F_SEAL_SEAL` proíbe é acrescentar mais selos e mapear memória
/// para lá do fim do ficheiro — e um `write(2)` normal **passa**.
///
/// Isto foi encontrado por um teste, não por leitura da documentação:
/// o teste escreve depois do selo e esperava `EPERM`, e a escrita foi
/// aceite com o ficheiro a passar de 6 para 9 bytes.
///
/// A distinção é esta:
///
/// | Selo | Proíbe |
/// | --- | --- |
/// | `F_SEAL_SHRINK` | `ftruncate` para **menor** |
/// | `F_SEAL_GROW`   | `write` que **aumenta**, e `mmap` para lá do fim |
/// | `F_SEAL_SEAL`   | mais selos, e `mmap` para lá do fim |
/// | `F_SEAL_WRITE`  | abrir para escrever — **incompatível** com ler |
///
/// `F_SEAL_WRITE` é o único que impediria escrever por inteiro, e é
/// exactamente o que não podemos levar: o leitor de media **tem** de
/// ler o ficheiro. `F_SEAL_GROW` é a marca certa — proíbe crescer sem
/// proibir ler.
///
/// `F_SEAL_SEAL` vem junto porque, sem ele, quem herde o descritor
/// poderia acrescentar selos — e um selo acrescentado com
/// `F_SEAL_WRITE` tornaria o ficheiro ilegível para quem ainda o
/// precisasse.
const SELO_CONTEUDO: i32 = 0x0001 | 0x0004;

/// Um ficheiro em RAM, selado no tamanho, pronto a ser lido por um
/// filho.
pub struct FicheiroEmRam {
    fd: RawFd,
    tamanho: usize,
}

impl FicheiroEmRam {
    /// Escreve `conteudo` num `memfd` e devolve-o.
    pub fn de_conteudo(nome: &str, conteudo: &[u8]) -> Result<Self, ErroMedia> {
        let fd = criar(nome)?;

        // Um ficheiro de zero bytes é legal — um áudio mais curto do
        // que um quadro dá exactamente isso. E `write` com comprimento
        // zero devolve 0 sem escrever, o que `escrito < 0` não confunde
        // com uma falha.
        if !conteudo.is_empty() {
            let escrito = unsafe { libc::write(fd, conteudo.as_ptr().cast(), conteudo.len()) };
            if escrito < 0 || escrito as usize != conteudo.len() {
                let errno = ultimo_erro();
                fechar(fd);
                return Err(ErroMedia::ExecucaoFalhou(errno));
            }
        }

        // `0600`, pelo motivo de `umask`: o `memfd` herda as permissões
        // do processo, e com `umask 022` isso seria legível por todos.
        if unsafe { libc::fchmod(fd, 0o600) } < 0 {
            let errno = ultimo_erro();
            fechar(fd);
            return Err(ErroMedia::ExecucaoFalhou(errno));
        }

        // Selar **depois** de escrever e de chmod: `F_SEAL_SEAL` proíbe
        // selar um descritor que ainda possa mudar de tamanho, e selar
        // antes faria a escrita falhar com `EPERM`.
        if unsafe { libc::fcntl(fd, libc::F_ADD_SEALS, SELO_CONTEUDO) } < 0 {
            let errno = ultimo_erro();
            fechar(fd);
            return Err(ErroMedia::ExecucaoFalhou(errno));
        }

        Ok(Self {
            fd,
            tamanho: conteudo.len(),
        })
    }

    /// O descritor, **transferido** para quem o pediu.
    ///
    /// O leitor de media herda-o no `exec`, e por isso não pode levar
    /// `FD_CLOEXEC`. `self` é consumido porque devolver o mesmo número
    /// duas vezes daria dois leitores com o mesmo descritor — e o
    /// segundo a fechá-lo deixaria o primeiro a ler um número
    /// reutilizado por outra coisa.
    pub fn para_herdar(self) -> RawFd {
        self.fd
    }

    /// O descritor, sem o consumir.
    ///
    /// Para os testes e para quem precisar de medir o conteúdo antes de
    /// o passar a alguém.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// O tamanho escrito, em bytes.
    pub fn tamanho(&self) -> usize {
        self.tamanho
    }

    /// Lê o conteúdo de volta do descritor.
    ///
    /// É a verificação de §4.6: «o ficheiro nunca toca o disco» é uma
    /// afirmação, e affirmá-la sem ler seria confiar na promessa.
    ///
    /// `pread` e não [`std::fs::File`] **de propósito**: um `File` toma
    /// posse do descritor e fecha-o ao cair, e o `fd` de quem o pediu
    /// deixaria de valer. `pread` não mexe na posição, não fecha nada
    /// e devolve exactamente o que se pediu.
    pub fn ler_de_volta(&self) -> Result<Vec<u8>, ErroMedia> {
        let mut saida = vec![0u8; self.tamanho];
        let mut lidos = 0usize;
        while lidos < self.tamanho {
            let n = unsafe {
                libc::pread(
                    self.fd,
                    saida[lidos..].as_mut_ptr().cast(),
                    self.tamanho - lidos,
                    lidos as libc::off_t,
                )
            };
            if n < 0 {
                // `EINTR` é um sinal, não um fim de leitura. Sem esta
                // distinção, um `SIGCHLD` durante a leitura faria o
                // conteúdo parecer truncado.
                if ultimo_erro() == libc::EINTR {
                    continue;
                }
                return Err(ErroMedia::ExecucaoFalhou(ultimo_erro()));
            }
            if n == 0 {
                // Fim de ficheiro antes do tamanho declarado. Num
                // `memfd` só acontece se o conteúdo não tiver sido
                // escrito — e nesse caso devolver menos bytes do que o
                // `tamanho` seria pior do que falhar.
                return Err(ErroMedia::ExecucaoFalhou(libc::EIO));
            }
            lidos += n as usize;
        }
        Ok(saida)
    }
}

/// Cria o `memfd` vazio, com as permissões de dono.
fn criar(nome: &str) -> Result<RawFd, ErroMedia> {
    // O nome é o que aparece em `/proc/self/fd`, e é o que alguém vê
    // numa lista de descritores. `CString::new` só falha com bytes
    // nulos — e um nome de ficheiro não tem, mas um `.expect()` aqui
    // seria um pânico por causa de um `str` que veio de fora.
    let nome = std::ffi::CString::new(nome).map_err(|_| ErroMedia::NomeInvalido)?;

    let fd = unsafe { libc::memfd_create(nome.as_ptr(), PERMITE_SELAR) };
    if fd < 0 {
        return Err(ErroMedia::ExecucaoFalhou(ultimo_erro()));
    }
    Ok(fd)
}

/// Fecha um descritor que ficou por caminho numa falha.
fn fechar(fd: RawFd) {
    unsafe { libc::close(fd) };
}

/// O `errno` mais recente, para meter no erro.
fn ultimo_erro() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

/// Escreve `conteudo` num `memfd` e devolve o descritor selado.
pub fn ficheiro_em_ram(nome: &str, conteudo: &[u8]) -> Result<RawFd, ErroMedia> {
    FicheiroEmRam::de_conteudo(nome, conteudo).map(FicheiroEmRam::para_herdar)
}

#[cfg(test)]
mod testes;
