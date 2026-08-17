//! The handful of commands you run in a project every day.
//!
//! P3-19 was "a tinker quick action — the PTY exists, so it is nearly free".
//! It is, and on its own it is also a single button. The thing that is actually
//! worth building is the set it belongs to: `artisan tinker`, `artisan migrate`,
//! `composer install`, `npm install`, `wp shell`. Each of those today means
//! opening a terminal, remembering the container name, and typing
//! `docker exec -it stackvo-<project> …`.
//!
//! ## The catalog is fixed, and that is the security model
//!
//! The frontend sends an **id**, never a command. `run` looks the id up and
//! builds the argv itself; there is no code path by which the webview can name
//! a program to execute. That is the same handle-not-a-path rule
//! [`crate::applog`] uses, for the same reason: a project pane that accepted an
//! arbitrary command string from its own frontend is a remote shell with extra
//! steps.
//!
//! That rule is intact and is **not** what B-4 changed.
//!
//! ## What a project may add, and why it is allowed to (B-4)
//!
//! [`CATALOG`] is eleven commands that most projects have. What it cannot know
//! is the one command *this* project runs every day — `artisan app:reindex`,
//! `npm run codegen`, a `bin/` script somebody wrote last week. So a project
//! declares its own in `stackvo.json`:
//!
//! ```json
//! "commands": {
//!   "reindex": { "exec": ["php", "artisan", "app:reindex"], "about": "Rebuild the search index" }
//! }
//! ```
//!
//! The webview still only ever sends `"reindex"`. What changed is where the
//! **workspace** may declare one — and `docs/durum.md` §5's first question was
//! exactly that distinction: the argument against a webview naming a program is
//! about a surface that runs code it did not choose; a file on disk in the
//! repository is not that surface.
//!
//! ### The container, and nothing else
//!
//! A declared command may only be `exec` — inside the project's own container.
//! There is no `host` form, and its absence is the whole reason this needed no
//! new approval flow. [`crate::hooks`] makes the argument in full: a container
//! already runs the repository's code, so a repository able to run a command in
//! it has gained nothing. A **host** step is what turns `git clone` plus a
//! button into arbitrary code execution, and that one has a consent record
//! keyed to a digest.
//!
//! So B-4 stops at the container line. Reaching past it is `hooks`' `host`
//! step, which already exists, already asks, and is a different decision than
//! the one that was taken here.
//!
//! ### An id may not be taken twice
//!
//! A declared command whose id is already in [`CATALOG`] is **refused**, and
//! reported as a manifest problem rather than silently winning or silently
//! losing. Either of those is the same failure: somebody presses a button
//! labelled `migrate` believing it is `php artisan migrate`.
//!
//! Every command is spawned as an argv array — never through a shell — so a
//! project called `a; rm -rf ~` is a container name that does not exist rather
//! than a second command.
//!
//! ## Two kinds, because they behave differently
//!
//! * **Interactive** (`tinker`, `wp shell`) needs a TTY and a human. It opens
//!   the user's own terminal, the same way the existing container-shell button
//!   does — an in-app pane would be a second, worse REPL next to the one they
//!   already have configured.
//! * **One-shot** (`migrate`, `composer install`) prints and exits. It runs
//!   through the operation console, streamed, which is where every other
//!   long-running thing in this app already reports.
//!
//! ## What is deliberately absent
//!
//! `migrate:fresh`, `db:wipe` and `composer update` are not here.
//! The first two drop the user's data behind a button whose label is four
//! characters different from the safe one, and the third rewrites a lock file —
//! all three are things to type deliberately, with the terminal button that is
//! one click away, not to offer next to `cache:clear`.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::path::Path;

/// What a project has to have for a command to be offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// `artisan` in the project root.
    Artisan,
    Composer,
    PackageJson,
    WpConfig,
    /// `bin/console` in the project root — Symfony.
    BinConsole,
    /// `manage.py` — Django, and nothing else, puts one at the root.
    ManagePy,
    /// `bin/rails` — Rails rather than merely Ruby.
    BinRails,
    /// `Gemfile` — any Ruby project, which is all `bundle install` needs.
    Gemfile,
}

#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub id: &'static str,
    /// Shown as typed, so what runs and what is displayed cannot drift.
    pub display: &'static str,
    /// argv, run inside the container. Never a shell string.
    pub argv: &'static [&'static str],
    pub needs: Needs,
    /// Interactive commands want a TTY and a human at it.
    pub interactive: bool,
    /// A one-liner on what it does, shown next to the command.
    pub about: &'static str,
}

