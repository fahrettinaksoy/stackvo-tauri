//! A snippet, the application it runs inside, and what came back.
//!
//! F-5 in `docs/durum.md`, and the row that named it also named the objection:
//! `tinker` over the PTY is "honest 90%, but not a workbench". §5.5 held the
//! remaining tenth as a **decision** rather than a task, because
//! [`crate::quickcmd`] had refused this in writing — "an in-app pane would be a
//! second, worse REPL next to the one they already have configured" — and
//! reversing a refusal is not something a commit should do quietly.
//!
//! ## The refusal was right, and this is not what it refused
//!
//! A *line* REPL in a pane is exactly what quickcmd described: a worse `tinker`
//! with no readline, no history file, no colours the user configured, and a
//! terminal one click away that does all of it better. Nothing here argues
//! otherwise, and `tinker` still opens the user's own terminal.
//!
//! What this is instead is the thing a terminal REPL cannot be. A snippet is
//! **twenty lines you edit**: you write a query, run it, change line three,
//! run it again. In `tinker` that is retyping, or scrolling back through a
//! history that holds each line separately. Here the snippet is a text you keep
//! and re-run, which is a different tool that happens to share a language.
//!
//! So the two are not ranked, they are split by what the person is doing:
//!
//! * **Exploring, one line at a time** → `tinker` in your own terminal, which
//!   is where the quickcmd catalogue still sends it.
//! * **Working on a snippet** → here.
//!
//! ## The snippet is one argument, and it never touches a shell
//!
//! Every runner below takes the code as **one argv element** —
//! `php artisan tinker --execute <code>`, `node -e <code>`. There is no shell
//! anywhere on the path, so a snippet containing `; rm -rf ~` is a snippet
//! containing those characters and nothing else. That is the same rule
//! [`crate::hooks`] and [`crate::quickcmd`] state as their security model, and
//! it is the reason this feature does not need a consent gate:
//!
//! * It runs **in the project's own container**, which already runs that
//!   repository's code. `hooks` makes the argument in full.
//! * The code comes from **the person at the keyboard**, typed into this
//!   window — not from a file in a repository somebody cloned. The webview
//!   naming a program is what quickcmd refuses, and it still cannot: the
//!   frontend sends a runner **id** and a body of code, and the id is what
//!   picks the program.
//!
//! There is no `host` runner and there will not be one. A snippet that has to
//! run on the developer's machine is the thing `hooks`' `host` step exists for,
//! approved against a digest first.
//!
//! ## Two tiers, said on screen rather than assumed
//!
//! A snippet is worth far more when the application is booted — `User::count()`
//! means nothing to a bare `php -r`. But a bare runner is not worthless either:
//! it is where you check what a regex does, or what `json_decode` returns. So
//! both are offered and each row says which it is ([`Runner::booted`]). A pane
//! that showed them as the same thing would be a pane that let somebody debug
//! for ten minutes before finding out their models were never loaded.
//!
//! ## What was measured before this was written
//!
//! Every runner's argv was run for real, because "the CLI documents this flag"
//! is how a feature ships broken:
//!
//! | Runner | Measured on | What came back |
//! | --- | --- | --- |
//! | `php artisan tinker --execute` | Laravel 13.25 | the code's own output, nothing else; an exception exits 1 |
//! | `python manage.py shell -c` | Django on Python 3.14 | printed output, exit 0; a raise exits 1 with the traceback |
//! | `bin/rails runner` | Rails 8.1.3 | the code ran; a raise exits 1 |
//! | `wp eval` | `wordpress:cli` | argv accepted — it failed on there being no WordPress at the path, which is the environment and not the form |
//! | `php -r`, `node -e` | this workspace's own project container | 42, and a throw exits 255 and 1 respectively |
//!
//! Two of those measurements changed the design. Laravel's `--execute` **does
//! not print the value of the last expression** the way the interactive REPL
//! does — `2+3;` produces nothing — so the pane says to `dump()`. And a PHP
//! fatal is written to **stdout**, not stderr, so success is read from the exit
//! code and never from "stderr was empty".

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;

/// How long a snippet may run before it is stopped.
///
/// Thirty seconds. Long enough for a query against a development database and
/// for a framework to boot — Laravel's `--execute` spends about a second of it
/// before the first line of anybody's code runs — and short enough that a loop
/// with no exit is a thing you notice rather than a fan that comes on.
pub const TIMEOUT_SECONDS: u64 = 30;

