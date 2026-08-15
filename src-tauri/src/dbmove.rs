//! Moving a database's contents from one instance to another.
//!
//! G-4. Instances made "MySQL 8.0 and MySQL 8.4 side by side" possible, which
//! immediately made "and now get my data into the new one" the obvious next
//! question — and the answer was dump to a file, find the file, restore into
//! the other one, three screens apart. This is that, as one operation, with the
//! part nobody can check by eye checked first.
//!
//! ## The compatibility verdict is the feature
//!
//! Dumping and restoring is two calls this app already has. What it did not
//! have is an answer to "will this work", and that answer is not obvious:
//!
//! * **Same engine, newer target** — the case people actually have, and the one
//!   that works. `mysqldump` output from 8.0 restores into 8.4.
//! * **Same engine, older target** — a downgrade, and the direction that breaks
//!   quietly. A dump from a newer server can use syntax, charsets and defaults
//!   the older one does not have, and the failure arrives partway through a
//!   restore that has already dropped the old data. Allowed, loudly.
//! * **MySQL ↔ MariaDB** — the same dump format in practice and genuinely
//!   different servers. Allowed with a warning, because refusing it would be
//!   refusing a thing that usually works and that nothing else here can do.
//! * **Anything else** — refused. A `mysqldump` file is not Postgres SQL and
//!   never becomes it by being fed to `psql`. A tool that accepted the request
//!   and produced a half-populated database would be worse than one that says
//!   no: the failure would arrive as thousands of syntax errors, after the
//!   target was already emptied.
//!
//! ## The target is emptied, and that is said before it happens
//!
//! `restore` replaces what is there. This is a plan-then-apply pair for exactly
//! that reason — the same shape as `hosts_plan`/`hosts_apply` — so the sentence
//! "everything in `mysql-8-4` will be replaced" is on screen before anybody
//! presses anything.
//!
//! ## Through a file, not a pipe
//!
//! The dump is written to a temp file and then restored from it. A pipe would
//! be tidier and would mean a failure halfway through leaves a target that is
//! neither the old data nor the new, with nothing to retry from. The file is
//! removed afterwards; on failure it is **kept**, and the error says where it
//! is, because at that point it is the only copy of the source that is not
//! inside a container.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// What moving from one instance to another would do.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub from: String,
    pub to: String,
    /// The engine on each side, so the screen can name it rather than the id.
    pub from_service: String,
    pub to_service: String,
    pub from_version: String,
    pub to_version: String,
    /// Whether this may proceed at all.
    pub possible: bool,
    /// Why not, when it may not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    /// True and not blocking. A downgrade is the important one.
    pub warnings: Vec<String>,
}

/// Read the two instances and decide.
///
/// No side effects and no container is touched, which is what makes it safe to
/// call while somebody is still choosing from a dropdown.
pub fn plan(root: &Path, from: &str, to: &str) -> Result<Plan> {
    let table = crate::instances::Table::load(root)?;

    let source = table
        .get(from)
        .ok_or_else(|| Error::not_found(format!("instance {from}")))?
        .clone();
    let target = table
        .get(to)
        .ok_or_else(|| Error::not_found(format!("instance {to}")))?
        .clone();

    let mut plan = Plan {
        from: source.id.clone(),
        to: target.id.clone(),
        from_service: source.service.clone(),
        to_service: target.service.clone(),
        from_version: source.version.clone(),
        to_version: target.version.clone(),
        possible: false,
        refused: None,
        warnings: Vec::new(),
    };

    if source.id == target.id {
        plan.refused = Some("the source and the target are the same instance".into());
        return Ok(plan);
    }

    let (Some(from_kind), Some(to_kind)) = (
        crate::db::Kind::from_service(&source.service),
        crate::db::Kind::from_service(&target.service),
    ) else {
        plan.refused = Some(format!(
            "{} or {} is not a database this app can dump",
            source.service, target.service
        ));
        return Ok(plan);
    };

    match compatible(from_kind, to_kind) {
        Compat::Same => {}
        Compat::Dialect(note) => plan.warnings.push(note),
        Compat::No(why) => {
            plan.refused = Some(why);
            return Ok(plan);
        }
    }

    // The direction that breaks quietly, and the only place this app can say so
    // before it happens.
    if is_downgrade(&source.version, &target.version) {
        plan.warnings.push(format!(
            "{} is older than {}: a dump from a newer server can use syntax and defaults \
             the older one does not have, and that failure arrives partway through a restore \
             that has already replaced the target's data",
            target.version, source.version
        ));
    }

    plan.warnings.push(format!(
        "everything in {} will be replaced by the contents of {}",
        target.id, source.id
    ));

    plan.possible = true;
    Ok(plan)
}

