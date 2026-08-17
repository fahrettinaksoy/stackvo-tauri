//! `stackvo` — the same core, from a terminal (A-1).
//!
//! Eight of the ten tools measured in `docs/durum.md` §2 ship a CLI. This one
//! sat unbuilt not because it was expensive but because it is a **third
//! surface**: the desktop reaches the core through `commands.rs`, the assistant
//! through `mcp.rs`, and a third consumer is a third thing that can drift away
//! from `contracts/ipc.json` while every test still passes. §5's second
//! question was whether to accept that, and the answer is the one `mcp.rs`
//! already demonstrated — accept it, and hold it the same way.
//!
//! ## How this is held to the contract
//!
//! Every entry in [`COMMANDS`] names the `contracts/ipc.json` command it
//! implements, and `tests/cli_surface.rs` cross-checks the pair: a command
//! naming something the contract does not declare fails the build, and so does
//! a command advertised as read-only whose contract command is a `mutation`.
//! That is exactly what `mcp.rs` does, for exactly the same reason — nothing
//! enforces any of it at compile time.
//!
//! One thing here is stronger than the MCP table. A tool there dispatches on
//! its *name*, so a table entry with no matching arm compiles and fails at call
//! time — the module says as much and leaves a fallback arm for it. Here the
//! table carries an [`Action`], dispatch matches on the enum, and the compiler
//! refuses a variant with no arm. There is no "listed but not implemented"
//! state to test for because there is no way to reach one.
//!
//! ## The two pieces that were missing
//!
//! `docs/durum.md` named them: an argument parser and a progress writer.
//!
//! **The parser** is [`parse`], and it is hand-rolled. `clap` is the obvious
//! answer and would be a dependency, which in this repository is a measured
//! decision rather than a reflex — the same call `agents::which` made against a
//! `which` crate. What is actually needed is long and short flags, `--flag=x`
//! and `--flag x`, and `--` to stop. What matters far more than the shape of
//! that code is that an unrecognised flag is an **error**: a CLI that ignores
//! `--tial 50` and quietly gives you the default has lied about what it did.
//!
//! **The progress writer** is [`Narrate`], the `ProgressSink` ADR 0005 left
//! room for. `Sink::App` posts to a window, `Null` drops, `Recording` keeps —
//! this one prints. It writes to **stderr**, and that split is the whole
//! discipline of this binary: stdout carries the answer, stderr carries the
//! narration, so `stackvo doctor --json | jq` works while a build is still
//! scrolling past.
//!
//! ## What it does not do
//!
//! It does not shell out to the desktop app, and it does not start one. Every
//! command here calls the same domain function the window calls, which is what
//! ADR 0001 bought: `&Path` and `&dyn ProgressSink` instead of `State` and
//! `AppHandle`. Nothing in this file names a Tauri type.
//!
//! Writes go through the same audit trail as the window's. A person who runs
//! `stackvo down` and a person who clicks the button leave the same record;
//! anything else would make the log a description of one surface rather than of
//! the machine.
//!
//! ## Exit codes
//!
//! Scripts branch on these, so they are part of the surface and are tested:
//!
//! | code | meaning |
//! |---|---|
//! | 0 | it worked |
//! | 1 | it failed |
//! | 2 | the command line was wrong |
//! | 3 | no workspace — nothing has been set up on this machine yet |
//! | 4 | the Docker engine is unreachable |
//!
//! 3 and 4 are separated from 1 because they are the two failures a wrapper
//! script wants to handle rather than report: one means "run the app once",
//! the other means "start Docker".

use crate::error::{Code, Error, Result};
use crate::progress::ProgressSink;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::path::Path;

/// What a command does, once its arguments are in hand.
///
/// Dispatch matches on this rather than on the command's name, so a new
/// variant that nobody wired up does not compile. See the module comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Status,
    Doctor,
    Projects,
    Project,
    Services,
    Logs,
    Certs,
    Databases,
    Mail,
    Mcp,
    Up,
    Down,
    Start,
    Stop,
    Restart,
    Generate,
    Xdebug,
    CertsRenew,
    McpInstall,
    McpRemove,
    /// `stackvo artisan migrate` — a fixed program, the rest of the line handed
    /// to it (A-3).
    Passthrough,
    /// `stackvo exec <program> …` — the program named by the caller.
    Exec,
    /// `stackvo shell` — an interactive shell in the project's container.
    Shell,
    /// `stackvo tui` — the full-screen surface (M-8).
    Tui,
    /// `stackvo commands` — what this project offers (B-4).
    Commands,
    /// `stackvo run <id>` — one of them.
    Run,
}

/// One flag a command accepts.
pub struct Flag {
    pub long: &'static str,
    pub short: Option<char>,
    /// The name of the value it takes, or `None` for a switch.
    pub value: Option<&'static str>,
    pub help: &'static str,
}

/// What a command stands on.
///
/// Nineteen commands implement a command the contract declares, and the whole
/// argument for a third surface is that the pair is checked. The shell
/// commands (A-3) do not, and **cannot**, so rather than inventing a contract
/// entry for them the exception is named here and given a gate of its own.
///
/// The reason they cannot is [`crate::quickcmd`]'s: the webview may never name
/// a program to execute, so `contracts/ipc.json` has no command that takes one
/// — it has `quickcmd_run`, which takes an **id** out of a fixed catalogue.
/// That rule is about a *webview*, which runs code it did not choose from
/// pages it did not write. A terminal is the opposite: the person typing
/// already has a shell, and `stackvo artisan migrate` is strictly less
/// dangerous than the `docker exec -it stackvo-shop php artisan migrate` they
/// would otherwise type, because this one cannot get the container name wrong.
///
/// So the boundary moves, and moving it is written down rather than assumed —
/// which is also why [`Backing::HostShell`] carries its own assertions in
/// `cli_surface.rs` instead of simply being skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backing {
    /// A command in `contracts/ipc.json`. Cross-checked by test.
    Contract(&'static str),
    /// A screen, driving several contract commands rather than standing for
    /// one. Every name is still checked; what changes is that there is more
    /// than one, because "which command does `stackvo tui` implement" has no
    /// single honest answer.
    Surface(&'static [&'static str]),
    /// Runs a program in the project's container. No contract command exists
    /// or should — see above.
    HostShell,
}

/// One command, and what it stands on.
pub struct Command {
    pub name: &'static str,
    pub action: Action,
    pub backing: Backing,
    /// True when running it changes something on disk or in the stack.
    pub writes: bool,
    /// Positional arguments, spelled as they appear in `--help`.
    pub args: &'static str,
    /// How many positionals are required, and how many are allowed.
    ///
    /// Ignored for a [`Backing::HostShell`] command, which takes whatever it is
    /// given and hands it on.
    pub arity: (usize, usize),
    /// What runs inside the container, before the caller's own arguments.
    /// Empty for everything that is not a passthrough.
    pub prefix: &'static [&'static str],
    pub flags: &'static [Flag],
    pub summary: &'static str,
}

impl Command {
    /// Does this command take the rest of the line verbatim?
    ///
    /// `stackvo artisan migrate --force` has to reach artisan with `--force`
    /// intact, so flag parsing **stops** at the command name. That is what
    /// `git`, `docker` and `kubectl` all do, and the alternative — parsing the
    /// whole line and hoping no flag collides — breaks the first time somebody
    /// runs `artisan migrate --force`, which is the most common artisan call
    /// there is.
    pub fn passthrough(&self) -> bool {
        matches!(self.backing, Backing::HostShell)
    }

    /// The contract command, when this command stands for exactly one.
    pub fn contract(&self) -> Option<&'static str> {
        match self.backing {
            Backing::Contract(name) => Some(name),
            Backing::Surface(_) | Backing::HostShell => None,
        }
    }

    /// Every contract command this drives — one, several, or none.
    ///
    /// What `cli_surface.rs` checks names against, so a screen's list is held
    /// to the contract exactly as a single command's name is.
    pub fn contracts(&self) -> Vec<&'static str> {
        match self.backing {
            Backing::Contract(name) => vec![name],
            Backing::Surface(names) => names.to_vec(),
            Backing::HostShell => Vec::new(),
        }
    }
}

/// Accepted everywhere, so no command has to declare them.
pub const GLOBAL: &[Flag] = &[
    Flag {
        long: "json",
        short: None,
        value: None,
        help: "Print the raw result instead of a table.",
    },
    Flag {
        long: "root",
        short: None,
        value: Some("path"),
        help: "Use this StackVo directory instead of the default.",
    },
    // Global rather than declared by the seven commands that read it, and the
    // reason is the passthrough rule: flag parsing stops at a shell command's
    // name, so `--project` has to be written before it. A flag that can only
    // appear before the command is a global flag.
    Flag {
        long: "project",
        short: Some('p'),
        value: Some("name"),
        help: "Which project's container to use. Default: the one the working directory is in.",
    },
    Flag {
        long: "quiet",
        short: Some('q'),
        value: None,
        help: "Suppress progress narration on stderr.",
    },
    Flag {
        long: "no-color",
        short: None,
        value: None,
        help: "Never colour the output.",
    },
    Flag {
        long: "help",
        short: Some('h'),
        value: None,
        help: "Show this, or a command's own usage.",
    },
    Flag {
        long: "version",
        short: Some('V'),
        value: None,
        help: "Print the version.",
    },
];

const TAIL: Flag = Flag {
    long: "tail",
    short: Some('n'),
    value: Some("lines"),
    help: "How many lines from the end. Default 100.",
};

const FOLLOW: Flag = Flag {
    long: "follow",
    short: Some('f'),
    value: None,
    help: "Keep printing as the container writes. Ends on Ctrl-C.",
};

const LIMIT: Flag = Flag {
    long: "limit",
    short: None,
    value: Some("n"),
    help: "How many messages. Default 25.",
};

const MODE: Flag = Flag {
    long: "mode",
    short: None,
    value: Some("mode"),
    help: "minimal | services | projects | all. Default minimal.",
};

const SCOPE: Flag = Flag {
    long: "scope",
    short: None,
    value: Some("scope"),
    help: "all | projects | services. Default all.",
};

const ALLOW_WRITES: Flag = Flag {
    long: "allow-writes",
    short: None,
    value: None,
    help: "Register the server with its writing tools enabled.",
};

/// The shell commands, which differ only by what runs before the caller's
/// arguments. One row each rather than one `stackvo exec php …` for all of
/// them, because `stackvo artisan migrate` is the line people actually want to
/// type and the indirection buys nothing.
macro_rules! shell_command {
    ($name:literal, $action:expr, $prefix:expr, $args:literal, $summary:literal) => {
        Command {
            name: $name,
            action: $action,
            backing: Backing::HostShell,
            // Classified by what it *can* do, not by what a given call does:
            // `php -v` changes nothing and `php -r 'unlink(…)'` changes plenty,
            // and the surface cannot tell them apart. The heading in `--help`
            // has to be true of every call under it.
            writes: true,
            args: $args,
            arity: (0, 0), // unused — a passthrough takes whatever it is given
            prefix: $prefix,
            flags: &[],
            summary: $summary,
        }
    };
}

