//! Plugins in the daemon (docs/knowledge/plugins.md): the requests, the Keychain, the check of a server, and what a run is handed.
//!
//! Nothing that can wait on something outside the daemon runs on a connection's thread or under the core's lock: a Keychain call, a
//! read of a plugin's folder, starting a server and its handshake all run on a thread of their own with a deadline, and the answer is
//! sent to the connection when it is ready. So a stop never waits on a plugin (the lesson of `docs/knowledge/run-guard.md`), and a
//! server that hangs or a keychain that asks the person a question holds nothing up. A server a check starts runs under the run guard:
//! when the daemon ends, however it ends, the guardian kills it and everything it started.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use neko_core::mcp::{self, Target};
use neko_core::plugins::PluginError;
use neko_core::secrets::{self, Secrets};
use neko_protocol::plugins::{self, Approval, Check, FieldKind, Parsed, Plugin};
use neko_protocol::{Change, ErrorCode, Reply, Request, ServerMessage};

use crate::server::{Shared, lock};

/// How long one Keychain call may take (the person may be asked a question).
const KEYCHAIN_LIMIT: Duration = Duration::from_secs(20);
/// How often the folders of plugins are looked at while a screen watches.
const WATCH_EVERY: Duration = Duration::from_secs(3);
/// Where a run's own MCP config lives, under the data folder.
pub const RUN_FILES: &str = "plugins-run";

/// What the daemon keeps for plugins.
pub struct Hub {
    secrets: Arc<dyn Secrets>,
    /// The plugins being checked now: a second check of one is refused while the first runs.
    checking: Mutex<HashSet<String>>,
}

impl Hub {
    pub fn new() -> Hub {
        Hub { secrets: Arc::from(secrets::from_environment()), checking: Mutex::new(HashSet::new()) }
    }
}

fn error(e: PluginError) -> Reply {
    let code = match &e {
        PluginError::NotFound => ErrorCode::NotFound,
        PluginError::Invalid(_) => ErrorCode::InvalidPlugin,
        PluginError::Refused(_) => ErrorCode::Refused,
        PluginError::Store(_) => ErrorCode::Internal,
    };
    Reply::error(code, e.to_string())
}

/// Tells every screen what changed. Called with the core locked, so events leave in revision order.
fn updated(shared: &Shared, plugin: &Plugin, revision: u64) {
    shared.broadcast(&ServerMessage::Event { revision, change: Change::PluginUpdated { plugin: plugin.clone() } });
}

fn one(shared: &Shared, done: Result<(Plugin, u64), PluginError>) -> Reply {
    match done {
        Ok((plugin, revision)) => {
            updated(shared, &plugin, revision);
            Reply::Plugin { plugin }
        }
        Err(e) => error(e),
    }
}

/// Runs `f` (a Keychain call) on a thread of its own and waits for it up to `limit`, noticing a stop. A call that never returns
/// leaves its thread behind and the caller with words for the person.
fn deadline<T: Send + 'static>(shared: &Shared, limit: Duration, f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new().name("neko-keychain".into()).spawn(move || {
        let _ = tx.send(f());
    }).map_err(|e| e.to_string())?;
    let until = Instant::now() + limit;
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(r) => return r,
            Err(RecvTimeoutError::Disconnected) => return Err("The Keychain call ended without an answer.".into()),
            Err(RecvTimeoutError::Timeout) => {
                if shared.stopping.load(Ordering::SeqCst) {
                    return Err("Neko is stopping.".into());
                }
                if Instant::now() >= until {
                    return Err("The Keychain did not answer in time. If macOS is asking whether Neko may use it, allow that and try again.".into());
                }
            }
        }
    }
}

fn keychain_get(shared: &Shared, account: String) -> Result<Option<String>, String> {
    let s = shared.plugins.secrets.clone();
    deadline(shared, KEYCHAIN_LIMIT, move || s.get(&account))
}

fn keychain_set(shared: &Shared, account: String, value: String) -> Result<(), String> {
    let s = shared.plugins.secrets.clone();
    deadline(shared, KEYCHAIN_LIMIT, move || s.set(&account, &value))
}

