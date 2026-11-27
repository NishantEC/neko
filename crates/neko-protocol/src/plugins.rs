//! Plugins (docs/knowledge/plugins.md): what Neko can be extended with. The first kind is an MCP server. The daemon keeps each plugin's
//! manifest, the digest of it the person approved and the plugin's settings; a screen lists and changes them through requests and
//! hears every change as an event.
//!
//! This module also holds the manifest itself (its file format, its checks, its digest) and the reading of pasted MCP JSON, because
//! the screen shows what a paste will add before it is sent, and must read it exactly as the daemon will.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The file a plugin folder holds.
pub const MANIFEST_FILE: &str = "neko-plugin.json";
/// The manifest format this build reads and writes (`neko_plugin` in the file).
pub const MANIFEST_FORMAT: u32 = 1;
/// The longest manifest file, in bytes.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
/// The longest plugin name, in characters.
pub const MAX_NAME: usize = 40;
/// How many plugins there may be.
pub const MAX_PLUGINS: usize = 64;
/// How many declared settings a plugin may have.
pub const MAX_FIELDS: usize = 20;
/// The longest value of a text, choice or folder setting, in characters.
pub const MAX_VALUE: usize = 2_000;
/// The longest secret, in bytes.
pub const MAX_SECRET: usize = 16 * 1024;
/// How long a check of a plugin may take.
pub const CHECK_SECONDS: u64 = 10;
/// The longest list of tool names a check keeps.
pub const MAX_TOOLS: usize = 500;

/// What kind of thing a plugin is. Only an MCP server for now; the field exists so later kinds fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Mcp,
}

/// What a declared setting holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    /// Kept in the macOS Keychain, never in the store; screens are told only whether it is set.
    Secret,
    YesNo,
    Choice,
    Folder,
}

/// One setting a plugin declares: Settings draws it as a field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingField {
    /// `a-z`, digits and `_`, 1 to 40 characters. Referred to in the manifest as `${settings.<key>}`.
    pub key: String,
    pub kind: FieldKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// What an unset field counts as (not for a secret). A yes/no default is `yes` or `no`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// The values of a choice field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// The plugin is left out of runs until this has a value.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
}

/// How an MCP server is reached: a program Claude Code starts (`command`), or a web address (`url`). Exactly one.
///
/// Any value may hold `${settings.<key>}` (the plugin's own setting, put in when a run starts) and, for a plugin from a folder,
/// `${plugin.dir}` (that folder).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mcp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment of the program: name to value (usually a `${settings.…}`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// `http` (the default) or `sse`, for a `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

/// A plugin's manifest: what it is, what it runs and what it lets the person set. The file `neko-plugin.json` and the record the
/// daemon approves are this. Its field order is part of the digest and never changes; a new field is left out of the JSON while
/// empty, so digests already approved stay the same.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The manifest format ([`MANIFEST_FORMAT`]).
    pub neko_plugin: u32,
    /// 1 to [`MAX_NAME`] characters of `A-Z a-z 0-9 - _`, no `__`. It is the server's name in Claude Code, so its tools are
    /// `mcp__<name>__<tool>`.
    pub name: String,
    #[serde(default = "zero_version")]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kind: PluginKind,
    pub mcp: Mcp,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingField>,
}

fn zero_version() -> String {
    "0".to_owned()
}

/// Whether `s` is a name a plugin may have.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().count() <= MAX_NAME && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') && !s.contains("__") && s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Any text made into a valid plugin name: other characters become `-`, `__` becomes `_`, the ends are trimmed.
pub fn sanitise_name(s: &str) -> String {
    let mut out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out: String = out.trim_matches(['-', '_']).chars().take(MAX_NAME).collect();
    let out = out.trim_matches(['-', '_']).to_owned();
    if out.is_empty() { "server".to_owned() } else { out }
}

fn valid_key(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn valid_env_name(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|c| c.is_ascii_alphanumeric() || c == '_') && s.len() <= 100
}

fn valid_header_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 100 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The `${settings.<key>}` references in `s`.
pub fn setting_refs(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(at) = rest.find("${") {
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else { break };
        if let Some(key) = after[..end].strip_prefix("settings.") {
            out.push(key.to_owned());
        }
        rest = &after[end + 1..];
    }
    out
}

