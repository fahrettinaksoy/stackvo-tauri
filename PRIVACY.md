# Privacy

## The short version

**There is no telemetry, and there is no plan to add any.** Nothing about how
you use this app is counted, sampled or sent anywhere. There is no account, no
sign-in, no crash reporting service and no server behind the app: it talks to
your own Docker daemon and your own filesystem, and the only thing it contacts
on its own initiative is the update endpoint — described below, and only from
the Settings screen.

This document exists because "we don't collect anything" is not a fact until
somebody writes down what "anything" was measured against. The readiness review
made the point in the other direction: a tool that quietly collects nothing and
a tool that quietly collects something look identical from the outside, so
silence is not a privacy property.

### If that ever changes

Any future telemetry would be **opt-in, off by default, and described here
before it ships**, with the payload listed field by field on the screen that
offers it. An update that starts sending something without that is a defect, not
a decision — report it as one (see [SECURITY.md](SECURITY.md)).

---

## What is stored, and where

Everything here is a plain file on your own machine. Nothing is encrypted at
rest and nothing needs a password to read, because none of it leaves the
account it belongs to.

| What                              | Where                                                                                                                                    | How long                                                                                                                 |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `preferences.json` — app settings | macOS `~/Library/Application Support/StackVo`, Windows `%APPDATA%\StackVo`, Linux `~/.config/stackvo`                                    | Until you delete it                                                                                                      |
| `preferences.corrupt-<UTC>.json`  | Beside it                                                                                                                                | Until you delete it — a file that failed to parse, kept rather than overwritten so the settings it held can be recovered |
| Application log, rotated daily    | macOS `~/Library/Logs/StackVo`, Windows `%LOCALAPPDATA%\StackVo\logs`, Linux `~/.local/state/stackvo/logs`                               | **7 files**, oldest deleted automatically                                                                                |
| `crash-<UTC>-<pid>.txt`           | With the logs                                                                                                                            | **10 reports**, oldest deleted automatically                                                                             |
| Diagnostic bundle (`.zip`)        | Only where you choose to save it, from the system save dialog                                                                            | Yours — the app never keeps a copy                                                                                       |
| Stack configuration and projects  | Your workspace: `.env`, `stackvo.json`, generated compose and server files, project sources, container logs under `logs/projects/<name>` | Until you delete them                                                                                                    |

Deleting all four of the app's own locations is supported and loses nothing but
settings and history. The workspace is separate on purpose: it is the stack, not
the app.

### What the log contains

Command lines the app runs, the output of `docker compose` and of the StackVo
CLI, the paths involved, and errors. That is enough to reconstruct what you were
doing with the app on a given day, which is exactly why the files stay on your
machine and rotate away after a week.

Values of keys the app knows to be secrets — `*_PASSWORD`, `*_SECRET`,
`*_TOKEN`, `*_KEY` and their family, as `config::Env::is_secret` defines it —
are **masked before the line is written**. The mask is applied a second time
when a diagnostic bundle is built, because a bundle made today can carry lines
written by an older build whose masking rule was narrower.

### What a diagnostic bundle contains

`about.txt` (version, platform), `preflight.json`, `doctor.json`,
`engine.json`, the rotating log files and any crash reports — each log capped at
1 MiB, with `truncated` naming the ones that were cut. **It does not include
`.env`, and it does not include project sources.** It is plain text and JSON so
that you can read it before you send it; the file listing is shown in the app
rather than a bare "saved" confirmation, for the same reason.

Where a bundle goes afterwards is entirely your decision — the app writes the
file and nothing else.

---

## What leaves this machine

### On the app's own initiative

| Host                                                                   | When                                                                                                                                                               | What is sent                                                                                                                                                                                                   |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `raw.githubusercontent.com` — the update endpoint in `tauri.conf.json` | When the **Settings** screen is opened, and when you press _Check for updates_. Not at launch, and not at all when the build has no update public key compiled in. | An ordinary HTTPS GET for a static `latest.json`. The app adds no identifier, no version parameter and no cookie. As with any HTTP request, whoever serves the file sees your IP address and the request time. |
| `127.0.0.1` — the mail catcher                                         | While the mail screen is open                                                                                                                                      | Loopback only. This traffic never reaches a network interface, and the HTTP client for it is built with `no_proxy()` precisely so a corporate proxy setting cannot pull it off the machine.                    |

That is the complete list. Everything else below happens because you asked for
it.

### Because you asked for it

| What you do                     | Where it goes                                                                                                                                                                                                                                                                        |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Create a project from a Git URL | The remote **you** typed, over the credentials your own `git` is configured with                                                                                                                                                                                                     |
| Start the stack                 | Whatever registry the images name — Docker Hub by default. Your Docker daemon does this, with its own configuration                                                                                                                                                                  |
| Open a share tunnel             | `cloudflared` publishes the site at a `trycloudflare.com` address. **The URL is public**: anyone who has it reaches the site on your machine, without a password, until you stop the tunnel                                                                                          |
| Follow a link in the app        | Your browser opens it. The app's own links point at `stackvo.github.io`, `github.com`, `docs.docker.com`, and — from the support and share menus — `bsky.app`, `buymeacoffee.com`, `discord.gg`, `fosstodon.org`, `reddit.com`, `twitter.com`, `www.linkedin.com`, `www.youtube.com` |
| Send a diagnostic bundle        | Wherever you send it                                                                                                                                                                                                                                                                 |

### While Docker builds an image

The generated Dockerfiles fetch packages during the build, from
`deb.nodesource.com` (Node.js), `dl.cloudsmith.io` (Caddy) and `rubygems.org`
(Ruby), plus whatever the base images pull from their own package mirrors. This
is Docker's network activity, on the machine, driven by files the app writes
into your workspace — you can read every one of them before building.

---

## What this app is _not_ protecting you from

Naming these is more useful than a reassurance:

- **`.env` holds database passwords in plain text.** File permissions are the
  only boundary. On a managed machine that file is also whatever your backup and
  sync tools do with it. The readiness review tracks OS-keystore storage as a
  future contract change; today this is the honest state.
- **Docker access is total.** The app manages containers, so it can read and
  write anything those containers mount — including your project sources.
- **A tunnel is a publication.** See above; it is the one action in the app that
  makes something on your machine reachable from the internet.
- **The app runs as you.** It is not a boundary against someone who already has
  your user account.

---

## How this document is kept true

`src-tauri/tests/privacy_claims.rs` scans the shipped code — the Rust
production regions, the front end, and the updater endpoint in
`tauri.conf.json` — for every host it can reach, and **fails the build if one of
them is not named in this file**. A dependency or a feature that starts talking
to somewhere new cannot land quietly; the build stops until this page says so.

What the test cannot settle is a claim about intent — "no telemetry" is not a
string a parser can find. What it can settle is the surface that claim is made
about, and that is what it holds.
