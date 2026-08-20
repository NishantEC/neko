//! [`sync_menu_frost`] — wraps the `⌘K` actions menu card so its real,
//! laid-out screen position is pushed to the native menu-overlay material
//! (`material::show_menu_overlay`) every frame it paints.
//!
//! **Paint-time position, not layout-time, and never computed independently
//! here.** `bounds` below is GPUI's own finished screen position for this
//! frame — delegated straight through from the wrapped child
//! (`request_layout` returns the child's own `LayoutId` unmodified, the same
//! shape `edge_fade::ScrollEdgeFade` uses), so this element never duplicates
//! `panel::render_actions_menu`'s own position math. Today that math is a
//! fixed bottom-right offset; if a future task gives the menu real
//! `anchored`/`snap_to_window_with_margin` clamping instead (`AGENTS.md`),
//! this keeps working completely unmodified, since whatever bounds GPUI
//! finishes laying the child out at is exactly what gets pushed to the
//! native view, every frame.
//!
//! **Only ever mounted while the menu is open** (`panel::Root::render`) —
//! there is no "hide" branch inside this element, because there is nothing
//! to hide when it simply isn't in the tree. Hiding the native overlay when
//! the menu closes is `panel::Root`'s own responsibility (every site that
//! clears `actions_menu` calls `material::hide_menu_overlay` — see
//! `Root::close_actions_menu`), not this element's, since a closed menu
//! unmounts this element entirely and it never paints again to say so.

use gpui::{
    AnyElement, App, Bounds, Element, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Window,
};

pub fn sync_menu_frost(child: impl IntoElement) -> MenuFrostSync {
    MenuFrostSync { child: child.into_any_element() }
}

pub struct MenuFrostSync {
    child: AnyElement,
}

impl Element for MenuFrostSync {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
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
    ) -> (LayoutId, ()) {
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
        self.child.paint(window, cx);
        if let Err(e) = crate::material::show_menu_overlay(
            window,
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        ) {
            eprintln!("neko: could not sync the menu frost overlay: {e}");
        }
    }
}

impl IntoElement for MenuFrostSync {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
