//! The wildcard certificate, and whether anything trusts it.
//!
//! HTTPS already worked before this module existed, and was entirely invisible.
//! mkcert issues a wildcard certificate covering the
//! dashboard and every project domain, and `traefik.sh` installs it as the
//! default certificate — but nothing in the app could say whether mkcert was
//! present, whether the CA was trusted, whether the certificate had expired, or
//! which domains it actually covered. A user whose brand-new project opened to a
//! browser warning had no way to find out why, and the answer was almost always
//! the same: the certificate was issued before that project existed.
//!
//! Two things the Bash helper does not do, and this does:
//!
//! * **Trust on Linux and Windows.** `trust_ca_in_keychain` opens with
//!   `[[ "$OSTYPE" != "darwin"* ]] && return 0`, so on every other platform the
//!   CA is generated, referenced by Traefik, and trusted by nobody.
//! * **Honouring `DEFAULT_TLD_SUFFIX`.** The helper hardcodes `stackvo.loc` in
//!   `get_project_domains` while `.env` offers the suffix as a setting. Change
//!   it and every service domain silently falls outside the certificate.
//!
//! The SAN list and the expiry are read out of the certificate file rather than
//! inferred from the project list, because the file is what the browser
//! validates against. A list of what *should* be covered agrees with the
//! certificate right up until the moment it matters.

use crate::error::{Code, Error, Result};
use crate::hosts::is_valid_domain;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Used when `.env` has no `DEFAULT_TLD_SUFFIX`. Matches the value the Bash
/// helper hardcodes, so a checkout without the key behaves as it always has.
pub const FALLBACK_SUFFIX: &str = "stackvo.loc";

pub fn cert_dir(root: &Path) -> PathBuf {
    root.join("generated").join("certs")
}

pub fn cert_path(root: &Path) -> PathBuf {
    cert_dir(root).join("stackvo-wildcard.crt")
}

pub fn key_path(root: &Path) -> PathBuf {
    cert_dir(root).join("stackvo-wildcard.key")
}

/// The certificate authority — the real file, not a copy of it.
///
/// This used to point at `generated/certs/stackvo-ca.crt`, a byte-identical
/// duplicate that `apply` wrote next to the certificate. That made sense while
/// the CA itself lived in mkcert's own platform directory, somewhere nobody
/// would think to look; it stopped making sense the moment the CA moved into
/// this app's own directory, and it read as two certificates in two places to
/// the person looking at them.
///
/// `root` is unused now and kept so every path in this module is asked for the
/// same way.
pub fn ca_path(_root: &Path) -> PathBuf {
    ca_file()
}

/// The same file, for callers that have no workspace root to hand.
///
/// Split out after the trust command was built from `ca_root()` — the
/// *directory* — and handed `security add-trusted-cert` a folder to read a
/// certificate from. One name for the directory and one for the file, so the
/// two cannot be confused by looking similar at a call site.
pub fn ca_file() -> PathBuf {
    ca_root().join("rootCA.pem")
}

// ------------------------------------------------------------- pure logic
//
// Everything below this line is a plain function over strings and bytes, with
// no `cfg` gates and no process spawning, so its tests run on every platform.
// That is the same reasoning `paths.rs` gives: behaviour verified only on the
// platform it ships to is behaviour nobody verifies.

/// Does one subjectAltName entry cover this hostname?
///
/// RFC 6125: a wildcard is only ever the leftmost label and matches exactly one
/// label. The distinction is the whole reason `missing` is worth computing —
/// `*.stackvo.loc` covers `adminer.stackvo.loc`, and covers neither the bare
/// `stackvo.loc` above it nor `a.b.stackvo.loc` below it. Treating the wildcard
/// as "anything ending in stackvo.loc" would report a working certificate for a
/// domain the browser is about to reject.
pub fn san_covers(san: &str, domain: &str) -> bool {
    let san = san.trim().to_ascii_lowercase();
    let domain = domain.trim().to_ascii_lowercase();

    let Some(suffix) = san.strip_prefix("*.") else {
        return san == domain;
    };

    // One label, and it must be a real one — `*.loc` must not match `.loc`.
    match domain.strip_suffix(&suffix) {
        Some(head) => {
            let head = head.strip_suffix('.').unwrap_or("");
            !head.is_empty() && !head.contains('.')
        }
        None => false,
    }
}

/// Is this hostname covered by any entry in the certificate?
pub fn covered_by(sans: &[String], domain: &str) -> bool {
    sans.iter().any(|san| san_covers(san, domain))
}

/// Everything the certificate has to carry: the suffix itself, one wildcard
/// beneath it, and every project domain.
///
/// The wildcard covers all service domains (`adminer.stackvo.loc` and friends)
/// without enumerating them, which is why enabling a service does not make the
/// certificate stale. Project domains are listed individually because they are
/// conventionally `<name>.loc` — outside the suffix entirely.
///
/// Invalid domains are dropped and returned separately rather than passed on.
/// `hosts.rs` refuses the whole request when one domain is malformed, and is
/// right to: it writes a single file where a bad entry corrupts the rest. Here
/// one unparseable manifest would otherwise cost every *other* project its
/// certificate, so the trade goes the other way — but the ones dropped are
/// named in `rejected` and surfaced in the UI, because dropping them silently is
/// the failure this module exists to remove.
pub fn required_domains(suffix: &str, project_domains: &[String]) -> (Vec<String>, Vec<String>) {
    let mut required: Vec<String> = Vec::new();
    let mut rejected: Vec<String> = Vec::new();

    let suffix = suffix.trim().to_ascii_lowercase();
    if is_valid_domain(&suffix) {
        required.push(suffix.clone());
        required.push(format!("*.{suffix}"));
    } else {
        rejected.push(suffix);
    }

    for domain in project_domains {
        let domain = domain.trim().to_ascii_lowercase();
        if domain.is_empty() {
            continue;
        }
        // `is_valid_wildcard_or_domain`, not `is_valid_domain`: a project may
        // declare `*.shop.loc` as an alias, and mkcert issues that as a SAN —
        // the same form this function already adds for the suffix two blocks
        // above. Validating it with the stricter check would have dropped the
        // one hostname a multi-tenant project exists to serve.
        if crate::hosts::is_valid_wildcard_or_domain(&domain) {
            required.push(domain);
        } else if !rejected.contains(&domain) {
            // A domain beginning with `-` would reach mkcert's argument parser
            // as a flag, not a hostname. Command spawns args separately so
            // there is no shell to inject into, but the *program* still reads
            // its own argv.
            rejected.push(domain);
        }
    }

    required.sort();
    required.dedup();
    (required, rejected)
}

