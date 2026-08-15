//! Answering for this machine's development names, so `/etc/hosts` stops being
//! the only way a project resolves.
//!
//! E-1. Every new project needs a line in `/etc/hosts`, and writing that file
//! needs an administrator password. That is a prompt per project, and it is
//! also the reason E-2's wildcards do not work at all: `/etc/hosts` maps names,
//! one at a time, and `*.shop.loc` is not a name.
//!
//! ## Why not dnsmasq
//!
//! Every comparable tool ships dnsmasq — DDEV and Valet both do — and it would
//! have worked. It also means a second binary to package for three platforms, a
//! config file generated from this app's state, a process supervised by
//! something, and a failure mode where the machine's name resolution depends on
//! a container being up. For a responder whose entire job is "say 127.0.0.1 to
//! anything under one suffix", that is a great deal of moving parts around a
//! function that fits on a page.
//!
//! ## It is not a resolver, and that is the security property
//!
//! This answers for **one suffix** and refuses everything else. It never
//! forwards, has no upstream, and holds no cache. An open forwarder listening
//! on a machine is a thing that can be pointed at — for amplification, for
//! poisoning what the machine believes — and a development tool has no business
//! becoming the resolver for anything it did not create.
//!
//! Concretely: a query for `shop.loc` is answered, a query for `google.com` is
//! `REFUSED`, and there is no code path that opens a socket to anywhere. It
//! binds loopback only, so nothing off this machine can reach it either.
//!
//! ## UDP and TCP, because a resolver picks
//!
//! A stub resolver is allowed to ask over TCP whenever it likes — after a
//! truncated answer, when it is retrying, or because that is simply what it
//! does — and a name server that only listens on UDP answers those with a
//! connection refused. So both are bound, from the same [`reply`], and a
//! machine that asks either way gets the same answer.
//!
//! ## A high port, because port 53 needs root and does not need to
//!
//! Binding 53 means privilege at every start. The resolver files on macOS and
//! the dnsmasq drop-ins on Linux both take a port, so the responder can run as
//! the user like the rest of the app. **Windows is the exception** and has to
//! be: its Name Resolution Policy Table names a server and has nowhere to put a
//! port. Windows also has no privileged-port rule, so binding 53 there costs
//! nothing — see [`PORT`].
//!
//! ## Three platforms, three mechanisms, and a detection step before each
//!
//! * **macOS** — `/etc/resolver/<tld>` is exactly this feature, per suffix,
//!   supported by the system resolver since 10.4.
//! * **Linux** — no `/etc/resolver`, and *which* file to write is a question
//!   about the machine rather than about the platform. So it is asked:
//!   NetworkManager's dnsmasq, a standalone dnsmasq, or systemd-resolved, in
//!   that order, and nothing is written when none of them is the thing in front
//!   of `resolv.conf`. A guessed path is worse than no feature — it is a
//!   machine whose name resolution was rearranged by an app that was wrong
//!   about how it worked.
//! * **Windows** — the NRPT. It takes a namespace and a server, applies to that
//!   suffix and nothing else, and is the one mechanism on that platform that is
//!   not "point the whole adapter somewhere". This module used to say Windows
//!   had no per-suffix mechanism at all, which was a statement about what had
//!   been looked for rather than about Windows.
//!
//! ## Nothing is trusted to have worked
//!
//! Every one of those mechanisms is applied and then **measured through the
//! machine's own resolver** — not by reading back the file this app just wrote,
//! which only proves the write happened. A name under the suffix has to come
//! back as loopback, and a public name that resolved before the change has to
//! still resolve after it. If either fails the change is **undone**, because
//! the failure mode being guarded against is a laptop that cannot resolve
//! anything and a user who has no idea this app is why.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The port the responder listens on.
///
/// Above 1024 everywhere it can be, so no privilege is needed, and not one of
/// the ports something else is likely to want: 15353 rather than 5353, because
/// 5353 is mDNS and a development tool binding the port Bonjour uses is a
/// support question.
///
/// Windows gets 53 because its NRPT rule names a server and cannot name a port.
/// That is not the compromise it looks like — Windows has no privileged-port
/// rule, so 53 on loopback is an ordinary bind there, and the responder still
/// runs as the user.
#[cfg(windows)]
pub const PORT: u16 = 53;
#[cfg(not(windows))]
pub const PORT: u16 = 15353;

/// The largest UDP DNS message this reads. A query that does not fit in 512
/// bytes is one this responder has no business answering.
const MAX: usize = 512;

/// How long a probe waits for its own responder. Loopback: anything that has
/// not answered by now is not going to.
const PROBE_TIMEOUT: Duration = Duration::from_millis(400);

/// How long a lookup through the *machine's* resolver is given. Longer, because
/// this one crosses the system resolver, a cache and possibly a network.
const SYSTEM_TIMEOUT: Duration = Duration::from_secs(3);

/// The name public resolution is measured against, before and after a change.
///
/// `example.com` rather than a vendor's: it is reserved for exactly this by
/// RFC 2606, so it is nobody's outage and nobody's telemetry. It is only ever
/// compared with itself — an offline machine answers "no" both times, which is
/// not a regression and does not trigger a rollback.
const PUBLIC_PROBE: &str = "example.com";

// ---------------------------------------------------------------- the suffix

/// The last label of the suffix, checked rather than taken on trust.
///
/// This value comes from `DEFAULT_TLD_SUFFIX` in the workspace `.env`, which is
/// a file the user edits, and it ends up in a path this app writes **as root**
/// and in a command line. `..` in a suffix used to build
/// `/etc/resolver/../../somewhere`, which is a root-owned write to a path of
/// the user's choosing — reachable only by editing your own `.env`, so not much
/// of an escalation, and still not a thing this should be capable of.
///
/// So: one label, letters, digits and hyphens, no leading or trailing hyphen.
/// Everything else is refused before it reaches a path or a shell.
pub fn tld_of(suffix: &str) -> Option<String> {
    let cleaned = suffix.trim().trim_matches('.').to_ascii_lowercase();
    let tld = cleaned.rsplit('.').next()?.to_string();
    if tld.is_empty() || tld.len() > 63 {
        return None;
    }
    if !tld.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return None;
    }
    if tld.starts_with('-') || tld.ends_with('-') {
        return None;
    }
    Some(tld)
}

fn require_tld(suffix: &str) -> Result<String> {
    tld_of(suffix).ok_or_else(|| {
        Error::new(
            Code::InvalidInput,
            format!("{suffix:?} does not end in a name a resolver could be pointed at"),
        )
        .with_hint(crate::hints::TLD_IS_ONE_LABEL)
    })
}

// ------------------------------------------------------------- the mechanism

/// How this machine can be told to ask us for one suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mechanism {
    /// macOS: a file per suffix, read by the system resolver itself.
    Resolver,
    /// Linux, NetworkManager running its own dnsmasq.
    NetworkManager,
    /// Linux, a dnsmasq of the machine's own.
    Dnsmasq,
    /// Linux, systemd-resolved.
    SystemdResolved,
    /// Windows: a Name Resolution Policy Table rule.
    Nrpt,
    /// Nothing recognisable is in front of `resolv.conf`. The line is reported;
    /// where it goes is a question only this machine's owner can answer.
    Manual,
}

impl Mechanism {
    /// Is this something the app can apply itself?
    pub fn writable(self) -> bool {
        self != Mechanism::Manual
    }
}

/// What detection found, worked out once.
///
/// Cached for the life of the process on purpose: this reads files and asks
/// `systemctl`, `status` is called every time the pane is opened, and a machine
/// does not swap the thing in front of its resolver while an app is running. A
/// machine that does gets the right answer after a restart, which is the same
/// deal every other "what is installed" answer in this app makes.
pub fn mechanism() -> Mechanism {
    static FOUND: std::sync::OnceLock<Mechanism> = std::sync::OnceLock::new();
    *FOUND.get_or_init(detect)
}

#[cfg(target_os = "macos")]
fn detect() -> Mechanism {
    Mechanism::Resolver
}

#[cfg(windows)]
fn detect() -> Mechanism {
    Mechanism::Nrpt
}

/// Which of the three Linux answers this machine actually uses.
///
/// Order matters and it is the order of specificity, not of preference. A
/// machine running NetworkManager's dnsmasq usually *also* has systemd-resolved
/// installed and a `/etc/dnsmasq.d` from the package; writing to the one that
/// is not in the path resolution actually takes produces a file, no error, and
/// no working name — the worst of the three outcomes, because it looks like
/// success.
#[cfg(all(unix, not(target_os = "macos")))]
fn detect() -> Mechanism {
    if network_manager_runs_dnsmasq() {
        return Mechanism::NetworkManager;
    }
    if Path::new("/etc/dnsmasq.d").is_dir() && unit_is_active("dnsmasq") {
        return Mechanism::Dnsmasq;
    }
    if unit_is_active("systemd-resolved") || Path::new("/run/systemd/resolve").is_dir() {
        return Mechanism::SystemdResolved;
    }
    Mechanism::Manual
}

