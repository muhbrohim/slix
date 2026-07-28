local M = {}

local defaults = { cmd = { "slix" }, project = nil, keymaps = true }
local config = vim.deepcopy(defaults)
local tabs = {}
local specs, spec_waiters, spec_generation

local function notify(message, level)
  vim.notify("[slix] " .. vim.trim(message or ""), level or vim.log.levels.INFO)
end

local function argv(...)
  local result = vim.deepcopy(config.cmd)
  if config.project then
    vim.list_extend(result, { "--project", config.project })
  end
  vim.list_extend(result, { ... })
  return result
end

local function system(args, callback)
  vim.system(args, { text = true }, function(result)
    vim.schedule(function() callback(result) end)
  end)
end

local function state(tab)
  tabs[tab] = tabs[tab] or {}
  return tabs[tab]
end

local function scratch_window(tab, kind)
  local current_tab = vim.api.nvim_get_current_tabpage()
  local item = state(tab)[kind]
  if item and vim.api.nvim_win_is_valid(item.win) and vim.api.nvim_buf_is_valid(item.buf) then
    return item.win, item.buf
  end

  vim.api.nvim_set_current_tabpage(tab)
  vim.cmd("rightbelow vsplit")
  local win = vim.api.nvim_get_current_win()
  local buf = item and vim.api.nvim_buf_is_valid(item.buf) and item.buf or vim.api.nvim_create_buf(false, true)
  vim.api.nvim_win_set_buf(win, buf)
  vim.bo[buf].buftype = "nofile"
  vim.bo[buf].bufhidden = "hide"
  vim.bo[buf].swapfile = false
  state(tab)[kind] = { win = win, buf = buf }
  if current_tab ~= tab then vim.api.nvim_set_current_tabpage(current_tab) end
  return win, buf
end

local function visible(value)
  return tostring(value or ""):gsub("\r", "\\r"):gsub("\n", "\\n")
end

