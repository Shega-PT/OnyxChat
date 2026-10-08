// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// paginas.rs — impede que material sensível seja descartado para swap
// ---------------------------------------------------------------------
// Porquê este módulo existe
//
// `Segredo` (ver `secret.rs`) garante que uma chave é apagada da memória
// quando é destruída. Essa garantia pressupõe que a página **ainda está
// em RAM**.
//
// Quando o kernel precisa de memória, pode escrever páginas anónimas
// (heap, `mmap`, pilha) numa área de swap. Se essa página contiver uma
// chave, a chave passa a existir fora do processo — num dispositivo
// persistente que a `zeroize` não toca. A promessa «a memória sensível
// é limpa» deixa de ser verdadeira.
//
// `mlockall(MCL_CURRENT | MCL_FUTURE)` fecha essa janela: o kernel passa
// a falhar a alocação de memória em vez de trocar, e as páginas de
// material sensível nunca saem de RAM enquanto o processo viver.
//
// ---------------------------------------------------------------------
// Decisões de projecto
//
// 1. **É uma operação de processo, não de segredo.** Trancar página a
//    página por chave seria subjectivo (dependeria do momento exacto de
//    alocação) e despejaria tranças órfãs quando um segredo morresse.
//    Tranca-se o processo inteiro uma vez, no arranque.
//
// 2. **Falha nunca impede o arranque.** Perder a protecção contra swap
//    é grave; não arrancar é pior. Se `RLIMIT_MEMLOCK` for insuficiente,
//    o daemon arranca, regista um aviso em stderr e prossegue. O estado
//    fica consultável para o resto do processo.
//
// 3. **Não é garantia de nada contra `root`.** `mlockall` protege
//    contra page-out. Não protege contra acesso directo à RAM, forense
//    de disco nem *cold-boot*. Ver `docs/threat_model.md` §A10.
//
// Ver `docs/key_management.md` §Exposição a swap e page-out.
// =====================================================================

use std::fmt;

/// Resultado de [`trancar_paginas`] — porque é que o tranco foi ou não
/// conseguido.
///
/// É `Copy` e trivialmente comparável para que o arranque do daemon possa
/// registar o estado sem molecular um resultado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoPaginas {
    /// As páginas do processo estão trancadas em RAM. As atribuições
    /// futuras também serão (`MCL_CURRENT | MCL_FUTURE`).
    Trancadas,
    /// O tranco **não** foi aplicado. O processo arranca normalmente,
    /// mas o material sensível pode ser descartado para swap.
    ///
    /// [`EstadoPaginas::motivo`] diz porquê, para que o aviso seja
    /// accionável em vez de genérico.
    Desbloqueadas(MotivoFalha),
}

/// Motivo pelo qual `mlockall` não foi aplicado.
///
/// Variante em vez de string livre: o daemon tem de conseguir distinguir
/// «limite de memória» de «plataforma sem suporte», porque as
/// mitigações são diferentes (a primeira resolve-se com `ulimit`, a
/// segunda não tem solução em código).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoFalha {
    /// `RLIMIT_MEMLOCK` insuficiente: o limite de memória trancada foi
    /// excedido. Mitigação: `ulimit -l`, ou
    /// `prlimit --memlock=unlimited`.
    LimiteMemlock,
    /// Erro do sistema sem uma das causas acima (o `errno` fica
    /// disponível em [`MotivoFalha::errno`]).
    Outro(i32),
}

impl MotivoFalha {
    /// `errno` original da chamada, quando aplicável.
    pub fn errno(self) -> Option<i32> {
        match self {
            MotivoFalha::LimiteMemlock => None,
            MotivoFalha::Outro(codigo) => Some(codigo),
        }
    }

    /// Classifica um `errno` da chamada `mlockall`.
    fn classificar(codigo: i32) -> Self {
        // ENOMEM: o limite de páginas trancadas foi excedido.
        // EPERM/EACCES: o limite não pode ser ultrapassado por este
        // utilizador.
        // Os dois são, para o efeito, "o limite não deixou trancar".
        match codigo {
            libc::ENOMEM | libc::EPERM | libc::EACCES => MotivoFalha::LimiteMemlock,
            outro => MotivoFalha::Outro(outro),
        }
    }
}

impl fmt::Display for MotivoFalha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MotivoFalha::LimiteMemlock => {
                write!(f, "RLIMIT_MEMLOCK insuficiente — `ulimit -l unlimited`")
            }
            MotivoFalha::Outro(codigo) => write!(f, "mlockall falhou (errno {codigo})"),
        }
    }
}

