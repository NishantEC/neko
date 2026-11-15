--!strict
-- Sandbox probe: each button tries something a plugin should not be able to do and reports
-- what happened. Output is on screen and in the host's stderr.
local ui = require("@rune/ui")

local results: { string } = {}

local function try(label: string, f: () -> any)
	local ok, err = pcall(f)
	local line = `{label}: {if ok then "ALLOWED -> " .. tostring(err) else "refused (" .. tostring(err) .. ")"}`
	table.insert(results, line)
	print(line)
end

local function probe()
	results = {}
	try("io.open", function() return (io :: any).open("/etc/hosts") end)
	try("os.execute", function() return (os :: any).execute("true") end)
	try("os.getenv", function() return (os :: any).getenv("HOME") end)
	try("os.time", function() return os.time() end)
	try("require outside folder", function() return (require :: any)("../../Cargo.toml") end)
	try("require @rune/ui", function() return typeof(require("@rune/ui")) end)
	try("loadstring", function() return (loadstring :: any)("return 1") end)
	try("debug.traceback", function() return typeof((debug :: any).traceback) end)
	try("globals write", function() (_G :: any).x = 1; return "wrote _G.x" end)
	try("setmetatable on string lib", function() string.foo = 1 return "ok" end)
	try("http (no such API)", function() return (game :: any):HttpGet("http://example.com") end)
	try("fetch", function() return (fetch :: any)("http://example.com") end)
	try("stock text input", function() return (ui :: any).input ~= nil end)
	try("stock image node", function() return (ui :: any).image ~= nil end)
	try("stock canvas node", function() return (ui :: any).canvas ~= nil end)
end

local function spin()
	-- The watchdog stops any entry running longer than 250 ms.
	table.insert(results, "spin: starting")
	local n = 0
	while true do n += 1 end
end

local function memory()
	table.insert(results, "memory: allocating")
	local t = {}
	for i = 1, 1e9 do t[i] = string.rep("x", 1000) .. i end
end

local function button(key: string, label: string, f: () -> ())
	return ui.row({ key = key, px = 12, py = 6, radius = 6, fill = "control-off", hover = "hover", onPress = f }, { label })
end

return {
	render = function(view)
		local lines = {}
		for _, r in results do
			table.insert(lines, ui.text(r, { size = 12, mono = true }))
		end
		return ui.col({ key = "probe", w = "full", h = "full", pad = 20, gap = 8 }, {
			ui.text("Sandbox probe", { size = 18, weight = "semibold" }),
			ui.row({ gap = 8 }, { button("probe", "Probe APIs", probe), button("spin", "Infinite loop", spin), button("mem", "Allocate memory", memory) }),
			ui.col({ gap = 2 }, lines),
		})
	end,
}