/// What reissuing would change: domains that would gain coverage, and domains
/// the certificate still carries that nothing asks for any more.
pub fn diff(covered: &[String], required: &[String]) -> (Vec<String>, Vec<String>) {
    let add: Vec<String> = required
        .iter()
        .filter(|d| !covered_by(covered, d))
        .cloned()
        .collect();

    // A stale SAN is not dangerous, but it is a domain the user deleted still
    // being vouched for, and it disappears on the next reissue — so say so
    // rather than let it look like the certificate shrank on its own.
    let remove: Vec<String> = covered
        .iter()
        .filter(|san| {
            !required
                .iter()
                .any(|r| r.eq_ignore_ascii_case(san) || san_covers(san, r))
        })
        .cloned()
        .collect();

    (add, remove)
}

/// What a certificate file says about itself.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertFacts {
    pub sans: Vec<String>,
    /// Unix seconds. Emitted as a number rather than a formatted date because
    /// the UI is bilingual and already formats timestamps in the user's locale;
    /// an English ASN.1 rendering baked in here could not be translated.
    pub not_after: Option<i64>,
    pub days_remaining: Option<i64>,
    pub expired: bool,
}

/// Read the SAN list and validity out of a PEM certificate.
pub fn parse_pem(bytes: &[u8]) -> Result<CertFacts> {
    use x509_parser::extensions::{GeneralName, ParsedExtension};

    let (_, pem) = x509_parser::pem::parse_x509_pem(bytes).map_err(|e| {
        Error::new(
            Code::InvalidInput,
            format!("the certificate is not readable PEM: {e}"),
        )
    })?;

    let cert = pem.parse_x509().map_err(|e| {
        Error::new(
            Code::InvalidInput,
            format!("the certificate could not be parsed: {e}"),
        )
    })?;

    let mut sans: Vec<String> = Vec::new();
    for ext in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                if let GeneralName::DNSName(dns) = name {
                    sans.push(dns.to_ascii_lowercase());
                }
            }
        }
    }
    sans.sort();
    sans.dedup();

    let validity = cert.validity();
    let not_after = validity.not_after.timestamp();
    // `time_to_expiration` is None once the certificate is past not_after,
    // which is exactly the state worth reporting rather than rounding to zero.
    let remaining = validity.time_to_expiration();

    Ok(CertFacts {
        sans,
        not_after: Some(not_after),
        days_remaining: remaining.map(|d| d.whole_days()),
        expired: remaining.is_none(),
    })
}

/// The CA's subject common name — mkcert writes `mkcert <user>@<host> (<Name>)`.
///
/// Used to look the CA up in a trust-store listing. Matching on the common name
/// rather than a fingerprint is deliberate: the listings the platforms print
/// (`security find-certificate`, `certutil -store`) render fingerprints in
/// different formats and some not at all, whereas every one of them prints the
/// subject.
pub fn ca_common_name(pem: &[u8]) -> Option<String> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(pem).ok()?;
    let cert = pem.parse_x509().ok()?;
    let name = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(str::to_string);
    name
}

/// Is our CA in the listing the platform just printed?
///
/// Kept separate from the command that produces the listing so the matching is
/// testable without a trust store to install into.
pub fn listing_contains(listing: &str, common_name: &str) -> bool {
    if common_name.is_empty() {
        return false;
    }
    let needle = common_name.to_ascii_lowercase();
    listing.to_ascii_lowercase().contains(&needle)
}

// ------------------------------------------------------------------- I/O

/// Where mkcert is, if it is anywhere.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mkcert {
    pub available: bool,
    pub version: Option<String>,
    /// `mkcert -CAROOT`. The CA lives here, not in the workspace — which is why
    /// reinstalling StackVo does not invalidate certificates already trusted.
    pub ca_root: Option<String>,
}

/// First line of stdout, or None when the program cannot be run at all.
/// Where mkcert keeps the certificate authority that signs everything here.
///
/// `<app root>/ca`, by handing mkcert a `CAROOT` rather than letting it use its
/// platform default — `~/Library/Application Support/mkcert` on macOS.
///
/// Two reasons, and the second one is a real failure that reached a user.
///
/// The app's own directory is the app's own directory. Everything else it
/// produces is under there; a certificate authority it created, for domains it
/// invented, is not the exception. Deleting `~/.stackvo` should leave nothing
/// behind, and until this it left a CA in a second place nobody had been told
/// about.
///
/// And a shared default is a directory this app does not control. The one it
/// was using had been created by `sudo mkcert -install` at some earlier point,
/// so it was owned by root with the key at mode 0400 — unreadable as the user,
/// permanently, with an error that pointed at mkcert rather than at ownership.
/// A directory this app creates is owned by whoever runs the app, which is the
/// only person who will ever need to read it.
pub fn ca_root() -> std::path::PathBuf {
    crate::workspace::app_root().join("ca")
}

