# Main window redesign: Neko, tickets, and the quick panel

Design source: Paper file "neko", artboards 10 Today, 11 Ticket open,
12 Quick panel, 13 Day one and problem states. Approved by the user.

## Product model

- Neko is the one agent the user talks to. Home is a conversation.
- Tickets are Neko's existing tasks. Status groups them into needs you
  (awaiting approval, ready for review, failed), working (queued, planning,
  building, reviewing) and done (completed, cancelled).
- Responsibilities create tickets on a schedule (existing MCP work).
- The quick panel is the pocket view; the main window is where work happens.
  One app, two windows, one daemon.

## Steps

1. Protocol and store: a durable, bounded Neko conversation stored beside the
   workbench snapshot, never inside its reserved task budget, plus ticket
   notes. Commands: SendMessage, AddTicketNote.
2. Daemon: a message runs one read-only Codex turn with a bounded summary of
   tickets and responsibilities. The reply may propose tickets; each becomes
   an ordinary queued task, so plans and approvals are unchanged. Ticket notes
   reach planner and builder prompts as user direction that cannot widen
   authority.
3. Main window: sidebar, Today (brief, ticket cards, chat, right rail),
   ticket panel with actions and notes, Tickets list, problem states. Existing
   tool and workspace editors stay reachable.
4. Quick panel: with an empty query, show tickets that need you and running
   agents above clipboard history.
5. Verification: unit tests for grouping, brief text, conversation bounds and
   reply parsing; full workspace tests; build; native window inspection.
