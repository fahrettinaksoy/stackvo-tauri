import { describe, it, expect, vi, beforeEach } from 'vitest';

/**
 * The `.env` editor six Settings panes share.
 *
 * It had no test. Six panes write the stack's configuration file through it,
 * and the parts that decide *what gets written* are three one-line arrow
 * functions with no obvious failure mode — which is the shape of code that is
 * never wrong until it is, and is then wrong everywhere at once.
 *
 * Three of those decisions are asserted below because each has a way of being
 * quietly wrong:
 *
 *   * the three-layer read, which is what lets a form say "this is the
 *     default" instead of showing every value as equally chosen;
 *   * `edit()` deleting a key when the value comes back, which is the
 *     difference between "there is something to write" and "somebody touched
 *     this";
 *   * the two boolean spellings, `true`/`false` and `on`/`off`, which are not
 *     interchangeable — compose reads one and the generated nginx and php.ini
 *     fragments read the other, so the wrong one produces a file that parses
 *     and does the opposite of what the switch said.
 */

const replies = {};
const calls = [];

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy(
    {},
    {
      get:
        (_t, name) =>
        (...args) => {
          calls.push([String(name), ...args]);
          const reply = replies[name];
          return typeof reply === 'function' ? reply(...args) : Promise.resolve(reply);
        },
    }
  ),
}));

const { useEnvEditor } = await import('@/composables/useEnvEditor');

/** An editor with the file and the shipped defaults already read. */
async function editor(file = {}, defaults = {}) {
  replies.envGet = { ...file };
  replies.envDefaults = { ...defaults };
  replies.envSet = () => Promise.resolve();

  const e = useEnvEditor();
  await e.loadDefaults();
  await e.load();
  return e;
}

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
});

describe('reading a value', () => {
  /**
   * Typed, then on disk, then shipped. Collapsing the three into one map would
   * lose `isDefault` on the first render, and with it the form's ability to
   * tell a decision from a default.
   */
  it('prefers an edit, then the file, then the shipped default', async () => {
    const e = await editor({ PHP_VERSION: '8.3' }, { PHP_VERSION: '8.4', TZ: 'UTC' });

    expect(e.effective('PHP_VERSION')).toBe('8.3');
    expect(e.effective('TZ'), 'a key only the binary knows').toBe('UTC');
    expect(e.effective('NOTHING_KNOWS_THIS'), 'never undefined').toBe('');

    e.edit('PHP_VERSION', '8.5');
    expect(e.effective('PHP_VERSION')).toBe('8.5');
  });

  it('knows when a value is the shipped default and can go back to it', async () => {
    const e = await editor({ TZ: 'Europe/Istanbul' }, { TZ: 'UTC' });

    expect(e.isDefault('TZ')).toBe(false);
    e.resetToDefault('TZ');
    expect(e.effective('TZ')).toBe('UTC');
    expect(e.isDefault('TZ')).toBe(true);
  });
});

describe('recording a change', () => {
  /**
   * The `delete` in `edit()`. Without it, typing a value and typing the
   * original back leaves the key in the diff: the save button stays lit and the
   * save writes a value identical to the one on disk — which for a routing key
   * would put a "regenerate to apply" notice on screen for a change nobody
   * made.
   */
  it('forgets an edit that returns the value to what the file holds', async () => {
    const e = await editor({ TZ: 'UTC' });

    expect(e.dirty.value).toBe(false);
    e.edit('TZ', 'Europe/Istanbul');
    expect(e.dirty.value).toBe(true);
    expect(e.changedCount.value).toBe(1);

    e.edit('TZ', 'UTC');
    expect(e.dirty.value, 'a round trip left something to write').toBe(false);
    expect(e.changedCount.value).toBe(0);
  });

  /**
   * A key absent from the file is not "unchanged" — it has to be written, or
   * setting a value the binary already ships as a default would save nothing.
   */
  it('records a value for a key the file does not carry', async () => {
    const e = await editor({}, { TZ: 'UTC' });

    e.edit('TZ', 'UTC');
    expect(e.dirty.value, 'writing the default is still a write').toBe(true);
  });
});

describe('the two boolean spellings', () => {
  it('keeps true/false and on/off apart', async () => {
    const e = await editor({ SSL_ENABLE: 'true', SERVER_GZIP: 'off' });

    expect(e.boolOf('SSL_ENABLE')).toBe(true);
    expect(e.onOff('SERVER_GZIP')).toBe(false);

    e.setBool('SSL_ENABLE', false);
    e.setOnOff('SERVER_GZIP', true);

    expect(e.edits.value.SSL_ENABLE, 'compose reads true/false').toBe('false');
    expect(e.edits.value.SERVER_GZIP, 'nginx and php.ini read on/off').toBe('on');
  });

  /** `on` is not truthy to `boolOf`, and `true` is not to `onOff`. */
  it('does not read one spelling as the other', async () => {
    const e = await editor({ A: 'on', B: 'true' });

    expect(e.boolOf('A')).toBe(false);
    expect(e.onOff('B')).toBe(false);
  });
});

