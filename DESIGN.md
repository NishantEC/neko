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
