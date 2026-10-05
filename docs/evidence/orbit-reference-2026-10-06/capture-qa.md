## Final reconciliation by the capture owner

The report below preserves the independent review of its **158-capture baseline** and original manifest hash. Its correction tables are historical findings, not outstanding caption work. The final manifest incorporates all listed title, action and visible-outcome corrections, including captures 64, 82 and 98.

Three follow-up screenshots were captured and visually checked by the parent agent:

- [159 — Pause notice](159-task-pause-preview.jpg) directly shows that Pause is disabled in the preview; no task was paused.
- [160 — Expanded test receipt](160-task-test-receipt-settled.jpg) visibly shows `pnpm test`, `exit 0`, four files and 42 tests passed. These are scripted results.
- [161 — Unobstructed failure receipt](161-task-deploy-failure-settled.jpg) visibly shows the deploy command, `exit 1`, the missing token explanation and Resolve action, without covering toasts.

These close the baseline Pause/test/error-card capture gaps. The independent reviewer did not inspect these three follow-up captures. Gesture, accessibility, timing-combination and breakpoint limits below remain. Paper captions and the final index follow the corrected manifest; the original 158 images are unchanged.

---

Review the correction tables before reusing the Orbit captions; use **85** for final task completion and **120** for the settled concurrent-agent inspector.

Status: read-only QA complete, 2026-10-06. Checked all **158 manifest entries against their saved DOM**, decoded all **158 JPEGs**, and visually inspected **31 captures**, including all 15 requested risk IDs. Only this report was written; no browser/Paper control or manifest changes.

Evidence root: `/Users/nish/Documents/neko/docs/evidence/orbit-reference-2026-10-06`. IDs below identify the matching JPG and TXT filenames. Manifest SHA-256 at start and finish: `3542ff698a6fbecd56d3fcd19c9426892995f9e68fc495ef85c3f9628943f806`.

## Remaining visible-result corrections

Parent already owns corrections **64, 82 and 98**, as confirmed in the latest instruction. They were checked in the initial pass and are excluded from the remaining-action table below; no repeat audit is needed.

| ID | Observed proof | Proposed caption |
|---|---|---|
| **66-parallel-design-copy** | JPG/TXT show Active 3: Atlas Working, Quill Needs you, Forge Running; Idle 3; Offline 1. No Blocked group or agent. | “Inspector drawer shows Atlas coordinating, Quill awaiting headline approval, Forge implementing, three idle agents and offline Ward.” |
| **73-test-receipt** | JPG shows a right-pointing chevron on `Ran pnpm test`, no expanded test card, and toast overlap. TXT test-row button lacks `[expanded]`; its hidden receipt text is still included. | “The pnpm test row remains collapsed and partly obscured by toasts; this frame does not show an expanded test receipt.” |
| **84-task-completed** | JPG shows `Approved by you`, Quill Done and scripted preview output. TXT still says task `Running`, although plan is `6 of 6`. Completed arrives in 85. | “Headline approval clears and Quill becomes Done. The DOM has 6/6 steps but task status is still Running; final Completed state is captured in 85.” |
| **85-task-pause-notice** | JPG/TXT show Completed, 6/6 steps, disabled Pause and “Build landing page is complete” toast. No Pause notice exists in either artifact. | “Task reaches Completed with all six steps finished; Pause is disabled and the completion toast is visible.” Change action to “Observe final completion after approval; Pause is disabled.” |
| **94-scene-stream** | JPG/TXT end with the new human message “What did the research show?” Existing Sage replies have earlier timestamps; the new Sage response has not appeared. | “Streaming scene opens Chat and appends the research question; the new Sage reply is not visible yet.” |
| **102-scene-multi** | JPG shows all six online agents Idle, TASK-142 Queued, empty execution and 0 tokens. | “Initial reset frame of the Several agents working scene: agents are still idle and execution is empty.” |
| **103-multi-inspector** | JPG/TXT show Active 1, Scout Thinking / Getting started; Idle 5; Ward Offline. | “Inspector opens during scene startup with only Scout Thinking. The settled six-active-agent roster is visible in 120.” |
| **106-scene-palette-start** | JPG/TXT show empty search, New Task selected and 26 root results. Filtering has not appeared. | “Command-palette scene opens the unfiltered root with 26 results; the Agents subpage is captured in 107.” |
| **119-keyboard-terminal** | JPG has no visible terminal and TXT has no Terminal region. The terminal is present in the following JPG/TXT, 120. This does not establish shortcut failure. | “Frame immediately after recorded Command-J input: Settings is visible without the terminal yet. The terminal is visible in 120.” |

## Tighten scope or attribution

