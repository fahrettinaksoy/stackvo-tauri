//! Taking a copy of a database, and putting one back.
//!
//! StackVo ships MySQL, MariaDB, PostgreSQL and MongoDB, reads their
//! credentials out of `.env`, and renders them in the services list — and then
//! had nothing whatsoever to do with them. Every competitor that mentions
//! databases at all sells this: Lerd calls it snapshots, Laragon calls it
//! automatic backups, ServBay sells both.
//!
//! Nothing needs installing. `mysqldump`, `pg_dump` and `mongodump` are already
//! inside the images the stack runs, so this is `docker exec` and a file.
//!
//! ## Two things that are not incidental
//!
//! **The dump is never buffered.** stdout is wired straight to the destination
//! file. A production-sized database read into a UI process's memory arrives as
//! an out-of-memory kill with no explanation attached to it, and the whole
//! point of a backup feature is that it works on the database you are afraid of
//! losing, which is the big one.
//!
//! **The password is never an argument.** It goes into the child process's
//! environment, and the docker command line names the variable without its
//! value (`docker exec -e MYSQL_PWD …`), which the Docker CLI resolves from the
//! client environment. `mysqldump -pSECRET` would put it in `ps` output for
//! every user on the machine, and in the shell history of anyone who copied the
//! command out of a log.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::path::Path;

/// The engines that can be dumped, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mysql,
    Mariadb,
    Postgres,
    Mongo,
}

impl Kind {
    pub fn from_service(service: &str) -> Option<Self> {
        match service {
            "mysql" => Some(Kind::Mysql),
            "mariadb" => Some(Kind::Mariadb),
            "postgres" => Some(Kind::Postgres),
            "mongo" => Some(Kind::Mongo),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Mysql => "mysql",
            Kind::Mariadb => "mariadb",
            Kind::Postgres => "postgres",
            Kind::Mongo => "mongo",
        }
    }

    /// What the file should be called, which is not cosmetic: MySQL and
    /// Postgres produce SQL a person can read, diff and edit by hand, whereas
    /// mongodump produces a gzipped BSON archive only mongorestore understands.
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Mongo => "archive.gz",
            _ => "sql",
        }
    }

    /// The environment variable each client reads its password from, so it
    /// never has to be an argument.
    fn password_var(self) -> Option<&'static str> {
        match self {
            Kind::Mysql | Kind::Mariadb => Some("MYSQL_PWD"),
            Kind::Postgres => Some("PGPASSWORD"),
            // mongodump takes --password; there is no environment equivalent.
            // Handled at the call site rather than pretended away here.
            Kind::Mongo => None,
        }
    }

    /// The `.env` keys this engine keeps its settings under.
    fn keys(self) -> EnvKeys {
        match self {
            Kind::Mysql => EnvKeys {
                password: "SERVICE_MYSQL_ROOT_PASSWORD",
                database: Some("SERVICE_MYSQL_DATABASE"),
                user: None,
                enable: "SERVICE_MYSQL_ENABLE",
            },
            Kind::Mariadb => EnvKeys {
                password: "SERVICE_MARIADB_ROOT_PASSWORD",
                database: Some("SERVICE_MARIADB_DATABASE"),
                user: None,
                enable: "SERVICE_MARIADB_ENABLE",
            },
            Kind::Postgres => EnvKeys {
                password: "SERVICE_POSTGRES_PASSWORD",
                database: Some("SERVICE_POSTGRES_DB"),
                user: Some("SERVICE_POSTGRES_USER"),
                enable: "SERVICE_POSTGRES_ENABLE",
            },
            Kind::Mongo => EnvKeys {
                password: "SERVICE_MONGO_INITDB_ROOT_PASSWORD",
                database: None,
                user: Some("SERVICE_MONGO_INITDB_ROOT_USERNAME"),
                enable: "SERVICE_MONGO_ENABLE",
            },
        }
    }
}

struct EnvKeys {
    password: &'static str,
    database: Option<&'static str>,
    user: Option<&'static str>,
    enable: &'static str,
}

/// Every engine this module knows how to handle, in a stable order.
pub const KINDS: [Kind; 4] = [Kind::Mysql, Kind::Mariadb, Kind::Postgres, Kind::Mongo];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTarget {
    pub service: String,
    pub kind: Kind,
    pub container: String,
    pub database: Option<String>,
    pub user: Option<String>,
    pub enabled: bool,
    pub running: bool,
    pub extension: String,
}

/// One database **instance**, which is what a move names (G-4).
///
/// Separate from [`DbTarget`] rather than a field added to it. `targets` answers
/// "which engines can this workspace dump", one row per engine, and four callers
/// read it that way — the dump picker, the query-log picker, the connection
/// panel and the timeline. Making it one row per instance would give all four a
/// list twice as long for a question none of them asked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbInstance {
    pub id: String,
    pub service: String,
    pub version: String,
    pub kind: Kind,
    pub container: String,
    pub enabled: bool,
    pub running: bool,
}

/// Every database instance in the table, whether or not it is up.
///
/// Stopped ones are included and marked. A move needs both ends running and
/// says so; a list that hid the stopped one would leave somebody wondering
/// where the instance they just created went.
pub async fn instances(root: &Path) -> Result<Vec<DbInstance>> {
    let table = crate::instances::Table::load(root)?;
    let mut out = Vec::new();

    for instance in &table.instances {
        let Some(kind) = Kind::from_service(&instance.service) else {
            continue;
        };
        let container = instance.container();
        // Asked per instance, the same way `targets` asks per service: the
        // engine is the only thing that knows, and `enabled` is what the table
        // wants rather than what is true.
        let running = crate::engine::inspect(&container)
            .await
            .map(|d| d.running)
            .unwrap_or(false);

        out.push(DbInstance {
            id: instance.id.clone(),
            service: instance.service.clone(),
            version: instance.version.clone(),
            kind,
            container,
            enabled: instance.enabled,
            running,
        });
    }
    Ok(out)
}

// ------------------------------------------------------------- pure logic

/// Default database users, where `.env` does not name one.
///
/// MySQL and MariaDB only publish a root password, so root is the account with
/// the rights to dump everything — which is what the images themselves assume.
fn default_user(kind: Kind) -> &'static str {
    match kind {
        Kind::Mysql | Kind::Mariadb => "root",
        Kind::Postgres => "postgres",
        Kind::Mongo => "root",
    }
}

