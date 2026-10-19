# `sidecar.md` — o contrato HTTP local

Este documento existe porque **três ficheiros de código o referenciam e
ele não existia**:

* `server/sidecar.py` — a decisão de não implementar WebSocket aponta
  para aqui;
* `server/origem.py` — a política de origens remete para aqui;
* `tests/test_origem.py` — um teste de guarda depende de `ONYX_ORIGENS`
  estar documentado, porque um nome mudado sem mudar a documentação
  faria a excepção desaparecer **em silêncio**.

Um documento que três sítios apontam e que não está escrito não é um
documento em falta: é uma referência pendurada, e uma referência
pendurada não se repará sozinha. É o que aconteceu até 2026-10-13, com a
decisão do WebSocket escrita no sítio certo — `UI/README.md` §Não há
WebSocket — e apontada a partir do sítio errado.

| Documento | Especifica |
| --- | --- |
| [`ipc.md`](ipc.md) | o protocolo Python↔daemon, pelo socket Unix |
| `sidecar.md` | o protocolo HTTP, entre a interface e o sidecar |

São **duas fronteiras diferentes** com sistemas de segurança diferentes, e
confundi-las é o erro que este documento existe para impedir.

---

## §O que é

O sidecar é um servidor HTTP em `127.0.0.1:8787` que serve duas coisas:

1. a interface construída (`UI/dist/`, em [`../UI/README.md`](../UI/README.md));
2. uma API em `/api`, que é o que [`bridge-adapter.js`](../UI/src/lib/onyx/bridge-adapter.js) fala.

Ele não fala Tor, não fala com o relay e não conhece o pipeline. Fala com
o `onyxchatd` **pelo socket Unix** e traduz pedidos em comandos IPC —
[`ipc.md`](ipc.md) §Comandos. A separação é deliberada: o
sidecar é a única peça que conhece os dois lados, e é a única que pode
escrever na loja de conversas.

Quem o arranca é `user/cli.py sidecar`, que **abre a conta antes de
servir**. Um sidecar sem conta aberta responde `503` às rotas que
dependem dela, e isso é dito no corpo do erro — não há um estado
misterioso em que a interface aparente funcionar.

---

## §A tabela de rotas

**38 rotas**, todas em `server/rotas.py`. A contagem e a correspondência
caminho a caminho são conferidas por
`tests/test_sidecar.py::test_a_tabela_de_rotas_da_documentacao_esta_certa`,
e não é decorativa: uma rota acrescentada sem documentar aqui é uma rota
que responde a pedidos que ninguém espera, e uma rota documentada que
já não existe é um botão partido que ainda parece vivo.

| Método | Caminho | Efeito |
| --- | --- | --- |
| `GET` | `/identidade` | identidade e impressão digital |
| `GET` | `/contagens` | contadores para a barra lateral |
| `GET` | `/conversas` | lista, com `?pesquisa=` |
| `GET` | `/conversas/{id}` | uma conversa e as suas mensagens |
| `POST` | `/mensagens` | **guarda texto**, não envelopes |
| `POST` | `/conversas/{id}/lida` | marca a conversa e as suas mensagens como lidas |
| `POST` | `/conversas/{id}/fixada` | fixa ou desfixa — corpo `{"valor": true\|false}` |
| `POST` | `/conversas/{id}/silenciada` | silencia ou dessilencia — corpo `{"valor": true\|false}` |
| `POST` | `/conversas/{id}/limpar` | apaga as mensagens, mantém a conversa |
| `GET` | `/conversas/{id}/transferencias` | transferências de media, com `?completas=` |
| `GET` | `/transferencias/{id}` | uma transferência, com `recebidasPedacos` |
| `DELETE` | `/transferencias/{id}` | apaga a transferência e **as suas** fatias |
| `POST` | `/transferencias/{id}/reproduzir` | monta em RAM e lança o leitor; devolve o `pid` |
| `GET` | `/media/estado` | se há leitor, e porque não há se não há |
| `POST` | `/media/streaming/iniciar` | liga a captura e a codificação VP8 |
| `GET` | `/media/streaming/quadro` | o próximo quadro de VP8, em base-64 |
| `GET` | `/media/streaming/audio` | o próximo pacote de Opus, em base-64, com a sequência e o `clockMs` |
| `GET` | `/armazenamento` | **todas** as transferências da conta, de todas as conversas |
| `POST` | `/ficheiros` | envia um ficheiro — corpo `{"conversa": id, "nome": "…", "conteudo": base64}` |
| `GET` | `/armazenamento` | **todas** as transferências da conta, de todas as conversas |
| `POST` | `/ficheiros` | envia um ficheiro — corpo `{"conversa": id, "nome": "…", "conteudo": base64}` |
| `POST` | `/media/streaming/imagem` | descodifica um VP8 para JPEG — corpo `{"vp8": base64, "largura": n, "altura": n}` |
| `POST` | `/media/streaming/parar` | diz à UI que pode deixar de pedir |
| `GET` | `/contactos` | lista, com `?pesquisa=` |
| `POST` | `/contactos/{id}/bloqueado` | bloqueia ou desbloqueia — corpo `{"valor": true\|false}` |
| `POST` | `/pedidos` | pede contacto |
| `GET` | `/pedidos` | pedidos recebidos, `?incluirBloqueados=` |
| `POST` | `/pedidos/{id}` | aceita ou recusa |
| `GET` | `/estado` | estado da rede, via IPC ao daemon |
| `GET` | `/seguranca` | o bloco `security` |
| `GET` | `/definicoes` | lê as definições |
| `PUT` | `/definicoes` | grava as definições |
| `GET` | `/descoberta/sugestao` | identidade sugerida |
| `GET` | `/descoberta` | resolve `ID → .onion` |
| `GET` | `/conta/estado` | `sem-conta`, `trancada` ou `desbloqueada` |
| `POST` | `/conta` | cria a conta |
| `POST` | `/conta/desbloquear` | abre com a frase de segurança |
| `POST` | `/conta/trancar` | tranca a sessão |
| `GET` | `/conta/exportar` | devolve o keystore cifrado, em base64 |
| `POST` | `/conta/restaurar` | repõe de uma cópia |

