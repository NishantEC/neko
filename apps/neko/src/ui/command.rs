//! rune-ui registry: command 0.11.0. Copied into your app by `rune-ui add command`; yours to change. Needs: kbd.
//!
//! A command palette: a search over grouped results. The **floating** material; the search is a bare line over a hairline (the panel is
//! the well's frame); the highlighted row takes the selected wash; shortcuts are `Kbd` caps. `build` draws it inline; `dialog` mounts
//! it as a modal near the top of the window (Rune's `El::modal`: focus trapped, Escape and a press outside close it).
//!
//! * **Rows.** A leading element (your own: a status mark, a portrait; or an icon), a title, an optional quiet second line
//!   (`subtitle`), a trailing hint in words (`hint`: "2m", "settled") and a shortcut. Results sit in sections with headings, in the
//!   order the sections first appear in your items.
//! * **Matching.** Fuzzy by default: every word of the query must match the title, the subtitle or the item's `keywords`, in order
//!   but not touching ("lgn" finds "Fix the login redirect"); word starts and runs of letters rank higher. The letters that matched are
//!   drawn semibold ([`Match::Words`] keeps the older rule: every word appears in the label). Rows inside a section are ranked.
//! * **Keys.** Up and Down move (and wrap), Return chooses (`on_select`), Command+Return is the alternate (`on_alt`: "open in the
//!   pane"), Escape closes. `hint(keys, words)` adds a footer that says so.
//! * **Empty query.** `recents(&[keys])` shows those items first under "Recent" (in that order) before the rest.
//! * **Loading.** `loading(true)` turns on a spinner in the field and keeps the rows that are there while slower ones arrive; with
//!   no rows it says "Searching…" instead of "No results".
//!
//! ```ignore
//! Command::new("palette", &s.query, |s: &mut App| &mut s.query).items(&items).highlight(s.at).recents(&s.recent).loading(s.searching)
//!     .on_highlight(|s: &mut App, i| s.at = i).on_select(|s: &mut App, key: &str| s.open(key)).on_alt(|s: &mut App, key: &str| s.open_in_pane(key))
//!     .hint("↩", "Open").hint("⌘ ↩", "Open in pane").dialog(t, s.palette, "search", |s: &mut App| s.palette = false, &mut layer)
//! ```

use std::rc::Rc;

use rune::{El, Icon, Key, KeyEvent, Span, TextEdit, col, el, row, text};
use rune_ui_style::weight;

use super::base::{Choose, Pick, Shape, States, Theme, glyph, paint, paint_state, spinner_in};
use super::icons;
use super::kbd::Kbd;

/// One result.
#[derive(Clone)]
pub struct CommandItem {
    pub key: String,
    pub label: String,
    pub group: String,
    pub icon: Option<&'static Icon>,
    pub shortcut: Option<String>,
    /// A quiet second line under the title.
    pub subtitle: Option<String>,
    /// Your own leading element (a status mark, a portrait) in place of the icon, centred in the icon's column and hidden from
    /// assistive technology (the row's name says what it shows).
    pub lead: Option<El>,
    /// Words at the row's end ("2m", "settled").
    pub hint: Option<String>,
    /// Words that find the row without being shown.
    pub keywords: Option<String>,
}

impl std::fmt::Debug for CommandItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandItem").field("key", &self.key).field("label", &self.label).field("group", &self.group).finish_non_exhaustive()
    }
}

impl CommandItem {
    pub fn new(key: impl Into<String>, label: impl Into<String>, group: impl Into<String>) -> CommandItem {
        CommandItem { key: key.into(), label: label.into(), group: group.into(), icon: None, shortcut: None, subtitle: None, lead: None, hint: None, keywords: None }
    }

    pub fn icon(mut self, i: &'static Icon) -> Self {
        self.icon = Some(i);
        self
    }

    pub fn shortcut(mut self, k: impl Into<String>) -> Self {
        self.shortcut = Some(k.into());
        self
    }

    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }

    pub fn lead(mut self, e: El) -> Self {
        self.lead = Some(e);
        self
    }

    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }

    pub fn keywords(mut self, k: impl Into<String>) -> Self {
        self.keywords = Some(k.into());
        self
    }
}

/// How the query finds rows.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Match {
    /// Every word of the query matches in order but not necessarily touching; ranked.
    #[default]
    Fuzzy,
    /// Every word of the query appears in the label (the rule before 0.11); not ranked.
    Words,
}

