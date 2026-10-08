# Ferramentas locais da interface

Toolchain que a interface precisa e que **não vem do sistema**. Vive
dentro de `UI/tools/` porque quem desenvolve a interface é a primeira
pessoa a tropeçar nela, e um requisito escondido noutro sítio é um
requisito que ninguém cumpre.

## Porquê trazer o Node consigo

O Vite 8 declara `engines.node = "^20.19.0 || >=22.12.0"` e usa
`util.styleText`, que só existe a partir do Node 20.12. Numa distribuição
com o Node 18, `npm run dev` falha com:

```
SyntaxError: The requested module 'node:util' does not provide an
export named 'styleText'
```

O erro não menciona versões. Quem o encontra passa a procurar um bug.

Três saídas eram possíveis:

| Saída | Porquê não |
| --- | --- |
| Exigir que se instale o Node certo pelo gestor de pacotes | Depende do pacote que a distribuição escolheu. Em Ubuntu 24.04 o Node 18 é o que vem; o Node 22 vive num PPA. |
| Aceitar o Node antigo e corrigir o Vite | É um retrocesso. O Vite 8 é o que está instalado. |
| **Trazer o Node consigo** | Previsível, sem `sudo`, sem conflito com outro projecto, apaga-se com a pasta. |

A cópia local tem um custo: **~120 MB descomprimidos, não versionados**.
É o mesmo acordo de `target/` no Rust e de `node_modules/` no npm — coisas
que se apagam e se reconstróem.

## Comandos

```bash
UI/tools/onyxchat instalar      # instala o Node local, se faltar
UI/tools/onyxchat instalar-ui   # o anterior + as dependências da interface
UI/tools/onyxchat verificar     # diz o que falta, sem instalar nada
UI/tools/onyxchat dev           # servidor de desenvolvimento
UI/tools/onyxchat build         # compila para dist/
UI/tools/onyxchat ver           # lint + tipos + construção
UI/tools/onyxchat <outro>       # passa ao npm
```

`verificar` é o único comando seguro correr em automação: não muda nada.

## Confiança no artefacto

O instalador **não** vai buscar os sumários ao nodejs.org — lê-os de
`versoes.txt`, versionado e revisto, e recusa-se a instalar se o SHA-256
não bater. A verificação acontece **antes** de descompactar, para que um
pacote adulterado nunca chegue a ser escrito em `UI/tools/node/`.

A justificação está em `versoes.txt` e vale a pena ler: um instalador que
descarrega o pacote e o verifica contra sumários do mesmo sítio verifica-o
contra si próprio, e muda de conteúdo sem deixar rasto no Git.

Actualizar o Node é uma alteração visível no repositório:

1. Ler `https://nodejs.org/dist/v22.14.0/SHASUMS256.txt`
2. Copiar o sumário da plataforma para baixo, e actualizar a versão
3. `UI/tools/onyxchat instalar`

## Quando não há ferramenta local

`onyxchat` **não falha** por isso. Se `tools/node/` não existir mas o
sistema tiver um Node suficientemente novo, usa-o e avisa. A cópia local é
uma conveniência, não um requisito — e recusar trabalhar por ela custaria
mais do que resolveria.

O que nunca é aceito é um Node **antigo**: aí a falha seria opaca e
apareceria no meio de um build, em vez de ser uma mensagem que diz o que
fazer.

## Plataformas

`instalar.sh` cobre `linux-x64`, `linux-arm64`, `darwin-x64` e
`darwin-arm64`. O projecto é testado em Linux (`docs/USER_GUIDE.md` §1.1);
fora dessas plataformas o script falha com mensagem explícita, em vez de
descompactar um pacote errado e deixar um `node` que não corre.