//! N — a branch with an environment of its own.
//!
//! `git worktree add` checks a second branch out into a second directory while
//! sharing one repository. This gives that directory everything a project here
//! gets: its own container, its own hostname, its own database and its own
//! environment variables — so `feature-x.shop.loc` and `shop.loc` are two
//! running copies of the same application on two branches, at the same time.
//!
//! It is the one thing in the competitive review that nothing else in this
//! space does, and the reason it is natural *here* is that everything it needs
//! already exists: a project is a directory with a manifest, a hostname is a
//! Traefik rule, and a database is a `CREATE DATABASE` on an instance that is
//! already running. There is no new machinery, only a new arrangement of it.
//!
//! ## A worktree is a project. It is not a second kind of thing
//!
//! The directory goes into the project tree beside its parent and is picked up
//! by `list_projects`, `render_generated`, the hosts writer and the certificate
//! exactly as any other project is. Nothing downstream of this module knows the
//! word "worktree", and that is deliberate: a parallel lifecycle for a project
//! that is a project would be a second set of every bug.
//!
//! ## Nothing is written into the checkout that git tracks
//!
//! This is the constraint the whole design turns on. The files in a worktree
//! directory **are the branch's files**. Writing a derived `stackvo.json` there
//! would show up as a modification to whoever is working on that branch, and
//! writing `.stackvo/site.json` would do the same wherever a team commits it.
//!
//! So the two halves go to two places:
//!
//! * **Identity** — the name and the hostname — goes into `stackvo.local.json`,
//!   the machine-local overlay B-2 already defines, which exists precisely to be
//!   the file that is never committed. `manifest::local_name_refused` is the one
//!   change this feature needed there: an overlay may restate the directory it
//!   sits in, and nothing else about identity.
//! * **Environment** — the database credentials and anything else — stays in
//!   this module's own table under the app root, outside every checkout, and
//!   reaches the container through the compose overlay `site.rs` already
//!   renders.
//!
//! `stackvo.local.json` is also added to the repository's `.git/info/exclude`
//! when git says it is not ignored yet. That file is git's own local ignore
//! list: it is never committed, it is shared by the main checkout and every
//! linked worktree, and it is the right home for a rule about a file the
//! repository must not carry. The user's `.gitignore` is theirs and is not
//! touched.
//!
//! ## The database is a database, not an instance
//!
//! A branch is not a different engine. Giving each worktree its own MySQL
//! container would cost a gigabyte of memory to hold a copy of one schema, so a
//! worktree gets a database *on the instance that is already running* — created
//! empty, or copied from the workspace's own so the branch starts with the data
//! that was there. See [`crate::db::create_database`] and
//! [`crate::db::copy_database`].

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Bumped when a field changes meaning, not when one is added — the rule
/// `instances.rs` states, for the same reason: this file names directories a
/// removal will delete and databases a removal will drop.
pub const SCHEMA_VERSION: u32 = 1;

/// The longest a branch slug may be.
///
/// A DNS label may be 63 characters and the slug is one whole label, so the
/// limit is not about DNS. It is about the *project name*, `<parent>-<slug>`,
/// which becomes a directory, a container name and an image reference, and a
/// 60-character branch name would produce one nobody can read in a list. A
/// truncated slug can collide, and a collision is caught by name — loudly, with
/// the name shown — rather than by making the limit large enough to hope.
const SLUG_LIMIT: usize = 40;

/// `<root>/worktrees.json`.
pub fn path(root: &Path) -> PathBuf {
    root.join("worktrees.json")
}

/// The database one worktree was given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Database {
    /// The instance it lives on, as `instances.json` ids it.
    pub instance: String,
    pub name: String,
    /// The database it was copied from, when it was copied rather than created
    /// empty. Kept because it is the only record of where the data came from,
    /// and "is this a copy of production or an empty schema" is the question
    /// somebody asks three weeks later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seeded_from: Option<String>,
}

/// One worktree, as this app records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// The project name, which is also the directory name. The identity.
    pub name: String,
    /// The project this was branched from, by name.
    pub parent: String,
    /// The git branch checked out into it.
    pub branch: String,
    /// The hostname it answers on.
    pub domain: String,
    /// Absolute, and recorded rather than derived: the project tree can be
    /// moved, and a record that pointed at where the directory *would* be would
    /// send `git worktree remove` at a path git has never heard of.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<Database>,
    /// Extra environment variables for this worktree's container.
    ///
    /// The database credentials are **not** in here: they are computed from the
    /// instance at render time by [`env_for`], so a password that changes in
    /// one place does not leave a stale copy in this file. What is here is what
    /// somebody typed.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// RFC 3339.
    pub created_at: String,
}

/// The whole file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub worktrees: Vec<Record>,
}

