# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_pipeline.py — fachada do pipeline K1→K9 (messenger.pipeline).

Valida a ordem obrigatória da decifragem (parse → assinatura local →
IPC) usando o servidor falso: mensagens adulteradas nunca chegam ao
daemon, e os erros do daemon propagam tipados.
"""

from __future__ import annotations

import pytest

from messenger import pipeline as modulo_pipeline
from messenger.envelope import EnvelopeInvalido, parsear
from messenger.ipc_client import ClienteIpc, ErroDaemon
from messenger.keys import Chaves, Identidade
from messenger.pipeline import AssinaturaInvalida, Pipeline
from tests.apoio import envelope_assinado, servidor_falso

CHAVES = Chaves(k1=b"\x01" * 32, k5=b"\x02" * 32, k9=b"\x03" * 32)


def test_caminho_socket_da_fachada(tmp_path) -> None:
    """A propriedade expõe o socket do cliente subjacente."""
    caminho = str(tmp_path / "s.sock")
    fachada = Pipeline(ClienteIpc(caminho))
    assert fachada.caminho_socket == caminho


def test_pipeline_padrao_usa_socket_por_omissao(monkeypatch) -> None:
    """Construída sem argumentos, usa ``ClienteIpc`` com o caminho por omissão."""
    monkeypatch.delenv("ONYXCHAT_SOCKET", raising=False)
    fachada = Pipeline()
    assert fachada.caminho_socket.startswith("/tmp/onyxchat-")


def test_cifrar_devolve_resposta_do_daemon(tmp_path) -> None:
    """ENCODE: o envelope devolvido pelo daemon atravessa a fachada."""
    identidade = Identidade.gerar()
    envelope = b"\x01" + b"z" * 116
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00" + envelope) as socket:
        saida = Pipeline(ClienteIpc(socket)).cifrar("Olá!", CHAVES, identidade)
    assert saida == envelope


def test_decifrar_com_assinatura_valida_chama_daemon(tmp_path) -> None:
    """Assinatura local OK → IPC é feito e o texto volta."""
    identidade = Identidade.gerar()
    envelope = envelope_assinado(identidade)
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, b"\x00" + "texto".encode()) as socket:
        texto = Pipeline(ClienteIpc(socket)).decifrar(
            envelope, CHAVES, identidade.pub
        )
    assert texto == "texto"


def test_decifrar_assinatura_invalida_nao_chama_daemon(tmp_path) -> None:
    """Pública trocada → ``AssinaturaInvalida`` **antes** de qualquer IPC."""
    identidade = Identidade.gerar()
    outra = Identidade.gerar()
    envelope = envelope_assinado(identidade)
    chamadas: list[bytes] = []
    caminho = tmp_path / "s.sock"
    with servidor_falso(caminho, lambda p: chamadas.append(p) or b"\x00") as socket:
        with pytest.raises(AssinaturaInvalida, match="rejeitada"):
            Pipeline(ClienteIpc(socket)).decifrar(envelope, CHAVES, outra.pub)
    assert chamadas == []  # o daemon nunca foi contactado


def test_decifrar_envelope_malformado(tmp_path) -> None:
    """Envelope curto → ``EnvelopeInvalido`` sem IPC."""
    caminho = tmp_path / "s.sock"
    chamadas: list[bytes] = []
    with servidor_falso(caminho, lambda p: chamadas.append(p) or b"\x00") as socket:
        with pytest.raises(EnvelopeInvalido, match="curto"):
            Pipeline(ClienteIpc(socket)).decifrar(b"curto", CHAVES, bytes(32))
    assert chamadas == []


def test_decifrar_erro_do_daemon_propaga(tmp_path) -> None:
    """Falha de decifragem no daemon (tag AEAD) chega tipada ao chamador."""
    identidade = Identidade.gerar()
    envelope = envelope_assinado(identidade)
    caminho = tmp_path / "s.sock"
    with servidor_falso(
        caminho, b"\x01\x05" + "tag AEAD inválida".encode("utf-8")
    ) as socket:
        with pytest.raises(ErroDaemon, match="DecifragemFalhou"):
            Pipeline(ClienteIpc(socket)).decifrar(envelope, CHAVES, identidade.pub)


def test_assinatura_local_cobre_regiao_inteira() -> None:
    """A verificação usa exatamente a região assinada do envelope."""
    identidade = Identidade.gerar()
    envelope = envelope_assinado(identidade)
    env = parsear(envelope)
    # Uma alteração em qualquer byte da região (não da assinatura) falha.
    dados = bytearray(envelope)
    dados[40] ^= 0x01
    assert not modulo_pipeline.verificar(
        bytes(dados)[:-64], env.assinatura, identidade.pub
    )
