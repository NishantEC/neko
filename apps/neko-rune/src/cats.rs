//! Cats in the window (docs/knowledge/cats.md, "Rune: slice 1"): the list the sidebar shows, the cat page and its panel, the New cat
//! sheet, the four files and their editor. State and rules only; `view/cats.rs` draws it and `link/cats.rs` is the wire.
//!
//! The daemon is the only writer and every write answers with the active list, so the list here is always what it last said. A
//! change another window makes reaches us through `Snapshot::cats_rev` (read from each snapshot, `cats_sync`). Words are Neko's own
//! (`docs/design/terminology.md`): cats, tasks, rounds, Rules, Task rules, Plan, Briefing.

use std::collections::{BTreeMap, HashMap};

use neko_protocol::cats::{Cat, CatFileKind, CatFiles, CatSuggestion, MAX_ACTIVE_CATS, MAX_CAT_FILE, ModelPolicy};
use neko_protocol::workbench::{RuntimePreferences, TaskStatus};
use rune::{Phase, PointerPress};

use crate::app::App;
use crate::cat_face::Marks;
use crate::extra::Going;
use crate::link::{Call, CatCall, CatsData, Reply, RuntimeChoice};
use crate::task_actions::Ask;

/// The sheet's text fields (page fields, `Pages::fields`).
pub const F_NAME: &str = "cat:name";
pub const F_AREA: &str = "cat:area";
pub const F_LIMIT: &str = "cat:limit";
pub const F_PURPOSE: &str = "cat:purpose";
/// The file editor's text.
pub const F_FILE: &str = "cat:file";

/// How many cats show as faces in the sidebar's row.
pub const PINNED_SHOWN: usize = 3;
/// How far a press must travel before it is a drag, in points.
pub const DRAG_SLOP: f32 = 6.0;

/// The cat page's right panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CatTab {
    Overview,
    Files,
}

impl CatTab {
    pub const ALL: [CatTab; 2] = [CatTab::Overview, CatTab::Files];

    pub fn title(self) -> &'static str {
        match self {
            CatTab::Overview => "Overview",
            CatTab::Files => "Files",
        }
    }
}

/// The words of a file, as Neko calls it (`terminology.md`), and what it is for.
pub fn file_title(kind: CatFileKind) -> &'static str {
    match kind {
        CatFileKind::Rules => "Rules",
        CatFileKind::TaskRules => "Task rules",
        CatFileKind::Plan => "Plan",
        CatFileKind::Briefing => "Briefing",
    }
}

pub fn file_purpose(kind: CatFileKind) -> &'static str {
    match kind {
        CatFileKind::Rules => "The cat's own rules",
        CatFileKind::TaskRules => "What every task it starts reads",
        CatFileKind::Plan => "What comes next",
        CatFileKind::Briefing => "The current summary",
    }
}

/// The New cat / Edit cat sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    /// The cat being edited; None for a new one.
    pub editing: Option<String>,
    pub face: String,
    /// The workspace's id (fixed once the cat exists).
    pub workspace: String,
    /// The model's (provider, id); None is Neko's default.
    pub model: Option<(String, String)>,
    /// The line under each field the daemon (or this client) refused, by field.
    pub errors: BTreeMap<&'static str, String>,
    pub busy: bool,
    /// Ask the daemon for a first cat once, when there are none.
    pub suggest_asked: bool,
    /// The cats there were when Create was sent, to find the one it made.
    known: Vec<String>,
}

/// The file the editor holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEditor {
    pub cat: String,
    pub kind: CatFileKind,
    /// The text as last saved (what Revert goes back to).
    pub saved: String,
    pub error: Option<String>,
    pub saving: bool,
    /// The file on disk is larger than 16 KB: the editor holds its first 16 KB.
    pub truncated: bool,
}

/// A sidebar row or face being dragged to a new place.
#[derive(Clone, Debug, PartialEq)]
pub struct Carry {
    pub id: String,
    /// The face row (true) or the list of rows (false) the cat belongs to: a cat never crosses between them by dragging.
    pub among_faces: bool,
    pub moved: bool,
    /// How many places it has been carried (negative is up or left).
    pub delta: i32,
}