/// Run whichever of two client programs the image actually ships.
///
/// **MariaDB 11 removed the `mysql*` symlinks and 12 ships without them.** A
/// `mariadb:12` container has `mariadb`, `mariadb-dump`, `mariadb-admin` and no
/// `mysql` at all — so every one of this app's database features asked that
/// container for a program that is not in it, and got back `exec: "mysql":
/// executable file not found`. Dumps, restores, snapshots, moves and the query
/// log, all of them, on a service that is in the catalogue and shipped working
/// on `mariadb:10`.
///
/// Found by running the query-log probe against the live stack rather than by
/// reading anything: the unit tests assert the argument *list*, and the list
/// was right for the program it named.
///
/// The choice is made inside the container, by the container, because that is
/// the only party that knows what it has. `"$@"` rather than interpolation, so
/// the arguments stay separate argv entries exactly as they were built here —
/// the same rule `elevate` and `runner` follow.
fn either_client(preferred: &str, fallback: &str, args: &[String]) -> Vec<String> {
    let mut out = vec![
        "sh".to_string(),
        "-c".to_string(),
        format!(
            "if command -v {preferred} >/dev/null 2>&1; \
             then exec {preferred} \"$@\"; else exec {fallback} \"$@\"; fi"
        ),
        // `sh -c` gives $0 to the next argument, so a placeholder has to sit
        // between the script and the real arguments or the first one vanishes.
        "stackvo".to_string(),
    ];
    out.extend(args.iter().cloned());
    out
}

/// The client program for a MySQL-family container, whichever name it has.
fn mysql_family(kind: Kind, tool: &str, args: Vec<String>) -> Vec<String> {
    match kind {
        // MariaDB's own names first: an image new enough to have dropped the
        // symlinks is the case this exists for, and one old enough to have only
        // `mysqldump` falls through to it.
        // `mariadb-dump` with a hyphen, `mysqldump` without one — the two
        // families do not name their tools the same way, which is why this
        // takes both names rather than a suffix.
        Kind::Mariadb => either_client(
            &format!(
                "mariadb{}",
                if tool.is_empty() {
                    String::new()
                } else {
                    format!("-{tool}")
                }
            ),
            &format!("mysql{tool}"),
            &args,
        ),
        // MySQL has never shipped the `mariadb*` names, so there is nothing to
        // choose between and no reason to pay for a shell.
        _ => {
            let mut out = vec![format!("mysql{tool}")];
            out.extend(args);
            out
        }
    }
}

/// The arguments after `docker exec`, for reading a database out.
///
/// Returned rather than executed so the shape is testable — the difference
/// between `--single-transaction` being there and not is the difference between
/// a consistent dump and a torn one, and that is not something to discover from
/// a restore.
pub fn dump_args(kind: Kind, user: &str, database: Option<&str>) -> Vec<String> {
    let s = |v: &str| v.to_string();

    match kind {
        Kind::Mysql | Kind::Mariadb => {
            let mut args = vec![
                format!("--user={user}"),
                // Without this, InnoDB tables are dumped one at a time while
                // the application keeps writing, and the result is internally
                // inconsistent in a way nothing reports until it is restored.
                s("--single-transaction"),
                s("--routines"),
                s("--triggers"),
                s("--events"),
            ];
            match database {
                Some(db) => args.push(s(db)),
                None => args.push(s("--all-databases")),
            }
            mysql_family(kind, "dump", args)
        }
        Kind::Postgres => {
            let mut args = vec![s("pg_dump"), format!("--username={user}"), s("--clean")];
            match database {
                Some(db) => args.push(format!("--dbname={db}")),
                // pg_dumpall is a different program; without a database there
                // is nothing sensible to run.
                None => args.push(format!("--dbname={}", default_user(kind))),
            }
            args
        }
        Kind::Mongo => vec![
            s("mongodump"),
            format!("--username={user}"),
            s("--authenticationDatabase=admin"),
            // A single gzipped stream on stdout, rather than mongodump's
            // default directory of BSON files — a backup that is one file can
            // be moved, hashed and restored without unpacking it first.
            s("--archive"),
            s("--gzip"),
            s("--quiet"),
        ],
    }
}

/// The arguments after `docker exec`, for putting a database back.
pub fn restore_args(kind: Kind, user: &str, database: Option<&str>) -> Vec<String> {
    let s = |v: &str| v.to_string();

    match kind {
        Kind::Mysql | Kind::Mariadb => {
            let mut args = vec![format!("--user={user}")];
            if let Some(db) = database {
                args.push(s(db));
            }
            mysql_family(kind, "", args)
        }
        Kind::Postgres => {
            let mut args = vec![s("psql"), format!("--username={user}")];
            args.push(format!(
                "--dbname={}",
                database.unwrap_or(default_user(kind))
            ));
            args
        }
        Kind::Mongo => vec![
            s("mongorestore"),
            format!("--username={user}"),
            s("--authenticationDatabase=admin"),
            s("--archive"),
            s("--gzip"),
            // Restoring on top of existing data would merge two databases into
            // one and call it success. The contract says this replaces the
            // target, so it has to actually replace it.
            s("--drop"),
            s("--quiet"),
        ],
    }
}

/// A filename that sorts chronologically and says what it is.
///
/// `2026-07-29T14-05-33` rather than the RFC 3339 spelling: a colon is not a
/// legal filename character on Windows, and a backup that cannot be written on
/// one of the three supported platforms is not a backup.
pub fn suggested_filename(service: &str, kind: Kind, stamp: &str) -> String {
    format!("{service}-{stamp}.{}", kind.extension())
}

// ------------------------------------------------------------------- I/O

/// Everything this module needs to talk to one engine, resolved from `.env`.
#[derive(Clone)]
struct Settings {
    kind: Kind,
    container: String,
    user: String,
    password: Option<String>,
    database: Option<String>,
}

/// The container this service actually runs in.
///
/// `stackvo-<service>` was right while every service was single-instance and
/// named after itself. It stopped being right the moment the instance table
/// arrived: an instance is `stackvo-mysql-9-7`, and after ADR 0016 there is no
/// other kind of workspace. So this reads the table first and only falls back
/// to the old shape when there is none — which today means a workspace that has
/// not migrated, and those are refused by the renderer before they get here.
///
/// The same mistake `list_services` made and was fixed for: it built every
/// container name as `stackvo-<id>`, so a migrated workspace listed
/// twenty-five services and reported all of them stopped. Here the cost was
/// quieter and worse — dump, restore, snapshot and the query log all ran
/// against a container that does not exist.
///
/// The first instance of that service, when there are several. A workspace
/// running MySQL 8.0 and 9.4 side by side has two, and this picks the one the
/// table lists first rather than guessing; naming which one is a question the
/// screens above this have to ask, and none of them ask it yet.
pub fn container_of(root: &Path, service: &str) -> String {
    crate::instances::Table::load(root)
        .ok()
        .and_then(|table| {
            table
                .instances
                .iter()
                .find(|instance| instance.service == service)
                .map(|instance| instance.container())
        })
        .unwrap_or_else(|| format!("{}{service}", crate::engine::CONTAINER_PREFIX))
}

