//! Reading a rival's installation, so its sites can be brought over.
//!
//! Two of them, and the reason is a window rather than a feature list. **XAMPP
//! has been frozen on PHP 8.2 since late 2023 and lost its add-on ecosystem in
//! September 2025**; **Laragon went commercial in 2025 with a nag screen on the
//! free tier and was forked**. Those are the two largest installed bases in this
//! category and both are looking around. Every serious competitor is courting
//! them explicitly — EnvKit imports Laragon in bulk, ForgeKit lists six sources,
//! Herd publishes guides — and StackVo could read neither (competitive review
//! §L).
//!
//! ## What an import is here, and why it has to copy
//!
//! Native-binary tools register a site wherever it happens to sit. This one
//! cannot: the generator bind-mounts `${PROJECTS}/<name>`, so a project lives
//! under the projects directory or it does not exist. An import is therefore a
//! **file operation** followed by the ordinary adoption path — the same "run it
//! through the path that already exists" that the declared-services work used,
//! and for the same reason: adoption already validates a manifest, asks for a
//! domain and refuses a name that is not safe.
//!
//! **Copy is the default and move is offered.** Moving somebody's site out from
//! under a still-installed XAMPP breaks the setup they are still evaluating
//! against, and a migration you cannot back out of is one people do not start.
//! The cost is disk, which is why [`Site::bytes`] is measured and shown before
//! anything is clicked.
//!
//! ## Nothing is ever written to the other installation
//!
//! Not one byte, in either mode — and `move` only removes what it has already
//! copied. EnvKit takes Laragon out of `PATH` as part of importing it; that is
//! a decision about somebody else's machine made on their behalf, and it is
//! exactly what this module does not do.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// A tool StackVo can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Xampp,
    Laragon,
    /// MAMP. The same shape as XAMPP — one directory of sites, no vhost file
    /// to read a name out of — and one of the three the competitive review
    /// named (L).
    Mamp,
    /// Laravel Valet. **Not** the same shape, and that is the whole of the work
    /// it needed: Valet has no directory of sites. It *parks* directories,
    /// meaning "every child of this is a site", and it *links* individual ones
    /// as symlinks under `~/.config/valet/Sites`. Both are read here.
    Valet,
    /// Laravel Sail, and a third shape again — the last of the three the
    /// competitive review named (L).
    ///
    /// Sail is not an installation at all. It is a composer package *inside* a
    /// project, so there is no prefix to look under and no registry to read:
    /// what identifies one is a `docker-compose.yml` that names `laravel/sail`.
    /// [`well_known`] therefore offers nothing for it, and would be guessing if
    /// it did — `~/Code` is a convention, not a fact about a machine. A Sail
    /// import is always "point at the folder", and the folder may be the
    /// project or the directory holding several of them.
    ///
    /// It is also the one source whose file says what the site *needs*. A Sail
    /// compose file lists mysql, redis, meilisearch and the rest as services,
    /// which is the same question StackVo's own catalogue answers — so those
    /// are read and reported, and an import can say what to switch on rather
    /// than leaving somebody to diff two compose files by eye.
    Sail,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Xampp => "xampp",
            Source::Laragon => "laragon",
            Source::Mamp => "mamp",
            Source::Valet => "valet",
            Source::Sail => "sail",
        }
    }

    /// Read from a path the user chose rather than from a known prefix.
    ///
    /// True for the two that have no installation directory: Valet is a
    /// composer package on `PATH` and Sail is one inside each project. The
    /// difference decides whether a source can appear in a scan at all.
    pub fn is_pointed_at(self) -> bool {
        matches!(self, Source::Valet | Source::Sail)
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [
            Source::Xampp,
            Source::Laragon,
            Source::Mamp,
            Source::Valet,
            Source::Sail,
        ]
        .into_iter()
        .find(|source| source.as_str() == id)
    }

    /// Where the sites live, relative to the installation root.
    fn web_root(self) -> &'static str {
        match self {
            Source::Xampp => "htdocs",
            Source::Laragon => "www",
            Source::Mamp => "htdocs",
            // Valet and Sail have none. The field exists for the tools that
            // keep their sites in one directory, and returning something
            // plausible here would send `scan_at` looking for a directory that
            // never exists.
            Source::Valet | Source::Sail => "",
        }
    }
}

/// One installation found on this machine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    pub source: Source,
    /// The installation root — the directory holding `htdocs` or `www`.
    pub path: String,
    pub sites: Vec<Site>,
}