/// `dns=dnsmasq` in NetworkManager's config, wherever that machine keeps it.
///
/// The directory `/etc/NetworkManager/dnsmasq.d` exists on most installations
/// whether or not the plugin is on, so its presence proves nothing — the
/// setting is the fact worth reading.
#[cfg(all(unix, not(target_os = "macos")))]
fn network_manager_runs_dnsmasq() -> bool {
    if !Path::new("/etc/NetworkManager").is_dir() {
        return false;
    }

    let mut files = vec![PathBuf::from("/etc/NetworkManager/NetworkManager.conf")];
    if let Ok(entries) = std::fs::read_dir("/etc/NetworkManager/conf.d") {
        files.extend(entries.flatten().map(|e| e.path()));
    }

    files.iter().any(|path| {
        std::fs::read_to_string(path).is_ok_and(|text| {
            text.lines().any(|line| {
                let line = line.split('#').next().unwrap_or("").replace(' ', "");
                line == "dns=dnsmasq"
            })
        })
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn unit_is_active(unit: &str) -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "active")
        .unwrap_or(false)
}

// ------------------------------------------------------------------ the plan

/// The exact change this app would make, before it makes it.
#[derive(Debug, Clone)]
pub struct Plan {
    pub mechanism: Mechanism,
    /// The file written, where the mechanism is a file.
    pub file: Option<PathBuf>,
    /// Its contents — or, for the NRPT, the rule in the words that add it.
    pub text: String,
    /// What has to be told about the change afterwards, run in the same
    /// elevated step so one password covers the whole operation.
    pub reload: Vec<String>,
}

/// The file macOS wants, and the line dnsmasq wants, are the same two facts.
pub fn resolver_text() -> String {
    format!("nameserver 127.0.0.1\nport {PORT}\n")
}

/// The dnsmasq / NetworkManager line.
pub fn forward_line(suffix: &str) -> String {
    let tld = tld_of(suffix).unwrap_or_else(|| suffix.to_string());
    format!("server=/{tld}/127.0.0.1#{PORT}")
}

/// The systemd-resolved drop-in.
///
/// `Domains=~loc` is a *routing* domain: it says which names this server is
/// asked about, and it is the whole reason this is not "point resolved at a
/// responder that refuses everything". `DNS=` takes a port after a colon,
/// which is what keeps the responder unprivileged here too.
fn resolved_text(tld: &str) -> String {
    format!("[Resolve]\nDNS=127.0.0.1:{PORT}\nDomains=~{tld}\n")
}

/// The NRPT rule, in the words that create it.
fn nrpt_text(tld: &str) -> String {
    format!("Add-DnsClientNrptRule -Namespace '.{tld}' -NameServers '127.0.0.1'")
}

pub fn plan(suffix: &str) -> Result<Plan> {
    Ok(plan_for(mechanism(), &require_tld(suffix)?))
}

/// The plan for a mechanism, without asking what this machine has.
///
/// Split out from [`plan`] for one reason, and it is the reason the Linux and
/// Windows halves of this module exist at all: on a macOS laptop `mechanism()`
/// answers `Resolver` and always will, so four of the five plans are code that
/// ships without a test ever having looked at them. A pure function of
/// `(mechanism, tld)` can be — and is — checked for every one of them, and what
/// is checked is the part that is worth checking: the path, and whether the
/// text names this machine and this port in that file's own syntax.
pub fn plan_for(mechanism: Mechanism, tld: &str) -> Plan {
    let reload = |argv: &[&str]| argv.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let suffix = tld;

    match mechanism {
        Mechanism::Resolver => Plan {
            mechanism: Mechanism::Resolver,
            file: Some(PathBuf::from("/etc/resolver").join(tld)),
            text: resolver_text(),
            reload: Vec::new(),
        },
        Mechanism::NetworkManager => Plan {
            mechanism: Mechanism::NetworkManager,
            file: Some(PathBuf::from("/etc/NetworkManager/dnsmasq.d/stackvo.conf")),
            text: format!("{}\n", forward_line(suffix)),
            // Reload rather than restart: restarting NetworkManager on a laptop
            // takes its connections down with it.
            reload: reload(&["systemctl", "reload", "NetworkManager"]),
        },
        Mechanism::Dnsmasq => Plan {
            mechanism: Mechanism::Dnsmasq,
            file: Some(PathBuf::from("/etc/dnsmasq.d/stackvo.conf")),
            text: format!("{}\n", forward_line(suffix)),
            reload: reload(&["systemctl", "restart", "dnsmasq"]),
        },
        Mechanism::SystemdResolved => Plan {
            mechanism: Mechanism::SystemdResolved,
            file: Some(PathBuf::from("/etc/systemd/resolved.conf.d/stackvo.conf")),
            text: resolved_text(tld),
            reload: reload(&["systemctl", "restart", "systemd-resolved"]),
        },
        Mechanism::Nrpt => Plan {
            mechanism: Mechanism::Nrpt,
            file: None,
            text: nrpt_text(tld),
            reload: Vec::new(),
        },
        Mechanism::Manual => Plan {
            mechanism: Mechanism::Manual,
            file: None,
            text: format!("{}\n", forward_line(suffix)),
            reload: Vec::new(),
        },
    }
}

/// Where macOS looks for a per-suffix resolver.
///
/// Keyed by the **last label** of the suffix, which is what the system reads:
/// a workspace on `stackvo.loc` is served by `/etc/resolver/loc`, because macOS
/// matches a resolver file against the domain's tail. Naming the file
/// `stackvo.loc` would work too and would be narrower — but then two workspaces
/// with different prefixes under one TLD would each need their own file and
/// each would answer for the other's names anyway.
pub fn resolver_path(suffix: &str) -> Option<PathBuf> {
    if mechanism() != Mechanism::Resolver {
        return None;
    }
    Some(PathBuf::from("/etc/resolver").join(tld_of(suffix)?))
}

// ------------------------------------------------------------ what is in place

/// Is the machine pointed at us for this suffix?
///
/// Compared on the facts rather than byte for byte: a user who added a comment
/// or a `search` line has a working resolver, and rewriting their file to make
/// it match a literal would be this app taking a file it did not create.
pub fn configured(suffix: &str) -> bool {
    let Ok(plan) = plan(suffix) else {
        return false;
    };
    match plan.mechanism {
        Mechanism::Manual => false,
        Mechanism::Nrpt => nrpt_rule_exists(suffix),
        _ => plan.file.as_deref().is_some_and(file_points_at_us),
    }
}

/// Where a file that was already there is kept, if this app overwrites one.
///
/// A suffix macOS knows about — `/etc/resolver/test` — is a file dnsmasq,
/// Valet or a colleague's script may have written first, and overwriting it
/// takes that suffix away from whatever wrote it with no way back. So it is
/// copied aside in the same elevated step, and [`remove`] puts it back.
fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".pre-stackvo");
    PathBuf::from(name)
}

/// A file at the path this app writes that belongs to something else.
///
/// `None` when there is nothing there, or when what is there already names us.
/// The first non-empty, non-comment line is carried because the pane has to say
/// *what* it would be replacing — "a file exists" is not enough to consent to.
pub fn foreign_file(suffix: &str) -> Option<String> {
    let plan = plan(suffix).ok()?;
    let path = plan.file.as_deref()?;
    if !path.exists() || file_points_at_us(path) {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let summary = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("(empty)")
        .to_string();
    Some(format!("{}: {summary}", path.display()))
}

/// Files this app wrote for a suffix this workspace no longer uses.
///
/// Only macOS can have these: its mechanism is one file **per suffix**, so
/// changing `DEFAULT_TLD_SUFFIX` from `loc` to `test` leaves
/// `/etc/resolver/loc` behind — pointing at a responder that now answers for
/// `.test` and **refuses** `.loc`. That is worse than doing nothing: before,
/// `.loc` names went upstream and failed honestly; afterwards they are refused
/// by a server on this machine. The other mechanisms write one file whose
/// contents are replaced, so rewriting it is the whole of the fix.
pub fn stale_files(suffix: &str) -> Vec<PathBuf> {
    if mechanism() != Mechanism::Resolver {
        return Vec::new();
    }
    let Some(tld) = tld_of(suffix) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir("/etc/resolver") else {
        return Vec::new();
    };

    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.file_name().is_some_and(|name| name != tld.as_str()))
        .filter(|path| path.is_file() && file_points_at_us(path))
        .collect();
    out.sort();
    out
}

/// Does this file name us, whatever else it says?
///
/// The two facts are the address and the port, and they are looked for in the
/// syntax of whichever file it is — `nameserver`/`port` on two lines for macOS,
/// `127.0.0.1#port` inside one directive for dnsmasq, `127.0.0.1:port` for
/// resolved. A substring test rather than an equality test, for the same reason
/// the macOS one always was one: the file may hold a user's own lines.
fn file_points_at_us(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let port = PORT.to_string();

    let mut nameserver = false;
    let mut port_named = false;
    for line in text.lines() {
        let line = line.trim();
        if line == "nameserver 127.0.0.1" || line == "nameserver ::1" {
            nameserver = true;
        }
        if line == format!("port {port}") {
            port_named = true;
        }
        // dnsmasq's `server=/loc/127.0.0.1#15353` and resolved's
        // `DNS=127.0.0.1:15353` both carry both facts on one line.
        if line.contains(&format!("127.0.0.1#{port}"))
            || line.contains(&format!("127.0.0.1:{port}"))
        {
            nameserver = true;
            port_named = true;
        }
    }
    nameserver && port_named
}

