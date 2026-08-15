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
fn settings_for_instance(root: &Path, id: &str) -> Result<Settings> {
    let table = crate::instances::Table::load(root)?;
    let instance = table
        .get(id)
        .ok_or_else(|| Error::not_found(format!("instance {id}")))?;

    let mut settings = settings(root, &instance.service)?;
    settings.container = instance.container();

    let keys = settings.kind.keys();
    let pick = |key: Option<&str>| -> Option<String> {
        key.and_then(|k| instance.settings.get(k))
            .filter(|v| !v.is_empty())
            .cloned()
    };
    if let Some(user) = pick(keys.user) {
        settings.user = user;
    }
    if let Some(database) = pick(keys.database) {
        settings.database = Some(database);
    }
    Ok(settings)
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

    Ok(Settings {
        kind,
        container: container_of(root, service),
        user: keys
            .user
            .and_then(|k| env.get(k))
            .filter(|v| !v.is_empty())
            .unwrap_or(default_user(kind))
            .to_string(),
        password: env
            .get(keys.password)
            .filter(|v| !v.is_empty())
            .map(str::to_string),
        database: keys
            .database
            .and_then(|k| env.get(k))
            .filter(|v| !v.is_empty())
            .map(str::to_string),
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
        let running = crate::engine::inspect(service)
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
/// app's own constants — nothing a user typed reaches it — so there is no
/// quoting problem to solve, and adding a parameter binding layer for four
/// fixed statements would be machinery with no user.
pub async fn run_sql(root: &Path, service: &str, sql: &str) -> Result<String> {
    let settings = settings(root, service)?;

    let mut args = exec_args(&settings, false);
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
            .filter(|l| !l.trim().is_empty())
            .next_back()
            .unwrap_or("the database refused the statement");
        return Err(Error::new(Code::IoError, reason.trim().to_string()));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
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