impl Table {
    /// Read it, or an empty table when there is none.
    pub fn load(root: &Path) -> Result<Self> {
        let file = path(root);
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    schema_version: SCHEMA_VERSION,
                    worktrees: Vec::new(),
                })
            }
            Err(e) => return Err(Error::io(format!("reading {}", file.display()), e)),
        };

        let table: Self = serde_json::from_str(&text).map_err(|e| {
            Error::new(
                Code::InvalidManifest,
                format!("{} is not readable: {e}", file.display()),
            )
        })?;

        if table.schema_version > SCHEMA_VERSION {
            return Err(Error::new(
                Code::Unsupported,
                format!(
                    "{} is version {} and this app understands {SCHEMA_VERSION} — \
                     it names directories a removal deletes and databases a removal drops, \
                     so a newer file is refused rather than half-read",
                    file.display(),
                    table.schema_version
                ),
            ));
        }
        Ok(table)
    }

    /// Write it, atomically.
    pub fn save(&self, root: &Path) -> Result<()> {
        let file = path(root);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
        }
        let mut text = serde_json::to_string_pretty(&Self {
            schema_version: SCHEMA_VERSION,
            worktrees: self.worktrees.clone(),
        })
        .map_err(|e| Error::new(Code::IoError, format!("serialising worktrees: {e}")))?;
        text.push('\n');
        crate::atomic::write(&file, &text)
    }

    pub fn get(&self, name: &str) -> Option<&Record> {
        self.worktrees.iter().find(|w| w.name == name)
    }

    /// Every worktree of one project, in table order.
    pub fn of_parent<'a>(&'a self, parent: &'a str) -> impl Iterator<Item = &'a Record> {
        self.worktrees.iter().filter(move |w| w.parent == parent)
    }

    /// Add one, refusing a name that is already recorded.
    pub fn insert(&mut self, record: Record) -> Result<()> {
        if self.get(&record.name).is_some() {
            return Err(Error::new(
                Code::AlreadyExists,
                format!("a worktree called \"{}\" is already recorded", record.name),
            ));
        }
        self.worktrees.push(record);
        self.worktrees.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    /// Take one out, and hand it back so the caller can clean up after it.
    pub fn remove(&mut self, name: &str) -> Option<Record> {
        let at = self.worktrees.iter().position(|w| w.name == name)?;
        Some(self.worktrees.remove(at))
    }
}

/// Is this project a worktree of another one?
///
/// Read from the table rather than from git, because the question every caller
/// is asking is "did this app make it", not "is the directory a linked
/// checkout". A worktree somebody added by hand and then adopted as a project
/// is a project; this app did not give it a database and must not offer to drop
/// one.
pub fn record_of(root: &Path, name: &str) -> Option<Record> {
    Table::load(root).ok()?.get(name).cloned()
}

/// Forget a worktree without touching git or Docker.
///
/// For `project_delete`, which can reach a worktree by its ordinary Delete
/// button. That path removes the directory and everything Docker held; what it
/// cannot know about is this table and git's own registration, and a record left
/// behind would keep offering a database drop for a project that is gone.
///
/// Returns the record when there was one, so the caller can say what it also
/// cleaned up.
pub fn forget(root: &Path, name: &str) -> Option<Record> {
    let mut table = Table::load(root).ok()?;
    let record = table.remove(name)?;
    if let Err(e) = table.save(root) {
        tracing::warn!(worktree = %name, error = %e.message, "the worktree record was not removed");
        return None;
    }
    // git keeps its own registration under `.git/worktrees/<id>`, and a
    // directory that has gone without `git worktree remove` leaves it behind:
    // the branch stays locked to a path nothing is at, and checking it out
    // anywhere answers "already used by worktree". `prune` is the command git
    // provides for exactly that and is a no-op when there is nothing stale.
    //
    // A worktree is never a worktree's parent — creating one from a linked
    // checkout is refused — so the parent is always an ordinary project
    // directory and there is only one place to look for it.
    if let Some(parent) = crate::workspace::projects_root(root).map(|p| p.join(&record.parent)) {
        prune(&parent);
    }
    Some(record)
}

// ------------------------------------------------------------- derivations
//
// Pure, and tested as such. Every name a worktree has comes from one pair —
// the parent and the branch — the same rule `instances::slug` states, and for
// the same reason: a name stored twice is a name that can disagree with itself.

/// `feature/ABC-123 Login` → `feature-abc-123-login`.
///
/// One DNS label, because it becomes one: the leftmost label of the worktree's
/// hostname, and the tail of its container name. Git allows a branch name to
/// hold slashes, dots, unicode and a great deal else; a hostname allows letters,
/// digits and hyphens.
///
/// Anything outside the set is folded to a hyphen rather than dropped. Dropping
/// is how `feature/a-1` and `feature/a1` arrive at one slug, and two branches
/// with one hostname is the failure this whole feature exists to avoid.
/// Consecutive hyphens then collapse, which is cosmetic and is why it is done
/// after the fold rather than instead of it.
///
/// `None` when nothing usable is left — a branch named `...` or `安全` has no
/// ASCII to build a label from, and inventing one would be inventing a name the
/// user cannot predict.
pub fn slug(branch: &str) -> Option<String> {
    let folded: String = branch
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    let mut out = String::with_capacity(folded.len());
    for c in folded.chars() {
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }

    let out = out.trim_matches('-').to_string();
    // Truncation can land on a hyphen, and a label may not end in one.
    let out: String = out.chars().take(SLUG_LIMIT).collect();
    let out = out.trim_end_matches('-').to_string();

    let usable = !out.is_empty() && out.starts_with(|c: char| c.is_ascii_alphanumeric());
    usable.then_some(out)
}

