# Neko design principles

The native client (`native/NekoKit`) follows these rules. They decide what a
new screen looks like before anyone draws it, and the tests in
`Tests/NekoNativeTests/DesignSystemGuardTests.swift` keep the mechanical ones
true. The design system itself lives in `Sources/NekoNative/DesignSystem/`.

Learned partly from FluidVoice (GPL-3.0, read for patterns only) and from
neko's own history in `AGENTS.md`. Where the two disagree, neko's measured
findings win.

## 1. Screens compose shells; they do not draw chrome

A page is a `NekoPage`, a modal is a `NekoSheet`, a titled group is a
`NekoSection`, a label-and-control line is a `NekoRow`, and "nothing here" is
a `NekoEmptyState`. A screen supplies content and actions. It never sets its
own outer padding, width, background, title font or header layout.

**Change the shell to change every screen.** If a screen needs something the
shell cannot express, extend the shell for everyone rather than adding a
local variant. A copy of a shell's padding or header inside a screen is a bug
waiting to drift.

## 2. One owner per visual decision

Every radius, font size, spacing step, colour and animation comes from a
token in `DesignSystem/Tokens.swift` or `DesignSystem/Motion.swift`:

| Decision | Token | Enforced |
| --- | --- | --- |
| Corner radius | `NekoRadius` | yes, no literals outside the design system |
| Type | `NekoFont` | yes, no `.system(size:)` outside the design system |
| Colour | `N` (surfaces, text, lines), `NekoStyle` (status) | yes, no hex or OKLCH literals outside the design system (`Activity/` owns its glyph hues) |
| Animation | `NekoMotion` | yes, no literal curves outside the design system |
| Button role | `.nekoButton(_:)` | yes, no raw `.bordered`/`.borderedProminent`/`.borderless` |
| Spacing | `NekoSpace`, `NekoLayout` | ratchet: the count of raw padding/spacing numbers may only fall |

A token used once is a literal with extra steps. Add a token when a second
screen needs the same value, not before.

## 3. Native first

Sidebar, toolbar, menus, segmented pickers, forms and sheets are AppKit/SwiftUI
controls and keep their platform materials and behaviour. Neko styles the
content area, not the window chrome. Toolbars keep `.automatic` button styling.

## 4. Glass is for controls; content is opaque

Liquid Glass (macOS 26) and materials are reserved for floating controls: the
composer, toasts, the quick panel. Reading surfaces stay opaque on a neutral
canvas. Every glass surface falls back to an opaque surface with a visible
border under **Reduce Transparency** or **Increased Contrast**. `liquidGlass`
does this itself, so no caller has to remember.

## 5. Colour means state

Chrome is neutral grey. Colour appears only where it carries meaning: amber
needs you (approval, the sidebar badge), sky is ready to review, mint is done,
coral failed, and Iris (`NekoStyle.accent`) is the primary action, selection
and focus. One meaning per colour, and never colour alone; every coloured
state also has text or a symbol.

Content text uses `N.text`, `N.text2` and `N.text3`. These are tuned to stay
readable on the brightest surface. System `.secondary` is for native chrome
(lists, menus, toolbars), where vibrancy should apply.

## 6. Motion confirms; it never decorates

Animations are short (`NekoMotion.hover` 120ms, `select` a 240ms spring,
`exit` 150ms), have a
cause the user just made, and go through `.nekoAnimation(_:value:)` or
`withNekoAnimation`. Both return no animation under **Reduce Motion**.
Nothing scales on hover, since a layout that moves under the pointer is
harder to click. Repeating animation is limited to "something is running"
indicators and stops under Reduce Motion.

## 7. Every empty list says why

An empty list states which of three things is true: nothing exists yet (and
how to make one), nothing matches (and what was searched), or it could not
load (and how to retry). `NekoEmptyState` has one case for each. A loading
state never claims emptiness.

## 8. Small files, typed state

A view file over about 400 lines should be split by section. State that
describes a whole screen (which page, which step) is an enum, not a string.
Logic that decides what a screen shows lives in a plain value type with a
unit test, so it can be checked without a window.

## 9. Proof is a picture or a test

A visual change ships with either a unit test of its presentation logic or an
offscreen render (`scripts/render-previews.sh`) checked against the result.
Renders happen offscreen, so no window ever takes keyboard focus. A compile is
not proof that a font, colour or material actually appeared.

Two things an offscreen render cannot show, so do not read them as defects:
prominent buttons and switches draw grey (the window is never active), and
glass draws nothing (there is no desktop behind it). Check those in the app.

## 10. One gate before main

`scripts/check.sh` runs before every install and, in `--quick` form, before
every commit that touches Rust or Swift. Nothing reaches `/Applications`
untested.

## Component catalogue

| Component | Use for |
| --- | --- |
| `NekoPage` | Every management page: title, message, toolbar actions, bounded scroll column |
| `NekoSection` | A titled group inside a page, with an optional status dot, detail and trailing actions |
| `NekoRow` | A label (plus optional detail) with a control on the right |
| `NekoSheet` | Every modal: header, scrolling body, footer actions, fixed width |
| `NekoEmptyState` | Empty, no-matches and failed-to-load states, inline or as a block |
| `NekoNotice` | The top-of-window notice and error toasts |
| `NekoToggleRow` | An on/off setting with its explanation |
| `NekoDividedList`, `NekoItemFooter` | Items separated by dividers; an item's meta line and actions |
| `NekoReadMore`, `NekoSavedText` | Saved text shown inert, folded when long |
| `NekoBadge` | How many things need you, on a sidebar row or tab (amber) |
| `StatusGlyph` | Ticket status ring |
| `NekoIconTile` | A neutral symbol tile for setting rows and empty states; tint it only to mean a state |
| `.nekoCard(...)`, `NekoRaisedSurface` | An opaque raised surface: soft shadow in light, lit top edge in dark, strong border under Increased Contrast |
| `.nekoElevation(.floating)` | The shadow for menus, popovers and previews over other content |
| `.nekoEntrance()` | How a toast arrives and leaves; a cross-fade under Reduce Motion |
| `.liquidGlass(...)` | A floating control surface, with accessibility fallbacks built in |
| `.nekoButton(_:)` | `.primary`, `.secondary`, `.quiet`, `.destructive` |
