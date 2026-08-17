//! Writing a service package, rather than only installing one.
//!
//! C-1. Everything a third-party package needs already exists: the format, the
//! validator, the compose policy, and a `local` source that installs from any
//! directory the user points at. What was missing was the act of *authoring*
//! one, and the reason is narrower and more annoying than "there is no editor".
//!
//! ## The obstacle is the sha256 bookkeeping, not the JSON
//!
//! A manifest states the hash of every file the package ships, and
//! [`crate::pkg::verify`] checks them on every read — deliberately, because the
//! point of writing a hash down is to catch the change nobody announced. Which
//! also means that opening `compose.yml`, changing one line and saving it
//! leaves a package that refuses to load, with a message about bytes rather
//! than about the line somebody just edited.
//!
//! A person can compute those by hand. Nobody will, twice. So the surface here
//! is two operations and neither of them is a text editor:
//!
//! * [`scaffold`] writes a package that is valid on the first read — identity,
//!   manifest, compose fragment, hashes already correct.
//! * [`reseal`] recomputes the hashes after somebody has edited the files, and
//!   re-runs every check the app would run at install time.
//!
//! ## Sealing is not a way to skip a check
//!
//! [`reseal`] recomputes hashes and *then* validates: it parses the manifest,
//! runs [`crate::pkg::Manifest::check`], renders the compose fragment past
//! [`crate::compose_policy`], and refuses the whole operation if any of those
//! fail. Writing the hashes of a fragment that the policy would reject would be
//! a tool for producing packages that install and cannot run, which is a worse
//! failure than not having the tool.
//!
//! The order matters and is the only order that works: the policy check reads
//! the file, so hashing first means the manifest describes what was checked.
//!
//! ## This authors a package; it does not publish one
//!
//! There is no signing here and no upload. `docs/servis-market-mimarisi.md`
//! §4.6 is explicit that third-party *distribution* needs a moderation process,
//! a publisher identity and a takedown mechanism, and that opening that gate is
//! a separate decision. Authoring a package for your own machine, or for a
//! mirror your organisation already runs, needs none of them — and the
//! organisation's half of the gate is `policy.market.allowedSources`.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// A package directory as this module found it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub service: String,
    pub version: String,
    /// The package directory, so a message can name a path the user recognises.
    pub dir: String,
    /// Files whose hash the manifest had wrong, by relative path.
    ///
    /// Empty after a [`reseal`]. Non-empty from a [`lint`] means "this is what
    /// sealing would change", which is the question somebody actually has.
    pub resealed: Vec<String>,
    /// Everything wrong that sealing cannot fix.
    pub problems: Vec<String>,
}

impl Report {
    pub fn is_valid(&self) -> bool {
        self.problems.is_empty()
    }
}

/// The files a scaffolded package is made of, relative to the version dir.
const MANIFEST: &str = "manifest.json";
const COMPOSE: &str = "compose.yml";

/// Write a new package that is valid the first time it is read.
///
/// `root` is the directory that will hold `packages/`, so a scaffold lands
/// where [`crate::pkg::Tree`] would look for it — a package written somewhere
/// the tree does not scan is one the user then has to be told how to move.
///
/// Refuses to overwrite. A scaffold that clobbered an existing package would
/// destroy exactly the work this module exists to make possible.
pub fn scaffold(
    root: &Path,
    category: &str,
    service: &str,
    version: &str,
    image: &str,
) -> Result<Report> {
    if !is_label(service) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("{service:?} is not a DNS label; a service id becomes a container name"),
        ));
    }
    if !is_label(category) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("{category:?} is not a valid category"),
        ));
    }
    if version.is_empty() || version.contains('/') || version.starts_with('.') {
        return Err(Error::new(
            Code::InvalidInput,
            format!("{version:?} is not a version; it becomes a directory name"),
        ));
    }

    let (repository, tag) = image
        .rsplit_once(':')
        .ok_or_else(|| {
            Error::new(
                Code::InvalidInput,
                format!("{image:?} names no tag; a package pins the image it runs"),
            )
        })
        .map(|(repo, tag)| (repo.to_string(), tag.to_string()))?;

    let service_dir = root.join("packages").join(category).join(service);
    let version_dir = service_dir.join("versions").join(version);
    if version_dir.exists() {
        return Err(Error::new(
            Code::Conflict,
            format!("{} already exists", version_dir.display()),
        ));
    }

    std::fs::create_dir_all(&version_dir)
        .map_err(|e| Error::io(format!("creating {}", version_dir.display()), e))?;

    // The identity is per service, so an existing one is left alone: adding
    // 8.4 to a package that already ships 8.0 must not rewrite its name,
    // summary or recommended version.
    let identity_path = service_dir.join("package.json");
    if !identity_path.is_file() {
        crate::atomic::write(&identity_path, &identity_json(service, category, version))?;
    }

    crate::atomic::write(&version_dir.join(COMPOSE), &compose_yml(service))?;
    crate::atomic::write(
        &version_dir.join(MANIFEST),
        &manifest_json(service, version, &repository, &tag),
    )?;

    // Sealed rather than hashed inline, so a scaffold goes through exactly the
    // checks an edited package does. A template that was valid when it was
    // written and stopped being valid when a rule changed is a template nobody
    // finds out about until a user does.
    reseal(&version_dir)
}

