//! An MCP server over the same core the app drives.
//!
//! Five of the eight competitors ship one — Herd, Lerd, EnvKit, FlyEnv and
//! ServBay — and in 2026 it is the price of entry rather than a differentiator.
//! What is different here is where the tool list comes from.
//!
//! ## The contract is the authority
//!
//! `contracts/ipc.json` already records every command, its arguments, its
//! return type and — the part that matters — its `kind`: `query`, `mutation`,
//! `operation` or `stream`. The table below names, for each tool, the command
//! it implements, and a test cross-checks the two: a tool naming a command that
//! does not exist fails, and a tool claiming to be read-only while its command
//! is declared a mutation fails. That is the same discipline suites E and F
//! apply to the IPC surface, for the same reason — nothing enforces any of it
//! at compile time.
//!
//! Generating the tools outright was the obvious move and is the wrong one:
//! dispatch cannot be generated, so a generated list would advertise tools that
//! then fail when called. A checked hand-written table advertises exactly what
//! it can do.
//!
//! ## What is exposed, and what is not
//!
//! Reads, freely. They are the answer to the question an assistant is usually
//! asked — "why is shop.loc not loading?" — which needs preflight, the hosts
//! file, the certificate's SAN list and a container's last hundred log lines,
//! and needs to change nothing to find out.
//!
//! Writes only behind `--allow-writes`, and only the few that are reversible
//! and cheap. Not a blanket ban on principle: **most mutations are not reachable
//! from here at all.** Thirty-four commands take an `AppHandle` because they
//! report progress through Tauri's event system, and a stdio subprocess has no
//! app to emit into. Decoupling that is a refactor of its own, and pretending
//! otherwise by shelling out to a second copy of the stack would be worse than
//! saying so.

use crate::error::Result;
use serde_json::{json, Value};

/// The protocol revision this server implements.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// One exposed tool, and the contract command it stands for.
pub struct Tool {
    pub name: &'static str,
    /// The `contracts/ipc.json` command this implements. Cross-checked by test.
    pub command: &'static str,
    pub description: &'static str,
    /// True when calling it changes something on disk or in the stack.
    pub writes: bool,
    /// JSON Schema for `arguments`.
    pub schema: fn() -> Value,
}

fn no_args() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn project_arg() -> Value {
    json!({
        "type": "object",
        "properties": { "name": { "type": "string", "description": "Project directory name." } },
        "required": ["name"],
        "additionalProperties": false
    })
}

fn logs_args() -> Value {
    json!({
        "type": "object",
        "properties": {
            "container": { "type": "string", "description": "Project or service id, without the stackvo- prefix." },
            "tail": { "type": "integer", "description": "How many lines from the end. Default 100.", "minimum": 1, "maximum": 2000 }
        },
        "required": ["container"],
        "additionalProperties": false
    })
}

fn limit_arg() -> Value {
    json!({
        "type": "object",
        "properties": { "limit": { "type": "integer", "minimum": 1, "maximum": 200 } },
        "additionalProperties": false
    })
}

fn stack_up_args() -> Value {
    json!({
        "type": "object",
        "properties": {
            "mode": {
                "type": "string",
                "enum": ["minimal", "services", "projects", "all"],
                "description": "Which profiles to bring up. Default minimal (core only)."
            }
        },
        "additionalProperties": false
    })
}

fn generate_args() -> Value {
    json!({
        "type": "object",
        "properties": {
            "scope": {
                "type": "string",
                "enum": ["all", "projects", "services"],
                "description": "What to regenerate. Default all."
            }
        },
        "additionalProperties": false
    })
}

