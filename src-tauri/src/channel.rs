//! Which update this install is offered, and whether it is offered at all.
//!
//! §3 #21 — release channels, staged rollout, rollback — sat behind #2 for a
//! reason that turned out to be arithmetic rather than engineering: none of it
//! means anything until there is a place to publish to and a key to sign with.
//! ADR 0025 answered both, so this is now ordinary work.
//!
//! ## What the updater plugin does and does not do
//!
//! `tauri-plugin-updater` fetches a manifest, compares its `version` against
//! the running one, verifies a signature, and installs. That is the whole of
//! it. It has no notion of a channel, no notion of a percentage, and — the one
//! that matters most — **no way to stop**. A release discovered to be broken
//! cannot be recalled, because every running copy will keep asking the same
//! endpoint and getting the same answer.
//!
//! So the three features are not three plugin settings. They are three fields
//! in the manifest and one decision made before the plugin is asked, and that
//! decision is what this module is.
//!
//! ## Rollback is the one that has to work first
//!
//! Channels and rollouts are conveniences. Rollback is the thing that decides
//! how bad a bad release gets, and the updater cannot do it by publishing an
//! older version: the plugin refuses to move backwards, so re-publishing 0.2.0
//! over 0.2.1 changes nothing for anybody who already took 0.2.1.
//!
//! Two fields, because "stop" and "go back" are different intentions:
//!
//! * `paused` — offer nothing. The release stands, and nobody else takes it.
//!   This is the button somebody presses at 2am, and it must not require
//!   building anything.
//! * `supersededBy` — offer this version *instead*, even though it is older.
//!   The explicit downgrade, which the plugin will not do on its own.
//!
//! ## Why the rollout bucket is hashed and not random
//!
//! A percentage implemented with a random number re-rolls on every check: an
//! install lands inside the 10% one hour and outside it the next, so a staged
//! rollout would offer an update, withdraw it, and offer it again. The bucket
//! has to be **stable for this install and this version**, and independent
//! between versions — otherwise the same unlucky 10% is first every time, and
//! a staged rollout stops being a sample.
//!
//! `sha256(install_id : version)`, first eight bytes, modulo 100.

use serde::{Deserialize, Serialize};

/// The stream an install follows.
///
/// Two, and adding a third is a decision rather than a variant: every channel
/// is a release somebody has to actually cut, and a channel nobody publishes to
/// is a setting that silently stops updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    /// The name this channel's manifest is published under.
    ///
    /// Stable keeps `latest.json` because that is the name `tauri-action`
    /// writes and ADR 0025 pointed the endpoint at; a beta manifest sits beside
    /// it under its own name rather than in a second release, so one publish
    /// produces both and they cannot drift apart.
    pub fn manifest_name(self) -> &'static str {
        match self {
            Channel::Stable => "latest.json",
            Channel::Beta => "beta.json",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "stable" => Some(Channel::Stable),
            "beta" => Some(Channel::Beta),
            _ => None,
        }
    }
}

/// The fields this app adds to the updater manifest.
///
/// `tauri-plugin-updater` ignores keys it does not know, so these travel in the
/// same `latest.json` it already reads — one file, one publish, no second
/// endpoint that can be forgotten.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rollout {
    /// The version the manifest offers.
    pub version: String,
    /// Which stream this manifest belongs to.
    pub channel: Channel,
    /// Percentage of installs that may take it, 0–100.
    ///
    /// Absent means 100: a manifest published without thinking about staging is
    /// a manifest offered to everybody, which is what happens today and what
    /// somebody who has not read this file will expect.
    #[serde(default = "everybody")]
    pub percent: u8,
    /// Offer nothing at all, whatever else this manifest says.
    pub paused: bool,
    /// Offer this version instead — including when it is older.
    ///
    /// The explicit downgrade. Empty means "no, take `version`".
    pub superseded_by: Option<String>,
}

fn everybody() -> u8 {
    100
}