local function table_output(results)
  local lines, fields = {}, {}
  for result_index, result in ipairs(results) do
    if #results > 1 then
      if #lines > 0 then table.insert(lines, "") end
      table.insert(lines, "MESSAGE " .. result_index)
    end
    local width = 4
    for _, field in ipairs(result.fields or {}) do width = math.max(width, #field.path) end
    table.insert(lines, string.format("%-" .. width .. "s  %11s  %s", "PATH", "BYTES", "VALUE"))
    table.insert(lines, string.rep("-", width) .. "  " .. string.rep("-", 11) .. "  -----")
    for _, field in ipairs(result.fields or {}) do
      table.insert(lines, string.format(
        "%-" .. width .. "s  %5d-%-5d  %s",
        field.path, field.start, field["end"], visible(field.value)
      ))
      fields[tostring(#lines)] = { spec_path = field.spec_path, spec_line = field.spec_line }
    end
    for _, diagnostic in ipairs(result.warnings or {}) do
      table.insert(lines, string.format("warning [%s]: %s", diagnostic.code, diagnostic.message))
    end
    for _, diagnostic in ipairs(result.errors or {}) do
      table.insert(lines, string.format("error [%s]: %s", diagnostic.code, diagnostic.message))
    end
  end
  return lines, fields
end

local function raw_output(results)
  local lines, fields = {}, {}
  for index, result in ipairs(results) do
    if index > 1 then table.insert(lines, "") end
    for _, field in ipairs(result.fields or {}) do
      table.insert(lines, field.path .. "=" .. visible(field.value))
      fields[tostring(#lines)] = { spec_path = field.spec_path, spec_line = field.spec_line }
    end
  end
  return lines, fields
end

local function show_parse(tab, decoded, stdout, format)
  if not vim.api.nvim_tabpage_is_valid(tab) then return end
  local results = decoded.results or {}
  local lines, fields, filetype
  if format == "json" then
    lines, fields, filetype = vim.split(vim.trim(stdout), "\n", { plain = true }), {}, "json"
  elseif format == "raw" then
    lines, fields, filetype = raw_output(results)
  else
    lines, fields, filetype = table_output(results)
  end
  if #lines == 0 then lines = { "No fields parsed" } end

  local win, buf = scratch_window(tab, "output")
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.bo[buf].filetype = filetype or "slix-output"
  vim.b[buf].slix_fields = fields
  local last = results[#results]
  local path = last and last.service and last.service.spec_path or nil
  vim.b[buf].slix_spec_path = path or nil
  state(tab).service_spec_path = path
  if vim.api.nvim_get_current_tabpage() == tab then
    vim.api.nvim_set_current_win(win)
    vim.api.nvim_win_set_cursor(win, { 1, 0 })
  end
end

local function parse(message, format)
  if not message or message == "" then
    notify("no message selected", vim.log.levels.WARN)
    return
  end
  format = format or "table"
  if not vim.tbl_contains({ "table", "json", "raw", "offsets" }, format) then
    notify("unknown format: " .. format, vim.log.levels.ERROR)
    return
  end
  local tab = vim.api.nvim_get_current_tabpage()
  local tab_state = state(tab)
  tab_state.parse_generation = (tab_state.parse_generation or 0) + 1
  local generation = tab_state.parse_generation
  system(argv("parse", "--output", "json", "--message", message), function(result)
    if state(tab).parse_generation ~= generation then return end
    if result.code ~= 0 and result.code ~= 1 then
      notify(result.stderr ~= "" and result.stderr or "parse failed", vim.log.levels.ERROR)
      return
    end
    local ok, decoded = pcall(vim.json.decode, result.stdout or "")
    if not ok or type(decoded) ~= "table" then
      notify(result.stderr ~= "" and result.stderr or "invalid parse JSON", vim.log.levels.ERROR)
      return
    end
    show_parse(tab, decoded, result.stdout, format == "offsets" and "table" or format)
  end)
end

local function range_text(first, last)
  return table.concat(vim.api.nvim_buf_get_lines(0, first - 1, last, false), "\n")
end

local function visual_text()
  local mode = vim.fn.mode()
  local lines = vim.fn.getregion(vim.fn.getpos("v"), vim.fn.getpos("."), { type = mode })
  return table.concat(lines, "\n")
end

function M.line(format)
  parse(vim.api.nvim_get_current_line(), format)
end

function M.selection(format, first, last)
  parse(first and range_text(first, last) or visual_text(), format)
end

function M.smart(format)
  local mode = vim.fn.mode()
  if mode == "v" or mode == "V" or mode == "\22" then M.selection(format) else M.line(format) end
end

local function normalize_code(code)
  return code and code:gsub("^%s*#", ""):gsub("%s*$", ""):upper() or nil
end

local function code_on_line()
  for token in vim.api.nvim_get_current_line():gmatch("#?[%a]+%d+") do
    return normalize_code(token)
  end
end

function M._open_path(path, line)
  if not path or path == "" or path == vim.NIL then
    notify("spec path is unavailable", vim.log.levels.WARN)
    return
  end
  path = vim.fn.fnamemodify(path, ":p")
  local tab = vim.api.nvim_get_current_tabpage()
  local item = state(tab).spec
  local win
  if item and vim.api.nvim_win_is_valid(item.win) then
    win = item.win
    vim.api.nvim_set_current_win(win)
  else
    vim.cmd("rightbelow vsplit")
    win = vim.api.nvim_get_current_win()
  end
  local buf = vim.fn.bufadd(path)
  vim.bo[buf].swapfile = false
  vim.fn.bufload(buf)
  vim.api.nvim_win_set_buf(win, buf)
  state(tab).spec = { win = win, buf = buf }
  vim.api.nvim_win_set_cursor(win, { math.max(1, math.min(tonumber(line) or 1, vim.api.nvim_buf_line_count(buf))), 0 })
end

function M._get_specs(callback, force)
  if specs and not force then callback(specs); return end
  if spec_waiters and not force then table.insert(spec_waiters, callback); return end
  spec_generation = (spec_generation or 0) + 1
  local generation = spec_generation
  spec_waiters = spec_waiters or {}
  table.insert(spec_waiters, callback)
  system(argv("list", "--json"), function(result)
    if generation ~= spec_generation then return end
    local waiters = spec_waiters
    spec_waiters = nil
    local ok, decoded = pcall(vim.json.decode, result.stdout or "")
    if result.code ~= 0 or not ok or type(decoded) ~= "table" then
      notify(result.stderr ~= "" and result.stderr or "could not load spec list", vim.log.levels.ERROR)
      for _, waiter in ipairs(waiters) do waiter(nil) end
      return
    end
    specs = decoded
    for _, waiter in ipairs(waiters) do waiter(specs) end
  end)
end

function M.open_spec(code)
  code = normalize_code(code ~= "" and code or nil)
  local tab_state = state(vim.api.nvim_get_current_tabpage())
  if not code and tab_state.service_spec_path then
    M._open_path(tab_state.service_spec_path)
    return
  end
  code = code or code_on_line()
  if not code then
    notify("no service code found", vim.log.levels.WARN)
    return
  end
  M._get_specs(function(entries)
    if not entries then return end
    for _, entry in ipairs(entries) do
      if normalize_code(entry.service_code) == code then
        M._open_path(entry.spec_path)
        return
      end
    end
    notify("spec not found for " .. code, vim.log.levels.WARN)
  end)
end

function M.find_spec()
  require("slix.telescope").find_specs()
end

function M.jump_field()
  local buf = vim.api.nvim_get_current_buf()
  local fields = vim.b[buf].slix_fields
  local field = fields and fields[tostring(vim.api.nvim_win_get_cursor(0)[1])]
  if not field then
    notify("current line is not a parsed field", vim.log.levels.WARN)
    return
  end
  M._open_path(field.spec_path, field.spec_line)
end

function M.reload_specs()
  specs = nil
  M._get_specs(function(entries)
    if entries then notify(string.format("reloaded %d specs", #entries)) end
  end, true)
end

function M.setup(opts)
  config = vim.tbl_deep_extend("force", vim.deepcopy(defaults), opts or {})
  if type(config.cmd) ~= "table" or #config.cmd == 0 then error("slix: cmd must be a non-empty argv list") end
  M._config = config

  local command = vim.api.nvim_create_user_command
  command("SlixLine", function(o) M.line(o.args ~= "" and o.args or nil) end,
    { nargs = "?", complete = function() return { "table", "json", "raw" } end, force = true })
  command("SlixSelection", function(o) M.selection(o.args ~= "" and o.args or nil, o.line1, o.line2) end,
    { nargs = "?", range = true, complete = function() return { "table", "json", "raw" } end, force = true })
  command("SlixOpenSpec", function(o) M.open_spec(o.args) end, { nargs = "?", force = true })
  command("SlixFindSpec", M.find_spec, { force = true })
  command("SlixJumpField", M.jump_field, { force = true })
  command("SlixReloadSpecs", M.reload_specs, { force = true })

  if config.keymaps then
    local map = vim.keymap.set
    map({ "n", "x" }, "<leader>ss", function() M.smart("table") end, { desc = "Slix: parse (table)" })
    map({ "n", "x" }, "<leader>sb", function() M.smart("offsets") end, { desc = "Slix: parse (table+offsets)" })
    map({ "n", "x" }, "<leader>sj", function() M.smart("json") end, { desc = "Slix: parse (JSON)" })
    map({ "n", "x" }, "<leader>sr", function() M.smart("raw") end, { desc = "Slix: parse (raw)" })
    map("n", "<leader>sS", function() M.open_spec() end, { desc = "Slix: open spec" })
    map("n", "<leader>sf", M.find_spec, { desc = "Slix: find spec" })
    map("n", "<leader>sd", M.jump_field, { desc = "Slix: jump to field" })
    map("n", "<leader>sR", M.reload_specs, { desc = "Slix: reload specs" })
  end
end

return M