/// Recompute the manifest's hashes, then check the package as if installing it.
///
/// The two halves are not separable and the order is not arbitrary — see the
/// module comment. A package that fails a check is reported and **not**
/// written: sealing is a bookkeeping tool, not a way past the validator.
pub fn reseal(version_dir: &Path) -> Result<Report> {
    let manifest_path = version_dir.join(MANIFEST);
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| Error::io(format!("reading {}", manifest_path.display()), e))?;

    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("{}: {e}", manifest_path.display()),
        )
    })?;

    let mut resealed = Vec::new();
    let mut problems = Vec::new();

    // Every place the format states a hash, in one list, so a manifest that
    // grows a fourth is a change here rather than a hash nobody recomputes.
    let slots: Vec<(Vec<String>, String)> = hash_slots(&value);
    for (path, file) in slots {
        let full = version_dir.join(&file);
        match std::fs::read(&full) {
            Ok(bytes) => {
                let actual = crate::pkg::sha256_hex(&bytes);
                if set_hash(&mut value, &path, &actual) {
                    resealed.push(file);
                }
            }
            Err(e) => problems.push(format!("{file}: {e}")),
        }
    }

    let sealed = serde_json::to_string_pretty(&value)
        .map_err(|e| Error::new(Code::InvalidManifest, format!("re-serialising: {e}")))?;

    // Parsed from the sealed text, not from `value`: what gets validated has to
    // be the bytes that would be written, or the check is of something else.
    let manifest = match crate::pkg::parse(&sealed) {
        Ok(manifest) => Some(manifest),
        Err(e) => {
            problems.push(e.message);
            None
        }
    };

    if let Some(manifest) = &manifest {
        // The policy, run on the template with its placeholders stubbed.
        //
        // This checks the half an author can act on — the *keys*. `privileged`,
        // `userns_mode`, `volumes_from` and every key nobody has considered are
        // refused here, at the moment somebody wrote them, rather than on a
        // stranger's machine at install time.
        //
        // It cannot check the other half, and pretending otherwise would be the
        // dishonest version of this tool. The policy's value rules ask whether a
        // mount source is one the *renderer* produced and whether `image:` is
        // the reference the app assembled — and neither of those values exists
        // until there is an instance to render for. So the stub stands in for
        // both, which means a fragment that hardcodes an image or a host path
        // is still caught (it is not the stub) while a fragment that uses the
        // declared names passes.
        //
        // The install-time check in `render.rs` remains the one that decides
        // whether this machine runs the thing. This one decides whether the
        // author has to find out from a user.
        match std::fs::read_to_string(version_dir.join(&manifest.compose.file)) {
            Ok(fragment) => {
                let who = format!("{}@{}", manifest.service, manifest.version);
                let stubbed = stub_placeholders(&fragment);
                let allowed = crate::compose_policy::Allowed {
                    image: STUB.to_string(),
                    mounts: [STUB.to_string()].into_iter().collect(),
                };
                if let Err(e) = crate::compose_policy::check(&who, &stubbed, &allowed) {
                    problems.push(e.message);
                }
            }
            Err(e) => problems.push(format!("{}: {e}", manifest.compose.file)),
        }
    }

    let report = Report {
        service: manifest
            .as_ref()
            .map(|m| m.service.clone())
            .unwrap_or_default(),
        version: manifest
            .as_ref()
            .map(|m| m.version.clone())
            .unwrap_or_default(),
        dir: version_dir.display().to_string(),
        resealed,
        problems,
    };

    if report.is_valid() {
        crate::atomic::write(&manifest_path, &format!("{sealed}\n"))?;
    }
    Ok(report)
}