fn keychain_delete(shared: &Shared, account: String) -> Result<(), String> {
    let s = shared.plugins.secrets.clone();
    deadline(shared, KEYCHAIN_LIMIT, move || s.delete(&account))
}

/// The secrets a plugin's set secret fields hold, from the Keychain: setting key to value. An error says which one failed.
fn read_secrets(shared: &Shared, p: &Plugin) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for f in p.manifest.settings.iter().filter(|f| f.kind == FieldKind::Secret) {
        if !p.value(&f.key).is_some_and(|v| v.set) {
            continue;
        }
        match keychain_get(shared, secrets::account(&p.id, &f.key)) {
            Ok(Some(v)) => {
                out.insert(f.key.clone(), v);
            }
            Ok(None) => return Err(format!("The Keychain no longer has {}. Set it again in Settings.", f.label)),
            Err(e) => return Err(format!("{e} ({})", f.label)),
        }
    }
    Ok(out)
}

/// Whether a request is one that may wait on something outside the daemon (the Keychain, a folder, a server): the connection does
/// not wait for it; the reply is sent when it is ready.
pub fn is_slow(r: &Request) -> bool {
    matches!(r, Request::AddMcpJson { .. } | Request::AddPluginFolder { .. } | Request::SetPluginSetting { .. } | Request::TestPlugin { .. } | Request::RemovePlugin { .. })
}

/// Answers a slow request on a thread of its own, sending the reply to the connection that asked.
pub fn spawn_slow(shared: &Arc<Shared>, conn: u64, reply_id: u64, request: Request) {
    let s = shared.clone();
    let spawned = std::thread::Builder::new().name("neko-plugin-request".into()).spawn(move || {
        let reply = slow(&s, request);
        s.send_to(conn, &ServerMessage::Reply { id: reply_id, reply });
    });
    if spawned.is_err() {
        shared.send_to(conn, &ServerMessage::Reply { id: reply_id, reply: Reply::error(ErrorCode::Internal, "Neko could not start a thread for this.") });
    }
}

fn slow(shared: &Arc<Shared>, request: Request) -> Reply {
    match request {
        Request::AddMcpJson { json, name } => add_json(shared, &json, name.as_deref()),
        Request::AddPluginFolder { folder } => add_folder(shared, &folder),
        Request::SetPluginSetting { plugin, key, value } => set_setting(shared, &plugin, &key, value),
        Request::TestPlugin { plugin } => test(shared, &plugin),
        Request::RemovePlugin { plugin } => remove(shared, &plugin),
        _ => Reply::error(ErrorCode::BadRequest, "Not a plugin request."),
    }
}

/// The requests answered at once, from the store alone. None: not one of them.
pub fn quick(shared: &Arc<Shared>, request: &Request) -> Option<Reply> {
    Some(match request {
        Request::ListPlugins => match lock(&shared.core).plugins() {
            Ok(plugins) => Reply::Plugins { plugins },
            Err(e) => Reply::error(ErrorCode::Internal, e.to_string()),
        },
        Request::UpdatePluginManifest { plugin, manifest } => {
            let mut core = lock(&shared.core);
            let done = core.update_plugin_manifest(plugin, manifest.clone());
            one(shared, done)
        }
        Request::ApprovePlugin { plugin, digest } => {
            let mut core = lock(&shared.core);
            let done = core.approve_plugin(plugin, digest);
            one(shared, done)
        }
        Request::DeclinePlugin { plugin } => {
            let mut core = lock(&shared.core);
            let done = core.decline_plugin(plugin);
            one(shared, done)
        }
        Request::SetPluginEnabled { plugin, on } => {
            let mut core = lock(&shared.core);
            let done = core.set_plugin_enabled(plugin, *on);
            one(shared, done)
        }
        Request::SetPluginAccess { plugin, access } => {
            let mut core = lock(&shared.core);
            let done = core.set_plugin_access(plugin, access.clone());
            one(shared, done)
        }
        _ => return None,
    })
}