enum Compat {
    Same,
    Dialect(String),
    No(String),
}

fn compatible(from: crate::db::Kind, to: crate::db::Kind) -> Compat {
    use crate::db::Kind::*;
    match (from, to) {
        (a, b) if a == b => Compat::Same,
        // The same dump format in practice, and genuinely different servers —
        // MariaDB has diverged on JSON, sequences and some functions. Refusing
        // it would refuse a thing that usually works and that nothing else here
        // can do.
        (Mysql, Mariadb) | (Mariadb, Mysql) => Compat::Dialect(
            "MySQL and MariaDB read the same dump format but are not the same server: \
             JSON columns, sequences and a handful of functions differ, so check the \
             result rather than assuming it"
                .into(),
        ),
        (a, b) => Compat::No(format!(
            "a {} dump is not {} input and does not become it by being fed to one — \
             this would empty {} and then fail on thousands of syntax errors",
            a.as_str(),
            b.as_str(),
            b.as_str()
        )),
    }
}

/// Is `to` an older version than `from`?
///
/// Compared numerically, segment by segment, and a version this cannot parse is
/// **not** reported as a downgrade. Guessing wrong in that direction produces a
/// warning about a risk that is not there, and a warning that fires on things
/// that are fine is one people stop reading — which costs the real one.
fn is_downgrade(from: &str, to: &str) -> bool {
    let parts = |v: &str| -> Vec<u32> {
        v.split(['.', '-'])
            .map_while(|part| part.parse::<u32>().ok())
            .collect()
    };
    let (a, b) = (parts(from), parts(to));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return y < x;
        }
    }
    false
}

/// Where the intermediate dump is written.
///
/// Named after both instances so two moves at once cannot land on one file, and
/// kept on failure — see the module comment.
pub fn staging(from: &str, to: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!("stackvo-move-{from}-to-{to}.{extension}"))
}

/// What a completed move did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    pub from: String,
    pub to: String,
    /// The size of the dump that crossed, which is the only number that says
    /// anything actually moved.
    pub bytes: u64,
}