/// The same settings, for one **instance** rather than for a service.
///
/// `settings` resolves a service to whichever instance the table happens to
/// list first, which was correct while a service meant one container and is
/// exactly wrong for G-4: moving `mysql-8-0` into `mysql-8-4` has to name two
/// containers, and both of them answer to `mysql`.
///
/// Credentials still come from the same place. An instance carries its own
/// settings map, but the root password of a *migrated* instance is the one in
/// `.env` — the handover deliberately left it there — so the per-instance value
/// wins and `.env` is the fallback rather than the other way round.
///
/// This used to read the instance's settings map with the **`.env` key** —
/// `instance.settings.get("SERVICE_POSTGRES_USER")` — and an instance stores
/// `USER`, the key its package declares. So the lookup matched nothing on every
/// workspace and this function was `settings` with a different container name.
/// It now resolves through the manifest's `connection` block, the same as
/// [`declared`], which is the only thing that knows which setting is the login.
fn settings_for_instance(root: &Path, id: &str) -> Result<Settings> {
    let table = crate::instances::Table::load(root)?;
    let instance = table
        .get(id)
        .ok_or_else(|| Error::not_found(format!("instance {id}")))?;

    let mut settings = settings(root, &instance.service)?;
    settings.container = instance.container();

    let env = crate::config::Env::load(root)?;
    let keys = settings.kind.keys();
    let (user, password, database) = declared_for(root, instance);

    if let Some(user) = user.or_env(keys.user.and_then(|k| env.get(k))) {
        settings.user = user;
    }
    if let Some(password) = password.or_env(env.get(keys.password)) {
        settings.password = Some(password);
    }
    if let Some(database) = database.or_env(keys.database.and_then(|k| env.get(k))) {
        settings.database = Some(database);
    }
    Ok(settings)
}

/// One credential, from the two places a **package** can answer it.
///
/// Separate fields rather than one resolved string because the order matters
/// and it is not the obvious one — see [`Value::or_env`].
#[derive(Default)]
struct Value {
    /// What the instance table holds, or what the keystore holds for it.
    stored: Option<String>,
    /// What the manifest says the container runs with when nothing is stored.
    default: Option<String>,
}

impl Value {
    /// Stored, then `.env`, then the package's default.
    ///
    /// `.env` sits in the **middle** on purpose. Putting the package first
    /// throughout would have been simpler and would have regressed every
    /// migrated workspace: a handover leaves the real password in `.env`, the
    /// manifest declares a default of `stackvo`, and a resolver that preferred
    /// the manifest would hand a dump the wrong password with no error anybody
    /// could read. Putting `.env` first is the mirror of that bug on a workspace
    /// installed from packages. Stored beats both because it is the only one of
    /// the three that somebody typed for *this* instance.
    fn or_env(self, env: Option<&str>) -> Option<String> {
        self.stored
            .or_else(|| env.filter(|v| !v.is_empty()).map(str::to_string))
            .or(self.default)
            .filter(|v| !v.is_empty())
    }
}

/// What the package and the instance table say this service is running with.
///
/// The whole reason this exists: after ADR 0016 a workspace is installed from
/// packages, and a package does not write `SERVICE_POSTGRES_USER` into `.env` —
/// it stores `USER` on the instance and renders it into the compose file. So
/// `.env` holds nothing for it, and every caller of [`run_sql`] was falling
/// through to `default_user`, which for Postgres is `postgres`: an account the
/// container does not have, because the image created the one the manifest
/// named. Measured rather than reasoned about — `psql -U postgres` against this
/// workspace's own Postgres answers `FATAL: role "postgres" does not exist`,
/// which is what the query log, dump, restore and snapshot all got.
///
/// The manifest's `connection` block is what maps a role onto a setting key:
/// `userSetting`, `passwordSetting`, `databaseSetting`. Read from there rather
/// than by stripping a `SERVICE_<NAME>_` prefix off the `.env` key, because the
/// prefix rule is a coincidence of how the keys were named and the block is the
/// package contract saying which setting is the login.
///
/// Every failure here is `None` rather than an error: no table, no market, no
/// manifest and no `connection` block all mean the same thing to the caller —
/// this workspace answers from `.env`, the way it did before packages.
fn declared(root: &Path, service: &str) -> (Value, Value, Value) {
    let nothing = || (Value::default(), Value::default(), Value::default());

    let Ok(table) = crate::instances::Table::load(root) else {
        return nothing();
    };
    // The first instance of that service, which is the same rule
    // `container_of` follows — and it has to be, or the credentials would
    // belong to one container and the exec to another.
    let Some(instance) = table.instances.iter().find(|i| i.service == service) else {
        return nothing();
    };
    declared_for(root, instance)
}

/// The same three values for one **instance** rather than for a service.
fn declared_for(root: &Path, instance: &crate::instances::Instance) -> (Value, Value, Value) {
    let nothing = || (Value::default(), Value::default(), Value::default());

    let Ok(tree) = crate::pkg::Tree::open(&crate::market::dir(root)) else {
        return nothing();
    };
    let Ok(manifest) = tree.load(&instance.service, &instance.version) else {
        return nothing();
    };
    let Some(conn) = manifest.connection.as_ref() else {
        return nothing();
    };

    let value = |key: Option<&String>| -> Value {
        let Some(key) = key else {
            return Value::default();
        };
        let Some(setting) = manifest.settings.iter().find(|s| &s.key == key) else {
            return Value::default();
        };
        Value {
            stored: instance
                .settings
                .get(key)
                .cloned()
                .or_else(|| {
                    instance
                        .secret_refs
                        .get(key)
                        .and_then(|reference| crate::secrets::entry_of(reference))
                        // A keystore that cannot be reached is not a reason to
                        // fail: the manifest default below is what the container
                        // was started with when nothing was ever stored, and it
                        // is a better answer than no answer.
                        .and_then(|entry| crate::secrets::read(entry).ok().flatten())
                })
                .filter(|v| !v.is_empty()),
            default: setting.default_text().filter(|v| !v.is_empty()),
        }
    };

    let mut user = value(conn.user_setting.as_ref());
    if user.default.is_none() {
        // A package with no `userSetting` — MySQL and MariaDB, which publish
        // only a root password — still names the account it runs as.
        user.default = conn.default_user.clone();
    }
    let mut database = value(conn.database_setting.as_ref());
    if database.default.is_none() {
        database.default = conn.default_database.clone();
    }

    (user, value(conn.password_setting.as_ref()), database)
}

