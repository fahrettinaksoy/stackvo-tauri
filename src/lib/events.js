import { listen } from '@tauri-apps/api/event';

/**
 * Event names from contracts/ipc.json.
 *
 * These are the same strings the Socket.io UI used, so ported listeners keep
 * working — except the terminal events, which moved from `terminal-*` to
 * `terminal:*` for consistency.
 */
export const EVENTS = {
  app: ['app:close_requested'],
  engine: ['engine:status_changed'],
  container: ['container:state_changed'],
  project: [
    'project:creating',
    'project:created',
    'project:deleting',
    'project:deleted',
    'project:starting',
    'project:started',
    'project:stopping',
    'project:stopped',
    'project:restarting',
    'project:restarted',
    'project:error',
  ],
  service: [
    'service:enabling',
    'service:enabled',
    'service:disabling',
    'service:disabled',
    'service:starting',
    'service:started',
    'service:stopping',
    'service:stopped',
    'service:restarting',
    'service:restarted',
    'service:progress',
    'service:error',
  ],
  build: ['build:start', 'build:progress', 'build:built', 'build:success', 'build:error'],
  generate: ['generate:start', 'generate:progress', 'generate:done'],
  compose: ['compose:progress', 'compose:done'],
  logs: ['logs:line', 'logs:closed'],
};

/**
 * Subscribe to several events at once. Returns a single unsubscribe function,
 * so a component can tear all of them down in one `onUnmounted`.
 *
 * @param {string[]} names
 * @param {(name: string, payload: any) => void} handler
 */
export async function listenAll(names, handler) {
  const unlisteners = await Promise.all(
    names.map((name) => listen(name, (event) => handler(name, event.payload)))
  );
  return () => unlisteners.forEach((off) => off());
}

/** Every lifecycle event that should make a list refetch. */
export const REFRESH_TRIGGERS = [
  // Pushed by the Docker event stream rather than found by polling — a
  // container that dies on its own now updates the UI immediately.
  'container:state_changed',
  'project:started',
  'project:stopped',
  'project:restarted',
  'project:created',
  'project:deleted',
  'service:started',
  'service:stopped',
  'service:restarted',
  'service:enabled',
  'service:disabled',
  'build:success',
  'compose:done',
];
