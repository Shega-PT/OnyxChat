# Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
#
# Licença base do projecto: PolyForm Noncommercial 1.0.0.
# Não é open source pela definição da OSI.
# Uso comercial exige autorização do titular — ver COMMERCIAL-LICENSE.md.

# Conta local

Como o OnyxChat guarda uma conta, o que a frase de segurança protege, e o
que este programa **não** tem.

## O que uma conta é

Não é um nome de utilizador e uma palavra-passe num servidor. É uma
identidade criptográfica num ficheiro cifrado:

| Peça | De onde vem | Para que serve |
| --- | --- | --- |
| seed Ed25519 (32 B) | `messenger.keys.Identidade.gerar()` | assinar |
| sal por conta (16 B) | `messenger.identidade.gerar_sal()` | impedir que dois nomes iguais deem o mesmo Onyx ID |
| Onyx ID | `identificador_de(nome, sal)` | ser encontrado |
| impressão (16 B) | `impressao_de(identificador)` | distinguir |

A frase de segurança não é comparada com um registo. **É a chave** com que
o ficheiro é cifrado: `messenger.storage.guardar_keystore` deriva dela uma
chave com PBKDF2-HMAC-SHA256 a 200 000 iterações e cifra com
ChaCha20-Poly1305. A confirmação de que a frase é a certa é a etiqueta
AEAD.

Isto quer dizer que não existe um campo onde a frase seja comparada, e não
deve passar a existir. Uma comparação é mais fraca do que uma etiqueta, e
guardaria a frase — ou o seu resumo — em lado nenhum.

## O ficheiro

`user/conta.keystore`, por omissão. O caminho é relativo ao **diretório
do projecto**, não ao diretório de execução: um ficheiro relativo ao
`cwd` aparece num sítio diferente conforme quem lançou o programa, que é a
forma mais comum de «a conta desapareceu».

`ONYX_CONTA` sobrepõe o caminho, e existe pelo mesmo motivo que em
`user/config.py` — os testes precisam de escrever num directório
temporário, e abrir o ficheiro a sério num teste é a forma mais rápida de
perder a conta de alguém.

O registo é cifrado com o formato de `messenger/storage.py` (magia
`ONYXKS\x00<n>`, sal e nonce no cabeçalho, `0600`) e leva dentro um
dicionário com `"tipo": "conta"` — o `tipo` é o que o distingue das
preferências, que usam a mesma mecânica.

## O que mais vive dentro do ficheiro

**As chaves de amizade.** `ACEITAR_AMIZADE` e `CONFIRMAR_AMIZADE` devolvem
192 bytes que o daemon só tem em memória, e que o cliente tem de guardar
para mandar mensagens depois. Vêm no mesmo dicionário, sob `"amizades"`,
e portanto **na mesma cifra** — a da frase de segurança.

Três decisões que valem a pena escrever, porque a escolha óbvia não é a
certa:

| onde | porquê não |
| --- | --- |
| a loja `sqlite3` do sidecar | punha chaves de mensagem num ficheiro que ninguém abre com uma frase |
| um keystore separado só para as amizades | obrigava a exportar e restaurar dois ficheiros, e o ecrã de recuperação a ter duas operações |
| **o mesmo dicionário** | uma cifra, uma cópia de segurança, e `exportar` continua a ser uma operação |

`amizades` é uma **lista de registos** e não uma tabela relacional
porque o registo é um `dataclass` congelado que vai inteiro para o
ficheiro. Acrescentar uma amizade devolve uma `Conta` **nova** —
`conta.com_amizade(a)` — e reescrever o ficheiro é uma segunda operação,
`conta.gravar(conta, frase)`. São duas de propósito: `com_amizade` não
pode falhar a meio de uma escrita, e quem chama tem de dizer as duas
coisas.

`Conta.amizades` é um `tuple`, e não uma `list`: um `dataclass`
`frozen=True` impede `conta.amizades = …` e **não** impede
`conta.amizades.append(…)`, que é a falha clássica desse padrão.

