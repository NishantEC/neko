//! Plugins (docs/knowledge/plugins.md): what Neko keeps of each plugin, and the rules that decide whether one may run.
//!
//! The trust root is the person's approval of a manifest's digest. A plugin runs only when the digest it has now is the digest the
//! person approved; a manifest that changed (an edit, or the file in its folder) is `changed` and is left out of runs until approved
//! again. Nothing here starts a process or touches the Keychain: the daemon does that, with deadlines.
//!
//! Settings the person sets are kept here (`plugin_values`) except a secret, whose row only says it is set; the secret itself is in
//! the Keychain (`secrets.rs`).

use std::collections::BTreeMap;
use std::path::Path;

use neko_protocol::plugins::{self, Access, Approval, Check, FieldKind, FieldValue, MANIFEST_FILE, MAX_PLUGINS, Manifest, Parsed, Plugin, PluginAsk, PluginKind, SettingField};
use rusqlite::{OptionalExtension, Row as SqlRow, params};

use crate::records::log_event;
use crate::store::{Store, StoreError};
use crate::Core;

/// Why a change to a plugin was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum PluginError {
    NotFound,
    /// What was given is not a plugin Neko can keep; the words say why.
    Invalid(String),
    /// Not allowed now (a manifest that changed while the person looked, an unapproved plugin, the limit); the words say why.
    Refused(String),
    Store(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::NotFound => f.write_str("There is no such plugin."),
            PluginError::Invalid(w) | PluginError::Refused(w) => f.write_str(w),
            PluginError::Store(e) => write!(f, "The plugin could not be saved: {e}"),
        }
    }
}

impl From<StoreError> for PluginError {
    fn from(e: StoreError) -> Self {
        PluginError::Store(e.to_string())
    }
}

/// A plugin as stored.
#[derive(Clone, Debug)]
pub struct Row {
    pub id: String,
    pub kind: PluginKind,
    pub folder: Option<String>,
    pub manifest: Manifest,
    pub digest: String,
    pub approved_digest: Option<String>,
    pub approved_manifest: Option<Manifest>,
    pub approved_ms: Option<i64>,
    pub declined_digest: Option<String>,
    pub enabled: bool,
    pub access: Access,
    pub last_check: Option<Check>,
    pub problem: Option<String>,
    pub created_ms: i64,
    pub changed_ms: i64,
    /// When the manifest became the one it is now (added, or changed): how long the plugin has waited to be approved.
    pub manifest_ms: i64,
}

impl Row {
    pub fn approval(&self) -> Approval {
        if self.approved_digest.as_deref() == Some(self.digest.as_str()) {
            Approval::Approved
        } else if self.declined_digest.as_deref() == Some(self.digest.as_str()) {
            Approval::Declined
        } else if self.approved_digest.is_some() {
            Approval::Changed
        } else {
            Approval::Waiting
        }
    }
}

const COLUMNS: &str = "id, kind, folder, manifest, digest, approved_digest, approved_manifest, approved_ms, declined_digest, enabled, access_mode, access_cats, last_check, problem, created_ms, changed_ms, manifest_ms";

fn json_err(i: usize, e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
}

fn row_of(r: &SqlRow<'_>) -> rusqlite::Result<Row> {
    let manifest: String = r.get(3)?;
    let approved: Option<String> = r.get(6)?;
    let mode: String = r.get(10)?;
    let cats: String = r.get(11)?;
    let check: Option<String> = r.get(12)?;
    Ok(Row {
        id: r.get(0)?,
        kind: crate::store::status_of(&r.get::<_, String>(1)?)?,
        folder: r.get(2)?,
        manifest: serde_json::from_str(&manifest).map_err(|e| json_err(3, e))?,
        digest: r.get(4)?,
        approved_digest: r.get(5)?,
        approved_manifest: approved.map(|a| serde_json::from_str(&a)).transpose().map_err(|e| json_err(6, e))?,
        approved_ms: r.get(7)?,
        declined_digest: r.get(8)?,
        enabled: r.get::<_, i64>(9)? != 0,
        access: Access { mode: crate::store::status_of(&mode)?, cats: serde_json::from_str(&cats).map_err(|e| json_err(11, e))? },
        last_check: check.map(|c| serde_json::from_str(&c)).transpose().map_err(|e| json_err(12, e))?,
        problem: r.get(13)?,
        created_ms: r.get(14)?,
        changed_ms: r.get(15)?,
        manifest_ms: r.get(16)?,
    })
}

