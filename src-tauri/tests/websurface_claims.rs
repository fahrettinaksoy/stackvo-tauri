//! `websurface::REACHES_THE_KEYSTORE` is complete, and stays complete.
//!
//! The module states the rule: a command may be served over the loopback
//! surface if it is a read **and** it does not hand back a stored secret. The
//! first half is derived from the contract every time it is asked. The second
//! half is a list of four names, and a list of four names is exactly the shape
//! of thing that is right on the day it is written and wrong six months later.
//!
//! §7 already carries one scar from this: "the four commands with no meaning on
//! the web" was hand-counted, and could only ever go wrong by the codebase
//! growing. That one is derived now. This is the same failure waiting in a
//! worse place — a fifth `secrets::read` in a query body would join the served
//! set silently, and the surface would still be honestly described as
//! read-only while serving a password.
//!
//! ## How the derivation works
//!
//! Every `#[tauri::command]` in `commands.rs`, with its body, plus one hop
//! through helpers defined in the same file. One hop and not zero because
//! `secret_value` exists — the readers call it rather than `secrets::read`
//! directly, and a scan that only looked at command bodies would find nothing
//! and cheerfully report the list complete.
//!
//! The needle is the keystore read itself rather than a name pattern. A
//! `contains("reveal")` heuristic would pass today and miss the first command
//! called something else, which is the only kind of miss that matters here.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Comments out, so a sentence explaining the rule is not read as the rule.
fn without_comments(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One function: which module it lives in, its name, whether it is a command,
/// and its body.
///
/// Bodies are cut at the next top-level `fn`, which is coarse and is enough:
/// the question is which names appear inside which function, and a body that
/// swallowed a few lines of the next one makes this *more* eager to report a
/// reader, never less.
struct Func {
    module: String,
    name: String,
    is_command: bool,
    body: String,
}

impl Func {
    fn key(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }
}

fn functions(module: &str, source: &str) -> Vec<Func> {
    let text = without_comments(source);
    let mut starts: Vec<usize> = Vec::new();
    for opening in ["\nfn ", "\npub fn ", "\npub async fn ", "\nasync fn "] {
        starts.extend(text.match_indices(opening).map(|(i, _)| i));
    }
    starts.sort_unstable();
    starts.dedup();

    let mut out = Vec::new();
    for (position, start) in starts.iter().enumerate() {
        let end = starts.get(position + 1).copied().unwrap_or(text.len());
        let block = &text[*start..end];

        let Some(after) = block.split("fn ").nth(1) else {
            continue;
        };
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }

        let head = &text[start.saturating_sub(400)..*start];
        let is_command = head
            .rsplit("\n\n")
            .next()
            .unwrap_or(head)
            .contains("#[tauri::command]");

        out.push(Func {
            module: module.to_string(),
            name,
            is_command,
            body: block.to_string(),
        });
    }
    out
}

/// Names that mean "a value came out of the OS keystore".
///
/// `secrets::read` is the keystore itself; `secret_value` is the helper the
/// revealing commands in `commands.rs` call, which consults the keystore and
/// falls back to the manifest default — either way what it returns is the
/// password the container is running with.
const KEYSTORE_READS: [&str; 2] = ["secrets::read", "secret_value("];

/// The function that IS the keystore, named by its own key.
///
/// Inside `secrets.rs` the call is written `read(...)`, not `secrets::read(...)`,
/// so the textual needle above never matches its definition. Seeding it by key
/// is what lets the closure walk *out* of that module.
const KEYSTORE_ITSELF: &str = "secrets::read";

