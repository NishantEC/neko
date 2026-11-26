//! Every scene, at every window shape, in light and dark: nothing cut off that is not meant to be, no two controls on top of each
//! other, the header band on the traffic lights' centre line, Figtree everywhere, and the sidebar folded at 760.

mod support;
use support::*;

/// The traffic lights' centre line: Rune centres the 14 point lights in the 52 point band (y 19).
const LINE: f32 = 26.0;

#[test]
fn every_scene_holds_the_structure_rules() {
    for &(name, scene) in scenes() {
        for size in SIZES {
            for dark in [false, true] {
                let rt = runtime(size, dark, scene);
                let at = format!("{name} at {}x{} {}", size.0, size.1, if dark { "dark" } else { "light" });
                let els = rt.elements();
                for e in &els {
                    let [x, _, w, _] = e.rect;
                    if e.opacity > 0.0 && w > 0.0 {
                        assert!(x >= -0.5 && x + w <= size.0 + 0.5, "{at}: {:?} runs out of the window: {:?}", e.key, e.rect);
                    }
                    if e.text_cut {
                        assert!(e.truncate, "{at}: text cut off without an ellipsis: {:?} {:?}", e.key, e.text);
                    }
                    if let (Some(text), Some(style)) = (&e.text, &e.text_style)
                        && !text.trim().is_empty()
                    {
                        let family = style.family.clone().unwrap_or_default();
                        assert!(family.to_lowercase().contains("figtree"), "{at}: {text:?} is set in {family:?}, not Figtree");
                    }
                }
                // Controls never sit on one another (a hit area and the control it holds are one).
                let controls: Vec<_> = els.iter().filter(|e| e.focusable && e.rect[2] > 0.0 && e.rect[3] > 0.0).collect();
                for (i, a) in controls.iter().enumerate() {
                    for b in &controls[i + 1..] {
                        if a.path.starts_with(&b.path) || b.path.starts_with(&a.path) {
                            continue;
                        }
                        let [ax, ay, aw, ah] = a.rect;
                        let [bx, by, bw, bh] = b.rect;
                        let overlap = (ax + aw).min(bx + bw) - ax.max(bx) > 0.5 && (ay + ah).min(by + bh) - ay.max(by) > 0.5;
                        assert!(!overlap, "{at}: {:?} and {:?} overlap", a.key, b.key);
                    }
                }
                // What sits in the band sits on the lights' centre line.
                for e in els.iter().filter(|e| matches!(e.key.as_deref(), Some("sidebar-hide" | "sidebar-show" | "band-title"))) {
                    let mid = e.rect[1] + e.rect[3] / 2.0;
                    assert!((mid - LINE).abs() <= 1.0, "{at}: {:?} is centred at {mid}, not on the band's line {LINE}", e.key);
                }
                let has = |k: &str| els.iter().any(|e| e.key.as_deref() == Some(k));
                if size.0 < 1000.0 {
                    assert!(!has("sidebar") && has("sidebar-show"), "{at}: the sidebar folds under 1000 and its button shows");
                    let show = els.iter().find(|e| e.key.as_deref() == Some("sidebar-show")).unwrap();
                    assert!(show.rect[0] >= 16.0 + 3.0 * 14.0, "{at}: the Show sidebar button is clear of the traffic lights: {:?}", show.rect);
                } else {
                    assert!(has("sidebar") && has("sidebar-hide"), "{at}: wide windows show the sidebar");
                }
            }
        }
    }
}

#[test]
fn light_and_dark_follow_the_system_live() {
    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    let page = |rt: &rune::Runtime<neko_app::state::App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(neko_app::view::ROOT)).unwrap().fill;
    let light = page(&rt);
    assert!(!neko_app::view::LOOK.with(|l| l.get()).0);
    rt.set_system_dark(true);
    settle(&mut rt);
    assert_ne!(page(&rt), light, "the page changes with the system's appearance, without a restart");
    assert!(neko_app::view::LOOK.with(|l| l.get()).0);
    rt.set_system_reduced_motion(true);
    settle(&mut rt);
    assert!(neko_app::view::LOOK.with(|l| l.get()).1, "Reduce Motion reaches rune-ui's theme");
}

#[test]
fn the_window_title_is_the_open_task_or_neko() {
    let rt = runtime((1200.0, 800.0), false, |_| {});
    assert_eq!(rt.state.window_title(), "Neko");
    let rt = runtime((1200.0, 800.0), false, neko_app::scenes::SCENES.iter().find(|s| s.0 == "task-open").unwrap().1);
    assert_eq!(rt.state.window_title(), "Rename the Settings button to Preferences across the app");
}
