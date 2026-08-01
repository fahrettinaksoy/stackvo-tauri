import { describe, it, expect } from 'vitest';
import { LEVELS, countByLevel, filterLines, highlight, parseLevel, withLevels } from '@/lib/logs';

/**
 * The fixtures are real lines, trimmed, from the logs on the machine this was
 * built against — a Laravel channel, a Monolog JSON channel and supervisord.
 * Invented samples would only prove the regexes match the regexes.
 */

const LARAVEL = '[2026-07-20 14:19:58] laravel.EMERGENCY: Unable to create configured logger.';
const LARAVEL_ERROR = '[2026-07-13 12:48:25] production.ERROR: Undefined array key "id"';
const STACK_HEAD = '[stacktrace]';
const STACK_FRAME =
  '#0 /var/www/html/vendor/laravel/framework/src/Illuminate/Log/LogManager.php(143)';
const JSON_INFO =
  '{"message":"news.relation.declared_parent_linked","context":{"agency":"aa"},"level":200,"level_name":"INFO"}';
const SUPERVISORD_INFO = "2026-07-01 09:33:39,696 INFO spawned: 'nginx' with pid 10";
const SUPERVISORD_WARN = '2026-07-01 09:33:40,800 WARN exited: nginx (exit status 1)';
const NGINX = '2026/07/29 10:00:00 [error] 123#123: *1 open() failed';
const PHP_FATAL = '[29-Jul-2026 10:00:00] PHP Fatal error:  Uncaught Error';
const SUPERVISOR_BARE = "supervisor: couldn't chdir to /var/www/parser.ajans: ENOENT";

describe('parseLevel', () => {
  it('reads the Monolog line format', () => {
    expect(parseLevel(LARAVEL)).toBe('critical');
    expect(parseLevel(LARAVEL_ERROR)).toBe('error');
  });

  it('reads the Monolog JSON format by number and by name', () => {
    expect(parseLevel(JSON_INFO)).toBe('info');
    expect(parseLevel('{"level":400,"message":"x"}')).toBe('error');
    expect(parseLevel('{"level_name":"WARNING","message":"x"}')).toBe('warning');
  });

  it('reads nginx, PHP and supervisord', () => {
    expect(parseLevel(NGINX)).toBe('error');
    expect(parseLevel(PHP_FATAL)).toBe('critical');
    expect(parseLevel(SUPERVISORD_INFO)).toBe('info');
    expect(parseLevel(SUPERVISORD_WARN)).toBe('warning');
  });

  it('returns null rather than guessing', () => {
    // These are the lines a stack trace is made of. Calling them `info` would
    // be inventing a severity for text that declared none.
    expect(parseLevel(STACK_HEAD)).toBeNull();
    expect(parseLevel(STACK_FRAME)).toBeNull();
    expect(parseLevel(SUPERVISOR_BARE)).toBeNull();
    expect(parseLevel('')).toBeNull();
    expect(parseLevel(undefined)).toBeNull();
  });

  it('does not let a JSON message body masquerade as another format', () => {
    // The message contains a line that would otherwise parse as ERROR.
    const line = '{"message":"[2026-07-20 14:19:58] laravel.ERROR: x","level":100}';
    expect(parseLevel(line)).toBe('debug');
  });

  it('survives a truncated JSON line', () => {
    expect(parseLevel('{"message":"cut off here')).toBeNull();
  });

  it('maps every level it can return into the offered set', () => {
    const produced = [
      LARAVEL,
      LARAVEL_ERROR,
      JSON_INFO,
      NGINX,
      PHP_FATAL,
      SUPERVISORD_INFO,
      SUPERVISORD_WARN,
    ].map(parseLevel);
    for (const level of produced) expect(LEVELS).toContain(level);
  });
});

