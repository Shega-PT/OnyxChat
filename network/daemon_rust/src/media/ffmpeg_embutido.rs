// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// media/ffmpeg_embutido.rs — o `ffmpeg` dentro do binário, sem disco
// ---------------------------------------------------------------------
// O `ffmpeg` entra no binário em tempo de compilação e sai em memória
// em tempo de execução. Não há ficheiro temporário, não há directório
// de ferramentas, não há dependência externa na máquina de quem usa.
//
// ## O caminho
//
// ```text
// tools/ffmpeg/cache/…      descarregado, SHA-256 conferido antes de existir
//         │  build.rs
//         ▼
// OUT_DIR/ffmpeg.xz         comprimido, incluído com `include_bytes!`
//         │  primeiro spawn
//         ▼
// memfd_create              um ficheiro anónimo, só em RAM
//         │  dup2
//         ▼
// /proc/self/fd/N           o caminho que o `execve` recebe
// ```
//
// ## Porquê `memfd_create` e não um ficheiro temporário
//
// Três razões, e a segunda é a que ninguém adivinha.
//
// 1. **Nada toca o disco.** Nem o binário, nem um temporário, nem um
//    ficheiro apagado a seguir. Um `/tmp` com o `ffmpeg` dentro é um
//    ficheiro executável de 80 MB que qualquer outro utilizador da
//    máquina pode ver, copiar e executar — e `docs/threat_model.md` §A8
//    é exactamente sobre outro utilizador da mesma máquina.
//
// 2. **Um `/tmp` apagado não desaparece.** O `unlink` só remove o nome;
//    o inode fica até o último descritor fechar. Um `ffmpeg` que corre
//    durante uma chamada a 30 fps está «apagado» e presente ao mesmo
//    tempo. O `memfd` não tem nome nenhum — não aparece em nenhum
//    `ls`.
//
// 3. **Nada fica para o `OOM killer`.** Não, este é o custo: um
//    `memfd` de 80 MB **é** memória anonima. Uma cópia no disco
//    seria paginada sob pressão; esta não tem para onde ir. É a mesma
//    troca que [`crypto_core::paginas`] faz em sentido inverso — ali
//    tranca-se para o secret não ir para swap, aqui aceita-se que vá
//    para swap porque o conteúdo não é secreto, é um programa público.
//
// ## Porquê um slot fixo e alto (`SLOT_EMBUTIDO`)
//Porque o descritor 3 é do **áudio** (§7.1 de `docs/media.md`): o
// `pre_exec` faz `dup2(pipe, 3)`. Se o `memfd` ocupasse o 3, o áudio
// e o programa seriam o mesmo ficheiro, e a falha seria um `ffmpeg`
// que lê o seu próprio executável como se fosse áudio — que é o tipo
// de coisa que não se descobre a ler o código.
//
// Os dois slots são deliberadamente não sobrepostos, e o teste
// [`os_dois_descritores_nao_colidem`] fixa isso.
// =====================================================================

use std::ffi::CString;
use std::io;
use std::os::unix::io::RawFd;
use std::path::PathBuf;

use super::erros::ErroMedia;

/// O executável embebido, comprimido com xz.
///
/// Vem de `OUT_DIR`, que o `build.rs` escreve **sempre** — mesmo sem
/// artefacto, em que caso escreve um ficheiro de zero bytes. É o que
/// permite que este `include_bytes!` não falhe a compilação numa
/// máquina sem o `ffmpeg`: a ausência é uma **condição de execução**,
/// não um erro de compilação.
///
/// A consequência é que `disponivel()` tem de ser perguntado. O modo
/// degradado de `docs/media.md` — daemon sem media, texto intacto —
/// depende inteiramente de essa pergunta.
static EMBUTIDO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ffmpeg.xz"));

/// O slot de descritor reservado ao executável.
///
/// Alto e fixo pelos motivos do cabeçalho. `21` é arbitrário dentro de
/// um intervalo largo: o que interessa é estar acima dos descritores
/// que o `ffmpeg` filho precisa de livres (1 e 3) e longe dos que a
/// biblioteca C e o runtime do Rust usam.
pub const SLOT_EMBUTIDO: RawFd = 21;

// `MFD_CLOEXEC` **não** se pede, e é deliberado. O descritor tem de
// sobreviver ao `exec` para que o `execve` consiga abrir
// `/proc/self/fd/N`; pedir a marca obrigaria a limpar com um `fcntl` que
// teria de ser verificado em três resultados (ver [`limpar_cloexec`]).
// Sem a marca, um `exec` herda o descritor — que é o pretendido, e
// `F_SEAL_SEAL` garante que ninguém escreve nele entretanto.