/// One site inside it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub name: String,
    pub path: String,
    /// What the rival serves it at, when the rival says so. Laragon writes a
    /// vhost per site; XAMPP serves `htdocs/<name>` as a path and has no name
    /// to read, so this is `None` and adoption asks.
    pub domain: Option<String>,
    /// Bytes on disk, so "copy" is a decision with a number attached rather
    /// than a button that turns out to have moved four gigabytes.
    pub bytes: u64,
    /// True when the walk stopped early — the size is a floor, not a total.
    pub partial: bool,
    /// What StackVo would build it as. The same inference an ordinary adoption
    /// uses, so an imported project is not a second class of project.
    pub detected: crate::detect::Detected,
    /// A directory of this name is already under `projects/`.
    pub taken: bool,
    /// Services the site's own compose file declares, mapped onto this app's
    /// catalogue where they match (Sail only — nothing else states them).
    ///
    /// Empty for every other source, and empty is honest there: XAMPP's sites
    /// do not say what they need, so the import must not invent it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<String>,
}

/// Directories inside an installation that are the tool, not a site.
///
/// XAMPP ships its own dashboard in `htdocs`, and offering to import
/// `dashboard` and `webalizer` as projects is how a list of eleven real sites
/// becomes a list of fifteen with four wrong ones in it.
const NOT_SITES: [&str; 8] = [
    "dashboard",
    "webalizer",
    "xampp",
    "img",
    "forbidden",
    "restricted",
    "favicon.ico",
    "applications.html",
];

/// Where these tools install themselves, per platform.
///
/// Well-known paths rather than a registry read or a `which`: both tools are
/// installed by dragging or by an installer with a fixed default, and a user
/// who moved theirs can point at it — [`scan_at`] takes a path.
pub fn well_known() -> Vec<(Source, PathBuf)> {
    let mut out = Vec::new();

    #[cfg(target_os = "macos")]
    {
        out.push((
            Source::Xampp,
            PathBuf::from("/Applications/XAMPP/xamppfiles"),
        ));
        out.push((Source::Xampp, PathBuf::from("/Applications/XAMPP")));
        out.push((Source::Mamp, PathBuf::from("/Applications/MAMP")));
        // Valet's root is its config directory, not an install prefix — it has
        // no install prefix, because it is a composer package on the user's
        // PATH. `~/.config/valet` is where it writes everything this reads.
        if let Some(home) = dirs::home_dir() {
            out.push((Source::Valet, home.join(".config/valet")));
        }
    }
    #[cfg(target_os = "windows")]
    {
        out.push((Source::Xampp, PathBuf::from("C:\\xampp")));
        out.push((Source::Mamp, PathBuf::from("C:\\MAMP")));
        out.push((Source::Laragon, PathBuf::from("C:\\laragon")));
        out.push((Source::Laragon, PathBuf::from("D:\\laragon")));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        out.push((Source::Xampp, PathBuf::from("/opt/lampp")));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Valet's Linux forks keep the same config path.
        if let Some(home) = dirs::home_dir() {
            out.push((Source::Valet, home.join(".config/valet")));
        }
    }

    // Laragon is a Windows product and is not offered elsewhere: listing a path
    // that cannot exist would be a row that is always empty, which reads as a
    // scan that failed rather than as a tool that is not installed. MAMP has no
    // Linux build for the same reason.
    out
}

// -------------------------------------------------------------- pure logic

/// The hostname a Laragon vhost declares, from the file's text.
///
/// `auto.<name>.test.conf` under `etc/apache2/sites-enabled/`, holding an
/// ordinary Apache `ServerName`. Parsed with a line scan rather than a config
/// grammar: one directive is wanted and the file is generated, so a parser
/// would be a dependency plus a second thing to be wrong about.
pub fn server_name(conf: &str) -> Option<String> {
    for line in conf.lines() {
        let line = line.trim();
        // `ServerAlias` is deliberately not read. It is a second name for the
        // same site, and a manifest has one `domain` — picking whichever came
        // first would be arbitrary. The extra names belong in `aliases`, which
        // the user can add after the import with the evidence in front of them.
        let Some(rest) = line
            .strip_prefix("ServerName ")
            .or_else(|| line.strip_prefix("servername "))
        else {
            continue;
        };
        let name = rest.trim().trim_matches('"').to_ascii_lowercase();
        if crate::hosts::is_valid_domain(&name) {
            return Some(name);
        }
    }
    None
}

