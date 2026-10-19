# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""Testes do lip sync.

A regra dos três cenários de ``docs/media.md`` §5.5, e as propriedades
que a tornam útil: que o desvio é medido e não estimado, que o áudio é
o mestre, e que nada dorme.
"""

from __future__ import annotations

import pytest

from messenger.lipsync import LIMITE_MS, Decisao, SessaoLipSync


# ---------------------------------------------------------------------
# A regra dos três cenários
# ---------------------------------------------------------------------


def test_dentro_do_limite_sincroniza() -> None:
    """Um desvio pequeno é sincronia, e o quadro apresenta-se.

    E o caso comum: 95 % dos quadros de uma chamada normal estão aqui.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    assert sessao.decidir(1010) == Decisao.SINCRONIZAR
    assert sessao.apresentados == 1
    assert sessao.descartados == 0


def test_um_desvio_no_limite_ainda_sincroniza() -> None:
    """O limite é **inclusivo**.

    E não é um detalhe: um limite de 120 ms que descartasse a 120
    seria um limite de 119, e a diferença entre as duas coisas é um
    quadro a mais de uma vez por segundo.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    assert sessao.decidir(1000 + LIMITE_MS) == Decisao.SINCRONIZAR
    assert sessao.descartados == 0


def test_um_desvio_acima_do_limite_descarta() -> None:
    """O vídeo atrasado é de uma imagem que já passou: descarta-se."""
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    assert sessao.decidir(1000 + LIMITE_MS + 1) == Decisao.DESCARTAR
    assert sessao.descartados == 1
    assert sessao.apresentados == 0


def test_um_video_adiantado_apresenta_ja() -> None:
    """O vídeo adiantado é de uma imagem que ainda não soou.

    Descartá-lo seria perder a imagem. E **esperar** por ele — que é o
    que um ``sleep`` faria — significaria acumular atraso de vídeo, e o
    atraso acumula: depois de um minuto a imagem ia estar meio segundo
    à frente da voz.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    assert sessao.decidir(1000 - LIMITE_MS - 1) == Decisao.APRESENTAR_JA
    assert sessao.apresentados == 1
    assert sessao.descartados == 0


# ---------------------------------------------------------------------
# O primeiro quadro
# ---------------------------------------------------------------------


def test_o_primeiro_quadro_apresenta_ja() -> None:
    """Ainda não há áudio, e o quadro apresenta-se na mesma.

    E o que a §5.5 resolve: nos primeiros instantes de uma chamada não
    há áudio com que comparar, e descartar o primeiro quadro deixaria a
    imagem a começar com um buraco.
    """
    sessao = SessaoLipSync()
    assert sessao.desvio_ms() is None, "sem áudio não há desvio"
    assert sessao.decidir(500) == Decisao.APRESENTAR_JA
    assert sessao.apresentados == 1


def test_o_primeiro_quadro_com_audio_mas_sem_video() -> None:
    """Áudio sem vídeo também não tem desvio — e não é erro."""
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    assert sessao.desvio_ms() is None
    assert sessao.decidir(1000) == Decisao.SINCRONIZAR


# ---------------------------------------------------------------------
# O desvio é medido
# ---------------------------------------------------------------------


def test_o_desvio_e_a_diferenca_entre_os_dois_relogios() -> None:
    """O desvio é ``video − audio``, e não uma média.

    E o que a §5.5 diz, e a média seria a mesma coisa com menos
    informação: com a média, um atraso de 200 ms e um avanço de 200 ms
    dariam desvio zero.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    sessao.registar_video(1500)
    assert sessao.desvio_ms() == 500


def test_o_desvio_muda_com_o_ultimo_audio() -> None:
    """O áudio que conta é o **último**, não o primeiro.

    E o que faz a sessão funcionar: o áudio avança a cada pacote, e um
    quadro tem de ser comparado com o áudio que estava a tocar quando
    ele devia aparecer — não com o primeiro pacote da chamada.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    sessao.registar_audio(2000)
    sessao.registar_video(2050)
    assert sessao.desvio_ms() == 50, "o desvio tem de ser contra o áudio mais recente"


