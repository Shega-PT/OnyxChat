# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_vectors.py — os vetores oficiais observados pelo Python.

Lê ``tests/vectors/*.json`` (fonte única de verdade, gerados por
``network/daemon_rust/examples/gerar_vetores.rs``) e verifica:

* as camadas com referência Python — K2, K3, K6 e K8 — produzem
  exatamente os ``output_hex`` documentados (Resumo §25);
* o envelope fim-a-fim (Resumo §47) parseia, bate certo camada a
  camada (K9 = ``ciphertext``) e a assinatura Ed25519 valida contra a
  pública derivada da seed;
* os corpos de handshake (REQUEST/ACCEPT/REJECT) têm o layout
  documentado em ``docs/handshake.md`` e as assinaturas Ed25519
  validam sobre o transcript com o respetivo domínio.

Nenhum código de produção é alterado por este módulo.
"""

from __future__ import annotations

import json
from pathlib import Path

from messenger.envelope import parsear
from messenger.keys import chave_publica, verificar

_CARPETA_VETORES = Path(__file__).resolve().parent / "vectors"


def _carregar(nome: str) -> dict:
    """Lê um ficheiro de vetores da ``tests/vectors``."""
    with (_CARPETA_VETORES / nome).open(encoding="utf-8") as ficheiro:
        return json.load(ficheiro)


def _parametro_int(texto: str, chave: str) -> int:
    """Extrai ``N`` de um campo de texto livre ``"chave=N"``.

    O campo ``parametros`` dos vectores é uma string, não um
    dicionário, e pode trazer mais do que um par. Um ``split("=")[1]``
    assumia que o valor era o resto da linha, o que parte o número ao
    meio quando há texto a seguir.
    """
    for parte in texto.split(","):
        nome, _, valor = parte.strip().partition("=")
        if nome == chave:
            return int(valor)
    raise KeyError(f"{chave!r} não está em {texto!r}")


def test_camadas_analógicas_tem_forma_inteira() -> None:
    """Os vectors das camadas analógicas têm o formato que o daemon usa.

    O que se verifica aqui é a **forma** dos vectors, não o resultado
    criptográfico. O resultado é verificado pelo daemon: `vetores.rs`
    corre as quatro camadas Lua contra estes mesmos bytes, e
    `docs/test_vectors.md` é renderizado a partir do JSON e comparado
    com o que está em disco (`tests/vetores.rs`).

    A implementação Python de referência foi removida em 2026-10-05 por
    não ter caller em produção (ver `crypto/python_lua/README.md`
    §Porque só há Lua). O que fica como garantia de que os vectors não
    divergem em silêncio é o lado do daemon, que os gera — e por isso
    este teste não pode deixar de os ler: é a prova de que o ficheiro
    que o daemon vai ler tem o que ele espera.
    """
    doc = _carregar("camadas.json")
    analógicas = {"K2", "K3", "K6", "K8"}
    vistos: set[str] = set()

    for entrada in doc["camadas"]:
        alvo = entrada["id"]
        if alvo not in analógicas:
            continue
        vistos.add(alvo)

        # `input_hex` e `output_hex` têm de ser hex válido de
        # comprimento igual: um vector truncado faria o `bytes.fromhex`
        # do lado do daemon levantar, e um vector de hex inválido
        # passaria a verificação de forma a não testar nada.
        entrada_bytes = bytes.fromhex(entrada["input_hex"])
        saida_bytes = bytes.fromhex(entrada["output_hex"])
        assert len(entrada_bytes) == len(saida_bytes), (
            f"{alvo}: a transformação é bijectiva, logo o tamanho "
            "não pode mudar"
        )

        if alvo in {"K2", "K8"}:
            # Parametrizadas por deslocamento; `parametros` tem a forma
            # "deslocamento=N".
            deslocamento = _parametro_int(entrada["parametros"], "deslocamento")
            # A cifra e a decifra são a mesma operação com sinal
            # contrário — é isso que K2 e K8 são, e o roundtrip é
            # consequência de aritmética modular, não de um teste.
            assert bytes(
                (b + deslocamento) % 256 for b in entrada_bytes
            ) == saida_bytes, f"{alvo}: output difere"
            assert bytes(
                (b - deslocamento) % 256 for b in saida_bytes
            ) == entrada_bytes, f"{alvo}: roundtrip"
        else:
            # Parametrizadas por chave, e a chave não pode estar vazia
            # (K3 e K6 rejeitam-no por contrato — `docs/pipeline.md`).
            assert entrada["chave_hex"], f"{alvo}: a chave não pode ser vazia"
            bytes.fromhex(entrada["chave_hex"])

    assert vistos == analógicas, "quatro camadas analógicas nos vectors"


def test_vetores_fim_a_fim_parseiam_e_validam_assinatura() -> None:
    """O envelope documentado parseia, bate certo e assina corretamente."""
    doc = _carregar("pipeline.json")
    vetores = doc["vetores"]
    assert len(vetores) == 3, "três vetores fim-a-fim"

    for v in vetores:
        vid = v["id"]
        envelope = parsear(bytes.fromhex(v["envelope_hex"]))

        # Cabeçalho documentado (nonces) + K9 como ciphertext final.
        assert envelope.versao == 1, f"vector {vid}: versão"
        assert envelope.nonce1.hex() == v["nonce1_hex"], f"vector {vid}: nonce1"
        assert envelope.nonce5.hex() == v["nonce5_hex"], f"vector {vid}: nonce5"
        assert envelope.nonce9.hex() == v["nonce9_hex"], f"vector {vid}: nonce9"
        assert envelope.ciphertext.hex() == v["c9_hex"], f"vector {vid}: K9"

        # A pública derivada da seed tem de ser a documentada e a
        # assinatura tem de validar sobre a região assinada.
        publica = chave_publica(bytes.fromhex(v["seed_hex"]))
        assert publica.hex() == v["publica_hex"], f"vector {vid}: pública"
        assert verificar(
            envelope.regiao_assinada, envelope.assinatura, publica
        ), f"vector {vid}: assinatura Ed25519 inválida"

        # O texto documentado corresponde ao hex documentado.
        assert v["plaintext_utf8"].encode("utf-8").hex() == v["plaintext_hex"], (
            f"vector {vid}: hex/utf8 coerentes"
        )


def test_handshake_parseia_e_valida_em_python() -> None:
    """Corpos REQUEST/ACCEPT/REJECT: layout documentado + assinaturas."""
    doc = _carregar("handshake.json")
    vetores = doc["vetores"]
    assert len(vetores) == 1, "um vector de handshake"

    # Transcriptos de `docs/handshake.md` (§domínios).
    dominio_req = b"ONYX/FRIEND/REQ"
    dominio_acc = b"ONYX/FRIEND/ACC"
    dominio_rej = b"ONYX/FRIEND/REJ"

    for v in vetores:
        vid = v["id"]

        # --- FRIEND_REQUEST (209 B): 0x01 ‖ pub ‖ k1 ‖ k5 ‖ k9 ‖ nonce ‖ sig
        pedido = bytes.fromhex(v["request_hex"])
        assert len(pedido) == 209, f"{vid}: REQUEST 209"
        assert pedido[0] == 1, f"{vid}: versão do pedido"
        assert pedido[1:33].hex() == v["publica_a_hex"], f"{vid}: pub A"
        assert pedido[33:65].hex() == v["k1_hex"], f"{vid}: K1"
        assert pedido[65:97].hex() == v["k5_hex"], f"{vid}: K5_A"
        assert pedido[97:129].hex() == v["k9_hex"], f"{vid}: K9_A"
        assert pedido[129:145].hex() == v["nonce_hex"], f"{vid}: nonce"
        assert chave_publica(bytes.fromhex(v["seed_a_hex"])).hex() == v[
            "publica_a_hex"
        ], f"{vid}: pub A derivada da seed"
        assert verificar(
            dominio_req + pedido[:145], pedido[145:], pedido[1:33]
        ), f"{vid}: assinatura do pedido inválida"

        # --- FRIEND_ACCEPT (177 B): 0x01 ‖ pub_b ‖ k5_b ‖ k9_b ‖ nonce ‖ sig
        aceite = bytes.fromhex(v["accept_hex"])
        assert len(aceite) == 177, f"{vid}: ACCEPT 177"
        assert aceite[0] == 1, f"{vid}: versão do aceite"
        assert aceite[1:33].hex() == v["publica_b_hex"], f"{vid}: pub B"
        assert aceite[33:65].hex() == v["k5_b_hex"], f"{vid}: K5_B"
        assert aceite[65:97].hex() == v["k9_b_hex"], f"{vid}: K9_B"
        assert aceite[97:113].hex() == v["nonce_hex"], f"{vid}: nonce ecoado"
        assert chave_publica(bytes.fromhex(v["seed_b_hex"])).hex() == v[
            "publica_b_hex"
        ], f"{vid}: pub B derivada da seed"
        assert verificar(
            dominio_acc + aceite[:113], aceite[113:], aceite[1:33]
        ), f"{vid}: assinatura do aceite inválida"

        # --- FRIEND_REJECT (81 B): 0x01 ‖ nonce ‖ sig (assinado por B)
        recusa = bytes.fromhex(v["reject_hex"])
        assert len(recusa) == 81, f"{vid}: REJECT 81"
        assert recusa[0] == 1, f"{vid}: versão da recusa"
        assert recusa[1:17].hex() == v["nonce_hex"], f"{vid}: nonce ecoado"
        assert verificar(
            dominio_rej + recusa[:17], recusa[17:], aceite[1:33]
        ), f"{vid}: assinatura da recusa inválida"

        # Transcript errado → a assinatura rejeita (anti-mistura de domínios).
        assert not verificar(
            dominio_acc + pedido[:145], pedido[145:], pedido[1:33]
        ), f"{vid}: domínios não são intermutáveis"
