//! Registering `stackvo-mcp` with the assistants already on this machine.
//!
//! The README asks the reader to find their client's configuration file, work
//! out its shape, and paste a JSON block into it with a path they have to
//! supply themselves. Every competitor with an MCP server stopped asking that a
//! year ago — `lerd mcp:enable-global` writes eight clients, ServBay installs
//! its own rules file, Herd goes through Laravel Boost — and the competitive
//! review called this the cheapest item in the whole document (K-1).
//!
//! It is cheap. It is also the place where a small tool can do real damage,
//! because the file it edits is not ours: `~/.cursor/mcp.json` holds every
//! other server that person configured, and `~/.claude.json` holds their
//! projects. Three rules follow from that and are the reason this module is
//! longer than the feature sounds.
//!
//! **Read, insert one key, write back.** Never render a config from a
//! template. Anything already in the file survives, including keys this code
//! has never heard of, because [`serde_json::Value`] round-trips what it does
//! not understand.
//!
//! **A file that does not parse is not edited.** VS Code's `mcp.json` and
//! several others are JSON *with comments*, which `serde_json` refuses — and
//! the tempting fix, stripping comments and rewriting, silently deletes
//! somebody's notes. So an unparseable file is reported as unparseable, with
//! the block to paste, which is the README's path aimed at the exact file
//! rather than at the reader's memory. [`Client::insert`] never guesses.
//!
//! **The old contents are kept.** One `.stackvo-backup` beside the file,
//! rewritten each time rather than accumulating dated copies: this directory
//! belongs to the user's editor, and a feature that litters it is a feature
//! they turn off.
//!
//! ## The two that used to be missing (K-1)
//!
//! **Codex** was absent because its configuration is TOML and editing TOML
//! while preserving comments and key order needs `toml_edit` — a dependency,
//! which in this repository is a measured decision rather than an afterthought.
//! It was measured: toml_edit and toml_writer are **already in `Cargo.lock`**
//! through Tauri's own graph, so taking them directly adds two dependency edges
//! and zero packages. `Cargo.toml` carries the numbers.
//!
//! The shape was not written from memory either. A real `~/.codex/config.toml`
//! on the machine this was built on holds `[mcp_servers.node_repl]` with
//! `command`, `args`, `startup_timeout_sec` and a nested `[mcp_servers.<name>.env]`
//! table, and OpenAI's own diagnostics reference documents the same
//! `[mcp_servers.<name>]` block. That file — comments, quoted keys like
//! `[plugins."browser@openai-bundled"]`, and all — is what
//! `examples/agent_config_probe.rs` runs this module against.
//!
//! **Zed** was absent because its `context_servers` shape changed across
//! releases and could not be verified against a running copy. It still cannot —
//! Zed is not installed here — so the shape comes from Zed's current published
//! documentation rather than from memory, and it is the flat one:
//! `"context_servers": { "<name>": { "command": "…", "args": [], "env": {} } }`,
//! with no `source` key. The older nested `command: { path, args, env }` form
//! and the `"source": "custom"` key are both gone from that page.
//!
//! Zed's path is the other half of the same problem: its documentation does not
//! state one, and Zed keeps some things under `~/.config/zed` and others under
//! `~/Library/Application Support/Zed`. So this **looks in both** and writes
//! whichever exists, rather than picking one and being wrong on half the
//! machines — see [`config_candidates`].
//!
//! ## The binary
//!
//! `stackvo-mcp` is a second binary in this crate and is **not** bundled with
//! the app today — `tauri.conf.json` declares no `externalBin` and
//! `release.yml` does not build it. So [`binary`] looks for it rather than
//! assuming it, and when it is not there [`status`] says so and installing is
//! refused. A registration naming a path that does not exist is worse than no
//! registration: the client reports a server that will not start, and the
//! reason is in a log the user never sees.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// The name every client files this server under.
///
/// One name across clients on purpose: somebody who has registered it twice
/// should see the same entry, and a per-client name would make "is it already
/// installed?" a different question in each file.
pub const ENTRY: &str = "stackvo";

/// The suffix of the copy taken before a file is rewritten.
pub const BACKUP_SUFFIX: &str = ".stackvo-backup";

/// How a client spells an MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `{ "mcpServers": { "<name>": { "command": …, "args": [], "env": {} } } }`
    /// — Claude Code, Claude Desktop, Cursor, Windsurf, Gemini CLI.
    McpServers,
    /// `{ "servers": { "<name>": { "type": "stdio", "command": …, … } } }` —
    /// VS Code's own format, which names the map differently and requires the
    /// transport to be stated.
    VsCode,
    /// `{ "context_servers": { "<name>": { "command": …, "args": [], "env": {} } } }`
    /// — Zed. The entry is the same object every other JSON client uses; only
    /// the map's name differs.
    Zed,
    /// `[mcp_servers.<name>]` with `command`, `args` and an `env` sub-table —
    /// Codex, and the only one of these that is not JSON.
    CodexToml,
}

impl Shape {
    /// The top-level key the map of servers lives under.
    pub fn key(self) -> &'static str {
        match self {
            Shape::McpServers => "mcpServers",
            Shape::VsCode => "servers",
            Shape::Zed => "context_servers",
            Shape::CodexToml => "mcp_servers",
        }
    }

    /// Is this file TOML rather than JSON?
    ///
    /// One question asked in four places rather than a `match` in each: the
    /// difference runs through parsing, editing, reading back and the check
    /// that decides whether a button is offered, and a branch missed in any one
    /// of them is a file edited with the wrong parser.
    pub fn is_toml(self) -> bool {
        self == Shape::CodexToml
    }
}

/// One assistant, and where it keeps its configuration.
pub struct Client {
    pub id: &'static str,
    /// Shown in the pane. Not translated: these are product names.
    pub label: &'static str,
    pub shape: Shape,
}