/// `s` with each `${settings.<key>}` replaced by `value(key)` and `${plugin.dir}` by `dir`. Anything else, `${HOME}` for one, is left
/// as it is for Claude Code to read.
pub fn expand(s: &str, dir: Option<&str>, value: &dyn Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(at) = rest.find("${") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[at..]);
            return out;
        };
        let inner = &after[..end];
        if let Some(key) = inner.strip_prefix("settings.") {
            out.push_str(&value(key));
        } else if inner == "plugin.dir" && dir.is_some() {
            out.push_str(dir.unwrap_or(""));
        } else {
            out.push_str(&rest[at..at + 2 + end + 1]);
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

impl Mcp {
    /// A web address rather than a program.
    pub fn is_remote(&self) -> bool {
        self.url.is_some()
    }

    fn texts(&self) -> impl Iterator<Item = &String> {
        self.command.iter().chain(self.args.iter()).chain(self.env.values()).chain(self.url.iter()).chain(self.headers.values())
    }
}

impl Manifest {
    /// Reads a manifest file's text (a plugin folder's `neko-plugin.json`).
    pub fn parse_file(text: &str) -> Result<Manifest, String> {
        if text.len() > MAX_MANIFEST_BYTES {
            return Err(format!("{MANIFEST_FILE} is longer than {} KB.", MAX_MANIFEST_BYTES / 1024));
        }
        let m: Manifest = serde_json::from_str(text).map_err(|e| format!("{MANIFEST_FILE} cannot be read: {}.", friendly(&e)))?;
        m.validate()?;
        Ok(m)
    }

    /// Whether the manifest is one Neko will keep, with the reason in words when it is not.
    pub fn validate(&self) -> Result<(), String> {
        if self.neko_plugin != MANIFEST_FORMAT {
            return Err(format!("This manifest is format {}; this Neko reads format {MANIFEST_FORMAT}.", self.neko_plugin));
        }
        if !valid_name(&self.name) {
            return Err(format!("“{}” is not a plugin name. Use 1 to {MAX_NAME} letters, digits, - or _.", self.name));
        }
        if self.version.is_empty() || self.version.chars().count() > 40 || self.version.contains('\n') {
            return Err("The version must be one short line.".into());
        }
        if self.description.as_ref().is_some_and(|d| d.chars().count() > 200 || d.contains('\n')) {
            return Err("The description must be one line of at most 200 characters.".into());
        }
        let m = &self.mcp;
        match (&m.command, &m.url) {
            (Some(_), Some(_)) => return Err("An MCP server has a command or a url, not both.".into()),
            (None, None) => return Err("An MCP server needs a command (a program to run) or a url.".into()),
            (Some(c), None) => {
                if c.trim().is_empty() {
                    return Err("The command is empty.".into());
                }
                if m.transport.is_some() || !m.headers.is_empty() {
                    return Err("A command takes no transport or headers; those are for a url.".into());
                }
            }
            (None, Some(u)) => {
                if !(u.starts_with("http://") || u.starts_with("https://")) {
                    return Err("A url must start with http:// or https://.".into());
                }
                if !m.args.is_empty() || !m.env.is_empty() {
                    return Err("A url takes no args or env; those are for a command.".into());
                }
                if let Some(t) = &m.transport
                    && t != "http"
                    && t != "sse"
                {
                    return Err(format!("The transport “{t}” is not one Neko knows. Use http or sse."));
                }
            }
        }
        if m.args.len() > 100 || m.texts().any(|t| t.len() > MAX_VALUE) {
            return Err("The command line is too long.".into());
        }
        if let Some(n) = m.env.keys().find(|n| !valid_env_name(n)) {
            return Err(format!("“{n}” is not an environment variable name."));
        }
        if let Some(n) = m.headers.keys().find(|n| !valid_header_name(n)) {
            return Err(format!("“{n}” is not a header name."));
        }
        if self.settings.len() > MAX_FIELDS {
            return Err(format!("A plugin may declare at most {MAX_FIELDS} settings."));
        }
        let mut keys = BTreeSet::new();
        for f in &self.settings {
            if !valid_key(&f.key) {
                return Err(format!("“{}” is not a setting key. Use lowercase letters, digits and _.", f.key));
            }
            if !keys.insert(f.key.as_str()) {
                return Err(format!("The setting “{}” is declared twice.", f.key));
            }
            if f.label.trim().is_empty() || f.label.chars().count() > 80 || f.label.contains('\n') {
                return Err(format!("The setting “{}” needs a one-line label of at most 80 characters.", f.key));
            }
            if f.help.as_ref().is_some_and(|h| h.chars().count() > 300) {
                return Err(format!("The help of “{}” is longer than 300 characters.", f.key));
            }
            match f.kind {
                FieldKind::Choice => {
                    if f.choices.is_empty() || f.choices.len() > 20 || f.choices.iter().any(|c| c.is_empty() || c.chars().count() > 80) {
                        return Err(format!("The choice “{}” needs 1 to 20 values of at most 80 characters.", f.key));
                    }
                }
                _ if !f.choices.is_empty() => return Err(format!("Only a choice has values; “{}” is not one.", f.key)),
                _ => {}
            }
            if let Some(d) = &f.default {
                let ok = match f.kind {
                    FieldKind::Secret => false,
                    FieldKind::YesNo => d == "yes" || d == "no",
                    FieldKind::Choice => f.choices.contains(d),
                    FieldKind::Text | FieldKind::Folder => d.chars().count() <= MAX_VALUE,
                };
                if !ok {
                    return Err(format!("“{}” has a default that does not fit its kind.", f.key));
                }
            }
        }
        for t in self.mcp.texts() {
            if let Some(k) = setting_refs(t).into_iter().find(|k| !keys.contains(k.as_str())) {
                return Err(format!("${{settings.{k}}} is used, but no setting “{k}” is declared."));
            }
        }
        if serde_json::to_string(self).map_or(0, |s| s.len()) > MAX_MANIFEST_BYTES {
            return Err("The manifest is too long.".into());
        }
        Ok(())
    }

    /// The digest the person approves: SHA-256 of the manifest's canonical JSON (its fields in their fixed order, maps by key,
    /// no spaces) and, for a plugin from a folder, that folder's path. The same manifest always gives the same digest, whatever
    /// order or spacing it was written in.
    pub fn digest(&self, folder: Option<&str>) -> String {
        let mut text = format!("neko-plugin-manifest-v{MANIFEST_FORMAT}\n{}", serde_json::to_string(self).unwrap_or_default());
        if let Some(f) = folder {
            text.push_str("\nfolder:");
            text.push_str(f);
        }
        format!("sha256:{}", sha256_hex(text.as_bytes()))
    }

    /// The program and its arguments as one line ("node server.js"), arguments with spaces quoted; for a url, the address.
    /// `${plugin.dir}` is shown as the folder; a `${settings.…}` stays as written.
    pub fn command_line(&self, dir: Option<&str>) -> String {
        let show = |s: &String| expand(s, dir, &|k| format!("${{settings.{k}}}"));
        if let Some(u) = &self.mcp.url {
            return show(u);
        }
        let quote = |a: String| if a.is_empty() || a.contains(char::is_whitespace) { format!("'{}'", a.replace('\'', "'\\''")) } else { a };
        self.mcp.command.iter().chain(self.mcp.args.iter()).map(|a| quote(show(a))).collect::<Vec<_>>().join(" ")
    }

    /// The names of what the plugin is handed: environment variables of a program, headers of a url. Sorted.
    pub fn passes(&self) -> Vec<String> {
        if self.mcp.is_remote() { self.mcp.headers.keys().cloned().collect() } else { self.mcp.env.keys().cloned().collect() }
    }

    /// What differs from `old`, one plain line each, for the card that asks again after a change.
    pub fn changes_from(&self, old: &Manifest, dir: Option<&str>) -> Vec<String> {
        let mut out = Vec::new();
        if self.name != old.name {
            out.push(format!("Name: {} to {}", old.name, self.name));
        }
        if self.version != old.version {
            out.push(format!("Version: {} to {}", old.version, self.version));
        }
        let (was, now) = (old.command_line(dir), self.command_line(dir));
        if was != now || old.mcp.is_remote() != self.mcp.is_remote() {
            out.push(format!("{}: {was} to {now}", if self.mcp.is_remote() { "Address" } else { "Runs" }));
        }
        if old.mcp.transport != self.mcp.transport {
            out.push(format!("Transport: {} to {}", old.mcp.transport.as_deref().unwrap_or("http"), self.mcp.transport.as_deref().unwrap_or("http")));
        }
        let (a, b): (BTreeSet<_>, BTreeSet<_>) = (old.passes().into_iter().collect(), self.passes().into_iter().collect());
        for n in b.difference(&a) {
            out.push(format!("Now also passes {n}"));
        }
        for n in a.difference(&b) {
            out.push(format!("No longer passes {n}"));
        }
        let values = |m: &Manifest| -> BTreeMap<String, String> { m.mcp.env.iter().chain(m.mcp.headers.iter()).map(|(k, v)| (k.clone(), v.clone())).collect() };
        let (va, vb) = (values(old), values(self));
        for (k, v) in &vb {
            if va.get(k).is_some_and(|o| o != v) {
                out.push(format!("{k} is now set from somewhere else"));
            }
        }
        let had: BTreeMap<_, _> = old.settings.iter().map(|f| (&f.key, f)).collect();
        for f in &self.settings {
            match had.get(&f.key) {
                None => out.push(format!("New setting: {}", f.label)),
                Some(o) if *o != f => out.push(format!("Setting changed: {}", f.label)),
                _ => {}
            }
        }
        for f in &old.settings {
            if !self.settings.iter().any(|n| n.key == f.key) {
                out.push(format!("Setting removed: {}", f.label));
            }
        }
        if out.is_empty() {
            out.push("Something else in the manifest changed.".into());
        }
        out
    }
}

/// What serde_json says, without the byte offsets it adds.
fn friendly(e: &serde_json::Error) -> String {
    let s = e.to_string();
    let s = s.split(" at line ").next().unwrap_or(&s).to_owned();
    if e.line() > 0 { format!("{s} (line {}, column {})", e.line(), e.column()) } else { s }
}

// ---- Pasting what people already have --------------------------------------------------------------------------------------------

/// One server read from pasted JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub manifest: Manifest,
    /// Values taken out of the paste (an environment variable's, a header's) to be kept as secrets of their own: (setting key,
    /// value). The manifest holds only a `${settings.<key>}` in their place.
    pub secrets: Vec<(String, String)>,
    /// What was left out of the paste, in words (a key Neko does not use).
    pub notes: Vec<String>,
}

