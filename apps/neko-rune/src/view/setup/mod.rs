//! Setup: one full view with a section rail at the left (200 wide, the same component as Settings' rail) in
//! this order: Packs, Workspaces, Watching, Tools, Skills, Memory, Profiles, Schedules. Each section is the web's
//! page of the same name made native (`web/src/pages/setup/*`): the same requests and the same states,
//! on Rune's page conventions (a 20 point header with a count, an 880 column, 32 point rows whose actions
//! show on hover and are all in the right-click menu, one quiet line and one action for an empty list).
//! Every destructive action goes through a confirmation with a coral button (`dialogs.rs`).
//!
//! Deep links from elsewhere select a section: [`App::open_setup`], [`App::open_connection`].

mod dialogs;
pub(crate) mod kit;
mod memory;
mod pack_dialog;
mod packs;
mod profiles;
mod schedules;
mod skills;
mod state;
mod tools;
mod watching;
pub(crate) mod workspaces;
pub mod words;

pub use state::{AddTab, Confirm, Dialog, Fetch, PackIntent, PackTab, Registry, SetupSection, SetupState};

use rune::{El, col, row, text};

use super::ui::{quiet_line, rail};
use super::{Frame, band, nav_label_row};
use crate::app::App;
use crate::theme::Neko;

/// What a section gives the frame: its header's count and controls, and its body. A connection's detail
/// gives a title of its own and a back arrow.
pub struct Page {
    pub count: Option<usize>,
    pub trailing: Vec<El>,
    pub body: El,
    pub title: Option<String>,
    pub back: Option<El>,
}

impl Page {
    /// A section with nothing to show yet, as one quiet line.
    pub fn quiet(t: &Neko, key: &str, line: &str) -> Page {
        Page { count: None, trailing: Vec::new(), body: quiet_line(t, key, line), title: None, back: None }
    }
}

/// How many setup items need the person (the sessions column's count): broken connections and pending proposals.
pub fn attention(s: &App) -> usize {
    s.setup_data().map_or(0, crate::link::SetupData::attention)
}

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>, frame: Frame) -> El {
    let section = s.setup.section;
    // The web's viewport for a Setup page is the content beside the rail.
    let rail_w = if frame.region_w < 760.0 { 150.0 } else { 200.0 };
    super::webkit::set_page_width(frame.region_w - rail_w);
    super::webkit::set_content_width(frame.region_w - rail_w - 2.0 * if frame.region_w < 760.0 { 16.0 } else { 28.0 });
    let page = match section {
        SetupSection::Packs => packs::view(s, t, layer),
        SetupSection::Workspaces => workspaces::view(s, t, layer),
        SetupSection::Watching => watching::view(s, t, layer),
        SetupSection::Tools => tools::view(s, t, layer),
        SetupSection::Skills => skills::view(s, t, layer),
        SetupSection::Memory => memory::view(s, t, layer),
        SetupSection::Profiles => profiles::view(s, t, layer),
        SetupSection::Schedules => schedules::view(s, t, layer),
    };
    dialogs::mount(s, t, layer, frame.win_h);
    // Under 760 points the rail narrows to 150 and the sides to 16, so the rows keep their room.
    let narrow = frame.region_w < 760.0;
    let side = if narrow { 16.0 } else { 28.0 };
    let rows: Vec<El> = SetupSection::ALL
        .iter()
        .map(|&sec| {
            nav_label_row(sec.title(), &format!("setup-section-{}", sec.key()), sec == section, t).on_press(move |s: &mut App| {
                s.open_setup(sec);
            })
        })
        .collect();
    let title = page.title.clone().unwrap_or_else(|| section.title().to_owned());
    let mut head = row().w_full().gap(10.0).center_y().flex_wrap().min_h(36.0);
    if let Some(back) = page.back {
        head = head.child(back);
    }
    head = head.child(text(title).key("title").font(rune::TextStyle { tracking: -0.41, ..t.ty.title }).color(t.text.base).truncate().min_w(0.0).role("heading"));
    if let Some(n) = page.count {
        // `PageHeader`'s count: 14 regular, tabular figures, on the title's baseline.
        head = head.child(text(n.to_string()).key("page-count").font(super::webkit::tnum(super::webkit::font(t, 14.0, 400.0, 22.75))).color(t.text.muted).mt(4.4));
    }
    head = head.child(kit::fill());
    for e in page.trailing {
        head = head.child(e);
    }
    let column = col().key(format!("full-setup-{}", section.key())).w_full().max_w(super::COLUMN + 2.0 * (side - 32.0)).px(side).pt(24.0).pb(40.0).gap(32.0).child(head).child(page.body);
    // The window's drag band, then the rail and the scrolling column.
    let scroll = col().key("page-scroll").grow(1.0).min_w(0.0).h_full().clip_rounded(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(row().w_full().justify_center().child(column));
    col().key("setup").w_full().h_full().child(band(t)).child(row().w_full().grow(1.0).min_h(0.0).child(rail("setup", narrow, None, rows)).child(scroll))
}