/// The clients this can write.
pub const CLIENTS: &[Client] = &[
    Client {
        id: "claude-code",
        label: "Claude Code",
        shape: Shape::McpServers,
    },
    Client {
        id: "claude-desktop",
        label: "Claude Desktop",
        shape: Shape::McpServers,
    },
    Client {
        id: "cursor",
        label: "Cursor",
        shape: Shape::McpServers,
    },
    Client {
        id: "windsurf",
        label: "Windsurf",
        shape: Shape::McpServers,
    },
    Client {
        id: "vscode",
        label: "VS Code",
        shape: Shape::VsCode,
    },
    Client {
        id: "gemini-cli",
        label: "Gemini CLI",
        shape: Shape::McpServers,
    },
    Client {
        id: "codex",
        label: "Codex",
        shape: Shape::CodexToml,
    },
    Client {
        id: "zed",
        label: "Zed",
        shape: Shape::Zed,
    },
];

pub fn client(id: &str) -> Option<&'static Client> {
    CLIENTS.iter().find(|c| c.id == id)
}

/// Where a client's configuration file is on this platform.
///
/// The first candidate that exists, or the first candidate when none does —
/// see [`config_candidates`] for why some clients have more than one.
///
/// `None` when the home directory cannot be found, which is the one case where
/// there is no answer rather than a wrong one.
pub fn config_path(id: &str) -> Option<PathBuf> {
    let candidates = config_candidates(id);
    candidates
        .iter()
        .find(|path| path.is_file())
        .or_else(|| candidates.first())
        .cloned()
}

/// Every place a client might keep its configuration, best first.
///
/// One entry for all but two of them, and the two are not an inconsistency:
///
/// * **Zed** does not document a path, and keeps some things under
///   `~/.config/zed` and others under `~/Library/Application Support/Zed`.
///   Picking one would be right on some machines and silently wrong on the
///   rest — writing a file Zed never reads is the exact failure this module
///   exists to avoid.
/// * **Codex** honours `CODEX_HOME`, and the machine this was written on sets
///   it. A hard-coded `~/.codex` would edit a file that installation does not
///   read.
pub fn config_candidates(id: &str) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    if id == "codex" {
        let base = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| home.join(".codex"));
        return vec![base.join("config.toml")];
    }

    if id == "zed" {
        // `mut` is used by the two cfg blocks below, and neither compiles on
        // Linux — where this was an `unused_mut` warning, and `-D warnings`.
        #[cfg_attr(
            not(any(target_os = "macos", target_os = "windows")),
            allow(unused_mut)
        )]
        let mut out = vec![home.join(".config/zed/settings.json")];
        #[cfg(target_os = "macos")]
        out.push(home.join("Library/Application Support/Zed/settings.json"));
        #[cfg(target_os = "windows")]
        if let Some(dir) = dirs::config_dir() {
            out.push(dir.join("Zed/settings.json"));
        }
        return out;
    }

    single_config_path(id, &home).into_iter().collect()
}

fn single_config_path(id: &str, home: &Path) -> Option<PathBuf> {
    #[allow(unused_variables)]
    let home = home.to_path_buf();

    Some(match id {
        // Claude Code keeps one file at the root of the home directory, and it
        // holds more than servers — every project it has opened is in there.
        // The most important file this module touches.
        "claude-code" => home.join(".claude.json"),
        "claude-desktop" => {
            #[cfg(target_os = "macos")]
            {
                home.join("Library/Application Support/Claude/claude_desktop_config.json")
            }
            #[cfg(target_os = "windows")]
            {
                dirs::config_dir()?.join("Claude/claude_desktop_config.json")
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                home.join(".config/Claude/claude_desktop_config.json")
            }
        }
        "cursor" => home.join(".cursor/mcp.json"),
        "windsurf" => home.join(".codeium/windsurf/mcp_config.json"),
        "vscode" => {
            #[cfg(target_os = "macos")]
            {
                home.join("Library/Application Support/Code/User/mcp.json")
            }
            #[cfg(target_os = "windows")]
            {
                dirs::config_dir()?.join("Code/User/mcp.json")
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                home.join(".config/Code/User/mcp.json")
            }
        }
        "gemini-cli" => home.join(".gemini/settings.json"),
        _ => return None,
    })
}

// ---------------------------------------------------------------- the binary

/// How `stackvo-mcp` was found, so the pane can say which copy is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// Beside the running executable — an installed app with the sidecar, or a
    /// `cargo install` that put both in the same directory.
    Sibling,
    /// A `target/{debug,release}` directory, which is where a checkout has it.
    Build,
    /// On `PATH`.
    Path,
}

/// The `stackvo-mcp` executable, and where it was found.
///
/// Searched rather than assumed, in the order a wrong answer is least likely:
/// the copy shipped beside this executable first, then a build in this
/// checkout, then `PATH`. Returning the *first* hit matters — a stale
/// `target/debug` build alongside an installed app should not win over the
/// installed one.
pub fn binary() -> Option<(PathBuf, Source)> {
    let name = if cfg!(windows) {
        "stackvo-mcp.exe"
    } else {
        "stackvo-mcp"
    };

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(name);
            if sibling.is_file() {
                return Some((sibling, Source::Sibling));
            }

            // A `cargo run` of the desktop app lands in target/<profile>/, and
            // so does the server: same directory, already covered above. What
            // this covers is the app running from one profile while the server
            // was built into the other — `cargo run` during development with a
            // `--release` server built for a client to launch.
            for profile in ["release", "debug"] {
                if let Some(target) = dir.parent() {
                    let other = target.join(profile).join(name);
                    if other.is_file() {
                        return Some((other, Source::Build));
                    }
                }
            }
        }
    }

    which(name).map(|path| (path, Source::Path))
}

/// The first entry on `PATH` that is a file with this name.
///
/// Six lines rather than a crate. It is not a general `which` — no PATHEXT
/// handling beyond the caller passing `.exe`, no executable-bit check, because
/// a file at that name that cannot be executed is a broken installation this
/// code cannot repair and reporting it as missing would be the wrong sentence.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