/// What is stored of one setting: its value (None for a secret, which is in the Keychain) and that it is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub value: Option<String>,
    pub secret: bool,
}

fn event(tx: &rusqlite::Transaction<'_>, now: i64, id: &str, kind: &str, data: serde_json::Value) -> Result<u64, StoreError> {
    log_event(tx, now, id, kind, data)
}

impl Store {
    pub fn plugin_rows(&self) -> Result<Vec<Row>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {COLUMNS} FROM plugins ORDER BY created_ms, id"))?;
        let rows = st.query_map([], row_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn plugin_row(&self, id: &str) -> Result<Option<Row>, StoreError> {
        Ok(self.conn.query_row(&format!("SELECT {COLUMNS} FROM plugins WHERE id = ?1"), [id], row_of).optional()?)
    }

    pub fn plugin_values(&self, id: &str) -> Result<BTreeMap<String, Stored>, StoreError> {
        let mut st = self.conn.prepare_cached("SELECT key, value, secret FROM plugin_values WHERE plugin_id = ?1")?;
        let rows = st.query_map([id], |r| Ok((r.get::<_, String>(0)?, Stored { value: r.get(1)?, secret: r.get::<_, i64>(2)? != 0 })))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every plugin as a screen sees it.
    pub fn plugins(&self) -> Result<Vec<Plugin>, StoreError> {
        self.plugin_rows()?.into_iter().map(|r| self.wire(r)).collect()
    }

    pub fn plugin(&self, id: &str) -> Result<Option<Plugin>, StoreError> {
        self.plugin_row(id)?.map(|r| self.wire(r)).transpose()
    }

    fn wire(&self, r: Row) -> Result<Plugin, StoreError> {
        let stored = self.plugin_values(&r.id)?;
        Ok(to_wire(r, &stored))
    }

    /// Writes new plugins (and which of their secrets have values) in one transaction, one event each.
    fn insert_plugins(&mut self, rows: &[(Row, Vec<String>)], now: i64) -> Result<Vec<u64>, StoreError> {
        let tx = self.conn.transaction()?;
        let mut seqs = Vec::new();
        for (r, secret_keys) in rows {
            tx.execute(
                "INSERT INTO plugins (id, kind, folder, manifest, digest, enabled, access_mode, access_cats, created_ms, changed_ms, name, name_key, manifest_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?9)",
                params![
                    r.id,
                    crate::store::status_name(r.kind),
                    r.folder,
                    serde_json::to_string(&r.manifest).unwrap_or_default(),
                    r.digest,
                    r.enabled,
                    crate::store::status_name(r.access.mode),
                    serde_json::to_string(&r.access.cats).unwrap_or_default(),
                    r.created_ms,
                    r.changed_ms,
                    r.manifest.name,
                    r.manifest.name.to_lowercase()
                ],
            )?;
            for k in secret_keys {
                tx.execute("INSERT INTO plugin_values (plugin_id, key, value, secret) VALUES (?1, ?2, NULL, 1)", params![r.id, k])?;
            }
            seqs.push(event(&tx, now, &r.id, "plugin_added", serde_json::json!({ "name": r.manifest.name, "digest": r.digest, "from": if r.folder.is_some() { "folder" } else { "paste" } }))?);
        }
        tx.commit()?;
        Ok(seqs)
    }

    /// Writes the row's changeable columns and one event.
    fn write_plugin(&mut self, r: &Row, kind: &str, data: serde_json::Value, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE plugins SET name = ?2, name_key = ?3, manifest = ?4, digest = ?5, approved_digest = ?6, approved_manifest = ?7, approved_ms = ?8, declined_digest = ?9, enabled = ?10,
                access_mode = ?11, access_cats = ?12, last_check = ?13, problem = ?14, changed_ms = ?15, manifest_ms = ?16 WHERE id = ?1",
            params![
                r.id,
                r.manifest.name,
                r.manifest.name.to_lowercase(),
                serde_json::to_string(&r.manifest).unwrap_or_default(),
                r.digest,
                r.approved_digest,
                r.approved_manifest.as_ref().and_then(|m| serde_json::to_string(m).ok()),
                r.approved_ms,
                r.declined_digest,
                r.enabled,
                crate::store::status_name(r.access.mode),
                serde_json::to_string(&r.access.cats).unwrap_or_default(),
                r.last_check.as_ref().and_then(|c| serde_json::to_string(c).ok()),
                r.problem,
                now,
                r.manifest_ms
            ],
        )?;
        let seq = event(&tx, now, &r.id, kind, data)?;
        tx.commit()?;
        Ok(seq)
    }

    /// Notes that a setting has a value (a secret's row holds none) or has not, with one event that never carries the value.
    fn write_value(&mut self, id: &str, key: &str, stored: Option<&Stored>, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        match stored {
            Some(s) => {
                tx.execute(
                    "INSERT INTO plugin_values (plugin_id, key, value, secret) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, secret = ?4",
                    params![id, key, s.value, s.secret],
                )?;
            }
            None => {
                tx.execute("DELETE FROM plugin_values WHERE plugin_id = ?1 AND key = ?2", params![id, key])?;
            }
        }
        tx.execute("UPDATE plugins SET changed_ms = ?2 WHERE id = ?1", params![id, now])?;
        let seq = event(&tx, now, id, "plugin_setting", serde_json::json!({ "key": key, "set": stored.is_some() }))?;
        tx.commit()?;
        Ok(seq)
    }

    fn delete_plugin(&mut self, id: &str, name: &str, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM plugins WHERE id = ?1", [id])?;
        let seq = event(&tx, now, id, "plugin_removed", serde_json::json!({ "name": name }))?;
        tx.commit()?;
        Ok(seq)
    }
}

/// A row and what is stored of its settings as a screen sees them.
pub fn to_wire(r: Row, stored: &BTreeMap<String, Stored>) -> Plugin {
    let approval = r.approval();
    let dir = r.folder.as_deref();
    let changes = match (&approval, &r.approved_manifest) {
        (Approval::Changed | Approval::Declined, Some(old)) => r.manifest.changes_from(old, dir),
        _ => Vec::new(),
    };
    let mut values = Vec::new();
    let mut missing = Vec::new();
    for f in &r.manifest.settings {
        let s = stored.get(&f.key);
        let set = s.is_some_and(|s| s.secret || s.value.as_deref().is_some_and(|v| !v.is_empty()));
        values.push(FieldValue { key: f.key.clone(), value: s.filter(|s| !s.secret).and_then(|s| s.value.clone()), set });
        if f.required && !set && f.default.is_none() {
            missing.push(f.label.clone());
        }
    }
    Plugin {
        id: r.id,
        name: r.manifest.name.clone(),
        version: r.manifest.version.clone(),
        description: r.manifest.description.clone(),
        kind: r.kind,
        folder: r.folder,
        digest: r.digest,
        manifest: r.manifest,
        approval,
        approved_digest: r.approved_digest,
        approved_ms: r.approved_ms,
        changes,
        enabled: r.enabled,
        access: r.access,
        values,
        missing,
        last_check: r.last_check,
        problem: r.problem,
        created_ms: r.created_ms,
        changed_ms: r.changed_ms,
    }
}

/// What "Needs you" shows for a plugin waiting for approval: exactly what will run.
pub fn ask_of(p: &Plugin) -> PluginAsk {
    PluginAsk {
        plugin: p.id.clone(),
        name: p.name.clone(),
        version: p.version.clone(),
        digest: p.digest.clone(),
        runs: p.manifest.command_line(p.folder.as_deref()),
        remote: p.manifest.mcp.is_remote(),
        passes: p.manifest.passes(),
        changed: p.approval == Approval::Changed,
        changes: p.changes.clone(),
        description: p.description.clone(),
        folder: p.folder.clone(),
    }
}

/// Reads a plugin folder's manifest. The folder must exist and hold a `neko-plugin.json` Neko can keep. Returns the manifest and the
/// folder's real path (links resolved).
pub fn read_folder(folder: &str) -> Result<(Manifest, String), PluginError> {
    let path = Path::new(folder);
    let real = path.canonicalize().map_err(|_| PluginError::Invalid(format!("{folder} is not a folder Neko can open.")))?;
    if !real.is_dir() {
        return Err(PluginError::Invalid(format!("{folder} is not a folder.")));
    }
    let file = real.join(MANIFEST_FILE);
    let text = read_manifest(&file).map_err(|e| match e {
        ReadError::Missing => PluginError::Invalid(format!("There is no {MANIFEST_FILE} in {}.", real.display())),
        ReadError::Bad(w) => PluginError::Invalid(w),
    })?;
    Ok((text, real.to_string_lossy().into_owned()))
}

/// Looks at a plugin's folder again: its manifest, or the words for why there is none. It reads files, so it may be slow: no lock is
/// held while it runs.
pub fn read_plugin_file(folder: &str) -> Result<Manifest, String> {
    if !Path::new(folder).is_dir() {
        return Err(format!("The folder {folder} is not there."));
    }
    read_manifest(&Path::new(folder).join(MANIFEST_FILE)).map_err(|e| match e {
        ReadError::Missing => format!("{MANIFEST_FILE} is not in {folder} any more."),
        ReadError::Bad(w) => w,
    })
}

enum ReadError {
    Missing,
    Bad(String),
}

fn read_manifest(file: &Path) -> Result<Manifest, ReadError> {
    use std::io::Read;
    let f = std::fs::File::open(file).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { ReadError::Missing } else { ReadError::Bad(format!("{} cannot be read: {e}.", file.display())) })?;
    let mut text = String::new();
    f.take(plugins::MAX_MANIFEST_BYTES as u64 + 1).read_to_string(&mut text).map_err(|e| ReadError::Bad(format!("{MANIFEST_FILE} cannot be read: {e}.")))?;
    Manifest::parse_file(&text).map_err(ReadError::Bad)
}

