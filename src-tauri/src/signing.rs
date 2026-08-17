//! Verifying that an index was signed by a key this machine already trusts.
//!
//! The first link of the chain `market.rs` describes, and the one it said was
//! missing:
//!
//! ```text
//!   a pinned key          →  registry.json          (here)
//!   registry.json         →  manifest.json          (market::refresh)
//!   manifest.json         →  every file it ships    (pkg::verify)
//! ```
//!
//! Until this existed `Trust::Signed` was a shape with no implementation, so a
//! third-party source could be *fetched* but never *believed*, and
//! `docs/durum.md`'s C item recorded the architecture as "ready" for a door it
//! could not actually hold shut. It can now.
//!
//! ## minisign, and why it costs nothing
//!
//! ADR 0015 says the registry gets its own ed25519 key pair, separate from the
//! updater's. minisign is ed25519 with a file format around it, it is what
//! Tauri's updater already uses to check a downloaded binary, and
//! `minisign-verify` is therefore **already in `Cargo.lock`** — measured:
//! taking it directly adds a dependency edge and **zero packages**.
//!
//! It also means the key ceremony has a tool that exists: `minisign -G`, or
//! Tauri's own `signer generate`. A scheme needing a bespoke tool is one whose
//! ceremony never happens.
//!
//! ## No key is shipped, and that is deliberate
//!
//! [`PINNED`] is empty. The official registry's key is `docs/durum.md` §5's
//! open decision — the same turn as the updater endpoint — and inventing a
//! placeholder would be worse than the gap, because every later reader would
//! believe the chain was closed. So:
//!
//! * With no key pinned, asking for a signed refresh **fails**, and says which
//!   of the two things is missing.
//! * An organisation running its own mirror pins **its own** key through
//!   `policy.market.additionalKeys` and gets real verification today. That is not
//!   a workaround; it is the case `policy.rs` was written for, and it is what
//!   makes third-party distribution an operational decision rather than a code
//!   one.
//!
//! ## Rotation is designed in, because it cannot be added later
//!
//! ADR 0015: "a pinning with no rotation plan is one whose only answer to a
//! leak is *everybody update the app*". So [`Keys`] holds a **set**, and a
//! machine trusts an index signed by any key in it. A new key is introduced by
//! a `known-keys.json` that is itself signed by a key already trusted —
//! [`adopt`] — so the trust moves forward on the strength of the old key
//! rather than on the strength of having been downloaded.
//!
//! What that deliberately cannot do is *remove* the compromised key on its
//! own say-so. A leaked key can sign a `known-keys.json` naming only itself,
//! so revocation is a build, and [`Keys::retired`] is what a build carries.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};

/// Keys compiled into this build.
///
/// Empty, and the module comment says why. A build with an official key adds
/// it here, and `this_build_pins_no_official_key_yet` is what makes adding one a
/// deliberate act with a document to update.
pub const PINNED: &[&str] = &[];

/// Keys this build refuses even when something presents them.
///
/// Separate from simply not being in [`PINNED`], because rotation moves keys
/// **in** at run time: a leaked key that is merely absent can be reintroduced
/// by a `known-keys.json` it signed itself. One listed here cannot, whatever
/// signs the document that names it.
pub const RETIRED: &[&str] = &[];

/// A minisign public key, as it appears in a `.pub` file's second line.
///
/// Stored as the base64 line rather than parsed bytes: it is what a person
/// copies out of a key file, what a policy file carries, and what an error
/// message has to be able to print back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublicKey(pub String);

impl PublicKey {
    /// The key id minisign puts in front of every signature.
    ///
    /// Eight bytes, shown so a refusal can say *which* key signed the thing it
    /// refused — "signed by a key this machine does not trust" is a sentence
    /// somebody can act on only if it names the key.
    pub fn id(&self) -> String {
        // The untrusted-comment line is optional in a `.pub` file; the key line
        // is the one that matters and is what this holds.
        let trimmed = self.0.trim();
        trimmed.chars().take(16).collect()
    }