Uma amizade com a mesma pública do par **substitui** a anterior em vez de
duplicar: refazer o handshake com alguém dá chaves novas, e guardar as
velhas ao lado das novas daria dois conjuntos para o mesmo contacto.

Um keystore escrito antes destas amizades **abre**, com a lista vazia: a
ausência é o estado correcto de quem ainda não fez um handshake. Já uma
amizade presente e malformada é recusada — um ficheiro com uma amizade a
meio não é um registo antigo, é um registo corrompido, e aceitá-lo daria
um `K9` de comprimento errado a cifrar.

Ver [`handshake.md`](handshake.md) §Ciclo de vida para o que isto muda
para quem usa o programa.

## Porque é que o sal fica **dentro** do texto cifrado

O identificador é público: é o que se dá a alguém para nos encontrar. Se o
sal também fosse público, quem tivesse o identificador poderia **confirmar
qualquer nome de utilizador que suspeitasse** — bastava computar
`identificador_de(suspeita, sal)` e comparar com o valor público.

Mantendo o sal dentro da cifra, quem tem o identificador não tem o sal, e a
busca não se pode fazer. É o motivo pelo qual a conta não tem um campo de
sal legível.

`tests/test_conta.py::test_o_sal_so_sai_do_ficheiro_cifrado` lê o ficheiro
tal como está no disco e verifica que nem o sal nem o identificador lá
aparecem em claro.

## Porque é que uma identidade nova é outra pessoa

Cada registo gera seed e sal novos. `criar()` sem `forcar` recusa se já
houver conta, porque substituir uma identidade sem o mandar é desaparecer
sem dar por isso — e cada identidade é uma pessoa distinta para quem já
conhecia esta.

É por isso que «recomeçar», no ecrã de recuperação, é uma porta e não uma
nota de rodapé: quem recomeçar tem de saber, **antes**, que o
identificador muda e que o caminho guardado por outra pessoa deixa de
funcionar.

## A força da frase

`estimar_entropia()` mede a entropia empírica do multiconjunto de
caracteres, multiplicada pelo comprimento:

```
H = −∑ pᵢ·log₂(pᵢ)        bits ≈ H · comprimento
```

Aceitam-se **dois** mínimos, e ambos têm de passar:

| Mínimo | Valor | Razão |
| --- | --- | --- |
| comprimento | 12 caracteres | uma frase de 8 com maiúscula e número cai em segundos |
| entropia estimada | 60 bits | doze `a` passam o comprimento e não passam a variedade |

### O que a medição **não** faz

Não mede resistência a um dicionário. `correct horse battery staple` dá
mais de 100 bits nesta fórmula e continua a ser adivinhável. A limitação é
declarada no código, no ecrã e num teste que a fixa
(`test_frases_ditas_da_um_valor_alto_e_nao_e_um_erro`), para que ninguém
possa ler o número como uma garantia.

Não há regra de «uma maiúscula, um número, um símbolo». Fixa o comprimento
e deixa passar `Passw0rd!`.

### Porquê PBKDF2 e não argon2

`storage.py` usa PBKDF2-HMAC-SHA256 a 200 000 iterações. Argon2id é melhor
— mais resistente a GPUs e a ataques que exploram a memória. Não foi
trocado nesta Etapa porque `storage.py` é partilhado com
`user/config.py` e com o cliente de linha de comandos, e mexer no KDF de
um keystore já existente exige **re-encriptar** o que está no disco dos
utilizadores. É uma migração, e está declarada no `roadmap.md`.

## Recuperação

Não há. E o ecrã de recuperação diz isso em vez de fingir o contrário.

O que existe, e é tudo o que existe:

1. **Tentar de novo** — teclado em maiúsculas, cedilha perdida, espaço no
   fim. É a causa mais comum e a interface diz qual é.
2. **Repor de uma cópia de segurança** — `exportar()` copia o ficheiro
   **já cifrado**, e a cópia abre com a mesma frase. É a única recuperação
   que o projecto consegue oferecer.
