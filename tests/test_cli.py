# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_cli.py — interface de linha de comando (user.cli).

Cobre os cinco subcomandos e o contrato de erros de ``main``: qualquer
erro esperado sai como ``onyxchat: …`` no stderr com código 1, sem
exceção para fora. O daemon é simulado com o servidor falso; o discovery
com funções substituídas.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from messenger.envelope import VERSAO
from messenger.ipc_client import CMD_ENCODE
from messenger.keys import Identidade
from server.discovery_server import DescobertaInvalida
from server.relay_server import RelayErro
from tests.apoio import envelope_assinado, servidor_falso
from user import cli, config as config_mod

HEX_K1 = "11" * 32
HEX_K5 = "22" * 32
HEX_K9 = "33" * 32


def _preparar_config(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, socket: str | None = None
) -> Path:
    """Define ``$ONYXCHAT_CONFIG`` em ``tmp_path`` e cria a configuração."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    config_mod.criar(caminho, socket=socket)
    return caminho


def test_parser_ajuda_e_argumentos_obrigatorios() -> None:
    """``--help`` sai a 0; sem subcomando/argumentos obrigatórios → 2."""
    with pytest.raises(SystemExit) as ajuda:
        cli.main(["--help"])
    assert ajuda.value.code == 0
    with pytest.raises(SystemExit) as vazio:
        cli.main([])
    assert vazio.value.code == 2
    with pytest.raises(SystemExit) as incompleto:
        cli.main(["enviar", "olá"])  # falta --k1 e --k9-par
    assert incompleto.value.code == 2


def test_iniciar_cria_config_e_mostra_pub(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``iniciar`` grava a config e imprime a pública em verde."""
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(tmp_path / "config.json"))
    assert cli.main(["iniciar", "--forcar"]) == 0
    saida = capsys.readouterr()
    assert "identidade criada: " in saida.out
    assert "\033[32m" in saida.out  # cor verde
    assert config_mod.carregar().pub.hex() in saida.out