fn xdebug_set_args() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "enabled": { "type": "boolean" }
        },
        "required": ["name", "enabled"],
        "additionalProperties": false
    })
}

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "stackvo_overview",
        command: "preflight",
        description: "The state of the whole stack: the chosen checkout, the Docker engine, \
                      every startup requirement and how many projects and services are running. \
                      Start here — it is the one call that says whether anything else will work.",
        writes: false,
        schema: no_args,
    },
    Tool {
        name: "stackvo_doctor",
        command: "doctor",
        description: "The full diagnosis: every startup requirement, every host port the stack \
                      will claim and who holds it now (the stack itself, another container by \
                      name, or a host process by name and pid), project domains missing from \
                      the hosts file, whether the generated config is older than its inputs, \
                      and how much disk unused images and volumes hold. Use this to explain a \
                      failed start — especially \"address already in use\".",
        writes: false,
        schema: no_args,
    },
    Tool {
        name: "stackvo_projects",
        command: "projects_list",
        description: "Every managed project: domain, runtime, whether it is built and running, \
                      whether its domain has a hosts entry, and any contract violations in its \
                      stackvo.json.",
        writes: false,
        schema: no_args,
    },
    Tool {
        name: "stackvo_project",
        command: "project_get",
        description: "One project in full: its manifest, its container, its Xdebug state, its \
                      PHP limits as the running container actually reports them, and whether the \
                      HTTPS certificate covers its domain. Use this to explain why a specific \
                      site does not load — or why an upload fails at a limit the user believes \
                      they have already raised.",
        writes: false,
        schema: project_arg,
    },
    Tool {
        name: "stackvo_services",
        command: "services_list",
        description: "The shared infrastructure services — databases, caches, search, queues — \
                      with which are enabled, which are running and what they depend on.",
        writes: false,
        schema: no_args,
    },
    Tool {
        name: "stackvo_logs",
        command: "container_logs_open",
        description: "The last lines of a container's log. Reads to the end and returns; it does \
                      not follow.",
        writes: false,
        schema: logs_args,
    },
    Tool {
        name: "stackvo_log_files",
        command: "app_logs_all",
        description: "Every log file every project writes, newest first, with its size and when \
                      it last changed. The container log carries only stdout — an application's \
                      own exception is in one of these. Read the timestamps to find where the \
                      activity is before opening anything: the file that changed a minute ago is \
                      the one worth reading. Needs no engine, so it still answers with Docker \
                      stopped.",
        writes: false,
        schema: no_args,
    },
    Tool {
        name: "stackvo_certificates",
        command: "cert_status",
        description: "The HTTPS certificate: which domains it covers, which it does not, whether \
                      its CA is trusted and when it expires. A domain missing here is a browser \
                      warning the user cannot otherwise explain.",
        writes: false,
        schema: no_args,
    },
    Tool {
        name: "stackvo_databases",
        command: "db_targets",
        description: "The database services that can be dumped, their database names and whether \
                      they are running.",
        writes: false,
        schema: no_args,
    },
    Tool {
        name: "stackvo_mail",
        command: "mail_messages",
        description: "The mail catcher's inbox: what the applications under test have sent.",
        writes: false,
        schema: limit_arg,
    },
    Tool {
        name: "stackvo_xdebug_set",
        command: "xdebug_set",
        description: "Turn step debugging on or off for one project. The extension is compiled \
                      in, so the project needs regenerating and rebuilding afterwards — the reply \
                      says whether that is outstanding.",
        writes: true,
        schema: xdebug_set_args,
    },
    Tool {
        name: "stackvo_certificates_reissue",
        command: "cert_apply",
        description: "Reissue the HTTPS certificate for the domains the projects actually have, \
                      and trust the CA if nothing does yet.",
        writes: true,
        schema: no_args,
    },
    Tool {
        name: "stackvo_project_start",
        command: "project_start",
        description: "Start one project's container. Idempotent: starting a running project \
                      succeeds silently.",
        writes: true,
        schema: project_arg,
    },
    Tool {
        name: "stackvo_project_stop",
        command: "project_stop",
        description: "Stop one project's container. Idempotent, like start.",
        writes: true,
        schema: project_arg,
    },
    Tool {
        name: "stackvo_stack_up",
        command: "compose_up",
        description: "Bring the stack up with docker compose — builds missing images, so the \
                      first run can take minutes. Runs to completion and reports the outcome.",
        writes: true,
        schema: stack_up_args,
    },
    Tool {
        name: "stackvo_stack_down",
        command: "compose_down",
        description: "Bring the whole stack down: every profile, projects included.",
        writes: true,
        schema: no_args,
    },
    Tool {
        name: "stackvo_generate",
        command: "generate_run",
        description: "Re-run the generator: derive the compose files, Dockerfiles and configs \
                      from .env and the project manifests. The doctor's 'generated config is \
                      stale' finding is repaired by exactly this.",
        writes: true,
        schema: generate_args,
    },
];

/// The tools this run offers.
pub fn visible(allow_writes: bool) -> impl Iterator<Item = &'static Tool> {
    TOOLS.iter().filter(move |t| allow_writes || !t.writes)
}

