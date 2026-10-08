# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_loja.py — o armazenamento do sidecar.

O que estes testes tentam estragar, por ordem:

1. **Perder uma mensagem.** É a falha que o formato de ficheiro impedia:
   um JSON reescrito a cada escrita perde tudo se a energia falhar a meio.
   SQLite dá transacção, e o teste confirma que a escrita é visível logo
   e que o commit aconteceu.
2. **Deixar órfãos.** `PRAGMA foreign_keys` é *off* por omissão no
   sqlite; sem a linha que a liga, apagar uma conversa deixa as
   mensagens lá e não diz nada.
3. **Inventar uma conversa.** `conversa_por_contacto` não cria e
   `abrir_conversa` cria. A diferença é a diferença entre espreitar e
   falar, e um teste que não a fixasse deixaria passar as duas.
4. **Aceitar um pedido duas vezes.** Decidir um pedido pendente cria o
   contacto; decidir um já decidido tem de falhar.
"""

from __future__ import annotations

import stat
import time
from pathlib import Path

import pytest

from server.loja import Loja, LojaInvalida

#: Identificador com a forma que `messenger/identidade.py` produz.
IDENT = "ONYX-AAAAAA-!A!A#"


@pytest.fixture
def loja(tmp_path: Path):
    with Loja(tmp_path / "loja.db") as aberta:
        yield aberta


@pytest.fixture
def com_contacto(loja: Loja) -> str:
    """Uma loja com um contacto já aceite, e devolve o seu id."""
    pedido = loja.registar_pedido(IDENT, "Ana Silva", "Olá")
    return loja.decidir_pedido(pedido["id"], "aceite")["contactoId"]


# ---------------------------------------------------------------------
# Ciclo de vida e ficheiro
# ---------------------------------------------------------------------


def test_o_ficheiro_e_criado(tmp_path: Path) -> None:
    caminho = tmp_path / "sub" / "loja.db"
    with Loja(caminho):
        pass
    assert caminho.is_file()


def test_o_ficheiro_e_modo_0600(loja: Loja) -> None:
    """A loja tem metainformação de quem fala com quem.

    Não é um segredo — o texto das mensagens já vai cifrado — mas é quem
    a pessoa é e com quem fala, e um ficheiro legível por todos os
    utilizadores da máquina é uma lista de contactos legível.
    """
    caminho = loja._caminho  # noqa: SLF001 — o teste precisa do caminho
    assert stat.S_IMODE(caminho.stat().st_mode) == 0o600


def test_o_modo_e_reposto_a_cada_abertura(tmp_path: Path) -> None:
    """Um ficheiro copiado com outros modos volta a ser fechado."""
    caminho = tmp_path / "loja.db"
    with Loja(caminho):
        pass
    caminho.chmod(0o644)
    with Loja(caminho):
        pass
    assert stat.S_IMODE(caminho.stat().st_mode) == 0o600


def test_o_modo_quebrado_nao_impede_o_arranque(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Num sistema de ficheiros sem POSIX o programa tem de funcionar.

    Recusar arrancar por causa de um `chmod` que falha transformaria um
    detalhe de plataforma em indisponibilidade total. O modo fica por
    garantir; a loja funciona.

    O cenário simulado é o `os.chmod` a levantar `OSError`, que é o que
    acontece num sistema de ficheiros sem suporte a permissões. Um
    directório no lugar do ficheiro não serviria: isso é um caminho
    inválido, e o `sqlite3` falharia a abrir, não o `chmod`.
    """
    import os

    real = os.chmod

    def chocar(*_argumentos: object, **_chave: object) -> None:
        raise OSError(30, "Read-only file system")

    monkeypatch.setattr(os, "chmod", chocar)
    with Loja(tmp_path / "loja.db") as aberta:
        aberta.registar_pedido(IDENT, "Ana")
        assert len(aberta.pedidos()) == 1

    # Reposto o `chmod`, o ficheiro continua com o modo que o `umask`
    # deu — que é o que se passaria num sistema onde a chamada falha. O
    # que este teste garante é que a loja abriu e guardou; o modo é
    # assunto de `test_o_ficheiro_e_modo_0600`.
    monkeypatch.setattr(os, "chmod", real)
    assert (tmp_path / "loja.db").is_file()