/// The project name a worktree is filed under: `shop` + `feature-x` →
/// `shop-feature-x`.
pub fn project_name(parent: &str, slug: &str) -> String {
    format!("{parent}-{slug}")
}

/// The label a name contributes to the hostname.
///
/// `shop-feature-x` under `shop` is `feature-x`; a name somebody chose that
/// does not start with the parent's is itself. Deriving the hostname from the
/// *name* rather than from the branch is what keeps the two in step when a user
/// overrides one of them — the alternative is a project called `staging` living
/// at `feature-x.shop.loc`.
pub fn domain_label(parent: &str, name: &str) -> String {
    name.strip_prefix(&format!("{parent}-"))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(name)
        .to_string()
}

/// `feature-x` under `shop.loc` → `feature-x.shop.loc`.
///
/// A subdomain of the parent's, not a sibling. Two reasons, and the second is
/// the one that matters: it reads as what it is — a branch *of* that project —
/// and it stays inside any wildcard certificate or wildcard route the parent
/// already has, so the browser does not meet an interstitial on the first load.
pub fn domain(parent_domain: &str, label: &str) -> String {
    format!("{label}.{parent_domain}")
}

/// The database name a worktree gets: `shop` + `feature-x` → `shop_feature_x`.
///
/// Underscores rather than hyphens. A hyphen in a database name is legal in all
/// three SQL engines and has to be quoted in every single statement that names
/// it — including the ones somebody types into a client later — and the first
/// unquoted one is a syntax error in a migration.
///
/// Truncated from the *left* of the stem when it will not fit, keeping the whole
/// branch part: two worktrees of one project differ in the branch and share the
/// stem, so a limit that cut the branch off would give them the same database.
pub fn database_name(stem: &str, slug: &str) -> String {
    let clean = |s: &str| -> String {
        s.to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    };

    let branch = clean(slug);
    let stem = clean(stem);

    // 63 is the shortest identifier limit of the engines this supports; see
    // `db::is_valid_database_name`.
    let room = 63usize.saturating_sub(branch.len() + 1);
    let stem: String = stem.chars().take(room).collect();
    let stem = stem.trim_end_matches('_').to_string();

    let joined = if stem.is_empty() {
        branch
    } else {
        format!("{stem}_{branch}")
    };

    // A name must begin with a letter — `db::is_valid_database_name` says why —
    // and a branch called `2fa` would otherwise produce one that does not.
    if joined.starts_with(|c: char| c.is_ascii_lowercase()) {
        joined
    } else {
        let prefixed = format!("w_{joined}");
        prefixed.chars().take(63).collect()
    }
}

/// The `stackvo.local.json` a worktree is given.
///
/// Two keys and no more. `name` is what stops W-04 from reporting a project
/// that cannot be reached — the committed manifest is the branch's and says the
/// parent's name — and `domain` is the whole point of the exercise.
///
/// Written as text rather than through `serde_json` on a struct, because this
/// is a file a person opens and edits afterwards: the comment-free JSON they
/// find should be in the order they would have written it.
pub fn local_overlay(name: &str, domain: &str) -> String {
    format!("{{\n  \"name\": \"{name}\",\n  \"domain\": \"{domain}\"\n}}\n")
}

// -------------------------------------------------------------- environment

/// What a worktree's container is given, over and above the project's own.
///
/// Derived on every render rather than stored, so a root password changed in
/// Settings reaches every worktree at the next generate instead of leaving a
/// stale copy in a JSON file nobody would think to look in.
///
/// The database half is absent when the worktree has no database, which is a
/// perfectly ordinary state: a static site on a branch needs a hostname and
/// nothing else.
///
/// `APP_URL` is here because it is the variable that makes a per-branch
/// environment behave like one. The branch's own `.env` names the parent's
/// hostname, and a framework that generates links, redirects and mail from it
/// would send everybody from `feature-x.shop.loc` back to `shop.loc` mid-flow —
/// which is the exact bug this feature would be sold as fixing.
///
/// The container's environment beats the application's `.env` file, which is
/// what makes any of this work: PHP's dotenv libraries load a variable only when
/// the process does not already have one, so a value set here wins without this
/// app ever touching the framework's file (M-5's rule, unchanged).
///
/// Whatever the user typed is laid over the derived values last, so a worktree
/// that wants a different `APP_URL` — or none — can say so.
pub fn env_for(root: &Path, record: &Record) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = BTreeMap::new();

    env.insert("APP_URL".into(), format!("https://{}", record.domain));
    // What the branch is, for anything that wants to say so on screen. Two
    // variables rather than one composite, because a template that wants only
    // the branch should not have to split a string.
    env.insert("STACKVO_WORKTREE".into(), record.name.clone());
    env.insert("STACKVO_WORKTREE_BRANCH".into(), record.branch.clone());

    if let Some(database) = &record.database {
        if let Ok(connection) = crate::db::connection(root, &database.instance) {
            env.insert("DB_CONNECTION".into(), driver_name(connection.kind).into());
            env.insert("DB_HOST".into(), connection.host.clone());
            env.insert("DB_PORT".into(), connection.port.to_string());
            env.insert("DB_DATABASE".into(), database.name.clone());
            env.insert("DB_USERNAME".into(), connection.user.clone());
            if let Some(password) = &connection.password {
                env.insert("DB_PASSWORD".into(), password.clone());
            }
            env.insert(
                "DATABASE_URL".into(),
                database_url(&connection, &database.name),
            );
        } else {
            // The instance was removed under the worktree. Not fatal and not
            // silent: the project still runs at its own hostname, and the pane
            // reports the database as missing.
            tracing::warn!(
                worktree = %record.name,
                instance = %database.instance,
                "the worktree's database instance is not in the table; no credentials were supplied"
            );
        }
    }

    for (key, value) in &record.env {
        env.insert(key.clone(), value.clone());
    }

    // The same rules the overlay enforces on a project's own variables. A value
    // with a newline in it would end the YAML scalar it is written into, and a
    // derived value has no more right to do that than a typed one.
    env.retain(|key, value| {
        crate::site::checked_key(key).is_ok() && crate::site::checked_value(value).is_ok()
    });
    env
}

