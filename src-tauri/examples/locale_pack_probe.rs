//! Does a language pack survive the round trip? (M-7)
//!
//! The claim M-7 makes is that adding a language is now a **file**, not a code
//! change. The unit tests check the tag rule and the string count; what they
//! cannot check is the part that decides whether the claim is true — that a
//! pack written to the real config directory is found there again, read back
//! as the same JSON, listed with the label it names itself by, and removed
//! completely.
//!
//! ```sh
//! cargo run --example locale_pack_probe
//! ```
//!
//! It writes exactly one pack under a tag no language uses (`qq`), and removes
//! it. Anything already in that directory is listed and left alone.

use stackvo_desktop_lib::locale;

const TAG: &str = "qq";

fn main() {
    let Some(dir) = locale::packs_dir() else {
        println!("this machine has no config directory; nothing was measured.");
        return;
    };
    println!("  packs live in {}", dir.display());

    let before = locale::packs();
    println!(
        "  {} pack(s) already installed: {:?}",
        before.len(),
        before.iter().map(|p| &p.tag).collect::<Vec<_>>()
    );
    if before.iter().any(|p| p.tag == TAG) {
        println!("  a pack is already installed under {TAG}; refusing to touch it.");
        return;
    }

    // A pack is the shipped catalogue's shape with some of it translated. Two
    // nested keys and a label is enough to exercise every rule this has.
    let messages = serde_json::json!({
        "language": { "label": "Qqish" },
        "app": { "refresh": "Ávfresk" },
        "nav": { "projects": "Prôjekt" }
    });

    let path = match locale::write_pack(TAG, &messages) {
        Ok(path) => path,
        Err(e) => {
            println!("  FAIL could not write the pack: {}", e.message);
            return;
        }
    };
    println!("  written to {path}");

    let listed = locale::packs();
    let found = listed.iter().find(|p| p.tag == TAG);
    let ok_listed =
        found.is_some_and(|p| p.label == "Qqish" && p.strings == 3 && p.broken.is_none());
    println!(
        "  {} listed as {:?}",
        if ok_listed { "ok  " } else { "FAIL" },
        found.map(|p| (&p.label, p.strings, &p.broken))
    );

    let read = locale::read_pack(TAG);
    let ok_read = read.as_ref().is_ok_and(|v| *v == messages);
    println!(
        "  {} read back {}",
        if ok_read { "ok  " } else { "FAIL" },
        if ok_read {
            "identical".to_string()
        } else {
            format!("{read:?}")
        }
    );

    // The failure this catches: a file with a trailing comma that vanishes from
    // the picker instead of saying what is wrong with it.
    let _ = std::fs::write(dir.join(format!("{TAG}.json")), "{ \"a\": 1, }");
    let broken = locale::packs().into_iter().find(|p| p.tag == TAG);
    let ok_broken = broken.as_ref().is_some_and(|p| p.broken.is_some());
    println!(
        "  {} a malformed pack is listed with its error, not skipped",
        if ok_broken { "ok  " } else { "FAIL" }
    );

    let removed = locale::delete_pack(TAG).is_ok() && !locale::packs().iter().any(|p| p.tag == TAG);
    println!(
        "  {} removed, and removing it again succeeds: {}",
        if removed { "ok  " } else { "FAIL" },
        locale::delete_pack(TAG).is_ok()
    );

    println!();
    let after = locale::packs();
    if ok_listed && ok_read && ok_broken && removed && after.len() == before.len() {
        println!("a language is a file this app finds, reads and can be told to forget.");
    } else {
        println!("the round trip did not hold; check {}", dir.display());
    }
}