fn secret_key(taken: &BTreeSet<String>, from: &str) -> String {
    let mut base: String = from.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    while base.contains("__") {
        base = base.replace("__", "_");
    }
    let base: String = base.trim_matches('_').chars().take(36).collect();
    let base = if base.is_empty() { "value".to_owned() } else { base };
    let mut key = base.clone();
    let mut n = 2;
    while taken.contains(&key) {
        key = format!("{base}_{n}");
        n += 1;
    }
    key
}

/// The name a server without one is given: from its package or program, else its address.
fn derived_name(server: &serde_json::Map<String, Value>) -> String {
    if let Some(u) = server.get("url").and_then(Value::as_str) {
        let host = u.split("://").nth(1).unwrap_or(u).split(['/', '?', '#']).next().unwrap_or("");
        let host = host.split(':').next().unwrap_or(host);
        return sanitise_name(host);
    }
    let command = server.get("command").and_then(Value::as_str).unwrap_or("server");
    let program = command.rsplit('/').next().unwrap_or(command);
    let args: Vec<&str> = server.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let runner = matches!(program, "npx" | "uvx" | "bunx" | "pnpx" | "pipx" | "deno" | "docker" | "node" | "python" | "python3" | "uv");
    if runner && let Some(a) = args.iter().find(|a| !a.starts_with('-') && !matches!(**a, "run" | "dlx" | "exec" | "dev")) {
        // A package ("@acme/docs-server@latest" gives "acme-docs-server") or a script's file name.
        let a = a.trim_start_matches('@');
        let a = a.split('@').next().unwrap_or(a);
        let a = a.trim_end_matches(".js").trim_end_matches(".py").trim_end_matches(".mjs");
        let a = if a.starts_with('/') || a.starts_with('.') { a.rsplit('/').next().unwrap_or(a) } else { a };
        return sanitise_name(a);
    }
    sanitise_name(program)
}