pub const COMMANDS: &[Command] = &[
    // ---- reads -----------------------------------------------------------
    Command {
        name: "status",
        action: Action::Status,
        backing: Backing::Contract("preflight"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "Whether anything will work: the workspace, the engine, every \
                  startup requirement, and how many projects are up.",
    },
    Command {
        name: "doctor",
        action: Action::Doctor,
        backing: Backing::Contract("doctor"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "The full diagnosis — requirements, port conflicts by holder, \
                  missing hosts entries, stale generated config, disk.",
    },
    Command {
        name: "projects",
        action: Action::Projects,
        backing: Backing::Contract("projects_list"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "Every managed project, with its domain and whether it is up.",
    },
    Command {
        name: "project",
        action: Action::Project,
        backing: Backing::Contract("project_get"),
        writes: false,
        args: "<name>",
        arity: (1, 1),
        prefix: &[],
        flags: &[],
        summary: "One project in full: manifest, container, Xdebug, the PHP \
                  limits the container actually reports, certificate cover.",
    },
    Command {
        name: "services",
        action: Action::Services,
        backing: Backing::Contract("services_list"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "The shared services — databases, caches, search — and their health.",
    },
    Command {
        name: "logs",
        action: Action::Logs,
        backing: Backing::Contract("container_logs_open"),
        writes: false,
        args: "<container>",
        arity: (1, 1),
        prefix: &[],
        flags: &[TAIL, FOLLOW],
        summary: "A container's output. The project or service id, without the \
                  stackvo- prefix.",
    },
    Command {
        name: "certs",
        action: Action::Certs,
        backing: Backing::Contract("cert_status"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "The HTTPS certificate: what it covers, what it misses, when it expires.",
    },
    Command {
        name: "db",
        action: Action::Databases,
        backing: Backing::Contract("db_targets"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "The database services, their databases and whether they are running.",
    },
    Command {
        name: "mail",
        action: Action::Mail,
        backing: Backing::Contract("mail_messages"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[LIMIT],
        summary: "The mail catcher's inbox: what the applications under test have sent.",
    },
    Command {
        name: "mcp",
        action: Action::Mcp,
        backing: Backing::Contract("agents_status"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "Which assistants on this machine have stackvo-mcp registered, \
                  and where each one's configuration file is.",
    },
    // ---- writes ----------------------------------------------------------
    Command {
        name: "up",
        action: Action::Up,
        backing: Backing::Contract("compose_up"),
        writes: true,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[MODE],
        summary: "Bring the stack up. Builds missing images, so a first run takes minutes.",
    },
    Command {
        name: "down",
        action: Action::Down,
        backing: Backing::Contract("compose_down"),
        writes: true,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "Bring the whole stack down: every profile, projects included.",
    },
    Command {
        name: "start",
        action: Action::Start,
        backing: Backing::Contract("project_start"),
        writes: true,
        args: "<project>",
        arity: (1, 1),
        prefix: &[],
        flags: &[],
        summary: "Start one project's container, then run its post-start hooks.",
    },
    Command {
        name: "stop",
        action: Action::Stop,
        backing: Backing::Contract("project_stop"),
        writes: true,
        args: "<project>",
        arity: (1, 1),
        prefix: &[],
        flags: &[],
        summary: "Run one project's pre-stop hooks, then stop its container.",
    },
    Command {
        name: "restart",
        action: Action::Restart,
        backing: Backing::Contract("project_restart"),
        writes: true,
        args: "<project>",
        arity: (1, 1),
        prefix: &[],
        flags: &[],
        summary: "Stop and start one project, with the hooks on both ends.",
    },
    Command {
        name: "generate",
        action: Action::Generate,
        backing: Backing::Contract("generate_run"),
        writes: true,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[SCOPE],
        summary: "Re-render the compose files, Dockerfiles and configs from the \
                  manifests. Repairs the doctor's \"generated config is stale\".",
    },
    Command {
        name: "xdebug",
        action: Action::Xdebug,
        backing: Backing::Contract("xdebug_set"),
        writes: true,
        args: "<project> on|off",
        arity: (2, 2),
        prefix: &[],
        flags: &[],
        summary: "Turn step debugging on or off for one project. The extension is \
                  compiled in, so a rebuild follows.",
    },
    Command {
        name: "certs-renew",
        action: Action::CertsRenew,
        backing: Backing::Contract("cert_apply"),
        writes: true,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "Reissue the certificate for the domains the projects have, and \
                  trust the CA if nothing does yet.",
    },
    Command {
        name: "mcp-install",
        action: Action::McpInstall,
        backing: Backing::Contract("agents_install"),
        writes: true,
        args: "<client>",
        arity: (1, 1),
        prefix: &[],
        flags: &[ALLOW_WRITES],
        summary: "Register stackvo-mcp with one assistant. `stackvo mcp` lists the ids.",
    },
    Command {
        name: "mcp-remove",
        action: Action::McpRemove,
        backing: Backing::Contract("agents_remove"),
        writes: true,
        args: "<client>",
        arity: (1, 1),
        prefix: &[],
        flags: &[],
        summary: "Take the stackvo entry back out of one assistant's configuration.",
    },
    Command {
        name: "commands",
        action: Action::Commands,
        backing: Backing::Contract("quick_commands"),
        writes: false,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "The commands this project offers: the built-in ones its files \
                  support, then the ones its own stackvo.json declares.",
    },
    // ---- a screen (M-8) ----------------------------------------------------
    Command {
        name: "tui",
        action: Action::Tui,
        // Several commands rather than one, because it is a screen: it lists,
        // it follows, and it starts and stops what is on it.
        backing: Backing::Surface(&[
            "projects_list",
            "services_list",
            "project_start",
            "project_stop",
            "container_logs_open",
        ]),
        writes: true,
        args: "",
        arity: (0, 0),
        prefix: &[],
        flags: &[],
        summary: "A full-screen view of the stack that you can work in: every project \
                  and service, live, with start and stop on the row under the cursor.",
    },
    Command {
        name: "run",
        action: Action::Run,
        backing: Backing::Contract("quick_command_run"),
        writes: true,
        args: "<id>",
        arity: (1, 1),
        prefix: &[],
        flags: &[],
        summary: "Run one of them by id. `stackvo commands` lists the ids; a \
                  project declares its own in stackvo.json.",
    },
    // ---- in the project's container (A-3) ---------------------------------
    shell_command!(
        "php",
        Action::Passthrough,
        &["php"],
        "[arguments…]",
        "Run PHP in this project's container — the version the project declares, \
         with its extensions and its php.ini, and nothing installed on the host."
    ),
    shell_command!(
        "artisan",
        Action::Passthrough,
        &["php", "artisan"],
        "[arguments…]",
        "Run an artisan command. `stackvo artisan migrate --force` reaches \
         artisan with `--force` intact."
    ),
    shell_command!(
        "composer",
        Action::Passthrough,
        &["composer"],
        "[arguments…]",
        "Run composer in the container, against the PHP the project actually \
         has — so a platform requirement resolves the way it will at run time."
    ),
    shell_command!(
        "npm",
        Action::Passthrough,
        &["npm"],
        "[arguments…]",
        "Run npm in the container."
    ),
    shell_command!(
        "node",
        Action::Passthrough,
        &["node"],
        "[arguments…]",
        "Run node in the container."
    ),
    shell_command!(
        "exec",
        Action::Exec,
        &[],
        "<program> [arguments…]",
        "Run any program in the container. The escape hatch for anything the \
         rows above do not cover."
    ),
    shell_command!(
        "shell",
        Action::Shell,
        &[],
        "",
        "Open an interactive shell in the container — bash where there is one, \
         sh otherwise."
    ),
];

pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

// --------------------------------------------------------------- parsing

/// One command line, resolved.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    /// `None` when the line was only `--help` or `--version`.
    pub command: Option<&'static str>,
    pub args: Vec<String>,
    /// Switches map to `"true"`; valued flags to their value.
    pub opts: BTreeMap<String, String>,
}

impl Parsed {
    pub fn on(&self, flag: &str) -> bool {
        self.opts.contains_key(flag)
    }

    pub fn value(&self, flag: &str) -> Option<&str> {
        self.opts.get(flag).map(String::as_str)
    }

    /// A numeric flag, or the default. A value that is not a number is an
    /// error rather than a silent fallback — `--tail abc` meaning 100 is the
    /// same lie as an ignored flag.
    pub fn number(&self, flag: &str, default: u32) -> Result<u32> {
        match self.opts.get(flag) {
            None => Ok(default),
            Some(raw) => raw.parse::<u32>().map_err(|_| {
                Error::new(
                    Code::InvalidInput,
                    format!("--{flag} takes a number, not \"{raw}\""),
                )
            }),
        }
    }

    pub fn resolved(&self) -> Option<&'static Command> {
        self.command.and_then(find)
    }
}

/// The flag with this long name or short letter, among a command's own and the
/// global set.
fn flag_of(
    command: Option<&'static Command>,
    long: &str,
    short: Option<char>,
) -> Option<&'static Flag> {
    let own = command.map(|c| c.flags).unwrap_or(&[]);
    own.iter().chain(GLOBAL.iter()).find(|f| match short {
        Some(letter) => f.short == Some(letter),
        None => f.long == long,
    })
}

/// Turn a command line into a [`Parsed`], or say why it cannot be one.
///
/// `argv` is what the process received **without** its own name. Flags may
/// appear before or after the command; a bare `--` ends flag parsing, so
/// `stackvo logs -- --weird-container-name` reaches the right place.
///
/// **A shell command ends flag parsing at its own name.** Everything after
/// `stackvo artisan` belongs to artisan, `--force` included. Without that rule
/// the most ordinary call there is — `artisan migrate --force` — would die on
/// this parser complaining about a flag it was never meant to read.
pub fn parse(argv: &[String]) -> Result<Parsed> {
    let mut out = Parsed::default();
    let mut command: Option<&'static Command> = None;
    let mut only_positionals = false;
    let mut index = 0;

    while index < argv.len() {
        let token = &argv[index];
        index += 1;

        if only_positionals {
            out.args.push(token.clone());
            continue;
        }

        if token == "--" {
            only_positionals = true;
            continue;
        }

        // A long flag, with or without `=value`.
        if let Some(rest) = token.strip_prefix("--") {
            let (name, inline) = match rest.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (rest, None),
            };

            let Some(flag) = flag_of(command, name, None) else {
                return Err(unknown_flag(command, &format!("--{name}")));
            };

            let value = match (flag.value, inline) {
                (None, Some(_)) => {
                    return Err(Error::new(
                        Code::InvalidInput,
                        format!("--{name} is a switch and takes no value"),
                    ))
                }
                (None, None) => "true".to_string(),
                (Some(_), Some(value)) => value,
                (Some(what), None) => {
                    let next = argv.get(index).cloned().ok_or_else(|| {
                        Error::new(
                            Code::InvalidInput,
                            format!("--{name} needs a {what} after it"),
                        )
                    })?;
                    index += 1;
                    next
                }
            };

            out.opts.insert(flag.long.to_string(), value);
            continue;
        }

        // Short flags. Not bundled: `-qf` is rejected rather than guessed at,
        // because the day one of these grows a value the bundle becomes
        // ambiguous and the guess becomes wrong silently.
        if token.len() == 2 && token.starts_with('-') {
            let letter = token.chars().nth(1).expect("two characters");
            let Some(flag) = flag_of(command, "", Some(letter)) else {
                return Err(unknown_flag(command, token));
            };

            let value = match flag.value {
                None => "true".to_string(),
                Some(what) => {
                    let next = argv.get(index).cloned().ok_or_else(|| {
                        Error::new(
                            Code::InvalidInput,
                            format!("-{letter} needs a {what} after it"),
                        )
                    })?;
                    index += 1;
                    next
                }
            };

            out.opts.insert(flag.long.to_string(), value);
            continue;
        }

        if token.starts_with('-') && token.len() > 2 {
            return Err(unknown_flag(command, token));
        }

        // The first bare word is the command; the rest are its arguments.
        if command.is_none() && out.command.is_none() {
            let found = find(token).ok_or_else(|| {
                let mut message = format!("there is no `stackvo {token}` command");
                if let Some(near) = nearest(token) {
                    message.push_str(&format!(" — did you mean `{near}`?"));
                }
                Error::new(Code::InvalidInput, message)
                    .with_hint("Run `stackvo --help` for the list.".to_string())
            })?;
            command = Some(found);
            out.command = Some(found.name);

            // A shell command owns the rest of the line. `stackvo artisan
            // migrate --force` must reach artisan whole, and a parser that kept
            // reading would eat `--force` and then complain about it.
            if found.passthrough() {
                out.args.extend_from_slice(&argv[index..]);
                return Ok(out);
            }
            continue;
        }

        out.args.push(token.clone());
    }

    // Arity is checked last, and skipped when the line is asking for help.
    // `stackvo logs --help` is a person who does not know what `logs` takes,
    // and answering them with "takes 1, and 0 were given" is the one reply
    // guaranteed not to contain what they asked for.
    if let Some(command) = command
        .filter(|c| !c.passthrough())
        .filter(|_| !out.opts.contains_key("help"))
    {
        let (min, max) = command.arity;
        if out.args.len() < min || out.args.len() > max {
            return Err(Error::new(
                Code::InvalidInput,
                format!(
                    "`stackvo {} {}` takes {}, and {} {} given",
                    command.name,
                    command.args,
                    if min == max {
                        format!("{min}")
                    } else {
                        format!("{min} to {max}")
                    },
                    out.args.len(),
                    if out.args.len() == 1 { "was" } else { "were" }
                ),
            ));
        }
    }

    Ok(out)
}

fn unknown_flag(command: Option<&'static Command>, spelled: &str) -> Error {
    let where_ = match command {
        Some(c) => format!("`stackvo {}`", c.name),
        None => "stackvo".to_string(),
    };
    Error::new(
        Code::InvalidInput,
        format!("{where_} does not take {spelled}"),
    )
    .with_hint(match command {
        Some(c) => format!("Run `stackvo {} --help` for its flags.", c.name),
        None => "Run `stackvo --help` for the flags.".to_string(),
    })
}

/// The closest command name, when one is close enough to be worth suggesting.
///
/// Levenshtein at a threshold of two, which catches the typos people actually
/// make — a transposition, a doubled letter, a missing one — and stops short of
/// suggesting `stop` for `status`.
fn nearest(typo: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .map(|c| (c.name, distance(typo, c.name)))
        .filter(|(_, d)| *d <= 2)
        .min_by_key(|(_, d)| *d)
        .map(|(name, _)| name)
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b.len()]
}

// -------------------------------------------------------------- narration

/// The progress writer ADR 0005 left room for: prints, to stderr.
///
/// stderr rather than stdout so `stackvo up --json > out` keeps the answer
/// clean while the build scrolls past on the terminal — and so a pipeline that
/// is only reading the result is not fed a compose build log.
pub struct Narrate {
    quiet: bool,
}

impl Narrate {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

impl ProgressSink for Narrate {
    fn event(&self, name: &str, payload: Value) {
        if self.quiet {
            return;
        }

        // Every long operation reports `line` as the child process wrote it;
        // the lifecycle events carry no line and are announced by their name.
        let mut err = std::io::stderr().lock();
        if let Some(line) = payload.get("line").and_then(Value::as_str) {
            let _ = writeln!(err, "{line}");
            return;
        }

        if name.ends_with(":error") {
            if let Some(message) = payload.get("error").and_then(Value::as_str) {
                let _ = writeln!(err, "{message}");
            }
        }
    }
}

// ---------------------------------------------------------------- colour

/// Whether to colour, decided once.
pub struct Style {
    colour: bool,
}

impl Style {
    /// Colour when stdout is a terminal, `NO_COLOR` is unset and `--no-color`
    /// was not given. The environment variable is honoured because a user who
    /// has set it once should not have to set it again per tool.
    pub fn resolve(disabled: bool) -> Self {
        Self {
            colour: !disabled
                && std::env::var_os("NO_COLOR").is_none()
                && std::io::stdout().is_terminal(),
        }
    }

    pub fn plain() -> Self {
        Self { colour: false }
    }

    /// Colour regardless of what stdout looks like.
    ///
    /// For `tui`, which has already taken the terminal: it refuses to start
    /// without one, so the question `resolve` asks has been answered by then —
    /// and asking it again would leave the screen grey on the one surface
    /// where colour is doing the most work.
    pub fn always() -> Self {
        Self { colour: true }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.colour {
            format!("\u{1b}[{code}m{text}\u{1b}[0m")
        } else {
            text.to_string()
        }
    }

    pub fn ok(&self, text: &str) -> String {
        self.paint("32", text)
    }

    pub fn warn(&self, text: &str) -> String {
        self.paint("33", text)
    }

    pub fn fail(&self, text: &str) -> String {
        self.paint("31", text)
    }

    pub fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    /// A preflight/doctor `state` string, coloured by what it means.
    fn state(&self, state: &str) -> String {
        match state {
            "ok" => self.ok("ok"),
            "warn" => self.warn("warn"),
            "fail" => self.fail("fail"),
            other => self.dim(other),
        }
    }

    fn yes_no(&self, value: bool) -> String {
        if value {
            self.ok("yes")
        } else {
            self.dim("no")
        }
    }
}

// ------------------------------------------------------------------ help

pub fn version_line() -> String {
    format!("stackvo {}", env!("CARGO_PKG_VERSION"))
}

pub fn help(style: &Style) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}\n\n",
        style.bold("stackvo — the StackVo stack from a terminal")
    ));
    out.push_str("Usage: stackvo <command> [arguments] [flags]\n\n");

    // Three groups rather than two, because a shell command is a third kind of
    // thing: `stackvo down` takes the stack away and `stackvo artisan migrate`
    // runs somebody's own command inside one container. Filing the second under
    // "Changes the stack" would be true and useless.
    let push = |title: &str, want: fn(&Command) -> bool, out: &mut String| {
        out.push_str(&format!("{}\n", style.bold(title)));
        let width = COMMANDS
            .iter()
            .filter(|c| want(c))
            .map(|c| c.name.len() + c.args.len() + 1)
            .max()
            .unwrap_or(0);

        for command in COMMANDS.iter().filter(|c| want(c)) {
            let spelled = if command.args.is_empty() {
                command.name.to_string()
            } else {
                format!("{} {}", command.name, command.args)
            };
            // The first sentence: the rest of the summary is for `--help` on
            // the command itself, and a list where every row wraps is a list
            // nobody scans.
            let first = command
                .summary
                .split_once(". ")
                .map(|(head, _)| format!("{head}."))
                .unwrap_or_else(|| command.summary.to_string());
            let first = first.split_whitespace().collect::<Vec<_>>().join(" ");
            out.push_str(&format!("  {spelled:width$}  {first}\n"));
        }
        out.push('\n');
    };

    push("Reads", |c| !c.writes, &mut out);
    push(
        "Changes the stack",
        |c| c.writes && !c.passthrough() && c.action != Action::Tui,
        &mut out,
    );
    push("Screens", |c| c.action == Action::Tui, &mut out);
    push(
        "Runs in the project's container",
        Command::passthrough,
        &mut out,
    );

    out.push_str(&format!("{}\n", style.bold("Flags")));
    for flag in GLOBAL {
        out.push_str(&format!("  {}\n", flag_line(flag)));
    }

    out.push_str(&format!(
        "\n{}\n",
        style.dim(
            "Exit: 0 ok · 1 failed · 2 bad command line · 3 no workspace · 4 Docker unreachable"
        )
    ));
    out.push_str(&style.dim("`stackvo <command> --help` for one command's own flags.\n"));
    out.push_str(&style.dim(
        "The container commands take the project the working directory is in; \
         --project names another.\n",
    ));
    // Said explicitly because it is not guessable: `stackvo artisan --help`
    // reaches artisan, which is correct and is also not where somebody looking
    // for this page will end up.
    out.push_str(
        &style.dim(
            "`stackvo artisan --help` goes to artisan — put --help first to see this app's.\n",
        ),
    );
    out
}