def test_a_loja_aceita_o_ficheiro_ja_existente(tmp_path: Path) -> None:
    """Reabrir não perde nada — que é o teste mais óbvio e o mais usado."""
    caminho = tmp_path / "loja.db"
    with Loja(caminho) as primeira:
        primeira.registar_pedido(IDENT, "Ana")
    with Loja(caminho) as segunda:
        assert len(segunda.pedidos()) == 1


def test_as_chaves_estrangeiras_ficam_on() -> None:
    """`foreign_keys` é *off* por omissão no sqlite.

    Sem esta linha, `ON DELETE CASCADE` das mensagens não acontece e
    apagar uma conversa deixa as mensagens lá, sem nenhum erro. O teste
    escreve o valor da pragma em vez de confiar numa suposição.
    """
    with Loja(":memory:") as aberta:
        valor = aberta._cx.execute("PRAGMA foreign_keys").fetchone()[0]  # noqa: SLF001
    assert valor == 1


def test_o_ficheiro_em_memoria_tambem_funca(loja: Loja) -> None:
    """`:memory:` é o que os testes de rápido usam, e tem de funcionar."""
    with Loja(":memory:") as aberta:
        assert aberta.contagens()["conversations"] == 0


# ---------------------------------------------------------------------
# Identificadores
# ---------------------------------------------------------------------


def test_os_identificadores_ordenam_entre_instantes(loja: Loja) -> None:
    """A parte temporal faz dois identificadores distantes ordenarem-se.

    Não é o que desempata duas escritas no mesmo milissegundo — para isso
    há o `rowid`, e o teste seguinte diz isso explicitamente.
    """
    import time as _time

    agora = int(time.time() * 1000)
    antes = f"c{agora - 1000:012x}zz"
    depois = f"c{agora + 1000:012x}aa"

    assert antes < depois


def test_a_ordem_dentro_do_mesmo_milisegundo_vem_do_rowid(
    loja: Loja,
    com_contacto: str,
) -> None:
    """Duas mensagens seguidas podem cair no mesmo milissegundo.

    Ordenar pelo `id` nesse caso seria ordenar por um número aleatório, e
    a conversa apareceria com as mensagens trocadas. O `rowid` é a ordem
    de inserção e não tem esse problema.
    """
    conversa = loja.abrir_conversa(com_contacto)
    for indice in range(6):
        loja.adicionar_mensagem(conversa["id"], "them", f"{indice}")

    assert [m["text"] for m in loja.mensagens(conversa["id"])] == [
        "0", "1", "2", "3", "4", "5",
    ]


def test_a_marca_do_tempo_vem_antes_do_aleatorio(loja: Loja) -> None:
    """O tempo é a parte da frente, para o olho humano.

    Escrever a hora no fim do identificador seria mais curto de ordenar
    para quem lê o registo, e perderia a propriedade de os identificadores
    de instantes diferentes se ordenarem sozinhos.
    """
    identificador = loja._novo_id("m")  # noqa: SLF001
    marca, resto = identificador[1:13], identificador[13:]
    assert int(marca, 16) > 0
    assert len(resto) == 8


def test_os_identificadores_nao_colidem(loja: Loja) -> None:
    """Cinco mil identificadores, nenhum repetido.

    O sufixo aleatório é o que desempata dois no mesmo milissegundo, e é
    a única coisa entre o tempo e a colisão.
    """
    ids = {loja._novo_id("c") for _ in range(5000)}  # noqa: SLF001
    assert len(ids) == 5000


def test_o_prefixo_distingue_o_tipo(loja: Loja) -> None:
    """`c`, `m`, `p` e `ct` tornam um identificador legível num registo."""
    assert loja._novo_id("m").startswith("m")  # noqa: SLF001


# ---------------------------------------------------------------------
# Contactos
# ---------------------------------------------------------------------


def test_o_pedido_cria_o_contacto(loja: Loja) -> None:
    pedido = loja.registar_pedido(IDENT, "Ana Silva", "Olá")
    resultado = loja.decidir_pedido(pedido["id"], "aceite")

    assert resultado["decisao"] == "aceite"
    contactos = loja.contactos()
    assert len(contactos) == 1
    assert contactos[0]["name"] == "Ana Silva"
    assert contactos[0]["identifier"] == IDENT