fn string_map(v: &Value, what: &str, server: &str) -> Result<BTreeMap<String, String>, String> {
    let Some(o) = v.as_object() else { return Err(format!("“{server}”: {what} must be an object of names and values.")) };
    let mut out = BTreeMap::new();
    for (k, v) in o {
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return Err(format!("“{server}”: the value of {k} in {what} must be text.")),
        };
        out.insert(k.clone(), s);
    }
    Ok(out)
}

fn server_from(name: &str, v: &Value) -> Result<Parsed, String> {
    let Some(o) = v.as_object() else { return Err(format!("“{name}” is not an object. A server looks like {{\"command\": \"node\", \"args\": [\"server.js\"]}}.")) };
    let mut notes = Vec::new();
    let mut mcp = Mcp::default();
    let mut settings: Vec<SettingField> = Vec::new();
    let mut secrets = Vec::new();
    let mut taken = BTreeSet::new();
    let mut take_secret = |from: &str, label: String, help: String, value: &str, settings: &mut Vec<SettingField>| -> String {
        // A value that already refers to Claude Code's own environment is passed through as written.
        if value.starts_with("${") && !value.starts_with("${settings.") {
            return value.to_owned();
        }
        let key = secret_key(&taken, from);
        taken.insert(key.clone());
        settings.push(SettingField { key: key.clone(), kind: FieldKind::Secret, label, help: Some(help), default: None, choices: Vec::new(), required: false });
        if !value.is_empty() {
            secrets.push((key.clone(), value.to_owned()));
        }
        format!("${{settings.{key}}}")
    };
    for (k, val) in o {
        match k.as_str() {
            "command" => mcp.command = Some(val.as_str().ok_or_else(|| format!("“{name}”: the command must be text."))?.to_owned()),
            "args" => {
                let a = val.as_array().ok_or_else(|| format!("“{name}”: args must be a list."))?;
                mcp.args = a.iter().map(|x| x.as_str().map(str::to_owned).ok_or_else(|| format!("“{name}”: every arg must be text."))).collect::<Result<_, _>>()?;
            }
            "url" => mcp.url = Some(val.as_str().ok_or_else(|| format!("“{name}”: the url must be text."))?.to_owned()),
            "type" | "transport" => match val.as_str() {
                Some("stdio") => {}
                Some("http" | "streamable-http" | "streamableHttp") => mcp.transport = None,
                Some("sse") => mcp.transport = Some("sse".into()),
                _ => return Err(format!("“{name}”: the type must be stdio, http or sse.")),
            },
            "env" | "headers" => {}
            "name" => {}
            other => notes.push(format!("“{other}” of {name} is not used by Neko and was left out.")),
        }
    }
    if let Some(e) = o.get("env") {
        for (n, val) in string_map(e, "env", name)? {
            let tmpl = take_secret(&n, n.clone(), format!("Passed to the server as {n}."), &val, &mut settings);
            mcp.env.insert(n, tmpl);
        }
    }
    if let Some(h) = o.get("headers") {
        for (n, val) in string_map(h, "headers", name)? {
            let tmpl = take_secret(&format!("header {n}"), n.clone(), format!("Sent to the server as the {n} header."), &val, &mut settings);
            mcp.headers.insert(n, tmpl);
        }
    }
    // A server given with a url and a "type" of stdio (or none) is a mistake the checks below name.
    let manifest = Manifest { neko_plugin: MANIFEST_FORMAT, name: name.to_owned(), version: zero_version(), description: None, kind: PluginKind::Mcp, mcp, settings };
    manifest.validate().map_err(|e| format!("“{name}”: {e}"))?;
    Ok(Parsed { manifest, secrets, notes })
}