/// The same checks with nothing written — "what would sealing change, and what
/// would still be wrong afterwards".
pub fn lint(version_dir: &Path) -> Result<Report> {
    // Read-only by copying the directory's manifest into a scratch value rather
    // than by threading a `dry_run` flag through `reseal`: a flag would mean
    // the checked path and the written path could drift, and this is the one
    // place where "what it reported" and "what it wrote" must be the same code.
    let manifest_path = version_dir.join(MANIFEST);
    let before = std::fs::read_to_string(&manifest_path)
        .map_err(|e| Error::io(format!("reading {}", manifest_path.display()), e))?;

    let report = reseal(version_dir)?;

    // Put it back if sealing wrote. `reseal` only writes a valid package, so
    // this restores the file somebody has not asked to change yet.
    if report.is_valid() && !report.resealed.is_empty() {
        crate::atomic::write(&manifest_path, &before)?;
    }
    Ok(report)
}

/// Replace every `{{ name }}` with [`STUB`], and report an unclosed one.
///
/// An unclosed brace is left as it is rather than repaired: `render.rs` refuses
/// it with a message about the fragment, and this file producing a *different*
/// answer to the same malformed input is how two validators come to disagree.
fn stub_placeholders(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut rest = fragment;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find("}}") else {
            out.push_str(rest);
            return out;
        };
        out.push_str(STUB);
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

fn is_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.ends_with('-')
}

/// Where a hash lives in a manifest document, as a JSON pointer path, with the
/// file it describes.
fn hash_slots(value: &serde_json::Value) -> Vec<(Vec<String>, String)> {
    let mut out = Vec::new();

    if let Some(file) = value.pointer("/compose/file").and_then(|v| v.as_str()) {
        out.push((vec!["compose".into(), "sha256".into()], file.to_string()));
    }
    if let Some(files) = value.get("files").and_then(|v| v.as_array()) {
        for (index, entry) in files.iter().enumerate() {
            if let Some(template) = entry.get("template").and_then(|v| v.as_str()) {
                out.push((
                    vec!["files".into(), index.to_string(), "sha256".into()],
                    template.to_string(),
                ));
            }
        }
    }
    if let Some(companions) = value.get("companions").and_then(|v| v.as_array()) {
        for (index, entry) in companions.iter().enumerate() {
            if let Some(file) = entry.pointer("/compose/file").and_then(|v| v.as_str()) {
                out.push((
                    vec![
                        "companions".into(),
                        index.to_string(),
                        "compose".into(),
                        "sha256".into(),
                    ],
                    file.to_string(),
                ));
            }
        }
    }
    out
}

/// Write a hash at a path, returning whether it differed from what was there.
fn set_hash(value: &mut serde_json::Value, path: &[String], hash: &str) -> bool {
    let mut node = value;
    for key in &path[..path.len() - 1] {
        node = match node {
            serde_json::Value::Array(items) => match key.parse::<usize>() {
                Ok(index) if index < items.len() => &mut items[index],
                _ => return false,
            },
            serde_json::Value::Object(map) => match map.get_mut(key) {
                Some(next) => next,
                None => return false,
            },
            _ => return false,
        };
    }

    let last = &path[path.len() - 1];
    let Some(map) = node.as_object_mut() else {
        return false;
    };
    let changed = map.get(last).and_then(|v| v.as_str()) != Some(hash);
    map.insert(last.clone(), serde_json::json!(hash));
    changed
}

// -------------------------------------------------------------- the template

fn identity_json(service: &str, category: &str, version: &str) -> String {
    format!(
        r#"{{
  "apiVersion": "{api}",
  "service": "{service}",
  "category": "{category}",
  "name": {{ "en": "{service}" }},
  "summary": {{ "en": "" }},
  "recommendedVersion": "{version}"
}}
"#,
        api = crate::pkg::API_VERSION
    )
}

