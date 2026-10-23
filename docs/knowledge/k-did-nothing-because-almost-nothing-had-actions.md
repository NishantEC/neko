## `⌘K` did nothing, because almost nothing had actions

Reported as "command K does nothing", and the keystroke, the handler and the
menu were all fine. **Only agent and clipboard rows carried any actions** —
apps, files, settings, commands, preferences and themes all had
`actions: Vec::new()`, so `open_actions_menu_for_selected_row` returned early
and set no state. That is *most* of a root query, and it is always what the
default selection is: the panel opens on an application row, and the one
menu-opening keystroke in the app was dead there.

`provider::path_actions` gives every row that names a real path **Reveal in
Finder** and **Copy path**, performed by `perform_path_action`. Those two
generalise where nothing else does: `Open` on an application *runs* it and on
a file hands it to whatever owns the extension, and neither is what somebody
wants when they are trying to find where a thing lives or paste its location.
`AGENTS.md` had recorded "reveal in Finder" as a real secondary action
deferred *only* because no secondary-action affordance existed — that stopped
being true when the `⌘K` menu shipped, and nobody went back.

An app's id is its bundle id rather than its path, so `AppsProvider` looks the
bundle up before revealing it, the same way `activate` does.

**One wording for one condition, while in there.** Unknown actions were
reported five different ways (`no such action: x`, `clipboard has no action
'x'`, `provider 'p' has no action 'x'`). They all say `no action 'x' on this
row` now, except the trait default, which names the provider because a caller
reaching that has a routing bug rather than a typo.

**Rows that genuinely have no action still say so by omission**: the `⌘K`
hint in the input row is gated on the selected row actually having one, so
its absence is the signal. That is subtle, and it is honest — a hint for a
keystroke that does nothing would be worse.