impl Core {
    pub fn plugins(&self) -> Result<Vec<Plugin>, StoreError> {
        self.store.plugins()
    }

    fn plugin_row(&self, id: &str) -> Result<Row, PluginError> {
        self.store.plugin_row(id)?.ok_or(PluginError::NotFound)
    }

    fn done(&self, id: &str, seq: u64) -> Result<(Plugin, u64), PluginError> {
        Ok((self.store.plugin(id)?.ok_or(PluginError::NotFound)?, seq))
    }

    fn name_taken(&self, name: &str, except: Option<&str>) -> Result<bool, PluginError> {
        Ok(self.store.plugin_rows()?.iter().any(|r| r.manifest.name.eq_ignore_ascii_case(name) && Some(r.id.as_str()) != except))
    }

    /// Adds the servers of a paste, or a folder's manifest, all or none. Each waits for approval; nothing runs. A paste's secrets are
    /// marked set here, and the caller keeps them in the Keychain under the new plugin's id (and removes the plugins again if it cannot).
    pub fn add_plugins(&mut self, found: Vec<Parsed>, folder: Option<&str>) -> Result<Vec<(Plugin, u64)>, PluginError> {
        if self.store.plugin_rows()?.len() + found.len() > MAX_PLUGINS {
            return Err(PluginError::Refused(format!("Neko keeps at most {MAX_PLUGINS} plugins. Remove one first.")));
        }
        let mut names = Vec::new();
        for p in &found {
            if self.name_taken(&p.manifest.name, None)? || names.contains(&p.manifest.name.to_lowercase()) {
                return Err(PluginError::Refused(format!("There is already a plugin called {}. Remove it first, or rename this server.", p.manifest.name)));
            }
            names.push(p.manifest.name.to_lowercase());
        }
        let now = self.now();
        let rows: Vec<(Row, Vec<String>)> = found
            .into_iter()
            .map(|p| {
                let id = self.new_id("plugin");
                let digest = p.manifest.digest(folder);
                let secrets = p.secrets.iter().map(|(k, _)| k.clone()).collect();
                (
                    Row { id, kind: p.manifest.kind, folder: folder.map(str::to_owned), digest, manifest: p.manifest, approved_digest: None, approved_manifest: None, approved_ms: None, declined_digest: None, enabled: true, access: Access::default(), last_check: None, problem: None, created_ms: now, changed_ms: now, manifest_ms: now },
                    secrets,
                )
            })
            .collect();
        let seqs = self.store.insert_plugins(&rows, now)?;
        rows.iter().zip(seqs).map(|((r, _), seq)| self.done(&r.id, seq)).collect()
    }

