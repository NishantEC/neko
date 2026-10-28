## One registry: plugins are not a separate world (2026-10-22)

Owner: "plugins and all are a different thing in our heads, it should submerge."
Full design: `docs/design/one-registry.md`. Built-in pages (`web/src/pages/index.ts`)
and pages in `<data dir>/plugins/<name>/` register through the **same**
`host.registerPage` (`web/src/host.ts`) into one registry (`registry.ts`); the sidebar,
⌘K and router read it. There is no Plugins sidebar group: a new id is a page of its
own (group from its manifest, marked "yours"); a built-in's id **replaces** it or
**wraps** it (`wrap(Original)`), shown as "Customised" with a **Reset to original**
(kept in the app's own storage, `_neko/overrides-off`). A built-in that needs
something from the shell gets it through the host, so a user page can reach it too.
`host.storage` is JSON kept by the daemon in `<data dir>/plugin-data/<id>/`
(`/api/storage/...`, 1 MiB / 256 keys, containment and symlink refusal, same
Host/Origin/token gates); `<data dir>/custom.css` is served live and linked last.
`host.addSection` / `host.addTab` extend Settings and the task sheet. The Plugins
page is **Customisations** (`#/plugins` redirects); **Change Neko** tells Neko to
choose a page/style of the user's (live in seconds) or an edit of the app source in
the neko checkout (live through self-update, previous UI kept; Settings → Neko
updates has **Roll back the web UI**, `RollBackWebUi`). User pages keep the
authority they had (a browser page with the bridge token); the daemon gained
storage and the stylesheet only. `scripts/verify/customise.mjs` checks all of it.

**Themes are files too (2026-10-28).** `<data dir>/themes/<id>.json` is a colour
theme (schema in `docs/design/themes.md` section 8): the daemon validates and
lists them (`ListThemes`), the Appearance page shows them in the light or dark
picker, and the "Make me a theme" button asks Neko to write one. `custom.css`
still wins over them (they live in `@layer neko-theme`).