// ------------------------------------------------------------- pure editing

/// The server entry itself, as the given client spells it.
///
/// `env` carries `STACKVO_ROOT` when a workspace is known. The server resolves
/// a workspace by itself and this is not required — but the resolution walks
/// default locations, and a client launched from a different working directory
/// than the app was is exactly when those disagree. Writing it down makes the
/// registration describe *this* installation rather than whichever one the
/// search happens to find.
pub fn entry(
    shape: Shape,
    command: &str,
    allow_writes: bool,
    root: Option<&str>,
) -> serde_json::Value {
    let mut object = serde_json::Map::new();

    if shape == Shape::VsCode {
        object.insert("type".into(), "stdio".into());
    }
    object.insert("command".into(), command.into());

    let args: Vec<serde_json::Value> = if allow_writes {
        vec!["--allow-writes".into()]
    } else {
        Vec::new()
    };
    object.insert("args".into(), args.into());

    if let Some(root) = root {
        let mut env = serde_json::Map::new();
        env.insert("STACKVO_ROOT".into(), root.into());
        object.insert("env".into(), env.into());
    }

    serde_json::Value::Object(object)
}

// ------------------------------------------------------------- TOML editing

/// Parse Codex's file, or say why it cannot be edited.
///
/// The same contract as [`document`] and for the same reasons: an empty file is
/// an empty document rather than an error, and anything that does not parse is
/// **reported** rather than rewritten. A TOML file that fails to parse is
/// usually one somebody is halfway through editing, and replacing it with a
/// clean render is how a tool eats an afternoon of somebody's configuration.
fn toml_document(text: &str) -> Result<toml_edit::DocumentMut> {
    text.parse::<toml_edit::DocumentMut>().map_err(|e| {
        Error::new(
            Code::InvalidInput,
            format!("the configuration file is not valid TOML: {e}"),
        )
        .with_hint(crate::hints::AGENT_CONFIG_UNPARSEABLE)
    })
}

/// `text` with our `[mcp_servers.stackvo]` block inserted or replaced.
///
/// `toml_edit` is what makes this safe: it keeps the document as it was written
/// — comments, blank lines, key order, `'single'` versus `"double"` quoting —
/// and edits the one table named here. Everything this file already held comes
/// back out unchanged, which is the same promise the JSON path makes through
/// `serde_json::Value`.
///
/// The table is created **implicit**, so a file with no `mcp_servers` at all
/// gains `[mcp_servers.stackvo]` and not an empty `[mcp_servers]` header above
/// it — the shape Codex itself writes.
pub fn toml_insert(
    text: &str,
    command: &str,
    allow_writes: bool,
    root: Option<&str>,
) -> Result<String> {
    use toml_edit::{value, Array, Item, Table};

    let mut document = toml_document(text)?;
    let table = document.as_table_mut();

    if !table.contains_key(Shape::CodexToml.key()) {
        let mut servers = Table::new();
        servers.set_implicit(true);
        table.insert(Shape::CodexToml.key(), Item::Table(servers));
    }

    // A `mcp_servers` that is a value rather than a table — someone's typo, or
    // an inline table — is refused rather than replaced, exactly as the JSON
    // path refuses a populated non-object.
    let Some(servers) = table
        .get_mut(Shape::CodexToml.key())
        .and_then(Item::as_table_mut)
    else {
        return Err(Error::new(
            Code::Conflict,
            format!(
                "`{}` in the configuration file is not a table",
                Shape::CodexToml.key()
            ),
        )
        .with_hint(crate::hints::AGENT_CONFIG_UNPARSEABLE));
    };

    let mut entry = Table::new();
    entry.insert("command", value(command));

    let mut args = Array::new();
    if allow_writes {
        args.push("--allow-writes");
    }
    entry.insert("args", value(args));

    if let Some(root) = root {
        let mut env = Table::new();
        env.insert("STACKVO_ROOT", value(root));
        entry.insert("env", Item::Table(env));
    }

    servers.insert(ENTRY, Item::Table(entry));
    Ok(document.to_string())
}

/// `text` without our block. An empty `mcp_servers` left behind is left behind,
/// for the reason [`remove`] gives about the JSON one.
pub fn toml_remove(text: &str) -> Result<String> {
    let mut document = toml_document(text)?;
    if let Some(servers) = document
        .as_table_mut()
        .get_mut(Shape::CodexToml.key())
        .and_then(toml_edit::Item::as_table_mut)
    {
        servers.remove(ENTRY);
    }
    Ok(document.to_string())
}

/// The command Codex's file already holds for us, if any.
pub fn toml_installed_command(text: &str) -> Option<String> {
    let document = text.parse::<toml_edit::DocumentMut>().ok()?;
    document
        .get(Shape::CodexToml.key())?
        .get(ENTRY)?
        .get("command")?
        .as_str()
        .map(str::to_string)
}

/// Parse a client's configuration file, or say why it cannot be edited.
///
/// An empty or whitespace-only file is an empty object, not an error: that is
/// what a file the client created and never wrote to looks like, and refusing
/// it would report a working installation as broken.
fn document(text: &str) -> Result<serde_json::Value> {
    if text.trim().is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }

    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        Error::new(
            Code::InvalidInput,
            format!("the configuration file is not valid JSON: {e}"),
        )
        .with_hint(crate::hints::AGENT_CONFIG_UNPARSEABLE)
    })?;

    // A top-level array or string parses and is not something a key can be put
    // into. Rewriting it would replace the file with an object, which is a
    // different kind of destruction from a torn write and just as complete.
    if !value.is_object() {
        return Err(Error::new(
            Code::InvalidInput,
            "the configuration file's top level is not a JSON object".to_string(),
        )
        .with_hint(crate::hints::AGENT_CONFIG_UNPARSEABLE));
    }

    Ok(value)
}