    /// Replaces a pasted plugin's manifest. The approval no longer matches it, so the plugin is `changed` until approved again.
    pub fn update_plugin_manifest(&mut self, id: &str, manifest: Manifest) -> Result<(Plugin, u64), PluginError> {
        let mut r = self.plugin_row(id)?;
        if r.folder.is_some() {
            return Err(PluginError::Refused("This plugin comes from a folder: edit the neko-plugin.json there.".into()));
        }
        manifest.validate().map_err(PluginError::Invalid)?;
        if self.name_taken(&manifest.name, Some(id))? {
            return Err(PluginError::Refused(format!("There is already a plugin called {}.", manifest.name)));
        }
        self.adopt(&mut r, manifest);
        let now = self.now();
        let seq = self.store.write_plugin(&r, "plugin_changed", serde_json::json!({ "name": r.manifest.name, "digest": r.digest }), now)?;
        self.done(id, seq)
    }

    fn adopt(&self, r: &mut Row, manifest: Manifest) {
        r.digest = manifest.digest(r.folder.as_deref());
        r.manifest = manifest;
        r.problem = None;
        r.manifest_ms = self.now();
    }

    /// Takes what a look at a plugin's folder found (`read_plugin_file`, done without any lock held). A manifest that differs from
    /// the one kept is adopted (and the plugin is `changed` if it had been approved); a folder that is gone or a manifest that cannot
    /// be read is a `problem`. None: nothing differs.
    pub fn apply_folder_read(&mut self, id: &str, read: Result<Manifest, String>) -> Result<Option<(Plugin, u64)>, PluginError> {
        let mut r = self.plugin_row(id)?;
        let Some(folder) = r.folder.clone() else { return Ok(None) };
        let now = self.now();
        match read {
            Ok(m) => {
                let digest = m.digest(Some(&folder));
                if digest == r.digest && r.problem.is_none() {
                    return Ok(None);
                }
                if digest != r.digest {
                    if self.name_taken(&m.name, Some(id))? {
                        r.problem = Some(format!("The manifest now names the plugin {}, which another plugin already has.", m.name));
                        let seq = self.store.write_plugin(&r, "plugin_problem", serde_json::json!({ "name": r.manifest.name }), now)?;
                        return self.done(id, seq).map(Some);
                    }
                    self.adopt(&mut r, m);
                    let seq = self.store.write_plugin(&r, "plugin_changed", serde_json::json!({ "name": r.manifest.name, "digest": r.digest }), now)?;
                    return self.done(id, seq).map(Some);
                }
                r.problem = None;
                let seq = self.store.write_plugin(&r, "plugin_problem_cleared", serde_json::json!({ "name": r.manifest.name }), now)?;
                self.done(id, seq).map(Some)
            }
            Err(words) => {
                if r.problem.as_deref() == Some(words.as_str()) {
                    return Ok(None);
                }
                r.problem = Some(words);
                let seq = self.store.write_plugin(&r, "plugin_problem", serde_json::json!({ "name": r.manifest.name }), now)?;
                self.done(id, seq).map(Some)
            }
        }
    }

