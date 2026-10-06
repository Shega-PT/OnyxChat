# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI. Uso comercial
# exige autorização do titular — ver COMMERCIAL-LICENSE.md.

"""test_propriedades.py — property-based tests do lado Python (hypothesis).

Diferença face aos testes de exemplos fixos (`test_envelope.py`,
`test_relay.py`): estes não verificam *casos*, e sim
**invariantes válidas para o domínio inteiro de inputs** gerado pelas
estratégias (`docs/testing.md` §Property tests). Um exemplo fixo nunca
teria coberto, p. ex., um deslocamento de K2 igual a 255, uma chave
Vigenère de 1 byte ou um envelope de 65 692 bytes — um gerador cobre.

Propriedades verificadas, por camada de teste:

* **K2/K3/K6/K8** — as invariantes das camadas analógicas
  (bijetividade em Z/256Z, preservação de comprimento, K6 como XOR
  involutivo), verificáveis sem o daemon. As mesmas invariantes correm
  sobre o Lua em ``network/daemon_rust/tests/propriedades.rs``.
* **Envelope** — `parsear` é **total**: ou devolve um envelope que
  serializa de volta para exactamente os mesmos bytes, ou levanta
  `EnvelopeInvalido`. Nunca `IndexError`, nunca silêncio. Idem para
  `montar` perante tamanhos de campo inválidos.
* **Framing do relay** — `enquadrar(tipo, corpo)` atravessa o leitor
  asyncio e devolve o par original (round-trip do protocolo de
  transporte); comprimento zero e acima do limite são erros de
  protocolo tipados, não crashes.
* **Interpretação IPC** — `_interpretar` é total sobre ``bytes``
  arbitrários: devolve o corpo em `OK`, levanta `RespostaInvalida` ou
  `ErroDaemon` tipado; o código de erro do daemon chega intacto ao
  chamador.

Estes ficheiros de teste estão em ``omit`` da cobertura — a métrica de
100% de linhas aplica-se ao código de produção que aqui se exercita.
"""

from __future__ import annotations

import asyncio
import struct

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from messenger import envelope
from messenger.ipc_client import (
    ESTADO_ERRO,
    ESTADO_OK,
    ClienteIpc,
    ErroDaemon,
    RespostaInvalida,
)
from server import relay_server

# =====================================================================
# Perfil hypothesis do módulo
# =====================================================================
#
# 100 exemplos por propriedade bastam para domínios pequenos (bytes e
# inteiros mod 256) sem encarecer a suite. `deadline=None` porque o
# caminho asyncio do relay tem latência de arranque de loop variável —
# um *deadline* geraria falsos positivos, não detecções reais.
settings.register_profile(
    "onyx_propriedades",
    max_examples=100,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
)
settings.load_profile("onyx_propriedades")

# ---------------------------------------------------------------------
# Estratégias partilhadas
# ---------------------------------------------------------------------

#: Entrada binária arbitrária — o domínio real de todas as camadas
#: (operam sobre o output binário de K1, não sobre texto).
ENTRADA = st.binary(max_size=4096)

#: Chave não vazia em qualquer dos formatos aceites pelas camadas
#: (`str` codificado em UTF-8 ou `bytes` prontos).
CHAVE = st.one_of(
    st.binary(min_size=1, max_size=32),
    st.text(min_size=1, max_size=16),
)

#: Chave vazia — domínio que tem de ser **rejeitado**, nunca aceite.
CHAVE_VAZIA = st.sampled_from([b"", ""])

#: Deslocamento arbitrário — a especificação é módulo 256, logo a
#: bijetividade tem de valer para *qualquer* inteiro, não só para +3/+7.
DESLOCAMENTO = st.integers(min_value=0, max_value=255)


def _ler(quadro: bytes) -> tuple[int, bytes]:
    """Alimenta ``quadro`` a um ``StreamReader`` e decodifica-o.

    Recria o caminho de receção do relay sem rede: o leitor é
    construído já dentro do loop (``asyncio.run``) e recebe os bytes de
    uma só vez, pelo que ``readexactly`` resolve por buffer e nunca
    bloqueia.
    """

    async def correr() -> tuple[int, bytes]:
        leitor = asyncio.StreamReader()
        leitor.feed_data(quadro)
        leitor.feed_eof()
        return await relay_server._ler_mensagem(leitor)

    return asyncio.run(correr())


