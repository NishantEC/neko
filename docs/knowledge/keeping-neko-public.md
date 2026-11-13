# Keeping Neko public (2026-11-13)

Neko's repository is public. Owner's rule: anyone may read and use it, and
nothing in it may reveal the owner's personal or work-related information.

## The rule

Nothing personal or work-related enters the repo: not in code, tests, docs,
design files, scripts, screenshots or captured evidence. That covers the
owner's names, handles and emails; home paths; employers and clients and their
products, repositories, orgs, tracker keys and PR numbers; MCP server names
that reveal an org; cloud project ids; real meeting, calendar or message
content; colleagues' names; and any screenshot or log of the owner's own apps
or data (a sidebar full of real ticket titles counts).

## Placeholders

| Instead of | Write |
| --- | --- |
| the owner, a home path | `Asha`, `/Users/example`, `~` |
| an employer or client | `Acme`, `Client A`, `Client B`, org `acme` |
| its repositories | `acme-api`, `acme-web`, `acme-app` |
| a tracker key, a PR number | `ENG-123`, `#101` |
| an org-revealing MCP server | `slack-work`, `slack-client-a`, `ops-mcp-prod` |
| a colleague | `alex`, `Sam` |
| the owner's other product | `ui-kit` (the reference UI kit the web UI was ported from) |

Third-party projects and public demos are public facts and may be named.
Captured screenshots must come from sample data (the Design lab, fixtures),
never from the owner's live workspace. When unsure, describe it generically.

## The check

`scripts/privacy-check.sh` fails on a real `/Users/<name>` path, an email that
is not a placeholder, and anything in `~/.config/neko/private-patterns.txt`
(one extended regex per line: names, employers, handles; case-insensitive).
That file is the owner's own list, lives outside the repo and must never be
committed. `NEKO_PRIVATE_PATTERNS` points the script at another file.

- `scripts/privacy-check.sh` scans every tracked text file.
- `scripts/privacy-check.sh --staged` scans only the lines a commit adds; the
  pre-commit hook (`.githooks/pre-commit`) runs it first, before the compile
  check.
- A line ending in `privacy-ok: <reason>` is skipped. Used once for the
  project's own public repository URL (the update check and the About link).

## History

Scrubbing the working tree does not remove old versions from git history; a
rewrite (`git filter-repo --replace-text` plus path removal) is a separate,
owner-decided step.