/// `text` with our entry inserted or replaced, and nothing else changed.
///
/// The whole feature's safety lives in this function, so it takes and returns
/// strings and touches no disk — the tests below drive it with the files real
/// clients ship.
pub fn insert(text: &str, shape: Shape, entry: serde_json::Value) -> Result<String> {
    let mut document = document(text)?;
    let object = document.as_object_mut().expect("checked in `document`");

    let servers = object
        .entry(shape.key())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    // The key exists but holds something else — a null left by a client that
    // writes the key before it has servers, or a list. Replacing a null is
    // right; replacing a populated non-object would discard it, so that is
    // refused instead.
    if servers.is_null() {
        *servers = serde_json::Value::Object(serde_json::Map::new());
    }
    let Some(map) = servers.as_object_mut() else {
        return Err(Error::new(
            Code::Conflict,
            format!(
                "`{}` in the configuration file is not an object",
                shape.key()
            ),
        )
        .with_hint(crate::hints::AGENT_CONFIG_UNPARSEABLE));
    };

    map.insert(ENTRY.to_string(), entry);
    render(&document, text)
}

/// `text` without our entry. Anything else in the file is untouched, including
/// an empty `mcpServers` left behind — removing the key as well would be this
/// code deciding something about a file it does not own.
pub fn remove(text: &str, shape: Shape) -> Result<String> {
    let mut document = document(text)?;
    let object = document.as_object_mut().expect("checked in `document`");

    if let Some(map) = object.get_mut(shape.key()).and_then(|v| v.as_object_mut()) {
        map.remove(ENTRY);
    }

    render(&document, text)
}

/// JSON in **the file's own indentation**, with a trailing newline.
///
/// This said "two-space, what every one of these files is already formatted as"
/// and that was a guess. `examples/agent_config_probe.rs`, run against the real
/// files on a developer's machine, found a four-space `~/.gemini/settings.json`
/// coming back two-space: 1,065 bytes in, 867 out, for an edit that added one
/// entry. Nothing was lost and the whole file still moved, which is the thing
/// this module refuses to do to a file it does not own.
///
/// Read from the first indented line rather than counted across the document: a
/// file mixes indentation only when somebody is already unhappy with it, and
/// the first line that is indented at all is the one an editor would have used
/// to guess the same thing.
fn render(document: &serde_json::Value, original: &str) -> Result<String> {
    let indent = detect_indent(original);
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut serialiser = serde_json::Serializer::with_formatter(&mut out, formatter);

    serde::Serialize::serialize(document, &mut serialiser).map_err(|e| {
        Error::new(
            Code::IoError,
            format!("serialising the configuration file: {e}"),
        )
    })?;

    let mut text = String::from_utf8(out).map_err(|e| {
        Error::new(
            Code::IoError,
            format!("the configuration file did not come back as text: {e}"),
        )
    })?;
    // Only where the file had one. A file that ended without a newline is a
    // file somebody's tooling wrote that way.
    if original.is_empty() || original.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

/// The indentation the file already uses: a tab, or however many spaces.
///
/// Two spaces when there is nothing to read it from, which is what every one of
/// these clients writes when it creates the file itself.
fn detect_indent(text: &str) -> String {
    for line in text.lines() {
        if line.starts_with('\t') {
            return "\t".to_string();
        }
        let spaces = line.len() - line.trim_start_matches(' ').len();
        if spaces > 0 && line.trim_start().starts_with('"') {
            return " ".repeat(spaces);
        }
    }
    "  ".to_string()
}

/// The entry a file already holds for us, if any.
pub fn installed_command(text: &str, shape: Shape) -> Option<String> {
    let document: serde_json::Value = serde_json::from_str(text).ok()?;
    document
        .get(shape.key())?
        .get(ENTRY)?
        .get("command")?
        .as_str()
        .map(str::to_string)
}

// -------------------------------------------------------------------- status

/// What one client looks like on this machine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub id: String,
    pub label: String,
    /// Absolute path to the configuration file, shown so the reader can open it
    /// when this refuses to write it.
    pub path: String,
    /// The file exists, or its directory does — meaning the client is on this
    /// machine and the file is ours to create.
    pub present: bool,
    /// The file exists right now. `present` without this means it would be
    /// created.
    pub exists: bool,
    /// The file is JSON this can edit. False means every button is withheld and
    /// the pane shows the block to paste.
    pub parseable: bool,
    /// The command currently registered under `stackvo`, when there is one.
    pub command: Option<String>,
    /// The registered command is the binary this app would install. False on a
    /// stale registration — the usual cause being a checkout that moved.
    pub current: bool,
}

/// The binary, the clients, and the block to paste when a file cannot be
/// written.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub binary: Option<String>,
    pub source: Option<Source>,
    pub root: Option<String>,
    pub clients: Vec<ClientStatus>,
}

/// Read every client's state. Never writes.
pub fn status(root: Option<&str>) -> Status {
    let found = binary();
    let command = found.as_ref().map(|(path, _)| path.display().to_string());

    let clients = CLIENTS
        .iter()
        .map(|client| {
            let path = config_path(client.id);
            let text = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
            let exists = path.as_ref().is_some_and(|p| p.is_file());

            // A file that is not there yet but whose directory is, is a client
            // that is installed and has simply never been given a server.
            let present = exists
                || path
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::is_dir))
                    .unwrap_or(false);

            // Asked of the parser that owns the format. A TOML file put
            // through `serde_json` is unparseable every time, which would show
            // Codex as broken on a machine where nothing is wrong.
            let parseable = match &text {
                Some(text) if client.shape.is_toml() => {
                    text.trim().is_empty() || toml_document(text).is_ok()
                }
                Some(text) => document(text).is_ok(),
                None => true,
            };

            let registered = text.as_deref().and_then(|text| {
                if client.shape.is_toml() {
                    toml_installed_command(text)
                } else {
                    installed_command(text, client.shape)
                }
            });

            ClientStatus {
                id: client.id.to_string(),
                label: client.label.to_string(),
                path: path
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| String::from("—")),
                present,
                exists,
                parseable,
                current: match (&registered, &command) {
                    (Some(registered), Some(command)) => registered == command,
                    _ => false,
                },
                command: registered,
            }
        })
        .collect();

    Status {
        binary: command,
        source: found.map(|(_, source)| source),
        root: root.map(str::to_string),
        clients,
    }
}