#[derive(Default)]
pub struct CatsState {
    /// The active cats in order; None until the daemon has answered.
    pub list: Option<Vec<Cat>>,
    /// The list was asked for and has not come back.
    pub asked: bool,
    /// The `cats_rev` the list was read at.
    pub rev_seen: u64,
    /// The cat whose page is open.
    pub open: Option<String>,
    pub tab: Option<CatTab>,
    pub sheet: Option<Sheet>,
    /// The runtimes and models the sheet offers.
    pub choices: Vec<RuntimeChoice>,
    pub files: HashMap<String, CatFiles>,
    pub editor: Option<FileEditor>,
    /// The page header's "..." menu.
    pub menu: bool,
    /// The right-click menu on a sidebar cat: which, and where.
    pub row_menu: Option<(String, PointerPress)>,
    pub carry: Option<Carry>,
    /// The press that ends a drag is not a click.
    pub suppress_press: bool,
    /// The context panel was opened by the cat page (and closes when the person leaves it).
    pub opened_panel: bool,
    /// A line for something that went wrong outside the sheet and the editor (a refused pin or reorder).
    pub notice: Option<String>,
    /// The sheet's workspace and model selects.
    pub select: Option<&'static str>,
    /// A suggestion that has arrived and not been used yet.
    pub suggestion: Option<CatSuggestion>,
}

// ---- pure rules --------------------------------------------------------------------------------

/// The cats as the sidebar shows them: up to three pinned (in order) as faces, then everything else in order as rows.
/// A fourth pinned cat is a row like the rest.
pub fn display(cats: &[Cat]) -> (Vec<&Cat>, Vec<&Cat>) {
    let mut sorted: Vec<&Cat> = cats.iter().collect();
    sorted.sort_by_key(|c| c.order);
    let mut faces = Vec::new();
    let mut rows = Vec::new();
    for c in sorted {
        if c.pinned && faces.len() < PINNED_SHOWN {
            faces.push(c);
        } else {
            rows.push(c);
        }
    }
    (faces, rows)
}

/// The ids in the order a drag of `id` by `delta` places leaves them: the cat moves within its own group (the faces, or the rows) and the
/// groups keep their places, so the new order is the faces then the rows, as drawn.
pub fn reordered(cats: &[Cat], id: &str, delta: i32) -> Vec<String> {
    let (faces, rows) = display(cats);
    let mut faces: Vec<String> = faces.into_iter().map(|c| c.id.clone()).collect();
    let mut rows: Vec<String> = rows.into_iter().map(|c| c.id.clone()).collect();
    let group = if faces.iter().any(|f| f == id) { &mut faces } else { &mut rows };
    if let Some(from) = group.iter().position(|c| c == id) {
        let to = (from as i64 + i64::from(delta)).clamp(0, group.len() as i64 - 1) as usize;
        let moved = group.remove(from);
        group.insert(to, moved);
    }
    faces.extend(rows);
    faces
}

/// A daily limit typed in dollars (`5`, `5.00`, `$5.5`) as micro-dollars; empty is no limit. The daemon's own ceiling is checked there.
pub fn micro_from_dollars(text: &str) -> Result<Option<u64>, String> {
    let said = "Enter a dollar amount such as 5.00, or leave it empty for no limit";
    let t = text.trim().trim_start_matches('$').trim();
    if t.is_empty() {
        return Ok(None);
    }
    let (whole, frac) = t.split_once('.').unwrap_or((t, ""));
    if whole.is_empty() && frac.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) || frac.len() > 6 || whole.len() > 7 {
        return Err(said.to_owned());
    }
    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| said.to_owned())? };
    let frac_micro: u64 = if frac.is_empty() { 0 } else { format!("{frac:0<6}").parse().map_err(|_| said.to_owned())? };
    match whole * 1_000_000 + frac_micro {
        0 => Err("A limit of nothing would stop the cat. Leave it empty for no limit".to_owned()),
        n => Ok(Some(n)),
    }
}

/// Micro-dollars as the field shows them: `5.00`, or more places only when the amount needs them.
pub fn dollars_text(micro: u64) -> String {
    if micro.is_multiple_of(10_000) {
        format!("{}.{:02}", micro / 1_000_000, (micro % 1_000_000) / 10_000)
    } else {
        format!("{}.{:06}", micro / 1_000_000, micro % 1_000_000).trim_end_matches('0').to_owned()
    }
}

/// Which field of the sheet a daemon refusal is about (the daemon's words name it), or "form" for the sheet as a whole.
pub fn field_of(message: &str) -> &'static str {
    let m = message.to_lowercase();
    if m.contains("workspace") {
        "workspace"
    } else if m.contains("daily limit") || m.contains("limit must") {
        "limit"
    } else if m.contains("purpose") {
        "purpose"
    } else if m.contains("area") {
        "area"
    } else if m.contains("face") {
        "face"
    } else if m.contains("name") || m.contains("called") {
        "name"
    } else if m.contains("model") || m.contains("runtime") || m.contains("provider") {
        "model"
    } else {
        "form"
    }
}

