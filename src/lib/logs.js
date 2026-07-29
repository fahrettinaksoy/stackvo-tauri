/**
 * Reading a level out of a log line, and filtering on it.
 *
 * Done on the client because filtering has to be instant on a buffer that is
 * already in memory — a round trip per keystroke would be slower than the
 * scroll it replaces.
 *
 * The formats here are the ones on the checkout this was written against, not
 * a survey of what Monolog can emit:
 *
 *   [2026-07-20 14:19:58] laravel.ERROR: message      Monolog, line format
 *   {"message":"…","level":200,"level_name":"INFO"}   Monolog, JSON format
 *   2026/07/29 10:00:00 [error] 123#123: message      nginx
 *   [29-Jul-2026 10:00:00] PHP Fatal error:  …        PHP / php-fpm
 *   2026-07-29 10:00:00,123 INFO spawned: …           supervisord
 *
 * Anything unrecognised gets `null`, which is a real answer and not the same as
 * `info`. Guessing a level for a line that never declared one is how a filter
 * starts hiding the line somebody was looking for.
 */

/** Ordered least to most severe; the UI shows them in this order. */
export const LEVELS = ['debug', 'info', 'notice', 'warning', 'error', 'critical'];

/**
 * Monolog's numeric levels, for the JSON format. PSR-3 severities, where a
 * bigger number is worse.
 */
const NUMERIC = [
  [600, 'critical'], // EMERGENCY
  [550, 'critical'], // ALERT
  [500, 'critical'], // CRITICAL
  [400, 'error'],
  [300, 'warning'],
  [250, 'notice'],
  [200, 'info'],
  [100, 'debug'],
];

/** Words seen in the wild, mapped onto the six the UI offers. */
const WORDS = {
  emergency: 'critical',
  alert: 'critical',
  critical: 'critical',
  crit: 'critical',
  fatal: 'critical',
  error: 'error',
  err: 'error',
  warning: 'warning',
  warn: 'warning',
  notice: 'notice',
  info: 'info',
  information: 'info',
  debug: 'debug',
  trace: 'debug',
};

function fromWord(word) {
  return WORDS[String(word).toLowerCase()] ?? null;
}

function fromNumber(value) {
  for (const [threshold, level] of NUMERIC) {
    if (value >= threshold) return level;
  }
  return 'debug';
}

/**
 * The level a line declares, or null when it declares none.
 *
 * Null is also the answer for a continuation line — the second line of a stack
 * trace is part of the entry above it and says nothing about severity on its
 * own. `withLevels` is what resolves that.
 */
export function parseLevel(line) {
  if (typeof line !== 'string' || !line) return null;

  // Monolog's JSON format. Checked first: a JSON line can contain anything at
  // all inside its message, including text that looks like another format.
  if (line.charCodeAt(0) === 123 /* { */) {
    try {
      const entry = JSON.parse(line);
      if (entry && typeof entry === 'object') {
        if (entry.level_name) return fromWord(entry.level_name);
        if (typeof entry.level === 'number') return fromNumber(entry.level);
        if (typeof entry.level === 'string') return fromWord(entry.level);
      }
    } catch {
      // A truncated JSON line is not a level; fall through to the patterns.
    }
    return null;
  }

  // Monolog line format: [date] channel.LEVEL: message
  const monolog = line.match(/^\[[^\]]+\]\s+[\w-]+\.([A-Z]+):/);
  if (monolog) return fromWord(monolog[1]);

  // nginx: date [level] pid#tid: message
  const nginx = line.match(/^\d{4}\/\d{2}\/\d{2}[^[]*\[(\w+)\]/);
  if (nginx) return fromWord(nginx[1]);

  // PHP and php-fpm: [date] PHP Fatal error: …, or a bare NOTICE:/WARNING:
  const php = line.match(/^\[[^\]]+\]\s+PHP\s+(\w+)/i);
  if (php) return fromWord(php[1]);

  const bare = line.match(
    /^\s*(EMERGENCY|ALERT|CRITICAL|CRIT|FATAL|ERROR|WARNING|WARN|NOTICE|INFO|DEBUG)\b[:\s]/i
  );
  if (bare) return fromWord(bare[1]);

  // supervisord: date,ms LEVEL message
  const supervisord = line.match(/^\d{4}-\d{2}-\d{2}\s[\d:,]+\s+(\w+)\s/);
  if (supervisord) return fromWord(supervisord[1]);

  return null;
}

/**
 * Attach a level to every line, letting continuations inherit.
 *
 * A stack trace is a dozen lines that declare nothing, under one line that
 * declared ERROR. Treating them as level-less would drop the whole trace the
 * moment anyone filtered to errors — leaving the one line that says something
 * went wrong and none of the lines that say where. So an undeclared line takes
 * the level of the entry it follows.
 *
 * Lines before the first declared level keep `null`, because there is genuinely
 * nothing to inherit: the buffer starts mid-file.
 */
export function withLevels(lines) {
  let current = null;
  return lines.map((line) => {
    const declared = parseLevel(line.text);
    if (declared) current = declared;
    return { ...line, level: declared ?? current, startsEntry: !!declared };
  });
}

/**
 * Filter by free text and by level.
 *
 * The text match is a plain case-insensitive substring, not a regular
 * expression: a half-typed regex throws or matches nothing, and a search box
 * that breaks while you are still typing in it is worse than one that cannot
 * express `\d+`.
 */
export function filterLines(lines, { query = '', levels = [] } = {}) {
  const needle = query.trim().toLowerCase();
  const wanted = levels.length ? new Set(levels) : null;

  return lines.filter((line) => {
    // A line with no level survives a level filter only when nothing above it
    // claimed one — see `withLevels`. Dropping it would hide the head of a file
    // whose format this does not recognise, which is the case where the user
    // most needs to see the raw text.
    if (wanted && line.level && !wanted.has(line.level)) return false;
    if (needle && !line.text.toLowerCase().includes(needle)) return false;
    return true;
  });
}

/** How many lines carry each level, for the filter's counts. */
export function countByLevel(lines) {
  const counts = Object.fromEntries(LEVELS.map((l) => [l, 0]));
  for (const line of lines) {
    if (line.level && counts[line.level] !== undefined) counts[line.level] += 1;
  }
  return counts;
}
