#!/usr/bin/env node
/**
 * The front end's types, generated from the contract.
 *
 *   node tools/generate-types.mjs           # write src/lib/ipc.d.ts
 *   node tools/generate-types.mjs --check   # fail if it is not current
 *
 * ## Why this and not `tauri-specta` (§3 #10)
 *
 * ADR 0006 measured `tauri-specta` and deferred it, on the cost of changing how
 * every command is declared. Re-measured before this was written, and one fact
 * settles it for this repository rather than merely delaying it:
 *
 * **There is no TypeScript here.** No `tsconfig.json`, no `.ts` file, no
 * `lang="ts"` in any of the 135 `.js`/`.vue` sources. `tauri-specta`'s output is
 * a TypeScript module of typed `invoke` wrappers — in a project with no
 * compiler configured, that is a file nothing reads and nothing checks, bought
 * with three new crates and an attribute on 245 command functions.
 *
 * The gap §3 #10 names is "the front end stays untyped". The generator was
 * never what was missing; the *types* were. And the contract already is the
 * single source of truth, already hand-written by ADR 0006's decision, and
 * already gated against the implementation by `contract_agreement.rs`. So the
 * types come from there, into a `.d.ts` — which editors apply to plain
 * JavaScript with no build step, no compiler and no file renamed.
 *
 * ## What this does not do, and it is the honest half
 *
 * A `.d.ts` beside JavaScript is **editor** typing: autocompletion, argument
 * names, a red squiggle on a misspelled command. It is not a build gate. Making
 * it one means `tsc --noEmit --checkJs` over 135 files, which is a cleanup with
 * its own size and its own decision. This is the half that has no prerequisite.
 *
 * ## The types are as good as the contract's prose, and that is measured
 *
 * `contracts/ipc.json` describes arguments as prose that begins with a type:
 * `"string — an https:// address"`, `"bool (default false)"`, `"string?"`. The
 * leading token is parsed and the rest is kept as the doc comment. Anything
 * this cannot read becomes `unknown` rather than a guess, and the count of
 * those is written into the generated header — a number that goes up is a
 * contract entry somebody wrote in a new shape, and it should be visible rather
 * than silently typed as `any`.
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CONTRACT = resolve(ROOT, 'contracts/ipc.json');
const WRAPPERS = resolve(ROOT, 'src/lib/ipc.js');
const OUTPUT = resolve(ROOT, 'src/lib/ipc.d.ts');

/**
 * What this could not read, by the text it could not read.
 *
 * A list rather than a count, and `--report` prints it. Every entry is a
 * contract field whose description does not begin with a type — which is worth
 * seeing, because the contract is hand-written (ADR 0006) and a field nobody
 * gave a type to is a field the front end was never going to be told about.
 */
const unread = [];

/**
 * A key that documents rather than describes a field.
 *
 * `contract_version.rs` states the rule for `note`: "a `note` key inside a type
 * is documentation that happens to live in an object". The contract spells it
 * three more ways — `reloadedNote`, `extensionsNote`, `effectiveNote` — and
 * typing those produced three properties no payload has ever carried.
 *
 * `$ref` belongs here for the same reason one level up: it points at a JSON
 * Schema, it is not a field. A type whose *only* key is `$ref` becomes an alias
 * instead; this covers the ones that carry it alongside real fields.
 */
const documentation = (field) =>
  field.startsWith('_') || field === '$ref' || /(^|[a-z])[Nn]ote$/.test(field);

/** Rust and contract spellings that are one of TypeScript's scalars. */
const SCALARS = {
  string: 'string',
  boolean: 'boolean',
  bool: 'boolean',
  number: 'number',
  int: 'number',
  u8: 'number',
  u16: 'number',
  u32: 'number',
  u64: 'number',
  i32: 'number',
  i64: 'number',
  f32: 'number',
  f64: 'number',
  usize: 'number',
  null: 'null',
  object: 'Record<string, unknown>',
};

/**
 * The type at the front of a description, and nothing after it.
 *
 * The separators are an em dash, an opening parenthesis, a comma or a spaced
 * hyphen — but only **outside** brackets. A plain split found the first comma
 * of `Array<{ service: string, name: string }>` and produced `Array<{ service:
 * string`, which is not a type, so nine list-shaped returns landed as
 * `unknown`. The depth counter is the whole fix.
 */