// --------------------------------------------------------------- protocol
//
// JSON-RPC 2.0 over stdio, hand-rolled. The surface is three methods; an SDK
// would be a dependency and a version to track for less code than this.

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn err(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Tool output. MCP carries text; JSON is what an assistant can actually use.
fn text_result(id: Value, body: &Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(body).unwrap_or_else(|_| body.to_string());
    ok(
        id,
        json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
    )
}

pub fn tools_list(allow_writes: bool) -> Value {
    let tools: Vec<Value> = visible(allow_writes)
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": (tool.schema)(),
                // Advertised so a client can require confirmation without
                // knowing what any individual tool does.
                "annotations": { "readOnlyHint": !tool.writes, "destructiveHint": tool.writes },
            })
        })
        .collect();

    json!({ "tools": tools })
}

/// Handle one request. `None` for a notification, which takes no reply.
pub async fn handle(request: &Value, allow_writes: bool) -> Option<Value> {
    let method = request.get("method")?.as_str()?;
    let id = request.get("id").cloned();

    // A notification has no id and must never be answered — a reply to one is a
    // protocol error that some clients treat as fatal.
    let id = id?;

    Some(match method {
        "initialize" => ok(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "stackvo", "version": env!("CARGO_PKG_VERSION") },
            }),
        ),
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, tools_list(allow_writes)),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            match call(name, &args, allow_writes).await {
                Ok(body) => text_result(id, &body, false),
                // Returned as tool output rather than a JSON-RPC error: an
                // assistant can read and act on "Docker is not running", which
                // a transport-level failure would hide from it.
                Err(e) => text_result(
                    id,
                    &json!({ "error": e.message, "code": format!("{:?}", e.code), "hint": e.hint }),
                    true,
                ),
            }
        }
        _ => err(id, -32601, "method not found"),
    })
}