/// Whether an NRPT rule for this suffix exists, asked of Windows itself.
///
/// Memoised for a few seconds because this spawns PowerShell — half a second
/// each time — and the pane asks after every action it takes. The cache is
/// dropped on every write from this module, so a rule this app just added or
/// removed is never reported from a stale answer.
#[cfg(windows)]
fn nrpt_rule_exists(suffix: &str) -> bool {
    let Some(tld) = tld_of(suffix) else {
        return false;
    };
    if let Some(answer) = nrpt_cache().read(&tld) {
        return answer;
    }

    let script =
        format!("@(Get-DnsClientNrptRule | Where-Object {{ $_.Namespace -eq '.{tld}' }}).Count");
    let found = powershell(&script)
        .map(|out| out.trim().parse::<u32>().unwrap_or(0) > 0)
        .unwrap_or(false);

    nrpt_cache().write(&tld, found);
    found
}

#[cfg(not(windows))]
fn nrpt_rule_exists(_suffix: &str) -> bool {
    false
}

#[cfg(windows)]
struct NrptCache {
    entries: std::sync::Mutex<std::collections::HashMap<String, (bool, Instant)>>,
}

#[cfg(windows)]
impl NrptCache {
    fn read(&self, tld: &str) -> Option<bool> {
        let entries = self.entries.lock().ok()?;
        let (answer, at) = entries.get(tld)?;
        (at.elapsed() < Duration::from_secs(5)).then_some(*answer)
    }
    fn write(&self, tld: &str, answer: bool) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(tld.to_string(), (answer, Instant::now()));
        }
    }
    fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

#[cfg(windows)]
fn nrpt_cache() -> &'static NrptCache {
    static CACHE: std::sync::OnceLock<NrptCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| NrptCache {
        entries: std::sync::Mutex::new(std::collections::HashMap::new()),
    })
}