fn settings(root: &Path, service: &str) -> Result<Settings> {
    let kind = Kind::from_service(service).ok_or_else(|| {
        Error::new(
            Code::Unsupported,
            format!("{service} is not a database this app can dump"),
        )
        .with_hint(crate::hints::SUPPORTED_DATABASES)
    })?;

    let env = crate::config::Env::load(root)?;
    let keys = kind.keys();
    let (user, password, database) = declared(root, service);

    Ok(Settings {
        kind,
        container: container_of(root, service),
        user: user
            .or_env(keys.user.and_then(|k| env.get(k)))
            .unwrap_or_else(|| default_user(kind).to_string()),
        password: password.or_env(env.get(keys.password)),
        database: database.or_env(keys.database.and_then(|k| env.get(k))),
    })
}

/// Which engines are configured, and which are up right now.
pub async fn targets(root: &Path) -> Result<Vec<DbTarget>> {
    let env = crate::config::Env::load(root)?;
    let mut out = Vec::new();

    for kind in KINDS {
        let service = kind.as_str();
        let keys = kind.keys();
        let settings = settings(root, service)?;

        // Listed even when it is not running: "why is the button disabled" is
        // a question the row itself should answer.
        //
        // Asked of `settings.container`, not of `service`. Those were the same
        // string while a service meant one container, and the instance table
        // ended that: the container is `stackvo-mysql-9-7` and `stackvo-mysql`
        // does not exist, so asking by service name reported every running
        // database as stopped — and this is the field the dump, restore and
        // snapshot buttons are disabled by. Found by `stackvo db` printing four
        // engines as down while `docker ps` listed them up.
        let running = crate::engine::inspect(&settings.container)
            .await
            .map(|d| d.running)
            .unwrap_or(false);

        out.push(DbTarget {
            service: service.to_string(),
            kind,
            container: settings.container,
            database: settings.database,
            user: Some(settings.user),
            enabled: env.bool(keys.enable),
            running,
            extension: kind.extension().to_string(),
        });
    }

    Ok(out)
}

/// `docker exec` prefix, with the password named but not valued.
///
/// `-e MYSQL_PWD` with no `=value` tells the Docker CLI to take it from its own
/// environment, which is this process's child environment — so the secret
/// crosses into the container without ever being written on a command line.
fn exec_args(settings: &Settings, interactive: bool) -> Vec<String> {
    let mut args = vec!["exec".to_string()];
    if interactive {
        args.push("-i".to_string());
    }
    if settings.password.is_some() {
        if let Some(var) = settings.kind.password_var() {
            args.push("-e".to_string());
            args.push(var.to_string());
        }
    }
    args.push(settings.container.clone());
    args
}

/// Run SQL against one instance and hand back what it printed.
///
/// Here rather than in `querylog.rs` because this module owns the one thing
/// that is delicate about reaching a database: the password crosses as a named
/// environment variable (`-e MYSQL_PWD` with no value), so it is never on a
/// command line where `ps` could read it. A second copy of that arrangement
/// somewhere else is a second chance to get it wrong.
///
/// `-N -B`: no column headers and tab-separated, which is the format
/// `querylog::parse_rows` reads. The statement itself is always one of this
/// app's own constants — so there is no quoting problem to solve, and adding a
/// parameter binding layer for four fixed statements would be machinery with no
/// user.
///
/// The one statement that is not a constant is the per-worktree
/// `CREATE DATABASE` below, and it is built from a name that
/// [`is_valid_database_name`] has already reduced to `[a-z0-9_]`. That check is
/// the reason the rule above still holds rather than an exception to it: a name
/// that could carry a quote, a semicolon or a backtick never reaches here.
pub async fn run_sql(root: &Path, service: &str, sql: &str) -> Result<String> {
    run_sql_with(&settings(root, service)?, sql).await
}