fn flag_line(flag: &Flag) -> String {
    let spelled = match (flag.short, flag.value) {
        (Some(s), Some(v)) => format!("-{s}, --{} <{v}>", flag.long),
        (Some(s), None) => format!("-{s}, --{}", flag.long),
        (None, Some(v)) => format!("    --{} <{v}>", flag.long),
        (None, None) => format!("    --{}", flag.long),
    };
    format!("{spelled:24}  {}", flag.help)
}

pub fn command_help(command: &Command, style: &Style) -> String {
    let spelled = if command.args.is_empty() {
        command.name.to_string()
    } else {
        format!("{} {}", command.name, command.args)
    };

    let mut out = format!("{}\n\n", style.bold(&format!("stackvo {spelled}")));
    out.push_str(&format!(
        "{}\n\n",
        command
            .summary
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    ));

    // Two different warnings, because they are two different risks. `down`
    // takes the stack away; `artisan` runs whatever you typed inside one
    // container. Printing the first sentence over the second would be true in
    // the letter that both are classified as writing, and misleading about
    // what is actually about to happen.
    if command.passthrough() {
        out.push_str(&format!(
            "{}\n\n",
            style.warn(
                "Runs what you give it, inside the container, and is recorded in the audit trail."
            )
        ));
    } else if command.writes {
        out.push_str(&format!(
            "{}\n\n",
            style.warn("This changes the stack, and is recorded in the audit trail.")
        ));
    }

    match command.backing {
        Backing::Contract(name) => {
            out.push_str(&format!("Implements the `{name}` command.\n\n"));
        }
        Backing::Surface(names) => {
            out.push_str(&format!("A screen over `{}`.\n\n", names.join("`, `")));
        }
        Backing::HostShell => {
            let shown = if command.prefix.is_empty() {
                "the program you name".to_string()
            } else {
                format!("`{}`", command.prefix.join(" "))
            };
            out.push_str(&format!(
                "Runs {shown} in the project's container. Everything after \
                 `stackvo {}` is passed on untouched, so its own flags reach \
                 it — write StackVo's flags before the command.\n\n",
                command.name
            ));
        }
    }

    out.push_str(&format!("{}\n", style.bold("Flags")));
    for flag in command.flags.iter().chain(GLOBAL.iter()) {
        out.push_str(&format!("  {}\n", flag_line(flag)));
    }
    out
}

// -------------------------------------------------------------- dispatch

/// What a command produced.
pub enum Outcome {
    /// A result to print — as JSON, or through [`render`].
    Value(Box<Value>),
    /// Already written to stdout as it arrived. `logs --follow` is the only
    /// one: a stream held until the end to be printed at once is not a stream.
    Streamed,
    /// A program ran in the container and this is what it exited with.
    ///
    /// Passed straight through rather than collapsed to success or failure:
    /// `stackvo artisan test` in a CI script is worth nothing if a failing test
    /// suite comes back as 0, and `phpunit` has more than one non-zero code.
    Exit(i32),
}

// ------------------------------------------------- the project's container
//
// A-3. `stackvo php -v` in a project directory runs the PHP that project
// declares, in the container it declares it in — which is the whole of what
// "host shell integration" means here. The competitors that do this put a
// shim on `PATH` and rewrite `php` itself; this does not, because the version
// is a property of a project rather than of a directory the shim guesses at.

/// The project a shell command will run in.
pub struct Target {
    pub name: String,
    pub container: String,
    pub running: bool,
    /// Where the project's source is mounted inside the container.
    ///
    /// `None` for a runtime whose image is **built** from the source rather
    /// than given it — Node and everything else. `generator.rs` writes no
    /// source mount for those on purpose: a bind mount over `/app` would
    /// shadow the built output. It matters here because it changes what the
    /// command does, so it is reported rather than assumed.
    pub mount: Option<&'static str>,
    /// The directory inside the container matching the caller's own, when the
    /// caller is somewhere under the project root and the source is mounted.
    pub workdir: Option<String>,
}

