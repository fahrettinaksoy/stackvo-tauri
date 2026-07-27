//! Reading and validating `projects/<name>/stackvo.json`.
//!
//! Implements `contracts/project.schema.json` — both the normalisation steps in
//! `x-stackvo-read-rules` and the checks the Bash parser performs silently or
//! not at all.
//!
//! Design rule: an invalid manifest is NEVER dropped. The Bash generator skips
//! a project with a missing domain and moves on, so the project simply vanishes
//! from the UI with no explanation. Here it comes back with `valid: false` and
//! the reasons attached, so the user can see what is wrong.

use crate::contracts::{cmp_php_version, php_extensions};
use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::cmp::Ordering;
use std::path::Path;

/// The generator's fallback when `php.extensions` is absent — seven entries,
/// NOT the 33 the UI pre-selects. See CONFLICTS.md C-05.
const GENERATOR_FALLBACK_EXTENSIONS: [&str; 7] = [
    "pdo",
    "pdo_mysql",
    "mysqli",
    "gd",
    "curl",
    "zip",
    "mbstring",
];

const SERVERS: [&str; 5] = ["nginx", "apache", "caddy", "frankenphp", "swoole"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Contract rule id where one applies (`C-01`, `W-01`, …), else a code.
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpConfig {
    pub version: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeConfig {
    pub version: String,
    pub install: String,
    pub build: Option<String>,
    pub start: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub name: String,
    pub domain: Option<String>,
    /// Canonical: `php` or `node`. Absent in the file means `php`.
    pub runtime: String,
    pub server: Option<String>,
    pub document_root: Option<String>,
    pub php: Option<PhpConfig>,
    pub node: Option<NodeConfig>,

    pub valid: bool,
    pub errors: Vec<Finding>,
    pub warnings: Vec<Finding>,
}

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// Read a manifest, normalise it, and collect every contract violation.
///
/// `dir_name` is the containing directory: the contract requires `name` to
/// match it (W-04), because `listProjects` keys containers off the directory.
pub fn read(path: &Path, dir_name: &str) -> Result<Manifest> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;

    let json: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("{} is not valid JSON: {e}", path.display()),
        )
    })?;

    Ok(normalize(&json, &raw, dir_name))
}

