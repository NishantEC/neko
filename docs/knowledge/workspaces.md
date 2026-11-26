# Workspaces (2026-11-26)

A workspace is a folder the person added, with a name. Tasks are made in one and listed under it in the sidebar.

## How other tools do it, and where Neko differs

In Paseo a project is a repository and a workspace is one working folder inside it, usually a worktree. In T3 Code projects are
folders and conversations sit under their project. Neko is for any work on the computer, so a task with no folder stays a
first-class thing: it goes in **General**.

## The rules

1. A workspace is a folder the person added, with a name. The name starts as the folder's name and can be changed (one line, at
   most 80 characters, not empty). Nothing is scanned or found by itself.
2. One workspace per folder. Folders are compared with links resolved (`check_folder`: absolute, there, a folder, readable).
   Adding a folder again answers with its workspace (`existed`); adding a removed one's folder brings it back with its tasks. A
   folder inside another workspace's folder is a workspace of its own.
3. **General** is built in (id `default`): no folder, each task gets a scratch folder of its own. It cannot be renamed or removed.
4. A task belongs to exactly one workspace, chosen when it is made, never changed. In a git workspace it works in its own worktree
   on `neko/<id>`; in a plain folder it works there directly; in General in a scratch folder.
5. **Removing** a workspace takes it off the list only. Nothing on disk is touched. Its tasks stay readable under "Earlier, removed
   workspace", and no new task can be made in it. The app asks first, saying exactly that.
6. A workspace whose folder is gone shows "Folder not found" with Locate… and Remove. Its tasks stay readable. A new task in it is
   refused with a plain line. Locate… moves the workspace (and its tasks' folder) to where the folder is now and runs
   `git worktree repair` for their worktrees.
7. Paths are stored absolute and shown with `~`.

## The tidy rule

The earlier project left 72 worktrees from 84 tasks. Now, after every run (`workplace::tidy`):

- A worktree whose branch has no commits the base lacks, no uncommitted changes (`git status --porcelain`, ignored files do not
  count) and is still on its own branch is removed with `git worktree remove` (no `--force`), and its branch is deleted only if it
  still points where it began (`git update-ref -d <ref> <base>`).
- An empty scratch folder is removed.
- Anything with work in it is kept. When git cannot say, nothing is deleted.
- A follow-up makes the folder again (`workplace::reopen`): the worktree on its branch, or a new branch from where it began.

Measured (`crates/neko-daemon/tests/workspaces.rs`): after 20 finished tasks (10 in a repository, 10 in General), 0 worktree
folders, 0 scratch folders, 0 `neko/` branches, and git lists only the person's own checkout. A task that edited files keeps its
worktree.

**Remove task** (the task's menu in the band) asks first, and is off while the agent works. It deletes the task's record. Its
worktree goes when nothing in it is uncommitted; a branch with commits keeps them in the repository. Uncommitted changes, or files
in a scratch folder, refuse the removal in plain words with Reveal in Finder: nothing is deleted.

## Screens

- **Sidebar:** New task, Home, then one section per workspace (General first, then the person's order, removed ones last while they
  have tasks). A header folds the section (remembered in the prefs, `folded-workspaces`) and shows a quiet count of what needs
  the person there (Needs you's items; not what is working). New task in this workspace and the menu (Rename…, Reveal in Finder, Locate…, Move up, Move down,
  Remove…) show on hover or focus. Eight tasks, then "Show N more". "Add workspace…" at the foot opens the folder panel.
- **Keys:** the sections and their rows are one list (`role list`, `focus_group`): one Tab stop, Up and Down move through headers
  and rows, typing a name goes to it. On a header, Left folds, Right opens, Return toggles, ⌥⌘↑ and ⌥⌘↓ move it. A header's own
  buttons are Tab stops of their own.
- **Composer:** a pop-up menu in its footer (General, the workspaces, "Add folder…"), remembered (`last-workspace`). ⌘N starts a
  task in the workspace of the task in view.
- **Task band:** the workspace's name, then the folder and branch. **Window title:** "<task title> — <workspace name>".
- **Settings ▸ Workspaces:** each workspace with its folder and task count; Rename…, Reveal in Finder (or Locate…), Remove….

## Reordering

By keyboard (⌥⌘↑ / ⌥⌘↓ on a header) and the menu's Move up / Move down. Dragging is not built yet.

## Not here yet, on purpose

Per-workspace rules and instructions, per-workspace permissions, cats, connections, a dashboard on Home, dragging to reorder.
