# Neko roadmap: from task runner to a second you

Goal: Neko keeps watch over the user's work (Sentry, Slack, GitHub, Linear and
anything else they connect), decides what matters the way they would, and does
the work, comparable to Hermes, OpenClaw and firstmate.

Out of scope for now, by the user's decision: reaching the user away from the
Mac (notifications, Slack/Telegram channels). Revisit after phase 5.

Every phase ends with automated tests, the end-to-end smoke test, one real
model run, a native window check, and an independent review. A phase is not
done until those pass.

## Phase 0: make what exists trustworthy (about 2 hours)

- Independent spec and quality review of the chat, ticket and main-window work
  (commits ca072cf..db0bc1e); fix every important finding.
- Real-model chat test: a question, and a request that opens a ticket.
- Real clicks and typing in the window: send a message, approve, add a note.
- Window glass matches the design; quick panel shows tickets that need you and
  running agents before you type.
- Done when: all of the above pass, then install the signed app.

## Phase 1: memory (about half a day)

Neko learns how the user works and remembers it.
- Durable memory: a profile of the user, notes per workspace, and a log of
  decisions (what they approved, rejected or corrected).
- After a chat turn or a finished ticket, a read-only turn proposes memory
  updates; small, bounded, and visible.
- A Memory page to read, edit and delete entries. Nothing hidden.
- Relevant memory is included in chat, planning and building prompts.
- Done when: a preference stated once changes a later plan without repeating it.

## Phase 2: tools in the chat (about half a day)

Neko can look things up while talking, not only open tickets.
- The chat turn gets the scoped MCP bridge with the tools granted to the
  selected workspace.
- Reading is allowed by grant; any action with side effects shows an approval
  card in the chat before it runs.
- Tool calls and receipts appear inline under the reply.
- Done when: "what's failing in Sentry for hme?" is answered from the real
  tool, and an action waits for approval.

## Phase 3: skills (about half a day)

- Discover skills from Codex, Claude and ~/.agents, plus a Neko-owned folder.
- Enable skills per workspace; enabled skills reach chat and worker runs.
- After a finished ticket, Neko can propose a new or improved skill; the user
  approves before it is saved.
- Browse and install from skills.sh, with its audit shown first.
- Done when: an enabled skill visibly changes how a ticket is planned.

## Phase 4: parallel workers with verification (about a day)

firstmate-style supervision.
- Several tickets run at once (bounded per workspace and overall).
- A ticket can be split into sub-tasks that run in parallel and are merged.
- Every result passes an independent verifier with evidence (tests run, diff
  in scope) before it reaches Needs you; failures retry or stop with a reason.
- Done when: three tickets run together, and a deliberately broken result is
  caught by the verifier.

## Phase 5: first run and import (about half a day)

The Paper onboarding (01 to 06).
- Import from Claude Code and Codex: MCP servers with their scopes, tokens into
  the Keychain, workspaces, skills and scheduled tasks.
- Tools grouped by scope, with New workspace and a Browse tab (MCP Registry and
  skills.sh).
- The first sweep ends in a real brief.
- Done when: a fresh install reaches a useful Today in under three minutes.

## Phase 6: agent profiles (later)

Separate agents with their own memory, tools and instructions (for example a
work Neko and a personal Neko), and cross-agent reading with permission.

## Deferred

Reaching the user away from the Mac.

