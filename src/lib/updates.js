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
 * Look for an update. Returns null when there is none, or when there is one
 * this install is not being offered.
 *
 * Never throws for the ordinary "no network" case — an app that shows an error
 * banner every time a laptop is offline trains people to ignore the banner.
 *
 * ## Two questions, and the plugin only answers the first
 *
 * `check()` asks "is there a newer version". Whether **this** install should
 * take it is a different question, and the plugin has no concept of it: no
 * channel, no staged wave, and — the one that matters — no way to stop. A
 * release found to be broken cannot be recalled, because every running copy
 * keeps asking the same endpoint and getting the same answer.
 *
 * `updater_offer` is that second question (§3 #21, `src-tauri/src/channel.rs`).
 * It reads the manifest the plugin already fetched — `rawJson`, so there is no
 * second request and no chance of the two disagreeing — and answers with what
 * this install should be offered.
 *
 * The order is the design: the decision is made **before** anything reaches the
 * user. Offering an update and then refusing to install it would be worse than
 * never offering it.
 */
export async function checkForUpdate({ channel = null } = {}) {
  try {
    const update = await check();
    if (!update?.available) return null;

    // A manifest the decision cannot be read out of is treated as "offer it",
    // not as "refuse it". The fields are additive: `latest.json` as published
    // today carries none of them, and a check that started refusing every
    // update the day this shipped would be a worse bug than the one it guards.
    const verdict = await api.updaterOffer(update.rawJson ?? {}, channel).catch(() => null);
    const outcome = verdict?.offer?.outcome;

    if (outcome && outcome !== 'update') {
      // `paused` is the publisher stopping a release; `waiting` is this install
      // not being in the wave yet; `otherChannel` is a manifest for a stream
      // this install does not follow. None is an error and none is an update.
      return null;
    }

    return {
      version: update.version,
      currentVersion: update.currentVersion,
      notes: update.body,
      date: update.date,
      /** Why this install was offered it — the bucket, so it can be checked. */
      offer: verdict ?? null,
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