/// A fragment that starts, answers on a port and says nothing the policy
/// forbids — the shortest thing that is a real service rather than an example
/// somebody has to finish before it does anything.
///
/// **One service body, at column zero, with no `services:` header.** The
/// renderer indents it into place, and a fragment that carried its own header
/// would be indented twice. `docs/servis-market-mimarisi.md` §3.3.
fn compose_yml(service: &str) -> String {
    format!(
        r#"# The compose fragment for {service}. One service body — the renderer
# supplies the `services:` header and the instance key.
#
# `contracts/compose-policy.json` says what may appear here; anything it does
# not list is refused, so a key nobody has considered fails closed rather than
# reaching the daemon.
#
# The only names available are the ones the manifest declares: image, instance,
# settings.*, port.*, volume.*, file.* and network. A fragment cannot read the
# process environment, which is the first line of defence in §4 and not a
# convenience limit.
image: "{{{{ image }}}}"
container_name: "{{{{ instance.container }}}}"
restart: unless-stopped
ports:
  - "{{{{ port.main }}}}:80"
networks:
  {{{{ network }}}}:
    aliases: {{{{ instance.aliases }}}}
"#
    )
}

/// A stand-in for every `{{{{ … }}}}` while checking a fragment nobody has
/// rendered yet.
const STUB: &str = "STACKVO_PLACEHOLDER";

fn manifest_json(service: &str, version: &str, repository: &str, tag: &str) -> String {
    // The hashes are placeholders: `scaffold` seals immediately, so the file
    // that reaches disk carries the real ones. Written as an obviously wrong
    // value rather than an empty string, so a manifest that somehow escaped the
    // seal fails the length check instead of looking half-filled.
    format!(
        r#"{{
  "apiVersion": "{api}",
  "service": "{service}",
  "version": "{version}",
  "image": {{
    "repository": "{repository}",
    "tag": "{tag}"
  }},
  "instancing": {{
    "multiple": true
  }},
  "ports": [
    {{
      "name": "main",
      "container": 80,
      "preferred": 8080,
      "primary": true
    }}
  ],
  "compose": {{
    "file": "{compose}",
    "sha256": "unsealed"
  }},
  "support": {{
    "status": "supported"
  }}
}}
"#,
        api = crate::pkg::API_VERSION,
        compose = COMPOSE
    )
}