/// Which project the caller means.
///
/// Named explicitly with `--project`, or worked out from the working
/// directory — which is the point of the feature. Matched against the real
/// project list rather than against a directory name, because those differ:
/// a worktree's name comes from `stackvo.local.json`, not from its folder.
pub async fn target(root: &Path, wanted: Option<&str>, cwd: &Path) -> Result<Target> {
    let projects = crate::commands::list_projects(root).await?;

    let project = match wanted {
        Some(name) => projects
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| Error::not_found(format!("project {name}")))?,
        None => {
            let index: Vec<&str> = projects.iter().map(|p| p.path.as_str()).collect();
            let chosen = enclosing(&index, cwd).ok_or_else(|| {
                Error::new(
                    Code::NotFound,
                    format!("{} is not inside a StackVo project", cwd.display()),
                )
                .with_hint(
                    "cd into a project directory, or name one with --project <name>.".to_string(),
                )
            })?;
            let chosen = chosen.to_string();
            projects
                .into_iter()
                .find(|p| p.path == chosen)
                .expect("the path came out of this list")
        }
    };

    // Only PHP projects are given their source; see `Target::mount`.
    let mount = (project.runtime == "php").then_some("/var/www/html");
    let workdir = workdir_for(mount, Path::new(&project.path), cwd);

    Ok(Target {
        container: project.container_name.clone(),
        name: project.name,
        running: project.running,
        mount,
        workdir,
    })
}

/// The project directory containing `cwd`, deepest first.
///
/// **Deepest, not first**, and that is the whole of it: a worktree lands in the
/// project tree as a project of its own, and any layout where one project
/// directory sits inside another would otherwise answer with the outer one —
/// running `stackvo artisan migrate` against the parent branch's container
/// while standing in the feature branch's directory. That failure is silent
/// and migrates the wrong database.
fn enclosing<'a>(paths: &[&'a str], cwd: &Path) -> Option<&'a str> {
    paths
        .iter()
        .filter(|path| cwd.starts_with(Path::new(path)))
        .max_by_key(|path| path.len())
        .copied()
}

/// The directory inside the container matching `cwd`.
///
/// `None` without a mount: there is no counterpart to map onto, and guessing
/// one would hand `docker exec -w` a path that does not exist, which fails the
/// call rather than falling back to the root.
fn workdir_for(mount: Option<&'static str>, project: &Path, cwd: &Path) -> Option<String> {
    let mount = mount?;
    let relative = cwd.strip_prefix(project).ok()?;
    if relative.as_os_str().is_empty() {
        return Some(mount.to_string());
    }

    // Joined with `/` explicitly rather than by `Path::join`. The container is
    // Linux whatever the host is, so a Windows caller's `app\Http` has to
    // arrive as `app/Http` — and on Windows `join` would produce the backslash
    // and the path would simply not be found.
    let joined = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    Some(format!("{mount}/{joined}"))
}

/// The `docker` arguments for running `argv` in this target's container.
///
/// Separated from running it so a test can assert on the whole line without a
/// daemon — which is also what lets `cli_surface.rs` check that every
/// `HostShell` command really does run in a container rather than on the host.
pub fn exec_argv(target: &Target, argv: &[String], tty: bool) -> Vec<String> {
    let mut out = vec!["exec".to_string(), "-i".to_string()];
    if tty {
        out.push("-t".to_string());
    }
    // `-w` only where the source is mounted. Without a mount the host's
    // subdirectory has no counterpart inside, and `docker exec -w` on a
    // directory that does not exist fails the call rather than falling back.
    if let Some(dir) = &target.workdir {
        out.push("-w".to_string());
        out.push(dir.clone());
    }
    out.push(target.container.clone());
    out.extend(argv.iter().cloned());
    out
}

/// The argv to run inside the container, for one shell command.
///
/// Public so `cli_surface.rs` can check that what `--help` advertises as the
/// prefix is what actually runs.
pub fn container_argv(command: &Command, args: &[String]) -> Result<Vec<String>> {
    match command.action {
        Action::Passthrough => {
            let mut argv: Vec<String> = command.prefix.iter().map(|s| s.to_string()).collect();
            argv.extend(args.iter().cloned());
            Ok(argv)
        }
        Action::Exec => {
            if args.is_empty() {
                return Err(Error::new(
                    Code::InvalidInput,
                    "`stackvo exec` needs a program to run",
                ));
            }
            Ok(args.to_vec())
        }
        // The same fallback `pty.rs` uses, for the same reason: several images
        // in this catalogue are Alpine-based and have no bash, and a hardcoded
        // `bash` simply fails to open on those.
        Action::Shell => Ok(vec![
            "sh".to_string(),
            "-c".to_string(),
            "command -v bash >/dev/null 2>&1 && exec bash -l || exec sh".to_string(),
        ]),
        _ => Err(Error::new(
            Code::Unsupported,
            format!("{} does not run in a container", command.name),
        )),
    }
}

/// Run one shell command and hand back what it exited with.
async fn run_in_container(
    command: &Command,
    parsed: &Parsed,
    root: &Path,
    style: &Style,
) -> Result<Outcome> {
    let cwd = std::env::current_dir().map_err(|e| Error::io("reading the working directory", e))?;
    let target = target(root, parsed.value("project"), &cwd).await?;

    if !target.running {
        return Err(
            Error::new(Code::Conflict, format!("{} is not running", target.name))
                .with_hint(format!("Start it with `stackvo start {}`.", target.name)),
        );
    }

    // Said once, on stderr, and only where it is true. A Node project's image
    // is built from its source, so `stackvo npm install` writes into a copy
    // that goes away with the container — which is a surprise worth one line
    // rather than a silent difference in what the same command means.
    if target.mount.is_none() && !parsed.on("quiet") {
        let _ = writeln!(
            std::io::stderr(),
            "{} {} has no source mount — anything written here stays in the container",
            style.warn("note"),
            target.name
        );
    }

    let argv = container_argv(command, &parsed.args)?;
    // A TTY only when there is one to inherit. `stackvo shell` wants one;
    // `echo 'select 1' | stackvo php -a` in a pipeline must not be given one,
    // and `docker exec -t` without a terminal fails outright.
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    let status = tokio::process::Command::new("docker")
        .args(exec_argv(&target, &argv, tty))
        // stdin, stdout and stderr are inherited, which is the whole design:
        // this process is a pipe with a name, not something that buffers a
        // build log and prints it at the end.
        .status()
        .await
        .map_err(|e| {
            Error::new(
                Code::EngineUnreachable,
                format!("could not run docker: {e}"),
            )
            .with_hint("Is Docker installed and on PATH?".to_string())
        })?;

    crate::audit::record(
        "cli_container_exec",
        format!("{}: {}", target.name, argv.join(" ")),
        if status.success() {
            crate::audit::Outcome::Ok
        } else {
            crate::audit::Outcome::Failed
        },
    );

    // `None` means a signal killed it — Ctrl-C on an interactive shell, which
    // is an ordinary way to end one rather than a failure to report.
    Ok(Outcome::Exit(status.code().unwrap_or(0)))
}

