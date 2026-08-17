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
    /// Is step debugging switched *on*, as distinct from the extension being
    /// compiled in?
    ///
    /// Two different things, and conflating them cost a rebuild per toggle.
    /// `extensions` decides what the image carries; this decides what
    /// `XDEBUG_MODE` the container starts with. Measured before the split: an
    /// image carrying Xdebug at `mode=off` runs at the same speed as one
    /// without it, while `mode=debug` costs about 6.7× on a call-heavy
    /// benchmark — so the extension can stay compiled in for nothing, and
    /// turning debugging on becomes a container recreate instead of an image
    /// rebuild.
    pub xdebug: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeConfig {
    pub version: String,
    pub install: String,
    pub build: Option<String>,
    pub start: String,
    pub port: u16,
    /// `npm`, `yarn` or `pnpm` — J-2, and `None` is load-bearing.
    ///
    /// Absent means the image is built exactly as it has always been built: no
    /// `corepack` line, `npm install`, `npm start`. `fixtures_differential.rs`
    /// compares the generated tree against output frozen from the Bash
    /// generator, so a default that quietly enabled anything would fail that
    /// comparison — correctly, because it would be a different image for every
    /// project that never asked for one.
    ///
    /// Naming one turns Corepack on, and that is the whole feature. Corepack is
    /// what makes `"packageManager": "pnpm@9.1.0"` in `package.json` mean
    /// something: without it the field is a comment, and the image installs
    /// with whatever version the base image happens to ship.
    pub package_manager: Option<String>,
}

/// The package managers Corepack can pin, and what each one calls its verbs.
///
/// Yarn and pnpm are here because Corepack ships shims for them; Bun is not,
/// because it is a runtime of its own in this app and not a way of installing
/// for Node. npm is included even though the base image already has it — the
/// point is not having the tool, it is having the *pinned version*, and that
/// only happens through Corepack.
pub const NODE_PACKAGE_MANAGERS: [&str; 3] = ["npm", "yarn", "pnpm"];

/// The runtimes that share one config shape: a container built from the
/// language's own image, `COPY . .`, an optional install and build step, and a
/// start command on a port Traefik proxies to. Node predates this list and
/// keeps its own struct for compatibility; structurally it is the same idea.
pub const LANG_RUNTIMES: [&str; 6] = ["python", "go", "ruby", "rust", "bun", "deno"];

/// One non-PHP, non-node runtime block — `python: { … }`, `go: { … }`, ….
///
/// `install` and `build` are optional because the interpreted/compiled split
/// is real: Python and Ruby install dependencies and run source, Go and Rust
/// compile — and a field that does not apply should be absent, not an empty
/// string someone has to know to leave alone.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LangConfig {
    pub version: String,
    pub install: Option<String>,
    pub build: Option<String>,
    pub start: String,
    pub port: u16,
}