/// Run a helper, with mkcert pointed at our own CA.
///
/// The environment is set for mkcert only. `security` and its equivalents read
/// the system trust store and have no business being told about `CAROOT`.
fn helper(program: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);

    // Not the app's own directory. A helper that inherits a cwd it cannot read
    // starts by complaining about it, and that noise reached a user on top of
    // the real error:
    //
    //   shell-init: error retrieving current directory: getcwd: cannot access
    //   parent directories: Operation not permitted
    //
    // Callers that need a working directory pass one, and it wins.
    command.current_dir("/");
    if program == "mkcert" {
        let root = ca_root();
        // mkcert creates it when missing, but then it is created by mkcert's
        // umask rather than ours — and ownership is the whole point here.
        let _ = std::fs::create_dir_all(&root);
        command.env("CAROOT", root);

        // And no Java. mkcert manages a JVM truststore whenever `JAVA_HOME` is
        // set, through `keytool`, and it aborts the *whole run* when that
        // fails — including a plain issue that was not asking for any of it.
        // Measured on a machine with Homebrew's openjdk@17, where the store is
        // not at the path mkcert expects:
        //
        //   with JAVA_HOME:    keytool error: Keystore file does not exist:
        //   without JAVA_HOME: It will expire on 1 November 2028
        //
        // Same command, same CA, same directory. This certificate is for a
        // browser and for Traefik; a JVM has never been in the picture, and
        // being unable to issue one because of a keystore nobody mentioned is
        // not a trade worth keeping. Somebody who does want Java to trust the
        // CA can run `mkcert -install` themselves.
        command.env_remove("JAVA_HOME");

        // And no stdin. `mkcert -install` shells out to `sudo`, which reads a
        // password from the terminal; a windowed app has none, so the prompt
        // went nowhere and the process waited for ever — the first-run screen
        // sat on "Issuing the certificate" until it was killed. With stdin
        // closed, sudo fails immediately and the app can say so. The trust
        // store is written by `install_ca` instead, through the same
        // authentication panel the hosts file uses.
        command.stdin(std::process::Stdio::null());
    }
    command
}

async fn probe(program: &str, args: &[&str]) -> Option<String> {
    let output = helper(program).args(args).output().await.ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

pub async fn mkcert() -> Mkcert {
    // mkcert takes single-dash flags; `--version` is not the same thing.
    let version = probe("mkcert", &["-version"]).await;
    Mkcert {
        available: version.is_some(),
        ca_root: if version.is_some() {
            probe("mkcert", &["-CAROOT"]).await
        } else {
            None
        },
        version,
    }
}

/// Whatever the platform will tell us about its trust store, as text.
///
/// Best-effort by design: returning `None` means "this machine gave no cheap
/// answer", which the UI must show differently from "not trusted". Claiming an
/// untrusted CA is trusted sends the user hunting through browser settings;
/// claiming a trusted one is not sends them re-running an install that already
/// worked. Neither is worth guessing to avoid one nullable field.
/// Does this machine accept the certificate we serve?
///
/// `security verify-cert`, which answers exactly that, after three attempts at
/// deducing it from text failed in three different directions:
///
///   1. `find-certificate` — asked whether the CA is in *a keychain*. It is
///      there whenever `add-trusted-cert` got that far, trusted or not, so this
///      reported success on a machine where Chrome refused every page.
///   2. `dump-trust-settings`, matching the common name — the trusted entry was
///      a *different* mkcert CA with a similar name.
///   3. `dump-trust-settings`, requiring `Number of trust settings > 0` — and
///      an empty settings list is macOS for "trust this for everything". The
///      certificate was trusted, the check said no, and the first-run screen
///      waited ninety seconds for something that had already happened.
///
/// Each of those was a guess about how to read an answer. This asks the
/// question: exit zero means a client will accept the chain. Against the
/// certificate rather than the CA, because that is what a browser evaluates —
/// a trusted CA with a leaf it did not sign is still a warning.
///
/// The `basic` policy, not `ssl`: the SSL policy also demands certificate
/// transparency, which a locally-installed root neither has nor needs, and
/// which browsers waive for exactly this case.
#[cfg(target_os = "macos")]
async fn system_accepts(cert: &Path) -> Option<bool> {
    if !cert.is_file() {
        return Some(false);
    }
    let output = helper("security")
        .args(["verify-cert", "-p", "basic", "-c"])
        .arg(cert)
        .output()
        .await
        .ok()?;
    Some(output.status.success())
}

#[cfg(target_os = "linux")]
async fn trust_listing() -> Option<String> {
    let mut found = String::new();
    let mut asked_anything = false;

    // Where `mkcert -install` puts the anchor, per distribution family.
    for dir in [
        "/usr/local/share/ca-certificates",
        "/etc/pki/ca-trust/source/anchors",
        "/etc/ca-certificates/trust-source/anchors",
    ] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            asked_anything = true;
            for entry in entries.flatten() {
                found.push_str(&entry.file_name().to_string_lossy());
                found.push('\n');
            }
        }
    }

    // The system store is not where Firefox and Chrome look. mkcert installs
    // into NSS as well, and a CA present in one and not the other is the exact
    // state that produces "it works in curl but not in the browser".
    if let Some(home) = dirs::home_dir() {
        let db = format!("sql:{}", home.join(".pki/nssdb").display());
        if let Ok(out) = tokio::process::Command::new("certutil")
            .args(["-L", "-d", &db])
            .output()
            .await
        {
            asked_anything = true;
            found.push_str(&String::from_utf8_lossy(&out.stdout));
        }
    }

    // The anchor filename is `mkcert_development_CA_<serial>`, which carries no
    // common name — so the filename listing is matched on that stem instead.
    asked_anything.then_some(found)
}