/// Run one parsed command line.
///
/// Every arm calls the same domain function the window's command calls. The
/// `&dyn ProgressSink` is what makes that possible without a running app —
/// ADR 0005 — and `sink` is [`Narrate`] in a terminal.
pub async fn run(parsed: &Parsed, sink: &dyn ProgressSink, style: &Style) -> Result<Outcome> {
    let command = parsed
        .resolved()
        .ok_or_else(|| Error::new(Code::InvalidInput, "no command"))?;

    let workspace = crate::workspace::resolve();
    let root = workspace.require_root()?;
    let value = |v: Value| Ok(Outcome::Value(Box::new(v)));

    match command.action {
        // ---- in the project's container (A-3) -----------------------------
        Action::Passthrough | Action::Exec | Action::Shell => {
            run_in_container(command, parsed, &root, style).await
        }

        // ---- the project's own commands (B-4) -----------------------------
        Action::Commands => {
            let cwd = std::env::current_dir()
                .map_err(|e| Error::io("reading the working directory", e))?;
            let target = target(&root, parsed.value("project"), &cwd).await?;
            value(json!(crate::quickcmd::for_project(&root, &target.name)?))
        }

        Action::Run => {
            let cwd = std::env::current_dir()
                .map_err(|e| Error::io("reading the working directory", e))?;
            let target = target(&root, parsed.value("project"), &cwd).await?;
            let id = &parsed.args[0];

            // Through `quickcmd::resolve`, which is the only place either kind
            // of command becomes an argv — so the terminal cannot reach one the
            // pane would not, and neither can name a program.
            let command = crate::quickcmd::resolve(&root, &target.name, id)?;

            if !target.running {
                return Err(
                    Error::new(Code::Conflict, format!("{} is not running", target.name))
                        .with_hint(format!("Start it with `stackvo start {}`.", target.name)),
                );
            }

            let argv = crate::quickcmd::exec_argv(&target.container, &command);
            let status = tokio::process::Command::new("docker")
                .args(&argv)
                .status()
                .await
                .map_err(|e| {
                    Error::new(
                        Code::EngineUnreachable,
                        format!("could not run docker: {e}"),
                    )
                })?;

            crate::audit::record(
                "cli_quick_command",
                format!("{}: {}", target.name, command.display),
                if status.success() {
                    crate::audit::Outcome::Ok
                } else {
                    crate::audit::Outcome::Failed
                },
            );

            Ok(Outcome::Exit(status.code().unwrap_or(0)))
        }

        // ---- a screen (M-8) -----------------------------------------------
        //
        // Returns only when the person leaves it, and has already drawn
        // everything it had to say — so there is nothing left to print and
        // `Exit(0)` is the whole result.
        Action::Tui => {
            crate::tui::run(root).await?;
            Ok(Outcome::Exit(0))
        }

        // ---- reads --------------------------------------------------------
        Action::Status => {
            let preflight = crate::preflight::run().await;
            let engine = crate::engine::status().await;
            let projects = crate::commands::list_projects(&root)
                .await
                .unwrap_or_default();

            value(json!({
                "workspace": { "root": workspace.root, "version": workspace.stackvo_version },
                "engine": engine,
                "preflight": preflight,
                "projects": {
                    "total": projects.len(),
                    "running": projects.iter().filter(|p| p.running).count(),
                    "withProblems": projects.iter().filter(|p| !p.manifest_valid).count(),
                },
            }))
        }

        Action::Doctor => value(json!(crate::doctor::run(Some(&root)).await)),

        Action::Projects => value(json!(crate::commands::list_projects(&root).await?)),

        Action::Project => {
            let name = &parsed.args[0];
            let projects = crate::commands::list_projects(&root).await?;
            let project = projects
                .into_iter()
                .find(|p| &p.name == name)
                .ok_or_else(|| Error::not_found(format!("project {name}")))?;

            let certs = crate::certs::status(&root).await;
            let covered = project
                .domain
                .as_deref()
                .map(|d| crate::certs::covered_by(&certs.covered, d));

            value(json!({
                "project": project,
                "xdebug": crate::xdebug::status(&root, name).await.ok(),
                "phpIni": crate::phpini::status(&root, name).await.ok(),
                "certificateCoversDomain": covered,
                "container": crate::engine::inspect(name).await.ok(),
            }))
        }

        Action::Services => value(json!(crate::commands::list_services(&root).await?)),

        Action::Logs => {
            use futures_util::StreamExt;

            let container = &parsed.args[0];
            let tail = parsed.number("tail", 100)?.clamp(1, 100_000);
            let follow = parsed.on("follow");
            let stream = crate::engine::logs_stream(container, tail, follow)?;

            // Written out as it arrives rather than collected — the point of
            // `--follow` is that the last line has not happened yet. In `--json`
            // the same lines come back as an array, because a JSON document
            // that is still being appended to is not a JSON document.
            if follow || !parsed.on("json") {
                let mut out = std::io::stdout().lock();
                tokio::pin!(stream);
                while let Some(line) = stream.next().await {
                    if writeln!(out, "{}", line.text).is_err() {
                        break; // the pipe closed — `stackvo logs | head`
                    }
                }
                return Ok(Outcome::Streamed);
            }

            let lines: Vec<String> = stream.map(|line| line.text).collect().await;
            value(json!({ "container": container, "lines": lines }))
        }

        Action::Certs => value(json!(crate::certs::status(&root).await)),

        Action::Databases => value(json!(crate::db::targets(&root).await?)),

        Action::Mail => {
            let limit = parsed.number("limit", 25)?.clamp(1, 500);
            let status = crate::mail::status(&root).await?;
            let messages = if status.running {
                crate::mail::messages(&root, limit)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            value(json!({ "status": status, "messages": messages }))
        }

        Action::Mcp => value(json!(crate::agents::status(workspace.root.as_deref()))),

        // ---- writes -------------------------------------------------------
        Action::Up => {
            let mode = parsed.value("mode").unwrap_or("minimal").to_string();
            let mut args = crate::runner::compose_base_args(&root);
            args.extend(crate::runner::profile_args(&mode, &[])?);
            args.extend([
                "up".into(),
                "-d".into(),
                "--build".into(),
                "--pull=missing".into(),
                "--remove-orphans".into(),
            ]);

            let outcome = operation(sink, "up", &mode, &args, &root).await;
            audit("cli_stack_up", &mode, outcome.is_ok());
            outcome?;
            value(json!({ "mode": mode, "up": true }))
        }

        Action::Down => {
            let mut args = crate::runner::compose_base_args(&root);
            args.extend([
                "--profile".into(),
                "core".into(),
                "--profile".into(),
                "services".into(),
                "--profile".into(),
                "projects".into(),
                "down".into(),
            ]);

            let outcome = operation(sink, "down", "stack", &args, &root).await;
            audit("cli_stack_down", "stack", outcome.is_ok());
            outcome?;
            value(json!({ "down": true }))
        }

        Action::Start | Action::Stop | Action::Restart => {
            let name = parsed.args[0].clone();
            let (phase, action) = match command.action {
                Action::Start => (crate::events::START, "cli_project_start"),
                Action::Stop => (crate::events::STOP, "cli_project_stop"),
                _ => (crate::events::RESTART, "cli_project_restart"),
            };

            // The same hooks the window runs, in the same order and on the same
            // sink. Skipping them here would make `stackvo stop` and the stop
            // button two different operations wearing one name.
            if matches!(command.action, Action::Stop | Action::Restart) {
                crate::hooks::run_for_project(sink, &root, &name, crate::hooks::Event::PreStop)
                    .await;
            }

            let running_after = phase.running_after;
            let outcome = crate::commands::lifecycle(sink, "project", &name, phase).await;

            if outcome.is_ok() && matches!(command.action, Action::Start | Action::Restart) {
                crate::hooks::run_for_project(sink, &root, &name, crate::hooks::Event::PostStart)
                    .await;
            }

            audit(action, &name, outcome.is_ok());
            outcome?;
            value(json!({ "project": name, "running": running_after }))
        }

        Action::Generate => {
            let scope = parsed.value("scope").unwrap_or("all").to_string();
            let report = crate::commands::write_generated(&root, &scope, |file| {
                crate::progress::emit(sink, "generate:progress", json!({ "line": file }));
            });
            audit("cli_generate", &scope, report.is_ok());
            value(report?)
        }

        Action::Xdebug => {
            let name = parsed.args[0].clone();
            let enabled = match parsed.args[1].as_str() {
                "on" | "true" | "1" => true,
                "off" | "false" | "0" => false,
                other => {
                    return Err(Error::new(
                        Code::InvalidInput,
                        format!("xdebug takes `on` or `off`, not \"{other}\""),
                    ))
                }
            };

            let status = crate::xdebug::set(&root, &name, enabled).await;
            audit("cli_xdebug_set", &name, status.is_ok());
            value(json!(status?))
        }

        Action::CertsRenew => {
            let plan = crate::certs::apply(&root, true).await;
            audit("cli_cert_apply", "certificate", plan.is_ok());
            value(json!(plan?))
        }

        Action::McpInstall => {
            let client = parsed.args[0].clone();
            let path = crate::agents::install(
                &client,
                parsed.on("allow-writes"),
                workspace.root.as_deref(),
            );
            audit("cli_agent_install", &client, path.is_ok());
            value(json!({ "client": client, "path": path? }))
        }

        Action::McpRemove => {
            let client = parsed.args[0].clone();
            let path = crate::agents::uninstall(&client);
            audit("cli_agent_remove", &client, path.is_ok());
            value(json!({ "client": client, "path": path? }))
        }
    }
}

/// One compose run, narrated. The operation id keeps log correlation working.
async fn operation(
    sink: &dyn ProgressSink,
    prefix: &str,
    subject: &str,
    args: &[String],
    cwd: &Path,
) -> Result<()> {
    let operation_id = crate::events::next_operation_id(prefix);
    crate::runner::run_operation(
        sink,
        crate::runner::Operation {
            operation_id: &operation_id,
            subject,
            progress_event: "cli:progress",
            finished_event: "cli:done",
            program: "docker",
            args,
            cwd,
            env: &[],
        },
    )
    .await
}

/// The same trail the window writes.
///
/// Prefixed `cli_` rather than reusing the window's action names: the log
/// answers "what happened to this machine", and "somebody ran this in a
/// terminal" is part of the answer, not noise to be flattened away.
fn audit(action: &'static str, subject: &str, ok: bool) {
    crate::audit::record(
        action,
        subject,
        if ok {
            crate::audit::Outcome::Ok
        } else {
            crate::audit::Outcome::Failed
        },
    );
}

// --------------------------------------------------------------- printing

/// The human rendering of a result.
///
/// Rendered **from the same `Value` that `--json` prints**, never from a
/// separate query. That is what makes the two modes describe one thing: a field
/// this cannot find is one the JSON does not have either, so the table cannot
/// quietly claim something the machine-readable output denies.
pub fn render(action: Action, value: &Value, style: &Style) -> String {
    match action {
        Action::Status => render_status(value, style),
        Action::Doctor => render_doctor(value, style),
        Action::Projects => render_projects(value, style),
        Action::Project => render_project(value, style),
        Action::Services => render_services(value, style),
        Action::Certs | Action::CertsRenew => render_certs(value, style),
        Action::Databases => render_databases(value, style),
        Action::Mail => render_mail(value, style),
        Action::Mcp => render_mcp(value, style),
        Action::Logs => lines(value).into_iter().map(|l| format!("{l}\n")).collect(),
        Action::Up | Action::Down | Action::Generate | Action::Xdebug => {
            render_write(action, value, style)
        }
        Action::Start | Action::Stop | Action::Restart => {
            let name = str_at(value, "project").unwrap_or("?");
            let up = value
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            format!(
                "{} {name} — {}\n",
                style.ok("ok"),
                if up { "running" } else { "stopped" }
            )
        }
        Action::McpInstall => format!(
            "{} registered with {} — {}\n",
            style.ok("ok"),
            str_at(value, "client").unwrap_or("?"),
            str_at(value, "path").unwrap_or("?")
        ),
        Action::McpRemove => format!(
            "{} removed from {} — {}\n",
            style.ok("ok"),
            str_at(value, "client").unwrap_or("?"),
            str_at(value, "path").unwrap_or("?")
        ),
        // A shell command has no result of its own to render: the program's
        // own output went straight to the terminal and its exit code is the
        // answer. Reached only if one of them ever returns `Outcome::Value`,
        // and the arm exists because the compiler asks for it rather than
        // because there is anything to say.
        Action::Commands => render_commands(value, style),
        // `run` streams the program's own output and exits with its code;
        // there is no result of its own to render.
        Action::Passthrough | Action::Exec | Action::Shell | Action::Tui | Action::Run => {
            String::new()
        }
    }
}

fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn lines(value: &Value) -> Vec<String> {
    value
        .get("lines")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Aligned columns, padded to the widest cell.
///
/// Width in `chars()`, not bytes: a project called `çiçek` is five columns and
/// seven bytes, and byte padding would push every row after it out of line.
fn table(headers: &[&str], rows: &[Vec<String>], style: &Style) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let width = |s: &str| visible_width(s);
    let mut widths: Vec<usize> = headers.iter().map(|h| width(h)).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(width(cell));
            }
        }
    }

    let mut out = String::new();
    let header: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| pad(h, widths[i]))
        .collect();
    out.push_str(&format!("{}\n", style.dim(header.join("  ").trim_end())));

    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, c)| pad(c, *widths.get(i).unwrap_or(&0)))
            .collect();
        out.push_str(&format!("{}\n", cells.join("  ").trim_end()));
    }

    out
}

/// A cell's printed width, ignoring the escape sequences colour added.
fn visible_width(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
            continue;
        }
        width += 1;
    }
    width
}

fn pad(text: &str, to: usize) -> String {
    let visible = visible_width(text);
    let mut out = text.to_string();
    for _ in visible..to {
        out.push(' ');
    }
    out
}

