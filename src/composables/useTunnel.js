import { ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * This project's tunnel sidecar, when one exists.
 *
 * The URL is Cloudflare's to assign and arrives seconds after the sidecar
 * starts, so starting polls until the status call can read it out of the
 * sidecar's log — the same place it is read from after an app restart.
 *
 * Lifted out of `ProjectDetail.vue` with the Tunnel pane under §14.16.
 */

/** How long to wait between polls, and how many times. */
export const TUNNEL_POLL_MS = 1500;
const TUNNEL_POLL_TRIES = 20;

export function useTunnel(name) {
  const tunnel = ref(null);
  const busy = ref(false);
  const error = ref(null);

  /**
   * The status call answers for every project at once; only this one's row
   * matters, and its absence is "no tunnel" rather than a failure.
   */
  async function load() {
    try {
      const all = await api.tunnelStatus();
      tunnel.value = all.find((t) => t.project === name.value) ?? null;
    } catch {
      tunnel.value = null;
    }
    return tunnel.value;
  }

  async function start() {
    busy.value = true;
    error.value = null;
    try {
      await api.tunnelStart(name.value);
      for (let i = 0; i < TUNNEL_POLL_TRIES; i++) {
        await new Promise((r) => setTimeout(r, TUNNEL_POLL_MS));
        await load();
        if (tunnel.value?.url) break;
      }
      return tunnel.value;
    } catch (e) {
      error.value = e;
      return null;
    } finally {
      // Cleared even when the poll ran out: the sidecar is up either way, and
      // a button left spinning would say otherwise.
      busy.value = false;
    }
  }

  async function stop() {
    busy.value = true;
    error.value = null;
    try {
      await api.tunnelStop(name.value);
      await load();
      return true;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      busy.value = false;
    }
  }

  return { tunnel, busy, error, load, start, stop };
}
