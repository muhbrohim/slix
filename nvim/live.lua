local source = vim.api.nvim_get_current_buf()
vim.bo[source].filetype = "slix-message"

local timer = vim.uv.new_timer()
local generation = 0
local panels = {}

local function close_panels()
  for _, panel in ipairs(panels) do
    if vim.api.nvim_win_is_valid(panel.win) then
      vim.api.nvim_win_close(panel.win, true)
    end
    if vim.api.nvim_buf_is_valid(panel.buf) then
      vim.api.nvim_buf_delete(panel.buf, { force = true })
    end
  end
  panels = {}
end

local function show(results)
  close_panels()
  vim.api.nvim_set_current_buf(source)
  for _, result in ipairs(results) do
    vim.cmd("rightbelow vsplit")
    local win = vim.api.nvim_get_current_win()
    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_win_set_buf(win, buf)
    vim.bo[buf].buftype = "nofile"
    vim.bo[buf].bufhidden = "wipe"
    vim.bo[buf].swapfile = false
    vim.bo[buf].filetype = "json"
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(result, "\n", { plain = true }))
    vim.bo[buf].modifiable = false
    table.insert(panels, { win = win, buf = buf })
  end
  local source_win = vim.fn.bufwinid(source)
  if source_win ~= -1 then vim.api.nvim_set_current_win(source_win) end
end

local function parse()
  generation = generation + 1
  local current = generation
  local lines = vim.api.nvim_buf_get_lines(source, 0, -1, false)
  local messages = {}
  for _, line in ipairs(lines) do
    if line ~= "" then table.insert(messages, line) end
  end
  if #messages == 0 then close_panels(); return end
  vim.system(
    { vim.g.slix_executable, "parse", "--protocol" },
    { stdin = table.concat(messages, "\n"), text = true },
    vim.schedule_wrap(function(result)
      if current ~= generation then return end
      if result.code ~= 0 and result.code ~= 1 then
        vim.notify(result.stderr, vim.log.levels.ERROR)
        return
      end
      local ok, decoded = pcall(vim.json.decode, result.stdout)
      if ok then show(decoded.rendered_json or {}) else vim.notify(result.stdout, vim.log.levels.ERROR) end
    end)
  )
end

local group = vim.api.nvim_create_augroup("SlixLive", { clear = true })
vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI" }, {
  group = group,
  buffer = source,
  callback = function()
    timer:stop()
    timer:start(180, 0, vim.schedule_wrap(parse))
  end,
})

vim.api.nvim_create_user_command("SlixParse", parse, {})
vim.keymap.set("n", "<leader>sj", parse, { buffer = source, desc = "Slix: parse messages" })