    /// The person approves exactly the manifest with `digest`. Refused when the plugin has another by now.
    pub fn approve_plugin(&mut self, id: &str, digest: &str) -> Result<(Plugin, u64), PluginError> {
        let mut r = self.plugin_row(id)?;
        if r.digest != digest {
            return Err(PluginError::Refused("This plugin changed while you were looking at it. Look at it again before approving.".into()));
        }
        if r.problem.is_some() {
            return Err(PluginError::Refused("Neko cannot read this plugin's manifest right now, so there is nothing to approve.".into()));
        }
        let now = self.now();
        r.approved_digest = Some(r.digest.clone());
        r.approved_manifest = Some(r.manifest.clone());
        r.approved_ms = Some(now);
        r.declined_digest = None;
        let seq = self.store.write_plugin(&r, "plugin_approved", serde_json::json!({ "name": r.manifest.name, "digest": r.digest }), now)?;
        self.done(id, seq)
    }

    pub fn decline_plugin(&mut self, id: &str) -> Result<(Plugin, u64), PluginError> {
        let mut r = self.plugin_row(id)?;
        if r.approval() == Approval::Approved {
            return Err(PluginError::Refused("This plugin is approved. Turn it off, or remove it.".into()));
        }
        r.declined_digest = Some(r.digest.clone());
        let now = self.now();
        let seq = self.store.write_plugin(&r, "plugin_declined", serde_json::json!({ "name": r.manifest.name, "digest": r.digest }), now)?;
        self.done(id, seq)
    }

