local M = {}

local function value(item, key, fallback)
  local result = item[key]
  if result == nil or result == vim.NIL then return fallback end
  return result
end

local function display(item)
  return string.format("%-10s  %-4s  %-9s  %s",
    value(item, "service_code", "?"), value(item, "category", "-"),
    value(item, "program", "-"), value(item, "endpoint", "<none>"))
end

local function fallback(entries)
  vim.ui.select(entries, { prompt = "Slix spec:", format_item = display }, function(item)
    if item then require("slix")._open_path(item.spec_path) end
  end)
end

local function telescope(entries, opts)
  local pickers = require("telescope.pickers")
  local finders = require("telescope.finders")
  local actions = require("telescope.actions")
  local action_state = require("telescope.actions.state")
  local conf = require("telescope.config").values
  local previewers = require("telescope.previewers")
  pickers.new(opts, {
    prompt_title = "Slix specs",
    finder = finders.new_table({
      results = entries,
      entry_maker = function(item)
        return {
          value = item,
          display = display(item),
          ordinal = table.concat({
            value(item, "service_code", ""), value(item, "category", ""),
            value(item, "section", ""), value(item, "program", ""), value(item, "endpoint", ""),
          }, " "),
          filename = item.spec_path,
        }
      end,
    }),
    sorter = conf.generic_sorter(opts),
    previewer = previewers.cat.new(opts),
    attach_mappings = function(prompt_bufnr)
      actions.select_default:replace(function()
        local selected = action_state.get_selected_entry()
        actions.close(prompt_bufnr)
        if selected then require("slix")._open_path(selected.value.spec_path) end
      end)
      return true
    end,
  }):find()
end

function M.find_specs(opts)
  require("slix")._get_specs(function(entries)
    if not entries or #entries == 0 then return end
    if pcall(require, "telescope.pickers") then telescope(entries, opts or {}) else fallback(entries) end
  end)
end

return M
