# Native Neko client

Goal: replace the GPUI presentation with SwiftUI/AppKit while retaining the Rust daemon and its authorization, storage, worker and MCP boundaries.

Architecture: native/NekoKit contains a framed Unix-socket transport library and native executable. The daemon remains the only database writer. Use macOS 26 Liquid Glass with system-material fallbacks on older systems. Paper's Today, ticket and palette directions remain the information architecture.

- [x] Inventory current surfaces in docs/native-parity.md.
- [ ] Build and test framed transport in Sources/NekoKit.
- [ ] Implement native workspace, chat, tickets, onboarding and folder selection.
- [ ] Implement tools, skills, profiles, responsibilities, schedules and memory.
- [ ] Implement AppKit palette, hotkey, preferences, clipboard and lifecycle.
- [ ] Package with bundled daemon; exercise isolated data, inspect running native UI and independently review.

Acceptance is per docs/native-parity.md. Compilation does not establish UI or live authenticated-provider parity. Preserve the working installed app until replacement verification succeeds.
