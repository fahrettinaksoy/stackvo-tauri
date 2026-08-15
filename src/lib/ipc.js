import { invoke } from '@tauri-apps/api/core';

/**
 * The one file that changes when moving off HTTP.
 *
 * The web UI's `lib/api.js` wrapped axios and had to undo two conventions: the
 * `{ success, data }` envelope (unwrap `.data`), and the case where HTTP 200
 * still meant failure (`success: false`). Neither exists over IPC — a Rust
 * command returns its payload on Ok and rejects on Err — so this file is
 * mostly about turning the Rust error struct back into a real `Error` that
 * `catch` blocks can use unchanged.
 *
 * Everything downstream (Pinia stores, views) keeps its existing shape.
 */

/** Error carrying the contract's machine-readable code, so callers can branch. */
export class StackvoError extends Error {
  constructor({ code, message, hint, hintKey, details }) {
    super(message || 'Unknown error');
    this.name = 'StackvoError';
    this.code = code || 'UNKNOWN';
    this.hint = hint;
    // The locale key for `hint`, from the catalogue in src-tauri/src/hints.rs.
    // Dropping it here would have made the whole hint translation a no-op in
    // production while every test that built a plain object still passed —
    // this class is the only path a real error takes.
    this.hintKey = hintKey;
    this.details = details;
  }

  /** Docker is not running — the state the web UI could never report. */
  get isEngineDown() {
    return this.code === 'ENGINE_UNREACHABLE';
  }

  /** No StackVo directory selected yet. */
  get needsWorkspace() {
    return this.code === 'NO_WORKSPACE';
  }
}

/**
 * Call a Rust command. Rejects with a StackvoError.
 * @param {string} command snake_case name from contracts/ipc.json
 * @param {object} [args]
 */
export async function call(command, args = {}) {
  try {
    return await invoke(command, args);
  } catch (raw) {
    // Rust's Error serialises to { code, message, hint?, hintKey?, details? }.
    // A panic or a missing command arrives as a bare string instead.
    if (raw && typeof raw === 'object' && 'code' in raw) {
      throw new StackvoError(raw);
    }
    throw new StackvoError({ code: 'UNKNOWN', message: String(raw) });
  }
}

/**
 * A list from the boundary, or an empty one.
 *
 * Nothing checks that a Rust command still returns the shape the frontend
 * believes it returns — `ipc.js` is hand-written and stays that way until types
 * are generated. A command that answers `null` or a bare object used to be
 * assigned straight into a `ref`, and the next `computed` read `.filter` or
 * iterated it and threw. In a desktop app that is not a missing list; the
 * render throws and the window is blank.
 *
 * Found three times before it was made one function: the inventory store, then
 * `LogView`, then `DumpView`. Anything downstream that assigns a boundary reply
 * into a list belongs here too.
 */
export function asList(value) {
  return Array.isArray(value) ? value : [];
}