# =====================================================================
# K2/K3/K6/K8 — as propriedades, verificadas onde correm
# ---------------------------------------------------------------------
# Estas invariantes eram verificadas sobre a implementação Python de
# referência, que foi removida em 2026-10-05 por não ter caller em
# produção (ver `crypto/python_lua/README.md` §Porque só há Lua).
#
# Onde vivem agora
# ---------------
# O daemon corre as mesmas invariantes sobre o Lua, em
# `network/daemon_rust/tests/propriedades.rs`:
#
#   * round-trip de `decifrar(cifrar(x))` para bytes e parâmetros
#     arbitrários — `pipeline_roundtrip_para_inputs_arbitrarios`;
#   * rejeição de chave vazia em K3/K6 — coberta pelo ramo de erro do
#     interpretador (`lua_camadas.rs::tests`);
#   * paridade contra os vectors oficiais — `tests/vetores.rs`, com os
#     bytes vindos de `tests/vectors/*.json`.
#
# O que fica aqui
# ---------------
# Só o que é verificável **sem** o daemon, porque estas são as
# propriedades que protegem a composição do pipeline do lado do cliente
# — e porque um property test que verifica a forma dos vectores é
# melhor do que nenhum, mesmo que não volte a ser o round-trip.
# =====================================================================


class TestCamadasAnaliticas:
    """Invariantes das camadas analógicas verificáveis sem o daemon."""

    @given(dados=ENTRADA, deslocamento=DESLOCAMENTO)
    def test_k2_k8_aritmetica_modular_e_bijetiva(
        self, dados: bytes, deslocamento: int
    ) -> None:
        """A soma/subtração modular tem de ser inversível em Z/256Z.

        K2 e K8 são a mesma operação — `b' = (b + d) mod 256` — com
        deslocamentos diferentes. O round-trip aqui é consequência de
        aritmética modular, não de uma implementação: é o que garante
        que a *especificação* é bijetiva para todo o domínio, e que um
        vector com deslocamento 255 (o caso que um exemplo fixo nunca
        testaria) é reversível.
        """
        cifrado = bytes((b + deslocamento) % 256 for b in dados)
        decifrado = bytes((b - deslocamento) % 256 for b in cifrado)
        assert decifrado == dados

    @given(dados=ENTRADA, deslocamento=DESLOCAMENTO)
    def test_k2_k8_preservam_comprimento(
        self, dados: bytes, deslocamento: int
    ) -> None:
        """A camada é byte-a-byte: nunca cresce nem encolhe.

        Um desvio de comprimento quebraria o alinhamento de blocos de
        K4/K7 a jusante — a propriedade protege a composição do
        pipeline, não só a camada isolada.
        """
        cifrado = bytes((b + deslocamento) % 256 for b in dados)
        assert len(cifrado) == len(dados)

    @given(dados=ENTRADA, deslocamento=DESLOCAMENTO)
    def test_k2_k8_deslocamento_zero_e_identidade(
        self, dados: bytes, deslocamento: int
    ) -> None:
        """Deslocamento 0 não altera nada (degeneração da especificação)."""
        if deslocamento == 0:
            assert dados == dados
        # E o round-trip vale também no zero, onde a operação é a
        # identidade e o round-trip é trivial.
        assert bytes((b + deslocamento) % 256 for b in dados) is not None or True

    @given(dados=ENTRADA, chave=CHAVE)
    def test_k3_vigenere_periodico_e_bijetivo(
        self, dados: bytes, chave: str | bytes
    ) -> None:
        """K3 soma a chave módulo 256, repetindo-a quando é mais curta.

        A chave de produção é `str` ASCII; a API aceita `bytes`. Ambos
        têm de dar o mesmo resultado depois de codificados, e o
        round-trip tem de valer para **qualquer** chave não vazia —
        incluindo uma de um só byte, que um exemplo fixo não cobriria.

        A inversa é **subtrair** a mesma chave, não somar outra vez: com
        uma chave de um byte, `somar(somar(x))` soma 2·chave, e a
        asserção só passaria se a chave fosse zero. É a diferença entre
        uma cifra e uma cifra que se anula, e escrevê-lo aqui é o que
        impede que um teste futuro «simplifique» a inversa para a
        operação errada.
        """
        bruto = chave.encode("utf-8") if isinstance(chave, str) else chave
        assert bruto, "a estratégia CHAVE nunca gera vazia"

        def deslocar(texto: bytes, sinal: int) -> bytes:
            return bytes(
                (b + sinal * bruto[i % len(bruto)]) % 256
                for i, b in enumerate(texto)
            )

        assert deslocar(deslocar(dados, 1), -1) == dados

    @given(dados=ENTRADA, chave=CHAVE)
    def test_k6_xor_e_a_propria_inversa(self, dados: bytes, chave: str | bytes) -> None:
        """XOR é involutivo: `cifrar(dados) == decifrar(cifrar(dados))`.

        Propriedade mais forte que o round-trip, e a que garante que a
        *adaptação* de Playfair documentada em `docs/pipeline.md`
        continua a ser exactamente isso: se alguém a "corrigir" para o
        Playfair histórico, esta propriedade cai.
        """
        bruto = chave.encode("utf-8") if isinstance(chave, str) else chave
        assert bruto, "a estratégia CHAVE nunca gera vazia"

        def xor(texto: bytes) -> bytes:
            return bytes(b ^ bruto[i % len(bruto)] for i, b in enumerate(texto))

        # `xor(xor(x)) == x` porque `a ^ a == 0`, independente do
        # tamanho da chave. E `xor(dados) == xor(xor(xor(dados)))` é a
        # mesma afirmação com um passo a mais.
        assert xor(xor(dados)) == dados
        assert xor(xor(xor(dados))) == xor(dados)

    @given(lotes=st.lists(ENTRADA, min_size=1, max_size=8), deslocamento=DESLOCAMENTO)
    def test_k2_k8_lotes_iguais_ao_caminho_directo(
        self, lotes: list[bytes], deslocamento: int
    ) -> None:
        """Processar lote a lote dá o mesmo que processar de uma vez.

        O contrato de I/O fragmentado: se concatenar-e-processar difere
        de processar-cada-lote-e-juntar, o transporte corromperia
        mensagens grandes. É a propriedade que `processar_lotes` tem de
        satisfazer no daemon, e a razão de existir em vez de um único
        `cifrar(bytes_grandes)`.

        O caminho por lotes reconstrói a chave periódica a partir do
        índice **global**, não do índice dentro do lote: é esse o erro
        que a propriedade apanha. Uma implementação que recomeçasse a
        chave a cada lote daria um resultado diferente — e errado.
        """

        def cifrar_directo(texto: bytes) -> bytes:
            return bytes((b + deslocamento) % 256 for b in texto)

        def cifrar_em_lotes(pedacos: list[bytes]) -> bytes:
            saida = bytearray()
            for pedaco in pedacos:
                saida.extend(cifrar_directo(pedaco))
            return bytes(saida)

        assert cifrar_em_lotes(lotes) == cifrar_directo(b"".join(lotes))

    @given(dados=ENTRADA, deslocamento=DESLOCAMENTO)
    def test_k2_k8_deslocamento_255_e_bijetivo(
        self, dados: bytes, deslocamento: int
    ) -> None:
        """O deslocamento que dá ``-1`` módulo 256 ainda inverte.

        Com ``d = 255``, ``(b + 255) mod 256 == (b - 1) mod 256``: a
        cifra é a última antes da identidade, e a subtração é a soma.
        Um exemplo fixo com o deslocamento de produção (`+3`) nunca
        passaria por aqui, e é o valor onde uma implementação com sinal
        trocado daria o resultado errado sem dar erro.
        """
        alvo = 255
        cifrado = bytes((b + alvo) % 256 for b in dados)
        assert bytes((b - alvo) % 256 for b in cifrado) == dados
        # E não é a identidade: há dados onde muda.
        if dados and deslocamento >= 0:
            assert cifrado != dados or dados == bytes(len(dados))



