//! Where a plugin's secrets are kept (docs/knowledge/plugins.md): the macOS Keychain, never the store, an event, a log or the
//! transcript. A secret setting of a plugin is one generic password: service `dev.neko.plugins`, account `<plugin id>/<setting key>`.
//!
//! The Keychain is reached through the `security-framework` crate (MIT or Apache-2.0), which calls the system's own API directly. The
//! other way, the `security` command, would put the secret in its arguments, where any process of the same user can read it while it
//! runs. A Keychain call can wait on the person (a prompt, a locked keychain), so the daemon never makes one on a connection's
//! thread without a deadline (`neko-daemon`'s `secrets` helper).
//!
//! Everything goes through [`Secrets`], so tests use [`MemorySecrets`] and the QA rig a [`FileSecrets`] (debug builds only) and none
//! ever touches the login keychain.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// The Keychain service every secret of Neko's plugins is kept under.
pub const SERVICE: &str = "dev.neko.plugins";

/// The account of one secret setting of one plugin.
pub fn account(plugin: &str, key: &str) -> String {
    format!("{plugin}/{key}")
}

/// A place secrets are kept. An error is words for the person.
pub trait Secrets: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    /// Deleting what is not there is fine.
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// The macOS login keychain.
#[cfg(target_os = "macos")]
pub struct Keychain;

#[cfg(target_os = "macos")]
impl Keychain {
    /// errSecItemNotFound.
    const NOT_FOUND: i32 = -25300;
}

#[cfg(target_os = "macos")]
impl Secrets for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match security_framework::passwords::get_generic_password(SERVICE, account) {
            Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
            Err(e) if e.code() == Self::NOT_FOUND => Ok(None),
            Err(e) => Err(format!("The Keychain would not give the secret ({e}).")),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        security_framework::passwords::set_generic_password(SERVICE, account, value.as_bytes()).map_err(|e| format!("The Keychain would not keep the secret ({e})."))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(SERVICE, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == Self::NOT_FOUND => Ok(()),
            Err(e) => Err(format!("The Keychain would not remove the secret ({e}).")),
        }
    }
}

/// Secrets in memory: for tests.
#[derive(Default)]
pub struct MemorySecrets(Mutex<BTreeMap<String, String>>);

impl Secrets for MemorySecrets {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(account.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(account);
        Ok(())
    }
}

/// Secrets in one JSON file (mode 0600). Only for a debug daemon the QA rig and tests start with `NEKO_SECRETS_FILE`; a release
/// build never reads that variable ([`from_environment`]), so no installed Neko keeps a secret anywhere but the Keychain.
pub struct FileSecrets {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileSecrets {
    pub fn new(path: PathBuf) -> FileSecrets {
        FileSecrets { path, lock: Mutex::new(()) }
    }

    fn read(&self) -> BTreeMap<String, String> {
        std::fs::read(&self.path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn write(&self, m: &BTreeMap<String, String>) -> Result<(), String> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&self.path).map_err(|e| e.to_string())?;
        f.write_all(serde_json::to_string(m).unwrap_or_default().as_bytes()).map_err(|e| e.to_string())
    }
}

impl Secrets for FileSecrets {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        let _g = self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(self.read().get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        let _g = self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut m = self.read();
        m.insert(account.to_owned(), value.to_owned());
        self.write(&m)
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let _g = self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut m = self.read();
        m.remove(account);
        self.write(&m)
    }
}

/// The secrets the daemon uses: the Keychain, except in a debug build whose environment names `NEKO_SECRETS_FILE`.
pub fn from_environment() -> Box<dyn Secrets> {
    #[cfg(debug_assertions)]
    if let Some(p) = std::env::var_os("NEKO_SECRETS_FILE").filter(|p| !p.is_empty()) {
        return Box::new(FileSecrets::new(PathBuf::from(p)));
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(Keychain)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(MemorySecrets::default())
    }
}
