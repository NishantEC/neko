# Native polish verification

Date: 2026-10-05. Reference: [20A · Complete chat — conversation first](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-1-0). Design contract: [native-polish.md](../design/native-polish.md).

## Delivered build

The installed `/Applications/Neko.app` is the signed SwiftUI/AppKit client built from `e6fbc501f57382750d8af8eaa3becc78fed4373d` on local `main`. The documentation commit following it does not change application code. No remote push was performed for this polish pass.

Implementation commits:

- `e5a90c2`: shared native chrome, chat/composer, agents, management pages, Settings and onboarding.
- `128e72e`: specific accessibility labels and correct empty-tool connection status.
- `5c49c02`: trailing-aligned Memory switches with explicit names and explanatory hints.
- `e6fbc50`: compact skill suggestions with expandable, verbatim review content and preserved acceptance controls.

`codesign --verify --deep --strict /Applications/Neko.app` passed. Installed and built client executable SHA-256 values matched:

```text
b9c249854554f2b5d1d8b129aa9a3fd1cfe82be18798477156c9372ad9ecfc1f
```

The final process check found one client and one daemon, both under `/Applications/Neko.app/Contents/MacOS`. Before the installer stopped them, the daemon snapshot contained 69 tickets, zero Planning/Building/Reviewing tickets, zero Queued tickets, and zero pending or queued Home messages. User data, credentials and preferences were preserved. Installer backups remain recoverable; they are not running instances.

## Automated checks

Final command:

```sh
swift test --package-path /Users/nish/Documents/neko/native/NekoKit
```

Result: 98 XCTest cases passed with zero failures; 11 Swift Testing tests passed. The optional `realDaemonPersistsNativeProtocolCommands` test was skipped because `NEKO_TEST_DAEMON` was not set. Draft scope, stale submission completion, attachments, chat scroll following, command parsing, evidence presentation and protocol tests remain in the suite. Two source-string tests enforcing the retired glass composer treatment were removed; they did not exercise send or draft behavior.

The signed release build passed using `scripts/build-native.sh`, followed by an idle-gated `scripts/install-native.sh --skip-build`. `git diff --check` passed. Local logs:

- `/tmp/neko-polish-skills-swift.log`
- `/tmp/neko-polish-final-build.log`
- `/tmp/neko-polish-final-install.log`

The inherited baseline verification reported core 646 passing with `--test-threads=2` and daemon 158 passing. Core's default-parallel timing fixture had failed before the bounded-parallel rerun. Rust suites were not rerun for this SwiftUI-only change; those baseline results are not new daemon verification.

## Installed screen and interaction checks

The native UI tool captured screenshots and accessibility trees in the Codex task. These are inline task evidence, not image files checked into this repository. Most workspace captures were 2224 × 1608 pixels (1112 × 804 points). The quick panel capture was 1520 × 1000 pixels. Checks used AX clicks and native keyboard input.

| Surface | Observed result |
| --- | --- |
| Home | Populated transcript, distinct speakers, readable lists, agent links and compact empty composer. Final app left here in All workspaces with Send disabled. |
| Agent chat | Opened the existing completed NEK-7E33 conversation. Result, reviewer content and evidence disclosure were reachable. Its saved command summary exposed 29 commands, including 1 failed, 10 incomplete and 2 shortened. This is presentation evidence, not a new verification of that ticket's fix. |
| Agent draft | Entered an unsent test draft, navigated away, reopened the conversation and verified it survived. Cleared the test text without sending. |
| All agents | Board and List switched through native controls. An unmatched search showed the empty state; Clear search restored rows. |
| Workspaces | Page and Settings sheet captured. Scrolled to full instructions and cancelled without saving. |
| Watching | Active watches appeared before suggestions. Expanded status, sources and result details were available. No watch was activated or changed. |
| Schedules | Empty state and New schedule editor captured. Cancelled without creating a schedule. |
| Tools | Connection list, existing connection details and tool search captured. Searching `get_user` narrowed the tool list to one. Failed zero-tool connections correctly showed Needs attention / No tools. No connection was added or granted access. |
| Skills | All-workspaces guidance and workspace empty-skill state captured. On the final build, the existing suggestion started collapsed. Expanded it, inspected the full selectable instructions, and scrolled to the visible Accept and Reject controls without invoking either. |
| Memory | Saved memories and suggestions captured. Final screenshot shows both settings switches on one trailing edge; AX reports Suggest new memories and Use saved memory. No memory settings or records changed. |
| Working style | Expanded recent decision details and inspected their saved source text. |
| Profiles | Page and Edit Profile sheet captured; cancelled without saving. |
| Settings | All seven tabs captured during the pass: General, AI, Search, Permissions, Diagnostics, Agents and About. Diagnostics progressed from empty to populated after its local check. Final build reconfirmed explicit General switch names and About build `e6fbc50`. No settings toggles changed. |
| Quick panel | Opened from the sidebar and captured populated results on the final build. A subsequent search-input attempt was stopped by the tool's window-change guard, so final-build query entry was not verified. |

## Limits and retained behavior

Onboarding received source changes and compiled, but the installed onboarding flow was not reopened by resetting user setup. MCP registry, import and manual connection flows were reviewed in source and compiled; this pass did not connect a new provider. Live model streaming, interruption, queued replies, provider compaction and IME composition were not exercised against a model.

Physical mouse hit testing, divider dragging, minimum-width resizing, a full VoiceOver walkthrough, alternate themes and Reduce Motion were not established by these captures. A coordinate click returned `noWindowsAvailable`; AX success is not equivalent to physical mouse proof. Native `NavigationSplitView`, toolbar/safe-area handling, `stableSplitPane` and custom `SidePanel` were retained.

The screen helper disconnected during QA, including after the Mac had been locked. Neko and its daemon remained running. Earlier diagnostics identified a `SkyComputerUseService` failure; reopening the installed app after the already-planned idle-gated installation restored screen access. No Neko crash was observed during the successful checks, but this does not close the physical divider-drag check.

Management text stays selectable and verbatim; it cannot turn saved source content into executable chat actions. Imported agent goals are labelled Task brief, and saved results do not borrow a changing ticket timestamp. Skill review continues to show the exact instructions being accepted, with source/audit information and unchanged review requirements. This work changes presentation, not task authority or publication policy.