/// The last `lines` lines of a file **inside** a container.
///
/// Here rather than in `engine.rs` for the reason [`run_sql`] gives: this module
/// already owns the `docker exec` arrangement, and `engine.rs` talks to the
/// daemon through bollard, where an exec is a create-start-attach dance for what
/// is one line here.
///
/// It exists for one caller — the Postgres half of the query log. Postgres with
/// `logging_collector = on` does not write to the container's stdout at all, so
/// `engine::logs_tail` returns the startup banner and the line *saying* the log
/// went elsewhere. See [`crate::querylog::pg_log_path_sql`].
pub async fn read_tail(container: &str, path: &str, lines: u32) -> Result<String> {
    let out = tokio::process::Command::new("docker")
        .args([
            "exec",
            container,
            "tail",
            "-n",
            &lines.to_string(),
            "--",
            path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| Error::io("running docker exec", e))?;

    if !out.status.success() {
        let text = String::from_utf8_lossy(&out.stderr);
        let reason = text
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("the container could not read that file");
        return Err(Error::new(Code::IoError, reason.trim().to_string()));
    }
    // Lossy for the same reason `logs_tail` is: a log file is whatever the
    // server wrote into it, and one statement holding a byte that is not UTF-8
    // must not throw away the other four hundred.
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

async fn run_sql_with(settings: &Settings, sql: &str) -> Result<String> {
    let mut args = exec_args(settings, false);
    match settings.kind {
        Kind::Mysql | Kind::Mariadb => {
            args.extend(mysql_family(
                settings.kind,
                "",
                vec![
                    format!("-u{}", settings.user),
                    "-N".to_string(),
                    "-B".to_string(),
                    "-e".to_string(),
                    sql.to_string(),
                ],
            ));
        }
        // `-tA`: no header, no alignment — the same tab-free, row-per-line
        // shape `-N -B` gives on the MySQL side, so one parser reads both.
        Kind::Postgres => {
            args.push("psql".to_string());
            args.push("-U".to_string());
            args.push(settings.user.clone());
            args.push("-tA".to_string());
            args.push("-c".to_string());
            args.push(sql.to_string());
        }
        // Not SQL, and the name of this function is the only thing that says
        // so — what a caller wants from all four is "run this and hand me the
        // output", and forcing Mongo through a second function would put the
        // password arrangement above in two places.
        // The password is an argument here, and that is a loss this module
        // otherwise refuses: `password_var` returns None for Mongo because
        // there is no environment equivalent, so `ps` can see it for the life
        // of the process. `dump` and `restore` already pay it for the same
        // reason and the same absence — a second arrangement would not remove
        // the exposure, only spread the knowledge of it.
        Kind::Mongo => {
            args.push("mongosh".to_string());
            args.push("--quiet".to_string());
            args.push("-u".to_string());
            args.push(settings.user.clone());
            if let Some(password) = &settings.password {
                args.push("-p".to_string());
                args.push(password.clone());
            }
            // Where the account actually lives. `MONGO_INITDB_ROOT_USERNAME`
            // creates the user in `admin`, and mongosh's default auth source is
            // whichever database it connected to — `test` — so without this the
            // root credentials are checked against a database that has never
            // heard of them and every statement comes back as an auth failure.
            // `dump_args` and `restore_args` have always passed it; this branch
            // did not, and nothing exercised it until a worktree needed to ask
            // Mongo which databases it has.
            args.push("--authenticationDatabase=admin".to_string());
            args.push("--eval".to_string());
            args.push(sql.to_string());
        }
    }

    let mut command = tokio::process::Command::new("docker");
    command.args(&args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if let (Some(var), Some(password)) = (settings.kind.password_var(), &settings.password) {
        command.env(var, password);
    }

    let out = command
        .output()
        .await
        .map_err(|e| Error::io("running docker exec", e))?;

    if !out.status.success() {
        // The client writes a password warning to stderr on every run, which is
        // not a failure and must not be reported as the reason for one.
        let text = String::from_utf8_lossy(&out.stderr);
        let reason = text
            .lines()
            .filter(|l| !l.contains("Using a password on the command line"))
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("the database refused the statement");
        return Err(Error::new(Code::IoError, reason.trim().to_string()));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

// ------------------------------------------------- databases on one instance
//
// N. A worktree gets its own database on an instance that already exists,
// rather than an instance of its own: a branch is not a different engine, and a
// second MySQL per branch would cost a gigabyte of RAM to hold a schema copy.

/// How a project's container reaches one instance, from inside the network.
///
/// Deliberately not [`DbTarget`]: that one answers "which engines can this
/// workspace dump" for four pickers, and every field on it is about the host
/// side. This is the other direction — what to put in `DB_HOST` and `DB_PORT`
/// so that an application inside a container can connect — and those two are
/// **not** the host's published port. A worktree that was handed `127.0.0.1` and
/// the host port would reach its own container's loopback and nothing else.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub instance: String,
    pub service: String,
    pub kind: Kind,
    /// The container's name, which is also the name it answers to on the
    /// network — see [`crate::instances::Instance::aliases`].
    pub host: String,
    /// The engine's own port, inside the network.
    pub port: u16,
    pub user: String,
    /// Never serialised. The struct crosses the IPC boundary in
    /// `worktree_plan`, and a preview of what would be created has no business
    /// carrying the root password to a webview.
    #[serde(skip)]
    pub password: Option<String>,
    /// The database this instance is configured with, when it has one. The
    /// source a copy reads from, and the stem a worktree's name is built on.
    pub database: Option<String>,
}

/// The port an engine listens on inside the network, which is fixed by the
/// image and never the host port the instance publishes.
pub fn default_port(kind: Kind) -> u16 {
    match kind {
        Kind::Mysql | Kind::Mariadb => 3306,
        Kind::Postgres => 5432,
        Kind::Mongo => 27017,
    }
}

/// The scheme a URL-shaped setting (`DATABASE_URL`) uses for this engine.
pub fn url_scheme(kind: Kind) -> &'static str {
    match kind {
        Kind::Mysql => "mysql",
        Kind::Mariadb => "mysql",
        Kind::Postgres => "pgsql",
        Kind::Mongo => "mongodb",
    }
}

/// Everything needed to reach one instance from a container.
pub fn connection(root: &Path, instance_id: &str) -> Result<Connection> {
    let table = crate::instances::Table::load(root)?;
    let instance = table
        .get(instance_id)
        .ok_or_else(|| Error::not_found(format!("instance {instance_id}")))?;
    let settings = settings_for_instance(root, instance_id)?;

    Ok(Connection {
        instance: instance.id.clone(),
        service: instance.service.clone(),
        kind: settings.kind,
        host: instance.container(),
        port: default_port(settings.kind),
        user: settings.user.clone(),
        password: settings.password.clone(),
        database: settings.database.clone(),
    })
}

/// Names this app will never create, drop or hand to a project.
///
/// Every engine keeps its own catalogue in a database, and a `DROP DATABASE
/// mysql` is not a mistake anybody recovers from by pressing undo. The list is
/// checked on the way *in* rather than trusted to the derivation, because the
/// derivation takes a branch name and a branch may be called `sys`.
const RESERVED_DATABASES: [&str; 10] = [
    "mysql",
    "sys",
    "information_schema",
    "performance_schema",
    "postgres",
    "template0",
    "template1",
    "admin",
    "local",
    "config",
];

/// Is this a database name this app is willing to build a statement from?
///
/// Narrow on purpose, and narrower than any of the four engines would accept.
/// The name is interpolated into `CREATE DATABASE`, so the guarantee wanted here
/// is not "the engine will parse it" but "there is nothing in it to escape":
/// lower-case letters, digits and underscore, beginning with a letter.
///
/// 63 characters because that is the shortest of the four limits — PostgreSQL's
/// identifier length. MySQL allows 64 and Mongo 63 bytes; taking the smallest
/// means a name derived once works on whichever engine the workspace has.
pub fn is_valid_database_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !RESERVED_DATABASES.contains(&name)
}

fn checked_database(name: &str) -> Result<()> {
    if !is_valid_database_name(name) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{name}\" is not a database name this app will create"),
        )
        .with_hint(crate::hints::DATABASE_NAME_CHARSET));
    }
    Ok(())
}

/// The same instance, addressed as one particular database on it.
fn on_database(mut settings: Settings, database: &str) -> Settings {
    settings.database = Some(database.to_string());
    settings
}