// The command surface, one thin wrapper each. Keeping them enumerated here —
// rather than letting views call `call('whatever')` — means the contract is
// greppable from the frontend and a typo fails at import, not at runtime.
export const api = {
  workspaceGet: () => call('workspace_get'),
  workspaceSet: (path) => call('workspace_set', { path }),
  /**
   * Record that the first-run setup finished.
   *
   * Written by the screen that runs it, after its last step — so a setup that
   * failed part way, or was skipped past, is offered again next launch.
   */
  bootstrapComplete: () => call('bootstrap_complete'),

  engineStatus: () => call('engine_status'),
  /** Everything that must be true before the app can work. See `preflight.rs`. */
  preflight: () => call('preflight'),
  preflightFix: (id) => call('preflight_fix', { id }),
  engineStart: () => call('engine_start'),
  /** The full diagnosis with named culprits. See `doctor.rs`. */
  doctor: () => call('doctor'),
  /** Dangling images by default; unused volumes only when explicitly asked. */
  /** `buildCache` — 'keep' | 'dangling' | 'all'. 'all' reclaims the layers
   *  every project image shares, so each one rebuilds from scratch next time. */
  dockerPrune: (images = true, volumes = false, buildCache = 'keep') =>
    call('docker_prune', { images, volumes, buildCache }),

  hostStats: () => call('host_stats'),
  dockerSystemResources: () => call('docker_system_resources'),
  /** Which stack member holds the bytes: per-container image + writable layer. */
  dockerDiskUsage: () => call('docker_disk_usage'),

  projectsList: () => call('projects_list'),
  servicesList: () => call('services_list'),

  catalogGet: () => call('catalog_get'),
  serverConfigGet: (server) => call('server_config_get', { server }),
  serverConfigSet: (server, content) => call('server_config_set', { server, content }),

  /**
   * The shipped templates, and which of them this workspace has taken over.
   *
   * A file under `core/` exists only because somebody chose to override it —
   * installing writes none — so `overridden` is simply whether it is there.
   */
  templatesList: () => call('templates_list'),
  /** Copy the shipped file into the workspace; resolves to its absolute path. */
  templateOverride: (path) => call('template_override', { path }),
  /** Delete the workspace's copy. The version in the binary takes over again. */
  templateRevert: (path) => call('template_revert', { path }),
  envGet: () => call('env_get'),
  envDefaults: () => call('env_defaults'),
  // `envReveal` was here and is not any more: its one caller was the services
  // detail sheet, which now asks `serviceReveal` so the workspace's own model
  // decides where the value lives. The Rust command stays — `service_reveal`
  // is what calls it on the `.env` path.

  // --- Phase 2: mutations ---------------------------------------------------
  projectStart: (name) => call('project_start', { name }),
  projectStop: (name) => call('project_stop', { name }),
  projectRestart: (name) => call('project_restart', { name }),
  /** Resolves with an operationId as soon as the build starts, not when it ends. */
  projectBuild: (name, noCache = false) => call('project_build', { name, noCache }),

  // Dispatches to the instance table or to `.env`, whichever this workspace
  // keeps its services in — the caller does not have to know which.
  serviceReveal: (service, key) => call('service_reveal', { service, key }),

  // The market and the instance table. Neither touches Docker: enabling an
  // instance needs the generate path to render from the table, and that swap is
  // a later phase — a button that wrote a row nothing renders would be worse
  // than no button.
  marketStatus: () => call('market_status'),
  marketRefresh: (location) => call('market_refresh', { location }),
  // C-1. Authoring a package rather than only installing one. The workspace
  // owns the path; these name a service and a version.
  packageScaffold: (category, service, version, image) =>
    call('package_scaffold', { category, service, version, image }),
  packageLint: (category, service, version) => call('package_lint', { category, service, version }),
  packageSeal: (category, service, version) => call('package_seal', { category, service, version }),

  marketCatalog: () => call('market_catalog'),
  marketInstall: (service, version) => call('market_install', { service, version }),
  marketUninstall: (service, version) => call('market_uninstall', { service, version }),

  // Try a source and say what it is, writing nothing. A "test" that cached
  // what it found would be the same act as a refresh with a different word on
  // the button.
  marketProbe: (location) => call('market_probe', { location }),

  // The one migration that touches a workspace somebody is already using, so
  // the preview is a separate call rather than a flag on the apply: what it
  // would do has to be readable before it is agreed to.
  handoverPreview: () => call('handover_preview'),
  handoverApply: () => call('handover_apply'),
  instanceList: () => call('instance_list'),
  // What creating one would produce — the manifest's settings with their
  // defaults, and the ports the allocator would pick — so the form can be shown
  // before anything is written. Nothing is reserved by asking.
  instancePlan: (service, version) => call('instance_plan', { service, version }),
  // `settings` and `ports` are what that form collected. Both null creates with
  // the package's own defaults, which is what the button did before the form
  // existed.
  instanceCreate: (service, version, settings = null, ports = null) =>
    call('instance_create', { service, version, settings, ports }),
  instanceRemove: (id) => call('instance_remove', { id }),
  instancePromote: (id) => call('instance_promote', { id }),
  instanceEnable: (id) => call('instance_enable', { id }),
  instanceDisable: (id) => call('instance_disable', { id }),
  instanceStart: (id) => call('instance_start', { id }),
  instanceStop: (id) => call('instance_stop', { id }),
  instanceRestart: (id) => call('instance_restart', { id }),

  // What an instance is configured with, from its manifest — the settings a
  // package declares, not `.env` keys. Applying recreates the container,
  // because a running one keeps the environment it was created with.
  instanceSettings: (id) => call('instance_settings', { id }),
  instanceReveal: (id, key) => call('instance_reveal', { id, key }),
  // `ports` travels with the settings rather than through a command of its own:
  // both need the container stopped and recreated, and two commands would do
  // that twice for one press of one button.
  instanceApplySettings: (id, patch, ports = null) =>
    call('instance_apply_settings', { id, patch, ports }),

  containerInspect: (name) => call('container_inspect', { name }),
  containerStats: (name) => call('container_stats', { name }),
  containerLogsOpen: (name, tail = 200, follow = true) =>
    call('container_logs_open', { name, tail, follow }),
  containerLogsClose: (streamId) => call('container_logs_close', { streamId }),

  // The files a project writes, which its container's stdout never carries: a
  // Laravel exception, an nginx 502, a queue worker that died. Read from the
  // host, so they still work when the container does not.
  appLogs: (name) => call('app_logs', { name }),
  /** `id` is an opaque handle from appLogs, never a path. Close with
   *  containerLogsClose — one registry, one way to stop a stream. */
  appLogOpen: (name, id, tailBytes = 65536) => call('app_log_open', { name, id, tailBytes }),

  // The same files, across every project at once — the view for "which of my
  // eight projects just errored", which you ask before you know where to look.
  appLogsAll: () => call('app_logs_all'),
  /** Live only: each file is adopted at its current end, because nothing here
   *  parses a timestamp and interleaved *history* from sixty files would be an
   *  ordering the backend cannot justify. Closed with containerLogsClose. */
  appLogsAllOpen: (projects = null) => call('app_logs_all_open', { projects }),

  envSet: (patch) => call('env_set', { patch }),
  generateRun: (scope = 'all') => call('generate_run', { scope }),
  composeUp: (mode = 'minimal', profiles = []) => call('compose_up', { mode, profiles }),
  composeDown: () => call('compose_down'),

  // --- Phase 3: desktop integration -----------------------------------------
  // Note: hosts_status and service_dependencies have no wrapper here on
  // purpose. projects_list already carries domainConfigured, and services_list
  // already carries required/optional/unmetDependencies — a second round trip
  // for the same facts is a way for the two to disagree.
  dbInstances: () => call('db_instances'),
  // I-2. Which projects nothing has asked for, and stopping them. The sweep
  // returns names because a background action that surprises somebody has to
  // be able to say exactly what it did.
  projectsIdle: () => call('projects_idle'),
  projectsSuspendIdle: () => call('projects_suspend_idle'),

  // G-4. Moving one instance's data into another. Planned first because the
  // target is emptied, which is a sentence somebody has to read.
  dbMovePlan: (from, to) => call('db_move_plan', { from, to }),
  dbMoveApply: (from, to) => call('db_move_apply', { from, to }),

  // E-4. Names pointed at something StackVo did not start. Saved whole: the
  // list is a handful of pairs in one table, and three commands over one small
  // document is three ways for it and the screen to disagree.
  routesList: () => call('routes_list'),
  routesSave: (routes) => call('routes_save', { routes }),

  // E-1. A responder for this machine's development names — one suffix,
  // refusing everything else. The resolver file is a separate call because it
  // asks for a password and changes how the whole machine resolves names,
  // which is the same separation `hostsPlan`/`hostsApply` has below.
  dnsStatus: () => call('dns_status'),
  dnsStart: () => call('dns_start'),
  dnsStop: () => call('dns_stop'),
  dnsResolverInstall: () => call('dns_resolver_install'),
  dnsResolverRemove: () => call('dns_resolver_remove'),
  /**
   * Measures the whole path rather than the half this app owns: the responder
   * over both transports, then the machine's own resolver. A status that says
   * "the file is written" cannot tell anyone whether a name resolves.
   */
  dnsCheck: () => call('dns_check'),

  /** Computes the change without elevating, so the UI can show a diff first. */
  hostsPlan: (add = [], remove = []) => call('hosts_plan', { add, remove }),
  hostsApply: (add = [], remove = []) => call('hosts_apply', { add, remove }),
  hostsMissing: () => call('hosts_missing'),
  /**
   * Only the two names the stack is addressed through.
   *
   * What the preflight gate offers, because that is what it blocks on. The
   * dashboard asks for `hostsMissing` — "fix everything" is a thing somebody
   * can ask for, but not a thing to do to them while a password prompt they
   * opened for two entries is on screen.
   */
  hostsMissingCore: () => call('hosts_missing_core'),
  hostsOverview: () => call('hosts_overview'),

  // --- Mail -----------------------------------------------------------------
  // Read in Rust, not here: the CSP allows `connect-src 'self' ipc:`, and
  // widening it to reach one localhost port would widen it for every page.
  mailStatus: () => call('mail_status'),
  mailMessages: (limit = 50) => call('mail_messages', { limit }),
  mailMessage: (id) => call('mail_message', { id }),
  mailClear: () => call('mail_clear'),

  /**
   * The mail relay (M-2): letting one caught message leave.
   *
   * `mailRelayGet` never returns the password — `hasPassword` is a boolean and
   * there is no command in this app that reads a stored credential back.
   * `mailRelaySet` takes `null` to leave the stored one alone and `''` to
   * remove it, because "do not touch it" and "clear it" are different
   * intentions that one field cannot carry.
   */
  mailRelayGet: () => call('mail_relay_get'),
  mailRelaySet: (config, password = null) => call('mail_relay_set', { config, password }),
  mailRelease: (id, to) => call('mail_release', { id, to }),
  mailDelete: (id) => call('mail_delete', { id }),
  /** Server-side search; Mailpit's own query syntax reaches it verbatim. */
  mailSearch: (query, limit = 100) => call('mail_search', { query, limit }),
  /** Client-compatibility report for the message's HTML. Null on MailHog. */
  mailHtmlCheck: (id) => call('mail_html_check', { id }),
  /** Follows every link — this one leaves the machine, so it is on demand. */
  mailLinkCheck: (id) => call('mail_link_check', { id }),
  mailAttachmentSave: (id, partId, path) => call('mail_attachment_save', { id, partId, path }),

  // --- Databases ------------------------------------------------------------
  dbTargets: () => call('db_targets'),
  /** Streams straight to the file; resolves with an operationId, not the dump. */
  dbDump: (service, path) => call('db_dump', { service, path }),
  /** DESTRUCTIVE — replaces the target database. Confirm before calling. */
  dbRestore: (service, path) => call('db_restore', { service, path }),
  /** Every named snapshot in the workspace, newest first. */
  dbSnapshots: () => call('db_snapshots'),
  /** Take one under a name. Returns an operation id; progress is on `db:*`. */
  dbSnapshotTake: (service, name) => call('db_snapshot_take', { service, name }),
  /** Put one back, replacing what is in the database. */
  dbSnapshotRestore: (service, name) => call('db_snapshot_restore', { service, name }),
  dbSnapshotDelete: (service, name) => call('db_snapshot_delete', { service, name }),
  /**
   * The string a client is pasted into, or null for a service without one.
   *
   * Two addresses come back: the host one and the container one. The password
   * is bullets until `reveal` — the same act `envReveal` is.
   */
  /**
   * F-1: what the database was asked, and what it was asked repeatedly.
   *
   * `supported: false` is a normal answer, not a failure — only MySQL and
   * MariaDB keep a log this can switch on without changing the image.
   */
  /**
   * F-2: dumps and queries on one axis.
   *
   * `service` is optional — without it this is the dumps alone. Queries carry
   * no request and that is deliberate, not missing: see the Rust module.
   */
  requestTimeline: (project, service = null) => call('request_timeline', { project, service }),
  /**
   * F-3: the same profile as a call tree, for the flame view.
   *
   * Separate from `profilerRead` because the tree is thousands of nodes and the
   * table is sixty rows — a pane that opens on the table should not carry the
   * graph across to ignore it.
   */
  profilerTree: (name, id) => call('profiler_tree', { name, id }),
  /**
   * The flame graph for a recorded trace (F-3).
   *
   * A different command from `profilerTree` because it is a different picture
   * read from a different file: cachegrind holds summed edges and traces hold
   * stacks, and only the second can say that one caller of a function was
   * expensive and another was not.
   */
  profilerFlame: (name, id) => call('profiler_flame', { name, id }),

  // I-1. The heavy directories of a project, in named volumes rather than on
  // the host filesystem — measured at 3.8x on a framework boot and 2.8x on the
  // writes a request makes. `perfSet` seeds the volume from the host before it
  // saves anything, which is why it is one call and not two.
  // M-5, M-6, M-10. Three per-project settings in one small document: whole
  // document rather than per key, because three commands over one file is three
  // chances for it and the screen to disagree.
  siteSettings: (name) => call('site_settings', { name }),
  siteSave: (name, env, directoryListing, sshAgent) =>
    call('site_save', { name, env, directoryListing, sshAgent }),

  perfStatus: (name) => call('perf_status', { name }),
  perfSet: (name, path, enabled) => call('perf_set', { name, path, enabled }),
  perfExport: (name, path) => call('perf_export', { name, path }),
  /** Deletes the volume. Separate from the switch, deliberately. */
  perfForget: (name, path) => call('perf_forget', { name, path }),
  queryLog: (service) => call('query_log', { service }),
  /** Start or stop recording. Stopping also clears what was collected. */
  queryLogRecord: (service, recording) => call('query_log_record', { service, recording }),
  /** Throw away the session so far, without stopping. */
  queryLogClear: (service) => call('query_log_clear', { service }),
  serviceConnection: (service, reveal = false) => call('service_connection', { service, reveal }),
  /**
   * Which desktop clients on this machine open this service's kind of address.
   *
   * Empty for most services and that is the answer, not a failure — the button
   * is keyed on this list having something in it.
   */
  serviceDbClients: (service) => call('service_db_clients', { service }),
  /**
   * Hand the host address to one of them. The empty id means the system handler.
   *
   * The string that goes across carries the real password, because one with
   * bullets in it fails to connect — so this is the same deliberate act
   * `envReveal` is, and it is recorded like one.
   */
  serviceOpenInClient: (service, client = '') =>
    call('service_open_in_client', { service, client }),

  // --- Xdebug ---------------------------------------------------------------
  // Three answers, not one: asked for in the manifest, compiled into the image,
  // live in the running container. They come apart, and each needs a different
  // fix.
  xdebugStatus: (name) => call('xdebug_status', { name }),
  xdebugSet: (name, enabled) => call('xdebug_set', { name, enabled }),

  // The project's PHP overrides. `.stackvo/php.ini` was documented for years
  // and mounted by nothing; the mount is a compose overlay this app layers.
  phpIniStatus: (name) => call('php_ini_status', { name }),
  /** `patch` maps a directive to its value; null removes it. Removing the last
   *  one removes the file, and the mount goes with it. */
  phpIniSet: (name, patch) => call('php_ini_set', { name, patch }),

  // The stack, as something a teammate can be handed. `stackvo.json` is already
  // in their clone; which services are on and at which versions is not — that
  // lives in .env, the one file nobody commits.
  /** Removes an extension the build cannot install. Changes nothing about what
   *  runs — it is already being dropped silently. */
  doctorDropExtension: (subject, extension) =>
    call('doctor_drop_extension', { subject, extension }),

  // dump()/dd() caught out of the response by a PHP file mounted into the
  // container. Toggling is a file appearing in a directory that is already
  // mounted, so it costs no container — which is the whole reason this
  // replaced Symfony's own collector run through `docker exec`.
  debugBridgeSet: (name, enabled) => call('debug_bridge_set', { name, enabled }),
  debugBridgeEvents: (name, since = 0) => call('debug_bridge_events', { name, since }),
  debugBridgeClear: (name) => call('debug_bridge_clear', { name }),
  debugBridgeOverview: () => call('debug_bridge_overview'),
  /** Streams as `logs:line`; close it with containerLogsClose. */
  /** Stops the in-container collector too — killing `docker exec` does not. */

  // A deployable image from the one the project already runs. The dev image
  // has no application code for PHP (it is bind-mounted) and carries Xdebug,
  // so this is a build, not a copy.
  releasePlan: (name, tag = null) => call('release_plan', { name, tag }),
  /** Builds, then runs the result and asks whether it leaked an .env. */
  releaseBuild: (name, tag = null) => call('release_build', { name, tag }),
  // H-1. Getting the built image somewhere, and something to run it with.
  // Planned first because the refusals — unverified image, no registry host —
  // are the whole reason the push is safe.
  releasePushPlan: (name, tag = null) => call('release_push_plan', { name, tag }),
  releasePush: (name, tag = null) => call('release_push', { name, tag }),
  releaseRecipe: (name, tag = null) => call('release_recipe', { name, tag }),

  releaseSave: (name, path, tag = null) => call('release_save', { name, tag, path }),
  /** Read a bundle back in on a machine that may have no registry at all. */
  releaseLoad: (path) => call('release_load', { path }),

  // Xdebug's own profiler. Blackfire needs an account and SPX is not in the
  // extension contract; xdebug.mode=profile needs neither.
  profilerStatus: (name) => call('profiler_status', { name }),
  /** 'debug' or 'profile' — never both: the two want opposite start triggers. */
  profilerSetMode: (name, mode) => call('profiler_set_mode', { name, mode }),
  profilerRead: (name, id) => call('profiler_read', { name, id }),
  profilerDelete: (name, id) => call('profiler_delete', { name, id }),
  profilerClear: (name) => call('profiler_clear', { name }),

  // The handful of commands you run in a project every day. The id is the only
  // thing that crosses — the argv is built on the Rust side from a fixed
  // catalog, so the webview cannot name a program to execute.
  quickCommands: (name) => call('quick_commands', { name }),
  /** Resolves to an operation id, or null for an interactive command that
   *  opened the user's own terminal. */
  quickCommandRun: (name, id) => call('quick_command_run', { name, id }),

  // Hot reload for node projects. Not a routing change: a node project has no
  // bind mount at all today, so the source in the container is a snapshot taken
  // when the image was built.
  devserverStatus: (name) => call('devserver_status', { name }),
  devserverSet: (name, enabled, command = null) =>
    call('devserver_set', { name, enabled, command }),

  // Somebody else's docker-compose.yml, read by Docker itself. Detection sees
  // the code; the compose file records what its author decided — the PHP
  // version, the domain, and which backing services the project needs.
  migrateScan: (name) => call('migrate_scan', { name }),
  migrateApply: (name, spec = null, services = true) =>
    call('migrate_apply', { name, spec, services }),

  presetExport: (name = null) => call('preset_export', { name }),
  presetSave: (path, name = null) => call('preset_save', { path, name }),
  /** Reviewed before applied, like hosts and certificates. */
  presetPlan: (path) => call('preset_plan', { path }),
  presetApply: (path) => call('preset_apply', { path }),

  // --- Certificates ---------------------------------------------------------
  // Same order as hosts: describe, then change. `certStatus` needs no engine —
  // a certificate issued before a project existed is just as wrong with the
  // stack down, and that is the case users actually hit.
  certStatus: () => call('cert_status'),
  certPlan: (installCa = true) => call('cert_plan', { installCa }),
  /** Reissues, and installs the CA when nothing trusts it yet. */
  certApply: (installCa = true) => call('cert_apply', { installCa }),
  /**
   * Trust the CA, in the user's own terminal.
   *
   * macOS will not let a windowed app change trust settings: `sudo` needs a
   * terminal, root-via-AppleScript is refused outright, and the user-domain
   * write exits 0 and does nothing. `mkcert -install` in a real terminal is
   * the one thing that works, so the app opens one.
   */
  certTrustInTerminal: () => call('cert_trust_in_terminal'),

  // --- Project lifecycle ----------------------------------------------------
  /** Opens the native picker, validates, and persists in one step. */
  workspacePick: () => call('workspace_pick'),
  projectGet: (name) => call('project_get', { name }),
  /** Fill a new directory with a framework via a throwaway container. */
  projectScaffold: (name, template) => call('project_scaffold', { name, template }),
  gitAvailable: () => call('git_available'),
  projectClone: (url, name = null) => call('project_clone', { url, name }),
  projectRegister: (name) => call('project_register', { name }),

  /** Every tunnel sidecar and its public URL, read live from its log. */
  tunnelStatus: () => call('tunnel_status'),
  tunnelStart: (name) => call('tunnel_start', { name }),
  tunnelStop: (name) => call('tunnel_stop', { name }),

  /**
   * A QR code for an address meant to be opened on another device (M-3).
   *
   * Returns the module matrix, not a picture: the caller draws it, so the same
   * symbol can be an SVG here and something else later without a second
   * encoder. Rejects text longer than a version 10 symbol holds rather than
   * encoding part of it.
   */
  qrEncode: (text) => call('qr_encode', { text }),

  /**
   * The page that lists every site, on the workspace suffix itself (M-4).
   *
   * `landingRefresh` is separate from `landingStart` on purpose: the sidecar
   * serves a file, so starting a project after the page was written leaves it
   * stale without anything having stopped. One button doing both would restart
   * a container to update a list.
   */
  /**
   * The redirect URI to register with an identity provider (M-12).
   *
   * The tunnel URL is read on the Rust side rather than passed in: a quick
   * tunnel's address changes on every start, and a callback registered from a
   * stale one fails at the last step of the flow.
   */
  oauthCallbacks: (name, path) => call('oauth_callbacks', { name, path }),

  /**
   * Stripe's own webhook listener, per project (M-11).
   *
   * `stripeKeySet` writes to the OS keystore and returns a boolean; there is
   * no command that reads a key back, deliberately — the pane can replace it
   * or clear it and never display it.
   */
  stripeStatus: () => call('stripe_status'),
  stripeKeySet: (name, key) => call('stripe_key_set', { name, key }),
  stripeStart: (name, path, events = []) => call('stripe_start', { name, path, events }),
  stripeStop: (name) => call('stripe_stop', { name }),

  landingStatus: () => call('landing_status'),
  landingStart: () => call('landing_start'),
  landingStop: () => call('landing_stop'),
  landingRefresh: () => call('landing_refresh'),

  /** Worker kinds this project offers, detected from its files. */
  workerOptions: (name) => call('worker_options', { name }),
  /** Every worker sidecar, restart counts included. */
  workerStatus: () => call('worker_status'),
  workerStart: (name, kind) => call('worker_start', { name, kind }),
  workerStop: (name, kind) => call('worker_stop', { name, kind }),
  /** Pre-flight a spec before anything touches disk. */
  projectValidate: (name, spec) => call('project_validate', { name, spec }),
  projectCreate: (spec) => call('project_create', { spec }),
  /** removeFiles defaults to false — deleting source code needs an opt-in. */
  projectDelete: (name, removeFiles = false) => call('project_delete', { name, removeFiles }),
  /** Folders under projects/ with no stackvo.json — real code, unmanaged. */
  /** What XAMPP and Laragon have on this machine. Reads only. */
  importsScan: () => call('imports_scan'),
  /** The same for an installation somewhere else; null when it is not one. */
  importsScanAt: (source, path) => call('imports_scan_at', { source, path }),
  /**
   * Copy (or move) one site into the workspace. The file half only — follow
   * with `projectAdopt`, so an imported project is validated like any other.
   */
  importsTake: (path, name, move = false) => call('imports_take', { path, name, move }),
  projectAdoptable: () => call('project_adoptable'),
  /** Writes the manifest for a directory that is already there. */
  /** `overrides` — `{domain, phpVersion, server, extensions}`, each optional —
   *  replaces only what it names; everything else still comes from detection
   *  over what is on disk. */
  projectAdopt: (name, spec = null, overrides = null) =>
    call('project_adopt', { name, spec, overrides }),
  projectManifestRead: (name) => call('project_manifest_read', { name }),
  projectManifestWrite: (name, manifest) => call('project_manifest_write', { name, manifest }),

  // B-2. `stackvo.local.json` — this machine's overrides for a committed
  // manifest. Text, not an object: the file is typed by hand and a struct
  // round-trip would reformat it.
  projectLocalRead: (name) => call('project_local_read', { name }),
  projectLocalWrite: (name, text) => call('project_local_write', { name, text }),

  // B-3. What a project's lifecycle hooks would run, and the approval for the
  // ones that touch this machine. The digest goes back with the approval on
  // purpose — it is a receipt for the list that was on screen.
  projectHooksPlan: (name) => call('project_hooks_plan', { name }),
  projectHooksApprove: (name, digest) => call('project_hooks_approve', { name, digest }),
  projectHooksRevoke: (name) => call('project_hooks_revoke', { name }),
  /**
   * Also answer on a name other devices on this network can resolve.
   *
   * Writes the intent only — the hostname is derived from this machine's
   * address every time it is asked for, so `lanStatus` is what says what it
   * currently is, and regenerating is still what puts it in the router.
   */
  projectLanShare: (name, enabled) => call('project_lan_share', { name, enabled }),
  /**
   * The address a phone on the same Wi-Fi would use, and the two ways there
   * isn't one: no network, or a public address, which is refused rather than
   * published.
   */
  lanStatus: () => call('lan_status'),
  /** What the repository declares, what this machine gives it, and the diff. */
  projectRequirements: (name) => call('project_requirements', { name }),
  /** Enable the declared services that are not on yet. Writes `.env` only. */
  projectRequirementsApply: (name) => call('project_requirements_apply', { name }),
  /** Write the `services` list into the project's committed `stackvo.json`. */
  projectRequirementsDeclare: (name, services) =>
    call('project_requirements_declare', { name, services }),

  updaterStatus: () => call('updater_status'),
  /** The third-party licence notice compiled into this build, as markdown. */
  licencesNotice: () => call('licences_notice'),
  /**
   * What an administrator has decided on this machine, if anything.
   *
   * Keys only, never values — `envGet` is the redacting reader and this must
   * not become a way past it.
   */
  policyStatus: () => call('policy_status'),
  /** Where each credential lives, and whether this machine has a keystore. */
  secretsStatus: () => call('secrets_status'),
  /** Move one credential out of `.env` and into the OS keystore. */
  secretMove: (key) => call('secret_move', { key }),
  /** Put it back in `.env` and forget the keystore entry. */
  secretRestore: (key) => call('secret_restore', { key }),
  /** Which assistants are installed, and which already point at the server. */
  agentsStatus: () => call('agents_status'),
  /**
   * Register the MCP server with one assistant.
   *
   * `allowWrites` is not optional on purpose. It is the argument that decides
   * whether that assistant can stop the stack, and a default here would be a
   * security decision made in a wrapper.
   */
  agentsInstall: (client, allowWrites) => call('agents_install', { client, allowWrites }),
  /** Take the entry back out of that assistant's configuration. */
  agentsRemove: (client) => call('agents_remove', { client }),
  /** The desktop's own accent colour, so the app can match it. */
  systemAccent: () => call('system_accent'),
  logsInfo: () => call('logs_info'),
  /** Writes the diagnostic archive to a path the user chose in the save dialog. */
  diagnosticsBundle: (path) => call('diagnostics_bundle', { path }),
  localeGet: () => call('locale_get'),

  /**
   * Language packs (M-7) — a language this build was not shipped with.
   *
   * One JSON file per language in the app's config directory, with the same
   * shape as `i18n/locales/en.js`. `localePacks` lists them, a broken one
   * included with its parse error: a hand-edited file that simply vanishes
   * from the picker is the worst failure this could have.
   *
   * `localePackWrite` is how "start a translation" works — the front end sends
   * the English catalogue, because the front end is where the catalogue lives.
   */
  localePacks: () => call('locale_packs'),
  localePackRead: (tag) => call('locale_pack_read', { tag }),
  localePackWrite: (tag, messages) => call('locale_pack_write', { tag, messages }),
  localePackDelete: (tag) => call('locale_pack_delete', { tag }),
  /** Redraw the tray and menu bar, optionally adopting a new catalog first. */
  trayRelabel: (labels) => call('tray_relabel', { labels }),
  appsAvailable: () => call('apps_available'),
  windowCloseAction: (action, remember) => call('window_close_action', { action, remember }),

  containerStatsHistory: (name) => call('container_stats_history', { name }),

  containersStartAll: () => call('containers_start_all'),
  containersStopAll: () => call('containers_stop_all'),
  containersRestartAll: () => call('containers_restart_all'),

  composeUpProject: (name) => call('compose_up_project', { name }),
  composeRestart: () => call('compose_restart'),

  openInEditor: (path) => call('open_in_editor', { path }),
  /** Opens in the browser chosen in Settings, or the system default. */
  openInBrowser: (url) => call('open_in_browser', { url }),
  openFolder: (path) => call('open_folder', { path }),
  prefsGet: () => call('prefs_get'),
  prefsSet: (patch) => call('prefs_set', { patch }),
  /** Renders every generated file and diffs it against the disk — a drift
   *  check, now that the Rust generator is the only writer. */
  generatorVerify: () => call('generator_verify'),
  /** Renders one project's Dockerfile without writing it. */
  projectDockerfilePreview: (name, strict = true) =>
    call('project_dockerfile_preview', { name, strict }),

  ptyOpen: (target, cols, rows) => call('pty_open', { target, cols, rows }),
  ptyWrite: (sessionId, data) => call('pty_write', { sessionId, data }),
  ptyResize: (sessionId, cols, rows) => call('pty_resize', { sessionId, cols, rows }),
  ptyClose: (sessionId) => call('pty_close', { sessionId }),
  terminalOpenExternal: (target) => call('terminal_open_external', { target }),
};

export default api;