/// The most code one snippet may carry, in bytes.
///
/// 64 KiB is far past any snippet somebody types and far short of a paste that
/// would have to be handled as a file. The limit exists so the failure is a
/// sentence rather than an argv the operating system refuses — `execve` caps a
/// single argument at 128 KiB on Linux, and hitting *that* produces
/// "argument list too long" from a layer that cannot explain itself.
pub const MAX_CODE: usize = 64 * 1024;

/// The most output one run hands back, per stream.
pub const MAX_OUTPUT: usize = 256 * 1024;

/// How many snippets are remembered per project.
pub const HISTORY: usize = 25;

/// Which interpreter a snippet is written for, and what it can see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    /// `php artisan tinker --execute` — the application booted.
    Laravel,
    /// `wp eval` — WordPress loaded.
    WordPress,
    /// `python manage.py shell -c` — the Django app loaded.
    Django,
    /// `bin/rails runner` — the Rails app loaded.
    Rails,
    /// `php -r` — the language, and nothing of the application.
    Php,
    /// `node -e` — likewise.
    Node,
}

/// One runner as the pane sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Runner {
    /// What the frontend sends back. Never a program name.
    pub id: String,
    /// The command as typed, with the snippet left off — so what runs and what
    /// is shown cannot drift, the same rule [`crate::quickcmd::Spec`] follows.
    pub display: String,
    /// For the editor: `php`, `python`, `ruby`, `javascript`.
    pub language: String,
    /// Is the application booted for this snippet?
    pub booted: bool,
    pub about: String,
    /// The file that made this runner an offer, so an unexpected list can be
    /// explained without guessing.
    pub because: String,
}

impl Flavour {
    pub fn id(self) -> &'static str {
        match self {
            Flavour::Laravel => "laravel",
            Flavour::WordPress => "wordpress",
            Flavour::Django => "django",
            Flavour::Rails => "rails",
            Flavour::Php => "php",
            Flavour::Node => "node",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "laravel" => Flavour::Laravel,
            "wordpress" => Flavour::WordPress,
            "django" => Flavour::Django,
            "rails" => Flavour::Rails,
            "php" => Flavour::Php,
            "node" => Flavour::Node,
            _ => return None,
        })
    }

    /// The command, with the snippet left off.
    pub fn display(self) -> &'static str {
        match self {
            Flavour::Laravel => "php artisan tinker --execute",
            Flavour::WordPress => "wp eval",
            Flavour::Django => "python manage.py shell -c",
            Flavour::Rails => "bin/rails runner",
            Flavour::Php => "php -r",
            Flavour::Node => "node -e",
        }
    }

    pub fn language(self) -> &'static str {
        match self {
            Flavour::Laravel | Flavour::WordPress | Flavour::Php => "php",
            Flavour::Django => "python",
            Flavour::Rails => "ruby",
            Flavour::Node => "javascript",
        }
    }

    /// Does the snippet run with the application loaded?
    pub fn booted(self) -> bool {
        !matches!(self, Flavour::Php | Flavour::Node)
    }

    fn about(self) -> &'static str {
        match self {
            Flavour::Laravel => "Your models, config and container, as the app sees them.",
            Flavour::WordPress => "WordPress loaded — posts, options, the lot.",
            Flavour::Django => "The Django app, with its models imported.",
            Flavour::Rails => "The Rails app, booted.",
            Flavour::Php => "PHP on its own. No application, no autoloader.",
            Flavour::Node => "Node on its own. No application, no modules of yours.",
        }
    }

    /// The file whose presence offered this runner.
    fn because(self) -> &'static str {
        match self {
            Flavour::Laravel => "artisan",
            Flavour::WordPress => "wp-config.php",
            Flavour::Django => "manage.py",
            Flavour::Rails => "bin/rails",
            Flavour::Php => "composer.json",
            Flavour::Node => "package.json",
        }
    }

    /// The argv this runner is, with `code` as its **last element**.
    ///
    /// Last on purpose and everywhere: it is one rule to check when reading
    /// this, and it means no flag can ever be built out of something the person
    /// typed. `--allow-root` sits before the snippet for that reason rather
    /// than after it, where the existing `wp shell` entry puts it — wp-cli
    /// accepts a global flag in either position, measured on `wordpress:cli`.
    pub fn argv(self, code: &str) -> Vec<String> {
        let mut argv: Vec<String> = match self {
            Flavour::Laravel => ["php", "artisan", "tinker", "--execute"],
            Flavour::WordPress => ["wp", "eval", "--allow-root", ""],
            Flavour::Django => ["python", "manage.py", "shell", "-c"],
            Flavour::Rails => ["bin/rails", "runner", "", ""],
            Flavour::Php => ["php", "-r", "", ""],
            Flavour::Node => ["node", "-e", "", ""],
        }
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect();
        argv.push(code.to_string());
        argv
    }
}