/// What this install should be offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome", content = "detail")]
pub enum Offer {
    /// Take this version.
    Update(String),
    /// Nothing to do: already current, or newer.
    UpToDate,
    /// The publisher stopped this release.
    Paused,
    /// In the channel, but not in this wave yet.
    Waiting { bucket: u8, percent: u8 },
    /// The manifest is for a different stream.
    OtherChannel(Channel),
}

/// Which hundredth this install falls in for this version.
///
/// Public because it is the part a person will want to check by hand when an
/// install did not get an update they expected — `bucket(id, v) < percent` is
/// the whole rule, and being able to compute it is the difference between a
/// staged rollout and a black box.
pub fn bucket(install_id: &str, version: &str) -> u8 {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(install_id.as_bytes());
    // A separator, so `("ab", "c")` and `("a", "bc")` are different installs
    // rather than the same one. `:` cannot appear in a version.
    hasher.update(b":");
    hasher.update(version.as_bytes());
    let digest = hasher.finalize();

    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(head) % 100) as u8
}

/// Compare two dotted versions numerically.
///
/// Not `semver`: the crate is a dependency for a comparison this needs three
/// numbers for, and the manifest's versions are this app's own. A part that
/// does not parse sorts as 0, which makes a malformed version *older* — the
/// safe direction, because the outcome is "no update" rather than a downgrade
/// to something unreadable.
fn newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parts(candidate), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// The whole decision, in the order it has to be made.
///
/// Order is the design. `paused` comes before everything because it is the
/// emergency stop and an emergency stop that can be overridden by a channel
/// mismatch is not one. `supersededBy` comes before the version comparison,
/// because its entire purpose is to defeat that comparison. The rollout bucket
/// comes last, because a wave is only meaningful for an update that would
/// otherwise be offered.
pub fn offer(manifest: &Rollout, wanted: Channel, install_id: &str, current: &str) -> Offer {
    if manifest.paused {
        return Offer::Paused;
    }
    if manifest.channel != wanted {
        return Offer::OtherChannel(manifest.channel);
    }

    // The deliberate downgrade. Checked against the RUNNING version, not
    // against the manifest's: superseding 0.2.1 with 0.2.0 must be a no-op for
    // somebody already on 0.2.0, or every install in the world reinstalls.
    if let Some(target) = manifest.superseded_by.as_deref().filter(|t| !t.is_empty()) {
        if target == current {
            return Offer::UpToDate;
        }
        return Offer::Update(target.to_string());
    }

    if !newer(&manifest.version, current) {
        return Offer::UpToDate;
    }

    let percent = manifest.percent.min(100);
    let bucket = bucket(install_id, &manifest.version);
    if bucket >= percent {
        return Offer::Waiting { bucket, percent };
    }

    Offer::Update(manifest.version.clone())
}

/// This install's identity, for the rollout bucket and nothing else.
///
/// A random string generated once and kept beside the preferences. Not a
/// machine id, not a hardware fingerprint, not anything derived from the user:
/// the only property required is that it is **stable and unique per install**,
/// and anything with more meaning than that is a value that leaves this machine
/// the first time somebody adds telemetry. `privacy_claims.rs` states the rule
/// this follows.
///
/// It never leaves the machine at all — the bucket is computed here and
/// compared here; the endpoint is asked for a file, not told who is asking.
pub fn install_id() -> String {
    let path = match crate::appdir::config() {
        Some(dir) => dir.join("install-id"),
        // No config directory is a machine this app can barely run on. A fixed
        // string means the rollout is not random for it, which is a worse
        // answer than a stable one only if you assume there are many such
        // machines — and an empty id would put every one of them in bucket 0,
        // i.e. always first into every wave.
        None => return "unidentified-install".to_string(),
    };

    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let fresh = fresh_id();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // A write that fails is not an error worth surfacing: the id is recomputed
    // next time, the install lands in a different bucket, and the only
    // consequence is that a staged rollout treats it as a new install. Failing
    // the update check over it would be worse than the thing it protects.
    let _ = std::fs::write(&path, &fresh);
    fresh
}

