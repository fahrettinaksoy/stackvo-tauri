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
  constructor({ code, message, hint, details }) {
    super(message || 'Unknown error');
    this.name = 'StackvoError';
    this.code = code || 'UNKNOWN';
    this.hint = hint;
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
    // Rust's Error serialises to { code, message, hint?, details? }. A panic or
    // a missing command arrives as a bare string instead.
    if (raw && typeof raw === 'object' && 'code' in raw) {
      throw new StackvoError(raw);
    }
    throw new StackvoError({ code: 'UNKNOWN', message: String(raw) });
  }
}

// The command surface, one thin wrapper each. Keeping them enumerated here —
// rather than letting views call `call('whatever')` — means the contract is
// greppable from the frontend and a typo fails at import, not at runtime.
export const api = {
  workspaceGet: () => call('workspace_get'),
  workspaceSet: (path) => call('workspace_set', { path }),

  engineStatus: () => call('engine_status'),
  engineStart: () => call('engine_start'),

  hostStats: () => call('host_stats'),
  dockerSystemResources: () => call('docker_system_resources'),

  projectsList: () => call('projects_list'),
  servicesList: () => call('services_list'),

  catalogGet: () => call('catalog_get'),
  envGet: () => call('env_get'),

  // --- Phase 2: mutations ---------------------------------------------------
  projectStart: (name) => call('project_start', { name }),
  projectStop: (name) => call('project_stop', { name }),
  projectRestart: (name) => call('project_restart', { name }),
  /** Resolves with an operationId as soon as the build starts, not when it ends. */
  projectBuild: (name, noCache = false) => call('project_build', { name, noCache }),

  serviceStart: (name) => call('service_start', { name }),
  serviceStop: (name) => call('service_stop', { name }),
  serviceRestart: (name) => call('service_restart', { name }),
  serviceEnable: (name) => call('service_enable', { name }),
  serviceDisable: (name) => call('service_disable', { name }),

  containerInspect: (name) => call('container_inspect', { name }),
  containerStats: (name) => call('container_stats', { name }),
  containerLogsOpen: (name, tail = 200, follow = true) =>
    call('container_logs_open', { name, tail, follow }),
  containerLogsClose: (streamId) => call('container_logs_close', { streamId }),

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

  // --- Project lifecycle ----------------------------------------------------
  /** Opens the native picker, validates, and persists in one step. */
  workspacePick: () => call('workspace_pick'),
  projectGet: (name) => call('project_get', { name }),
  /** Pre-flight a spec before anything touches disk. */
  projectValidate: (name, spec) => call('project_validate', { name, spec }),
  projectCreate: (spec) => call('project_create', { spec }),
  /** removeFiles defaults to false — deleting source code needs an opt-in. */
  projectDelete: (name, removeFiles = false) => call('project_delete', { name, removeFiles }),
  projectManifestRead: (name) => call('project_manifest_read', { name }),
  projectManifestWrite: (name, manifest) => call('project_manifest_write', { name, manifest }),

  updaterStatus: () => call('updater_status'),
  logsInfo: () => call('logs_info'),
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
  prefsGet: () => call('prefs_get'),
  prefsSet: (patch) => call('prefs_set', { patch }),
  /** Renders every generated file with the Rust port and diffs it against the
   *  Bash output. readyToTakeOver is the gate for the migration. */
  generatorVerify: () => call('generator_verify'),
  /**
   * Generate with a chosen engine.
   *   'bash'   — what StackVo does today (default)
   *   'verify' — bash writes, the Rust port is compared against it
   *   'rust'   — refuses to write unless the two agree byte-for-byte
   */
  generateWith: (scope = 'all', engineMode = 'bash') =>
    call('generate_with', { scope, engineMode }),
  /** Runs the Rust generator port; matchesBashOutput is the live differential. */
  projectDockerfilePreview: (name, strict = true) =>
    call('project_dockerfile_preview', { name, strict }),

  ptyOpen: (target, cols, rows) => call('pty_open', { target, cols, rows }),
  ptyWrite: (sessionId, data) => call('pty_write', { sessionId, data }),
  ptyResize: (sessionId, cols, rows) => call('pty_resize', { sessionId, cols, rows }),
  ptyClose: (sessionId) => call('pty_close', { sessionId }),
  terminalOpenExternal: (target) => call('terminal_open_external', { target }),
};

export default api;
