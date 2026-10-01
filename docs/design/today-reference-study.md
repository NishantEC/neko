# Today screen reference study

The Neko implementation uses these apps as design references, not as copied UI or code. Neko keeps its own neutral palette, sidebar, approval boundaries, and workspace model.

| Reference | Useful pattern | Neko adaptation |
| --- | --- | --- |
| [OpenChamber](https://github.com/openchamber/openchamber) | Conversation stays the primary reading surface; tool activity is compact and inspectable; a secondary pane holds work details. | Keep the transcript readable, group tool calls behind a disclosure, and open a Work inspector only on request. |
| [Superset](https://github.com/superset-sh/superset) | Work and review state are visible without turning every event into a large chat card. | Put actionable and in-progress tasks in the Work inspector, with a route to the full board. |
| [AionUi](https://github.com/iOfficeAI/AionUi) | A calm first-use canvas leads to a single obvious starting action. | Replace the three oversized status cards with a short greeting, quiet inline counts, and one action. |

## Current Today hierarchy

1. The conversation and composer own the available width by default.
2. Tool calls remain in the turn they belong to, collapsed unless details are needed; pending approvals stay visible.
3. Work is a contextual inspector, not a permanent empty column. Its counts and rows follow the selected workspace; All workspaces combines them.
4. The empty state explains what Neko can do and gives one starting action. Status is supporting information, not the hero.

The inspector must not imply that tasks in another workspace belong to the current one. Closing it must not change the current conversation or draft. No permission or tool-grant behavior changes in this pass.
