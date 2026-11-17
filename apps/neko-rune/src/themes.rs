//! The themes the person can pick (Settings, Appearance) and how Rune paints one (parity batch D).
//!
//! The web UI's themes are data: the built-ins are `web/src/themes/builtin/*.json` and the person's own are the files
//! `ListThemes` returns. `web/src/themes/derive.js` (`resolveColors`) turns one into the colours the app reads, and writes
//! them as CSS variables (`builtin.generated.css`). This module reads the same JSON, ports the same derivation and maps the
//! result onto Rune's token table ([`crate::theme::Hex`]): the web's `--background`, `--card`, `--popover`, `--sidebar`,
//! `--foreground`, `--strong`, `--muted-foreground`, `--border`, `--code`, `--primary`, `--warning`, `--info`, `--success`
//! and `--destructive`. A test parses the generated CSS (the one the token test of `theme.rs` parses) and requires every
//! built-in to resolve to exactly those values, so the port cannot drift from the web.
//!
//! A theme with a `backdrop` (Glass) is frosted in the web. Rune paints its solid colours: it has no blur behind a window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::Value;

use crate::theme::{Hex, Neko};

/// The theme ids the web uses when none was chosen (`DEFAULT_LIGHT`, `DEFAULT_DARK` in `theme.ts`).
pub const DEFAULT_LIGHT: &str = "neutral-light";
pub const DEFAULT_DARK: &str = "neutral";

/// The order the built-ins are listed in (`ORDER` in `theme.ts`).
const ORDER: [&str; 12] = ["neutral-light", "catppuccin-latte", "gruvbox-light", "solarized-light", "neutral", "glass", "catppuccin-mocha", "gruvbox-dark", "solarized-dark", "nord", "tokyo-night", "rose-pine"];

const BUILTIN: [&str; 12] = [
    include_str!("../../../web/src/themes/builtin/catppuccin-latte.json"),
    include_str!("../../../web/src/themes/builtin/catppuccin-mocha.json"),
    include_str!("../../../web/src/themes/builtin/glass.json"),
    include_str!("../../../web/src/themes/builtin/gruvbox-dark.json"),
    include_str!("../../../web/src/themes/builtin/gruvbox-light.json"),
    include_str!("../../../web/src/themes/builtin/neutral-light.json"),
    include_str!("../../../web/src/themes/builtin/neutral.json"),
    include_str!("../../../web/src/themes/builtin/nord.json"),
    include_str!("../../../web/src/themes/builtin/rose-pine.json"),
    include_str!("../../../web/src/themes/builtin/solarized-dark.json"),
    include_str!("../../../web/src/themes/builtin/solarized-light.json"),
    include_str!("../../../web/src/themes/builtin/tokyo-night.json"),
];

// ---- colour maths (derive.js) ------------------------------------------------------------------

type Rgb = [f64; 3];

/// "#abc", "#aabbcc" or "#aabbccdd" as [r, g, b, a] (a is 0 to 1).
pub fn parse_hex(text: &str) -> Option<[f64; 4]> {
    let h = text.strip_prefix('#')?;
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let long: String = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => h.to_owned(),
        _ => return None,
    };
    let n = |i: usize| u8::from_str_radix(&long[i..i + 2], 16).map(f64::from).ok();
    Some([n(0)?, n(2)?, n(4)?, if long.len() == 8 { n(6)? / 255.0 } else { 1.0 }])
}

fn byte(x: f64) -> u8 {
    x.round().clamp(0.0, 255.0) as u8
}