pub fn normalize(json: &serde_json::Value, raw: &str, dir_name: &str) -> Manifest {
    let mut errors: Vec<Finding> = Vec::new();
    let mut warnings: Vec<Finding> = Vec::new();

    let mut error = |code: &str, path: &str, msg: String| {
        errors.push(Finding {
            code: code.into(),
            path: path.into(),
            message: msg,
        })
    };

    // ---- name -----------------------------------------------------------
    let name = str_field(json, "name").unwrap_or_else(|| dir_name.to_string());
    if str_field(json, "name").is_none() {
        error("MISSING_NAME", "name", "`name` is required".into());
    } else if name != dir_name {
        error(
            "W-04",
            "name",
            format!("`name` is \"{name}\" but the directory is \"{dir_name}\"; containers are keyed off the directory, so the project would be unreachable"),
        );
    }

    // ---- domain (required; no fallback exists) ---------------------------
    let domain = str_field(json, "domain");
    if domain.is_none() {
        error(
            "MISSING_DOMAIN",
            "domain",
            "`domain` is required — the generator aborts this project without it".into(),
        );
    }

    // ---- runtime, with the legacy aliases ---------------------------------
    let declared = str_field(json, "runtime");
    let runtime = match declared.as_deref() {
        None => {
            warnings.push(Finding {
                code: "RUNTIME_IMPLICIT".into(),
                path: "runtime".into(),
                message: "no `runtime` key; defaulting to \"php\" per the contract".into(),
            });
            "php".to_string()
        }
        Some("php") => "php".to_string(),
        Some("node") => "node".to_string(),
        Some(alias @ ("nodejs" | "js")) => {
            error(
                "C-01",
                "runtime",
                format!("\"{alias}\" is a legacy alias; the canonical id is \"node\""),
            );
            "node".to_string()
        }
        Some(other) => {
            error(
                "C-02",
                "runtime",
                format!("runtime \"{other}\" has no generator — only php and node are implemented"),
            );
            other.to_string()
        }
    };

    // A `nodejs` block is the signature of a manifest written by the web UI,
    // which also omits `runtime` — so it generates as PHP. See C-01.
    if json.get("nodejs").is_some() {
        error(
            "C-01",
            "nodejs",
            "runtime block is named \"nodejs\"; the canonical key is \"node\". Written by the web UI, this manifest generates as PHP and cannot build".into(),
        );
    }
    for orphan in ["python", "ruby", "golang", "go", "rust"] {
        if json.get(orphan).is_some() {
            error(
                "C-02",
                orphan,
                format!("runtime block \"{orphan}\" has no generator"),
            );
        }
    }

    // ---- server / webserver ----------------------------------------------
    let has_server = json.get("server").is_some();
    let has_webserver = json.get("webserver").is_some();
    if has_server && has_webserver {
        error(
            "C-10",
            "server",
            "both `server` and `webserver` are present; emit only `server`".into(),
        );
    } else if has_webserver {
        warnings.push(Finding {
            code: "C-10".into(),
            path: "webserver".into(),
            message: "`webserver` is the deprecated spelling; the canonical field is `server`"
                .into(),
        });
    }

    let server = str_field(json, "server").or_else(|| str_field(json, "webserver"));
    if let Some(s) = &server {
        if !SERVERS.contains(&s.as_str()) {
            error(
                "INVALID_SERVER",
                "server",
                format!("\"{s}\" is not one of {}", SERVERS.join(", ")),
            );
        }
    }

    // ---- runtime blocks ---------------------------------------------------
    let php = (runtime == "php")
        .then(|| read_php(json, &mut errors, &mut warnings))
        .flatten();
    let node = (runtime == "node")
        .then(|| read_node(json, &mut errors, &mut warnings))
        .flatten();

    if runtime == "node" {
        for k in ["server", "webserver", "document_root", "php"] {
            if json.get(k).is_some() {
                warnings.push(Finding {
                    code: "NODE_EXTRA_KEY".into(),
                    path: k.into(),
                    message: format!("`{k}` is ignored when runtime is node"),
                });
            }
        }
    }

    // ---- write rules the Bash parser depends on ---------------------------
    check_extension_layout(raw, &mut errors);

    Manifest {
        name,
        domain,
        runtime,
        server,
        document_root: str_field(json, "document_root"),
        php,
        node,
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

fn read_php(
    json: &serde_json::Value,
    errors: &mut Vec<Finding>,
    warnings: &mut Vec<Finding>,
) -> Option<PhpConfig> {
    let Some(block) = json.get("php") else {
        errors.push(Finding {
            code: "MISSING_PHP_BLOCK".into(),
            path: "php".into(),
            message: "runtime is php but there is no `php` block".into(),
        });
        return None;
    };

    let version = str_field(block, "version").unwrap_or_else(|| {
        errors.push(Finding {
            code: "MISSING_PHP_VERSION".into(),
            path: "php.version".into(),
            message: "`php.version` is required".into(),
        });
        "8.2".to_string()
    });

    if cmp_php_version(&version, "8.0") == Ordering::Less {
        warnings.push(Finding {
            code: "C-13".into(),
            path: "php.version".into(),
            message: format!(
                "PHP {version} is below the v1 floor of 8.0; the extension matrix assumes 8.0+"
            ),
        });
    }

    let extensions = match block.get("extensions").and_then(|v| v.as_array()) {
        None => GENERATOR_FALLBACK_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect(),
        Some(list) => {
            // C-04: the Bash extractor's window is 50 lines.
            if list.len() > 50 {
                errors.push(Finding {
                    code: "C-04".into(),
                    path: "php.extensions".into(),
                    message: format!(
                        "{} extensions; the Bash parser reads only 50, so {} would be silently dropped",
                        list.len(),
                        list.len() - 50
                    ),
                });
            }

            let matrix = &php_extensions().extensions;
            let mut out = Vec::new();
            for item in list {
                let Some(ext) = item.as_str() else {
                    errors.push(Finding {
                        code: "INVALID_EXTENSIONS".into(),
                        path: "php.extensions".into(),
                        message: "extension entries must be strings".into(),
                    });
                    continue;
                };

                if !ext
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                {
                    errors.push(Finding {
                        code: "C-14".into(),
                        path: format!("php.extensions[{ext}]"),
                        message: format!(
                            "\"{ext}\" has characters outside [a-z0-9_]; the Bash extractor cannot match it and drops it silently"
                        ),
                    });
                    continue;
                }

                match matrix.get(ext) {
                    None => errors.push(Finding {
                        code: "UNKNOWN_EXTENSION".into(),
                        path: format!("php.extensions[{ext}]"),
                        message: format!("\"{ext}\" is not in the extension matrix"),
                    }),
                    Some(spec) => {
                        if let Some(removed) = &spec.removed_in {
                            if cmp_php_version(&version, removed) != Ordering::Less {
                                errors.push(Finding {
                                    code: "C-06".into(),
                                    path: format!("php.extensions[{ext}]"),
                                    message: format!(
                                        "\"{ext}\" was removed in PHP {removed} but this project targets {version}; the Bash generator skips it silently"
                                    ),
                                });
                            }
                        }
                        if let Some(min) = &spec.min_php {
                            if cmp_php_version(&version, min) == Ordering::Less {
                                errors.push(Finding {
                                    code: "MIN_PHP".into(),
                                    path: format!("php.extensions[{ext}]"),
                                    message: format!(
                                        "\"{ext}\" needs PHP >= {min}, project targets {version}"
                                    ),
                                });
                            }
                        }
                        if spec.install == "special" {
                            errors.push(Finding {
                                code: "UNSUPPORTED".into(),
                                path: format!("php.extensions[{ext}]"),
                                message: format!("\"{ext}\" needs a bespoke install path that v1 does not implement"),
                            });
                        }
                        if spec.install == "composer" {
                            warnings.push(Finding {
                                code: "C-05".into(),
                                path: format!("php.extensions[{ext}]"),
                                message: format!("\"{ext}\" is a Composer package, not an extension; it produces no install line"),
                            });
                        }
                        out.push(ext.to_string());
                    }
                }
            }
            out
        }
    };

    Some(PhpConfig {
        version,
        extensions,
    })
}

fn read_node(
    json: &serde_json::Value,
    errors: &mut Vec<Finding>,
    warnings: &mut Vec<Finding>,
) -> Option<NodeConfig> {
    let Some(block) = json.get("node") else {
        errors.push(Finding {
            code: "MISSING_NODE_BLOCK".into(),
            path: "node".into(),
            message: "runtime is node but there is no `node` block".into(),
        });
        return None;
    };

    let version = str_field(block, "version").unwrap_or_else(|| {
        errors.push(Finding {
            code: "MISSING_NODE_VERSION".into(),
            path: "node.version".into(),
            message: "`node.version` is required".into(),
        });
        "22".to_string()
    });

    let start = str_field(block, "start").unwrap_or_else(|| "npm start".to_string());

    // Only flag what plausibly binds loopback: an explicit localhost, or a dev
    // server that defaults to it with no --host override.
    let loopback = start.contains("localhost") || start.contains("127.0.0.1");
    let dev_server = [
        "vite",
        "next dev",
        "nuxt dev",
        "npm run dev",
        "yarn dev",
        "pnpm dev",
    ]
    .iter()
    .any(|p| start.contains(p));
    if loopback || (dev_server && !start.contains("--host")) {
        warnings.push(Finding {
            code: "BIND_LOCALHOST".into(),
            path: "node.start".into(),
            message: format!(
                "`{start}` binds loopback by default; Traefik cannot reach it — add --host 0.0.0.0"
            ),
        });
    }

    let port = block.get("port").and_then(|v| v.as_u64()).unwrap_or(3000);
    if port == 0 || port > 65535 {
        errors.push(Finding {
            code: "INVALID_PORT".into(),
            path: "node.port".into(),
            message: format!("`node.port` {port} is out of range"),
        });
    }

    Some(NodeConfig {
        version,
        install: str_field(block, "install").unwrap_or_else(|| "npm install".to_string()),
        build: str_field(block, "build"),
        start,
        port: port.clamp(1, 65535) as u16,
    })
}

/// W-01: `php.extensions` must be the last key in the document.
///
/// The Bash extractor takes every quoted lowercase token in the 50 lines after
/// the `"extensions"` marker, so any key emitted after the array is swallowed
/// into the extension list and reaches `docker-php-ext-install` as a bogus name.
fn check_extension_layout(raw: &str, errors: &mut Vec<Finding>) {
    let Some(marker) = raw.rfind("\"extensions\"") else {
        return;
    };
    let Some(close_rel) = raw[marker..].find(']') else {
        return;
    };
    let close = marker + close_rel;

    let tail = &raw[close + 1..];
    if !tail
        .chars()
        .all(|c| c.is_whitespace() || matches!(c, '}' | ']' | ','))
    {
        errors.push(Finding {
            code: "W-01".into(),
            path: "php.extensions".into(),
            message:
                "keys appear after `php.extensions`; the Bash extractor swallows the following tokens as extension names"
                    .into(),
        });
    }

    let span = raw[marker..close].lines().count();
    if span > 50 {
        errors.push(Finding {
            code: "C-04".into(),
            path: "php.extensions".into(),
            message: format!("the extensions array spans {span} lines; the parser window is 50"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str, dir: &str) -> Manifest {
        let json: serde_json::Value = serde_json::from_str(raw).unwrap();
        normalize(&json, raw, dir)
    }

    #[test]
    fn php_runtime_is_the_default_and_defaults_are_applied() {
        let raw = r#"{
  "name": "shop",
  "domain": "shop.loc",
  "webserver": "nginx",
  "php": { "version": "8.4" }
}"#;
        let m = parse(raw, "shop");
        assert_eq!(m.runtime, "php");
        assert_eq!(m.server.as_deref(), Some("nginx"));
        // Absent extensions yield the SEVEN-entry generator fallback, not the
        // 33 the UI pre-selects (C-05).
        assert_eq!(m.php.unwrap().extensions.len(), 7);
        assert!(m.valid, "{:?}", m.errors);
        assert!(m.warnings.iter().any(|w| w.code == "C-10"));
    }

    #[test]
    fn ui_written_node_manifest_is_caught() {
        // Exactly what ProjectService.createProject writes: a `nodejs` block
        // and no `runtime` key. The Bash generator treats this as PHP 22.
        let raw = r#"{
  "name": "app",
  "domain": "app.loc",
  "server": "nginx",
  "document_root": "public",
  "nodejs": { "version": "22" }
}"#;
        let m = parse(raw, "app");
        assert!(!m.valid);
        assert!(m
            .errors
            .iter()
            .any(|e| e.code == "C-01" && e.path == "nodejs"));
    }

    #[test]
    fn missing_domain_is_an_error_not_a_silent_skip() {
        let raw = r#"{ "name": "x", "php": { "version": "8.4" } }"#;
        let m = parse(raw, "x");
        assert!(!m.valid);
        assert!(m.errors.iter().any(|e| e.code == "MISSING_DOMAIN"));
        // Still returned, so the UI can show the project and explain itself.
        assert_eq!(m.name, "x");
    }

    #[test]
    fn imap_on_php_84_is_rejected() {
        let raw = r#"{
  "name": "legacy",
  "domain": "legacy.loc",
  "php": { "version": "8.4", "extensions": ["mbstring", "imap"] }
}"#;
        let m = parse(raw, "legacy");
        assert!(m.errors.iter().any(|e| e.code == "C-06"));
    }

    #[test]
    fn keys_after_extensions_break_the_bash_parser() {
        let raw = r#"{
  "name": "ordered",
  "domain": "ordered.loc",
  "php": { "version": "8.4", "extensions": ["mbstring"] },
  "document_root": "public"
}"#;
        let m = parse(raw, "ordered");
        assert!(m.errors.iter().any(|e| e.code == "W-01"), "{:?}", m.errors);
    }

    #[test]
    fn name_must_match_the_directory() {
        let raw = r#"{ "name": "other", "domain": "a.loc", "php": { "version": "8.4" } }"#;
        let m = parse(raw, "actual");
        assert!(m.errors.iter().any(|e| e.code == "W-04"));
    }

    #[test]
    fn node_defaults_and_loopback_warning() {
        let raw = r#"{
  "name": "web",
  "domain": "web.loc",
  "runtime": "node",
  "node": { "version": "22", "start": "npm run dev" }
}"#;
        let m = parse(raw, "web");
        assert!(m.valid, "{:?}", m.errors);
        let node = m.node.unwrap();
        assert_eq!(node.port, 3000);
        assert_eq!(node.install, "npm install");
        assert!(m.warnings.iter().any(|w| w.code == "BIND_LOCALHOST"));
    }

    #[test]
    fn explicit_host_flag_suppresses_the_loopback_warning() {
        let raw = r#"{
  "name": "web",
  "domain": "web.loc",
  "runtime": "node",
  "node": { "version": "22", "start": "npm run dev -- --host 0.0.0.0 --port 3000" }
}"#;
        let m = parse(raw, "web");
        assert!(!m.warnings.iter().any(|w| w.code == "BIND_LOCALHOST"));
    }
}