/// Everything on offer. Adding a row here is the only way to add a command.
pub const CATALOG: &[Spec] = &[
    Spec {
        id: "tinker",
        display: "php artisan tinker",
        argv: &["php", "artisan", "tinker"],
        needs: Needs::Artisan,
        interactive: true,
        about: "A REPL with the application booted.",
    },
    Spec {
        id: "migrate",
        display: "php artisan migrate",
        argv: &["php", "artisan", "migrate", "--force"],
        needs: Needs::Artisan,
        interactive: false,
        // `--force` is not "do it anyway": Laravel refuses to migrate
        // non-interactively when it thinks it is in production, and there is no
        // prompt to answer inside an operation console. Without it the command
        // hangs on a question nobody can see.
        about: "Run pending migrations.",
    },
    Spec {
        id: "migrate-status",
        display: "php artisan migrate:status",
        argv: &["php", "artisan", "migrate:status"],
        needs: Needs::Artisan,
        interactive: false,
        about: "Which migrations have run.",
    },
    Spec {
        id: "optimize-clear",
        display: "php artisan optimize:clear",
        argv: &["php", "artisan", "optimize:clear"],
        needs: Needs::Artisan,
        interactive: false,
        // One command instead of four: it clears config, route, view and event
        // caches together, which is what people actually mean by "clear the
        // cache" and what they otherwise run one at a time until it works.
        about: "Clear every cached config, route and view.",
    },
    Spec {
        id: "route-list",
        display: "php artisan route:list",
        argv: &["php", "artisan", "route:list"],
        needs: Needs::Artisan,
        interactive: false,
        about: "Every registered route.",
    },
    Spec {
        id: "queue-restart",
        display: "php artisan queue:restart",
        argv: &["php", "artisan", "queue:restart"],
        needs: Needs::Artisan,
        interactive: false,
        // Workers hold the old code in memory until they are told to stop.
        // After a deploy or an edit this is the difference between the fix
        // being live and the queue quietly running yesterday's build.
        about: "Tell the queue workers to pick up new code.",
    },
    Spec {
        id: "storage-link",
        display: "php artisan storage:link",
        argv: &["php", "artisan", "storage:link"],
        needs: Needs::Artisan,
        interactive: false,
        about: "Create the public/storage symlink.",
    },
    Spec {
        id: "composer-install",
        display: "composer install",
        argv: &["composer", "install", "--no-interaction"],
        needs: Needs::Composer,
        interactive: false,
        about: "Install PHP dependencies from the lock file.",
    },
    Spec {
        id: "composer-dump",
        display: "composer dump-autoload",
        argv: &["composer", "dump-autoload", "--no-interaction"],
        needs: Needs::Composer,
        interactive: false,
        about: "Rebuild the autoloader after adding a class.",
    },
    Spec {
        id: "npm-install",
        display: "npm install",
        argv: &["npm", "install"],
        needs: Needs::PackageJson,
        interactive: false,
        about: "Install JavaScript dependencies.",
    },
    Spec {
        id: "npm-build",
        display: "npm run build",
        argv: &["npm", "run", "build"],
        needs: Needs::PackageJson,
        interactive: false,
        about: "Build front-end assets.",
    },
    Spec {
        id: "wp-shell",
        display: "wp shell",
        argv: &["wp", "shell", "--allow-root"],
        needs: Needs::WpConfig,
        interactive: true,
        about: "A REPL with WordPress loaded.",
    },
    Spec {
        id: "wp-plugin-list",
        display: "wp plugin list",
        argv: &["wp", "plugin", "list", "--allow-root"],
        needs: Needs::WpConfig,
        interactive: false,
        about: "Installed plugins and their status.",
    },
    // ---------------------------------------------------------- Symfony
    //
    // M-9. Laravel and WordPress had a row here from the start and the other
    // three frameworks this app scaffolds did not, which made "quick commands"
    // read as "Laravel commands". Each of the rows below is the same shape as
    // the ones above — a fixed id, an argv, and a marker file that only that
    // framework writes — so none of it touches the rule that the webview names
    // an id and never a program.
    Spec {
        id: "symfony-cache-clear",
        display: "php bin/console cache:clear",
        argv: &["php", "bin/console", "cache:clear", "--no-interaction"],
        needs: Needs::BinConsole,
        interactive: false,
        // Symfony's cache holds the compiled container, and a service added to
        // a YAML file is invisible until this runs. It is the `optimize:clear`
        // of this framework and the first thing anybody types.
        about: "Rebuild the compiled container and cached config.",
    },
    Spec {
        id: "symfony-router",
        display: "php bin/console debug:router",
        argv: &["php", "bin/console", "debug:router"],
        needs: Needs::BinConsole,
        interactive: false,
        about: "Every registered route.",
    },
    Spec {
        id: "symfony-migrate",
        display: "php bin/console doctrine:migrations:migrate",
        argv: &[
            "php",
            "bin/console",
            "doctrine:migrations:migrate",
            "--no-interaction",
            "--allow-no-migration",
        ],
        needs: Needs::BinConsole,
        interactive: false,
        // `--allow-no-migration` because Doctrine exits non-zero when there is
        // nothing to run, which an operation console reports as a failure —
        // "already up to date" is not an error anybody wants a red line for.
        about: "Run pending Doctrine migrations.",
    },
    Spec {
        id: "symfony-migrate-status",
        display: "php bin/console doctrine:migrations:status",
        argv: &["php", "bin/console", "doctrine:migrations:status"],
        needs: Needs::BinConsole,
        interactive: false,
        about: "Which Doctrine migrations have run.",
    },
    // ---------------------------------------------------------- Django
    Spec {
        id: "django-migrate",
        display: "python manage.py migrate",
        argv: &["python", "manage.py", "migrate", "--noinput"],
        needs: Needs::ManagePy,
        interactive: false,
        about: "Apply pending migrations.",
    },
    Spec {
        id: "django-migrate-status",
        display: "python manage.py showmigrations",
        argv: &["python", "manage.py", "showmigrations"],
        needs: Needs::ManagePy,
        interactive: false,
        about: "Which migrations have run.",
    },
    Spec {
        id: "django-collectstatic",
        display: "python manage.py collectstatic",
        argv: &["python", "manage.py", "collectstatic", "--noinput"],
        needs: Needs::ManagePy,
        interactive: false,
        // `--noinput` answers the "this will overwrite existing files" prompt,
        // which is a hang rather than a question with nobody at the console.
        about: "Gather static files into the served directory.",
    },
    Spec {
        id: "django-shell",
        display: "python manage.py shell",
        argv: &["python", "manage.py", "shell"],
        needs: Needs::ManagePy,
        interactive: true,
        about: "A REPL with the application booted.",
    },
    // ---------------------------------------------------------- Rails
    //
    // Run through `bundle exec` rather than as `bin/rails`, which is what the
    // marker file is: a binstub is only executable if its permission bit
    // survived the checkout, and `docker exec … bin/rails` on one that lost it
    // is a permission error rather than a missing framework. `bundle exec`
    // needs the gem, not the bit.
    Spec {
        id: "rails-migrate",
        display: "bundle exec rails db:migrate",
        argv: &["bundle", "exec", "rails", "db:migrate"],
        needs: Needs::BinRails,
        interactive: false,
        about: "Run pending migrations.",
    },
    Spec {
        id: "rails-migrate-status",
        display: "bundle exec rails db:migrate:status",
        argv: &["bundle", "exec", "rails", "db:migrate:status"],
        needs: Needs::BinRails,
        interactive: false,
        about: "Which migrations have run.",
    },
    Spec {
        id: "rails-routes",
        display: "bundle exec rails routes",
        argv: &["bundle", "exec", "rails", "routes"],
        needs: Needs::BinRails,
        interactive: false,
        about: "Every registered route.",
    },
    Spec {
        id: "rails-console",
        display: "bundle exec rails console",
        argv: &["bundle", "exec", "rails", "console"],
        needs: Needs::BinRails,
        interactive: true,
        about: "A REPL with the application booted.",
    },
    Spec {
        id: "bundle-install",
        display: "bundle install",
        argv: &["bundle", "install"],
        needs: Needs::Gemfile,
        interactive: false,
        // The one Ruby row that asks for a `Gemfile` and no more: installing
        // from a lock file is what every Ruby project does, Rails or not.
        about: "Install Ruby dependencies from the lock file.",
    },
];

