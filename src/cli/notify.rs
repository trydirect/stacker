//! Terminal and desktop notifications for deploy results.
//!
//! Sends OS-level notifications (macOS/Linux) and a terminal bell as
//! fallback.  Used by the `--notify` flag on `stacker deploy`.

use std::process::Command;

/// Fire a notification after a deploy finishes.
///
/// - macOS: uses `osascript` to display a native notification with sound.
/// - Linux: uses `notify-send` (GNOME/libnotify) if available.
/// - All platforms: writes BEL (`\x07`) to stderr so the terminal can
///   beep or flash.
pub fn deploy_notify(success: bool, project_name: &str) {
    let (title, body, sound) = if success {
        (
            "Stacker",
            format!("{} deployed successfully", project_name),
            "Glass",
        )
    } else {
        (
            "Stacker",
            format!("{} deploy failed", project_name),
            "Submarine",
        )
    };

    os_notify(title, &body, sound);

    // Terminal bell — works in most terminals as an audio/visual cue.
    eprint!("\x07");
}

// ── Platform-specific helpers ──────────────────────────

#[cfg(target_os = "macos")]
fn os_notify(title: &str, body: &str, sound: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\" sound name \"{}\"",
        body, title, sound,
    );
    let _ = Command::new("osascript").args(["-e", &script]).status();
}

#[cfg(target_os = "linux")]
fn os_notify(title: &str, body: &str, _sound: &str) {
    // notify-send is the standard desktop-notification tool on Linux.
    // Silently fails if not installed (e.g. headless server).
    let _ = Command::new("notify-send").args([title, body]).status();
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn os_notify(_title: &str, _body: &str, _sound: &str) {
    // No desktop notification support on this platform; bell-only.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke-test: calling `deploy_notify` must not panic.
    #[test]
    fn deploy_notify_does_not_panic() {
        deploy_notify(true, "test-app");
        deploy_notify(false, "test-app");
    }
}
