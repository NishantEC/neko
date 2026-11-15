local ui = require("@rune/ui")

local INITIAL = { 12, 30, 22, 41, 18, 35, 27 }
local DAYS = 7

local CANVAS_W = 280
local CANVAS_H = 130
local BASELINE = 122 -- y of the axis line
local TOP = 10 -- free space above the tallest bar
local BAR_W = 30
local BAR_GAP = 10
local BAR_X0 = 5

local function copyInitial()
    local out = {}
    for i = 1, #INITIAL do
        out[i] = INITIAL[i]
    end
    return out
end

local state = {
    values = copyInitial(),
    presses = 0,
}

local function total()
    local sum = 0
    for i = 1, #state.values do
        sum += state.values[i]
    end
    return sum
end

local function chartOps()
    local ops = {}
    local maxv = 1
    for i = 1, #state.values do
        if state.values[i] > maxv then
            maxv = state.values[i]
        end
    end
    local avail = BASELINE - TOP
    for i = 1, #state.values do
        local h = state.values[i] / maxv * avail
        local x = BAR_X0 + (i - 1) * (BAR_W + BAR_GAP)
        table.insert(ops, {
            kind = "rect",
            x = x,
            y = BASELINE - h,
            w = BAR_W,
            h = h,
            color = "#4c8dff",
        })
    end
    table.insert(ops, {
        kind = "line",
        x1 = 0,
        y1 = BASELINE,
        x2 = CANVAS_W,
        y2 = BASELINE,
        w = 1,
        color = "#8a8f98",
    })
    return ops
end

local function addRandomDay()
    table.remove(state.values, 1)
    table.insert(state.values, math.random(5, 50))
    state.presses += 1
end

local function reset()
    state.values = copyInitial()
    state.presses += 1
end

local function button(key: string, label: string, handler)
    return ui.row({
        key = key,
        px = 12,
        py = 6,
        radius = 6,
        fill = "control-off",
        hover = "hover",
        press = "press",
        onPress = handler,
    }, { label })
end

return {
    render = function(view)
        local sum = total()
        local avg = sum / #state.values
        local summary = string.format("Total: %d, average: %.1f", sum, avg)
        local updated = string.format("last updated by button press %d", state.presses)

        return ui.col({ pad = 16, gap = 12 }, {
            ui.text("Usage", { size = 22, weight = "bold" }),
            ui.canvas({ w = CANVAS_W, h = CANVAS_H, ops = chartOps() }),
            ui.row({ gap = 8 }, {
                button("add", "Add random day", addRandomDay),
                button("reset", "Reset", reset),
            }),
            ui.text(summary, {}),
            ui.row({ gap = 12, align = "center" }, {
                ui.image({ src = "logo.png", w = 48, h = 48 }),
                ui.text(updated, {}),
            }),
        })
    end,

    saveHotState = function()
        return state
    end,

    loadHotState = function(saved)
        if type(saved) ~= "table" then
            return
        end
        if type(saved.values) == "table" and #saved.values == DAYS then
            local vals = {}
            for i = 1, DAYS do
                vals[i] = tonumber(saved.values[i]) or 0
            end
            state.values = vals
        end
        if type(saved.presses) == "number" then
            state.presses = saved.presses
        end
    end,
}