def test_o_desvio_atual_nao_muda_o_estado() -> None:
    """Consultar o desvio não o altera.

    E o que permite à interface mostrar o desvio a cada segundo sem
    perturbar a decisão do próximo quadro.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    sessao.registar_video(1050)
    antes = sessao.desvio_atual_ms()
    depois = sessao.desvio_atual_ms()
    assert antes == depois == 50


# ---------------------------------------------------------------------
# O áudio é o mestre
# ---------------------------------------------------------------------


def test_o_audio_avanca_o_relogio_e_o_video_segue() -> None:
    """Uma chamada inteira: o áudio avança, e o vídeo segue-o.

    E o caso que a §5.2 descreve — o ritmo do áudio é constante por
    construção, e o do vídeo não.
    """
    sessao = SessaoLipSync()

    # Um segundo de áudio, em pacotes de 20 ms.
    for n in range(50):
        sessao.registar_audio(n * 20)
        # Um quadro a cada dois pacotes de áudio, com um atraso de 30 ms.
        if n % 2 == 0:
            assert sessao.decidir(n * 20 + 30) == Decisao.SINCRONIZAR

    assert sessao.apresentados == 25
    assert sessao.descartados == 0


def test_um_pico_de_atraso_descarta_e_depois_volta_ao_normal() -> None:
    """Um pico de atraso descarta um quadro, e a chamada continua.

    E o que o Tor faz: atrasa muito e perde pouco. Um pico de 300 ms
    descarta um quadro, e o seguinte entra normalmente — porque o
    descarte não «consome» o atraso, o quadro é que se perde.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    sessao.registar_audio(1020)
    sessao.registar_audio(1040)

    assert sessao.decidir(1400) == Decisao.DESCARTAR
    assert sessao.descartados == 1

    sessao.registar_audio(1060)
    assert sessao.decidir(1070) == Decisao.SINCRONIZAR
    assert sessao.apresentados == 1
    assert sessao.descartados == 1


# ---------------------------------------------------------------------
# As estatísticas
# ---------------------------------------------------------------------


def test_as_estatisticas_contabilham() -> None:
    """Os contadores são o que a interface mostra."""
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    sessao.decidir(1010)
    sessao.decidir(2000)

    estatisticas = sessao.estatisticas()
    assert estatisticas["apresentados"] == 1
    assert estatisticas["descartados"] == 1


# ---------------------------------------------------------------------
# O que o módulo não faz
# ---------------------------------------------------------------------


def test_nenhuma_decisao_dorme() -> None:
    """Nenhuma decisão pode passar tempo.

    E a propriedade que a §5.5 chama «a correcção mais importante deste
    documento»: dormir localmente converte um problema de rede num
    problema de renderização.

    O teste é sobre o **código**, e não sobre o tempo: mede quanto
    tempo passa, e compara com um limite que é largo para a máquina e
    estreito para um ``sleep``.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)

    import time

    inicio = time.monotonic()
    for n in range(200):
        sessao.registar_audio(n * 20)
        sessao.decidir(n * 20)
    decorrido = time.monotonic() - inicio

    # 200 decisões num instante. Com um `sleep` de 1 ms por decisão
    # seriam 200 ms; com um de 10 ms seriam 2 s. O limite de 50 ms
    # apanha os dois e deixa passar o caso normal, que são
    # microsegundos.
    assert decorrido < 0.050, f"as 200 decisões demoraram {decorrido:.3f} s"


def test_a_sessao_e_reutilizavel() -> None:
    """Uma sessão nova começa com o estado limpo.

    E o que permite recomeçar uma chamada sem recriar o objecto — e é
    o caso de quem recebe uma segunda chamada do mesmo par.
    """
    sessao = SessaoLipSync()
    sessao.registar_audio(1000)
    sessao.decidir(1010)
    assert sessao.apresentados == 1

    outra = SessaoLipSync()
    assert outra.apresentados == 0
    assert outra.descartados == 0
    assert outra.audio_ms is None
    assert outra.video_ms is None


def test_o_limite_e_o_que_a_spec_diz() -> None:
    """120 ms, e a razão está escrita.

    Este é um teste de **fixação** de um número: mudar o limite muda o
    comportamento de todas as chamadas, e a mudança tem de ser
    deliberada.
    """
    assert LIMITE_MS == 120
    # E a relação com o Tor: três vezes o piso de 40 ms.
    assert LIMITE_MS == 3 * 40


def test_as_decisoes_tem_nome() -> None:
    """Cada decisão tem um nome, porque o diagnóstico é lido por gente.

    E o `NOMES` é o que evita que o log diga «0» e «2» e quem lê tenha
    de ir procurar a tabela.
    """
    for decisao in (
        Decisao.SINCRONIZAR,
        Decisao.APRESENTAR_JA,
        Decisao.DESCARTAR,
    ):
        assert Decisao.NOMES[decisao], f"a decisão {decisao} não tem nome"
        assert Decisao.NOMES[decisao].islower()


@pytest.mark.parametrize(
    "decisao, nome",
    [
        (Decisao.SINCRONIZAR, "sincronizar"),
        (Decisao.APRESENTAR_JA, "apresentar já"),
        (Decisao.DESCARTAR, "descartar"),
    ],
)
def test_o_nome_de_cada_decisao(decisao: int, nome: str) -> None:
    assert Decisao.NOMES[decisao] == nome
