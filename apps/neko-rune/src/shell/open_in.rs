//! "Open Neko in": a port of `OpenNeko` in `WebHome.swift` (the choice, its two titles) and of the launch rules in `NekoAppDelegate`.
//!
//! The Swift app's default is the browser; Rune's is **its own window** (decided for P5a). The choice is kept in this client's saved
//! preferences (`app::load_open_in`), as Swift keeps it in `UserDefaults` (`neko.openIn`). Browser opens the daemon's signed web address
//! (`SignedWebUrl`), which is how `WebHomeLocation` signs it in Swift. Started at login, Rune stays quiet: no window, no browser.

/// Where "Open Neko" goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum OpenIn {
    Browser,
    #[default]
    Window,
}

impl OpenIn {
    pub const ALL: [OpenIn; 2] = [OpenIn::Browser, OpenIn::Window];

    /// `OpenNeko.title`.
    pub fn title(self) -> &'static str {
        match self {
            OpenIn::Browser => "Browser",
            OpenIn::Window => "Neko window",
        }
    }

    /// The saved value (`OpenNeko.rawValue`).
    pub fn key(self) -> &'static str {
        match self {
            OpenIn::Browser => "browser",
            OpenIn::Window => "window",
        }
    }

    /// A saved value read back; anything else is the default.
    pub fn from_key(key: &str) -> OpenIn {
        OpenIn::ALL.into_iter().find(|o| o.key() == key).unwrap_or_default()
    }
}

/// The LaunchAgent's label (`Uninstaller.launchAgentLabel`, `preferences.rs`): launchd names it in `XPC_SERVICE_NAME` for what it starts.
pub const LAUNCH_AGENT_LABEL: &str = "com.neko.launcher";

/// Whether this start is the login item's: launchd started it from the LaunchAgent (`XPC_SERVICE_NAME` is its label), or `NEKO_RUNE_AT_LOGIN=1`
/// says so. Swift reads the same fact from the launch Apple event (`keyAELaunchedAsLogInItem`); a Rune process has no delegate to see it
/// in, and the bundle's own login item is P5b's (docs/knowledge/rune-as-the-app.md).
pub fn launched_at_login(env: impl Fn(&str) -> Option<String>) -> bool {
    env("NEKO_RUNE_AT_LOGIN").as_deref() == Some("1") || env("XPC_SERVICE_NAME").as_deref() == Some(LAUNCH_AGENT_LABEL)
}

/// What a start does about the window and the browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Startup {
    /// Open Rune's window now.
    pub window: bool,
    /// Open the browser now (the signed web address; the window if it is not known).
    pub browser: bool,
}

/// `NekoAppDelegate.applicationDidFinishLaunching`: at login, nothing opens. Otherwise the choice: the browser (and no window), or the window.
/// Only `--app` mode changes how Rune starts; without it the window opens as ever.
pub fn startup(app_mode_holds_duties: bool, open_in: OpenIn, at_login: bool) -> Startup {
    if !app_mode_holds_duties {
        return Startup { window: true, browser: false };
    }
    match (at_login, open_in) {
        (true, _) => Startup { window: false, browser: false },
        (false, OpenIn::Browser) => Startup { window: false, browser: true },
        (false, OpenIn::Window) => Startup { window: true, browser: false },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_rune_s_own_window_and_the_titles_are_swifts() {
        assert_eq!(OpenIn::default(), OpenIn::Window);
        assert_eq!(OpenIn::Browser.title(), "Browser");
        assert_eq!(OpenIn::Window.title(), "Neko window");
        assert_eq!(OpenIn::from_key("browser"), OpenIn::Browser);
        assert_eq!(OpenIn::from_key("window"), OpenIn::Window);
        assert_eq!(OpenIn::from_key("anything else"), OpenIn::Window);
        for o in OpenIn::ALL {
            assert_eq!(OpenIn::from_key(o.key()), o);
        }
    }

    #[test]
    fn a_login_start_is_recognised_by_the_launch_agent_or_the_override() {
        let with = |k: &'static str, v: &'static str| move |name: &str| (name == k).then(|| v.to_owned());
        assert!(launched_at_login(with("XPC_SERVICE_NAME", "com.neko.launcher")));
        assert!(launched_at_login(with("NEKO_RUNE_AT_LOGIN", "1")));
        assert!(!launched_at_login(with("XPC_SERVICE_NAME", "0")), "a Terminal run is not a login");
        assert!(!launched_at_login(with("XPC_SERVICE_NAME", "application.com.apple.Terminal.1234")));
        assert!(!launched_at_login(|_| None));
    }

    #[test]
    fn startup_is_quiet_at_login_and_follows_the_choice_otherwise() {
        assert_eq!(startup(true, OpenIn::Window, true), Startup { window: false, browser: false });
        assert_eq!(startup(true, OpenIn::Browser, true), Startup { window: false, browser: false });
        assert_eq!(startup(true, OpenIn::Browser, false), Startup { window: false, browser: true });
        assert_eq!(startup(true, OpenIn::Window, false), Startup { window: true, browser: false });
        // Not the app (no `--app`, or the Swift app holds the duties): a plain client's window.
        assert_eq!(startup(false, OpenIn::Browser, true), Startup { window: true, browser: false });
    }
}
