# Codex quick attention loop — verification record

## Scope

This delivery adds a local Codex attention surface, not a workspace manager or
third-party work inbox. The daemon supervises one Codex stdio child and keeps a
compact local snapshot for the palette. A task tile, approval row, selected
task quick view, and explicit task-start path all read or control that one
projection. The search path does not parse session files.

History is conditional: the app-server must accept experimental capability
negotiation before a person opening one task can request bounded visible
summaries. The selected task retains at most 40 summaries and the cache holds
one selected task at a time.

## Automated verification

| Check | Result |
| --- | --- |
| `cargo fmt --check` | **Blocked by existing workspace formatting drift** (exit 1). It reported formatting diffs across Rust source outside this documentation-only change; no source was reformatted here. |
| `cargo test --workspace` | **Passed** (exit 0): 409 tests passed, 0 failed, 5 ignored. The harness emitted three existing dead-code warnings and Cargo reported two future-incompatibility warnings. |
| `cargo clippy --all-targets -- -D warnings` | **Blocked by existing dead-code diagnostics** (exit 101): unused Codex reader helper; unused daemon snapshot field; and unused quota-poll constant/functions, including the verification-harness build. No lint was suppressed. |
| Focused clippy: `cargo clippy -p neko-core -p neko-client -p neko-protocol -p neko --all-targets -- -D warnings` | **Passed** (exit 0). Cargo still reported the same two upstream future-incompatibility warnings. |
| `rustfmt --edition 2024 --check crates/neko-daemon/src/bin/verify_harness.rs` | **Blocked by existing formatting drift** in the harness's included `crates/neko-daemon/src/codex.rs`; the harness edit itself was left untouched by the check and no formatter was run. |

## Live verification

An isolated run used the verification-only daemon harness, a staged Neko
client, and a synthetic local stdio server under an empty temporary home. It
exercised the real client, socket protocol, and Codex supervision path without
starting the shipped daemon or its systemwide clipboard-capture loop, reading a
session file, or creating a real Codex task.

| Check | Result |
| --- | --- |
| Initial warmed projection | 1 synthetic Codex task item and 1 approval row were returned by the live palette search. |
| Approval content and controls | The synthetic row carried a nonempty detail and 2 actions. Decline was marked destructive, matching the panel's explicit second-confirmation contract. No approval response was sent. |
| Child stop/degradation | After the exact synthetic app-server child stopped, 1 retained Codex task was marked unavailable, 0 approval rows remained, and application search still returned 5 rows while the launcher process remained alive. |

The native development binary is not an accessibility-discoverable app in the
available desktop-control surface, so its rendered tile, actions menu, and
confirmation dialog could not be safely inspected or clicked. The run did open
the real non-activating Neko window, but no screenshot was captured and no GUI
approval was declined. The existing headless panel test for destructive action
confirmation passed in the workspace suite; that is evidence of the guard,
not a substitute for the unavailable visual check.

No real prompts, task titles, project names, paths, screenshots, tokens, or
task text were recorded; the committed fixture uses only its literal synthetic
values. The disposable local test material was moved to Trash after the run.

## Reproducible, local-only recipe

Run this only from a disposable checkout with the repository binaries already
built. It uses `verify_harness`, whose source deliberately omits the real
daemon's systemwide clipboard-capture loop while starting only the existing
Codex actor against the synthetic fixture. The committed
`docs/evidence/codex-quick-attention-loop-fixture.py` is a synthetic
`codex app-server --stdio`: it contains no real task, project, prompt, token,
or session data and records method names only.

