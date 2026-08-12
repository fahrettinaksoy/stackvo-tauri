import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';

/**
 * Native notifications for work that finishes while the window is elsewhere.
 *
 * The web UI could only ever notify you through a browser tab that was already
 * open on it — which is the one case where you did not need notifying.
 */

let granted = null;

async function ensurePermission() {
  if (granted !== null) return granted;
  granted = await isPermissionGranted();
  if (!granted) granted = (await requestPermission()) === 'granted';
  return granted;
}

/**
 * @param {string} title
 * @param {string} body
 */
export async function notify(title, body) {
  try {
    if (!(await ensurePermission())) return;
    sendNotification({ title, body });
  } catch {
    // A refused or unavailable notification is not worth failing an operation
    // over — the UI already showed the same outcome inline.
  }
}

/** Only notify for outcomes worth interrupting someone about. */
export function shouldNotify(eventName) {
  return ['build:success', 'build:error', 'compose:done', 'project:error'].includes(eventName);
}
