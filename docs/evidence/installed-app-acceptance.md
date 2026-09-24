# Installed native build — 2026-09-25

User approved installation and native acceptance after commit `3e0ab6f`.
Installed `/Users/nish/Applications/Neko.app` from freshly built debug client and
daemon. No existing bundle or Neko process was found in the standard application
locations, main checkout or worktree. No existing app was deleted/replaced.

Both Mach-O files and the bundle are Apple Development-signed using the user's
personal identity. `codesign --verify --deep --strict` passes on the installed
path. The bundle includes the existing `packaging/icon/neko.icns` and records
`NekoSourceRevision=3e0ab6f`. This is a local development-signed build, not a
notarized distribution release.

## Native verification

- LaunchServices launched the installed bundle, which spawned its own sibling
  daemon. Both process paths were verified under the installed app.
- Disposable profile data came from a fresh passing `smoke-profiles.mjs` run,
  `neko-profiles-smoke-zuXMGm`. The native log reports `loaded=true`,
  `connected=true`, and `key=false`.
- Window-only profile capture 35898 still showed stale initial Offline chrome.
  This remains a failed loaded-data visual check, not a successful profile UI
  acceptance result.
- A separate fresh-data Welcome launch rendered the six-step setup and existing
  mark correctly in window 35900, `key=false`.
- Accessibility trust is false for this app. Hotkey registration was deliberately
  disabled during evidence runs and is not accepted as verified.

Logs and captures: `/tmp/neko-install.xP41tg/` (`profiles.log`, `profiles.png`,
`setup.log`, `setup.png`). Both test client/daemon pairs were terminated afterward;
the installed app remains available for normal user launch. No synthetic input,
focus opt-in, real provider sign-in or production task execution was performed.

The existing database was preserved and backed up consistently through SQLite
to `/tmp/neko-install.xP41tg/pre-install-neko.db` before the user's first launch.
Its settings contained no workbench snapshot, so no existing Neko responsibilities
or scheduled-agent plans were found. The backup contains private application data
and stays inside the owner-only temporary directory; it is not committed.

## Still needs the user

Open the installed app normally and check navigation/input, then grant
Accessibility if a global shortcut is wanted. External MCP account sign-in,
manual setup timing and native interaction remain unverified. The repository's
no-synthetic-input and non-activating-evidence rules were preserved.