/// Valet's sites, as [`Install`] rows.
///
/// `None` when there is no config at all — an absent `~/.config/valet` is
/// "Valet is not installed", and a row for it would read as a scan that failed.
fn scan_valet(root: &Path, projects: Option<&Path>) -> Option<Install> {
    if !root.join("config.json").is_file() {
        return None;
    }
    let (_, tld) = valet_config(root);

    let mut sites = Vec::new();
    for (name, path) in valet_sites(root) {
        // A link whose target is gone is reported by Valet too and is worth
        // seeing; it is skipped here because there is nothing to copy, and an
        // import row with no bytes behind it is a button that fails.
        if !path.is_dir() {
            continue;
        }
        let (bytes, partial) = measure(&path);
        sites.push(Site {
            // Valet knows the hostname exactly — the site name plus the
            // configured suffix — which is more than XAMPP can say and is why
            // this is `Some` where XAMPP's is `None`.
            domain: Some(format!("{name}.{tld}")),
            taken: projects.is_some_and(|p| p.join(name.to_ascii_lowercase()).exists()),
            path: path.display().to_string(),
            name,
            bytes,
            partial,
            detected: crate::detect::detect(&path),
            services: Vec::new(),
        });
    }

    Some(Install {
        source: Source::Valet,
        path: root.display().to_string(),
        sites,
    })
}

/// Where Valet says its sites are, and what suffix it serves them under.
///
/// `~/.config/valet/config.json` holds `paths` — the directories that were
/// `valet park`ed — and `tld`. Both are read with `serde_json` rather than
/// assumed: the key was `domain` before Valet 3 and is `tld` after, so a build
/// that guessed would silently produce `.test` for somebody serving `.localhost`.
pub fn valet_config(root: &Path) -> (Vec<PathBuf>, String) {
    let mut parked = Vec::new();
    let mut tld = "test".to_string();

    if let Ok(text) = std::fs::read_to_string(root.join("config.json")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(list) = value.get("paths").and_then(|v| v.as_array()) {
                parked.extend(list.iter().filter_map(|v| v.as_str()).map(PathBuf::from));
            }
            // `tld` since Valet 3, `domain` before it. Read in that order so a
            // config carrying both — an upgrade leaves one behind — takes the
            // current one.
            if let Some(value) = value
                .get("tld")
                .or_else(|| value.get("domain"))
                .and_then(|v| v.as_str())
            {
                let value = value.trim().trim_start_matches('.');
                if !value.is_empty() {
                    tld = value.to_ascii_lowercase();
                }
            }
        }
    }
    (parked, tld)
}

/// Every site Valet serves, from both of the ways it can be told about one.
///
/// A **linked** site is a symlink under `Sites/` whose name is the hostname;
/// the target is where the code is. A **parked** directory means every child of
/// it is a site named after its own directory. Reading only one of the two
/// would miss half of somebody's setup, and which half depends on how they
/// happen to work.
///
/// Linked wins on a collision, because that is what Valet does: an explicit
/// link is the thing somebody typed.
pub fn valet_sites(root: &Path) -> Vec<(String, PathBuf)> {
    let (parked, _) = valet_config(root);
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for entry in std::fs::read_dir(root.join("Sites"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // `read_link` rather than `canonicalize`: a link whose target has been
        // deleted still tells us the name Valet serves, and canonicalize would
        // drop the row entirely.
        let Ok(target) = std::fs::read_link(&path) else {
            continue;
        };
        if seen.insert(name.to_string()) {
            out.push((name.to_string(), target));
        }
    }

    for dir in parked {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !is_site(name) {
                continue;
            }
            if seen.insert(name.to_string()) {
                out.push((name.to_string(), path));
            }
        }
    }

    out.sort_by_key(|(name, _)| name.to_ascii_lowercase());
    out
}

/// Is this a site, or part of the tool?
pub fn is_site(name: &str) -> bool {
    !name.starts_with('.') && !NOT_SITES.contains(&name.to_ascii_lowercase().as_str())
}

// ------------------------------------------------------------------- I/O

/// How much of a tree to measure before giving up.
///
/// A size is shown so somebody can decide whether to copy; it does not have to
/// be exact, and walking a 200,000-file `node_modules` to three decimal places
/// on a page that lists eleven sites is time nobody asked for. The cap is
/// reported as [`Site::partial`] rather than hidden — a number that silently
/// stopped counting is worse than no number.
const MAX_ENTRIES: usize = 20_000;