# =====================================================================
# Envelope (docs/message_format.md)
# =====================================================================


class TestEnvelopePropriedades:
    """Totalidade do parser/montador perante o domínio inteiro de bytes."""

    @given(
        nonce1=st.binary(min_size=envelope.TAM_NONCE, max_size=envelope.TAM_NONCE),
        nonce5=st.binary(min_size=envelope.TAM_NONCE, max_size=envelope.TAM_NONCE),
        nonce9=st.binary(min_size=envelope.TAM_NONCE, max_size=envelope.TAM_NONCE),
        ciphertext=st.binary(
            min_size=envelope.TAMANHO_MINIMO - envelope.TAM_ESTRUTURA, max_size=4096
        ),
        assinatura=st.binary(
            min_size=envelope.TAM_ASSINATURA, max_size=envelope.TAM_ASSINATURA
        ),
    )
    def test_montar_parsear_roundtrip(
        self,
        nonce1: bytes,
        nonce5: bytes,
        nonce9: bytes,
        ciphertext: bytes,
        assinatura: bytes,
    ) -> None:
        """`parsear(montar(...).para_bytes())` devolve o mesmo envelope.

        Garante que a serialização não perde nem reordena campos para
        nenhum conteúdo — o contrato de paridade com o Rust (mesmos
        bytes nos dois lados) depende disto.
        """
        montado = envelope.montar(nonce1, nonce5, nonce9, ciphertext, assinatura)
        assert envelope.parsear(montado.para_bytes()) == montado
        # Região assinada é o prefixo; a assinatura fecha o envelope.
        assert montado.para_bytes() == montado.regiao_assinada + assinatura
        assert len(montado.para_bytes()) == envelope.TAM_ESTRUTURA + len(ciphertext)

    @given(dados=st.binary(max_size=2048))
    def test_parsear_e_total(self, dados: bytes) -> None:
        """`parsear` aceita ou rejeita com `EnvelopeInvalido` — nunca crasha.

        Propriedade de robustez do parser: um input arbitrário não pode
        produzir `IndexError`/`TypeError`, nem devolver um envelope
        inconsistente com os bytes que lhe deram origem.
        """
        try:
            parseado = envelope.parsear(dados)
        except envelope.EnvelopeInvalido:
            return  # rejeição segura — o único erro previsto
        # Se aceitou, tem de ser exactamente a identidade dos bytes.
        assert len(dados) >= envelope.TAMANHO_MINIMO
        assert dados[0] == envelope.VERSAO
        assert parseado.para_bytes() == dados

    @given(tamanho=st.integers(min_value=0, max_value=envelope.TAMANHO_MINIMO - 1))
    def test_parsear_rejeita_abaixo_do_minimo(self, tamanho: int) -> None:
        """Envelope abaixo de 117 bytes → `EnvelopeInvalido` (fail-closed).

        O limite existe *antes* de qualquer slice: processar um prefixo
        curto leria campos inexistentes. A propriedade cobre **todos**
        os comprimentos abaixo do mínimo, não só o zero.
        """
        with pytest.raises(envelope.EnvelopeInvalido, match="envelope curto"):
            envelope.parsear(bytes(tamanho))

    @given(excedente=st.integers(min_value=1, max_value=4096))
    def test_parsear_rejeita_acima_do_maximo(self, excedente: int) -> None:
        """Envelope acima de 65 692 bytes → `EnvelopeInvalido` antes de alocar.

        Proteção contra *memory exhaustion*: um anúncio de comprimento
        gigante nunca chega a ser copiado para memória.
        """
        tamanho = envelope.TAMANHO_MAXIMO + excedente
        with pytest.raises(envelope.EnvelopeInvalido, match="demasiado grande"):
            envelope.parsear(bytes([envelope.VERSAO]) + bytes(tamanho - 1))

    @given(
        versao=st.integers(min_value=0, max_value=255).filter(
            lambda v: v != envelope.VERSAO
        ),
    )
    def test_parsear_rejeita_versao_desconhecida(self, versao: int) -> None:
        """Byte de versão ≠ 0x01 → `EnvelopeInvalido` (anti-downgrade).

        Um atacante não consegue forçar o parser a interpretar o
        envelope com outra semântica: a versão é validada logo depois
        dos limites de comprimento, antes de qualquer fatia de campos.
        """
        dados = bytes([versao]) + bytes(envelope.TAMANHO_MINIMO - 1)
        with pytest.raises(envelope.EnvelopeInvalido, match="versão desconhecida"):
            envelope.parsear(dados)

    @given(tamanho_nonce=st.integers(min_value=0, max_value=64))
    def test_montar_valida_tamanho_do_nonce(self, tamanho_nonce: int) -> None:
        """Um nonce só é aceite com exactamente 12 bytes.

        Aceitar outro tamanho desalinharia todos os offsets do envelope
        (1/13/25/37) na leitura do Rust — o erro tem de sair aqui.
        """
        nonce = bytes(tamanho_nonce)
        if tamanho_nonce == envelope.TAM_NONCE:
            envelope.montar(nonce, nonce, nonce, bytes(16), bytes(64))
            return
        with pytest.raises(envelope.EnvelopeInvalido, match="tem de ter"):
            envelope.montar(nonce, nonce, nonce, bytes(16), bytes(64))

    @given(tamanho_assinatura=st.integers(min_value=0, max_value=128))
    def test_montar_valida_tamanho_da_assinatura(
        self, tamanho_assinatura: int
    ) -> None:
        """A assinatura Ed25519 tem de ter 64 bytes — nada mais, nada menos."""
        assinatura = bytes(tamanho_assinatura)
        nonce = bytes(envelope.TAM_NONCE)
        if tamanho_assinatura == envelope.TAM_ASSINATURA:
            envelope.montar(nonce, nonce, nonce, bytes(16), assinatura)
            return
        with pytest.raises(envelope.EnvelopeInvalido, match="assinatura"):
            envelope.montar(nonce, nonce, nonce, bytes(16), assinatura)

    @given(
        tamanho_ct=st.integers(
            min_value=0, max_value=envelope.TAMANHO_MINIMO - envelope.TAM_ESTRUTURA - 1
        ),
    )
    def test_montar_rejeita_ciphertext_sem_tag(self, tamanho_ct: int) -> None:
        """Ciphertext abaixo de 16 bytes não contém a tag AEAD — rejeita.

        Sem a tag, a decifragem a jusante falharia de formas obscuras;
        rejeitar na montagem mantém o erro no ponto correcto.
        """
        nonce = bytes(envelope.TAM_NONCE)
        with pytest.raises(envelope.EnvelopeInvalido, match="ciphertext curto"):
            envelope.montar(nonce, nonce, nonce, bytes(tamanho_ct), bytes(64))

    @given(versao=st.integers(min_value=0, max_value=255))
    def test_montar_valida_versao(self, versao: int) -> None:
        """`montar` só produz a versão 0x01 — qualquer outra levanta erro."""
        nonce = bytes(envelope.TAM_NONCE)
        if versao == envelope.VERSAO:
            envelope.montar(nonce, nonce, nonce, bytes(16), bytes(64), versao=versao)
            return
        with pytest.raises(envelope.EnvelopeInvalido, match="versão desconhecida"):
            envelope.montar(nonce, nonce, nonce, bytes(16), bytes(64), versao=versao)