/// Reads what people already have for Claude Desktop, Claude Code, Cursor and the like: one server's JSON, or a `{"mcpServers": {…}}`
/// block (or just the map of servers). `name` names a single server that has none. Each server becomes one [`Parsed`]; every value
/// of its `env` and `headers` becomes a secret setting, so nothing secret stays in the manifest.
pub fn parse_mcp_json(text: &str, name: Option<&str>) -> Result<Vec<Parsed>, String> {
    if text.trim().is_empty() {
        return Err("Paste a server's JSON here.".into());
    }
    if text.len() > MAX_MANIFEST_BYTES {
        return Err("That is too long to be one server's settings.".into());
    }
    let v: Value = serde_json::from_str(text).map_err(|e| format!("That is not valid JSON: {}.", friendly(&e)))?;
    let Some(top) = v.as_object() else { return Err("Expected a JSON object, like {\"mcpServers\": {…}} or {\"command\": …}.".into()) };
    let looks_like_server = |o: &serde_json::Map<String, Value>| o.contains_key("command") || o.contains_key("url");
    let servers: Vec<(String, &Value)> = if let Some(m) = top.get("mcpServers").or_else(|| top.get("servers")).and_then(Value::as_object) {
        m.iter().map(|(k, v)| (k.clone(), v)).collect()
    } else if looks_like_server(top) {
        let given = name.map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned).or_else(|| top.get("name").and_then(Value::as_str).map(str::to_owned));
        vec![(given.map_or_else(|| derived_name(top), |n| sanitise_name(&n)), &v)]
    } else if !top.is_empty() && top.values().all(|x| x.as_object().is_some_and(looks_like_server)) {
        top.iter().map(|(k, v)| (k.clone(), v)).collect()
    } else {
        return Err("Neko did not find an MCP server in this. A server has a \"command\" (a program to run) or a \"url\".".into());
    };
    if servers.is_empty() {
        return Err("There are no servers in this.".into());
    }
    if servers.len() > MAX_PLUGINS {
        return Err(format!("That is more than {MAX_PLUGINS} servers."));
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (key, val) in servers {
        let name = sanitise_name(&key);
        if !seen.insert(name.to_lowercase()) {
            return Err(format!("Two servers would both be called “{name}”."));
        }
        out.push(server_from(&name, val)?);
    }
    Ok(out)
}

