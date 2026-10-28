# Themes for the web UI: research and proposal

Status: proposal, nothing built (2026-10-28). Research read from the installed
Paseo (`/Applications/Paseo.app`, bundle `app-dist/_expo/static/js/web/index-*.js`,
read only) and from Neko's source.

## 1. What Paseo does

**One list, one setting, not two pickers.** The owner remembered separate Light
and Dark pickers and Gruvbox-style names. The bundle says otherwise, and the two
memories come from two different features:

| Feature | Setting | Choices |
| --- | --- | --- |
| App theme | `theme` (plus `pluginThemeId`) | `auto` (shown "System"), `light`, `dark`, then the dark variants `zinc`, `midnight`, `claude`, `ghostty`, `pureBlack` (shown "Pure black"), then any plugin theme |
| Code highlight theme | `syntaxTheme` (default `one`) | GitHub, Catppuccin, Dracula, Tokyo Night, One, Nord, Gruvbox, Solarized (all but Dracula have a light and a dark palette; the app's light/dark picks which) |

So Gruvbox, Nord and the rest exist in Paseo only as *code colours*. The app
chrome has three primary choices and five dark variants.

- **Where it lives.** `app-settings` in the renderer's storage (this machine
  holds `"theme":"auto"`), not `~/.paseo/config.json`. It is per app install.
- **Definition.** A theme is a JS object of semantic colours built by two
  helpers (`buildDarkSemanticColors`, `buildLightSemanticColors`) from about 18
  inputs: `surface0..surface4`, `surfaceSidebar`, `surfaceDiffEmpty`,
  `foreground`, `foregroundMuted`, `foregroundExtraMuted`, `border`,
  `borderAccent`, `accent`, `accentBright`, `destructive`, `terminalBlack`,
  `terminalBrightBlack`, `ring`. Everything else (popover, secondary, muted,
  input, hover and selected sidebar, ANSI terminal colours, diff colours) is
  derived. Status colours (success, danger, warning, merged) and the focus tint
  are fixed per light/dark, so a variant cannot change them. Shared by all
  themes: spacing, type scale, radii, shadows.
- **Applied** through react-native-unistyles: each theme is registered by name
  and `UnistylesRuntime.setTheme(name)` swaps it. `auto` turns on
  `setAdaptiveThemes(true)`, which follows the OS and always lands on plain
  `light` or plain `dark`. **Following the system cannot pick a variant**: a
  user who likes Midnight must give up system-follow. This is the gap the
  Neko design closes.
- **Plugin themes** are a strict schema: `{ id, name, appearance: "light" |
  "dark", colors: { background, foreground, raised, control, border,
  mutedForeground, ring, accent? } }`, hex only (3, 6 or 8 digits), extra keys
  rejected. Eight colours; Paseo derives the rest. Light or dark is declared by
  the theme.
- **Settings UI.** Appearance has a "Theme" row with a dropdown: Light, Dark,
  System, a separator, the five dark variants, a separator, plugin themes. Each
  item carries a one-colour swatch (`#ffffff`, `#2D8B62`, `#808080`, `#4A6BA8`,
  `#D97757`, `#8caaee`, `#000000`). No previews of the real surfaces. A second
  row picks the code highlight theme from a dropdown.
- **Not found:** any contrast test on the built-in themes.

Worth taking from Paseo: derive most tokens from a small set; a tiny strict
plugin schema; dark variants that differ in surface temperature rather than
hue. Not worth taking: one flat list, system-follow that forgets the variant,
fixed status colours.

## 2. What Neko has, and the gaps

**Web today** (`web/src/theme.ts`, `styles/globals.css`): four choices, System,
Light, Dark, Glass, kept as `neko-theme` in `localStorage` (so per browser, not
per Neko). `<html>` gets `dark` or `light`, and `glass` on top of `dark`.
Tokens are hand-written in `:root`, `.dark` and `html.glass`: about 45
variables (`--background --card --popover --foreground --strong --primary
--secondary --muted --accent --destructive --border --input --ring --info
--success --warning`, each with `-foreground` where needed, `--sidebar*`,
`--code`, `--bubble`, `--panel*`, `--glow-*`, scrollbar). The builder's
`--builder-chrome` and `--builder-pane` (tabs and panes) are derived from
those in `work/builder/BuilderViewTabs.css` and `work/dock/dock.css`, with
different rules for `.dark`. Picker UI exists in three places: `ThemeMenu`
(sidebar gear), the Appearance row in Settings → General, and four ⌘K commands.