// ---------------------------------------------------------------- writing

/// Serialise a manifest, honouring the write rules in `project.schema.json`.
///
/// Not `serde_json::to_string_pretty`: the Bash parser is line-oriented and
/// order-sensitive, so the layout is part of the contract, not a style choice.
///
///   W-01  `php.extensions` must be the LAST key in the document.
///   W-02  exactly one runtime block.
///   W-03  one array element per line, 2-space indent.
///
/// Getting this wrong does not produce invalid JSON — it produces a file the
/// Bash generator misreads, which is far harder to notice.
pub fn to_json(manifest: &Manifest) -> String {
    let mut out = String::from("{\n");
    let mut lines: Vec<String> = Vec::new();

    let quote = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());

    lines.push(format!("  \"name\": {}", quote(&manifest.name)));
    if let Some(domain) = &manifest.domain {
        lines.push(format!("  \"domain\": {}", quote(domain)));
    }
    // Always explicit, even though readers default it — see CONFLICTS.md C-01.
    lines.push(format!("  \"runtime\": {}", quote(&manifest.runtime)));

    if manifest.runtime == "php" {
        if let Some(server) = &manifest.server {
            // Canonical spelling only; `webserver` is read-support, not output.
            lines.push(format!("  \"server\": {}", quote(server)));
        }
        lines.push(format!(
            "  \"document_root\": {}",
            quote(manifest.document_root.as_deref().unwrap_or("public"))
        ));
    }

    if let Some(node) = &manifest.node {
        let mut block = String::from("  \"node\": {\n");
        let mut fields = vec![
            format!("    \"version\": {}", quote(&node.version)),
            format!("    \"install\": {}", quote(&node.install)),
        ];
        if let Some(build) = &node.build {
            fields.push(format!("    \"build\": {}", quote(build)));
        }
        fields.push(format!("    \"start\": {}", quote(&node.start)));
        fields.push(format!("    \"port\": {}", node.port));
        block.push_str(&fields.join(",\n"));
        block.push_str("\n  }");
        lines.push(block);
    }

    // The php block goes LAST because its `extensions` array must be the final
    // key in the document (W-01).
    if let Some(php) = &manifest.php {
        let mut block = String::from("  \"php\": {\n");
        block.push_str(&format!("    \"version\": {},\n", quote(&php.version)));
        block.push_str("    \"extensions\": [\n");
        let items: Vec<String> = php
            .extensions
            .iter()
            .map(|e| format!("      {}", quote(e)))
            .collect();
        block.push_str(&items.join(",\n"));
        block.push_str("\n    ]\n  }");
        lines.push(block);
    }

    out.push_str(&lines.join(",\n"));
    out.push_str("\n}\n");
    out
}