fn measure(dir: &Path) -> (u64, bool) {
    let mut total = 0u64;
    let mut seen = 0usize;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return (total, true);
            }
            match entry.file_type() {
                // Not followed. A symlink into the same tree is a loop, and one
                // pointing at `/` is a walk of the whole disk.
                Ok(kind) if kind.is_symlink() => continue,
                Ok(kind) if kind.is_dir() => stack.push(entry.path()),
                _ => total += entry.metadata().map(|m| m.len()).unwrap_or(0),
            }
        }
    }
    (total, false)
}

/// The sites in one installation.
pub fn scan_at(source: Source, install: &Path, projects: Option<&Path>) -> Option<Install> {
    // Valet keeps no directory of sites, so it takes the other path entirely.
    // Folding it into the loop below would mean a `web_root` that is a lie and
    // a special case in the middle of a walk that is about directories.
    if source == Source::Valet {
        return scan_valet(install, projects);
    }
    // Sail is a project rather than an installation — see the enum.
    if source == Source::Sail {
        return scan_sail(install, projects);
    }

    let web = install.join(source.web_root());
    if !web.is_dir() {
        return None;
    }

    let vhosts = laragon_domains(source, install);
    let mut sites = Vec::new();

    for entry in std::fs::read_dir(&web).into_iter().flatten().flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_site(name) {
            continue;
        }

        let (bytes, partial) = measure(&path);
        sites.push(Site {
            domain: vhosts
                .iter()
                .find(|(site, _)| site.eq_ignore_ascii_case(name))
                .map(|(_, domain)| domain.clone()),
            taken: projects.is_some_and(|p| p.join(name.to_ascii_lowercase()).exists()),
            name: name.to_string(),
            path: path.display().to_string(),
            bytes,
            partial,
            detected: crate::detect::detect(&path),
            services: Vec::new(),
        });
    }

    sites.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    Some(Install {
        source,
        path: install.display().to_string(),
        sites,
    })
}

// ------------------------------------------------------------------- Sail

/// The services a Sail compose file declares, as this app's own service ids.
///
/// Read by line rather than with a YAML parser, and the reasoning is
/// `laragon_domains`': the file is generated by `sail:install` from a fixed
/// template, one fact is wanted from it, and a YAML dependency would be a
/// second thing to be wrong about.
///
/// The indentation is **read from the file** rather than assumed.
/// `xdebug::generated_services` matches exactly two spaces because it reads
/// compose files this app wrote; Sail's template uses four, and a copy of that
/// rule found nothing at all — which the test using the real template is what
/// caught. So the first key inside `services:` sets the depth, and keys at that
/// depth are the service names.
///
/// Sail's names are not all this app's names, and the map is where the value
/// is: somebody with `pgsql` and `mailpit` in their compose file wants
/// `postgres` and `mailpit` switched on here. A service with no counterpart —
/// `selenium`, `soketi` — is left out rather than guessed at, because an
/// import that silently drops something is better than one that silently
/// substitutes.
pub fn sail_services(compose: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_services = false;
    let mut depth: Option<usize> = None;

    for raw in compose.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            // A top-level key ends the block. `volumes:` and `networks:` have
            // indented keys of their own, and `sail` is one of them.
            in_services = line.starts_with("services:");
            depth = None;
            continue;
        }
        if !in_services {
            continue;
        }

        // The first indented line sets what a service key looks like here.
        let at = *depth.get_or_insert(indent);
        if indent != at || line.trim_start().starts_with('-') {
            continue;
        }
        let Some(name) = line.trim().split(':').next().map(str::trim) else {
            continue;
        };

        if let Some(mapped) = sail_service_id(name) {
            if !out.contains(&mapped.to_string()) {
                out.push(mapped.to_string());
            }
        }
    }

    out
}

/// One Sail service name as this app spells it, when it has a counterpart.
fn sail_service_id(name: &str) -> Option<&'static str> {
    Some(match name {
        // `laravel.test` is the application itself — the thing being imported,
        // not a service to switch on beside it.
        "mysql" | "mariadb" => {
            if name == "mariadb" {
                "mariadb"
            } else {
                "mysql"
            }
        }
        "pgsql" => "postgres",
        "mongodb" => "mongo",
        "redis" => "redis",
        "memcached" => "memcached",
        "mailpit" => "mailpit",
        // Sail's older template shipped MailHog under this name, and a project
        // that has not been updated still says it.
        "mailhog" => "mailhog",
        "meilisearch" | "typesense" | "minio" | "selenium" | "soketi" | "laravel.test" => {
            return None
        }
        _ => return None,
    })
}