function headOf(raw) {
  let depth = 0;
  for (let i = 0; i < raw.length; i += 1) {
    const ch = raw[i];
    if (ch === '<' || ch === '{' || ch === '[' || ch === '(') depth += 1;
    else if (ch === '>' || ch === '}' || ch === ']' || ch === ')') depth -= 1;

    if (depth > 0) continue;
    const rest = raw.slice(i);
    // An opening parenthesis at depth 0 has just been counted, so it reads as
    // depth 1 here; the separators below are the ones that end a type.
    if (/^\s+—\s+/.test(rest) || /^,\s/.test(rest) || /^\s+-\s+/.test(rest)) {
      return raw.slice(0, i).trim();
    }
    if (/^\s+\(/.test(rest)) return raw.slice(0, i).trim();
  }
  return raw.trim();
}

/**
 * One contract type as TypeScript.
 *
 * Reads the leading token and stops. `"string — an https:// address or a
 * directory path"` is `string`; the sentence after it is documentation and goes
 * in the doc comment, where a person reads it.
 *
 * Takes the raw value, not a string: several types in the contract describe a
 * nested object by *being* one, and a converter that stringified it would type
 * a real shape as `[object Object]`. Those recurse.
 */
export function tsType(text, known = new Set(), where = '') {
  // A nested shape, written as a nested object. Recursing produces an inline
  // type rather than an index signature, so the fields survive.
  if (text && typeof text === 'object' && !Array.isArray(text)) {
    const fields = Object.entries(text)
      .filter(([field]) => !documentation(field))
      .map(([field, value]) => {
        const inner = tsType(value, known, `${where}.${field}`);
        return `${field}${inner.optional ? '?' : ''}: ${inner.type}`;
      });
    return { type: `{ ${fields.join('; ')} }`, optional: false };
  }

  const raw = String(text).trim();
  const head = headOf(raw);

  const optional = head.endsWith('?');
  const base = optional ? head.slice(0, -1) : head;

  /** One member of a union, or the whole thing when there is no `|`. */
  const one = (part) => {
    const value = part.trim();
    if (!value) return null;

    // A string literal, in either quoting the contract uses.
    if (/^'[^']*'$/.test(value)) return value;
    if (/^"[^"]*"$/.test(value)) return `'${value.slice(1, -1)}'`;

    if (Object.hasOwn(SCALARS, value)) return SCALARS[value];

    // `T[]`, for any `T` this can read.
    if (value.endsWith('[]')) {
      const inner = one(value.slice(0, -2));
      // `A | B` inside an array needs the parentheses back.
      if (inner) return /[ |]/.test(inner) ? `(${inner})[]` : `${inner}[]`;
      return null;
    }

    // A named type the contract declares.
    if (known.has(value)) return value;

    // `Record<…>`, `map<…>` and inline object shapes written as prose. Real,
    // but not worth a parser — an index signature is true and useful where
    // `any` is neither.
    // `Array<{ … }>`, which the contract uses for a list of inline shapes.
    const array = value.match(/^Array<(.*)>$/is);
    if (array) {
      const inner = one(array[1]);
      return `${inner && !/[ |]/.test(inner) ? inner : 'Record<string, unknown>'}[]`;
    }

    if (/^(Record|map)</i.test(value) || value.startsWith('{') || value.startsWith('[')) {
      return 'Record<string, unknown>';
    }

    return null;
  };

  // Split on top-level `|` only: a union inside `Record<a|b, c>` is not this
  // one, and splitting through it would produce two halves of a broken type.
  const parts = [];
  let depth = 0;
  let current = '';
  for (const ch of base) {
    if (ch === '<' || ch === '{' || ch === '[') depth += 1;
    if (ch === '>' || ch === '}' || ch === ']') depth -= 1;
    if (ch === '|' && depth === 0) {
      parts.push(current);
      current = '';
    } else current += ch;
  }
  parts.push(current);

  const mapped = parts.map(one);
  if (mapped.some((part) => part === null)) {
    unread.push({ where, text: raw.slice(0, 80) });
    return { type: 'unknown', optional };
  }

  return { type: [...new Set(mapped)].join(' | '), optional };
}

/**
 * The wrappers `ipc.js` actually exports, and the command each one calls.
 *
 * Parsed rather than derived from the contract: the contract has commands with
 * no wrapper (the desktop-only four, among others), and declaring a method the
 * module does not export would be a type that lies in the direction that costs
 * the most — an editor offering a call that fails at run time.
 *
 * Two-space indent and a `call('…')` somewhere in the member. Prettier splits
 * long members across lines, so the value is read up to the next member rather
 * than to the end of the line.
 */
export function wrappersOf(source) {
  const body = source.split('export const api = {')[1];
  if (!body) throw new Error('src/lib/ipc.js no longer declares `export const api = {`');

  const out = [];
  const members = body.split(/\n(?=\s{2}[A-Za-z0-9_]+:)/);
  for (const member of members) {
    const name = member.match(/^\s{2}([A-Za-z0-9_]+):/);
    const command = member.match(/call\(\s*'([a-z0-9_]+)'/);
    if (name && command) out.push([name[1], command[1]]);
  }
  return out;
}

/** A doc comment from whatever prose the contract carries. */
function doc(lines, indent = '  ') {
  const kept = lines.filter(Boolean).flatMap((line) => String(line).split('\n'));
  if (!kept.length) return '';
  if (kept.length === 1 && kept[0].length < 90) return `${indent}/** ${kept[0]} */\n`;
  return `${indent}/**\n${kept.map((l) => `${indent} * ${l}`).join('\n')}\n${indent} */\n`;
}

function generate() {
  const contract = JSON.parse(readFileSync(CONTRACT, 'utf8'));
  const known = new Set(Object.keys(contract.types ?? {}));
  unread.length = 0;

  const parts = [];

  // ---- the named types -------------------------------------------------
  const types = [];
  for (const [name, shape] of Object.entries(contract.types ?? {})) {
    if (typeof shape !== 'object' || shape === null || Array.isArray(shape)) continue;

    // A type that is only a `$ref` is defined by a JSON Schema rather than
    // here. An interface with a `$ref` property would be a shape no payload
    // has; an alias with the schema named is what a reader can follow.
    if (Object.keys(shape).length === 1 && shape.$ref) {
      types.push(
        `/** Defined by \`${shape.$ref}\` — this contract only names it. */\n` +
          `export type ${name} = Record<string, unknown>;`
      );
      continue;
    }

    // `"...": "every field of Worktree, flattened"` is inheritance written in
    // prose. Emitting it as a property called `...` would be nonsense; reading
    // it as `extends` is what it says.
    const spread =
      typeof shape['...'] === 'string' ? shape['...'].match(/every field of (\w+)/) : null;
    const extend = spread && known.has(spread[1]) ? ` extends ${spread[1]}` : '';

    const fields = [];
    for (const [field, description] of Object.entries(shape)) {
      if (field === '...') continue;
      // `note` keys are documentation that happens to live in an object —
      // `contract_version.rs` says so, and typing one as a field would put a
      // property on the interface that no payload ever carries.
      if (documentation(field)) continue;
      const { type, optional } = tsType(description, known, `${name}.${field}`);
      // A nested shape is its own documentation; repeating `[object Object]`
      // above it would be a comment that says less than the type.
      const prose = typeof description === 'string' ? [description] : [];
      fields.push(`${doc(prose, '    ')}    ${field}${optional ? '?' : ''}: ${type};`);
    }
    types.push(`export interface ${name}${extend} {\n${fields.join('\n')}\n}`);
  }

  // ---- the api ---------------------------------------------------------
  const commands = contract.commands ?? {};
  const wrappers = wrappersOf(readFileSync(WRAPPERS, 'utf8'));
  const methods = [];
  let unbacked = 0;

  for (const [js, command] of wrappers) {
    const entry = commands[command];
    if (!entry) {
      // `contract_agreement.rs` is the gate for this; here it is only a reason
      // to type the method as taking anything rather than to invent a shape.
      unbacked += 1;
      methods.push(`  ${js}(...args: unknown[]): Promise<unknown>;`);
      continue;
    }

    const args = Object.entries(entry.args ?? {}).map(([arg, description]) => {
      const { type, optional } = tsType(description, known, `${command}(${arg})`);
      return `${arg}${optional ? '?' : ''}: ${type}`;
    });

    const returns =
      entry.returns && entry.returns !== 'void'
        ? tsType(entry.returns, known, `${command}()`).type
        : 'void';

    methods.push(
      doc([entry.why, entry.returnsNote].filter(Boolean)) +
        `  ${js}(${args.join(', ')}): Promise<${returns}>;`
    );
  }

  const header = `/**
 * GENERATED — do not edit. \`node tools/generate-types.mjs\`
 *
 * The IPC surface as types, from \`contracts/ipc.json\`. §3 #10.
 *
 * Applies to plain JavaScript: an editor reads this beside \`ipc.js\` and offers
 * the argument names, the return shape and a complaint about a method that does
 * not exist. There is no compiler in this project and this does not add one —
 * \`tools/generate-types.mjs\` says what that would take and why it is separate.
 *
 * Measured at generation: ${Object.keys(contract.types ?? {}).length} named types, ${wrappers.length} wrappers, ${unread.length} field(s) the
 * contract's prose could not be read as a type (typed \`unknown\`)${unbacked ? `, ${unbacked} wrapper(s) with no contract entry` : ''}.
 */
`;

  parts.push(header);
  parts.push(types.join('\n\n'));
  parts.push('');
  parts.push(`export interface StackvoApi {\n${methods.join('\n')}\n}`);
  parts.push('');
  parts.push('export declare const api: StackvoApi;');
  parts.push('');
  parts.push(`export declare class StackvoError extends Error {
  code: string;
  hint?: string;
  hintKey?: string;
  details?: Record<string, unknown>;
}`);
  parts.push('');
  parts.push(`export declare function call<T = unknown>(
  command: string,
  args?: Record<string, unknown>
): Promise<T>;`);
  parts.push('');
  parts.push('export declare function asList<T>(value: T[] | null | undefined): T[];');
  parts.push('');

  return parts.join('\n');
}

/**
 * Only when run, never when imported.
 *
 * `tests/generated-types.spec.js` imports `tsType` and `wrappersOf` to test
 * them. Without this guard that import *rewrote* `src/lib/ipc.d.ts` as a side
 * effect of running the test suite — so the file was regenerated by whichever
 * ran last, and `--check` in CI would have been comparing against something a
 * test wrote.
 */
const invoked = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (invoked) main();

function main() {
  const generated = generate();

  // What the contract did not say. Printed on request rather than always: it is a
  // list of contract entries to improve, not an error — a field with no type in
  // its description is still a field that works, it is only one the editor cannot
  // help with.
  if (process.argv.includes('--report')) {
    console.log(`${unread.length} field(s) with no readable type:`);
    for (const { where, text } of unread) console.log(`  ${where}\n    ${text}`);

    // The finding that came out of building this. Most of what is left is not
    // prose the parser cannot read — it is a **type name the contract uses and
    // never declares**. Nothing noticed before because nothing consumed the type
    // table; `contract_agreement.rs` checks that commands and their arguments
    // agree with the code, not that every name in `returns` exists.
    const missing = [
      ...new Set(
        unread
          .map(({ text }) => text.trim().replace(/\[\]$/, ''))
          .filter((name) => /^[A-Z][A-Za-z0-9]*$/.test(name))
      ),
    ].sort();
    if (missing.length) {
      console.log(
        `\n${missing.length} type(s) referenced by the contract and never declared in its \`types\`:`
      );
      console.log(`  ${missing.join(', ')}`);
    }
    process.exit(0);
  }

  if (process.argv.includes('--check')) {
    let current = '';
    try {
      current = readFileSync(OUTPUT, 'utf8');
    } catch {
      console.error('src/lib/ipc.d.ts is missing. Run: node tools/generate-types.mjs');
      process.exit(1);
    }
    if (current !== generated) {
      console.error(
        'src/lib/ipc.d.ts is out of date with contracts/ipc.json.\n' +
          'Run: node tools/generate-types.mjs'
      );
      process.exit(1);
    }
    console.log('src/lib/ipc.d.ts matches the contract');
  } else {
    writeFileSync(OUTPUT, generated);
    console.log(`wrote ${OUTPUT}`);
  }
}
