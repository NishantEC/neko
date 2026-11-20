# The deadline default (2026-11-20)

Spec: `docs/design/super-agent-features-2026-11.md`, F7. A goal that is Blocked on its one question can take its own
default answer after a wait the person chose. Off unless set.

- **Setting it.** `Goal::default_after_hours` (1 to 168, `None` is off), through `/goal ... --default-after <hours>`,
  `GoalCommand::Create { default_after_hours }` or `GoalCommand::SetDefaultAfter { id, hours }` (these are `Goals`
  sub-commands, not top-level requests). A goal a model proposed (`by_person == false`) ignores it. The web puts the
  select under the question ("If you do not answer, Neko takes the default after [Off]") and in a live goal's detail.
- **When it acts** (`blocked_deadline`, from `step`): the goal has a deadline; its question has the judge's own default
  (`Blocked::real_default`; the filler "Use your best judgement and keep going." never counts); it was asked since this
  build (`asked_ms > 0`); nobody switched it off for this question (`deadline_off`); the time is up; and its ticket has a
  start checkpoint. Without a checkpoint nothing is answered: `deadline_off` is set, one journal line says why, and it is
  not repeated. The answer goes through `apply_answer` inside `commit`, so a person's answer racing the tick wins.
  Journal: `Took the default after <h> h: <default>`.
- **The undo target.** `DefaultTaken { undo_run = next_run + 1, expires_ms = now + 7 days, .. }`: the next run's start
  checkpoint is the folder right before the post-answer work (it does not exist until that run begins). The goal tick
  clears an expired target ("The undo for the default answer expired"). Checkpoints themselves are kept 14 days.
- **`GoalCommand::UndoDefault { goal }`** (`Controller::undo_default`, taken before the database guard, under the goal
  lock): refuses with no target, a finished goal or an expired one; if the run exists it previews the undo
  (`through_later: true`) and a refusal comes back as its `checkpoint:<code>: <message>` text with nothing changed
  (busy, unsaved changes, pushed commits...); then it pauses the goal, undoes through the checkpoint code (a safety
  snapshot is taken first, as for any undo), and puts the question back with `deadline_off`, raised in the Inbox again.
  If no run has begun, only goal state is restored, unless the next turn is already queued (`in_turn`): that is refused
  as busy, because a goal-only undo cannot take back a turn the ticket will start. If the run existed but its
  checkpoint was dropped, the goal is left Paused and says the files cannot be restored. The web confirms ("Undo the
  default answer?") and shows a refusal under the row.
- Risks: the undo also undoes later runs; a pull request or pushed commits make the existing refusals apply. Auto-answering
  acts for the person, so it is opt-in, judge-given defaults only, checkpointed, and never shorter than an hour.
