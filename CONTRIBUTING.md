# Contribuir para o OnyxChat

Obrigada por querer contribuir. Antes de mais nada, duas coisas que este
documento explica e que são a condição para o teu contributo ser aceite.

## 1. A licença base é PolyForm Noncommercial

Qualquer uso, alteração ou redistribuição para fins **não-comerciais** é
permitido e não precisa de pedido. Fork não comercial é legal e não
requer autorização.

O uso **comercial** — vender, oferecer como serviço, integrar num
produto pago — exige uma licença separada do titular. Ver
`COMMERCIAL-LICENSE.md` e `AUTHORIZED.md`.

## 2. Contribuições são cedidas ao titular

Ao submeteres um *pull request*, cedes ao titular do projecto os direitos
de autor sobre a tua contribuição, de forma mundial, sem royalties e
irrevogável. Isto é o costume em projectos que mantêm uma licença base
própria, e não é uma exigência incomum.

**Porquê, e porque é agora.** O OnyxChat é um projecto de uma pessoa.
Se uma primeira contribuição externa entrar sem cessão, a titularidade
fragmenta-se: a partir desse momento há duas partes com direitos
diferentes sobre o mesmo código, o que complica qualquer licença
comercial futura e é praticamente irrecuperável. Recolher a cessão
quando o repositório é ainda só teu custa uma linha de conversa. Recolher
depois de já ter publicada dezenas de contribuições custa um advogado.

Se preferires que o teu contributo **não** entre na cadeia de
titularidade, há um caminho: mantém o teu patch como fork próprio e não
subjas *pull request*. Ele continua a ser legal — só não entra no
repositório principal.

## O que tornas um *pull request* aceitável

- Testes. Toda a alteração de comportamento vem acompanhada de teste
  que falha sem ela. O projecto exige **100% de cobertura** em Python.
- Comentários em português europeu, no mesmo registo do código
  circundante. Explicam **porquê**, não **o quê**.
- O gate de memória passa: `./scripts/testar.sh`. O projecto é
  sensível a RAM e um build que não cabe é uma falha, mesmo que o código
  esteja certo.
- Documentação actualizada quando o comportamento muda. Os documentos
  são normativos, não ilustrativos.

## Se tens medo de Licensing

Se fores uma empresa que precisa de garantias antes de investir no
projecto, fala connosco antes de começar. A licença comercial é
negociável — mas tem de ser negociada, e é mais simples faze-lo antes de
escrever código.

---

**Contacto:** abre uma *issue* no repositório, ou escreve para o endereço
indicado em `COPYRIGHT.md`.