def test_iniciar_com_opcoes_e_erro_sem_forcar(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Socket/pasta ficam guardados; segunda execução sem --forcar falha."""
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(tmp_path / "conf.json"))
    pasta = tmp_path / "dados"
    assert cli.main(["iniciar", "--socket", "/tmp/meu.sock", "--pasta", str(pasta)]) == 0
    cfg = config_mod.carregar()
    assert cfg.socket == "/tmp/meu.sock" and cfg.pasta_dados == pasta
    # Já existe → ConfigInvalida apanhada → 1 com aviso no stderr.
    assert cli.main(["iniciar"]) == 1
    erro = capsys.readouterr().err
    assert erro.startswith("\033[31monyxchat: ")  # cor vermelha
    assert "já existe" in erro


def test_identidade_mostra_publica(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``identidade`` imprime a pública (hex) a partir da config."""
    _preparar_config(tmp_path, monkeypatch)
    assert cli.main(["identidade"]) == 0
    saida = capsys.readouterr().out
    assert "pub: " in saida and "\033[34m" in saida  # cor azul


def test_enviar_com_socket_override(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``enviar`` usa ``--socket`` (mesmo com socket na config) e imprime hex."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    recebidos: list[bytes] = []
    with servidor_falso(
        caminho,
        lambda p: recebidos.append(p) or b"\x00" + bytes([VERSAO]) + b"e" * 116,
    ) as socket:
        codigo = cli.main(
            [
                "enviar",
                "olá mundo",
                "--k1",
                HEX_K1,
                "--k9-par",
                HEX_K9,
                "--socket",
                socket,
            ]
        )
    assert codigo == 0
    envelope_hex = capsys.readouterr().out.strip()
    assert bytes.fromhex(envelope_hex)[0] == VERSAO
    assert recebidos[0][0] == CMD_ENCODE  # pedido ENCODE correcto


def test_enviar_usa_socket_da_config(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Sem ``--socket``, ``enviar`` usa o ``socket`` da config."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    with servidor_falso(caminho, b"\x00" + bytes([VERSAO]) + b"e" * 116):
        codigo = cli.main(["enviar", "ola", "--k1", HEX_K1, "--k9-par", HEX_K9])
    assert codigo == 0
    assert capsys.readouterr().out.strip()  # hex do envelope


def test_receber_roundtrip_com_socket_override(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``receber`` valida a assinatura, decifra no daemon e imprime."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    remetente = Identidade.gerar()
    envelope = envelope_assinado(remetente, b"segredo autenticado".ljust(32, b"!"))
    with servidor_falso(caminho, b"\x00" + "mensagem clara".encode()) as socket:
        codigo = cli.main(
            [
                "receber",
                envelope.hex(),
                "--k1",
                HEX_K1,
                "--k5-par",
                HEX_K5,
                "--pub-par",
                remetente.pub.hex(),
                "--socket",
                socket,
            ]
        )
    assert codigo == 0
    assert capsys.readouterr().out == "mensagem clara\n"


def test_receber_usa_socket_da_config(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Sem ``--socket``, ``receber`` usa o da config (ramo de config)."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    remetente = Identidade.gerar()
    envelope = envelope_assinado(remetente)
    with servidor_falso(caminho, b"\x00" + "ok".encode()):
        codigo = cli.main(
            [
                "receber",
                envelope.hex(),
                "--k1",
                HEX_K1,
                "--k5-par",
                HEX_K5,
                "--pub-par",
                remetente.pub.hex(),
            ]
        )
    assert codigo == 0
    assert capsys.readouterr().out == "ok\n"


def test_receber_assinatura_invalida(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Pública trocada → ``AssinaturaInvalida`` → 1 (nunca ao daemon)."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    envelope = envelope_assinado(Identidade.gerar())
    outra_pub = Identidade.gerar().pub
    with servidor_falso(caminho, b"\x00nao-deve-chegar"):
        codigo = cli.main(
            [
                "receber",
                envelope.hex(),
                "--k1",
                HEX_K1,
                "--k5-par",
                HEX_K5,
                "--pub-par",
                outra_pub.hex(),
            ]
        )
    assert codigo == 1
    assert "assinatura" in capsys.readouterr().err


def test_receber_envelope_e_hex_invalidos(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Envelope curto ou hex malformado → 1 com aviso no stderr."""
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/nada.sock")
    assert (
        cli.main(
            [
                "receber",
                "00",  # 1 byte < 117
                "--k1",
                HEX_K1,
                "--k5-par",
                HEX_K5,
                "--pub-par",
                "00" * 32,
            ]
        )
        == 1
    )
    assert "curto" in capsys.readouterr().err
    assert (
        cli.main(
            [
                "receber",
                "zz",
                "--k1",
                HEX_K1,
                "--k5-par",
                HEX_K5,
                "--pub-par",
                "00" * 32,
            ]
        )
        == 1
    )
    assert "onyxchat:" in capsys.readouterr().err  # ValueError (hex)


def test_enviar_daemon_em_baixo(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Sem daemon → ``DaemonIndisponivel`` (ErroIpc) → 1."""
    _preparar_config(tmp_path, monkeypatch)
    caminho = str(tmp_path / "nao-existe.sock")
    codigo = cli.main(
        [
            "enviar",
            "olá",
            "--k1",
            HEX_K1,
            "--k9-par",
            HEX_K9,
            "--socket",
            caminho,
        ]
    )
    assert codigo == 1
    erro = capsys.readouterr().err
    assert "onyxchat:" in erro and "inacessível" in erro


def test_enviar_erro_do_daemon(monkeypatch, tmp_path: Path, capsys) -> None:
    """Daemon responde ERRO → ``ErroDaemon`` nomeado → 1."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    with servidor_falso(caminho, b"\x01\x05tag AEAD") as socket:
        codigo = cli.main(
            [
                "enviar",
                "x",
                "--k1",
                HEX_K1,
                "--k9-par",
                HEX_K9,
                "--socket",
                socket,
            ]
        )
    assert codigo == 1
    assert "DecifragemFalhou" in capsys.readouterr().err


def test_descobrir_encontrado(capsys, monkeypatch) -> None:
    """Descoberta com sucesso → imprime o .onion e devolve 0."""
    monkeypatch.setattr(cli, "consultar_descoberta", lambda _s, _i: "abc.onion")
    assert cli.main(["descobrir", "amigo"]) == 0
    assert capsys.readouterr().out == "abc.onion\n"


def test_descobrir_desconhecido(capsys, monkeypatch) -> None:
    """Resposta 404 (``None``) → aviso amarelo no stderr e código 1."""
    monkeypatch.setattr(cli, "consultar_descoberta", lambda _s, _i: None)
    assert cli.main(["descobrir", "fantasma"]) == 1
    erro = capsys.readouterr().err
    assert "desconhecido" in erro
    assert "\033[33m" in erro  # cor amarela


def test_descobrir_erro_tipado_e_generico(capsys, monkeypatch) -> None:
    """``DescobertaInvalida`` e ``ValueError`` genérico → 1."""
    def avariado(_s: str, _i: str) -> str:
        raise DescobertaInvalida("servidor em baixo")

    monkeypatch.setattr(cli, "consultar_descoberta", avariado)
    assert cli.main(["descobrir", "x"]) == 1
    assert "servidor em baixo" in capsys.readouterr().err

    def generico(_s: str, _i: str) -> str:
        raise ValueError("erro inesperado")

    monkeypatch.setattr(cli, "consultar_descoberta", generico)
    assert cli.main(["descobrir", "x"]) == 1
    assert "erro inesperado" in capsys.readouterr().err


def test_descobrir_servidor_por_omissao(monkeypatch) -> None:
    """O endpoint por omissão é ``http://127.0.0.1:8789``."""
    capturado: list[tuple[str, str]] = []

    def espiar(servidor: str, identificador: str) -> str | None:
        capturado.append((servidor, identificador))
        return "x.onion"

    monkeypatch.setattr(cli, "consultar_descoberta", espiar)
    assert cli.main(["descobrir", "amigo"]) == 0
    assert capturado == [(cli.SERVIDOR_DESCOBERTA, "amigo")]


# ---------------------------------------------------------------------
# Etapa 7 — rede, handshake e relé
# ---------------------------------------------------------------------

HEX_PUB_LOCAL = "44" * 32
HEX_PUB_PAR = "55" * 32

#: Endereço `.onion` v3 bem formado (56 caracteres de base32). A CLI
#: valida o destino antes de emitir `LIGAR`, pelo que um `.onion` de
#: exemplo tem de respeitar o formato real.
ONION_TESTE = "a" * 56 + ".onion"
HEX_SEED = "66" * 32
HEX_NONCE = "77" * 16
CORPO_PEDIDO = b"p" * 305  # 3*32 + TAM_CORPO_PEDIDO
CORPO_ACEITE = b"a" * 369  # TAM_AMIZADE + TAM_CORPO_ACEITE
CORPO_RECUSA = b"r" * 81


def test_estado_mostra_tor_ligacao_e_amigos(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``estado`` traduz os bytes do daemon e usa o ``--socket``."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    corpo = bytes([0x02, 0x01]) + (3).to_bytes(2, "little") + b"abc.onion"
    with servidor_falso(caminho, b"\x00" + corpo) as socket:
        assert cli.main(["estado", "--socket", socket]) == 0
    saida = capsys.readouterr().out
    assert "tor: ativo | ligado: sim | amigos: 3 | onion: abc.onion" in saida
    assert "\033[34m" in saida  # cor azul


def test_estado_com_tor_desconhecido_e_sem_ligacao(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Código de tor fora de ``NOMES_TOR`` e ``onion`` vazio → fallbacks."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    corpo = bytes([0x99, 0x00]) + (0).to_bytes(2, "little")
    with servidor_falso(caminho, b"\x00" + corpo):
        assert cli.main(["estado"]) == 0  # usa o socket da config
    saida = capsys.readouterr().out
    assert "tor: tor0x99 | ligado: nao | amigos: 0 | onion: -" in saida


def test_ouvir_imprime_onion_sem_cor(monkeypatch, tmp_path: Path, capsys) -> None:
    """``ouvir`` devolve o ``.onion`` em texto limpo (legível por scripts)."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    with servidor_falso(caminho, b"\x00onion-abc.onion"):
        assert cli.main(["ouvir"]) == 0
    assert capsys.readouterr().out == "onion-abc.onion\n"


def test_ligar_modos_e_hex(monkeypatch, tmp_path: Path, capsys) -> None:
    """``ligar`` envia o modo, as cadeias e as duas públicas no corpo."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    pedidos: list[bytes] = []

    def responder(pedido: bytes) -> bytes:
        pedidos.append(pedido)
        return b"\x00"

    with servidor_falso(caminho, responder) as socket:
        codigo = cli.main(
            [
                "ligar",
                "--modo",
                "relay",
                "--endpoint",
                "127.0.0.1:8788",
                ONION_TESTE,
                HEX_PUB_LOCAL,
                HEX_PUB_PAR,
                "--socket",
                socket,
            ]
        )
    assert codigo == 0
    assert f"ligado (relay) a {ONION_TESTE}" in capsys.readouterr().out
    pedido = pedidos[0]
    assert pedido[0] == 0x05  # CMD_LIGAR
    assert pedido[1] == 0x01  # MODO_RELAY
    assert pedido[2:4] == (14).to_bytes(2, "little")  # "127.0.0.1:8788"
    assert bytes.fromhex(HEX_PUB_LOCAL) in pedido
    assert bytes.fromhex(HEX_PUB_PAR) in pedido


def test_ligar_direto_com_id_resolve_via_discovery(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Em modo directo, um ID hex é resolvido antes de chegar ao daemon.

    O daemon não tem cliente de discovery (recusaria o ID com `0x0D`), logo
    a resolução é do cliente. Este teste prova que o `.onion` resolvido é
    o que vai no corpo do pedido — não o ID.
    """
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    pedidos: list[bytes] = []

    def responder(pedido: bytes) -> bytes:
        pedidos.append(pedido)
        return b"\x00"

    def consulta(_servidor: str, identificador: str) -> str:
        assert identificador == HEX_PUB_PAR, "consultou o ID errado"
        return ONION_TESTE

    monkeypatch.setattr(cli, "resolver_ou_usar", _resolver_com(consulta))

    with servidor_falso(caminho, responder) as socket:
        codigo = cli.main(
            [
                "ligar",
                "--modo",
                "direto",
                HEX_PUB_PAR,
                HEX_PUB_LOCAL,
                HEX_PUB_PAR,
                "--socket",
                socket,
            ]
        )
    assert codigo == 0
    assert f"ligado (direto) a {ONION_TESTE}" in capsys.readouterr().out
    pedido = pedidos[0]
    assert pedido[0] == 0x05  # CMD_LIGAR
    # Framing do corpo de LIGAR (docs/ipc_spec.md §Pedidos):
    #   [modo:1][len_ep:2][ep][len_dest:2][dest][pub_propria:32][pub_par:32]
    # Logo, com o byte do comando em pedido[0], o endpoint começa em 4.
    tam_endpoint = int.from_bytes(pedido[2:4], "little")
    inicio_destino = 4 + tam_endpoint
    tam_destino = int.from_bytes(pedido[inicio_destino : inicio_destino + 2], "little")
    destino = pedido[inicio_destino + 2 : inicio_destino + 2 + tam_destino].decode()
    assert destino == ONION_TESTE, "o corpo tem de levar o .onion, não o ID"


def _resolver_com(consulta) -> object:
    """Fabrica um ``resolver_ou_usar`` com a consulta injectada."""

    def resolver(destino: str, modo: int, servidor: str) -> str:
        from messenger.descoberta import resolver_ou_usar

        return resolver_ou_usar(destino, modo, servidor, consulta)

    return resolver


def test_ligar_com_id_desconhecido_da_erro(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """ID não registado ⇒ código 1 e mensagem accionável; nada é enviado."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    enviados: list[bytes] = []

    def responder(pedido: bytes) -> bytes:
        enviados.append(pedido)
        return b"\x00"

    monkeypatch.setattr(
        cli, "resolver_ou_usar", _resolver_com(lambda _s, _i: None)
    )

    with servidor_falso(caminho, responder) as socket:
        codigo = cli.main(
            [
                "ligar",
                HEX_PUB_PAR,
                HEX_PUB_LOCAL,
                HEX_PUB_PAR,
                "--socket",
                socket,
            ]
        )
    assert codigo == 1
    erro = capsys.readouterr().err
    assert "não está registado" in erro
    assert not enviados, "não pode enviar um pedido que não deveria sair"


def test_ligar_com_destino_malformado_da_erro(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Um destino que não é `.onion` nem ID é recusado com código 1."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    enviados: list[bytes] = []

    def responder(pedido: bytes) -> bytes:
        enviados.append(pedido)
        return b"\x00"

    with servidor_falso(caminho, responder) as socket:
        codigo = cli.main(
            [
                "ligar",
                "nao-e-um-destino",
                HEX_PUB_LOCAL,
                HEX_PUB_PAR,
                "--socket",
                socket,
            ]
        )
    assert codigo == 1
    assert "destino inválido" in capsys.readouterr().err
    assert not enviados


def test_enviar_p2p_envia_hex_como_frame(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``enviar-p2p`` rejeita hex inválido (1) e envia o envelope (0)."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    recebidos: list[bytes] = []
    with servidor_falso(
        caminho, lambda p: recebidos.append(p) or b"\x00"
    ) as socket:
        assert cli.main(["enviar-p2p", "abc", "--socket", socket]) == 1
        assert cli.main(["enviar-p2p", "aabb", "--socket", socket]) == 0
    erros = capsys.readouterr()
    assert "onyxchat:" in erros.err  # ValueError (hex par)
    assert "enviado (2 bytes)" in erros.out
    assert recebidos[0][0] == 0x06  # CMD_ENVIAR
    assert recebidos[0][1:] == b"\xaa\xbb"


def test_receber_p2p_imprime_tipo_e_corpo(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``receber-p2p`` imprime ``<tipo> <hex>`` e envia o timeout u32."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    pedidos: list[bytes] = []

    def responder(pedido: bytes) -> bytes:
        pedidos.append(pedido)
        return b"\x00" + bytes([0x01]) + b"\xde\xad"

    with servidor_falso(caminho, responder):
        assert cli.main(["receber-p2p", "--timeout-ms", "750"]) == 0
    assert capsys.readouterr().out == "01 dead\n"
    assert pedidos[0][0] == 0x07  # CMD_RECEBER
    assert pedidos[0][1:] == (750).to_bytes(4, "little")


def test_receber_p2p_timeout_por_omissao(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """Sem ``--timeout-ms`` o pedido leva 5000 ms."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    pedidos: list[bytes] = []
    with servidor_falso(
        caminho, lambda p: pedidos.append(p) or b"\x00\x20"
    ):
        assert cli.main(["receber-p2p"]) == 0
    assert capsys.readouterr().out == "20 \n"
    assert pedidos[0][1:] == (5000).to_bytes(4, "little")


def test_fechar_imprime_confirmação(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``fechar`` delega em ``FECHAR`` e confirma em verde."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket="/tmp/ignorado.sock")
    with servidor_falso(caminho, b"\x00") as socket:
        assert cli.main(["fechar", "--socket", socket]) == 0
    saida = capsys.readouterr().out
    assert "ligacao fechada" in saida and "\033[32m" in saida


def test_pedir_amizade_imprime_chaves_e_corpo(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``pedir-amizade`` imprime k1/k5/k9 e o corpo a entregar ao par."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    with servidor_falso(caminho, b"\x00" + CORPO_PEDIDO):
        assert cli.main(["pedir-amizade", HEX_PUB_PAR, "--seed", HEX_SEED]) == 0
    linhas = dict(
        linha.split("=", 1) for linha in capsys.readouterr().out.splitlines()
    )
    assert set(linhas) == {"k1", "k5", "k9", "corpo"}
    assert len(bytes.fromhex(linhas["k1"])) == 32
    assert len(bytes.fromhex(linhas["corpo"])) == 209


def test_aceitar_amizade_imprime_seis_chaves_e_aceite(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``aceitar-amizade`` imprime as 6 chaves + corpo de ``FRIEND_ACCEPT``."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    with servidor_falso(caminho, b"\x00" + CORPO_ACEITE):
        assert (
            cli.main(["aceitar-amizade", "00" * 209, "--seed", HEX_SEED]) == 0
        )
    linhas = dict(
        linha.split("=", 1) for linha in capsys.readouterr().out.splitlines()
    )
    assert set(linhas) == {
        "k1", "k5_proprio", "k9_proprio", "k5_par", "k9_par", "publica", "corpo",
    }
    assert len(bytes.fromhex(linhas["publica"])) == 32
    assert len(bytes.fromhex(linhas["corpo"])) == 177


def test_recusar_amizade_imprime_corpo(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``recusar-amizade`` devolve o ``FRIEND_REJECT`` de 81 bytes."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    pedidos: list[bytes] = []

    def responder(pedido: bytes) -> bytes:
        pedidos.append(pedido)
        return b"\x00" + CORPO_RECUSA

    with servidor_falso(caminho, responder):
        assert cli.main(["recusar-amizade", HEX_NONCE, "--seed", HEX_SEED]) == 0
    assert capsys.readouterr().out == f"corpo={CORPO_RECUSA.hex()}\n"
    assert pedidos[0][0] == 0x0B  # CMD_RECUSAR_AMIZADE


def test_confirmar_amizade_imprime_chaves_sem_corpo(
    monkeypatch, tmp_path: Path, capsys
) -> None:
    """``confirmar-amizade`` fecha a amizade e imprime só as 6 chaves."""
    caminho = tmp_path / "s.sock"
    _preparar_config(tmp_path, monkeypatch, socket=str(caminho))
    with servidor_falso(caminho, b"\x00" + b"c" * 192):
        assert cli.main(["confirmar-amizade", "00" * 177, "--seed", HEX_SEED]) == 0
    linhas = dict(
        linha.split("=", 1) for linha in capsys.readouterr().out.splitlines()
    )
    assert "corpo" not in linhas and len(linhas) == 6


def test_relay_corre_o_servidor(monkeypatch, capsys) -> None:
    """``relay`` avisa da porta e delega em ``relay_server.executar``."""
    chamadas: list[tuple[str, int]] = []
    monkeypatch.setattr(
        cli.relay_server,
        "executar",
        lambda host, porta: chamadas.append((host, porta)),
    )
    assert cli.main(["relay"]) == 0
    assert "relay a escuta em 127.0.0.1:8788" in capsys.readouterr().out
    assert cli.main(["relay", "--host", "0.0.0.0", "--porta", "9"]) == 0
    assert chamadas == [("127.0.0.1", 8788), ("0.0.0.0", 9)]


def test_relay_erro_tipado_sai_com_1(monkeypatch, tmp_path, capsys) -> None:
    """``RelayErro`` está no contrato de erros de ``main`` → 1."""
    def avariar(_host: str, _porta: int) -> None:
        raise RelayErro(0x04, "caixa cheia")

    monkeypatch.setattr(cli.relay_server, "executar", avariar)
    assert cli.main(["relay"]) == 1
    erro = capsys.readouterr().err
    assert erro.startswith("\033[31monyxchat: ")
    assert "SemEspaco" in erro


# ---------------------------------------------------------------------
# Passphrase e configuração cifrada
# ---------------------------------------------------------------------


def test_iniciar_com_passphrase_cifra_as_chaves(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture
) -> None:
    """``iniciar --passphrase-ficheiro`` produz um keystore cifrado."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    ficheiro = tmp_path / "pass.txt"
    ficheiro.write_text("uma passphrase\n", encoding="utf-8")

    codigo = cli.main(["iniciar", "--passphrase-ficheiro", str(ficheiro)])

    assert codigo == 0
    assert config_mod.caminho_keystore(caminho).exists()
    saida = capsys.readouterr().out
    assert "chaves cifradas" in saida
    assert "irrecuperável" in saida


def test_passphrase_ficheiro_usa_so_a_primeira_linha(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """O ``\\n`` final de um ficheiro escrito com `echo` não entra na passphrase.

    Sem isto, `echo pass > f; --passphrase-ficheiro f` cifrava com
    ``"pass\\n"`` e a leitura seguinte, que não traria o `\\n`, dava
    «passphrase errada» numa configuração acabada de criar.
    """
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    ficheiro = tmp_path / "pass.txt"
    ficheiro.write_text("segredo\nsegunda linha\n", encoding="utf-8")

    assert cli.main(["iniciar", "--passphrase-ficheiro", str(ficheiro)]) == 0
    # Reabre com a mesma passphrase, sem o `\n`.
    reopened = config_mod.carregar(caminho, "segredo")
    assert reopened.pub == config_mod.carregar(caminho, "segredo").pub


def test_passphrase_vem_do_ambiente(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """``$ONYXCHAT_PASSPHRASE`` evita o prompt em uso programático."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    monkeypatch.setenv("ONYXCHAT_PASSPHRASE", "do ambiente")
    assert cli.main(["iniciar", "--cifrar"]) == 0
    assert config_mod.carregar(caminho, "do ambiente").k5 is not None


def test_ficheiro_de_passphrase_ilegivel(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Ficheiro que não existe → mensagem, não traceback."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    with pytest.raises(SystemExit, match="não consegui ler"):
        cli.main(["iniciar", "--passphrase-ficheiro", str(tmp_path / "nao-existe")])


def test_ficheiro_de_passphrase_vazio(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Ficheiro vazio → recusa, porque uma passphrase vazia cifra com nada."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    ficheiro = tmp_path / "vazio.txt"
    ficheiro.write_text("\n", encoding="utf-8")
    with pytest.raises(SystemExit, match="vazio"):
        cli.main(["iniciar", "--passphrase-ficheiro", str(ficheiro)])


def test_cifrar_sem_passphrase_recusa(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """``cifrar`` sem nenhuma fonte de passphrase não faz nada."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    config_mod.criar(caminho)
    monkeypatch.delenv("ONYXCHAT_PASSPHRASE", raising=False)
    with pytest.raises(SystemExit, match="nenhuma passphrase"):
        cli.main(["cifrar"])
    # A configuração continua em v1.
    assert json.loads(caminho.read_text(encoding="utf-8"))["versao"] == 1


def test_cifrar_migra_e_mantem_a_identidade(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """``cifrar`` reescreve o formato sem tocar na identidade."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    original = config_mod.criar(caminho)
    monkeypatch.setenv("ONYXCHAT_PASSPHRASE", "a passa")

    assert cli.main(["cifrar"]) == 0

    migrada = config_mod.carregar(caminho, "a passa")
    assert migrada.pub == original.pub
    assert migrada.k5 == original.k5


def test_cifrar_ja_cifrada_da_erro_legivel(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture
) -> None:
    """``cifrar`` duas vezes → mensagem no stderr, código 1."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    config_mod.criar(caminho, passphrase="p")
    monkeypatch.setenv("ONYXCHAT_PASSPHRASE", "p")

    assert cli.main(["cifrar"]) == 1
    assert "já está cifrada" in capsys.readouterr().err


def test_subcomando_pede_passphrase_depois_de_a_config_estar_cifrada(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Um subcomando qualquer pergunta quando encontra a config cifrada.

    É o caminho de um utilizador que migrou e se esqueceu de que agora
    é preciso a passphrase: sem isto, todos os subcomandos dariam
    «passphrase necessária» sem explicar o que fazer.
    """
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    original = config_mod.criar(caminho, passphrase="segredo")

    monkeypatch.delenv("ONYXCHAT_PASSPHRASE", raising=False)
    respostas = iter(["segredo"])
    monkeypatch.setattr(cli, "_pedir_passphrase", lambda confirmar=False: next(respostas))

    assert cli.main(["identidade"]) == 0
    # E a identidade é a mesma, provando que a passphrase era a certa.
    assert config_mod.carregar(caminho, "segredo").pub == original.pub


def test_passphrase_errada_da_mensagem_esperada(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture
) -> None:
    """Passphrase errada sai como erro do CLI, não como excepção."""
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    config_mod.criar(caminho, passphrase="a certa")
    monkeypatch.setenv("ONYXCHAT_PASSPHRASE", "a errada")

    assert cli.main(["identidade"]) == 1
    assert "passphrase errada" in capsys.readouterr().err


def test_confirmacao_e_pedida_so_a_criar_identidade(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Só a criação de identidade pede confirmação.

    `confirmar=True` é o que faz `_pedir_passphrase` ler duas vezes. Noutro
    lado, uma segunda prompt seria uma segunda hipótese de o utilizador
    se enganar sem nenhum ganho — quem está a *abrir* uma identidade
    conhece-a.
    """
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    monkeypatch.delenv("ONYXCHAT_PASSPHRASE", raising=False)

    pedidos: list[bool] = []

    def registar(confirmar: bool = False) -> str:
        pedidos.append(confirmar)
        return "p"

    monkeypatch.setattr(cli, "_pedir_passphrase", registar)
    assert cli.main(["iniciar", "--cifrar"]) == 0
    assert pedidos == [True], "criar identidade tem de confirmar"

    # Abrir a mesma identidade não confirma.
    pedidos.clear()
    assert cli.main(["identidade"]) == 0
    assert pedidos == [False], "abrir não tem de confirmar"


def test_confirmacao_divergente_aborta(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Duas passphrases diferentes abortam **antes** de cifrar a identidade.

    Cifrar com a primeira passphrase e só depois descobrir que a segunda
    não bate deixaria o utilizador com uma identidade que não consegue
    abrir — e sem forma de a recuperar. A comparação é feita dentro de
    `_pedir_passphrase`, que é quem tem as duas respostas.
    """
    monkeypatch.setattr(cli.getpass, "getpass", lambda prompt="": {"passphrase: ": "primeira", "repetir a passphrase: ": "segunda"}[prompt])

    with pytest.raises(SystemExit, match="não coincidem"):
        cli._pedir_passphrase(confirmar=True)


def test_prompt_com_entrada_fechada_explica_as_alternativas(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Um `EOFError` na prompt dá a mensagem com as duas alternativas.

    `onyxchat iniciar < /dev/null` é um erro comum em scripts; a
    mensagem tem de dizer o que fazer, não apenas o que aconteceu.
    """
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))

    def eof(prompt: str = "") -> str:
        raise EOFError

    monkeypatch.setattr(cli.getpass, "getpass", eof)
    with pytest.raises(SystemExit, match="entrada está fechada"):
        cli.main(["iniciar", "--cifrar"])


def test_prompt_com_confirmacao_e_entrada_fechada_no_segundo_prompt(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """O `EOFError` também é apanhado na segunda prompt.

    `onyxchat iniciar` a meio de um pipe, ou um utilizador que carrega
    Ctrl-D depois de escrever a primeira passphrase, dá `EOFError` na
    segunda leitura. Sem este ramo, a excepção escapava do `SystemExit`
    de «não coincidem» e o utilizador via um traceback.
    """
    monkeypatch.delenv("ONYXCHAT_PASSPHRASE", raising=False)
    respostas = iter(["primeira"])

    def parcial(prompt: str = "") -> str:
        try:
            return next(respostas)
        except StopIteration:
            raise EOFError from None

    monkeypatch.setattr(cli.getpass, "getpass", parcial)
    with pytest.raises(SystemExit, match="entrada está fechada"):
        cli._pedir_passphrase(confirmar=True)


def test_prompt_simples_devolve_a_passphrase(monkeypatch: pytest.MonkeyPatch) -> None:
    """Sem confirmação, o prompt é lido uma vez e devolvido tal e qual.

    Cobre o caminho normal de `_pedir_passphrase`: pede-se confirmação
    em `iniciar` e os testes de `EOFError` fecham a prompt antes da
    devolução, portanto este é o único que a executa.
    """
    monkeypatch.delenv("ONYXCHAT_PASSPHRASE", raising=False)
    monkeypatch.setattr(cli.getpass, "getpass", lambda prompt="": "a minha")
    assert cli._pedir_passphrase() == "a minha"


def test_seed_omitida_e_lida_do_config(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Sem ``--seed``, a seed vem do ``config.json``.

    É a mitigação da lacuna A8: `/proc/<pid>/cmdline` é modo ``0444``
    e `ptrace_scope = 1` não o protege, portanto a seed não deve
    aparecer em `argv` por omissão.
    """
    caminho = tmp_path / "config.json"
    monkeypatch.setenv("ONYXCHAT_CONFIG", str(caminho))
    cfg = config_mod.criar(caminho)

    argumentos = cli.argparse.Namespace(seed=None)
    assert cli._seed_local(argumentos) == cfg.identidade.seed


def test_seed_explicita_tem_precedencia(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """``--seed`` continua a ganhar, para quando o daemon tem outra identidade."""
    outra = Identidade.gerar()
    argumentos = cli.argparse.Namespace(seed=outra.seed.hex())
    assert cli._seed_local(argumentos) == outra.seed


def test_seed_invalida_da_erro_legivel() -> None:
    """Hex inválido em ``--seed`` → mensagem, não ``ValueError`` cru."""
    argumentos = cli.argparse.Namespace(seed="não é hex")
    with pytest.raises(SystemExit, match="hex válido"):
        cli._seed_local(argumentos)
