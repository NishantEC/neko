# Unified local import design

## Decision

Neko will automatically perform a bounded, read-only local discovery when the
first-run setup reaches **Import local setup**. The review surface will combine
Codex, Claude, and optional Paseo compatibility sources with local skills and
candidate workspaces. Nothing is activated, granted, executed, or copied until
the person explicitly selects items and presses **Import selected**.

This replaces the current fragmented journey: a manual Codex/Claude scan,
skills discovered elsewhere, and Paseo hidden behind a legacy environment
flag. It does not turn Neko into an importer of opaque agent histories or a
sync service.

## Product flow

1. Setup enters Import local setup and immediately starts a background scan.
   The UI says what is being read and remains usable while it runs.
2. A single results page groups every candidate by **source** (Codex, Claude,
   Paseo, local skill folders) and **scope** (Global or a detected workspace).
   It shows only metadata, validity warnings, credential-presence markers, and
   counts; secret values never leave daemon memory.
3. Each candidate starts unselected. The person may select individual MCP
   definitions, skills, workspaces, and paused schedules. A source-level
   select action is convenience only; it does not change the default.
4. The person explicitly confirms credential copying and separately trusts
   selected stdio MCP processes. Import remains available without either.
5. Applying the reviewed selection creates workspaces, registers MCP
   definitions and skill records, and saves schedules as paused drafts.
   Existing matching records are preserved rather than overwritten.
6. The result view states what was imported and what still needs action:
   MCPs need enablement and workspace grants, skills need review and per-
   workspace activation, and schedules need explicit enablement.

## Safety contract

- Discovery is read-only, bounded by existing file and candidate limits, and
  never launches a server, runs a skill, requests a network connection, or
  grants a tool.
- Connection secrets remain in the daemon's ephemeral discovery session and
  are redacted from the protocol preview, logs, settings, and UI.
- Imported MCP definitions stay disabled. A global definition is a reusable
  configuration, not global authority; grants remain workspace-specific.
- Imported skills remain disabled. Neko records their source path and content
  hash, then requires the existing review/activation flow per workspace.
- Paseo compatibility discovery is read-only and opt-in only when compatible
  data actually exists. Neko imports portable workspace/config metadata, not
  live processes, standing permissions, or opaque conversation state.
- An expired preview cannot be applied. Re-scan and explicit review are
  required after ten minutes or any daemon restart.

## Architecture

The existing `ImportCommand::Discover` / `Apply` session is retained as the
authority boundary. The protocol preview becomes an explicit typed ledger with
four independently selectable candidate kinds: workspace, MCP connection,
skill, and schedule. Every candidate has a stable reviewed ID, source,
optional workspace path, display metadata, and a problem field. Secret-bearing
configuration stays only in the daemon-side `Discovery` object.

`neko_core::setup_import` owns source adapters. Codex and Claude keep their
current adapters; Paseo is added as a compatibility adapter behind the same
read-only interface. `neko_core::skills` contributes discovered local skill
records to the same preview without moving their files. The daemon creates one
preview session and validates selections against it. The onboarding UI starts
one automatic discovery request when the Import step is entered, renders a
unified source/scope review, and applies only selected reviewed IDs.

This keeps Neko's existing daemon/client split intact: GPUI renders state and
sends typed commands; the daemon owns filesystem reads, SQLite, secrets,
validation, and durable application.

## Explicit non-goals

- No automatic activation of MCPs, skills, schedules, or agent permissions.
- No copying source directories, rewriting Codex/Claude/Paseo configuration,
  or modifying their credentials.
- No import of live agent processes, transcripts, worktrees, cloud accounts,
  or remote provider state in this milestone.
- No Zeron dependency, sync protocol, CRDT store, or multi-device feature.

## Zeron patterns to adapt, not copy

Neko may adapt the separation of a local engine from the UI, typed commands at
that boundary, source-specific adapters, explicit local-first operation, and
attention-oriented presentation. Its multi-device/sync architecture is outside
this milestone. The relevant public references are Zeron's README and
architecture document; this design does not treat either as a drop-in API or
an implementation dependency.

## Acceptance criteria

1. Reaching Import local setup begins exactly one automatic read-only scan per
   setup session; retry remains available after a failure.
2. A preview can show Codex, Claude, Paseo-compatible, workspace, MCP, skill,
   and schedule candidates together, grouped by source and scope.
3. No candidate is selected, enabled, granted, executed, or credential-copied
   by discovery alone.
4. Apply rejects IDs absent from the unexpired preview and preserves existing
   matching records.
5. Imported MCPs are disabled with zero grants; imported skills are disabled
   until review/activation; imported schedules are paused.
6. Unit, daemon integration, UI-state, and smoke tests cover redaction,
   automatic discovery once-per-session, selection validation, source/scope
   rendering, zero implicit activation, and a Paseo fixture.
7. A real native onboarding capture verifies the scan, review, and post-import
   states with keyboard navigation, focus visibility, reduced-motion-safe
   progress, and VoiceOver labels.

## Alternatives rejected

### Silent full migration

Fast, but it would copy credentials and activate local executables without
clear consent. It violates Neko's permission model.

### Keep separate import screens

Preserves existing code paths, but leaves the first-run experience incomplete
and makes users manually reconcile scopes across tools and skills.

### Rebuild around Zeron's sync/session engine

Useful as a future reference but disproportionate for a local configuration
migration. It expands the trust and data-model surface before the basic setup
flow works.
