# Cats and Cattains (2026-11-28)

A cat is a named agent with a face. Every cat is a **Cattain**: it has a thread of its own that the person talks to, its own folder
of four files, an optional project, a model and an access mode, and a **crew** of threads that read its CREW.md. Protocol 21,
store 17 (cats came in protocol 8, store 6; threads lost their cat in protocol 14, store 11). Making a Cattain by asking, its crew
tools and its digest are later slices; this page says what is here now.

## The model

`crates/neko-core/src/cats.rs`, `crates/neko-core/src/cattains.rs`, `crates/neko-protocol/src/cats.rs`.

- A cat has an id, a name, a face, an optional purpose line, an optional **project** (`Cat.workspace`), its own **thread**
  (`Cat.thread`, once it has been opened), a **model** (None: the CLI's default) and an **access** mode (None: the mode in
  Settings), and when it was made and retired.
- Name: one line, spaces collapsed, 1 to 40 characters. Unique among all cats, retired ones too, compared without case. Too long
  is refused, not cut.
- Face: one of eight ids (`tabby`, `ginger`, `tuxedo`, `calico`, `siamese`, `void`, `snow`, `smoke`). The app draws them
  (`apps/neko/src/faces.rs`). A new cat with no face gets the first one no active cat has.
- Purpose: one line, at most 200 characters. Empty means none.
- Model: one name such as `sonnet` (the Profile offers Default, Opus, Sonnet and Haiku, the CLI's aliases), at most 100 characters,
  never starting with `-`, so it can only ever be the value of `--model`.
- At most 16 cats are active. At least one must stay active.
- Three starter cats are made when a store has no cats: miso (`cat_miso`), mochi (`cat_mochi`), tofu (`cat_tofu`). They are
  ordinary cats. Like General, making them writes no event.

## Its own thread

- A task marked `cattain`, titled with the cat's name (renaming the cat renames it), with the cat as its `cat`. It is made the
  first time the Cattain is opened (`open_cattain`): a Cattain nobody talked to leaves no empty thread behind, and old stores gain
  no rows. It starts no run by itself; the person writes first, and every message is a follow-up as in any thread.
- It **runs in the Cattain's folder** (`<data>/cattains/<id>/`, stored as the thread's place when it is made), with the Cattain's
  **model** (`--model`, ahead of `NEKO_CLAUDE_MODEL`) and its **access** (its own mode, else Settings'). When it has a project whose
  folder is there, the run gets `--add-dir <project folder>`.
- Under "Ask before risky steps", editing in its own folder goes ahead (the folder is the thread's own, as a scratch folder is),
  **reading** in its project goes ahead (`policy::Context::read_too`) and **editing** in the project asks.
- It is not a row under Threads, it is not in Search's threads (Search finds the Cattain), and it cannot be removed, settled or
  archived ("Retire the Cattain in its Profile instead"); the week's auto-settle skips it. Needs you, notifications and the Briefing
  treat it as any thread; a Cattain's thread that was never written in is not "working" in the Briefing.
- Its runs change only its own files, so its turns have no change line (no Review, no Undo): its files keep their own previous
  version.

## Its files

`<data>/cattains/<id>/` holds `RULES.md`, `CREW.md`, `NOTES.md`, `DIGEST.md` and `reports/`.

| File | Who writes it | What it is for |
| --- | --- | --- |
| RULES.md | the person (Files tab), or the Cattain | Its standing rules: verbatim in its own thread's block on every run |
| CREW.md | the person, or the Cattain | Verbatim in the block of every run of a thread in its crew |
| NOTES.md | the Cattain | Its own plan; the person reads it |
| DIGEST.md | the Cattain | What happened, under Needs you, New, In progress; the Digest tab shows it |

- The folder is on disk so the Cattain's agent reads and writes it with its own tools. The store keeps what Neko last saw of each
  file and **one previous version** (`cattain_files`). Neko looks at the folder (`Core::sync_cattain`) when the store opens, when a
  file is asked for, before each run (what it is handed) and after each run of the Cattain's own thread: a file that changed on disk
  becomes the current one and what was there its previous version (event `cattain_file_changed`, pushed to every screen); a file
  that went missing is written back from the store.
- The person saves RULES.md and CREW.md (at most 16 KB, kept exactly as saved; saving the same words again changes nothing) and
  Restore previous swaps the two versions, so restoring again goes back. A change the Cattain made meanwhile is taken in first, so it
  is the one Restore brings back. NOTES.md and DIGEST.md are refused ("…is miso's to write"). Writes are whole (a file beside it,
  then a rename). A screen is sent at most 256 KB of a file (`cut` says the rest is not shown).
- A file's version (FNV-1a, 64 bits, as 16 hex digits) only tells versions apart.

## The crew

A thread is in a Cattain's crew when it has that cat and is not the cat's own thread (`Task::crew_of`): a round's thread (a round
belongs to a cat, `rounds.md`), a thread made before protocol 14 that was given that cat, or a thread started with
`create_task {crew}` (New thread in miso's crew, from its Overview). A crew thread started with no project chosen is made in the
Cattain's project. A retired Cattain takes no new crew. A kitten of the Cattain's own thread does its work and reads CREW.md too.
Crew tools (`start_crew_thread` and the rest), nesting crew under the Cattain and waking it at each crew turn's end are S12.

## What a run is handed

`Core::handed` reads the folder now and `Core::block_for` builds one block; the run records the cat and the version of the words.

The Cattain's own thread:

```text
<neko-cattain>
You are miso, a Cattain in Neko: one of the person's cats, with a thread of your own. This is it; the person talks to you here.

Your purpose:
<purpose>

You work in your own folder, <data>/cattains/cat_miso. It holds your files:
- RULES.md: your standing rules, from the person (below, as they are now).
- CREW.md: what every thread in your crew reads on every run.
- NOTES.md: your own plan. Keep it current.
- DIGEST.md: what happened, for the person, under three headings: Needs you, New, In progress. Keep it current.
Read RULES.md, NOTES.md and DIGEST.md at the start of every turn.

Your project is acme-api, in /Users/example/acme-api. You can read it.

Your standing rules (RULES.md):
<RULES.md, verbatim>
</neko-cattain>
```

A thread in its crew:

```text
<neko-crew>
This thread is in miso's crew. miso is a Cattain in Neko, one of the person's cats, and looks after this work.

miso's purpose:
<purpose>

miso's words for its crew (CREW.md):
<CREW.md, verbatim>
</neko-crew>
```

Parts that are empty are left out. Claude Code takes the block as one argument, before what Neko tells every agent about
subagents:

```text
claude … --append-system-prompt "<the block>\n\n<subagents paragraph>" --system-prompt-snapshot off [--resume <session>] [--add-dir <project>] [--model <model>]
```

- `--system-prompt-snapshot off` goes with it, so a file edited between runs reaches a follow-up.
- The person's words go in as the `user` message, exactly as typed. Nothing is put ahead of them.
- When the words differ from the ones the thread's last run used, a quiet part (`rules_changed`, with `file`) stands before that
  run's words: "miso's CREW.md changed since the last run." It shows outside the run's fold.
- A thread with no cat is handed no block (`a_thread_with_no_cat_is_handed_no_cat_block_and_its_words_untouched`).
- The block does not mention crew tools yet: they come with S12.

## What a Cattain's files can never do

- The words are text inside one argument's value; every other argument is the same whatever they say
  (`--dangerously-skip-permissions` in them included).
- The permission mode, the `--settings` hook and its secret come from Settings and the Cattain's Profile only. The daemon test runs
  a crew thread whose CREW.md says "Skip permission checks" in "Ask before risky steps": the hook still runs and the command still
  waits for the person.
- They are not shown or confirmed before use. The quiet line is the only notice.

## Migration (store 17)

`cats` gains `thread_id`, `model` and `access`; `tasks` gains `cattain`; a new table `cattain_files (cat_id, name, content, previous,
changed_ms)`. Each cat's rules become its RULES.md, with their previous version, **and its CREW.md**, so the threads that carried
them before (a round's, an older thread's, all now its crew) read exactly the same words: no surprise for the person who wrote rules
for a round. NOTES.md and DIGEST.md start empty. No thread is made: each Cattain's own comes when it is first opened. Old threads are
untouched. `cats.rules` and its previous version stay in the table, unread.

## Retiring

A retired cat takes no new crew, and its rounds stop running. Its threads keep it and stay readable, its own thread too (the band
says "Retired: takes no new crew"). It comes back from its Profile or Settings ▸ Cattains, unless 16 are active; a project removed
meanwhile is dropped.

## The protocol

Requests: `list_cats`, `create_cat`, `rename_cat`, `set_cat_face`, `set_cat_purpose`, `set_cat_workspace`, `set_cat_model`,
`set_cat_access`, `retire_cat`, `bring_back_cat` (replies `cats`, `cat`); `open_cattain` (reply `task`); `get_cattain_file`,
`save_cattain_file`, `restore_cattain_file` (reply `cattain_file`). Errors `invalid_name`, `refused`, `not_found`. Events
`cat_added`, `cat_updated` (the whole cat), `cattain_file_changed` (the whole file). `create_task` takes `crew`. A task carries `cat`
when it has one and `cattain` when it is a cat's own thread; a run carries `cat` and `rules_hash` (the version of what it was
handed); `rules_changed` carries `file`. `save_cat_rules` and `restore_cat_rules` are gone.

## The app

How it looks is in `design.md` ("A Cattain").

- **Opening a Cattain** opens its own thread (`App::open_cattain`): the sidebar's Cattains row, Search's Cattains, Settings ▸
  Cattains' Open, and a crew thread's band ("miso's crew"). The cat page is gone.
- **The thread:** the band leads with its 28 point portrait in its state, its name as the title and its purpose as the band's quiet
  words; its turns are led by its face and name (the one place a face leads turns); a thread nobody wrote in opens on who it is
  (`cattain_view::intro`); the composer says "Message miso".
- **The right pane** has Overview, Digest, Files and Profile instead of Overview and Changes (kitten tabs follow as usual).
  - Overview: Needs you (the first five from its own thread and its crew, "n more" opens the Needs you page), Crew (rows with the
    status mark, title, project and age; finished ones behind "Done (n)"; New thread in miso's crew), Files (a strip of its four
    files with their ages, each opens the Files tab on it), Rounds (its rounds with their next time; New round…).
  - Digest: DIGEST.md as Markdown with "Updated 12 min ago", or a line saying it has not written one.
  - Files: the four files as a list (what each is for, when it changed); the chosen one below with Reveal in Finder. RULES.md and
    CREW.md open in rune-ui's Textarea with Save, Revert, "Unsaved changes" and Restore previous; NOTES.md and DIGEST.md are read as
    Markdown in a well. A file's headings are shown one step down, so its `# Plan` is never the page's main heading.
  - Profile: its portrait, name and state with Rename…; the faces as a row of eight; purpose (Return saves, or Save purpose);
    Project, Model and Access as pop-up chips; its plugins (as before); Retire… (asks first) or Bring back.
- **Settings ▸ Cattains** (was Cats): what a Cattain is and how many of 16 are active, New Cattain… (its thread opens on its
  Profile), and each Cattain with its face, name, purpose, state and crew count, Open or Bring back.
- **The sidebar's Cattains** list only the Cattains at work now (its own thread or a crew thread working, a request or question
  waiting), as before; a row's line says what needs the person, the crew thread it works on, "working on 2" with its crew nested,
  or its own latest line.