fn to_hex([r, g, b]: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

fn rgb(hex: &str) -> Rgb {
    let [r, g, b, _] = parse_hex(hex).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    [r, g, b]
}

fn mix(from: &str, to: &str, t: f64) -> String {
    let (a, b) = (rgb(from), rgb(to));
    to_hex([0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t))
}

fn luminance(hex: &str) -> f64 {
    let [r, g, b] = rgb(hex).map(|v| v / 255.0).map(|v| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn over(color: &str, base: &str) -> String {
    let [r, g, b, a] = parse_hex(color).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let bb = rgb(base);
    to_hex([r * a + bb[0] * (1.0 - a), g * a + bb[1] * (1.0 - a), b * a + bb[2] * (1.0 - a)])
}

/// The WCAG contrast ratio of two opaque colours.
pub fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Moves `color` towards white or black until it clears `min` on every surface (the policy: move the text, never the surface).
fn ensure(color: &str, surfaces: &[&str], min: f64) -> String {
    let light = surfaces.iter().all(|s| luminance(s) > 0.4);
    let target = if light { "#000000" } else { "#ffffff" };
    let mut step = 0;
    // The web steps a float by 0.02 up to 1.0001: 51 steps.
    while step <= 50 {
        let c = mix(color, target, f64::from(step) * 0.02);
        if surfaces.iter().all(|s| contrast(&c, s) >= min) {
            return c;
        }
        step += 1;
    }
    target.to_owned()
}

fn best_on(surface: &str, candidates: &[&str]) -> String {
    candidates.iter().find(|c| contrast(c, surface) >= 7.0).map(|c| (*c).to_owned()).unwrap_or_else(|| {
        candidates.iter().copied().fold(candidates[0], |best, c| if contrast(c, surface) > contrast(best, surface) { c } else { best }).to_owned()
    })
}

/// The colours the app reads, resolved (`resolveColors`): all opaque hex.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub dark: bool,
    pub background: String,
    pub card: String,
    pub popover: String,
    pub sidebar: String,
    pub foreground: String,
    pub strong: String,
    pub muted: String,
    pub primary: String,
    pub primary_foreground: String,
    pub destructive: String,
    pub success: String,
    pub warning: String,
    pub info: String,
    pub border: String,
    pub code: String,
    /// The border's opacity (`--border`): 0.07 in dark, 0.08 in light, 0.14 with a backdrop.
    pub border_alpha: f32,
    /// What the web's contrast rule measures besides the text greys (`contrastPairs` in `derive.js`).
    pub sidebar_foreground: String,
    pub bubble: String,
    pub bubble_foreground: String,
    pub status_foregrounds: [String; 4],
    /// The surfaces as they look (with a backdrop: the translucent fill over the brightest part of the wash).
    pub effective: [String; 4],
}

const BACKDROP_ALPHA: [(&str, f64); 5] = [("background", 0.42), ("card", 0.48), ("popover", 0.78), ("sidebar", 0.46), ("code", 0.62)];

fn alpha_hex(color: &str, alpha: f64) -> String {
    let [r, g, b, _] = parse_hex(color).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    format!("#{:02x}{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b), byte(alpha * 255.0))
}

/// What the surfaces look like with a backdrop: their translucent fill over the brightest part of the wash.
fn effective(backdrop: &Value, solid: [&str; 4]) -> [String; 4] {
    let first = backdrop["base"][0].as_str().unwrap_or("#000000");
    let base: String = first.chars().take(7).collect();
    let mut wash: Vec<String> = backdrop["base"].as_array().map(|b| b.iter().filter_map(|c| c.as_str()).map(|c| c.chars().take(7).collect()).collect()).unwrap_or_default();
    for g in backdrop["glows"].as_array().into_iter().flatten() {
        let color = g["color"].as_str().unwrap_or("#000000");
        wash.push(over(&alpha_hex(color, g["alpha"].as_f64().unwrap_or(0.0)), &base));
    }
    let brightest = wash.into_iter().reduce(|a, b| if luminance(&b) > luminance(&a) { b } else { a }).unwrap_or(base);
    let names = ["background", "card", "popover", "sidebar"];
    let mut out: [String; 4] = Default::default();
    for (i, name) in names.iter().enumerate() {
        let a = BACKDROP_ALPHA.iter().find(|(n, _)| n == name).map(|(_, a)| *a);
        out[i] = match a {
            Some(a) => over(&alpha_hex(solid[i], a), &brightest),
            None => solid[i].to_owned(),
        };
    }
    out
}

/// `resolveColors` for one theme's JSON (`colors` and the optional `backdrop`); None when it lacks the required colours.
pub fn resolve(theme: &Value) -> Option<Resolved> {
    let dark = theme["appearance"].as_str()? == "dark";
    let set = &theme["colors"];
    let get = |k: &str| set[k].as_str().filter(|v| parse_hex(v).is_some()).map(str::to_owned);
    let (bg, fg, primary) = (get("background")?, get("foreground")?, get("primary")?);
    let (white, black) = ("#ffffff", "#000000");
    let card = get("card").unwrap_or_else(|| if dark { mix(&bg, white, 0.04) } else { mix(&bg, white, 0.6) });
    let popover = get("popover").unwrap_or_else(|| if dark { mix(&card, white, 0.03) } else { card.clone() });
    let sidebar = get("sidebar").unwrap_or_else(|| if dark { mix(&bg, black, 0.2) } else { mix(&bg, black, 0.025) });
    let strong = get("strong").unwrap_or_else(|| if dark { mix(&fg, white, 0.5) } else { mix(&fg, black, 0.55) });
    let status = if dark { ["#e5484d", "#4cc38a", "#e0a042", "#5b9cf5"] } else { ["#e6293f", "#00af6f", "#e19b1b", "#0083e0"] };
    let destructive = get("destructive").unwrap_or_else(|| status[0].to_owned());
    let success = get("success").unwrap_or_else(|| status[1].to_owned());
    let warning = get("warning").unwrap_or_else(|| status[2].to_owned());
    let info = get("info").unwrap_or_else(|| status[3].to_owned());
    let backdrop = theme.get("backdrop").filter(|b| b.is_object());
    let eff = match backdrop {
        Some(b) => effective(b, [&bg, &card, &popover, &sidebar]),
        None => [bg.clone(), card.clone(), popover.clone(), sidebar.clone()],
    };
    let muted = get("mutedForeground").unwrap_or_else(|| ensure(&mix(&fg, &bg, 0.38), &[&eff[0], &eff[1], &eff[2]], 4.5));
    let primary_foreground = get("primaryForeground").unwrap_or_else(|| best_on(&primary, &[&bg, white, black]));
    let all = [eff[0].as_str(), eff[1].as_str(), eff[2].as_str(), eff[3].as_str()];
    let bubble = get("bubble").unwrap_or_else(|| if dark { mix(&bg, &primary, 0.35) } else { mix(white, &primary, 0.14) });
    let bubble_foreground = get("bubbleForeground").unwrap_or_else(|| best_on(&bubble, &[&strong, white, black]));
    let status_foregrounds = [&destructive, &success, &warning, &info].map(|c| ensure(c, &all, 4.5));
    let sidebar_foreground = ensure(&mix(&fg, &sidebar, 0.25), &[&eff[3]], 4.5);
    Some(Resolved {
        sidebar_foreground,
        bubble,
        bubble_foreground,
        status_foregrounds,
        effective: eff.clone(),
        dark,
        code: mix(&bg, &fg, 0.04),
        border: get("border").unwrap_or_else(|| fg.clone()),
        border_alpha: if backdrop.is_some() { 0.14 } else if dark { 0.07 } else { 0.08 },
        strong,
        muted,
        primary_foreground,
        background: bg,
        card,
        popover,
        sidebar,
        foreground: fg,
        primary,
        destructive,
        success,
        warning,
        info,
    })
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

impl Resolved {
    /// Rune's token table for this theme. The strings are leaked: a few dozen bytes per theme the person has ever picked,
    /// kept once by [`painted`].
    pub fn hex(&self) -> Hex {
        Hex {
            background: leak(self.background.clone()),
            card: leak(self.card.clone()),
            popover: leak(self.popover.clone()),
            sidebar: leak(self.sidebar.clone()),
            foreground: leak(self.foreground.clone()),
            strong: leak(self.strong.clone()),
            muted: leak(self.muted.clone()),
            border: leak(self.border.clone()),
            border_alpha: self.border_alpha,
            code: leak(self.code.clone()),
            iris: leak(self.primary.clone()),
            amber: leak(self.warning.clone()),
            sky: leak(self.info.clone()),
            mint: leak(self.success.clone()),
            coral: leak(self.destructive.clone()),
        }
    }
}

// ---- the list ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Builtin,
    User,
}

/// One theme in a picker (`ThemeInfo` in `theme.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeEntry {
    pub id: String,
    pub name: String,
    pub dark: bool,
    pub family: String,
    pub source: Source,
    /// A file of the person's with a built-in's id: removing the file brings the built-in back.
    pub overrides: bool,
    /// The file in the themes folder, for a user theme.
    pub file: Option<String>,
    pub colors: Resolved,
    /// Text pairs under 4.5:1 (the web's warning chip).
    pub low_contrast: bool,
}

/// The web's `contrastFailures` is non-empty: a text colour under 4.5:1 on a surface it is read on.
pub fn low_contrast(c: &Resolved) -> bool {
    let [background, card, popover, sidebar] = &c.effective;
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    for surface in [background, card, popover, sidebar] {
        pairs.push((&c.foreground, surface));
        pairs.push((&c.strong, surface));
    }
    for surface in [background, card, popover] {
        pairs.push((&c.muted, surface));
    }
    pairs.push((&c.sidebar_foreground, sidebar));
    pairs.push((&c.primary_foreground, &c.primary));
    pairs.push((&c.bubble_foreground, &c.bubble));
    pairs.push((&c.foreground, &c.code));
    for fg in &c.status_foregrounds {
        pairs.push((fg, background));
        pairs.push((fg, card));
    }
    pairs.iter().any(|(a, b)| contrast(a, b) < 4.5)
}

fn entry(data: &Value, source: Source, file: Option<String>) -> Option<ThemeEntry> {
    let colors = resolve(data)?;
    let name = data["name"].as_str()?.to_owned();
    let id = data["id"].as_str()?.to_owned();
    Some(ThemeEntry {
        family: data["family"].as_str().map(str::to_owned).unwrap_or_else(|| if source == Source::Builtin { name.clone() } else { String::new() }),
        id,
        name,
        dark: colors.dark,
        source,
        overrides: false,
        file,
        low_contrast: low_contrast(&colors),
        colors,
    })
}

/// The built-ins, in the web's order.
pub fn builtins() -> Vec<ThemeEntry> {
    let mut list: Vec<ThemeEntry> = BUILTIN.iter().filter_map(|text| serde_json::from_str::<Value>(text).ok()).filter_map(|v| entry(&v, Source::Builtin, None)).collect();
    list.sort_by_key(|t| ORDER.iter().position(|id| *id == t.id).unwrap_or(ORDER.len()));
    list
}

/// Built-ins plus the person's files; a user theme with a built-in's id replaces it and marks `overrides`.
pub fn merged(user: &[(String, String, Value)]) -> Vec<ThemeEntry> {
    let mut list = builtins();
    for (id, file, data) in user {
        let Some(mut theme) = entry(data, Source::User, Some(file.clone())) else { continue };
        if theme.id != *id {
            continue;
        }
        if let Some(at) = list.iter().position(|t| t.id == theme.id) {
            theme.overrides = true;
            list[at] = theme;
        } else {
            list.push(theme);
        }
    }
    list
}

// ---- painting ---------------------------------------------------------------------------------

thread_local! {
    static PAINTED: RefCell<HashMap<(String, bool), Rc<Neko>>> = RefCell::new(HashMap::new());
}

fn key(r: &Resolved) -> String {
    format!("{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}", r.background, r.card, r.popover, r.sidebar, r.foreground, r.strong, r.muted, r.border, r.code, r.primary, r.warning, r.info, r.success, r.destructive, r.border_alpha)
}

/// The theme painted from a resolved theme, built once per distinct set of colours.
pub fn painted(r: &Resolved) -> Rc<Neko> {
    let k = (key(r), r.dark);
    if let Some(found) = PAINTED.with(|p| p.borrow().get(&k).cloned()) {
        return found;
    }
    let built = Rc::new(Neko::from_hex(r.dark, r.hex()));
    PAINTED.with(|p| p.borrow_mut().insert(k, built.clone()));
    built
}

/// The three colours a swatch shows: the page, a card on it, and the accent.
pub fn swatch(t: &ThemeEntry) -> [String; 3] {
    [t.colors.background.clone(), t.colors.card.clone(), t.colors.primary.clone()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSS: &str = include_str!("../../../web/src/themes/builtin.generated.css");

    fn block(id: &str) -> HashMap<String, String> {
        let head = format!("html[data-neko-theme=\"{id}\"] {{");
        let at = CSS.find(&head).unwrap_or_else(|| panic!("no block for {id}"));
        let body = &CSS[at + head.len()..];
        let body = &body[..body.find('}').unwrap()];
        body.lines()
            .filter_map(|l| {
                let (name, value) = l.trim().strip_suffix(';')?.split_once(':')?;
                Some((name.trim().trim_start_matches("--").to_owned(), value.trim().to_owned()))
            })
            .collect()
    }

    fn rgba(value: &str) -> (String, f32) {
        let inner = value.strip_prefix("rgba(").and_then(|v| v.strip_suffix(')')).unwrap();
        let p: Vec<&str> = inner.split(',').map(str::trim).collect();
        (format!("#{:02x}{:02x}{:02x}", p[0].parse::<u8>().unwrap(), p[1].parse::<u8>().unwrap(), p[2].parse::<u8>().unwrap()), p[3].parse().unwrap())
    }

    #[test]
    fn every_builtin_resolves_to_the_colours_the_web_generated() {
        let all = builtins();
        assert_eq!(all.len(), 12);
        assert_eq!(all.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ORDER);
        for t in &all {
            let css = block(&t.id);
            let c = &t.colors;
            // A frosted theme's surfaces are translucent in the web; the solid colours behind them are what Rune paints.
            let frosted = css["background"].starts_with("rgba");
            let pairs = [("foreground", &c.foreground), ("strong", &c.strong), ("primary", &c.primary), ("muted-foreground", &c.muted), ("destructive", &c.destructive), ("success", &c.success), ("warning", &c.warning), ("info", &c.info)];
            for (name, ours) in pairs {
                assert_eq!(&css[name], ours, "{}: --{name}", t.id);
            }
            if !frosted {
                for (name, ours) in [("background", &c.background), ("card", &c.card), ("popover", &c.popover), ("sidebar", &c.sidebar), ("code", &c.code)] {
                    assert_eq!(&css[name], ours, "{}: --{name}", t.id);
                }
            }
            for (name, ours) in [("sidebar-foreground", &c.sidebar_foreground), ("bubble", &c.bubble), ("bubble-foreground", &c.bubble_foreground), ("destructive-foreground", &c.status_foregrounds[0]), ("success-foreground", &c.status_foregrounds[1]), ("warning-foreground", &c.status_foregrounds[2]), ("info-foreground", &c.status_foregrounds[3])] {
                assert_eq!(&css[name], ours, "{}: --{name}", t.id);
            }
            let (edge, alpha) = rgba(&css["border"]);
            assert_eq!((edge.as_str(), alpha), (c.border.as_str(), c.border_alpha), "{}: --border", t.id);
        }
    }

    #[test]
    fn a_users_theme_derives_what_it_leaves_out_as_the_web_does() {
        let data = serde_json::json!({"id":"ocean","name":"Ocean","appearance":"dark","colors":{"background":"#0b1f2a","foreground":"#d7e9f0","primary":"#3ab0d0"}});
        let r = resolve(&data).unwrap();
        assert_eq!(r.card, mix("#0b1f2a", "#ffffff", 0.04));
        assert_eq!(r.sidebar, mix("#0b1f2a", "#000000", 0.2));
        assert_eq!(r.destructive, "#e5484d");
        assert!(contrast(&r.muted, &r.background) >= 4.5, "a derived muted colour is nudged until it reads");
        assert!(resolve(&serde_json::json!({"appearance":"dark","colors":{"background":"#000"}})).is_none(), "a theme needs its three colours");
        assert!(resolve(&serde_json::json!({"appearance":"dark","colors":{"background":"nope","foreground":"#fff","primary":"#fff"}})).is_none());
    }

    #[test]
    fn a_user_theme_with_a_builtins_id_replaces_it_and_is_marked() {
        let own = serde_json::json!({"id":"nord","name":"My Nord","appearance":"dark","colors":{"background":"#101010","foreground":"#eeeeee","primary":"#88c0d0"}});
        let list = merged(&[("nord".into(), "nord.json".into(), own)]);
        let nord = list.iter().find(|t| t.id == "nord").unwrap();
        assert!(nord.overrides && nord.source == Source::User && nord.name == "My Nord");
        assert_eq!(list.len(), 12);
    }

    #[test]
    fn a_chosen_theme_paints_its_tokens() {
        let nord = builtins().into_iter().find(|t| t.id == "nord").unwrap();
        let t = painted(&nord.colors);
        assert_eq!(t.surface.window.hex()[..7], nord.colors.background);
        assert_eq!(t.meaning.iris.hex()[..7], nord.colors.primary);
        assert!(Rc::ptr_eq(&t, &painted(&nord.colors)), "built once per set of colours");
    }
}
