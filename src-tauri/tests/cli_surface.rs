//! The CLI against `contracts/ipc.json`, held the way `mcp.rs` is held.
//!
//! A-1 sat unbuilt because a CLI is a **third surface** — the desktop reaches
//! the core one way, an assistant another, and a third consumer is a third
//! thing that can drift from the contract while every existing test passes.
//! `docs/durum.md` §5 asked whether that was acceptable. It is, on the
//! condition this file enforces: every command names the contract command it
//! implements, and the pair is checked.
//!
//! What each check buys:
//!
//! * **A command naming nothing.** A command renamed in `ipc.json` leaves
//!   `stackvo doctor` describing something that no longer exists, and without
//!   this the first person to find out is the one who ran it.
//! * **A read that writes.** `COMMANDS` splits into "Reads" and "Changes the
//!   stack" in `--help`, and somebody reads that list before typing into a
//!   machine they care about. A command under the wrong heading is worse than
//!   no heading.
//! * **A write that reads.** The converse, and the reason it matters is the
//!   audit trail: writes are recorded, reads are not, so a mutation filed
//!   under "reads" is one that happens without a record.
//!
//! Deliberately not checked here: that the human rendering says the right
//! thing. That is settled in `cli.rs`'s own tests against fixture values,
//! because it is a claim about text rather than about the surface.

use stackvo_desktop_lib::cli::{self, COMMANDS};

fn contract() -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .join("contracts/ipc.json");
    let text = std::fs::read_to_string(&path).expect("contracts/ipc.json is readable");
    serde_json::from_str(&text).expect("the contract is valid JSON")
}

#[test]
fn every_command_names_a_real_contract_command() {
    let ipc = contract();
    let commands = ipc["commands"]
        .as_object()
        .expect("the contract declares commands");

    // `contracts()` rather than `contract()`: a screen names several and every
    // one is checked. A shell command names none and is covered by the
    // assertions further down.
    for command in COMMANDS {
        for contract in command.contracts() {
            assert!(
                commands.contains_key(contract),
                "`stackvo {}` names `{contract}`, which is not in contracts/ipc.json",
                command.name,
            );
        }
    }
}

#[test]
fn reading_commands_are_backed_by_reading_contract_commands() {
    let ipc = contract();

    for command in COMMANDS.iter().filter(|c| !c.writes) {
        assert!(
            !command.contracts().is_empty(),
            "`stackvo {}` is listed under Reads and names no contract command",
            command.name
        );
        for contract in command.contracts() {
            let kind = ipc["commands"][contract]["kind"]
                .as_str()
                .unwrap_or("unknown");
            assert!(
                matches!(kind, "query" | "stream"),
                "`stackvo {}` is listed under Reads but `{contract}` is declared `{kind}`",
                command.name,
            );
        }
    }
}

#[test]
fn writing_commands_are_backed_by_mutating_contract_commands() {
    let ipc = contract();

    // A screen is judged by whether it can change anything, not by whether
    // everything it drives does: `tui` lists as well as toggles, and demanding
    // that every name be a mutation would push the list out of the
    // declaration — which is the one place a reader sees what it touches.
    for command in COMMANDS.iter().filter(|c| c.writes) {
        let names = command.contracts();
        if names.is_empty() {
            continue; // a shell command, checked below
        }
        let kinds: Vec<&str> = names
            .iter()
            .map(|c| ipc["commands"][c]["kind"].as_str().unwrap_or("unknown"))
            .collect();
        assert!(
            kinds.iter().any(|k| matches!(*k, "mutation" | "operation")),
            "`stackvo {}` is listed as changing the stack but drives only {kinds:?}",
            command.name,
        );
    }
}

/// The gate needs something to gate.
///
/// Both halves asserted because a table that drifted to all-reads or all-writes
/// would pass the two tests above while making the split in `--help` — the one
/// thing a person reads before typing — meaningless.
#[test]
fn the_surface_has_both_kinds() {
    assert!(COMMANDS.iter().any(|c| c.writes));
    assert!(COMMANDS.iter().any(|c| !c.writes));
}