/// Run one tool.
pub async fn call(name: &str, args: &Value, allow_writes: bool) -> Result<Value> {
    use crate::error::{Code, Error};

    let tool = TOOLS
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| Error::new(Code::NotFound, format!("no tool named {name}")))?;

    if tool.writes && !allow_writes {
        return Err(Error::new(
            Code::PermissionDenied,
            format!("{name} changes the stack and this server was started read-only"),
        )
        .with_hint(crate::hints::MCP_NEEDS_ALLOW_WRITES));
    }

    let ws = crate::workspace::resolve();
    let root = ws.require_root()?;
    let string = |key: &str| args.get(key).and_then(|v| v.as_str()).map(str::to_string);

    match name {
        "stackvo_overview" => {
            let preflight = crate::preflight::run().await;
            let engine = crate::engine::status().await;
            let projects = crate::commands::list_projects(&root)
                .await
                .unwrap_or_default();

            Ok(json!({
                "workspace": { "root": ws.root, "version": ws.stackvo_version },
                "engine": engine,
                "preflight": preflight,
                "projects": {
                    "total": projects.len(),
                    "running": projects.iter().filter(|p| p.running).count(),
                    "withProblems": projects.iter().filter(|p| !p.manifest_valid).count(),
                },
            }))
        }

        "stackvo_doctor" => Ok(json!(crate::doctor::run(Some(&root)).await)),

        "stackvo_log_files" => Ok(json!(crate::applog::candidates_all(&root)?)),

        "stackvo_projects" => Ok(json!(crate::commands::list_projects(&root).await?)),

        "stackvo_project" => {
            let name = string("name")
                .ok_or_else(|| Error::new(Code::InvalidInput, "`name` is required"))?;

            let projects = crate::commands::list_projects(&root).await?;
            let project = projects
                .into_iter()
                .find(|p| p.name == name)
                .ok_or_else(|| Error::not_found(format!("project {name}")))?;

            // The three questions that actually explain a site not loading,
            // answered together rather than left as three more tool calls.
            let certs = crate::certs::status(&root).await;
            let covered = project
                .domain
                .as_deref()
                .map(|d| crate::certs::covered_by(&certs.covered, d));

            Ok(json!({
                "project": project,
                "xdebug": crate::xdebug::status(&root, &name).await.ok(),
                // Carries `effective` — what PHP in the container actually has
                // — which turns "why is my upload failing at 2M" from a guess
                // into a reading.
                "phpIni": crate::phpini::status(&root, &name).await.ok(),
                "certificateCoversDomain": covered,
                "container": crate::engine::inspect(&name).await.ok(),
            }))
        }

        "stackvo_services" => Ok(json!(crate::commands::list_services(&root).await?)),

        "stackvo_logs" => {
            use futures_util::StreamExt;

            let container = string("container")
                .ok_or_else(|| Error::new(Code::InvalidInput, "`container` is required"))?;
            let tail = args
                .get("tail")
                .and_then(|v| v.as_u64())
                .unwrap_or(100)
                .clamp(1, 2000) as u32;

            // follow: false, so the stream ends on its own at the last line.
            let stream = crate::engine::logs_stream(&container, tail, false)?;
            let lines: Vec<String> = stream.map(|line| line.text).collect().await;

            Ok(json!({ "container": container, "lines": lines }))
        }

        "stackvo_certificates" => Ok(json!(crate::certs::status(&root).await)),

        "stackvo_databases" => Ok(json!(crate::db::targets(&root).await?)),

        "stackvo_mail" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(25) as u32;
            let status = crate::mail::status(&root).await?;
            let messages = if status.running {
                crate::mail::messages(&root, limit)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            Ok(json!({ "status": status, "messages": messages }))
        }

        "stackvo_xdebug_set" => {
            let name = string("name")
                .ok_or_else(|| Error::new(Code::InvalidInput, "`name` is required"))?;
            let enabled = args
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| Error::new(Code::InvalidInput, "`enabled` is required"))?;

            Ok(json!(crate::xdebug::set(&root, &name, enabled).await?))
        }

        "stackvo_certificates_reissue" => Ok(json!(crate::certs::apply(&root, true).await?)),

        // ---- operations, headless ------------------------------------------
        //
        // These were unreachable from this server while the runner was welded
        // to AppHandle. `progress::Null` drops the progress events (there is
        // no window to receive them); the outcome is the return value, which
        // is what an MCP client wants anyway. `run_operation` awaits the
        // process to completion, so a reply here means the work is done, not
        // merely started.
        "stackvo_project_start" => {
            let name = string("name")
                .ok_or_else(|| Error::new(Code::InvalidInput, "`name` is required"))?;
            crate::engine::start_container(&name).await?;
            Ok(json!({ "project": name, "running": true }))
        }

        "stackvo_project_stop" => {
            let name = string("name")
                .ok_or_else(|| Error::new(Code::InvalidInput, "`name` is required"))?;
            crate::engine::stop_container(&name).await?;
            Ok(json!({ "project": name, "running": false }))
        }

        "stackvo_stack_up" => {
            let mode = string("mode").unwrap_or_else(|| "minimal".into());
            let mut cmd_args = crate::runner::compose_base_args(&root);
            cmd_args.extend(crate::runner::profile_args(&mode, &[])?);
            cmd_args.extend([
                "up".into(),
                "-d".into(),
                "--build".into(),
                "--pull=missing".into(),
                "--remove-orphans".into(),
            ]);
            run_headless("up", &mode, "docker", &cmd_args, &root).await?;
            Ok(json!({ "mode": mode, "up": true }))
        }

        "stackvo_stack_down" => {
            let mut cmd_args = crate::runner::compose_base_args(&root);
            cmd_args.extend([
                "--profile".into(),
                "core".into(),
                "--profile".into(),
                "services".into(),
                "--profile".into(),
                "projects".into(),
                "down".into(),
            ]);
            run_headless("down", "stack", "docker", &cmd_args, &root).await?;
            Ok(json!({ "down": true }))
        }

        "stackvo_generate" => {
            // The Rust writer directly — same renderer the app uses, no shell.
            let scope = string("scope").unwrap_or_else(|| "all".into());
            let report = crate::commands::write_generated(&root, &scope, |_| {})?;
            Ok(report)
        }

        // Unreachable while the table and this match agree, which is what the
        // test below is for.
        _ => Err(Error::new(
            Code::Unsupported,
            format!("{name} is listed but not implemented"),
        )),
    }
}

