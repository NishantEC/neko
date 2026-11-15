--!strict
-- A page added at runtime: a list, a button, and one (pretend) daemon request.
-- Edit and save: the page reloads in place and keeps `state` (saveHotState / loadHotState).
local ui = require("@rune/ui")
local host = require("@neko/host")

local state = {
	tasks = {} :: { { id: number, title: string } },
	loading = false,
	loads = 0,
	picked = 0,
}

local function load()
	if state.loading then
		return
	end
	state.loading = true
	-- Waits for the host's worker thread; the UI keeps drawing while this task is parked.
	local reply = host.request('{"ListTasks":{}}')
	state.loading = false
	state.loads += 1
	if reply and reply.ok then
		state.tasks = reply.tasks
	end
end

local function render(view)
	local rows = {}
	for _, t in state.tasks do
		table.insert(
			rows,
			ui.row({
				key = `task-{t.id}`,
				w = "full",
				px = 12,
				py = 8,
				radius = 6,
				gap = 8,
				fill = if t.id == state.picked then "control-off" else nil,
				hover = "hover",
				onPress = function()
					state.picked = t.id
				end,
			}, { ui.text(`#{t.id}`, { color = "text-secondary", size = 12 }), ui.text(t.title) })
		)
	end
	return ui.col({ key = "tasks", w = "full", h = "full", pad = 20, gap = 12 }, {
		ui.text("Tasks", { size = 18, weight = "semibold" }),
		ui.row({ gap = 8, align = "center" }, {
			ui.row({
				key = "load",
				px = 12,
				py = 6,
				radius = 6,
				fill = "control-off",
				hover = "hover",
				press = "press",
				onPress = load,
			}, { if state.loading then "Loading…" else "Load tasks" }),
			ui.text(`{#state.tasks} tasks, loaded {state.loads} times`, { size = 12, color = "text-secondary" }),
		}),
		ui.col({ gap = 2, w = "full" }, rows),
	})
end

return {
	render = render,
	saveHotState = function()
		return state
	end,
	loadHotState = function(saved)
		for k, v in saved do
			(state :: any)[k] = v
		end
	end,
}
