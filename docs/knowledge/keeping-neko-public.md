# Keeping Neko public

Neko is a public repository. Nothing personal or work-related may enter it: not in code, comments, tests, fixtures, docs, renders
or commit messages.

## What to write instead

| Instead of | Write |
| --- | --- |
| a real person | Asha, Lee, "You" |
| an employer or client | Acme, Client A |
| a ticket key | ENG-123 |
| a home folder | `/Users/example` |
| an email address | `name@example.com` |
| a repository | `acme-api` |
| cats | miso, mochi, tofu |

## The check

`scripts/privacy-check.sh` scans every tracked text file; `--staged` scans only the lines a commit adds, and the pre-commit hook
runs that. It has two layers: generic rules (a real home folder, a real email address), and the owner's own list of patterns,
read from a file outside the repository (`NEKO_PRIVATE_PATTERNS`, default under `~/.config/neko/`). That file must never be
committed. A line that names something public on purpose carries the marker `privacy-ok` with the reason.

Commit identity counts too: commits use a no-reply address, never a personal one. Set it in the repository before the first
commit.