#[cfg(target_os = "windows")]
async fn trust_listing() -> Option<String> {
    let output = tokio::process::Command::new("certutil")
        .args(["-store", "-user", "Root"])
        .output()
        .await
        .ok()?;

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Will a client accept what we serve? `None` when the platform would not say.
///
/// Named for the CA because that is what a person changes, but asked of the
/// certificate, which is what a browser judges.
#[cfg(target_os = "macos")]
pub async fn ca_trusted_for(cert: &Path) -> Option<bool> {
    system_accepts(cert).await
}

/// Is our CA trusted? `None` when the platform would not say.
#[cfg(not(target_os = "macos"))]
pub async fn ca_trusted(ca_pem: Option<&[u8]>) -> Option<bool> {
    let listing = trust_listing().await?;

    // On Linux the system store is a directory of files named after the CA's
    // serial, not its subject, so the stem is the only thing to match on.
    if listing.contains("mkcert_development_CA") {
        return Some(true);
    }

    let name = ca_pem.and_then(ca_common_name)?;
    Some(listing_contains(&listing, &name))
}

/// The domain suffix every service sits under.
pub fn suffix(root: &Path) -> String {
    crate::config::Env::load(root)
        .ok()
        .and_then(|env| env.get("DEFAULT_TLD_SUFFIX").map(str::to_string))
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| FALLBACK_SUFFIX.to_string())
}

/// Every project's domain, read straight off disk.
///
/// Deliberately not `list_projects`: that one talks to Docker, and the whole
/// point of reporting on certificates is to still work when the engine is down.
/// A manifest that will not parse is skipped rather than fatal — it is already
/// reported as invalid everywhere else in the app.
pub fn project_domains(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Some(projects) = crate::workspace::projects_root(root) else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&projects) else {
        return out;
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let manifest_path = dir.join("stackvo.json");
        if !manifest_path.is_file() {
            continue;
        }
        if let Ok(manifest) = crate::manifest::read(&manifest_path, name) {
            if let Some(domain) = manifest.domain {
                out.push(domain);
            }
            // Extra hostnames are certificate subjects on exactly the same
            // terms as the main one — a name the browser reaches and the
            // certificate does not carry is a warning interstitial, and a
            // multi-tenant project would meet one on every tenant. Wildcards
            // come through as they are written: `required_domains` keeps them
            // and mkcert issues `*.shop.loc` as a SAN.
            out.extend(manifest.aliases);
        }
    }

    out.sort();
    out.dedup();
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertStatus {
    pub ssl_enabled: bool,
    pub mkcert_available: bool,
    pub mkcert_version: Option<String>,
    pub ca_root: Option<String>,
    pub ca_path: Option<String>,
    pub ca_trusted: Option<bool>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub not_after: Option<i64>,
    pub days_remaining: Option<i64>,
    pub expired: bool,
    pub covered: Vec<String>,
    pub required: Vec<String>,
    pub missing: Vec<String>,
    pub rejected: Vec<String>,
    /// True when reissuing would change anything, including when the file is
    /// simply not there yet.
    pub stale: bool,
    /// Set when the certificate exists but could not be read, so "covers
    /// nothing" is never mistaken for "covers nothing yet".
    pub error: Option<String>,
}