/// A scratch directory for a caller that has no workspace — used by the
/// command layer, which validates a directory the user picked.
pub fn version_dir(root: &Path, category: &str, service: &str, version: &str) -> PathBuf {
    root.join("packages")
        .join(category)
        .join(service)
        .join("versions")
        .join(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock is gone from this on purpose — see `idle.rs`'s `workspace`.
    ///
    /// `SystemTime::now().as_nanos()` reads as a unique value and is not one:
    /// it is quantised to a microsecond, and parallel test threads inside the
    /// same one collide. It was harmless here because `name` was already doing
    /// the work, which is exactly how the idiom spread to two helpers where
    /// `name` was absent and the collision was real.
    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stackvo-authoring-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The property the whole module is for: what it writes loads, and the
    /// hashes are right without anybody computing one.
    #[test]
    fn a_scaffolded_package_is_valid_on_the_first_read() {
        let root = temp("scaffold");
        let report = scaffold(&root, "databases", "widget", "1.0", "widget:1.0").unwrap();
        assert!(report.is_valid(), "{:?}", report.problems);

        let tree = crate::pkg::Tree::open(&root).unwrap();
        let manifest = tree.load("widget", "1.0").expect("the tree must load it");
        assert_eq!(manifest.image.repository, "widget");
        assert_eq!(manifest.image.tag, "1.0");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Editing the fragment breaks the package, and that is the problem this
    /// exists to solve rather than a defect.
    #[test]
    fn editing_a_file_breaks_the_package_and_sealing_fixes_it() {
        let root = temp("reseal");
        scaffold(&root, "databases", "widget", "1.0", "widget:1.0").unwrap();
        let dir = version_dir(&root, "databases", "widget", "1.0");

        let compose = dir.join(COMPOSE);
        let text = std::fs::read_to_string(&compose).unwrap();
        std::fs::write(&compose, text.replace("unless-stopped", "always")).unwrap();

        let tree = crate::pkg::Tree::open(&root).unwrap();
        assert!(
            tree.load("widget", "1.0").is_err(),
            "an edited file must fail verification"
        );

        let report = reseal(&dir).unwrap();
        assert!(report.is_valid(), "{:?}", report.problems);
        assert_eq!(report.resealed, vec![COMPOSE.to_string()]);

        let tree = crate::pkg::Tree::open(&root).unwrap();
        assert!(tree.load("widget", "1.0").is_ok());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Sealing must not be a way past the validator — the failure this module
    /// would be worse than useless for having.
    #[test]
    fn a_fragment_the_policy_refuses_is_not_sealed() {
        let root = temp("policy");
        scaffold(&root, "databases", "widget", "1.0", "widget:1.0").unwrap();
        let dir = version_dir(&root, "databases", "widget", "1.0");

        let compose = dir.join(COMPOSE);
        let text = std::fs::read_to_string(&compose).unwrap();
        // Column zero: the fragment is one service body, so a key of the
        // service is unindented. Indented, it belongs to the block above it and
        // the policy would rightly not read it as a service key.
        std::fs::write(&compose, format!("{text}privileged: true\n")).unwrap();

        let report = reseal(&dir).unwrap();
        assert!(!report.is_valid(), "privileged must be refused");

        // And the manifest still holds the old hash, so nothing downstream
        // believes the package is intact.
        let manifest = std::fs::read_to_string(dir.join(MANIFEST)).unwrap();
        let sealed: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        let stated = sealed.pointer("/compose/sha256").unwrap().as_str().unwrap();
        let actual = crate::pkg::sha256_hex(&std::fs::read(&compose).unwrap());
        assert_ne!(stated, actual);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `lint` answers "what would change" without changing it.
    #[test]
    fn lint_reports_what_sealing_would_do_and_writes_nothing() {
        let root = temp("lint");
        scaffold(&root, "databases", "widget", "1.0", "widget:1.0").unwrap();
        let dir = version_dir(&root, "databases", "widget", "1.0");

        let compose = dir.join(COMPOSE);
        let text = std::fs::read_to_string(&compose).unwrap();
        std::fs::write(&compose, text.replace("unless-stopped", "always")).unwrap();

        let before = std::fs::read_to_string(dir.join(MANIFEST)).unwrap();
        let report = lint(&dir).unwrap();
        assert_eq!(report.resealed, vec![COMPOSE.to_string()]);
        assert_eq!(std::fs::read_to_string(dir.join(MANIFEST)).unwrap(), before);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Adding a version to a package must not rewrite the identity somebody
    /// already filled in.
    #[test]
    fn a_second_version_leaves_the_identity_alone() {
        let root = temp("identity");
        scaffold(&root, "databases", "widget", "1.0", "widget:1.0").unwrap();

        let identity = root.join("packages/databases/widget/package.json");
        let edited = std::fs::read_to_string(&identity).unwrap().replace(
            r#""summary": { "en": "" }"#,
            r#""summary": { "en": "mine" }"#,
        );
        std::fs::write(&identity, &edited).unwrap();

        scaffold(&root, "databases", "widget", "2.0", "widget:2.0").unwrap();
        assert_eq!(std::fs::read_to_string(&identity).unwrap(), edited);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scaffolding_over_an_existing_version_is_refused() {
        let root = temp("clobber");
        scaffold(&root, "databases", "widget", "1.0", "widget:1.0").unwrap();
        assert!(scaffold(&root, "databases", "widget", "1.0", "widget:1.0").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A service id becomes a container name; the refusal has to be at the door.
    #[test]
    fn an_id_that_is_not_a_dns_label_is_refused() {
        let root = temp("id");
        for bad in ["Widget", "wid_get", "1widget", "widget-", ""] {
            assert!(
                scaffold(&root, "databases", bad, "1.0", "x:1").is_err(),
                "{bad:?} was accepted"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A package pins the image it runs — a floating reference is the thing
    /// `is_moving_tag` exists to complain about, and a missing one is worse.
    #[test]
    fn an_image_with_no_tag_is_refused() {
        let root = temp("tag");
        assert!(scaffold(&root, "databases", "widget", "1.0", "widget").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