fn add(shared: &Arc<Shared>, found: Vec<Parsed>, folder: Option<&str>) -> Reply {
    let secrets: Vec<Vec<(String, String)>> = found.iter().map(|p| p.secrets.clone()).collect();
    let added = {
        let mut core = lock(&shared.core);
        match core.add_plugins(found, folder) {
            Ok(a) => {
                for (p, rev) in &a {
                    shared.broadcast(&ServerMessage::Event { revision: *rev, change: Change::PluginAdded { plugin: p.clone() } });
                }
                a
            }
            Err(e) => return error(e),
        }
    };
    // The secrets of a paste go to the Keychain now, outside any lock. If one cannot, nothing is left half done.
    let mut kept: Vec<String> = Vec::new();
    for ((p, _), secs) in added.iter().zip(&secrets) {
        for (key, value) in secs {
            let account = secrets::account(&p.id, key);
            match keychain_set(shared, account.clone(), value.clone()) {
                Ok(()) => kept.push(account),
                Err(e) => {
                    for a in kept {
                        let _ = keychain_delete(shared, a);
                    }
                    let mut core = lock(&shared.core);
                    for (p, _) in &added {
                        if let Ok((_, rev)) = core.remove_plugin(&p.id) {
                            shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::PluginRemoved { plugin: p.id.clone() } });
                        }
                    }
                    return Reply::error(ErrorCode::Internal, format!("Nothing was added. {e}"));
                }
            }
        }
    }
    poke();
    Reply::Plugins { plugins: added.into_iter().map(|(p, _)| p).collect() }
}

fn add_json(shared: &Arc<Shared>, json: &str, name: Option<&str>) -> Reply {
    match plugins::parse_mcp_json(json, name) {
        Ok(found) => add(shared, found, None),
        Err(why) => Reply::error(ErrorCode::InvalidPlugin, why),
    }
}

fn add_folder(shared: &Arc<Shared>, folder: &str) -> Reply {
    match neko_core::plugins::read_folder(folder) {
        Ok((manifest, real)) => match add(shared, vec![Parsed { manifest, secrets: Vec::new(), notes: Vec::new() }], Some(&real)) {
            Reply::Plugins { mut plugins } if !plugins.is_empty() => Reply::Plugin { plugin: plugins.remove(0) },
            other => other,
        },
        Err(e) => error(e),
    }
}

fn set_setting(shared: &Arc<Shared>, id: &str, key: &str, value: Option<String>) -> Reply {
    let value = value.filter(|v| !v.is_empty());
    let field = match lock(&shared.core).check_plugin_setting(id, key, value.as_deref()) {
        Ok(f) => f,
        Err(e) => return error(e),
    };
    if field.kind == FieldKind::Secret {
        let account = secrets::account(id, key);
        let done = match &value {
            Some(v) => keychain_set(shared, account, v.clone()),
            None => keychain_delete(shared, account),
        };
        if let Err(e) = done {
            return Reply::error(ErrorCode::Internal, e);
        }
    }
    let mut core = lock(&shared.core);
    let done = core.put_plugin_setting(id, key, value.as_deref());
    one(shared, done)
}

fn remove(shared: &Arc<Shared>, id: &str) -> Reply {
    let Some(p) = lock(&shared.core).plugins().ok().and_then(|l| l.into_iter().find(|p| p.id == id)) else { return Reply::error(ErrorCode::NotFound, "There is no such plugin.") };
    // Its secrets first: a plugin is never removed while its secrets stay behind.
    for f in p.manifest.settings.iter().filter(|f| f.kind == FieldKind::Secret) {
        if let Err(e) = keychain_delete(shared, secrets::account(id, &f.key)) {
            return Reply::error(ErrorCode::Internal, format!("{} was kept, because its secrets could not be removed from the Keychain. {e}", p.name));
        }
    }
    let mut core = lock(&shared.core);
    match core.remove_plugin(id) {
        Ok((_, revision)) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::PluginRemoved { plugin: id.to_owned() } });
            Reply::Ok
        }
        Err(e) => error(e),
    }
}