pub async fn status(root: &Path) -> CertStatus {
    let env_ssl = crate::config::Env::load(root)
        .map(|env| env.bool("SSL_ENABLE"))
        .unwrap_or(false);

    let mkcert = mkcert().await;
    let cert_file = cert_path(root);
    let key_file = key_path(root);
    let ca_file = ca_path(root);

    let (required, rejected) = required_domains(&suffix(root), &project_domains(root));

    let (facts, error) = match std::fs::read(&cert_file) {
        Ok(pem) => match parse_pem(&pem) {
            Ok(facts) => (Some(facts), None),
            Err(e) => (None, Some(e.message)),
        },
        Err(_) => (None, None),
    };

    let covered = facts.as_ref().map(|f| f.sans.clone()).unwrap_or_default();
    let (add, _) = diff(&covered, &required);

    let ca_pem = std::fs::read(&ca_file).ok();
    // macOS is asked about the certificate itself; everywhere else the trust
    // store is a list of files and the CA's name is all there is to match on.
    #[cfg(target_os = "macos")]
    let ca_trusted = {
        let _ = &ca_pem;
        ca_trusted_for(&cert_file).await
    };
    #[cfg(not(target_os = "macos"))]
    let ca_trusted = ca_trusted(ca_pem.as_deref()).await;

    CertStatus {
        ssl_enabled: env_ssl,
        mkcert_available: mkcert.available,
        mkcert_version: mkcert.version,
        ca_root: mkcert.ca_root,
        ca_path: ca_file.exists().then(|| ca_file.display().to_string()),
        ca_trusted,
        cert_path: cert_file.exists().then(|| cert_file.display().to_string()),
        key_path: key_file.exists().then(|| key_file.display().to_string()),
        not_after: facts.as_ref().and_then(|f| f.not_after),
        days_remaining: facts.as_ref().and_then(|f| f.days_remaining),
        expired: facts.as_ref().is_some_and(|f| f.expired),
        // A certificate the user cannot regenerate is still worth describing,
        // so staleness covers the absent case too.
        stale: facts.is_none() || !add.is_empty() || facts.as_ref().is_some_and(|f| f.expired),
        missing: add,
        covered,
        required,
        rejected,
        error,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertPlan {
    pub add: Vec<String>,
    pub remove: Vec<String>,
    /// The exact argument list mkcert will be given — the same value `apply`
    /// uses, so what the user approves is what lands.
    pub domains: Vec<String>,
    pub covered: Vec<String>,
    pub rejected: Vec<String>,
    pub changed: bool,
    pub cert_path: String,
    pub install_ca: bool,
    /// Why the certificate authority was not added to the system trust store,
    /// when it was meant to be.
    ///
    /// Not an error from `apply`. Trusting the CA and issuing the certificate
    /// are two jobs, and the first failing must not cost the second: a stack
    /// with an untrusted certificate serves every domain and shows a browser
    /// warning, while a stack with no certificate at all serves nothing —
    /// Traefik builds no TLS store and every name drops the connection. The
    /// first is a state worth leaving somebody in; the second is not.
    #[serde(default)]
    pub trust_failed: Option<String>,
    /// Whether the running proxy was made to re-read the certificate.
    ///
    /// Only `apply` sets this; `plan` leaves it false, because planning changes
    /// nothing. False after an apply means the certificate is on disk but
    /// Traefik is still serving the previous one — see `reload_proxy`.
    #[serde(default)]
    pub reloaded: bool,
}

/// What reissuing would do, without running anything.
pub async fn plan(root: &Path, install_ca: bool) -> Result<CertPlan> {
    let status = status(root).await;
    let (add, remove) = diff(&status.covered, &status.required);

    Ok(CertPlan {
        // `changed` is not just the SAN diff: an expired or absent certificate
        // needs reissuing even when it covers exactly the right names.
        changed: !add.is_empty()
            || !remove.is_empty()
            || status.expired
            || status.cert_path.is_none(),
        add,
        remove,
        domains: status.required.clone(),
        covered: status.covered,
        rejected: status.rejected,
        cert_path: cert_path(root).display().to_string(),
        install_ca: install_ca && status.ca_trusted != Some(true),
        trust_failed: None,
        // Planning issues nothing, so there is nothing for the proxy to reread.
        reloaded: false,
    })
}

/// Reissue the certificate, and install the CA when it is not trusted yet.
///
/// Issuing writes only inside the workspace and never elevates. Installing the
/// CA delegates to `mkcert -install`, which knows four trust stores this app
/// has no business reimplementing — and which, on Linux, shells out to sudo.
/// A GUI process has no terminal for that, so the failure is reported with the
/// command to run rather than swallowed: a CA that silently failed to install
/// is indistinguishable from one that worked, right up until the browser
/// disagrees.
pub async fn apply(root: &Path, install_ca: bool) -> Result<CertPlan> {
    let plan = plan(root, install_ca).await?;

    let mkcert = mkcert().await;
    if !mkcert.available {
        return Err(Error::new(
            Code::Unsupported,
            "mkcert is not installed, so certificates cannot be issued.",
        )
        .with_hint(crate::hints::INSTALL_MKCERT));
    }

    if plan.domains.is_empty() {
        return Err(Error::new(
            Code::InvalidInput,
            "There are no valid domains to issue a certificate for.",
        )
        .with_hint(crate::hints::CHECK_TLD_AND_DOMAINS));
    }

    let dir = cert_dir(root);
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error::io(format!("creating {}", dir.display()), e))?;

    let mut args = vec![
        "-cert-file".to_string(),
        cert_path(root).display().to_string(),
        "-key-file".to_string(),
        key_path(root).display().to_string(),
    ];
    args.extend(plan.domains.iter().cloned());

    run("mkcert", &args, Some(&dir)).await?;

    // No copy of the CA is written. There was one — `stackvo-ca.crt`, beside
    // the certificate — from when the CA lived in mkcert's own directory and
    // needed surfacing. It lives here now, so the copy was the same file twice
    // under two names, which is exactly how it was reported.

    // A new file on disk is not a new certificate in the browser.
    let mut plan = plan;
    plan.reloaded = reload_proxy(root);

    // Last, and allowed to fail. The certificate exists by now, so a refused or
    // dismissed authentication prompt costs a browser warning rather than the
    // whole stack.
    if plan.install_ca {
        if let Err(e) = trust_ca(&mkcert).await {
            tracing::warn!(error = %e.message, "the CA was not added to the trust store");
            plan.trust_failed = Some(e.message);
        }
    }

    Ok(plan)
}

/// Make this machine trust the certificate authority.
///
/// On macOS: it cannot be done from here, and saying so is the whole function.
///
/// Three attempts, each measured. `mkcert -install` shells out to `sudo`, which
/// reads a password from a terminal a windowed app does not have — it waited
/// for ever. The same write as root through `osascript` came back
/// `SecTrustSettingsSetTrustSettings: the authorization was denied since no
/// user interaction was possible`. And `security add-trusted-cert` against the
/// user domain, which needs no root at all, **exits 0 and changes nothing**:
/// the trust dump was byte-identical either side of it.
///
/// Modifying trust settings needs an authorization the Security framework will
/// only grant interactively, and a background child process of a windowed app
/// is not a place it will ask. So this reports what is true, and the UI offers
/// [`crate::commands::cert_trust_in_terminal`] — `mkcert -install` in the
/// user's own terminal, where `sudo` can ask and be answered.
#[cfg(target_os = "macos")]
async fn trust_ca(_mkcert: &Mkcert) -> Result<()> {
    Err(Error::new(
        Code::Unsupported,
        "macOS will not let a windowed app change the certificate trust settings.",
    )
    .with_hint(crate::hints::CERTIFICATE_ISSUED_BUT_UNTRUSTED))
}

/// Elsewhere, hand it back to mkcert.
///
/// It still cannot prompt — stdin is closed for every helper — so it fails
/// quickly and says what to run instead of waiting for a password nobody can
/// type.
#[cfg(not(target_os = "macos"))]
async fn trust_ca(_mkcert: &Mkcert) -> Result<()> {
    run("mkcert", &["-install".to_string()], None)
        .await
        .map_err(|e| e.with_hint(crate::hints::RUN_MKCERT_INSTALL))
}

/// The Traefik dynamic configuration directory — watched, unlike the certs.
fn dynamic_dir(root: &Path) -> PathBuf {
    root.join("generated").join("traefik").join("dynamic")
}

/// Extensions Traefik's file provider parses. Anything else in the directory —
/// including the `.tmp` sibling an atomic write stages — is ignored by it.
const DYNAMIC_EXTENSIONS: [&str; 4] = ["yml", "yaml", "toml", "json"];

/// Make the running proxy serve the certificate that was just issued.
///
/// Traefik's file provider watches `generated/traefik/dynamic` with
/// `watch: true`, and reloads when a file *there* changes. The certificates are
/// not there: `routes.yml` points at `/certs/stackvo-wildcard.crt`, and Traefik
/// reads a `certFile` only while parsing the dynamic configuration. Reissuing
/// therefore replaces the file and changes nothing a browser sees, until
/// something unrelated makes Traefik parse again.
///
/// That is not hypothetical. On the checkout this was written against, Traefik
/// had been up two days and was serving a certificate a full day older than the
/// one on disk — a reissue that reported success and did nothing.
///
/// Rewriting the watched files with their own bytes is the entire reload, and
/// is cheaper than restarting the proxy: no dropped connections, and every
/// project keeps answering. The contents are unchanged, so nothing under
/// `generated/` diverges from what the generator would write — which matters,
/// because that output is under a byte-for-byte contract.
///
/// **The write has to happen in place.** `atomic::write` stages a sibling and
/// renames it over the target, and that was measured against a running Traefik:
/// after the rename the proxy went on serving the previous certificate, while
/// the same file rewritten in place was picked up within seconds. A rename
/// swaps the inode rather than writing to the watched one, and the file
/// provider does not treat that as a change to a file it is watching.
///
/// Writing identical bytes in place is safe in a way an ordinary in-place write
/// is not, which is why the atomicity is not missed: the file is opened without
/// truncation and every byte written is equal to the byte it replaces, so its
/// length never changes and an interruption at any point leaves exactly the
/// contents that were already there. There is no torn state to land in.
///
/// Best-effort by design: the certificate is already issued by the time this
/// runs, and failing the whole operation over a reload would report a failure
/// for work that succeeded. The caller is told which happened instead.
pub fn reload_proxy(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dynamic_dir(root)) else {
        return false;
    };

    let mut poked = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_config = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| DYNAMIC_EXTENSIONS.contains(&e));
        if !is_config || !path.is_file() {
            continue;
        }
        // Read, then write the same bytes back over themselves. An mtime-only
        // touch is a chmod event, which fsnotify watchers routinely drop.
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if rewrite_in_place(&path, text.as_bytes()).is_ok() {
            poked = true;
        }
    }
    poked
}

