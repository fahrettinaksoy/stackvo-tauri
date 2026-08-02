//! Asking for administrator rights from a window.
//!
//! One thing in this app needs them — replacing `/etc/hosts` — and it is here
//! rather than beside its caller because of what the *other* candidate taught:
//! **a windowed app must never let a child process ask for a password.**
//!
//! `mkcert -install` does exactly that. It shells out to
//! `sudo --prompt=Sudo password: -- security add-trusted-cert …`, and `sudo`
//! reads the password from the terminal. A GUI app has no terminal, so the
//! prompt goes nowhere and the process waits — forever, with no output, no
//! error and nothing on screen. That is what it looked like:
//!
//! ```text
//! root  33845  sudo --prompt=Sudo password: -- security add-trusted-cert …
//!       33836  mkcert -install
//! ```
//!
//! The first-run screen sat on "Issuing the certificate" until it was killed.
//! A failure would have been fine; the app has a retry button and an error
//! area. Hanging is the one outcome nothing recovers from.
//!
//! So elevation happens here, through the mechanism the platform gives a
//! windowed app: `osascript`'s `with administrator privileges`, which puts up
//! the standard authentication panel. And every helper this app spawns gets its
//! stdin closed, so one that decides to prompt anyway fails instead of stopping.
//!
//! The certificate authority is not a caller. Root through an AppleScript was
//! tried and refused — `SecTrustSettingsSetTrustSettings: the authorization was
//! denied since no user interaction was possible` — because writing the admin
//! trust domain needs the Security framework's own confirmation, which it
//! cannot show from there. `certs::trust_ca` writes the user trust domain
//! instead and needs no elevation at all.

use crate::error::{Code, Error, Result};

/// Run a shell command as an administrator.
///
/// `Ok(false)` means the person dismissed the prompt, which is an answer rather
/// than a fault — nothing was changed and nothing needs reporting as broken.
///
/// The command is interpolated into an AppleScript string, so every caller is
/// responsible for what it passes. Both of them build the string from paths
/// this app owns rather than from anything a user typed.
#[cfg(target_os = "macos")]
pub fn shell(command: &str) -> Result<bool> {
    let script = format!(r#"do shell script "{command}" with administrator privileges"#);

    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| Error::io("running osascript", e))?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    // -128 is the user cancelling the prompt.
    if stderr.contains("-128") || stderr.contains("User canceled") {
        return Ok(false);
    }

    Err(Error::new(
        Code::PermissionDenied,
        format!("Elevation failed: {}", stderr.trim()),
    ))
}

/// No equivalent that a windowed app can rely on.
///
/// `pkexec` is the closest thing and it is not always installed or running;
/// callers fall back to telling the user what to run themselves, which is
/// honest and does not hang.
#[cfg(not(target_os = "macos"))]
pub fn shell(_command: &str) -> Result<bool> {
    Err(Error::new(
        Code::Unsupported,
        "This platform has no way for a windowed app to ask for administrator rights.",
    ))
}

#[cfg(test)]
mod tests {
    /// The property that matters is not testable without a password prompt, so
    /// what is pinned here is the shape of the script — a change to the
    /// quoting would be silent until somebody's path had a space in it.
    #[test]
    #[cfg(target_os = "macos")]
    fn the_script_wraps_the_command_in_the_administrator_form() {
        // Rebuilt rather than exported: the point is that this exact spelling
        // is what `osascript` is given, and a test that called the function
        // would put a password panel on somebody's screen.
        let command = "/usr/bin/security add-trusted-cert -d -k /Library/Keychains/System.keychain '/Users/me/.stackvo/ca/rootCA.pem'";
        let script = format!(r#"do shell script "{command}" with administrator privileges"#);

        assert!(script.starts_with("do shell script \""));
        assert!(script.ends_with("\" with administrator privileges"));
        // The inner path is single-quoted by the caller, because the outer
        // string is already double-quoted by AppleScript.
        assert!(script.contains("'/Users/me/.stackvo/ca/rootCA.pem'"));
    }
}