    pub fn set_plugin_enabled(&mut self, id: &str, on: bool) -> Result<(Plugin, u64), PluginError> {
        let mut r = self.plugin_row(id)?;
        r.enabled = on;
        let now = self.now();
        let seq = self.store.write_plugin(&r, if on { "plugin_enabled" } else { "plugin_disabled" }, serde_json::json!({ "name": r.manifest.name }), now)?;
        self.done(id, seq)
    }

    pub fn set_plugin_access(&mut self, id: &str, access: Access) -> Result<(Plugin, u64), PluginError> {
        let mut r = self.plugin_row(id)?;
        let cats = self.store.list_cats()?;
        let mut seen = Vec::new();
        for c in &access.cats {
            if !cats.iter().any(|k| &k.id == c) {
                return Err(PluginError::Refused("One of those cats is not there any more.".into()));
            }
            if !seen.contains(c) {
                seen.push(c.clone());
            }
        }
        if access.mode == plugins::AccessMode::Only && seen.is_empty() {
            return Err(PluginError::Refused("Choose at least one cat, or let every cat use it.".into()));
        }
        r.access = Access { mode: access.mode, cats: if access.mode == plugins::AccessMode::All { Vec::new() } else { seen } };
        let now = self.now();
        let seq = self.store.write_plugin(&r, "plugin_access", serde_json::json!({ "name": r.manifest.name, "mode": crate::store::status_name(r.access.mode), "cats": r.access.cats }), now)?;
        self.done(id, seq)
    }