### As cinco rotas de media

Vêm de [`media.md`](media.md) §4 e são as **únicas** que a interface
pode chamar para media: a reprodução não passa por aqui (`docs/sidecar.md`
§O canal de eventos e `media.md` §4.6), e o que chega ao ecrã é sempre o
**envelope**, nunca texto claro.

* `GET /conversas/{id}/transferencias` devolve **as incompletas também**,
  por omissão. Uma transferência a meio é informação, e escondê-la faz
  o utilizador concluir que não existe em vez de estar a chegar. O
  filtro `?completas=` tem três estados — `1`, `0` e `todos` — e
  `0` **não** é a mesma coisa que omitir o parâmetro.
* `GET /transferencias/{id}` acrescenta `recebidasPedacos`, que é o
  número que permite «a chegar, 42 de 204» sem o cliente contar
  envelopes.
* `DELETE /transferencias/{id}` apaga a transferência e as suas fatias,
  filtradas pelo `transferencia_id`. **Não** apaga o texto da conversa
  nem as outras transferências — e **não propaga** para o outro
  participante, porque nunca houve canal para isso.
* `POST /transferencias/{id}/reproduzir` pede ao daemon que monte em
  RAM e lance o leitor, e devolve o `pid`. Uma transferência
  **incompleta** dá `409` e não `404`: ela existe e está a meio, e
  escondê-la faria a interface dizer que não chegou nada.
* `GET /media/estado` diz se há leitor e porque não há se não há. As três
  coisas que podem faltar — daemon, loja apontada, leitor — falham
  **independentemente**, e oferecer um botão que vai falhar é pior do que
  não o oferecer.

### As cinco acções do menu de contexto

Todas são `POST`, todas com efeito, e todas são `POST` **por uma razão
de segurança**: `GET`, `HEAD` e `OPTIONS` estão isentos da exigência de
`Origin` em [`origem.py`](../server/origem.py), porque se presume que não
têm efeito. Uma acção registada como leitura ficaria de fora dessa
verificação — seria uma porta que se pode atravessar sem querer.
`tests/test_sidecar.py::test_as_rotas_do_menu_tem_todas_accao_e_todas_sao_escritas`
verifica que as cinco continuam a ser `POST`.

Recebem o valor de destino no corpo (`{"valor": true}`) em vez de
alternarem sozinhas. `{"valor": "false"}` — uma **string** — é `400`, e
não `false`: em Python toda a string não vazia é verdadeira, e um cliente
que mandasse texto em vez de JSON desligaria a conversa em vez de a
recusar. Um corpo sem o campo é `400` pelo mesmo motivo: tratá-lo como
`false` faria um pedido malformado desligar o que estava ligado.

### `/api/mensagens` guarda texto, não envelopes

A rota mais importante deste documento. `/api/mensagens` escreve **o texto
que a pessoa escreveu** na loja `sqlite3` — não o envelope. O envelope é
montado pelo daemon, no outro lado, e o que chega ao ecrã é o que a
pessoa escreveu.

Isto é o que `tests/test_e2e_sidecar.py` fixa: a loja do sidecar e o
envelope do daemon são **dois objectos diferentes**, e um teste que
confundisse os dois passaria a medir a coisa errada. Ver
[`roadmap.md`](roadmap.md) §PLANEADO para o que falta aqui.

---

## §O canal de eventos

**`GET /api/eventos` responde `501`, e é de propósito.**

O plano original era «HTTP + WebSocket». O WebSocket ficou
por fazer, e a razão está escrita:

* As vinte funções do contrato em `bridge-adapter.js` são **todas**
  pedido/resposta. Nenhuma precisa de empurrar.
* Nenhum consumidor do contrato pede um canal de empurrar.
* Uma implementação de RFC 6455 feita à mão, sem consumidor, seria
  exactamente o mecanismo especulativo que o resto deste projecto
  recusa.

Um `501` com a razão é melhor do que as duas alternativas: um
WebSocket meio implementado, que a interface não sabe usar, ou um `404`
que não diz nada e transforma uma decisão em mistério.

> A contagem é **37**, não 28 nem 22. `bridge-adapter.js` e
> `mock-adapter.js` têm 37 métodos cada, e `data-adapter.js` declara 37.
> Os **28** eram os métodos antes das rotas de reprodução; o **22** era o
> número de **rotas** desta tabela numa versão ainda mais antiga, e as
> contagens foram trocadas na documentação até 2026-10-13: `sidecar.py`
> dizia «vinte e uma funções do contrato» e `UI/README.md` dizia «22
> métodos» — nenhum dos dois era certo, e nenhum era o número do outro.
>
> A relação entre rotas e métodos não é de um para um: as **38** rotas
> dão 37 métodos, porque `/api/eventos` não tem método nenhum — não é
> pedido/resposta, responde `501` de propósito.

---

## §As origens: porque `127.0.0.1` não é privado

Este é o ponto que `server/origem.py` documenta e que `tests/test_origem.py`
vigia. Um servidor em `127.0.0.1` **não é uma fronteira de segurança**:
qualquer página num browser pode fazer `fetch` para ele, e o CORS só
impede a **leitura** da resposta — o **envio** acontece na mesma.

A defesa são quatro cabeçalhos, e estão em `origem.py`:

| Cabeçalho | Apanha |
| --- | --- |
| `Host` | *rebinding* de DNS — o nome hostil a resolver para `127.0.0.1` |
| `Origin` | o caso geral — quem pode ler |
| `Sec-Fetch-Site` | o reforço — relação real, que um cliente HTTP bruto não forja |
| `Sec-Fetch-Mode`/`Method` | `POST`/`PUT`/`DELETE` sem `Origin` |

### `ONYX_ORIGENS`

**Nome exacto: `ONYX_ORIGENS`.** É `AMBIENTE_EXTRAS` em `origem.py`, e
`tests/test_origem.py::test_a_variavel_tem_o_nome_certo` existe
precisamente para o proteger.

Uma variável vazia **não muda nada**: quem não a define fica com as três
origens por omissão.

```sh
ONYX_ORIGENS=http://127.0.0.1:8787,https://exemplo.pt onyxchat sidecar
```

As três por omissão são `http://127.0.0.1:8787`, `http://localhost:8787` e
`http://[::1]:8787`. `localhost` e `127.0.0.1` são **o mesmo endereço
escrito de duas maneiras**, e o browser trata-os como origens distintas.
Deixar `localhost` de fora faria a defesa passar a ser um problema em vez
de uma protecção: quem abrisse o URL à mão deixaria de funcionar, e
acusaria o servidor de estar partido.

### O que é leitura

`GET`, `HEAD` e `OPTIONS` — e só estes. São isentos da exigência de
`Origin` porque **não têm efeito**, e um pedido sem `Origin` é quase
sempre uma navegação. `test_origem.py::test_metodos_desconhecidos_com_efeito_sao_recusados`
existe porque a lista é **fechada**: um método novo no servidor sem
entrada aqui daria uma operação com efeito que ninguém classificou.

---

## §Erros

O corpo traz o código e a mensagem **em português**, e **nenhum detalhe
técnico**: um atacante que descubra a porta não deve ficar com uma lista
de excepções que o ajuda a mapear o programa.

| Código | Significado |
| --- | --- |
| `400` | pedido malformado |
| `403` | origem não permitida |
| `404` | não encontrado |
| `405` | método não permitido |
| `413` | corpo demasiado grande |
| `415` | tipo de conteúdo não suportado |
| `500` | erro interno |
| `501` | **não implementado** — só `/api/eventos` |
| `503` | o programa local não está disponível |

A interface traduz a chave; o corpo não é para ela. Ver
`UI/src/lib/onyx/bridge-adapter.js` §`ErroDoSidecar`.

---

## §O que este documento obriga

Ao acrescentar uma rota, três coisas têm de mudar no mesmo commit:

1. a tabela de rotas acima;
2. um teste em `tests/test_sidecar.py`;
3. um método em **ambos** os adaptadores — `mock-adapter.js` e
   `bridge-adapter.js`.

O terceiro é o que costuma ser esquecido, e o esquecimento é invisível:
a demonstração continua a funcionar porque tem um caminho próprio, e a
falha só aparece quando alguém desliga o interruptor. É a razão de os
dois adaptadores terem o **mesmo número de métodos** — o número é uma
conferência, não um dado.