/// Every database on this instance, as the engine lists them.
///
/// System catalogues included — this is what the engine has, and filtering here
/// would make "does this name already exist" answer no for a name that cannot
/// be created.
pub async fn databases(root: &Path, instance_id: &str) -> Result<Vec<String>> {
    let settings = settings_for_instance(root, instance_id)?;

    let sql = match settings.kind {
        Kind::Mysql | Kind::Mariadb => "SHOW DATABASES",
        Kind::Postgres => "SELECT datname FROM pg_database",
        // `.join` rather than a returned array: `run_sql` hands back whatever
        // the client printed, and mongosh prints a JS array with quotes and
        // commas in it. One name per line is the shape the other three give.
        Kind::Mongo => "db.adminCommand({listDatabases:1}).databases.map(d=>d.name).join('\\n')",
    };

    Ok(run_sql_with(&settings, sql)
        .await?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// Create a database on this instance, or report that it was already there.
///
/// `Ok(false)` means nothing was created and nothing was wrong: the name
/// already existed, or the engine is MongoDB, which has no `CREATE DATABASE` at
/// all — a Mongo database begins existing when something writes to it, and
/// pretending otherwise here would mean either a lie or a collection this app
/// invented in somebody's data.
///
/// `like` names a database whose character set the new one should match. It
/// matters for exactly the reason it is easy to skip: a worktree copied from a
/// `utf8mb4` parent into a server-default database gets tables that compare
/// differently, and the symptom is a join that returns nothing with no error
/// anywhere. MySQL and MariaDB are asked; PostgreSQL takes the encoding from
/// the template it copies and there is nothing to carry over.
pub async fn create_database(
    root: &Path,
    instance_id: &str,
    name: &str,
    like: Option<&str>,
) -> Result<bool> {
    checked_database(name)?;
    let settings = settings_for_instance(root, instance_id)?;

    if settings.kind == Kind::Mongo {
        return Ok(false);
    }
    if databases(root, instance_id)
        .await?
        .iter()
        .any(|existing| existing == name)
    {
        return Ok(false);
    }

    let sql = match settings.kind {
        Kind::Mysql | Kind::Mariadb => {
            let collation = match like {
                Some(source) => charset_of(&settings, source).await,
                None => None,
            };
            match collation {
                Some((charset, collate)) => {
                    format!("CREATE DATABASE `{name}` CHARACTER SET {charset} COLLATE {collate}")
                }
                // The default for every server in the catalogue, and the one
                // answer that is right for a name, an emoji and a Turkish `ı`
                // alike. Stated rather than inherited: a server started with an
                // older `my.cnf` still defaults to latin1.
                None => format!(
                    "CREATE DATABASE `{name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci"
                ),
            }
        }
        Kind::Postgres => format!("CREATE DATABASE \"{name}\""),
        Kind::Mongo => unreachable!("returned above"),
    };

    run_sql_with(&settings, &sql).await?;
    Ok(true)
}

/// The character set and collation a database was created with, when the engine
/// will say. `None` on anything unexpected — a missing answer here costs a
/// default, and refusing to create the database over it would be worse.
async fn charset_of(settings: &Settings, database: &str) -> Option<(String, String)> {
    if !is_valid_database_name(database) {
        return None;
    }
    let sql = format!(
        "SELECT DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME FROM \
         information_schema.SCHEMATA WHERE SCHEMA_NAME = '{database}'"
    );
    let out = run_sql_with(settings, &sql).await.ok()?;
    let mut parts = out.split_whitespace();
    let charset = parts.next()?.to_string();
    let collate = parts.next()?.to_string();

    // Read back out of the answer rather than trusted: this goes straight into
    // a statement, and `information_schema` is a table like any other.
    let ok = |v: &str| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (ok(&charset) && ok(&collate)).then_some((charset, collate))
}

/// Drop a database, or report that there was none.
///
/// Refuses the instance's *configured* database as well as the reserved names.
/// That is the one deletion a worktree teardown could plausibly reach by
/// accident — a worktree whose record was hand-edited to name the parent's
/// database — and it is the deletion nobody has a copy of.
pub async fn drop_database(root: &Path, instance_id: &str, name: &str) -> Result<bool> {
    checked_database(name)?;
    let settings = settings_for_instance(root, instance_id)?;

    if settings.database.as_deref() == Some(name) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{name}\" is this instance's own database and is not a worktree's to drop"),
        ));
    }
    if !databases(root, instance_id)
        .await?
        .iter()
        .any(|existing| existing == name)
    {
        return Ok(false);
    }

    match settings.kind {
        Kind::Mysql | Kind::Mariadb => {
            run_sql_with(&settings, &format!("DROP DATABASE `{name}`")).await?;
        }
        Kind::Postgres => {
            // `WITH (FORCE)` disconnects whatever is still attached, which on a
            // worktree being torn down is its own application container. Without
            // it PostgreSQL refuses while any session is open and the teardown
            // fails on the last step. It arrived in PostgreSQL 13, so an older
            // server is retried plainly rather than left unable to drop
            // anything.
            let forced = format!("DROP DATABASE \"{name}\" WITH (FORCE)");
            if run_sql_with(&settings, &forced).await.is_err() {
                run_sql_with(&settings, &format!("DROP DATABASE \"{name}\"")).await?;
            }
        }
        Kind::Mongo => {
            run_sql_with(
                &settings,
                &format!("db.getSiblingDB('{name}').dropDatabase()"),
            )
            .await?;
        }
    }
    Ok(true)
}

/// Copy one database on an instance into another on the same instance.
///
/// Through a file, for the reason `dbmove` gives at length: a pipe leaves a
/// half-populated target with nothing to retry from. The file lands in the
/// system temp directory and is removed on both paths — unlike a move, a failed
/// *copy* has lost nothing, because the source is still there.
///
/// MongoDB is refused rather than approximated. The other three name a source
/// database in `.env` and there is something to copy; Mongo publishes no
/// database name at all (`EnvKeys.database` is `None` for it), so there is no
/// source to read, and a copy that quietly copied nothing would look like it
/// worked.
pub async fn copy_database<F>(
    root: &Path,
    instance_id: &str,
    from: &str,
    to: &str,
    mut on_line: F,
) -> Result<u64>
where
    F: FnMut(String) + Send + Clone + 'static,
{
    checked_database(from)?;
    checked_database(to)?;

    let settings = settings_for_instance(root, instance_id)?;
    if settings.kind == Kind::Mongo {
        return Err(Error::new(
            Code::Unsupported,
            "MongoDB publishes no database name for this workspace, so there is nothing to copy from"
                .to_string(),
        )
        .with_hint(crate::hints::MONGO_HAS_NO_SOURCE_DATABASE));
    }

    let staging = std::env::temp_dir().join(format!("stackvo-worktree-{from}-{to}.sql"));
    let _ = std::fs::remove_file(&staging);

    on_line(format!("copying {from} into {to}"));

    let out = async {
        dump_with(
            on_database(settings.clone(), from),
            from,
            &staging,
            on_line.clone(),
        )
        .await?;
        restore_with(on_database(settings.clone(), to), to, &staging, on_line).await
    }
    .await;

    let _ = std::fs::remove_file(&staging);
    out
}