/// What one run produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub runner: String,
    /// The command as typed, snippet excluded — what the reader should compare
    /// the output against.
    pub display: String,
    pub stdout: String,
    pub stderr: String,
    /// `None` when the process was killed rather than exiting.
    pub exit_code: Option<i32>,
    /// Wall clock, in milliseconds.
    pub ms: u64,
    /// Did it hit [`TIMEOUT_SECONDS`]?
    pub timed_out: bool,
    /// Either stream cut at [`MAX_OUTPUT`].
    pub truncated: bool,
    /// Was the limit enforced **inside** the container?
    ///
    /// See [`timeout_argv`]. False means the snippet may still be running in
    /// there after this app stopped waiting, and the pane says so rather than
    /// letting a quiet leak look like a clean stop.
    pub limited: bool,
}

/// One remembered snippet.
///
/// The code, never the output. A snippet is what the person wrote and is the
/// thing they want back; the output is the **application's data** — a row, a
/// customer, a token — and this module's rule is the one
/// [`crate::querylog`] states: what came out of somebody's database is not
/// written to disk by this app.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    /// Seconds since the epoch.
    pub at: u64,
    pub runner: String,
    pub code: String,
}

// -------------------------------------------------------------- pure logic

/// The runners this project has the files for, booted ones first.
///
/// Driven off the same [`crate::detect::Fingerprint`] adoption and the quick
/// commands use, so "does this project have artisan" is answered in one place.
///
/// Laravel needs `laravel/tinker` in `composer.json` as well as `artisan`:
/// `--execute` is Tinker's flag, and a Laravel installed with `--no-dev` does
/// not have it. Offering it there would mean a button whose failure is
/// `Command "tinker" is not defined` — which reads as a broken application
/// rather than as a package that is not installed. The bare `php -r` row is
/// still offered, so such a project is not left with nothing.
pub fn runners(print: &crate::detect::Fingerprint) -> Vec<Runner> {
    let mut out = Vec::new();
    let mut add = |flavour: Flavour| {
        out.push(Runner {
            id: flavour.id().to_string(),
            display: flavour.display().to_string(),
            language: flavour.language().to_string(),
            booted: flavour.booted(),
            about: flavour.about().to_string(),
            because: flavour.because().to_string(),
        });
    };

    if print.artisan && has(&print.composer_requires, "laravel/tinker") {
        add(Flavour::Laravel);
    }
    if print.wp_config {
        add(Flavour::WordPress);
    }
    if print.manage_py {
        add(Flavour::Django);
    }
    if print.bin_rails {
        add(Flavour::Rails);
    }
    // The bare pair last, because a booted runner is what somebody came for and
    // a list that opened on `php -r` would bury it.
    if print.composer_json {
        add(Flavour::Php);
    }
    if print.package_json {
        add(Flavour::Node);
    }
    out
}

fn has(list: &[String], package: &str) -> bool {
    list.iter().any(|name| name == package)
}

/// Refuse a snippet that cannot be run before anything is spawned.
///
/// Empty is refused rather than run: `php -r ''` exits 0 with no output, which
/// on screen is indistinguishable from a snippet that worked and printed
/// nothing.
pub fn check(code: &str) -> Result<()> {
    if code.trim().is_empty() {
        return Err(Error::new(Code::InvalidInput, "there is nothing to run"));
    }
    if code.len() > MAX_CODE {
        return Err(Error::new(
            Code::InvalidInput,
            format!(
                "a snippet is at most {} KiB; this one is {} KiB",
                MAX_CODE / 1024,
                code.len() / 1024
            ),
        ));
    }
    Ok(())
}

