import { describe, it, expect } from 'vitest';
import {
  blankForm,
  compareVersions,
  formFromManifest,
  formToSpec,
  isIncompatible,
  specsDiffer,
} from '@/lib/manifest';

/**
 * These functions sit between a form and a file that a Bash parser reads with
 * grep. The contract's write rules (contracts/project.schema.json) are not
 * style preferences — breaking one produces a manifest that still parses as
 * JSON and still generates a Dockerfile, just the wrong one. So the round trip
 * is tested as a round trip, not field by field.
 */

describe('compareVersions', () => {
  it('orders by numeric part, not lexically', () => {
    // '8.10' sorts before '8.9' as a string, which is how an extension gets
    // wrongly flagged as removed.
    expect(compareVersions('8.10', '8.9')).toBe(1);
    expect(compareVersions('8.2', '8.2')).toBe(0);
    expect(compareVersions('7.4', '8.0')).toBe(-1);
  });

  it('treats a missing component as zero', () => {
    expect(compareVersions('8', '8.0')).toBe(0);
    expect(compareVersions('8.1', '8')).toBe(1);
  });
});

describe('isIncompatible', () => {
  it('rejects an extension removed at or after the chosen version', () => {
    expect(isIncompatible({ removedIn: '8.0' }, '8.2')).toBe(true);
    expect(isIncompatible({ removedIn: '8.0' }, '8.0')).toBe(true);
    expect(isIncompatible({ removedIn: '8.0' }, '7.4')).toBe(false);
  });

  it('rejects an extension that needs a newer PHP than the one chosen', () => {
    expect(isIncompatible({ minPhp: '8.1' }, '8.0')).toBe(true);
    expect(isIncompatible({ minPhp: '8.1' }, '8.1')).toBe(false);
  });

  it('judges nothing before a version has been chosen', () => {
    expect(isIncompatible({ minPhp: '8.1' }, '')).toBe(false);
  });
});

describe('formToSpec', () => {
  it('emits exactly one runtime block (W-02)', () => {
    const php = formToSpec({ ...blankForm(), name: 'shop', phpVersion: '8.2' });
    expect(php.php).toBeDefined();
    expect(php.node).toBeUndefined();

    const node = formToSpec({ ...blankForm(), name: 'app', runtime: 'node', nodeVersion: '22' });
    expect(node.node).toBeDefined();
    expect(node.php).toBeUndefined();
    // The PHP-only keys are forbidden alongside a node block by the schema.
    expect(node.server).toBeUndefined();
    expect(node.document_root).toBeUndefined();
  });

  it('writes document_root in the file spelling, not the manifest reader’s', () => {
    const spec = formToSpec({ ...blankForm(), name: 'shop', documentRoot: 'web' });
    expect(spec.document_root).toBe('web');
    expect(spec.documentRoot).toBeUndefined();
  });

  it('derives the domain from the name only when one was not given', () => {
    expect(formToSpec({ ...blankForm(), name: 'shop' }).domain).toBe('shop.loc');
    expect(formToSpec({ ...blankForm(), name: 'shop', domain: 'buy.test' }).domain).toBe(
      'buy.test'
    );
  });

  it('omits an empty build command rather than writing a blank one', () => {
    const form = { ...blankForm(), name: 'app', runtime: 'node', nodeVersion: '22', build: '' };
    expect('build' in formToSpec(form).node).toBe(false);
    expect(formToSpec({ ...form, build: 'npm run build' }).node.build).toBe('npm run build');
  });

  it('sends the port as a number, however the text field held it', () => {
    const form = { ...blankForm(), name: 'app', runtime: 'node', nodeVersion: '22', port: '3000' };
    expect(formToSpec(form).node.port).toBe(3000);
  });

  it('copies the extension array instead of aliasing the form’s', () => {
    const form = { ...blankForm(), name: 'shop', phpVersion: '8.2', extensions: ['gd'] };
    const spec = formToSpec(form);
    form.extensions.push('redis');
    expect(spec.php.extensions).toEqual(['gd']);
  });
});

describe('formFromManifest', () => {
  it('round-trips a PHP manifest back to the same spec', () => {
    // Shaped as Rust serialises it: camelCase, with the reader's diagnostics.
    const manifest = {
      name: 'shop',
      domain: 'shop.loc',
      runtime: 'php',
      server: 'apache',
      documentRoot: 'web',
      php: { version: '8.2', extensions: ['mbstring', 'pdo'] },
      node: null,
      valid: true,
      errors: [],
      warnings: [],
    };
    expect(formToSpec(formFromManifest(manifest))).toEqual({
      name: 'shop',
      domain: 'shop.loc',
      runtime: 'php',
      server: 'apache',
      document_root: 'web',
      php: { version: '8.2', extensions: ['mbstring', 'pdo'] },
    });
  });

  it('round-trips a node manifest, keeping an absent build absent', () => {
    const manifest = {
      name: 'app',
      domain: 'app.loc',
      runtime: 'node',
      server: null,
      documentRoot: null,
      php: null,
      node: { version: '20', install: 'pnpm i', start: 'node server.js', port: 4000 },
      valid: true,
      errors: [],
      warnings: [],
    };
    const spec = formToSpec(formFromManifest(manifest));
    expect(spec.node).toEqual({
      version: '20',
      install: 'pnpm i',
      start: 'node server.js',
      port: 4000,
    });
    expect('build' in spec.node).toBe(false);
  });

  it('leaves the other runtime on defaults so switching lands somewhere valid', () => {
    const form = formFromManifest({
      name: 'shop',
      domain: 'shop.loc',
      runtime: 'php',
      php: { version: '8.2', extensions: [] },
    });
    // Detection can be wrong, and the fix is to switch runtime here. Landing on
    // an empty version and an empty start command would just move the problem.
    expect(form.install).toBe('npm install');
    expect(form.port).toBe(3000);
    expect(form.start).not.toBe('');
  });

  it('survives a manifest with nothing in it', () => {
    expect(formFromManifest(null)).toEqual(blankForm());
    expect(formFromManifest({}).runtime).toBe('php');
  });

  it('keeps the project’s own extension list rather than a default set', () => {
    const form = formFromManifest({
      name: 'shop',
      runtime: 'php',
      php: { version: '8.2', extensions: [] },
    });
    // An empty list is a choice the user made; replacing it with the built-in
    // default set on load would silently reinstate seven extensions on save.
    expect(form.extensions).toEqual([]);
  });
});

describe('specsDiffer', () => {
  it('sees no change when the form was edited back to where it started', () => {
    const form = { ...blankForm(), name: 'shop', phpVersion: '8.2' };
    const original = formToSpec(form);
    expect(specsDiffer(original, formToSpec({ ...form, build: 'typed then cleared' }))).toBe(false);
  });

  it('sees a real edit', () => {
    const form = { ...blankForm(), name: 'shop', phpVersion: '8.2' };
    expect(specsDiffer(formToSpec(form), formToSpec({ ...form, phpVersion: '8.3' }))).toBe(true);
  });
});
