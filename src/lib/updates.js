import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { api } from '@/lib/ipc';

/**
 * Signed auto-updates.
 *
 * StackVo updates today are `git pull` plus a Docker image rebuild — the
 * dashboard is a container, so shipping a new one means rebuilding it. A
 * desktop app can just replace its own binary, provided the replacement is
 * signed with a key the app already trusts.
 *
 * That trust is the whole mechanism: Tauri verifies the bundle's signature
 * against the public key compiled into the app. An unsigned or wrongly-signed
 * update is refused before a byte of it runs. The private key is a release
 * secret and never appears in this repository.
 */

/**
 * Can this build verify an update at all?
 *
 * Asked of the Rust side rather than assumed. Without a public key compiled in
 * there is nothing to verify a bundle against, so every check fails inside the
 * plugin with a message about signatures — which reads like a server problem
 * and is actually a build problem. Distinguishing the two is the point.
 */
export async function updatesConfigured() {
  try {
    const status = await api.updaterStatus();
    return status.configured;
  } catch {
    return false;
  }
}

/**
 * Look for an update. Returns null when there is none.
 *
 * Never throws for the ordinary "no network" case — an app that shows an error
 * banner every time a laptop is offline trains people to ignore the banner.
 */
export async function checkForUpdate() {
  try {
    const update = await check();
    if (!update?.available) return null;

    return {
      version: update.version,
      currentVersion: update.currentVersion,
      notes: update.body,
      date: update.date,
      /** Download, install, then restart. Progress is reported per chunk. */
      install: async (onProgress) => {
        let downloaded = 0;
        let total = 0;

        await update.downloadAndInstall((event) => {
          switch (event.event) {
            case 'Started':
              total = event.data.contentLength ?? 0;
              break;
            case 'Progress':
              downloaded += event.data.chunkLength ?? 0;
              onProgress?.({ downloaded, total });
              break;
            case 'Finished':
              onProgress?.({ downloaded: total, total });
              break;
          }
        });

        // The new binary is in place; the running one has to hand over.
        await relaunch();
      },
    };
  } catch (e) {
    // Distinguish "cannot reach the endpoint" from "the signature did not
    // verify" — the second is a security event and must not be silent.
    const message = String(e);
    if (/signature|pubkey|public key/i.test(message)) {
      throw new Error(`Update signature could not be verified: ${message}`);
    }
    return null;
  }
}