/// One command as the UI sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommand {
    pub id: String,
    pub display: String,
    pub about: String,
    pub interactive: bool,
    /// What the offer is based on, so an unexpected list can be explained.
    pub because: String,
    /// Declared by the project rather than compiled in (B-4).
    ///
    /// On screen rather than merely in the data: a row that came out of the
    /// repository somebody cloned is a different kind of thing from one this
    /// application shipped, and the person deciding whether to press it is
    /// entitled to know which they are looking at.
    pub declared: bool,
}

/// A command a project declared in its own `stackvo.json` (B-4).
///
/// Owned rather than `&'static`, which is the one structural difference from
/// [`Spec`] and the reason [`Resolved`] exists: a catalogue entry lives in the
/// binary, and this one lives in a file that can change while the app is open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Declared {
    /// In the order the file had them.
    ///
    /// A list of pairs rather than a map, and the reason is the order: a
    /// `BTreeMap` would alphabetise, so a pane would list somebody's commands
    /// in an order they did not choose and a manifest saved from the form
    /// would come back reordered. `IndexMap` would keep it and is not a direct
    /// dependency here; at 32 entries a linear lookup is not worth one.
    #[serde(flatten)]
    by_id: std::collections::BTreeMap<String, DeclaredCommand>,
    /// The ids, in file order. `by_id` carries the values.
    #[serde(skip)]
    order: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredCommand {
    /// argv, run inside the container. Never a shell string, never a host
    /// process — see the module comment.
    pub argv: Vec<String>,
    pub about: String,
    pub interactive: bool,
}

