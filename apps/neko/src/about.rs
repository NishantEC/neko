//! Neko ▸ About Neko: AppKit's own About panel (`orderFrontStandardAboutPanelWithOptions:`), as every Mac app shows, with the name, the
//! version from the bundle, the commit it was built from (`NekoGitCommit`, written by `scripts/build-app.sh`), the copyright line
//! and the licence. Rune's `MenuItem::about` replaces the standard item; Rune has no panel of its own, so the panel is raw AppKit.
//! What it says is [`lines`], a pure function.

/// The copyright line.
pub const COPYRIGHT: &str = "Copyright © 2026 The Neko authors";
/// The licence, under the copyright.
pub const LICENCE: &str = "Released under the MIT License.";

/// What the panel shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct About {
    pub name: String,
    /// "0.1.0": the bundle's `CFBundleShortVersionString`.
    pub version: String,
    /// In brackets after the version: "commit 1a2b3c4d", or "development build" outside a bundle.
    pub build: String,
    pub copyright: String,
    pub credits: String,
}

/// `short_version` and `commit` as the bundle's Info.plist has them (`None` outside a bundle).
pub fn lines(short_version: Option<&str>, commit: Option<&str>) -> About {
    let version = short_version.map(str::trim).filter(|v| !v.is_empty()).unwrap_or(env!("CARGO_PKG_VERSION")).to_owned();
    let commit = commit.map(str::trim).filter(|c| !c.is_empty() && c.chars().all(|ch| ch.is_ascii_hexdigit()));
    let build = match commit {
        Some(c) => format!("commit {}", &c[..c.len().min(8)]),
        None => "development build".to_owned(),
    };
    About { name: "Neko".to_owned(), version, build, copyright: COPYRIGHT.to_owned(), credits: LICENCE.to_owned() }
}

/// Shows the panel (macOS; nothing elsewhere).
pub fn show() {
    #[cfg(target_os = "macos")]
    mac::show();
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadMarker, class, msg_send};
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    fn info(key: &str) -> Option<String> {
        // SAFETY: NSBundle reads on the main thread; an Info.plist string key holds a string (anything else is not read).
        unsafe {
            let bundle: Option<Retained<AnyObject>> = msg_send![class!(NSBundle), mainBundle];
            let value: Option<Retained<AnyObject>> = msg_send![&*bundle?, objectForInfoDictionaryKey: &*NSString::from_str(key)];
            let value = value?;
            let is_string: bool = msg_send![&*value, isKindOfClass: class!(NSString)];
            is_string.then(|| Retained::cast_unchecked::<NSString>(value).to_string())
        }
    }

    pub fn show() {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let about = super::lines(info("CFBundleShortVersionString").as_deref(), info("NekoGitCommit").as_deref());
        // SAFETY: main thread; every object passed is retained for the call; the option keys are AppKit's documented string values.
        unsafe {
            let credits: Retained<AnyObject> = msg_send![msg_send![class!(NSAttributedString), alloc], initWithString: &*NSString::from_str(&about.credits)];
            let pairs: [(&str, Retained<AnyObject>); 5] = [
                ("ApplicationName", Retained::cast_unchecked(NSString::from_str(&about.name))),
                ("ApplicationVersion", Retained::cast_unchecked(NSString::from_str(&about.version))),
                ("Version", Retained::cast_unchecked(NSString::from_str(&about.build))),
                ("Copyright", Retained::cast_unchecked(NSString::from_str(&about.copyright))),
                ("Credits", credits),
            ];
            let dict: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), dictionary];
            for (key, value) in &pairs {
                let _: () = msg_send![&*dict, setObject: &**value, forKey: &*NSString::from_str(key)];
            }
            let app = NSApplication::sharedApplication(mtm);
            let _: () = msg_send![&*app, orderFrontStandardAboutPanelWithOptions: &*dict];
            app.activate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_names_the_version_the_commit_the_copyright_and_the_licence() {
        let a = lines(Some("0.1.0"), Some("1a2b3c4d5e6f7a8b9c0d"));
        assert_eq!(a.name, "Neko");
        assert_eq!(a.version, "0.1.0");
        assert_eq!(a.build, "commit 1a2b3c4d");
        assert_eq!(a.copyright, "Copyright © 2026 The Neko authors");
        assert!(a.credits.contains("MIT"));
    }

    #[test]
    fn outside_a_bundle_it_says_development_build() {
        let a = lines(None, None);
        assert_eq!(a.build, "development build");
        assert_eq!(a.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(lines(Some(" "), Some("not a commit")).build, "development build", "only a hex commit is shown");
    }
}
