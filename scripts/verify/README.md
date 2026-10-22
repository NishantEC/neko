# Per-area web checks

One `.mjs` file per area (`tasks.mjs`, `settings.mjs`, ...). `scripts/web-verify.mjs`
runs each after the Home/gallery/plugin checks, with the live page already signed in.

```js
export default async function ({ page, shot, a11y, check, sleep, PORT, DATA, OUT }) {
  await page.goto(`http://127.0.0.1:${PORT}/#/tasks`);
  await page.waitForSelector("[data-slot='task-row']");
  check(true, "tasks list renders");
  await shot("tasks-list");        // -> $NEKO_WEB_SHOTS/tasks-list.png
  await a11y("tasks (dark)");      // obvious accessibility problems
}
```

Several people can run this at once: give each run its own
`NEKO_WEB_PORT`, `NEKO_WEB_VERIFY_DATA`, `NEKO_WEB_SHOTS` and `NEKO_WEB_LEASE`, and
`NEKO_WEB_ONLY=tasks` to run just your area.