```sh
set -e
cargo build --bin neko --bin verify_harness
original_home="$HOME"
fixture_root="$(mktemp -d /tmp/neko-codex.XXXXXX)"
mkdir -p "$fixture_root/home" "$fixture_root/bin" \
  "$fixture_root/staged-neko" "$fixture_root/no-daemon-path"
cp "$PWD/docs/evidence/codex-quick-attention-loop-fixture.py" \
  "$fixture_root/bin/codex"
chmod +x "$fixture_root/bin/codex"
cp ./target/debug/neko "$fixture_root/staged-neko/neko"
export HOME="$fixture_root/home"
export NEKO_VERIFY_FIXTURE_ROOT="$fixture_root"
export NEKO_CODEX_PATH="$fixture_root/bin/codex"
export NEKO_CODEX_FIXTURE_LOG="$fixture_root/methods.log"
export NEKO_CODEX_FIXTURE_PID_FILE="$fixture_root/codex.pid"
export NEKO_CODEX_FIXTURE_STOP_FILE="$fixture_root/stop"
cleanup() {
  if [ -f "$NEKO_CODEX_FIXTURE_PID_FILE" ]; then
    fixture_pid="$(cat "$NEKO_CODEX_FIXTURE_PID_FILE")"
    fixture_command="$(ps -p "$fixture_pid" -o command= 2>/dev/null || true)"
    case "$fixture_command" in
      *"$fixture_root/bin/codex app-server --stdio"*) kill "$fixture_pid" 2>/dev/null || true ;;
    esac
  fi
  kill "${neko_pid:-}" "${harness_pid:-}" 2>/dev/null || true
  wait "${neko_pid:-}" "${harness_pid:-}" 2>/dev/null || true
  mv "$fixture_root" "$original_home/.Trash/"
}
trap cleanup EXIT
./target/debug/verify_harness >"$fixture_root/harness.out" 2>"$fixture_root/harness.err" &
harness_pid=$!
sleep 1
PATH="$fixture_root/no-daemon-path" NEKO_SHOW_ON_LAUNCH=1 \
  "$fixture_root/staged-neko/neko" >"$fixture_root/neko.out" 2>"$fixture_root/neko.err" &
neko_pid=$!
sleep 1
socket="$HOME/Library/Application Support/neko/neko.sock"
python3 docs/evidence/codex-quick-attention-loop-fixture.py \
  --probe "$socket" --tasks 1 --approvals 1 --unavailable 0 --min-apps 1
grep -Fx 'initialize' "$NEKO_CODEX_FIXTURE_LOG"
grep -Fx 'initialized' "$NEKO_CODEX_FIXTURE_LOG"
grep -Fx 'thread/list' "$NEKO_CODEX_FIXTURE_LOG"
grep -F 'failed to spawn neko-daemon' "$fixture_root/neko.err"
touch "$NEKO_CODEX_FIXTURE_STOP_FILE"
kill "$(cat "$NEKO_CODEX_FIXTURE_PID_FILE")"
sleep 1
python3 docs/evidence/codex-quick-attention-loop-fixture.py \
  --probe "$socket" --tasks 1 --approvals 0 --unavailable 1 --min-apps 1
```

The first probe asserts one synthetic task, one approval, no unavailable task,
and at least one application row. The method log asserts membership of the
three named bootstrap methods without recording parameters; the three `grep`
checks do not assert their order. The second probe, after only
the synthetic child is stopped, asserts that its retained task is visibly
unavailable, its approval has cleared, and application search still works. Do
not send an approval response during this check. The staged client has no
`neko-daemon` sibling and runs with a `PATH` containing no daemon binary; the
required launch failure proves it cannot start the real daemon. The harness is
the only process hosting the socket, and it omits clipboard capture. The short
`/tmp` home keeps the Unix-socket path below macOS's limit. The exit trap stops
only the recorded fixture child, staged client, and harness before moving the
exact disposable directory to Trash.

The committed recipe was rerun for this record with the debug verification
harness and a staged copy of the debug client. Its sanitized outputs were
`tasks=1 approvals=1 unavailable=0 apps=5`, the three expected bootstrap
method names, then `tasks=1 approvals=0 unavailable=1 apps=5` after stopping
only the synthetic child. The staged client reported the expected failure to
resolve `neko-daemon`; no shipped daemon was launched.

## Safety boundary

No external message, pull request, session-file read, or persistent workspace
was created for this verification. A live check may use only a disposable local
task and must leave approvals ungranted; a decline is permitted only through
the UI's explicit destructive confirmation.