/// Sixteen bytes of randomness, hex.
///
/// From the OS. Not from a clock: `architecture_claims.rs` bans a clock as an
/// identity, and it banned it because two things built from
/// `SystemTime::now()` in the same microsecond are the same thing — which for
/// a rollout bucket would mean two installs that always update together.
///
/// A failure to read the OS generator falls back to a fixed string rather than
/// to a weaker source. That is a worse *bucket* — every such install lands in
/// the same one — and it is the honest failure: a home-made generator here
/// would be a value that looks random and is not, which is the state this
/// comment exists to prevent.
fn fresh_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        return "unidentified-install".to_string();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str) -> Rollout {
        Rollout {
            version: version.to_string(),
            channel: Channel::Stable,
            percent: 100,
            paused: false,
            superseded_by: None,
        }
    }

    #[test]
    fn an_ordinary_newer_release_is_offered() {
        assert_eq!(
            offer(&manifest("0.2.0"), Channel::Stable, "install", "0.1.0"),
            Offer::Update("0.2.0".into())
        );
    }

    #[test]
    fn the_running_version_and_anything_older_is_not() {
        for current in ["0.2.0", "0.3.0"] {
            assert_eq!(
                offer(&manifest("0.2.0"), Channel::Stable, "install", current),
                Offer::UpToDate,
                "current {current}"
            );
        }
    }

    #[test]
    fn pausing_beats_everything_else_in_the_manifest() {
        // The 2am button. It has to win over a channel match, a rollout wave
        // and a supersede — anything that can override it is a way for a
        // recalled release to keep installing.
        let mut m = manifest("0.2.0");
        m.paused = true;
        m.percent = 100;
        m.superseded_by = Some("0.1.9".into());
        assert_eq!(
            offer(&m, Channel::Stable, "install", "0.1.0"),
            Offer::Paused
        );
    }

    #[test]
    fn a_supersede_goes_backwards_which_is_the_whole_point() {
        // The updater plugin will not do this on its own: republishing an older
        // version leaves everybody who already took the bad one on it.
        let mut m = manifest("0.2.1");
        m.superseded_by = Some("0.2.0".into());
        assert_eq!(
            offer(&m, Channel::Stable, "install", "0.2.1"),
            Offer::Update("0.2.0".into())
        );
    }

    #[test]
    fn a_supersede_to_the_version_already_running_does_nothing() {
        // Checked against the RUNNING version rather than the manifest's. The
        // other way round, every install on 0.2.0 would reinstall 0.2.0 on
        // every check, forever.
        let mut m = manifest("0.2.1");
        m.superseded_by = Some("0.2.0".into());
        assert_eq!(
            offer(&m, Channel::Stable, "install", "0.2.0"),
            Offer::UpToDate
        );
    }

    #[test]
    fn a_manifest_for_another_channel_is_not_offered() {
        let mut m = manifest("0.9.0");
        m.channel = Channel::Beta;
        assert_eq!(
            offer(&m, Channel::Stable, "install", "0.1.0"),
            Offer::OtherChannel(Channel::Beta)
        );
    }

    #[test]
    fn a_bucket_is_stable_for_one_install_and_one_version() {
        // The failure a random number would have: an install inside the wave on
        // one check and outside it on the next, so the update appears and
        // disappears.
        for _ in 0..5 {
            assert_eq!(bucket("install-a", "0.2.0"), bucket("install-a", "0.2.0"));
        }
    }

    #[test]
    fn a_bucket_moves_between_versions_so_the_same_installs_are_not_always_first() {
        // Independence between versions is what makes a rollout a sample. If
        // the bucket were a property of the install alone, the same unlucky 1%
        // would take every release first, for ever.
        let ids: Vec<&str> = vec!["a", "b", "c", "d", "e", "f", "g", "h"];
        let moved = ids
            .iter()
            .filter(|id| bucket(id, "0.2.0") != bucket(id, "0.3.0"))
            .count();
        assert!(
            moved >= 6,
            "only {moved} of 8 installs changed bucket between versions"
        );
    }

    #[test]
    fn the_separator_keeps_two_different_pairs_apart() {
        // Without it, `("ab", "c")` and `("a", "bc")` hash the same bytes and
        // are one install.
        assert_ne!(bucket("ab", "c"), bucket("a", "bc"));
    }

    #[test]
    fn a_wave_offers_roughly_its_percentage() {
        // Not exactly: a hash is not a shuffle, and demanding an exact split
        // would be a test of the digest rather than of the rule. What is
        // checked is that the number is in the right neighbourhood over enough
        // installs to mean something — a rollout that offers 90% at `percent:
        // 10` is the bug worth catching.
        let mut m = manifest("0.2.0");
        m.percent = 10;

        let offered = (0..1000)
            .filter(|i| {
                matches!(
                    offer(&m, Channel::Stable, &format!("install-{i}"), "0.1.0"),
                    Offer::Update(_)
                )
            })
            .count();
        assert!(
            (60..=140).contains(&offered),
            "{offered} of 1000 installs were offered a 10% rollout"
        );
    }

    #[test]
    fn a_zero_percent_wave_offers_nobody_and_a_hundred_offers_everybody() {
        let mut m = manifest("0.2.0");
        for (percent, expect_any) in [(0u8, false), (100, true)] {
            m.percent = percent;
            let offered = (0..200)
                .filter(|i| {
                    matches!(
                        offer(&m, Channel::Stable, &format!("install-{i}"), "0.1.0"),
                        Offer::Update(_)
                    )
                })
                .count();
            assert_eq!(
                offered > 0,
                expect_any,
                "{offered} offered at percent {percent}"
            );
            if percent == 100 {
                assert_eq!(offered, 200, "a full rollout must reach every install");
            }
        }
    }

    #[test]
    fn a_percentage_above_a_hundred_is_a_full_rollout_rather_than_nobody() {
        // `bucket` is 0..=99, so an unclamped 200 would still offer everybody —
        // but `percent` is a u8 read from a file somebody edits, and the
        // clamping is stated rather than emergent.
        let mut m = manifest("0.2.0");
        m.percent = 250;
        assert_eq!(
            offer(&m, Channel::Stable, "install", "0.1.0"),
            Offer::Update("0.2.0".into())
        );
    }

    #[test]
    fn a_manifest_with_no_percent_reaches_everybody() {
        // The compatibility case: `latest.json` as it is published today has
        // none of these fields. Deserialising it must mean "offered to all",
        // not "offered to nobody", or adding this module would stop every
        // update in the field.
        let json = r#"{"version":"0.2.0","notes":"","pub_date":"","platforms":{}}"#;
        let m: Rollout = serde_json::from_str(json).expect("an ordinary manifest parses");
        assert_eq!(m.percent, 100);
        assert_eq!(m.channel, Channel::Stable);
        assert!(!m.paused);
        assert_eq!(
            offer(&m, Channel::Stable, "install", "0.1.0"),
            Offer::Update("0.2.0".into())
        );
    }

    #[test]
    fn an_unreadable_version_is_treated_as_older_rather_than_newer() {
        // The safe direction: the outcome is "no update", not a move to
        // something nothing could parse.
        let m = manifest("not-a-version");
        assert_eq!(
            offer(&m, Channel::Stable, "install", "0.1.0"),
            Offer::UpToDate
        );
    }

    #[test]
    fn channels_name_different_files_in_one_release() {
        assert_eq!(Channel::Stable.manifest_name(), "latest.json");
        assert_ne!(
            Channel::Beta.manifest_name(),
            Channel::Stable.manifest_name()
        );
        assert_eq!(Channel::parse("BETA"), Some(Channel::Beta));
        assert_eq!(Channel::parse("nightly"), None);
    }

    #[test]
    fn version_comparison_is_numeric_and_not_lexical() {
        // `"0.10.0" > "0.9.0"` is false as strings, and an updater that never
        // offered 0.10 would look like a broken endpoint.
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("0.9.0", "0.10.0"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(!newer("0.2.0", "0.2.0"));
    }
}