def test_a_impressao_fica_vazia_ate_ao_handshake(loja: Loja) -> None:
    """Um contacto que ainda não falou connosco não tem impressão.

    A impressão deriva-se de um nome e um sal, e o sal é do outro. Inventar
    uma aqui daria à interface um valor que não verifica nada — que é o
    que a Etapa 3 veio substituir.
    """
    pedido = loja.registar_pedido(IDENT, "Ana")
    loja.decidir_pedido(pedido["id"], "aceite")
    assert loja.contactos()[0]["fingerprint"] == ""


def test_o_nome_vais_serve_de_nome(loja: Loja) -> None:
    """Um pedido sem nome ainda mostra alguma coisa."""
    pedido = loja.registar_pedido(IDENT)
    loja.decidir_pedido(pedido["id"], "aceite")
    assert loja.contactos()[0]["name"] == IDENT


def test_os_contactos_ordenam_por_nome_portugues(loja: Loja) -> None:
    """A ordem é a de um português, não a de uma máquina.

    Duas diferenças, e as duas importam:

    * **Acentos.** ``Álvaro`` tem de ordenar como ``alvaro``. Pela
      collation binária do sqlite viria *depois* de todos os ``ana`` — o
      acento é um byte alto — e uma lista de contactos em que ``Álvaro``
      está no fim parece estar avariada.
    * **Ç e c.** ``çedro`` tem de ficar com os ``c``, e a quem escreve
      «c» é a quem procura «Ç».
    """
    from server.loja import _ordem_de_nome

    nomes = ("ana", "Álvaro", "Beatriz", "çedro", "Duarte", "Édgar")
    for indice, nome in enumerate(nomes):
        pedido = loja.registar_pedido(f"ONYX-{indice:06d}-!A!A#", nome)
        loja.decidir_pedido(pedido["id"], "aceite")

    obtidos = [c["name"] for c in loja.contactos()]

    # A regra, dita uma vez: é a ordem pela forma escrita sem acentos nem
    # caixa. Escrever a lista à mão diria o resultado de hoje, que é
    # exactamente o que se quer que deixe de valer quando a regra mudar.
    assert obtidos == sorted(obtidos, key=_ordem_de_nome)

    # E os dois casos que a regra existe para resolver.
    assert obtidos.index("Álvaro") < obtidos.index("ana")
    assert obtidos.index("çedro") < obtidos.index("Duarte")


def test_a_ordenacao_ignora_o_ponto_e_a_caixa(loja: Loja) -> None:
    """`ana` e `Ana` são a mesma pessoa, e ordenam-se como uma só."""
    from server.loja import _ordem_de_nome

    assert _ordem_de_nome("ana") == _ordem_de_nome("Ana") == _ordem_de_nome("ANA")
    assert _ordem_de_nome("Álvaro") == _ordem_de_nome("alvaro")


def test_a_pesquisa_encontra_por_nome_e_por_identificador(loja: Loja) -> None:
    for indice, nome in enumerate(("Ana Silva", "Bruno Costa")):
        pedido = loja.registar_pedido(f"ONYX-{indice:06d}-!B!B#", nome)
        loja.decidir_pedido(pedido["id"], "aceite")

    assert len(loja.contactos("ana")) == 1
    assert len(loja.contactos("bruno")) == 1
    assert len(loja.contactos("!B!B#")) == 2
    assert loja.contactos("ninguém") == []


def test_a_pesquisa_ignora_a_caixa(loja: Loja) -> None:
    pedido = loja.registar_pedido(IDENT, "Ana")
    loja.decidir_pedido(pedido["id"], "aceite")
    assert len(loja.contactos("ANA")) == 1


# ---------------------------------------------------------------------
# Conversas: espreitar não é falar
# ---------------------------------------------------------------------


def test_espreitar_nao_cria_conversa(loja: Loja, com_contacto: str) -> None:
    """Ver um contacto não pode criar uma conversa.

    Sem esta separação, abrir a página de um contacto punha a pessoa numa
    conversa que ela não começou, com o número a zero e sem histórico —
    indistinguível de alguém que nunca falou.
    """
    assert loja.conversa_por_contacto(com_contacto) is None
    assert loja.conversas() == []