/// Starts the plugin's server and asks it who it is and what tools it has. Only a plugin the person approved, exactly as it is now.
fn test(shared: &Arc<Shared>, id: &str) -> Reply {
    // A plugin from a folder is looked at again first: what runs is what the file says now.
    refresh_one(shared, id);
    let p = match lock(&shared.core).plugins() {
        Ok(l) => l.into_iter().find(|p| p.id == id),
        Err(e) => return Reply::error(ErrorCode::Internal, e.to_string()),
    };
    let Some(p) = p else { return Reply::error(ErrorCode::NotFound, "There is no such plugin.") };
    if p.approval != Approval::Approved {
        return Reply::error(ErrorCode::Refused, "Neko runs only what you approved. Approve this plugin first.");
    }
    if let Some(why) = &p.problem {
        return Reply::error(ErrorCode::Refused, why.clone());
    }
    if p.manifest.mcp.is_remote() {
        return Reply::error(ErrorCode::Refused, "Neko cannot test a server reached by a web address yet. Claude Code connects to it when a thread runs.");
    }
    if !lock(&shared.plugins.checking).insert(id.to_owned()) {
        return Reply::error(ErrorCode::Busy, "This plugin is being tested already.");
    }
    let done = check(shared, &p);
    lock(&shared.plugins.checking).remove(id);
    let check = match done {
        Ok(f) => Check { at_ms: 0, ok: true, server: f.server, version: f.version, tools: f.tools, error: None },
        Err(why) => Check { at_ms: 0, ok: false, server: None, version: None, tools: Vec::new(), error: Some(why) },
    };
    let mut core = lock(&shared.core);
    let check = Check { at_ms: core.now(), ..check };
    let done = core.record_plugin_check(id, check);
    one(shared, done)
}

fn check(shared: &Arc<Shared>, p: &Plugin) -> Result<mcp::Found, String> {
    let secrets = read_secrets(shared, p)?;
    let stored = lock(&shared.core).store.plugin_values(&p.id).map_err(|e| e.to_string())?;
    let Target::Stdio(launch) = mcp::target(p, &stored, &secrets) else { return Err("Neko cannot test a server reached by a web address yet.".into()) };
    let guard = std::env::current_exe().map_err(|e| e.to_string())?;
    let cwd = p.folder.as_deref().map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir);
    // The PATH may take a few seconds to read the first time; the deadline counts from the start of the conversation.
    let path = mcp::path();
    mcp::check(&guard, &launch, &cwd, &path, check_limit())
}

/// How long a check may take: [`plugins::CHECK_SECONDS`]; tests shorten it with `NEKO_PLUGIN_CHECK_MS`.
fn check_limit() -> Duration {
    std::env::var("NEKO_PLUGIN_CHECK_MS").ok().and_then(|v| v.parse().ok()).map_or(Duration::from_secs(plugins::CHECK_SECONDS), Duration::from_millis)
}

// ---- Folders -----------------------------------------------------------------------------------------------------------------------

/// Looks at one folder plugin again, outside any lock, and takes what it finds.
fn refresh_one(shared: &Arc<Shared>, id: &str) {
    let folder = lock(&shared.core).store.plugin_row(id).ok().flatten().and_then(|r| r.folder);
    let Some(folder) = folder else { return };
    let read = neko_core::plugins::read_plugin_file(&folder);
    let mut core = lock(&shared.core);
    if let Ok(Some((p, rev))) = core.apply_folder_read(id, read) {
        updated(shared, &p, rev);
    }
}

/// Looks at every plugin folder again. Returns how many there are.
fn refresh_all(shared: &Arc<Shared>) -> usize {
    let folders: Vec<(String, String)> = lock(&shared.core).store.plugin_rows().unwrap_or_default().into_iter().filter_map(|r| r.folder.map(|f| (r.id, f))).collect();
    for (id, folder) in &folders {
        if shared.stopping.load(Ordering::SeqCst) {
            break;
        }
        let read = neko_core::plugins::read_plugin_file(folder);
        let mut core = lock(&shared.core);
        if let Ok(Some((p, rev))) = core.apply_folder_read(id, read) {
            updated(shared, &p, rev);
        }
    }
    folders.len()
}

