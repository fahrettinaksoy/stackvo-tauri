//! The desktop accent readback.
//!
//! Asserted rather than eyeballed because the failure mode is silent: a shape
//! change or an unparsed preference resolves to "no colour", the setting quietly
//! keeps the app's own blue, and nothing anywhere says the read failed.

use stackvo_desktop_lib::commands;

#[test]
fn reports_a_usable_colour() {
    let value = commands::system_accent();

    let available = value["available"].as_bool().expect("available is a bool");

    #[cfg(target_os = "macos")]
    {
        assert!(
            available,
            "macOS always resolves an accent, multicolour included"
        );

        let hex = value["hex"].as_str().expect("hex is a string");
        assert_eq!(hex.len(), 7, "#rrggbb, got {hex}");
        assert!(hex.starts_with('#'), "hex is #-prefixed, got {hex}");
        assert!(
            hex[1..].chars().all(|c| c.is_ascii_hexdigit()),
            "hex digits only, got {hex}"
        );

        // The name has to be one the mapping knows, or the colour silently
        // falls back to blue for everyone who picked something else.
        let name = value["name"].as_str().expect("name is a string");
        assert!(
            ["Blue", "Purple", "Pink", "Red", "Orange", "Yellow", "Green", "Graphite"]
                .contains(&name),
            "unmapped accent name {name:?} — the mapping in commands.rs needs it"
        );
    }

    #[cfg(not(target_os = "macos"))]
    assert!(!available, "only macOS is wired up so far");
}