/// A row that matched: the item, the letters that matched (character positions) in its title and in its subtitle, and its rank.
#[derive(Clone, Debug)]
pub struct Hit<'a> {
    pub item: &'a CommandItem,
    pub title: Vec<usize>,
    pub subtitle: Vec<usize>,
    pub score: i32,
}

/// Matches `query` (no spaces) in `text` as a subsequence, ignoring case: its score (higher is better) and the character positions
/// that matched, or none. Word starts, a run of touching letters and an early start score higher; skipped letters cost a little.
pub fn fuzzy(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    let q: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let orig: Vec<char> = text.chars().collect();
    let t: Vec<char> = orig.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let (n, m) = (t.len(), q.len());
    if m == 0 {
        return Some((0, Vec::new()));
    }
    if m > n {
        return None;
    }
    let bonus = |j: usize| -> i32 {
        if j == 0 {
            return 10;
        }
        let (p, c) = (orig[j - 1], orig[j]);
        if !p.is_alphanumeric() {
            8
        } else if p.is_lowercase() && c.is_uppercase() {
            6
        } else {
            0
        }
    };
    const NONE: i32 = i32::MIN / 2;
    // dp[i][j]: the best score matching q[..=i] with q[i] at text[j]; from[i][j]: where q[i - 1] was.
    let mut dp = vec![vec![NONE; n]; m];
    let mut from = vec![vec![usize::MAX; n]; m];
    for (j, d) in dp[0].iter_mut().enumerate() {
        if t[j] == q[0] {
            *d = 1 + bonus(j) - (j as i32).min(6);
        }
    }
    for i in 1..m {
        // The best earlier end k < j - 1, less one for each letter skipped since.
        let (mut best, mut best_at) = (NONE, usize::MAX);
        for j in i..n {
            if j >= 2 && dp[i - 1][j - 2] > NONE {
                if best > NONE {
                    best -= 1;
                }
                if dp[i - 1][j - 2] >= best {
                    best = dp[i - 1][j - 2];
                    best_at = j - 2;
                }
            } else if best > NONE {
                best -= 1;
            }
            if t[j] != q[i] {
                continue;
            }
            let mut top = (NONE, usize::MAX);
            if dp[i - 1][j - 1] > NONE {
                top = (dp[i - 1][j - 1] + 6, j - 1);
            }
            if best > top.0 {
                top = (best, best_at);
            }
            if top.0 > NONE {
                dp[i][j] = top.0 + 1 + bonus(j);
                from[i][j] = top.1;
            }
        }
    }
    let (mut score, mut at) = (NONE, usize::MAX);
    for (j, &d) in dp[m - 1].iter().enumerate() {
        if d > score {
            score = d;
            at = j;
        }
    }
    if score <= NONE {
        return None;
    }
    let mut pos = vec![0; m];
    for i in (0..m).rev() {
        pos[i] = at;
        if i > 0 {
            at = from[i][at];
        }
    }
    Some((score, pos))
}

/// Where each of `words` (lowercase) begins inside `text` (substring matches), as character positions covering them.
fn substring_positions(word: &str, text: &str) -> Option<Vec<usize>> {
    let hay: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let needle: Vec<char> = word.chars().collect();
    if needle.is_empty() || needle.len() > hay.len() {
        return needle.is_empty().then(Vec::new);
    }
    (0..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == needle[..]).map(|i| (i..i + needle.len()).collect())
}

/// The items `query` keeps, in order: every word of the query appears in the label, ignoring case.
pub fn filter<'a>(items: &'a [CommandItem], query: &str) -> Vec<&'a CommandItem> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    items.iter().filter(|i| {
        let l = i.label.to_lowercase();
        words.iter().all(|w| l.contains(w.as_str()))
    }).collect()
}