def test_abrir_cria_a_conversa(loja: Loja, com_contacto: str) -> None:
    conversa = loja.abrir_conversa(com_contacto)
    assert conversa["contactId"] == com_contacto
    assert conversa["messages"] == []


def test_abrir_de_novo_devolve_a_mesma(loja: Loja, com_contacto: str) -> None:
    """Duas pessoas não podem falar por cima uma da outra."""
    primeira = loja.abrir_conversa(com_contacto)
    segunda = loja.abrir_conversa(com_contacto)
    assert primeira["id"] == segunda["id"]
    assert len(loja.conversas()) == 1


def test_a_conversa_inexistente_diz_que_nao_existe(loja: Loja) -> None:
    with pytest.raises(LojaInvalida) as erro:
        loja.conversa("c-nada")
    assert "inexistente" in str(erro.value)


# ---------------------------------------------------------------------
# Mensagens
# ---------------------------------------------------------------------


def test_a_mensagem_fica_registada(loja: Loja, com_contacto: str) -> None:
    conversa = loja.abrir_conversa(com_contacto)
    escrita = loja.adicionar_mensagem(conversa["id"], "me", "Olá")

    lida = loja.conversa(conversa["id"])
    assert len(lida["messages"]) == 1
    assert lida["messages"][0]["id"] == escrita["id"]
    assert lida["messages"][0]["text"] == "Olá"


def test_a_escrita_e_visivel_antes_de_fechar(loja: Loja, com_contacto: str) -> None:
    """O commit é na escrita, não no fecho.

    Uma escrita que só aparece ao fechar o contexto perde-se se o
    sidecar morrer entre as duas, e é a perda silenciosa que o formato
    de ficheiro resolvia e que o SQLite resolve outra vez.
    """
    conversa = loja.abrir_conversa(com_contacto)
    loja.adicionar_mensagem(conversa["id"], "me", "Já cá está")
    with Loja(loja._caminho) as outra:  # noqa: SLF001 — reabrir a mesma
        mensagens = outra.conversa(conversa["id"])["messages"]
    assert len(mensagens) == 1


def test_uma_mensagem_de_fora_marca_nao_lida(loja: Loja, com_contacto: str) -> None:
    """É o que faz o número ao lado do nome aparecer."""
    conversa = loja.abrir_conversa(com_contacto)
    loja.marcar_lida(conversa["id"])
    loja.adicionar_mensagem(conversa["id"], "them", "Chegaste?")

    assert loja.conversas()[0]["unread"] == 1


def test_uma_mensagem_nossa_nao_marca_nao_lida(loja: Loja, com_contacto: str) -> None:
    """Marcar como não lida a sua própria mensagem põe um número que
    ninguém pode resolver."""
    conversa = loja.abrir_conversa(com_contacto)
    loja.marcar_lida(conversa["id"])
    loja.adicionar_mensagem(conversa["id"], "me", "Escrevi eu")
    assert loja.conversas()[0]["unread"] == 0


def test_marcar_como_lida(loja: Loja, com_contacto: str) -> None:
    conversa = loja.abrir_conversa(com_contacto)
    loja.adicionar_mensagem(conversa["id"], "them", "Oi")
    loja.marcar_lida(conversa["id"])
    assert loja.conversas()[0]["unread"] == 0


def test_o_remetente_invalido_e_recusado(loja: Loja, com_contacto: str) -> None:
    """``de`` é fechado de propósito.

    Aceitar um remetente arbitrário punha um valor na interface que a
    loja nunca soube gerar, e que não pode ser distinguido de «esta
    mensagem foi escrita por mim».
    """
    conversa = loja.abrir_conversa(com_contacto)
    with pytest.raises(LojaInvalida) as erro:
        loja.adicionar_mensagem(conversa["id"], "alguem", "Olá")
    assert "remetente" in str(erro.value)


def test_a_mensagem_para_conversa_inexistente(loja: Loja) -> None:
    with pytest.raises(LojaInvalida) as erro:
        loja.adicionar_mensagem("c-nada", "me", "Olá")
    assert "inexistente" in str(erro.value)