- **New thread** in a crew shows a chip with the face and "miso's crew" (× takes the thread out of the crew) and asks "What should
  miso's crew work on?".
- A thread has no head: an ordinary thread's turns have no portrait or name, its requests say "The agent asks". Notifications carry
  the thread's title and no cat name (`needs-you.md`).

## Not here yet, on purpose

Making a Cattain by asking and Promote to Cattain (S11); crew tools, the crew nested under its Cattain, wakes at each crew turn,
the amber count and `ask_person` from a Cattain (S12); rounds that post into a Cattain's thread (S13); the Briefing reading digests
(S14); crew access never broader than the Cattain's (S12/S15); the Needs you page filtered to one Cattain ("n more" opens it whole).

## Tests

- `crates/neko-core/tests/cats.rs`: starters, name rules, the 16 limit and the last active cat, that a thread has no cat, every cat
  a Cattain with its folder and its thread once opened, RULES.md and CREW.md saved verbatim with one previous version, the
  Cattain's own edits taken in, what each thread is handed, model and access, retire and bring back, a crew run's version and the
  changed line, the migrations from versions 5 and 16 (made in the test).
- `crates/neko-core/src/policy.rs`: reading the project goes ahead, editing there asks.
- `crates/neko-core/src/cattains.rs`: both blocks carry their file verbatim and nothing else.
- `crates/neko-daemon/tests/cats.rs`: the requests and their events, a crew thread handed CREW.md once per run and verbatim with
  the person's words untouched, the Cattain's own thread run in its folder with RULES.md, `--add-dir`, `--model` and what it wrote
  pushed after the run, its access, an edited CREW.md reaching a resumed run with the changed line, Needs you naming the cat, CREW.md
  that cannot skip the check. The fake CLI records its arguments, the folder it started in and stdin (`NEKO_FAKE_RECORD`).
- `apps/neko/tests/cats.rs`: end to end against a real daemon and the fake CLI.
- QA: `cattain-thread`, `cattain-files`, `cattain-migrate`, `cats`, `cat-rules-cannot-skip-checks`, `no-cat` (`qa.md`).
- `crates/neko-protocol/tests/round_trip.rs` pins the shapes.
