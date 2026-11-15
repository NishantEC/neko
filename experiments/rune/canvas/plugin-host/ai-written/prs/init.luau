local ui = require("@rune/ui")
local host = require("@neko/host")

local state = {
    prs = {},
    filter = "",
    selected = nil,
    refreshing = false,
}

local function refresh()
    if state.refreshing then
        return
    end
    state.refreshing = true
    local ok, reply = pcall(host.request, '{"ListPrs":{}}')
    state.refreshing = false
    if ok and type(reply) == "table" and type(reply.tasks) == "table" then
        local list = {}
        for _, item in ipairs(reply.tasks) do
            table.insert(list, { id = item.id, title = tostring(item.title or "") })
        end
        state.prs = list
    end
end

local function select(id)
    state.selected = id
end

local function visible()
    local needle = string.lower(state.filter or "")
    local out = {}
    for _, pr in ipairs(state.prs) do
        if needle == "" or string.find(string.lower(pr.title), needle, 1, true) then
            table.insert(out, pr)
        end
    end
    return out
end

local function findSelected()
    if state.selected == nil then
        return nil
    end
    for _, pr in ipairs(state.prs) do
        if pr.id == state.selected then
            return pr
        end
    end
    return nil
end

local function prRow(pr)
    local isSel = state.selected ~= nil and pr.id == state.selected
    local id = pr.id
    return ui.row({
        key = "pr-" .. tostring(id),
        px = 10,
        py = 6,
        gap = 8,
        radius = 6,
        align = "center",
        fill = isSel and ui.hex("#4c8dff") or nil,
        hover = "hover",
        press = "press",
        selected = isSel,
        onPress = function()
            select(id)
        end,
    }, {
        ui.text("#" .. tostring(id), { color = isSel and "#ffffff" or "text-secondary", mono = true }),
        ui.text(pr.title, { color = isSel and "#ffffff" or "text", truncate = true }),
    })
end

return {
    render = function(view)
        local shown = visible()
        local rows = {}
        for _, pr in ipairs(shown) do
            table.insert(rows, prRow(pr))
        end

        local sel = findSelected()
        local detail
        if sel then
            detail = ui.col({ gap = 6 }, {
                ui.text(sel.title, { size = 18, weight = "semibold" }),
                ui.text("#" .. tostring(sel.id), { color = "text-secondary", mono = true }),
                ui.text("Selected", { color = "text-secondary" }),
            })
        else
            detail = ui.text("Select a pull request", { color = "text-secondary" })
        end

        local label = state.refreshing and "Refreshing…" or "Refresh"
        local counts = tostring(#shown) .. " shown of " .. tostring(#state.prs)

        return ui.col({ pad = 16, gap = 12, w = "full" }, {
            ui.row({ gap = 12, align = "center" }, {
                ui.text("Pull requests", { size = 20, weight = "bold" }),
                ui.spacer(),
                ui.row({
                    key = "refresh",
                    px = 10,
                    py = 4,
                    radius = 6,
                    fill = "control-off",
                    hover = "hover",
                    press = "press",
                    disabled = state.refreshing,
                    onPress = function()
                        refresh()
                    end,
                }, { label }),
            }),
            ui.row({ gap = 16 }, {
                ui.col({ w = 360, gap = 8 }, {
                    ui.input({ id = "filter", placeholder = "Filter by title" }),
                    ui.text(counts, { size = 12, color = "text-secondary" }),
                    ui.col({ gap = 2 }, rows),
                }),
                ui.box({ grow = 1, pad = 12 }, { detail }),
            }),
        })
    end,

    onInput = function(id: string, text: string)
        if id == "filter" then
            state.filter = text
        end
    end,

    saveHotState = function()
        local prs = {}
        for _, pr in ipairs(state.prs) do
            table.insert(prs, { id = pr.id, title = pr.title })
        end
        return {
            prs = prs,
            filter = state.filter,
            selected = state.selected,
        }
    end,

    loadHotState = function(saved)
        if type(saved) ~= "table" then
            return
        end
        if type(saved.prs) == "table" then
            local prs = {}
            for _, pr in ipairs(saved.prs) do
                table.insert(prs, { id = pr.id, title = tostring(pr.title or "") })
            end
            state.prs = prs
        end
        if type(saved.filter) == "string" then
            state.filter = saved.filter
        end
        state.selected = saved.selected
        state.refreshing = false
    end,
}