impl Declared {
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&DeclaredCommand> {
        self.by_id.get(id)
    }

    /// In the order the manifest declared them.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &DeclaredCommand)> {
        self.order
            .iter()
            .filter_map(|id| self.by_id.get_key_value(id.as_str()))
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }
}

/// A command that is about to run, from wherever it came from.
///
/// The two sources meet here and nowhere else, so every caller past this point
/// — the operation runner, the external terminal, the CLI — is written once
/// and cannot treat a declared command differently by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub id: String,
    pub display: String,
    pub argv: Vec<String>,
    pub interactive: bool,
    pub declared: bool,
}

impl From<&'static Spec> for Resolved {
    fn from(spec: &'static Spec) -> Self {
        Self {
            id: spec.id.to_string(),
            display: spec.display.to_string(),
            argv: spec.argv.iter().map(|s| s.to_string()).collect(),
            interactive: spec.interactive,
            declared: false,
        }
    }
}

/// Read `commands` out of a manifest.
///
/// Shaped like [`crate::hooks::parse`] and returning problems the same way,
/// because it is the same job on the same file: what could be understood is
/// kept, what could not is reported by path, and neither stops the manifest
/// from loading. A project with one malformed command still has a name, a
/// domain and a container.
pub fn parse(json: &serde_json::Value) -> (Declared, Vec<crate::hooks::Problem>) {
    use crate::hooks::Problem;

    let mut out = Declared::default();
    let mut problems = Vec::new();

    let Some(block) = json.get("commands") else {
        return (out, problems);
    };
    let Some(map) = block.as_object() else {
        problems.push(Problem {
            path: "commands".into(),
            message: "`commands` must be an object keyed by id".into(),
        });
        return (out, problems);
    };

    for (id, value) in map {
        let at = format!("commands.{id}");
        let bad = |message: String| Problem {
            path: at.clone(),
            message,
        };

        if !is_safe_id(id) {
            problems.push(bad(format!(
                "\"{id}\" is not a usable id — lower-case letters, digits and \
                 dashes, up to 40 characters"
            )));
            continue;
        }

        // The catalogue wins by refusing rather than by overriding. Either
        // silent outcome ends with somebody pressing a button whose label
        // does not describe what it runs.
        if find(id).is_some() {
            problems.push(bad(format!(
                "\"{id}\" is already a built-in command; declared commands \
                 cannot replace one — rename it"
            )));
            continue;
        }

        let Some(object) = value.as_object() else {
            problems.push(bad("a command is an object with `exec`".into()));
            continue;
        };

        // `host` is named in the error rather than ignored: somebody who tried
        // it has a mental model to correct, and the correction is a real
        // feature that exists elsewhere.
        if object.contains_key("host") {
            problems.push(bad(
                "a declared command runs in the project's container; `host` is \
                 not accepted here. A step that has to run on this machine is a \
                 hook, where it is approved against a digest first"
                    .into(),
            ));
            continue;
        }

        let Some(exec) = object.get("exec") else {
            problems.push(bad("a command needs `exec`".into()));
            continue;
        };

        let argv = match argv_of(exec) {
            Ok(argv) => argv,
            Err(message) => {
                problems.push(bad(message));
                continue;
            }
        };

        out.order.push(id.clone());
        out.by_id.insert(
            id.clone(),
            DeclaredCommand {
                about: object
                    .get("about")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                interactive: object
                    .get("interactive")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                argv,
            },
        );
    }

    (out, problems)
}

/// An id that is safe as a map key, a DOM id and an argument.
///
/// Deliberately narrower than "any string": the id travels to the webview and
/// back and is compared against the catalogue, and a value that needs escaping
/// somewhere in that round trip is one that will eventually not be escaped.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// An argv array, or why it is not one.
///
/// A string is refused rather than split on spaces. Splitting is how
/// `["sh", "-c", "a && b"]` becomes four arguments and how a path with a space
/// in it becomes two — and the whole of this module's security model is that a
/// command is an array nobody re-parses.
fn argv_of(value: &serde_json::Value) -> std::result::Result<Vec<String>, String> {
    let Some(list) = value.as_array() else {
        return Err(
            "`exec` is an argv array — [\"php\", \"artisan\", \"app:reindex\"] — never a \
             command string, because nothing here spawns a shell to re-split one"
                .into(),
        );
    };
    if list.is_empty() {
        return Err("`exec` needs at least the program to run".into());
    }
    if list.len() > 32 {
        return Err("`exec` takes at most 32 arguments".into());
    }

    let mut argv = Vec::with_capacity(list.len());
    for item in list {
        match item.as_str() {
            Some(text) if !text.is_empty() => argv.push(text.to_string()),
            Some(_) => return Err("an argument cannot be empty".into()),
            None => return Err("every argument is a string".into()),
        }
    }
    Ok(argv)
}