fn render_status(value: &Value, style: &Style) -> String {
    let mut out = String::new();

    let root = value
        .pointer("/workspace/root")
        .and_then(Value::as_str)
        .unwrap_or("—");
    out.push_str(&format!("{:<12}{root}\n", "workspace"));

    let engine = value.get("engine").cloned().unwrap_or(Value::Null);
    let reachable = engine
        .get("reachable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let version = engine
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("not reachable");
    out.push_str(&format!(
        "{:<12}{} {}\n",
        "engine",
        if reachable {
            style.ok("up")
        } else {
            style.fail("down")
        },
        style.dim(version)
    ));

    let total = value
        .pointer("/projects/total")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let running = value
        .pointer("/projects/running")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let problems = value
        .pointer("/projects/withProblems")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    out.push_str(&format!(
        "{:<12}{running}/{total} running{}\n",
        "projects",
        if problems > 0 {
            format!(", {}", style.warn(&format!("{problems} with problems")))
        } else {
            String::new()
        }
    ));

    let preflight = value.get("preflight").cloned().unwrap_or(Value::Null);
    let ready = preflight
        .get("ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    out.push_str(&format!(
        "{:<12}{}\n",
        "ready",
        if ready {
            style.ok("yes")
        } else {
            style.fail("no")
        }
    ));

    // Only what is not `ok`. A list of what is fine is a list people stop
    // reading — the same argument `doctor.rs` makes about its DNS row.
    let rows: Vec<Vec<String>> = array(&preflight, "requirements")
        .iter()
        .filter(|r| r.get("state").and_then(Value::as_str) != Some("ok"))
        .map(|r| {
            vec![
                style.state(r.get("state").and_then(Value::as_str).unwrap_or("unknown")),
                r.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                r.get("detail")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();

    if !rows.is_empty() {
        out.push('\n');
        out.push_str(&table(&["", "requirement", "detail"], &rows, style));
    }

    out
}

fn render_doctor(value: &Value, style: &Style) -> String {
    let mut out = String::new();
    let mut findings = 0;

    let requirements: Vec<Vec<String>> = value
        .pointer("/preflight/requirements")
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|r| r.get("state").and_then(Value::as_str) != Some("ok"))
        .map(|r| {
            vec![
                style.state(r.get("state").and_then(Value::as_str).unwrap_or("unknown")),
                r.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                r.get("detail")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();

    if !requirements.is_empty() {
        findings += requirements.len();
        out.push_str(&format!("{}\n", style.bold("Requirements")));
        out.push_str(&table(&["", "id", "detail"], &requirements, style));
        out.push('\n');
    }

    let core: Vec<Vec<String>> = array(value, "core")
        .iter()
        .filter(|c| c.get("state").and_then(Value::as_str) != Some("ok"))
        .map(|c| {
            vec![
                style.state(c.get("state").and_then(Value::as_str).unwrap_or("unknown")),
                c.get("service")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                c.get("container")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();

    if !core.is_empty() {
        findings += core.len();
        out.push_str(&format!("{}\n", style.bold("Core containers")));
        out.push_str(&table(&["", "service", "container"], &core, style));
        out.push('\n');
    }

    // Every port the stack needs that somebody else is holding, named. This is
    // the row that turns "address already in use" into an answer.
    let ports: Vec<Vec<String>> = array(value, "ports")
        .iter()
        .filter(|p| p.get("state").and_then(Value::as_str) != Some("ok"))
        .map(|p| {
            vec![
                style.state(p.get("state").and_then(Value::as_str).unwrap_or("unknown")),
                p.get("port")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .to_string(),
                p.get("requiredBy")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                match (
                    p.get("process").and_then(Value::as_str),
                    p.get("pid").and_then(Value::as_u64),
                ) {
                    (Some(name), Some(pid)) => format!("{name} ({pid})"),
                    (Some(name), None) => name.to_string(),
                    _ => String::new(),
                },
            ]
        })
        .collect();

    if !ports.is_empty() {
        findings += ports.len();
        out.push_str(&format!("{}\n", style.bold("Ports")));
        out.push_str(&table(&["", "port", "needed by", "held by"], &ports, style));
        out.push('\n');
    }

    let missing: Vec<&str> = array(value, "hostsMissing")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if !missing.is_empty() {
        findings += missing.len();
        out.push_str(&format!("{}\n", style.bold("Missing from the hosts file")));
        for domain in &missing {
            out.push_str(&format!("  {domain}\n"));
        }
        out.push_str(
            &style.dim("  Add them from the app — the hosts file is written under review.\n\n"),
        );
    }

    let generated = value.get("generated").cloned().unwrap_or(Value::Null);
    if generated.get("state").and_then(Value::as_str) != Some("ok") {
        findings += 1;
        out.push_str(&format!(
            "{} {}\n",
            style.bold("Generated config"),
            style.state(
                generated
                    .get("state")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            )
        ));
        if let Some(detail) = generated.get("detail").and_then(Value::as_str) {
            out.push_str(&format!("  {detail}\n"));
        }
        out.push_str(&style.dim("  Repaired by `stackvo generate`.\n\n"));
    }

    if let Some(dns) = value.get("dns").filter(|d| !d.is_null()) {
        findings += 1;
        out.push_str(&format!(
            "{} {} resolves through port {}, and nothing is answering there\n\n",
            style.bold("DNS"),
            dns.get("suffix").and_then(Value::as_str).unwrap_or("?"),
            dns.get("port").and_then(Value::as_u64).unwrap_or(0)
        ));
    }

    let extensions: Vec<Vec<String>> = array(value, "extensions")
        .iter()
        .map(|e| {
            vec![
                e.get("subject")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                e.get("extension")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                e.get("detail")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();
    if !extensions.is_empty() {
        findings += extensions.len();
        out.push_str(&format!(
            "{}\n",
            style.bold("Extensions that will be dropped")
        ));
        out.push_str(&table(
            &["subject", "extension", "detail"],
            &extensions,
            style,
        ));
        out.push('\n');
    }

    // Withdrawn packages last, because it is the one finding that is not about
    // this machine being misconfigured: somebody else changed their mind about
    // bytes that are already here.
    let revoked: Vec<Vec<String>> = array(value, "revoked")
        .iter()
        .map(|r| {
            vec![
                r.get("instance")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                format!(
                    "{}@{}",
                    r.get("service").and_then(Value::as_str).unwrap_or("?"),
                    r.get("version").and_then(Value::as_str).unwrap_or("?")
                ),
                r.get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();
    if !revoked.is_empty() {
        findings += revoked.len();
        out.push_str(&format!("{}\n", style.bold("Withdrawn by their publisher")));
        out.push_str(&table(&["instance", "package", "reason"], &revoked, style));
        out.push('\n');
    }

    if findings == 0 {
        out.push_str(&format!("{} nothing to report\n", style.ok("ok")));
    }

    out
}

fn render_projects(value: &Value, style: &Style) -> String {
    let rows: Vec<Vec<String>> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|p| {
            let running = p.get("running").and_then(Value::as_bool).unwrap_or(false);
            let mut notes = Vec::new();
            if p.get("manifestValid").and_then(Value::as_bool) == Some(false) {
                notes.push(style.fail("manifest"));
            }
            if p.get("generatedStale").and_then(Value::as_bool) == Some(true) {
                notes.push(style.warn("stale"));
            }
            if p.get("domainConfigured").and_then(Value::as_bool) == Some(false) {
                notes.push(style.warn("no hosts entry"));
            }

            vec![
                if running {
                    style.ok("up")
                } else {
                    style.dim("down")
                },
                p.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                p.get("domain")
                    .and_then(Value::as_str)
                    .unwrap_or("—")
                    .to_string(),
                p.get("runtime")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                notes.join(" "),
            ]
        })
        .collect();

    if rows.is_empty() {
        return format!("{}\n", style.dim("no projects"));
    }

    table(&["", "name", "domain", "runtime", ""], &rows, style)
}

fn render_project(value: &Value, style: &Style) -> String {
    let project = value.get("project").cloned().unwrap_or(Value::Null);
    let mut out = String::new();

    let running = project
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    out.push_str(&format!(
        "{} {}\n",
        style.bold(project.get("name").and_then(Value::as_str).unwrap_or("?")),
        if running {
            style.ok("running")
        } else {
            style.dim("stopped")
        }
    ));

    let mut row = |label: &str, text: String| {
        out.push_str(&format!("{:<14}{text}\n", label));
    };

    row(
        "domain",
        project
            .get("domain")
            .and_then(Value::as_str)
            .unwrap_or("—")
            .to_string(),
    );
    row(
        "runtime",
        project
            .get("runtime")
            .and_then(Value::as_str)
            .unwrap_or("—")
            .to_string(),
    );
    row(
        "path",
        project
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("—")
            .to_string(),
    );
    row(
        "built",
        style.yes_no(
            project
                .get("built")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
    );

    // The three that explain a site not loading, which is what this command is
    // usually being asked.
    row(
        "hosts entry",
        style.yes_no(
            project
                .get("domainConfigured")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
    );

    match value
        .get("certificateCoversDomain")
        .and_then(Value::as_bool)
    {
        Some(true) => row("certificate", style.ok("covers this domain")),
        Some(false) => row("certificate", style.fail("does not cover this domain")),
        None => row("certificate", style.dim("no domain to check")),
    }

    if let Some(xdebug) = value.get("xdebug").filter(|x| !x.is_null()) {
        let on = xdebug
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let active = xdebug.get("active").and_then(Value::as_bool);
        row(
            "xdebug",
            match (on, active) {
                (true, Some(false)) => format!(
                    "{} {}",
                    style.warn("on"),
                    style.dim("— not compiled in yet, rebuild the project")
                ),
                (true, _) => style.ok("on"),
                (false, _) => style.dim("off"),
            },
        );
    }

    if project.get("generatedStale").and_then(Value::as_bool) == Some(true) {
        out.push_str(&format!(
            "\n{} the manifest has changed since anything was generated — `stackvo generate`\n",
            style.warn("stale")
        ));
    }

    out
}

fn render_services(value: &Value, style: &Style) -> String {
    let rows: Vec<Vec<String>> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|s| {
            let running = s.get("running").and_then(Value::as_bool).unwrap_or(false);
            let enabled = s.get("enabled").and_then(Value::as_bool).unwrap_or(false);
            let health = s.get("health").and_then(Value::as_str);

            vec![
                match (running, health) {
                    // `running` and `healthy` are different questions, and the
                    // reason `Service::health` exists: a database refusing every
                    // connection is a running container.
                    (true, Some("unhealthy")) => style.fail("sick"),
                    (true, Some("starting")) => style.warn("starting"),
                    (true, _) => style.ok("up"),
                    (false, _) if enabled => style.dim("down"),
                    (false, _) => style.dim("off"),
                },
                s.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                s.get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                s.get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();

    if rows.is_empty() {
        return format!("{}\n", style.dim("no services"));
    }

    table(&["", "service", "version", "url"], &rows, style)
}

fn render_certs(value: &Value, style: &Style) -> String {
    // `certs` returns a status and `certs-renew` a plan; the fields they share
    // are the ones printed, so one renderer serves both.
    let mut out = String::new();

    if let Some(days) = value.get("daysRemaining").and_then(Value::as_i64) {
        let expired = value
            .get("expired")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        out.push_str(&format!(
            "{:<12}{}\n",
            "expires",
            if expired {
                style.fail("expired")
            } else if days < 14 {
                style.warn(&format!("in {days} days"))
            } else {
                style.ok(&format!("in {days} days"))
            }
        ));
    }

    if let Some(trusted) = value.get("caTrusted").and_then(Value::as_bool) {
        out.push_str(&format!("{:<12}{}\n", "CA trusted", style.yes_no(trusted)));
    }

    let covered: Vec<&str> = array(value, "covered")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let missing: Vec<&str> = array(value, "missing")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let rejected: Vec<&str> = array(value, "rejected")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let added: Vec<&str> = array(value, "add")
        .iter()
        .filter_map(Value::as_str)
        .collect();

    if !added.is_empty() {
        out.push_str(&format!("{:<12}{}\n", "added", style.ok(&added.join(", "))));
    }
    if !covered.is_empty() {
        out.push_str(&format!("{:<12}{}\n", "covers", covered.join(", ")));
    }
    if !missing.is_empty() {
        out.push_str(&format!(
            "{:<12}{}\n",
            "missing",
            style.fail(&missing.join(", "))
        ));
        out.push_str(
            &style.dim("             `stackvo certs-renew` issues one that covers them.\n"),
        );
    }
    if !rejected.is_empty() {
        out.push_str(&format!(
            "{:<12}{}\n",
            "rejected",
            style.warn(&rejected.join(", "))
        ));
    }

    if let Some(error) = value.get("error").and_then(Value::as_str) {
        out.push_str(&format!("{:<12}{}\n", "error", style.fail(error)));
    }

    if out.is_empty() {
        out.push_str(&format!("{}\n", style.dim("no certificate yet")));
    }

    out
}

fn render_databases(value: &Value, style: &Style) -> String {
    let rows: Vec<Vec<String>> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|t| {
            vec![
                if t.get("running").and_then(Value::as_bool).unwrap_or(false) {
                    style.ok("up")
                } else {
                    style.dim("down")
                },
                t.get("service")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                t.get("database")
                    .and_then(Value::as_str)
                    .unwrap_or("—")
                    .to_string(),
                t.get("user")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();

    if rows.is_empty() {
        return format!("{}\n", style.dim("no database services"));
    }

    table(&["", "service", "database", "user"], &rows, style)
}

fn render_mail(value: &Value, style: &Style) -> String {
    let status = value.get("status").cloned().unwrap_or(Value::Null);
    let mut out = String::new();

    if !status
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return format!("{}\n", style.dim("the mail catcher is not running"));
    }

    if let Some(url) = status.get("uiUrl").and_then(Value::as_str) {
        out.push_str(&format!("{}\n\n", style.dim(url)));
    }

    let rows: Vec<Vec<String>> = array(value, "messages")
        .iter()
        .map(|m| {
            vec![
                m.get("from")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                array(m, "to")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                m.get("subject")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();

    if rows.is_empty() {
        out.push_str(&format!("{}\n", style.dim("the inbox is empty")));
        return out;
    }

    out.push_str(&table(&["from", "to", "subject"], &rows, style));
    out
}

fn render_commands(value: &Value, style: &Style) -> String {
    let rows: Vec<Vec<String>> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|c| {
            vec![
                c.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                c.get("display")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                // Where it came from, marked rather than left to be inferred
                // from the `because` column — the same thing the pane says.
                if c.get("declared").and_then(Value::as_bool) == Some(true) {
                    style.warn("project")
                } else {
                    style.dim("built in")
                },
                c.get("about")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();

    if rows.is_empty() {
        return format!("{}\n", style.dim("no commands for this project"));
    }

    let mut out = table(&["id", "runs", "from", ""], &rows, style);
    out.push_str(&style.dim("\n`stackvo run <id>` runs one.\n"));
    out
}

fn render_mcp(value: &Value, style: &Style) -> String {
    let mut out = String::new();

    match value.get("binary").and_then(Value::as_str) {
        Some(path) => out.push_str(&format!("{:<10}{path}\n\n", "server")),
        None => {
            out.push_str(&format!(
                "{} stackvo-mcp is not on this machine\n",
                style.warn("missing")
            ));
            out.push_str(
                &style.dim(
                    "         cargo build --release --bin stackvo-mcp, then run this again.\n\n",
                ),
            );
        }
    }

    let rows: Vec<Vec<String>> = array(value, "clients")
        .iter()
        .map(|c| {
            let present = c.get("present").and_then(Value::as_bool).unwrap_or(false);
            let parseable = c.get("parseable").and_then(Value::as_bool).unwrap_or(true);
            let command = c.get("command").and_then(Value::as_str);
            let current = c.get("current").and_then(Value::as_bool).unwrap_or(false);

            let state = match (parseable, present, command, current) {
                (false, _, _, _) => style.warn("unreadable"),
                (_, _, Some(_), true) => style.ok("registered"),
                (_, _, Some(_), false) => style.warn("stale"),
                (_, true, None, _) => style.dim("not registered"),
                _ => style.dim("not installed"),
            };

            vec![
                state,
                c.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                c.get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                c.get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .collect();

    out.push_str(&table(&["", "id", "client", "configuration"], &rows, style));
    out.push_str(
        &style.dim("\n`stackvo mcp-install <id>` registers it. Settings → Agents does the same.\n"),
    );
    out
}

fn render_write(action: Action, value: &Value, style: &Style) -> String {
    match action {
        Action::Up => format!(
            "{} the stack is up ({})\n",
            style.ok("ok"),
            str_at(value, "mode").unwrap_or("minimal")
        ),
        Action::Down => format!("{} the stack is down\n", style.ok("ok")),
        Action::Generate => {
            // `written` is the count and `files` the list — read as the report
            // actually shapes them. It said `0 files written` next to fourteen
            // narrated filenames until this was checked against the real
            // `--json`, which is the whole argument for rendering from the same
            // value rather than from a second query.
            let written = value.get("written").and_then(Value::as_u64).unwrap_or(0);
            let mut out = format!("{} {written} files written\n", style.ok("ok"));

            for warning in array(value, "warnings").iter().filter_map(Value::as_str) {
                out.push_str(&format!("{} {warning}\n", style.warn("warn")));
            }
            let skipped = array(value, "skipped").len();
            if skipped > 0 {
                out.push_str(&style.dim(&format!("   {skipped} skipped\n")));
            }
            out
        }
        Action::Xdebug => {
            let on = value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let active = value.get("active").and_then(Value::as_bool);
            let mut out = format!(
                "{} xdebug is {}\n",
                style.ok("ok"),
                if on { "on" } else { "off" }
            );
            if on && active == Some(false) {
                out.push_str(&style.dim(
                    "   the extension is compiled in, so the project needs generating and rebuilding\n",
                ));
            }
            out
        }
        _ => String::new(),
    }
}

// ------------------------------------------------------------------ entry

/// What the process should exit with. See the module comment's table.
pub fn exit_code(error: &Error) -> i32 {
    match error.code {
        Code::InvalidInput => 2,
        Code::NoWorkspace => 3,
        Code::EngineUnreachable => 4,
        _ => 1,
    }
}

/// The whole run: parse, dispatch, print, and say what to exit with.
///
/// `argv` is the process arguments **without** the program name.
pub async fn main(argv: Vec<String>) -> i32 {
    // Parsed before anything else looks at it, so `--no-color` is honoured even
    // by the error a bad command line produces.
    let disable_colour = argv.iter().any(|a| a == "--no-color");
    let style = Style::resolve(disable_colour);

    let parsed = match parse(&argv) {
        Ok(parsed) => parsed,
        Err(e) => {
            report(&e, &style);
            return exit_code(&e);
        }
    };

    if parsed.on("version") {
        println!("{}", version_line());
        return 0;
    }

    // `--root` before any workspace is resolved: every command below reads it
    // through `workspace::resolve`, which reads the environment.
    if let Some(root) = parsed.value("root") {
        std::env::set_var("STACKVO_ROOT", root);
    }

    let Some(command) = parsed.resolved() else {
        print!("{}", help(&style));
        // No command and no `--help` is a person who does not know what to type,
        // not an error to be scripted against; `--help` asked for the same text.
        return 0;
    };

    if parsed.on("help") {
        print!("{}", command_help(command, &style));
        return 0;
    }

    let sink = Narrate::new(parsed.on("quiet"));

    match run(&parsed, &sink, &style).await {
        Ok(Outcome::Streamed) => 0,
        Ok(Outcome::Exit(code)) => code,
        Ok(Outcome::Value(value)) => {
            if parsed.on("json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&*value).unwrap_or_else(|_| value.to_string())
                );
            } else {
                print!("{}", render(command.action, &value, &style));
            }
            0
        }
        Err(e) => {
            report(&e, &style);
            exit_code(&e)
        }
    }
}

/// An error, on stderr, with the hint the catalogue carries for it.
///
/// stderr because stdout is the answer, and a failure has no answer to put
/// there — a script redirecting stdout to a file should get an empty file and
/// a non-zero status, not a file with an error message in it.
fn report(error: &Error, style: &Style) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{} {}", style.fail("error"), error.message);
    if let Some(hint) = &error.hint {
        let _ = writeln!(err, "      {}", style.dim(hint));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn a_command_with_no_flags_parses() {
        let parsed = parse(&argv("projects")).unwrap();
        assert_eq!(parsed.command, Some("projects"));
        assert!(parsed.args.is_empty());
    }

    #[test]
    fn a_positional_reaches_the_command() {
        let parsed = parse(&argv("project shop")).unwrap();
        assert_eq!(parsed.command, Some("project"));
        assert_eq!(parsed.args, ["shop"]);
    }

    #[test]
    fn both_spellings_of_a_valued_flag_agree() {
        assert_eq!(
            parse(&argv("logs shop --tail 50")).unwrap().value("tail"),
            Some("50")
        );
        assert_eq!(
            parse(&argv("logs shop --tail=50")).unwrap().value("tail"),
            Some("50")
        );
        assert_eq!(
            parse(&argv("logs shop -n 50")).unwrap().value("tail"),
            Some("50")
        );
    }

    #[test]
    fn a_switch_is_true_by_being_present() {
        let parsed = parse(&argv("logs shop --follow")).unwrap();
        assert!(parsed.on("follow"));
        assert!(!parse(&argv("logs shop")).unwrap().on("follow"));
    }

    #[test]
    fn a_global_flag_works_before_and_after_the_command() {
        assert!(parse(&argv("--json projects")).unwrap().on("json"));
        assert!(parse(&argv("projects --json")).unwrap().on("json"));
    }

    /// The rule this parser exists for. A CLI that shrugs at `--tial 50` and
    /// uses the default has told the user it did something it did not do.
    #[test]
    fn an_unknown_flag_is_refused_and_named() {
        let error = parse(&argv("logs shop --tial 50")).unwrap_err();
        assert_eq!(error.code, Code::InvalidInput);
        assert!(error.message.contains("--tial"), "{}", error.message);
    }

    /// And a flag belonging to a different command is just as unknown — `--tail`
    /// on `projects` would otherwise be accepted and silently ignored.
    #[test]
    fn a_flag_from_another_command_is_refused() {
        assert!(parse(&argv("projects --tail 5")).is_err());
        assert!(parse(&argv("logs shop --tail 5")).is_ok());
    }

    #[test]
    fn a_valued_flag_with_nothing_after_it_is_an_error() {
        let error = parse(&argv("logs shop --tail")).unwrap_err();
        assert!(error.message.contains("needs a"), "{}", error.message);
    }

    #[test]
    fn a_switch_given_a_value_is_an_error() {
        let error = parse(&argv("logs shop --follow=yes")).unwrap_err();
        assert!(
            error.message.contains("takes no value"),
            "{}",
            error.message
        );
    }

    #[test]
    fn too_few_or_too_many_positionals_are_refused() {
        assert!(parse(&argv("project")).is_err());
        assert!(parse(&argv("project a b")).is_err());
        assert!(parse(&argv("xdebug shop on")).is_ok());
        assert!(parse(&argv("xdebug shop")).is_err());
    }

    /// Asking a command what it takes must not be answered with a complaint
    /// about not having given it. `stackvo logs --help` did exactly that.
    #[test]
    fn help_survives_the_arity_check() {
        for line in ["logs --help", "project -h", "xdebug --help"] {
            let parsed = parse(&argv(line))
                .unwrap_or_else(|e| panic!("`stackvo {line}` was refused: {}", e.message));
            assert!(parsed.on("help"));
            assert!(parsed.resolved().is_some());
        }
    }

    #[test]
    fn a_double_dash_ends_flag_parsing() {
        let parsed = parse(&argv("logs -- --weird")).unwrap();
        assert_eq!(parsed.args, ["--weird"]);
    }

    #[test]
    fn an_unknown_command_suggests_the_nearest_one() {
        let error = parse(&argv("porject")).unwrap_err();
        assert!(error.message.contains("project"), "{}", error.message);
    }

    /// And does not suggest one that is merely short. `stop` is three edits
    /// from `status`; offering it would send somebody to the wrong command.
    #[test]
    fn a_distant_name_gets_no_suggestion() {
        let error = parse(&argv("zzzzzzzz")).unwrap_err();
        assert!(!error.message.contains("did you mean"), "{}", error.message);
    }

    #[test]
    fn a_bad_number_is_an_error_rather_than_the_default() {
        let parsed = parse(&argv("logs shop --tail abc")).unwrap();
        let error = parsed.number("tail", 100).unwrap_err();
        assert!(
            error.message.contains("takes a number"),
            "{}",
            error.message
        );
        assert_eq!(
            parse(&argv("logs shop"))
                .unwrap()
                .number("tail", 100)
                .unwrap(),
            100
        );
    }

    #[test]
    fn every_command_name_is_unique() {
        let mut names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "two commands share a name");
    }

    /// A command whose arity does not admit its own spelled arguments is one
    /// whose `--help` is a lie.
    #[test]
    fn the_spelled_arguments_match_the_arity() {
        for command in COMMANDS.iter().filter(|c| !c.passthrough()) {
            let spelled = command.args.split_whitespace().count();
            let (min, max) = command.arity;
            assert!(min <= max, "{} has an impossible arity", command.name);
            assert_eq!(
                spelled, max,
                "`{} {}` spells {spelled} arguments and allows {max}",
                command.name, command.args
            );
        }
    }

    /// A passthrough has no arity, so its usage line has to say so.
    ///
    /// `stackvo artisan <name>` would read as taking exactly one argument, and
    /// somebody would believe it. The ellipsis is the only thing standing
    /// between the reader and that reading, so it is checked rather than left
    /// to whoever adds the next row.
    #[test]
    fn a_passthroughs_usage_line_says_it_takes_everything() {
        for command in COMMANDS.iter().filter(|c| c.passthrough()) {
            assert!(
                command.args.is_empty() || command.args.contains('…'),
                "`{} {}` reads as a fixed argument list and is not one",
                command.name,
                command.args
            );
        }
    }

    #[test]
    fn exit_codes_separate_the_two_a_script_can_act_on() {
        assert_eq!(exit_code(&Error::no_workspace()), 3);
        assert_eq!(
            exit_code(&Error::new(
                Code::EngineUnreachable,
                "docker is not running"
            )),
            4
        );
        assert_eq!(exit_code(&Error::new(Code::InvalidInput, "bad")), 2);
        assert_eq!(exit_code(&Error::new(Code::BuildFailed, "boom")), 1);
    }

    #[test]
    fn help_lists_every_command_and_both_headings() {
        let text = help(&Style::plain());
        for command in COMMANDS {
            assert!(
                text.contains(command.name),
                "{} is not in --help",
                command.name
            );
        }
        assert!(text.contains("Reads"));
        assert!(text.contains("Changes the stack"));
    }

    /// A writing command's own help has to say so — this is the only warning
    /// somebody gets before `stackvo down` takes the stack away.
    #[test]
    fn a_writing_commands_help_says_it_writes() {
        let text = command_help(find("down").unwrap(), &Style::plain());
        assert!(text.contains("changes the stack"), "{text}");
        assert!(
            text.contains("compose_down"),
            "the contract command is named"
        );

        let read = command_help(find("projects").unwrap(), &Style::plain());
        assert!(!read.contains("changes the stack"));
    }

    #[test]
    fn colour_is_off_when_it_is_turned_off() {
        let plain = Style::plain();
        assert_eq!(plain.ok("up"), "up");
        assert!(!help(&plain).contains('\u{1b}'));
    }

    /// Padding counts printed columns, not bytes — otherwise a coloured or
    /// non-ASCII cell pushes every row after it out of line.
    #[test]
    fn columns_align_through_colour_and_multibyte_names() {
        let style = Style::plain();
        assert_eq!(visible_width("\u{1b}[32mup\u{1b}[0m"), 2);
        assert_eq!(visible_width("çiçek"), 5);

        // One multi-byte cell and one wearing colour: byte-counting fails the
        // first, and counting escape sequences as printed fails the second.
        let rows = vec![
            vec!["çiçek".to_string(), "a".to_string()],
            vec!["\u{1b}[32mup\u{1b}[0m".to_string(), "b".to_string()],
        ];
        let text = table(&["name", "v"], &rows, &style);

        // Where the second column starts, in printed columns, on every line.
        let starts: Vec<usize> = text
            .lines()
            .map(|line| {
                let chars: Vec<char> = strip_escapes(line).chars().collect();
                let gap = chars.iter().position(|c| *c == ' ').expect("two columns");
                gap + chars[gap..]
                    .iter()
                    .position(|c| *c != ' ')
                    .expect("a second column")
            })
            .collect();

        assert!(
            starts.windows(2).all(|pair| pair[0] == pair[1]),
            "the second column starts at {starts:?}:\n{text}"
        );
    }

    fn strip_escapes(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }

    #[test]
    fn the_projects_table_renders_from_the_json_it_would_have_printed() {
        let value = json!([
            { "name": "shop", "domain": "shop.loc", "runtime": "php-8.3",
              "running": true, "manifestValid": true, "generatedStale": false,
              "domainConfigured": true },
            { "name": "blog", "domain": "blog.loc", "runtime": "node-22",
              "running": false, "manifestValid": false, "generatedStale": true,
              "domainConfigured": true },
        ]);

        let text = render(Action::Projects, &value, &Style::plain());
        assert!(text.contains("shop.loc"));
        assert!(text.contains("up"));
        assert!(text.contains("down"));
        assert!(text.contains("manifest"), "an invalid manifest has to show");
        assert!(text.contains("stale"));
    }

    #[test]
    fn an_empty_list_says_so_rather_than_printing_a_bare_header() {
        let text = render(Action::Projects, &json!([]), &Style::plain());
        assert_eq!(text, "no projects\n");
    }

    /// The doctor prints findings, not a clean bill of health item by item.
    #[test]
    fn the_doctor_reports_only_what_is_wrong() {
        let clean = json!({
            "preflight": { "requirements": [ { "id": "docker", "state": "ok" } ] },
            "core": [ { "service": "traefik", "state": "ok" } ],
            "ports": [ { "port": 80, "state": "ok" } ],
            "hostsMissing": [],
            "generated": { "state": "ok" },
            "dns": null,
            "extensions": [],
        });
        assert_eq!(
            render(Action::Doctor, &clean, &Style::plain()),
            "ok nothing to report\n"
        );

        let busy = json!({
            "preflight": { "requirements": [] },
            "core": [],
            "ports": [ { "port": 80, "state": "fail", "requiredBy": "traefik",
                         "process": "nginx", "pid": 4242 } ],
            "hostsMissing": ["shop.loc"],
            "generated": { "state": "warn", "detail": "older than stackvo.json" },
            "dns": null,
            "extensions": [],
        });
        let text = render(Action::Doctor, &busy, &Style::plain());
        assert!(text.contains("nginx (4242)"), "{text}");
        assert!(text.contains("shop.loc"));
        assert!(text.contains("stackvo generate"), "the repair is named");
    }

    /// A running-but-unhealthy service must not read as a healthy one — the
    /// distinction `Service::health` was added for.
    #[test]
    fn an_unhealthy_service_is_not_shown_as_up() {
        let value = json!([
            { "id": "mysql", "running": true, "enabled": true, "health": "unhealthy" },
            { "id": "redis", "running": true, "enabled": true, "health": "healthy" },
        ]);
        let text = render(Action::Services, &value, &Style::plain());
        assert!(text.contains("sick"), "{text}");
        assert!(text.contains("up"));
    }

    #[test]
    fn the_certificate_names_what_it_does_not_cover() {
        let value = json!({
            "daysRemaining": 300, "expired": false, "caTrusted": true,
            "covered": ["shop.loc"], "missing": ["blog.loc"], "rejected": [],
        });
        let text = render(Action::Certs, &value, &Style::plain());
        assert!(text.contains("blog.loc"));
        assert!(text.contains("certs-renew"), "the repair is named: {text}");
    }

    /// The pane and this print the same states from the same status, so a
    /// person reading one and then the other is not told two things.
    #[test]
    fn the_mcp_table_distinguishes_registered_from_stale() {
        let value = json!({
            "binary": "/opt/stackvo-mcp",
            "clients": [
                { "id": "claude-code", "label": "Claude Code", "path": "/h/.claude.json",
                  "present": true, "parseable": true, "command": "/opt/stackvo-mcp", "current": true },
                { "id": "cursor", "label": "Cursor", "path": "/h/.cursor/mcp.json",
                  "present": true, "parseable": true, "command": "/old/stackvo-mcp", "current": false },
                { "id": "zed", "label": "Zed", "path": "/h/.config/zed/settings.json",
                  "present": false, "parseable": true, "command": null, "current": false },
            ],
        });

        let text = render(Action::Mcp, &value, &Style::plain());
        assert!(text.contains("registered"));
        assert!(text.contains("stale"));
        assert!(text.contains("not installed"));
        assert!(text.contains("mcp-install"), "the next step is named");
    }

    /// With no binary the table still prints — "Cursor is here and nothing is
    /// registered" is an answer — and the build command is given.
    #[test]
    fn a_missing_server_binary_says_how_to_get_one() {
        let value = json!({ "binary": null, "clients": [] });
        let text = render(Action::Mcp, &value, &Style::plain());
        assert!(text.contains("not on this machine"), "{text}");
        assert!(text.contains("--bin stackvo-mcp"), "{text}");
    }

    // ---------------------------------------------- the shell commands (A-3)

    fn target() -> Target {
        Target {
            name: "shop".into(),
            container: "stackvo-shop".into(),
            running: true,
            mount: Some("/var/www/html"),
            workdir: Some("/var/www/html".into()),
        }
    }

    #[test]
    fn a_prefix_command_puts_its_own_words_first() {
        let args = ["migrate".to_string(), "--force".to_string()];
        let argv = container_argv(find("artisan").unwrap(), &args).unwrap();
        assert_eq!(argv, ["php", "artisan", "migrate", "--force"]);

        let argv = container_argv(find("php").unwrap(), &["-v".to_string()]).unwrap();
        assert_eq!(argv, ["php", "-v"]);
    }

    #[test]
    fn exec_takes_the_program_from_the_caller_and_needs_one() {
        let args = ["ls".to_string(), "-la".to_string()];
        let argv = container_argv(find("exec").unwrap(), &args).unwrap();
        assert_eq!(argv, ["ls", "-la"]);

        let error = container_argv(find("exec").unwrap(), &[]).unwrap_err();
        assert_eq!(error.code, Code::InvalidInput);
    }

    /// Several images in this catalogue are Alpine-based and ship no bash. A
    /// hardcoded `bash` is the bug `pty.rs` already had and fixed.
    #[test]
    fn the_shell_falls_back_when_there_is_no_bash() {
        let argv = container_argv(find("shell").unwrap(), &[]).unwrap();
        let line = argv.join(" ");
        assert!(line.contains("exec bash"), "{line}");
        assert!(line.contains("|| exec sh"), "{line}");
    }

    /// A TTY is asked for only when there is one, because `docker exec -t`
    /// without a terminal fails outright — which would break every use of this
    /// in a pipeline or a CI job.
    #[test]
    fn a_tty_is_requested_only_when_asked_for() {
        let with = exec_argv(&target(), &["php".to_string()], true);
        assert!(with.contains(&"-t".to_string()));
        assert!(with.contains(&"-i".to_string()));

        let without = exec_argv(&target(), &["php".to_string()], false);
        assert!(!without.contains(&"-t".to_string()));
        assert!(
            without.contains(&"-i".to_string()),
            "stdin is always connected — `echo … | stackvo php` has to work"
        );
    }

    #[test]
    fn the_working_directory_is_only_set_where_there_is_a_mount() {
        let mut unmounted = target();
        unmounted.mount = None;
        unmounted.workdir = None;

        let argv = exec_argv(&unmounted, &["npm".to_string()], false);
        assert!(
            !argv.contains(&"-w".to_string()),
            "a project with no source mount has no directory to map onto: {argv:?}"
        );
    }

    /// Standing in a subdirectory runs there, which is the point of the
    /// feature: `stackvo artisan` from `app/Http` behaves like `artisan` would.
    #[test]
    fn a_subdirectory_maps_onto_the_same_place_inside() {
        let project = Path::new("/Users/x/www/shop");

        assert_eq!(
            workdir_for(Some("/var/www/html"), project, project),
            Some("/var/www/html".to_string())
        );
        assert_eq!(
            workdir_for(
                Some("/var/www/html"),
                project,
                Path::new("/Users/x/www/shop/app/Http")
            ),
            Some("/var/www/html/app/Http".to_string())
        );
        // Outside the project entirely — nothing to map.
        assert_eq!(
            workdir_for(Some("/var/www/html"), project, Path::new("/tmp")),
            None
        );
        // And no mount means no answer at all, whatever the directory.
        assert_eq!(workdir_for(None, project, project), None);
    }

    /// The nested case, which is the one that would be silently wrong: a
    /// worktree inside a project directory must win over the project.
    #[test]
    fn the_deepest_enclosing_project_wins() {
        let paths = ["/w/shop", "/w/shop/.worktrees/feature-x", "/w/blog"];

        assert_eq!(enclosing(&paths, Path::new("/w/shop/app")), Some("/w/shop"));
        assert_eq!(
            enclosing(&paths, Path::new("/w/shop/.worktrees/feature-x/app")),
            Some("/w/shop/.worktrees/feature-x"),
            "the parent branch's container would have been the wrong answer"
        );
        assert_eq!(enclosing(&paths, Path::new("/elsewhere")), None);
    }

    /// A sibling whose name merely starts with the same letters is not a
    /// parent. `starts_with` on a `Path` compares components, and this is the
    /// test that keeps it from being rewritten as a string comparison.
    #[test]
    fn a_similarly_named_sibling_is_not_a_match() {
        let paths = ["/w/shop"];
        assert_eq!(enclosing(&paths, Path::new("/w/shop-staging")), None);
    }

    #[test]
    fn narration_is_silent_when_asked_to_be() {
        // Nothing to assert but that it does not panic on either payload shape;
        // the sink's contract is that it never fails an operation.
        let quiet = Narrate::new(true);
        quiet.event("cli:progress", json!({ "line": "step" }));
        let loud = Narrate::new(false);
        loud.event("project:error", json!({ "error": "no such container" }));
    }
}
