-- =====================================================================
-- deslocamento.lua — camada de deslocamento: b' = (b + k) mod 256
-- ---------------------------------------------------------------------
-- Uma única implementação para as duas camadas que a usam:
--
--   K2  Cesariana      k = +3   (DESLOCAMENTO_K2)
--   K8  Substituição   k = +7   (DESLOCAMENTO_K8)
--
-- Especificação (docs/pipeline.md): soma de `k` a cada byte, módulo
-- 256. A bijetividade (qualquer `k` é reversível por `−k` em Z/256Z)
-- é o que torna a camada reversível; o papel dela no pipeline é
-- ofuscação antes das camadas seguintes, não segredos.
--
-- ## Porque é um ficheiro e não dois
--
-- Até à fase G6 havia `cesar.lua` e `substituicao.lua`, com o corpo
-- byte-a-byte idêntico e apenas o cabeçalho em comum. O algoritmo de
-- K2 é o de K8 com outro `k`; manter dois ficheiros obrigava a
-- sincronizar duas implementações que só podiam divergir, e nada
-- verificasse que as divergissem.
--
-- A diferença entre as camadas vive agora no chamador, que é quem
-- sabe qual é o `k` certo para cada posição do pipeline. Se amanhã
-- K8 mudar de +7 para outra coisa, muda-se uma constante em Rust — e
-- esta função não se mexe.
--
-- Contrato: devolve uma tabela com
--   M.cifrar(dados, k)    → string (b + k) mod 256
--   M.decifrar(dados, k)  → string (b − k) mod 256
--
-- Strings em Lua são buffers de bytes arbitrários: isto trabalha com
-- dados binários (a saída de K1 ou de K7), não apenas texto UTF-8.
-- =====================================================================

local M = {}

--- Aplica `deslocamento` a cada byte de `dados`.
-- @param dados string binária (entrada)
-- @param deslocamento inteiro com sinal (o módulo é implícito)
-- @return string com o mesmo comprimento, transformada byte-a-byte
function M.cifrar(dados, deslocamento)
  local partes = {}
  for i = 1, #dados do
    local b = string.byte(dados, i)
    partes[i] = string.char((b + deslocamento) % 256)
  end
  return table.concat(partes)
end

--- Inversa de M.cifrar: subtrai o deslocamento.
function M.decifrar(dados, deslocamento)
  -- (b − k) mod 256 em Lua: o operador `%` devolve sempre um resultado
  -- em [0, 256) mesmo com minuendo negativo, por isso `-k` chega —
  -- não é preciso o `if b < k then b + 256 − k` que o Pascal faria, e
  -- essa diferença é o tipo de detalhe que divergia entre os dois
  -- ficheiros quando ambos existiam.
  return M.cifrar(dados, -deslocamento)
end

return M
