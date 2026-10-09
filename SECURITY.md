# Security

Este projecto trata segredos, agentes de rede sem identidade e
autenticidade. Uma falha numa destas camadas é uma falha de
confidencialidade ou de integridade para quem o usa, e o modelo de
ameaças está escrito em [`docs/threat_model.md`](docs/threat_model.md).

Este ficheiro diz **onde reportar** e **o que esperar**. Não repete o
modelo.

## Onde reportar

**NÃO abra um issue público.**

Um issue público descreve a falha a toda a gente antes de haver versão
corrigida, e a janela entre a descrição e a correcção é exactamente a
janela em que o bug é explorado.

Abra um issue **privado** (GitHub: *Security* → *Report a vulnerability*)
ou envie para o endereço em [`COPYRIGHT.md`](COPYRIGHT.md). Se preferir
privacidade total e não quiser conta no GitHub, o mesmo endereço serve.

## O que incluir

O que ajuda a triagem, e o que costuma atrasar um relatório:

* **O que observe** — não o que acha que é a causa. Um stack trace
  vale mais do que uma teoria.
* **Passos para reproduzir**, se os tiver.
* **A versão**, com `onyxchat --version`.
* **Se é Known**: um aviso que já está declarado em
  [`docs/threat_model.md`](docs/threat_model.md) §O que o modelo **não**
  cobre, ou em [`docs/security_model.md`](docs/security_model.md) §Limitações
  conhecidas, não é uma vulnerabilidade — é uma limitação conhecida, e
  dizer isso é a forma mais rápida de receber uma resposta útil.

## O que esperar

| Situação | Resposta |
| --- | --- |
| Relatório válido, dentro do modelo de ameaça | Confirmação em 72 horas, e uma correcção com aviso ou número de versão |
| Relatório válido, **fora** do modelo de ameaça | Explicação de porquê, com o ficheiro que o declara. Não vai ser corrigido — está declarado |
| Relatório inválido | Explicação de porquê. Sem unhelpful trocas: isto é uma pessoa que perdeu tempo |
| Sem resposta em 72 horas | Repetir no mesmo issue. Passados 30 dias sem resposta, o issue pode ser aberto publicamente |

**Não há estratégia de divulgação coordenada nem programa de bug bounty.**
O projecto é de uma pessoa e não tem orçamento para um. Um programa de
recompensa que não pode ser pago é pior do que não haver — cria uma
expectativa que não se cumpre.

O que há é o que sempre teve de haver: um aviso de segurança e um número
de versão.

## O que este projecto já faz por si

Não é configuração de servidor, é regra escrita e imposta por portão:

| Camada | Ferramenta | Onde está escrito |
| --- | --- | --- |
| Segredos no histórico | `gitleaks` | [`scripts/verificar_seguranca.sh`](../scripts/verificar_seguranca.sh) §1 |
| Advisories de Rust | `cargo audit` | §2 |
| Advisories de JavaScript | `npm audit`, comparado com [`seguranca-excepcoes.toml`](../seguranca-excepcoes.toml) | §3 |
| Licenças (PolyForm Noncommercial) | `cargo deny`, com [`deny.toml`](../deny.toml) | §4 |
| Os próprios workflows | `zizmor`, e `uses:` fixados por SHA | §5 |

Correm **igual** na máquina de quem desenvolve e no CI
(`.github/workflows/seguranca.yml`), e no CI uma ferramenta ausente é
**falha** — porque uma auditoria que salta em silêncio não é uma
auditoria.

Cada advisory aceite tem uma **razão verificável no código** e uma
**data de revisão**. Uma excepção sem razão é opinião; uma excepção sem
data é um ficheiro de excepções que ninguém reevalua.

## O que este projecto **não** faz

Escrito para não haver dúvida:

* **Não** há SBOM nem assinatura de artefactos. São coisas de distribuição,
  e não há distribuição.
* **Não** há fuzzing de código de terceiros — só o nosso
  ([`docs/testing.md`](testing.md) §Fuzzing).
* **Não** há auditoria de Python. `pyproject.toml` tem `dependencies = []`
  e quatro pacotes de desenvolvimento; não há superfície de execução para
  varrer. Quando a houver, o passo aparece.
* **Não** há promessa de que o software está livre de vulnerabilidades.
  O que há é: as fronteiras estão declaradas, os testes de segurança
  estão escritos, e a auditoria corre. Uma promessa do contrário seria
  exactamente o que este ficheiro diz para não fazer.