/// Run a PowerShell script unprivileged and return its stdout.
#[cfg(windows)]
fn powershell(script: &str) -> Result<String> {
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| Error::io("running powershell", e))?;
    if !output.status.success() {
        return Err(Error::new(
            Code::IoError,
            format!(
                "powershell refused: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ----------------------------------------------------------------- the status

/// What is set up and what is not, for a screen that has to explain it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub mechanism: Mechanism,
    /// Whether this app can make the change itself, or only report it.
    pub writable: bool,
    /// The suffix this workspace's names end in.
    pub suffix: String,
    pub tld: String,
    pub port: u16,
    /// Whether the responder is answering right now, on UDP.
    pub listening: bool,
    /// Whether TCP is bound too. Separate because a machine can lose one of
    /// them — a port half-taken — and a screen that averages the two lies.
    pub tcp: bool,
    /// The file this app writes, where the mechanism is a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Whether the machine currently asks us for this suffix.
    pub configured: bool,
    /// The text this app would write, or the line the user must place.
    pub instruction: String,
    /// What is reloaded after the write, spelled out rather than done quietly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reload: Option<String>,
    /// A file already at that path, belonging to something else, and what it
    /// says. Shown before the switch is pressed, never discovered afterwards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign: Option<String>,
    /// Files this app wrote for a suffix this workspace has since left.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<String>,
    /// The machine asks us and nothing is answering — the state in which every
    /// name under the suffix fails, and the one nothing else on screen reports.
    pub broken: bool,
}

pub fn status(suffix: &str, listening: bool, tcp: bool) -> Status {
    let tld = tld_of(suffix).unwrap_or_default();
    let plan = plan(suffix).ok();
    let mechanism = plan
        .as_ref()
        .map(|p| p.mechanism)
        .unwrap_or(Mechanism::Manual);
    let configured = configured(suffix);

    Status {
        mechanism,
        writable: mechanism.writable() && elevation_available(),
        suffix: suffix.to_string(),
        tld,
        port: PORT,
        listening,
        tcp,
        configured,
        foreign: foreign_file(suffix),
        stale: stale_files(suffix)
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        // Asked of the socket rather than of `listening`, which is this
        // process's opinion: a responder whose thread died, or one in another
        // copy of the app, are both cases where the flag and the machine
        // disagree and the machine is right.
        broken: configured && !answering(suffix),
        instruction: plan
            .as_ref()
            .map(|p| p.text.clone())
            .unwrap_or_else(|| forward_line(suffix)),
        file: plan
            .as_ref()
            .and_then(|p| p.file.as_ref())
            .map(|p| p.display().to_string()),
        reload: plan
            .as_ref()
            .filter(|p| !p.reload.is_empty())
            .map(|p| p.reload.join(" ")),
    }
}

/// Can this app raise a password prompt at all on this machine?
///
/// A mechanism that exists and no way to apply it is a switch that fails on
/// press, which the pane would rather not draw. Linux without a polkit agent is
/// the real case: the file is known, the line is right, and the only honest
/// offer is "run this yourself".
fn elevation_available() -> bool {
    crate::elevate::available()
}

// ----------------------------------------------------------------- applying it

/// Point this machine at the responder, and prove it worked.
///
/// The steps in order, because each one exists to stop a specific way this goes
/// wrong:
///
/// 1. **Refuse if nothing is listening.** Pointing a machine's resolver at a
///    closed port is how a suffix stops resolving altogether.
/// 2. **Remember whether the internet resolved.** Not "does it resolve now" —
///    the comparison is with this machine a second ago, so a laptop on a train
///    is not told its DNS broke.
/// 3. Write, elevated, as a staged copy.
/// 4. **Ask the machine**, not the file. A file that exists proves a write
///    happened and nothing else.
/// 5. **Undo it** if the suffix does not resolve, or if public names stopped.
pub fn install(suffix: &str) -> Result<()> {
    let plan = plan(suffix)?;
    if plan.mechanism == Mechanism::Manual {
        return Err(Error::new(
            Code::Unsupported,
            "nothing recognisable is in front of this machine's resolver, so there is no file to write",
        )
        .with_hint(crate::hints::DNS_PLACE_THE_LINE_YOURSELF));
    }

    if !answering(suffix) {
        return Err(Error::new(
            Code::Conflict,
            format!("nothing is answering on 127.0.0.1:{PORT}, so this would point the machine at a closed port"),
        )
        .with_hint(crate::hints::DNS_START_THE_RESPONDER_FIRST));
    }

    let public_before = resolves(PUBLIC_PROBE);
    let foreign = foreign_file(suffix).is_some();
    let stale = stale_files(suffix);

    write_plan(&plan, foreign, &stale)?;

    match settle(suffix, public_before) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Best effort, and deliberately not reported over the top of the
            // error that caused it: what the user needs to read is why the
            // change was refused, and the state they are left in is the state
            // they started in.
            let _ = remove(suffix);
            Err(e)
        }
    }
}

/// Wait for the machine to notice, then check what it does.
fn settle(suffix: &str, public_before: bool) -> Result<()> {
    let probe = probe_name(suffix);

    // A resolver reads its new configuration on its own schedule — immediately
    // for macOS, after a reload for the others — so this asks repeatedly for a
    // few seconds rather than once and giving up.
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut resolved_here = false;
    while Instant::now() < deadline {
        if resolves_to_loopback(&probe) {
            resolved_here = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    if !resolved_here {
        return Err(Error::new(
            Code::IoError,
            format!("this machine still does not resolve {probe} to 127.0.0.1, so the change was undone"),
        )
        .with_hint(crate::hints::DNS_MACHINE_IS_NOT_ASKING_US));
    }

    if public_before && !resolves(PUBLIC_PROBE) {
        return Err(Error::new(
            Code::IoError,
            format!("{PUBLIC_PROBE} stopped resolving after the change, so the change was undone"),
        )
        .with_hint(crate::hints::DNS_PUBLIC_NAMES_STOPPED));
    }

    Ok(())
}

/// A name under the suffix that cannot be anything else's.
///
/// Not the workspace's own domain: that one may well be in `/etc/hosts`
/// already, and a check that passes because of the file it is meant to replace
/// proves nothing at all.
pub fn probe_name(suffix: &str) -> String {
    let tld = tld_of(suffix).unwrap_or_else(|| "invalid".into());
    format!("stackvo-dns-check.{tld}")
}

/// The line that runs as root, built rather than executed.
///
/// Separated from [`write_plan`] so it can be read by a test. It is the one
/// string in this module that runs with privilege, every path in it comes from
/// somewhere a user can influence, and until this was split the only thing
/// checking its shape was whoever last read the function.
fn write_command(
    plan: &Plan,
    path: &Path,
    staged: &Path,
    foreign: bool,
    stale: &[PathBuf],
) -> String {
    let parent = path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "/etc".into());

    let mut steps = Vec::new();
    // The directory may not exist on a machine that has never had one, and it
    // is created in the same elevated step rather than in a second prompt.
    steps.push(format!("mkdir -p {}", shell_quote(&parent)));
    if foreign {
        steps.push(format!(
            "cp {} {}",
            shell_quote(&path.display().to_string()),
            shell_quote(&backup_path(path).display().to_string())
        ));
    }
    steps.push(format!(
        "cp {} {}",
        shell_quote(&staged.display().to_string()),
        shell_quote(&path.display().to_string())
    ));
    for old in stale {
        steps.push(format!("rm -f {}", shell_quote(&old.display().to_string())));
    }
    steps.extend(reload_step(plan));
    steps.join(" && ")
}

/// The inverse, same treatment: a backup goes back, and nothing of ours stays.
fn remove_command(plan: &Plan, path: &Path, backup_exists: bool, stale: &[PathBuf]) -> String {
    let mut steps = Vec::new();
    // Putting somebody's file back is the inverse of having moved it aside, and
    // `mv` rather than `cp` so the backup does not linger and get restored a
    // second time over a file the user has since written themselves.
    if backup_exists {
        steps.push(format!(
            "mv {} {}",
            shell_quote(&backup_path(path).display().to_string()),
            shell_quote(&path.display().to_string())
        ));
    } else {
        steps.push(format!(
            "rm -f {}",
            shell_quote(&path.display().to_string())
        ));
    }
    // Anything this app left under another suffix goes with it. "Off" that
    // leaves a file behind is not off.
    for old in stale {
        steps.push(format!("rm -f {}", shell_quote(&old.display().to_string())));
    }
    steps.extend(reload_step(plan));
    steps.join(" && ")
}

fn reload_step(plan: &Plan) -> Option<String> {
    (!plan.reload.is_empty()).then(|| {
        plan.reload
            .iter()
            .map(|word| shell_quote(word))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// The elevated half: a staged copy plus whatever has to be reloaded, in one
/// step so one password covers it.
///
/// The same shape as [`crate::hosts::apply`], and for the same reason: what is
/// run as root is a copy of a file whose contents the user has already seen on
/// screen, never a command carrying a value from anywhere else.
///
/// Three things happen under the one password, and all three are decided *here*
/// rather than in the shell — `[ -f x ] && …` inside an elevated one-liner is a
/// second program nobody reviews:
///
/// * a file already there that is not ours is copied aside first;
/// * the file is written;
/// * resolver files left over from a suffix this workspace has left are
///   removed, because on macOS they would otherwise keep refusing a TLD that
///   used to resolve.
fn write_plan(plan: &Plan, foreign: bool, stale: &[PathBuf]) -> Result<()> {
    #[cfg(windows)]
    if plan.mechanism == Mechanism::Nrpt {
        return write_nrpt(plan);
    }

    let Some(path) = plan.file.as_ref() else {
        return Err(Error::new(
            Code::Unsupported,
            "this mechanism has no file to write",
        ));
    };

    let staged = std::env::temp_dir().join("stackvo-dns-staged");
    std::fs::write(&staged, &plan.text).map_err(|e| Error::io("staging the resolver file", e))?;

    let command = write_command(plan, path, &staged, foreign, stale);
    let ok = crate::elevate::run(&["/bin/sh", "-c", &command])?;
    let _ = std::fs::remove_file(&staged);

    if !ok {
        return Err(Error::new(
            Code::PermissionDenied,
            format!("{} was not written.", path.display()),
        ));
    }
    Ok(())
}

/// Add the NRPT rule, replacing any rule this app left for the same namespace.
#[cfg(windows)]
fn write_nrpt(plan: &Plan) -> Result<()> {
    let namespace = plan
        .text
        .split('\'')
        .nth(1)
        .ok_or_else(|| Error::new(Code::InvalidInput, "the NRPT rule has no namespace"))?
        .to_string();

    // Removing first: `Add-DnsClientNrptRule` is happy to add a second rule for
    // a namespace that already has one, and two rules for one suffix is a table
    // nobody can reason about later.
    let script = format!(
        "Get-DnsClientNrptRule | Where-Object {{ $_.Namespace -eq '{namespace}' }} | Remove-DnsClientNrptRule -Force -ErrorAction SilentlyContinue; \
         Add-DnsClientNrptRule -Namespace '{namespace}' -NameServers '127.0.0.1' -Comment 'StackVo'; \
         ipconfig /flushdns | Out-Null"
    );

    let ok = crate::elevate::run_powershell(&script)?;
    nrpt_cache().clear();
    if !ok {
        return Err(Error::new(
            Code::PermissionDenied,
            "the NRPT rule was not added.",
        ));
    }
    Ok(())
}

/// Take it away again, so turning this off is as easy as turning it on.
pub fn remove(suffix: &str) -> Result<()> {
    let plan = plan(suffix)?;

    #[cfg(windows)]
    if plan.mechanism == Mechanism::Nrpt {
        let tld = require_tld(suffix)?;
        let script = format!(
            "Get-DnsClientNrptRule | Where-Object {{ $_.Namespace -eq '.{tld}' }} | Remove-DnsClientNrptRule -Force; \
             ipconfig /flushdns | Out-Null"
        );
        let ok = crate::elevate::run_powershell(&script)?;
        nrpt_cache().clear();
        return if ok {
            Ok(())
        } else {
            Err(Error::new(
                Code::PermissionDenied,
                "the NRPT rule was not removed.",
            ))
        };
    }

    let Some(path) = plan.file.as_ref() else {
        return Err(Error::new(
            Code::Unsupported,
            "this platform has nothing for this app to remove",
        ));
    };

    let backup = backup_path(path);
    let stale = stale_files(suffix);
    if !path.exists() && !backup.exists() && stale.is_empty() {
        return Ok(());
    }

    let command = remove_command(&plan, path, backup.exists(), &stale);
    let ok = crate::elevate::run(&["/bin/sh", "-c", &command])?;
    if !ok {
        return Err(Error::new(
            Code::PermissionDenied,
            format!("{} was not removed.", path.display()),
        ));
    }
    Ok(())
}

/// Single-quote a value for `sh -c`.
///
/// Every value that reaches this is built by this module — a temp file, a path
/// under `/etc` whose last label [`tld_of`] has already checked, and reload
/// commands that are constants. It is here anyway because the next person to
/// add a fourth mechanism will not know that, and a quoting function that
/// exists is cheaper than the review that would have caught its absence.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

// -------------------------------------------------------------- measuring it

/// One thing that was checked, and what came back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub ok: bool,
    pub detail: String,
}

impl Probe {
    fn new(ok: bool, detail: impl Into<String>) -> Self {
        Probe {
            ok,
            detail: detail.into(),
        }
    }
}

/// The end-to-end answer, in the four pieces that can fail separately.
///
/// They are separate because the repair is different for each: a responder that
/// does not answer is this app's fault, a machine that does not ask it is the
/// resolver file's, and public names failing is the one that matters more than
/// the feature does.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    /// Asked the responder directly, over UDP.
    pub udp: Probe,
    /// Asked the responder directly, over TCP.
    pub tcp: Probe,
    /// Asked this machine, the way a browser would.
    pub system: Probe,
    /// Whether the rest of the internet still resolves.
    pub public: Probe,
    pub ok: bool,
}

pub fn check(suffix: &str) -> Check {
    let name = probe_name(suffix);

    let udp = match ask_udp(&name) {
        Some(ip) if ip.is_loopback() => Probe::new(true, format!("127.0.0.1:{PORT} answered {ip}")),
        Some(ip) => Probe::new(false, format!("answered {ip}, which is not this machine")),
        None => Probe::new(false, format!("nothing answered on 127.0.0.1:{PORT}")),
    };
    let tcp = match ask_tcp(&name) {
        Some(ip) if ip.is_loopback() => Probe::new(true, format!("127.0.0.1:{PORT} answered {ip}")),
        Some(ip) => Probe::new(false, format!("answered {ip}, which is not this machine")),
        None => Probe::new(false, format!("nothing answered on tcp/{PORT}")),
    };
    let system = if resolves_to_loopback(&name) {
        Probe::new(true, format!("{name} resolves to this machine"))
    } else {
        Probe::new(false, format!("{name} does not resolve here"))
    };
    let public = if resolves(PUBLIC_PROBE) {
        Probe::new(true, format!("{PUBLIC_PROBE} still resolves"))
    } else {
        Probe::new(
            false,
            format!("{PUBLIC_PROBE} does not resolve — this machine may simply be offline"),
        )
    };

    let ok = udp.ok && system.ok;
    Check {
        name,
        udp,
        tcp,
        system,
        public,
        ok,
    }
}

/// Does something answer for this suffix on our own port right now?
///
/// A question about the socket, asked over the socket. Reading a flag out of
/// this process would say "yes" for a responder whose thread died, and reading
/// a preference would say "yes" for one that never started.
pub fn answering(suffix: &str) -> bool {
    ask_udp(&probe_name(suffix)).is_some_and(|ip| ip.is_loopback())
}

/// The two questions the hosts file used to be the only answer to: is the
/// machine pointed at us, and is anyone home.
///
/// Cheap on purpose — a loopback round trip and a file read — because this is
/// asked on the way to listing projects, and a name lookup per project would
/// put a network timeout in front of a screen.
///
/// Held for two seconds because the expensive case is the *unhappy* one: a
/// machine pointed at a port nothing answers pays the probe's full timeout, and
/// a screen refreshing three panels would pay it three times for an answer that
/// cannot have changed in between.
pub fn covers(suffix: &str) -> bool {
    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<(String, bool, Instant)>>> =
        std::sync::OnceLock::new();
    let cell = LAST.get_or_init(|| std::sync::Mutex::new(None));

    if let Ok(seen) = cell.lock() {
        if let Some((for_suffix, answer, at)) = seen.as_ref() {
            if for_suffix == suffix && at.elapsed() < Duration::from_secs(2) {
                return *answer;
            }
        }
    }

    let answer = configured(suffix) && answering(suffix);
    if let Ok(mut seen) = cell.lock() {
        *seen = Some((suffix.to_string(), answer, Instant::now()));
    }
    answer
}

fn query_bytes(name: &str, qtype: u16) -> Vec<u8> {
    let mut out = vec![0x2b, 0x1c, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out.extend_from_slice(&qtype.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out
}

fn ask_udp(name: &str) -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    socket.set_read_timeout(Some(PROBE_TIMEOUT)).ok()?;
    socket
        .send_to(&query_bytes(name, TYPE_A), (Ipv4Addr::LOCALHOST, PORT))
        .ok()?;
    let mut buf = [0u8; MAX];
    let (len, _) = socket.recv_from(&mut buf).ok()?;
    first_a(&buf[..len])
}

fn ask_tcp(name: &str) -> Option<Ipv4Addr> {
    use std::io::{Read, Write};

    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, PORT));
    let mut stream = TcpStream::connect_timeout(&address, PROBE_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(PROBE_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(PROBE_TIMEOUT)).ok()?;

    let message = query_bytes(name, TYPE_A);
    let mut framed = (message.len() as u16).to_be_bytes().to_vec();
    framed.extend_from_slice(&message);
    stream.write_all(&framed).ok()?;

    let mut head = [0u8; 2];
    stream.read_exact(&mut head).ok()?;
    let len = u16::from_be_bytes(head) as usize;
    if len == 0 || len > MAX * 8 {
        return None;
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).ok()?;
    first_a(&body)
}

/// The first A record in a reply, if it is an answer to what we asked.
///
/// Only ever pointed at this module's own responder, so the shape is known —
/// which is exactly why it is still written defensively: the one time it is
/// pointed at something else, that something else is not obliged to be kind.
fn first_a(message: &[u8]) -> Option<Ipv4Addr> {
    if message.len() < 12 || message[2] & 0x80 == 0 {
        return None;
    }
    if u16::from_be_bytes([message[6], message[7]]) == 0 {
        return None;
    }

    let mut at = 12 + question_span(message)?;
    // The name: a compression pointer, or labels ending in a zero byte.
    match message.get(at) {
        Some(&byte) if byte & 0xC0 == 0xC0 => at += 2,
        Some(_) => {
            while let Some(&len) = message.get(at) {
                at += 1 + len as usize;
                if len == 0 {
                    break;
                }
            }
        }
        None => return None,
    }

    let rtype = u16::from_be_bytes([*message.get(at)?, *message.get(at + 1)?]);
    let rdlen = u16::from_be_bytes([*message.get(at + 8)?, *message.get(at + 9)?]) as usize;
    if rtype != TYPE_A || rdlen != 4 {
        return None;
    }
    let data = message.get(at + 10..at + 14)?;
    Some(Ipv4Addr::new(data[0], data[1], data[2], data[3]))
}

/// Ask the machine, the way anything else on it would.
///
/// `getaddrinfo` on its own thread with a deadline: a resolver that has been
/// pointed somewhere unreachable does not fail, it waits, and this is called
/// from a path that has to answer a screen.
fn lookup(name: &str) -> Option<Vec<IpAddr>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let owned = name.to_string();
    std::thread::Builder::new()
        .name("stackvo-dns-lookup".into())
        .spawn(move || {
            let answer = (owned.as_str(), 0u16)
                .to_socket_addrs()
                .map(|addrs| addrs.map(|a| a.ip()).collect::<Vec<_>>())
                .ok();
            // The receiver may have given up; nothing here cares.
            let _ = tx.send(answer);
        })
        .ok()?;

    rx.recv_timeout(SYSTEM_TIMEOUT).ok().flatten()
}

fn resolves(name: &str) -> bool {
    lookup(name).is_some_and(|addrs| !addrs.is_empty())
}

fn resolves_to_loopback(name: &str) -> bool {
    lookup(name).is_some_and(|addrs| !addrs.is_empty() && addrs.iter().all(|ip| ip.is_loopback()))
}

// ------------------------------------------------------------ the responder

/// One question, as far as this needs to understand it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

pub const TYPE_A: u16 = 1;
pub const TYPE_AAAA: u16 = 28;
pub const TYPE_OPT: u16 = 41;
pub const CLASS_IN: u16 = 1;

const RCODE_FORMERR: u8 = 1;
const RCODE_REFUSED: u8 = 5;

/// What reading the question section produced.
enum Read {
    /// A question this responder can reason about, and its length in bytes.
    Question(Question, usize),
    /// The section is well formed but the name is not text — a name this cannot
    /// serve, which is a refusal rather than a parse failure. The length is
    /// still known, so the question can be echoed back.
    Unreadable(usize),
    /// Not a question section at all.
    Malformed,
}

/// Walk the question section.
///
/// Compression pointers are refused rather than followed. A pointer in the
/// question section is not something a real resolver emits, and following one
/// means implementing loop detection for a message this responder is only ever
/// going to answer with 127.0.0.1 — an attack surface bought for nothing.
fn read_question(message: &[u8]) -> Read {
    if message.len() < 12 {
        return Read::Malformed;
    }
    if u16::from_be_bytes([message[4], message[5]]) != 1 {
        // Zero questions is not a query; more than one is legal on the wire and
        // implemented by nothing, and answering the first while ignoring the
        // rest is the kind of half-answer that confuses a resolver.
        return Read::Malformed;
    }

    let mut at = 12;
    let mut name = String::new();
    let mut readable = true;
    loop {
        let Some(&len) = message.get(at) else {
            return Read::Malformed;
        };
        if len & 0xC0 != 0 {
            return Read::Malformed;
        }
        at += 1;
        if len == 0 {
            break;
        }
        let end = at + len as usize;
        let Some(label) = message.get(at..end) else {
            return Read::Malformed;
        };
        // A label is bytes, not necessarily UTF-8.
        match std::str::from_utf8(label) {
            Ok(text) => {
                if !name.is_empty() {
                    name.push('.');
                }
                name.push_str(&text.to_ascii_lowercase());
            }
            Err(_) => readable = false,
        }
        at = end;
    }

    let Some(tail) = message.get(at..at + 4) else {
        return Read::Malformed;
    };
    let span = at + 4 - 12;
    if !readable {
        return Read::Unreadable(span);
    }
    Read::Question(
        Question {
            name,
            qtype: u16::from_be_bytes([tail[0], tail[1]]),
            qclass: u16::from_be_bytes([tail[2], tail[3]]),
        },
        span,
    )
}

/// Read the question out of a query.
pub fn parse_question(message: &[u8]) -> std::result::Result<Question, u8> {
    match read_question(message) {
        Read::Question(question, _) => Ok(question),
        Read::Unreadable(_) => Err(RCODE_REFUSED),
        Read::Malformed => Err(RCODE_FORMERR),
    }
}

/// How many bytes the question occupies after the header.
fn question_span(message: &[u8]) -> Option<usize> {
    match read_question(message) {
        Read::Question(_, span) | Read::Unreadable(span) => Some(span),
        Read::Malformed => None,
    }
}

/// Build the reply to a query, or `None` when there is nothing to reply to.
///
/// `suffix` is the workspace's, and the match is on a label boundary: a
/// workspace on `loc` serves `shop.loc` and `a.b.shop.loc` — which is E-2's
/// wildcard, for free, because a suffix match does not care how many labels
/// precede it — and does not serve `notloc` or `evil-loc.com`.
pub fn reply(message: &[u8], suffix: &str) -> Option<Vec<u8>> {
    if message.len() < 12 {
        return None;
    }
    // A response, not a query. Answering one would be a loop.
    if message[2] & 0x80 != 0 {
        return None;
    }

    let (question, span) = match read_question(message) {
        Read::Question(question, span) => (Some(question), span),
        Read::Unreadable(span) => (None, span),
        // Nothing was understood, so there is no question to echo and the
        // header says so: QDCOUNT 0 with an empty body is a consistent message,
        // where QDCOUNT 1 with an empty body is one a parser calls malformed.
        Read::Malformed => return Some(header(message, RCODE_FORMERR, 0, 0, 0)),
    };

    // An EDNS query gets an EDNS answer. Without it a resolver concludes this
    // server does not speak EDNS, which is true and harmless — but the OPT
    // record costs eleven bytes and keeps the exchange in the shape the other
    // side expects.
    let opt = has_opt(message, span);

    let Some(question) = question else {
        return Some(echo(message, span, RCODE_REFUSED, 0, opt, &[]));
    };

    if question.qclass != CLASS_IN || !serves(&question.name, suffix) {
        return Some(echo(message, span, RCODE_REFUSED, 0, opt, &[]));
    }

    // NODATA, not NXDOMAIN: the name exists, this record type does not. A
    // resolver told NXDOMAIN for an MX query caches the *name* as absent, and
    // the next A query for it never reaches here. Browsers ask for HTTPS
    // records (type 65) before they ask for an address, so this path is on the
    // way to every page load, not an exotic case.
    if question.qtype != TYPE_A && question.qtype != TYPE_AAAA {
        return Some(echo(message, span, 0, 0, opt, &[]));
    }

    let mut record = Vec::with_capacity(28);
    // A compression pointer to the question's name at offset 12, which is where
    // every DNS answer puts it.
    record.extend_from_slice(&[0xC0, 0x0C]);
    record.extend_from_slice(&question.qtype.to_be_bytes());
    record.extend_from_slice(&CLASS_IN.to_be_bytes());
    // Sixty seconds. Long enough that a page load does not re-query per asset,
    // short enough that removing the resolver takes effect while somebody is
    // still looking at the screen they removed it from.
    record.extend_from_slice(&60u32.to_be_bytes());
    if question.qtype == TYPE_A {
        record.extend_from_slice(&4u16.to_be_bytes());
        record.extend_from_slice(&Ipv4Addr::LOCALHOST.octets());
    } else {
        record.extend_from_slice(&16u16.to_be_bytes());
        record.extend_from_slice(&std::net::Ipv6Addr::LOCALHOST.octets());
    }

    Some(echo(message, span, 0, 1, opt, &record))
}

/// Does the query carry an OPT record?
///
/// Only the first record of the additional section is looked at, which is where
/// every resolver puts it. A query that hides its OPT behind something else
/// gets an answer without one, which is legal and means "no EDNS here".
fn has_opt(message: &[u8], span: usize) -> bool {
    if u16::from_be_bytes([message[10], message[11]]) == 0 {
        return false;
    }
    let at = 12 + span;
    // Root name, then the type. OPT always has an empty name.
    matches!(message.get(at..at + 3), Some([0x00, hi, lo]) if u16::from_be_bytes([*hi, *lo]) == TYPE_OPT)
}

/// A reply that echoes the question it answers, which is not optional.
///
/// A stub resolver compares the question in the reply with the one it sent, and
/// drops what does not match — so a header claiming one question over an empty
/// body is not a fast failure, it is a five-second timeout. This module shipped
/// exactly that for every REFUSED and every NODATA, and `dig` said so:
/// *"Message parser reports malformed message packet"*.
///
/// The bytes are copied rather than re-encoded, because the comparison on the
/// other side is byte for byte.
fn echo(message: &[u8], span: usize, rcode: u8, ancount: u16, opt: bool, answer: &[u8]) -> Vec<u8> {
    let mut out = header(message, rcode, 1, ancount, u16::from(opt));
    out.extend_from_slice(&message[12..12 + span]);
    out.extend_from_slice(answer);
    if opt {
        // Name: root. Type: OPT. Class: the largest reply we would ever send,
        // which is the field's meaning here. TTL: extended rcode 0, version 0,
        // no flags. No options.
        out.extend_from_slice(&[0x00]);
        out.extend_from_slice(&TYPE_OPT.to_be_bytes());
        out.extend_from_slice(&(MAX as u16).to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
    }
    out
}

/// The twelve bytes every reply starts with.
fn header(message: &[u8], rcode: u8, qdcount: u16, ancount: u16, arcount: u16) -> Vec<u8> {
    let mut out = vec![0u8; 12];
    out[0] = message[0];
    out[1] = message[1];
    // QR=1, AA=1, and RD copied back from the query the way every responder
    // does. RA stays 0: recursion is exactly what this does not offer.
    out[2] = 0x84 | (message[2] & 0x01);
    out[3] = rcode & 0x0F;
    out[4..6].copy_from_slice(&qdcount.to_be_bytes());
    out[6..8].copy_from_slice(&ancount.to_be_bytes());
    out[10..12].copy_from_slice(&arcount.to_be_bytes());
    out
}

/// Does this responder answer for this name?
fn serves(name: &str, suffix: &str) -> bool {
    let Some(tld) = tld_of(suffix) else {
        return false;
    };
    // The last label, so a workspace on `stackvo.loc` answers for every `.loc`
    // name — see `resolver_path` for why the resolver is registered that way
    // and why the two have to agree.
    name == tld || name.ends_with(&format!(".{tld}"))
}

/// Serve UDP until the socket is dropped.
///
/// Blocking, on its own thread. A responder this size has no reason to be
/// async, and the one thing it must not do is share a runtime with anything
/// that can be slow: name resolution that waits behind a Docker call is a
/// browser that hangs.
pub fn serve(socket: UdpSocket, suffix: String, stop: Arc<AtomicBool>) {
    let mut buf = [0u8; MAX];
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let (len, from) = match socket.recv_from(&mut buf) {
            Ok(pair) => pair,
            // A timeout is how the stop flag gets looked at; anything else on a
            // bound loopback socket is not worth tearing the responder down for.
            Err(_) => continue,
        };
        if let Some(out) = reply(&buf[..len], &suffix) {
            let _ = socket.send_to(&out, from);
        }
    }
}

/// How many TCP conversations may be in flight at once.
///
/// A cap rather than a queue: this is loopback, the answers are one packet, and
/// the only thing that gets anywhere near the limit is something that is not a
/// resolver. Refusing the thirty-third connection is a better failure than
/// spawning threads until the machine notices.
const MAX_TCP: usize = 32;

/// Serve TCP until told to stop.
///
/// Accepts non-blocking with a short sleep, so the stop flag is read on the
/// same schedule as UDP's read timeout. Each conversation gets its own thread
/// with deadlines on both directions: a client that connects and says nothing
/// must not be able to hold a slot for ever.
pub fn serve_tcp(listener: TcpListener, suffix: String, stop: Arc<AtomicBool>) {
    let live = Arc::new(AtomicUsize::new(0));
    if listener.set_nonblocking(true).is_err() {
        return;
    }

    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if live.load(Ordering::Relaxed) >= MAX_TCP {
                    // Dropping the stream closes it, which is the honest answer
                    // to "this server is busy" for a protocol with no way to
                    // say so.
                    continue;
                }
                live.fetch_add(1, Ordering::Relaxed);
                let suffix = suffix.clone();
                let held = Arc::clone(&live);
                let spawned = std::thread::Builder::new()
                    .name("stackvo-dns-tcp".into())
                    .spawn(move || {
                        converse(stream, &suffix);
                        held.fetch_sub(1, Ordering::Relaxed);
                    });
                if spawned.is_err() {
                    live.fetch_sub(1, Ordering::Relaxed);
                }
            }
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// One TCP conversation: length-prefixed messages until the client goes away.
fn converse(mut stream: TcpStream, suffix: &str) {
    use std::io::{Read as _, Write as _};

    // Accepted sockets inherit the listener's non-blocking flag on some
    // platforms and not others, which is the kind of difference that shows up
    // as "works on my machine".
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let deadline = Some(Duration::from_secs(5));
    if stream.set_read_timeout(deadline).is_err() || stream.set_write_timeout(deadline).is_err() {
        return;
    }

    loop {
        let mut head = [0u8; 2];
        if stream.read_exact(&mut head).is_err() {
            return;
        }
        let len = u16::from_be_bytes(head) as usize;
        // A DNS message over TCP may be up to 65535 bytes. This one answers for
        // one suffix with one address, so anything past a UDP message is a
        // client that has confused this for a resolver.
        if len == 0 || len > MAX {
            return;
        }
        let mut body = vec![0u8; len];
        if stream.read_exact(&mut body).is_err() {
            return;
        }

        let Some(out) = reply(&body, suffix) else {
            return;
        };
        let mut framed = (out.len() as u16).to_be_bytes().to_vec();
        framed.extend_from_slice(&out);
        if stream.write_all(&framed).is_err() {
            return;
        }
    }
}

/// Bind loopback UDP, or say why not.
pub fn bind() -> Result<UdpSocket> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, PORT));
    let socket = UdpSocket::bind(addr).map_err(|e| {
        Error::new(
            Code::IoError,
            format!("the DNS responder could not bind 127.0.0.1:{PORT}: {e}"),
        )
        .with_hint(crate::hints::DNS_PORT_ALREADY_ANSWERING)
    })?;
    // So the stop flag is read at least this often.
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|e| Error::io("configuring the DNS socket", e))?;
    Ok(socket)
}

