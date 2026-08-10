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
//! ## What is deliberately not here
//!
//! **Codex.** Its configuration is TOML (`~/.codex/config.toml`), and editing
//! TOML while preserving comments and key order needs `toml_edit` — a
//! dependency, which in this repository is a measured decision rather than an
//! afterthought (ADR 0010's cost section is the precedent). Writing it with a
//! plain serialiser would reformat a file we do not own, which is exactly the
//! rule above.
//!
//! **Zed.** Its `context_servers` shape changed across releases and could not
//! be verified against a running copy on the machine this was written on. A
//! shape written from memory is a config that silently does nothing.
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
}

impl Shape {
    /// The top-level key the map of servers lives under.
    fn key(self) -> &'static str {
        match self {
            Shape::McpServers => "mcpServers",
            Shape::VsCode => "servers",
        }
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
];

pub fn client(id: &str) -> Option<&'static Client> {
    CLIENTS.iter().find(|c| c.id == id)
}

/// Where a client's configuration file is on this platform.
///
/// `None` when the home directory cannot be found, which is the one case where
/// there is no answer rather than a wrong one.
pub fn config_path(id: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;

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
    render(&document)
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

    render(&document)
}

/// Two-space JSON with a trailing newline — what every one of these files is
/// already formatted as, and what an editor writing it back will produce.
fn render(document: &serde_json::Value) -> Result<String> {
    let mut text = serde_json::to_string_pretty(document).map_err(|e| {
        Error::new(
            Code::IoError,
            format!("serialising the configuration file: {e}"),
        )
    })?;
    text.push('\n');
    Ok(text)
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

            let parseable = match &text {
                Some(text) => document(text).is_ok(),
                None => true,
            };

            let registered = text
                .as_deref()
                .and_then(|text| installed_command(text, client.shape));

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

    let entry = entry(
        client.shape,
        &binary.display().to_string(),
        allow_writes,
        root,
    );
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
    rewrite(id, |text| remove(text, client.shape))
}

#[cfg(test)]
mod tests {
    use super::*;

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
