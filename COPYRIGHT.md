# Copyright e titularidade — OnyxChat

Este ficheiro responde a uma pergunta que a licença não responde: **quem é o dono disto, e com que capacidade.**

## Titularidade

```
Titular dos direitos de autor:  Carlos Alberto da Silva Brito
Identidade pública do projecto: ShegaPT (pseudónimo)
Ano da primeira publicação:     2026
Licença base:                   PolyForm Noncommercial 1.0.0
```

O pseudónimo `ShegaPT` é o nome sob o qual o projecto é publicado e sob
o qual aparece nos metadados dos pacotes (`Cargo.toml`, `pyproject.toml`)
e em cada `Required Notice:` dos ficheiros de fonte. É o nome que a
comunidade vê.

O nome civil acima é o titular legal. Está aqui, e não nos metadados
públicos, por uma razão prática: **o que se publica fica no histórico
do Git para sempre.** Um `git filter-repo` não apaga o que já foi
clonado. Publicar o nome civil é uma decisão que se toma uma vez, antes
do primeiro commit público, e que não tem reversão prática.

> **Nome preenchido.** O nome civil está escrito neste ficheiro, no
> cabeçalho do `LICENSE`, no `Required Notice:` de cada ficheiro de
> fonte e no aviso que acompanha o pacote construído.
>
> A verificação é o teste de guarda **`licenca_e_metadados_sao_coerentes`**,
> em [`network/daemon_rust/tests/guarda_tor.rs`](network/daemon_rust/tests/guarda_tor.rs).
> Aponta-se o ficheiro porque ele ser procurado **pelo nome** não é o
> suficiente: durante meses foi exactamente esse o problema — o teste
> existe, funciona, e mesmo assim não se encontrava quem o procurasse
> num `grep` com os filtros errados.
>
> O que ele verifica hoje:
>
> | Verificação | Onde |
> | --- | --- |
> | a `LICENSE` contém o texto da PolyForm | ponto (1) |
> | todos os `Cargo.toml` com campo `license` dizem o mesmo | ponto (2) |
> | o `pyproject.toml` diz o mesmo | ponto (3) |
> | nenhum marcador ficou por resolver, em `tests/` e `fuzz/` incluídos | ponto (4) |
> | cada ficheiro de fonte carrega o `Required Notice:` — incluindo os 91 `.js`/`.jsx` | ponto (5) |
> | o endereço do repositório é o mesmo em todos os ficheiros | ponto (6) |
>
> ## O guard que não via o seu próprio ficheiro
>
> Até 2026-10-13 o guard excluía `tests/` da contagem de marcadores —
> a exclusão é correcta para o **aviso**, porque ficheiros de teste não
> são distribuídos — e a consequência foi que `guarda_tor.rs:1` carregava
> um marcador por resolver na linha 1, invisível ao guard que mora
> nesse ficheiro.
>
> Passou meses assim. O guard dava verde, e o erro estava no seu próprio
> cabeçalho.
>
> A correcção foi **separar as duas perguntas**: o aviso é uma questão
> de distribuição, o marcador é uma questão de higiene. São quase a mesma
> lista de ficheiros e não podem ser a mesma, porque a diferença entre
> elas — `tests/` e `fuzz/` — é exactamente onde o marcador estava.
>
> ## O pacote construído
>
> A PolyForm fala de quem recebe «uma cópia de **qualquer parte**» do
> software. A parte que sai da máquina não são os ficheiros de origem:
> é `UI/dist/`, que o sidecar serve em `127.0.0.1`. O aviso desse
> pacote é posto em [`UI/vite.config.js`](UI/vite.config.js) num
> `postBanner`, e é verificado pelo job da interface no CI — **não** por
> este guard, porque o guard lê fontes e o pacote é uma consequência da
> construção.

## A empresa que ainda não existe

Se mais tarde fundares uma empresa para o explorar comercialmente, o
copyright **continua a ser teu** até fazeres uma cessão de direitos de
autor **por escrito**. Uma empresa não herda a titularidade por
simples decisão de fundar: precisa de cessão, e a cessão tem de ser
escrita.

Sem cessão, a empresa fica dependente de uma licença tua e complica
qualquer contrato com terceiros — exactamente o tipo de atrito que é
barato evitar agora e caro resolver depois.

Em Portugal a cessão de direitos de autor tem de constar de documento
escrito. Mostra-o a um advogado em Portugal antes de emitir a primeira
licença comercial a um terceiro, porque é a partir daí que os contratos
começam a existir.

## Contribuições — o que acontece quando alguém contribui

A `CONTRIBUTING.md` exige cessão de direitos de autor a favor do
titular antes de qualquer contribuição ser aceite. Isto não é
paranoia: sem isso, o primeiro PR externo fragmenta a titularidade e
ninguém a consegue desfazer depois.

Um fork não comercial é legal e não precisa de autorização — a
PolyForm Noncommercial concede-o explicitamente. A titularidade fragmenta-se
quando alguém **contribui para o repositório principal**, não quando faz
um fork à parte. É por isso que a exigência está no fluxo de contribuição
e não no código.