/// The MCP server and the CLI expose the same core; where they name the same
/// contract command, they must agree about whether it writes.
///
/// Not a requirement that they expose the same set — they do not, and should
/// not: `logs --follow` is a CLI answer and a useless one over JSON-RPC. What
/// would be indefensible is `compose_down` being a write in one surface and a
/// read in the other, because then one of the two is lying to somebody.
#[test]
fn the_two_surfaces_agree_about_what_writes() {
    for command in COMMANDS {
        let Some(contract) = command.contract() else {
            continue;
        };
        let Some(tool) = stackvo_desktop_lib::mcp::TOOLS
            .iter()
            .find(|t| t.command == contract)
        else {
            continue;
        };

        assert_eq!(
            command.writes, tool.writes,
            "`stackvo {}` and `{}` both drive `{contract}` and disagree about whether it writes",
            command.name, tool.name
        );
    }
}

// -------------------------------------------------- the shell commands (A-3)
//
// `Backing::HostShell` is the one exception to "every command names a contract
// command", and an exception nobody checks is a hole. These four assertions are
// what keeps it a boundary rather than an escape hatch: a future command that
// wanted to skip the contract would have to be a container passthrough that
// writes, which is exactly the class the exception was opened for.

/// A screen names more than one command, or it is not a screen.
///
/// `Backing::Surface` exists because "which command does `stackvo tui`
/// implement" has no single honest answer. One that named a single command
/// should have been a `Contract`, and the looser rule above — any one of the
/// names may be the mutation — would then be a hole rather than a shape.
#[test]
fn a_screen_drives_more_than_one_command() {
    for command in COMMANDS {
        if let stackvo_desktop_lib::cli::Backing::Surface(names) = command.backing {
            assert!(
                names.len() > 1,
                "`stackvo {}` is declared a screen over one command",
                command.name
            );
        }
    }
}

/// A shell command runs **in a container**, never on the host.
///
/// This is the whole justification for the exception. `stackvo php` is defended
/// as "less dangerous than the `docker exec` you would otherwise type"; the
/// moment one of these spawned a host process that sentence stops being true,
/// and nothing else in the tree would notice.
#[test]
fn every_shell_command_runs_inside_the_container() {
    let target = cli::Target {
        name: "shop".into(),
        container: "stackvo-shop".into(),
        running: true,
        mount: Some("/var/www/html"),
        workdir: Some("/var/www/html".into()),
    };

    for command in COMMANDS.iter().filter(|c| c.passthrough()) {
        let argv = cli::exec_argv(&target, &["placeholder".to_string()], false);

        assert_eq!(
            argv.first().map(String::as_str),
            Some("exec"),
            "`stackvo {}` does not go through `docker exec`",
            command.name
        );
        assert!(
            argv.contains(&"stackvo-shop".to_string()),
            "`stackvo {}` names no container",
            command.name
        );
    }
}

/// Every shell command is classified as writing.
///
/// Not because every call changes something — `php -v` does not — but because
/// the surface cannot tell, and `--help`'s headings have to be true of every
/// call under them. It is also what keeps the read-only test above able to
/// `expect()` a contract on everything it looks at.
#[test]
fn shell_commands_are_classified_as_writing() {
    for command in COMMANDS.iter().filter(|c| c.passthrough()) {
        assert!(
            command.writes,
            "`stackvo {}` runs an arbitrary program and is not marked as writing",
            command.name
        );
        assert!(command.contract().is_none());
    }
}