describe('withLevels', () => {
  it('gives a stack trace the level of the entry it belongs to', () => {
    const lines = [LARAVEL_ERROR, STACK_HEAD, STACK_FRAME].map((text) => ({ text }));
    const tagged = withLevels(lines);
    expect(tagged.map((l) => l.level)).toEqual(['error', 'error', 'error']);
    // Only the first line actually declared it — the UI uses this to tell an
    // entry from its continuation.
    expect(tagged.map((l) => l.startsEntry)).toEqual([true, false, false]);
  });

  it('leaves lines before the first declared level unlabelled', () => {
    // The buffer starts mid-file, so there is genuinely nothing to inherit.
    const tagged = withLevels([{ text: STACK_FRAME }, { text: LARAVEL_ERROR }]);
    expect(tagged[0].level).toBeNull();
    expect(tagged[1].level).toBe('error');
  });

  it('switches level at the next entry', () => {
    const tagged = withLevels(
      [LARAVEL_ERROR, STACK_FRAME, SUPERVISORD_INFO, SUPERVISOR_BARE].map((text) => ({ text }))
    );
    expect(tagged.map((l) => l.level)).toEqual(['error', 'error', 'info', 'info']);
  });

  /**
   * The cross-project tail interleaves sixty files into one buffer, so the line
   * above a stack frame is routinely from a different project. Inheriting from
   * it would paint one project's line with another project's severity — and
   * then hide it under a level filter that has no idea it is looking at the
   * wrong file.
   */
  it('does not let one origin inherit another origin s level', () => {
    const tagged = withLevels([
      { text: LARAVEL_ERROR, origin: 'shop' },
      { text: SUPERVISOR_BARE, origin: 'blog' },
      { text: STACK_FRAME, origin: 'shop' },
    ]);

    expect(tagged.map((l) => l.level)).toEqual(['error', null, 'error']);
  });

  it('keeps a single stream s inheritance unchanged when no origin is given', () => {
    // Every line shares the one empty origin, which is the plain running level.
    const tagged = withLevels([{ text: LARAVEL_ERROR }, { text: STACK_FRAME }]);
    expect(tagged.map((l) => l.level)).toEqual(['error', 'error']);
  });
});

describe('filterLines', () => {
  const tagged = withLevels(
    [LARAVEL_ERROR, STACK_HEAD, STACK_FRAME, SUPERVISORD_INFO].map((text) => ({ text }))
  );

  it('keeps a whole entry, not just its first line', () => {
    // Filtering to errors and getting back the one line that says something
    // broke — without the lines that say where — is the failure this prevents.
    const errors = filterLines(tagged, { levels: ['error'] });
    expect(errors).toHaveLength(3);
    expect(errors.some((l) => l.text === STACK_FRAME)).toBe(true);
  });

  it('matches text case-insensitively', () => {
    expect(filterLines(tagged, { query: 'UNDEFINED' })).toHaveLength(1);
    expect(filterLines(tagged, { query: 'nothing here' })).toHaveLength(0);
  });

  it('treats a regex as literal text rather than throwing', () => {
    // A half-typed regex must not break the box you are typing it into.
    expect(() => filterLines(tagged, { query: '[(' })).not.toThrow();
    expect(filterLines(tagged, { query: '[stack' })).toHaveLength(1);
  });

  it('combines text and level', () => {
    expect(filterLines(tagged, { levels: ['error'], query: 'LogManager' })).toHaveLength(1);
  });

  it('returns everything when nothing is asked for', () => {
    expect(filterLines(tagged, {})).toHaveLength(4);
    expect(filterLines(tagged)).toHaveLength(4);
  });

  it('keeps unlabelled lines under a level filter', () => {
    // An unrecognised format is exactly when the raw text matters most; hiding
    // it because it declared no level would be the wrong way round.
    const unknown = withLevels([{ text: SUPERVISOR_BARE }]);
    expect(filterLines(unknown, { levels: ['error'] })).toHaveLength(1);
  });
});

describe('countByLevel', () => {
  it('counts every level it offers, including the empty ones', () => {
    const counts = countByLevel(withLevels([{ text: LARAVEL_ERROR }, { text: STACK_FRAME }]));
    expect(counts.error).toBe(2);
    expect(counts.debug).toBe(0);
    expect(Object.keys(counts).sort()).toEqual([...LEVELS].sort());
  });
});

describe('search modes', () => {
  it('treats a plain query as literal text, not a pattern', () => {
    const lines = [{ text: 'GET /a.b' }, { text: 'GET /axb' }];
    const hit = filterLines(lines, { query: 'a.b' });
    expect(hit).toHaveLength(1);
    expect(hit[0].text).toBe('GET /a.b');
  });

  it('matches with a regex when asked, and matches nothing while one is half-typed', () => {
    const lines = [{ text: 'status=500' }, { text: 'status=200' }];
    expect(filterLines(lines, { query: 'status=5\\d\\d', regex: true })).toHaveLength(1);
    // `(` is what every regex looks like one keystroke in. Flashing the whole
    // buffer back mid-word is worse than showing nothing.
    expect(filterLines(lines, { query: 'status=(', regex: true })).toHaveLength(0);
  });

  it('splits a line around its matches so they can be marked', () => {
    const parts = highlight('GET /users 500', '500');
    expect(parts.map((p) => p.text).join('')).toBe('GET /users 500');
    expect(parts.filter((p) => p.hit).map((p) => p.text)).toEqual(['500']);
  });

  it('does not hang on a zero-width match', () => {
    // `a*` matches the empty string at every position; advancing lastIndex by
    // hand is what keeps this from looping forever on a legal pattern.
    const parts = highlight('bbb', 'a*', true);
    expect(parts.map((p) => p.text).join('')).toBe('bbb');
  });
});