3. **Recomeçar** — cria uma identidade nova. A antiga **não** é apagada:
   apagar junto seria o pior dos dois. A eliminação é uma operação
   separada, `eliminar()`, que não pede confirmação porque a pergunta vive
   na interface.

Um caminho «derive a identidade de uma frase-mestra em vez de um
ficheiro» foi considerado e rejeitado: o identificador é público, e um sal
público transforma o identificador público num oráculo de confirmação de
nomes (ver acima).

### Exportar copia, não re-cifra

A cópia é byte a byte igual ao original (`copyfile`), o que significa que
**não pode ser lida sem a frase** e por isso pode ir para um disco externo
ou um serviço de ficheiros sem se transformar num segredo novo.

Re-derivar a conta e re-cifrá-la com outra chave daria o mesmo resultado com
mais uma chave viva num sítio onde não deve estar.

### O ficheiro exportado vai cifrado e com `0600`

`shutil.copyfile` não copia o modo, e o `umask` por omissão deixa o
ficheiro legível por todos os utilizadores da máquina. Por isso
`exportar()` e `restaurar()` fazem `chmod 0o600` depois de escrever.

### Restaurar valida antes de sobrescrever

A frase é verificada na **cópia** antes de o destino ser tocado. Uma cópia
guardada noutro sítio e aberta com a frase actual por hábito é o caso
comum; se a verificação viesse depois da escrita, a conta boa seria
substituída por uma cópia que não abre — e a pessoa ficaria sem conta **e**
sem a cópia que tentava usar.

## Erros, e o que não se distingue

| Excepção | Quando | Distinguível? |
| --- | --- | --- |
| `FraseForte` | a frase não cumpre os mínimos | sim, antes de qualquer criptografia |
| `ContaInvalida` | registo de conta incompleto, de versão futura, ou cujas derivadas não batem | sim, e é informação útil |
| `storage.PassphraseErrada` | frase errada **ou** ficheiro adulterado | **não** |
| `storage.KeystoreInvalido` | ficheiro ausente ou de formato desconhecido | **não**, por decisão |

A indistinguibilidade de «frase errada» e «adulterado» é a propriedade
que torna a excepção segura de expor. Se as duas levantassem excepções
diferentes, quem apoiasse o dedo no botão de entrada podia usá-las como
oráculo: testar palavra-passe a palavra-passe e observar a diferença.

`test_a_excepcao_nao_distingue_frase_errada_de_adulteracao` fixa isso com
um byte trocado no texto cifrado.

## A verificação do registo ao abrir

`_de_dados()` volta a derivar o identificador a partir do nome e do sal do
ficheiro, e compara com o que está gravado. Um ficheiro cujas derivadas não
batem foi adulterado, ou foi escrito por outra versão da fórmula — e nos
dois casos a conta não abre, porque o que a pessoa confirmaria a si
própria deixaria de ser verdade.

O identificador e a impressão são verificados **em separado**: manter o
identificador certo e trocar a impressão é um ataque plausível, porque
faz quem lê a impressão ver o valor que espera.

## Referência

```python
from messenger import conta

conta.estimar_entropia("quatro cavalos lentos numa mare")  # 113.2
conta.conta_existe()                                        # False
conta.criar("ana", "quatro cavalos lentos numa mare")       # Conta(...)
conta.entrar("quatro cavalos lentos numa mare")             # Conta(...)
conta.exportar("~/copias/ana.keystore", frase)               # Path
conta.eliminar()                                            # True
```

## Leitura relacionada

- `messenger/identidade.py` — a fórmula do Onyx ID e da impressão
- `messenger/storage.py` — o keystore cifrado
- `messenger/keys.py` — a identidade Ed25519
- [`../UI/README.md`](../UI/README.md) §Acesso — os ecrãs do lado da interface
- [`roadmap.md`](roadmap.md) — Ecrãs de acesso locais