impl fmt::Display for EstadoPaginas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EstadoPaginas::Trancadas => write!(f, "páginas trancadas em RAM (mlockall)"),
            EstadoPaginas::Desbloqueadas(MotivoFalha::LimiteMemlock) => write!(
                f,
                "páginas NÃO trancadas: RLIMIT_MEMLOCK insuficiente (ulimit -l)"
            ),
            EstadoPaginas::Desbloqueadas(MotivoFalha::Outro(codigo)) => {
                write!(f, "páginas NÃO trancadas: mlockall falhou (errno {codigo})")
            }
        }
    }
}

// Guarda o estado observado na última chamada a [`trancar_paginas`].
//
// `AtomicU8` em vez de `Mutex`: a leitura é feita uma vez no arranque e
// nunca escrita depois, logo não há contenção. O valor é o discriminante
// de `EstadoPaginas` (0 = trancado, 1 = desbloqueado) — guardado como
// `u8` para caber num atómico sem alocação.
static ESTADO: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

/// `errno` da última falha, para diagnostico. `0` quando não houve falha.
static ULTIMO_ERRNO: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Tranca as páginas do processo em RAM e tranca as atribuições futuras.
///
/// Idempotente e segura para chamar várias vezes. Devolve o estado
/// observado — **nunca** um `Err` que o arranque tenha de tratar: a
/// decisão de continuar é do chamador (ver `docs/threat_model.md`
/// §Requisitos resultantes, ponto 6).
///
/// # Exemplo
///
/// ```no_run
/// use crypto_core::paginas::{EstadoPaginas, trancar_paginas};
///
/// match trancar_paginas() {
///     EstadoPaginas::Trancadas => eprintln!("mlock: activo"),
///     EstadoPaginas::Desbloqueadas(motivo) => {
///         eprintln!("mlock: inactivo ({motivo}) — as chaves podem ir para swap");
///     }
/// }
/// ```
pub fn trancar_paginas() -> EstadoPaginas {
    // `MCL_CURRENT` tranca o que já está mapeado; `MCL_FUTURE` manda o
    // kernel trancar o que for mapeado a partir de agora. Os dois são
    // necessários: sem `MCL_CURRENT` as chaves já existentes ficariam
    // desprotegidas; sem `MCL_FUTURE`, as chaves criadas depois escapariam.
    let resultado = unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE) };

    if resultado == 0 {
        ESTADO.store(0, std::sync::atomic::Ordering::Release);
        ULTIMO_ERRNO.store(0, std::sync::atomic::Ordering::Release);
        return EstadoPaginas::Trancadas;
    }

    // Falhou: o `errno` tem de ser lido **imediatamente**, antes de
    // qualquer outra chamada libc (qualquer uma o pode sobrescrever).
    let codigo = std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO);
    let motivo = MotivoFalha::classificar(codigo);
    ESTADO.store(1, std::sync::atomic::Ordering::Release);
    ULTIMO_ERRNO.store(codigo, std::sync::atomic::Ordering::Release);
    EstadoPaginas::Desbloqueadas(motivo)
}

/// Estado das páginas sem voltar a chamar `mlockall`.
///
/// Antes de qualquer [`trancar_paginas`], devolve
/// [`EstadoPaginas::Desbloqueadas`] com um motivo indicativo de que
/// ninguém perguntou — que é a verdade: o estado é desconhecido, não
/// «desbloqueado».
pub fn estado_paginas() -> EstadoPaginas {
    if ESTADO.load(std::sync::atomic::Ordering::Acquire) == 0 {
        return EstadoPaginas::Trancadas;
    }
    let codigo = ULTIMO_ERRNO.load(std::sync::atomic::Ordering::Acquire);
    if codigo == 0 {
        // Nunca houve tentativa: não é uma falha, é ausência de informação.
        EstadoPaginas::Desbloqueadas(MotivoFalha::Outro(0))
    } else {
        EstadoPaginas::Desbloqueadas(MotivoFalha::classificar(codigo))
    }
}

/// Liberta o tranco (usado no encerramento e nos testes).
///
/// Numa aplicação que trata chaves, o tranco só deveria ser libertado
/// quando já não restam segredos — daí ser explícito e não implícito.
pub fn destrancar_paginas() {
    unsafe {
        libc::munlockall();
    }
    ESTADO.store(1, std::sync::atomic::Ordering::Release);
    ULTIMO_ERRNO.store(0, std::sync::atomic::Ordering::Release);
}