**Native** (`crates/neko/src/theme.rs`, `neko_protocol::BUILTIN_THEMES`,
`PaletteThemes.swift`): 18 themes, one id each, stored as one setting
(`neko_core::themes`, `GetTheme`, `ThemeChanged`): `neutral`, `ember`,
`catnap`, `glass`, `catppuccin-{mocha,macchiato,frappe,latte}`,
`gruvbox-{dark,light}`, `solarized-{dark,light}`, `nord`, `tokyo-night`,
`rose-pine`, `rose-pine-dawn`, `dracula`, `everforest-dark`. A `Spec` of about
19 independent colours; `build()` derives the rest; palettes are vendored MIT
with a licence table. `themes_all_pass_wcag_aa` fails the build under 4.5:1 for
11 text-on-surface pairs. Four of the 18 are light.

**Gaps**

1. Web and native disagree: different lists, different storage, no shared
   setting. The native quick panel and the web UI can show different looks.
2. Only one light and one dark look on the web, and Glass lives outside the
   light/dark idea (it is a fourth radio that secretly means "dark plus").
3. No contrast gate on the web. Glass has a one-off evidence report; nothing
   fails a build.
4. Tokens are hand-copied three times (`:root`, `.dark`, `html.glass`), so a
   new theme means 45 edited lines and no derivation rules.
5. First paint: `index.html` hard-codes `class="dark"` and the choice is
   applied from `main.tsx` (a deferred module), so a light user can see one dark
   frame.
6. Persistence is per browser (`localStorage`), but Neko is one daemon seen
   from the browser and the native window.
7. Plugins cannot contribute a theme; they can only override variables from
   `custom.css`.

## 3. Proposal

**Two pickers plus a mode.** Settings: `mode` (`system` | `light` | `dark`),
`light` (a theme id whose appearance is light), `dark` (a theme id whose
appearance is dark). The active theme is `mode === "system" ? (OS dark ? dark :
light) : mode`, then that slot's id. This is the thing Paseo cannot do.

**A theme is data**, one JSON object, `web/src/themes/themes.json`:

```json
{
  "id": "gruvbox-dark", "name": "Gruvbox Dark", "appearance": "dark", "family": "Gruvbox",
  "colors": {
    "background": "#282828", "card": "#32302f", "popover": "#3c3836", "sidebar": "#1d2021",
    "foreground": "#ebdbb2", "strong": "#fbf1c7", "mutedForeground": "#bdae93",
    "border": "#ebdbb2",
    "primary": "#d79921", "primaryForeground": "#1d2021",
    "destructive": "#fb4934", "success": "#b8bb26", "warning": "#fabd2f", "info": "#83a598",
    "bubble": "#504945", "bubbleForeground": "#fbf1c7"
  }
}
```

Required: `appearance`, `background`, `foreground`, `primary`. Everything else is
derived when absent, so a plugin theme can be six lines. A theme may also set
`"effects": { "glass": true }` (Glass only, see §4). The authored set is the
user's "colours for background, card, border, text, primary, accent, status
colours, sidebar, builder chrome/pane":

| Group | Authored | Derived (one function, one place, like native `build()`) |
| --- | --- | --- |
| Surfaces | `background`, `card`, `popover`, `sidebar` | `--panel*`, `--code` (mix of background), `--input` |
| Text | `foreground`, `strong`, `mutedForeground` | `--card-foreground`, `--popover-foreground`, `--sidebar-foreground`, `--code-foreground` |
| Lines | `border` (hue) | `--border` at 7% dark / 8% light, `--sidebar-border`, `--input`, `--panel-border` |
| Action | `primary`, `primaryForeground` | `--ring` (primary at 55-65%), `--sidebar-primary*`, `--glow-*` |
| Hover | none | `--secondary`, `--muted`, `--accent`, `--sidebar-accent` = foreground at 4-7% |
| Status | `destructive`, `success`, `warning`, `info` | the `-foreground` text twin of each, lightened or darkened until it clears 4.5:1 on `card` |
| Chat | `bubble`, `bubbleForeground` | none |
| Builder | none | `--builder-chrome` = light ? `sidebar` : `background`; `--builder-pane` = light ? `card` : `background` mixed 4% toward white. This is today's rule, so the tabs look the same. |