    fn parse(&self) -> Result<minisign_verify::PublicKey> {
        minisign_verify::PublicKey::from_base64(self.0.trim()).map_err(|e| {
            Error::new(
                Code::InvalidInput,
                format!("\"{}…\" is not a minisign public key: {e}", self.id()),
            )
            .with_hint(
                "A key is the second line of a minisign `.pub` file — the one that is not \
                 a comment."
                    .to_string(),
            )
        })
    }
}

/// Everything this machine will accept a signature from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keys {
    /// Trusted, in the order they were learned: compiled in first, adopted
    /// after.
    pub trusted: Vec<PublicKey>,
    /// Refused whatever presents them. Only a build can add to this.
    #[serde(default)]
    pub retired: Vec<PublicKey>,
}

impl Keys {
    /// What this build trusts before anything is adopted.
    pub fn pinned() -> Self {
        Self {
            trusted: PINNED.iter().map(|k| PublicKey(k.to_string())).collect(),
            retired: RETIRED.iter().map(|k| PublicKey(k.to_string())).collect(),
        }
    }

    /// The same, plus whatever an administrator pinned through policy.
    ///
    /// `policy.market.additionalKeys` was written for this and had no reader
    /// until now — its own comment says "for an organisation signing its own
    /// mirror". Added rather than replacing: a machine that forgot the
    /// official key while trusting a mirror would be one that could not go
    /// back to the official registry without a reinstall.
    ///
    /// A retired key is refused here too. Policy is an administrator's file,
    /// and an administrator can be handed a key that has since leaked.
    pub fn with_policy(mut self, keys: &[String]) -> Self {
        for key in keys {
            let key = PublicKey(key.trim().to_string());
            if key.0.is_empty() || self.trusted.contains(&key) {
                continue;
            }
            if self.is_retired(&key) {
                tracing::warn!(key = %key.id(), "policy names a retired key; ignoring it");
                continue;
            }
            self.trusted.push(key);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.trusted.is_empty()
    }

    fn is_retired(&self, key: &PublicKey) -> bool {
        self.retired.iter().any(|k| k.0.trim() == key.0.trim())
    }

    /// Check `signature` over `bytes` against every key, and say which one
    /// worked.
    ///
    /// Every key is tried because rotation means more than one is legitimate
    /// at once. A retired key is skipped **before** it is tried rather than
    /// after: a signature that verifies and is then discarded is one somebody
    /// eventually refactors into an acceptance.
    pub fn verify(&self, bytes: &[u8], signature: &str) -> Result<PublicKey> {
        if self.is_empty() {
            return Err(Error::new(
                Code::Unsupported,
                "no registry key is pinned in this build, so a signature cannot be checked",
            )
            .with_hint(crate::hints::NO_REGISTRY_KEY));
        }

        let signature = minisign_verify::Signature::decode(signature.trim()).map_err(|e| {
            Error::new(
                Code::InvalidManifest,
                format!("the signature file is not a minisign signature: {e}"),
            )
        })?;

        let mut tried = Vec::new();
        for key in &self.trusted {
            if self.is_retired(key) {
                continue;
            }
            let parsed = key.parse()?;
            // `true` — accept minisign's older, non-prehashed signatures too.
            //
            // This was `false` first, on the reasoning that the two modes sign
            // different things and accepting both would let a signature made
            // over one be valid for another. Working through it, that is not
            // what happens: the mode is declared in the signature file, the
            // verifier hashes or does not hash accordingly, and presenting a
            // signature of one kind as the other simply fails to verify. There
            // is no mode-confusion to win.
            //
            // So refusing legacy bought nothing and cost something real — an
            // organisation whose mirror was signed by an older `minisign`
            // would have been refused with no way to tell why from the
            // message. `policy.market.additionalKeys` exists for exactly those
            // people.
            if parsed.verify(bytes, &signature, true).is_ok() {
                return Ok(key.clone());
            }
            tried.push(key.id());
        }

        Err(Error::new(
            Code::PermissionDenied,
            format!(
                "the index is signed, but by none of the {} key(s) this machine trusts ({})",
                tried.len(),
                tried.join(", ")
            ),
        )
        .with_hint(crate::hints::SIGNED_BY_UNKNOWN_KEY))
    }
}

/// A `known-keys.json`: the document that introduces a new key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownKeys {
    pub schema_version: u32,
    /// Monotonic, for the same reason the index has one: replaying an older
    /// document is how a key that was rotated away comes back.
    pub sequence: u64,
    pub keys: Vec<PublicKey>,
}