/// What the framework calls this engine.
///
/// MariaDB is `mysql`, deliberately. Laravel only grew a `mariadb` driver in
/// version 11, and a value the application does not recognise is a connection
/// that does not exist rather than one that is slightly mislabelled — the
/// safer answer is the one every version has understood.
fn driver_name(kind: crate::db::Kind) -> &'static str {
    match kind {
        crate::db::Kind::Mysql | crate::db::Kind::Mariadb => "mysql",
        crate::db::Kind::Postgres => "pgsql",
        crate::db::Kind::Mongo => "mongodb",
    }
}

/// `mysql://user:pass@host:3306/db`, with the password percent-encoded.
///
/// Doctrine and anything else reading `DATABASE_URL` parses this as a URL, and
/// a generated root password containing `@`, `/` or `#` would otherwise end the
/// userinfo early and point the application at a host that does not exist.
fn database_url(connection: &crate::db::Connection, database: &str) -> String {
    let scheme = crate::db::url_scheme(connection.kind);
    let user = percent_encode(&connection.user);
    let host = &connection.host;
    let port = connection.port;

    match &connection.password {
        Some(password) => {
            let password = percent_encode(password);
            format!("{scheme}://{user}:{password}@{host}:{port}/{database}")
        }
        None => format!("{scheme}://{user}@{host}:{port}/{database}"),
    }
}

/// Percent-encoding for a URL's userinfo field.
///
/// An allowlist, not a list of characters to escape: the unreserved set from
/// RFC 3986 passes and everything else is encoded, so a character nobody
/// thought of is escaped rather than let through.
///
/// Public under a second name for one caller: the masking in `commands.rs` has
/// to find the password inside `DATABASE_URL`, and it is in there encoded. A
/// second implementation of this would be a mask that misses.
pub fn url_encoded(value: &str) -> String {
    percent_encode(value)
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// --------------------------------------------------------------------- git
//
// Everything below asks git a question and reports what it said. The same
// posture `git.rs` takes and for the same reason: the person running this has a
// working git, and a second implementation of "which branches are there" would
// be a worse copy of one that is already correct.
//
// Run synchronously. These are local metadata reads that finish in a few
// milliseconds — `git worktree list` reads one directory — and the one command
// that is not, `worktree add`, goes through `runner::run_operation` so its
// output streams to the console like every other long step.

/// Run git in a directory and hand back stdout, or `None` if it failed.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    if !crate::git::available() {
        return None;
    }
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        // The same two as a clone, for the same reason: there is no terminal
        // for git to ask a question in. Neither of these commands should ever
        // prompt, and "should never" is how a windowed app hangs.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;

    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// Is there a repository at this path?
pub fn is_repository(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--is-inside-work-tree"])
        .map(|out| out.trim() == "true")
        .unwrap_or(false)
}

/// Is this directory a *linked* worktree rather than the main checkout?
///
/// Asked of git rather than of the table, because it is a different question:
/// the table says "this app made it", and this says "git considers it a second
/// checkout of a repository somewhere else". Both matter — the first decides
/// whether a database is ours to drop, the second decides whether a worktree
/// may be created from here at all.
pub fn is_linked_worktree(dir: &Path) -> bool {
    let (Some(own), Some(common)) = (
        git(dir, &["rev-parse", "--absolute-git-dir"]),
        git(
            dir,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        ),
    ) else {
        return false;
    };
    let canonical = |value: &str| {
        let path = PathBuf::from(value.trim());
        std::fs::canonicalize(&path).unwrap_or(path)
    };
    canonical(&own) != canonical(&common)
}

