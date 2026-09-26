# Folder workspaces and import cleanup

Status: approved direction from the existing Paper `neko` onboarding artboards
(`03c`, `03e`, and `04`) and the user's request to make workspace selection
independent of Git. Implementation and destructive cleanup remain separate.

## User outcome

A workspace is an existing local directory the user chooses. Git is an
optional capability of that directory, not a prerequisite for adding it.
During import, actual long-lived project folders are easy to review; generated
checkouts, one-off Codex output folders, test fixtures, and other historical
paths do not crowd the primary choices. Every existing directory remains
selectable through an explicit secondary list or folder picker.

## Import review

The existing Paper `03e · Choose workspaces individually` layout remains the
visual source. Its primary list contains likely project folders; an initially
collapsed `Other folders` section contains generated and transient locations,
including managed worktrees and dated scratch directories. Rows explain why a
folder is in that section. A missing directory is not selectable, and is
reported separately rather than labeled `Needs attention` among usable paths.
Classification only affects presentation; it never deletes or modifies a
source directory. Selection remains individual, with no automatic selection
of associated skills, MCP connections, or schedules.

An existing non-Git directory is selectable. Import canonicalizes its path and
keeps source and workspace scope. The same physical skill file discovered from
both a global root and a selected folder appears once in review, with its
global definition and provenance preserved. Distinct definitions are not
silently merged.

## Runtime boundary

Neko can use any saved folder for conversation, read-only context, scoped
skills, and explicitly granted MCP tools. The isolated code-fix flow still
requires a Git worktree; that requirement is checked and explained when the
user requests that flow. Neko does not initialize Git automatically or imply
that a folder-based workspace gives cross-workspace filesystem isolation.
Existing saved Git workspaces and task history retain their IDs and paths.

## Paper source of truth

Update the existing `03c`, `03e`, and `04` artboards to match the behavior
above. Keep the six-step onboarding sequence and the distinct review states.
Inventory the board before deletion; remove only artboards that are clearly
superseded, never current interaction states or unrelated product screens.
Record which nodes were changed or removed so cleanup is reviewable.

## Generated folders on disk

This is a separate, final phase. First produce an exact path-by-path inventory
of candidate generated folders, size, contents, Git status, and any active
process references. Do not infer deletability from import classification.
Obtain confirmation of the exact targets before removal, use recoverable Trash
movement where practical, and verify each target after the move. Never delete
named projects, active worktrees, or user-created output merely because they
appear in a dated or temporary parent folder.

## Verification

Tests cover non-Git workspace selection, Git-only task gating, stable existing
workspace data, one physical skill in global-plus-home discovery, generated
folder classification and opt-in selection, and missing-path behavior. Verify
the native onboarding with a real discovery preview against a fresh isolated
data directory. Reinstall only after implementation and UI verification; the
user's standing instruction requires clearing Neko's previous onboarding
state during each reinstall.
