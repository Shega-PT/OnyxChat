-- =====================================================================
-- playfair.lua — camada K6 do pipeline: Playfair adaptado a bytes
-- ---------------------------------------------------------------------
-- Especificação (docs/pipeline.md): "versão adaptada — XOR repetido
-- com a chave". A cifra de Playfair original opera sobre bigramas de
-- letras; a adaptação byte-a-byte do projeto aplica XOR periódico com
-- a palavra-chave:
--   b' = b XOR chave[i mod #chave]
-- Chave de produção: "PLAYFAIR" (8 bytes).
--
-- XOR é a própria inversa, portanto cifrar e decifrar são a mesma
-- operação — mantêm-se as duas funções para uniformidade do contrato
-- das camadas analógicas.
-- =====================================================================

local M = {}

--- Exige chave não vazia (falha explícita > identidade silenciosa).
local function exigir_chave(chave)
  if type(chave) ~= "string" or #chave == 0 then
    error("playfair: chave inválida (tem de ser string não vazia)", 3)
  end
  return #chave
end

--- Aplica XOR repetido da chave sobre todos os bytes de `dados`.
function M.cifrar(dados, chave)
  local n = exigir_chave(chave)
  local partes = {}
  for i = 1, #dados do
    local b = string.byte(dados, i)
    local k = string.byte(chave, ((i - 1) % n) + 1)
    partes[i] = string.char(b ~ k) -- operador bitwise XOR do Lua 5.3+
  end
  return table.concat(partes)
end

--- Inversa de M.cifrar (XOR é involutivo: decifrar = cifrar).
function M.decifrar(dados, chave)
  return M.cifrar(dados, chave)
end

return M