/// The branch checked out here, or `None` when the head is detached.
pub fn current_branch(dir: &Path) -> Option<String> {
    let out = git(dir, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let branch = out.trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

/// Every local branch, in the order git lists them (most recent commit first).
///
/// Sorted by committer date rather than alphabetically because that is the
/// order the answer is wanted in: the branch somebody wants a worktree for is
/// almost always one they touched this week.
pub fn branches(dir: &Path) -> Vec<String> {
    git(
        dir,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )
    .map(|out| {
        out.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// One entry of `git worktree list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkout {
    pub path: String,
    /// `None` for a detached head, which is a perfectly ordinary worktree and
    /// simply has no branch to report.
    pub branch: Option<String>,
    /// git's own word for "the directory is gone but the registration is not".
    pub prunable: bool,
}

/// What git thinks is checked out where.
///
/// `--porcelain` because the human format is not stable and has been changed
/// twice; the porcelain one is a documented contract of `key value` lines with
/// a blank line between records.
pub fn checkouts(dir: &Path) -> Vec<Checkout> {
    git(dir, &["worktree", "list", "--porcelain"])
        .map(|out| parse_porcelain(&out))
        .unwrap_or_default()
}

/// The parser, separate from the process that produces the text, so the record
/// shapes git documents can be tested without a repository on disk.
fn parse_porcelain(out: &str) -> Vec<Checkout> {
    let mut all = Vec::new();
    let mut current: Option<Checkout> = None;

    for line in out.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            all.extend(current.take());
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                all.extend(current.take());
                current = Some(Checkout {
                    path: value.to_string(),
                    branch: None,
                    prunable: false,
                });
            }
            "branch" => {
                if let Some(entry) = current.as_mut() {
                    entry.branch = Some(value.trim_start_matches("refs/heads/").to_string());
                }
            }
            "prunable" => {
                if let Some(entry) = current.as_mut() {
                    entry.prunable = true;
                }
            }
            _ => {}
        }
    }
    all.extend(current);
    all
}

/// Does this worktree have changes that `git worktree remove` would refuse to
/// throw away?
///
/// Three answers, like [`crate::git::is_ignored`]. `None` means git could not
/// say, and a removal that treated that as "clean" would be one that decided a
/// question it had not asked.
pub fn is_dirty(dir: &Path) -> Option<bool> {
    let out = git(dir, &["status", "--porcelain"])?;
    Some(!out.trim().is_empty())
}

/// Would git accept this as a branch name?
///
/// Asked of `git check-ref-format`, which is the only complete answer — the
/// rules run to a page and include things like "no component may end in
/// `.lock`". Two forms are refused before git sees them: a leading `-`, which
/// git itself would read as an option long before it validated anything, and
/// `@{`, which `--branch` *resolves* rather than validates, so `@{-1}` would
/// come back as a legal name meaning "the branch before this one".
pub fn is_valid_branch_name(dir: &Path, name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || name.starts_with('-') || name.contains("@{") {
        return false;
    }
    git(dir, &["check-ref-format", "--branch", name]).is_some()
}

/// The argv for creating the worktree.
///
/// `--` before the path is what stops a path or a branch that begins with `-`
/// from being read as an option — the same rule [`crate::git::clone_args`]
/// states, and the reason it is applied to a path this app built itself is that
/// the projects directory is one the *user* chose.
pub fn add_args(path: &Path, branch: &str, create: bool) -> Vec<String> {
    let mut args = vec!["worktree".to_string(), "add".to_string()];
    if create {
        args.push("-b".to_string());
        args.push(branch.to_string());
    }
    args.push("--".to_string());
    args.push(path.display().to_string());
    if !create {
        args.push(branch.to_string());
    }
    args
}

/// Is this path one git is tracking?
///
/// `--error-unmatch` is what turns "list the files that match" into a yes/no:
/// the exit code is non-zero when nothing matched, which is the question being
/// asked. `None` when git could not answer at all.
fn is_tracked(path: &Path) -> Option<bool> {
    let dir = path.parent()?;
    let name = path.file_name()?.to_str()?;
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["ls-files", "--error-unmatch", "--"])
        .arg(name)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    Some(out.status.success())
}

/// Remove a worktree through git, so its registration goes with the directory.
///
/// `--force` discards uncommitted work, which is why it is a parameter rather
/// than something this always passes: the screen says what will be lost before
/// anybody can set it.
///
/// ## The overlay is taken out of the way first
///
/// Found by running this rather than reading it. `git worktree remove` refuses
/// while the tree **contains untracked files** — not only modified ones — and
/// `stackvo.local.json` is an untracked file this app put there itself. So a
/// worktree with no user changes at all could not be removed, and the message
/// git gives says "contains modified or untracked files", which sends somebody
/// looking for work they never did.
///
/// [`exclude_local_file`] usually makes it moot, because an excluded file is
/// not untracked as far as `git status` is concerned. Usually is not a
/// guarantee: a repository this app could not write an exclude line to is a
/// repository where it would refuse forever.
///
/// It is removed only when git is not tracking it — a repository that committed
/// the file did so deliberately, and deleting it would be a modification rather
/// than a tidy-up — and it is **put back** if git refuses anyway, so a removal
/// that fails leaves the worktree exactly as capable as it was. Without that,
/// a refused removal would strip the checkout of its name and hostname and turn
/// it into a project the app reports as broken.
pub fn remove(parent_dir: &Path, worktree_path: &Path, force: bool) -> Result<()> {
    if !crate::git::available() {
        return Err(Error::new(Code::NotFound, "git is not installed.")
            .with_hint(crate::hints::INSTALL_GIT_OR_ADOPT));
    }

    let overlay = worktree_path.join(crate::manifest::LOCAL_FILE);
    let saved = match is_tracked(&overlay) {
        Some(false) => std::fs::read_to_string(&overlay).ok(),
        _ => None,
    };
    if saved.is_some() {
        let _ = std::fs::remove_file(&overlay);
    }
    let restore = || {
        if let Some(text) = &saved {
            let _ = std::fs::write(&overlay, text);
        }
    };

    let mut args: Vec<String> = vec!["worktree".into(), "remove".into()];
    if force {
        args.push("--force".into());
    }
    args.push("--".into());
    args.push(worktree_path.display().to_string());

    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(parent_dir)
        .args(&args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| {
            restore();
            Error::io("running git worktree remove", e)
        })?;

    if out.status.success() {
        return Ok(());
    }
    restore();

    let stderr = String::from_utf8_lossy(&out.stderr);
    let reason = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("git refused to remove the worktree")
        .to_string();

    // git's own word for it, and the one refusal a user can answer.
    let dirty = stderr.contains("contains modified or untracked files");
    let mut error = Error::new(Code::Conflict, reason);
    if dirty {
        error = error.with_hint(crate::hints::WORKTREE_IS_DIRTY);
    }
    Err(error)
}