    /// Checks a setting's value against what the plugin declared: which field it is, and that the value fits. `value` None clears.
    pub fn check_plugin_setting(&self, id: &str, key: &str, value: Option<&str>) -> Result<SettingField, PluginError> {
        let r = self.plugin_row(id)?;
        let f = r.manifest.settings.iter().find(|f| f.key == key).ok_or_else(|| PluginError::Invalid(format!("This plugin has no setting called {key}.")))?.clone();
        let Some(v) = value else { return Ok(f) };
        let bad = |w: String| Err(PluginError::Invalid(w));
        match f.kind {
            FieldKind::Secret if v.len() > plugins::MAX_SECRET => return bad("That secret is too long.".into()),
            FieldKind::Secret => {}
            _ if v.chars().count() > plugins::MAX_VALUE => return bad(format!("{} can be at most {} characters.", f.label, plugins::MAX_VALUE)),
            FieldKind::YesNo if v != "yes" && v != "no" => return bad(format!("{} is yes or no.", f.label)),
            FieldKind::Choice if !f.choices.iter().any(|c| c == v) => return bad(format!("That is not one of the choices for {}.", f.label)),
            FieldKind::Folder if !v.starts_with('/') => return bad(format!("{} must be a folder's full path.", f.label)),
            _ => {}
        }
        Ok(f)
    }

    /// Records a setting: a plain value is kept here; a secret only as set (the caller put it in the Keychain). None clears it.
    pub fn put_plugin_setting(&mut self, id: &str, key: &str, value: Option<&str>) -> Result<(Plugin, u64), PluginError> {
        let f = self.check_plugin_setting(id, key, value)?;
        let stored = value.filter(|v| !v.is_empty()).map(|v| if f.kind == FieldKind::Secret { Stored { value: None, secret: true } } else { Stored { value: Some(v.to_owned()), secret: false } });
        let now = self.now();
        let seq = self.store.write_value(id, key, stored.as_ref(), now)?;
        self.done(id, seq)
    }

    pub fn record_plugin_check(&mut self, id: &str, check: Check) -> Result<(Plugin, u64), PluginError> {
        let mut r = self.plugin_row(id)?;
        let data = serde_json::json!({ "name": r.manifest.name, "ok": check.ok, "tools": check.tools.len() });
        r.last_check = Some(check);
        let now = self.now();
        let seq = self.store.write_plugin(&r, "plugin_checked", data, now)?;
        // A check does not count as a change of the plugin itself.
        self.done(id, seq)
    }

    /// Removes the plugin and its settings. Returns the secret accounts to delete from the Keychain, and the event's revision.
    pub fn remove_plugin(&mut self, id: &str) -> Result<(Vec<String>, u64), PluginError> {
        let r = self.plugin_row(id)?;
        let accounts: Vec<String> = r.manifest.settings.iter().filter(|f| f.kind == FieldKind::Secret).map(|f| crate::secrets::account(id, &f.key)).collect();
        let now = self.now();
        let seq = self.store.delete_plugin(id, &r.manifest.name, now)?;
        Ok((accounts, seq))
    }

    /// The plugins whose servers go to a run of `cat`: approved as they are now, on, set up and open to the cat.
    pub fn plugins_for(&self, cat: &str) -> Result<Vec<Plugin>, StoreError> {
        Ok(self.store.plugins()?.into_iter().filter(|p| p.runs_for(cat)).collect())
    }
}

/// The text a setting's value takes in a command line, environment or header: what is set, else its default; a yes/no as `true` or
/// `false`; nothing as an empty string.
pub fn value_text(f: &SettingField, stored: Option<&Stored>, secret: Option<&str>) -> String {
    let raw = match f.kind {
        FieldKind::Secret => secret.map(str::to_owned),
        _ => stored.and_then(|s| s.value.clone()).filter(|v| !v.is_empty()).or_else(|| f.default.clone()),
    };
    match (f.kind, raw) {
        (FieldKind::YesNo, Some(v)) => (v == "yes").to_string(),
        (_, Some(v)) => v,
        (_, None) => String::new(),
    }
}