static THREAD: OnceLock<std::thread::Thread> = OnceLock::new();

/// A screen subscribed, or a folder plugin was added: the watcher looks (it sleeps while there is nothing to watch).
pub fn poke() {
    if let Some(t) = THREAD.get() {
        t.unpark();
    }
}

/// Starts the thread that notices a plugin's `neko-plugin.json` changing, every few seconds while a screen is subscribed and some
/// plugin comes from a folder. With nothing to watch it sleeps and sets no timer.
pub fn spawn(shared: Arc<Shared>) -> std::io::Result<()> {
    let h = std::thread::Builder::new().name("neko-plugin-files".into()).spawn(move || {
        loop {
            if shared.stopping.load(Ordering::SeqCst) {
                return;
            }
            if !shared.screens() {
                std::thread::park();
                continue;
            }
            if refresh_all(&shared) == 0 {
                std::thread::park();
                continue;
            }
            std::thread::park_timeout(WATCH_EVERY);
        }
    })?;
    let _ = THREAD.set(h.thread().clone());
    Ok(())
}

/// The daemon is stopping: the watcher ends now.
pub fn stopping() {
    poke();
}

// ---- What a run is handed --------------------------------------------------------------------------------------------------------

/// A run's own MCP config, and the plugins in it. The folder holding it goes when this is dropped: when the run ends, however it ends.
pub struct RunFiles {
    dir: Option<PathBuf>,
    /// The file to hand Claude Code (`--mcp-config`); None when the run has no plugins.
    pub config: Option<PathBuf>,
    /// The plugins in it, by name.
    pub names: Vec<String>,
    /// What was left out and why, in words for the task.
    pub left_out: Vec<String>,
}

impl Drop for RunFiles {
    fn drop(&mut self) {
        if let Some(d) = &self.dir {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

/// The plugins a run of `cat` gets: approved exactly as they are now (a folder's file is read again first), on, set up and open to the
/// cat. Their servers, with their settings put in (secrets read from the Keychain now and written only here), go into one file made
/// for this run alone: its folder 0700, the file 0600.
pub fn for_run(shared: &Arc<Shared>, data_dir: &Path, cat: &str, run: &str) -> RunFiles {
    let mut files = RunFiles { dir: None, config: None, names: Vec::new(), left_out: Vec::new() };
    refresh_all(shared);
    let candidates = match lock(&shared.core).plugins_for(cat) {
        Ok(c) => c,
        Err(_) => return files,
    };
    let mut servers: Vec<(String, Target)> = Vec::new();
    for p in candidates {
        let secrets = match read_secrets(shared, &p) {
            Ok(s) => s,
            Err(why) => {
                files.left_out.push(format!("{} was left out of this run. {why}", p.name));
                continue;
            }
        };
        let Ok(stored) = lock(&shared.core).store.plugin_values(&p.id) else { continue };
        servers.push((p.name.clone(), mcp::target(&p, &stored, &secrets)));
    }
    if servers.is_empty() {
        return files;
    }
    let root = data_dir.join(RUN_FILES);
    let dir = root.join(run);
    let written = (|| -> std::io::Result<PathBuf> {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        let file = dir.join("mcp.json");
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&file)?;
        f.write_all(mcp::claude_config(&servers).to_string().as_bytes())?;
        Ok(file)
    })();
    files.dir = Some(dir);
    match written {
        Ok(file) => {
            files.config = Some(file);
            files.names = servers.into_iter().map(|(n, _)| n).collect();
        }
        Err(e) => files.left_out.push(format!("Plugins were left out of this run: Neko could not write their settings ({e}).")),
    }
    files
}

/// At start: a run's config the last daemon left behind (it was killed) goes.
pub fn sweep(data_dir: &Path) {
    let _ = std::fs::remove_dir_all(data_dir.join(RUN_FILES));
}
