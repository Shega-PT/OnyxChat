# Discovery Server (bootstrap `ID → .onion`)

## Papel

O discovery server resolve uma coisa só:

```text
Onyx-ID  →  endereço .onion atual     (com TTL)
```

É **infraestrutura de bootstrap**, não servidor central de comunicação.
Permite que um Onyx encontre o endpoint atual de outro Onyx (hidden
services efémeros trocam de endereço), nada mais.

## O que o discovery NÃO faz

* **não** armazena mensagens;
* **não** mantém conversações;
* **não** controla a sessão criptográfica;
* **não** decifra conteúdo;
* **não** funciona como servidor central obrigatório de mensagens;
* **não** participa no handshake;
* **não** conhece chaves.

Sem o discovery, o sistema não deixa de ser seguro — deixa de ser
*conveniente* (é preciso obter o `.onion` por outro canal).

---

## Protocolo

Interface HTTP local, JSON, duas rotas:

| Rota | Método | Corpo/Parâmetro | Resposta |
| --- | --- | --- | --- |
| `/registrar` | `POST` | `{"id", "onion", "ttl"?}` | `200 {}` ou `400` |
| `/onion/<id>` | `GET` | — | `200 {"id","onion"}` ou `404` |

### Validação

| Campo    | Regra                                                        |
| -------- | ------------------------------------------------------------ |
| `id`     | `^[A-Za-z0-9._-]{1,64}$`                                     |
| `onion`  | `^[a-z2-7]{56}\.onion$` (v3)                                 |
| `ttl`    | `> 0`; omissão = 300 s                                       |

Qualquer violação → `400` com mensagem fixa (sem ecoar input).

### Semântica TTL

* Cada entrada expira após `ttl` segundos (relógio monotónico).
* Consulta a entrada expirada → `404` **e** remoção imediata.
* `limpar()` remove expiradas proativamente.
* Registo repetido do mesmo `id` **substitui** a entrada anterior.

**Propósito do TTL:** os hidden services são efémeros — sem expiração,
o discovery acumularia entradas mortas e tornar-se-ia um registo
permanente de quem existiu.

---

## Discovery ≠ Routing

| Discovery (existe) | Routing (não existe) |
| --- | --- |
| mapa `ID → .onion` | transporte de dados entre participantes |
| resolve *onde* está o endpoint | move *os bytes* pelo caminho |
| TTL efémero | — |

A distinção é documentativa e arquitetural: o OnyxChat atual **não**
tem routing descentralizado (`roadmap.md` §PLANEADO).

---

## Metadados observáveis

O discovery observa:

* quem consulta que `id`;
* quem se regista, quando, e o `.onion` correspondente.

**Não observa:** mensagens, chaves, conteúdo, quem fala com quem (as
consultas indicam intenção de contato, não a comunicação em si).

**Mitigação possível (não implementada):** correr instâncias próprias
ou atrás de Tor. O protocolo não exige servidor único — qualquer
implementação que cumpra as duas rotas é compatível.

> **`PLANEADO` (F2.1) — a frequência de consultas passa a ser
> observável de forma mais granular.** Automatizar a resolução
> `ID → .onion` dentro de `ligar` significa que o discovery deixa de
> ver apenas consultas explícitas do operador e passa a ver **um
> consulta por tentativa de ligação**, sem intervenção humana. O
> conteúdo da consulta é o mesmo (quem procura que `id`); o que muda é
> a **frequência** e a correlação com instantes de actividade.
>
> Isto não é uma regressão em relação ao que já é observável — o
> discovery já vê `id` e momento de registo — mas é um aumento
> mensurável de granularidade. Está registado em
> [`privacy_model.md`](privacy_model.md) §4. Se a frequência for
> relevante para a ameaça em causa, a mitigação é correr instâncias
> próprias do discovery.

---

## Componente substituível

O discovery atual **não** é uma dependência conceptual permanente: a
arquitetura pressupõe que possa ser substituído no futuro por
mecanismos distribuídos (`roadmap.md` §PLANEADO). O que este documento
garante é que a implementação atual não torna essa substituição
estruturalmente impossível: o cliente trata o discovery como um
serviço com um contrato mínimo de duas operações.

---

## Implementação

* **Servidor:** `server/discovery_server.py` — núcleo `Descoberta` puro,
  com relógio injectável (testável sem dormir), transporte
  `http.server` da stdlib. Ponto de entrada por `__main__`, que sobe em
  `127.0.0.1:8789` (porta fixa, sem opções de linha de comando);
  para outras interfaces/portas chamar `criar_servidor(host, porta)`.
* **Cliente:** `consultar_descoberta(servidor, id)` em
  `server/discovery_server.py`, exposta pelo subcomando
  `onyxchat descobrir` em `user/cli.py`. **Não** existe um cliente de
  discovery em `messenger/` na implementação actual — uma versão
  anterior deste documento apontava para lá em erro.
* **`PLANEADO` (F2.1):** a resolução `ID → .onion` passa a estar
  disponível como função reutilizável em `messenger/`, para que `ligar`
  possa aceitar um ID hex em modo directo e resolver o `.onion` antes de
  emitir `LIGAR` ao daemon (que não tem cliente HTTP). Ver
  [`ipc_spec.md`](ipc_spec.md) §`destino`.
* **Testes:** `tests/test_discovery.py`.
