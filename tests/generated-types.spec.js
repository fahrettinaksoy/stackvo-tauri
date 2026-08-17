import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { tsType, wrappersOf } from '../tools/generate-types.mjs';

/**
 * The contract's prose, read as types. §3 #10.
 *
 * `tools/generate-types.mjs` says why this exists instead of `tauri-specta`;
 * the short version is that there is no TypeScript in this repository, so a
 * generator whose output is a `.ts` module would produce a file nothing reads.
 * A `.d.ts` beside plain JavaScript is read by an editor with no build step.
 *
 * ## What is worth testing here and what is not
 *
 * The generated file itself is checked by `npm run types:check`, in CI, against
 * the contract — a snapshot test of the same bytes would be a second copy of
 * that answer. What this covers is the **converter**, because it is where a
 * silent wrong answer lives: every failure mode below produced a type that
 * compiled, read plausibly, and was wrong.
 */

const ROOT = resolve(import.meta.dirname, '..');
const KNOWN = new Set(['Workspace', 'Instance', 'Worktree']);
const type = (text) => tsType(text, KNOWN).type;

describe('reading a type out of the contract’s prose', () => {
  it('stops at the sentence, which is documentation rather than type', () => {
    expect(type('string — an https:// address or a directory path')).toBe('string');
    expect(type('bool (default false)')).toBe('boolean');
    expect(type('string, and the one the tray shows')).toBe('string');
  });

  it('maps the Rust spellings the contract actually uses', () => {
    for (const rust of ['u8', 'u16', 'u32', 'u64', 'i32', 'i64', 'usize', 'int']) {
      expect(type(rust), rust).toBe('number');
    }
    expect(type('bool')).toBe('boolean');
  });

  it('reads `?` as optional rather than as part of the name', () => {
    expect(tsType('string?', KNOWN)).toEqual({ type: 'string', optional: true });
    expect(tsType('u64?', KNOWN)).toEqual({ type: 'number', optional: true });
  });

  it('keeps a union, including the null half', () => {
    // 16 fields in the contract are spelled `string | null`, and every one of
    // them was `unknown` until the union was split — which is the difference
    // between "this can be absent" and "nobody knows what this is".
    expect(type('string | null')).toBe('string | null');
    expect(type("'php'|'node'|null")).toBe("'php' | 'node' | null");
    expect(type('Worktree | null')).toBe('Worktree | null');
  });

  it('does not split a union that is inside brackets', () => {
    // `Record<a|b, c>` split on its `|` gives two halves of a broken type, and
    // both halves look like types.
    expect(type('Record<string|number, string>')).toBe('Record<string, unknown>');
  });

  it('finds the type in front of a comma that is inside a shape', () => {
    // The bug this repository's contract has nine of: a plain split on `, `
    // cut `Array<{ service: string, name: string }>` after `service: string`,
    // and nine list-shaped returns became `unknown` for a comma.
    expect(type('Array<{ service: string, name: string, bytes: number }>')).toBe(
      'Record<string, unknown>[]'
    );
  });

  it('names a declared type and refuses to invent an undeclared one', () => {
    expect(type('Workspace')).toBe('Workspace');
    expect(type('Instance[]')).toBe('Instance[]');
    // The honest half. `CpuStats` is referenced by the contract and never
    // declared in it — `unknown` says so, where `any` would hide it and a
    // guessed shape would be a lie an editor repeats.
    expect(type('CpuStats')).toBe('unknown');
  });

  it('recurses into a shape written as a nested object', () => {
    // Several contract types describe a child by *being* one. Stringifying it
    // gave `[object Object]` — 16 of them.
    expect(type({ name: 'string', port: 'u16', label: 'string?' })).toBe(
      '{ name: string; port: number; label?: string }'
    );
  });

  it('drops the keys that are documentation living in an object', () => {
    // `contract_version.rs` states the rule for `note`; the contract spells it
    // three more ways, and `$ref` points at a schema rather than at a field.
    expect(type({ name: 'string', note: 'anything at all', effectiveNote: 'prose' })).toBe(
      '{ name: string }'
    );
    expect(type({ name: 'string', $ref: 'project.schema.json' })).toBe('{ name: string }');
  });
});

describe('the wrappers the module actually exports', () => {
  it('reads a member whichever way prettier wrapped it', () => {
    const source = `export const api = {
  short: () => call('short_one'),
  wrapped: (a, b) =>
    call('wrapped_one', { a, b }),
  withComment: () => call('third_one'),
};`;
    expect(wrappersOf(source)).toEqual([
      ['short', 'short_one'],
      ['wrapped', 'wrapped_one'],
      ['withComment', 'third_one'],
    ]);
  });

  it('refuses a module it cannot find the object in', () => {
    // Silently returning nothing would generate an empty api interface, which
    // type-checks and tells an editor that no command exists.
    expect(() => wrappersOf('export const notTheApi = {}')).toThrow(/export const api/);
  });

  it('reads every wrapper the real module exports', () => {
    const found = wrappersOf(readFileSync(resolve(ROOT, 'src/lib/ipc.js'), 'utf8'));
    // The count is held against the tree by `platform_matrix_claims.rs`; this
    // only asserts the parser did not quietly stop matching, which is how a
    // generator produces a smaller file that still passes its own check.
    expect(found.length).toBeGreaterThan(200);
    expect(found).toContainEqual(['marketBundle', 'market_bundle']);
  });
});

describe('the generated file', () => {
  const generated = readFileSync(resolve(ROOT, 'src/lib/ipc.d.ts'), 'utf8');

  it('says it is generated, in the first thing anybody reads', () => {
    expect(generated.slice(0, 200)).toContain('GENERATED');
    expect(generated).toContain('tools/generate-types.mjs');
  });

  it('declares the api and the error shape the front end branches on', () => {
    expect(generated).toContain('export declare const api: StackvoApi;');
    // ADR 0004: `code` is what a caller switches on, and a `.d.ts` that left it
    // out would make the one field the front end branches on invisible.
    expect(generated).toMatch(/class StackvoError[\s\S]*code: string;/);
  });

  it('carries the number it could not read, rather than hiding it', () => {
    expect(generated).toMatch(/\d+ field\(s\) the/);
  });
});