/// One headless operation: same runner, same argv the window path builds, no
/// events. The operation id keeps log correlation working.
async fn run_headless(
    prefix: &str,
    subject: &str,
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
) -> Result<()> {
    let operation_id = crate::events::next_operation_id(prefix);
    // The Tauri-free sink, so this whole path names no Tauri type at all —
    // `events::Sink::Headless` did the same job but dragged `AppHandle` into
    // the signature it lives in.
    crate::runner::run_operation(
        &crate::progress::Null,
        crate::runner::Operation {
            operation_id: &operation_id,
            subject,
            // Names are still required by the struct; nothing receives them.
            progress_event: "mcp:progress",
            finished_event: "mcp:done",
            program,
            args,
            cwd,
            env: &[],
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The check that makes the table trustworthy: every tool names a command
    /// the contract actually declares. Without it, a command renamed in
    /// `ipc.json` leaves a tool describing something that no longer exists, and
    /// nothing says so.
    #[test]
    fn every_tool_names_a_real_contract_command() {
        let ipc = crate::contracts::ipc();
        let commands = ipc["commands"]
            .as_object()
            .expect("the contract declares commands");

        for tool in TOOLS {
            assert!(
                commands.contains_key(tool.command),
                "{} names `{}`, which is not in contracts/ipc.json",
                tool.name,
                tool.command
            );
        }
    }

    /// A read-only tool backed by a mutating command is the failure this whole
    /// arrangement exists to prevent: an assistant told a call was safe, making
    /// it, and changing the user's stack.
    #[test]
    fn read_only_tools_are_backed_by_read_only_commands() {
        let ipc = crate::contracts::ipc();

        for tool in TOOLS.iter().filter(|t| !t.writes) {
            let kind = ipc["commands"][tool.command]["kind"]
                .as_str()
                .unwrap_or("unknown");
            assert!(
                matches!(kind, "query" | "stream"),
                "{} is advertised read-only but `{}` is declared `{kind}`",
                tool.name,
                tool.command
            );
        }
    }

    /// And the converse: a writing tool must not be backed by a query, which
    /// would mean the confirmation gate is guarding nothing.
    #[test]
    fn writing_tools_are_backed_by_mutating_commands() {
        let ipc = crate::contracts::ipc();

        for tool in TOOLS.iter().filter(|t| t.writes) {
            let kind = ipc["commands"][tool.command]["kind"]
                .as_str()
                .unwrap_or("unknown");
            assert!(
                matches!(kind, "mutation" | "operation"),
                "{} is gated behind --allow-writes but `{}` is only a `{kind}`",
                tool.name,
                tool.command
            );
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let mut names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two tools share a name");
    }

    /// The default is read-only. A server that offered its writing tools
    /// unless told not to would be one flag away from an assistant rebuilding
    /// somebody's stack unasked.
    #[test]
    fn writing_tools_are_hidden_unless_asked_for() {
        let listed = tools_list(false);
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();

        assert!(names.contains(&"stackvo_projects"));
        assert!(!names.contains(&"stackvo_xdebug_set"));
        assert!(
            TOOLS.iter().any(|t| t.writes),
            "the gate needs something to gate"
        );

        let with_writes = tools_list(true);
        assert!(with_writes["tools"].as_array().unwrap().len() > names.len());
    }

    /// Every advertised tool is annotated, so a client can decide about a tool
    /// it has never seen.
    #[test]
    fn tools_carry_read_only_annotations() {
        let listed = tools_list(true);
        for entry in listed["tools"].as_array().unwrap() {
            let name = entry["name"].as_str().unwrap();
            let tool = TOOLS.iter().find(|t| t.name == name).unwrap();
            assert_eq!(entry["annotations"]["readOnlyHint"], json!(!tool.writes));
            assert_eq!(entry["annotations"]["destructiveHint"], json!(tool.writes));
        }
    }

    #[tokio::test]
    async fn initialize_reports_the_protocol_and_the_tool_capability() {
        let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
        let response = handle(&request, false)
            .await
            .expect("initialize is answered");

        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["result"]["serverInfo"]["name"], "stackvo");
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    /// Replying to a notification is a protocol error some clients treat as
    /// fatal, and `notifications/initialized` arrives on every session.
    #[tokio::test]
    async fn notifications_are_not_answered() {
        let request = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(handle(&request, false).await.is_none());
    }

    #[tokio::test]
    async fn an_unknown_method_is_a_json_rpc_error_not_a_panic() {
        let request = json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/list" });
        let response = handle(&request, false).await.unwrap();
        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(response["id"], 7);
    }

    /// A refused write comes back as tool output, not a transport error: the
    /// assistant has to be able to read the reason and say it.
    #[tokio::test]
    async fn a_write_refused_in_read_only_mode_explains_itself() {
        let request = json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "stackvo_xdebug_set", "arguments": { "name": "shop", "enabled": true } }
        });

        let response = handle(&request, false).await.unwrap();
        assert_eq!(response["result"]["isError"], json!(true));

        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("read-only"), "got: {text}");
        assert!(text.contains("--allow-writes"), "the fix has to be named");
    }

    #[tokio::test]
    async fn an_unknown_tool_is_reported_as_tool_output() {
        let request = json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "stackvo_delete_everything", "arguments": {} }
        });

        let response = handle(&request, true).await.unwrap();
        assert_eq!(response["result"]["isError"], json!(true));
    }
}