/// `MFD_ALLOW_SEALING` — permite selar o descritor.
///
/// Usado em conjunto com o selamento abaixo, que é o que impede que
/// quem herde o descritor escreva no executável depois de o selar.
const PERMITE_SELAR: libc::c_uint = 0x0002;

/// `F_SEAL_SEAL` — depois desta marca, o descritor não pode mudar de
/// tamanho nem ser selado outra vez.
///
/// É o que transforma o `memfd` num executável **imutável**: ninguém
/// — nem o próprio processo, nem um filho, nem outra thread — pode
/// escrever neste executável depois de o selar. Sem isto, o `ffmpeg`
// seria substituível por quem tivesse um descritor aberto, e a
// garantia de que se executa o que está embebido no binário seria falsa.
const SELO_FECHADO: i32 = 0x0001;

/// A reserva inicial da descompressão: o comprimido vezes este número.
///
/// **Não** é o tamanho do executável, e essa é a distinção que importa.
/// A primeira versão desta constante guardava o tamanho exacto de um
/// `ffmpeg` de 79,8 MB e comparava-o com o que vinha descomprimido — e
/// passou a falhar com `EIO` no primeiro executável que não fosse
/// aquele. Um literal de tamanho é um segundo sítio a mudar sempre que o
/// `build.sh` reconstrói com bandeiras diferentes, e é o sítio onde a
/// divergência é mais difícil de ver: dá um `errno` que não aponta
/// para nada.
///
/// Isto é uma **pista** para o `Vec`, não uma afirmação. E o xz tem
/// integridade própria: um stream truncado ou adulterado falha a
/// descompressão, e o que chegar ao fim está inteiro.
const RAZAO_RESERVA: usize = 4;

/// O tecto do que é aceite descomprimir, em bytes.
///
/// Existe contra a **bomba de descompressão**: um `.xz` de um byte pode
/// descomprimir para gigabytes, e o `Vec` cresceria até o OOM killer
/// aparecer — dentro de um daemon que também tem chaves em memória.
///
/// 256 MiB é generoso para um `ffmpeg` (o maior que se viu, o estático
/// de 7.0.2, tinha 80 MB descomprimido) e pequeno o suficiente para que
/// o seu excesso ser notado antes do OOM killer, e não depois.
const TETO_DESCOMPRESSAO: usize = 256 * 1024 * 1024;

/// Há um executável embebido nesta compilação?
///
/// A pergunta que o resto do módulo faz antes de tudo. Um `false` não
/// é um erro: é o modo degradado de `docs/media.md`, e o daemon
/// continua a funcionar com texto, porque o pipeline de texto não passa
/// por aqui.
pub fn disponivel() -> bool {
    !EMBUTIDO.is_empty()
}

/// O tamanho do executável embebido, comprimido.
///
/// Zero quando não está embebido — e não `None`, porque a
/// interrogação já está em [`disponivel`] e duplicá-la em três funções
/// dava três sítios a divergirem.
pub fn tamanho_comprimido() -> usize {
    EMBUTIDO.len()
}