// --------------------------------------------------------------------- write

/// Read, edit and write one client's file.
///
/// `edit` receives the current text — empty when there is no file — and
/// returns what should replace it. The backup and the atomic write are here so
/// that install and remove cannot differ about them.
fn rewrite(id: &str, edit: impl FnOnce(&str) -> Result<String>) -> Result<String> {
    let Some(path) = config_path(id) else {
        return Err(Error::new(
            Code::NotFound,
            format!("no configuration path is known for {id}"),
        ));
    };

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = edit(&existing)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }

    // Only when there was something to lose. Writing a backup of a file that
    // did not exist leaves an empty `.stackvo-backup` that reads as a lost
    // configuration.
    if !existing.is_empty() {
        let backup = backup_path(&path);
        std::fs::write(&backup, &existing)
            .map_err(|e| Error::io(format!("writing {}", backup.display()), e))?;
    }

    crate::atomic::write(&path, &updated)?;
    Ok(path.display().to_string())
}

/// `~/.cursor/mcp.json` → `~/.cursor/mcp.json.stackvo-backup`.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(BACKUP_SUFFIX);
    path.with_file_name(name)
}

/// Register the server with one client. Returns the file written.
pub fn install(id: &str, allow_writes: bool, root: Option<&str>) -> Result<String> {
    let Some(client) = client(id) else {
        return Err(Error::new(
            Code::InvalidInput,
            format!("unknown client {id}"),
        ));
    };

    let Some((binary, _)) = binary() else {
        return Err(Error::new(
            Code::NotFound,
            "stackvo-mcp was not found on this machine".to_string(),
        )
        .with_hint(crate::hints::BUILD_THE_MCP_SERVER));
    };

    let command = binary.display().to_string();
    if client.shape.is_toml() {
        return rewrite(id, |text| toml_insert(text, &command, allow_writes, root));
    }

    let entry = entry(client.shape, &command, allow_writes, root);
    rewrite(id, |text| insert(text, client.shape, entry))
}

/// Take the entry back out. Returns the file written.
pub fn uninstall(id: &str) -> Result<String> {
    let Some(client) = client(id) else {
        return Err(Error::new(
            Code::InvalidInput,
            format!("unknown client {id}"),
        ));
    };
    if client.shape.is_toml() {
        return rewrite(id, toml_remove);
    }
    rewrite(id, |text| remove(text, client.shape))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file's own indentation is what it gets back.
    ///
    /// This module said "two-space, what every one of these files is already
    /// formatted as" and a real `~/.gemini/settings.json` was four-space: the
    /// round trip moved 1,065 bytes to 867 for an edit that added one entry.
    #[test]
    fn a_files_own_shape_survives_the_edit() {
        let four = "{\n    \"a\": {\n        \"b\": 1\n    }\n}\n";
        let out = insert(
            four,
            Shape::McpServers,
            entry(Shape::McpServers, "/bin/x", false, None),
        )
        .unwrap();
        assert!(
            out.contains("\n    \"a\""),
            "four spaces became something else: {out}"
        );
        assert_eq!(
            remove(&out, Shape::McpServers).unwrap(),
            format!("{{\n    \"a\": {{\n        \"b\": 1\n    }},\n    \"mcpServers\": {{}}\n}}\n"),
            "everything but the map we created has to come back"
        );

        let tabbed = "{\n\t\"a\": 1\n}\n";
        let out = insert(
            tabbed,
            Shape::McpServers,
            entry(Shape::McpServers, "/bin/x", false, None),
        )
        .unwrap();
        assert!(out.contains("\n\t\"a\""), "a tab-indented file: {out}");

        // A file that ended without a newline is one somebody's tooling wrote
        // that way, and adding one is still a change to a file we do not own.
        let no_newline = "{\"a\": 1}";
        let out = insert(
            no_newline,
            Shape::McpServers,
            entry(Shape::McpServers, "/bin/x", false, None),
        )
        .unwrap();
        assert!(!out.ends_with("\n\n"), "{out:?}");
    }

    /// Key order is the file's, not the alphabet's.
    ///
    /// `serde_json::Map` is a BTreeMap unless `preserve_order` is on, so every
    /// object this app rewrote came back sorted. On a 58 KB `~/.claude.json`
    /// that is a whole-file diff produced by adding one entry — measured, not
    /// imagined, by `examples/agent_config_probe.rs`.
    #[test]
    fn the_order_a_file_had_is_the_order_it_keeps() {
        let source = r#"{
  "zeta": 1,
  "alpha": 2,
  "middle": { "z": 1, "a": 2 }
}"#;
        let out = insert(
            source,
            Shape::McpServers,
            entry(Shape::McpServers, "/bin/x", false, None),
        )
        .unwrap();

        assert!(
            out.find("\"zeta\"").unwrap() < out.find("\"alpha\"").unwrap(),
            "the top level was sorted: {out}"
        );
        assert!(
            out.find("\"z\"").unwrap() < out.find("\"a\"").unwrap(),
            "a nested object was sorted: {out}"
        );
        // And ours goes at the end, where a new key belongs.
        assert!(out.find("mcpServers").unwrap() > out.find("middle").unwrap());
    }

    /// Codex's file, in the shape a real one has.
    ///
    /// Copied from the structure of `~/.codex/config.toml` on the machine this
    /// was written on: a bare key above every table, a quoted table name with
    /// an `@` in it, an existing MCP server with a nested `env` table, a
    /// single-quoted value, and comments that mark somebody else's managed
    /// block. Every one of those is a thing a plain serialiser would move,
    /// requote or delete.
    const CODEX: &str = r#"
notify = ["/Applications/Codex.app/Contents/MacOS/Client", "turn-ended"]