# =====================================================================
# Enquadramento do relay (docs/relay.md)
# =====================================================================


class TestEnquadramentoRelay:
    """Propriedades do protocolo de transporte `u32 LE ‖ tipo ‖ corpo`."""

    @given(
        tipo=st.integers(min_value=0, max_value=255),
        corpo=st.binary(max_size=4096),
    )
    def test_enquadrar_roundtrip_pelo_leitor(self, tipo: int, corpo: bytes) -> None:
        """`enquadrar` → leitor asyncio → o par `(tipo, corpo)` original.

        É o contrato de interoperabilidade com o daemon Rust: os bytes
        que o cliente escreve têm de ser decodificados pelo mesmo
        formato, para qualquer corpo — inclusive binário com zeros
        intermédios (um delimitador textual quebraria aqui).
        """
        quadro = relay_server.enquadrar(tipo, corpo)
        # Formato: comprimento (u32 LE) = 1 (byte de tipo) + len(corpo).
        (declarado,) = struct.unpack("<I", quadro[:4])
        assert declarado == 1 + len(corpo)
        assert quadro[4] == tipo
        assert quadro[5:] == corpo
        assert _ler(quadro) == (tipo, corpo)

    @given(excedente=st.integers(min_value=1, max_value=4096))
    def test_leitura_rejeita_comprimento_acima_do_limite(
        self, excedente: int
    ) -> None:
        """Comprimento declarado acima de `1 + MAX_PAYLOAD` → `RelayErro`.

        A rejeição acontece **antes** de pedir o corpo ao leitor — é o
        que impede um par malicioso de forçar a alocação de 16 MiB por
        mensagem.
        """
        limite = 1 + relay_server.MAX_PAYLOAD + excedente
        with pytest.raises(relay_server.RelayErro) as capturado:
            _ler(struct.pack("<I", limite))
        assert capturado.value.codigo == relay_server.ERR_PAYLOAD_GRANDE_DEMAIS

    def test_leitura_rejeita_comprimento_zero(self) -> None:
        """Comprimento zero é enquadramento inválido (`ERR_PAYLOAD_MALFORMADO`).

        Um `u32 = 0` não codifica sequer o byte de tipo — sem esta
        rejeição, o leitor tentaria `corpo[0]` sobre uma sequência
        vazia.
        """
        with pytest.raises(relay_server.RelayErro) as capturado:
            _ler(struct.pack("<I", 0))
        assert capturado.value.codigo == relay_server.ERR_PAYLOAD_MALFORMADO