/// Whether a text fits a cat file (the daemon's limit, in bytes).
pub fn fits(text: &str) -> bool {
    text.len() <= MAX_CAT_FILE
}

/// "12.3 KB" for the editor's footer.
pub fn size_words(bytes: usize) -> String {
    if bytes < 1024 { format!("{bytes} bytes") } else { format!("{:.1} KB", bytes as f64 / 1024.0) }
}

// ---- the app's side ----------------------------------------------------------------------------

impl App {
    /// The cat with this id, from the list the daemon last gave.
    pub fn cat(&self, id: &str) -> Option<&Cat> {
        self.cats.list.as_ref()?.iter().find(|c| c.id == id)
    }

    /// The cat whose page is open.
    pub fn open_cat_row(&self) -> Option<&Cat> {
        self.cat(self.cats.open.as_deref()?)
    }

    pub fn cat_tab(&self) -> CatTab {
        self.cats.tab.unwrap_or(CatTab::Overview)
    }

    /// The workspace's name for a cat's header ("Acme web").
    pub fn workspace_name_of(&self, id: &str) -> String {
        self.summary.as_ref().and_then(|m| m.workspaces.iter().find(|w| w.id == id)).map(|w| w.name.clone()).unwrap_or_default()
    }

    /// What a face shows for a cat: running tasks and a task that waits for approval.
    pub fn cat_marks(&self, id: &str) -> Marks {
        let Some(m) = self.summary.as_ref() else { return Marks::default() };
        let mut marks = Marks::default();
        for t in m.tasks.iter().filter(|t| m.meta.get(&t.id).and_then(|x| x.cat_id.as_deref()) == Some(id)) {
            marks.working |= matches!(t.status, TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing);
            marks.needs_you |= t.status == TaskStatus::AwaitingApproval;
        }
        marks
    }

    /// A cat's tasks in the states the overview groups (`rows` are task ids, newest first).
    pub fn cat_tasks(&self, id: &str, want: impl Fn(TaskStatus) -> bool) -> Vec<&crate::link::TaskRow> {
        let Some(m) = self.summary.as_ref() else { return Vec::new() };
        let mut rows: Vec<&crate::link::TaskRow> = m.tasks.iter().filter(|t| want(t.status) && m.meta.get(&t.id).and_then(|x| x.cat_id.as_deref()) == Some(id)).collect();
        rows.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
        rows
    }

    // ---- reading the list

    /// A snapshot arrived: ask for the list the first time and whenever `cats_rev` moved (a cat's files or thread changed somewhere).
    pub fn cats_sync(&mut self, rev: u64) {
        let first = self.cats.list.is_none() && !self.cats.asked;
        let moved = self.cats.list.is_some() && rev != self.cats.rev_seen;
        if first || moved {
            self.cats.asked = true;
            self.cats.rev_seen = rev;
            self.call(Call::Cats(Box::new(CatCall::List)));
            // What is open is read again: the cat's own files may have changed under it.
            if moved && let Some(id) = self.cats.open.clone() && self.cat_tab() == CatTab::Files {
                self.call(Call::Cats(Box::new(CatCall::Files { id })));
            }
        }
    }

    // ---- the page

    /// Opens a cat's page in the middle and its panel at the right.
    pub fn open_cat(&mut self, id: &str) {
        if self.cats.open.as_deref() == Some(id) {
            return;
        }
        if !self.guard(Going::Cat(id.to_owned())) {
            return;
        }
        if self.selected.is_some() {
            self.park_draft();
        }
        self.selected = None;
        self.wanted.set(None);
        self.view = None;
        self.plugin_view = None;
        self.new_form = None;
        self.notice = None;
        self.more_open = false;
        self.cats.open = Some(id.to_owned());
        self.panel.expanded = false;
        self.cats.tab = Some(CatTab::Overview);
        self.cats.editor = None;
        self.cats.menu = false;
        self.cats.row_menu = None;
        if !self.layout.context.is_open() {
            self.layout.context.set_open(true);
            self.cats.opened_panel = true;
        }
    }

