-- A card for the Neko column: how many sessions need you, and the first three.
-- A card is a table with slot, id, title, an optional icon, render(state, host) and an optional init(host).
-- host.read runs off the UI thread and can only be called from a task, so init starts one.
local ui = require("@rune/ui")

local REFRESH_SECONDS = 20

return {
	slot = "card",
	id = "needs-you",
	title = "Needs you",
	icon = "inbox",

	init = function(host)
		local state = { loaded = false, count = 0, rows = {}, err = nil }
		task.spawn(function()
			while true do
				local workbench, err = host.read("Workbench")
				if workbench then
					local rows, count = {}, 0
					for _, session in workbench.tasks do
						if session.needs_you then
							count += 1
							if #rows < 3 then
								table.insert(rows, { id = session.id, title = session.title, glyph = session.glyph })
							end
						end
					end
					state.count, state.rows, state.err = count, rows, nil
				else
					state.err = err
				end
				state.loaded = true
				task.wait(REFRESH_SECONDS)
			end
		end)
		return state
	end,

	render = function(state, host)
		if not state.loaded then
			return ui.text("Looking…", { muted = true })
		end
		if state.err then
			return ui.text("Couldn't read your sessions: " .. state.err, { muted = true, wrap = true })
		end
		if state.count == 0 then
			return ui.text("Nothing needs you right now", { muted = true })
		end
		local lines = {
			ui.text(state.count == 1 and "1 session needs you" or `{state.count} sessions need you`, { weight = "medium" }),
		}
		for _, row in state.rows do
			local id = row.id
			table.insert(lines, ui.row({
				key = "needs-" .. id,
				gap = 8,
				py = 3,
				align = "center",
				onPress = function()
					host.open("tasks?task=" .. id)
				end,
			}, {
				ui.glyph({ state = row.glyph }),
				ui.text(row.title, { truncate = true, grow = 1 }),
			}))
		end
		table.insert(lines, ui.row({ pt = 4 }, {
			ui.button({
				label = "Ask about these",
				key = "ask-needs",
				on_press = function()
					host.ask("Which of the sessions that need me should I look at first, and why?")
				end,
			}),
		}))
		return ui.col({ gap = 2 }, lines)
	end,
}