// ---- What the daemon keeps and a screen shows ---------------------------------------------------------------------------------------

/// Whether the person approved this manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    /// Added, and not yet approved: it does not run.
    Waiting,
    /// The person approved exactly this manifest (its digest).
    Approved,
    /// It was approved, and the manifest is not what was approved any more: it does not run until approved again.
    Changed,
    /// The person said no to this manifest. It does not run and does not ask again until the manifest changes.
    Declined,
}

/// Which cats may use a plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    /// Every cat (the default).
    #[default]
    All,
    /// Only the cats listed.
    Only,
    /// Every cat except those listed.
    Except,
}

/// Which cats may use a plugin: every cat, unless the person narrowed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Access {
    pub mode: AccessMode,
    /// Cat ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cats: Vec<String>,
}

impl Access {
    pub fn allows(&self, cat: &str) -> bool {
        match self.mode {
            AccessMode::All => true,
            AccessMode::Only => self.cats.iter().any(|c| c == cat),
            AccessMode::Except => !self.cats.iter().any(|c| c == cat),
        }
    }
}

/// What the last check of a plugin found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub at_ms: i64,
    pub ok: bool,
    /// The server's own name and version, as it said them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Why it failed, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One setting's value as a screen sees it: a secret is never in it, only whether it is set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldValue {
    pub key: String,
    /// The value of a field that is not a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// There is a value (a secret's only trace).
    pub set: bool,
}

/// Where a plugin is in words, from what a screen knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginState {
    Waiting,
    Declined,
    Changed,
    /// Its folder or manifest file cannot be read.
    Problem,
    /// A setting it needs has no value.
    NeedsSetup,
    On,
    Off,
}

