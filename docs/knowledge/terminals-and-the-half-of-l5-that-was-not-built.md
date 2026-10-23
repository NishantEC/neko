## Terminals, and the half of L5 that was not built

`neko_core::terminals` — a `Terminals` mode listing Paseo's supervised
shells, each one's last screen in the detail pane, kill in `⌘K`.

**One call, not twenty-two.** `list_terminals` defaults to *the caller's own*
working directory, and a top-level caller like neko has none — a bare `{}`
comes back `cwd is required`. The plan for this said "fan out over
`list_workspaces`", which at 22 workspaces would have been forty-four `curl`
processes per keystroke. The tool takes **`all: true`**. Reading the schema
properly was worth more than any amount of optimising the wrong shape.

**The directory names the row.** Paseo numbers terminals per working
directory, so every one of them is called `Terminal 1` — the same collision
the agent tiles hit when three sessions shared a prompt. The path leads, the
name follows it for the case where one directory really does have several.

**Enter opens the folder; kill is `⌘K` and destructive.** Killing is the only
other verb this provider has, and a mis-keyed Enter that killed a session is
exactly the failure the arrangement exists to prevent.

**Private-use glyphs are replaced with spaces, not deleted.** A prompt theme
draws its separators from a patched font's private use area (U+E000–U+F8FF),
and neko renders in the system font, which has nothing there — every one
would be a tofu box. They become **spaces** rather than nothing because
terminal output is column-aligned, and deleting characters would shift every
line that had one out of step with the lines that did not.

**`SearchItem::preview` is the protocol's answer to "text too big for a
row".** The clipboard detail pane predates it and renders `SearchItem::id`,
which works only because a clipboard entry's id *is* its content; nothing
else has that coincidence. The same change stopped `render_mode_detail`
labelling every mode's fields as the clipboard's — a terminal's working
directory under a heading reading "Application" is a wrong label on a right
value, which is worse than no label.

**Captures are rate-limited, not cached** (`CAPTURE_TTL`, 2s): output changes
constantly, but every keystroke re-runs `search`, and capturing N terminals
per character is N `curl` pairs per letter. Bounded at `MAX_PREVIEWS` (8) and
fanned out in parallel, so the pane's freshness is not a function of how many
sessions happen to be open.

**Workspace scripts remain unbuilt, and that is a finding rather than a
gap.** `list_workspace_scripts` needs a `workspaceId` and all 22 real
workspaces return `{"scripts": []}`; no repo on this machine carries a Paseo
config. Building it would be unverifiable code — the same reason `usage`
declined Cursor, Kimi, MiniMax and Z.AI.

Verified live: `"terminals"` reaches the command; two real sessions listed
from one call; a preview carrying genuine output
(`❯ echo hello-from-neko` / `hello-from-neko`) with no private-use character
left in it; filtering to `neko` narrowed to one; kill removed them and the
list emptied. Every probe session was killed afterwards.