| ID | Observed proof / limit | Proposed caption or action |
|---|---|---|
| **08-command-filtered** | JPG confirms zero results for `forge` on the root page. The source map and 107 establish a separate agent subpage. | “Root command search for forge returns zero results; agent choices are available under Open Agent.” |
| **13-task-planning** | JPG/TXT already show Completed, three completed steps and disabled Pause. Current observed caption is accurate; “first execution tick” action is not. | Action: “Observe assigned Scout task after completion.” Preserve ID for existing references. |
| **36-runs** | JPG shows four Completed rows. Source map identifies three historical literal rows and a current #218 row using mutable demo state, including the `TASKS[0]` coupling risk. | “Runs lists current demo run #218 and three seeded historical runs. Displayed statuses and metrics are simulated; #218 is not a fixed historical literal.” |
| **61-code-copy** | JPG/TXT visibly say “Copied to clipboard.” No independent clipboard readback was performed. | “Copy code displays a Copied to clipboard success toast; clipboard contents were not independently verified.” |
| **74-deploy-failure** | Expanded failure card begins at the viewport bottom; sticky toasts cover part and the remaining content is clipped. | “Deploy-failure detail is expanded but partially clipped/covered by toasts. This frame is not a complete unobstructed error-card reference.” |
| **95-scene-stream-progress** | JPG shows Sage's complete three-point response; no thinking label or in-flight text is visible. | “Sage's scripted reply is rendered with three research findings and a collapsed analysis disclosure.” |
| **101-scene-completion-result** | JPG shows checked `Generated 3 variants`, Pixel Idle, task Queued. No completion toast or visible variant cards. | “Pixel's generated-variants row is complete and collapsed; TASK-142 remains Queued. No completion toast is visible in this frame.” |
| **120-keyboard-inspector** | JPG shows an open right inspector drawer, dimmed Settings, six active agents and an open terminal. | “After recorded Command-I input, the inspector drawer is open with six active agents; Terminal is also visible.” |
| **125-tablet-sidebar** | JPG shows no sidebar at 768 px; 126 shows the 52 px icon rail. “Drawer” implies the phone-specific treatment. | “At 768 px the sidebar is hidden and code spans the content width; toggling again exposes the compact icon rail in 126.” |
| **146-reset-world** | JPG/TXT show queued empty main task, idle agents, retained task tabs and an Opus selection toast. Conversation and autonomy are not displayed. Source map says reset preserves them; this image alone does not prove that. | “Idle scene resets the main task and agents while earlier tabs remain. Opus is selected with a model-change toast; conversation/preference retention is source-derived.” |
| **153-middle-close-tab** | TXT removes the Projects tab. Retained Projects content is source behavior, not shown by a reopen in this capture. | “Projects tab is absent after the recorded middle-click. Source retains its content; this frame does not demonstrate reopening it.” |
| **156-reload-reset** | JPG shows five seed tabs, seed task rows, Opus, idle agents and empty Recent activity. Chat is not open. Model was already Opus in 155. | “Reload restores the seed Home/task list and five tabs with empty Recent activity. Transcript reset and reset of a nondefault model are not demonstrated by this frame.” |

## Integrity and verified exclusions

All IDs 1–158 are present and unique, all referenced JPG/TXT files exist, and every TXT is nonempty. All JPEGs fully decode and have nonuniform pixels; no wholly blank/corrupt image was detected. This is not a visual certification of every content region.

The blank historical task regions in **27, 131, 132, 133** are visually confirmed with intact shell/breadcrumbs. They are captured prototype gaps, not missing screenshots. **155** correctly shows the intentional No open tabs screen and command hint. **20** really shows Forge thinking; **70** really shows three variant cards; **126** really shows the compact icon rail. Do not replace these with source-expected states.

Visually inspected via `view_image`: **8, 13, 20, 27, 36, 61, 64, 66, 70, 73, 74, 82, 84, 85, 94, 95, 98, 101, 102, 103, 106, 119, 120, 125, 126, 131, 132, 133, 146, 155, 156**. Other entries received DOM/caption comparison and image integrity checks, not direct visual review. DOM snapshots include hidden disclosures, collapsed drawers and pre-animation attributes; screenshot pixels decide visibility.

## Source inventory and remaining coverage limits

Read `/tmp/neko-orbit-demo-map.md` in full. Its “source inspection complete” status describes the bounded public-source inventory; its rendered behavior and transitions are explicitly runtime-unverified. The capture manifest broadly represents all ten sidebar destinations, shared Chat, both file viewers, main/created/historical/empty task surfaces, seven agent cards, menu families, four palette subpages, and all twelve scene triggers. That is broad surface coverage, not exhaustive transition testing.

1. **Timing and work:** 94/95 bracket question → settled reply, not word-by-word streaming. 98 misses live handoff/analysis progression; 102/103 precede the multi-agent steady state (120 supplies that reference). 105 proves queuing, not queued → running → completed. Secondary Blocked-scene grant/recovery/status, every routing family's completion and every evidence disclosure are not fully captured. Source-only Pause behavior remains unproved by 85.
2. **Gestures and keyboard:** No complete evidence for all root commands, nested Backspace, arrow wrapping, Escape precedence, scrim/outside-click dismissal, focus return/trapping, Enter/Space activation, empty Home send, all hero faces/hover behavior, or toast dismissal/overflow. Action strings record intended/performed inputs but are not interaction recordings. Screenshots alone cannot prove animation quality or reduced-motion suppression.
3. **Layout:** Actual JPG sizes are 1280×720 (17), 959×1128 (108), 1440×900 (3), 768×1024 (3), 390×844 (4), 863×1017 (4), 864×1017 (19). These sample widths do not establish both sides of every 1180/1000/860/620 breakpoint, all drawer combinations, clipping/focus behavior, or screen-reader accessibility. Example clipping is present in 74.
4. **Continuity and source-only behavior:** 141 records a reopened chat draft, but 146/156 do not visually prove every preserved/reset field. Do not promote source findings about retained DOM, empty handlers, all display-only rows, the #218/new-task coupling, or preference persistence into individually tested outcomes. Historical blank details are the exception: all four are directly captured and visually checked here.
5. **Simulation boundary:** Roles, permissions, tools, command output, tests, source citations, deploy URLs and model/autonomy controls are demo choreography/display data per the source map. They establish no real execution, credential grant, provider/model behavior, offline delivery, deployment, or Neko implementation coverage. 61 establishes a displayed copy-success result, not clipboard readback.

Baseline follow-up requested caption reconciliation. Completed by the parent as recorded at the top of this report; original capture IDs are preserved.
