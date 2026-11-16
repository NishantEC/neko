//! What this app can do on the Mac, as Settings' Permissions section reports it. Three of the five are this process's own
//! to ask macOS (Accessibility, Full Disk Access, Screen Recording); Calendar and Reminders are what the daemon already
//! knows (`CalendarEvents`' status, `PhoneSettings`' status), so those need no new request. Each is shown with a button that
//! opens its pane of System Settings.

/// Whether one kind of access is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Access {
    Granted,
    Denied,
    /// Not known: nothing has asked, or this Mac cannot say without asking the person.
    #[default]
    Unknown,
}

impl Access {
    pub fn words(self) -> &'static str {
        match self {
            Access::Granted => "Allowed",
            Access::Denied => "Not allowed",
            Access::Unknown => "Not checked",
        }
    }
}

/// The three this app asks macOS itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Local {
    pub accessibility: Access,
    pub full_disk: Access,
    pub screen_recording: Access,
}

/// The panes of System Settings, by the name macOS gives their privacy anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Accessibility,
    FullDisk,
    Calendars,
    Reminders,
    ScreenRecording,
}

impl Pane {
    /// The address `open` takes to show that pane.
    pub fn url(self) -> &'static str {
        match self {
            Pane::Accessibility => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            Pane::FullDisk => "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
            Pane::Calendars => "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars",
            Pane::Reminders => "x-apple.systempreferences:com.apple.preference.security?Privacy_Reminders",
            Pane::ScreenRecording => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
        }
    }
}

/// Nothing known (headless runs and tests never ask the Mac).
pub fn none() -> Local {
    Local::default()
}

#[cfg(target_os = "macos")]
mod mac {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        pub fn CGPreflightScreenCaptureAccess() -> bool;
    }
}

/// Asks macOS what this process may do. None of these prompts the person: `AXIsProcessTrusted` and
/// `CGPreflightScreenCaptureAccess` only report, and Full Disk Access is tried by listing a folder macOS protects
/// (Safari's), which is refused without it.
pub fn check() -> Local {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: both are plain queries with no arguments and no state of ours.
        let (ax, screen) = unsafe { (mac::AXIsProcessTrusted(), mac::CGPreflightScreenCaptureAccess()) };
        let flag = |on: bool| if on { Access::Granted } else { Access::Denied };
        let full_disk = match std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join("Library/Safari")) {
            Some(path) => match std::fs::read_dir(path) {
                Ok(_) => Access::Granted,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Access::Denied,
                Err(_) => Access::Unknown,
            },
            None => Access::Unknown,
        };
        Local { accessibility: flag(ax), full_disk, screen_recording: flag(screen) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Local::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pane_opens_the_privacy_anchor_of_its_name() {
        for pane in [Pane::Accessibility, Pane::FullDisk, Pane::Calendars, Pane::Reminders, Pane::ScreenRecording] {
            assert!(pane.url().starts_with("x-apple.systempreferences:com.apple.preference.security?Privacy_"), "{pane:?}");
        }
        assert_eq!(none(), Local::default());
        assert_eq!(Access::Granted.words(), "Allowed");
    }
}
