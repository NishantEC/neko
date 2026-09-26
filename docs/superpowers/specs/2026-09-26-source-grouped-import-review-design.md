# Source-grouped import review

## Goal

Make the setup import screen reviewable when a Mac has many local Codex,
Claude, Paseo, agent and workspace definitions. A person must be able to see
where each import comes from without reading a repeated `source:` field on
every row.

## Layout

The import review renders source sections in lexical order. A section title is
the candidate's existing provenance label: for example `Agents`, `Claude`,
`Claude + Codex`, `Codex`, `Paseo`, or `Workspace`. Each section contains its
own candidates in a stable secondary order: global scope first, then workspace
scope, then candidate type and name.

Rows show the candidate type and name, scope, optional credential marker and
unavailable reason. The redundant per-row provenance string is removed because
the section establishes it. Empty sections do not render.

## Duplicates and source conflicts

Import discovery already collapses exact duplicate skills and MCP definitions.
When a definition is found in more than one source, it appears once under a
combined source section such as `Claude + Codex`; it is never copied into both
source sections. Definitions that differ in configuration or credentials remain
separate so import cannot discard information.

## Behavior and safety

Grouping is presentation-only. It cannot alter candidate identity, selection,
workspace eligibility, import order, consent, disabled-by-default MCP/skill
state, or paused schedules. Existing unavailable/error messages stay directly
below their item. Refresh rebuilds the source groups from the new preview.

## Verification

Unit tests cover lexical grouping, combined-source placement, stable candidate
order and the existing selection behavior. A live first-run check uses the
real local discovery preview and confirms one source heading per group, MCP
rows labelled `MCP server`, and no repeated global skill rows.
