# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning is [semver](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Two more release targets: Linux aarch64 and Windows ARM64** (§3 #22). Six
  in the matrix instead of four, on **native ARM runners** rather than by
  cross-compilation — a Tauri bundle is a platform installer, a `.deb`, an
  AppImage, an MSI, and cross-building one means a second toolchain and a
  second set of bundler quirks to keep working for a machine GitHub already
  rents by the minute. Every `matrix.os == 'ubuntu-latest'` style condition
  became a family test, or the new rows would have skipped the steps that make
  their artefacts installable; the SBOM stays pinned to one target rather than
  to the Linux family, because two attachments with one name is a collision
  rather than a second document.

  **Unverified until a tag runs it**, and the row in `docs/durum.md` says so
  rather than moving to done. Nothing on a developer's machine can prove that a
  runner label exists or that the bundler is happy on it. `fail-fast: false`,
  which was already there, is what keeps a wrong guess from taking the other
  four builds down with it.

- **An accessibility statement, and the measurement that makes it true**
  (§3 #25). [`docs/accessibility.md`](docs/accessibility.md), in the shape
  EN 301 549 asks for: conformance status, what was measured and how, the
  features that exist for it, and — at the same length — what it cannot claim.

  The row said the prerequisite was already there: "axe runs on four pages in a
  real engine, zero serious or critical". Both halves of that turned out to be
  weaker than they read, and the statement could not honestly be written on top
  of them:

  - The run was **scoped to `#app`**, and Vuetify's overlay container is a
    *sibling* of `#app` rather than a child. Every tooltip, menu, dialog and
    side sheet in the application was outside the measurement.
  - Those overlays were **covering the page**. A closed overlay keeps a
    full-viewport box at `z-index: 2000`, so axe could not resolve a background
    for anything underneath and skipped the contrast rule almost everywhere.
    The zero was partly the page being hidden from the checker.
  - Four of the nine routes were covered, and the four were the ones somebody
    thought of.

  All three are closed: the run is over the whole document and every route the
  router declares, and `tests/accessibility-claims.spec.js` fails if the
  statement, the suite and the router come apart — including if the `#app`
  scope ever comes back. The result is **zero violations at every severity on
  all nine routes**, which is a different sentence from the one it replaced.

- **A workbench: a snippet, the application it runs inside, and what came
  back** (F-5, ADR 0022). The row called `tinker` over the PTY "honest 90%, but
  not a workbench", and §5.5 held the remaining tenth as a **decision** rather
  than a task — because `quickcmd.rs` had refused an in-app REPL in writing, and
  reversing a refusal is not something a commit does quietly.

  **The refusal was right and it still stands.** A *line* REPL in a pane is
  exactly what it described: a worse `tinker` with no readline, no history file,
  none of the colours somebody configured, and a terminal one click away that
  does all of it better. `tinker` still opens the user's own terminal from the
  command list. What was accepted is the tool a line REPL cannot be: a snippet
  is **twenty lines you edit** — write a query, run it, change line three, run
  it again — which in a REPL is retyping. The two are not ranked, they are split
  by what the person is doing, and on screen they sit one above the other: the
  terminal pane, then this.

  **The security model is extended one level down, not broken.** The webview
  sends a runner **id** and a body of code; the id is what picks the program, so
  `laravel` means `php artisan tinker --execute` and nothing else. The snippet
  is **one argv element and always the last**, so `; rm -rf ~` is a string
  rather than a second command. No consent gate, for two reasons that are the
  whole argument: it runs in the project's **own container**, which already runs
  that repository's code, and the code comes from the **person at the keyboard**
  rather than from a file in a repository somebody cloned. There is no `host`
  runner and there will not be one — that is `hooks`' `host` step, approved
  against a digest first.

  Six runners in two tiers, and every row says which it is: `php artisan tinker
  --execute`, `wp eval`, `python manage.py shell -c` and `bin/rails runner` boot
  the application; `php -r` and `node -e` are the language on its own. A pane
  that showed them alike would let somebody debug for ten minutes before finding
  out their models were never loaded. Laravel is offered only where
  `composer.json` also declares `laravel/tinker`, because `--execute` is
  Tinker's flag and a `--no-dev` install does not have it.

  **Every argv was measured before it was written**, on Laravel 13.25, Django on
  Python 3.14, Rails 8.1.3, `wordpress:cli`, and this workspace's own project
  container. Two of those measurements changed the design. Laravel's
  `--execute` does **not** echo the value of the last expression the way the
  interactive REPL does — `2+3;` produces nothing — so the pane says to
  `dump()`. And a PHP fatal is written to **stdout**, not stderr, so success is
  read from the exit code; a pane that decided by looking at stderr would have
  drawn a green chip over an uncaught exception, and a test now holds that.

  **The limit runs inside the container.** Killing a `docker exec` client does
  not stop what it started, so this app's own clock alone would have left a
  snippet with a loop in it burning a CPU in somebody's container after the pane
  said "timed out". The command is wrapped in `timeout 30` — measured present in
  the php, node, python, ruby and wordpress-cli images and in this workspace's
  project container, where `timeout 1 sleep 3` exits 124. "Every image I
  checked" is not "every image", so there is a fallback and the result carries
  `limited`, which the pane shows as a warning rather than as silence.

  History keeps **the code, never the output**: a snippet is what the person
  wrote and is the thing they want back, while the output is the application's
  data — the rule `querylog` states. It lives in the app's own config directory
  beside `hook-consent.json` rather than in the project, because a file written
  into a checkout turns up in somebody's `git status`.

  `examples/repl_probe.rs` runs the whole thing against the projects on the
  machine, and it distinguishes a broken runner from a project whose
  dependencies were never installed — on this workspace it reports the Laravel
  runner as skipped, with `vendor/` missing as the reason, and measures the
  other two.

- **`stackvo`, a command-line interface** (A-1). Eight of the ten tools this app
  is measured against ship one. It sat unbuilt because it is a **third surface**
  — the window reaches the core one way, an assistant another — and a third
  consumer is a third thing that can drift away from `contracts/ipc.json` while
  every existing test still passes.

  It is accepted on the condition the MCP server was accepted on: **every
  command names the contract command it implements**, and `cli_surface.rs`
  cross-checks the pair. A command naming something the contract does not
  declare fails the build, and so does a command listed under `--help`'s
  "Reads" heading whose contract command is a mutation — because that heading
  is what somebody reads before typing into a machine they care about.

  One part is tighter than the MCP table. A tool there dispatches on its *name*,
  so a table entry with no matching arm compiles and fails when called; the
  module says so and keeps a fallback for it. Here the table carries an
  `Action`, dispatch matches on the enum, and the compiler refuses a variant
  with no arm. There is no "listed but not implemented" state to test for
  because there is no way to reach one.

  Twenty commands: ten that read (`status`, `doctor`, `projects`, `project`,
  `services`, `logs`, `certs`, `db`, `mail`, `mcp`) and ten that change the
  stack (`up`, `down`, `start`, `stop`, `restart`, `generate`, `xdebug`,
  `certs-renew`, `mcp-install`, `mcp-remove`). Every one has `--json`, so the
  same value the table is rendered from is available to a script — the human
  output is built *from* that value rather than from a second query, which is
  how the two cannot come to describe different things.

  **stdout is the answer, stderr is the narration.** The progress writer ADR
  0005 left room for is the fourth sink, and it prints to stderr — so
  `stackvo doctor --json | jq` works while a compose build is still scrolling
  past, and a failure leaves an empty file and a non-zero status rather than a
  file with an error message in it. The exit code distinguishes the two
  failures a wrapper script wants to handle rather than report: 3 is "nothing is
  set up on this machine", 4 is "Docker is not running".

  **No argument-parsing dependency.** What is needed is long and short flags,
  `--flag=x` and `--flag x`, and `--` to stop; what matters far more than the
  shape of that code is that an unrecognised flag is an **error**. A CLI that
  shrugs at `--tial 50` and quietly uses the default has told you it did
  something it did not do.

  **The last Tauri binding in the lifecycle path went with it.** `run_hooks`
  needed an `AppHandle` for one reason — building the sink — so its body moved
  to `hooks::run_for_project`, which takes the sink instead. `stackvo stop` and
  the stop button now run the same hooks rather than being two operations
  wearing one name. Writes land in the same audit trail, under `cli_` names:
  the log answers "what happened to this machine", and "somebody ran this in a
  terminal" is part of that answer.

- **The package index can be verified, withdrawn versions are refused** (C).
  `market.rs` described a chain of three links and said the first one — *a
  pinned key → registry.json* — was missing. `Trust::Signed` was a shape with
  no implementation, so a third-party source could be fetched but never
  believed, and the claim that the architecture was "ready" for third-party
  distribution was not true of a door that could not be shut.

  It is now. `signing.rs` verifies a minisign signature over the index against
  keys the machine already trusts, and `refresh` runs it **before the index is
  parsed** — the same ordering this module already applied to a manifest, for
  the same reason.

  **Zero new packages.** minisign is ed25519 with a file format around it and
  is what `tauri-plugin-updater` already uses, so `minisign-verify` was in
  `Cargo.lock` already — measured, not assumed. It also means ADR 0015's key
  ceremony has a tool that exists (`minisign -G`); a scheme needing a bespoke
  tool is one whose ceremony never happens.

  **No official key is shipped, and a test keeps it that way.** Inventing a
  placeholder would be worse than the gap, because every later reader would
  believe the chain was closed. A build with no key refuses a signed refresh
  and names *that* as the missing half. An organisation running its own mirror
  is not waiting on any of it: it signs its own index and pins its own key
  through `policy.market.additionalKeys` — a field written for exactly this
  and, until now, read by nothing.

  **Rotation is designed in, because it cannot be added afterwards.** A machine
  holds a *set*, and a new key arrives in a `known-keys.json` signed by one
  already trusted. What that deliberately cannot do is remove a compromised
  key on its own say-so — a leaked key can sign a document naming only itself —
  so retirement is a property of a build, and a retired key cannot be brought
  back by any document or by policy.

  **Takedown has both halves.** A withdrawn version is refused at install
  rather than warned about: ADR 0014 keeps it in the index so a machine can
  find out what happened to one it already has, and whether a *new* install may
  proceed is a different question. The other half is `doctor`, which lists
  installed versions the publisher has since withdrawn — without it the
  container keeps running, the stack looks healthy, and the withdrawal is a
  line in an index nobody re-reads.

  Two decisions changed while this was being written, both because a test
  disagreed. Legacy minisign signatures are accepted after all — the reasoning
  for refusing them (that the two modes sign different things) does not survive
  contact with how the mode is declared and checked, and refusing bought
  nothing while costing an organisation whose mirror was signed by an older
  tool. And the pinned-key check now happens *before* the signature file is
  fetched: fetching first told a machine with no key `registry.json.minisig: No
  such file`, sending somebody to ask their publisher for a signature when the
  missing half was on this side.

- **A project may declare its own commands** (B-4). The catalogue is eleven
  commands most projects have; what it cannot know is the one *this* project
  runs every day — `artisan app:reindex`, `npm run codegen`, a `bin/` script
  somebody wrote last week. Now `stackvo.json` can say so:

  ```json
  "commands": {
    "reindex": { "exec": ["php", "artisan", "app:reindex"], "about": "Rebuild the search index" }
  }
  ```

  They appear in the project pane beside the built-in ones, marked as coming
  from the project, and in the terminal as `stackvo commands` and
  `stackvo run <id>`.

  **The security rule the catalogue existed for is intact.** The webview still
  only ever sends an **id** — it cannot name a program, and `quickcmd::resolve`
  is the one place either kind becomes an argv. What changed is where a command
  may be *declared*, and `docs/durum.md` §5 had been holding exactly that
  distinction open: the argument against a webview naming a program is about a
  surface that runs code it did not choose, and a file committed to the
  repository is not that surface.

  **It stops at the container line, and that is why it needed no approval
  flow.** A declared command may only be `exec` — inside the project's own
  container. There is no `host` form. `hooks.rs` already makes the argument: a
  container runs the repository's code anyway, so a repository able to run a
  command in it has gained nothing, whereas a host step is what turns `git
  clone` plus a button into arbitrary code execution — and that one has a
  consent record keyed to a digest. Declaring `host` here is refused by name,
  so an author is told where the real feature lives.

  Three rules make a quiet mistake impossible. `exec` is an **argv array**, and
  a command string is refused rather than split — splitting is how
  `sh -c "a && b"` becomes four arguments. An id already in the catalogue is
  **refused** rather than allowed to win or lose silently, because either ends
  with somebody pressing a button labelled `migrate` that is not
  `php artisan migrate`. And an id is lower-case letters, digits and dashes,
  because it travels to the webview and back.

  The manifest serialiser learned the block too, for the reason the hooks block
  is written out: that text is rewritten on every form save, so a field the
  serialiser does not know about disappears the first time somebody changes an
  unrelated setting.

- **`stackvo tui`, a terminal screen you can work in** (M-8). M-8 is
  "alternative surfaces", and its own record already said what makes something
  one: the tray stopped being a shortcut the day it could **act** without
  raising the window. The same test applies here, so this is not a report —
  it lists every project and service live, moves a cursor through them, and
  starts and stops the row it is on. `l` shows a container's last lines, `r`
  refreshes, `q` leaves.

  **No TUI library.** Measured before deciding, the way `keyring` and
  `toml_edit` were: `ratatui` puts **25 new packages** in `Cargo.lock`
  (649 → 674) — a layout solver, a widget set, two `unicode-width` crates, an
  LRU cache, `strum`, `darling`, a second `rustix` — for a list, a detail line
  and a status bar. Drawing is `cli::Style` and the column arithmetic the CLI
  tables already use; the cursor, the alternate screen and colour are escape
  sequences, which are text. Raw mode is the only part that needs an operating
  system and both halves were already in the lock file — `libc` through
  `portable-pty`, `windows-sys` through Tauri. **Zero new packages**, measured
  rather than claimed.

  **The risk is the terminal, and it is paid for.** One left in raw mode has no
  echo, no line editing and no working `Ctrl-C`, and the way out is typing
  `reset` blind. All four exits are covered: `Drop` for returning and for `?`,
  a panic hook because release builds are `panic = "abort"` and `Drop` does not
  run, and `Ctrl-C` read as a key because raw mode stops the terminal turning
  it into a signal. The restore goes through one function that *takes* the
  saved settings, so a hook and a `Drop` firing together still put the terminal
  back exactly once.

  And it is held by running rather than by reading. `examples/tui_probe.rs`
  opens a real pty, runs the real binary inside it, sends `j` and `q`, and
  reads the terminal's own settings back to confirm echo and line mode
  returned — eleven checks, because a screen that agreed with its own
  expectations is the QR encoder's mistake in a new place.

  `cli::Backing` gained a third value for it. A screen implements no single
  contract command; it drives several, and every name is still checked, with a
  test that a screen names more than one — a row that named exactly one should
  have been a `Contract`.

- **`stackvo php …`, `stackvo artisan …` — the project's container from the
  working directory** (A-3). `cd` into a project and `stackvo php -v` reports
  the PHP that project declares, with its extensions and its `php.ini`, from a
  machine with no PHP installed on it. `artisan`, `composer`, `npm`, `node`, a
  general `exec` and an interactive `shell` alongside it.

  Which project comes from the working directory, matched against the real
  project list rather than a folder name — a worktree's name lives in
  `stackvo.local.json`, not in its directory. The **deepest** enclosing project
  wins, because a worktree sits inside the project tree and answering with the
  parent would migrate the wrong database while the caller stood in the feature
  branch. `--project` names another one from anywhere.

  **The subdirectory maps through.** `stackvo artisan` from `app/Http` runs in
  `/var/www/html/app/Http`, so it behaves the way `artisan` on the host would.
  Only where the source is mounted: `generator.rs` writes no source mount for
  Node and the rest, because a bind mount over `/app` would shadow the built
  output — so there is no counterpart directory to map onto, and the one line
  that says anything written there stays in the container goes to stderr.

  **Flag parsing stops at the command name.** `stackvo artisan migrate --force`
  reaches artisan whole; a parser that kept reading would eat `--force` and then
  complain about it, which would break the most common artisan call there is.
  The cost is that StackVo's own flags come first — `--project` included, which
  is why it is global — and that `stackvo artisan --help` goes to artisan.
  `stackvo --help artisan` prints this app's, and the main help says so.

  **The exit code is passed through**, because `stackvo artisan test` in a CI
  script is worth nothing if a failing suite comes back as 0. A TTY is requested
  only when there is one to inherit, so `echo … | stackvo php` works in a
  pipeline and `stackvo shell` still gets a terminal.

  These are the first commands with no `contracts/ipc.json` command behind them,
  and that is a decision rather than an omission — ADR 0018. The webview may
  never name a program to execute, so the contract has none that takes one; a
  terminal is the opposite case, and `stackvo artisan migrate` is strictly safer
  than the `docker exec -it stackvo-shop php artisan migrate` it replaces
  because it cannot get the container name wrong. The exception is not a gap in
  the gate: `Backing::HostShell` carries four assertions of its own, including
  that every one of them runs through `docker exec` and none on the host.

- **An environment per worktree** (N). `git worktree add` gives a branch its own
  directory; this gives that directory its own hostname, its own database and
  its own environment, so `shop.loc` and `feature-x.shop.loc` are two branches
  of one application running at the same time. Of the ten tools this app is
  measured against, one does this.

  **A worktree is a project, and nothing below the module knows the word.** The
  directory lands in the project tree beside its parent, and `list_projects`,
  the renderer, the hosts writer and the certificate pick it up as they pick up
  anything else. A parallel lifecycle for something that is already a project
  would have been a second copy of every bug.

  **Nothing is written into the checkout that git tracks**, and that constraint
  is what the design turns on: the files in a worktree directory belong to the
  branch, so a derived `stackvo.json` would appear as a change to whoever is
  working on it. Identity — the name and the hostname — goes into
  `stackvo.local.json`, the machine-local overlay that exists to be the file
  that is never committed, and it is added to the repository's
  `.git/info/exclude`, which is git's own per-clone ignore list rather than the
  `.gitignore` the team shares. Everything else — the database credentials,
  `APP_URL`, the branch name — is kept outside every checkout and reaches the
  container through the compose overlay that per-project variables already use.
  A branch that carries no manifest at all gets a full one derived from its
  parent's, with the parent's extra hostnames dropped so it cannot claim them.

  That cost one change in the manifest layer, and it narrows the rule as it
  widens it: a local overlay may set `name` **only to the directory it is
  sitting in**. Every other value is refused exactly as the whole key used to
  be, because the hazard never changed — a project renamed locally builds one
  image and looks for another. Restating the directory is the one thing a local
  file can say about identity that cannot be wrong.

  **The database is a database, not an instance.** A branch is not a different
  engine, and a MySQL container per branch would spend a gigabyte of memory
  holding a copy of one schema. So a worktree gets a database on the instance
  that is already running — empty, or copied from the workspace's own. The copy
  reads the source's character set on MySQL rather than taking the server
  default: a `utf8mb4` parent copied into a server-default database produces
  tables that compare differently, with no error anywhere and a join that
  returns nothing as the only symptom.

  **Three bugs came out of running it rather than reading it.** `git worktree
  remove` refuses while the tree contains **untracked** files, not only modified
  ones — and the untracked file was this app's own overlay, so a worktree with
  no user changes could not be removed, and git's message sent you looking for
  work you never did. The fix moves the overlay out of the way first, only when
  git is not tracking it, and **puts it back** if git refuses anyway: without
  that, a failed removal would strip the checkout of its name and hostname and
  leave a project the app then reports as broken.

  The second had been there for as long as the Mongo branch of `run_sql`: it
  never passed `--authenticationDatabase=admin`, and the root account lives in
  `admin` while mongosh authenticates against whatever database it connected to.
  That branch could never authenticate at all. `mongodump` and `mongorestore`
  have always carried the flag; nothing had asked Mongo a question until a
  worktree needed to know which databases it had.

  The third was an ordering one, and the sort that stays wrong for years because
  nothing visible breaks: the compose overlay is built only for projects that
  have a **compose service**, and a worktree gains one in the file the generate
  writes — so rendering the overlay first produced one with the branch's
  database credentials missing. The next `docker compose` re-rendered it and
  quietly corrected itself.

  **The database half was run against live engines**, because a `CREATE
  DATABASE` can be right in every unit test and wrong at the server — the lesson
  `mariadb-dump` and the QR encoder each cost this repository once. Two
  `#[ignore]`d tests create, list and drop on MySQL 9.7, MariaDB 12.3 and
  MongoDB 8.0, and check that the workspace's own database is refused. They do
  not run in CI: a test that fails for the absence of Docker is one people learn
  to ignore for real reasons too.

  `APP_URL` is supplied deliberately. The branch's own `.env` names the parent's
  hostname, and a framework generating links from it would send everybody back
  to `shop.loc` mid-flow — the exact bug this feature would be sold as fixing.
  The container's environment wins over the application's `.env` file, which is
  what lets this work without M-5's rule bending: the framework's own file is
  still never touched.

- **The eight "small items" that were not small** (M-2, M-3, M-4, M-7, M-8, M-9,
  M-11, M-12). Each carried a dependency, a decision or a product surface, and
  the previous round said so and stopped. This round pays the costs.

  **Releasing one caught message** (M-2). The catcher goes on catching
  everything; a message you pick is sent on to a real address through a relay
  you configure. Deliberately not the other shape — pointing the application at
  a real SMTP server sends the forty password resets a test suite generates in
  an hour to whatever addresses the fixtures contain. The service package is
  untouched: Mailpit reads its relay settings from its own environment, and this
  app reaches that through a compose overlay, the mechanism `site.rs` and
  `perf.rs` already use. The catcher's compose service is found by its **image**,
  because the key is `mailpit` on one workspace and `mailpit-1-30` on another.
  The password is in the OS keystore.

  **A QR code for every address meant for another device** (M-3). The LAN name
  and the tunnel URL both exist to be opened on a phone, and both were things
  you typed by hand. The encoder is written here — a QR code is a closed
  specification with published vectors, so the usual argument for a maintained
  dependency does not apply — and it is verified against **macOS's own decoder**
  rather than against its author's expectations. That measurement earned its
  keep immediately: every unit test passed while no decoder on earth could read
  the output, because the first copy of the format information was written along
  row 8 instead of down column 8. The format names the mask, so everything else
  being correct bought nothing.

  **A landing page listing every site** (M-4). It needed no new name: the app
  already writes the bare suffix into `/etc/hosts` and already issues a
  certificate for it, and opening `https://stackvo.loc` got Traefik's own 404.
  So this is a small sidecar answering on a name the stack had claimed and left
  empty. `nginx:alpine` rather than the `alpine:3` already pulled elsewhere,
  because Alpine's busybox is built without the `httpd` applet — found by
  running it, not by reading about it.

  **Language packs** (M-7). Adding a language was a source change in three
  files and a rebuild, which is why "the app speaks N languages" was never
  going to be answered by translating 2,000 strings — the strings were not the
  blocker. A pack is now one JSON file in the config directory, discovered at
  startup and listed beside the built-in two, with the share of the app it
  covers stated. No machine translation and no English relabelled as another
  language: a string that falls back to English is honest, a fabricated one is a
  sentence somebody has to find and disbelieve.

  **A tray that can act** (M-8). Every tray entry raised the window, which made
  it a launcher rather than a surface — and "tray-only" was therefore a mode you
  could not work in. Starting and stopping a project now happens without the
  window coming forward, with a notification as the acknowledgement. The
  coloured dot stays at the top level: "is my stack up" is a glance.

  **Framework commands** (M-9). Symfony, Django, Rails and plain Ruby, each on a
  marker file only that framework writes. Rails is found by `bin/rails` rather
  than by a Gemfile — which is Sinatra and Jekyll as often as it is Rails — and
  runs through `bundle exec`, because a binstub only works if its permission bit
  survived the checkout. This is **not** the B-4 unlock: every row is still
  compiled in and the webview still sends an id.

  **A Stripe webhook listener** (M-11). `stripe listen` connects outward, so
  nothing has to be reachable and the signing secret is stable for the session —
  unlike a tunnel, whose URL changes on every start. The API key is in the OS
  keystore and reaches the container through the environment, never as an
  argument that `docker inspect` and the operation console would both print.

  **The OAuth callback** (M-12). Defined by reading what the providers require:
  a redirect URI is a **browser redirect, not a fetch**, so `https://shop.loc/...`
  works for the flow. What varies is whether a provider accepts the string at
  registration, which is a per-provider rule that is invisible at their console
  and is now written down beside both addresses.


- **A local DNS responder** (E-1). Every new project needed a line in
  `/etc/hosts` and an administrator password to put it there. Now a responder
  answers for the workspace's whole suffix, and `*.shop.loc` works — which
  `/etc/hosts` cannot express at all, and which is the only reason E-2 was left
  half-done.

  **Not dnsmasq**, which is what every comparable tool ships. It would have
  worked, and it means a second binary packaged for three platforms, a config
  file generated from this app's state, a process supervised by something, and
  a failure mode where the machine's name resolution depends on a container
  being up. For a responder whose entire job is "say 127.0.0.1 to anything
  under one suffix", that is a lot of moving parts around a function that fits
  on a page.

  **It is not a resolver, and that is the security property.** It answers for
  one suffix and refuses everything else. It never forwards, has no upstream
  and holds no cache — an open forwarder listening on a machine is a thing that
  can be pointed at, and a development tool has no business becoming the
  resolver for anything it did not create. `shop.loc` is answered, `google.com`
  is REFUSED, and there is no code path that opens a socket to anywhere. It
  binds loopback, so nothing off the machine can reach it either.

  A high port, because 53 needs root at every start and mostly does not need
  to: the resolver files on macOS and the dnsmasq drop-ins on Linux both take a
  port, so the responder runs as the user. Windows is the exception and has to
  be — its NRPT rule names a server and has nowhere to put a port — and it is
  also the platform with no privileged-port rule, so binding 53 on loopback
  there costs nothing. The one privileged act is pointing the machine at us,
  and it is a separate button from starting the responder — the same separation
  `hosts_plan`/`hosts_apply` has, because folding them together would make a
  password prompt appear from something that reads like turning a feature on.

  **It answers over TCP as well as UDP.** A stub resolver picks its own
  transport — after a truncated answer, on a retry, or because that is simply
  what it does — and a name server that only listens on UDP answers those with
  a connection refused.

  **Every reply echoes the question it answers.** This is the bug the first
  round shipped and the probe missed: a REFUSED or a NODATA carried a header
  claiming one question over a body with none, and `dig` said so on a line
  above the one the probe was reading — *"Message parser reports malformed
  message packet"*. A lenient tool reads it anyway; a stub resolver drops what
  it cannot match against the query it sent, and a dropped reply is not a fast
  failure, it is a five-second timeout. The NODATA path is not an exotic one:
  every Chrome and Safari page load asks for an HTTPS record (type 65) before
  it asks for an address, so this was on the way to every page. An EDNS query
  now gets an EDNS answer back, too.

  **Three platforms, three mechanisms, and a detection step in front of each.**
  macOS writes `/etc/resolver/<tld>`. Linux is not one answer but a question
  about the machine, so it is asked: NetworkManager's dnsmasq, a standalone
  dnsmasq, or systemd-resolved, in that order, and nothing is written when none
  of them is what sits in front of `resolv.conf` — a guessed path is worse than
  no feature, because it rearranges name resolution on a machine this app was
  wrong about. Windows gets an NRPT rule, which takes a namespace and a server,
  applies to that suffix and nothing else. The previous round said Windows had
  no per-suffix mechanism at all; that was a statement about what had been
  looked for.

  **Nothing is trusted to have worked.** Reading back the file this app just
  wrote proves a write happened and nothing else, so applying the change is
  followed by measuring it *through the machine's own resolver*: a name under
  the suffix has to come back as loopback, and a public name that resolved
  before the change has to still resolve after it. If either fails, the change
  is undone. That is what makes writing a resolver file on a Linux distribution
  nobody here runs an acceptable thing to do — the failure mode being guarded
  against is a laptop that cannot resolve anything and a user with no idea this
  app is why. The same four measurements are a **Test it** button on the pane,
  reported separately, because the repair differs: a responder that does not
  answer is this app's fault, a machine that does not ask it is the resolver
  file's, and public names failing matters more than the feature does.

  **A file that is already there is not overwritten.** `/etc/resolver/test` is
  a path dnsmasq, Valet or a colleague's script may own, and taking a suffix
  away from another tool with no way back is not a thing to discover
  afterwards. The pane says what is there and what it says, the file is copied
  aside under the same password, and turning this off **puts it back**.

  **The file a suffix change leaves behind is removed.** macOS's mechanism is
  one file per suffix, so moving a workspace from `.loc` to `.test` used to
  leave `/etc/resolver/loc` pointing at a responder that now answers for
  `.test` and **refuses** `.loc` — worse than never having written it, because
  before that those names went upstream and failed honestly. They are found,
  named on the pane, and removed by the next apply or by turning the feature
  off.

  **The doctor knows the one failure nothing else reports.** The machine's
  resolver names a port, something else takes that port, and every project
  domain stops resolving while the app, the containers and the proxy all look
  healthy. It is a row on the doctor page now, and `null` in every other state
  — including the feature being off — because a doctor that lists what is fine
  is one people stop reading.

    **The responder comes back with the app.** Turning this on and quitting used
  to leave the machine pointed at a port nothing was bound to, and every project
  domain stopped resolving until somebody found the switch again. The condition
  for starting at launch is read off the machine — a resolver file that names us
  *is* the record that this was turned on — rather than out of a preference that
  can disagree with what the machine actually does.

  **A name answered by DNS is no longer counted as missing from the hosts
  file.** It was, which meant setting this up left the same per-project nagging
  in place and could hold the first-run gate shut over two names that resolve.

  **The four plans this machine cannot build are tested anyway.** On a macOS
  laptop the mechanism is always the resolver file, so the NetworkManager,
  dnsmasq, systemd-resolved and NRPT plans would otherwise ship without
  anything having read them; the plan is a pure function of (mechanism, TLD)
  and every one of them is checked for where it writes and whether its text
  names this machine and this port **in that file's own syntax**. So is the
  elevated command line itself — that the backup is taken before the write that
  would destroy it, that the steps are joined with `&&` so a failed one stops
  the rest, and that the reload comes last.

  The unit tests build the query with the same code that reads it, which proves
  self-consistency and nothing else. So `examples/dns_probe.rs` asks the running
  responder with the system's own `dig` — ten queries across UDP, TCP and EDNS,
  covering the wildcard, the REFUSED and the type-65 NODATA, with `dig`'s
  malformed-message warning counted as a failure rather than printed and
  ignored — and then runs the app's own self-test beside them.

  The suffix reaches a path this app writes as root, and it comes from a line
  in a file the user edits. `DEFAULT_TLD_SUFFIX=loc/../../etc` used to build
  `/etc/resolver/../../etc` — reachable only by editing your own `.env`, so
  barely an escalation, and still not something a function that runs `cp` as
  root should be capable of. One label, letters, digits and hyphens; everything
  else is refused before it reaches a path or a shell.

- **A real flame graph, and the four things running F found broken** (F-3, F-1).

  **F-3 could not be closed by drawing the same numbers better.** A flame graph
  is built from stacks — each measurement carrying its whole path, so a function
  called from two places is two boxes with their own widths — and cachegrind
  holds *edges*: the summed cost of "A called B" over every place A called B.
  `profile::call_tree` said so in its own comment and the screen honestly called
  itself a call tree. No arrangement of those edges recovers what the file does
  not contain, so the input had to change.

  Xdebug already writes the other kind. `xdebug.mode=trace` with
  `trace_format=1` records one line per function **entry and exit**, each with
  its depth and a timestamp; folding the gaps onto whatever is on the stack
  gives, per distinct path, the time that path was on it. That is a flame graph,
  and its widths are measured rather than sampled. **Trace** is now a third
  Xdebug mode beside stepping and profiling — a third mode and not a checkbox,
  because it writes a different file that a different parser reads and it costs
  far more to record.

  Measured on the running stack, not reasoned about
  (`examples/trace_probe.rs`): a PHP program calling `slow()` from two parents,
  60ms under one and 10ms under the other, comes back as **62,089µs and
  11,167µs in two boxes**. That is the sentence cachegrind cannot say.

  **Profiling had never written a file.** `xdebug.output_dir` named
  `/var/log/xdebug` from the day it shipped, and nothing on either side of the
  mount ever created that directory. Xdebug does not create it and does not
  complain — it writes nothing, silently — so switching profiling on, reloading
  with the trigger and finding an empty list was the *normal* outcome, with no
  error anywhere to say why. It is created before every compose command now.

  **MariaDB 12 had no client to talk to.** MariaDB 11 removed the `mysql*`
  symlinks and 12 ships without them, so a `mariadb:12` container has `mariadb`
  and `mariadb-dump` and no `mysql` at all — and every database feature in this
  app asked it for `mysql`. Dumps, restores, snapshots, moves and the query log,
  all of them, on a service that is in the catalogue. The unit tests passed
  throughout, because they assert the argument *list* and the list was right for
  the program it named. The container picks now, by asking itself which of the
  two it has.

  **The Mongo query log recorded nothing on a fresh database, and was
  unreadable when it did.** Profiling in Mongo is per database and was applied
  to the databases that existed when the button was pressed — of which a
  freshly started container has none, so the switch turned on nothing and
  honestly reported itself as off, and the everyday case (an application
  creating its database on the first write) was missed entirely. `admin` now
  carries the session, so "on" is a fact the server holds, and every read
  switches profiling on for any database that has appeared since. What it
  recorded was then shown raw: five hundred characters of `$clusterTime`, a
  signature, a session id and a read preference per row, with the `find` and the
  `filter` somewhere in the middle. The driver's envelope is stripped from what
  is displayed as well as from what is counted as a shape — one list, so a key
  that is noise in one cannot still be noise in the other.

  `examples/querylog_probe.rs` is how the last three were found and is what
  keeps them found: it switches recording on against every database that is up,
  asks a question it can recognise coming back — including the N+1 shape the
  feature exists for — reads the session, and puts each database back the way it
  found it.

- **A performance layer for the directories a bind mount is slowest at** (I-1).

  Bind-mounted source on macOS and Windows is the single most common reason
  people leave a Docker-based workflow, and this section's own note said the
  remaining work was "a sync layer". It is not what was built, because measuring
  the question the *feature* has to answer changed the answer.

  `mount_bench` had established the general number — `:cached` and `:delegated`
  are inert, and bind→volume is 2–3× on metadata and writes.
  `examples/perf_layer_bench.rs` asks the narrower one, on an 8,000-file
  `vendor/`, with the source left bind-mounted where an editor can see it:

  ```text
                  bind    vendor in a volume    + storage/framework
    boot         1.47s    0.39s   (3.8x)        0.40s
    stat         0.42s    0.39s                 0.34s
    write        1.14s    1.21s   (nothing)     0.41s   (2.8x)
  ```

  Two of those rows decided the design. `vendor` buys the framework boot and
  does **nothing** for writes; `storage/framework` is what buys the writes. A
  single "make it faster" switch would hide that, and the directories it moved
  would be a guess about somebody's project — so the feature is a list of
  directories, each with its own switch, suggested from what the project
  actually has.

  **Mutagen is not bundled and no bidirectional sync was written.** The first is
  a second binary to package, sign and update for three platforms — the call
  this repository already made against dnsmasq, for the same reasons. The second
  is a hard problem with a long tail whose half-correct version does not fail
  loudly, it loses a file somebody wrote. Neither is needed here: nothing on the
  host writes these directories, so there is nothing to reconcile.

  The price is real and is on the row that charges it — an editor can no longer
  see `vendor/` — with a button that copies a snapshot back to the host for the
  index. The two cliffs are closed: a fresh named volume is **empty**, so
  enabling seeds it from the host copy first and refuses to save the setting if
  that fails; and deleting the volume is its own act, never the switch's side
  effect, because what is in there may be the only copy.

  Verified against the running engine rather than assumed: compose **appends** a
  second file's `volumes:` rather than replacing the first's, and a volume
  mounted at a sub-path shadows the bind — the container reads the volume, the
  host copy stays where it is, and what the container writes never reaches the
  host filesystem. That last one is the whole mechanism, and it is also why the
  seeding step exists.

- **Codex and Zed register the MCP server too — and every file now comes back
  the way it went in** (K-1).

  Two clients were missing and the module header said why. Both reasons were
  answered rather than worked around.

  **Codex** keeps its configuration in TOML, and editing TOML while preserving
  comments, key order and quoting style needs `toml_edit` — a dependency, which
  in this repository is a measured decision. Measured: `toml_edit` and
  `toml_writer` are **already in `Cargo.lock` and `NOTICE.md`** through Tauri's
  own graph, so the lock file gains two dependency edges and zero packages. The
  shape was not remembered either: a real `~/.codex/config.toml` holds
  `[mcp_servers.<name>]` with `command`, `args` and a nested `env` table, and
  OpenAI's own diagnostics reference documents the same block.

  **Zed** could not be verified against a running copy — it still cannot, so the
  shape comes from Zed's current published documentation: the flat
  `"context_servers": { "<name>": { "command": …, "args": [], "env": {} } }`,
  with no `source` key. Zed does not document *where* that file is and keeps
  some things under `~/.config/zed` and others under `~/Library/Application
  Support/Zed`, so both are looked for and whichever exists is written. Picking
  one would have been silently wrong on half the machines.

  **And running it against real files found an older fault.**
  `examples/agent_config_probe.rs` copies each installed client's actual
  configuration, registers the server, removes it again, and compares the result
  with the original **byte for byte**. The new TOML path was exact on the first
  run. Four of the JSON ones were not: `serde_json::Map` is a BTreeMap without
  `preserve_order`, so every file this app rewrote came back **alphabetised**,
  with its indentation replaced by two spaces. Nothing was lost and a 58 KB
  `~/.claude.json` still moved from end to end for an edit that added one entry
  — which is exactly what this module's first rule says it will not do.

  Both are closed. Key order is preserved (again with zero new packages —
  `indexmap` was already there) and a file is written back with its own
  indentation, tabs included, and its own trailing newline or lack of one. Five
  of five real files now round-trip byte for byte; the only difference left is
  the empty `mcpServers` map that removing deliberately leaves behind.

  One existing test had to change, and it is the interesting one: it asserted
  that a spec serialised through `Value` *tripped* the W-01 rule, because the
  sorted keys put `extensions` before `version` and broke the Bash parser. That
  ordering can no longer be produced. The rule is still tested — against the
  literal it used to produce — and the test now records that the path which
  caused it is gone.

- **Sail is importable, and MAMP and Valet finally are too** (L).

  The line in the plan read "MAMP, Sail, Valet" and was half wrong before any
  of this: MAMP and Valet were already written, and the **point-at-it-yourself
  path refused them**. `imports_scan_at` knew two of the five sources and the
  menu offered the same two, so somebody whose MAMP is not in `/Applications`
  was told it "is not a tool this app can read" — and for Valet and Sail, which
  no scan can ever find, that path is the only one there is.

  **Laravel Sail is a third shape.** XAMPP, Laragon and MAMP keep one directory
  of sites; Valet keeps none and is read from its own config; Sail is not an
  installation at all, but a composer package inside each project. What
  identifies one is a `docker-compose.yml` that names `laravel/sail` — a compose
  file alone is not evidence, since every second PHP project has one. So Sail is
  never offered as a well-known path (`~/Code` is a convention, not a fact about
  a machine) and is always "point at the folder", which may be the project or a
  directory holding several.

  It is also the only source that says **what a site needs**: its compose file
  lists the services, and those are mapped onto this app's own catalogue —
  `pgsql` to `postgres`, `mongodb` to `mongo`. Anything with no counterpart is
  left out rather than substituted, so an import can say what to switch on
  without describing something nobody wrote.

  `examples/import_probe.rs` builds each tool's real layout in a temp directory
  and runs the shipped scanner over it. It found the bug this class of work
  keeps producing: Sail's template is indented with **four** spaces, the service
  reader was written against a two-space rule copied from this app's own
  generated compose, and it read no services at all. The indentation is taken
  from the file now.

- **Four of the twelve small items, and eight that are not small** (M).

  **Per-project environment variables** (M-5), a **directory listing switch**
  (M-6) and **SSH agent forwarding** (M-10) share one file — `.stackvo/site.json`
  — because they are one kind of thing: a setting the generator cannot read from
  the manifest, since `project.schema.json` is `additionalProperties: false` and
  frozen. They do not share a destination, which is the part worth knowing: the
  variables and the agent are a compose overlay, the listing is a *generated
  server config*, so one save runs both paths.

  The variables are set on the container and are never written into the
  application's own `.env` — that file belongs to Laravel, Symfony and everything
  since. A value carrying a newline is refused rather than escaped: the overlay
  is YAML, where a newline ends a scalar and everything after it is read as
  configuration somebody else wrote.

  Agent forwarding is the one with a real edge. **The socket is not
  `$SSH_AUTH_SOCK`**: on macOS and Windows the daemon runs in a VM where the
  host's path means nothing, and Docker Desktop publishes the agent at a fixed
  path instead. Measured rather than trusted — a container with that path
  mounted answers `ssh-add -l` with *"The agent has no identities"*, which is
  the agent **replying**, where an unforwarded one says it could not open a
  connection at all. It is off by default and per project, because anything
  running in that container can sign with every key in the agent for as long as
  it is up, and that is the right trade for a `composer install` and the wrong
  one as a permanent setting.

  **Favourites** (M-1) pin a project to the top of the list. A preference, not a
  manifest key: a favourite is about the person, and `stackvo.json` is committed
  — writing one there would put "Ali likes this project" in a teammate's diff.
  It sorts rather than filters, so it cannot become a mode somebody is stuck in.

  The other eight were measured against the code and **the "cheap" label did not
  hold**, so they are recorded with what each actually costs rather than left
  looking like an afternoon: mail relay is SMTP credentials and a decision about
  where they live; a QR code needs an encoder, and hand-rolling Reed-Solomon
  with no decoder on this machine to check it against is the exact move this
  repository refuses; a landing page needs something that serves static files,
  which Traefik does not, so it is a new container in the stack; a third UI
  language is two thousand translated strings and not a code change; alternative
  surfaces are a product surface; framework passthrough commands wait on the
  B-4 decision `quickcmd.rs` already took deliberately; the Stripe listener
  needs an account and its CLI; and the OAuth item is not yet specified enough
  to build.

- **Custom routes** (E-4). A name can be pointed at something StackVo did not
  start — a dev server started by hand, a service in another tool, a staging
  host. Traefik and the certificate were already here; the only addresses
  either would serve were the ones this app generated.

  The whole difficulty is one string. `http://localhost:3000` is read *inside
  Traefik's container*, where `localhost` is Traefik: the config loads, the
  browser gets a 502, and nothing anywhere says why. So `localhost` and
  `127.0.0.1` are rewritten to `host.docker.internal` and the row says they
  were. Refusing them would be defensible and useless — it is the address
  people have, and correcting it is something this app knows how to do. The
  base compose file gains `host.docker.internal:host-gateway` on Traefik, since
  that name resolves by itself only on Docker Desktop.

  Everything the check changes is reported rather than quietly applied: a path
  is dropped, because a proxy target is an origin and Traefik ignores the rest;
  a name outside the suffix is flagged, because the wildcard certificate does
  not cover it. Each of those is otherwise a silent failure.

  Routers are named from the domain rather than a position — a router keyed by
  index changes identity when one above it is deleted, and Traefik reads that
  as one router removed and another added, dropping a live connection for a
  route nobody touched. Saving replaces the whole list and checks every row
  before writing any, so one bad row fails the save instead of writing half of
  it, and a duplicate domain is refused: two routers on one name resolve by
  whichever Traefik read last, which is a coin toss the user cannot see.

- **Writing a service package, not only installing one** (C-1). A Market
  toolbar button creates a package, checks one, and re-seals one after an edit.

  The obstacle was never the JSON. A manifest states the sha256 of every file
  it ships and the app verifies them on every read — deliberately, because the
  point of writing a hash down is to catch the change nobody announced. Which
  also means opening `compose.yml`, changing one line and saving leaves a
  package that refuses to load, complaining about bytes rather than about the
  line just typed. A person can compute those by hand once; nobody does it
  twice. So the surface is create, check, seal, and the files themselves are
  edited in whatever the author already uses — the same reasoning `quickcmd`
  gives for opening their terminal instead of shipping a worse one.

  Sealing is not a way past the validator. It recomputes hashes and *then*
  parses the manifest, runs the manifest's own checks and puts the fragment
  through the compose policy — and writes nothing if any of those fail, so the
  manifest keeps describing the old bytes and nothing downstream believes a
  broken package is intact. A tool that sealed a fragment the machine would
  refuse to run would be producing packages that install and cannot start.

  The policy check runs on the template with its `{{ … }}` stubbed, and that is
  half a check rather than a whole one, said plainly: the key rules —
  `privileged`, `userns_mode`, and every key nobody has considered — are caught
  at the moment somebody writes them, while the value rules ask whether a mount
  source is one the *renderer* produced, and those values do not exist until
  there is an instance. `render.rs` remains the check that decides whether this
  machine runs the thing; this one decides whether the author finds out from a
  user.

  The webview names a service and a version, never a directory: the path is
  built from the workspace root and checked components, the same
  handle-not-a-path rule `applog` and `quickcmd` state as their security model.

- **`policy.market.allowedSources`** (C-2). An organisation that runs its own
  package mirror can name it and the machine will fetch from nothing else.
  `docs/servis-market-mimarisi.md` §4.6 asks for exactly this as the enterprise
  half of the third-party gate.

  Two spellings because a source is two different things. An `https://` entry
  matches on the **host**, so one line naming a mirror allows every path on it
  — matching the whole string would refuse the same mirror over a trailing
  slash. A local path matches as a **directory prefix on a boundary**, so
  `/opt/stackvo` allows `/opt/stackvo/packages` and refuses `/opt/stackvo-evil`;
  a bare `starts_with` would let the second through, which is the same class of
  bug as reading `mysql:8.0`'s tag as a port.

  Enforced in `market::open`, the one place a source becomes something that can
  read bytes — including for a *remembered* source, so a policy that arrives
  after somebody has already fetched takes effect on the next refresh rather
  than on the next fresh install. Not a security boundary; ADR 0009's sentence
  holds here as everywhere in that file.

  A test caught a real bug in the first version: splitting on the first `://`
  read `/tmp/https://packages.example` as the host `packages.example`, so a
  directory anybody can create under `/tmp` would have satisfied a policy
  naming a mirror. The scheme has to actually be a scheme.

- **Lifecycle hooks** (B-3). `stackvo.json` may declare commands to run on
  `post-build`, `post-start` and `pre-stop`, and the project detail page shows
  them beside the manifest that declares them.

  This is the most dangerous feature in the application and the design is
  mostly about that, so it is stated plainly: a hook lives in a repository, so
  the sequence is clone, open, press Start, and commands somebody else wrote
  run. That is the shape of a malicious `postinstall`.

  Two kinds of step, because they are not the same risk. A step that runs
  **inside the project's container** needs no approval — that container already
  runs the repository's code, its entrypoint and its dependencies, so a
  repository able to run a command in it has gained nothing it did not already
  have. A step that runs **on the machine** is gated, and the gate is consent
  to a *digest of the exact commands*: approving means "I read these", and a
  hook that changes — or a commit that changes one — asks again. A per-project
  checkbox would have meant reviewing a repository once and then trusting
  whatever it grew afterwards, which is the property that makes supply-chain
  attacks work. The approval sends the digest back to the backend, which
  refuses it if the manifest moved in between, so it is a receipt for the list
  that was on screen.

  A step is an argv array and never a command string. Everything here spawns
  argv and never a shell — that rule *is* the security model in `runner.rs` and
  `quickcmd.rs` — and a hook taking shell text would be the one place a shell
  came back, holding text from a cloned repository. The cost is real: no `&&`,
  no pipes, no globs. A step that needs them is a script, and a script is one
  element — `["sh", "scripts/seed.sh"]` — which is the user choosing a shell
  and naming the file, not this app deciding every hook is shell.

  `pre-start` is deliberately not an event. There is no container to run in
  before a start, so its only possible occupant is a host step, and a slot that
  can only hold the dangerous kind is an invitation rather than a feature.

  A malformed hook is a warning, not an error: a typo in an optional
  convenience must not be why a project cannot be opened. Steps run in order
  and stop at the first failure, and a hook failure does **not** fail the start
  it hangs off — a container that started is started, and reporting otherwise
  would leave nobody able to tell which half broke.

  `policy.hooks` gives an administrator `enabled` and `allowHost`. Like
  `market.requireSignature`, neither can loosen anything: `allowHost` false
  stops host steps and true is already the default. In particular neither
  replaces consent — an administrator can forbid host steps fleet-wide and
  cannot approve them on a user's behalf, because approval is something a
  person does after reading a list and a file pushed to three hundred laptops
  has read nothing.

### Fixed

- **Seventeen WCAG failures, none of which any previous run could have
  reported** (§3 #25). The corrected axe pass above found them; each is a
  success criterion rather than a piece of advice. Closed tooltips exposed as
  unnamed `role="tooltip"` nodes; three `<nav>` landmarks with no names; no
  `<h1>` on any page; a table column header with no text; and nine contrast
  failures — the page subtitle at 3.62:1, secondary text at 2.67:1, every form
  label at 4.25:1, tile labels at 4.49:1 and footers at 3.42:1.

  The last of them is the interesting one. A status colour has two jobs — a
  fill (a dot, a chip) and text — and the palette is chosen for the first:
  `#4CAF50` reads as running at a glance and is 2.77:1 as a sentence.
  Darkening the palette would be wrong twice over, because a darker dot is a
  worse dot and because `colorblind` is Okabe-Ito, whose values are the entire
  reason somebody picks it. So `src/lib/contrast.js` derives a **text variant**
  of each status colour against the theme's own surface, moved only far enough
  to meet 4.5:1 and with the hue kept, and `global.css` points the text
  utilities at it — every existing `text-success` in the application became
  readable without one call site changing.

  Derived rather than hand-picked because three palettes × four roles × two
  themes is twenty-four values to keep right by hand, and the surface under all
  of them is a setting. Being derived, it is also testable, and the test earned
  its place immediately: `readable` mixed a colour and then asked for the
  contrast of the mixture, and `parse` took strings only — so every one of
  those questions answered `null`, which compares false against everything. The
  loop ran its hundred steps, improved nothing, and returned its input. The
  only symptom was axe reporting exactly the ratio it had before the fix.

- **The hosts file asked for a password it did not need** (§3 #35). `apply`
  elevated unconditionally — a polkit dialog or a UAC prompt even where the
  file was already this process's to write: a root shell, a CI runner, a
  machine whose administrator made `/etc/hosts` group-writable on purpose. A
  password prompt that cannot change the outcome is one that teaches people to
  type their password at anything that asks, which is the opposite of what a
  single elevation point is for. It now writes directly when it can and
  elevates only when it cannot.

  Written in place rather than through `atomic::write`, which is the one place
  in this application that rule is wrong: an atomic write replaces the inode,
  so `/etc/hosts` would come back carrying the mode and owner of whatever this
  process created. A hosts file that lands as `0600 developer:staff` is one the
  next tool cannot read, and it does not announce itself.

  That is also what made the row testable. `hosts_path()` now honours
  `STACKVO_HOSTS_PATH` — the seam `STACKVO_ROOT` already is — and
  `tests/hosts_roundtrip.rs` runs plan, write, read-back, idempotence and
  removal against a temporary file, in its own process so no parallel test can
  be handed the fixture. CI already runs `cargo test` on three operating
  systems, so this closes the hosts half of "the privilege paths never ran on
  Windows or Linux". The half that remains is the elevation dialog itself,
  which needs a human, and the row says so rather than claiming the item.

- **`list_projects` has no cache, and now there is a measurement saying it
  should not** (§3 #27). The row stood at half done with "no cache" as the gap,
  which is the kind of gap that is obviously worth closing until somebody looks
  at the numbers. `examples/list_bench.rs` looks at them, and splits the call
  into the half that grows with the workspace and the half that does not:

  | | 1 project | 50 projects |
  | --- | --- | --- |
  | the whole call | 26.7 ms | 38.1 ms |
  | of which the engine | 24.6 ms | 34.4 ms |
  | the tree, by difference | 2.1 ms | 3.7 ms |
  | per project | 2.09 ms | **0.07 ms** |

  The half a cache would help is free: fifty projects cost under four
  milliseconds of directory scanning and manifest reading. Everything else is
  one `stackvo_containers()` call — a fixed cost — and what that call produces
  is `running`, the one field on the row that must never be stale. It is what
  the start, stop, rebuild and terminal buttons are enabled by. So the only
  cache worth having is the one that would be wrong, and the reasoning now
  lives on the function with the numbers beside it rather than in a queue.

- **Right-to-left turned the app root round and left the rest of the window
  behind** (§3 #24). The row read "no `rtl` configuration in `vuetify.js`",
  which was wrong twice: the configuration is in `appearance.js`, where it
  belongs, and the real gap was somewhere else again — three places, none of
  them visible without an engine to lay the window out.

  `<html>` carried no `dir`, so the document's own direction never changed. The
  overlay container is a sibling of `#app`, so **every dialog, side sheet, menu
  and tooltip stayed left-to-right inside a mirrored window** — the same
  structural fact that had been hiding half the axe run. And both navigation
  drawers were pinned to `location="left"`, a physical side, so the primary
  navigation sat on the wrong edge for a right-to-left reader while its own
  contents mirrored.

  Setting `dir` on the document settles the first two at once, because
  `direction` inherits and the overlay container is a child of `body`. The
  drawers now say `start` and the side sheet `end`. Twenty-two physical CSS
  properties across fourteen files became logical ones —
  `margin-left: auto` is the "push the rest to the far side" idiom, and in a
  mirrored window the far side is the other one. `tests/e2e/rtl.e2e.js`
  measures the result with boxes; the jsdom test that already held the wiring
  now also holds the attribute.

- **The query log's Postgres half had never run, and three separate things had
  to be true before it could** (F-1). §2 carried the row as half-done with the
  reason "this workspace has no Postgres installed; the probe skips it and says
  so". A Postgres was installed. The probe stopped skipping, and what it found
  was not one gap but three, each of which alone was enough to make the pane
  show "recording" over an empty list — the worst shape a diagnostic can fail
  in, because it looks exactly like a page that ran no queries.

  **One: nothing could log in.** `db::settings` resolved the user and password
  out of `.env`, and after ADR 0016 a workspace is installed from packages — a
  package stores `USER` on the instance and renders it into the compose file, so
  `.env` holds nothing for it. Every caller fell through to a hardcoded default,
  which for Postgres is `postgres`: an account the container does not have,
  because the image created the one the manifest named. `psql -U postgres`
  against this workspace answers `FATAL: role "postgres" does not exist`, and
  that was the reply the query log, dump, restore and snapshot all got. It now
  resolves through the manifest's `connection` block — `userSetting`,
  `passwordSetting`, `databaseSetting` — which is the package contract saying
  which setting is the login, then the keystore, then `.env`, then the manifest
  default. `.env` sits in the **middle** deliberately: putting the package first
  would hand a migrated workspace's dump the manifest's default password instead
  of the real one in `.env`, which is the same bug facing the other way.

  `settings_for_instance` was reading the instance's settings map with the
  `.env` key — `instance.settings.get("SERVICE_POSTGRES_USER")` for a map whose
  key is `USER` — so it matched nothing on every workspace and was `settings`
  with a different container name. Same fix, same place.

  **Two: it was reading the wrong file.** `read` pulled the container's log
  stream, which is correct for a stock `postgres` image and wrong for every
  Postgres this app installs: the packaged `postgresql.conf` sets
  `logging_collector = on`, and a collector takes stderr out of the stream and
  writes it to a file under the data directory. `docker logs` on a four-hour-old
  container held the startup banner and one line saying the log went elsewhere.
  It now asks the server — `pg_current_logfile('stderr')` — and reads the file
  from inside the container, falling back to the stream when there is no
  collector. That answers log rotation too, which a hardcoded path would not.

  **Three: the format was pinned by half.** `log_statement` and
  `log_line_prefix` were pinned, `log_destination` was not — so a workspace
  configured for `csvlog` or `jsonlog` writes the same statements in a shape the
  parser cannot read, and unlike a wrong prefix that is invisible in everything
  the pane shows. Now pinned with the other two and reset with them.

  The `%n` epoch escape was the one thing here that looked like a version risk,
  so it was measured rather than assumed: `postgres:12` — the oldest version the
  catalogue offers — and `postgres:14` both write it. An escape a server did not
  understand would be dropped silently, which is the same empty-list failure
  again.

  Every statement this module sends Postgres now carries a
  `/* stackvo:querylog */` comment and is filtered on that, rather than on a
  keyword list that had to grow with each new statement. That list also matched
  every `SHOW` — including the reader's own, hidden in order to conceal the
  tool's one.

  Verified by `examples/querylog_probe.rs` against the live stack rather than
  against fixtures: `mysql:9.7`, `mariadb:12.3`, `postgres:14` and `mongo:8.0`
  each switched on, asked an N+1 of five, and reported the group.

- **"Start again" did nothing on Postgres, and was hidden so it would not
  show.** The log belongs to the server and this app must not rewrite it — true,
  and the wrong conclusion was drawn from it. "Cannot delete" is not "cannot
  start again". `clear` now writes a **watermark** statement, and the reader
  drops everything above it on the next read: the person pressing the button
  gets what they meant, and the server's log is left exactly as it was. Stopping
  writes it too, so a session no longer opens on the previous one's statements.

  The probe checks it, because this is the one operation whose failure is
  invisible from the caller's side — a `clear` that returns `Ok(())` having done
  nothing looks identical to one that worked.

  What the watermark cannot do is take the statements off the server's disk, and
  recording on Postgres puts every one of them there. The pane says so, beside
  the switch, while it is recording — the moment somebody decides whether to
  record against a database holding real rows.

- **Every database reported itself stopped while it was running.**
  `db::targets` asked the engine about the **service** name — `stackvo-mysql` —
  but the container comes from the instance table and is called
  `stackvo-mysql-9-7`. There has been no `stackvo-mysql` since instances landed,
  so the lookup missed every time and `running` was false for all four engines
  with all four up. That field is what the dump, restore, snapshot and query-log
  controls are disabled by, so the feature was unreachable on a working stack.
  `db::instances`, immediately above it, had always asked by container name.

  Found by running the new CLI rather than by reading: `stackvo db` printed four
  engines as down while `docker ps` listed them up. Nothing in the unit tests
  could have caught it — they check the argument list, and the argument list was
  correct for the container it named.

- **The contract gate's service suite checked a directory ADR 0016 deleted.**
  Suite C was built entirely around `skeleton/core/templates/services/` — one
  directory per service — and asked three things of it. That directory is gone
  and services are packages fetched at run time, so the checks reported
  twenty-five errors about files somebody deliberately removed, on every run.
  A gate that is always red is a gate nobody reads.

  What replaces it runs in the other direction and is the check that still has
  a subject: every `SERVICE_*_ENABLE` switch must name a service the catalogue
  knows. The obvious direction — every declared service has a switch — is wrong
  by `env.schema.json`'s own account of itself, which says the list is "the
  vocabulary, not an inventory" and names solr and clickhouse as entries with
  nothing behind them. Read this way it still catches a real defect: a
  `SERVICE_MYQSL_ENABLE` is a switch that will never turn anything on and
  nothing else in the toolchain would notice.

- **`stackvo.local.json`, this machine's overrides** (B-2). A committed
  manifest is the whole of what makes a checkout reproducible, and is exactly
  why there was nowhere to say "on *this* machine, PHP 8.3, because I am
  chasing a bug in it". Now there is a file beside it that is not committed,
  and the project detail page has an editor for it below the manifest editor.

  Merged as JSON before validation rather than as fields afterwards. That is
  what lets an override be *checked*: a local file saying `"aliases": ["not a
  hostname"]` is reported by the same rule the committed file would be, because
  validation happens on the way in and a post-merge value would arrive after
  it. The merge nests one level, so a file setting only `php.version` keeps
  `php.extensions` — a whole-value overlay would leave the project building
  fine and unable to reach its database. Arrays are replaced whole, because
  "also this" and "only this" are both defensible readings of a one-item list
  and neither should be guessed at.

  `name` and `runtime` are refused, and named when they are. `name` keys the
  container and the image; `runtime` is not a property of a machine — "PHP
  here, Go on my laptop" describes two different programs. A refused key is a
  warning when the file is read, so a project whose local file predates a
  change still runs, and an error when it is written, because at that moment
  somebody is typing it and can fix it.

  The guard is at the write, not at each caller. `manifest::read` now returns
  the effective manifest, because that is what all twenty-odd readers that run,
  render or inspect a project want, and making the overlay opt-in would have
  been twenty-odd chances to forget it. The five callers that read in order to
  write back ask for `read_committed` — and forgetting *that* is not silent:
  `manifest::write` refuses a manifest carrying overrides. So the mistake that
  would land one developer's settings in everybody's clone fails loudly, and
  the mistake that costs nothing is the default.

  Whether the file stays out of a commit is git's business and this app does
  not write ignore rules, so it asks `git check-ignore` and reports the answer
  — three states, not two. "git had no answer" (no git, not a repository) is
  not a warning: a directory that is not under version control has nothing to
  leak into anybody's clone.

- **A command palette** (A-2). ⌘K on a Mac, Ctrl+K elsewhere, and a button in
  the toolbar with those keys printed on it — a shortcut nobody is told about
  is not a second way into the app, which was the whole of what the gap said.
  It goes to any of the seven pages, opens any project, and starts, stops,
  restarts or builds one; it drives the three stack-wide actions and the theme,
  the language and the new-project drawer.

  The list is built from the stores every time it opens rather than registered
  by pages as they mount. A registry sounds tidier and is wrong here: a command
  a page registered outlives the mount that can run it, so leaving the page
  leaves an entry whose handler closes over a component that is gone — and
  nothing about that failure is visible, because the row is listed and the
  click does nothing.

  Nothing is offered that would fail. A stopped project has no Stop, one that
  was never built has only Build, and a project whose domain has no hosts entry
  has no "open in the browser" — the same rules the project rail's menu already
  applies. The one exception is deliberate: the stack-wide actions stay in the
  list, greyed, when the engine is down, because absent would read as a missing
  feature where greyed reads as the state it is.

  The matcher is substring, not fuzzy. A subsequence matcher — the kind where
  `sts` finds `SeTtingS` — would also return "Start all containers" and "Stop
  all containers" for that query, and a score nobody can predict would then
  decide which came first. Ranking is by where the hit lands: a label starting
  with the query, then one containing it, then a match that was only in the
  section name or the hint.

  Not registered as an operating-system shortcut, though Tauri can. That would
  take ⌘K away from every other application on the machine for a palette that
  can only act on the window in front of you. It is a `keydown` on `window`,
  and it does fire from inside text fields, because a shortcut that stopped
  working because the cursor was in the search box would be the one case a user
  reaches for it hardest. It is off entirely while a first-run gate is up, for
  the reason the toolbar and both rails are hidden there: every command acts on
  a workspace or a daemon that is the thing missing.

  Keyboard-driven, so focus never leaves the input and the rows are named to a
  screen reader through `aria-activedescendant` on a `role="listbox"` — the
  pattern that exists for exactly this. The rows are still buttons: they are
  clickable too, and a div with a click handler is not reachable, not announced
  and not a control.

- **A call tree for a profile** (F-3). The cost table answers where the time
  went; it cannot answer what called that, and the parser had been reading
  caller→callee edges all along — it needs them to attribute an inclusive cost
  — and discarding the caller. It is deliberately called a call tree and not a
  flame graph: a flame graph is built from sampled stacks, so a box's width is
  how often that exact path appeared, and cachegrind holds no stacks. A branch
  here means "reaching B through A cost this much in total", which is the
  question people bring to a profile and is answerable from the file; "this
  path was taken this often" is not, and no arrangement of the edges recovers
  it. Recursion stops at the first repeat on the path and the frame is kept and
  marked, because a recursive call is a fact about the program rather than an
  edge to hide. Drawn as buttons rather than a canvas, so the rows take focus,
  carry `aria-expanded`, and can be found with the browser's own search.

- The query log covers all four databases the stack runs: Mongo joined by the
  same route Postgres did. Its note said the profiler is per-database and
  writes a capped collection — both true, and neither a reason it cannot be
  done; it is a loop. A Mongo query is a document, so the shape is the
  command's keys with the values thrown away (`{find:"users",filter:{_id:3}}`
  becomes `find users filter{_id}`), keys sorted because a driver may
  serialise a document in any order, and per-connection bookkeeping dropped
  because leaving it in would make every statement its own shape. One limit is
  measured and stated: profiling is set on the databases that exist when the
  switch is pressed, so a server with none reports itself off rather than
  pretending.
- The query log now covers Postgres as well. The first version left it out and
  wrote the reason down — its log is a stream whose format `log_line_prefix`
  changes — which was true and incomplete: this app can set that prefix itself.
  `ALTER SYSTEM` plus `pg_reload_conf()` takes effect without a restart, `%n`
  stamps each line with a Unix epoch so a Postgres statement lands on the same
  axis as a MySQL one and as a dump, and the official image writes to the
  container's stream rather than a file. Turning it off resets both settings,
  because `ALTER SYSTEM` writes a file that survives a restart and leaving a
  changed setting behind is no better than leaving the log on.
- The timeline carries mail as well, so a page load reads as what the code
  thought, what it asked the database, and what it sent. The two catchers
  disagree about the date format — Mailpit answers RFC 3339, MailHog hands back
  the message's own RFC 2822 header — and both are parsed. A date in neither
  spelling is left off the axis rather than placed at the epoch: on a timeline
  1970 is not a missing value, it is a wrong one that drags everything else into
  a corner.
- **A request timeline** (F-2), which puts what the code thought it had and what
  it actually asked the database for on one axis. Dumps carry the request they
  happened in, so several from one page load group together and the group is
  named. Queries do not, and that is stated rather than smoothed over: nothing
  in a database's general log says which HTTP request caused a statement, and
  inferring it from what sits either side would be wrong the first time two
  requests overlap — silently. Attributing a query to a request needs the
  application to say so, which is code inside somebody's project and the thing
  this was built to avoid needing.

- **Query log and N+1 detection** (F-1, which §2 calls the largest product gap).
  That row said it needed a collector inside the container; for MySQL and
  MariaDB it does not, and this is why. Both keep a general query log that can
  be pointed at a table and switched on with two `SET GLOBAL` statements at
  runtime — no agent, no image change, no restart, no code in the application.
  Statements are reduced to a *shape* (`WHERE id = 1` and `WHERE id = 4711` are
  one question) and a shape seen three or more times is reported, which is the
  N+1 pattern. It is a session rather than a feed: the log is unsampled and
  costs write throughput, so you switch it on, reload the page you are
  investigating, look, and switch it off — and switching it off clears what was
  collected, because the log holds statement text. Postgres and Mongo answer
  "not this kind" rather than an error; their logs are a stream in a
  configurable format and a per-database profiler, which are real work of a
  different shape.

- The front end is now tested in a real browser engine (Playwright), not only in
  jsdom, and axe runs over four whole pages there. jsdom has no layout, so no
  test in this repository could establish that a control is visible, has a size,
  or can be reached by keyboard — which is why the two bugs of that kind that
  shipped were both found by a person. `tauri-driver` is deliberately not used:
  Tauri does not support it on macOS, and a suite its authors cannot run is one
  that rots until CI is the only thing that has seen it pass. This drives the
  webview and replaces one global, `window.__TAURI_INTERNALS__.invoke`, which is
  the process boundary at the same seam ADR 0001 draws it.
- Its first run found six real defects, all invisible to the existing suites:
  six anchors that carried a click handler and no `href` — so the domain in
  every row of the projects table was unreachable by keyboard and announced as
  plain text; seven action buttons in that table with no accessible name, now
  carrying the project's name because twenty rows all saying "Delete" tell a
  screen reader user nothing; two clickable icons that were both `role="button"`
  and `aria-hidden`; and a dashboard body that scrolled but could not take
  focus. A source-reading guard was added for the first class, verified by
  breaking it on purpose.
- Solr and ClickHouse in the service catalogue (27 services, 107 versions), and
  they are the first two that were never templates in this binary — every other
  entry arrived by migration, these arrived as packages. Both were measured
  rather than written from documentation: ClickHouse's healthcheck has to work
  whatever credentials are set, because a `health` block cannot read settings,
  and it does because the `default` user stays password-free even when
  `CLICKHOUSE_USER` is given. ClickHouse also refuses to start on a container's
  default 1024 file descriptors, so the fragment raises them.
- Bun and Deno as project runtimes, on the same shared template the other four
  lang runtimes use. They are not a flavour of `node`: both read the same
  `package.json`, but each is built from its own image with its own verbs, and
  folding them in would have meant one block whose meaning depended on a
  sibling key. Deno is pinned to a full patch version because `denoland/deno`
  publishes no major or minor tag — checked against the registry, and held by a
  test so a later tidy-up cannot shorten it into an image that does not exist.
  Detection treats them differently on purpose: `deno.json` is exclusive and
  decides, while a Bun lockfile beside an npm one decides nothing and falls
  through to the answer that was right before Bun was an option.
- `node.package_manager` — name npm, Yarn or pnpm and the image enables
  Corepack, which is what makes a `packageManager` field in `package.json` pin
  anything at all; without it that field is a comment. Absent is not `npm`: a
  project that names nothing builds the image it always has, byte for byte,
  which is what stops this from silently changing every existing Node project.
- A project can answer on a name other devices on the same network resolve, so
  the site opens on a real phone without editing a file on that phone. The name
  is `<project>.<address-with-dashes>.sslip.io`, which is resolved
  arithmetically out of the name itself — nothing is registered, nothing is
  published, and no traffic leaves the network. It is derived and never stored:
  a DHCP lease expires and a written-down name then points at whichever machine
  took the address, so what is stored is the intent and the app reports when
  what it rendered no longer matches where it is. Only a private address is
  offered; a public one is refused rather than publishing a development site
  under a name anybody can resolve. The visiting browser will warn about the
  certificate — it is the local CA's and that device has never seen it — and
  the pane says so beside the address, because on a phone that warning and a
  name that does not resolve look identical and only one of them is expected.
- A service's connection string can be opened in a desktop database client,
  not only copied. Which clients are offered is read from the applications
  themselves rather than from a list in this repository — on macOS from each
  bundle's own `CFBundleURLTypes`. That is not a detail: Redis Insight
  registers exactly one URL scheme, `redisinsight`, so a table written from the
  name would have offered it for `redis://` and produced a button that launches
  an application which then ignores the address it was handed. The host address
  is the one that goes across, always; the container address is a name on a
  Docker network that no client on this desktop can resolve.
- `examples/mount_bench.rs`, which measures what a bind mount costs by putting
  the same real Laravel project in front of the same PHP four ways — plain
  bind, `:cached`, `:delegated`, and a named volume — and timing an install, a
  stat pass over the tree, a small-file write loop and the framework's own test
  suite. The named volume is not a shipping option; it is the ceiling, because
  a synchronising layer can at best close the distance between it and a bind.
  The program also reports how long an empty container takes in each mode, and
  refuses to let the comparison be read when that constant moves — the machine
  being busy is otherwise indistinguishable from a mount being slow. On an
  Apple-silicon Docker Desktop the answer is that the consistency flags are
  inert — `:cached` and `:delegated` cannot be told apart from a plain bind —
  while the distance to a named volume is 2–3× on metadata and small writes.
  Nothing in the app changes yet; the number is what the decision needs.
- Project details as a page (`/projects/:name`) rather than a dialog, with
  indicator, configuration and container sections, plus the manifest editor and
  Dockerfile preview the dialog carried.
- A diagnostic log on disk, rotated daily and capped at seven files. Secret
  values in subprocess output are masked before they are written. Settings
  points at the folder.
- One-operation-per-subject locking at the IPC boundary, so the tray, a second
  view and a shortcut cannot start the same thing twice.
- Supply-chain gates: `cargo-deny`, Dependabot, and `npm audit` in CI.
- Front-end tests (vitest) and linting (ESLint + Prettier), both wired into CI.

### Removed

- `connect.rs`'s compiled-in table of twenty-five connection shapes, and the
  `.env` branch of `connect::of` that read it. A workspace with no instance
  table can no longer render a stack, so it has no running service to ask
  about — that branch was unreachable, and it carried a second copy of what
  every package manifest declares in its own `connection` block.
- `examples/build_packages.rs`, the one-shot tool that turned the embedded
  templates into packages. It still compiled and could no longer run: its input
  was the template directory removed with the migration gate, so every read
  would have returned nothing. A tool that cannot work is worse than no tool,
  because it looks like one.

### Changed

- **Turning Xdebug off no longer costs the next `on` a rebuild.** The extension
  being compiled in and step debugging being switched on were the same fact,
  so switching off removed the extension from the image — minutes for something
  that should take seconds. They are separate now (`php.xdebug` in the
  manifest): the extension goes in the first time and stays, and a toggle moves
  one environment variable. Measured before it was designed, because the answer
  depended on the number: an image carrying Xdebug at `mode=off` runs at the
  speed of one without it (0.009s against 0.009s), while `mode=debug` costs
  about 6.7×. The same measurement retired an assumption:
  `start_with_request=trigger` does *not* reduce that cost — it is
  indistinguishable from `default`, because the hook is loaded whenever the mode
  is on and the trigger only decides whether to dial the debugger. The pane says
  which toggle rebuilds and which restarts, because otherwise the second one
  being far faster reads as a fault.

- **A workspace that still keeps its services in `.env` is migrated behind a
  gate, and the old path is gone.** StackVo rendered services two ways — from
  `.env` and twenty-five templates compiled into the binary, or from
  `instances.json` and the package catalogue — so that existing installs kept
  working while the second was built. Keeping both is what left the catalogue
  with two lists that disagreed: adding Solr and ClickHouse as packages made a
  project declaring `services: ["solr"]` get a correct declaration met with a
  warning that could not be fixed. `MigrationGate` is the fourth of the
  first-run screens; it shows the plan before writing anything, backs `.env` up
  to `.env.pre-market.bak`, and can be left — the app then opens without
  services, which is still a reverse proxy, a certificate authority and a
  project runner. What leaving it does not do is bring the old stack back.
  A workspace reaching the renderer without a table is refused by name rather
  than rendered into an empty stack.

- The unmanaged code on the Projects page — folders under `projects/` with no
  `stackvo.json`, and the XAMPP and Laragon folder pickers — moved off the page
  and into an overflow menu beside Refresh. The button carries a count, because
  those folders are invisible everywhere else in the app and that is exactly
  why they accumulate.
- `.env` and `stackvo.json` are written atomically, and `.env` patches are
  serialised so two callers cannot lose each other's change.
- The Rust toolchain is pinned rather than tracking `stable`.
- The bundle ships one font format instead of four: 5.2 MB of assets down to
  2.1 MB.

### Fixed

- Dump, restore, snapshot and the new query log all ran against a container
  that does not exist on a migrated machine. `db.rs` built the name as
  `stackvo-<service>`, which was right while every service was single-instance
  and named after itself and has not been right since the instance table
  arrived — an instance is `stackvo-mysql-9-7`, and after the migration gate
  there is no other kind of workspace. The same mistake `list_services` was
  fixed for, in a quieter and more expensive place.

- A container that passes or fails its own healthcheck now reaches the UI.
  Docker reports a health verdict as its own event rather than as a state
  change, so it was dropped with `exec_start` and friends — a service that had
  genuinely become healthy kept the hourglass it was given at boot until
  something else made the app refetch, which is why leaving the page and coming
  back "fixed" it.
- Project and service names are validated at every entry point. A name
  containing `..` previously produced a path outside the workspace, which
  mattered most in `project_delete`, where the next call is `remove_dir_all`.
- A service id the contract does not define is refused instead of writing a
  `SERVICE_<JUNK>_ENABLE` key into the user's `.env`.
- Per-container stats history no longer keeps series for containers that are
  gone.

### Security

- Releases cannot be published without an updater public key and signing
  secret; the workflow fails in preflight rather than shipping artifacts the
  updater will refuse.
