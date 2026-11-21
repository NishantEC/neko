# 0003. Everything is a plugin: a small kernel and one plugin model

Status: **Proposed (2026-11-21).** Nothing here is built. Edits no code; the migration in section 6 is the plan.
Builds on [0002](0002-rune-for-everything.md) ("Plugin slots": the same four slots serve built-in and third-party features),
`docs/knowledge/one-registry-plugins-are-not-a-separate-world.md` (owner: "it should submerge") and
`docs/architecture/system-map-2026-11.md` (the seams S1 to S10 this ADR leans on).

Reading guide: sections 3 and 4 are the design; section 5 is the table to argue with; section 7 is what the owner has to decide.

---

## 1. Context

**The request.** The owner wants "our entire system to be based on plugins only, similar to how DeepSeek has it". DeepSeek Harness
(`dsh`) is the product the owner keeps pointing at: its docs say "Every part of the product is a plugin, including the model
adapter, the tool registry, the session log, and the agent loop itself" and "There is no privileged core to patch"
([docs/architecture.md](https://github.com/deepseek-ai/deepseek-harness/blob/2e39e704/docs/architecture.md)).

**What Neko is today** (system map, section 2 and 7): one resident Rust daemon with about fifteen independent timer threads on a
shared `Arc<Mutex<Db>>`, a typed `Request` enum of about 75 variants in `crates/neko-protocol/src/lib.rs`, and three clients
(Rune, the web UI, the Swift shell) that each hard-code their own screens. Features were added one at a time, and each brought its own
storage, clock, gate, wire variant and screens. Neko has also grown **eleven separate extension mechanisms** that do not know about
each other (appendix A). The system map already names the cost: five time triggers with five gates (S2), seven representations of
"needs the person" (S1), run launching defined per call site (S3), four storage conventions (S5). Cats (`Request::Cats`, its own table,
its own tick) and Packs (`Request::Packs`, ownership records, digest approval) are the two newest features and already have the
shape of a plugin without the name.

**The owner's two removals.** The owner just removed two installed web plugins, `health` and `3d-viewer`. That is the model:
a feature the owner can take away without losing the product is an **optional plugin**, not a built-in. Today the 3D viewer exists
twice as built-in code (`web/src/canvas/viewers/ModelViewer`, `apps/neko-rune/src/viewers/model3d.rs`, about 600 KB of web chunk) and
cannot be switched off. I could not find the source of the `health` plugin in the repository (the data folder no longer has it; the
nearest built-ins are the "Usage and machine" page and Settings, Diagnostics), so this ADR treats it as optional and does not
claim to know what it showed.

**What "plugin" has to mean here.** DeepSeek's plugins are JavaScript modules loaded into one Node process and hot-reloaded. Neko's
daemon is compiled Rust with safety properties that rest on host-held authority (leases, the admission gate, the process guardian).
A literal copy ("load arbitrary code into the daemon") would give up exactly what the system map section 6 calls Neko's safety. So the
ADR separates three things that "plugin" blurs: **a boundary** (a feature touches the rest only through declared services, events
and contribution points), **a lifecycle** (it can be started, stopped, disabled and removed without a restart, leaving nothing behind),
and **a trust tier** (who wrote it and how hard it is fenced). Everything Neko ships gets the first two. Only code that is not ours
is fenced by the third.

---

## 2. How DeepSeek does it, and what to copy

Studied 2026-11-21 at `deepseek-ai/deepseek-harness` `2e39e704` (pushed 2026-11-18) and `deepseek-ai/dsh-superagent-prototype`
`8c32a9dc` (2026-10-31), both MIT. Reading only; nothing was run. The earlier studies are
`docs/research/deepseek-harness.md` and `docs/research/harness-architectures-2026-11.md`; this section adds what they did not
read: the Cordis source and the plugin manager.

### 2.1 Cordis, the framework underneath

`dsh` vendors Cordis (MIT, copyright Shigma; `vendor/cordis`, version 4.0.0-rc.12 per
[docs/rescope.md](https://github.com/deepseek-ai/deepseek-harness/blob/2e39e704/docs/rescope.md)) and builds everything on five ideas
([docs/cordis-primer.md](https://github.com/deepseek-ai/deepseek-harness/blob/2e39e704/docs/cordis-primer.md)):

- **A plugin** is a function or class with optional `name`, `Config` (a standard schema validated before start), `inject` (the
  services it needs) and `apply(ctx)` (`vendor/cordis/src/registry.ts`, `Plugin` namespace).
- **A context is a repository of services.** A service claims a key such as `ctx.tools` or `ctx.sessions`; consumers find it by key,
  never by importing the provider.
- **`inject` is the load order.** A plugin that names a missing service waits. Ordering is a dependency graph, not boot code.
- **Typed events with dispatch modes**: `emit` (observe), `waterfall` (around-middleware, each listener calls `next()` or
  short-circuits), `parallel`, `serial`, `bail` (stop at the first answer).
- **Registrations are reversible effects.** Tools, prompt sections, listeners and providers are installed through `ctx.effect()` or
  `ctx.on()`; disposing the plugin's *fiber* runs the disposers, so unloading unwinds exactly what loading added.

A plugin instance is a **fiber** with a state machine: `PENDING` (waiting for services), `LOADING`, `ACTIVE`, `FAILED`, `UNLOADING`,
`DISPOSED` (`vendor/cordis/src/fiber.ts`, `FiberState`). A failed fiber "requires an explicit `update()` to recover". Config can be
declared `Volatile`, read with `.get()`, and changed at runtime without restarting the plugin (`loader/volatile-update`).
`isolate` realms (`vendor/loader/src/config/isolate.ts`) give a group of entries their own instance of a service, which is how one
agent preset gets a different capability set from another.

### 2.2 Composition: bundles, profiles, patches

A running `dsh` is "a plugin tree composed at boot from ordered layers" (architecture.md, "Profiles and bundles"). A **bundle** is
a package whose `package.json` has a `dsh.bundle.patch` pointing at a YAML file of rows (`{ id, name, config, disabled }`). A
**profile** (`dsh.profile`) lists the bundles it stacks. Layers apply in order: profile bundles, the profile's `cordis.patch.yml`, a
home patch, then `--patch` overlays. A patch targets a row by `id` and replaces its whole config or inserts rows.
`dsh --profile web --dump-config` prints the resolved tree. Shipped profiles: `web`, `headless`, `sdk`, `sdk-minimal`, `acp`; the
base bundle (`dsh-base`) holds model adapters, tools, persistence, sandbox policy, settings, credentials and telemetry. A second
level, the **agent preset**, is one row whose `config.plugins` lists the plugins *for one agent* (Standard, Creator, PTC, Minimal).

### 2.3 What a plugin contributes

architecture.md, "Where new behavior goes", is a 24-row table; the ones that matter here: a model adapter (`ctx.llm`), a model-facing
tool (`ctx.tools`, whose schema and its own prompt section register together and vanish together), a human command (`ctx.commands`,
no model turn), background jobs (`ctx.jobs`), a webhook rule, a filesystem or sandbox provider, an interception on an `agent/*` or
`tools/*` event, durable session state (extend `SessionEventMap`), UI (typed **slots**, `ctx.slots.register`, for example
`settings.section`, `plugins.detail.badge`), and a capability **seam** (a Service Definition, a Provider and a Consumer; "one role
alone is not a seam"). A plugin with a browser half declares `dsh.client` in its `package.json`; a client module system serves that
half and attaches it to the Loader row named after the package
([docs/cookbook/adding-a-settings-card.md](https://github.com/deepseek-ai/deepseek-harness/blob/2e39e704/docs/cookbook/adding-a-settings-card.md)).
Tools are declared with typed parameters and a canonical output schema, and `ctx.tools.guard()` is "a final monotonic deny that
later listeners cannot undo" ([docs/cookbook/adding-a-tool.md](https://github.com/deepseek-ai/deepseek-harness/blob/2e39e704/docs/cookbook/adding-a-tool.md)).
The superagent prototype is itself one plugin (`dsh-superagent`) that exposes a narrow `orchestrator` service; its GitHub and Feishu
**sources** are separate plugins that `inject: ['orchestrator']` and register a source kind as an effect
(`docs/008-cookbook.md`, section 9; `cordis.patch.yml`).

### 2.4 Lifecycle, hot reload and error containment

`dsh-hmr` watches the profile files and reloads; Plugin Manager (`packages/boot/plugin-manager/README.md`) toggles an entry by
writing only `disabled` into the profile patch (and removes the override when the layers below already give that state, so the patch
records departures only), installs and removes bundles through pnpm, and returns `restart-required` when HMR is off. The failure
table in `packages/boot/app-boot/README.md` is the honest part: a bad optional plugin warns and the rest continue; a bad *required*
plugin stops startup; a synchronous `apply()` throw, an invalid config or a missing service is contained to that entry on reload; **but
"detached asynchronous work outside the `apply()` return Promise produces an unhandled rejection" and an uncaught exception in a
callback are fatal for the whole process, "regardless of entry id"**. `sanitizeProfile` is the recovery path: it renames the profile's
patch to a `.bak-<timestamp>` file without loading any plugin.

### 2.5 What DeepSeek does *not* give us

- **No isolation.** Plugins are in-process JavaScript with host permissions; the manager tool needs full access or a one-call approval
  "because installing a plugin loads code with host permissions into every session" (`docs/research/deepseek-harness.md`, section 1).
  The permission knobs (`SandboxMode`, approval policy) govern what the *model* may do, not what a plugin may do.
- **No privileged core is rhetorical.** There is a spine: Cordis itself (Context, Fiber, registry), the loader and app-boot, and the
  `core/*` packages (session, system-prompt, tools, agent, agent-loop, scope, llm). `docs/capability-seams.md` calls them "core spine
  services". The honest claim is "a small spine, everything else in the same shape".
- **No per-plugin storage model.** Session state is one append-only log; plugins extend its event map.

### 2.6 Copy and do not copy

| Copy (ideas; no code needed, so no licence notice) | Do not copy |
| --- | --- |
| A plugin is a *declaration plus a function that registers effects*; unloading runs the disposers (reversible effects) | Loading third-party code into the daemon with host permissions |
| Services by key with declared dependencies; a missing service means "pending", not "crashed" | A JavaScript runtime or Cordis itself (we are Rust; the idea ports, the library does not) |
| The fiber state machine and "a failed plugin needs an explicit retry" | Module-level hot reload of the whole tree (the earlier study already ruled it out for Neko) |
| Dispatch modes: observe, waterfall, serial checkpoint, bail, plus a monotonic deny | Fatal-on-detached-failure. Neko is resident; one plugin's thread must not end the daemon |
| Profile = ordered layers; patches that record only departures; a printable resolved tree | YAML as the store. Neko keeps profiles in its database like every other setting |
| Each tool registers its own prompt section next to it; an order table; golden files | A plugin installing plugins through a package manager at run time |
| `Volatile` config: change a setting without restarting the plugin | Making the model the plugin author's only path (the Creator loop is useful; it is not the model) |
| Typed UI slots, a "client half" declared in the manifest, `plugins.detail.*` slots for plugins to annotate each other | The "no core" claim: we keep a named kernel and say so |
| Seams as three roles (definition, provider, consumer), and "a service nobody consumes is not a seam" | |

Licence position: both repositories and vendored Cordis are MIT. This ADR copies no code and no prose; the identifiers `fiber`,
`inject`, `volatile` and the dispatch-mode names are used as plain vocabulary. If a later slice ports a test or a table verbatim, it
carries the MIT notice in `THIRD_PARTY_LICENSES/` as the existing three.js and Excalidraw notices do.

---

## 3. The kernel

**Rule: the kernel is what must work for a plugin to exist, and what must hold when a plugin is wrong.** Everything else is a
plugin, including the task pipeline. The kernel has no knowledge of tasks, cats, goals, routines, calendars or any feature noun.

| # | Kernel part | Why it cannot be a plugin | Today |
| --- | --- | --- | --- |
| K1 | **Process shell**: singleton bind, startup order, shutdown, panic policy, `--safe-mode`, `--dump-plugins`, the guardian re-exec | A plugin cannot start the thing that starts plugins | `crates/neko-daemon/src/main.rs` |
| K2 | **Transport and protocol framing**: Unix socket, HTTP bridge with Host/Origin/Sec-Fetch/token gates, `Frame`, request routing to a plugin by id, event broadcast and SSE, the artifact CSP | These are the gates every plugin sits behind; a plugin must not be able to widen them | `server.rs`, `web.rs`, `neko-protocol` |
| K3 | **Store**: `Db`, WAL settings, transactions, content stamp cache, quiet / deferred / immediate write modes, the migration runner, per-plugin namespace enforcement | One writer; the load cache is only correct if every write goes through `Db` (CLAUDE.md, "Anything `load` reads must be written through `Db`'s methods") | `neko-core/src/db.rs` |
| K4 | **Plugin host**: manifests, registry, dependency resolver, lifecycle state machine, effects and disposers, service registry, event bus, contribution registries, plugin status, profiles | This ADR | new |
| K5 | **Run substrate**: one `RunLauncher` (spawn under the guardian, process-group kill, descendants kill, wall-clock timeouts, `RunSpec`, the run registry) | Process containment must not depend on a plugin being well behaved | `native_runner.rs` (`spawn_guarded`, `execute_with_guard`); S3 says it is defined per call site today |
| K6 | **Admission gate and the machine steward**: `may_start`, steward `holding` and `cap`, daily spend ceiling, the `no_agent_start_bypasses_may_start` invariant | "The owner wants the Mac cool and quiet" (CLAUDE.md) cannot be opt-out | `admission.rs`, `machine.rs` |
| K7 | **Authority**: leases (`capability.rs`), `RunAuthority`, role checks, the MCP bridge transport, Keychain access, `redact::scrub`, the digest-approval primitive | A plugin must not issue itself authority | `mcp_host/capability.rs`, `packs.rs` approval |
| K8 | **Usage ledger write path**: `usage_events` append and the ceiling read | The gate reads it; one line per run for every kind (fixes S4) | `usage_ledger.rs`, `spend.rs` |

Not in the kernel, on purpose: tasks (the `Task` row), workspaces, the scheduler tick, the pipeline, runtimes (Claude, Codex,
OpenCode), the MCP connection manager, the Home chat, every feature in section 5, every client screen, the self-updater, themes,
skills. The kernel is estimated at 8 to 10 thousand lines of today's 80 thousand in `neko-core` and `neko-daemon`; I did not
count it precisely, and the number is a target to hold slice by slice, not a measurement.

**Foundation plugins.** Some plugins are depended on by nearly everything and cannot sensibly be switched off: `neko.workspaces`,
`neko.sessions` (the task rows and their events), `neko.pipeline`, one agent runtime, `neko.needs-you` and `neko.plugin-manager`. They are ordinary plugins in code (manifest, services, effects, tests) but marked `essential = true`: the UI
shows them as "Part of Neko" with no switch, and the host refuses to start without them. This is the answer to "what is the
privileged core": it is a *named list of plugins* with no special powers, plus the eight kernel parts above. If a foundation plugin
fails, the daemon starts in **degraded mode** (section 4.9), it does not exit.

**Enforced, not promised.** Three tests keep the line (the system map's `no_agent_start_bypasses_may_start` is the template):

1. `kernel_names_no_feature`: the kernel module tree imports nothing from `plugins/*`, checked from the module graph.
2. `plugins_touch_the_world_through_ctx`: a source scan that fails on `std::thread::spawn`, `Command::new`, raw SQL outside the
   plugin's namespace, `RunSpec` construction and `spawn_guarded` inside a plugin directory.
3. `disabling_removes_everything`: for every registered plugin, start it, stop it, and assert every registry (tools, requests,
   events, triggers, slots, services, threads) is back to empty. This is the reversible-effects invariant, and it is the single most
   important test in the design.

---

## 4. The plugin model

### 4.1 Identity, versioning and the manifest

A plugin has an **id** (`neko.cats` for ours; `author.name` for others; lowercase letters, digits, `-`, `.`), a **version** (semver),
and an **api** range (the host API it was written for, `api = "^1"`). One manifest schema serves every tier. Third-party plugins
ship it as `plugin.toml` (the successor of `pack.toml`; the reader accepts both for two releases). Built-in plugins declare the same
struct as a Rust `const`, and the host can serialize it, so Setup shows built-ins and installed plugins from one list.

```toml
id          = "neko.calendar"
name        = "Calendar"
version     = "1.0.0"
api         = "^1"
tier        = "builtin"            # builtin | script | process
description = "macOS Calendar plus Neko's dated work."
licence     = "MIT"
essential   = false                # foundation plugins: true
default     = "off"                # on | off  (what a fresh profile does)

[requires]                         # a missing one => state Pending, never a crash
plugins   = ["neko.sessions ^1"]
services  = ["clock", "needs-you?"]            # "?" = optional
client    = ["os.calendar"]                    # something only a client can do (EventKit)
binaries  = []                                 # as packs: named, never pathed

[provides]
services  = ["calendar@1"]

[permissions]                      # words shown before enabling, plus the closed list the host enforces
summary      = "Reads your calendar events from the Mac app and shows them next to Neko's work."
capabilities = ["tasks:read", "storage:own"]

[storage]
tables = 0
kv_quota = "1MiB"
migrations = []

[config]                           # JSON-schema subset; drives the Settings section in every client
  [config.days_ahead]  type = "int"   default = 35  min = 7  max = 90  volatile = true

[contributes]                      # section 4.6; every row has an id unique within the plugin
requests   = [{ ns = "calendar", ops = ["status", "events", "store", "set-enabled"] }]
ui         = [{ slot = "page", id = "calendar", title = "Calendar", icon = "calendar", group = "main", web = "calendar", rune = "calendar" }]
commands   = [{ id = "go", title = "Go to Calendar" }]
settings   = [{ id = "calendar", title = "Calendar" }]
```

**Rules lifted from Packs, unchanged:** approval is bound to a SHA-256 digest of the normalized manifest plus a hash of every file
the plugin ships; `Preview` first, `Install { source, digest }` recomputes and refuses on a mismatch; an update whose capabilities,
commands, config, contributions or code hash differ is refused until the new digest is approved; an installed plugin whose files no
longer match is `Changed` and does nothing until re-approved; ownership records and `.neko-pack`-style markers decide what a removal
may delete (`docs/knowledge/packs.md`). The install journal, all-or-nothing rollback and crash recovery carry over to every
plugin that has files.

### 4.2 Isolation tiers

| Tier | What it is | Runs | Who may write it | Fence |
| --- | --- | --- | --- | --- |
| **builtin** | Rust crates compiled into the daemon, one module (later one crate) per plugin | In the daemon, on threads the host owns | Us | Review, the three tests in section 3, `catch_unwind` at every host-owned boundary. **Not a sandbox**: a built-in could still misbehave, which is why third-party Rust is never loaded here |
| **script** (UI) | Luau in Rune (exists: `apps/neko-rune/src/luau/`); web JavaScript in the web UI (exists: `<data>/plugins/<id>/`) | In the client | Anyone | Luau: 250 ms watchdog, 64 MB, CPU share, fixed `host.read` list, no raw colours (`docs/knowledge/luau-plugins.md`). Web: today a plugin runs in the page with the page's authority (`one-registry...md`: "User pages keep the authority they had"), which is weaker than it sounds; slice 9 moves it behind a sandboxed frame and a capability proxy, the way artifacts already run |
| **process** | An out-of-process program the host spawns and talks to | In a child under `spawn_guarded`, `env_clear`, timeouts, optional `sandbox-exec` profile derived from the capability list | Anyone | Process containment already exists (K5). **Today the only process-tier contribution is an MCP server** (`[[mcp]]` in a pack: tools). A JSON-lines plugin protocol for events and triggers is designed in slice 9, not now |

Consequences that are decisions, not details: (1) there is **no third-party Rust in the daemon**, ever; (2) a feature that needs
daemon-side logic and is not ours is an MCP tool plus a skill plus a routine plus a script UI, exactly what a pack is today; (3) the
tier is in the manifest and cannot be raised by an update without a new approval.

### 4.3 Lifecycle

States (the fiber machine, with Neko's two additions): `Installed` -> `Pending { missing }` -> `Starting` -> `Active` ->
`Stopping` -> `Stopped`; plus `Failed { reason, retry_at }`, `Changed` (digest mismatch, from packs) and `Quarantined` (crash loop,
needs the person). Every transition is a status line the Plugins page shows with its reason.

```rust
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &'static Manifest;
    /// Register effects through ctx. Returning Err puts this plugin in Failed; nothing else notices.
    fn start(&self, ctx: &mut PluginCtx) -> Result<(), StartError>;
    /// A volatile field changed, or the config did: keep running, or ask for a restart.
    fn config_changed(&self, ctx: &PluginCtx, next: &Config) -> Reload; // Reload::Live | Reload::Restart
    fn health(&self, ctx: &PluginCtx) -> Health;                        // Healthy | NeedsSetup(why) | Failing(why)
}
```

There is **no `stop`**. Dropping the `PluginCtx` runs the recorded disposers last-in, first-out (a registration is an effect, as in
Cordis). Order: start in dependency order (topological sort over `requires.plugins` and `requires.services`, independent plugins
in parallel, database migrations serialized); stop in reverse; when a provider stops, its dependents drop to `Pending` first, then
resume when it returns. Timeouts: `start` 10 s (then `Failed { timeout }`), disposers 5 s per plugin. A `Failed` plugin retries
with the backoff the watch tick already uses (5 s growing to 1 h); after five failures it waits for the person (`Retry`) or a config
change, the same rule as "a failed fiber requires an explicit update". `Health` reuses the pack statuses
(Healthy, NeedsSetup, Failing, Changed).

### 4.4 Services and the context

**`PluginCtx` is the only door.** It is handed to `start`, scoped to one plugin, and everything the plugin registers through it is
recorded as an effect for removal. Its surface (each call checks the plugin's capability set):

| Call | Does |
| --- | --- |
| `ctx.provide::<dyn S>(key, Arc<impl S>)` / `ctx.service::<dyn S>(key)` | Provide and consume a typed service by key. `service` returns `Err(Missing)` only for an *optional* dependency; a required one is guaranteed present while the plugin is `Active` |
| `ctx.on(event, handler)` / `ctx.emit(event)` | The event bus (4.5) |
| `ctx.every(trigger, gate, f)` | A timer, schedule or event trigger the host owns (replaces fifteen `thread::spawn` loops; fixes S2) |
| `ctx.spawn(name, f)` | A host-owned thread with a cancel token, `catch_unwind`, and a status; joined on stop |
| `ctx.tool(spec, handler)` | A tool for agents on the MCP bridge (4.6) |
| `ctx.handle(ns, handler)` | Requests under the plugin's namespace |
| `ctx.db()` / `ctx.kv()` / `ctx.files()` | Namespaced storage (4.8) |
| `ctx.run(kind, spec, lease)` | Start an agent run **only** through K5 and K6; there is no other way to spawn one |
| `ctx.needs_you(item)` / `ctx.notify(msg)` | Through the `needs-you` and `notify` services, when present |

**Services are the seams.** Each feature seam in the system map becomes a service with the three roles Cordis uses: a *definition*
(a trait, in `neko-plugin-api`), a *provider* (one plugin) and a *consumer* (others). The first set: `clock` / `triggers` (S2),
`needs-you` (S1), `runtimes` (the Claude, Codex and OpenCode adapters; one provider each), `sessions`, `workspaces`, `artifacts`,
`notify` (ntfy, Reminders, Dock badge), `ledger` (read side), `skills`, `memory-context` (S9), `search` (the old `Provider` seam).
A plugin states `requires` for what it consumes, so the dependency graph is data: `neko.goals` requires `sessions`, `triggers`,
`needs-you`, `runtimes`; `neko.phone` requires `needs-you` and `notify`.

**Scopes.** Cordis scopes a registration to one agent through that agent's own context; Neko has four scopes already (global,
workspace, cat, session). `ctx.in(Scope::Workspace(id))` returns a view whose registrations disappear with that scope, and a profile
or a workspace can choose a plugin subset (4.10). This replaces the per-run-kind authority models of S10 with one lease that
carries a plugin-tool allow-set.

### 4.5 Events

An in-process bus with the dispatch modes that earn their place:

| Mode | Neko use | Example |
| --- | --- | --- |
| **emit** (observe, never awaited) | Facts after the fact | `session/status`, `run/finished`, `pr/updated`, `artifact/saved`, `plugin/state` |
| **waterfall** (around-middleware; the listener calls `next()` or answers) | Rewriting, only where the owner allows | `prompt/assemble` (sections), `reply/render` |
| **serial** (a checkpoint, in registration order, awaited) | The one place work hooks a boundary | `session/turn-ended`: goals decide continue, wait or done here, exactly where DeepSeek's `agent/turn-stopping` hooks goal rounds |
| **guard** (bail, monotonic: a plugin can only *hold*, never admit) | Extra reasons not to start | `admit/veto`: a plugin may add a hold reason to `may_start`; it can never override the steward, the cap or the ceiling |

Rules. Durable facts stay rows owned by their plugin (task events, cat messages, the ledger); events are live notifications, and a
plugin that must not miss one keeps a cursor over the owner's rows, as `cat_wakes` already does. Handlers have a 50 ms budget on the
caller's thread; anything longer must enqueue. Event names are `<plugin-short>/<name>`, declared in the manifest
(`contributes.events` for ones it emits) so the catalogue is data and a golden test lists it.

### 4.6 Contribution points

The complete list of things a plugin can add. Each is declared in `[contributes]` (so a disabled plugin shows nothing, and a
Preview can show the person what they are approving) and registered as an effect in `start`.

| Point | Key | Replaces today | Notes |
| --- | --- | --- | --- |
| Requests | `requests` | A variant per feature in `Request` (`Packs`, `Cats`, 70 more) | Built-ins keep **a typed variant per plugin** carrying the plugin's own command enum (the shape `Request::Cats(CatCommand)` already has); third-party plugins use one generic envelope `Request::Plugin { id, op, args }`. The Rune allow-list test (`tests/allow_list.rs`) then keys on `(plugin, op)` |
| Agent tools | `tools` | `neko_protocol::crew::TOOLS`, `crew_definitions`, the `crew_call` arms, `CrewRole::allows` | Name, parameter schema, roles allowed, read-only hint, idempotency key rule; appears on the bridge's `neko_list_tools` while the plugin is Active and the run's lease allows it |
| Prompt sections | `prompt` | `INSTRUCTION`, `super_instruction`, the "Installed viewers" section (`viewers::installed_section`) | A section with an order number from a central table (the band is owned by the kernel), registered beside the tool it explains, gone with it. Golden files per profile (`crates/neko-core/tests/prompts/` already pins four) |
| Triggers ("rounds") | `triggers` | Schedules, Routines, Watches, goal `Wait`, cat wakes, digest, curator, improve (S2) | One due-store, one gate: `ctx.every(Trigger::{Time, Cadence, Event, Pid, PrState}, Gate::..., f)`. The gate always includes `may_start` |
| Needs-you sources | `needs_you` | Seven representations (S1) | A *kind* of item (a goal question, a pack change, a suggestion) projected into the one `needs-you` service; the Inbox, phone push, cat wakes and the digest read that one list |
| Runtimes | `runtimes` | `native_runner::cli_workers::Agent`, the Codex adapter, `AgentRuntime.provider` | A provider of the `runtimes` service: command builder, event parser, sandbox profile, resume support flags |
| Search and quick-panel providers | `search` | `neko_core::provider::Provider`, 24 impls | The trait stays, registered from a plugin instead of `AppState::new` |
| Viewers and fences | `viewers`, `fences` | `web/src/canvas/viewers.ts`, `apps/neko-rune/src/viewers/mod.rs`, `neko_core::viewers::BUILTIN` | One registry per client, fed by manifests; `BUILTIN` is generated, no longer a list kept in step by hand |
| Artifact kinds | `artifact_kinds` | `artifacts::kind_of`, `check_contents`, `mime` | Check rules and the served type per kind |
| UI slots | `ui` | Page lists in `web/src/pages/index.ts`, Rune's sidebar and Setup rails, Luau `card`/`view` | See 4.7 |
| Settings sections | `settings` | `web/src/pages/setup/settings/registry.ts`, Rune `settings_*.rs` | Generated from `[config]`; a plugin may ship its own component instead |
| Commands | `commands` | `registerCommands`, `palette.rs` | Entries in the palette, run without a model turn (DeepSeek's `ctx.commands`) |
| Storage | `storage` | `_v1:` settings rows, ad hoc tables | 4.8 |
| Skills | `skills` | `<data>/skills`, pack `[[skills]]` | Folders with `SKILL.md`, locked from the curator as packs do |
| MCP servers | `mcp` | pack `[[mcp]]`, `AddConnection` | Granted to no workspace until the person grants them |
| Digest lines and notifier channels | `digest`, `channels` | `digest.rs` line kinds, `phone.rs` | A plugin adds a line to the morning digest or a way to reach the person |
| Health checks | `health` | pack `[health]`, `MachineHealth` | One line each in Usage and Diagnostics |
| Themes | `themes` | `<data>/themes/*.json`, `themes.rs` | Token files, validated by the existing contract tests |

### 4.7 UI contribution: slots, and the client half

DeepSeek's answer, a typed slot registry plus a client half the host serves, fits Neko with one twist: **enablement is
daemon-owned data**. The daemon says which plugins are Active and which slots they declared; each client renders only those.

- **One slot vocabulary for both clients**: `sidebar.item` (a page), `page`, `session.header.action`, `session.menu`,
  `composer.command`, `context.tab`, `context.viewer` (by kind), `neko.card`, `home.block`, `settings.section`,
  `palette.command`, `statusbar.item`, `reply.fence`, `task.sheet.tab`, and `plugin.detail.section` (a plugin annotating another's
  page, DeepSeek's `plugins.detail.*`). These are the four slots of 0002 (viewers, views, session actions, cards) plus the ones the
  web host (`registerPage`, `registerCommands`, `addSection`, `addTab`, `registerViewer`, `registerFence`) already has.
- **Client halves** are declared per client in the manifest: `web = "<module>"` (a built-in lazy chunk or a `<data>/plugins` module),
  `rune = "<component key>"` (compiled into Rune for built-ins) or `luau = [files]` (the pack field that exists). The key is
  `<plugin id>:<slot id>`.
- **The data path**: `Snapshot` carries `plugins_rev` (the way it carries `cats_rev`) and `Plugins(Slots)` returns the active slot
  list. A client re-reads on a revision change, mounts a renderer for each slot it knows, and ignores one it does not (and reports
  it, as `ReportApp` reports pages). Disabling a plugin therefore removes its sidebar entry, palette commands, settings section and
  viewers in every open window within one revision, without either client being rebuilt.
- **Client-only capabilities** (the Swift shell's EventKit, hotkey, notifications; Rune's `wry` web view) are advertised by the
  client at connect (`os.calendar`, `os.notify`, `os.hotkey`). A plugin states `requires.client`; if no connected client offers it
  the plugin is `Pending { missing: "os.calendar" }` and the Plugins page says "needs the Mac app".
- **Cost, stated plainly.** For built-ins the Rune binary contains the code of every built-in plugin, enabled or not; only the
  web UI can lazy-load. That is acceptable (Rune is one static binary), but it means "optional" for Rune means "not shown", not "not
  shipped". The only built-in code worth omitting from builds is heavy and rare (the 3D mesh path, the Excalidraw chunk); those are
  the optional plugins in section 5.

### 4.8 Storage per plugin

Three kinds, all behind `ctx`, all namespaced by plugin id, none touching the workbench snapshot:

1. **kv**: JSON rows under `plugin:<id>:<key>`, with the write mode a property of the key (`immediate`, `quiet`, `deferred`),
   quota 1 MiB and 256 keys by default (the web plugin storage limits today, `/api/storage/...`).
2. **tables**: the plugin lists versioned migrations in its manifest; the host runs them in one transaction before `start`, creates
   only tables named `p_<id>_*`, records `plugin_migrations(plugin, version)`, refuses to start on a downgrade, and leaves data
   untouched on failure. Cats (`cats`, `cat_messages`, `cat_wakes`) and usage (`usage_events`) are already in this shape; moving them
   is a rename and a manifest. This is the rule S5 asks for: *entities are tables with a per-family revision; blobs are for config*.
3. **files**: `<data>/plugin-data/<id>/` (the directory web plugin storage already uses; `cats/<id>/` and artifacts keep their
   paths and are owned through a declared path map). Containment, symlink refusal and the size caps are the existing readers'.

All writes go through `Db`'s namespaced methods, so the content-stamp cache stays correct (CLAUDE.md). A plugin that changes on a
timer gets its own row (the 2026-10-21 rule). **Removal**: disabling keeps all data; removing a *third-party* plugin archives its
files and rows to `cleanup-archive/` for 30 days (the packs rule); removing a built-in is only disabling it.

### 4.9 Permissions, approval and error containment

**Capabilities** are a closed list the host enforces on `PluginCtx`: `storage:own` (implicit), `tasks:read`, `tasks:write`,
`runs:start` (always through the gate), `tools:call:<connection>`, `net:<host>`, `fs:<declared path>`, `secrets:<name>`,
`needs-you:raise`, `notify`, `events:<name>`, `ui:<slot>`, `exec:<binary>` (process tier), `serve:tools` (agent tools). The manifest
states them and a plain-words `summary`; **Setup shows the words and the list, and approves the digest** (4.1). For tier *script*,
the host API simply does not offer a call the plugin lacks (Luau's fixed `host.read` list is the model). For tier *builtin* the
capability list is **declared and tested, not sandboxed** (the section 3 source scan checks it against the calls made); it exists so
a Preview can say what a built-in can do and so an update that adds a capability is visible: after a self-update, Setup lists "what
changed" per built-in whose capability set differs from the last one the person saw.

**What happens when something is wrong** (modelled on DeepSeek's failure table, with the fatal rows removed):

| Failure | Effect |
| --- | --- |
| Manifest invalid, entry missing, digest mismatch | Listed as Broken or `Changed` with the error; nothing starts (the web plugin list already does this) |
| Required service missing | `Pending { missing }`; starts when the provider appears |
| `start` returns Err, panics or times out | `Failed`; backoff 5 s to 1 h; five failures wait for the person; dependents drop to `Pending` |
| Panic in a plugin thread or handler | Caught at the host boundary (`ctx.spawn`, `ctx.every`, handler dispatch); counted. Five in ten minutes: `Quarantined` and one needs-you item. **Does not end the daemon** (DeepSeek's detached rejection does end its process) |
| Panic while holding the `Db` lock | `Db` recovers a poisoned lock (the mutex is shared by about fifteen threads today); the invariant "no plugin callback runs while the lock is held" is part of `plugins_touch_the_world_through_ctx` |
| Migration fails | Plugin `Failed { migration }`, the transaction rolled back, data untouched |
| Foundation plugin fails | **Degraded mode**: the daemon stays up, the socket answers, the status strip and Plugins page say what is down and why, new agent starts hold with the reason (the same `Held:` line `admission.rs` uses). DeepSeek stops startup on a required entry; Neko is resident and must stay reachable to be fixable |
| Daemon will not boot with the current plugin set | `neko-daemon --safe-mode` starts foundation plugins only (DeepSeek's `sanitizeProfile` is the precedent) and keeps the profile intact |
| A script plugin errors or is killed | The slot shows one quiet "<title> stopped: <reason>" line and Reload; the app never notices (exists for Luau) |
| A process-tier plugin dies | Restart with backoff under the same guard; leftover descendants are killed (K5) |

### 4.10 Hot enable and disable; profiles

**Enable** runs the lifecycle in 4.3 and bumps `plugins_rev`. **Disable** is a *drain*: the plugin gets no new work (its triggers
are removed, its tools stop being listed, its slots drop), in-flight agent runs it started finish within their own timeout, and then
the disposers run. **Remove** (third-party only) cancels in-flight work first. The mid-run rule that exists today for connections
("a connection that breaks during a run still discards it") stays for removal and quarantine, and is softened for an ordinary
disable so the person turning a feature off does not kill work.

**Hot reload** is per plugin and per tier: Luau and web scripts reload in seconds (exists); a process plugin restarts; a *builtin*
changes only with a new binary (the self-updater, which already knows how to swap safely). Config edits are live for `volatile`
fields and restart the plugin otherwise. We do **not** copy tree-wide module HMR.

**Profiles are plugin sets.** A profile row (in the database, `plugin_profiles_v1`) is `{ name, enable[], disable[], config{id: {...}} }`
applied over each manifest's `default`. Layers, in order, as DeepSeek stacks them: manifest defaults, the active profile, a
per-workspace override, then `NEKO_PLUGINS=` or `--plugin` for one launch. A change records only departures from the layer below
(the plugin manager's rule), and `neko-daemon --dump-plugins` prints the resolved tree with the layer that decided each row.

| Profile | Plugins | Why |
| --- | --- | --- |
| `full` | All built-ins whose `default = "on"` | Today's product, the default |
| `quiet` | Foundation, pipeline, delivery, canvas; no cats, rounds, watches, phone, improve | The "record, do not act" posture, the first-run narrow default the owner left open |
| `minimal` | Foundation, one runtime, the web UI | A headless daemon for another client |
| `safe` | Foundation only | Boot flag, not stored |

A **workspace** can also choose a subset of tool and trigger plugins, which is DeepSeek's agent preset: a run's lease carries the
plugin-tool allow-set derived from the workspace's profile and the agent profile (`agent_profiles.rs`).

---

## 5. Mapping: every feature to a plugin

Contribution codes: **Rq** requests, **T** agent tools, **P** prompt section, **Tr** triggers, **N** needs-you source, **Rt** runtime,
**S** search provider, **V** viewer/fence, **UI** slots (page, card, tab, menu), **St** settings section, **C** palette command,
**D** digest line, **Ch** notifier channel, **Hc** health check, **Th** theme, **Sk** skills, **M** MCP servers, **Tb** own tables or
files. Effort: **S** up to a day, **M** two to four days, **L** one to two weeks (an agent plus review; the sum is in section 6).
"Foundation" = `essential`, no switch. "Default" is for the `full` profile.

| Feature today | Plugin | Default | Contribution points | Effort | Notes |
| --- | --- | --- | --- | --- | --- |
| Workspaces, folders, scopes | `neko.workspaces` | Foundation | Rq, UI, Tb | M | `Workspace` is referenced everywhere; moves with sessions |
| Tasks, events, replies, seen, duplicates, naming | `neko.sessions` | Foundation | Rq, T, Tb, UI | L | The `Task` row and status machine; the snapshot slice for tasks |
| Scheduler tick, scout/build/review/repair, evidence gate, worktrees, parking, splits | `neko.pipeline` | Foundation | Tr, Rq | L | The biggest move; last |
| Publish, PR watch, merge, land on main | `neko.delivery` | On | Tr, Rq, UI, N | M | Only the daemon commits and pushes (unchanged) |
| Checkpoints and undo | `neko.checkpoints` | On | Rq, T, Tb | M | Hidden refs stay git's |
| Claude Code, Codex, OpenCode adapters | `neko.runtime-claude`, `-codex`, `-opencode` | One foundation, rest On | Rt | M each | Behind K5; ends per-site fallback code (S3) |
| MCP connections, OAuth, credentials, registry, Tools page | `neko.mcp` | On | Rq, T, St, UI, Tb | L | The bridge transport and leases stay K7; this feeds tools into it |
| Delegation tools and command queue | `neko.delegation` | On | T, P, Rq, Tb | M | Replaces the static `TOOLS` array |
| Home chat, tool cards, full power | `neko.chat` | On | Rq, P, UI | L | Becomes a kind of conversation (S8) when cats/tasks follow |
| Cats | `neko.cats` | On | Rq, Tr, UI, Tb, T, P | M | Already table, variant, tick, files: nearest to done |
| Routines ("rounds"), guard rails, catch-up | `neko.rounds` | On | Tr, Rq, N, UI, Tb | L | Rename in code follows the owner's words |
| Watches, probes, triage | `neko.watches` | On | Tr, Rq, N, UI | M | Superagent's Source plugins are the DeepSeek analogue |
| Schedules (RRULE plans) | `neko.schedules` | On | Tr, Rq, UI | S | Becomes a trigger kind of `neko.rounds`; page stays |
| Goals, judge, evidence, receipts, deadline default | `neko.goals` | On | Tr, N, Rq, P, UI, Tb | L | Hooks `session/turn-ended` (serial) |
| Needs-you / Inbox | `neko.needs-you` | Foundation | N, Rq, UI, Tb | M | Fixes S1: one row family, one list |
| Calendar (EventKit) | `neko.calendar` | **Off** (as today) | Rq, UI, C, St, Hc | M | **First slice.** Needs a client capability |
| Phone push (ntfy, Reminders) | `neko.phone` | On, quiet | Ch, Tr, St, Rq | M | Reads `needs-you`; EventKit half is a client capability |
| Morning digest, briefing | `neko.digest` | On | D, UI (card, home block), Tr | S | Lines contributed by others |
| Issues (planned, D1 to D7 of the Capy plan) | `neko.issues` | On | Rq, T, P, Tb, UI, N, C | L | **Born as a plugin**; the SDK's first from-scratch customer |
| Usage ledger page, history, CSV, quota meters | `neko.usage` | On | Rq, UI, Hc | M | The write path and ceiling stay K8 |
| Rune live browser tab (CDP, watch-only) | `neko.browser-view` | On | UI (context tab) | M | Client-only plugin; loopback discovery unchanged |
| Terminals tab | `neko.terminal` | On | UI (context tab), Rq | S | |
| Canvas, artifacts, comment mode, `ShowArtifact`, `OpenTab` | `neko.canvas` | On | Rq, T, UI, V, artifact kinds, Tb | M | Provides the `artifacts` service; the installed web `canvas` plugin (Excalidraw) overlaps and folds in |
| Viewers: html, image, markdown, code, csv/tsv, PDF | `neko.viewers-core` | On | V | S | Registry fed by manifest; `viewers::BUILTIN` generated |
| 3D viewer (was the removed `3d-viewer`; web `ModelViewer`, Rune `model3d`) | `neko.viewer-3d` | **Off, offered** | V | S | Optional; "Enable 3D viewer" is offered the first time a `.glb` arrives |
| Sketch (Excalidraw) | `neko.viewer-sketch` | **Off, offered** | V, artifact kinds | S | 1.1 MB chunk plus fonts; same offer rule |
| Health (was the removed `health` plugin) | `neko.health` | **Off** | Hc, UI | S | Optional by the owner's decision; contents unknown to this ADR, so the slice is "list it, empty" |
| Packs | `neko.plugin-manager` | Foundation | Rq, UI, St | L | Packs become the install flow for every plugin (4.1); the page replaces Packs and Customisations |
| Luau host (Rune) | Rune client script runtime | Foundation (client) | UI | S | The tier-1 runtime for Rune, not a daemon plugin; unchanged |
| Web plugin host, `/api/plugins`, `host.*`, Customisations, Change Neko | folds into `neko.plugin-manager` and the web client's slot registry | Foundation | Rq, UI | M | Daemon routes become plugin-manager requests |
| Self-improvement | `neko.improve` | On, quiet | Tr, N, Rq, UI, Tb | M | |
| Skills, lessons, curator | `neko.skills` | On | Sk, Tr, Rq, P, UI | M | |
| Memory, decisions, rulebook, learning | `neko.memory` | On | Rq, P, UI, Tr | M | Provides `memory-context` (S9) |
| Agent profiles | `neko.profiles` | On | Rq, UI | S | Becomes the per-run plugin-tool allow-set |
| Self-update | `neko.self-update` | On | Tr, Rq, St, UI | M | Needs the kernel's restart service; cannot update itself while disabled, which the UI says |
| Machine steward (reading) | kernel K6 | n/a | none | n/a | Stays the kernel; Usage page reads it through a service |
| Agent rules file, `--machine-check` hook | `neko.agent-rules` | On | Rq, St | S | Publishes `agent-rules.md` for outside agents |
| Themes | `neko.themes` | On | Th, UI, St | S | Theme files become a contribution; contract tests stay |
| Quick panel: apps, clipboard, files, 24 `Provider` impls | `neko.quick-panel`, `neko.clipboard`, `neko.apps`, `neko.files` | On | S, Rq, C | M | Launcher heritage; the seam already exists |
| Overlays (activity pill, launcher panel, stop-all key) | Rune client `neko.overlays` | On (behind its flag until P5) | UI | M | Client-only |
| Swift shell: window, menu bar, hotkey, EventKit | the OS client; advertises `os.*` capabilities | n/a | client capabilities | S | Not a plugin; it is what plugins' `requires.client` names |
| Legacy: `agents.rs`, `codex.rs` pollers, `permissions.rs`, `usage.rs`, GPUI client | **delete**, do not port | n/a | none | M | S12; ADR 0001 decision still open |

Count: 42 plugin ids above the line, 6 of them foundation (workspaces, sessions, pipeline, needs-you, plugin-manager and one runtime). Features not in the table: none that I could find in the system map's
component table (section 2), its extension-point table (section 7) or `web/src/pages/index.ts`; I did not read every Rune view.

---

## 6. Migration plan: thin slices

Principles. Each slice ships a build the owner can try; no slice moves a feature *and* changes its behaviour; the old request keeps
working as an alias until its clients move (the protocol is private to our own clients and they self-update together, so aliases are
short-lived). The old code path is deleted in the same slice that replaces it. Sizes assume the one-builder-at-a-time rule the
machine needs; the whole plan is **about ten to twelve weeks of agent time**, and the owner sees a visible build after slice 1
(~5 days), 2 and 3 (~2 weeks total).

| # | Slice | What the owner sees | Rough size |
| --- | --- | --- | --- |
| **1** | **Plugin host + Calendar end to end** | Setup has a Plugins page listing every built-in with state; Calendar is a switch there: off removes the sidebar entry, palette command and settings section in Rune and web within a second, and the Mac app stops syncing; on brings it back with its data | ~5 days |
| **2** | **Viewers as contributions** | The 3D viewer and the sketch are optional plugins ("Enable 3D viewer" offered on the first `.glb`); the viewer list is generated from manifests in both clients; the web build stops shipping the 3D and sketch chunks (about 2 MB, `docs/knowledge/everything-renders.md`) until they are enabled | ~3 days |
| **3** | **Triggers service + digest + phone + schedules** | Nothing new on screen except Plugins rows and one honest "next due" list; five time clocks become one due-store with one gate, which closes S2 | ~5 days |
| **4** | **`neko.needs-you`** | One Inbox list that every kind of question feeds; the fake `goals:` and `improve:` routines are gone (S1) | ~3 days |
| **5** | **`Runs` service (H3 of the system map)** | Held starts and running counts include routines, watches, curator and judge, not only tickets and cats | ~5 days |
| **6** | **Cats, watches, rounds, goals** (in that order) | Each is a row on the Plugins page you can drain and stop; `quiet` profile exists | cats ~3 days, watches ~4, rounds ~7, goals ~8 |
| **7** | **`neko.issues`, born as a plugin** | The Issues page and its agent tools | ~8 days |
| **8** | **Tools as a contribution: `neko.delegation`, `neko.mcp`, `neko.chat`** | A plugin adds an agent tool by manifest; the static `TOOLS` array and `crew_call` match arms are gone | ~10 days |
| **9** | **Plugin manager: packs, web plugins and Luau under one install flow; web plugins behind a sandboxed frame; decide the process-tier protocol** | One "Plugins" page replaces Packs and Customisations; a web plugin can no longer reach the page's authority directly | ~6 days |
| **10** | **Foundation: sessions, pipeline, delivery, checkpoints, runtimes; delete the legacy set** | No change on screen; `neko-core` and `neko-daemon` no longer contain feature names outside plugin directories | ~3 weeks |

**Slice 1 in detail (what to build, and why Calendar).**

*Why Calendar.* It is small (about 1,800 lines across `neko-core/src/calendar.rs`, `web/src/pages/Calendar.tsx` and
`apps/neko-rune/src/view/calendar.rs`, plus the Swift bridge), **already behaves like a plugin** (it is off on
a fresh data folder, owns one quiet row, and has `SetCalendarEnabled`), and it exercises every new piece at once: a manifest, an
enable switch that spans two clients and the Swift shell, a page slot, a palette command, a settings section, namespaced storage,
a `requires.client` capability (EventKit), and a request namespace (three variants today, `CalendarEvents`, `CalendarStatus`,
`SetCalendarEvents`). The alternatives were weaker: the 3D viewer needs no daemon half (slice 2); cats is entangled with sessions,
tasks, crew roles and an unfinished batch; the digest is a tick inside the phone thread.

*Build, in order.* (1) `plugins` module in the kernel: `Manifest`, registry, resolver, lifecycle states, disposers, `ctx.provide`,
`ctx.every`, `ctx.spawn`, `ctx.handle`, `ctx.kv`, `plugins_rev`; (2) `Request::Plugins(List | Slots | Enable | Disable | Config)`,
`neko-daemon --dump-plugins`, `--safe-mode`; (3) a Setup rail "Plugins" in Rune and web, read-only list plus the switch, status
reasons; (4) `neko.calendar` as the first plugin, `SetCalendarEnabled` kept as an alias that calls `Enable`; (5) the Swift shell
advertises `os.calendar` and stops its sync timer when the plugin is not Active; (6) the three tests of section 3 plus lifecycle
unit tests (Pending to Active, dispose order, a failing `start` contained, a panicking tick contained, disable leaves registries
empty, a profile layer overrides a default). Rune's sidebar and the web page list read `Plugins(Slots)` for the Calendar entry
instead of hard-coding it.

*Done means:* toggling Calendar off and on in Setup, in both clients at once, leaves no Calendar entry, command or timer while off,
and loses no data; `neko-daemon --dump-plugins` prints the resolved tree; the lifecycle tests pass; `scripts/check.sh` is green.

---

## 7. Risks and open questions

Each is a choice for the owner. My recommendation is first.

1. **How literal is "only"?** *Choice:* (a) the kernel in section 3 plus foundation plugins that are plugins in shape but cannot be
   switched off; (b) also let the owner disable the pipeline, sessions and runtimes (a daemon that does nothing). *Recommend (a).*
   (b) costs a "plugin-less" mode with nothing to show and buys nothing; the structural gain (the boundary, the lifecycle, the tests)
   is the same either way. Risk if (a) is wrong: the owner finds `essential` is a loophole.

2. **How far do third-party plugins reach?** *Choice:* (a) script UI plus MCP servers plus skills plus routines only (what packs are
   today), web plugins moved behind a sandboxed frame; (b) also an out-of-process plugin protocol for events and triggers;
   (c) loadable Rust. *Recommend (a) now, decide (b) in slice 9, never (c).* Loading third-party native code into the daemon removes
   the safety property the whole system map rests on. Known weakness today: web plugins run with the page's authority, and the bridge
   token is in the page; until slice 9 a web plugin is trusted like code the owner wrote.

3. **One noun, and what it breaks.** *Choice:* rename "packs" to "plugins" everywhere now (`plugin.toml`, "Plugins" in Setup, wire
   `Packs` kept as an alias), or keep "pack" as the third-party bundle and "plugin" as the in-product word. *Recommend rename now,
   with a two-release read of `pack.toml`.* The owner's own sentence ("it should submerge") and the cost of explaining two nouns both
   say so. Risk: installed packs' approved digests change if the manifest is normalized differently; the digest function must keep
   producing the old digest for an old manifest, and a test pins it.

4. **What is on by default, and what is removable.** *Choice:* which built-ins are `default = "on"` for a new person, and whether
   the owner wants the `quiet` profile as the first-run default (cats, rounds, watches, phone and improve off until switched on, as
   Calendar and routines already are per `docs/knowledge/first-run.md`). *Recommend `quiet` for new installs and `full` for the
   owner's existing profile.* Also: *removing* a built-in is only disabling it, and its data stays; confirm that is what "removed"
   should mean for the two plugins the owner just removed (the 3D viewer and `health` become switchable, not deletable).

5. **Wire and crate shape.** *Choice:* (a) one typed `Request` variant per built-in plugin (the `Cats` and `Packs` shape) plus a
   generic envelope for third parties, built-ins compiled as modules until slice 8, then split into crates; (b) a fully generic
   envelope for everything now; (c) split into crates first. *Recommend (a).* (b) throws away compile-time types on a 75-variant
   protocol that both Rust clients share; (c) front-loads build-time cost (`neko-core` is already 60 thousand lines in its top-level
   files) before the boundary is proven. Risk with (a): the typed enum keeps growing one variant per plugin; that is bounded by the
   plugin count (about 42) and each variant lives in its plugin's own module, so it is a table of contents, not a coupling.

Risks I do not put to the owner but want on the record: the mutex-poisoning exposure of one shared `Db` lock gets *worse* with
many plugins unless slice 1 includes the poison recovery and the "no callback under the lock" rule; the Rune binary carries every
built-in plugin's code, so "optional" there means hidden, not absent; built-in capability lists are discipline, not a sandbox, and
the ADR says so rather than implying otherwise; and a plugin model invites a plugin for everything, so each new feature needs a
one-line reason that it is not a *part* of an existing plugin (the same discipline S7's "stop adding synonyms" asks for).

What was not verified: nothing here was built or run. DeepSeek claims come from reading the repository at the commits named in
section 2, not from running `dsh`. The 8 to 10 thousand line kernel estimate is a target. The `health` plugin's contents are unknown.

---

## Appendix A. Extension mechanisms that exist today (the eleven this ADR folds into one)

| # | Mechanism | Where | Reach |
| --- | --- | --- | --- |
| 1 | Search `Provider` trait | `neko-core/src/provider.rs`; 24 `impl Provider` | Quick panel results |
| 2 | Agent runner variants | `native_runner/cli_workers.rs`, Codex adapter, `AgentRuntime.provider`, `agent_catalog.rs` | Every run kind |
| 3 | MCP connections | `mcp_host`, per-workspace grants | Agent tools |
| 4 | Delegation tools (code: `crew`, to be renamed) | `neko_protocol::crew::TOOLS`, `mcp_host/bridge.rs`, `workbench/crew.rs` | Chat, ticket runs, cats |
| 5 | Packs (`pack.toml`) | `neko-core/src/packs.rs`, `packs/` | Tools, skills, UI, routines, in one approval |
| 6 | Luau slots (card, view) | `apps/neko-rune/src/luau/` | Rune only |
| 7 | Web plugins, viewers, fences, pages, settings sections, task tabs | `web/src/{host.ts,registry.ts,plugins.tsx,canvas/viewers.ts}`, `/api/plugins`, `/api/storage` | Web UI and the Home prompt |
| 8 | Themes (files and token contract) | `themes.rs`, `<data>/themes` | All clients |
| 9 | Skills, rulebook, agent profiles | `skills/`, `rulebook.rs`, `agent_profiles.rs` | Every agent prompt |
| 10 | Routine kinds, templates, standing orders | `neko-protocol/src/routines.rs`, `routines.rs` | Routines and packs |
| 11 | Customisation by agent: Change Neko, `custom.css`, the plugin guide, `--open-page`, `ReportPlugin` | `web/src/pages/customisations/`, `docs/design/plugin-guide.md` | The owner's own pages and styles |

Also extension-shaped but not counted: cat faces and files (list edits), the Rune request allow-list (`tests/allow_list.rs`), and the
`Request` enum itself, which is how every feature so far was added.

## Appendix B. DeepSeek sources read

`docs/architecture.md`, `docs/cordis-primer.md`, `docs/capability-seams.md`, `docs/rescope.md`, `docs/cookbook/adding-a-tool.md`,
`docs/cookbook/adding-a-package.md`, `docs/cookbook/adding-a-settings-card.md`, `packages/boot/app-boot/README.md`,
`packages/boot/plugin-manager/README.md`, `vendor/cordis/{README.md,src/registry.ts,src/fiber.ts}`,
`vendor/loader/src/config/isolate.ts` in `deepseek-ai/deepseek-harness` at `2e39e704`; `docs/007-architecture.md`,
`docs/008-cookbook.md`, `cordis.patch.yml`, `package.json` in `deepseek-ai/dsh-superagent-prototype` at `8c32a9dc`.