describe('list values', () => {
  it('splits, trims and drops the empties', async () => {
    const e = await editor({ EXT: ' gd , mbstring ,,redis ' });
    expect(e.listOf('EXT')).toEqual(['gd', 'mbstring', 'redis']);
  });

  it('writes a list back as a bare comma list', async () => {
    const e = await editor({ EXT: 'gd' });
    e.setList('EXT', [' redis ', '', 'imagick']);
    expect(e.edits.value.EXT).toBe('redis,imagick');
  });

  it('reads an absent key as an empty list rather than throwing', async () => {
    const e = await editor();
    expect(e.listOf('NOT_SET')).toEqual([]);
  });
});

describe('saving', () => {
  it('writes only the diff and re-reads the file afterwards', async () => {
    const e = await editor({ TZ: 'UTC', PHP_VERSION: '8.4' });
    e.edit('TZ', 'Europe/Istanbul');

    const keys = await e.save();

    expect(keys).toEqual(['TZ']);
    expect(
      calls.find(([n]) => n === 'envSet'),
      'sent more than it changed'
    ).toEqual(['envSet', { TZ: 'Europe/Istanbul' }]);
    // Re-read rather than merged locally: the Rust side normalises what it
    // writes, and a local merge would show the user their own input.
    expect(calls.filter(([n]) => n === 'envGet').length).toBeGreaterThan(1);
    expect(e.dirty.value, 'the diff survived the save').toBe(false);
  });

  it('runs the caller-supplied follow-up before it reports success', async () => {
    const e = await editor({ DEFAULT_TLD_SUFFIX: 'stackvo.loc' });
    e.edit('DEFAULT_TLD_SUFFIX', 'dev.loc');

    const order = [];
    await e.save(async () => order.push('follow-up'));
    order.push('returned');

    // The store's cached TLD has to land before the confirmation, or every
    // domain the app shows is the previous suffix until a reload.
    expect(order).toEqual(['follow-up', 'returned']);
    expect(e.saved.value).toBe(true);
  });

  it('keeps the diff and reports the error when the write fails', async () => {
    const e = await editor({ TZ: 'UTC' });
    e.edit('TZ', 'Europe/Istanbul');
    replies.envSet = () => Promise.reject(new Error('read-only workspace'));

    const keys = await e.save();

    expect(keys).toEqual([]);
    expect(e.error.value.message).toBe('read-only workspace');
    expect(e.saved.value, 'a failed save announced itself as done').toBe(false);
    expect(e.dirty.value, "the user's edits were thrown away").toBe(true);
  });
});

describe('what a save still needs', () => {
  /**
   * Changing the suffix rewrites every routing label and moves what the
   * certificate has to cover, and none of it reaches the running stack until
   * the files are regenerated. Saving and staying silent is how a setting looks
   * like it did nothing.
   */
  it('flags a routing change until the regenerate clears it', async () => {
    const e = await editor({ DEFAULT_TLD_SUFFIX: 'stackvo.loc' });

    expect(e.routingChanged.value).toBe(false);
    e.edit('DEFAULT_TLD_SUFFIX', 'dev.loc');
    await e.save();

    expect(e.routingChanged.value).toBe(true);
    expect(e.suffixChanged.value, 'the suffix has its own extra notice').toBe(true);

    e.clearPending();
    expect(e.routingChanged.value).toBe(false);
  });

  it('says nothing about a change that takes effect on its own', async () => {
    const e = await editor({ TZ: 'UTC' });
    e.edit('TZ', 'Europe/Istanbul');
    await e.save();

    expect(e.routingChanged.value).toBe(false);
    expect(e.suffixChanged.value).toBe(false);
  });
});

describe('loading', () => {
  it('reports a failure and leaves an empty file rather than a stale one', async () => {
    replies.envDefaults = { TZ: 'UTC' };
    replies.envGet = () => Promise.reject(new Error('no workspace'));

    const e = useEnvEditor();
    await e.loadDefaults();
    await e.load();

    expect(e.error.value.message).toBe('no workspace');
    // The defaults still answer, so a form opens populated rather than blank.
    expect(e.effective('TZ')).toBe('UTC');
  });

  /** Re-reading has to drop edits, or a discarded change survives a reload. */
  it('clears pending edits on load', async () => {
    const e = await editor({ TZ: 'UTC' });
    e.edit('TZ', 'Europe/Istanbul');
    await e.load();

    expect(e.dirty.value).toBe(false);
  });

  /** Absent defaults are a missing convenience, not a failure to open. */
  it('survives the defaults being unreadable', async () => {
    replies.envDefaults = () => Promise.reject(new Error('binary is odd'));
    replies.envGet = { TZ: 'UTC' };

    const e = useEnvEditor();
    await e.loadDefaults();
    await e.load();

    expect(e.effective('TZ')).toBe('UTC');
    expect(e.error.value).toBe(null);
  });
});
