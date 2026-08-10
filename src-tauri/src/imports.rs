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
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Xampp => "xampp",
            Source::Laragon => "laragon",
        }
    }

    /// Where the sites live, relative to the installation root.
    fn web_root(self) -> &'static str {
        match self {
            Source::Xampp => "htdocs",
            Source::Laragon => "www",
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
    }
    #[cfg(target_os = "windows")]
    {
        out.push((Source::Xampp, PathBuf::from("C:\\xampp")));
        out.push((Source::Laragon, PathBuf::from("C:\\laragon")));
        out.push((Source::Laragon, PathBuf::from("D:\\laragon")));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        out.push((Source::Xampp, PathBuf::from("/opt/lampp")));
    }

    // Laragon is a Windows product and is not offered elsewhere: listing a path
    // that cannot exist would be a row that is always empty, which reads as a
    // scan that failed rather than as a tool that is not installed.
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
}
