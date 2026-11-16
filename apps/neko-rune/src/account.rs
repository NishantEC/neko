//! The person at the keyboard, for the sessions column's footer and Home's greeting: the Mac account's
//! full name and picture (polish spec §1, §2). Read from the system on a thread when the window opens;
//! a headless run has none and shows initials on an Iris circle, as a Mac without a picture does.

use std::ffi::CStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rune::Pixels;

/// The macOS account behind the window.
#[derive(Clone)]
pub struct Account {
    /// "Ada Lovelace"; the login name when the account has no full name; empty in a headless run.
    pub name: String,
    /// The account picture, once read (`picture_ready`); drawn as a circle.
    pub picture: Pixels,
    pub picture_ready: Arc<AtomicBool>,
}

impl Default for Account {
    fn default() -> Self {
        Account { name: String::new(), picture: Pixels::new(), picture_ready: Arc::new(AtomicBool::new(false)) }
    }
}

impl Account {
    /// The first word of the name ("Ada"), for the greeting.
    pub fn first_name(&self) -> &str {
        first_name(&self.name)
    }

    pub fn initials(&self) -> String {
        initials(&self.name)
    }

    pub fn has_picture(&self) -> bool {
        self.picture_ready.load(Ordering::Acquire)
    }

    /// Reads the name now and the picture on a thread of its own (`dscl`), calling `wake` when it lands.
    pub fn load(wake: impl Fn() + Send + 'static) -> Account {
        let account = Account { name: system_name(), ..Account::default() };
        let (picture, ready, login) = (account.picture.clone(), account.picture_ready.clone(), login_name());
        std::thread::spawn(move || {
            let Some(bytes) = std::process::Command::new("/usr/bin/dscl").args([".", "-read", &format!("/Users/{login}"), "JPEGPhoto"]).output().ok().filter(|o| o.status.success()).and_then(|o| parse_dscl_photo(&String::from_utf8_lossy(&o.stdout))) else {
                return;
            };
            let Ok(img) = image::load_from_memory(&bytes) else { return };
            let side = img.width().min(img.height());
            let square = img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side).resize_exact(96, 96, image::imageops::FilterType::Triangle);
            picture.set(96, 96, square.to_rgba8().into_raw());
            ready.store(true, Ordering::Release);
            wake();
        });
        account
    }
}

fn login_name() -> String {
    std::env::var("USER").unwrap_or_default()
}

/// The account's full name from the password database (`pw_gecos`), else the login name.
fn system_name() -> String {
    // SAFETY: `getuid` is always safe; `getpwuid` returns null or a pointer to a static record that is
    // read at once, on this thread, before anything else can overwrite it.
    let gecos = unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_gecos.is_null() { None } else { Some(CStr::from_ptr((*pw).pw_gecos).to_string_lossy().into_owned()) }
    };
    gecos.map(|g| g.split(',').next().unwrap_or("").trim().to_owned()).filter(|g| !g.is_empty()).unwrap_or_else(login_name)
}

/// "Ada Lovelace" is "Ada".
pub fn first_name(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or("")
}

/// "Ada Lovelace" is "AL"; one word is its first letter; nothing is "N" (Neko).
pub fn initials(name: &str) -> String {
    let mut words = name.split_whitespace().filter_map(|w| w.chars().next());
    let (a, b) = (words.next(), words.last());
    match (a, b) {
        (Some(a), Some(b)) => format!("{}{}", a.to_uppercase(), b.to_uppercase()),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "N".to_owned(),
    }
}

/// `dscl . -read /Users/me JPEGPhoto` prints `JPEGPhoto:` and then the photo as hex, in rows of spaced
/// bytes. The bytes, or None when the account has no picture.
pub fn parse_dscl_photo(output: &str) -> Option<Vec<u8>> {
    let hex: String = output.strip_prefix("JPEGPhoto:")?.chars().filter(char::is_ascii_hexdigit).collect();
    if hex.len() < 16 || hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

/// "Good morning" before 12, "Good afternoon" before 18, "Good evening" after (polish spec §2).
pub fn greeting(hour: u32) -> &'static str {
    match hour {
        0..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_make_first_names_and_initials() {
        assert_eq!(first_name("Ada Lovelace"), "Ada");
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("ada king byron"), "AB", "first and last word");
        assert_eq!(initials("cher"), "C");
        assert_eq!(initials(""), "N");
        assert_eq!(first_name(""), "");
    }

    #[test]
    fn the_greeting_changes_at_noon_and_at_six() {
        assert_eq!([0, 5, 11].map(greeting), ["Good morning"; 3]);
        assert_eq!([12, 15, 17].map(greeting), ["Good afternoon"; 3]);
        assert_eq!([18, 21, 23].map(greeting), ["Good evening"; 3]);
    }

    #[test]
    fn dscl_photo_output_is_hex_bytes() {
        let out = "JPEGPhoto:\n ffd8ffe0 00104a46 49460001 0100\n";
        assert_eq!(parse_dscl_photo(out), Some(vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00]));
        assert_eq!(parse_dscl_photo("No such key: JPEGPhoto"), None);
        assert_eq!(parse_dscl_photo("JPEGPhoto:\n"), None);
    }
}
