# Paper-native workspace refinement

Status: approved design source already exists in Paper file `neko` (Today,
Ticket open, Quick panel, Tools by scope, and the onboarding arc). This note
records the implementation decisions needed to bring the native app back to
that source rather than creating a second visual language.

## Intent

Neko is a calm, always-available personal-agent workspace. It is not a form
builder. The main window is a conversation with a compact workspace switcher,
attention-aware ticket cards, and a narrow operational rail. The command panel
remains the fast path; the full window is where a person understands and steers
work.

## Workspace creation

Selecting `+` opens a compact creation state in the main content area. It asks
only for a workspace name and its local repository folder. Workspace
instructions are available behind `Add instructions`, so a first workspace can
be created without facing an editor dump. The app derives a suggested name from
the chosen path when the user leaves the name empty. Errors name the missing
field in plain language; no colour-only errors.

The detailed workspace editor remains available after creation through the
workspace's Settings affordance. It is a maintenance screen, not the first
run experience.

## Visual and interaction contract

- Match Paper's neutral black/glass palette: one low-contrast surface family,
  subtle white hairlines, 14px cards, and a 236px sidebar / 320px rail.
- Today has one readable 640px conversation column. Ticket rows are compact
  decision cards; a selected ticket opens in the right-side detail pane.
- The sidebar labels are Today, Tickets, Agents, Responsibilities, Tools &
  skills. Secondary configuration is reachable but does not compete with work.
- Reveal transitions use the existing 150ms content fade. Hover and selection
  feedback must stay interruptible. Reduced Motion keeps state changes but
  removes positional travel.
- Every action remains keyboard reachable (Tab + Enter/Space), has a visible
  focus treatment, and is at least a 24px target. Inputs retain visible labels;
  errors include text.

## Out of scope

No new service integration, MCP protocol change, or unattended external write
is implied by this visual refinement. The generic MCP boundary remains intact.
