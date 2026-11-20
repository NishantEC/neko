# Neko's words (2026-11-21)

Neko has its own vocabulary. Use these words in the UI, prompts, docs and code; do not borrow other products' terms (no "captain", "crew", "thread", "first mate").

| Concept | Neko's word | Notes |
| --- | --- | --- |
| The agent the person talks to, who leads everything | **Neko** (the head cat) | One per install. |
| A named helper that looks after one area (a workspace or a part of one) | **cat** | e.g. miso (Web), mochi (Billing), tofu (Infra). "New cat", "your cats", "miso's tasks". Each cat has a face (an illustrated cat), a name, an area, a model choice, a daily limit and a purpose. |
| A piece of work an agent does | **task** | The only word for it. Replaces session, ticket and thread in UI and docs. |
| Recurring or triggered work | **round** | A cat's patrol: "Every weekday at 9:00, miso's round: review open pull requests". Replaces routines, schedules, watches and automations in the UI. |
| The daily summary and a cat's current summary | **Briefing** | Home's top section; each cat keeps its own briefing file. |
| Something waiting for the person | **Needs you** | Has a question, a recommended default and a deadline. Actions: Use default, Answer, Later, Dismiss. |
| Something resolved | **Done** | Needs-you items that were answered, defaulted or dismissed move to Done. |
| A cat's files | **Rules**, **Task rules**, **Plan**, **Briefing** | Rules = the cat's own; Task rules = what every task it starts reads; Plan; Briefing. |
| Recorded proof of finished work | **Receipt** | Unchanged. |

Code follows the same words: the `Captain` types, `captains` table and `Request::Captains` from Phase 1 batches B1–B2 are renamed to `Cat`, `cats` and `Request::Cats` (with a migration), and `RULES.md / CREW.md / PLAN.md / DIGEST.md` become `RULES.md / TASK_RULES.md / PLAN.md / BRIEFING.md`.