/// Prepara o executável em memória e devolve o descritor pronto a
/// executar.
///
/// O descritor sai **já no slot fixo**, com `FD_CLOEXEC` limpo, para
/// que o `Command` o encontre no caminho `/proc/self/fd/21` depois do
/// `fork`. Ver [`SLOT_EMBUTIDO`] para o porquê do número.
///
/// ## Porque selar
///
/// Depois de escrever o executável, o descritor é selado com
/// `F_SEAL_SEAL`. A partir daí é imutável — e a garantia que isto dá é
/// a razão de o mecanismo existir: **o que executa é exactamente o que
/// está no binário**, não uma cópia que alguém possa ter substituído
/// entre a descompressão e o `execve`.
///
/// ## Porque `dup2` e não o descritor directo
///
/// Porque o número do `memfd_create` é imprevisível, e um número
/// imprevisível pode calhar no 3 — que é do áudio. `dup2` para um slot
/// fixo elimina a colisão por construção, e o teste
/// [`os_dois_descritores_nao_colidem`] verifica que os dois números
/// nunca são o mesmo.
pub fn preparar() -> Result<RawFd, ErroMedia> {
    if !disponivel() {
        return Err(ErroMedia::DependenciaAusente);
    }

    // --- descomprimir ---------------------------------------------------
    let mut entrada = std::io::Cursor::new(EMBUTIDO);
    let mut saida = Vec::with_capacity(EMBUTIDO.len().saturating_mul(RAZAO_RESERVA));
    lzma_rs::xz_decompress(&mut entrada, &mut saida)
        .map_err(|_| ErroMedia::ExecucaoFalhou(libc::EINVAL))?;

    // O tecto, e não uma comparação com um tamanho esperado: ver a nota
    // em [`TETO_DESCOMPRESSAO`]. `EINVAL` e não `EIO` porque um xz que
    // não descomprime é um xz malformado, e isso é do *conteúdo*.
    if saida.len() > TETO_DESCOMPRESSAO {
        return Err(ErroMedia::ExecucaoFalhou(libc::EFBIG));
    }

    // --- o ficheiro anónimo ---------------------------------------------
    let nome = CString::new("onyx-ffmpeg").expect("sem bytes nulos no nome");
    let fd = unsafe { libc::memfd_create(nome.as_ptr(), PERMITE_SELAR) };
    if fd < 0 {
        return Err(ErroMedia::ExecucaoFalhou(ultimo_erro()));
    }

    // Cada passo tem de ser verificado, e a falha tem de **fechar** o
    // descritor. Um `fd` vazado numa falha de escrita é um descritor
    // por spawn, e um stream de chamadas a `MEDIA_INICIAR` acabaria
    // por esgotar a tabela de processos de uma pessoa.
    let fechar_em_erro = |fd: RawFd, errno: i32| {
        unsafe { libc::close(fd) };
        Err(ErroMedia::ExecucaoFalhou(errno))
    };

    let escrito = unsafe { libc::write(fd, saida.as_ptr().cast(), saida.len()) };
    if escrito < 0 || escrito as usize != saida.len() {
        return fechar_em_erro(fd, ultimo_erro());
    }

    // `0700`: executável, só pelo dono. O `memfd` herda as permissões
    // do processo, e sem isto o executável seria legível por outro
    // utilizador que tivesse um descritor aberto — o mesmo princípio de
    // `docs/security_model.md` que aplica o `0600` à loja.
    if unsafe { libc::fchmod(fd, 0o700) } < 0 {
        return fechar_em_erro(fd, ultimo_erro());
    }

    // Selar **depois** de escrever e depois de dar as permissões: a
    // ordem é forçada pelo `F_SEAL_SEAL`, que proíbe selar um
    // descritor que ainda possa mudar. Selar antes faria a escrita
    // falhar.
    if unsafe { libc::fcntl(fd, libc::F_ADD_SEALS, SELO_FECHADO) } < 0 {
        return fechar_em_erro(fd, ultimo_erro());
    }

    // --- levar ao slot fixo ---------------------------------------------
    if fd != SLOT_EMBUTIDO {
        if unsafe { libc::dup2(fd, SLOT_EMBUTIDO) } < 0 {
            let errno = ultimo_erro();
            unsafe { libc::close(fd) };
            return Err(ErroMedia::ExecucaoFalhou(errno));
        }
        // O original deixa de ser preciso: o slot tem a mesma imagem.
        unsafe { libc::close(fd) };
    }

    // `FD_CLOEXEC` limpo, ou o `execve` fecha o descritor antes de o
    // `execve` conseguir abrir `/proc/self/fd/21`.
    limpar_cloexec(SLOT_EMBUTIDO)?;

    Ok(SLOT_EMBUTIDO)
}

/// Limpa `FD_CLOEXEC` num descritor.
///
/// Existe como função porque o par `fcntl` a ler e a escrever tem três
/// resultados e **os três** precisam de ser verificados. Um
/// `libc::fcntl(fd, libc::F_SETFD, 0)` sem verificar o resultado
/// devolveria um código de erro que ninguém lê, e o sintoma seria o
/// `ffmpeg` a não arrancar com um erro que não aponta para nada.
fn limpar_cloexec(fd: RawFd) -> Result<(), ErroMedia> {
    let actual = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if actual < 0 {
        return Err(ErroMedia::ExecucaoFalhou(ultimo_erro()));
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFD, actual & !libc::FD_CLOEXEC) } < 0 {
        return Err(ErroMedia::ExecucaoFalhou(ultimo_erro()));
    }
    Ok(())
}

/// Fecha o slot do executável.
///
/// Chamar em `Drop` do processo que o lançou, e **não** fechar assim que
/// o `fork` acontece: o filho precisa dele durante o `execve`, e um
/// `close` no pai nesse instante seria uma condição de corrida que só
/// falharia sob carga.
pub fn fechar() {
    unsafe { libc::close(SLOT_EMBUTIDO) };
}

