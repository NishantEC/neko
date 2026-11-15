# Writing a plugin for the Rune plugin host

A plugin is a folder `plugins/<name>/` with an `init.luau` (more `.luau` files may sit beside
it; `require("@self/other")` loads `other.luau`). It appears as a page in the sidebar within a
second of being written, and reloads in place when saved (its `state` survives if it exports
`saveHotState` / `loadHotState`). There is no build step.

Language: Luau (Roblox's typed Lua). Start the file with `--!strict` if you want type checks.
The module returns a table:

```luau
return {
    render = function(view) ... return node end,   -- required
    onInput = function(id: string, text: string) end, -- optional, see ui.input
    saveHotState = function() return state end,       -- optional
    loadHotState = function(saved) ... end,           -- optional
}
```

`render` runs again after every event (a press, an edit) and when a task finishes. Keep your
state in local tables at the top of the file and mutate it in handlers.

## Host module

```luau
local host = require("@neko/host")
local reply = host.request('{"ListTasks":{}}')  -- a JSON string, as for the daemon
-- reply is a table: { ok = true, echo = "<your json>", tasks = { { id = 1, title = "..." }, ... } }
local text = host.inputValue("q")  -- the current text of the ui.input with id "q"
```

`host.request` waits (about 150 ms) without freezing the UI, but only works inside a handler
(an `onPress` function) or a `task.spawn`, never directly inside `render`.

## Nodes: `local ui = require("@rune/ui")`

From Rune's `@rune/ui`: `ui.row(props, children)`, `ui.col(props, children)`,
`ui.box(props, children)`, `ui.text(content, props)`, `ui.spacer()`, `ui.hex("#rrggbb")`,
`ui.virtualList{...}`, `ui.each(items, key, version, build)`. A string child is text; `nil` and
`false` children are skipped.

Props (all optional): `key` (a unique string, needed for any element a person presses),
`w`/`h` (points, or `"full"`), `minW`..`maxH`, `grow`, `shrink`, `pad`, `px`, `py`, `pt`, `pb`,
`pl`, `pr`, `m`.., `gap`, `align` (`"center"`, `"start"`, `"end"`), `justify`, `center`, `wrap`,
`fill`, `hover`, `press` (colours for those states), `border`, `borderWidth`, `radius`, `clip`,
`scrollY`, `scrollX`, `opacity`, `role`, `label`, `onPress` (a function), `focusable`, `disabled`.
Text props: `size`, `weight` (`"regular"`, `"medium"`, `"semibold"`, `"bold"`), `color`, `mono`,
`truncate`, `noWrap`, `textAlign`.

Colours are palette names (`"bg"`, `"text"`, `"text-secondary"`, `"separator"`, `"control-off"`,
`"hover"`, `"press"`) or `ui.hex("#4c8dff")` (a plain `"#4c8dff"` string also works).

The host adds three more nodes:

* `ui.input({ id = "q", placeholder = "Type here" })`: a one-line text field. Give it a unique
  `id`. After each edit the host calls your exported `onInput(id, text)`.
* `ui.image({ src = "logo.png", w = 96, h = 96 })`: a PNG or JPEG file inside your own plugin
  folder (no `..`, no absolute paths).
* `ui.canvas({ w = 180, h = 104, ops = { ... } })`: draws plain data. Each op is a table with
  `kind = "rect"` (`x`, `y`, `w`, `h`, `color`) or `kind = "line"` (`x1`, `y1`, `x2`, `y2`, `w`,
  `color`), with `color` a `"#rrggbb"` string. Coordinates are points from the canvas's top-left.

## Tasks

`task.spawn(fn)`, `task.delay(seconds, fn)`, `task.wait(seconds)` (inside a task only) and
`task.cancel(thread)`. A handler runs as a task, so it may call `host.request` and `task.wait`.

## Limits

No `io`, no `os` (except `os.time`), no network, no files outside the plugin folder. One entry
into your code may run 250 ms and your VM may use 32 MB; past that the entry is stopped and
the page shows an error card, and the app carries on.