/// Is this directory a Sail project?
///
/// The compose file has to *name* Sail. A `docker-compose.yml` alone is not
/// evidence — every second PHP project has one — and importing an arbitrary
/// compose project as if it were Sail would produce a manifest describing
/// something nobody wrote.
fn sail_compose(dir: &Path) -> Option<(PathBuf, String)> {
    for name in ["docker-compose.yml", "docker-compose.yaml", "compose.yml"] {
        let path = dir.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if text.contains("laravel/sail") || text.contains("sail-8.") || text.contains("sail-7.") {
            return Some((path, text));
        }
    }
    // A project whose compose file was deleted but whose dependency is still
    // installed is still a Sail project, and this is the second half of the
    // same question rather than a guess.
    dir.join("vendor/laravel/sail")
        .is_dir()
        .then(|| (dir.join("docker-compose.yml"), String::new()))
}

/// A Sail project, or the directory holding several of them.
///
/// Both, because both are what somebody points at: "import this project" and
/// "import from my code folder" are the same intention at two scales, and
/// asking which one they meant is a question the directory itself answers.
fn scan_sail(at: &Path, projects: Option<&Path>) -> Option<Install> {
    let mut sites = Vec::new();

    if let Some(site) = sail_site(at, projects) {
        sites.push(site);
    } else {
        for entry in std::fs::read_dir(at).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Some(site) = sail_site(&path, projects) {
                sites.push(site);
            }
        }
    }

    if sites.is_empty() {
        return None;
    }

    sites.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    Some(Install {
        source: Source::Sail,
        path: at.display().to_string(),
        sites,
    })
}

fn sail_site(dir: &Path, projects: Option<&Path>) -> Option<Site> {
    let (_, compose) = sail_compose(dir)?;
    let name = dir.file_name().and_then(|n| n.to_str())?;
    if !is_site(name) {
        return None;
    }

    let (bytes, partial) = measure(dir);
    Some(Site {
        // `APP_URL` is the site's own answer to "what is this served at", and
        // it is the only source here that has one written down by the user
        // rather than generated. A URL is not a hostname, so it is reduced to
        // one and checked — `http://localhost` is Sail's default and is not a
        // domain worth importing.
        domain: std::fs::read_to_string(dir.join(".env"))
            .ok()
            .and_then(|env| app_url_host(&env)),
        taken: projects.is_some_and(|p| p.join(name.to_ascii_lowercase()).exists()),
        name: name.to_string(),
        path: dir.display().to_string(),
        bytes,
        partial,
        detected: crate::detect::detect(dir),
        services: sail_services(&compose),
    })
}

/// The hostname in `APP_URL`, when it is one worth carrying over.
///
/// `localhost` and `127.0.0.1` are Sail's own defaults and mean "no domain
/// chosen" — importing them would put a name in the manifest that this app
/// would then serve, and it is not the name anybody wanted.
pub fn app_url_host(env: &str) -> Option<String> {
    for line in env.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("APP_URL=") else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        let host = value
            .rsplit("//")
            .next()?
            .split('/')
            .next()?
            .split(':')
            .next()?
            .to_ascii_lowercase();

        if host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() {
            return None;
        }
        return crate::hosts::is_valid_domain(&host).then_some(host);
    }
    None
}

/// `(site directory, hostname)` from Laragon's generated vhosts.
fn laragon_domains(source: Source, install: &Path) -> Vec<(String, String)> {
    if source != Source::Laragon {
        return Vec::new();
    }

    let dir = install.join("etc").join("apache2").join("sites-enabled");
    let mut out = Vec::new();

    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(file) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // `auto.<site>.<tld>.conf` — the site is the first label after `auto.`.
        let Some(rest) = file
            .strip_prefix("auto.")
            .and_then(|r| r.strip_suffix(".conf"))
        else {
            continue;
        };
        let Some(site) = rest.split('.').next().filter(|s| !s.is_empty()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(domain) = server_name(&text) {
            out.push((site.to_string(), domain));
        }
    }

    out
}

/// Every installation this machine has.
pub fn scan(projects: Option<&Path>) -> Vec<Install> {
    let mut out: Vec<Install> = Vec::new();

    for (source, path) in well_known() {
        // The macOS pair — `/Applications/XAMPP` and its `xamppfiles` — would
        // otherwise report the same htdocs twice through two paths.
        if out.iter().any(|found| found.source == source) {
            continue;
        }
        if let Some(install) = scan_at(source, &path, projects) {
            out.push(install);
        }
    }

    out
}

