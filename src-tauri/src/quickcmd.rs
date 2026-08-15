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
//! The frontend sends an **id**, never a command. `run` looks the id up in
//! [`CATALOG`] and builds the argv itself; there is no code path by which the
//! webview can name a program to execute. That is the same handle-not-a-path
//! rule [`crate::applog`] uses, for the same reason: a project pane that
//! accepted an arbitrary command string from its own frontend is a remote shell
//! with extra steps.
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
        })
        .collect()
}

pub fn find(id: &str) -> Option<&'static Spec> {
    CATALOG.iter().find(|spec| spec.id == id)
}

/// `docker exec` argv for a command. Interactive adds `-it`.
///
/// Built here rather than at the call site so the two callers — the operation
/// runner and the external terminal — cannot disagree about what a command is.
pub fn exec_argv(container: &str, spec: &Spec) -> Vec<String> {
    let mut argv = vec!["exec".to_string()];
    if spec.interactive {
        argv.push("-it".to_string());
    }
    argv.push(container.to_string());
    argv.extend(spec.argv.iter().map(|s| s.to_string()));
    argv
}

// ------------------------------------------------------------------- I/O

/// What this project can run right now.
pub fn for_project(root: &Path, name: &str) -> Result<Vec<QuickCommand>> {
    let dir = crate::workspace::project_dir(root, name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    Ok(available(&crate::detect::fingerprint(&dir)))
}

/// Resolve an id against the catalog, refusing anything else.
///
/// The whole point of the id: the frontend cannot name a program, only pick
/// one that was compiled in.
pub fn resolve(id: &str) -> Result<&'static Spec> {
    find(id).ok_or_else(|| {
        Error::new(Code::NotFound, format!("\"{id}\" is not a known command"))
            .with_hint(crate::hints::QUICK_COMMANDS_ARE_FIXED)
    })
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

    /// The security model in one assertion: an id that is not in the catalog
    /// resolves to nothing, so there is no path from the webview to an
    /// arbitrary `docker exec`.
    #[test]
    fn only_catalog_ids_resolve() {
        assert!(resolve("tinker").is_ok());
        assert!(resolve("rm -rf /").is_err());
        assert!(resolve("").is_err());
        assert!(resolve("../../bin/sh").is_err());
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
        let spec = resolve("migrate").unwrap();
        let argv = exec_argv("stackvo-a; rm -rf ~", spec);

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
        assert!(exec_argv("c", resolve("tinker").unwrap()).contains(&"-it".to_string()));
        assert!(!exec_argv("c", resolve("migrate").unwrap()).contains(&"-it".to_string()));
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
            let spec = resolve(id).unwrap();
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