    /// Anything else was opened (a task, a view, Home): the cat page goes, and the panel it opened with it.
    pub fn leave_cats(&mut self) {
        if self.cats.open.take().is_none() {
            return;
        }
        self.cats.editor = None;
        self.cats.menu = false;
        self.pages.set_field(F_FILE, "");
        if std::mem::take(&mut self.cats.opened_panel) {
            self.layout.context.set_open(false);
            self.panel.expanded = false;
        }
    }

    /// A tab of the panel; leaving Files with an unsaved file asks first.
    pub fn cat_choose_tab(&mut self, tab: CatTab) {
        if self.cat_tab() == tab {
            return;
        }
        if tab != CatTab::Files && !self.guard(Going::CatTab(tab)) {
            return;
        }
        self.cats.tab = Some(tab);
        if tab == CatTab::Files && let Some(id) = self.cats.open.clone() {
            self.call(Call::Cats(Box::new(CatCall::Files { id })));
        } else if tab != CatTab::Files {
            self.cats.editor = None;
        }
    }

    // ---- pin, move, archive

    pub fn cat_pin(&mut self, id: &str, pinned: bool) {
        self.cats.row_menu = None;
        self.cats.menu = false;
        self.call(Call::Cats(Box::new(CatCall::Pin { id: id.to_owned(), pinned })));
    }

    /// Moves a cat `delta` places within its group (the menu's Move up and Move down, and a drag).
    pub fn cat_move(&mut self, id: &str, delta: i32) {
        self.cats.row_menu = None;
        let Some(list) = self.cats.list.as_ref() else { return };
        let ids = reordered(list, id, delta);
        // Nothing moved (the edge of its group): nothing to ask.
        if display_ids(list) == ids {
            return;
        }
        self.call(Call::Cats(Box::new(CatCall::Reorder { ids })));
    }

