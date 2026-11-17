-- A view (More, Plugin pages): every routine with its status glyph, a filter box, and when each last ran.
-- Views fill the 880 point column under the page's own header; the title is the header.
local ui = require("@rune/ui")

local REFRESH_SECONDS = 30

local function ago(host, ms)
	if not ms then
		return "not yet"
	end
	local minutes = math.max(0, (host.now() - ms) // 60000)
	if minutes < 1 then
		return "just now"
	elseif minutes < 60 then
		return `{minutes} min ago`
	elseif minutes < 1440 then
		return `{minutes // 60} h ago`
	end
	return `{minutes // 1440} d ago`
end

return {
	slot = "view",
	id = "routines",
	title = "Routines at a glance",

	init = function(host)
		local state = { loaded = false, routines = {}, filter = "", err = nil }
		task.spawn(function()
			while true do
				local routines, err = host.read("Routines")
				if routines then
					state.routines, state.err = routines, nil
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
		local children = {
			ui.input({
				key = "filter",
				value = state.filter,
				placeholder = "Filter routines",
				on_change = function(text)
					state.filter = text
				end,
			}),
		}
		if not state.loaded then
			table.insert(children, ui.text("Looking…", { muted = true }))
		elseif state.err then
			table.insert(children, ui.text("Couldn't read your routines: " .. state.err, { muted = true, wrap = true }))
		else
			local needle = string.lower(state.filter)
			local shown = 0
			for _, routine in state.routines do
				if needle == "" or string.find(string.lower(routine.name), needle, 1, true) then
					shown += 1
					table.insert(children, ui.row({
						key = "routine-" .. routine.id,
						gap = 10,
						py = 5,
						align = "center",
					}, {
						ui.glyph({ state = routine.glyph }),
						ui.text(routine.name, { truncate = true, grow = 1 }),
						ui.text(routine.cadence, { muted = true, size = 12 }),
						ui.text(ago(host, routine.last_ms), { muted = true, size = 12 }),
					}))
				end
			end
			if shown == 0 then
				table.insert(children, ui.text(#state.routines == 0 and "No routines yet" or "No routine matches", { muted = true }))
			end
		end
		return ui.col({ gap = 8, w = "full" }, children)
	end,
}
