//! The redirect URI to register with an identity provider (M-12).
//!
//! This was on the list as "an OAuth callback for `.loc`" and was left alone
//! because it was not defined enough to build. Reading what the providers
//! actually require settles it, and the answer is smaller and sharper than the
//! item sounded:
//!
//! **A redirect URI is a browser redirect, not a fetch.** The provider sends a
//! `302` to the visitor's browser; it never resolves the hostname or opens a
//! connection itself. So `https://shop.loc/auth/callback` works — the browser
//! is on this machine, the name is in this machine's hosts file, and the
//! certificate is issued by a CA this machine trusts. Nothing has to be public
//! for the *flow* to work.
//!
//! What varies is whether the provider will **accept the string** at
//! registration time. Some validate it against a public suffix list or insist
//! on `localhost`; those are the ones that need [`crate::tunnel`]'s public URL
//! instead. That distinction is the whole feature, and it is a fact about each
//! provider rather than something this app can do anything about — so it is
//! written down here, per provider, and the pane says which of the two
//! addresses to paste.
//!
//! ## What this deliberately does not do
//!
//! It does not implement OAuth. There is no client, no token exchange and no
//! store for a secret: the application being developed does all of that, and an
//! app that held somebody's client secret to be helpful would be a second place
//! for it to leak from. This produces two strings and the rules for choosing
//! between them.

use crate::error::{Code, Error, Result};
use serde::Serialize;

/// Whether a provider will accept a name that is not on the public internet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Accepts {
    /// Any https URL, private hostnames included — the local address works.
    Any,
    /// Public hostnames only, or `localhost` — the tunnel URL is the answer.
    PublicOnly,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub accepts: Accepts,
    /// Why, in one line. Every row here is a rule somebody hits at the console
    /// with no explanation attached, so the reason travels with the answer.
    pub note: &'static str,
}

/// The providers people actually register a development callback with.
///
/// Not a catalogue to be completed: each row is a rule that was read from that
/// provider's own documentation, and a row nobody checked would be worse than
/// an absent one — somebody would paste the wrong address on its authority.
pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "github",
        label: "GitHub",
        accepts: Accepts::Any,
        note: "Accepts any URL for an OAuth app, including a private hostname.",
    },
    Provider {
        id: "gitlab",
        label: "GitLab",
        accepts: Accepts::Any,
        note: "Accepts any URL; http is allowed only for localhost.",
    },
    Provider {
        id: "entra",
        label: "Microsoft Entra ID",
        accepts: Accepts::Any,
        note: "Accepts any https URL. http is allowed only for localhost.",
    },
    Provider {
        id: "auth0",
        label: "Auth0",
        accepts: Accepts::Any,
        note: "Accepts any URL in the allowed-callbacks list, wildcards included.",
    },
    Provider {
        id: "google",
        label: "Google",
        accepts: Accepts::PublicOnly,
        note: "Refuses hostnames that are not in the public suffix list; only localhost is exempt.",
    },
    Provider {
        id: "facebook",
        label: "Facebook",
        accepts: Accepts::PublicOnly,
        note: "Requires a public https URL and verifies the domain.",
    },
    Provider {
        id: "apple",
        label: "Sign in with Apple",
        accepts: Accepts::PublicOnly,
        note: "Requires a registered, publicly resolvable domain; localhost is not accepted.",
    },
];

/// Both addresses, and which providers take which.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Callbacks {
    /// The path, as it was normalised — echoed so the screen shows what is
    /// actually in the URLs rather than what was typed.
    pub path: String,
    /// `https://<domain><path>`, or `None` when the project has no domain.
    pub local: Option<String>,
    /// The same path on the running tunnel, when one is up.
    pub public: Option<String>,
    pub providers: &'static [Provider],
}

/// Normalise a callback path.
///
/// Leading slash added, duplicate slashes collapsed, and a query string or
/// fragment refused rather than silently kept: most providers compare the
/// registered URI to the one they are redirected to **exactly**, and a `?` in a
/// registration is a mismatch at the last step of the flow with an error
/// message that names neither side.
pub fn checked_path(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.contains(['?', '#']) {
        return Err(Error::new(
            Code::InvalidInput,
            "a redirect URI is compared exactly, so it cannot carry a query or a fragment",
        ));
    }
    if trimmed.contains(char::is_whitespace) {
        return Err(Error::new(
            Code::InvalidInput,
            "a redirect URI cannot contain a space",
        ));
    }

    let mut out = String::from("/");
    let mut last_slash = true;
    for ch in trimmed.trim_start_matches('/').chars() {
        if ch == '/' {
            if last_slash {
                continue;
            }
            last_slash = true;
        } else {
            last_slash = false;
        }
        out.push(ch);
    }
    // A trailing slash is a different URI to every provider that compares
    // exactly, so it is kept if it was typed and never added.
    Ok(out)
}

/// Join a base URL and an already-checked path.
pub fn join(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_normalised_rather_than_guessed_at() {
        assert_eq!(checked_path("auth/callback").unwrap(), "/auth/callback");
        assert_eq!(checked_path("/auth/callback").unwrap(), "/auth/callback");
        assert_eq!(checked_path("//auth//callback").unwrap(), "/auth/callback");
        assert_eq!(checked_path("  /login  ").unwrap(), "/login");
        // Kept, because it is a different URI to a provider that compares
        // exactly — and never added for the same reason.
        assert_eq!(checked_path("/callback/").unwrap(), "/callback/");
        assert_eq!(checked_path("").unwrap(), "/");
    }

    /// A query string in a registered redirect URI is a mismatch at the last
    /// step of the flow, reported by an error that names neither side.
    #[test]
    fn a_query_or_a_space_is_refused_rather_than_stripped() {
        assert!(checked_path("/callback?next=/home").is_err());
        assert!(checked_path("/callback#top").is_err());
        assert!(checked_path("/two words").is_err());
    }

    #[test]
    fn joining_never_doubles_the_slash() {
        assert_eq!(join("https://shop.loc", "/cb"), "https://shop.loc/cb");
        assert_eq!(join("https://shop.loc/", "/cb"), "https://shop.loc/cb");
    }

    /// Every row has to say why, because the rule is invisible at the console
    /// and somebody will paste on this table's authority.
    #[test]
    fn every_provider_says_what_it_accepts_and_why() {
        assert!(!PROVIDERS.is_empty());
        for provider in PROVIDERS {
            assert!(!provider.note.is_empty(), "{} has no reason", provider.id);
            assert!(provider.note.ends_with('.'), "{}", provider.id);
        }
        // Both answers are represented; a table where every row said the same
        // thing would be a table nobody needs to read.
        assert!(PROVIDERS.iter().any(|p| p.accepts == Accepts::Any));
        assert!(PROVIDERS.iter().any(|p| p.accepts == Accepts::PublicOnly));
    }
}