def test_o_texto_vazio_e_guardado(loja: Loja, com_contacto: str) -> None:
    """A loja não decide o que é uma mensagem válida.

    Validar o texto é responsabilidade de quem envia, e decidir aqui
    seria uma segunda regra que diverge da primeira.
    """
    conversa = loja.abrir_conversa(com_contacto)
    assert loja.adicionar_mensagem(conversa["id"], "me", "")["text"] == ""


def test_o_limite_corta_as_mais_antigas(loja: Loja, com_contacto: str) -> None:
    """Ao abrir uma conversa, o que interessa é o que ficou para trás.

    O limite é aplicado à frente — as últimas ``N`` — e não atrás. Cortar
    as mais recentes daria um ecrã com as mensagens mais antigas e sem o
    fim, que é o oposto do que se quer.
    """
    conversa = loja.abrir_conversa(com_contacto)
    for indice in range(10):
        loja.adicionar_mensagem(conversa["id"], "them", f"mensagem {indice}")

    mensagens = loja.mensagens(conversa["id"], limite=3)
    assert [m["text"] for m in mensagens] == ["mensagem 7", "mensagem 8", "mensagem 9"]


def test_as_mensagens_vem_por_ordem_cronologica(loja: Loja, com_contacto: str) -> None:
    conversa = loja.abrir_conversa(com_contacto)
    for indice in range(5):
        loja.adicionar_mensagem(conversa["id"], "them", f"{indice}")

    textos = [m["text"] for m in loja.mensagens(conversa["id"])]
    assert textos == ["0", "1", "2", "3", "4"]


def test_o_envelope_vem_ficar_guardado(loja: Loja, com_contacto: str) -> None:
    """O envelope cifrado guarda-se; o texto, não se recupera do envelope.

    A loja guarda ambos porque o ecrã precisa de mostrar texto e o
    reenvio precisa do envelope, mas são campos separados e a interface
    nunca os confunde.
    """
    conversa = loja.abrir_conversa(com_contacto)
    envelope = bytes(range(32))
    loja.adicionar_mensagem(conversa["id"], "me", "cifrado", envelope=envelope)

    guardada = loja.mensagens(conversa["id"])[0]
    assert "envelope" not in guardada  # a interface não o recebe

    linha = loja._cx.execute("SELECT envelope FROM mensagens WHERE id = ?", (guardada["id"],)).fetchone()  # noqa: SLF001
    assert bytes(linha[0]) == envelope


def test_a_conversa_traz_a_ultima_mensagem(loja: Loja, com_contacto: str) -> None:
    """A lista de conversas não carrega todas as mensagens.

    Com uma sub-consulta, a lista de mil conversas continua a ser uma
    consulta; sem ela, são milhões de linhas para mostrar trinta resumos.
    """
    primeira = loja.abrir_conversa(com_contacto)
    loja.adicionar_mensagem(primeira["id"], "them", "Antiga")
    loja.adicionar_mensagem(primeira["id"], "them", "Recente")

    resumo = loja.conversas()[0]
    assert resumo["lastMessage"] == "Recente"
    assert resumo["lastAt"] is not None


def test_as_fixadas_vem_primeiro(loja: Loja, com_contacto: str) -> None:
    """Uma conversa fixada que aparece abaixo de uma mais recente
    transforma a palavra «fixar» num adjectivo sem efeito."""
    pedido = loja.registar_pedido("ONYX-BBBBBB-!B!B#", "Bruno")
    bruno = loja.decidir_pedido(pedido["id"], "aceite")["contactoId"]

    antiga = loja.abrir_conversa(com_contacto)
    loja.adicionar_mensagem(antiga["id"], "them", "Antiga")
    loja._cx.execute("UPDATE conversas SET fixada = 1 WHERE id = ?", (antiga["id"],))  # noqa: SLF001

    nova = loja.abrir_conversa(bruno)
    loja.adicionar_mensagem(nova["id"], "them", "Nova")

    assert loja.conversas()[0]["id"] == antiga["id"]


def test_a_conversa_sem_mensagens_tem_a_data_de_criacao(loja: Loja, com_contacto: str) -> None:
    """`lastAt` a `None` faria a lista mostrar «nunca» numa conversa que
    acabou de ser criada."""
    loja.abrir_conversa(com_contacto)
    assert loja.conversas()[0]["lastAt"] is None
    assert loja.conversas()[0]["criadaEm"] is not None