/// Write `bytes` over the start of an existing file without truncating it.
///
/// Only ever called with the file's own contents — see `reload_proxy`.
fn rewrite_in_place(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.write_all(bytes)?;
    // Without this the write can sit in the page cache, and the watcher has
    // nothing to notice.
    file.sync_all()
}

/// Run a program and turn a non-zero exit into an Error carrying its stderr.
async fn run(program: &str, args: &[String], cwd: Option<&Path>) -> Result<()> {
    let mut command = helper(program);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    let output = command
        .output()
        .await
        .map_err(|e| Error::io(format!("running {program}"), e))?;

    if output.status.success() {
        return Ok(());
    }

    // mkcert writes its progress and its failures both to stderr, so the last
    // non-empty line is the closest thing to a reason it offers.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("no output");

    let error = Error::new(
        Code::PermissionDenied,
        format!("{program} failed: {reason}"),
    );
    match unreadable_ca_hint(reason) {
        Some(hint) => Err(error.with_hint(hint)),
        None => Err(error),
    }
}

/// The one mkcert failure whose obvious fix makes it worse.
///
/// `failed to read the CA key: … permission denied` means mkcert's own
/// certificate authority is owned by root, which happens when `mkcert -install`
/// was run once under `sudo`. Every later run as the ordinary user then fails
/// on a key it cannot read — and the standing advice, "run `mkcert -install` in
/// a terminal", does nothing about it. Run *that* under sudo, which is what
/// somebody reaches for next, and the CA is re-created owned by root again.
///
/// Measured on the machine that reported it: the CA directory was `root:staff`
/// with the key at mode 0400, dated weeks before the failure.
fn unreadable_ca_hint(reason: &str) -> Option<String> {
    let reason = reason.to_ascii_lowercase();
    if !(reason.contains("ca key") && reason.contains("permission denied")) {
        return None;
    }

    let dir = ca_root().display().to_string();

    Some(format!(
        "mkcert's certificate authority is owned by root, so it cannot be read as you. \
         It gets that way when `mkcert -install` is run once with sudo. Give it back with:\n\n\
         sudo chown -R \"$(id -un)\" \"{dir}\"\n\n\
         Then try again. Do not re-run mkcert with sudo — that recreates the same problem."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project may name a wildcard, and the certificate has to carry it.
    ///
    /// `required_domains` used to validate every project domain with
    /// `is_valid_domain`, which rejects an asterisk — so the one hostname a
    /// multi-tenant project exists to serve would have been dropped into
    /// `rejected` while the suffix's own `*.stackvo.loc`, added two blocks
    /// above, went through unchecked.
    #[test]
    fn a_projects_wildcard_alias_is_a_subject_like_any_other() {
        let (required, rejected) = required_domains(
            "stackvo.loc",
            &[
                "shop.loc".to_string(),
                "*.shop.loc".to_string(),
                "api.shop.loc".to_string(),
            ],
        );

        assert!(required.contains(&"*.shop.loc".to_string()), "{required:?}");
        assert!(required.contains(&"api.shop.loc".to_string()));
        assert!(rejected.is_empty(), "{rejected:?}");

        // And it covers what it claims to, on RFC 6125's terms — one label,
        // and not the name above it.
        assert!(covered_by(&required, "tenant1.shop.loc"));
        assert!(!covered_by(&required, "a.b.shop.loc"));
        assert!(
            covered_by(&required, "shop.loc"),
            "the bare name is listed too"
        );
    }

    /// An asterisk anywhere else is still refused, and named.
    #[test]
    fn an_asterisk_that_is_not_a_wildcard_is_still_rejected() {
        let (required, rejected) = required_domains("stackvo.loc", &["*.*.shop.loc".to_string()]);
        assert!(!required.iter().any(|d| d.contains("*.*")));
        assert_eq!(rejected, ["*.*.shop.loc"]);
    }

    /// Everything mkcert is handed, and the one thing it is not.
    ///
    /// Both were real failures on the machine that reported them. `CAROOT`
    /// because mkcert's platform default is a directory this app does not own —
    /// the one in use had been created by `sudo mkcert -install` and was
    /// root-owned, unreadable, permanently. `JAVA_HOME` because mkcert manages
    /// a JVM truststore whenever it is set and aborts the whole run when
    /// `keytool` fails, which it did: same command, same CA, issued fine with
    /// the variable removed and refused with it present.
    #[test]
    fn mkcert_is_pointed_at_our_ca_and_told_nothing_about_java() {
        let command = helper("mkcert");
        let envs: Vec<_> = command.as_std().get_envs().collect();

        let caroot = envs
            .iter()
            .find(|(k, _)| *k == std::ffi::OsStr::new("CAROOT"))
            .and_then(|(_, v)| *v)
            .expect("mkcert must be told where the CA lives");
        assert_eq!(std::path::Path::new(caroot), ca_root());
        assert!(
            ca_root().starts_with(crate::workspace::app_root()),
            "the CA belongs inside the directory the app owns"
        );

        // `get_envs` reports a removal as a key mapped to None.
        let java = envs
            .iter()
            .find(|(k, _)| *k == std::ffi::OsStr::new("JAVA_HOME"));
        assert_eq!(
            java.map(|(_, v)| *v),
            Some(None),
            "JAVA_HOME must be removed, not merely left alone"
        );

        // And only for mkcert: the trust-store helpers read the system's own
        // state and have no business being told about either.
        let other = helper("security");
        assert_eq!(other.as_std().get_envs().count(), 0);
    }

    /// A directory is not a certificate.
    ///
    /// The trust command was built from `ca_root()` and handed
    /// `security add-trusted-cert` a folder, which it cannot read a
    /// certificate out of. The two names looked alike at the call site and
    /// nothing else could tell them apart.
    #[test]
    fn the_ca_file_is_a_file_inside_the_ca_directory() {
        let dir = ca_root();
        let file = ca_file();

        assert!(file.starts_with(&dir), "{file:?} is not inside {dir:?}");
        assert_ne!(file, dir, "the file and the directory are the same path");
        assert_eq!(
            file.file_name().and_then(|n| n.to_str()),
            Some("rootCA.pem"),
            "mkcert writes its authority under this name"
        );
    }

    /// The check asks the system instead of reading its prose.
    ///
    /// Three text-parsing versions preceded it and each was wrong differently:
    /// presence in a keychain read as trust; a similarly named CA read as ours;
    /// and an empty trust-settings list read as "not trusted" when it is macOS
    /// for "trust this for everything" — that last one left the first-run
    /// screen waiting ninety seconds for something that had already happened.
    ///
    /// So there is nothing left to unit test here: the answer comes from
    /// `security verify-cert`, and asserting that it is called correctly means
    /// calling it. This checks the one thing that is still ours to get wrong —
    /// that the question is asked about the certificate a browser evaluates,
    /// not about the authority behind it.
    #[test]
    #[cfg(target_os = "macos")]
    fn trust_is_judged_on_the_certificate_that_gets_served() {
        let root = std::path::Path::new("/tmp/stackvo-trust-shape");
        assert_eq!(
            cert_path(root).file_name().and_then(|n| n.to_str()),
            Some("stackvo-wildcard.crt"),
            "the leaf is what a browser judges; a trusted CA with a leaf it did \
             not sign is still a warning"
        );
        assert_ne!(cert_path(root), ca_file());
    }

    /// The failure whose obvious fix makes it worse.
    #[test]
    fn a_root_owned_ca_is_named_as_such_instead_of_sent_back_to_mkcert() {
        // What mkcert actually printed on the machine that reported this.
        let real = "ERROR: failed to read the CA key: open /Users/me/Library/Application \
                    Support/mkcert/rootCA-key.pem: permission denied";
        let hint = unreadable_ca_hint(real).expect("this is the case the hint exists for");
        assert!(hint.contains("chown"), "the fix is ownership: {hint}");
        assert!(
            hint.contains("Do not re-run mkcert with sudo"),
            "sudo is what somebody reaches for next, and it recreates the problem: {hint}"
        );

        // Everything else keeps mkcert's own words and the standing advice.
        assert!(unreadable_ca_hint("ERROR: failed to find any PEM data").is_none());
        assert!(
            unreadable_ca_hint("permission denied").is_none(),
            "not every denial is the CA"
        );
        assert!(unreadable_ca_hint("no output").is_none());
    }

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    /// Reissuing writes a file Traefik is not watching. What makes the new
    /// certificate reach a browser is a change under `traefik/dynamic`, so the
    /// reload has to rewrite every config file there — and leave their contents
    /// exactly as they were, because `generated/` is under a byte-for-byte
    /// contract with the Bash generator.
    #[test]
    fn the_reload_rewrites_watched_config_without_changing_it() {
        let root = std::env::temp_dir().join("stackvo-certs-reload");
        let dir = root.join("generated").join("traefik").join("dynamic");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&dir).unwrap();

        let routes = dir.join("routes.yml");
        let body = "http:\n  routers: {}\n";
        std::fs::write(&routes, body).unwrap();
        // Traefik's file provider parses by extension; a README next to the
        // config is not configuration and must be left alone.
        let readme = dir.join("README.md");
        std::fs::write(&readme, "notes").unwrap();

        // The inode has to survive: a staged-and-renamed replacement is not
        // seen by Traefik's watcher, which is the whole reason this rewrites in
        // place. Measured against a running proxy — see `reload_proxy`.
        let inode_before = std::fs::metadata(&routes).unwrap();

        assert!(reload_proxy(&root));
        assert_eq!(std::fs::read_to_string(&routes).unwrap(), body);
        assert_eq!(std::fs::read_to_string(&readme).unwrap(), "notes");

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                std::fs::metadata(&routes).unwrap().ino(),
                inode_before.ino(),
                "the file was replaced rather than written to"
            );
        }
        #[cfg(not(unix))]
        let _ = inode_before;

        // Nothing staged, so nothing left over to be parsed as configuration.
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers.len(), 2, "left {leftovers:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A workspace that has never been generated has no such directory, and
    /// reporting a reload that did not happen is how `reloaded` becomes a lie.
    #[test]
    fn nothing_to_reload_is_reported_as_nothing() {
        let root = std::env::temp_dir().join("stackvo-certs-reload-absent");
        let _ = std::fs::remove_dir_all(&root);
        assert!(!reload_proxy(&root));
    }

    /// The bug this exists to prevent: treating `*.stackvo.loc` as "ends with
    /// stackvo.loc" reports `a.b.stackvo.loc` and the bare `stackvo.loc` as
    /// covered. A browser rejects both, so the app would be showing a green
    /// certificate over a page that will not load.
    #[test]
    fn a_wildcard_matches_exactly_one_label() {
        assert!(san_covers("*.stackvo.loc", "adminer.stackvo.loc"));
        assert!(san_covers("*.stackvo.loc", "a-b.stackvo.loc"));

        assert!(
            !san_covers("*.stackvo.loc", "stackvo.loc"),
            "the bare suffix needs its own SAN"
        );
        assert!(
            !san_covers("*.stackvo.loc", "a.b.stackvo.loc"),
            "a wildcard does not span a dot"
        );
        assert!(!san_covers("*.stackvo.loc", ".stackvo.loc"));
        assert!(!san_covers("*.stackvo.loc", "notstackvo.loc"));
        assert!(!san_covers("*.stackvo.loc", "shop.loc"));
    }

    #[test]
    fn exact_entries_match_case_insensitively() {
        assert!(san_covers("shop.loc", "SHOP.loc"));
        assert!(!san_covers("shop.loc", "api.shop.loc"));
    }

    #[test]
    fn the_suffix_contributes_both_itself_and_its_wildcard() {
        let (required, rejected) = required_domains("stackvo.loc", &s(&["shop.loc"]));
        assert_eq!(required, s(&["*.stackvo.loc", "shop.loc", "stackvo.loc"]));
        assert!(rejected.is_empty());
    }

    /// The Bash helper hardcodes `stackvo.loc`, so changing the setting left
    /// every service domain outside the certificate with no warning.
    #[test]
    fn a_custom_suffix_is_honoured() {
        let (required, _) = required_domains("dev.test", &[]);
        assert!(required.contains(&"dev.test".to_string()));
        assert!(required.contains(&"*.dev.test".to_string()));
        assert!(!required.iter().any(|d| d.contains("stackvo")));
    }

    /// A domain starting with `-` reaches mkcert's own argument parser as a
    /// flag. There is no shell involved, so this is not shell injection — the
    /// program simply reads its argv, and `--key-file=…` in a manifest would be
    /// read as one.
    #[test]
    fn a_domain_that_looks_like_a_flag_is_rejected_and_named() {
        let (required, rejected) =
            required_domains("stackvo.loc", &s(&["-key-file=/tmp/x", "good.loc"]));

        assert!(required.contains(&"good.loc".to_string()));
        assert!(!required.iter().any(|d| d.starts_with('-')));
        assert_eq!(
            rejected,
            s(&["-key-file=/tmp/x"]),
            "dropped, but not silently — this is what the UI shows"
        );
    }

    /// One bad manifest must not cost every other project its certificate. That
    /// is the opposite of the hosts file's rule, and the difference is that a
    /// hosts file is one document where a bad line corrupts its neighbours.
    #[test]
    fn one_bad_domain_does_not_take_the_others_down() {
        let (required, rejected) = required_domains("stackvo.loc", &s(&["shop..loc", "api.loc"]));
        assert!(required.contains(&"api.loc".to_string()));
        assert_eq!(rejected, s(&["shop..loc"]));
    }

    #[test]
    fn missing_domains_are_the_ones_no_san_covers() {
        let covered = s(&["stackvo.loc", "*.stackvo.loc", "old.loc"]);
        let (required, _) = required_domains("stackvo.loc", &s(&["shop.loc"]));

        let (add, remove) = diff(&covered, &required);
        assert_eq!(add, s(&["shop.loc"]), "the new project is not covered yet");
        assert_eq!(
            remove,
            s(&["old.loc"]),
            "a deleted project still vouched for"
        );
    }

    #[test]
    fn a_certificate_that_already_covers_everything_needs_no_change() {
        let (required, _) = required_domains("stackvo.loc", &s(&["shop.loc"]));
        let (add, remove) = diff(&required, &required);
        assert!(add.is_empty() && remove.is_empty());
    }

    /// A service domain is covered by the wildcard, so enabling a service must
    /// not report the certificate as stale.
    #[test]
    fn service_domains_do_not_make_the_certificate_stale() {
        let covered = s(&["stackvo.loc", "*.stackvo.loc"]);
        assert!(covered_by(&covered, "adminer.stackvo.loc"));
        assert!(covered_by(&covered, "phpmyadmin.stackvo.loc"));
    }

    #[test]
    fn a_ca_listing_is_matched_by_common_name() {
        let listing = "Subject: CN=mkcert dev@laptop (Dev), OU=dev@laptop";
        assert!(listing_contains(listing, "mkcert dev@laptop (Dev)"));
        assert!(!listing_contains(listing, "mkcert other@host (Other)"));
        assert!(
            !listing_contains(listing, ""),
            "an unreadable CA name must not match everything"
        );
    }

    #[test]
    fn garbage_is_not_mistaken_for_a_certificate() {
        assert!(parse_pem(b"not a certificate").is_err());
        assert!(ca_common_name(b"not a certificate").is_none());
    }
}
