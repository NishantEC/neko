# Rune polish spec (2026-11-16)

These are the owner's decisions, so implement exactly. Measurements are in logical px, on one 4/8 px rhythm. They are taken from a reference sidebar the owner admired. We match its precision and structure, never its name, brand or artwork.

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
- **Composer card** 24 below: radius 16, fill at foreground 4% over the art, 1 px border at foreground 10%, padding 16.
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
- **Budget:** at most 1.0 ms GPU per frame at 2560×1600, measured.
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
