//! A small, bounded `gpui::ImageCache` for the results list's row icons.
//!
//! `data/neko-leak-audit/report.md` §2 alternative #2 / §4 item 3 (firstmate
//! home): GPUI's sprite atlas only reclaims a whole texture once every tile
//! inside it has been individually removed (`MetalAtlas::remove`,
//! `gpui-0.2.2/src/platform/mac/metal_atlas.rs`), and the *only*
//! caller-facing entry point that removes a tile at all is
//! `Window::drop_image`/`image_cache(...)` — neither was ever called
//! anywhere in this crate before this file. Every `img(path)` row icon
//! (`panel::render_row`, `Icon::Image` case) went through GPUI's default,
//! never-evicted per-`App` asset cache instead. The report ranks this
//! below a GPUI window-activation code path as the primary driver of the
//! 20 GB captain-reported growth (that cause is still unconfirmed — see the
//! report) but calls it a real, unbounded hole this crate owns outright and
//! can close cheaply. This is that close.
//!
//! **Why bounded-LRU, not `RetainAllImageCache` cleared at each summon.**
//! The report's own §2 alt #2 establishes neko's row-icon identity space is
//! small and closed — at most ~150 apps plus one shared System Settings
//! icon (`AGENTS.md`'s "Icons" section; file-search and clipboard rows use
//! painted `Glyph`s, never `img()` — confirmed by `render_row`'s own
//! `Icon` match, `panel.rs`). A full `.clear()` on every `Root::reset_for_summon`
//! (a fresh summon, i.e. most real summons) would drop icons that are about
//! to be shown again immediately, forcing every visible row to redecode
//! from disk and reload asynchronously on almost every summon — exactly
//! the kind of eager eviction the task brief warns can reintroduce the
//! "icon doesn't appear until relaunch" defect class `AGENTS.md`'s "Icons"
//! section already fixed twice. A bounded LRU cache never does that in
//! ordinary operation (its capacity, [`ROW_ICON_CACHE_CAPACITY`], sits
//! comfortably above the real identity space), while still giving the
//! crate a real, working `drop_image` call for the first time — closing
//! the report's gap without reintroducing a staleness bug. Eviction is
//! continuous, not tied to a single call site: it happens the moment a
//! genuinely new icon identity is requested while the cache is already at
//! capacity, which includes both `Root::reset_for_summon` (a fresh query
//! can pull in icons for apps the LRU hasn't seen recently) and
//! `Root::refresh_icons` (a `Icon::Placeholder` promoted to `Icon::Image`
//! is, by construction, a resource identity the cache has never loaded
//! before) — see `panel.rs`'s own doc comments on those two methods for
//! why nothing else needs to change there: the cache is installed once, on
//! the results-list container, via `.image_cache(...)`, and every row's
//! `img(path)` element already loads through it on every render.
//!
//! Implementation is adapted from `gpui`'s own shipped
//! `examples/image_gallery.rs::SimpleLruCache` — Apache-2.0, the license
//! `gpui` itself is already vendored under (see `AGENTS.md`'s "The GPUI
//! dependency decision"), not one of this project's GPL-licensed reference
//! apps. Not a byte-for-byte copy: the recency/eviction bookkeeping is
//! split into its own `RecencyOrder` type below specifically so it can be
//! unit-tested directly, without a live GPUI window — see the module doc
//! comment on `RecencyOrder` and the `tests` module for why.

use std::collections::HashMap;
use std::sync::Arc;

use futures::FutureExt;
use gpui::{
    App, AppContext, Asset, AssetLogger, Entity, ImageAssetLoader, ImageCache, ImageCacheError,
    ImageCacheItem, RenderImage, Resource, Window, hash,
};

/// Comfortably above the real, closed row-icon identity space this app
/// renders as a cached raster (`Icon::Image`) — see this module's own doc
/// comment above for the ~150-app-plus-one-shared-icon figure and its
/// source. Headroom over that figure means ordinary operation never evicts
/// a still-relevant icon; the cap still bounds worst-case atlas residency
/// instead of leaving it, as before this task, completely unbounded.
pub(crate) const ROW_ICON_CACHE_CAPACITY: usize = 256;