/// Write a manifest to `<project_dir>/stackvo.json`, refusing anything invalid.
pub fn write(path: &Path, manifest: &Manifest) -> Result<()> {
    if manifest.php.is_some() && manifest.node.is_some() {
        return Err(Error::new(
            Code::InvalidManifest,
            "a manifest may declare only one runtime block (W-02)",
        ));
    }
    if manifest.domain.is_none() {
        return Err(Error::new(Code::InvalidManifest, "`domain` is required"));
    }

    let text = to_json(manifest);

    // Round-trip before touching disk: if our own output does not parse back
    // clean, the bug is here and must not reach the user's project directory.
    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("generated invalid JSON: {e}"),
        )
    })?;
    let check = normalize(&parsed, &text, &manifest.name);
    if !check.valid {
        return Err(Error::new(
            Code::InvalidManifest,
            format!(
                "generated manifest fails validation: {}",
                check
                    .errors
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
    }

    // Atomic: this is a file in the user's repository, and a torn write would
    // leave a project StackVo can no longer read.
    crate::atomic::write(path, &text)
}

#[cfg(test)]
mod write_tests {
    use super::*;

    fn php_manifest() -> Manifest {
        Manifest {
            name: "shop".into(),
            domain: Some("shop.loc".into()),
            runtime: "php".into(),
            server: Some("nginx".into()),
            document_root: Some("public".into()),
            php: Some(PhpConfig {
                version: "8.4".into(),
                extensions: vec!["mbstring".into(), "pdo".into(), "pdo_mysql".into()],
            }),
            node: None,
            valid: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn extensions_are_the_last_key_in_the_document() {
        let text = to_json(&php_manifest());
        let close = text.rfind(']').unwrap();
        let tail = &text[close + 1..];
        assert!(
            tail.chars()
                .all(|c| c.is_whitespace() || matches!(c, '}' | ']' | ',')),
            "W-01 violated, tail was {tail:?}"
        );
    }

    #[test]
    fn output_round_trips_through_the_reader_cleanly() {
        let text = to_json(&php_manifest());
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let back = normalize(&json, &text, "shop");

        assert!(back.valid, "{:?}", back.errors);
        // And no legacy-spelling warning, because we emit the canonical field.
        assert!(!back.warnings.iter().any(|w| w.code == "C-10"));
        assert_eq!(back.php.unwrap().extensions.len(), 3);
    }

    #[test]
    fn runtime_is_always_written_explicitly() {
        // Readers default it, but leaving it out is what makes a UI-written
        // Node project generate as PHP (C-01).
        assert!(to_json(&php_manifest()).contains("\"runtime\": \"php\""));
    }

    #[test]
    fn one_extension_per_line() {
        let text = to_json(&php_manifest());
        for ext in ["mbstring", "pdo", "pdo_mysql"] {
            assert!(
                text.lines()
                    .any(|l| l.trim().trim_end_matches(',') == format!("\"{ext}\"")),
                "{ext} is not on its own line"
            );
        }
    }

    #[test]
    fn node_manifests_omit_php_only_fields() {
        let m = Manifest {
            name: "web".into(),
            domain: Some("web.loc".into()),
            runtime: "node".into(),
            server: None,
            document_root: None,
            php: None,
            node: Some(NodeConfig {
                version: "22".into(),
                install: "npm install".into(),
                build: Some("npm run build".into()),
                start: "node server.js".into(),
                port: 3000,
            }),
            valid: true,
            errors: vec![],
            warnings: vec![],
        };
        let text = to_json(&m);
        assert!(text.contains("\"runtime\": \"node\""));
        assert!(text.contains("\"node\": {"));
        assert!(!text.contains("document_root"));
        assert!(!text.contains("\"server\""));

        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(normalize(&json, &text, "web").valid);
    }

    #[test]
    fn write_refuses_two_runtime_blocks() {
        let mut m = php_manifest();
        m.node = Some(NodeConfig {
            version: "22".into(),
            install: "npm install".into(),
            build: None,
            start: "npm start".into(),
            port: 3000,
        });
        let path = std::env::temp_dir().join("stackvo-write-test.json");
        assert!(write(&path, &m).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
