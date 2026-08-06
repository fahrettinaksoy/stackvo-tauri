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
  /** One secret, unmasked, on explicit request. See `env_reveal` in Rust. */
  envReveal: (key) => call('env_reveal', { key }),

  // --- Phase 2: mutations ---------------------------------------------------
  projectStart: (name) => call('project_start', { name }),
  projectStop: (name) => call('project_stop', { name }),
  projectRestart: (name) => call('project_restart', { name }),
  /** Resolves with an operationId as soon as the build starts, not when it ends. */
  projectBuild: (name, noCache = false) => call('project_build', { name, noCache }),

  serviceStart: (name) => call('service_start', { name }),
  serviceStop: (name) => call('service_stop', { name }),
  serviceRestart: (name) => call('service_restart', { name }),
  serviceSettings: (name) => call('service_settings', { name }),
  serviceApplySettings: (name, patch) => call('service_apply_settings', { name, patch }),
  serviceEnable: (name) => call('service_enable', { name }),
  serviceDisable: (name) => call('service_disable', { name }),

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
  releaseSave: (name, path, tag = null) => call('release_save', { name, tag, path }),

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
  projectAdoptable: () => call('project_adoptable'),
  /** Writes the manifest for a directory that is already there. */
  /** `overrides` — `{domain, phpVersion, server, extensions}`, each optional —
   *  replaces only what it names; everything else still comes from detection
   *  over what is on disk. */
  projectAdopt: (name, spec = null, overrides = null) =>
    call('project_adopt', { name, spec, overrides }),
  projectManifestRead: (name) => call('project_manifest_read', { name }),
  projectManifestWrite: (name, manifest) => call('project_manifest_write', { name, manifest }),

  updaterStatus: () => call('updater_status'),
  /** The desktop's own accent colour, so the app can match it. */
  systemAccent: () => call('system_accent'),
  logsInfo: () => call('logs_info'),
  /** Writes the diagnostic archive to a path the user chose in the save dialog. */
  diagnosticsBundle: (path) => call('diagnostics_bundle', { path }),
  localeGet: () => call('locale_get'),
  trayRelabel: () => call('tray_relabel'),
  appsAvailable: () => call('apps_available'),
  windowCloseAction: (action, remember) => call('window_close_action', { action, remember }),

  containerStatsHistory: (name) => call('container_stats_history', { name }),

  containersStartAll: () => call('containers_start_all'),
  containersStopAll: () => call('containers_stop_all'),
  containersRestartAll: () => call('containers_restart_all'),

  composeUpService: (name) => call('compose_up_service', { name }),
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