fn declared() -> BTreeSet<String> {
    // Read out of the module's own source rather than linked against, so this
    // test fails with a diff of names rather than with a compile error in a
    // file nobody was editing.
    let source = without_comments(&read("src-tauri/src/websurface.rs"));
    let start = source
        .find("pub const REACHES_THE_KEYSTORE")
        .expect("websurface.rs declares REACHES_THE_KEYSTORE");
    // `= [`, not the first `[` — that one belongs to `[&str; N]`, and slicing
    // from it swallowed the type and every comment in the array. The list came
    // back empty and the test reported nothing denied.
    // `+ 3` lands past the `[`. At `+ 2` the first split piece was
    // `["instance_reveal"`, whose `strip_prefix('"')` fails — so the array's
    // FIRST entry was dropped, every time, silently. The test then reported
    // that entry as an undeclared secret-reader, which read like a finding.
    let open = source[start..]
        .find("= [")
        .expect("the constant is an array")
        + start
        + 3;
    let close = source[open..].find("];").expect("the array is closed") + open;

    source[open..close]
        .split(',')
        .filter_map(|piece| {
            let t = piece.trim();
            let t = t.strip_prefix('"')?;
            t.strip_suffix('"').map(String::from)
        })
        .collect()
}

/// Which commands reach a keystore read, at any depth, across the whole crate.
///
/// ## Why the graph is module-qualified
///
/// The first version keyed functions by bare name and merged across files. It
/// reported **101 of 112 queries** as secret-readers: `read`, `load` and `of`
/// are defined in a dozen modules, so one keystore-reading `read` made every
/// caller of any `read(` a reader and the closure ate the crate. Over-denial is
/// the safe direction and a surface that serves eleven commands is still not a
/// surface — "safe" is not the same as "useful", and a gate that refuses
/// everything answers nothing.
///
/// So an edge exists when a body names `other::fn(` — or plain `fn(` for a
/// function in the same module, which is how Rust is written. Still textual,
/// still approximate, and now approximate at the resolution the language has.
///
/// ## Why a fixpoint rather than one hop
///
/// `service_reveal` is three lines and delegates to `instance_reveal`, which
/// delegates to the helper that opens the keystore. One hop found the second
/// and walked past the first — the thing with `reveal` in its name.
/// `service_connection` is two modules away and takes a caller-controlled
/// `reveal: bool`, so no fixed depth would have covered both.
fn derived() -> BTreeSet<String> {
    let src = repo_root().join("src-tauri/src");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&src)
        .expect("src-tauri/src is readable")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();

    let mut all: Vec<Func> = Vec::new();
    for path in &files {
        let module = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("a source file has a stem")
            .to_string();
        let source = std::fs::read_to_string(path).expect("a source file is readable");
        all.extend(functions(&module, &source));
    }
    assert!(
        all.len() > 500,
        "only {} function(s) were found across the crate — the scan stopped \
         matching, and an empty graph reaches nothing",
        all.len()
    );

    let mut reaching: BTreeSet<String> = all
        .iter()
        .filter(|f| KEYSTORE_READS.iter().any(|needle| f.body.contains(needle)))
        .map(Func::key)
        .collect();
    reaching.insert(KEYSTORE_ITSELF.to_string());

    loop {
        let mut grew = false;
        for func in &all {
            let key = func.key();
            if reaching.contains(&key) {
                continue;
            }
            let calls_one = reaching.iter().any(|target| {
                if target == &key {
                    return false;
                }
                let (module, name) = target.split_once("::").expect("every key is module::name");
                // A bare name is only followed inside its own module: a short
                // helper matched as a substring everywhere is what collapsed
                // the first version of this graph.
                if module == func.module
                    && name.len() >= 4
                    && func.body.contains(&format!("{name}("))
                {
                    return true;
                }
                func.body.contains(&format!("{module}::{name}("))
            });
            if calls_one {
                reaching.insert(key);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }

    all.into_iter()
        .filter(|f| f.is_command && reaching.contains(&f.key()))
        .map(|f| f.name)
        .collect()
}

/// The list, against the code.
#[test]
fn every_query_that_reaches_the_keystore_is_named_as_one() {
    let contract: serde_json::Value =
        serde_json::from_str(&read("contracts/ipc.json")).expect("contracts/ipc.json parses");

    let is_query = |name: &str| {
        contract["commands"][name]
            .get("kind")
            .and_then(|k| k.as_str())
            == Some("query")
    };

    // Only queries matter here. A mutation that reads a secret is already
    // refused by the first rule, and naming it in REACHES_THE_KEYSTORE would say the
    // list is about something it is not.
    let found: BTreeSet<String> = derived().into_iter().filter(|n| is_query(n)).collect();
    let named = declared();

    assert!(
        !found.is_empty(),
        "the scan found no command reaching the keystore at all, which means it \
         stopped matching — `secret_value` and `secrets::read` are both still in \
         commands.rs, so an empty result is the scanner failing, not the code \
         improving"
    );

    let missing: Vec<&String> = found.difference(&named).collect();
    assert!(
        missing.is_empty(),
        "{} query command(s) hand back a stored secret and are NOT in \
         `websurface::REACHES_THE_KEYSTORE`:\n{}\n\nEach one would be served over the \
         loopback surface, which is described as reads-only and would be \
         telling the truth while returning a password. Add it to the constant, \
         or stop the command reading the keystore.",
        missing.len(),
        missing
            .iter()
            .map(|n| format!("  {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let stale: Vec<&String> = named.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "{} name(s) in `websurface::REACHES_THE_KEYSTORE` no longer reach the \
         keystore:\n{}\n\nA denial nobody needs is a denial nobody re-reads — \
         remove it, or say in the constant why it stays.",
        stale.len(),
        stale
            .iter()
            .map(|n| format!("  {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The surface is a strict subset of the reads, and a large one.
///
/// Two ways this decision could quietly become something else: the served set
/// collapsing to nothing (a surface that answers no question is not the thing
/// §5 agreed to), or growing to include everything (the secret rule doing
/// nothing). Both are numbers, so both can be held.
#[test]
fn the_served_set_is_every_read_except_the_ones_that_carry_a_secret() {
    let contract: serde_json::Value =
        serde_json::from_str(&read("contracts/ipc.json")).expect("contracts/ipc.json parses");
    let commands = contract["commands"]
        .as_object()
        .expect("the contract has commands");

    let queries: BTreeSet<String> = commands
        .iter()
        .filter(|(_, v)| v.get("kind").and_then(|k| k.as_str()) == Some("query"))
        .map(|(k, _)| k.clone())
        .collect();
    let denied = declared();

    let served = queries.len() - denied.iter().filter(|d| queries.contains(*d)).count();

    assert_eq!(
        served,
        queries.len() - denied.len(),
        "a name in REACHES_THE_KEYSTORE is not a query, so the arithmetic above is \
         describing a different set than the one that gets served"
    );
    assert!(
        served > 50,
        "only {served} command(s) would be served. A loopback surface that \
         answers almost nothing is not what §5 agreed to — if the denial list \
         has grown this far, the decision needs revisiting rather than the \
         number."
    );
    assert!(
        !denied.is_empty(),
        "nothing is denied, so `exposable` is `kind == query` with extra steps \
         — and `instance_reveal` is a query"
    );
}

/// Nothing that writes, runs or elevates can reach the surface.
///
/// The first rule, checked against the contract rather than against the module,
/// so a change to `exposable` that widened it would be caught here even if the
/// module's own tests were updated to match.
#[test]
fn no_kind_other_than_query_is_ever_exposable() {
    let contract: serde_json::Value =
        serde_json::from_str(&read("contracts/ipc.json")).expect("contracts/ipc.json parses");

    for (name, entry) in contract["commands"]
        .as_object()
        .expect("the contract has commands")
    {
        let kind = entry.get("kind").and_then(|k| k.as_str()).unwrap_or("?");
        if kind == "query" {
            continue;
        }
        assert!(
            !stackvo_desktop_lib::websurface::exposable(name),
            "`{name}` is `kind: {kind}` and would be served. Only a query may \
             be — an operation spawns, a mutation writes, and `stream` opens a \
             channel this surface has no way to carry."
        );
    }
}