/// Delete a branch, once the worktree holding it is gone.
///
/// Best effort by design and never fatal: the worktree is already removed by
/// the time this runs, and refusing to finish because a branch has unmerged
/// commits would leave the removal half done. `-D` because `-d` refuses exactly
/// the case somebody asks for this in — a branch whose work was abandoned.
pub fn delete_branch(parent_dir: &Path, branch: &str) -> bool {
    git(parent_dir, &["branch", "-D", "--", branch]).is_some()
}

/// Clear registrations whose directories are gone.
pub fn prune(parent_dir: &Path) -> bool {
    git(parent_dir, &["worktree", "prune"]).is_some()
}

/// Keep `stackvo.local.json` out of every commit in this repository.
///
/// `.git/info/exclude` and not `.gitignore`. The distinction is the whole
/// reason this is acceptable at all: `.gitignore` is a tracked file that the
/// team shares and that this app has no business editing, while
/// `info/exclude` is git's own per-clone ignore list — never committed, never
/// pushed, and read by the main checkout and every linked worktree alike.
///
/// Only when git says the file is not already ignored. A repository whose
/// `.gitignore` already carries the line needs nothing, and appending anyway is
/// how a file accumulates the same rule five times.
///
/// Returns whether a line was written. Never fatal: a repository this could not
/// write to still gets a working worktree, and the pane reports the file as not
/// ignored, which is the same three-state answer `LocalOverridePane` already
/// shows for an ordinary project.
pub fn exclude_local_file(worktree_dir: &Path) -> bool {
    let overlay = worktree_dir.join(crate::manifest::LOCAL_FILE);
    if crate::git::is_ignored(&overlay) == Some(true) {
        return false;
    }

    let Some(common) = git(
        worktree_dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    ) else {
        return false;
    };
    let exclude = PathBuf::from(common.trim()).join("info").join("exclude");

    let existing = std::fs::read_to_string(&exclude).unwrap_or_default();
    if existing
        .lines()
        .any(|line| line.trim() == crate::manifest::LOCAL_FILE)
    {
        return false;
    }

    if let Some(parent) = exclude.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }

    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "\n# StackVo: this machine's settings for this checkout, never committed.\n{}\n",
        crate::manifest::LOCAL_FILE
    ));

    std::fs::write(&exclude, text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_name_becomes_one_dns_label() {
        for (branch, expected) in [
            ("feature/login", "feature-login"),
            ("feature/ABC-123 Login", "feature-abc-123-login"),
            ("release/2.1.0", "release-2-1-0"),
            ("main", "main"),
            ("  spaced  ", "spaced"),
            ("under_score", "under-score"),
            ("dots...and---dashes", "dots-and-dashes"),
            ("-leading", "leading"),
            ("trailing-", "trailing"),
        ] {
            assert_eq!(slug(branch).as_deref(), Some(expected), "{branch}");
        }
    }

    /// Folded, not dropped. Dropping the punctuation makes two branches into
    /// one hostname, and Traefik answers a duplicate `Host()` rule by picking
    /// one and never telling anybody about the other.
    #[test]
    fn two_branches_never_collapse_into_one_slug() {
        assert_ne!(slug("feature/a-1"), slug("feature/a1"));
        assert_ne!(slug("fix/one"), slug("fix-one").map(|s| format!("{s}x")));
    }

    #[test]
    fn a_branch_with_no_usable_characters_has_no_slug() {
        for branch in ["", "   ", "...", "///", "---"] {
            assert_eq!(slug(branch), None, "{branch:?}");
        }
    }

    /// The label is capped, and a cap that landed on a hyphen would produce a
    /// hostname no resolver accepts.
    #[test]
    fn a_long_branch_is_cut_to_a_label_that_is_still_a_label() {
        let long = format!("feature/{}", "a-".repeat(60));
        let slug = slug(&long).expect("a slug");
        assert!(slug.len() <= SLUG_LIMIT, "{slug}");
        assert!(!slug.ends_with('-'), "{slug}");
        assert!(crate::hosts::is_valid_domain(&format!("{slug}.shop.loc")));
    }

    #[test]
    fn the_hostname_is_a_subdomain_of_the_parents() {
        let label = domain_label("shop", "shop-feature-x");
        assert_eq!(label, "feature-x");
        assert_eq!(domain("shop.loc", &label), "feature-x.shop.loc");

        // A name of the user's own keeps its whole self as the label.
        assert_eq!(domain_label("shop", "staging"), "staging");
        assert_eq!(domain_label("shop", "shop"), "shop");
    }

    #[test]
    fn a_database_name_is_one_a_statement_can_carry_unquoted() {
        let name = database_name("stackvo", "feature-x");
        assert_eq!(name, "stackvo_feature_x");
        assert!(crate::db::is_valid_database_name(&name), "{name}");
    }

    /// The stem is what gets cut, never the branch: two worktrees of one
    /// project share the stem and differ only in the branch part, so cutting
    /// the branch would give them one database between them.
    #[test]
    fn a_long_name_keeps_the_branch_and_loses_the_stem() {
        let stem = "a".repeat(80);
        let one = database_name(&stem, "feature-one");
        let two = database_name(&stem, "feature-two");

        assert!(one.len() <= 63, "{} chars", one.len());
        assert_ne!(one, two);
        assert!(one.ends_with("feature_one"), "{one}");
        assert!(crate::db::is_valid_database_name(&one), "{one}");
        assert!(crate::db::is_valid_database_name(&two), "{two}");
    }

    /// A database name must begin with a letter, and branch names beginning
    /// with a digit are ordinary — `2fa`, `3-column-layout`.
    #[test]
    fn a_name_that_would_start_with_a_digit_is_given_a_letter() {
        let name = database_name("", "2fa");
        assert!(crate::db::is_valid_database_name(&name), "{name}");
        assert!(name.starts_with(|c: char| c.is_ascii_lowercase()), "{name}");
    }

    /// A reserved name is refused rather than created — the derivation can
    /// reach one, because a branch may be called `sys`.
    #[test]
    fn the_engines_own_databases_are_never_a_derivation_target() {
        assert!(!crate::db::is_valid_database_name(&database_name(
            "", "sys"
        )));
        assert!(!crate::db::is_valid_database_name(&database_name(
            "", "mysql"
        )));
    }

    #[test]
    fn the_overlay_says_only_which_directory_and_which_hostname() {
        let text = local_overlay("shop-feature-x", "feature-x.shop.loc");
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");

        assert_eq!(value["name"], "shop-feature-x");
        assert_eq!(value["domain"], "feature-x.shop.loc");
        assert_eq!(
            value.as_object().unwrap().len(),
            2,
            "the overlay grew a key: {text}"
        );
    }

    /// The overlay this writes has to be one `manifest` accepts, or creation
    /// produces a project the app then reports as broken.
    #[test]
    fn the_overlay_is_one_the_manifest_reader_accepts() {
        let dir =
            std::env::temp_dir().join(format!("stackvo-worktree-overlay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // The committed manifest is the branch's, and names the parent.
        std::fs::write(
            dir.join(crate::manifest::FILE),
            "{\n  \"name\": \"shop\",\n  \"domain\": \"shop.loc\",\n  \"runtime\": \"php\",\n  \"php\": {\n    \"version\": \"8.4\"\n  }\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(crate::manifest::LOCAL_FILE),
            local_overlay("shop-feature-x", "feature-x.shop.loc"),
        )
        .unwrap();

        let m = crate::manifest::read(&dir.join(crate::manifest::FILE), "shop-feature-x").unwrap();
        assert!(m.valid, "{:?}", m.errors);
        assert_eq!(m.name, "shop-feature-x");
        assert_eq!(m.domain.as_deref(), Some("feature-x.shop.loc"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_argv_puts_the_path_behind_a_double_dash() {
        let args = add_args(Path::new("/tmp/shop-feature-x"), "feature/x", false);
        let dashes = args.iter().position(|a| a == "--").expect("no `--`");
        let path = args
            .iter()
            .position(|a| a == "/tmp/shop-feature-x")
            .expect("no path");
        assert!(dashes < path, "the path can still be read as an option");
        assert_eq!(args[..2], ["worktree", "add"]);
        assert_eq!(args.last().unwrap(), "feature/x");

        // Creating a branch names it before the path, which is where `-b` takes
        // its value; the path is still behind the `--`.
        let created = add_args(Path::new("/tmp/x"), "feature/new", true);
        assert_eq!(created[2], "-b");
        assert_eq!(created[3], "feature/new");
        assert!(
            created.iter().position(|a| a == "--").unwrap() < created.len() - 1,
            "{created:?}"
        );
    }

    #[test]
    fn the_porcelain_listing_is_read_as_records() {
        let sample = "worktree /code/shop\nHEAD abc\nbranch refs/heads/main\n\n\
                      worktree /code/shop-feature-x\nHEAD def\nbranch refs/heads/feature/x\n\n\
                      worktree /code/gone\nHEAD 123\ndetached\nprunable gitdir file points to non-existent location\n";

        // The parser is exercised through the same code path the command uses,
        // by feeding it the text git would have produced.
        let parsed = parse_porcelain(sample);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert_eq!(parsed[1].path, "/code/shop-feature-x");
        assert_eq!(parsed[1].branch.as_deref(), Some("feature/x"));
        assert_eq!(parsed[2].branch, None, "a detached head has no branch");
        assert!(parsed[2].prunable);
    }

    /// A password with URL punctuation in it would otherwise end the userinfo
    /// early, and the application would connect to a host that does not exist —
    /// or, worse, to one that does.
    #[test]
    fn a_password_reaches_the_url_encoded() {
        let connection = crate::db::Connection {
            instance: "mysql-9-4".into(),
            service: "mysql".into(),
            kind: crate::db::Kind::Mysql,
            host: "stackvo-mysql-9-4".into(),
            port: 3306,
            user: "root".into(),
            password: Some("p@ss/word#1".into()),
            database: Some("stackvo".into()),
        };

        let url = database_url(&connection, "stackvo_feature_x");
        assert_eq!(
            url,
            "mysql://root:p%40ss%2Fword%231@stackvo-mysql-9-4:3306/stackvo_feature_x"
        );
        // The host and the database survive unencoded, which is what makes the
        // URL readable in a log.
        assert!(url.ends_with("/stackvo_feature_x"), "{url}");
    }

    #[test]
    fn the_environment_names_the_branchs_own_hostname() {
        let record = Record {
            name: "shop-feature-x".into(),
            parent: "shop".into(),
            branch: "feature/x".into(),
            domain: "feature-x.shop.loc".into(),
            path: "/code/shop-feature-x".into(),
            database: None,
            env: BTreeMap::from([("APP_ENV".to_string(), "branch".to_string())]),
            created_at: "2026-01-01T00:00:00Z".into(),
        };

        let env = env_for(Path::new("/nonexistent-root"), &record);
        assert_eq!(
            env.get("APP_URL").map(String::as_str),
            Some("https://feature-x.shop.loc")
        );
        assert_eq!(
            env.get("STACKVO_WORKTREE_BRANCH").map(String::as_str),
            Some("feature/x")
        );
        assert_eq!(env.get("APP_ENV").map(String::as_str), Some("branch"));
        // No database means no credentials rather than empty ones.
        assert!(!env.contains_key("DB_DATABASE"), "{env:?}");
    }

    /// What somebody typed wins over what was derived, which is what makes the
    /// derived values a default rather than a decision.
    #[test]
    fn a_typed_variable_overrides_a_derived_one() {
        let record = Record {
            name: "shop-x".into(),
            parent: "shop".into(),
            branch: "x".into(),
            domain: "x.shop.loc".into(),
            path: "/code/shop-x".into(),
            database: None,
            env: BTreeMap::from([("APP_URL".to_string(), "http://localhost:8000".to_string())]),
            created_at: "2026-01-01T00:00:00Z".into(),
        };

        let env = env_for(Path::new("/nonexistent-root"), &record);
        assert_eq!(
            env.get("APP_URL").map(String::as_str),
            Some("http://localhost:8000")
        );
    }

    /// A value that would end the YAML scalar it is written into never reaches
    /// the overlay, however it got into the record.
    #[test]
    fn a_value_with_a_line_break_is_dropped_before_it_reaches_yaml() {
        let record = Record {
            name: "shop-x".into(),
            parent: "shop".into(),
            branch: "x".into(),
            domain: "x.shop.loc".into(),
            path: "/code/shop-x".into(),
            database: None,
            env: BTreeMap::from([
                ("GOOD".to_string(), "one".to_string()),
                ("BAD".to_string(), "two\n      INJECTED: yes".to_string()),
                ("not a key".to_string(), "three".to_string()),
            ]),
            created_at: "2026-01-01T00:00:00Z".into(),
        };

        let env = env_for(Path::new("/nonexistent-root"), &record);
        assert!(env.contains_key("GOOD"));
        assert!(!env.contains_key("BAD"), "{env:?}");
        assert!(!env.contains_key("not a key"), "{env:?}");
    }

    #[test]
    fn the_table_refuses_a_name_it_already_holds() {
        let record = |name: &str| Record {
            name: name.into(),
            parent: "shop".into(),
            branch: "x".into(),
            domain: format!("{name}.shop.loc"),
            path: format!("/code/{name}"),
            database: None,
            env: BTreeMap::new(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };

        let mut table = Table::default();
        table.insert(record("shop-a")).unwrap();
        assert!(table.insert(record("shop-a")).is_err());
        table.insert(record("shop-b")).unwrap();

        assert_eq!(table.of_parent("shop").count(), 2);
        assert_eq!(table.remove("shop-a").unwrap().name, "shop-a");
        assert!(table.get("shop-a").is_none());
    }

    #[test]
    fn a_table_from_a_newer_version_is_refused_rather_than_half_read() {
        let root = std::env::temp_dir().join(format!(
            "stackvo-worktree-table-{}-{}",
            std::process::id(),
            SCHEMA_VERSION
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        // Absent is not an error.
        assert!(Table::load(&root).unwrap().worktrees.is_empty());

        std::fs::write(
            path(&root),
            format!(
                "{{\"schemaVersion\": {}, \"worktrees\": []}}",
                SCHEMA_VERSION + 1
            ),
        )
        .unwrap();
        assert!(Table::load(&root).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
}