/// What a runtime's block defaults to when a field is omitted — the working
/// convention of each ecosystem, not an invention.
pub fn lang_defaults(runtime: &str) -> Option<LangConfig> {
    match runtime {
        "python" => Some(LangConfig {
            version: "3.13".into(),
            install: Some("pip install --no-cache-dir -r requirements.txt".into()),
            build: None,
            start: "python main.py".into(),
            port: 8000,
        }),
        "go" => Some(LangConfig {
            version: "1.23".into(),
            install: None,
            build: Some("go build -o /app/server .".into()),
            start: "/app/server".into(),
            port: 8080,
        }),
        "ruby" => Some(LangConfig {
            version: "3.3".into(),
            install: Some("bundle install".into()),
            build: None,
            start: "bundle exec ruby app.rb".into(),
            port: 4567,
        }),
        // `cargo run --release` reuses what `cargo build --release` produced,
        // so the CMD does not compile twice — and it works whatever the
        // binary is called, which a `./target/release/<name>` guess does not.
        "rust" => Some(LangConfig {
            version: "1".into(),
            install: None,
            build: Some("cargo build --release".into()),
            start: "cargo run --release".into(),
            port: 8080,
        }),
        // `bun install` and `bun run start` rather than the npm spellings: Bun
        // reads the same `package.json` scripts, and using its own verbs is
        // what makes the lockfile it writes (`bun.lock`) the one it reads back.
        // 3000 is what `Bun.serve` listens on with nothing configured.
        "bun" => Some(LangConfig {
            version: "1".into(),
            install: Some("bun install".into()),
            build: None,
            start: "bun run start".into(),
            port: 3000,
        }),
        // The one runtime here pinned to a patch version, and not by choice:
        // `denoland/deno` publishes no major or minor tag at all. `deno:2` and
        // `deno:2.9` are both absent from the registry — checked, not assumed —
        // so a manifest saying `"version": "2"` would build against an image
        // that does not exist. Every other entry in this table can float
        // because its publisher lets it.
        //
        // `deno install` with no arguments is Deno 2's "resolve what the
        // manifest asks for", so dependencies land at build time rather than on
        // the first request.
        "deno" => Some(LangConfig {
            version: "2.9.5".into(),
            install: Some("deno install".into()),
            build: None,
            start: "deno task start".into(),
            port: 8000,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub name: String,
    pub domain: Option<String>,
    /// Canonical: `php`, `node`, `python`, `go`, `ruby` or `rust`. Absent in
    /// the file means `php`.
    pub runtime: String,
    pub server: Option<String>,
    pub document_root: Option<String>,
    /// Extra hostnames the same project answers on, beside [`Self::domain`].
    ///
    /// Lower-cased, de-duplicated, and never containing `domain` itself. A
    /// leading `*.` is allowed and is the one entry that behaves differently
    /// everywhere downstream: it reaches the certificate and the router and
    /// cannot reach `/etc/hosts`.
    pub aliases: Vec<String>,
    /// Also answer on a name other devices on this network can resolve.
    ///
    /// The intent, not the name. What that name is depends on this machine's
    /// address at the moment it is asked for, and [`crate::lan`] says at length
    /// why writing it down would be writing down something that expires.
    pub lan_share: bool,
    /// Catalog ids of the backing services this project needs.
    ///
    /// The half of an environment definition that travels with the repository:
    /// `stackvo.json` already said what to build the project *with*, and this
    /// says what it needs *around* it. Empty is the overwhelmingly common case
    /// and means "nothing declared", not "nothing needed" — an existing project
    /// gains the field only when somebody writes it.
    pub services: Vec<String>,
    pub php: Option<PhpConfig>,
    pub node: Option<NodeConfig>,
    /// The block for a `LANG_RUNTIMES` runtime, keyed in the file by the
    /// runtime's own name (`"python": { … }`).
    pub lang: Option<LangConfig>,

    pub valid: bool,
    pub errors: Vec<Finding>,
    pub warnings: Vec<Finding>,

    /// Commands to run when this project starts, stops or is rebuilt (B-3).
    ///
    /// Read here so a malformed hook is a manifest finding like everything
    /// else, and so `read` is the one place that knows how a project is
    /// described. What may actually run is [`crate::hooks::plan`]'s business,
    /// not this struct's — a manifest states an intent and a policy and a
    /// consent record decide whether it is honoured.
    #[serde(skip_serializing_if = "crate::hooks::Hooks::is_empty")]
    pub hooks: crate::hooks::Hooks,

    /// Commands this project offers next to the built-in ones (B-4).
    ///
    /// Here for the same reason `hooks` is: a malformed declaration becomes a
    /// manifest finding rather than a surprise at the moment somebody presses
    /// the button, and `read` stays the one place that knows how a project is
    /// described. What may actually run is [`crate::quickcmd`]'s business —
    /// these are container commands and can be nothing else.
    #[serde(skip_serializing_if = "crate::quickcmd::Declared::is_empty")]
    pub commands: crate::quickcmd::Declared,

    /// Which fields this machine's `stackvo.local.json` supplied, dotted.
    ///
    /// Empty for a manifest read straight from the committed file, which is
    /// what makes it the guard rather than only a label: [`write`] refuses a
    /// manifest whose `local` is non-empty, so one developer's overrides
    /// cannot be saved into the file the team shares. See [`read_effective`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local: Vec<String>,
}

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// Read a manifest, normalise it, and collect every contract violation.
///
/// `dir_name` is the containing directory: the contract requires `name` to
/// match it (W-04), because `listProjects` keys containers off the directory.
///
/// **This is the effective manifest** — the committed file with this machine's
/// `stackvo.local.json` laid over it (B-2). That is the default because it is
/// what every reader that runs, renders or inspects a project wants, and there
/// are twenty-odd of them: making the overlay the thing you opt *into* would
/// mean twenty-odd chances to forget it, each one a machine-local setting that
/// silently does nothing.
///
/// The five callers that read a manifest in order to write it back want
/// [`read_committed`] instead, and forgetting that is not silent: [`write`]
/// refuses a manifest carrying overrides. So the direction that costs somebody
/// else an afternoon fails loudly, and the direction that costs nothing is the
/// default.
pub fn read(path: &Path, dir_name: &str) -> Result<Manifest> {
    // `read` is given the file; the overlay is a sibling of it. A project
    // directory is always the parent — every call site builds this path as
    // `<dir>/stackvo.json` — and a path with no parent simply has no overlay.
    read_effective(path, dir_name)
}

/// The committed manifest alone, with no machine-local overlay.
///
/// For the callers that read in order to write back. See [`read`].
pub fn read_committed(path: &Path, dir_name: &str) -> Result<Manifest> {
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

/// Validate a spec that is not (yet) a file.
///
/// One of the contract's rules is about the *layout of a document* — W-01,
/// `php.extensions` must be the last key. A `serde_json::Value` has no layout:
/// `serde_json`'s map is a `BTreeMap`, so pretty-printing one sorts the keys
/// and lands `extensions` before `version`, tripping W-01 on a spec that is
/// perfectly fine.
///
/// The bytes that will actually reach disk come from [`to_json`], which exists
/// precisely to satisfy that rule — so those are the bytes the layout check is
/// run against. The first pass runs with no bytes at all, which skips the
/// layout check, only to obtain the manifest `to_json` needs.
pub fn normalize_spec(json: &serde_json::Value, dir_name: &str) -> Manifest {
    let probe = normalize(json, "", dir_name);
    normalize(json, &to_json(&probe), dir_name)
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

    // A capital in the name is not a style question: `image: stackvo-<name>`
    // is an image reference, and Docker refuses those unless they are
    // lower-case. Created projects are canonicalised before the directory is
    // made (`workspace::canonical_name`); an adopted directory keeps its own
    // name, so this says why the build the generator writes is unusual.
    if name != name.to_ascii_lowercase() {
        warnings.push(Finding {
            code: "NAME_CASE".into(),
            path: "name".into(),
            message: format!(
                "\"{name}\" has capitals; Docker image references must be lower-case, so the image is tagged \"stackvo-{}\" while the directory keeps its own spelling",
                name.to_ascii_lowercase()
            ),
        });
    }

    // ---- domain (required; no fallback exists) ---------------------------
    //
    // Lower-cased on the way in. Hostnames are case-insensitive, but the
    // Traefik `Host()` rule, the hosts-file line and the certificate SAN are
    // three separate strings compared byte-for-byte in three separate places —
    // `Aksoyca.loc` in the manifest is a project that resolves and then 404s.
    let domain = str_field(json, "domain").map(|d| d.trim().to_ascii_lowercase());
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
        Some(id @ ("php" | "node" | "python" | "go" | "ruby" | "rust" | "bun" | "deno")) => {
            id.to_string()
        }
        Some(alias @ ("nodejs" | "js")) => {
            error(
                "C-01",
                "runtime",
                format!("\"{alias}\" is a legacy alias; the canonical id is \"node\""),
            );
            "node".to_string()
        }
        Some(alias @ "golang") => {
            error(
                "C-02",
                "runtime",
                format!("\"{alias}\" is not a runtime id; the canonical id is \"go\""),
            );
            "go".to_string()
        }
        Some(other) => {
            error(
                "C-02",
                "runtime",
                format!(
                    "runtime \"{other}\" has no generator — php, node, python, go, ruby, rust, bun and deno are implemented"
                ),
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
    // A runtime block that does not belong to the declared runtime is dead
    // weight at best and a sign the file was hand-merged at worst.
    for orphan in ["python", "ruby", "golang", "go", "rust", "node"] {
        if orphan != runtime && json.get(orphan).is_some() {
            error(
                "C-02",
                orphan,
                format!("runtime block \"{orphan}\" does not match runtime \"{runtime}\""),
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
    let lang = LANG_RUNTIMES
        .contains(&runtime.as_str())
        .then(|| read_lang(json, &runtime, &mut errors))
        .flatten();

    if runtime != "php" {
        for k in ["server", "webserver", "document_root", "php"] {
            if json.get(k).is_some() {
                warnings.push(Finding {
                    code: "NODE_EXTRA_KEY".into(),
                    path: k.into(),
                    message: format!("`{k}` is ignored when runtime is {runtime}"),
                });
            }
        }
    }

    // ---- extra hostnames --------------------------------------------------
    let aliases = read_aliases(json, domain.as_deref(), &mut warnings);

    // The intent only. The name it produces is derived from whatever this
    // machine's address is when it is asked for — see `lan.rs` for why storing
    // it would be storing something that stops being true.
    let lan_share = json
        .get("lan_share")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // ---- declared services ------------------------------------------------
    let services = read_services(json, &mut warnings);

    // ---- lifecycle hooks (B-3) --------------------------------------------
    //
    // Warnings, never errors. A typo in an optional convenience must not be
    // the reason a project cannot be opened or built; the step that could not
    // be read simply does not run, and the reason is on screen.
    let (hooks, hook_problems) = crate::hooks::parse(json);
    for problem in hook_problems {
        warnings.push(Finding {
            code: "HOOK".into(),
            path: problem.path,
            message: problem.message,
        });
    }

    // ---- declared commands (B-4) ------------------------------------------
    //
    // Warnings for the same reason hooks are: a project with one unreadable
    // command still has ten that work, and refusing to open it would be the
    // wrong trade for a convenience.
    let (commands, command_problems) = crate::quickcmd::parse(json);
    for problem in command_problems {
        warnings.push(Finding {
            code: "COMMAND".into(),
            path: problem.path,
            message: problem.message,
        });
    }

    // ---- write rules the Bash parser depends on ---------------------------
    check_extension_layout(raw, &mut errors);

    Manifest {
        name,
        domain,
        runtime,
        server,
        document_root: str_field(json, "document_root"),
        aliases,
        lan_share,
        services,
        php,
        node,
        lang,
        hooks,
        commands,
        valid: errors.is_empty(),
        errors,
        warnings,
        local: Vec::new(),
    }
}

/// The committed manifest, and the machine-local file that may sit beside it.
pub const FILE: &str = "stackvo.json";
pub const LOCAL_FILE: &str = "stackvo.local.json";

/// Keys the local overlay may never carry, whatever they say.
///
/// `runtime` is not a property of a machine. A repository is a PHP project or a
/// Go one; "PHP here, Go on my laptop" describes two different programs, and
/// every downstream decision — the image, the server, the health check — hangs
/// off it.
///
/// `name` is **not** on this list, and [`local_name_refused`] says why.
pub const LOCAL_REFUSED: [&str; 1] = ["runtime"];

/// May the local overlay set `name` to this?
///
/// Only to the directory it is sitting in, and that narrow permission is the
/// whole of it. `name` keys the container, the image and the directory lookup,
/// so a machine that renamed a project locally would build one thing and look
/// for another — which is why every other value is refused exactly as it was
/// when this key was on [`LOCAL_REFUSED`] outright.
///
/// What the permission is for is N, per-worktree environments. `git worktree
/// add` checks a branch out into its own directory, and that directory is a
/// project of its own here: its own container, its own domain, its own
/// database. What it cannot have is its own `stackvo.json`, because the file in
/// it is the *branch's* — writing to it would show up as a modification to
/// whoever is working on that branch. So the committed manifest keeps saying
/// `shop` while the directory is `shop-feature-x`, and W-04 fires on a mismatch
/// that this app created deliberately and knows the reason for.
///
/// Restating the directory is the one thing a local file can say about identity
/// that cannot be wrong: the value is checked against the directory it was read
/// from, so the file can reconcile the manifest with where the checkout lives
/// and can express nothing else. A different name is still refused, and still
/// named in a warning rather than dropped.
fn local_name_refused(value: &serde_json::Value, dir_name: &str) -> bool {
    value.as_str() != Some(dir_name)
}

/// Read the committed manifest with this machine's overrides laid over it.
///
/// B-2. `stackvo.json` is committed, which is the whole of what makes a
/// checkout reproducible — and is also why there was nowhere to say "on *this*
/// machine, PHP 8.3, because I am chasing a bug in it". The answer everywhere
/// else in this space is a second file that is not committed, and this is that
/// file.
///
/// ## Merged as JSON, before validation, not as fields afterwards
///
/// The overlay is a shallow key merge on the parsed document, with one level of
/// nesting for the runtime blocks, and then the *result* goes through
/// [`normalize`] exactly once. Merging normalised `Manifest`s instead would
/// mean every rule in this file needing a second opinion about which half a
/// value came from, and — worse — an override could not be validated at all,
/// because validation happens on the way in and it would arrive afterwards. A
/// local file that says `"domain": "not a domain"` fails the same check the
/// committed one would.
///
/// The nested merge matters: a local file that sets only `php.version` must not
/// drop `php.extensions`, and a whole-value overlay would.
///
/// ## The layout rule is checked against the committed bytes
///
/// W-01 is about the shape of a document a Bash parser reads, and the document
/// that parser reads is `stackvo.json`. So `raw` stays the committed text; the
/// local file has no layout obligations because nothing but this reads it.
///
/// ## Refusals are named, never dropped
///
/// A key in [`LOCAL_REFUSED`] produces a warning that says it was ignored. A
/// local file that quietly does less than it says is how somebody concludes the
/// feature is broken after changing the one field it will not take.
pub fn read_effective(committed_path: &Path, dir_name: &str) -> Result<Manifest> {
    let raw = std::fs::read_to_string(committed_path)
        .map_err(|e| Error::io(format!("reading {}", committed_path.display()), e))?;

    let mut json: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("{} is not valid JSON: {e}", committed_path.display()),
        )
    })?;

    // A sibling of the file we were handed, rather than `<dir>/stackvo.json`
    // rebuilt from a directory: the caller already knows which file it means,
    // and deriving the name back would put an assumption here that only some
    // of the twenty-odd call sites happen to satisfy.
    let Some(local_path) = committed_path.parent().map(|dir| dir.join(LOCAL_FILE)) else {
        return Ok(normalize(&json, &raw, dir_name));
    };
    if !local_path.is_file() {
        return Ok(normalize(&json, &raw, dir_name));
    }

    let local_raw = std::fs::read_to_string(&local_path)
        .map_err(|e| Error::io(format!("reading {}", local_path.display()), e))?;

    // A broken local file is refused rather than skipped. Skipping it would run
    // the project on the committed settings while the developer believes their
    // override is in force, which is the one outcome worse than not having the
    // feature.
    let local: serde_json::Value = serde_json::from_str(&local_raw).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("{} is not valid JSON: {e}", local_path.display()),
        )
    })?;

    let (applied, refused) = overlay(&mut json, &local, Some(dir_name));
    let mut manifest = normalize(&json, &raw, dir_name);

    for key in refused {
        manifest.warnings.push(Finding {
            code: "LOCAL_REFUSED".into(),
            path: key.clone(),
            message: format!(
                "`{key}` in {LOCAL_FILE} was ignored; it describes the repository rather than this machine"
            ),
        });
    }
    manifest.local = applied;
    Ok(manifest)
}

/// Lay `local` over `base`, returning the dotted paths applied and refused.
///
/// Only objects nest. An array — `aliases`, `services`, `php.extensions` — is
/// replaced whole, because the alternative is deciding whether a local file
/// listing one alias means "also this" or "only this", and both readings are
/// defensible, which is exactly why neither should be guessed at.
///
/// `dir_name` is here for one key: [`local_name_refused`] judges `name` against
/// the directory rather than against a list. `None` marks a nested call, where
/// `name` stays refused outright — a `name` inside `php` is not the project's
/// and there is no directory to check it against, so the answer it had before
/// this permission existed is still the right one.
fn overlay(
    base: &mut serde_json::Value,
    local: &serde_json::Value,
    dir_name: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    let mut applied = Vec::new();
    let mut refused = Vec::new();

    let (Some(base_map), Some(local_map)) = (base.as_object_mut(), local.as_object()) else {
        return (applied, refused);
    };

    for (key, value) in local_map {
        let name_refused =
            key == "name" && dir_name.is_none_or(|dir| local_name_refused(value, dir));
        if LOCAL_REFUSED.contains(&key.as_str()) || name_refused {
            refused.push(key.clone());
            continue;
        }

        let nested = matches!(value, serde_json::Value::Object(_))
            && matches!(base_map.get(key), Some(serde_json::Value::Object(_)));

        if nested {
            let slot = base_map.get_mut(key).expect("checked just above");
            let (inner, _) = overlay(slot, value, None);
            for field in inner {
                applied.push(format!("{key}.{field}"));
            }
        } else {
            base_map.insert(key.clone(), value.clone());
            applied.push(key.clone());
        }
    }

    (applied, refused)
}

/// What the machine-local overlay is, as the editor sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalOverride {
    /// The file's text, or empty when there is no file.
    pub text: String,
    pub exists: bool,
    /// Dotted paths the overlay currently supplies, from the last read.
    pub applied: Vec<String>,
    /// Keys present in the file that [`LOCAL_REFUSED`] rejects.
    pub refused: Vec<String>,
    /// Whether git would keep this file out of a commit.
    ///
    /// `None` means git had no answer — see [`crate::git::is_ignored`]. The
    /// screen has three things to say here and only one of them is a warning.
    pub ignored: Option<bool>,
}

/// Read the machine-local overlay, whether or not there is one.
pub fn read_local(dir: &Path, dir_name: &str) -> Result<LocalOverride> {
    let path = dir.join(LOCAL_FILE);
    if !path.is_file() {
        return Ok(LocalOverride {
            text: String::new(),
            exists: false,
            applied: Vec::new(),
            refused: Vec::new(),
            ignored: None,
        });
    }

    let text = std::fs::read_to_string(&path)
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;

    // Read through the same path the renderer uses rather than re-deriving the
    // two lists here. A second implementation of "what does this file do" is a
    // second answer, and the screen would be reporting the one nothing runs on.
    let effective = read_effective(&dir.join(FILE), dir_name)?;
    let refused = effective
        .warnings
        .iter()
        .filter(|w| w.code == "LOCAL_REFUSED")
        .map(|w| w.path.clone())
        .collect();

    Ok(LocalOverride {
        text,
        exists: true,
        applied: effective.local,
        refused,
        ignored: crate::git::is_ignored(&path),
    })
}

/// Write the machine-local overlay, refusing what it may not carry.
///
/// A refused key is a *warning* on the way in ([`read_effective`]) and an
/// *error* here, and the difference is deliberate: on the way in the file may
/// predate a change and the project should still run, while here somebody is
/// typing it now and can fix it in the second it takes to read the message.
///
/// Empty text deletes the file rather than writing `{}`. "No overrides" and "an
/// empty overrides file" are the same state, and only one of them is visible in
/// a directory listing as something to wonder about.
pub fn write_local(dir: &Path, dir_name: &str, text: &str) -> Result<LocalOverride> {
    let path = dir.join(LOCAL_FILE);

    if text.trim().is_empty() {
        if path.is_file() {
            std::fs::remove_file(&path)
                .map_err(|e| Error::io(format!("removing {}", path.display()), e))?;
        }
        return read_local(dir, dir_name);
    }

    let json: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| Error::new(Code::InvalidManifest, format!("{LOCAL_FILE}: {e}")))?;

    let Some(map) = json.as_object() else {
        return Err(Error::new(
            Code::InvalidManifest,
            format!("{LOCAL_FILE} must be a JSON object"),
        ));
    };

    let refused: Vec<&str> = LOCAL_REFUSED
        .iter()
        .copied()
        .filter(|key| map.contains_key(*key))
        .collect();
    if !refused.is_empty() {
        return Err(Error::new(
            Code::InvalidManifest,
            format!(
                "{LOCAL_FILE} may not set {}; those describe the repository rather than this machine",
                refused.join(" or ")
            ),
        ));
    }

    // The one key with a value-dependent answer, so it carries its own message:
    // "may not set name" would be false — it may, to exactly one string — and a
    // message that says the wrong thing costs somebody the ten minutes it takes
    // to find out which.
    if let Some(value) = map.get("name") {
        if local_name_refused(value, dir_name) {
            return Err(Error::new(
                Code::InvalidManifest,
                format!(
                    "{LOCAL_FILE} may only set `name` to \"{dir_name}\", the directory it is in; \
                     renaming a project locally would build one image and look for another"
                ),
            ));
        }
    }

    // Validated as the merged document, not on its own: a local file is a
    // fragment and would fail half the contract read alone — it has no `name`,
    // usually no `domain`. What has to be valid is what the renderer will see.
    let mut merged: serde_json::Value = {
        let committed = std::fs::read_to_string(dir.join(FILE))
            .map_err(|e| Error::io(format!("reading {}", dir.join(FILE).display()), e))?;
        serde_json::from_str(&committed)
            .map_err(|e| Error::new(Code::InvalidManifest, format!("{FILE}: {e}")))?
    };
    overlay(&mut merged, &json, Some(dir_name));
    let check = normalize(&merged, "", dir_name);
    if !check.valid {
        return Err(Error::new(
            Code::InvalidManifest,
            format!(
                "with {LOCAL_FILE} applied the manifest is invalid: {}",
                check
                    .errors
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
    }

    crate::atomic::write(&path, text)?;
    read_local(dir, dir_name)
}

/// Whether a hostname is one `/etc/hosts` can carry.
pub fn resolves_through_hosts(value: &str) -> bool {
    !value.starts_with("*.")
}

/// `aliases`, normalised the way `domain` is and checked the way it is not.
///
/// Warnings rather than errors throughout, for the reason `services` gives: an
/// invalid manifest is a project the app will not build, and a mistyped extra
/// hostname must not cost somebody the project that works at its main one.
fn read_aliases(
    json: &serde_json::Value,
    domain: Option<&str>,
    warnings: &mut Vec<Finding>,
) -> Vec<String> {
    let Some(value) = json.get("aliases") else {
        return Vec::new();
    };

    let Some(items) = value.as_array() else {
        warnings.push(Finding {
            code: "ALIASES_NOT_A_LIST".into(),
            path: "aliases".into(),
            message: "`aliases` is not an array; nothing was read from it".into(),
        });
        return Vec::new();
    };

    let mut out: Vec<String> = Vec::new();

    for (index, item) in items.iter().enumerate() {
        let Some(raw) = item.as_str() else {
            warnings.push(Finding {
                code: "ALIASES_NOT_A_STRING".into(),
                path: format!("aliases[{index}]"),
                message: "entries must be hostnames, as strings".into(),
            });
            continue;
        };

        // The same normalisation `domain` gets, and for the same reason: the
        // hosts line, the Traefik rule and the certificate SAN are three
        // strings compared byte for byte in three places.
        let alias = raw.trim().to_ascii_lowercase();
        if alias.is_empty() {
            continue;
        }

        if !crate::hosts::is_valid_wildcard_or_domain(&alias) {
            warnings.push(Finding {
                code: "INVALID_ALIAS".into(),
                path: format!("aliases[{index}]"),
                message: format!(
                    "\"{alias}\" is not a hostname; a wildcard is written `*.example.loc` and \
                     may only replace the leftmost label"
                ),
            });
            continue;
        }

        // Dropped rather than reported: repeating the main domain in the list
        // is a reasonable thing to write and means exactly what leaving it out
        // means. Keeping it would put the name in the Traefik rule twice.
        if Some(alias.as_str()) == domain {
            continue;
        }

        if out.contains(&alias) {
            warnings.push(Finding {
                code: "ALIASES_DUPLICATE".into(),
                path: format!("aliases[{index}]"),
                message: format!("\"{alias}\" is listed more than once"),
            });
            continue;
        }

        out.push(alias);
    }

    out
}

/// `services`, checked against the catalog and normalised.
///
/// Every fault here is a **warning**, never an error, and the distinction is
/// the whole design of the field. An error makes the manifest invalid, and an
/// invalid manifest is a project the app refuses to build — so a typo in an
/// optional convenience would take somebody's whole project offline. A warning
/// leaves the project working and the declaration visible as unmet, which is
/// also what `project_requirements` will report a moment later.
///
/// Unknown ids are kept rather than dropped, for the same reason: a
/// declaration that silently disappears is one nobody can debug. `preset::plan`
/// rejects it by name further down the path, once, where the reason can be
/// shown.
fn read_services(json: &serde_json::Value, warnings: &mut Vec<Finding>) -> Vec<String> {
    let Some(value) = json.get("services") else {
        return Vec::new();
    };

    let Some(items) = value.as_array() else {
        warnings.push(Finding {
            code: "SERVICES_NOT_A_LIST".into(),
            path: "services".into(),
            message: "`services` is not an array; nothing was read from it".into(),
        });
        return Vec::new();
    };

    let catalog = crate::contracts::env_schema();
    let mut out: Vec<String> = Vec::new();

    for (index, item) in items.iter().enumerate() {
        let Some(id) = item.as_str() else {
            warnings.push(Finding {
                code: "SERVICES_NOT_A_STRING".into(),
                path: format!("services[{index}]"),
                message: "entries must be service ids, as strings".into(),
            });
            continue;
        };

        // Trimmed and lower-cased on the way in, like `domain`: the id becomes
        // `SERVICE_<NAME>_ENABLE` by uppercasing, and " Redis" would produce a
        // key with a space in it that `.env` cannot express.
        let id = id.trim().to_ascii_lowercase();
        if id.is_empty() {
            continue;
        }

        if out.contains(&id) {
            warnings.push(Finding {
                code: "SERVICES_DUPLICATE".into(),
                path: format!("services[{index}]"),
                message: format!("\"{id}\" is listed more than once"),
            });
            continue;
        }

        if !catalog.knows_service(&id) {
            warnings.push(Finding {
                code: "UNKNOWN_SERVICE".into(),
                path: format!("services[{index}]"),
                message: format!(
                    "\"{id}\" is not a service this version of StackVo has a template for"
                ),
            });
        }

        out.push(id);
    }

    out
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
            // No ceiling on the count. There used to be one — C-04, the Bash
            // extractor's `grep -A 50` window, which dropped extension 51
            // onward in silence. The generator is Rust with a real JSON parser
            // now, so the only limit left is what the catalog offers.
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
        // Absent is off, and absent is what every manifest on disk says — the
        // field did not exist until the extension and the switch were split.
        xdebug: block
            .get("xdebug")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        extensions,
    })
}

/// Read a `LANG_RUNTIMES` block. Missing fields fall back to the runtime's
/// ecosystem defaults; a missing *block* is fine for exactly the same reason —
/// `{"runtime": "python"}` is a complete manifest that runs the defaults.
fn read_lang(
    json: &serde_json::Value,
    runtime: &str,
    errors: &mut Vec<Finding>,
) -> Option<LangConfig> {
    let defaults = lang_defaults(runtime)?;
    let Some(block) = json.get(runtime) else {
        return Some(defaults);
    };

    let port = match block.get("port") {
        None => defaults.port,
        Some(v) => match v.as_u64().and_then(|p| u16::try_from(p).ok()) {
            Some(p) if p > 0 => p,
            _ => {
                errors.push(Finding {
                    code: "INVALID_PORT".into(),
                    path: format!("{runtime}.port"),
                    message: format!("`{runtime}.port` must be a port number"),
                });
                defaults.port
            }
        },
    };

    Some(LangConfig {
        version: str_field(block, "version").unwrap_or(defaults.version),
        // An explicitly empty string means "no step", distinct from absent
        // (which means "the default step").
        install: match str_field(block, "install") {
            Some(s) if s.is_empty() => None,
            Some(s) => Some(s),
            None => defaults.install,
        },
        build: match str_field(block, "build") {
            Some(s) if s.is_empty() => None,
            Some(s) => Some(s),
            None => defaults.build,
        },
        start: str_field(block, "start").unwrap_or(defaults.start),
        port,
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

    // Read before `start` and `install`, because it is what their defaults are
    // taken from. An unknown name is a warning and falls back to absent: a
    // typo must not cost somebody the project, and the fallback is the
    // behaviour the project had before it named one.
    let package_manager = str_field(block, "package_manager")
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .and_then(|value| {
            if NODE_PACKAGE_MANAGERS.contains(&value.as_str()) {
                Some(value)
            } else {
                warnings.push(Finding {
                    code: "UNKNOWN_PACKAGE_MANAGER".into(),
                    path: "node.package_manager".into(),
                    message: format!(
                        "\"{value}\" is not one Corepack can pin; expected one of {}",
                        NODE_PACKAGE_MANAGERS.join(", ")
                    ),
                });
                None
            }
        });
    let pm = package_manager.as_deref().unwrap_or("npm");

    let start = str_field(block, "start").unwrap_or_else(|| format!("{pm} start"));

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
        install: str_field(block, "install").unwrap_or_else(|| format!("{pm} install")),
        build: str_field(block, "build"),
        start,
        port: port.clamp(1, 65535) as u16,
        package_manager,
    })
}

/// W-01: `php.extensions` must be the last key in the document.
///
/// Historically because the Bash extractor took every quoted lowercase token
/// after the `"extensions"` marker and fed it to `docker-php-ext-install`. That
/// extractor is gone, and with it C-04's 50-line window, which used to be
/// checked here too; the canonical layout stays because [`to_json`] writes it
/// and a manifest that keeps one shape produces readable diffs.
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
            message: "keys appear after `php.extensions`; the canonical layout puts the array last"
                .into(),
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
    fn a_spec_is_judged_on_the_bytes_it_would_become() {
        // Exactly what the New Project sheet sends.
        let spec = serde_json::json!({
            "name": "aksoyca",
            "domain": "aksoyca.loc",
            "runtime": "php",
            "server": "nginx",
            "document_root": "public",
            "php": { "version": "8.4", "extensions": ["bcmath", "mbstring", "pdo_mysql"] }
        });

        // W-01 is about *bytes*: a key after `extensions` breaks the Bash
        // parser, so the rule fires on this ordering however it was produced.
        // This literal is what `serde_json::to_string_pretty` used to make of
        // the spec above, because `Value` sorted its keys — which disabled the
        // Create button on a perfectly valid project.
        let sorted = r#"{
  "domain": "aksoyca.loc",
  "name": "aksoyca",
  "php": { "extensions": ["bcmath"], "version": "8.4" }
}"#;
        assert!(
            normalize(&serde_json::from_str(sorted).unwrap(), sorted, "aksoyca")
                .errors
                .iter()
                .any(|e| e.code == "W-01"),
            "the rule this bug ran into still fires on that ordering"
        );

        // The ordering itself can no longer be produced here: `serde_json` is
        // built with `preserve_order` (see Cargo.toml — it is there so that
        // rewriting somebody's assistant configuration does not alphabetise it),
        // so a `Value` now serialises in the order it was built.
        let round_trip = serde_json::to_string_pretty(&spec).unwrap();
        assert!(
            round_trip.find("\"version\"").unwrap() < round_trip.find("\"extensions\"").unwrap(),
            "keys came back sorted: {round_trip}"
        );
        assert!(
            normalize(&spec, &round_trip, "aksoyca")
                .errors
                .iter()
                .all(|e| e.code != "W-01"),
            "the sorted-key serialisation was the whole of this bug"
        );

        let m = normalize_spec(&spec, "aksoyca");
        assert!(m.valid, "{:?}", m.errors);
    }

    // ------------------------------------------------- extra hostnames (E-2)

    #[test]
    fn aliases_are_normalised_the_way_the_domain_is() {
        let m = parse(
            r#"{"name":"shop","domain":"shop.loc","php":{"version":"8.4"},
                "aliases":[" API.Shop.LOC ","*.shop.loc","shop.loc","api.shop.loc"]}"#,
            "shop",
        );

        // Lower-cased and trimmed, because the hosts line, the Traefik rule and
        // the certificate SAN are three strings compared byte for byte.
        // `shop.loc` itself is dropped: repeating the main domain means what
        // leaving it out means, and keeping it would name it twice in the rule.
        assert_eq!(m.aliases, ["api.shop.loc", "*.shop.loc"]);
        assert!(m.warnings.iter().any(|w| w.code == "ALIASES_DUPLICATE"));
        assert!(m.valid, "{:?}", m.errors);
    }

    /// A wildcard is one label deep and leftmost — anything else is a hostname
    /// with an asterisk in it, which mkcert refuses and Traefik matches never.
    #[test]
    fn only_a_leftmost_single_label_wildcard_is_a_wildcard() {
        let m = parse(
            r#"{"name":"shop","domain":"shop.loc","php":{"version":"8.4"},
                "aliases":["*.shop.loc","*.*.shop.loc","api.*.shop.loc","*shop.loc"]}"#,
            "shop",
        );

        assert_eq!(m.aliases, ["*.shop.loc"]);
        assert_eq!(
            m.warnings
                .iter()
                .filter(|w| w.code == "INVALID_ALIAS")
                .count(),
            3
        );
        // Still valid: a mistyped extra hostname must not cost somebody the
        // project that works at its main one.
        assert!(m.valid, "{:?}", m.errors);
    }

    #[test]
    fn a_wildcard_is_the_one_name_a_hosts_file_cannot_carry() {
        assert!(resolves_through_hosts("api.shop.loc"));
        assert!(!resolves_through_hosts("*.shop.loc"));
    }

    // ------------------------------------------------ declared services (B-1)

    #[test]
    fn declared_services_are_normalised_and_kept_in_order() {
        let m = parse(
            r#"{"name":"shop","domain":"shop.loc","php":{"version":"8.4"},"services":["  MySQL ","redis","mysql",""]}"#,
            "shop",
        );

        // Order is the author's, not alphabetical: a manifest is read by people
        // and reordering their list on every write produces diff noise nobody
        // asked for. The duplicate is dropped and reported.
        assert_eq!(m.services, ["mysql", "redis"]);
        assert!(m.warnings.iter().any(|w| w.code == "SERVICES_DUPLICATE"));
        assert!(m.valid, "{:?}", m.errors);
    }

    /// The rule the whole field hangs on: a typo must not take a project
    /// offline. An invalid manifest is one the app refuses to build.
    #[test]
    fn an_unknown_service_is_a_warning_and_the_project_still_builds() {
        let m = parse(
            r#"{"name":"shop","domain":"shop.loc","php":{"version":"8.4"},"services":["mysql","postgress"]}"#,
            "shop",
        );

        assert!(m.valid, "{:?}", m.errors);
        assert!(m.warnings.iter().any(|w| w.code == "UNKNOWN_SERVICE"));
        // Kept, not dropped — a declaration that silently disappears is one
        // nobody can debug, and the planner rejects it by name further on.
        assert_eq!(m.services, ["mysql", "postgress"]);
    }

    #[test]
    fn a_services_key_of_the_wrong_shape_is_reported_rather_than_obeyed() {
        let m = parse(
            r#"{"name":"shop","domain":"shop.loc","php":{"version":"8.4"},"services":"mysql"}"#,
            "shop",
        );
        assert!(m.services.is_empty());
        assert!(m.warnings.iter().any(|w| w.code == "SERVICES_NOT_A_LIST"));
        assert!(m.valid);
    }

    #[test]
    fn normalize_spec_still_reports_real_faults() {
        // Layout is forgiven, semantics are not: imap does not build on 8.4.
        let spec = serde_json::json!({
            "name": "legacy",
            "domain": "legacy.loc",
            "runtime": "php",
            "php": { "version": "8.4", "extensions": ["imap"] }
        });
        let m = normalize_spec(&spec, "legacy");
        assert!(!m.valid);
        assert!(m.errors.iter().any(|e| e.code == "C-06"), "{:?}", m.errors);
    }

    #[test]
    fn a_domain_is_canonicalised_and_a_capitalised_name_is_flagged() {
        let raw = r#"{
  "name": "Aksoyca",
  "domain": "Aksoyca.LOC",
  "php": { "version": "8.4" }
}"#;
        let m = parse(raw, "Aksoyca");
        // The Traefik rule, the hosts line and the certificate are three
        // byte-for-byte comparisons of this string.
        assert_eq!(m.domain.as_deref(), Some("aksoyca.loc"));
        // The name is the directory's, so it is not rewritten — but the image
        // reference it produces is not a legal one, and that is worth saying.
        assert_eq!(m.name, "Aksoyca");
        assert!(
            m.warnings.iter().any(|w| w.code == "NAME_CASE"),
            "{:?}",
            m.warnings
        );
        assert!(m.valid, "{:?}", m.errors);
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

    /// J-2, and the half that matters most: a project that never named a
    /// package manager must be read exactly as it was before the field existed.
    ///
    /// Absent is not `npm`. If it defaulted to a value, every node manifest on
    /// disk would start enabling Corepack in its image the next time anything
    /// touched it — a different build for thousands of projects that asked for
    /// nothing.
    #[test]
    fn a_node_project_that_names_no_package_manager_is_unchanged() {
        let raw = r#"{
  "name": "web",
  "domain": "web.loc",
  "runtime": "node",
  "node": { "version": "22" }
}"#;
        let m = parse(raw, "web");
        let node = m.node.unwrap();
        assert_eq!(node.package_manager, None, "absent is not npm");
        assert_eq!(node.install, "npm install");
        assert_eq!(node.start, "npm start");
    }

    /// Naming one moves the defaults with it, so the manifest does not have to
    /// repeat the choice three times.
    #[test]
    fn naming_a_package_manager_moves_the_install_and_start_defaults() {
        let raw = r#"{
  "name": "web",
  "domain": "web.loc",
  "runtime": "node",
  "node": { "version": "22", "package_manager": "pnpm" }
}"#;
        let m = parse(raw, "web");
        assert!(m.valid, "{:?}", m.errors);
        let node = m.node.unwrap();
        assert_eq!(node.package_manager.as_deref(), Some("pnpm"));
        assert_eq!(node.install, "pnpm install");
        assert_eq!(node.start, "pnpm start");
    }

    /// An explicit command still wins — the default is a convenience, not a
    /// rule about what may be run.
    #[test]
    fn an_explicit_command_outranks_the_package_managers_default() {
        let raw = r#"{
  "name": "web",
  "domain": "web.loc",
  "runtime": "node",
  "node": { "version": "22", "package_manager": "yarn", "install": "yarn install --immutable" }
}"#;
        let m = parse(raw, "web");
        let node = m.node.unwrap();
        assert_eq!(node.install, "yarn install --immutable");
        assert_eq!(node.start, "yarn start");
    }

    /// A typo must not cost somebody the project. It warns and falls back to
    /// absent, which is the behaviour the project had before it named one.
    #[test]
    fn an_unknown_package_manager_warns_and_falls_back_rather_than_failing() {
        let raw = r#"{
  "name": "web",
  "domain": "web.loc",
  "runtime": "node",
  "node": { "version": "22", "package_manager": "pnmp" }
}"#;
        let m = parse(raw, "web");
        assert!(m.valid, "a typo here must not invalidate the manifest");
        let node = m.node.unwrap();
        assert_eq!(node.package_manager, None);
        assert_eq!(node.install, "npm install");
        assert!(m
            .warnings
            .iter()
            .any(|w| w.code == "UNKNOWN_PACKAGE_MANAGER"));
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

    // Beside `domain`, which is what it extends — and, like `services`, before
    // the runtime blocks: W-01 reserves the end of the document for
    // `php.extensions`.
    if !manifest.aliases.is_empty() {
        let items: Vec<String> = manifest
            .aliases
            .iter()
            .map(|host| format!("    {}", quote(host)))
            .collect();
        lines.push(format!("  \"aliases\": [\n{}\n  ]", items.join(",\n")));
    }

    // Written only when true, the way `aliases` is written only when non-empty:
    // the absent key and `false` mean the same thing, and a manifest that gains
    // a line saying "no" for every switch this app ever adds is one nobody can
    // read. Written at all, though, and that is the point — this round-trips
    // through `project_manifest_write` on every form save, so a field the
    // serialiser did not know about would be silently dropped the first time
    // somebody edited an unrelated setting.
    if manifest.lan_share {
        lines.push("  \"lan_share\": true".to_string());
    }

    if !manifest.services.is_empty() {
        let items: Vec<String> = manifest
            .services
            .iter()
            .map(|id| format!("    {}", quote(id)))
            .collect();
        lines.push(format!("  \"services\": [\n{}\n  ]", items.join(",\n")));
    }

    // After `services` and before the runtime blocks, for the reason `aliases`
    // is: W-01 reserves the end of the document for `php.extensions`, and a
    // block written after it would trip the layout rule on a manifest that is
    // otherwise fine.
    //
    // Written at all because of the warning above `lan_share`: this text
    // round-trips through `project_manifest_write` on every form save, and a
    // field the serialiser does not know about is one that disappears the first
    // time somebody edits an unrelated setting. Hooks disappearing silently
    // would be a project that quietly stopped migrating on start.
    if !manifest.hooks.is_empty() {
        let mut groups: Vec<String> = Vec::new();
        for event in crate::hooks::Event::ALL {
            let steps = manifest.hooks.steps(event);
            if steps.is_empty() {
                continue;
            }
            let items: Vec<String> = steps
                .iter()
                .map(|step| {
                    let key = match step.kind {
                        crate::hooks::Kind::Exec => "exec",
                        crate::hooks::Kind::Host => "host",
                    };
                    let argv: Vec<String> = step.argv.iter().map(|a| quote(a)).collect();
                    format!("      {{ {}: [{}] }}", quote(key), argv.join(", "))
                })
                .collect();
            groups.push(format!(
                "    {}: [\n{}\n    ]",
                quote(event.key()),
                items.join(",\n")
            ));
        }
        lines.push(format!("  \"hooks\": {{\n{}\n  }}", groups.join(",\n")));
    }

    // B-4, and here for exactly the reason the hooks block above is: this text
    // is what `project_manifest_write` saves on every form submission, so a
    // field the serialiser does not know about is one that disappears the
    // first time somebody changes an unrelated setting. A project quietly
    // losing the command it runs every day is the same class of bug as one
    // that quietly stopped migrating on start.
    if !manifest.commands.is_empty() {
        let items: Vec<String> = manifest
            .commands
            .iter()
            .map(|(id, command)| {
                let argv: Vec<String> = command.argv.iter().map(|a| quote(a)).collect();
                let mut fields = vec![format!("\"exec\": [{}]", argv.join(", "))];
                if !command.about.is_empty() {
                    fields.push(format!("\"about\": {}", quote(&command.about)));
                }
                // Written only when true: `false` is the default and a
                // manifest full of restated defaults is one nobody reads.
                if command.interactive {
                    fields.push("\"interactive\": true".to_string());
                }
                format!("    {}: {{ {} }}", quote(id), fields.join(", "))
            })
            .collect();
        lines.push(format!("  \"commands\": {{\n{}\n  }}", items.join(",\n")));
    }

    if let Some(lang) = &manifest.lang {
        let mut block = format!("  {}: {{\n", quote(&manifest.runtime));
        let mut fields = vec![format!("    \"version\": {}", quote(&lang.version))];
        if let Some(install) = &lang.install {
            fields.push(format!("    \"install\": {}", quote(install)));
        }
        if let Some(build) = &lang.build {
            fields.push(format!("    \"build\": {}", quote(build)));
        }
        fields.push(format!("    \"start\": {}", quote(&lang.start)));
        fields.push(format!("    \"port\": {}", lang.port));
        block.push_str(&fields.join(",\n"));
        block.push_str("\n  }");
        lines.push(block);
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
        // Only when named. Absent is not `"npm"` — it is "this project never
        // asked", and writing a default here would turn every existing node
        // manifest into one that enables Corepack the next time it is saved.
        if let Some(pm) = &node.package_manager {
            fields.push(format!("    \"package_manager\": {}", quote(pm)));
        }
        block.push_str(&fields.join(",\n"));
        block.push_str("\n  }");
        lines.push(block);
    }

    // The php block goes LAST because its `extensions` array must be the final
    // key in the document (W-01).
    if let Some(php) = &manifest.php {
        let mut block = String::from("  \"php\": {\n");
        block.push_str(&format!("    \"version\": {},\n", quote(&php.version)));
        // Only when on. Off is the absence of the key, the way `lan_share` is:
        // a manifest that gained a line saying "no" for every switch this app
        // ever adds is one nobody can read. W-01 keeps `extensions` last, so
        // this goes above it.
        if php.xdebug {
            block.push_str("    \"xdebug\": true,\n");
        }
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
    // The B-2 guard, and the reason `local` is a field rather than a return
    // value nobody has to keep. Every other mistake around a machine-local
    // overlay is loud — an override that fails to apply is noticed within
    // seconds. This one is silent and lands in somebody else's checkout: read
    // the effective manifest, edit one unrelated setting in the form, save, and
    // this machine's PHP version is now the team's. So it is refused here,
    // where every write goes, rather than trusted to each caller.
    if !manifest.local.is_empty() {
        return Err(Error::new(
            Code::InvalidManifest,
            format!(
                "refusing to write a manifest carrying this machine's overrides ({}); \
                 {LOCAL_FILE} is not committed and its values must not reach {FILE}",
                manifest.local.join(", ")
            ),
        ));
    }
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
            aliases: vec![],
            lan_share: false,
            services: vec![],
            php: Some(PhpConfig {
                version: "8.4".into(),
                xdebug: false,
                extensions: vec!["mbstring".into(), "pdo".into(), "pdo_mysql".into()],
            }),
            node: None,
            lang: None,
            valid: true,
            errors: vec![],
            warnings: vec![],
            hooks: Default::default(),
            commands: Default::default(),
            local: Vec::new(),
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

    /// W-01 reserves the end of the document for `php.extensions`, so a new
    /// key added anywhere after it silently breaks the layout rule.
    #[test]
    fn services_are_written_before_the_php_block_and_survive_the_round_trip() {
        let mut m = php_manifest();
        m.services = vec!["mysql".into(), "redis".into()];
        let text = to_json(&m);

        // The block, not the word: `"runtime": "php"` carries `"php"` too, and
        // matching that made the first version of this test pass on a file
        // where the order was right for the wrong reason.
        assert!(
            text.find("  \"services\": [").unwrap() < text.find("  \"php\": {").unwrap(),
            "{text}"
        );

        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let back = normalize(&json, &text, "shop");
        assert!(back.valid, "{:?}", back.errors);
        assert_eq!(back.services, ["mysql", "redis"]);
    }

    #[test]
    fn aliases_are_written_before_the_php_block_and_survive_the_round_trip() {
        let mut m = php_manifest();
        m.aliases = vec!["api.shop.loc".into(), "*.shop.loc".into()];
        let text = to_json(&m);

        assert!(
            text.find("  \"aliases\": [").unwrap() < text.find("  \"php\": {").unwrap(),
            "{text}"
        );

        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let back = normalize(&json, &text, "shop");
        assert!(back.valid, "{:?}", back.errors);
        assert_eq!(back.aliases, ["api.shop.loc", "*.shop.loc"]);
    }

    /// The bug this is the guard for is not in reading — it is in writing.
    ///
    /// Every form save on the project page goes out through
    /// `project_manifest_write`, which round-trips the whole document through
    /// `to_json`. A field the serialiser did not know about survives being read
    /// perfectly well and is then dropped the first time somebody edits an
    /// unrelated setting — so the project silently stops answering on the LAN,
    /// with nothing on screen having said so.
    #[test]
    fn lan_share_survives_the_round_trip_a_form_save_puts_it_through() {
        let mut m = php_manifest();
        m.lan_share = true;
        let text = to_json(&m);

        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let back = normalize(&json, &text, "shop");
        assert!(back.valid, "{:?}", back.errors);
        assert!(back.lan_share, "the switch was on and came back off");
    }

    /// Off is the absence of the key, not a line saying "no". Every switch this
    /// app ever adds writing its own `false` is a manifest nobody can read.
    #[test]
    fn a_project_that_does_not_share_writes_no_key_for_it() {
        let m = php_manifest();
        assert!(!m.lan_share);
        assert!(!to_json(&m).contains("lan_share"));

        let text = to_json(&m);
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(!normalize(&json, &text, "shop").lan_share);
    }

    /// The same trap `lan_share` fell into, for the switch F-4 added.
    ///
    /// Every form save round-trips the whole document through `to_json`. A
    /// field the serialiser did not know about survives being read and is then
    /// dropped the first time somebody edits an unrelated setting — so a
    /// project quietly stops being debuggable with nothing on screen saying so.
    #[test]
    fn the_xdebug_switch_survives_the_round_trip() {
        let mut m = php_manifest();
        m.php.as_mut().unwrap().xdebug = true;
        let text = to_json(&m);

        // Above `extensions`, which W-01 keeps last.
        assert!(
            text.find("\"xdebug\": true").unwrap() < text.find("\"extensions\"").unwrap(),
            "{text}"
        );

        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let back = normalize(&json, &text, "shop");
        assert!(back.valid, "{:?}", back.errors);
        assert!(
            back.php.unwrap().xdebug,
            "the switch was on and came back off"
        );
    }

    /// Off writes no key, the way `lan_share` does — and every manifest on disk
    /// predates the field, so absent has to mean off.
    #[test]
    fn a_project_with_debugging_off_writes_no_xdebug_key() {
        let m = php_manifest();
        assert!(!m.php.as_ref().unwrap().xdebug);
        assert!(!to_json(&m).contains("\"xdebug\""));
    }

    /// The field is optional and almost every manifest on disk predates it.
    #[test]
    fn a_manifest_without_the_key_declares_nothing_and_writes_nothing() {
        let mut m = php_manifest();
        m.services.clear();
        assert!(!to_json(&m).contains("services"));
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
    fn a_bare_lang_runtime_is_a_complete_manifest_on_defaults() {
        // `{"runtime": "go"}` runs the ecosystem defaults — same contract as
        // node's optional fields, extended to the whole block.
        let json = serde_json::json!({ "name": "svc", "domain": "svc.loc", "runtime": "go" });
        let m = normalize(&json, "{}", "svc");
        assert!(m.valid, "{:?}", m.errors);
        let lang = m.lang.expect("go gets a lang block");
        assert_eq!(lang.build.as_deref(), Some("go build -o /app/server ."));
        assert_eq!(lang.install, None);
        assert_eq!(lang.port, 8080);
    }

    /// J-1. Bun and Deno are lang runtimes, not a flavour of node.
    ///
    /// They read the same `package.json` and are built from their own images
    /// with their own verbs, so folding them into the node block would mean one
    /// block whose meaning depended on a sibling key.
    #[test]
    fn bun_and_deno_are_runtimes_of_their_own() {
        for (runtime, start) in [("bun", "bun run start"), ("deno", "deno task start")] {
            let json = serde_json::json!({
                "name": "app", "domain": "app.loc", "runtime": runtime
            });
            let m = normalize(&json, "{}", "app");
            assert!(m.valid, "{runtime}: {:?}", m.errors);
            assert_eq!(m.runtime, runtime);
            assert!(m.node.is_none(), "{runtime} must not produce a node block");
            let lang = m
                .lang
                .unwrap_or_else(|| panic!("{runtime} gets a lang block"));
            assert_eq!(lang.start, start);
        }
    }

    /// The registry constraint, held as a test because it is the one thing here
    /// that a reader would otherwise have to take on trust.
    ///
    /// `denoland/deno` publishes no major or minor tag — `deno:2` and `deno:2.9`
    /// are both absent — so the default has to be a full patch version. If
    /// somebody "tidies" it to `2`, every Deno project stops building against
    /// an image that does not exist, and the error arrives at build time rather
    /// than here.
    #[test]
    fn the_deno_default_is_a_full_version_because_the_publisher_ships_no_other() {
        let deno = lang_defaults("deno").unwrap();
        assert_eq!(
            deno.version.split('.').count(),
            3,
            "denoland/deno has no major-only or minor-only tag; got {:?}",
            deno.version
        );
    }

    #[test]
    fn lang_block_fields_override_defaults_and_empty_string_means_no_step() {
        let json = serde_json::json!({
            "name": "api", "domain": "api.loc", "runtime": "python",
            "python": { "version": "3.12", "start": "uvicorn app:app --host 0.0.0.0 --port 8000", "install": "" }
        });
        let m = normalize(&json, "{}", "api");
        let lang = m.lang.unwrap();
        assert_eq!(lang.version, "3.12");
        assert!(lang.start.starts_with("uvicorn"));
        // Explicitly empty is "skip the step", distinct from absent-take-default.
        assert_eq!(lang.install, None);
    }

    #[test]
    fn a_runtime_block_that_contradicts_the_runtime_is_an_error() {
        let json = serde_json::json!({
            "name": "x", "domain": "x.loc", "runtime": "python",
            "go": { "version": "1.23" }
        });
        let m = normalize(&json, "{}", "x");
        assert!(m.errors.iter().any(|e| e.path == "go"), "{:?}", m.errors);
    }

    #[test]
    fn golang_is_corrected_to_go_and_lang_manifests_round_trip() {
        let json = serde_json::json!({ "name": "svc", "domain": "svc.loc", "runtime": "golang" });
        let m = normalize(&json, "{}", "svc");
        assert_eq!(m.runtime, "go");

        // to_json writes the block under the runtime's own key, and reading
        // that back yields the same config — the round trip the settings
        // sheet depends on.
        let m2 = normalize(
            &serde_json::from_str(&to_json(&m)).unwrap(),
            &to_json(&m),
            "svc",
        );
        assert_eq!(m2.runtime, "go");
        assert_eq!(m2.lang, m.lang);
    }

    #[test]
    fn node_manifests_omit_php_only_fields() {
        let m = Manifest {
            name: "web".into(),
            domain: Some("web.loc".into()),
            runtime: "node".into(),
            server: None,
            document_root: None,
            aliases: vec![],
            lan_share: false,
            services: vec![],
            php: None,
            node: Some(NodeConfig {
                version: "22".into(),
                install: "npm install".into(),
                build: Some("npm run build".into()),
                start: "node server.js".into(),
                port: 3000,
                package_manager: None,
            }),
            lang: None,
            valid: true,
            errors: vec![],
            warnings: vec![],
            hooks: Default::default(),
            commands: Default::default(),
            local: Vec::new(),
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
            package_manager: None,
        });
        let path = std::env::temp_dir().join("stackvo-write-test.json");
        assert!(write(&path, &m).is_err());
        let _ = std::fs::remove_file(&path);
    }

    /// The same one-liner the reader tests use, repeated because this module
    /// is a different `mod tests` and importing across two of them for four
    /// lines would be worse than four lines.
    fn read_text(raw: &str, dir: &str) -> Manifest {
        let json: serde_json::Value = serde_json::from_str(raw).unwrap();
        normalize(&json, raw, dir)
    }

    /// The hazard `to_json`'s own comment names: a field the serialiser does
    /// not know about disappears the first time somebody edits an unrelated
    /// setting. For hooks that would be a project that quietly stopped
    /// migrating on start.
    #[test]
    fn hooks_survive_the_editor_round_trip() {
        let raw = r#"{
  "name": "shop",
  "domain": "shop.loc",
  "runtime": "php",
  "document_root": "public",
  "hooks": {
    "post-start": [
      { "exec": ["php", "artisan", "migrate", "--force"] },
      { "host": ["say", "up"] }
    ]
  },
  "php": {
    "version": "8.4",
    "extensions": ["pdo_mysql"]
  }
}
"#;
        let first = read_text(raw, "shop");
        let again = read_text(&to_json(&first), "shop");

        let steps = again.hooks.steps(crate::hooks::Event::PostStart);
        assert_eq!(steps.len(), 2, "{}", to_json(&first));
        assert_eq!(steps[0].argv, vec!["php", "artisan", "migrate", "--force"]);
        assert_eq!(steps[1].kind, crate::hooks::Kind::Host);

        // And the layout rule still holds: `php.extensions` is last.
        assert!(again.valid, "{:?}", again.errors);
    }

    /// Declared commands survive a form save (B-4).
    ///
    /// The same hazard the hooks round trip covers, and the reason both are
    /// written: this text is what `project_manifest_write` saves whenever
    /// somebody changes an unrelated setting, so a field the serialiser does
    /// not know about disappears silently. Losing the command a project runs
    /// every day is the same class of bug as one that quietly stopped
    /// migrating on start.
    #[test]
    fn declared_commands_survive_the_editor_round_trip() {
        let raw = r#"{
  "name": "shop",
  "domain": "shop.loc",
  "runtime": "php",
  "commands": {
    "reindex": { "exec": ["php", "artisan", "app:reindex"], "about": "Rebuild the index" },
    "console": { "exec": ["php", "artisan", "tinker"], "interactive": true },
    "bare": { "exec": ["php", "-v"] }
  },
  "php": {
    "version": "8.4"
  }
}
"#;
        let first = read_text(raw, "shop");
        assert!(first.warnings.is_empty(), "{:?}", first.warnings);
        let again = read_text(&to_json(&first), "shop");

        assert_eq!(again.commands.len(), 3, "{}", to_json(&first));

        let reindex = again.commands.get("reindex").expect("kept");
        assert_eq!(reindex.argv, ["php", "artisan", "app:reindex"]);
        assert_eq!(reindex.about, "Rebuild the index");
        assert!(!reindex.interactive);

        assert!(again.commands.get("console").unwrap().interactive);
        // `about` and `interactive` are optional and come back as they went in.
        assert_eq!(again.commands.get("bare").unwrap().about, "");

        // The order the author wrote, not the alphabetical one a map would give.
        let ids: Vec<&String> = again.commands.iter().map(|(id, _)| id).collect();
        assert_eq!(ids, ["reindex", "console", "bare"]);

        assert!(again.valid, "{:?}", again.errors);
    }

    /// A malformed declaration is a warning, like a malformed hook: a project
    /// with one unreadable command still has a name, a domain and a container.
    #[test]
    fn a_broken_declared_command_is_a_warning_and_the_manifest_stays_valid() {
        let m = read_text(
            r#"{"name":"shop","domain":"shop.loc","runtime":"php","commands":{"x":{"host":["./deploy.sh"]}},"php":{"version":"8.4"}}"#,
            "shop",
        );
        assert!(m.valid, "{:?}", m.errors);
        assert!(m.commands.is_empty());
        assert_eq!(m.warnings.iter().filter(|w| w.code == "COMMAND").count(), 1);
    }

    /// A malformed hook must not stop a project being opened or built.
    #[test]
    fn a_broken_hook_is_a_warning_and_the_manifest_stays_valid() {
        let m = read_text(
            r#"{"name":"shop","domain":"shop.loc","runtime":"php","hooks":{"post-start":[{"exec":"composer install"}]},"php":{"version":"8.4"}}"#,
            "shop",
        );
        assert!(m.valid, "{:?}", m.errors);
        assert!(
            m.warnings.iter().any(|w| w.code == "HOOK"),
            "{:?}",
            m.warnings
        );
    }

    // ---- B-2: the machine-local overlay -----------------------------------

    /// A project directory with a committed manifest and, optionally, a local
    /// one — in a fresh temp dir per test, because these read real files.
    fn project(dir_name: &str, committed: &str, local: Option<&str>) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-local-{dir_name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), committed).unwrap();
        if let Some(text) = local {
            std::fs::write(dir.join(LOCAL_FILE), text).unwrap();
        }
        dir
    }

    const COMMITTED: &str = r#"{
  "name": "shop",
  "domain": "shop.loc",
  "runtime": "php",
  "document_root": "public",
  "php": {
    "version": "8.4",
    "extensions": ["pdo_mysql", "redis"]
  }
}
"#;

    #[test]
    fn with_no_local_file_the_committed_manifest_is_what_is_read() {
        let dir = project("none", COMMITTED, None);
        let m = read(&dir.join(FILE), "shop").unwrap();
        assert_eq!(m.php.as_ref().unwrap().version, "8.4");
        assert!(m.local.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole point of the feature: a version this machine wants, without
    /// the file the team shares saying anything about it.
    #[test]
    fn a_local_file_overrides_the_committed_value_and_says_which() {
        let dir = project("php", COMMITTED, Some(r#"{"php": {"version": "8.3"}}"#));
        let m = read(&dir.join(FILE), "shop").unwrap();
        assert_eq!(m.php.as_ref().unwrap().version, "8.3");
        assert_eq!(m.local, vec!["php.version".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The reason the merge nests. A whole-value overlay would leave this
    /// project with no extensions at all, which is a build that succeeds and an
    /// application that cannot reach its database.
    #[test]
    fn overriding_one_field_of_a_block_keeps_the_rest_of_it() {
        let dir = project("nest", COMMITTED, Some(r#"{"php": {"version": "8.3"}}"#));
        let m = read(&dir.join(FILE), "shop").unwrap();
        assert_eq!(
            m.php.as_ref().unwrap().extensions,
            vec!["pdo_mysql", "redis"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An array is replaced whole, deliberately — see `overlay`.
    #[test]
    fn a_local_array_replaces_rather_than_appends() {
        let dir = project("arr", COMMITTED, Some(r#"{"php": {"extensions": ["gd"]}}"#));
        let m = read(&dir.join(FILE), "shop").unwrap();
        assert_eq!(m.php.as_ref().unwrap().extensions, vec!["gd"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The reason the merge happens before validation rather than after: a
    /// value from the local file is checked by the same rules, so a typo in it
    /// is reported instead of being carried into the renderer.
    #[test]
    fn an_override_is_validated_the_same_way_the_committed_value_would_be() {
        let dir = project("bad", COMMITTED, Some(r#"{"aliases": ["not a hostname"]}"#));
        let m = read(&dir.join(FILE), "shop").unwrap();
        assert!(
            m.warnings.iter().any(|w| w.code == "INVALID_ALIAS"),
            "{:?}",
            m.warnings
        );
        assert!(
            m.aliases.is_empty(),
            "the bad alias must not reach the router"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Named, not dropped. A local file that silently ignores the one key
    /// somebody set is how the feature gets written off as broken.
    #[test]
    fn a_refused_key_is_reported_rather_than_quietly_ignored() {
        let dir = project(
            "refused",
            COMMITTED,
            Some(r#"{"name": "elsewhere", "runtime": "node", "domain": "mine.loc"}"#),
        );
        let m = read(&dir.join(FILE), "shop").unwrap();
        assert_eq!(m.name, "shop");
        assert_eq!(m.runtime, "php");
        assert_eq!(m.domain.as_deref(), Some("mine.loc"));

        let refused: Vec<&str> = m
            .warnings
            .iter()
            .filter(|w| w.code == "LOCAL_REFUSED")
            .map(|w| w.path.as_str())
            .collect();
        assert_eq!(refused, vec!["name", "runtime"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// N. The one identity a local file may state: the directory it is in.
    ///
    /// This is what makes a worktree checkout a project of its own. The
    /// committed manifest in it belongs to the branch and says `shop`; the
    /// directory is `shop-feature-x` because two checkouts cannot share one
    /// folder; and W-04 would report that as a project that cannot be reached.
    #[test]
    fn a_local_file_may_restate_the_directory_it_is_in() {
        let dir = project(
            "worktree",
            COMMITTED,
            Some(r#"{"name": "shop-feature-x", "domain": "feature-x.shop.loc"}"#),
        );
        let m = read(&dir.join(FILE), "shop-feature-x").unwrap();

        assert_eq!(m.name, "shop-feature-x");
        assert_eq!(m.domain.as_deref(), Some("feature-x.shop.loc"));
        assert!(m.valid, "W-04 still fires: {:?}", m.errors);
        assert!(m.local.contains(&"name".to_string()));
        assert!(
            !m.warnings.iter().any(|w| w.code == "LOCAL_REFUSED"),
            "{:?}",
            m.warnings
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// And nothing wider than that. A name that is not the directory is the
    /// case the key was refused outright for, and it stays refused.
    #[test]
    fn a_local_file_may_not_rename_a_project_to_anything_else() {
        let dir = project("renamed", COMMITTED, Some(r#"{"name": "somewhere-else"}"#));
        let m = read(&dir.join(FILE), "shop").unwrap();

        assert_eq!(m.name, "shop", "the committed name stood");
        assert!(
            m.warnings
                .iter()
                .any(|w| w.code == "LOCAL_REFUSED" && w.path == "name"),
            "{:?}",
            m.warnings
        );

        // The write path refuses it too, and says which value it would take.
        let err = write_local(&dir, "shop", r#"{"name": "somewhere-else"}"#).unwrap_err();
        assert!(err.message.contains("\"shop\""), "{}", err.message);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The editor accepts what the worktree path writes, so the pane and the
    /// creation flow cannot disagree about what a legal overlay is.
    #[test]
    fn the_editor_accepts_the_overlay_a_worktree_is_given() {
        let dir = project("editable", COMMITTED, None);
        let text = r#"{"name": "shop-feature-x", "domain": "feature-x.shop.loc"}"#;

        let state = write_local(&dir, "shop-feature-x", text).unwrap();
        assert!(state.exists);
        assert!(state.refused.is_empty(), "{:?}", state.refused);
        assert!(state.applied.contains(&"domain".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Refused rather than skipped: running on the committed settings while the
    /// developer believes an override is in force is worse than not having one.
    #[test]
    fn a_local_file_that_is_not_json_fails_the_read() {
        let dir = project("broken", COMMITTED, Some("{ this is not json"));
        let err = read(&dir.join(FILE), "shop").unwrap_err();
        assert!(err.message.contains(LOCAL_FILE), "{}", err.message);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_committed_ignores_the_local_file_entirely() {
        let dir = project(
            "committed",
            COMMITTED,
            Some(r#"{"php": {"version": "8.3"}}"#),
        );
        let m = read_committed(&dir.join(FILE), "shop").unwrap();
        assert_eq!(m.php.as_ref().unwrap().version, "8.4");
        assert!(m.local.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The guard. Everything else about this feature fails loudly on its own;
    /// this is the one path where a mistake ends up in somebody else's clone.
    #[test]
    fn writing_back_an_overlaid_manifest_is_refused() {
        let dir = project("guard", COMMITTED, Some(r#"{"php": {"version": "8.3"}}"#));
        let m = read(&dir.join(FILE), "shop").unwrap();
        assert!(
            !m.local.is_empty(),
            "fixture must actually carry an override"
        );

        let err = write(&dir.join(FILE), &m).unwrap_err();
        assert!(err.message.contains("php.version"), "{}", err.message);

        // And the committed file is untouched.
        let after = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(after.contains("8.4"), "{after}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