/// The argv for `docker exec`, with the in-container limit in front.
///
/// `timeout` rather than only this process's own clock, and the difference is
/// not academic: killing a `docker exec` **client** does not stop what it
/// started. Without this, a snippet with a loop in it keeps a CPU busy inside
/// somebody's container after the pane has said "timed out", and nothing in the
/// app would ever mention it again.
///
/// Measured before it was relied on: `timeout` is present in `php:8.4-cli`,
/// `node:22-alpine`, `python:3-slim`, `ruby:3-slim`, `wordpress:cli` and in this
/// workspace's own project container, where `timeout 1 sleep 3` exits 124. It
/// is in coreutils and in busybox, which is every base image this app's
/// packages build on — but "every image I checked" is not "every image", so
/// [`run`] falls back and [`Run::limited`] records which happened.
pub fn timeout_argv(container: &str, flavour: Flavour, code: &str, limit: bool) -> Vec<String> {
    let mut argv = vec!["exec".to_string(), container.to_string()];
    if limit {
        argv.push("timeout".to_string());
        argv.push(TIMEOUT_SECONDS.to_string());
    }
    argv.extend(flavour.argv(code));
    argv
}

/// Did the engine refuse because `timeout` is not in that image?
///
/// Matched on the message rather than on the exit status because Docker's own
/// exit code for it — 126 — is also what a container's program returns when it
/// cannot execute something itself, and retrying a snippet that failed for its
/// own reasons would run somebody's code twice.
fn missing_timeout(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("executable file not found") && lower.contains("timeout")
}

/// Cut a stream to [`MAX_OUTPUT`], keeping the **end**.
///
/// The end rather than the beginning: a stack trace's useful half is its last
/// lines, and a snippet that printed a hundred thousand rows is one whose
/// interesting part is where it stopped.
fn cut(text: &str) -> (String, bool) {
    if text.len() <= MAX_OUTPUT {
        return (text.to_string(), false);
    }
    let start = text.len() - MAX_OUTPUT;
    // Never split a character in half.
    let start = (start..text.len())
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(text.len());
    (text[start..].to_string(), true)
}

/// Put one snippet at the front, drop a duplicate, and cap the list.
///
/// A re-run does not add a second row — it moves the one that is there back to
/// the top. A history whose first ten entries are the same snippet is one
/// nobody scrolls.
pub fn remember(history: &mut Vec<Snippet>, entry: Snippet) {
    history.retain(|old| !(old.code == entry.code && old.runner == entry.runner));
    history.insert(0, entry);
    history.truncate(HISTORY);
}

// ------------------------------------------------------------------- I/O

/// Where the remembered snippets live.
///
/// Beside `hook-consent.json` and `stats-history.json` in the app's own config
/// directory, and deliberately **not** in the project: a file written into
/// somebody's checkout is a file that turns up in their `git status`, which is
/// the rule [`crate::worktree`] follows for the same reason.
pub fn history_path() -> Option<std::path::PathBuf> {
    crate::appdir::config().map(|dir| dir.join("repl-snippets.json"))
}

/// The shape written today. Stamped so a later change has somewhere to branch.
const SCHEMA_VERSION: u64 = 1;

#[derive(Serialize, Deserialize, Default)]
struct Stored {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    /// Keyed by project name.
    projects: std::collections::BTreeMap<String, Vec<Snippet>>,
}

fn load_all() -> Stored {
    let Some(path) = history_path() else {
        return Stored::default();
    };
    // No file is a first run, which is the one case that must stay silent.
    let Ok(text) = std::fs::read_to_string(path) else {
        return Stored::default();
    };
    let Ok(stored) = serde_json::from_str::<Stored>(&text) else {
        return Stored::default();
    };
    // An unknown version is not readable by definition — the field exists to
    // say so, and guessing is how a half-read file becomes a list nobody can
    // explain.
    if stored.schema_version != SCHEMA_VERSION {
        return Stored::default();
    }
    stored
}

fn save_all(stored: &Stored) {
    let Some(path) = history_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(stored) {
        // A history that could not be written is not worth failing a run over:
        // the person got their output, which is what they asked for.
        let _ = crate::atomic::write(&path, &text);
    }
}

/// What this project has run before, newest first.
pub fn history(project: &str) -> Vec<Snippet> {
    load_all().projects.remove(project).unwrap_or_default()
}

/// Forget this project's snippets.
pub fn forget(project: &str) {
    let mut stored = load_all();
    stored.schema_version = SCHEMA_VERSION;
    stored.projects.remove(project);
    save_all(&stored);
}