/// A plugin as a screen sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plugin {
    /// `plugin_` and 26 letters and digits.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kind: PluginKind,
    /// The folder it came from; None for one added by pasting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The manifest as it is now (for a folder, as the file last read).
    pub manifest: Manifest,
    /// The digest of it ([`Manifest::digest`]): what an approval is for.
    pub digest: String,
    pub approval: Approval,
    /// The digest the person approved, and when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_ms: Option<i64>,
    /// While `changed`: what differs from what was approved, one line each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
    pub enabled: bool,
    pub access: Access,
    /// One per declared setting.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<FieldValue>,
    /// The labels of the required settings that have no value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<Check>,
    /// Why Neko cannot read it (a folder that is gone, a manifest that no longer parses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub created_ms: i64,
    pub changed_ms: i64,
}

impl Plugin {
    pub fn state(&self) -> PluginState {
        match self.approval {
            Approval::Waiting => PluginState::Waiting,
            Approval::Declined => PluginState::Declined,
            Approval::Changed => PluginState::Changed,
            Approval::Approved if self.problem.is_some() => PluginState::Problem,
            Approval::Approved if !self.enabled => PluginState::Off,
            Approval::Approved if !self.missing.is_empty() => PluginState::NeedsSetup,
            Approval::Approved => PluginState::On,
        }
    }

    /// Whether the plugin goes to the runs of `cat`: approved as it is, on, set up and open to that cat.
    pub fn runs_for(&self, cat: &str) -> bool {
        self.state() == PluginState::On && self.access.allows(cat)
    }

    pub fn value(&self, key: &str) -> Option<&FieldValue> {
        self.values.iter().find(|v| v.key == key)
    }
}

/// What "Needs you" shows for a plugin waiting for approval: exactly what will run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginAsk {
    pub plugin: String,
    pub name: String,
    pub version: String,
    /// The digest this card is for; approving names it, so a manifest that changed meanwhile is never approved by mistake.
    pub digest: String,
    /// The program and arguments, or the address ([`Manifest::command_line`]).
    pub runs: String,
    pub remote: bool,
    /// The environment variables (or headers) it is handed, by name only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passes: Vec<String>,
    /// Asked again because the manifest changed since the person approved it; `changes` says what differs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Where it came from: its folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// `mcp__<server>__<tool>` as `(server, tool)`: how Claude Code names an MCP server's tool. None for any other name.
pub fn mcp_tool(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    (!server.is_empty() && !tool.is_empty()).then_some((server, tool))
}

// ---- SHA-256 ------------------------------------------------------------------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `data`, as 64 hex digits (written here so Neko takes no dependency for the one hash its trust rests on).
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, c) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        assert_eq!(sha256_hex(&vec![b'a'; 1000]), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }

    #[test]
    fn names_are_made_and_checked() {
        assert!(valid_name("acme-docs") && valid_name("a_b-9"));
        assert!(!valid_name("") && !valid_name("a b") && !valid_name("a__b") && !valid_name("-a") && !valid_name(&"x".repeat(41)));
        assert_eq!(sanitise_name("Acme Docs!"), "Acme-Docs");
        assert_eq!(sanitise_name("a__b"), "a_b");
        assert_eq!(sanitise_name("***"), "server");
    }

    #[test]
    fn tool_names_split_into_server_and_tool() {
        assert_eq!(mcp_tool("mcp__acme-docs__search"), Some(("acme-docs", "search")));
        assert_eq!(mcp_tool("mcp__a__b__c"), Some(("a", "b__c")));
        assert_eq!(mcp_tool("Bash"), None);
        assert_eq!(mcp_tool("mcp__a"), None);
    }

    #[test]
    fn templates_are_expanded_and_others_left_alone() {
        let v = |k: &str| format!("<{k}>");
        assert_eq!(expand("Bearer ${settings.token}", None, &v), "Bearer <token>");
        assert_eq!(expand("${plugin.dir}/s.js", Some("/p"), &v), "/p/s.js");
        assert_eq!(expand("${plugin.dir}/s.js", None, &v), "${plugin.dir}/s.js");
        assert_eq!(expand("${HOME}/x ${oops", None, &v), "${HOME}/x ${oops");
        assert_eq!(setting_refs("a ${settings.x} ${HOME} ${settings.y}"), vec!["x", "y"]);
    }
}