/// A rotation document has to be newer than the one already applied.
///
/// Its own function so it can be checked directly: reaching it through
/// [`adopt`] needs a signature over a JSON document, and a test that cannot
/// reach a rule is a rule with no test. Replaying yesterday's document is how
/// a key that was rotated away comes back.
fn moves_forward(sequence: u64, last: u64) -> Result<()> {
    if sequence <= last {
        return Err(Error::new(
            Code::Conflict,
            format!(
                "known-keys.json is at sequence {sequence} and this machine already has \
                 {last} — an older document is how a rotated-away key comes back"
            ),
        ));
    }
    Ok(())
}

/// Learn the keys in `document`, on the strength of a key already trusted.
///
/// The rotation step from ADR 0015. Three things it will not do, each of which
/// would turn rotation into a way in:
///
/// * **Accept an unsigned document.** The signature is checked against the
///   keys already held, before a field is read.
/// * **Go backwards.** A lower `sequence` is refused, so yesterday's document
///   cannot reintroduce a key today's removed.
/// * **Un-retire.** A key in [`Keys::retired`] stays out even when a perfectly
///   valid document names it, because the document could have been signed by
///   the leaked key itself.
pub fn adopt(
    current: &Keys,
    document: &[u8],
    signature: &str,
    last_sequence: u64,
) -> Result<(Keys, PublicKey)> {
    let by = current.verify(document, signature)?;

    let parsed: KnownKeys = serde_json::from_slice(document).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("known-keys.json is unreadable: {e}"),
        )
    })?;

    if parsed.schema_version != 1 {
        return Err(Error::new(
            Code::Unsupported,
            format!(
                "known-keys.json declares schema version {}, and this build reads 1",
                parsed.schema_version
            ),
        ));
    }

    moves_forward(parsed.sequence, last_sequence)?;

    let mut next = current.clone();
    next.trusted.clear();
    for key in parsed.keys {
        if current.is_retired(&key) {
            // Named and skipped rather than failing the whole document: the
            // rest of a rotation is still worth applying, and a build that
            // retired a key means it whatever a document says.
            tracing::warn!(key = %key.id(), "known-keys.json names a retired key; ignoring it");
            continue;
        }
        next.trusted.push(key);
    }

    if next.trusted.is_empty() {
        return Err(Error::new(
            Code::InvalidManifest,
            "known-keys.json would leave this machine trusting nothing",
        ));
    }

    Ok((next, by))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real minisign key and a real signature it made, over the bytes
    /// `test`. Taken from `minisign-verify`'s own test vector, which is the
    /// point: a key pair this file generated for itself would let it agree
    /// with its own idea of what minisign is, and the QR encoder already cost
    /// this repository that lesson once.
    ///
    /// Without a **passing** verification here, every other test in this
    /// module still passes when `verify` is broken to always fail.
    const TEST_PUB: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const TEST_PAYLOAD: &[u8] = b"test";
    const TEST_SIG: &str = "untrusted comment: signature from minisign secret key
RWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=
trusted comment: timestamp:1555779966\tfile:test
QtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";

    /// A different, valid key that did not make that signature.
    const OTHER_PUB: &str = "RWTf9M1kNu0hxJVCiHNYRJ3n8UGQeb1XwSpDGSLYqAqZOHTNVwQBrq0X";

    fn keys(list: &[&str]) -> Keys {
        Keys {
            trusted: list.iter().map(|k| PublicKey(k.to_string())).collect(),
            retired: Vec::new(),
        }
    }

    /// The gap this module exists to close, stated as a test: with nothing
    /// pinned, a signed refresh is refused rather than quietly downgraded.
    #[test]
    fn a_build_with_no_pinned_key_refuses_rather_than_accepting() {
        let error = Keys::default()
            .verify(b"anything", "irrelevant")
            .unwrap_err();
        assert_eq!(error.code, Code::Unsupported);
        assert!(
            error.message.contains("no registry key"),
            "{}",
            error.message
        );
    }

    /// And the shipped constant really is empty, so nobody reads the sentence
    /// above and assumes a key arrived at some point.
    #[test]
    fn this_build_pins_no_official_key_yet() {
        assert!(
            PINNED.is_empty(),
            "a key was added — update docs/durum.md's C item and ADR 0015, because \
             the chain is now closed and the document says it is not"
        );
        assert!(Keys::pinned().is_empty());
    }

    #[test]
    fn a_policy_key_is_added_rather_than_replacing_what_is_pinned() {
        let with = keys(&[TEST_PUB]).with_policy(&[OTHER_PUB.to_string()]);
        assert_eq!(with.trusted.len(), 2);
        assert_eq!(with.trusted[0].0, TEST_PUB, "the pinned key stays first");

        // Blank, repeated and absent values are not new keys.
        assert_eq!(
            with.clone().with_policy(&[OTHER_PUB.into()]).trusted.len(),
            2
        );
        assert_eq!(
            with.clone().with_policy(&["   ".to_string()]).trusted.len(),
            2
        );
        assert_eq!(with.with_policy(&[]).trusted.len(), 2);
    }

    /// An administrator can be handed a key that has since leaked, so policy
    /// does not get to reintroduce a retired one either.
    #[test]
    fn policy_cannot_reintroduce_a_retired_key() {
        let current = Keys {
            trusted: vec![PublicKey(TEST_PUB.into())],
            retired: vec![PublicKey(OTHER_PUB.into())],
        };
        let after = current.with_policy(&[OTHER_PUB.to_string()]);
        assert_eq!(after.trusted.len(), 1);
    }

    #[test]
    fn a_malformed_key_or_signature_is_reported_as_one() {
        let error = keys(&[TEST_PUB])
            .verify(b"x", "not a signature")
            .unwrap_err();
        assert!(error.message.contains("signature"), "{}", error.message);

        let error = keys(&["not a key"])
            .verify(TEST_PAYLOAD, TEST_SIG)
            .unwrap_err();
        assert!(error.message.contains("public key"), "{}", error.message);
    }

    /// The positive path, against a real signature. Everything else in this
    /// module is a refusal, and a verifier that refuses everything passes all
    /// of them.
    #[test]
    fn a_real_signature_from_a_trusted_key_verifies_and_says_which_one() {
        let by = keys(&[TEST_PUB])
            .verify(TEST_PAYLOAD, TEST_SIG)
            .expect("the vector's own key made this signature");
        assert_eq!(by.0, TEST_PUB);
    }

    /// And it is the *bytes* that are verified. One changed character is a
    /// different document, which is the whole point of signing an index.
    #[test]
    fn a_single_changed_byte_is_refused() {
        let error = keys(&[TEST_PUB]).verify(b"Test", TEST_SIG).unwrap_err();
        assert_eq!(error.code, Code::PermissionDenied);
    }

    /// Rotation, working: a machine holding several keys accepts an index
    /// signed by any of them, whichever order they were learned in.
    #[test]
    fn any_trusted_key_may_have_signed_it() {
        assert!(keys(&[OTHER_PUB, TEST_PUB])
            .verify(TEST_PAYLOAD, TEST_SIG)
            .is_ok());
        assert!(keys(&[TEST_PUB, OTHER_PUB])
            .verify(TEST_PAYLOAD, TEST_SIG)
            .is_ok());
    }

    #[test]
    fn a_signature_from_an_untrusted_key_names_the_keys_that_were_tried() {
        let error = keys(&[OTHER_PUB])
            .verify(TEST_PAYLOAD, TEST_SIG)
            .unwrap_err();
        assert_eq!(error.code, Code::PermissionDenied);
        assert!(
            error.message.contains(&PublicKey(OTHER_PUB.into()).id()),
            "the refusal has to name the key that was tried: {}",
            error.message
        );
    }

    /// A retired key does not verify even when it is also listed as trusted —
    /// which is the state a build enters the moment a leak is discovered.
    #[test]
    fn a_retired_key_cannot_verify_even_while_it_is_still_trusted() {
        let leaked = Keys {
            trusted: vec![PublicKey(TEST_PUB.into())],
            retired: vec![PublicKey(TEST_PUB.into())],
        };
        let error = leaked.verify(TEST_PAYLOAD, TEST_SIG).unwrap_err();
        assert_eq!(error.code, Code::PermissionDenied);
    }

    /// An unverifiable rotation document never reaches its own fields.
    ///
    /// The ordering is the property: `adopt` checks the signature before it
    /// parses, so a document whose *contents* look right cannot get anywhere
    /// on the strength of having been downloaded.
    #[test]
    fn a_rotation_document_is_verified_before_it_is_read() {
        let document = serde_json::to_vec(&KnownKeys {
            schema_version: 1,
            sequence: 4,
            keys: vec![PublicKey(OTHER_PUB.into())],
        })
        .unwrap();

        // Perfectly well-formed, signed by nothing this machine trusts.
        let error = adopt(&keys(&[TEST_PUB]), &document, TEST_SIG, 0).unwrap_err();
        assert_eq!(error.code, Code::PermissionDenied);

        // And with no key at all, the refusal is the other one.
        let error = adopt(&Keys::default(), &document, TEST_SIG, 0).unwrap_err();
        assert_eq!(error.code, Code::Unsupported);
    }

    /// The rotation path, past the signature: `test` verifies, so `adopt`
    /// reaches its own parsing and fails there rather than on trust. That
    /// ordering is the thing being asserted.
    #[test]
    fn a_signed_rotation_gets_past_verification_to_its_own_rules() {
        // The vector signs the literal bytes `test`, so that is the document.
        // Standing in for a real known-keys.json, which is why this asserts on
        // the refusal rather than on a parsed result.
        let error = adopt(&keys(&[TEST_PUB]), TEST_PAYLOAD, TEST_SIG, 0).unwrap_err();
        assert_eq!(
            error.code,
            Code::InvalidManifest,
            "the signature verified and parsing is what failed: {}",
            error.message
        );
        assert!(
            error.message.contains("known-keys.json"),
            "{}",
            error.message
        );
    }

    /// Rotation refuses to go backwards, for the same reason the index does.
    #[test]
    fn an_older_or_repeated_known_keys_document_is_refused() {
        assert!(moves_forward(10, 9).is_ok());

        for (sequence, last) in [(9, 9), (8, 9), (0, 1)] {
            let error = moves_forward(sequence, last).unwrap_err();
            assert_eq!(error.code, Code::Conflict, "{sequence} after {last}");
        }

        // Equal is refused as well as lower: re-serving the same document is
        // how a key removed in the next one stays.
        assert!(moves_forward(1, 1).is_err());
    }

    /// A retired key cannot be brought back by a document, however that
    /// document was signed — including by the leaked key itself.
    #[test]
    fn a_retired_key_is_skipped_when_a_document_names_it() {
        let current = Keys {
            trusted: vec![PublicKey(TEST_PUB.into())],
            retired: vec![PublicKey(OTHER_PUB.into())],
        };
        assert!(current.is_retired(&PublicKey(OTHER_PUB.into())));
        assert!(!current.is_retired(&PublicKey(TEST_PUB.into())));
    }

    #[test]
    fn a_key_id_is_short_and_printable() {
        let id = PublicKey(TEST_PUB.into()).id();
        assert_eq!(id.len(), 16);
        assert!(TEST_PUB.starts_with(&id));
    }
}