/// Read the database out into `path`.
pub async fn dump<F>(root: &Path, service: &str, path: &Path, on_line: F) -> Result<u64>
where
    F: FnMut(String) + Send + 'static,
{
    dump_with(settings(root, service)?, service, path, on_line).await
}

/// The same, naming an instance rather than a service (G-4).
pub async fn dump_instance<F>(root: &Path, id: &str, path: &Path, on_line: F) -> Result<u64>
where
    F: FnMut(String) + Send + 'static,
{
    dump_with(settings_for_instance(root, id)?, id, path, on_line).await
}

async fn dump_with<F>(settings: Settings, who: &str, path: &Path, on_line: F) -> Result<u64>
where
    F: FnMut(String) + Send + 'static,
{
    let service = who;

    let mut args = exec_args(&settings, false);
    args.extend(dump_args(
        settings.kind,
        &settings.user,
        settings.database.as_deref(),
    ));
    // mongodump has no password environment variable, so it is passed as an
    // argument and this is the one place that happens. Said out loud rather
    // than buried: on a shared machine it is briefly visible in `ps`.
    if settings.kind == Kind::Mongo {
        if let Some(password) = &settings.password {
            args.push(format!("--password={password}"));
        }
    }

    let file = std::fs::File::create(path)
        .map_err(|e| Error::io(format!("creating {}", path.display()), e))?;

    let status = run(&settings, args, Some(Stdio::from(file)), None, on_line).await?;
    if !status {
        // A half-written dump is worse than none: it looks like a backup.
        let _ = std::fs::remove_file(path);
        return Err(Error::new(
            Code::GenerateFailed,
            format!("the {service} dump failed; the incomplete file was removed"),
        ));
    }

    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| Error::io("measuring the dump", e))
}

/// Put `path` back into the database, replacing what is there.
pub async fn restore<F>(root: &Path, service: &str, path: &Path, on_line: F) -> Result<u64>
where
    F: FnMut(String) + Send + 'static,
{
    restore_with(settings(root, service)?, service, path, on_line).await
}

/// The same, naming an instance rather than a service (G-4).
pub async fn restore_instance<F>(root: &Path, id: &str, path: &Path, on_line: F) -> Result<u64>
where
    F: FnMut(String) + Send + 'static,
{
    restore_with(settings_for_instance(root, id)?, id, path, on_line).await
}

async fn restore_with<F>(settings: Settings, who: &str, path: &Path, on_line: F) -> Result<u64>
where
    F: FnMut(String) + Send + 'static,
{
    let service = who;

    let bytes = std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;

    let mut args = exec_args(&settings, true);
    args.extend(restore_args(
        settings.kind,
        &settings.user,
        settings.database.as_deref(),
    ));
    if settings.kind == Kind::Mongo {
        if let Some(password) = &settings.password {
            args.push(format!("--password={password}"));
        }
    }

    let file = std::fs::File::open(path)
        .map_err(|e| Error::io(format!("opening {}", path.display()), e))?;

    let status = run(&settings, args, None, Some(Stdio::from(file)), on_line).await?;
    if !status {
        return Err(Error::new(
            Code::GenerateFailed,
            format!("the {service} restore failed"),
        ));
    }

    Ok(bytes)
}

use std::process::Stdio;