    /// "Archive miso?": nothing is sent until the coral button is pressed.
    pub fn ask_archive_cat(&mut self, id: &str) {
        self.cats.row_menu = None;
        self.cats.menu = false;
        let Some(name) = self.cat(id).map(|c| c.name.clone()) else { return };
        self.confirm(Ask {
            title: format!("Archive {name}?"),
            detail: "Its tasks and files are kept.".into(),
            ok: "Archive".into(),
            cancel: "Keep".into(),
            destructive: true,
            call: Call::Cats(Box::new(CatCall::Archive { id: id.to_owned() })),
            done: format!("Archived {name}."),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    // ---- dragging a cat in the sidebar (Rune's `on_drag`)

    /// A cat's pointer gesture: Down picks it up, Move carries it by whole places (`step` points each, along `horizontal`), Up drops it.
    pub fn cat_drag(&mut self, id: &str, phase: Phase, travel: (f32, f32), step: f32, horizontal: bool, cancelled: bool) {
        let places = |travel: (f32, f32)| -> i32 { ((if horizontal { travel.0 } else { travel.1 }) / step).round() as i32 };
        match phase {
            Phase::Down => {
                self.cats.suppress_press = false;
                let Some(list) = self.cats.list.as_ref() else { return };
                let among_faces = display(list).0.iter().any(|c| c.id == id);
                self.cats.carry = Some(Carry { id: id.to_owned(), among_faces, moved: false, delta: 0 });
            }
            Phase::Move => {
                let Some(mut carry) = self.cats.carry.take() else { return };
                if !carry.moved && travel.0.hypot(travel.1) > DRAG_SLOP {
                    carry.moved = true;
                }
                if carry.moved {
                    carry.delta = places(travel);
                }
                self.cats.carry = Some(carry);
            }
            Phase::Up => {
                let Some(carry) = self.cats.carry.take() else { return };
                if !carry.moved {
                    return;
                }
                self.cats.suppress_press = true;
                if !cancelled {
                    let delta = places(travel);
                    if delta != 0 {
                        self.cat_move(&carry.id, delta);
                    }
                }
            }
        }
    }

    // ---- the sheet

    fn sheet_defaults(&mut self) -> Sheet {
        let used: Vec<String> = self.cats.list.as_ref().map(|l| l.iter().map(|c| c.face.0.clone()).collect()).unwrap_or_default();
        // The first face nobody wears yet, so a second cat does not look like the first.
        let face = crate::cat_face::ids().into_iter().find(|f| !used.iter().any(|u| u == f)).unwrap_or("black-glasses").to_owned();
        let workspace = self.scope().map(str::to_owned).or_else(|| self.summary.as_ref().and_then(|m| m.workspaces.first()).map(|w| w.id.clone())).unwrap_or_default();
        for key in [F_NAME, F_AREA, F_LIMIT, F_PURPOSE] {
            self.pages.set_field(key, "");
        }
        Sheet { editing: None, face, workspace, model: None, errors: BTreeMap::new(), busy: false, suggest_asked: false, known: Vec::new() }
    }

    /// "New cat" (the sidebar's plus and its empty state).
    pub fn open_new_cat(&mut self) {
        let count = self.cats.list.as_ref().map_or(0, Vec::len);
        if count >= MAX_ACTIVE_CATS {
            self.show_toast(&format!("You can have at most {MAX_ACTIVE_CATS} cats. Archive one first."));
            return;
        }
        self.cats.menu = false;
        self.cats.row_menu = None;
        let mut sheet = self.sheet_defaults();
        // The first cat starts from what the daemon suggests (the busiest workspace of the last 30 days).
        if count == 0 {
            sheet.suggest_asked = true;
            self.call(Call::Cats(Box::new(CatCall::Suggest)));
        }
        self.cats.sheet = Some(sheet);
        self.call(Call::Models);
    }

    /// "Edit" on a cat: the sheet with its words in it.
    pub fn open_edit_cat(&mut self, id: &str) {
        let Some(cat) = self.cat(id).cloned() else { return };
        self.cats.menu = false;
        self.cats.row_menu = None;
        self.pages.set_field(F_NAME, &cat.name);
        self.pages.set_field(F_AREA, &cat.area);
        self.pages.set_field(F_LIMIT, &cat.daily_limit_micro_usd.map(dollars_text).unwrap_or_default());
        self.pages.set_field(F_PURPOSE, &cat.purpose);
        let p = &cat.model_policy.preferences;
        let model = p.provider.clone().map(|provider| (provider, p.model.clone().unwrap_or_default()));
        self.cats.sheet = Some(Sheet { editing: Some(cat.id.clone()), face: cat.face.0.clone(), workspace: cat.workspace_id.clone(), model, errors: BTreeMap::new(), busy: false, suggest_asked: false, known: Vec::new() });
        self.call(Call::Models);
    }

    pub fn close_cat_sheet(&mut self) {
        self.cats.sheet = None;
        self.cats.select = None;
        self.cats.suggestion = None;
    }

    pub fn sheet_face(&mut self, face: &str) {
        if let Some(s) = self.cats.sheet.as_mut() {
            s.face = face.to_owned();
            s.errors.remove("face");
        }
    }

    pub fn sheet_workspace(&mut self, id: &str) {
        self.cats.select = None;
        if let Some(s) = self.cats.sheet.as_mut() {
            s.workspace = id.to_owned();
            s.errors.remove("workspace");
        }
    }

    pub fn sheet_model(&mut self, model: Option<(String, String)>) {
        self.cats.select = None;
        if let Some(s) = self.cats.sheet.as_mut() {
            s.model = model;
            s.errors.remove("model");
        }
    }

    /// The model policy the sheet makes: Neko's default, or the runtime and model chosen.
    fn sheet_policy(&self, s: &Sheet, base: &ModelPolicy) -> ModelPolicy {
        let preferences = match &s.model {
            None => RuntimePreferences { provider: None, model: None, ..base.preferences.clone() },
            Some((provider, model)) => RuntimePreferences { provider: Some(provider.clone()), model: Some(model.clone()).filter(|m| !m.is_empty()), ..base.preferences.clone() },
        };
        ModelPolicy { preferences, per_session: base.per_session }
    }

    /// Why Create is off, or None when the sheet can go.
    pub fn sheet_blocker(&self) -> Option<&'static str> {
        let s = self.cats.sheet.as_ref()?;
        if s.busy {
            return Some("Saving");
        }
        if self.pages.text(F_NAME).trim().is_empty() {
            return Some("Give the cat a name");
        }
        if s.editing.is_none() && s.workspace.is_empty() {
            return Some("Choose a workspace");
        }
        None
    }

    /// Create or Save. What this client can refuse it refuses under the field; the rest is the daemon's to say.
    pub fn sheet_submit(&mut self) {
        let Some(sheet) = self.cats.sheet.clone() else { return };
        if sheet.busy {
            return;
        }
        let mut errors: BTreeMap<&'static str, String> = BTreeMap::new();
        let name = self.pages.text(F_NAME).trim().to_owned();
        let area = self.pages.text(F_AREA).trim().to_owned();
        let purpose = self.pages.text(F_PURPOSE).trim().to_owned();
        if name.is_empty() {
            errors.insert("name", "Give the cat a name".to_owned());
        }
        if sheet.editing.is_none() && sheet.workspace.is_empty() {
            errors.insert("workspace", "Choose a workspace for the cat to look after".to_owned());
        }
        let limit = match micro_from_dollars(self.pages.text(F_LIMIT)) {
            Ok(l) => l,
            Err(e) => {
                errors.insert("limit", e);
                None
            }
        };
        if !errors.is_empty() {
            if let Some(s) = self.cats.sheet.as_mut() {
                s.errors = errors;
            }
            return;
        }
        let call = match &sheet.editing {
            None => CatCall::Create { name, face: sheet.face.clone(), workspace_id: sheet.workspace.clone(), area, model_policy: self.sheet_policy(&sheet, &ModelPolicy::default()), daily_limit_micro_usd: limit, purpose },
            Some(id) => {
                let Some(cat) = self.cat(id).cloned() else { return };
                let policy = self.sheet_policy(&sheet, &cat.model_policy);
                CatCall::Update {
                    id: id.clone(),
                    name: (name != cat.name).then_some(name),
                    face: (sheet.face != cat.face.0).then(|| sheet.face.clone()),
                    area: (area != cat.area).then_some(area),
                    model_policy: (policy != cat.model_policy).then_some(policy),
                    daily_limit_micro_usd: (limit != cat.daily_limit_micro_usd).then_some(limit),
                    purpose: (purpose != cat.purpose).then_some(purpose),
                }
            }
        };
        let known = self.cats.list.as_ref().map(|l| l.iter().map(|c| c.id.clone()).collect()).unwrap_or_default();
        if let Some(s) = self.cats.sheet.as_mut() {
            s.errors.clear();
            s.busy = true;
            s.known = known;
        }
        self.call(Call::Cats(Box::new(call)));
    }

    // ---- the files

    /// Opens one file in the editor.
    pub fn open_cat_file(&mut self, kind: CatFileKind) {
        let Some(id) = self.cats.open.clone() else { return };
        if self.cats.editor.as_ref().is_some_and(|e| e.kind == kind) {
            return;
        }
        if !self.guard(Going::CatFile(Some(kind))) {
            return;
        }
        self.start_editor(&id, kind);
    }

    fn start_editor(&mut self, id: &str, kind: CatFileKind) {
        let file = self.cats.files.get(id).and_then(|f| f.files.iter().find(|x| x.kind == kind));
        let (text, truncated) = file.map(|f| (f.text.clone(), f.truncated)).unwrap_or_default();
        self.pages.set_field(F_FILE, &text);
        self.cats.editor = Some(FileEditor { cat: id.to_owned(), kind, saved: text, error: None, saving: false, truncated });
        if !self.cats.files.contains_key(id) {
            self.call(Call::Cats(Box::new(CatCall::Files { id: id.to_owned() })));
        }
    }

    /// "Back to the files" (the editor's close); an unsaved file asks first.
    pub fn close_cat_file(&mut self) {
        if self.guard(Going::CatFile(None)) {
            self.cats.editor = None;
            self.pages.set_field(F_FILE, "");
        }
    }

    pub fn cat_file_dirty(&self) -> bool {
        self.cats.editor.as_ref().is_some_and(|e| self.pages.text(F_FILE) != e.saved)
    }

    pub fn revert_cat_file(&mut self) {
        if let Some(e) = self.cats.editor.as_mut() {
            e.error = None;
            let saved = e.saved.clone();
            self.pages.set_field(F_FILE, &saved);
        }
    }

    /// Save (the button and ⌘S). Text over 16 KB is refused here with the daemon's own sentence; empty text clears the file, which asks first.
    pub fn save_cat_file(&mut self) {
        let Some(e) = self.cats.editor.clone() else { return };
        let text = self.pages.text(F_FILE).to_owned();
        if text == e.saved || e.saving {
            return;
        }
        if !fits(&text) {
            if let Some(ed) = self.cats.editor.as_mut() {
                ed.error = Some(format!("Keep it under 16 KB. This is {}.", size_words(text.len())));
            }
            return;
        }
        let call = Call::Cats(Box::new(CatCall::WriteFile { id: e.cat.clone(), kind: e.kind, text: text.clone() }));
        if text.trim().is_empty() {
            let name = self.cat(&e.cat).map(|c| c.name.clone()).unwrap_or_default();
            self.confirm(Ask {
                title: format!("Clear {}?", file_title(e.kind)),
                detail: format!("{name}'s {} file is emptied. What it held is kept as {}.prev.", file_title(e.kind), e.kind.file_name()),
                ok: "Clear".into(),
                cancel: "Keep it".into(),
                destructive: true,
                call: Call::Cats(Box::new(CatCall::WriteFile { id: e.cat.clone(), kind: e.kind, text: String::new() })),
                done: format!("Cleared {}.", file_title(e.kind)),
                field: None,
                list: Vec::new(),
                more: Vec::new(),
                toggle: None,
            });
            return;
        }
        if let Some(ed) = self.cats.editor.as_mut() {
            ed.saving = true;
            ed.error = None;
        }
        self.call(call);
    }

    // ---- what came back

    /// A daemon answer to a cat request. True when something on screen changed.
    pub fn cats_reply(&mut self, call: &CatCall, reply: Reply) -> bool {
        match reply {
            Reply::Cats(data) => self.cats_data(call, *data),
            Reply::Refused(message) => self.cats_refused(call, message),
            Reply::Failed(error) => {
                let line = crate::link::calm(&error, std::time::Duration::from_secs(1));
                self.cats_refused(call, line)
            }
            _ => false,
        }
    }

    fn cats_data(&mut self, call: &CatCall, data: CatsData) -> bool {
        match data {
            CatsData::Listing(cats) => {
                self.cats.asked = false;
                self.cats.notice = None;
                let prior: Vec<String> = self.cats.list.as_ref().map(|l| l.iter().map(|c| c.id.clone()).collect()).unwrap_or_default();
                self.cats.list = Some(cats.clone());
                // An open page whose cat is gone (archived here or somewhere else) goes back to Home.
                if let Some(open) = self.cats.open.clone()
                    && !cats.iter().any(|c| c.id == open)
                {
                    self.leave_cats();
                    self.open_new();
                }
                match call {
                    CatCall::Create { .. } => {
                        let known = self.cats.sheet.as_ref().map(|s| s.known.clone()).unwrap_or(prior);
                        let made = cats.iter().find(|c| !known.contains(&c.id)).map(|c| (c.id.clone(), c.name.clone()));
                        self.close_cat_sheet();
                        if let Some((id, name)) = made {
                            self.open_cat(&id);
                            self.show_toast(&format!("{name} is here."));
                        }
                    }
                    CatCall::Update { .. } => self.close_cat_sheet(),
                    _ => {}
                }
                true
            }
            CatsData::Suggestion(s) => {
                // The first cat's sheet starts from the suggestion, unless the person has already typed.
                let fresh = self.cats.sheet.as_ref().is_some_and(|sh| sh.editing.is_none() && sh.suggest_asked) && self.pages.text(F_NAME).is_empty() && self.pages.text(F_AREA).is_empty() && self.pages.text(F_PURPOSE).is_empty();
                if fresh {
                    self.pages.set_field(F_NAME, &s.name);
                    self.pages.set_field(F_AREA, &s.area);
                    self.pages.set_field(F_PURPOSE, &s.purpose);
                    if let Some(sh) = self.cats.sheet.as_mut() {
                        if crate::cat_face::ids().contains(&s.face.0.as_str()) {
                            sh.face = s.face.0.clone();
                        }
                        if self.summary.as_ref().is_some_and(|m| m.workspaces.iter().any(|w| w.id == s.workspace_id)) {
                            sh.workspace = s.workspace_id.clone();
                        }
                    }
                }
                self.cats.suggestion = Some(s);
                true
            }
            CatsData::Files(files) => {
                let id = files.id.clone();
                // A file the editor holds: what we just saved is now what is saved; a file changed elsewhere replaces an untouched editor.
                let dirty = self.cat_file_dirty();
                let editing = self.cats.editor.as_ref().filter(|e| e.cat == id).map(|e| (e.kind, e.saving));
                if let Some((kind, saving)) = editing
                    && let Some(file) = files.files.iter().find(|f| f.kind == kind)
                {
                    let text = file.text.clone();
                    let truncated = file.truncated;
                    if saving || !dirty {
                        self.pages.set_field(F_FILE, &text);
                        if let Some(e) = self.cats.editor.as_mut() {
                            e.saved = text;
                            e.truncated = truncated;
                            e.error = None;
                            e.saving = false;
                        }
                    }
                    if saving {
                        self.show_toast(&format!("Saved {}.", file_title(kind)));
                    }
                }
                self.cats.files.insert(id, files);
                true
            }
        }
    }

    fn cats_refused(&mut self, call: &CatCall, message: String) -> bool {
        match call {
            CatCall::Create { .. } | CatCall::Update { .. } => {
                if let Some(s) = self.cats.sheet.as_mut() {
                    s.busy = false;
                    s.errors.insert(field_of(&message), message);
                }
            }
            CatCall::WriteFile { .. } => {
                if let Some(e) = self.cats.editor.as_mut() {
                    e.saving = false;
                    e.error = Some(message);
                }
            }
            CatCall::List => {
                self.cats.asked = false;
            }
            CatCall::Files { .. } | CatCall::Suggest => {}
            // A refused archive, pin or reorder is said once, in the daemon's words.
            _ => self.show_toast(&message),
        }
        true
    }
}

/// The ids in the order the sidebar draws them (faces, then rows).
fn display_ids(cats: &[Cat]) -> Vec<String> {
    let (faces, rows) = display(cats);
    faces.into_iter().chain(rows).map(|c| c.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(id: &str, order: i32, pinned: bool) -> Cat {
        Cat { id: id.into(), name: id.into(), order, pinned, ..Cat::default() }
    }

    #[test]
    fn up_to_three_pinned_cats_are_faces_in_order_and_the_rest_are_rows() {
        let cats = vec![cat("e", 4, false), cat("a", 0, true), cat("d", 3, true), cat("b", 1, false), cat("c", 2, true), cat("f", 5, true)];
        let (faces, rows) = display(&cats);
        assert_eq!(faces.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["a", "c", "d"]);
        // A fourth pinned cat is a row, in its own place.
        assert_eq!(rows.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["b", "e", "f"]);
    }

    #[test]
    fn a_drag_moves_a_cat_inside_its_own_group_only() {
        let cats = vec![cat("a", 0, true), cat("b", 1, true), cat("c", 2, false), cat("d", 3, false), cat("e", 4, false)];
        assert_eq!(reordered(&cats, "a", 1), ["b", "a", "c", "d", "e"]);
        assert_eq!(reordered(&cats, "e", -2), ["a", "b", "e", "c", "d"]);
        // Past the end of the group stops at the end; a face never becomes a row by dragging.
        assert_eq!(reordered(&cats, "b", 5), ["a", "b", "c", "d", "e"]);
        assert_eq!(reordered(&cats, "c", -9), ["a", "b", "c", "d", "e"]);
        assert_eq!(reordered(&cats, "d", 1), ["a", "b", "c", "e", "d"]);
    }

    #[test]
    fn dollars_become_micro_dollars_and_back() {
        assert_eq!(micro_from_dollars(""), Ok(None));
        assert_eq!(micro_from_dollars("  "), Ok(None));
        assert_eq!(micro_from_dollars("5"), Ok(Some(5_000_000)));
        assert_eq!(micro_from_dollars("$5.50"), Ok(Some(5_500_000)));
        assert_eq!(micro_from_dollars("0.000001"), Ok(Some(1)));
        assert_eq!(micro_from_dollars(".5"), Ok(Some(500_000)));
        for bad in ["five", "5.1234567", "-3", "1,000", "5 dollars", "."] {
            assert!(micro_from_dollars(bad).is_err(), "{bad}");
        }
        assert!(micro_from_dollars("0").is_err(), "a limit of zero would stop the cat");
        assert_eq!(dollars_text(5_000_000), "5.00");
        assert_eq!(dollars_text(5_500_000), "5.50");
        assert_eq!(dollars_text(1), "0.000001");
        assert_eq!(micro_from_dollars(&dollars_text(1_234_567)), Ok(Some(1_234_567)));
    }

    #[test]
    fn a_refusal_lands_under_the_field_it_is_about() {
        for (message, field) in [
            ("Give the cat a name", "name"),
            ("There is already a cat called miso", "name"),
            ("A cat's name can be at most 40 characters", "name"),
            ("The area can be at most 40 characters", "area"),
            ("The purpose can be at most 2000 characters", "purpose"),
            ("The daily limit must be between 1 micro-dollar and 1,000,000 dollars", "limit"),
            ("That workspace no longer exists", "workspace"),
            ("Unknown face \"rainbow\"", "face"),
            ("You can have at most 16 cats; archive one first", "form"),
        ] {
            assert_eq!(field_of(message), field, "{message}");
        }
    }

    #[test]
    fn a_file_fits_up_to_sixteen_kilobytes() {
        assert!(fits(&"a".repeat(16 * 1024)));
        assert!(!fits(&"a".repeat(16 * 1024 + 1)));
        assert_eq!(size_words(512), "512 bytes");
        assert_eq!(size_words(12_595), "12.3 KB");
    }
}