[marketplaces.openai-bundled]
last_updated = "2026-06-27T13:25:50Z"
source_type = "local"

[plugins."browser@openai-bundled"]
enabled = true

[mcp_servers.node_repl]
args = []
command = "/Applications/Codex.app/Contents/Resources/node_repl"
startup_timeout_sec = 120

[mcp_servers.node_repl.env]
NODE_REPL_TRUSTED_CODE_PATHS = "/Users/me/.codex"

# >>> somebody-else SessionStart >>>
[[hooks.session_start]]
command = 'echo "prefer the graph"'
# <<< somebody-else SessionStart <<<
"#;

    /// The promise of the whole module, on the format that could not keep it
    /// until now: everything already in the file comes back out unchanged.
    #[test]
    fn a_codex_file_survives_being_written_to() {
        let out = toml_insert(CODEX, "/usr/local/bin/stackvo-mcp", false, None).unwrap();

        // Somebody else's server, with its nested table and its extra key.
        assert!(out.contains("[mcp_servers.node_repl]"), "{out}");
        assert!(out.contains("startup_timeout_sec = 120"));
        assert!(out.contains("[mcp_servers.node_repl.env]"));
        // A quoted table name, a bare key above the tables, a single-quoted
        // value and the comments around it.
        assert!(out.contains(r#"[plugins."browser@openai-bundled"]"#));
        assert!(out.contains(
            r#"notify = ["/Applications/Codex.app/Contents/MacOS/Client", "turn-ended"]"#
        ));
        assert!(
            out.contains("command = 'echo \"prefer the graph\"'"),
            "{out}"
        );
        assert!(out.contains("# >>> somebody-else SessionStart >>>"));
        assert!(out.contains("# <<< somebody-else SessionStart <<<"));

        // And ours is in it.
        assert!(out.contains("[mcp_servers.stackvo]"), "{out}");
        assert!(out.contains(r#"command = "/usr/local/bin/stackvo-mcp""#));
        assert_eq!(
            toml_installed_command(&out).as_deref(),
            Some("/usr/local/bin/stackvo-mcp")
        );
    }

    /// A file with no `mcp_servers` at all gains the block Codex writes, and
    /// not an empty parent header above it.
    #[test]
    fn a_file_without_the_table_gains_the_shape_codex_writes() {
        let out = toml_insert("model = \"gpt-5\"\n", "/bin/mcp", true, Some("/w")).unwrap();

        assert!(
            out.contains("model = \"gpt-5\""),
            "the file's own key: {out}"
        );
        assert!(out.contains("[mcp_servers.stackvo]"), "{out}");
        assert!(
            !out.contains("[mcp_servers]\n"),
            "an empty parent header is not what Codex writes: {out}"
        );
        assert!(out.contains(r#"args = ["--allow-writes"]"#), "{out}");
        assert!(out.contains("[mcp_servers.stackvo.env]"), "{out}");
        assert!(out.contains(r#"STACKVO_ROOT = "/w""#), "{out}");

        // It has to parse as what it claims to be.
        let parsed: toml_edit::DocumentMut = out.parse().expect("valid TOML");
        assert_eq!(
            parsed["mcp_servers"]["stackvo"]["command"].as_str(),
            Some("/bin/mcp")
        );
    }

    /// Registering twice is one entry, not two, and the second registration
    /// wins — the same rule the JSON path follows.
    #[test]
    fn registering_codex_twice_replaces_rather_than_appends() {
        let once = toml_insert(CODEX, "/old/stackvo-mcp", false, None).unwrap();
        let twice = toml_insert(&once, "/new/stackvo-mcp", true, None).unwrap();

        assert_eq!(twice.matches("[mcp_servers.stackvo]").count(), 1, "{twice}");
        assert_eq!(
            toml_installed_command(&twice).as_deref(),
            Some("/new/stackvo-mcp")
        );
        assert!(twice.contains(r#"args = ["--allow-writes"]"#));
        assert!(twice.contains("[mcp_servers.node_repl]"), "still theirs");
    }

    /// Taking it out leaves everything else, including the empty parent — this
    /// module does not decide things about a file it does not own.
    #[test]
    fn removing_from_codex_leaves_the_rest_alone() {
        let with = toml_insert(CODEX, "/bin/mcp", false, Some("/w")).unwrap();
        let without = toml_remove(&with).unwrap();

        assert!(!without.contains("[mcp_servers.stackvo]"), "{without}");
        assert!(!without.contains("STACKVO_ROOT"));
        assert!(without.contains("[mcp_servers.node_repl]"));
        assert!(without.contains(r#"[plugins."browser@openai-bundled"]"#));
        assert_eq!(toml_installed_command(&without), None);
        // Removing what is not there is not an error.
        assert!(toml_remove(&without).is_ok());
    }

    /// A file halfway through being edited is reported, never rewritten.
    #[test]
    fn a_broken_codex_file_is_refused_rather_than_replaced() {
        let broken = "[mcp_servers.node_repl\ncommand = \"x\"\n";
        let error = toml_insert(broken, "/bin/mcp", false, None).unwrap_err();
        assert_eq!(error.code, Code::InvalidInput);
        assert!(
            error.message.contains("not valid TOML"),
            "{}",
            error.message
        );

        // And a key that is not a table is refused rather than overwritten.
        let wrong = "mcp_servers = 3\n";
        assert_eq!(
            toml_insert(wrong, "/bin/mcp", false, None)
                .unwrap_err()
                .code,
            Code::Conflict
        );
    }

    /// Zed's map has its own name and the same entry every other JSON client
    /// uses. Verified against Zed's current published documentation — the older
    /// nested `command: { path, args }` form and the `source` key are gone.
    #[test]
    fn zed_is_written_under_its_own_key_with_a_flat_command() {
        let out = insert(
            "{}",
            Shape::Zed,
            entry(Shape::Zed, "/bin/stackvo-mcp", false, Some("/w")),
        )
        .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let server = &parsed["context_servers"]["stackvo"];
        assert_eq!(server["command"], "/bin/stackvo-mcp");
        assert!(server["args"].is_array());
        assert_eq!(server["env"]["STACKVO_ROOT"], "/w");
        assert!(
            server.get("source").is_none(),
            "the docs have no source key"
        );
        assert!(server.get("type").is_none(), "that one is VS Code's");
        assert!(parsed.get("mcpServers").is_none(), "wrong map: {out}");
    }

    /// The two new clients have to be reachable the way every other one is —
    /// this is what a registration button is wired to.
    #[test]
    fn both_new_clients_are_known_and_have_a_path() {
        for id in ["codex", "zed"] {
            let client = client(id).unwrap_or_else(|| panic!("{id} is not in CLIENTS"));
            assert!(!client.label.is_empty());
            let candidates = config_candidates(id);
            assert!(!candidates.is_empty(), "{id} has nowhere to write");
            assert!(config_path(id).is_some(), "{id} resolved to no path");
        }

        // Codex's file is TOML and Zed's is not; everything downstream branches
        // on this and a wrong answer picks the wrong parser.
        assert!(client("codex").unwrap().shape.is_toml());
        assert!(!client("zed").unwrap().shape.is_toml());
    }

    /// Zed does not document a path and keeps things in two places, so both are
    /// looked for. A machine with neither still gets an answer to show.
    #[test]
    fn zed_is_looked_for_in_both_of_its_homes() {
        let candidates = config_candidates("zed");
        assert!(candidates
            .iter()
            .any(|p| p.ends_with(".config/zed/settings.json")));
        #[cfg(target_os = "macos")]
        assert!(
            candidates.len() > 1,
            "macOS keeps some of Zed's files under Application Support"
        );
    }

    /// `CODEX_HOME` moves the whole directory, and the machine this was written
    /// on sets it. A hard-coded `~/.codex` would edit a file that installation
    /// does not read.
    #[test]
    fn codex_follows_its_own_home_variable() {
        // Serialised against the other environment-reading tests by being the
        // only one that touches this variable.
        let previous = std::env::var_os("CODEX_HOME");
        // SAFETY: single-threaded within this test, and restored below.
        unsafe { std::env::set_var("CODEX_HOME", "/tmp/elsewhere") };
        assert_eq!(
            config_candidates("codex")
                .first()
                .map(|p| p.display().to_string()),
            Some("/tmp/elsewhere/config.toml".to_string())
        );

        unsafe { std::env::remove_var("CODEX_HOME") };
        assert!(config_candidates("codex")[0].ends_with(".codex/config.toml"));

        if let Some(value) = previous {
            unsafe { std::env::set_var("CODEX_HOME", value) };
        }
    }

    /// Cursor's file with two servers already in it, formatted as Cursor writes
    /// it. The point of every test below is that this survives.
    const CURSOR: &str = r#"{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_TOKEN": "ghp_example" }
    },
    "postgres": { "command": "/usr/local/bin/mcp-postgres" }
  }
}
"#;

    fn stackvo(text: &str) -> serde_json::Value {
        let document: serde_json::Value = serde_json::from_str(text).unwrap();
        document["mcpServers"]["stackvo"].clone()
    }

    #[test]
    fn an_existing_server_is_left_exactly_as_it_was() {
        let entry = entry(Shape::McpServers, "/opt/stackvo-mcp", false, None);
        let out = insert(CURSOR, Shape::McpServers, entry).unwrap();
        let after: serde_json::Value = serde_json::from_str(&out).unwrap();

        // Not "there are still three servers" — the contents, field by field.
        // A merge that kept the names and dropped the environment would pass
        // the count and lose somebody's token.
        assert_eq!(
            after["mcpServers"]["github"],
            serde_json::json!({
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-github"],
                "env": { "GITHUB_TOKEN": "ghp_example" }
            })
        );
        assert_eq!(
            after["mcpServers"]["postgres"]["command"],
            "/usr/local/bin/mcp-postgres"
        );
        assert_eq!(after["mcpServers"].as_object().unwrap().len(), 3);
    }

    /// Keys this code has never heard of are the common case: `~/.claude.json`
    /// carries a project list, an install id and more.
    #[test]
    fn keys_the_installer_does_not_understand_survive() {
        let before = r#"{"projects":{"/Users/x/work":{"allowedTools":[]}},"numStartups":41}"#;
        let entry = entry(Shape::McpServers, "/opt/stackvo-mcp", false, None);
        let out = insert(before, Shape::McpServers, entry).unwrap();
        let after: serde_json::Value = serde_json::from_str(&out).unwrap();

        assert_eq!(after["numStartups"], 41);
        assert_eq!(
            after["projects"]["/Users/x/work"]["allowedTools"],
            serde_json::json!([])
        );
        assert_eq!(
            after["mcpServers"]["stackvo"]["command"],
            "/opt/stackvo-mcp"
        );
    }

    #[test]
    fn installing_twice_replaces_rather_than_duplicates() {
        let first = insert(
            CURSOR,
            Shape::McpServers,
            entry(Shape::McpServers, "/old/stackvo-mcp", false, None),
        )
        .unwrap();
        let second = insert(
            &first,
            Shape::McpServers,
            entry(
                Shape::McpServers,
                "/new/stackvo-mcp",
                true,
                Some("/srv/stack"),
            ),
        )
        .unwrap();

        assert_eq!(stackvo(&second)["command"], "/new/stackvo-mcp");
        assert_eq!(
            stackvo(&second)["args"],
            serde_json::json!(["--allow-writes"])
        );
        assert_eq!(stackvo(&second)["env"]["STACKVO_ROOT"], "/srv/stack");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&second).unwrap()["mcpServers"]
                .as_object()
                .unwrap()
                .len(),
            3
        );
    }

    /// The write flag is the whole security question this feature raises: it
    /// grants an assistant `stack_down`. It must never be on unless asked for.
    #[test]
    fn the_write_flag_is_absent_unless_it_was_asked_for() {
        let off = entry(Shape::McpServers, "/opt/stackvo-mcp", false, None);
        assert_eq!(off["args"], serde_json::json!([]));

        let on = entry(Shape::McpServers, "/opt/stackvo-mcp", true, None);
        assert_eq!(on["args"], serde_json::json!(["--allow-writes"]));
    }

    #[test]
    fn removing_leaves_every_other_server_behind() {
        let with = insert(
            CURSOR,
            Shape::McpServers,
            entry(Shape::McpServers, "/opt/stackvo-mcp", false, None),
        )
        .unwrap();
        let without = remove(&with, Shape::McpServers).unwrap();
        let after: serde_json::Value = serde_json::from_str(&without).unwrap();

        assert!(after["mcpServers"].get("stackvo").is_none());
        assert_eq!(after["mcpServers"].as_object().unwrap().len(), 2);
        // Idempotent: removing what is not there is not an error, so a second
        // click cannot fail.
        assert!(remove(&without, Shape::McpServers).is_ok());
    }

    /// JSON with comments is what VS Code ships. Stripping them to make the
    /// edit possible would delete the reader's own notes from their own file.
    #[test]
    fn a_file_with_comments_is_refused_rather_than_rewritten() {
        let jsonc = "{\n  // the servers I use\n  \"servers\": {}\n}\n";
        let error = insert(
            jsonc,
            Shape::VsCode,
            entry(Shape::VsCode, "/opt/stackvo-mcp", false, None),
        )
        .unwrap_err();

        assert_eq!(error.code, Code::InvalidInput);
        // The key, not the English: that is what the front end translates, and
        // a hint that arrives without one reaches a Turkish reader in English.
        assert_eq!(
            error.hint_key,
            Some(crate::hints::AGENT_CONFIG_UNPARSEABLE.key)
        );
    }

    /// A top-level array parses. Writing an object over it would be a complete
    /// loss of the file with no read error to explain it.
    #[test]
    fn a_document_that_is_not_an_object_is_refused() {
        for text in ["[1, 2, 3]", "\"a string\"", "42"] {
            assert!(
                insert(
                    text,
                    Shape::McpServers,
                    entry(Shape::McpServers, "/opt/stackvo-mcp", false, None)
                )
                .is_err(),
                "{text} must not be overwritten"
            );
        }
    }

    /// An empty file is what a client leaves after creating the path and
    /// writing nothing. Treating it as a parse error would report a healthy
    /// installation as broken.
    #[test]
    fn an_empty_file_is_an_empty_document() {
        for text in ["", "   \n"] {
            let out = insert(
                text,
                Shape::McpServers,
                entry(Shape::McpServers, "/opt/stackvo-mcp", false, None),
            )
            .unwrap();
            assert_eq!(stackvo(&out)["command"], "/opt/stackvo-mcp");
        }
    }

    /// A client that writes the key before it has any servers leaves a null.
    /// That is a value with nothing to lose, so it is replaced; a populated
    /// value of the wrong type is not.
    #[test]
    fn a_null_server_map_is_replaced_and_a_populated_one_is_not() {
        let null = insert(
            r#"{"mcpServers": null}"#,
            Shape::McpServers,
            entry(Shape::McpServers, "/opt/stackvo-mcp", false, None),
        )
        .unwrap();
        assert_eq!(stackvo(&null)["command"], "/opt/stackvo-mcp");

        let wrong = insert(
            r#"{"mcpServers": ["github"]}"#,
            Shape::McpServers,
            entry(Shape::McpServers, "/opt/stackvo-mcp", false, None),
        );
        assert_eq!(wrong.unwrap_err().code, Code::Conflict);
    }

    /// VS Code names the map differently and requires the transport. Getting
    /// either wrong produces a file the editor reads without complaint and a
    /// server that never appears.
    #[test]
    fn vs_code_is_written_in_vs_codes_own_shape() {
        let out = insert(
            "{}",
            Shape::VsCode,
            entry(Shape::VsCode, "/opt/stackvo-mcp", false, None),
        )
        .unwrap();
        let after: serde_json::Value = serde_json::from_str(&out).unwrap();

        assert_eq!(after["servers"]["stackvo"]["type"], "stdio");
        assert_eq!(after["servers"]["stackvo"]["command"], "/opt/stackvo-mcp");
        assert!(after.get("mcpServers").is_none());

        // And the other five do not carry `type`, which is not part of their
        // schema.
        let other = entry(Shape::McpServers, "/opt/stackvo-mcp", false, None);
        assert!(other.get("type").is_none());
    }

    #[test]
    fn every_client_has_a_path_and_a_unique_id() {
        let mut seen = std::collections::BTreeSet::new();
        for client in CLIENTS {
            assert!(seen.insert(client.id), "{} is listed twice", client.id);
            assert!(
                config_path(client.id).is_some(),
                "{} has no configuration path",
                client.id
            );
            assert!(super::client(client.id).is_some());
        }
        assert!(config_path("no-such-client").is_none());
    }

    #[test]
    fn the_backup_sits_beside_the_file_it_copies() {
        let path = Path::new("/Users/x/.cursor/mcp.json");
        assert_eq!(
            backup_path(path),
            Path::new("/Users/x/.cursor/mcp.json.stackvo-backup")
        );
    }

    /// `status` reads the real home directory, so what can be asserted without
    /// one is its shape — every client answered for, and nothing invented when
    /// the binary is absent.
    #[test]
    fn status_answers_for_every_client() {
        let status = status(Some("/srv/stack"));
        assert_eq!(status.clients.len(), CLIENTS.len());
        assert_eq!(status.root.as_deref(), Some("/srv/stack"));

        for client in &status.clients {
            // A file that does not exist cannot be registered, and a client
            // reported as carrying a command it does not have would send
            // somebody looking for an entry that is not there.
            if !client.exists {
                assert!(client.command.is_none(), "{}", client.id);
                assert!(!client.current, "{}", client.id);
            }
        }
    }
}