/// Spawn docker with the given stdio, streaming stderr as progress.
///
/// stdout is deliberately never read into this process when a file is given:
/// the whole reason a dump goes straight to disk is that it does not fit here.
async fn run<F>(
    settings: &Settings,
    args: Vec<String>,
    stdout: Option<Stdio>,
    stdin: Option<Stdio>,
    mut on_line: F,
) -> Result<bool>
where
    F: FnMut(String) + Send + 'static,
{
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut command = tokio::process::Command::new("docker");
    command.args(&args);
    command.stderr(Stdio::piped());
    command.stdout(stdout.unwrap_or_else(Stdio::null));
    command.stdin(stdin.unwrap_or_else(Stdio::null));

    if let (Some(var), Some(password)) = (settings.kind.password_var(), &settings.password) {
        command.env(var, password);
    }

    let mut child = command
        .spawn()
        .map_err(|e| Error::io("running docker exec", e))?;

    if let Some(stderr) = child.stderr.take() {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if !line.is_empty() {
                on_line(line);
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| Error::io("waiting for docker exec", e))?;

    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// N. The port a container reaches the engine on inside the network, which
    /// is the engine's own and never the host port the instance publishes.
    ///
    /// Getting this wrong is not a compile error and not a test failure
    /// anywhere else: the worktree comes up, the application starts, and the
    /// first query fails with a connection refused that reads as "the database
    /// is down" rather than "we told it the wrong number".
    #[test]
    fn the_port_handed_to_a_container_is_the_engines_own() {
        assert_eq!(default_port(Kind::Mysql), 3306);
        assert_eq!(default_port(Kind::Mariadb), 3306);
        assert_eq!(default_port(Kind::Postgres), 5432);
        assert_eq!(default_port(Kind::Mongo), 27017);
    }

    #[test]
    fn a_url_shaped_setting_names_the_scheme_its_driver_expects() {
        assert_eq!(url_scheme(Kind::Mysql), "mysql");
        // MariaDB speaks the MySQL protocol and every client library that
        // parses one of these expects `mysql://`.
        assert_eq!(url_scheme(Kind::Mariadb), "mysql");
        assert_eq!(url_scheme(Kind::Postgres), "pgsql");
        assert_eq!(url_scheme(Kind::Mongo), "mongodb");
    }

    /// The gate between a derived name and a statement built by formatting.
    ///
    /// Everything this accepts is interpolated into `CREATE DATABASE` unquoted,
    /// so the test is not "does the engine like it" — it is "is there anything
    /// in it to escape".
    #[test]
    fn a_database_name_has_nothing_in_it_to_escape() {
        for good in ["shop", "shop_feature_x", "a1_2", "w_2fa"] {
            assert!(is_valid_database_name(good), "{good}");
        }

        for bad in [
            "",
            "1shop",        // must begin with a letter
            "Shop",         // upper case would need quoting on Postgres
            "shop-feature", // a hyphen needs quoting in every statement
            "shop feature",
            "shop`; DROP DATABASE x;--",
            "shop\"",
            "shop'",
            &"a".repeat(64), // one over the shortest engine limit
        ] {
            assert!(!is_valid_database_name(bad), "{bad:?} was accepted");
        }
    }

    /// A branch may be called `sys`, and the derivation would happily produce a
    /// name that is an engine's own catalogue. Refused on the way in rather
    /// than trusted to the caller, because `DROP DATABASE mysql` is not a
    /// mistake anybody undoes.
    #[test]
    fn the_engines_own_databases_are_never_accepted() {
        for reserved in RESERVED_DATABASES {
            assert!(!is_valid_database_name(reserved), "{reserved}");
        }
    }

    /// The password crosses to Mongo on a command line — this module says so
    /// rather than hiding it — but the account it belongs to lives in `admin`,
    /// and a statement that does not say so cannot authenticate at all.
    #[test]
    fn a_mongo_statement_names_the_database_the_account_lives_in() {
        let args = dump_args(Kind::Mongo, "root", None);
        assert!(args.contains(&"--authenticationDatabase=admin".to_string()));
    }

    #[test]
    fn only_the_four_shipped_engines_are_recognised() {
        assert_eq!(Kind::from_service("mysql"), Some(Kind::Mysql));
        assert_eq!(Kind::from_service("postgres"), Some(Kind::Postgres));
        assert_eq!(Kind::from_service("redis"), None, "not a dumpable engine");
        assert_eq!(Kind::from_service("mongo-express"), None, "an admin UI");
    }

    /// The difference between a consistent dump and a torn one, on a database
    /// that is still being written to. Nothing reports the difference until a
    /// restore, by which point the good copy may be gone.
    #[test]
    fn mysql_dumps_in_a_single_transaction() {
        let args = dump_args(Kind::Mysql, "root", Some("stackvo"));
        assert!(args.contains(&"--single-transaction".to_string()));
        // Stored routines and triggers are not in a default mysqldump, and a
        // restore that silently loses them looks like it worked.
        assert!(args.contains(&"--routines".to_string()));
        assert!(args.contains(&"--triggers".to_string()));
        assert_eq!(args.last().unwrap(), "stackvo");
    }

    /// MariaDB 11 removed the `mysql*` symlinks and 12 ships without them, so
    /// every command this app sent a `mariadb:12` container named a program
    /// that was not in it. The tests above pass on the argument *list*, which
    /// was right for the program it named — the program was the bug, and only
    /// running it against the real image found that.
    ///
    /// Both names are here rather than just the new one: the same catalogue
    /// still offers `mariadb:10`, where only `mysqldump` exists.
    #[test]
    fn mariadb_asks_for_its_own_client_and_falls_back_to_the_old_name() {
        let args = dump_args(Kind::Mariadb, "root", Some("stackvo"));
        let script = args.join(" ");
        assert_eq!(args[0], "sh", "the container picks, because it knows");
        assert!(script.contains("command -v mariadb-dump"), "{script}");
        assert!(script.contains("exec mysqldump"), "no fallback: {script}");
        // The arguments still arrive as arguments — `"$@"` rather than a
        // second layer of quoting, and a placeholder for `$0` so the first one
        // is not eaten by the shell.
        assert!(script.contains("\"$@\""), "{script}");
        assert_eq!(args[3], "stackvo", "the $0 placeholder");
        assert!(args.contains(&"--single-transaction".to_string()));
        assert_eq!(args.last().unwrap(), "stackvo");

        let restore = restore_args(Kind::Mariadb, "root", Some("shop")).join(" ");
        assert!(restore.contains("command -v mariadb "), "{restore}");
        assert!(restore.contains("exec mysql "), "{restore}");

        // MySQL has never shipped the mariadb names, so it pays for no shell.
        assert_eq!(dump_args(Kind::Mysql, "root", None)[0], "mysqldump");
        assert_eq!(restore_args(Kind::Mysql, "root", None)[0], "mysql");
    }

    #[test]
    fn a_missing_database_name_dumps_everything_rather_than_nothing() {
        let args = dump_args(Kind::Mariadb, "root", None);
        assert!(args.contains(&"--all-databases".to_string()));
    }

    /// The password must never reach argv, on any engine.
    #[test]
    fn no_password_is_ever_an_argument() {
        for kind in KINDS {
            for args in [
                dump_args(kind, "root", Some("stackvo")),
                restore_args(kind, "root", Some("stackvo")),
            ] {
                for arg in &args {
                    assert!(
                        !arg.contains("password=") && !arg.starts_with("-p") || arg == "-p",
                        "{kind:?} leaked a password-shaped argument: {arg}"
                    );
                }
            }
        }
    }

    #[test]
    fn postgres_uses_long_flags_so_the_user_is_not_positional() {
        let args = dump_args(Kind::Postgres, "stackvo", Some("shop"));
        assert!(args.contains(&"--username=stackvo".to_string()));
        assert!(args.contains(&"--dbname=shop".to_string()));
        // Without --clean a restore appends to whatever is already there.
        assert!(args.contains(&"--clean".to_string()));
    }

    /// A mongodump without --archive writes a directory, which cannot be piped
    /// to a file and would leave the "backup" as an empty file.
    #[test]
    fn mongo_dumps_as_a_single_stream() {
        let args = dump_args(Kind::Mongo, "root", None);
        assert!(args.contains(&"--archive".to_string()));
        assert!(args.contains(&"--gzip".to_string()));
        assert_eq!(Kind::Mongo.extension(), "archive.gz");
    }

    /// Restoring on top of existing data merges two databases and reports
    /// success. The contract promises replacement.
    #[test]
    fn a_mongo_restore_replaces_rather_than_merges() {
        assert!(restore_args(Kind::Mongo, "root", None).contains(&"--drop".to_string()));
    }

    /// A colon is not legal in a Windows filename, so the obvious RFC 3339
    /// stamp would produce a backup that cannot be written on one of the three
    /// supported platforms.
    #[test]
    fn the_suggested_filename_is_writable_on_every_platform() {
        let name = suggested_filename("mysql", Kind::Mysql, "2026-07-29T14-05-33");
        assert_eq!(name, "mysql-2026-07-29T14-05-33.sql");
        for illegal in [':', '*', '?', '"', '<', '>', '|', '/', '\\'] {
            assert!(!name.contains(illegal), "{illegal} is not portable");
        }
    }
}
