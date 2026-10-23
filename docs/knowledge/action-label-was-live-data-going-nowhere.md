## `action_label` was live data going nowhere

Found by photographing the surfaces that had never been seen, which is what
the captures were for.

**Every provider has always set `SearchItem::action_label` and nothing
rendered it.** The footer that used to carry it was removed and nothing
replaced it, so the field was dead across the whole app. That is tolerable
for a row whose verb is obvious — Enter on an application launches it — and
it is not tolerable for two of the rows built for the control plane:

- **A schedule's Enter pauses *or* resumes**, and the label is the only thing
  in the interface that says which. `schedules.rs` sets it per row precisely
  because "a label reading Pause on something already paused is the one thing
  a person could not recover from misreading" — and then no label was shown
  at all.
- **An `ask` row's Enter runs a proposed tool call.** A confirmation step
  that does not say it is one is not a confirmation. The whole safety story
  of `neko_core::ask` is "it proposes, you confirm", and the confirm half was
  invisible.

`render_row` now draws it **on the selected row only** — Raycast's own
arrangement. It is the row about to be acted on, it costs no chrome, and a
verb on all eight rows at once would be eight answers to a question that has
one. Dropped for `compact` rows (the 264px mode column has no room, the same
rule that drops subtitle and accessory there).

Captured: `docs/evidence/`'s `schedules-mode.png` (`ACTIVE` · `Pause ↵`),
`agents-mode-actions-menu.png` (`LIVE` · `Send ↵`, with the five session
modes plus Cancel run and a red Archive), `ask-neko-invitation.png` and
`ask-neko-proposed-call.png` — the second showing
`Cancel the run in agent 05475348` over
`cancel_agent(agentId: 05475348-2409-4eca-aa5f-3446f369ee66)`, which is the
two-step working end to end.

`NEKO_MODE_QUERY` / `NEKO_MODE_CONFIRM` are the hooks that made the two-step
photographable: every hook before them stopped at the moment a mode opened,
which is enough for a list and not for a mode whose whole behaviour is a
second step.