/// The rows `query` finds, sections in the order they first appear in `items`, ranked inside each section (as listed under
/// [`Match::Words`]). An empty query keeps every item in order.
pub fn search<'a>(items: &'a [CommandItem], query: &str, mode: Match) -> Vec<Hit<'a>> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut hits: Vec<(usize, Hit<'a>)> = Vec::new();
    'items: for (n, item) in items.iter().enumerate() {
        let mut hit = Hit { item, title: Vec::new(), subtitle: Vec::new(), score: 0 };
        for w in &words {
            match mode {
                Match::Words => match substring_positions(w, &item.label) {
                    Some(p) => hit.title.extend(p),
                    None => continue 'items,
                },
                Match::Fuzzy => {
                    // The title counts most; the subtitle and the hidden keywords find the row too.
                    let t = fuzzy(w, &item.label).map(|(s, p)| (s + 12, p, 0));
                    let sub = item.subtitle.as_deref().and_then(|x| fuzzy(w, x)).map(|(s, p)| (s, p, 1));
                    let kw = item.keywords.as_deref().and_then(|x| fuzzy(w, x)).map(|(s, _)| (s - 4, Vec::new(), 2));
                    match [t, sub, kw].into_iter().flatten().max_by_key(|(s, _, _)| *s) {
                        Some((s, p, 0)) => {
                            hit.score += s;
                            hit.title.extend(p);
                        }
                        Some((s, p, 1)) => {
                            hit.score += s;
                            hit.subtitle.extend(p);
                        }
                        Some((s, _, _)) => hit.score += s,
                        None => continue 'items,
                    }
                }
            }
        }
        hit.title.sort_unstable();
        hit.title.dedup();
        hit.subtitle.sort_unstable();
        hit.subtitle.dedup();
        hits.push((n, hit));
    }
    let mut order: Vec<&str> = Vec::new();
    for i in items {
        if !order.contains(&i.group.as_str()) {
            order.push(&i.group);
        }
    }
    let section = |h: &Hit| order.iter().position(|g| *g == h.item.group).unwrap_or(0);
    hits.sort_by(|(na, a), (nb, b)| section(a).cmp(&section(b)).then(if mode == Match::Fuzzy { b.score.cmp(&a.score) } else { std::cmp::Ordering::Equal }).then(na.cmp(nb)));
    hits.into_iter().map(|(_, h)| h).collect()
}

/// `text` with the characters at `at` drawn semibold in `ink` (the rest as `text` is given).
fn marked(words: &str, at: &[usize], base: rune::TextStyle, strong: rune::TextStyle, ink: rune::Color, t: &Theme) -> El {
    let e = text(words.to_owned()).font(base).color(ink).truncate().min_w(0.0).a11y_hidden();
    if at.is_empty() {
        return e;
    }
    let bytes: Vec<usize> = words.char_indices().map(|(b, _)| b).chain(std::iter::once(words.len())).collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0;
    while i < at.len() {
        let mut j = i;
        while j + 1 < at.len() && at[j + 1] == at[j] + 1 {
            j += 1;
        }
        if at[j] < bytes.len() - 1 {
            spans.push(Span::new(bytes[at[i]], bytes[at[j] + 1]).weight(strong.weight).color(t.ink()));
        }
        i = j + 1;
    }
    e.spans(spans)
}

pub struct Command<'a, S: 'static> {
    key: String,
    edit: &'a TextEdit,
    access: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    placeholder: String,
    items: &'a [CommandItem],
    highlight: usize,
    empty: String,
    width: f32,
    top: f32,
    mode: Match,
    limit: usize,
    recents: Vec<String>,
    recent_label: String,
    loading: bool,
    hints: Vec<(String, String)>,
    move_to: Option<Choose<S>>,
    select: Option<Pick<S>>,
    alt: Option<Pick<S>>,
}

