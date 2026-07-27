//! Host paths that have to survive a trip into Docker.
//!
//! This is the part of the Windows story that is not "recompile and hope".
//! Bind mounts in `docker-compose.projects.yml` carry absolute *host* paths,
//! and Docker Desktop on Windows expects them in a specific shape:
//!
//!   `C:\Users\me\stackvo`  →  `/c/Users/me/stackvo`
//!
//! A backslash path in a compose file is not merely ugly — YAML treats `\` as
//! an escape inside double quotes, and Compose's own path parser splits mount
//! specs on `:`, which a drive letter also contains. Getting this wrong
//! produces mounts that silently point at the wrong place.
//!
//! On macOS and Linux every function here is the identity, so the same code
//! path runs everywhere and the Windows behaviour is exercised by tests on
//! every platform rather than only on Windows.

/// Convert a host path into the form Docker bind mounts expect.
///
/// Windows only: `C:\Users\me` → `/c/Users/me`. Elsewhere the path is already
/// in the right shape and is returned unchanged.
///
/// Takes a `&str` rather than a `&Path` so the Windows behaviour can be tested
/// on any platform — constructing a `Path` from a Windows-style string on Unix
/// does not produce a Windows path, so a `Path`-based API would be untestable
/// exactly where it matters.
pub fn to_docker_mount(text: &str) -> String {
    // A UNC prefix (\\?\C:\…) shows up in canonicalised Windows paths and is
    // meaningless to Docker.
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);

    // Already POSIX, or a UNC network share we should not mangle.
    if !looks_like_windows_path(text) {
        return text.replace('\\', "/");
    }

    let (drive, rest) = text.split_at(1);
    // Skip the ":" and any following separator.
    let rest = rest.trim_start_matches(':');
    let rest = rest.replace('\\', "/");
    let rest = rest.trim_start_matches('/');

    format!("/{}/{}", drive.to_lowercase(), rest)
}

/// `C:\…` or `C:/…` — a drive letter followed by a colon.
fn looks_like_windows_path(text: &str) -> bool {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), Some(':')) => c.is_ascii_alphabetic(),
        _ => false,
    }
}

/// The Docker endpoint for this platform.
///
/// On Windows the daemon listens on a named pipe rather than a unix socket,
/// which is why `engine::resolve_endpoint` cannot simply look for `.sock`
/// files there.
pub const WINDOWS_NAMED_PIPE: &str = r"\\.\pipe\docker_engine";

/// Is this endpoint a named pipe rather than a socket path?
///
/// Docker writes `DOCKER_HOST` as `npipe:////./pipe/docker_engine` — the scheme
/// followed by a FORWARD-slash pipe path — while the pipe itself is normally
/// written `\\.\pipe\docker_engine`. Both forms reach here, so both count.
pub fn is_named_pipe(endpoint: &str) -> bool {
    let bare = strip_endpoint_scheme(endpoint);
    endpoint.starts_with("npipe:")
        || bare.starts_with(r"\\.\pipe\")
        || bare.starts_with("//./pipe/")
}

/// Normalise a `DOCKER_HOST` value into a bare endpoint.
pub fn strip_endpoint_scheme(host: &str) -> &str {
    for scheme in ["unix://", "npipe://", "npipe:"] {
        if let Some(rest) = host.strip_prefix(scheme) {
            return rest;
        }
    }
    host
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_drive_paths_become_docker_mounts() {
        assert_eq!(
            to_docker_mount(r"C:\Users\me\stackvo"),
            "/c/Users/me/stackvo"
        );
        assert_eq!(to_docker_mount(r"D:\projects"), "/d/projects");
        // Forward slashes are just as valid on Windows.
        assert_eq!(to_docker_mount("C:/Users/me"), "/c/Users/me");
    }

    #[test]
    fn the_unc_prefix_from_canonicalize_is_dropped() {
        // std::fs::canonicalize returns \\?\C:\… on Windows; Docker rejects it.
        assert_eq!(to_docker_mount(r"\\?\C:\Users\me"), "/c/Users/me");
    }

    #[test]
    fn posix_paths_are_untouched() {
        assert_eq!(to_docker_mount("/Users/me/stackvo"), "/Users/me/stackvo");
        assert_eq!(
            to_docker_mount("/var/run/docker.sock"),
            "/var/run/docker.sock"
        );
    }

    #[test]
    fn a_drive_root_does_not_produce_a_double_slash() {
        assert_eq!(to_docker_mount(r"C:\"), "/c/");
        assert_eq!(to_docker_mount("C:"), "/c/");
    }

    #[test]
    fn only_a_single_letter_before_the_colon_counts_as_a_drive() {
        // A Windows-style drive is one letter. Anything else is a path that
        // happens to contain a colon and must not be rewritten.
        assert_eq!(to_docker_mount("http://example.com"), "http://example.com");
        assert_eq!(to_docker_mount("/tmp/weird:name"), "/tmp/weird:name");
    }

    #[test]
    fn named_pipes_are_recognised_in_both_spellings() {
        // The backslash form, as the pipe is normally written…
        assert!(is_named_pipe(WINDOWS_NAMED_PIPE));
        // …and the forward-slash form Docker puts in DOCKER_HOST.
        assert!(is_named_pipe("npipe:////./pipe/docker_engine"));
        assert!(is_named_pipe("//./pipe/docker_engine"));

        assert!(!is_named_pipe("/var/run/docker.sock"));
        assert!(!is_named_pipe("unix:///var/run/docker.sock"));
    }

    #[test]
    fn endpoint_schemes_are_stripped() {
        assert_eq!(
            strip_endpoint_scheme("unix:///var/run/docker.sock"),
            "/var/run/docker.sock"
        );
        assert_eq!(
            strip_endpoint_scheme("npipe:////./pipe/docker_engine"),
            "//./pipe/docker_engine"
        );
        assert_eq!(strip_endpoint_scheme("/already/bare"), "/already/bare");
    }
}
