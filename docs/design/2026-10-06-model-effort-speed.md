# Model, effort and speed in Neko

Research completed 2026-10-06. This is a proposed implementation, not shipped
runtime behavior. Neko source inspected at `d0bc88e` on local `main`.

## Direction

Replace the Design lab task-team control with a model/effort control. Keep the
pearl, the inline composer and a separate Plugins control. Task membership and
progress belong with the conversation's task header.

The supplied references show two views of the same selection: an effort-first
panel with a model link and lightning control, and a model-first list. Support
both within one anchored native popover rather than making users manage a large
list of model/effort/speed combinations.

The runtime adapter should discover available choices. Neko's supervisor may
choose among those choices when the user selects **Neko decides**. Those are
separate responsibilities: a model can judge a task, but it should not invent
runtime capabilities, account access or remaining quota.

## Source findings

| Reference | Observed implementation | Useful Neko behavior |
| --- | --- | --- |
| [Zeron compact control](https://github.com/zeronsh/zeron/blob/c8eb7524e06d8483c2c9054a8180b61e19256a74/crates/ui/src/pickers/compact.rs#L586) | Slider stops come from the selected model's reasoning ladder or an effort-shaped option. Selecting a stop selects a discrete value. The panel links to a model list and separately resolves Fast from model options. | One compact control, a discrete native slider, visible current value, keyboard access and a model-list page. |
| [Zeron Codex adapter](https://github.com/zeronsh/zeron/blob/c8eb7524e06d8483c2c9054a8180b61e19256a74/crates/harness/src/codex/mod.rs#L368) | Reads `supportedReasoningEfforts`, `serviceTiers`, `additionalSpeedTiers` and defaults from the catalog. Sends model, effort and service tier separately. Zeron also has curated fallbacks and provider-specific effort conversions. | Discover capabilities through the existing catalog; preserve wire IDs and labels. Report any effective conversion rather than presenting a requested value as confirmed. |
| [Paseo speed control](https://github.com/getpaseo/paseo/blob/4ed13fadb63a2d729f1f99eb5512253bcedeb585/packages/server/src/server/agent/providers/codex-feature-definitions.ts#L12) | Builds a Speed select from advertised tiers, adds Normal, and uses a lightning icon on desktop. No tiers means no speed control. | A toggle suffices for two states; use a small menu when there are more. Do not discard a third tier to fit a toggle. |
| [Paseo turn configuration](https://github.com/getpaseo/paseo/blob/4ed13fadb63a2d729f1f99eb5512253bcedeb585/packages/server/src/server/agent/providers/codex-app-server-agent.ts#L4187) | Sends separate `model`, `effort` and `serviceTier` values. Model changes reconcile speed support. Changing effort while a turn runs returns an applies-next-turn notice. Unsupported tiers are rejected. | Per-conversation settings, explicit next-turn behavior, validation after model changes. |
| [Paseo plugin contract](https://github.com/getpaseo/paseo/blob/4ed13fadb63a2d729f1f99eb5512253bcedeb585/public-docs/plugins/providers.md#L102) | Providers return catalogs before session creation and publish effective session configuration afterward. Session settings can add toggle/select controls at runtime. | One capability-driven interface that can accommodate future adapters. Separate advertised, requested and effective values. |
| [Firstmate dispatch profiles](https://github.com/kunchenguid/firstmate/blob/main/docs/configuration.md#crew-dispatch-profiles-configcrew-dispatchjson) | Its supervisor chooses a rule at intake, resolves profiles against quota information, and passes concrete harness/model/effort flags. Its scripts validate rather than interpret task intent. | Task-aware automatic selection with deterministic validation and persisted choices. This source does not establish universal mid-run failover. |

Zeron's current compact-control source matched the existing October 5 local
reference byte-for-byte. Paseo's older local reference was from August 23, so
the findings above use freshly fetched October 5 `main` source instead.

Firstmate is a supervisor distribution over other harnesses, not a composer
implementation. Its [Pi supervision configuration](https://github.com/kunchenguid/firstmate/blob/main/docs/configuration.md#pi-supervision-branch-model-and-effort-configsupervision-branch-model-configsupervision-branch-effort)
also asks Pi for eligible models and effort levels rather than owning a second
catalog, and lets background supervision use a cheaper model than the main chat.

## Fast and effort are distinct

Effort generally sets reasoning depth; speed selects a processing tier. High
effort and Fast can coexist when a runtime supports that combination. Fast can
consume allowance or money more quickly. Show provider-supplied explanations;
do not promise a universal speed multiplier or task-completion time.

[Official Codex speed documentation](https://learn.chatgpt.com/docs/agent-configuration/speed)
and [OpenAI API Fast-mode documentation](https://developers.openai.com/api/docs/guides/fast-mode)
describe service tiers separately from reasoning effort. API responses can
report a different served tier, so a requested Fast setting is not proof that a
request ran Fast.

The local catalog also gives **Ultra** an orchestration description: maximum
reasoning with automatic task delegation. Consequently, a blanket claim that
every effort setting affects only thinking would be wrong for this runtime.
Ultra effort and Ultrafast speed remain distinct values. Provider-owned
delegation does not establish a fixed number of Neko child tickets.

## Live local discovery

Queried the installed `codex-cli 0.160.0` using a short-lived app-server from an
empty scratch directory, with `model_provider="openai"` and experimental API
capabilities. Only initialization and paginated `model/list` were requested.
No prompt, task, model generation, account change or daemon restart occurred.

| Advertised route | Effort choices | Additional speed choices |
| --- | --- | --- |
| GPT-6 Astra | Low, Medium, High, Extra high, Max, Ultra | Fast |
| GPT-6.1 Sol | Low, Medium, High, Extra high, Max, Ultra | Fast |
| GPT-6 Luna | Low, Medium, High, Extra high, Max | Fast |
| Claude Fable / Opus / Sonnet through this Codex catalog | Low through Ultra | None advertised |
| Grok 4.20 non-reasoning through this catalog | No adjustable effort | Fast |

This catalog returned `priority` as the Fast wire ID and `fast` as an additional
speed alias. Deduplicate display choices while retaining the adapter's correct
wire mapping. It did not advertise Ultrafast for these routes. Do not expose
Ultrafast solely because another app or account has it.

These are advertised capabilities, not successful inference checks or proof of
remaining allowance. A direct Claude adapter may expose different choices from
Claude routed through Codex. Runtime/provider identity must remain part of the
selection key. The raw local response is at
`/tmp/neko-runtime-capabilities-20261006.json`; it is an ephemeral diagnostic.

## Two native designs

### A. Effort-first pearl panel — recommended

Composer: `pearl  Neko decides ▾` and the existing separate `Plugins` control.
With a manual override, show `pearl  Astra · High ▾`, plus a small lightning
indicator when an accelerated tier is selected.

Click opens an anchored SwiftUI popover with the effective effort title,
clickable model name, discrete effort slider, and lightning speed control.
The model link replaces the popover content with a searchable model list.
A labelled reset action returns to Neko decides. The chosen model's supported
levels determine slider stops; unsupported effort removes the slider. Every
stop exposes its label through accessibility and keyboard controls.

Neko decides must not be an inert label. During automatic dispatch, expose the
resolved model, effort and speed with one short reason, such as a task needing
more reasoning. Keep source text legible and animation confined to the pearl
and intentional interaction feedback. No perpetual shimmer while typing.

### B. Separate compact controls

Composer: `pearl  Astra ▾` · `High ▾` · `lightning` · `Plugins`.
Each setting is reachable in one click. It consumes more horizontal space and
is most useful when users regularly override effort. Use the same state and
adapter contract as A so the two previews remain behaviorally comparable.

Both designs are implementable with SwiftUI popovers, native lists/pickers and
the existing Metal pearl. Neither requires a webview or a detached dock.

## Implementation boundary in Neko

Neko already reads effort IDs into `CatalogModel.reasoningEfforts`. It drops
their descriptions, default effort and speed-tier metadata. `AgentRuntime`
currently stores only provider and model. `ComposerRuntimeMenu` changes that
runtime globally for future runs. No automatic model/effort/speed policy is
implemented by the existing pearl preview.

1. Extend the existing catalog with effort labels/descriptions/default and
   speed choices/default. Keep absent, unknown and unsupported distinct;
   runtime-advertised capabilities take precedence over a versioned fallback.
2. Persist conversation preferences and a resolved runtime selection per
   admitted run: provider, model, effort, speed, selection mode and reason.
   Keep global settings as defaults. Validate again at dispatch and include
   the complete selection in connection-check identity, not just model ID.
3. Adapt the selection to the actual executor. Codex exec uses supported
   per-process config overrides for effort and service tier; an app-server
   adapter uses turn parameters. Other adapters declare their mappings.
   Verify fresh and resumed execution; do not assume an override survives a
   resume. Do not rewrite global CLI configuration.
4. Implement Neko decides as a bounded selection over valid candidates,
   informed by task type, explicit user priorities and fresh usage data when
   available. Manual pins win. Unknown quota remains unknown. Paid accelerated
   tiers follow an explicit saved spending preference. Persist the result
   before work starts; reevaluate at a new turn/phase boundary, not halfway
   through an arbitrary running command.
5. Reuse that contract in the native control. Show requested versus effective
   state accurately, including next-turn changes and provider errors. Runtime
   choice must not silently change tool permissions or publication authority.

Begin with capability discovery, the real manual selector and round-trip
execution evidence. Automatic selection can then use the same verified seam.
Do not ship a Neko decides control until it selects and records a real runtime.

## Acceptance evidence still required for implementation

- Catalog fixtures: differing effort ladders; unsupported controls; multiple
  speed tiers; aliases; defaults; model changes; stale discovery.
- Wire-level tests: selections reach fresh and resumed runs; one conversation
  cannot change another's settings; active work keeps its captured selection.
- Routing checks: manual pins, missing quota, classifier failure, unavailable
  candidates and saved spending rules have deterministic outcomes.
- Installed native checks: two layouts, keyboard, screen reader labels,
  reduced motion, popover placement and typing responsiveness.
- A bounded real inference check for each implemented adapter path, reporting
  actual versus merely requested settings where the runtime exposes them.

This research pass verified source and live catalog metadata. It did not modify
the app, run model inference, rerun test suites or verify a new native UI.