/// Dump from one instance and restore into the other.
///
/// Refuses on its own rather than trusting the caller's plan: the plan crossed
/// an IPC boundary and came back, and a check that only runs on the way out is
/// not a check.
pub async fn run<F>(root: &Path, from: &str, to: &str, mut on_line: F) -> Result<Moved>
where
    F: FnMut(String) + Send + Clone + 'static,
{
    let plan = plan(root, from, to)?;
    if !plan.possible {
        return Err(Error::new(
            Code::Unsupported,
            plan.refused
                .unwrap_or_else(|| "this move is not possible".into()),
        ));
    }

    let kind = crate::db::Kind::from_service(&plan.from_service)
        .ok_or_else(|| Error::new(Code::Unsupported, "not a database this app can dump"))?;
    let path = staging(from, to, kind.extension());

    on_line(format!("dumping {from}"));
    let bytes = crate::db::dump_instance(root, from, &path, on_line.clone()).await?;

    on_line(format!("restoring into {to} ({bytes} bytes)"));
    match crate::db::restore_instance(root, to, &path, on_line.clone()).await {
        Ok(_) => {
            let _ = std::fs::remove_file(&path);
            Ok(Moved {
                from: from.to_string(),
                to: to.to_string(),
                bytes,
            })
        }
        Err(e) => {
            // Kept, and named. At this moment it is the only copy of the source
            // that is not inside a container, and the target has already been
            // replaced — deleting it would be deleting the way back.
            Err(Error::new(
                Code::GenerateFailed,
                format!(
                    "{}. The dump was kept at {} — {to} has already been replaced, so this \
                     file is the way back",
                    e.message,
                    path.display()
                ),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Kind;

    fn verdict(from: Kind, to: Kind) -> &'static str {
        match compatible(from, to) {
            Compat::Same => "same",
            Compat::Dialect(_) => "dialect",
            Compat::No(_) => "no",
        }
    }

    /// The case people actually have.
    #[test]
    fn the_same_engine_is_always_allowed() {
        for kind in [Kind::Mysql, Kind::Mariadb, Kind::Postgres, Kind::Mongo] {
            assert_eq!(verdict(kind, kind), "same", "{kind:?}");
        }
    }

    /// Refusing this would refuse a thing that usually works.
    #[test]
    fn mysql_and_mariadb_are_allowed_with_a_warning_in_both_directions() {
        assert_eq!(verdict(Kind::Mysql, Kind::Mariadb), "dialect");
        assert_eq!(verdict(Kind::Mariadb, Kind::Mysql), "dialect");
    }

    /// The failure this refusal exists to prevent is not "it does not work" —
    /// it is "the target is emptied and then it does not work".
    #[test]
    fn crossing_engine_families_is_refused_and_the_message_says_why() {
        assert_eq!(verdict(Kind::Mysql, Kind::Postgres), "no");
        assert_eq!(verdict(Kind::Postgres, Kind::Mongo), "no");
        assert_eq!(verdict(Kind::Mongo, Kind::Mysql), "no");

        let Compat::No(why) = compatible(Kind::Mysql, Kind::Postgres) else {
            panic!("expected a refusal");
        };
        assert!(why.contains("empty"), "{why}");
    }

    // ---- the version comparison -------------------------------------------

    #[test]
    fn an_older_target_is_a_downgrade() {
        assert!(is_downgrade("8.4", "8.0"));
        assert!(is_downgrade("8.0.35", "8.0.30"));
        assert!(is_downgrade("16", "15"));
    }

    #[test]
    fn an_equal_or_newer_target_is_not() {
        assert!(!is_downgrade("8.0", "8.4"));
        assert!(!is_downgrade("8.0", "8.0"));
        assert!(!is_downgrade("8.0", "8.0.1"));
    }

    /// A warning that fires on things that are fine is one people stop reading,
    /// which costs the real one.
    #[test]
    fn a_version_that_cannot_be_compared_is_not_called_a_downgrade() {
        assert!(!is_downgrade("latest", "8.0"));
        assert!(!is_downgrade("8.0", "alpine"));
        assert!(!is_downgrade("", "8.0"));
    }

    /// Two moves at once must not write to one file.
    #[test]
    fn the_staging_file_is_named_after_both_ends() {
        let a = staging("mysql-8-0", "mysql-8-4", "sql");
        let b = staging("mysql-8-0", "mariadb-11-4", "sql");
        assert_ne!(a, b);
        assert!(a.to_string_lossy().contains("mysql-8-0"));
        assert!(a.to_string_lossy().contains("mysql-8-4"));
    }

    /// Mongo's dump is a gzipped archive, and a `.sql` name for it would be a
    /// file nothing can open by its extension.
    #[test]
    fn the_staging_file_carries_the_engines_own_extension() {
        assert!(staging("a", "b", Kind::Mongo.extension())
            .to_string_lossy()
            .ends_with(".archive.gz"));
    }
}