fn record(project: &str, entry: Snippet) {
    let mut stored = load_all();
    stored.schema_version = SCHEMA_VERSION;
    let list = stored.projects.entry(project.to_string()).or_default();
    remember(list, entry);
    save_all(&stored);
}

/// The runners this project can offer right now.
pub fn for_project(root: &Path, name: &str) -> Result<Vec<Runner>> {
    let dir = crate::workspace::project_dir(root, name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    Ok(runners(&crate::detect::fingerprint(&dir)))
}

/// Run one snippet in the project's container and hand back everything about it.
///
/// The runner is looked up by id here, which is what keeps the frontend from
/// naming a program: `laravel` picks `php artisan tinker --execute`, and an id
/// that is not in [`Flavour::from_id`] is refused before anything is spawned.
pub async fn run(root: &Path, project: &str, runner: &str, code: &str) -> Result<Run> {
    let dir = crate::workspace::project_dir(root, project)?;
    let Some(flavour) = Flavour::from_id(runner) else {
        return Err(Error::new(
            Code::InvalidInput,
            format!("{runner} is not a runner this app knows"),
        ));
    };
    // Offered *for this project*, not merely known: a `django` id sent at a
    // Laravel project would otherwise spawn `python manage.py` in a container
    // that has neither, and the failure would arrive as a shell's "not found".
    if !runners(&crate::detect::fingerprint(&dir))
        .iter()
        .any(|offered| offered.id == runner)
    {
        return Err(Error::new(
            Code::Unsupported,
            format!("{project} has nothing for the {runner} runner to load"),
        )
        .with_hint(crate::hints::REPL_RUNNER_NEEDS_FILES));
    }
    check(code)?;

    let container = crate::engine::container_name(project);
    let running = crate::engine::inspect(project)
        .await
        .map(|details| details.running)
        .unwrap_or(false);
    if !running {
        return Err(
            Error::new(Code::Conflict, format!("{project} is not running"))
                .with_hint(crate::hints::START_PROJECT_FOR_COMMANDS),
        );
    }

    let started = std::time::Instant::now();
    let mut limited = true;
    let mut output = exec(&timeout_argv(&container, flavour, code, limited)).await?;

    // The one image without `timeout` in it should not be an image where this
    // feature is broken.
    if missing_timeout(&String::from_utf8_lossy(&output.stderr)) {
        limited = false;
        output = exec(&timeout_argv(&container, flavour, code, limited)).await?;
    }

    let exit_code = output.status.code();
    // 124 is what `timeout` exits with when it fired. Reported as a timeout
    // rather than as an exit code, because the snippet did not choose it.
    let timed_out = limited && exit_code == Some(124);

    let (stdout, cut_out) = cut(&String::from_utf8_lossy(&output.stdout));
    let (stderr, cut_err) = cut(&String::from_utf8_lossy(&output.stderr));

    record(
        project,
        Snippet {
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default(),
            runner: runner.to_string(),
            code: code.to_string(),
        },
    );

    Ok(Run {
        runner: runner.to_string(),
        display: flavour.display().to_string(),
        stdout,
        stderr,
        exit_code,
        ms: started.elapsed().as_millis() as u64,
        timed_out,
        truncated: cut_out || cut_err,
        limited,
    })
}

/// Spawn `docker` and wait, with this process's own clock as the second limit.
///
/// Two limits rather than one, and they guard different failures: `timeout`
/// inside the container stops the snippet, and this one stops **the wait** —
/// a `docker exec` that never returns because the daemon is wedged is not
/// something an in-container limit can do anything about. The outer one is
/// deliberately looser, so the inner one is what normally fires and the pane
/// can say which.
async fn exec(argv: &[String]) -> Result<std::process::Output> {
    let mut command = tokio::process::Command::new("docker");
    command.args(argv);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let child = command
        .spawn()
        .map_err(|e| Error::io("running docker exec", e))?;

    match tokio::time::timeout(
        std::time::Duration::from_secs(TIMEOUT_SECONDS + 15),
        child.wait_with_output(),
    )
    .await
    {
        Ok(result) => result.map_err(|e| Error::io("reading the snippet's output", e)),
        Err(_) => Err(Error::new(
            Code::IoError,
            "the engine did not answer while the snippet was running",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print_with(edit: impl Fn(&mut crate::detect::Fingerprint)) -> crate::detect::Fingerprint {
        let mut print = crate::detect::Fingerprint::default();
        edit(&mut print);
        print
    }

    /// The snippet is the last argument of every runner, and it is one
    /// argument. That is the whole security model — a snippet full of shell
    /// metacharacters is a string, not a second command.
    #[test]
    fn the_snippet_is_one_argument_and_it_is_the_last_one() {
        let nasty = "; rm -rf ~ && echo $(whoami) | tee /tmp/x";
        for flavour in [
            Flavour::Laravel,
            Flavour::WordPress,
            Flavour::Django,
            Flavour::Rails,
            Flavour::Php,
            Flavour::Node,
        ] {
            let argv = flavour.argv(nasty);
            assert_eq!(argv.last().map(String::as_str), Some(nasty), "{flavour:?}");
            assert_eq!(
                argv.iter().filter(|part| part.contains("rm -rf")).count(),
                1,
                "{flavour:?} carries the snippet more than once"
            );
            // And nothing before it was built out of what somebody typed.
            for part in &argv[..argv.len() - 1] {
                assert!(!part.contains(nasty), "{flavour:?}");
            }
        }
    }

    /// The argv forms, exactly as they were measured. A change to one of these
    /// lines is a change somebody has to go and re-run against the real thing.
    #[test]
    fn each_runner_is_the_command_that_was_measured() {
        assert_eq!(
            Flavour::Laravel.argv("echo 1;"),
            ["php", "artisan", "tinker", "--execute", "echo 1;"]
        );
        assert_eq!(
            Flavour::WordPress.argv("echo 1;"),
            ["wp", "eval", "--allow-root", "echo 1;"]
        );
        assert_eq!(
            Flavour::Django.argv("print(1)"),
            ["python", "manage.py", "shell", "-c", "print(1)"]
        );
        assert_eq!(
            Flavour::Rails.argv("puts 1"),
            ["bin/rails", "runner", "puts 1"]
        );
        assert_eq!(Flavour::Php.argv("echo 1;"), ["php", "-r", "echo 1;"]);
        assert_eq!(
            Flavour::Node.argv("console.log(1)"),
            ["node", "-e", "console.log(1)"]
        );

        // And every display line is the same command with the snippet left off,
        // so the pane cannot show one thing and run another.
        for flavour in [Flavour::Laravel, Flavour::Django, Flavour::Php] {
            let argv = flavour.argv("X");
            assert_eq!(argv[..argv.len() - 1].join(" "), flavour.display());
        }
    }

    /// `--execute` is Tinker's, not Laravel's. A project without the package
    /// gets the bare runner rather than a button that fails with "Command
    /// \"tinker\" is not defined".
    #[test]
    fn laravel_is_offered_only_where_tinker_is_installed() {
        let without = runners(&print_with(|p| {
            p.artisan = true;
            p.composer_json = true;
        }));
        assert!(
            !without.iter().any(|r| r.id == "laravel"),
            "{:?}",
            without.iter().map(|r| &r.id).collect::<Vec<_>>()
        );
        // Not left with nothing, though.
        assert!(without.iter().any(|r| r.id == "php"));

        let with = runners(&print_with(|p| {
            p.artisan = true;
            p.composer_json = true;
            p.composer_requires = vec!["laravel/tinker".into()];
        }));
        assert_eq!(with.first().map(|r| r.id.as_str()), Some("laravel"));
    }

    /// A booted runner is what somebody came for; a list that opened on
    /// `php -r` would bury it.
    #[test]
    fn booted_runners_come_first() {
        let list = runners(&print_with(|p| {
            p.composer_json = true;
            p.package_json = true;
            p.manage_py = true;
        }));
        let booted: Vec<bool> = list.iter().map(|r| r.booted).collect();
        assert_eq!(booted, [true, false, false], "{list:?}");
        assert_eq!(list[0].id, "django");
    }

    /// Nothing to run is refused rather than run: `php -r ''` exits 0 with no
    /// output, which on screen is a snippet that worked and printed nothing.
    #[test]
    fn an_empty_snippet_is_refused_by_name() {
        assert_eq!(check("   \n\t ").unwrap_err().code, Code::InvalidInput);
        assert!(check("echo 1;").is_ok());

        let long = "x".repeat(MAX_CODE + 1);
        let err = check(&long).unwrap_err();
        assert_eq!(err.code, Code::InvalidInput);
        assert!(err.message.contains("64 KiB"), "{}", err.message);
    }

    /// The limit goes in front of the runner, inside the container — see
    /// `timeout_argv` for why the app's own clock is not enough.
    #[test]
    fn the_limit_runs_inside_the_container() {
        let argv = timeout_argv("c", Flavour::Php, "echo 1;", true);
        assert_eq!(argv, ["exec", "c", "timeout", "30", "php", "-r", "echo 1;"]);

        // And the fallback is the same command without it, not a different one.
        let bare = timeout_argv("c", Flavour::Php, "echo 1;", false);
        assert_eq!(bare, ["exec", "c", "php", "-r", "echo 1;"]);
    }

    /// The retry exists for one failure and must not fire for any other —
    /// re-running a snippet that failed on its own terms would run somebody's
    /// code twice.
    #[test]
    fn only_a_missing_timeout_is_retried() {
        assert!(missing_timeout(
            "OCI runtime exec failed: exec failed: unable to start container process: \
             exec: \"timeout\": executable file not found in $PATH"
        ));
        assert!(!missing_timeout(
            "PHP Fatal error: Uncaught RuntimeException"
        ));
        // A snippet that shells out to something missing is not this.
        assert!(!missing_timeout(
            "sh: 1: composer: executable file not found"
        ));
    }

    /// A stack trace's useful half is its last lines.
    #[test]
    fn output_is_cut_from_the_front_so_the_end_survives() {
        let text = format!("{}THE-END", "x".repeat(MAX_OUTPUT));
        let (cut_text, was_cut) = cut(&text);
        assert!(was_cut);
        assert!(cut_text.ends_with("THE-END"));
        assert_eq!(cut_text.len(), MAX_OUTPUT);

        let (short, untouched) = cut("hello");
        assert_eq!(short, "hello");
        assert!(!untouched);
    }

    /// Cutting must not split a character in half — a truncated UTF-8 sequence
    /// is a string the frontend renders as a replacement character where the
    /// data had a letter.
    #[test]
    fn cutting_lands_on_a_character_boundary() {
        let text = format!("{}ğüş", "é".repeat(MAX_OUTPUT));
        let (cut_text, _) = cut(&text);
        assert!(cut_text.ends_with("ğüş"));
        assert!(!cut_text.contains('\u{FFFD}'));
    }

    /// Re-running a snippet moves it back to the top rather than filling the
    /// list with copies of itself.
    #[test]
    fn a_re_run_moves_the_snippet_rather_than_duplicating_it() {
        let mut history = Vec::new();
        let entry = |at, code: &str| Snippet {
            at,
            runner: "php".into(),
            code: code.into(),
        };

        remember(&mut history, entry(1, "a"));
        remember(&mut history, entry(2, "b"));
        remember(&mut history, entry(3, "a"));

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].code, "a");
        assert_eq!(history[0].at, 3, "the newest run is the one kept");
        assert_eq!(history[1].code, "b");

        // The same code under a different runner is a different snippet.
        remember(
            &mut history,
            Snippet {
                at: 4,
                runner: "node".into(),
                code: "a".into(),
            },
        );
        assert_eq!(history.len(), 3);
    }

    /// The cap is a cap, and it drops the oldest.
    #[test]
    fn the_history_stops_at_its_limit() {
        let mut history = Vec::new();
        for i in 0..HISTORY + 10 {
            remember(
                &mut history,
                Snippet {
                    at: i as u64,
                    runner: "php".into(),
                    code: format!("snippet {i}"),
                },
            );
        }
        assert_eq!(history.len(), HISTORY);
        assert_eq!(history[0].code, format!("snippet {}", HISTORY + 9));
    }

    /// An id the frontend invents is refused before anything is spawned — the
    /// same rule `quickcmd` states, which is that the webview picks, never
    /// names.
    #[test]
    fn an_unknown_runner_id_is_not_a_program() {
        assert!(Flavour::from_id("php").is_some());
        assert!(Flavour::from_id("bash").is_none());
        assert!(Flavour::from_id("../../bin/sh").is_none());

        // And every id the pane can show round-trips.
        for runner in runners(&print_with(|p| {
            p.artisan = true;
            p.composer_json = true;
            p.package_json = true;
            p.wp_config = true;
            p.manage_py = true;
            p.bin_rails = true;
            p.composer_requires = vec!["laravel/tinker".into()];
        })) {
            assert!(Flavour::from_id(&runner.id).is_some(), "{}", runner.id);
        }
    }
}