/// How many decoded conversation images to hold in GPU memory at once.
///
/// `TURN_LIMIT` is 40 turns and only user turns carry pictures, so a window
/// that showed nothing but screenshots would still sit inside this — while
/// keeping the bound far below the row cache's, because each entry here is a
/// full-size image rather than a 128px icon.
pub(crate) const CONVERSATION_IMAGE_CACHE_CAPACITY: usize = 24;

/// Pure recency-order bookkeeping for a bounded cache, deliberately split
/// out of `ImageCache::load` below. `ImageCache::load`'s own `window`
/// parameter (specifically `window.current_view()`) is only valid to call
/// from inside a real GPUI paint pass — calling it from a plain `#[test]`
/// panics (`Window::current_view`'s own doc comment / debug assertion).
/// Driving a real paint pass just to test eviction bookkeeping would mean
/// this task's own unit tests start doing the thing its brief explicitly
/// rules out ("do not put a filesystem check on the summon path... do not
/// launch the client... nothing that renders") in spirit if not in the
/// letter — a headless GPUI window is not the real app, but exercising
/// GPUI's element/paint tree is closer to "rendering" than this task's
/// verification should lean on for a fact as simple as "capacity is
/// respected." Splitting the bound/eviction *policy* out into a plain,
/// `Window`-free type lets that fact be proven with a plain `#[test]`
/// instead. See the `tests` module for exactly what this leaves untested
/// at the real `ImageCache::load` / GPUI-integration level.
#[derive(Debug)]
struct RecencyOrder {
    capacity: usize,
    /// Most-recently-used first.
    order: Vec<u64>,
}

impl RecencyOrder {
    fn new(capacity: usize) -> Self {
        Self { capacity, order: Vec::with_capacity(capacity) }
    }

    /// Marks `key` as most-recently-used. A key that was already tracked
    /// just moves to the front (a cache *hit* never evicts anything). A
    /// genuinely new key that pushes the tracked set past `capacity`
    /// returns the key that should be evicted to make room for it.
    fn touch(&mut self, key: u64) -> Option<u64> {
        if let Some(pos) = self.order.iter().position(|k| *k == key) {
            self.order.remove(pos);
            self.order.insert(0, key);
            return None;
        }
        self.order.insert(0, key);
        if self.order.len() > self.capacity { self.order.pop() } else { None }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.order.len()
    }
}

/// See the module doc comment above for the full "why" — this is the type,
/// `RowIconCache::new` is the constructor wired into `panel::Root::new`.
pub struct RowIconCache {
    recency: RecencyOrder,
    entries: HashMap<u64, ImageCacheItem>,
}

impl RowIconCache {
    /// The real constructor — always `ROW_ICON_CACHE_CAPACITY`. Wired into
    /// `panel::Root::new` the same way `RetainAllImageCache::new`/
    /// `TextField::new` are: `cx.new(...)`'s own `App`-deref reborrow, not
    /// a `Context<Root>` this module needs to know about.
    pub fn new(cx: &mut App) -> Entity<Self> {
        Self::with_capacity(ROW_ICON_CACHE_CAPACITY, cx)
    }

    /// A conversation's own image cache.
    ///
    /// **Deliberately a second instance rather than sharing the row cache**,
    /// which is sized for ~150 distinct 22px app icons. A chat image is
    /// hundreds of times the decoded area, and letting the two compete would
    /// let one screenshot evict a screenful of row artwork — or, the other
    /// way, let a scrolled list evict the picture somebody is looking at.
    /// Small on purpose: only what a transcript window can actually show.
    pub fn for_conversation(cx: &mut App) -> Entity<Self> {
        Self::with_capacity(CONVERSATION_IMAGE_CACHE_CAPACITY, cx)
    }

