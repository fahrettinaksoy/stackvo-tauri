# Security

## Reporting a vulnerability

Please report privately, not as a public issue: open a
[security advisory](https://github.com/stackvo/stackvo-tauri/security/advisories/new),
or email **backend@cyh.com.tr**.

Include what you did, what happened, and which version and platform. A proof of
concept helps but is not required to file.

You will get an acknowledgement within 72 hours and an assessment within a week.
If a fix is needed, we will agree a disclosure date with you before publishing.

## What this app can reach

StackVo Desktop is a local development tool, so its capabilities are broader
than a typical GUI. Understanding them is the fastest route to a good report:

| Capability                 | Why                                                        | Boundary                                                                                                                          |
| -------------------------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Docker socket / named pipe | Manages the containers that are the product                | Read from `DOCKER_HOST`, the docker context, then well-known sockets. No TLS stack is compiled in — the daemon is local by design |
| `/etc/hosts`               | Project domains must resolve                               | Writes only inside a `# >>> stackvo >>>` marker block, after showing the user a diff. Elevation via osascript / pkexec / UAC      |
| Project directories        | Reads and writes `stackvo.json` and generated build inputs | Confined to `<workspace>/projects/<name>`; names are validated and the resolved path is checked for containment                   |
| Subprocesses               | Runs the StackVo CLI, `docker compose`, and a PTY          | Spawned without a shell — arguments are passed as a vector, never interpolated into a command line                                |
| `.env`                     | Enables services and stores stack configuration            | Patched line-in-place; keys and values are validated so the format cannot be broken                                               |

The webview itself is deliberately weak: `capabilities/default.json` grants no
blanket plugin permissions, and everything above happens in typed Rust commands
behind the IPC boundary.

## What is in scope

- Escaping the workspace — reading or writing outside `<workspace>/projects/`
- Reaching Docker, `/etc/hosts` or a subprocess with input the user did not
  supply, for example through a crafted `stackvo.json` or `.env`
- Secrets leaving the process: in an event payload, in the diagnostic log, or in
  a rendered view
- Defeating update signature verification

## What is not

- Anything requiring an attacker who already has your user account. This app
  runs as you and manages your own machine; it is not a privilege boundary
  against yourself.
- Docker being able to do Docker things. Access to the socket is the point.
- Advisories on unmaintained transitive crates from Tauri's Linux stack. These
  are tracked in `src-tauri/deny.toml` with the reasoning; a _vulnerability_ in
  one of them is in scope.

## Supported versions

Only the latest release. This is a pre-1.0 project and there is no backport
branch.
