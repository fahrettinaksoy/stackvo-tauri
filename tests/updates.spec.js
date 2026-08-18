import { describe, it, expect, vi, beforeEach } from 'vitest';

/**
 * The update a person is shown, and the ones they are not. §3 #21.
 *
 * `check()` from the plugin answers one question: is there a newer version.
 * Whether **this** install should take it is a different question, and the
 * plugin has no concept of it — no channel, no staged wave, and no way to stop
 * a release that turned out to be broken.
 *
 * What is tested here is not the decision. That is `channel.rs`'s, it is pure,
 * and it has sixteen tests of its own. What is tested here is the **order**: a
 * refusal has to land before anything reaches the user, because offering an
 * update and then declining to install it is worse than never offering it.
 */

const check = vi.fn();
const relaunch = vi.fn();
const updaterOffer = vi.fn();

vi.mock('@tauri-apps/plugin-updater', () => ({ check: (...a) => check(...a) }));
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: (...a) => relaunch(...a) }));
vi.mock('@/lib/ipc', () => ({
  api: {
    updaterOffer: (...a) => updaterOffer(...a),
    updaterStatus: () => Promise.resolve({ configured: true }),
  },
}));

const { checkForUpdate } = await import('../src/lib/updates.js');

/** What the plugin hands back when a newer version exists. */
function available(rawJson = { version: '0.2.0' }) {
  return {
    available: true,
    version: '0.2.0',
    currentVersion: '0.1.0',
    body: 'notes',
    date: '2026-01-01',
    rawJson,
    downloadAndInstall: vi.fn(),
  };
}

const offered = (outcome, detail = null) => ({ offer: { outcome, detail }, bucket: 7 });

beforeEach(() => {
  check.mockReset();
  updaterOffer.mockReset();
});

describe('an update nobody should be offered', () => {
  it('is not returned when the publisher paused the release', async () => {
    // The 2am button. Everything else about the manifest still says "0.2.0 is
    // newer", and the plugin would install it.
    check.mockResolvedValue(available());
    updaterOffer.mockResolvedValue(offered('paused'));

    expect(await checkForUpdate()).toBeNull();
  });

  it('is not returned when this install is not in the wave yet', async () => {
    check.mockResolvedValue(available());
    updaterOffer.mockResolvedValue(offered('waiting', { bucket: 71, percent: 10 }));

    expect(await checkForUpdate()).toBeNull();
  });

  it('is not returned when the manifest is for another channel', async () => {
    check.mockResolvedValue(available());
    updaterOffer.mockResolvedValue(offered('otherChannel', 'beta'));

    expect(await checkForUpdate()).toBeNull();
  });
});

describe('the order the decision is made in', () => {
  it('asks before returning anything, not after', async () => {
    // The whole point. A version that asked afterwards would put the update on
    // the screen and then refuse to install it.
    const seen = [];
    check.mockImplementation(async () => {
      seen.push('check');
      return available();
    });
    updaterOffer.mockImplementation(async () => {
      seen.push('offer');
      return offered('paused');
    });

    const result = await checkForUpdate();
    expect(seen).toEqual(['check', 'offer']);
    expect(result).toBeNull();
  });

  it('reads the manifest the plugin already fetched rather than fetching it again', async () => {
    // Two requests would be two answers, and the endpoint is allowed to change
    // between them — a paused release could be un-paused by a race.
    const manifest = { version: '0.2.0', paused: true, percent: 5 };
    check.mockResolvedValue(available(manifest));
    updaterOffer.mockResolvedValue(offered('paused'));

    await checkForUpdate();
    expect(updaterOffer).toHaveBeenCalledWith(manifest, null);
  });

  it('passes the channel it was given', async () => {
    check.mockResolvedValue(available());
    updaterOffer.mockResolvedValue(offered('update', '0.2.0'));

    await checkForUpdate({ channel: 'beta' });
    expect(updaterOffer).toHaveBeenCalledWith(expect.anything(), 'beta');
  });

  it('never asks at all when there is no update', async () => {
    check.mockResolvedValue({ available: false });
    expect(await checkForUpdate()).toBeNull();
    expect(updaterOffer).not.toHaveBeenCalled();
  });
});

describe('an update this install should take', () => {
  it('comes back with the version, the notes and how it was decided', async () => {
    check.mockResolvedValue(available());
    updaterOffer.mockResolvedValue(offered('update', '0.2.0'));

    const update = await checkForUpdate();
    expect(update).toMatchObject({ version: '0.2.0', currentVersion: '0.1.0', notes: 'notes' });
    // The bucket travels with it: somebody who wants to know why they were in
    // this wave can read the number rather than guess at it.
    expect(update.offer.bucket).toBe(7);
    expect(typeof update.install).toBe('function');
  });
});

describe('when the decision cannot be reached', () => {
  it('offers the update rather than refusing it', async () => {
    // The compatibility direction, and it is the load-bearing one. `latest.json`
    // as published today carries none of these fields, and older builds have no
    // `updater_offer` command at all. A check that started refusing every update
    // the day this shipped would be a worse bug than the one it guards against.
    check.mockResolvedValue(available());
    updaterOffer.mockRejectedValue(new Error('no such command'));

    const update = await checkForUpdate();
    expect(update).not.toBeNull();
    expect(update.version).toBe('0.2.0');
    expect(update.offer).toBeNull();
  });

  it('offers it when the manifest carries none of the new fields', async () => {
    // An ordinary manifest: the Rust side defaults `percent` to 100 and
    // `channel` to stable, so the outcome is a plain `update`.
    check.mockResolvedValue(available({ version: '0.2.0', notes: '', platforms: {} }));
    updaterOffer.mockResolvedValue(offered('update', '0.2.0'));

    expect(await checkForUpdate()).not.toBeNull();
  });
});

describe('a signature that does not verify', () => {
  it('is raised rather than swallowed, unlike a network failure', async () => {
    check.mockRejectedValue(new Error('signature verification failed'));
    await expect(checkForUpdate()).rejects.toThrow(/signature/i);
  });

  it('but an unreachable endpoint is silent', async () => {
    // An app that shows an error banner every time a laptop is offline trains
    // people to ignore the banner.
    check.mockRejectedValue(new Error('error sending request'));
    expect(await checkForUpdate()).toBeNull();
  });
});