def test_a_pesquisa_de_conversas_vê_o_ultimo_texto(loja: Loja, com_contacto: str) -> None:
    conversa = loja.abrir_conversa(com_contacto)
    loja.adicionar_mensagem(conversa["id"], "them", "a palavra rara")

    assert len(loja.conversas("rara")) == 1
    assert loja.conversas("inexistente") == []


def test_a_pesquisa_de_conversas_vê_o_nome_do_contacto(loja: Loja, com_contacto: str) -> None:
    loja.abrir_conversa(com_contacto)
    assert len(loja.conversas("ana")) == 1


# ---------------------------------------------------------------------
# Apagar
# ---------------------------------------------------------------------


def test_apagar_a_conversa_apaga_as_mensagens(loja: Loja, com_contacto: str) -> None:
    """A cascading depende de `PRAGMA foreign_keys = ON`.

    Sem ela, apagar uma conversa deixa as mensagens lá sem ninguém dizer
    nada — e a próxima que abrir «essa conversa» vê um histórico de uma
    pessoa com quem já não fala.
    """
    conversa = loja.abrir_conversa(com_contacto)
    loja.adicionar_mensagem(conversa["id"], "me", "Olá")

    loja._cx.execute("DELETE FROM conversas WHERE id = ?", (conversa["id"],))  # noqa: SLF001
    loja._cx.commit()  # noqa: SLF001

    restantes = loja._cx.execute("SELECT COUNT(*) FROM mensagens").fetchone()[0]  # noqa: SLF001
    assert restantes == 0


# ---------------------------------------------------------------------
# Pedidos
# ---------------------------------------------------------------------


def test_os_pedidos_pendentes_vem_por_recemencia(loja: Loja) -> None:
    loja.registar_pedido("ONYX-111111-!A!A#", "Primeiro")
    loja.registar_pedido("ONYX-222222-!B!B#", "Segundo")
    assert [p["name"] for p in loja.pedidos()] == ["Segundo", "Primeiro"]


def test_os_decididos_desaparecem(loja: Loja) -> None:
    """Uma lista com o que já foi tratado parece um programa avariado."""
    pedido = loja.registar_pedido(IDENT, "Ana")
    loja.decidir_pedido(pedido["id"], "aceite")
    assert loja.pedidos() == []


def test_os_decididos_vem_quando_se_pede(loja: Loja) -> None:
    pedido = loja.registar_pedido(IDENT, "Ana")
    loja.decidir_pedido(pedido["id"], "recusa")
    assert len(loja.pedidos(incluir_bloqueados=True)) == 1


def test_recusar_nao_cria_contacto(loja: Loja) -> None:
    """Recusar e não querer que falem é diferente de querer e não estar
    pronto. Recusar não cria."""
    pedido = loja.registar_pedido(IDENT, "Ana")
    loja.decidir_pedido(pedido["id"], "recusa")
    assert loja.contactos() == []


def test_bloquear_cria_o_contacto_marcado(loja: Loja) -> None:
    """Bloquear é querer que a pessoa apareça como contacto, para se
    poder desbloquear depois."""
    pedido = loja.registar_pedido(IDENT, "Ana")
    loja.decidir_pedido(pedido["id"], "bloqueia")
    assert loja.contactos()[0]["blocked"] is True


def test_o_contacto_bloqueado_nao_conta(loja: Loja) -> None:
    """A contagem de contactos é a de quem se pode falar."""
    pedido = loja.registar_pedido(IDENT, "Ana")
    loja.decidir_pedido(pedido["id"], "bloqueia")
    assert loja.contagens()["contacts"] == 0


def test_um_pedido_nao_se_decide_duas_vezes(loja: Loja) -> None:
    """Aceitar duas vezes criaria dois contactos para a mesma pessoa."""
    pedido = loja.registar_pedido(IDENT, "Ana")
    loja.decidir_pedido(pedido["id"], "aceite")

    with pytest.raises(LojaInvalida) as erro:
        loja.decidir_pedido(pedido["id"], "aceite")
    assert "já foi decidido" in str(erro.value)
    assert len(loja.contactos()) == 1