    /// Only exposed so tests can use a tiny capacity rather than inserting
    /// 256+ entries — never called with anything but
    /// `ROW_ICON_CACHE_CAPACITY` outside of tests.
    pub(crate) fn with_capacity(capacity: usize, cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|_cx| Self {
            recency: RecencyOrder::new(capacity),
            entries: HashMap::with_capacity(capacity),
        });
        // Mirrors `RetainAllImageCache::new`'s own release hook exactly —
        // an image the atlas is still holding for us must be released when
        // this entity itself goes away (process teardown; this cache is a
        // process-lifetime singleton in practice, but the release path
        // exists so that isn't an assumption baked into `Drop` instead).
        cx.observe_release(&entity, |cache, cx| {
            for (_, mut item) in std::mem::take(&mut cache.entries) {
                if let Some(Ok(image)) = item.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        entity
    }

    /// Number of distinct icon identities currently retained — test-only
    /// (also handy from a debugger), not part of the `ImageCache` contract.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        debug_assert_eq!(self.recency.len(), self.entries.len(), "recency order and entries must stay in sync");
        self.entries.len()
    }

    /// Whether `resource` has resolved (successfully or not — either way
    /// its background load finished) — test-only. Deliberately does not
    /// need `Window`/`App`: `ImageCacheItem::get` itself needs neither, so
    /// this can be called from a plain entity `update` in a test, unlike
    /// `ImageCache::load` itself.
    #[cfg(test)]
    pub(crate) fn is_resolved(&mut self, resource: &Resource) -> Option<bool> {
        self.entries.get_mut(&hash(resource)).map(|item| item.get().is_some())
    }
}

impl ImageCache for RowIconCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);

        if let Some(item) = self.entries.get_mut(&key) {
            self.recency.touch(key);
            return item.get();
        }

        let fut = AssetLogger::<ImageAssetLoader>::load(resource.clone(), cx);
        let task = cx.background_executor().spawn(fut).shared();

        if let Some(evicted_key) = self.recency.touch(key)
            && let Some(mut evicted) = self.entries.remove(&evicted_key)
            && let Some(Ok(image)) = evicted.get()
        {
            // The only `drop_image` call anywhere in this crate — see the
            // module doc comment for why that used to be zero.
            cx.drop_image(image, Some(window));
        }

        self.entries.insert(key, ImageCacheItem::Loading(task.clone()));

        let entity = window.current_view();
        window
            .spawn(cx, {
                async move |cx| {
                    _ = task.await;
                    cx.on_next_frame(move |_, cx| {
                        cx.notify(entity);
                    });
                }
            })
            .detach();

        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gpui::{Context, IntoElement, ParentElement, Render, TestAppContext, div, img};

    use super::*;

    // --- Pure bookkeeping: no GPUI window needed at all -------------------

    #[test]
    fn a_new_key_is_tracked_until_capacity_is_exceeded() {
        let mut order = RecencyOrder::new(3);
        assert_eq!(order.touch(1), None);
        assert_eq!(order.touch(2), None);
        assert_eq!(order.touch(3), None);
        assert_eq!(order.len(), 3, "exactly at capacity, nothing evicted yet");
    }

    #[test]
    fn a_new_key_past_capacity_evicts_the_least_recently_used_one() {
        let mut order = RecencyOrder::new(2);
        order.touch(1);
        order.touch(2);
        // 1 is now the least-recently-used of the two.
        assert_eq!(order.touch(3), Some(1), "the oldest untouched key must be the one evicted");
        assert_eq!(order.len(), 2, "the bound must never be exceeded, no matter how many distinct keys are requested");
    }

    #[test]
    fn re_touching_an_existing_key_never_evicts() {
        let mut order = RecencyOrder::new(2);
        order.touch(1);
        order.touch(2);
        // Re-touching 1 (a cache hit) must not evict anything, and must
        // protect 1 from the next real eviction.
        assert_eq!(order.touch(1), None);
        assert_eq!(order.touch(3), Some(2), "1 was refreshed by the re-touch above, so 2 (now the oldest) is evicted instead");
    }

    #[test]
    fn many_more_distinct_keys_than_capacity_still_never_exceed_the_bound() {
        let mut order = RecencyOrder::new(4);
        for key in 0..64u64 {
            order.touch(key);
            assert!(order.len() <= 4, "capacity must hold under sustained pressure, not just for one insert");
        }
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn an_evicted_key_can_be_tracked_again_without_getting_stuck() {
        let mut order = RecencyOrder::new(2);
        order.touch(1);
        order.touch(2);
        assert_eq!(order.touch(3), Some(1), "1 gets evicted to make room for 3");
        // Re-requesting 1 (e.g. a row that scrolled back into view) must
        // succeed as an ordinary new key, not be treated as already
        // present or otherwise wedged.
        assert_eq!(order.touch(1), Some(2), "1 is genuinely new again, so it evicts the current oldest (2)");
        assert_eq!(order.len(), 2);
    }

    // --- Real `ImageCache::load`, through an actual (headless, no OS
    // window, no screen pixels — `gpui::TestAppContext`'s own test
    // platform) GPUI paint pass. This is the one part of this module that
    // cannot be exercised without touching GPUI's paint pipeline at all —
    // see `RecencyOrder`'s own doc comment for why the bound/eviction
    // policy itself is proven above instead, without this. What this test
    // adds on top: proof that `RowIconCache`'s actual `window.current_view()`
    // + `window.spawn(...)` + `cx.drop_image(...)` wiring — the part that
    // cannot be pure-Rust-tested — doesn't panic and does resolve a
    // resource's load to completion. Never opens a real OS window and never
    // renders a single pixel to the captain's screen: `TestAppContext`'s
    // windows are backed by `TestWindow`/a test platform, the same
    // mechanism `text_field.rs`'s own `#[gpui::test]`s already use in this
    // crate.

    struct ProbeView {
        cache: Entity<RowIconCache>,
        resources: Vec<PathBuf>,
    }

    impl Render for ProbeView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let mut container = div().image_cache(self.cache.clone());
            for path in &self.resources {
                container = container.child(img(path.clone()));
            }
            container
        }
    }

    fn nonexistent_path(label: &str) -> PathBuf {
        // Doesn't need to exist: this proves the cache's own load/eviction
        // plumbing, not that decoding real PNG bytes succeeds (GPUI's own
        // already-relied-upon `ImageAssetLoader`, not this module) — a
        // load that resolves to `Err` (file not found) still proves the
        // resource resolved instead of hanging, which is exactly "a
        // late-arriving icon still renders" at this layer: the daemon, not
        // this cache, is what guarantees a real row's path exists on disk
        // (`AppsProvider::search`, see `AGENTS.md`'s "Icons" section).
        PathBuf::from(format!("/tmp/neko-row-icon-cache-test-{label}-{}.png", std::process::id()))
    }

    #[gpui::test]
    fn a_resource_requested_through_a_real_paint_pass_resolves_after_background_work_completes(
        cx: &mut TestAppContext,
    ) {
        let cache = cx.update(|cx| RowIconCache::with_capacity(8, cx));
        let resource = nonexistent_path("late");
        let view = cx.add_window(|_window, _cx| ProbeView {
            cache: cache.clone(),
            resources: vec![resource.clone()],
        });
        let _ = view;

        cx.run_until_parked();

        let resolved = cache.update(cx, |cache, _cx| {
            cache.is_resolved(&Resource::from(resource))
        });
        assert_eq!(
            resolved,
            Some(true),
            "a resource painted once must resolve (successfully or not) once its background load \
             completes — it must never be stuck permanently unresolved"
        );
    }

    #[gpui::test]
    fn a_single_paint_pass_over_more_resources_than_capacity_still_respects_the_bound(
        cx: &mut TestAppContext,
    ) {
        let cache = cx.update(|cx| RowIconCache::with_capacity(3, cx));
        let resources: Vec<PathBuf> =
            (0..8).map(|n| nonexistent_path(&format!("bound-{n}"))).collect();
        let view = cx.add_window(|_window, _cx| ProbeView { cache: cache.clone(), resources });
        let _ = view;

        cx.run_until_parked();

        cache.update(cx, |cache, _cx| {
            assert_eq!(
                cache.len(),
                3,
                "8 distinct icons painted in one pass against a capacity of 3 must never retain more than 3"
            );
        });
    }
}