/// The passthrough rule, at the parser.
///
/// `artisan migrate --force` is the most common artisan call there is, and a
/// parser that read `--force` as its own would refuse it. Asserted on the
/// parsed arguments rather than on the parser's shape, because what matters is
/// that the flag *arrives*.
#[test]
fn a_shell_commands_own_flags_survive_the_parser() {
    let argv: Vec<String> = "artisan migrate --force --seed"
        .split_whitespace()
        .map(str::to_string)
        .collect();

    let parsed = cli::parse(&argv).expect("a passthrough line parses");
    assert_eq!(parsed.command, Some("artisan"));
    assert_eq!(parsed.args, ["migrate", "--force", "--seed"]);

    // And StackVo's own flags still work — before the command, which is where
    // the parser stops looking.
    let argv: Vec<String> = "--project shop artisan migrate --force"
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let parsed = cli::parse(&argv).expect("globals before the command parse");
    assert_eq!(parsed.value("project"), Some("shop"));
    assert_eq!(parsed.args, ["migrate", "--force"]);
}

/// The prefix in the table is what actually runs.
///
/// `--help` prints `php artisan` for `stackvo artisan`; this is the assertion
/// that the two cannot drift, which is the same promise `quickcmd::Spec` makes
/// with its `display` field.
#[test]
fn the_advertised_prefix_is_the_argv_that_runs() {
    let target = cli::Target {
        name: "shop".into(),
        container: "stackvo-shop".into(),
        running: true,
        mount: Some("/var/www/html"),
        workdir: None,
    };

    for command in COMMANDS.iter().filter(|c| !c.prefix.is_empty()) {
        let argv = cli::container_argv(command, &["migrate".to_string()])
            .unwrap_or_else(|e| panic!("`stackvo {}`: {}", command.name, e.message));

        assert!(
            argv.starts_with(
                &command
                    .prefix
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            ),
            "`stackvo {}` advertises `{:?}` and runs `{argv:?}`",
            command.name,
            command.prefix
        );
        assert_eq!(argv.last().map(String::as_str), Some("migrate"));

        // And it survives the trip through `docker exec` intact.
        let full = cli::exec_argv(&target, &argv, false);
        assert!(full.ends_with(&argv), "{full:?} lost the command");
    }
}

/// A parse of every command's own spelled usage.
///
/// `--help` prints `xdebug <project> on|off`; this checks that a line shaped
/// like it actually parses, so the usage cannot describe a command line the
/// parser refuses.
#[test]
fn every_commands_own_usage_parses() {
    for command in COMMANDS.iter().filter(|c| !c.passthrough()) {
        let mut argv = vec![command.name.to_string()];
        // One placeholder per spelled argument. The value does not matter to
        // the parser, only the count does.
        for _ in 0..command.arity.1 {
            argv.push("placeholder".to_string());
        }

        let parsed = cli::parse(&argv)
            .unwrap_or_else(|e| panic!("`stackvo {}` does not parse: {}", command.name, e.message));

        assert_eq!(parsed.command, Some(command.name));
        assert_eq!(parsed.args.len(), command.arity.1);
    }
}

/// Every flag a command declares is one the parser will take from it.
///
/// A `Flag` in the table that the parser rejects would be advertised in
/// `--help` and refused when typed — the exact failure this whole file is
/// about, one layer down.
#[test]
fn every_declared_flag_is_accepted_by_the_command_that_declares_it() {
    for command in COMMANDS.iter().filter(|c| !c.passthrough()) {
        for flag in command.flags {
            let mut argv = vec![command.name.to_string()];
            for _ in 0..command.arity.1 {
                argv.push("placeholder".to_string());
            }
            argv.push(format!("--{}", flag.long));
            if flag.value.is_some() {
                argv.push("1".to_string());
            }

            let parsed = cli::parse(&argv).unwrap_or_else(|e| {
                panic!(
                    "`stackvo {} --{}` is advertised and refused: {}",
                    command.name, flag.long, e.message
                )
            });
            assert!(
                parsed.on(flag.long),
                "--{} parsed but did not arrive",
                flag.long
            );
        }
    }
}