/// Copy a directory tree, refusing to descend into a symlink.
///
/// `fs::copy` per file rather than a shell `cp -r`: this app spawns no shell,
/// and a recursive copy that follows links can walk out of the tree it was
/// given.
pub fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;

    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());

        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --------------------------------------------------------------- Sail

    /// A `docker-compose.yml` as `sail:install` writes one.
    const SAIL_COMPOSE: &str = r#"services:
    laravel.test:
        build:
            context: './vendor/laravel/sail/runtimes/8.4'
            dockerfile: Dockerfile
        image: 'sail-8.4/app'
        ports:
            - '${APP_PORT:-80}:80'
        depends_on:
            - mysql
            - redis
    mysql:
        image: 'mysql/mysql-server:8.0'
        environment:
            MYSQL_ROOT_PASSWORD: '${DB_PASSWORD}'
    redis:
        image: 'redis:alpine'
    meilisearch:
        image: 'getmeili/meilisearch:latest'
    mailpit:
        image: 'axllent/mailpit:latest'
    selenium:
        image: selenium/standalone-chromium
networks:
    sail:
        driver: bridge
volumes:
    sail-mysql:
        driver: local
"#;

    /// The services a Sail file names, as this app spells them — and only the
    /// ones it has a counterpart for.
    #[test]
    fn sail_services_are_read_and_translated() {
        let found = sail_services(SAIL_COMPOSE);

        assert!(found.contains(&"mysql".to_string()), "{found:?}");
        assert!(found.contains(&"redis".to_string()), "{found:?}");
        assert!(found.contains(&"mailpit".to_string()), "{found:?}");

        // The application itself is what is being imported, not a service
        // beside it.
        assert!(!found.contains(&"laravel.test".to_string()));
        // No counterpart in the catalogue: left out rather than substituted.
        for absent in ["meilisearch", "selenium"] {
            assert!(!found.iter().any(|s| s == absent), "{absent} in {found:?}");
        }
        // `networks:` and `volumes:` have two-space keys too, and `sail` is not
        // a service.
        assert!(!found.iter().any(|s| s == "sail"));
    }

    /// Sail's own names are not this app's names, and the map is the value.
    #[test]
    fn sails_names_are_mapped_onto_this_apps_catalogue() {
        assert_eq!(sail_service_id("pgsql"), Some("postgres"));
        assert_eq!(sail_service_id("mongodb"), Some("mongo"));
        assert_eq!(sail_service_id("mariadb"), Some("mariadb"));
        assert_eq!(sail_service_id("mysql"), Some("mysql"));
        // An old template still says mailhog, and that project is still real.
        assert_eq!(sail_service_id("mailhog"), Some("mailhog"));
        assert_eq!(sail_service_id("minio"), None);
        assert_eq!(sail_service_id("something-else"), None);
    }

    /// `APP_URL` is the only domain any of these sources has written down by a
    /// person. Sail's defaults are not one.
    #[test]
    fn an_app_url_becomes_a_domain_only_when_it_is_one() {
        assert_eq!(
            app_url_host("APP_NAME=Shop\nAPP_URL=http://shop.test\n").as_deref(),
            Some("shop.test")
        );
        assert_eq!(
            app_url_host("APP_URL=\"https://Shop.Test:8443/app\"\n").as_deref(),
            Some("shop.test"),
            "the port and the path are not part of the name"
        );

        for default in [
            "APP_URL=http://localhost\n",
            "APP_URL=http://localhost:8080\n",
            "APP_URL=http://127.0.0.1\n",
            "APP_URL=\n",
            "APP_NAME=Shop\n",
        ] {
            assert_eq!(app_url_host(default), None, "{default:?}");
        }
    }

    /// A compose file that is not Sail's is not imported as Sail: every second
    /// PHP project has a `docker-compose.yml`, and reading one as a Sail
    /// project would produce a manifest describing something nobody wrote.
    #[test]
    fn only_a_compose_file_that_names_sail_counts() {
        let dir = std::env::temp_dir().join(format!("stackvo-sail-{}", std::process::id()));
        let project = dir.join("shop");
        std::fs::create_dir_all(&project).unwrap();

        std::fs::write(
            project.join("docker-compose.yml"),
            "services:\n  db:\n    image: postgres\n",
        )
        .unwrap();
        assert!(sail_compose(&project).is_none(), "not a Sail project");
        assert!(scan_at(Source::Sail, &project, None).is_none());

        std::fs::write(project.join("docker-compose.yml"), SAIL_COMPOSE).unwrap();
        let install = scan_at(Source::Sail, &project, None).expect("a Sail project");
        assert_eq!(install.sites.len(), 1);
        assert_eq!(install.sites[0].name, "shop");
        assert!(install.sites[0].services.contains(&"mysql".to_string()));

        // And the directory *holding* projects works too, which is what
        // somebody pointing at their code folder means.
        let outer = scan_at(Source::Sail, &dir, None).expect("a folder of projects");
        assert_eq!(outer.sites.len(), 1);
        assert_eq!(outer.sites[0].name, "shop");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A project whose compose file was deleted but whose dependency is still
    /// there is still a Sail project.
    #[test]
    fn the_dependency_alone_is_enough_to_recognise_one() {
        let dir = std::env::temp_dir().join(format!("stackvo-sail-dep-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("vendor/laravel/sail")).unwrap();

        assert!(sail_compose(&dir).is_some());
        let install = scan_at(Source::Sail, &dir, None).expect("still a Sail project");
        assert!(
            install.sites[0].services.is_empty(),
            "with no file there is nothing to read, and nothing is invented"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every source has to be reachable by the id the front end sends, or the
    /// "point at it yourself" path refuses a tool this module can read — which
    /// is what happened to MAMP and Valet.
    #[test]
    fn every_source_can_be_named_by_the_front_end() {
        for source in [
            Source::Xampp,
            Source::Laragon,
            Source::Mamp,
            Source::Valet,
            Source::Sail,
        ] {
            assert_eq!(Source::from_id(source.as_str()), Some(source));
        }
        assert_eq!(Source::from_id("herd"), None);

        // The two with no installation prefix are the two a scan cannot find.
        assert!(Source::Valet.is_pointed_at());
        assert!(Source::Sail.is_pointed_at());
        assert!(!Source::Xampp.is_pointed_at());
        assert!(!Source::Mamp.is_pointed_at());
    }

    #[test]
    fn the_tools_own_directories_are_not_offered_as_sites() {
        for name in ["dashboard", "webalizer", "img", "XAMPP", ".git"] {
            assert!(!is_site(name), "{name} is not a site");
        }
        for name in ["shop", "my-app", "laravel8"] {
            assert!(is_site(name), "{name} is a site");
        }
    }

    #[test]
    fn a_laragon_vhost_yields_its_server_name() {
        let conf = "\
<VirtualHost *:80>
  DocumentRoot \"C:/laragon/www/shop/public\"
  ServerName shop.test
  ServerAlias *.shop.test
</VirtualHost>
";
        assert_eq!(server_name(conf).as_deref(), Some("shop.test"));
    }

    /// A hostname that is not one is not a domain to adopt at: adoption would
    /// take it, write it into a manifest, and produce a project that resolves
    /// nowhere.
    #[test]
    fn a_vhost_without_a_usable_name_yields_nothing() {
        assert!(server_name("<VirtualHost *:80>\n</VirtualHost>\n").is_none());
        assert!(server_name("ServerName localhost\n").is_none(), "one label");
        assert!(server_name("ServerName \n").is_none());
    }

    /// `ServerAlias` is a second name for the same site and a manifest has one
    /// domain. Reading it would make the choice arbitrary.
    #[test]
    fn an_alias_is_not_mistaken_for_the_name() {
        assert!(server_name("ServerAlias other.test\n").is_none());
    }

    #[test]
    fn scanning_a_directory_that_is_not_an_installation_finds_nothing() {
        let dir = std::env::temp_dir().join(format!("stackvo-imports-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert!(scan_at(Source::Xampp, &dir, None).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Valet (L) --------------------------------------------------------

    /// Named by the caller rather than timestamped — `idle.rs`'s `workspace`
    /// carries the reason. `SystemTime::now().as_nanos()` looks like an
    /// identity and is not one: it is quantised to a microsecond, and parallel
    /// test threads inside the same one get the same directory.
    ///
    /// The name is per *call*, not per test, because
    /// `the_suffix_is_read_from_either_spelling_and_the_current_one_wins` asks
    /// for four of these and each has to be its own Valet installation.
    fn valet_root(name: &str, config: &str, links: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-valet-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Sites")).unwrap();
        std::fs::write(dir.join("config.json"), config).unwrap();
        // `name` is read only by the symlink below, which Windows does not
        // have — the fixture there is the parked directory alone. Found by
        // `tools/linux/run.sh --windows`, where `-D warnings` makes it an error.
        #[cfg_attr(not(unix), allow(unused_variables))]
        for (name, target) in links {
            let target = dir.join(target);
            std::fs::create_dir_all(&target).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, dir.join("Sites").join(name)).unwrap();
        }
        dir
    }

    /// The key was `domain` before Valet 3 and is `tld` after. A build that
    /// guessed would quietly serve `.test` to somebody using `.localhost`.
    #[test]
    fn the_suffix_is_read_from_either_spelling_and_the_current_one_wins() {
        let a = valet_root("suffix-tld", r#"{"tld":"localhost"}"#, &[]);
        assert_eq!(valet_config(&a).1, "localhost");

        let b = valet_root("suffix-domain", r#"{"domain":"dev"}"#, &[]);
        assert_eq!(valet_config(&b).1, "dev");

        // An upgrade leaves the old key behind; the current one must win.
        let c = valet_root("suffix-both", r#"{"domain":"dev","tld":"test"}"#, &[]);
        assert_eq!(valet_config(&c).1, "test");

        // No config at all is Valet's own default.
        let d = valet_root("suffix-none", "{}", &[]);
        assert_eq!(valet_config(&d).1, "test");

        for dir in [a, b, c, d] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn a_leading_dot_in_the_suffix_is_not_repeated_in_the_hostname() {
        let dir = valet_root(
            "leading-dot",
            r#"{"tld":".test"}"#,
            &[("shop", "code/shop")],
        );
        let install = scan_valet(&dir, None).unwrap();
        assert_eq!(install.sites[0].domain.as_deref(), Some("shop.test"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reading only one of Valet's two ways of knowing about a site would miss
    /// half of somebody's setup, and which half depends on how they work.
    #[test]
    fn both_linked_and_parked_sites_are_found() {
        let dir = valet_root(
            "linked-and-parked",
            r#"{"tld":"test"}"#,
            &[("linked", "elsewhere/linked")],
        );
        let parked = dir.join("parked");
        std::fs::create_dir_all(parked.join("shop")).unwrap();
        std::fs::create_dir_all(parked.join("blog")).unwrap();
        std::fs::write(
            dir.join("config.json"),
            format!(r#"{{"tld":"test","paths":["{}"]}}"#, parked.display()),
        )
        .unwrap();

        let names: Vec<String> = valet_sites(&dir).into_iter().map(|(n, _)| n).collect();
        assert!(names.contains(&"linked".to_string()), "{names:?}");
        assert!(names.contains(&"shop".to_string()), "{names:?}");
        assert!(names.contains(&"blog".to_string()), "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Valet does the same: an explicit link is the thing somebody typed.
    #[test]
    fn a_link_wins_over_a_parked_directory_of_the_same_name() {
        let dir = valet_root("link-wins", r#"{"tld":"test"}"#, &[("shop", "linked/shop")]);
        let parked = dir.join("parked");
        std::fs::create_dir_all(parked.join("shop")).unwrap();
        std::fs::write(
            dir.join("config.json"),
            format!(r#"{{"tld":"test","paths":["{}"]}}"#, parked.display()),
        )
        .unwrap();

        let sites = valet_sites(&dir);
        let shop: Vec<_> = sites.iter().filter(|(n, _)| n == "shop").collect();
        assert_eq!(shop.len(), 1, "one row per name");
        assert!(
            shop[0].1.to_string_lossy().contains("linked"),
            "{:?}",
            shop[0].1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No config is "Valet is not installed", and a row for it would read as a
    /// scan that failed.
    #[test]
    fn a_machine_without_valet_yields_no_install_rather_than_an_empty_one() {
        let dir = std::env::temp_dir().join("stackvo-valet-absent");
        assert!(scan_valet(&dir, None).is_none());
    }

    /// Valet knows the hostname exactly, which is more than XAMPP can say.
    #[test]
    fn a_valet_site_arrives_with_its_domain_already_known() {
        let dir = valet_root(
            "domain-known",
            r#"{"tld":"test"}"#,
            &[("shop", "code/shop")],
        );
        let install = scan_valet(&dir, None).unwrap();
        assert_eq!(install.source, Source::Valet);
        assert_eq!(install.sites.len(), 1);
        assert_eq!(install.sites[0].domain.as_deref(), Some("shop.test"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MAMP is XAMPP's shape, and the point of the test is that it went through
    /// the same path rather than gaining one of its own.
    #[test]
    fn mamp_reads_its_htdocs_the_way_xampp_does() {
        assert_eq!(Source::Mamp.web_root(), "htdocs");
        assert_eq!(Source::Valet.web_root(), "");
    }
}
