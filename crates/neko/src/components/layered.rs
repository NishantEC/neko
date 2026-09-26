//! `Window::paint_layer` discipline for floating chrome — comet craft study
//! recommendation 3 (`data/neko-comet-design/report.md`, firstmate home).
//! comet's `crates/ui/src/frost.rs` wraps a popover/dialog card's entire
//! paint (blur, tint, border, rows, text) in one `window.paint_layer(bounds,
//! |window| {...})` call, and its own top comment states why: with
//! per-primitive bounds-tree ordering, a hover repaint elsewhere can
//! reassign the card's own quads to paint in the wrong relative order —
//! washes, dividers, and borders intermittently rendering behind content
//! that should sit on top of them. Inside one `paint_layer`, the whole
//! subtree's internal stacking order is structural, not incidental to
//! whatever else painted this frame.
//!
//! **The rule this file exists to make easy to follow, stated once here
//! rather than re-derived at each call site**: any future floating/overlaid
//! chrome that needs to guarantee its own internal paint order (a tint under
//! content, a close button over a thumbnail, anything layered) should wrap
//! its content in [`layered`] rather than relying on sibling `.child()`
//! order being stable. Cheap — `Window::paint_layer` is in published
//! `gpui-0.2.2` (confirmed by reading `gpui-0.2.2/src/window.rs:2782`) — and
//! it closes off a class of bug this codebase hasn't hit yet but is one new
//! overlay away from being able to (see `panel::Root::render_actions_menu`,
//! this task's own floating chrome, for the first real call site).
//!
//! **Honest scope**: this is a `gpui` scene-graph-level guarantee. It would
//! not have prevented either of the two real floating-chrome-adjacent
//! defects this repo already fixed (`AGENTS.md`, "The double-panel shadow
//! defect" — AppKit's own window drop shadow, a level below `gpui`'s scene
//! graph entirely; "Mode view resize seam" — a `gpui`-internal
//! `viewport_size` cache going stale, not a paint-order issue). It guards a
//! different, not-yet-hit failure mode: multiple paint primitives inside one
//! floating card silently reordering relative to each other.
//!
//! Reimplemented from scratch against the published `gpui = "0.2.2"` crate —
//! comet's own `Layered` (`frost.rs:116-172`) was read for the *shape* (a
//! thin `Element` wrapper whose `paint` delegates to `window.paint_layer`),
//! not copied; this module has no backdrop-blur concept at all (comet's
//! `Frosted`/`frost.rs`, the file this pattern lives alongside there, needs
//! `paint_backdrop_blur` — a `gpui` fork-only primitive, confirmed absent
//! from published `gpui-0.2.2` — which is why neko has no equivalent file).

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};

/// Wraps `child` so its whole subtree paints inside one
/// `Window::paint_layer` call — see this module's own doc comment for why.
/// Layout-transparent: forwards the child's own `request_layout`/`prepaint`
/// untouched, so wrapping an element in this never changes how it's sized or
/// positioned by its parent.
pub struct Layered {
    child: AnyElement,
}

/// Wrap `child` in one atomic paint layer — see this module's own doc
/// comment.
pub fn layered(child: impl IntoElement) -> Layered {
    Layered {
        child: child.into_any_element(),
    }
}

impl IntoElement for Layered {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Layered {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.paint_layer(bounds, |window| {
            self.child.paint(window, cx);
        });
    }
}