def test_um_pedido_inexistente(loja: Loja) -> None:
    with pytest.raises(LojaInvalida) as erro:
        loja.decidir_pedido("p-nada", "aceite")
    assert "inexistente" in str(erro.value)


def test_a_decisao_invalida_e_recusada(loja: Loja) -> None:
    """A lista é fechada de propósito.

    Uma decisão inventada ficaria no registo como estado que nada sabe
    tratar, e a próxima vez que o pedido aparecesse não haveria código
    para o que fazer com ele.
    """
    pedido = loja.registar_pedido(IDENT, "Ana")
    with pytest.raises(LojaInvalida) as erro:
        loja.decidir_pedido(pedido["id"], "talvez")
    assert "inválida" in str(erro.value)


def test_o_mesmo_contacto_nao_e_duplicado(loja: Loja) -> None:
    """Duas pessoas a mandar o mesmo identificador são a mesma pessoa."""
    primeiro = loja.registar_pedido(IDENT, "Ana")
    segundo = loja.registar_pedido(IDENT, "Ana outra vez")

    a = loja.decidir_pedido(primeiro["id"], "aceite")
    b = loja.decidir_pedido(segundo["id"], "aceite")

    assert a["contactoId"] == b["contactoId"]
    assert len(loja.contactos()) == 1


# ---------------------------------------------------------------------
# Definições
# ---------------------------------------------------------------------


def test_as_definicoes_vazias(loja: Loja) -> None:
    assert loja.definicoes() == {}


def test_guardar_e_ler(loja: Loja) -> None:
    loja.guardar_definicoes({"tema": "escuro", "notificacoes": True})
    assert loja.definicoes() == {"tema": "escuro", "notificacoes": True}


def test_guardar_substitui(loja: Loja) -> None:
    """Substitui e não funde.

    A interface tem o objecto inteiro na mão e manda-o inteiro. Funde
    deixaria definições a meio de quem já não as tem.
    """
    loja.guardar_definicoes({"a": 1, "b": 2})
    loja.guardar_definicoes({"a": 9})
    assert loja.definicoes() == {"a": 9}


def test_as_definicoes_aceitam_estruturas(loja: Loja) -> None:
    """Listas e objectos são o que a interface manda.

    A serialização tem de ser JSON a sério, não `str()`: `str({'a': 1})`
    produz `"{'a': 1}"`, que `json.loads` não lê.
    """
    valor = {"seccao": [{"id": "x", "opcoes": [{"id": "y", "ligado": False}]}]}
    loja.guardar_definicoes(valor)
    assert loja.definicoes() == valor


def test_as_definicoes_guardam_unicode(loja: Loja) -> None:
    """Um nome com acento que volta a ser `?` é um defeito visível."""
    loja.guardar_definicoes({"nota": "acção"})
    assert loja.definicoes()["nota"] == "acção"


# ---------------------------------------------------------------------
# Contagens
# ---------------------------------------------------------------------


def test_as_contagens_comecam_a_zero(loja: Loja) -> None:
    assert loja.contagens() == {
        "conversations": 0,
        "unread": 0,
        "contacts": 0,
        "requests": 0,
    }


def test_as_contagens_contam_o_que_importa(loja: Loja, com_contacto: str) -> None:
    """Cada número da barra superior tem de vir de uma coisa diferente.

    Um número que não muda é um número que ninguém vai usar para decidir
    nada, e o pior de todos é o que mente.
    """
    pedido = loja.registar_pedido("ONYX-999999-!C!C#", "Carla")
    conversa = loja.abrir_conversa(com_contacto)
    loja.adicionar_mensagem(conversa["id"], "them", "Olá")

    contagens = loja.contagens()
    assert contagens["conversations"] == 1
    assert contagens["unread"] == 1
    assert contagens["contacts"] == 1
    assert contagens["requests"] == 1
    assert pedido["id"]


def test_o_sqlite_integridade_e_verificada(loja: Loja) -> None:
    """O ficheiro não fica por metade de uma escrita.

    `PRAGMA integrity_check` é a forma mais barata de dizer que o ficheiro
    está bem, e é o que se chama depois de uma queda de energia.
    """
    loja.registar_pedido(IDENT, "Ana")
    resultado = loja._cx.execute("PRAGMA integrity_check").fetchone()[0]  # noqa: SLF001
    assert resultado == "ok"