// -------------------------------------------------------------- pure logic

impl Needs {
    fn marker(self) -> &'static str {
        match self {
            Needs::Artisan => "artisan",
            Needs::Composer => "composer.json",
            Needs::PackageJson => "package.json",
            Needs::WpConfig => "wp-config.php",
            Needs::BinConsole => "bin/console",
            Needs::ManagePy => "manage.py",
            Needs::BinRails => "bin/rails",
            Needs::Gemfile => "Gemfile",
        }
    }

    fn present(self, print: &crate::detect::Fingerprint) -> bool {
        match self {
            Needs::Artisan => print.artisan,
            Needs::Composer => print.composer_json,
            Needs::PackageJson => print.package_json,
            Needs::WpConfig => print.wp_config,
            Needs::BinConsole => print.bin_console,
            Needs::ManagePy => print.manage_py,
            Needs::BinRails => print.bin_rails,
            Needs::Gemfile => print.gemfile,
        }
    }
}

/// The commands this project has the files for.
///
/// Driven off the same [`crate::detect::Fingerprint`] that adoption uses, so
/// "does this project have artisan" is answered in one place. Offering a
/// command the project cannot run is worse than not offering it: the failure
/// arrives as `sh: artisan: not found` in an operation console, which reads as
/// a broken app rather than a button that never applied.
pub fn available(print: &crate::detect::Fingerprint) -> Vec<QuickCommand> {
    CATALOG
        .iter()
        .filter(|spec| spec.needs.present(print))
        .map(|spec| QuickCommand {
            id: spec.id.to_string(),
            display: spec.display.to_string(),
            about: spec.about.to_string(),
            interactive: spec.interactive,
            because: spec.needs.marker().to_string(),
            declared: false,
        })
        .collect()
}

/// Everything on offer: what the files support, then what the project declared.
///
/// Declared commands come **last** and are marked. Two reasons, and neither is
/// cosmetic: the built-in rows are in a fixed order people learn, and a
/// repository that could reorder the list could put its own row where
/// `migrate` usually is.
///
/// No filtering by fingerprint for the declared half. A project that names a
/// command is asserting it can run it, and this application guessing otherwise
/// — hiding `bin/console` because `Needs::BinConsole` looked at the wrong path
/// — would be a button that vanished for a reason nobody could see.
pub fn offered(print: &crate::detect::Fingerprint, declared: &Declared) -> Vec<QuickCommand> {
    let mut out = available(print);
    out.extend(declared.iter().map(|(id, command)| QuickCommand {
        id: id.clone(),
        display: command.argv.join(" "),
        about: command.about.clone(),
        interactive: command.interactive,
        because: "stackvo.json".to_string(),
        declared: true,
    }));
    out
}

pub fn find(id: &str) -> Option<&'static Spec> {
    CATALOG.iter().find(|spec| spec.id == id)
}

/// `docker exec` argv for a command. Interactive adds `-it`.
///
/// Built here rather than at the call site so every caller — the operation
/// runner, the external terminal, the CLI — cannot disagree about what a
/// command is. It takes a [`Resolved`], which is where the built-in and the
/// declared halves have already become one shape: there is no branch below
/// this line that could treat a declared command more freely.
pub fn exec_argv(container: &str, command: &Resolved) -> Vec<String> {
    let mut argv = vec!["exec".to_string()];
    if command.interactive {
        argv.push("-it".to_string());
    }
    argv.push(container.to_string());
    argv.extend(command.argv.iter().cloned());
    argv
}

// ------------------------------------------------------------------- I/O

