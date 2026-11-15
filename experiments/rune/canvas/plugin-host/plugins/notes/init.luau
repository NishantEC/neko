--!strict
-- Uses the three nodes the host adds beyond stock @rune/ui: ui.input, ui.image, ui.canvas.
local ui = require("@rune/ui")
local host = require("@neko/host")

local state = { query = "", bars = { 30, 62, 48, 90, 71 } }

local function chart()
	local ops = {}
	for i, v in state.bars do
		table.insert(ops, { kind = "rect", x = (i - 1) * 34 + 4, y = 100 - v, w = 24, h = v, color = "#4c8dff" })
	end
	table.insert(ops, { kind = "line", x1 = 0, y1 = 100, x2 = 180, y2 = 100, w = 2, color = "#8a8f98" })
	return ui.canvas({ w = 180, h = 104, ops = ops })
end

return {
	-- Called by the host after every edit of a ui.input (the host's own contract, not Rune's).
	onInput = function(id: string, text: string)
		if id == "q" then
			state.query = text
		end
	end,
	render = function(view)
		return ui.col({ key = "notes", w = "full", h = "full", pad = 20, gap = 14 }, {
			ui.text("Notes", { size = 18, weight = "semibold" }),
			ui.input({ id = "q", placeholder = "Type here" }),
			ui.text(`you typed: {state.query} ({#host.inputValue("q")} chars)`, { color = "text-secondary", size = 12 }),
			ui.row({ gap = 16, align = "center" }, {
				ui.image({ src = "logo.png", w = 96, h = 96 }),
				chart(),
			}),
		})
	end,
}
