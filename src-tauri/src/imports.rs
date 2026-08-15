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
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Xampp => "xampp",
            Source::Laragon => "laragon",
            Source::Mamp => "mamp",
            Source::Valet => "valet",
        }
    }

    /// Where the sites live, relative to the installation root.
    fn web_root(self) -> &'static str {
        match self {
            Source::Xampp => "htdocs",
            Source::Laragon => "www",
            Source::Mamp => "htdocs",
            // Valet has none. The field exists for the tools that keep their
            // sites in one directory, and returning something plausible here
            // would send `scan_at` looking for a directory that never exists.
            Source::Valet => "",
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

    fn valet_root(config: &str, links: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-valet-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("Sites")).unwrap();
        std::fs::write(dir.join("config.json"), config).unwrap();
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
        let a = valet_root(r#"{"tld":"localhost"}"#, &[]);
        assert_eq!(valet_config(&a).1, "localhost");

        let b = valet_root(r#"{"domain":"dev"}"#, &[]);
        assert_eq!(valet_config(&b).1, "dev");

        // An upgrade leaves the old key behind; the current one must win.
        let c = valet_root(r#"{"domain":"dev","tld":"test"}"#, &[]);
        assert_eq!(valet_config(&c).1, "test");

        // No config at all is Valet's own default.
        let d = valet_root("{}", &[]);
        assert_eq!(valet_config(&d).1, "test");

        for dir in [a, b, c, d] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn a_leading_dot_in_the_suffix_is_not_repeated_in_the_hostname() {
        let dir = valet_root(r#"{"tld":".test"}"#, &[("shop", "code/shop")]);
        let install = scan_valet(&dir, None).unwrap();
        assert_eq!(install.sites[0].domain.as_deref(), Some("shop.test"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reading only one of Valet's two ways of knowing about a site would miss
    /// half of somebody's setup, and which half depends on how they work.
    #[test]
    fn both_linked_and_parked_sites_are_found() {
        let dir = valet_root(r#"{"tld":"test"}"#, &[("linked", "elsewhere/linked")]);
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
        let dir = valet_root(r#"{"tld":"test"}"#, &[("shop", "linked/shop")]);
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
        let dir = valet_root(r#"{"tld":"test"}"#, &[("shop", "code/shop")]);
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