/// What this project can run right now.
pub fn for_project(root: &Path, name: &str) -> Result<Vec<QuickCommand>> {
    let dir = crate::workspace::project_dir(root, name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    Ok(offered(
        &crate::detect::fingerprint(&dir),
        &declared_for(root, name),
    ))
}

/// The commands this project declares, or none.
///
/// A manifest that will not parse yields an empty set rather than an error: the
/// built-in commands are still perfectly runnable, and losing the whole pane
/// over a typo in one declaration would be the wrong trade. The typo is
/// reported where every other manifest problem is.
///
/// Reads the **effective** manifest, so `stackvo.local.json` can override it —
/// the same rule hooks follow, and the case B-2 exists for.
pub fn declared_for(root: &Path, name: &str) -> Declared {
    crate::workspace::project_dir(root, name)
        .ok()
        .and_then(|dir| crate::manifest::read(&dir.join("stackvo.json"), name).ok())
        .map(|manifest| manifest.commands)
        .unwrap_or_default()
}

/// Resolve an id, refusing anything that is neither built in nor declared.
///
/// The whole point of the id survives B-4: the frontend still cannot name a
/// program. It can pick one that was compiled in, or one the **project's own
/// file** declared — and this function is the only place either becomes an
/// argv.
pub fn resolve(root: &Path, project: &str, id: &str) -> Result<Resolved> {
    resolve_with(&declared_for(root, project), id)
}

/// The same resolution, against a set already in hand.
///
/// Split out so the rule can be tested without a project directory, and so the
/// two callers that already hold the manifest — the pane's listing and the
/// CLI — do not read it twice.
pub fn resolve_with(declared: &Declared, id: &str) -> Result<Resolved> {
    if let Some(spec) = find(id) {
        return Ok(Resolved::from(spec));
    }

    if let Some(command) = declared.get(id) {
        return Ok(Resolved {
            id: id.to_string(),
            display: command.argv.join(" "),
            argv: command.argv.clone(),
            interactive: command.interactive,
            declared: true,
        });
    }

    Err(
        Error::new(Code::NotFound, format!("\"{id}\" is not a known command"))
            .with_hint(crate::hints::QUICK_COMMANDS_ARE_FIXED),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Fingerprint;

    fn laravel() -> Fingerprint {
        Fingerprint {
            artisan: true,
            composer_json: true,
            package_json: true,
            ..Default::default()
        }
    }

    /// The security model in one assertion, and B-4 did not weaken it: an id
    /// that is neither compiled in nor declared by the project resolves to
    /// nothing, so there is still no path from the webview to an arbitrary
    /// `docker exec`.
    #[test]
    fn only_known_ids_resolve() {
        let none = Declared::default();
        assert!(resolve_with(&none, "tinker").is_ok());
        assert!(resolve_with(&none, "rm -rf /").is_err());
        assert!(resolve_with(&none, "").is_err());
        assert!(resolve_with(&none, "../../bin/sh").is_err());
        // Declared by *some* project is not declared by this one.
        assert!(resolve_with(&none, "reindex").is_err());
    }

    // ------------------------------------------- declared commands (B-4)

    fn declared(json: &str) -> (Declared, Vec<crate::hooks::Problem>) {
        parse(&serde_json::from_str(json).expect("the fixture is JSON"))
    }

    #[test]
    fn a_declared_command_is_read_and_offered_after_the_built_in_ones() {
        let (commands, problems) = declared(
            r#"{ "commands": {
                   "reindex": { "exec": ["php","artisan","app:reindex"], "about": "Rebuild it" },
                   "codegen": { "exec": ["npm","run","codegen"] }
                 } }"#,
        );
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(commands.len(), 2);

        let offered = offered(&laravel(), &commands);
        let ids: Vec<&str> = offered.iter().map(|c| c.id.as_str()).collect();

        // The built-in rows keep their order and their place at the top: a
        // repository that could reorder the list could put its own row where
        // `migrate` usually is.
        assert_eq!(ids.first(), Some(&"tinker"));
        assert_eq!(&ids[ids.len() - 2..], ["reindex", "codegen"]);

        let mine = offered.iter().find(|c| c.id == "reindex").unwrap();
        assert!(
            mine.declared,
            "the pane has to be able to say where it came from"
        );
        assert_eq!(mine.because, "stackvo.json");
        assert_eq!(mine.display, "php artisan app:reindex");
        assert!(!offered.iter().find(|c| c.id == "tinker").unwrap().declared);
    }

    /// The line B-4 stops at. A host step is what turns `git clone` plus a
    /// button into arbitrary code execution, and it has a consent record
    /// somewhere else; here it is refused, by name, so the author is told
    /// where the real feature lives.
    #[test]
    fn a_declared_command_may_not_run_on_the_host() {
        let (commands, problems) =
            declared(r#"{ "commands": { "deploy": { "host": ["./deploy.sh"] } } }"#);
        assert!(commands.is_empty(), "nothing may have been accepted");
        assert_eq!(problems.len(), 1);
        assert!(
            problems[0].message.contains("hook"),
            "{}",
            problems[0].message
        );
    }

    /// A command string is refused rather than split. Splitting on spaces is
    /// how `sh -c "a && b"` becomes four arguments and how a path with a space
    /// becomes two — and this module's whole model is an array nobody re-parses.
    #[test]
    fn exec_must_be_an_array_and_is_never_split() {
        let (commands, problems) =
            declared(r#"{ "commands": { "x": { "exec": "php artisan app:reindex" } } }"#);
        assert!(commands.is_empty());
        assert!(
            problems[0].message.contains("argv array"),
            "{}",
            problems[0].message
        );

        let (_, empty) = declared(r#"{ "commands": { "x": { "exec": [] } } }"#);
        assert_eq!(empty.len(), 1);
        let (_, holes) = declared(r#"{ "commands": { "x": { "exec": ["php", ""] } } }"#);
        assert_eq!(holes.len(), 1);
        let (_, typed) = declared(r#"{ "commands": { "x": { "exec": ["php", 7] } } }"#);
        assert_eq!(typed.len(), 1);
    }

    /// Shadowing a built-in is refused rather than resolved either way.
    /// Whichever silent outcome you pick, somebody presses a button labelled
    /// `migrate` believing it is `php artisan migrate`.
    #[test]
    fn a_declared_command_cannot_take_a_built_in_id() {
        let (commands, problems) =
            declared(r#"{ "commands": { "migrate": { "exec": ["true"] } } }"#);
        assert!(commands.is_empty());
        assert!(
            problems[0].message.contains("built-in"),
            "{}",
            problems[0].message
        );

        // And the built-in still resolves to the built-in.
        let resolved = resolve_with(&commands, "migrate").unwrap();
        assert!(!resolved.declared);
        assert_eq!(resolved.argv, ["php", "artisan", "migrate", "--force"]);
    }

    #[test]
    fn an_id_that_would_need_escaping_somewhere_is_refused() {
        for bad in ["Reindex", "re index", "re/index", "../x", "re;index", ""] {
            let json = format!(r#"{{ "commands": {{ "{bad}": {{ "exec": ["true"] }} }} }}"#);
            let (commands, problems) = declared(&json);
            assert!(
                commands.is_empty() && !problems.is_empty(),
                "\"{bad}\" was accepted as an id"
            );
        }
        let (ok, none) = declared(r#"{ "commands": { "app-reindex2": { "exec": ["true"] } } }"#);
        assert!(none.is_empty() && ok.len() == 1);
    }

    /// One bad declaration must not cost the project its other commands, for
    /// the same reason a bad hook does not stop a project opening.
    #[test]
    fn one_unreadable_command_does_not_take_the_others_with_it() {
        let (commands, problems) = declared(
            r#"{ "commands": {
                   "good": { "exec": ["php","-v"] },
                   "bad":  { "exec": "not an array" }
                 } }"#,
        );
        assert_eq!(commands.len(), 1);
        assert!(commands.get("good").is_some());
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].path, "commands.bad");
    }

    /// A declared command reaches `docker exec` the same way a built-in does —
    /// through one function, so there is no branch below it that could treat a
    /// declared command more freely.
    #[test]
    fn a_declared_command_runs_through_the_same_exec_path() {
        let (commands, _) = declared(
            r#"{ "commands": { "shell-ish": { "exec": ["sh","-c","echo hi"], "interactive": true } } }"#,
        );
        let resolved = resolve_with(&commands, "shell-ish").unwrap();
        assert!(resolved.declared && resolved.interactive);

        let argv = exec_argv("stackvo-shop", &resolved);
        assert_eq!(argv[0], "exec");
        assert!(argv.contains(&"-it".to_string()));
        assert!(argv.contains(&"stackvo-shop".to_string()));
        assert!(argv.ends_with(&["sh".to_string(), "-c".to_string(), "echo hi".to_string()]));
        // Never a shell on the host, and never a string: the argv arrives whole.
        assert_eq!(argv.iter().filter(|a| *a == "echo hi").count(), 1);
    }

    #[test]
    fn a_manifest_with_no_commands_block_declares_nothing() {
        let (commands, problems) = declared(r#"{ "name": "shop" }"#);
        assert!(commands.is_empty() && problems.is_empty());

        let (_, wrong) = declared(r#"{ "commands": ["php"] }"#);
        assert_eq!(wrong.len(), 1, "a list is not an object keyed by id");
    }

    /// Offering a command the project cannot run produces `artisan: not found`
    /// in an operation console, which reads as a broken app rather than as a
    /// button that never applied.
    #[test]
    fn commands_are_offered_only_when_their_marker_file_exists() {
        let plain_php = Fingerprint {
            composer_json: true,
            ..Default::default()
        };
        let offered = available(&plain_php);
        let ids: Vec<&str> = offered.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["composer-install", "composer-dump"]);

        let ids: Vec<String> = available(&laravel()).iter().map(|c| c.id.clone()).collect();
        assert!(ids.contains(&"tinker".to_string()));
        assert!(ids.contains(&"npm-install".to_string()));
        assert!(!ids.contains(&"wp-shell".to_string()));

        assert!(available(&Fingerprint::default()).is_empty());
    }

    /// Argv, never a shell string. A project named `a; rm -rf ~` has to be a
    /// container name that does not exist, not a second command.
    #[test]
    fn a_hostile_container_name_is_one_argument() {
        let spec = resolve_with(&Declared::default(), "migrate").unwrap();
        let argv = exec_argv("stackvo-a; rm -rf ~", &spec);

        assert_eq!(argv[0], "exec");
        assert_eq!(argv[1], "stackvo-a; rm -rf ~");
        assert_eq!(&argv[2..], ["php", "artisan", "migrate", "--force"]);
        // No element is a shell invocation.
        assert!(!argv.iter().any(|a| a == "sh" || a == "bash" || a == "-c"));
    }

    /// Only interactive commands get a TTY. `-it` on a one-shot run through the
    /// operation console attaches a terminal nothing is reading, and Docker
    /// refuses `-t` outright when stdin is not a TTY.
    #[test]
    fn only_interactive_commands_ask_for_a_tty() {
        assert!(
            exec_argv("c", &resolve_with(&Declared::default(), "tinker").unwrap())
                .contains(&"-it".to_string())
        );
        assert!(
            !exec_argv("c", &resolve_with(&Declared::default(), "migrate").unwrap())
                .contains(&"-it".to_string())
        );
    }

    /// Laravel refuses to migrate non-interactively when it believes it is in
    /// production, and there is no prompt to answer inside an operation
    /// console — without `--force` the command hangs on a question nobody sees.
    #[test]
    fn non_interactive_commands_carry_their_no_prompt_flag() {
        for id in [
            "migrate",
            "composer-install",
            "composer-dump",
            "symfony-cache-clear",
            "symfony-migrate",
            "django-migrate",
            "django-collectstatic",
        ] {
            let spec = resolve_with(&Declared::default(), id).unwrap();
            assert!(
                spec.argv
                    .iter()
                    .any(|a| *a == "--force" || *a == "--no-interaction" || *a == "--noinput"),
                "{id} can stop for a prompt inside the console"
            );
        }
    }

    /// Data loss is not a button next to `cache:clear`.
    #[test]
    fn destructive_commands_are_not_in_the_catalog() {
        for banned in [
            "migrate:fresh",
            "migrate:reset",
            "db:wipe",
            "update",
            // The same rule read across the frameworks M-9 added: each of these
            // drops the developer's data, and each is one word away from the
            // safe row sitting next to it.
            "db:drop",
            "db:reset",
            "doctrine:schema:drop",
            "doctrine:database:drop",
            "flush",
        ] {
            assert!(
                !CATALOG.iter().any(|s| s.argv.contains(&banned)),
                "{banned} is on offer"
            );
        }
    }

    /// M-9. Each framework's rows appear on its own marker and on no other's —
    /// the failure this prevents is a Symfony button on a Laravel project,
    /// which fails as `Could not open input file: bin/console` after the click.
    #[test]
    fn each_framework_is_offered_on_its_own_marker() {
        let ids = |print: &Fingerprint| -> Vec<String> {
            available(print).iter().map(|c| c.id.clone()).collect()
        };

        let symfony = Fingerprint {
            composer_json: true,
            bin_console: true,
            ..Default::default()
        };
        let offered = ids(&symfony);
        assert!(offered.contains(&"symfony-cache-clear".to_string()));
        assert!(offered.contains(&"composer-install".to_string()));
        assert!(!offered.contains(&"migrate".to_string()));

        let django = Fingerprint {
            manage_py: true,
            python_deps: true,
            ..Default::default()
        };
        let offered = ids(&django);
        assert!(offered.contains(&"django-migrate".to_string()));
        assert!(offered.contains(&"django-shell".to_string()));
        assert!(!offered.contains(&"symfony-router".to_string()));

        let rails = Fingerprint {
            gemfile: true,
            bin_rails: true,
            ..Default::default()
        };
        let offered = ids(&rails);
        assert!(offered.contains(&"rails-migrate".to_string()));
        assert!(offered.contains(&"bundle-install".to_string()));

        // A Gemfile is Sinatra and Jekyll as often as it is Rails, so the
        // framework rows must not follow it — `rails: command not found` is
        // the outcome this marker split exists to prevent.
        let sinatra = Fingerprint {
            gemfile: true,
            ..Default::default()
        };
        assert_eq!(ids(&sinatra), ["bundle-install"]);
    }

    /// Every row a person can click either says what it is running or is
    /// pointed at a framework that is definitely there. A `display` that has
    /// drifted from `argv` is a button that runs something other than its
    /// label, which is the one failure nobody can debug from the screen.
    #[test]
    fn the_label_is_what_runs() {
        for spec in CATALOG {
            let shown: Vec<&str> = spec.display.split_whitespace().collect();
            let run: Vec<&str> = spec.argv.to_vec();
            assert_eq!(
                shown,
                run[..shown.len().min(run.len())].to_vec(),
                "{} shows {:?} and runs {:?}",
                spec.id,
                spec.display,
                spec.argv
            );
            // Flags may be hidden — `--force`, `--noinput` — but nothing else.
            assert!(
                run[shown.len().min(run.len())..]
                    .iter()
                    .all(|a| a.starts_with("--")),
                "{} runs arguments its label does not show",
                spec.id
            );
        }
    }

    #[test]
    fn every_id_is_unique() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|s| s.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate command id");
    }
}
