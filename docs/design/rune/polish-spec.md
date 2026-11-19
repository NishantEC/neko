# Rune polish spec (2026-11-16)

These are the owner's decisions, so implement exactly. Measurements are in logical px, on one 4/8 px rhythm. They are taken from a reference sidebar the owner admired. We match its precision and structure, never its name, brand or artwork.

## 0. Built against real data, matched to the web UI

The first build looked right on test data and wrong on the owner's real data. These rules come first.

**Real-shaped data, never committed.**
- Copy the owner's daemon database to a temporary data dir: `cp "$HOME/Library/Application Support/neko/neko.db"*` into `/tmp/neko-polish-data/`.
- Run the isolated `web_harness` daemon on that copy. Never point anything at the live data dir, and never send writes.
- Screenshots of real data live only in `/tmp/neko-polish/`. They are **never committed**: the repo is public.
- Committed snaps use the synthetic harness data, as before.

**The web UI is the visual reference.**
- Rune's theme takes the **exact** token values from the web UI's CSS variables in `web/src` (background, foreground, muted, faint, border, primary, card, radius), and its font sizes and weights. Inter is the font face, and code uses the web's mono stack.
- Extract the values; do not approximate them. Write a test that compares Rune's token table with the parsed CSS values.
- For every screen, capture the web UI and the Rune app on the same copied data at the same window size (1440×900, 2×) and save them as pairs in `/tmp/neko-polish/<screen>-{web,rune}.png`.
  - Capture the web UI with the shared headless Chrome (`~/.agents/BROWSER.md`).
  - Capture the Rune window with `screencapture -x -o -l <window id>`.
- A screen is done when the owner's reviewer finds no difference in type, colour, spacing or hierarchy, other than the layout changes this spec asks for.

**Grouping rules (sessions column).**

| Group | What goes in it | Shown |
|---|---|---|
| **Needs you** | Tasks waiting on the person (plan approval, a question, a review or merge decision, a budget or turn cap) whose last activity is within 7 days, plus tasks that **failed within the last 48 hours** | At most 8 rows, then "Show N more" |
| **Running** | Active states (planning, building, reviewing) | All |
| **Recent** | Completed, failed or cancelled within the last 7 days, newest first | At most 5, plus "View all" |
| **Waiting a while** | Tasks still waiting on the person with no activity for more than 7 days | After Recent, **collapsed by default**, with a count |

- Failures older than 48 hours never appear in Needs you.
- Anything older than 7 days that isn't waiting appears only under "View all".

**Neko's chat holds conversation only.**
- System notices (self-update, reinstall, outage, rollback, anything posted by `neko_chat::post_notice`) never render as chat messages.
- They fold into one quiet **Activity** line at the top of the chat: an icon, the latest notice in 12.5 muted, and "+N" faint. Clicking it expands the list in place; it is collapsed by default.
- The chat shows the last 30 messages, with "Earlier messages" at the top to load more.

**Window chrome.**
- Hide the title bar and run the content under it (full-size content view).
- Place the traffic lights so their centres line up with the sidebar header row's centre, 12 px from the window's left edge.
- The sidebar header's text starts after the traffic lights: an 80 px left inset on that row only.
- Dragging the header rows of the sidebar, session view and Neko column moves the window.
- **A folded sessions column has a way back (2026-11-19).** While it is folded, the session region's header row starts with a **"Show sidebar" button**: 28×28, radius 8, the foreground at 4% under the pointer, a 16 px stroke-1.5 "sidebar-left" glyph (`icons::SIDEBAR_LEFT`), its left edge at x = 80 from the window's left (just after the traffic lights) and centred on their line (y = 28). Tooltip "Show sidebar ⌘\", accessible label "Show sidebar". The header's own content (title, status glyph) moves right of it. The fold button in the sidebar's header keeps its place and its tooltip is "Hide sidebar ⌘\". **⌘\** toggles the column both ways (⌘B, the older binding, still does); the fold is saved with the widths. On Home, which has no header bar, the same button floats at the same spot over the art (§2).

## 1. Sessions column (sidebar)

- **Width:** 248 by default, resizable 232–360. **Outer padding:** 12 on all sides.
- **Row height:** 32 for every row. **Gap between sections:** 8. **Row radius:** 8. Row inner padding is 8 horizontal; the icon box is 16, then an 8 gap, then the label.
- **Label:** 13.5 regular. **Meta (right):** 12.5, muted, tabular numbers. **Section header:** 12.5 medium, muted, with the count right-aligned. Header rows are also 32 tall.
- **Hover:** fill foreground at 4%. **Selected:** foreground at 7%. **Primary row:** "New session" has a constant foreground fill of 7% and an Iris "+" icon.

Top to bottom:

| Block | Height | Content |
|---|---|---|
| Header | 32 | Neko mark (16) + "Neko" 14 semibold; collapse-sidebar icon button on the right |
| gap 8 | | |
| Search | 32 | Field with radius 8 and a 1 px border at foreground 10%; magnifier icon, "Search sessions" placeholder, a faint "⌘K" hint on the right. Clicking it opens the ⌘K palette. |
| gap 8 | | |
| New session | 32 | Primary row |
| Nav | 3 × 32 | Routines (active count) · Calendar (count of today's events) · Workspaces (count) |
| gap 8 | | |
| Needs you | 32 + n × 32 | Header "Needs you N". Rows show a **reason icon**, the title and the age. |
| gap 8 | | |
| Running | 32 + n × 32 | Header "Running N". Rows show a **progress ring**, the title and "4/7". |
| gap 8 | | |
| Recent | 32 + n × 32 | Header "Recent" with a quiet "View all" link on the right. Rows show a done icon, the title and the age. Show at most 5. |
| flexible space | | The list scrolls inside this area. Header, search, New session and Nav stay pinned. |
| Account footer | 40 | Avatar 24 (the macOS account picture, else initials on an Iris circle) · name 13.5 · subtitle 12 muted ("3 agents · Claude, Codex"). A chevron on the right opens a menu: Settings · Usage · Plugins · Open the web UI · Quit. |

**Reason icons (Needs you).** Stroke 1.5, size 16, colour muted. Amber only for the icon itself on hover.

| Waiting on | Icon |
|---|---|
| Plan approval | list (three lines) |
| A question or answer | `?` |
| A review or merge decision | branch fork |
| Failed | × in coral |
| Budget or turn cap | gauge |
| Anything else | dot |

**Progress ring (Running).** 16 px, 2 px track at foreground 12%, Iris arc. Fraction: goal turns used / max when there's a goal, else plan steps done / total, else an indeterminate rotating 90° arc (1.2 s per turn, static under Reduce Motion). Meta shows "n/m".

**Done icons (Recent).** Check for completed, × for failed or cancelled, both in muted grey. Colour appears only in the Needs-you and Running icons.

## 2. Home (session region when no session is selected, and after "New session")

- Centred column, max width 680, vertically centred slightly above the middle (40% from the top).
- **Greeting:** "Good evening, {first name}" (morning before 12, afternoon before 18, evening after). 34 semibold, Neko mark (28) to its left.
- **Status line** under it, 13.5: an Iris square (6) with "3 running", then an Amber square (6) with "3 need you". Each part is clickable and filters the sessions list. It shows "Nothing running" when both are 0.
- **Composer card** 24 below: **frosted glass over the art** (2026-11-19), radius 16, padding 16. Behind it, inside its rect and clipped to its radius, a **backdrop blur of the art, sigma 20 logical px** (scaled by the window scale); over the blur a tint of the theme's background colour (**55% alpha in dark, 60% in light**); a 1 px border at foreground 10%; a 1 px inner top highlight at foreground 6%. The text and controls inside draw above the glass at full opacity (no blur, no dither). **Fallback:** under Reduce Transparency, or when the window's target cannot be copied, the card is opaque: the background colour with foreground 4% on it, the same border. Only Home uses glass; session views, pages and the Neko column are unchanged. The floating "Show sidebar" button on Home (when the sessions column is folded, §0) sits on the same glass at radius 8.
- **Greeting and status line** stay crisp over the art (no glass of their own), each with a soft shadow for legibility: black at 35% alpha in dark, white at 50% in light, blur 8, offset 0,1. The radial scrim (§3) stays.
  - **Text area:** min 3 lines, grows to 12. Placeholder "Describe a task for {workspace}".
  - **Bottom bar,** left to right: a "+" attach button (32 square); workspace select (folder icon, workspace name, branch in muted, chevron); runtime select (Claude / Codex / OpenCode, each with a small logo-less glyph and name). On the right: a **Plan / Build** segmented control (Plan = planning first, Build = start building), a model select ("Auto" by default), and a send button (32 square, radius 10, filled Iris when text is present, else foreground 10%).
  - ⌘↩ sends. Sending creates the session and selects it.

## 3. Home art (our own, original)

A full-bleed background behind Home only, drawn as a GPU shader through Rune's effect/Compositor path.

- **Scene:** a night sky with sparse stars, a large soft moon top-right, and a calm sea filling the lower third, with a moon-glade reflection. On the horizon, the silhouette of a **sitting cat** (Neko's motif) on a small rock, facing the moon. Do not use boats or any element from the reference image.
- **Style:** ordered-dither / halftone in one hue (the theme's Iris) on the near-black background, the 1-bit look made with a Bayer 8×8 threshold over a smooth value field. No raster images; everything is procedural.
- **Motion:** stars twinkle (each a random phase, period 3–7 s), the sea's halftone drifts at 6 px/s, and the moon glade shimmers.
  - It stops completely under Reduce Motion.
  - It pauses when the window is inactive or Home is hidden.
- **Light theme:** the same scene as dawn. Pale sky, Iris at 35% strength, and a darker cat silhouette.
- **Legibility:** behind the greeting and composer, a radial scrim at background 70% (radius 520) keeps the text at AA contrast.
- **Budget:** at most 1.0 ms GPU per frame at 2560×1600, measured, **the art and the composer's glass together** (the glass added about 0.1 ms to the art's 0.8; numbers in `docs/knowledge/rune-polish.md`). The art pauses when the window is inactive or Home is hidden, and then nothing re-blurs.
- **Sessions:** the art never appears behind sessions, only on Home.

## 4. Everything else, made consistent

- **Neko column and context panel:** the same row metrics (32 rows, 12 padding, 8 gaps, 8 radius) and section header style as the sidebar.
- **Session view header:** 32 high, aligned to the sidebar header's baseline.
- **One icon set,** stroke 1.5, 16 px, in one module: `apps/neko-rune/src/icons.rs`. No emoji. No mixed stroke widths.
- **Focus ring:** 2 px Iris at 60%, offset 2, on every focusable control.
- **Motion:**
  - hover fills: 120 ms ease-out;
  - panel open and close: 180 ms spring (rune-motion, Snappy);
  - nothing else animates.
- **Window background:** sidebar at background + 2% lightness, session region at background, both in dark and light.