impl<'a, S: 'static> Command<'a, S> {
    pub fn new(key: impl Into<String>, query: &'a TextEdit, access: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static) -> Command<'a, S> {
        Command { key: key.into(), edit: query, access: Rc::new(access), placeholder: "Type a command or search…".to_owned(), items: &[], highlight: 0, empty: "No results".to_owned(), width: 520.0, top: 96.0, mode: Match::Fuzzy, limit: 50, recents: Vec::new(), recent_label: "Recent".to_owned(), loading: false, hints: Vec::new(), move_to: None, select: None, alt: None }
    }

    pub fn items(mut self, items: &'a [CommandItem]) -> Self {
        self.items = items;
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    /// The highlighted row among the rows shown.
    pub fn highlight(mut self, i: usize) -> Self {
        self.highlight = i;
        self
    }

    pub fn empty(mut self, words: impl Into<String>) -> Self {
        self.empty = words.into();
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// How far down the window the modal sits (default 96 points).
    pub fn top(mut self, points: f32) -> Self {
        self.top = points;
        self
    }

    /// Fuzzy (the default) or every-word matching.
    pub fn matching(mut self, m: Match) -> Self {
        self.mode = m;
        self
    }

    /// The most rows shown (default 50).
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n.max(1);
        self
    }

    /// The keys of items to show first, under "Recent", while the query is empty.
    pub fn recents(mut self, keys: &[String]) -> Self {
        self.recents = keys.to_vec();
        self
    }

    /// The heading over the recents (default "Recent").
    pub fn recent_label(mut self, l: impl Into<String>) -> Self {
        self.recent_label = l.into();
        self
    }

    /// Slower results are still on their way: a spinner in the field, rows kept in place.
    pub fn loading(mut self, on: bool) -> Self {
        self.loading = on;
        self
    }

    /// A line in the footer that says what a key does ("↩", "Open").
    pub fn hint(mut self, keys: impl Into<String>, words: impl Into<String>) -> Self {
        self.hints.push((keys.into(), words.into()));
        self
    }

    /// Up and Down moved the highlight to this row.
    pub fn on_highlight(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.move_to = Some(Rc::new(f));
        self
    }

    /// A row chosen (by Return or a press): its key.
    pub fn on_select(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.select = Some(Rc::new(f));
        self
    }

    /// The alternate choice (Command+Return) of the highlighted row: its key.
    pub fn on_alt(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.alt = Some(Rc::new(f));
        self
    }

    /// The rows shown, each with the heading of the section it belongs to.
    fn rows(&self) -> Vec<(String, Hit<'a>)> {
        let q = self.edit.text.trim();
        let mut out: Vec<(String, Hit<'a>)> = Vec::new();
        if q.is_empty() && !self.recents.is_empty() {
            for k in &self.recents {
                if let Some(item) = self.items.iter().find(|i| i.key == *k) {
                    out.push((self.recent_label.clone(), Hit { item, title: Vec::new(), subtitle: Vec::new(), score: 0 }));
                }
            }
        }
        let kept: Vec<String> = out.iter().map(|(_, h)| h.item.key.clone()).collect();
        for h in search(self.items, q, self.mode) {
            if !kept.contains(&h.item.key) {
                out.push((h.item.group.clone(), h));
            }
        }
        out.truncate(self.limit);
        out
    }

    pub fn build(self, t: &Theme) -> El {
        let shown = self.rows();
        let at = self.highlight.min(shown.len().saturating_sub(1));
        let access = self.access.clone();
        let keys: Vec<String> = shown.iter().map(|(_, h)| h.item.key.clone()).collect();
        let descendant = keys.get(at).map(|k| format!("{}-{k}", self.key));
        let (move_to, select, alt) = (self.move_to.clone(), self.select.clone(), self.alt.clone());
        let n = shown.len();
        let mut input = rune::text_input(self.edit, t.ty.md)
            .key(format!("{}-input", self.key))
            .single_line()
            .grow(1.0)
            .min_w(0.0)
            .color(t.ink())
            .placeholder(self.placeholder.clone(), t.ink_faint())
            .caret(t.ink(), t.c.accent.fg.alpha(0.28))
            .label("Command")
            .on_edit(rune::edit_access(move |s: &mut S| access(s)))
            .on_input_key(move |s: &mut S, k: &KeyEvent, _: &mut rune::Tx| {
                match k.key {
                    Key::Down if n > 0 => move_to.as_ref().map(|f| f(s, (at + 1) % n)).is_some(),
                    Key::Up if n > 0 => move_to.as_ref().map(|f| f(s, (at + n - 1) % n)).is_some(),
                    Key::Enter => {
                        let on = if k.mods.cmd { alt.as_ref() } else { select.as_ref() };
                        match (keys.get(at), on) {
                            (Some(key), Some(f)) => {
                                f(s, key);
                                true
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                }
            });
        if let Some(k) = descendant {
            input = input.active_descendant(k);
        }
        let mut search = row().w_full().h(t.control.lg + 4.0).px(t.metrics.inset - 4.0).gap(t.space.at(2.5)).center_y().border_sides([0.0, 0.0, 1.0, 0.0], t.c.line).child(glyph(&icons::SEARCH, t.control.icon_lg, t.ink_faint())).child(input);
        if self.loading {
            search = search.child(spinner_in(t, t.control.icon, t.ink_faint(), "Searching"));
        }
        let mut list = col().key(format!("{}-list", self.key)).role("listbox").label("Commands").busy(self.loading).w_full().max_h(360.0).padding(t.space.at(1.5)).gap(1.0).clip_rounded(0.0).scroll(rune::ScrollSpec { stick_bottom: false });
        if shown.is_empty() {
            let words = if self.loading { "Searching…".to_owned() } else { self.empty.clone() };
            list = list.child(row().w_full().h(64.0).center().child(text(words).font(t.ty.base).color(t.ink_muted())));
        }
        let strong = weight(t.ty.base, t.ty.semibold);
        let mut section: Option<(String, El)> = None;
        let mut sections: Vec<El> = Vec::new();
        let close = |sec: Option<(String, El)>, out: &mut Vec<El>| {
            if let Some((_, g)) = sec {
                out.push(g);
            }
        };
        for (i, (heading, hit)) in shown.iter().enumerate() {
            let item = hit.item;
            if section.as_ref().is_none_or(|(h, _)| h != heading) {
                close(section.take(), &mut sections);
                let head = row().h(28.0).px(t.space.at(2.5)).center_y().child(text(heading.clone()).font(weight(t.ty.xs, t.ty.medium)).color(t.ink_faint()).role("heading").level(3).label(heading.clone()));
                section = Some((heading.clone(), col().role("group").label(heading.clone()).w_full().gap(1.0).child(head)));
            }
            let st = States { selected: i == at, ..States::default() };
            let two = item.subtitle.is_some();
            let h = if two { t.metrics.row + t.space.at(4.0) } else { t.metrics.row };
            let mut r = row().key(format!("{}-{}", self.key, item.key)).role("option").label(item.label.clone()).position_in_set(i + 1, n).focus_selected(i == at).h(h).px(t.metrics.pad_md - 4.0).gap(t.metrics.gap).center_y().radius(t.radius.md).round_as(t);
            if let Some(sub) = &item.subtitle {
                r = r.description(sub.clone());
            }
            r = paint_state(r, &t.mat.row, st);
            let ink = if i == at { t.ink() } else { t.ink_muted() };
            if let Some(e) = &item.lead {
                r = r.child(row().size(t.control.icon_lg, t.control.icon_lg).center().a11y_hidden().child(e.clone()));
            } else if let Some(ic) = item.icon {
                r = r.child(glyph(ic, t.control.icon_lg, ink));
            }
            let title = marked(&item.label, &hit.title, t.ty.base, strong, t.ink(), t);
            let words = match &item.subtitle {
                Some(sub) => col().grow(1.0).shrink().min_w(0.0).child(title).child(marked(sub, &hit.subtitle, t.ty.sm, weight(t.ty.sm, t.ty.medium), t.ink_faint(), t)),
                None => title.grow(1.0),
            };
            r = r.child(words);
            if let Some(hint) = &item.hint {
                r = r.child(text(hint.clone()).font(t.ty.xs).color(t.ink_faint()).a11y_hidden());
            }
            if let Some(k) = &item.shortcut {
                r = r.child(Kbd::new(format!("{}-{}-keys", self.key, item.key), k.clone()).build(t));
            }
            if let Some(f) = self.select.clone() {
                let key = item.key.clone();
                r = r.on_press(move |s: &mut S| f(s, &key));
            }
            if let Some((h, g)) = section.take() {
                section = Some((h, g.child(r)));
            }
        }
        close(section.take(), &mut sections);
        list = list.children(sections);
        let mut p = col().key(self.key.clone()).role("dialog").label("Command palette").w(self.width).radius(t.radius.xl).round_as(t).clip_rounded(t.radius.xl);
        p = paint(p, &t.mat.floating.base).child(search).child(list);
        if !self.hints.is_empty() {
            let mut foot = row().key(format!("{}-hints", self.key)).w_full().h(t.control.md).px(t.metrics.inset - 4.0).gap(t.space.at(4.0)).center_y().border_sides([1.0, 0.0, 0.0, 0.0], t.c.line);
            for (j, (keys, words)) in self.hints.iter().enumerate() {
                foot = foot.child(row().gap(t.space.at(1.5)).center_y().child(Kbd::new(format!("{}-hint-{j}", self.key), keys.clone()).build(t)).child(text(words.clone()).font(t.ty.xs).color(t.ink_faint())));
            }
            p = p.child(foot);
        }
        p
    }

    /// Mounts the palette as a modal near the top of the window while `open`; focus goes back to `return_to` when it closes.
    pub fn dialog(self, t: &Theme, open: bool, return_to: &str, close: impl Fn(&mut S) + 'static, layer: &mut Vec<El>) {
        if !open {
            return;
        }
        let key = self.key.clone();
        let top = self.top;
        let panel = self.build(t).focus_return(return_to).modal(true, close);
        layer.push(el().key(format!("{key}-scrim")).absolute(0.0, 0.0).w_full().h_full().fill(t.scrim_soft()).a11y_hidden());
        layer.push(col().key(format!("{key}-layer")).absolute(0.0, 0.0).w_full().h_full().pt(top).items_center().child(panel));
    }
}