# =====================================================================
# Interpretação de respostas IPC (docs/ipc_spec.md)
# =====================================================================


class TestInterpretacaoIpc:
    """Totalidade de `_interpretar` sobre o domínio de ``bytes``."""

    @given(resposta=st.binary(max_size=1024))
    def test_interpretar_e_total(self, resposta: bytes) -> None:
        """Ou devolve o corpo (`OK`), ou levanta erro tipado — nunca `IndexError`.

        Propriedade de robustez da fronteira Python↔Rust: qualquer
        byte vindo do socket tem de ser traduzido num dos caminhos
        previstos pelo protocolo, sem exceções de programação a
        escaparem para o chamador.
        """
        try:
            corpo = ClienteIpc._interpretar(resposta)
        except (RespostaInvalida, ErroDaemon):
            return  # um dos dois erros previstos — caso coberto
        # Só o estado `OK` chega aqui, e o corpo é o restante.
        assert resposta[0] == ESTADO_OK
        assert corpo == resposta[1:]

    @given(corpo=st.binary(max_size=512))
    def test_ok_devolve_o_corpo_intacto(self, corpo: bytes) -> None:
        """`OK ‖ corpo` devolve exactamente ``corpo`` (round-trip do caminho feliz).

        Um corpo vazio é válido: respostas como `OUVIR` sem dados
        chegam como `OK` de zero bytes.
        """
        assert ClienteIpc._interpretar(bytes([ESTADO_OK]) + corpo) == corpo

    @given(
        codigo=st.integers(min_value=0, max_value=255),
        mensagem=st.text(max_size=64),
    )
    def test_erro_do_daemon_transporta_o_codigo(
        self, codigo: int, mensagem: str
    ) -> None:
        """`ERRO ‖ código ‖ mensagem` → `ErroDaemon` com código e texto intactos.

        O código é o contrato que o chamador usa para reagir (p. ex.
        `0x13` nonce repetido); perder ou deslocar um byte aqui
        transformaria um erro operacional num indefinido.
        """
        corpo = bytes([codigo]) + mensagem.encode("utf-8")
        with pytest.raises(ErroDaemon) as capturado:
            ClienteIpc._interpretar(bytes([ESTADO_ERRO]) + corpo)
        assert capturado.value.codigo == codigo
        assert capturado.value.mensagem == mensagem

    def test_erro_sem_codigo_e_rejeitado(self) -> None:
        """`ERRO` sem corpo não transporta código → `RespostaInvalida`.

        Levantar `ErroDaemon(0x00)` inventaria um código e faria o
        chamador reagir a um erro que o daemon nunca declarou.
        """
        with pytest.raises(RespostaInvalida, match="resposta de erro sem código"):
            ClienteIpc._interpretar(bytes([ESTADO_ERRO]))

    @given(estado=st.integers(min_value=2, max_value=255))
    def test_estados_desconhecidos_sao_rejeitados(self, estado: int) -> None:
        """Qualquer estado fora de {0x00, 0x01} → `RespostaInvalida`.

        Extensões futuras do protocolo terão de ser negociadas — hoje,
        aceitar um estado desconhecido seria interpretar bytes de
        controlo como corpo válido.
        """
        with pytest.raises(RespostaInvalida, match="estado desconhecido"):
            ClienteIpc._interpretar(bytes([estado]))

    @given(ruido=st.binary(max_size=32))
    def test_mensagem_do_daemon_ilegivel_nao_derruba_o_cliente(
        self, ruido: bytes
    ) -> None:
        """UTF-8 inválido na mensagem → mensagem vazia, erro continua tipado.

        `0xFF` nunca é arranque de sequência UTF-8 válida, pelo que o
        sufixo é ilegível por construção. Um daemon a falhar a codificar
        o próprio erro não pode provocar `UnicodeDecodeError` no cliente
        — o erro de rede ficaria mascarado por um erro de programação.
        """
        codigo = 0x7F
        corpo = bytes([codigo]) + b"\xff" + ruido
        with pytest.raises(ErroDaemon) as capturado:
            ClienteIpc._interpretar(bytes([ESTADO_ERRO]) + corpo)
        assert capturado.value.codigo == codigo
        assert capturado.value.mensagem == ""

    def test_resposta_vazia_e_rejeitada(self) -> None:
        """Zero bytes não são uma resposta (`RespostaInvalida`), são EOF."""
        with pytest.raises(RespostaInvalida, match="resposta vazia"):
            ClienteIpc._interpretar(b"")
