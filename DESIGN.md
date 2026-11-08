# DESIGN.md: how Neko looks and behaves

Any agent changing Neko's interface (the web app in `web/`, plugins, artifacts
made for Neko itself) follows this file; it wins over a generic design system.
The native rules and the component catalogue are in
`docs/design/principles.md`; this file is their web-and-workspace companion.
Studied from yoji (Rhys Hanak, 2026-11-05: "I hated context switching between
agents and apps"): its principles, never its brand or assets.

## 1. One window, opened as tabs

Neko is one window. Every place opens as a **tab** in a tab strip at the top,
with back and forward and `+`: a page (Inbox, Tasks, Calendar, Canvas), a
workspace, a single task, an agent, an artifact. Opening something from a link
opens or focuses its tab instead of navigating away. Nothing important lives
only in a sidebar; the sidebar, if shown, is a launcher.

## 2. Quiet surfaces, colour only for meaning

Near-black (dark) or near-white (light) surfaces, one border tone, one large
window radius, text in three greys (`--foreground`, muted, faint). Colour
appears only where it carries meaning, one meaning per colour, always paired
with text or a symbol (principles §5):

| Colour | Means |
|---|---|
| Iris (`--primary`) | the primary action, focus, selection, **now** (today, the current-time line) and **references** (a task id or artifact named in text, which is a link) |
| Amber | needs you; in progress |
| Sky | ready to review |
| Mint | done; calendar events |
| Coral | failed; urgent |

No decorative gradients inside the app. Backgrounds behind a screenshot are
for marketing, not the product.

## 3. One status language

Every task, routine item and run uses the same glyphs, everywhere:
dotted circle = not started / backlog, empty circle = to do, half-filled amber
circle = in progress, sky circle = in review, mint check = done, coral cross =
failed. A short id sits beside a title in the faint grey (`NK-36`), and work
with parts shows a ring and a count (`3/5`). Groups are collapsible with a
count (`In progress 14`); an empty group says so in one line (`No tasks`).

## 4. Agents are a crew

An agent is shown with its identity (name, icon, the model it runs) and its
state in words ("Idle · 6 in crew", "Working on NK-14"). When one agent hands
work to another, the conversation shows the handoff on one line
(`Builder → Reviewer  NK-14: …`). The person can hide the crew lines.

## 5. Everything links

A task id, artifact, routine or workspace named anywhere (a reply, the Inbox,
a commit message) is an Iris link that opens its tab. Tasks with a date appear
on the Calendar's all-day row beside meetings; artifacts appear on the Canvas
and beside the conversation that made them.

## 6. Copy is calm and honest

Lead with the status, then the bottleneck, then what (if anything) needs the
person. Say where a claim comes from when it was not checked ("per NK-14's
report, not verified"). When nothing needs the person, say exactly that
("Nothing needs you right now") instead of filling space.

## 7. Type and density

Inter at the sizes the web kit defines; titles semibold, body regular, ids and
timestamps faint and tabular. Comfortable rows (one line per item, details on
hover or in the tab), generous outer margins, no boxes inside boxes.

## 8. Calm pages: the enforceable rules (2026-11-08)

The owner: "the lines, the excessive titles … these actions are divided". A
page that breaks one of these is not finished. The kit makes the right thing
the easy thing: `SetupPage` / `WorkHeader` / `PageHeader` (title + `count`),
`Section` (quiet group heading + `count`, `collapsible`), `Panel` (a list
frame with no card), `Row`, `RowActions` / `RowAction`, `GlyphButton`
(`web/src/setup/kit.tsx`, `web/src/ui/status-glyph.tsx`).

1. **Page header = title only**, with an inline faint count (`Tasks 24`). At
   most one primary button on the right; every secondary page action goes in
   an overflow (`…`) menu or the right-click menu. No subtitle, no eyebrow,
   no sentence explaining the page, no section intro. Help text belongs in a
   tooltip, an empty state or the place where the choice is made.
2. **No lines between rows, no cards around lists.** Rows are separated by
   space and light up on hover (`hover:bg-foreground/[0.03]`, 10px radius).
   Group headings are small and quiet (12.5px, medium, muted, faint count),
   and collapsible when a page has several. A hairline is allowed only where
   two independent scroll areas meet (a split view's rail).
3. **One line per row**: status glyph, faint id, title, then right-aligned
   quiet meta (scope or repository, relative time). Secondary text (an error,
   a detail) only when it needs the person, truncated to one line, in full on
   expand, hover (`title`) or in the item's tab. Nothing that is the same on
   every row (the owner's own name, the default runtime) is shown on a row.
4. **Row actions on hover and focus.** Small icon buttons at the right
   (`RowActions`), each also in the row's right-click menu. Nothing on a row
   is permanent except the single most common action, expressed as the
   glyph (`GlyphButton`: click the glyph to mark done). A switch that shows
   state (on / paused) may stay.
5. **The rail holds six places**: Home, Inbox, Tasks, Calendar, Workspaces,
   and Settings at its foot, with tooltips. Every other page is one click
   away in "More" (a labelled menu), in ⌘K and in the `+` tab. A new page
   joins "More", not the rail.
6. **Tabs fit their title** (72–200px), with the same padding and a 2px gap;
   the active tab is a soft fill and medium weight, no border or underline.
   The strip has no bottom rule.
7. **One type scale**: page title 20px semibold; group heading 12.5px medium
   muted; row title 13.5px regular; meta and secondary 12.5px muted; ids 12px
   faint tabular. Bold is for the page title only; no bold-label-plus-
   paragraph blocks inside lists.
8. **Accessibility is not traded for calm**: hover-only actions are focusable
   and appear on focus; the glyph button is named for its action; every row
   action has an `aria-label` and a tooltip; collapsible groups are buttons
   with `aria-expanded`.