Because the builder tokens stay derived, a theme cannot make the tabs
inconsistent with the page.

**Shipped list: 12**, ids aligned with the native `BUILTIN_THEMES`:

- Light (4): `neutral-light` "Neko Light" (today's `:root`; **new native id**),
  `catppuccin-latte`, `gruvbox-light`, `solarized-light`.
- Dark (8): `neutral` "Neko Dark" (today's `.dark`), `glass`,
  `catppuccin-mocha`, `gruvbox-dark`, `solarized-dark`, `nord`, `tokyo-night`,
  `rose-pine`.

All eleven non-Neko ones already exist natively, MIT-vendored, with tuned hex
to start from (port the `Spec` values; web needs extra pairs checked, see
below). Held back for a second batch, already native: `rose-pine-dawn`,
`dracula`, `everforest-dark`, `catppuccin-{macchiato,frappe}`, `ember`,
`catnap`. Not taken from Paseo: Zinc, Midnight, Claude, Ghostty (all dark
variants of near-neutral surfaces; Neko Dark and Nord cover them). Pure Black
(OLED) is a cheap add later.

**Contrast rules**, reusing the native test's shape. A node script
`web/scripts/verify/themes.mjs` (the web has no unit-test runner; the other
checks are `scripts/verify/*.mjs`) runs the real `derive` function over every
theme and fails under **4.5:1** for text pairs, composited over the surface
first when a colour has alpha:

- `foreground`, `strong` and `mutedForeground` on `background`, `card`,
  `popover`, `sidebar`
- `primaryForeground` on `primary`; `bubbleForeground` on `bubble`
- each `*-foreground` status twin on `background` and `card`
- `code-foreground` on `code`

and one structural rule, from the native lesson that tuning must not hide a
failure by flattening a surface: `card` vs `background` and `sidebar` vs
`background` must be at least a visible step apart (threshold fixed from Neko
Dark's current values when the script is written), and `border` over `card` at
least 1.3:1 as a decoration. **Policy: move the text, not the surface.** Glass
keeps its existing rule: text measured against the brightest point of its wash.
A script also checks that every theme id exists in `BUILTIN_THEMES` (parsed from
`lib.rs`) and that the five shared colours (panel, raised, text primary and
secondary, danger) equal the native hex, so the two lists cannot drift.

Built-ins are gated; a plugin theme below 4.5:1 is still selectable but shown
with a "Low contrast" badge.

## 4. How it applies

**CSS comes from the data, not from JS at paint time.** A pure
`derive(theme) -> CSS custom properties` function (plain JS so the generator,
the verify script and the browser share it) is run by `pnpm themes`, which
writes `web/src/themes/themes.css` (committed; `verify` fails if it is stale).
Each theme is one block, `html[data-neko-theme="gruvbox-dark"] { --background:
…; }`, about 45 lines, so roughly 25 KB, a few KB gzipped. The two Neko
defaults replace the hand-written `:root` and `.dark`; `html.glass`'s wash and
backdrop rules move into the Glass block, keyed on `data-neko-theme="glass"`.

**Order of precedence stays the owner's.** The generated blocks go in
`@layer neko-theme`. Unlayered CSS always beats layered CSS, so a plugin's
`custom.css` (linked last, unlayered) still wins on any variable, exactly as it
beats `.dark` today. Do not set variables as inline styles on `<html>`: inline
beats `custom.css`.

**Attributes on `<html>`:** `data-neko-theme=<id>`, `data-appearance=light|dark`,
plus the existing `class="dark"` or `class="light"`, `style.colorScheme`, and
`class="glass"` for Glass, all kept so existing component CSS (`.dark
.builder-shell`, Tailwind `dark:` variants, `html.glass …`) and plugins keep
working. **Do not** use the attribute name `data-theme-id`: the T3 chat files
(`chat/t3/ComposerBanner.tsx`, `ComposerSurface.tsx`) already select on
`html[data-theme-id]`, and setting it would switch on rules that are written
for a different token set.

**No flash.** `index.html` gets a small blocking classic script in `<head>`,
`<script src="/theme-boot.js">` (the daemon's CSP is `script-src 'self'`, which
allows an external file and forbids an inline one). It reads
`localStorage["neko-appearance"]` (`{mode, light, dark}`), asks `matchMedia`,
sets the three attributes and `colorScheme`, and updates the `<meta
name="theme-color">`. It runs before first paint, so the page never shows the
wrong theme. It must not import anything. The hard-coded `class="dark"` in
`index.html` becomes the fallback only when the script cannot run.

**Where it persists: the daemon, with `localStorage` as a boot cache.**

- Source of truth: three daemon settings, `appearance.mode`,
  `appearance.light`, `appearance.dark` (the existing `settings` KV, as
  `neko_core::themes` does). One new request pair (`GetAppearance`,
  `SetAppearance`, which validates ids and appearance) and one event
  (`AppearanceChanged`), the same shape as `GetTheme`/`ThemeChanged`.
- Why not `localStorage` alone: it is per browser origin, so Chrome, Safari and
  Neko's own window each keep a different answer, and the native quick panel
  cannot read it. The owner wants all of them to agree.
- Why still mirror it in `localStorage`: the boot script cannot wait for a
  daemon round trip without a flash. The mirror is written after every change
  and after every load; the daemon wins on conflict, and an
  `AppearanceChanged` event updates open tabs live.
- System-follow stays client-side (`matchMedia`), because only the client knows
  its OS appearance. The daemon stores the choice, never the resolved result.
- Native then reads the same triple (§7 step 8) and resolves it with its own
  system appearance. The legacy single `theme` id keeps working until then.

## 5. Settings UI

**Settings → Appearance** becomes its own section (today a single row in
General). Top to bottom:

1. **Mode**: a three-way segmented switch, System / Light / Dark, with a
   one-line hint ("System follows your Mac. Light and Dark each use the theme
   you pick below.").
2. **Light theme** grid and **Dark theme** grid, one after the other. Each is a
   `radiogroup` of cards (4 and 8 cards, 4 per row on desktop). A card is a
   96 x 64 mock window drawn *from that theme's own tokens*: sidebar strip,
   background, one raised card, two text lines (foreground and muted), a
   primary pill, a status dot row. Under it the name, and a check on the
   selected one. The mock is the live swatch; there is no screenshot asset to
   maintain. A grid's heading says when it is in use ("In use now", or "Used
   when the system is light"). Arrow keys move, Enter or click selects; the
   check is shape plus text, not colour alone.
3. Selecting in the grid that is currently in use applies at once; selecting in
   the other grid only saves (its mock is the preview).
4. A small "Plugins can add themes" link to Customisations, shown only when
   there is at least one plugin theme (they appear as a third group, "Yours").

**Sidebar gear (`ThemeMenu`)** keeps its place and gets: a label "Appearance"
with the three mode radios; two submenus, "Light theme" and "Dark theme", each a
radio list with a small two-tone swatch (background and primary); a link "All
appearance settings"; then Keyboard shortcuts as now. The trigger's tooltip
reads "Theme: Gruvbox Dark (system)".

**⌘K** replaces its four commands with "Mode: System", "Mode: Light", "Mode:
Dark" and one "Theme: <name>" per theme (12, plus plugins), keyworded by family
("gruvbox", "catppuccin dark"). Rule: picking a theme sets its appearance's
slot, and if the other appearance is currently on screen it also sets the mode
to this one, so the command visibly does what it says.

## 6. Plugins, and migrating `neko-theme`

**Plugin compatibility.** No variable is renamed or removed, `dark`/`light`/
`glass` classes stay, and `custom.css` still wins (layering above). Anything a
page reads through `var(--…)` follows the theme; raw hex in a plugin never did
and still will not. Additions, all optional:

- `host.theme`: `get()` returns `{ mode, appearance, id, name }`, `subscribe`
  fires on any change, so a canvas or chart can redraw. Needs a line in the
  `host` doc header (`web/src/host.ts`).
- `host.registerTheme(spec)`: a plugin contributes a theme in the §3 schema.
  Strict like Paseo's: unknown keys rejected, hex only, ids namespaced as
  `<plugin>/<id>` so they cannot shadow a built-in, injected as a runtime
  `<style>` using the same `derive` (allowed by `style-src 'unsafe-inline'`) and
  inside `@layer neko-theme`. A theme that fails validation is skipped with a
  note on the Customisations page. Removing the plugin removes its theme; if it
  was selected, that slot falls back to the default and the owner is not asked.
- `scripts/verify/customise.mjs` gains a case: a plugin with `custom.css`
  overriding `--primary` under every built-in theme still wins.

**Migration of the `neko-theme` localStorage value** (runs once, in the boot
code, only if `neko-appearance` is missing; the daemon is seeded from it on
first connect, and then `neko-theme` is removed):

| Old value | mode | light | dark |
| --- | --- | --- | --- |
| absent or `system` | `system` | `neutral-light` | `neutral` |
| `light` | `light` | `neutral-light` | `neutral` |
| `dark` | `dark` | `neutral-light` | `neutral` |
| `glass` | `dark` | `neutral-light` | `glass` |

So nobody's screen changes on upgrade. `?theme=light|dark|glass` keeps working
for `scripts/web-verify.mjs` and `scripts/verify/shell.mjs`, and also accepts
any theme id (`?theme=gruvbox-dark`), which gives the verify scripts a way to
screenshot every theme. Daemon seeding when no browser has ever connected: the
defaults above (`system`, `neutral-light`, `neutral`); if the native legacy
`theme` id is light, it seeds `light`, otherwise it is left alone.

## 7. Build plan

Web-only steps ship with `scripts/ship-web.sh` (about 1 min); steps 4 and 8
touch Rust or Swift and need `install-native.sh`.

1. **Data, derive, generator, contrast script.** `themes.json` with Neko Light,
   Neko Dark and Glass reproduced from today's CSS, `derive`, `pnpm themes`,
   `verify/themes.mjs`, and the layered `themes.css`. Done when Neko Light/Dark
   screenshots match today's within rounding. ~4 h.
2. **Nine more themes**, ported from the native `Spec`s, tuned to AA. ~4 h (the
   native hex is already tuned for text-on-panel; the extra web pairs and
   status twins are where the tuning goes).
3. **`theme.ts` rewrite**: mode, two slots, `theme-boot.js`, migration, query
   param, `host.theme`. ~3 h.
4. **Daemon**: three settings, `GetAppearance`, `SetAppearance`,
   `AppearanceChanged`, seed from legacy, a test each. Web syncs in `theme.ts`.
   ~3 h.
5. **Appearance section** (mode switch, two grids, the swatch mock). ~4 h.
6. **Sidebar menu and ⌘K.** ~2 h.
7. **Plugins**: `registerTheme`, validation, `customise.mjs` cases, doc header.
   ~3 h.
8. **Native alignment** (separate task): add `neutral-light` to
   `BUILTIN_THEMES`, `theme.rs` and `PaletteThemes.swift`; quick panel and the
   embedded web view read the triple and follow macOS appearance; the native
   theme picker becomes light/dark slots. ~1 day.
9. **Verify**: `web-verify` screenshots of 3 key pages x 12 themes, plus the
   native-embedded view. ~2 h.

Web total about 2.5 days (steps 1 to 7, 9); native about 1 more.

**Risks**

- **Layer precedence.** Moving tokens into `@layer neko-theme` means any
  *unlayered* rule that still sets a token (the old `html.glass` block, a
  stray `.dark` override) silently beats the theme. Step 1 must grep for every
  rule that assigns one of the 45 variables and fold it in.
- **Derived colours drift from today's hand tuning.** The Neko defaults use
  hand-picked oklch values. Mitigation: author them as hex and compare
  screenshots; if a derived token is off, author it explicitly instead of
  bending the derivation.
- **Light variants of dark-first components.** `.dark .builder-shell` and
  similar rules assume two looks; a third light theme with a dark sidebar
  (Solarized) could expose them. Check the builder, dock and chat in every
  light theme.
- **Glass across the list.** It needs a wash and backdrop blur; it stays a
  special theme (`effects.glass`), cannot be authored by a plugin in v1, and
  keeps the solid fallback when `backdrop-filter` is missing.
- **Boot script drift.** `theme-boot.js` duplicates a little of `theme.ts`
  (resolve mode and set attributes). Keep it under 40 lines and verify it
  against `theme.ts` in `verify/themes.mjs`.
- **Licensing.** Every added palette needs its MIT row in
  `docs/evidence/themes-report.md`; the native ones already have it.
- **Not in scope:** code-block syntax colours (Paseo's second picker). Neko's
  chat code blocks could take a per-theme palette later; it needs its own
  contrast pass.