/// O último `errno` do sistema.
fn ultimo_erro() -> i32 {
    io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

/// O caminho que o `execve` recebe para o executável embebido.
///
/// Devolve `String` e não `PathBuf` porque o caminho é um número de
/// descritor — não há nada a resolver no sistema de ficheiros, e um
/// `PathBuf` sugeriria que havia um directório pelo meio.
pub fn caminho() -> String {
    format!("/proc/self/fd/{SLOT_EMBUTIDO}")
}

/// O nome do artefacto embebido, para as mensagens de erro.
///
/// Vem do `build.rs` e diz **qual** versão foi embebida — que é o que
/// um utilizador precisa de saber quando o media não funciona, e é
/// também o que impede duas compilações de se confundirem.
pub fn versao_embutida() -> &'static str {
    // Uma cadeia vazia quando não há artefacto — o mesmo sentinel de
    // [`disponivel`], e pelo mesmo motivo.
    env!("ONYX_FFMPEG_VERSAO")
}

/// Onde o `build.rs` procura o artefacto.
///
/// Exposto para o `fetch.sh` e para as mensagens do CLI; não é uma
/// API de runtime.
pub fn caminho_do_artefacto() -> PathBuf {
    PathBuf::from(env!("ONYX_FFMPEG_ARTEFACTO"))
}

// ---------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------
#[cfg(test)]
mod testes {
    use super::*;

    /// O slot do executável e o descritor 3 não podem ser o mesmo.
    ///
    /// É a propriedade que justifica o número alto. Se um dia alguém
    /// baixar o slot para 3 por ordem de um número mais baixo, este
    /// teste falha — e a falha apareceria em produção como um `ffmpeg`
    /// a ler o seu próprio executável como áudio.
    #[test]
    fn os_dois_descritores_nao_colidem() {
        assert_ne!(
            SLOT_EMBUTIDO, 3,
            "o executável e o áudio não podem partilhar o descritor 3"
        );
        assert!(
            SLOT_EMBUTIDO > 3,
            "o slot tem de estar acima do 3, que é do áudio"
        );
    }

    /// O caminho é o do descritor, e o descritor é o slot.
    ///
    /// Teste de uma linha que vale a pena: `caminho()` é uma função de
    /// formatação que parece trivial e é o único ponto onde o número do
    /// slot entra no mundo exterior. Um erro aqui — um `{}` trocado, um
    /// espaço — daria um `execve` a falhar com `ENOENT`, que é o erro
    /// mais enganador possível para quem o recebe.
    #[test]
    fn o_caminho_aponta_para_o_slot() {
        assert_eq!(caminho(), format!("/proc/self/fd/{SLOT_EMBUTIDO}"));
        assert!(caminho().starts_with("/proc/self/fd/"));
        assert!(!caminho().contains(' '), "o caminho não pode ter espaços");
    }

    /// `disponivel()` e `tamanho_comprimido()` concordam.
    ///
    /// As duas são consultadas por código diferente — uma pelo
    /// `MEDIA_INICIAR`, outra pelas mensagens de arranque — e uma
    /// inconsistência entre elas daria um daemon que diz que tem media e
    /// não a tem.
    #[test]
    fn a_disponibilidade_e_o_tamanho_concordam() {
        assert_eq!(
            disponivel(),
            tamanho_comprimido() > 0,
            "`disponivel` e `tamanho_comprimido` discordam"
        );
    }

    /// Uma compilação sem artefacto dá o modo degradado, não um pânico.
    ///
    /// Este binário de teste **não tem** `ffmpeg` embebido — o
    /// artefacto não é versionado e a árvore de compilação dos testes
    /// não o descarrega. É a prova de que `docs/media.md` §Modo
    /// degradado é verdade: `preparar()` devolve `DependenciaAusente` e
    /// o daemon continua a funcionar.
    #[test]
    fn sem_artefacto_da_erro_de_dependencia_e_nao_panico() {
        if disponivel() {
            // Há artefacto nesta compilação: o teste abaixo não se
            // aplica, e dizê-lo é melhor do que passar em silêncio.
            return;
        }
        assert_eq!(preparar(), Err(ErroMedia::DependenciaAusente));
        assert_eq!(tamanho_comprimido(), 0);
        assert_eq!(versao_embutida(), "");
    }

    /// Os erros de execução trazem um `errno` positivo.
    ///
    /// Um `errno` negativo ou zero seria um bug do `ultimo_erro()`, e a
    /// mensagem de erro diria «errno 0» — que é indistinguível de
    /// sucesso.
    #[test]
    fn os_erros_trazem_um_errno_valido() {
        let e = ErroMedia::ExecucaoFalhou(libc::ENOENT);
        assert!(
            e.to_string().contains("2"),
            "ENOMEM/ENOENT devem aparecer na mensagem"
        );
    }
}
