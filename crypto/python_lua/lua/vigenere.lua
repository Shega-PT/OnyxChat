-- =====================================================================
-- vigenere.lua — camada K3 do pipeline: Vigenère byte-a-byte
-- ---------------------------------------------------------------------
-- Especificação (docs/pipeline.md):
--   b' = (b + chave[i mod #chave]) mod 256
-- Chave de produção: "VIGENERE" (8 bytes).
--
-- Devolve uma tabela com:
--   M.cifrar(dados, chave)  → soma modular periódica da chave
--   M.decifrar(dados, chave) → subtração modular periódica da chave
--
-- Chave vazia é rejeitada explicitamente (não existe cifragem sem
-- chave — falhar cedo é melhor que produzir identidade silenciosa).
-- =====================================================================

local M = {}

--- Valida a chave e devolve o comprimento (para reaproveitar no ciclo).
local function exigir_chave(chave)
  if type(chave) ~= "string" or #chave == 0 then
    error("vigenere: chave inválida (tem de ser string não vazia)", 3)
  end
  return #chave
end

--- Soma cada byte com a chave repetida (Vigenère clássico em Z/256Z).
function M.cifrar(dados, chave)
  local n = exigir_chave(chave)
  local partes = {}
  for i = 1, #dados do
    local b = string.byte(dados, i)
    local k = string.byte(chave, ((i - 1) % n) + 1)
    partes[i] = string.char((b + k) % 256)
  end
  return table.concat(partes)
end

--- Inversa de M.cifrar (subtração modular com a mesma chave).
function M.decifrar(dados, chave)
  local n = exigir_chave(chave)
  local partes = {}
  for i = 1, #dados do
    local b = string.byte(dados, i)
    local k = string.byte(chave, ((i - 1) % n) + 1)
    partes[i] = string.char((b - k) % 256)
  end
  return table.concat(partes)
end

return M
