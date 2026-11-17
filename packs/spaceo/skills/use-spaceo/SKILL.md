---
name: use-spaceo
description: Use SpaceO to work in Mac apps on a separate virtual display, never on the person's real screen.
---

# Use SpaceO for Mac app work

SpaceO gives you your own invisible display on this Mac. Use it for any work that has to happen inside a Mac app, so the person can keep using their screen.

## Rules

1. Create a session first: `spaceo_session_create`. Every app you open and every action you take belongs to that session.
2. Open the app inside the session (`spaceo_open_app`, `spaceo_open_url`), then act through the `spaceo_*` tools: read the interface (`spaceo_read_screen`, `spaceo_find`), then `spaceo_click`, `spaceo_type`, `spaceo_press_key`, `spaceo_menu`.
3. Check every receipt. Each one says confirmed, unconfirmed or refused. Treat unconfirmed as "not done yet": look at the screen again (`spaceo_read_screen` or `spaceo_screenshot`) before the next step. Never repeat a refused action without changing something.
4. Never use the person's real screen while SpaceO is available: do not drive their physical display, bring apps to the front on it, or take over the keyboard or pointer.
5. When a login, a payment, a two-factor prompt or any credential is needed, stop. Call `spaceo_session_pause` so the person can take over from SpaceO Viewer, say plainly what they need to do, and continue only after `spaceo_session_resume`.
6. When the work is done, destroy the session with `spaceo_session_destroy` so nothing is left running.

## If something is wrong

If a `spaceo_*` tool is missing or errors, tell the person SpaceO is not ready (its pack's health check in Neko's Setup shows why) instead of falling back to their real screen.