/// `RLIMIT_MEMLOCK` actual, em bytes.
///
/// Exposto para o arranque poder registar o limite actual no aviso de
/// falha — um valor concreto é mais accionável do que «insuficiente».
pub fn limite_memlock() -> Option<u64> {
    let mut limite: libc::rlimit = unsafe { std::mem::zeroed() };
    // SAFETY: `limite` é um `rlimit` válido e `getrlimit` escreve nele
    // como `out`. A estrutura é `Copy`-compatível e não tem referências.
    let ok = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut limite) } == 0;
    if !ok {
        return None;
    }
    // `rlim_t` é `u64` no Linux; emite-se a largura de forma explícita
    // para não depender do alvo.
    let actual: u64 = limite.rlim_cur;
    let maximo: u64 = limite.rlim_max;
    // RLIM_INFINITY lê-se como o valor máximo da largura.
    Some(if actual == u64::MAX {
        maximo
    } else {
        actual
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lê o `RLIMIT_MEMLOCK` actual.
    fn limite_actual() -> libc::rlimit {
        let mut limite: libc::rlimit = unsafe { std::mem::zeroed() };
        // SAFETY: `limite` é um `rlimit` válido escrito como `out`.
        let ok = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut limite) } == 0;
        assert!(ok, "getrlimit tem de funcionar neste sistema");
        limite
    }

    /// Guarda o `RLIMIT_MEMLOCK` e baixa-o para `bytes`.
    ///
    /// Devolve o valor original para ser restaurado. Com um limite muito
    /// baixo, `mlockall` **tem** de falhar — é a única forma de
    /// exercitar o caminho de degradação de forma determinística, em vez
    /// de esperar que a máquina onde os testes correm já tenha pouca
    /// memória.
    ///
    /// Devolve `None` se o `setrlimit` for recusado (limite hard baixo,
    /// o que acontece sem privilégios): nesse caso o teste não pode
    /// forçar a falha e tem de o declarar em vez de fingir.
    fn com_limite_memlock( bytes: libc::rlim_t) -> Option<libc::rlimit> {
        let original = limite_actual();
        let restrito = libc::rlimit {
            rlim_cur: bytes,
            rlim_max: original.rlim_max,
        };
        let aplicado = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &restrito) };
        if aplicado != 0 {
            return None;
        }
        Some(original)
    }

    /// Restaura o `RLIMIT_MEMLOCK` guardado por [`com_limite_memlock`].
    fn restaurar_limite(original: libc::rlimit) {
        let ok = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &original) };
        assert_eq!(ok, 0, "tem de ser possível restaurar RLIMIT_MEMLOCK");
    }

    // -----------------------------------------------------------------
    // Caminho feliz
    // -----------------------------------------------------------------

    /// Com memória suficiente, `mlockall` funciona e é idempotente:
    /// chamar duas vezes não pode degradar o estado.
    ///
    /// Não é `#[should_panic]`: os dois resultados são comportamento
    /// correcto, porque uma máquina com `RLIMIT_MEMLOCK` baixo é
    //  legítima. A asserção é sobre **coerência**, não sobre o valor.
    #[test]
    fn trancar_paginas_e_idempotente() {
        let primeiro = trancar_paginas();
        assert_eq!(
            estado_paginas(),
            primeiro,
            "o estado registado tem de coincidir com o devolvido"
        );

        let segundo = trancar_paginas();
        assert_eq!(
            estado_paginas(),
            segundo,
            "mlockall é idempotente: a segunda chamada não altera o estado"
        );

        destrancar_paginas();
        assert_eq!(
            estado_paginas(),
            EstadoPaginas::Desbloqueadas(MotivoFalha::Outro(0)),
            "destrancar tem de repor «estado desconhecido», não uma falha"
        );
    }

    // -----------------------------------------------------------------
    // Caminho de degradação (forçado, não dependente da máquina)
    // -----------------------------------------------------------------

    /// **O caminho de degradação tem de existir e ser accionável.** Com
    /// `RLIMIT_MEMLOCK = 0`, `mlockall` não pode trancar nada: tem de
    /// falhar, ser classificado como limite, e continuar utilizável.
    ///
    /// Este é o teste que garante que uma regressão que fizesse
    /// `trancar_paginas` devolver sempre `Trancadas` **nunca passaria**
    /// em qualquer máquina.
    #[test]
    fn limite_zero_faz_a_trancar_falhar() {
        let Some(original) = com_limite_memlock(0) else {
            // Sem permissão para baixar o limite: o teste declara-o em vez
            // de passar em falso.
            panic!(
                "RLIMIT_MEMLOCK não pode ser baixado para 0 — o caminho de \
                 degradação fica por verificar neste ambiente"
            );
        };

        let estado = trancar_paginas();

        restaurar_limite(original);

        // O `errno` tem de ter sido classificado como limite, não opaco.
        assert_eq!(
            estado,
            EstadoPaginas::Desbloqueadas(MotivoFalha::LimiteMemlock),
            "com RLIMIT_MEMLOCK=0 a falha tem de ser classificada como limite"
        );
        // E o estado consultado tem de reflectir a mesma falha — sem
        // isto, um `estado_paginas()` que devolvesse sempre
        // `Trancadas` passaria despercebida.
        assert_eq!(
            estado_paginas(),
            EstadoPaginas::Desbloqueadas(MotivoFalha::LimiteMemlock),
            "estado_paginas tem de reler a falha de mlockall"
        );

        destrancar_paginas();
    }

    /// Mesmo forçando a falha, o processo tem de continuar utilizável:
    /// é degradação graciosa, não abortar.
    #[test]
    fn falha_no_tranco_nao_derruba_o_uso_de_segredos() {
        let Some(original) = com_limite_memlock(0) else {
            panic!("RLIMIT_MEMLOCK não pode ser baixado — degradação por verificar");
        };

        let _ = trancar_paginas();

        // Com as páginas por trancar, o `Segredo` tem de continuar a
        // funcionar normalmente — é isto que «degradação graciosa» quer
        // dizer na prática: perde-se a protecção, não a funcionalidade.
        let mut segredo = crate::secret::Segredo::novo([0xA5; crate::secret::TAMANHO_CHAVE]);
        assert_eq!(
            segredo.como_bytes(),
            &[0xA5; crate::secret::TAMANHO_CHAVE],
            "um segredo tem de ser utilizável sem mlock"
        );
        segredo.zerar();
        assert!(segredo.esta_apagado(), "e tem de continuar a ser apagável");

        restaurar_limite(original);
        destrancar_paginas();
    }

    // -----------------------------------------------------------------
    // Diagnóstico
    // -----------------------------------------------------------------

    /// `Display` tem de ser accionável: o arranque escreve-o em stderr,
    /// e um aviso genérico não ajuda ninguém a resolver o problema.
    #[test]
    fn display_e_acionavel() {
        let trancadas = EstadoPaginas::Trancadas.to_string();
        assert!(trancadas.contains("trancadas"), "falta dizer o que fez: {trancadas}");
        assert!(trancadas.contains("mlockall"), "tem de citar a chamada: {trancadas}");

        let limite = EstadoPaginas::Desbloqueadas(MotivoFalha::LimiteMemlock).to_string();
        assert!(limite.contains("NÃO"), "tem de salientar que falhou: {limite}");
        assert!(limite.contains("ulimit"), "tem de dizer como resolver: {limite}");

        let errno = EstadoPaginas::Desbloqueadas(MotivoFalha::Outro(libc::EIO)).to_string();
        assert!(errno.contains("errno"), "tem de indicar o errno: {errno}");
        assert!(errno.contains(&libc::EIO.to_string()), "tem de dar o valor: {errno}");
    }

    /// A classificação de `errno` distingue dois casos com mitigações
    /// diferentes: um limite de memória resolve-se com `ulimit`, um erro
    /// desconhecido não.
    #[test]
    fn classificar_errno_distingue_limite_de_erro() {
        assert_eq!(MotivoFalha::classificar(libc::ENOMEM), MotivoFalha::LimiteMemlock);
        assert_eq!(MotivoFalha::classificar(libc::EPERM), MotivoFalha::LimiteMemlock);
        assert_eq!(MotivoFalha::classificar(libc::EACCES), MotivoFalha::LimiteMemlock);
        assert_eq!(
            MotivoFalha::classificar(libc::EINVAL),
            MotivoFalha::Outro(libc::EINVAL)
        );
    }

    /// `errno()` só é `Some` num erro não classificado — é o que permite
    /// ao arranque mostrar o número em vez de um texto fixo.
    #[test]
    fn errno_presente_só_quando_aplicavel() {
        assert_eq!(MotivoFalha::LimiteMemlock.errno(), None);
        assert_eq!(MotivoFalha::Outro(42).errno(), Some(42));
    }

    /// `limite_memlock` tem de devolver um valor utilizável: o arranque
    /// usa-o no aviso.
    #[test]
    fn limite_memlock_e_plausivel() {
        // Numa máquina com `RLIMIT_MEMLOCK` infinito, o limite devolvido
        // tem de ser o valor máximo, não `u64::MAX` cru — é o que o
        // arranque mostra a quem tem de o corrigir.
        match limite_memlock() {
            Some(bytes) => {
                assert!(bytes > 0, "limite de zero não é um limite real: {bytes}");
                assert!(bytes < 1 << 40, "limite de {bytes} bytes é implausível");
                assert_ne!(
                    bytes,
                    u64::MAX,
                    "um limite infinito tem de ser reportado como valor máximo, não como u64::MAX"
                );
            }
            None => {
                // `getrlimit` falhou: aceitável em contentores
                // apertados. O aviso de `main.rs` trata o `None`.
            }
        }
    }
}