/// Bind loopback TCP.
///
/// Separate from [`bind`] and separately fallible: losing TCP costs the
/// occasional retry, losing UDP costs the feature, and one error type for both
/// would make the app refuse to start over the cheaper half.
pub fn bind_tcp() -> Result<TcpListener> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, PORT));
    TcpListener::bind(addr).map_err(|e| {
        Error::new(
            Code::IoError,
            format!("the DNS responder could not bind tcp/{PORT}: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A query for `name`, as bytes.
    fn query(name: &str, qtype: u16) -> Vec<u8> {
        let mut out = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in name.split('.') {
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0);
        out.extend_from_slice(&qtype.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out
    }

    /// The same query with an EDNS OPT record in the additional section, which
    /// is what a modern resolver actually sends.
    fn query_edns(name: &str, qtype: u16) -> Vec<u8> {
        let mut out = query(name, qtype);
        out[11] = 1; // ARCOUNT
        out.extend_from_slice(&[0x00]);
        out.extend_from_slice(&TYPE_OPT.to_be_bytes());
        out.extend_from_slice(&1232u16.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out
    }

    fn rcode(reply: &[u8]) -> u8 {
        reply[3] & 0x0F
    }

    fn counts(reply: &[u8]) -> (u16, u16, u16) {
        (
            u16::from_be_bytes([reply[4], reply[5]]),
            u16::from_be_bytes([reply[6], reply[7]]),
            u16::from_be_bytes([reply[10], reply[11]]),
        )
    }

    #[test]
    fn a_name_under_the_suffix_is_answered_with_loopback() {
        let out = reply(&query("shop.loc", TYPE_A), "stackvo.loc").unwrap();
        assert_eq!(rcode(&out), 0);
        assert_eq!(counts(&out).1, 1, "one answer");
        assert_eq!(&out[out.len() - 4..], &[127, 0, 0, 1]);
        // The id is echoed, and a resolver drops a reply whose id differs.
        assert_eq!(&out[..2], &[0x12, 0x34]);
    }

    /// The half `/etc/hosts` cannot do, and the reason E-2 was left at 🟡.
    #[test]
    fn a_wildcard_falls_out_of_a_suffix_match() {
        for name in ["a.shop.loc", "deep.nested.shop.loc", "loc"] {
            let out = reply(&query(name, TYPE_A), "stackvo.loc").unwrap();
            assert_eq!(rcode(&out), 0, "{name}");
        }
    }

    /// The security property. Not a resolver, not a forwarder, no upstream.
    #[test]
    fn anything_outside_the_suffix_is_refused() {
        for name in ["google.com", "notloc", "evil-loc.com", "loc.evil.com"] {
            let out = reply(&query(name, TYPE_A), "stackvo.loc").unwrap();
            assert_eq!(rcode(&out), RCODE_REFUSED, "{name} was not refused");
            assert_eq!(counts(&out).1, 0, "{name}");
        }
    }

    #[test]
    fn aaaa_is_answered_with_the_v6_loopback() {
        let out = reply(&query("shop.loc", TYPE_AAAA), "loc").unwrap();
        assert_eq!(rcode(&out), 0);
        assert_eq!(
            &out[out.len() - 16..],
            &std::net::Ipv6Addr::LOCALHOST.octets()
        );
    }

    /// NXDOMAIN for an MX query would poison the name for the A query after it.
    #[test]
    fn an_unserved_type_is_nodata_and_not_nxdomain() {
        let out = reply(&query("shop.loc", 15), "loc").unwrap();
        // 0 is NOERROR; 3 would be NXDOMAIN, which is the wrong answer here
        // and the reason this test exists.
        assert_eq!(rcode(&out), 0);
        assert_eq!(counts(&out).1, 0, "no answers");
    }

    /// The bug `dig` reported as *"malformed message packet"*: a header that
    /// claims a question over a body that carries none. Every REFUSED and every
    /// NODATA this module sent was one, which includes the type-65 query a
    /// browser makes before every page load.
    #[test]
    fn a_refusal_echoes_the_question_it_refuses() {
        for (name, suffix, expected) in [
            ("google.com", "loc", RCODE_REFUSED),
            ("shop.loc", "loc", 0), // the NODATA path, below
        ] {
            let message = query(name, 65);
            let out = reply(&message, suffix).unwrap();
            assert_eq!(rcode(&out), expected, "{name}");
            assert_eq!(counts(&out).0, 1, "{name}: QDCOUNT");
            assert_eq!(
                &out[12..],
                &message[12..],
                "{name}: the question was not echoed"
            );
        }
    }

    /// A format error is the one reply with nothing to echo, so it says so.
    #[test]
    fn a_format_error_claims_no_question() {
        let mut message = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        message.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1]);
        let out = reply(&message, "loc").unwrap();
        assert_eq!(rcode(&out), RCODE_FORMERR);
        assert_eq!(counts(&out), (0, 0, 0));
        assert_eq!(out.len(), 12);
    }

    /// A pointer in the question is refused rather than followed — loop
    /// detection bought for a responder that always says 127.0.0.1 is an attack
    /// surface bought for nothing.
    #[test]
    fn a_compression_pointer_in_the_question_is_a_format_error() {
        let mut message = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        message.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1]);
        let out = reply(&message, "loc").unwrap();
        assert_eq!(rcode(&out), RCODE_FORMERR);
    }

    /// Every one of these used to be a panic waiting for a stray packet.
    #[test]
    fn a_truncated_or_absurd_message_never_panics() {
        for message in [
            vec![],
            vec![0x12],
            vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0],
            vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0, 9, b'a'],
            vec![0x12, 0x34, 0x01, 0x00, 0, 2, 0, 0, 0, 0, 0, 0],
            // An additional-section count with no additional section: the OPT
            // check reads past the question and must not walk off the end.
            vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 1],
        ] {
            let _ = reply(&message, "loc");
        }
    }

    /// Answering a response is how two responders talk to each other for ever.
    #[test]
    fn a_response_is_not_answered() {
        let mut message = query("shop.loc", TYPE_A);
        message[2] |= 0x80;
        assert!(reply(&message, "loc").is_none());
    }

    /// A resolver compares the echoed question with the one it sent.
    #[test]
    fn the_question_is_echoed_byte_for_byte() {
        let message = query("shop.loc", TYPE_A);
        let out = reply(&message, "loc").unwrap();
        assert_eq!(&out[12..12 + (message.len() - 12)], &message[12..]);
    }

    /// An EDNS query gets an OPT record back, and a plain one does not.
    #[test]
    fn edns_is_answered_in_kind() {
        let out = reply(&query_edns("shop.loc", TYPE_A), "loc").unwrap();
        assert_eq!(rcode(&out), 0);
        assert_eq!(counts(&out), (1, 1, 1), "question, answer, OPT");
        // Root name, OPT, and the size this responder will ever send.
        assert_eq!(&out[out.len() - 11..out.len() - 8], &[0x00, 0x00, 0x29]);
        assert_eq!(
            u16::from_be_bytes([out[out.len() - 8], out[out.len() - 7]]),
            MAX as u16
        );

        let plain = reply(&query("shop.loc", TYPE_A), "loc").unwrap();
        assert_eq!(counts(&plain).2, 0, "no OPT was asked for");
    }

    /// A refusal to an EDNS query is still an EDNS reply.
    #[test]
    fn edns_survives_a_refusal() {
        let out = reply(&query_edns("google.com", TYPE_A), "loc").unwrap();
        assert_eq!(rcode(&out), RCODE_REFUSED);
        assert_eq!(counts(&out), (1, 0, 1));
    }

    /// A label that is not UTF-8 is a name this cannot serve, not a crash —
    /// and the question still comes back, so the asker can match it up.
    #[test]
    fn a_non_utf8_label_is_refused() {
        let mut message = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        message.extend_from_slice(&[2, 0xff, 0xfe, 3, b'l', b'o', b'c', 0, 0, 1, 0, 1]);
        let out = reply(&message, "loc").unwrap();
        assert_eq!(rcode(&out), RCODE_REFUSED);
        assert_eq!(counts(&out).0, 1);
        assert_eq!(&out[12..], &message[12..]);
    }

    /// The parser this module's own probes use, against the bytes it produces.
    #[test]
    fn a_reply_can_be_read_back_as_an_address() {
        let out = reply(&query("shop.loc", TYPE_A), "loc").unwrap();
        assert_eq!(first_a(&out), Some(Ipv4Addr::LOCALHOST));

        let refused = reply(&query("google.com", TYPE_A), "loc").unwrap();
        assert_eq!(first_a(&refused), None, "a refusal carries no address");

        let edns = reply(&query_edns("shop.loc", TYPE_A), "loc").unwrap();
        assert_eq!(first_a(&edns), Some(Ipv4Addr::LOCALHOST));
    }

    // ---- the suffix ---------------------------------------------------------

    /// The value this comes from is a line in a file the user edits, and it
    /// ends up in a path written as root.
    #[test]
    fn a_suffix_that_is_not_one_label_is_refused() {
        assert_eq!(tld_of("stackvo.loc").as_deref(), Some("loc"));
        assert_eq!(tld_of("LOC").as_deref(), Some("loc"));
        assert_eq!(tld_of("a.b.test").as_deref(), Some("test"));
        assert_eq!(tld_of("my-stack.dev-local").as_deref(), Some("dev-local"));

        for hostile in [
            "",
            ".",
            "..",
            "loc/../../etc",
            "../../etc/sudoers.d/x",
            "loc; rm -rf /",
            "loc it",
            "-loc",
            "loc-",
            "loc$(id)",
        ] {
            assert_eq!(tld_of(hostile), None, "{hostile:?} was accepted");
        }
    }

    /// A suffix that cannot be a TLD serves nothing at all, rather than
    /// serving everything.
    #[test]
    fn a_refused_suffix_answers_for_no_name() {
        assert!(!serves("shop.loc", "loc/../etc"));
        assert!(!serves("anything", ""));
    }

    // ---- the platform half ------------------------------------------------

    #[test]
    fn the_resolver_file_is_named_after_the_last_label() {
        if mechanism() != Mechanism::Resolver {
            return;
        }
        assert_eq!(
            resolver_path("stackvo.loc"),
            Some(PathBuf::from("/etc/resolver/loc"))
        );
        assert_eq!(
            resolver_path("test"),
            Some(PathBuf::from("/etc/resolver/test"))
        );
        assert_eq!(resolver_path("../etc"), None);
    }

    /// The file the app writes and the suffix the responder serves have to
    /// agree, or the machine asks a responder that refuses.
    #[test]
    fn the_resolver_file_and_the_served_suffix_agree() {
        assert!(serves("shop.loc", "stackvo.loc"));
        assert!(serves("anything.loc", "stackvo.loc"));
        assert!(!serves("shop.test", "stackvo.loc"));
    }

    #[test]
    fn the_forward_line_names_the_same_port_the_responder_binds() {
        assert_eq!(
            forward_line("stackvo.loc"),
            format!("server=/loc/127.0.0.1#{PORT}")
        );
        assert!(resolver_text().contains(&format!("port {PORT}")));
        assert!(resolved_text("loc").contains(&format!("DNS=127.0.0.1:{PORT}")));
        assert!(resolved_text("loc").contains("Domains=~loc"));
    }

    /// Every mechanism has to name this machine and this port, in its own
    /// syntax, or the file is written and nothing asks us.
    #[test]
    fn every_plan_points_at_the_responder() {
        for text in [
            resolver_text(),
            format!("{}\n", forward_line("stackvo.loc")),
            resolved_text("loc"),
        ] {
            let dir = std::env::temp_dir().join(format!(
                "stackvo-dns-plan-{}-{}",
                std::process::id(),
                text.len()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("conf");
            std::fs::write(&path, &text).unwrap();
            assert!(file_points_at_us(&path), "{text:?} was not recognised");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// The four plans a macOS machine never builds.
    ///
    /// `mechanism()` here answers `Resolver` and always will, so without this
    /// the Linux and Windows halves ship with nothing having read them. Each
    /// case asserts the two facts that make the file work — where it goes, and
    /// that its text names this machine and this port **in that file's own
    /// syntax**, which is the part that differs and therefore the part that can
    /// be wrong.
    #[test]
    fn every_mechanism_writes_something_that_points_here() {
        let port_line = format!("port {PORT}");
        let dnsmasq_line = format!("server=/loc/127.0.0.1#{PORT}");
        let resolved_line = format!("DNS=127.0.0.1:{PORT}");

        let cases = [
            (
                Mechanism::Resolver,
                "/etc/resolver/loc",
                vec!["nameserver 127.0.0.1", port_line.as_str()],
                "",
            ),
            (
                Mechanism::NetworkManager,
                "/etc/NetworkManager/dnsmasq.d/stackvo.conf",
                vec![dnsmasq_line.as_str()],
                "systemctl reload NetworkManager",
            ),
            (
                Mechanism::Dnsmasq,
                "/etc/dnsmasq.d/stackvo.conf",
                vec![dnsmasq_line.as_str()],
                "systemctl restart dnsmasq",
            ),
            (
                Mechanism::SystemdResolved,
                "/etc/systemd/resolved.conf.d/stackvo.conf",
                vec![
                    "[Resolve]",
                    resolved_line.as_str(),
                    // The routing domain. Without it, resolved is being asked
                    // to send *everything* to a server that refuses everything.
                    "Domains=~loc",
                ],
                "systemctl restart systemd-resolved",
            ),
        ];

        for (mechanism, path, must_contain, reload) in cases {
            let plan = plan_for(mechanism, "loc");
            assert_eq!(
                plan.file.as_deref(),
                Some(Path::new(path)),
                "{mechanism:?} writes the wrong file"
            );
            for needle in must_contain {
                assert!(
                    plan.text.contains(needle),
                    "{mechanism:?}: {:?} does not contain {needle:?}",
                    plan.text
                );
            }
            assert_eq!(plan.reload.join(" "), reload, "{mechanism:?}");
            assert!(
                plan.text.ends_with('\n'),
                "{mechanism:?}: a config file without a final newline is one the \
                 next appended line joins onto"
            );
        }

        // Windows names a rule rather than a file, and the namespace has to
        // carry the leading dot or the NRPT matches the literal name `loc`.
        let nrpt = plan_for(Mechanism::Nrpt, "loc");
        assert_eq!(nrpt.file, None);
        assert!(nrpt.text.contains("-Namespace '.loc'"), "{}", nrpt.text);
        assert!(nrpt.text.contains("-NameServers '127.0.0.1'"));

        // And the honest one: a line to place, and nothing written.
        let manual = plan_for(Mechanism::Manual, "loc");
        assert_eq!(manual.file, None);
        assert!(manual.text.contains(&format!("127.0.0.1#{PORT}")));
        assert!(!manual.mechanism.writable());
    }

    /// The dnsmasq and resolved files are recognised as ours when read back,
    /// which is what `configured` is built on. A plan the status cannot
    /// recognise is a switch that never latches.
    #[test]
    fn every_written_plan_reads_back_as_ours() {
        for mechanism in [
            Mechanism::Resolver,
            Mechanism::NetworkManager,
            Mechanism::Dnsmasq,
            Mechanism::SystemdResolved,
        ] {
            let plan = plan_for(mechanism, "loc");
            let dir = std::env::temp_dir().join(format!(
                "stackvo-dns-readback-{}-{mechanism:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("conf");
            std::fs::write(&path, &plan.text).unwrap();
            assert!(file_points_at_us(&path), "{mechanism:?} is not read back");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// A resolver file that belongs to something else — dnsmasq, Valet, a
    /// colleague's script — must be reported before it is overwritten, and it
    /// is reported with what it *says*, because "a file exists" is not enough
    /// to consent to replacing it.
    #[test]
    fn a_file_that_is_not_ours_is_summarised_rather_than_hidden() {
        let dir = std::env::temp_dir().join(format!("stackvo-dns-foreign-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loc");

        std::fs::write(&path, "# somebody's file\nnameserver 127.0.0.1\nport 53\n").unwrap();
        assert!(!file_points_at_us(&path), "port 53 is not this responder");

        // The summary skips the comment and reports the first line that says
        // something.
        let text = std::fs::read_to_string(&path).unwrap();
        let summary = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'));
        assert_eq!(summary, Some("nameserver 127.0.0.1"));

        assert_eq!(
            backup_path(&path),
            dir.join("loc.pre-stackvo"),
            "the backup sits beside the file it copies"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A user's own comment or `search` line must not make this report the
    /// resolver as unconfigured — the app did not create that file's style.
    #[test]
    fn a_resolver_file_with_extra_lines_still_counts() {
        let dir = std::env::temp_dir().join(format!("stackvo-dns-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loc");

        std::fs::write(
            &path,
            format!("# stackvo\nnameserver 127.0.0.1\nport {PORT}\n"),
        )
        .unwrap();
        assert!(file_points_at_us(&path));

        std::fs::write(&path, "nameserver 127.0.0.1\n").unwrap();
        assert!(
            !file_points_at_us(&path),
            "a file with no port points at 53"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quoting_survives_a_path_with_a_quote_in_it() {
        assert_eq!(shell_quote("/tmp/a'b"), r"'/tmp/a'\''b'");
    }

    /// The one line in this module that runs as root, read rather than run.
    ///
    /// Ordering is the assertion: the backup has to be taken **before** the
    /// write that would destroy what it copies, and `&&` rather than `;` so a
    /// failed backup stops the write instead of proceeding without one.
    #[test]
    fn the_elevated_line_backs_up_before_it_overwrites() {
        let plan = plan_for(Mechanism::Resolver, "loc");
        let path = plan.file.clone().unwrap();
        let staged = PathBuf::from("/tmp/stackvo-dns-staged");

        let plain = write_command(&plan, &path, &staged, false, &[]);
        assert_eq!(
            plain,
            "mkdir -p '/etc/resolver' && cp '/tmp/stackvo-dns-staged' '/etc/resolver/loc'"
        );

        let careful = write_command(
            &plan,
            &path,
            &staged,
            true,
            &[PathBuf::from("/etc/resolver/test")],
        );
        let backup = careful.find(".pre-stackvo").expect("a backup step");
        let write = careful
            .find("cp '/tmp/stackvo-dns-staged'")
            .expect("the write");
        assert!(
            backup < write,
            "the backup is taken after the overwrite: {careful}"
        );
        assert!(careful.contains("rm -f '/etc/resolver/test'"), "{careful}");
        assert!(
            !careful.contains(" ; "),
            "one failed step must stop the rest"
        );
    }

    /// Reload is the last step and only when the mechanism has one — telling
    /// dnsmasq to re-read a file that was not written yet is a reload of the
    /// old contents.
    #[test]
    fn the_reload_comes_last_and_only_where_there_is_one() {
        let plan = plan_for(Mechanism::NetworkManager, "loc");
        let path = plan.file.clone().unwrap();
        let command = write_command(&plan, &path, Path::new("/tmp/staged"), false, &[]);
        assert!(
            command.ends_with("&& 'systemctl' 'reload' 'NetworkManager'"),
            "{command}"
        );
        assert!(command.find("cp '/tmp/staged'").unwrap() < command.find("systemctl").unwrap());

        let resolver = plan_for(Mechanism::Resolver, "loc");
        assert_eq!(reload_step(&resolver), None, "macOS reloads nothing");
    }

    /// Turning it off restores what was there, and takes every file this app
    /// wrote with it — including the one a suffix change left behind.
    #[test]
    fn removing_puts_a_borrowed_file_back() {
        let plan = plan_for(Mechanism::Resolver, "test");
        let path = plan.file.clone().unwrap();

        let restored = remove_command(&plan, &path, true, &[PathBuf::from("/etc/resolver/loc")]);
        assert_eq!(
            restored,
            "mv '/etc/resolver/test.pre-stackvo' '/etc/resolver/test' && rm -f '/etc/resolver/loc'"
        );

        let plain = remove_command(&plan, &path, false, &[]);
        assert_eq!(plain, "rm -f '/etc/resolver/test'");
    }

    /// The probe name is under the suffix and is not a name anything else would
    /// have put in `/etc/hosts` — a check that passed because of the file it
    /// replaces would prove nothing.
    #[test]
    fn the_probe_name_is_served_by_the_responder() {
        let name = probe_name("stackvo.loc");
        assert!(name.ends_with(".loc"));
        assert!(serves(&name, "stackvo.loc"));
        let out = reply(&query(&name, TYPE_A), "stackvo.loc").unwrap();
        assert_eq!(rcode(&out), 0);
    }

    /// Nothing is listening in a unit test, and the probes have to say so
    /// rather than hang or panic.
    #[test]
    fn the_probes_are_honest_when_nothing_answers() {
        // Bound by nothing in this process; a machine running the real app is
        // the one case this cannot assert, so it only asserts termination.
        let _ = answering("stackvo.loc");
        let _ = covers("stackvo.loc");
    }
}
