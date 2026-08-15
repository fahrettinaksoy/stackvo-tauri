import { describe, it, expect } from 'vitest';
import {
  blankForm,
  domainAdvice,
  domainSuggestions,
  humaniseField,
  parentDomain,
  compareVersions,
  formFromManifest,
  formToSpec,
  isIncompatible,
  specsDiffer,
  LANG_DEFAULTS,
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

  /**
   * J-2, and the assertion the field's design rests on: an unset picker must
   * not become `"npm"`.
   *
   * Every node manifest on disk was written before this field existed. If the
   * blank entry serialised as a value, the first save of an unrelated setting
   * would enable Corepack in that project's image — a different build for
   * something that asked for nothing.
   */
  it('omits package_manager entirely when nothing was chosen', () => {
    const spec = formToSpec({ ...blankForm(), name: 'app', runtime: 'node', nodeVersion: '22' });
    expect(spec.node).toBeDefined();
    expect('package_manager' in spec.node).toBe(false);
  });

  it('writes package_manager in the file spelling when one was chosen', () => {
    const spec = formToSpec({
      ...blankForm(),
      name: 'app',
      runtime: 'node',
      nodeVersion: '22',
      packageManager: 'pnpm',
    });
    expect(spec.node.package_manager).toBe('pnpm');
    expect(spec.node.packageManager).toBeUndefined();
  });

  /**
   * J-1. Bun and Deno write their own block, not a node one — the same W-02
   * rule the test above states, extended to the two runtimes most likely to be
   * mistaken for node.
   */
  it('gives bun and deno a block of their own rather than a node block', () => {
    for (const runtime of ['bun', 'deno']) {
      const spec = formToSpec({
        ...blankForm(),
        name: 'app',
        runtime,
        langVersion: LANG_DEFAULTS[runtime].version,
        langStart: LANG_DEFAULTS[runtime].start,
        langPort: LANG_DEFAULTS[runtime].port,
      });
      expect(spec[runtime], runtime).toBeDefined();
      expect(spec.node, runtime).toBeUndefined();
      expect(spec.php, runtime).toBeUndefined();
      expect(spec[runtime].start).toBe(LANG_DEFAULTS[runtime].start);
    }
  });

  /**
   * denoland/deno publishes no major or minor tag, so a shortened default
   * would name an image that does not exist. The Rust side holds the same
   * claim; this holds the copy the form seeds from.
   */
  it('seeds deno with a full patch version, because there is no other tag', () => {
    expect(LANG_DEFAULTS.deno.version.split('.')).toHaveLength(3);
  });

  it('writes document_root in the file spelling, not the manifest reader’s', () => {
    const spec = formToSpec({ ...blankForm(), name: 'shop', documentRoot: 'web' });
    expect(spec.document_root).toBe('web');
    expect(spec.documentRoot).toBeUndefined();
  });

  it('derives the domain from the name only when one was not given', () => {
    expect(formToSpec({ ...blankForm(), name: 'shop' }, 'stackvo.loc').domain).toBe(
      'shop.stackvo.loc'
    );
    expect(
      formToSpec({ ...blankForm(), name: 'shop', domain: 'buy.test' }, 'stackvo.loc').domain
    ).toBe('buy.test');
  });

  // It used to hardcode `.loc` while the stack was configured for
  // `stackvo.loc`, so a project created without a typed domain answered at an
  // address nothing routed to. The suffix now comes from the configured value.
  it('uses the configured suffix, and refuses to guess one', () => {
    expect(formToSpec({ ...blankForm(), name: 'shop' }, 'dev.test').domain).toBe('shop.dev.test');
    // No suffix means no domain — the schema rejects that, which is the point.
    // Inventing one would put the project at a hostname nobody serves.
    expect(formToSpec({ ...blankForm(), name: 'shop' }).domain).toBe('');
    expect(formToSpec({ ...blankForm(), name: 'shop' }, '  ').domain).toBe('');
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

/**
 * The round trip that a Save button depends on.
 *
 * `formToSpec` produces the whole manifest, so a field the form does not carry
 * is a field that saving the settings sheet deletes — silently, from a file in
 * the user's repository, with no error anywhere. Two keys arrived after the
 * form was written and both would have gone that way.
 */
describe('fields the form does not edit but must not lose', () => {
  it('carries aliases and services from a manifest back into a spec', () => {
    const manifest = {
      name: 'shop',
      domain: 'shop.loc',
      runtime: 'php',
      aliases: ['api.shop.loc', '*.shop.loc'],
      services: ['mysql', 'redis'],
      php: { version: '8.4', extensions: ['mbstring'] },
    };

    const spec = formToSpec(formFromManifest(manifest), 'stackvo.loc');
    expect(spec.aliases).toEqual(['api.shop.loc', '*.shop.loc']);
    expect(spec.services).toEqual(['mysql', 'redis']);
  });

  it('writes neither key when there is nothing in it', () => {
    const spec = formToSpec(
      formFromManifest({ name: 'shop', domain: 'shop.loc', runtime: 'php' }),
      'stackvo.loc'
    );
    // Not `[]`: almost every manifest on disk predates both keys, and adding
    // two empty arrays is a diff in somebody's repository that says nothing.
    expect(spec).not.toHaveProperty('aliases');
    expect(spec).not.toHaveProperty('services');
  });

  it('does not share the arrays with the manifest it was loaded from', () => {
    const manifest = { name: 'shop', domain: 'shop.loc', runtime: 'php', aliases: ['a.loc'] };
    const form = formFromManifest(manifest);
    form.aliases.push('b.loc');
    expect(manifest.aliases).toEqual(['a.loc']);
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

describe('domain suggestions', () => {
  it('leads with the configured suffix and never repeats one', () => {
    // The configured suffix is `loc`-based here, which also appears in the
    // built-in list — offering `shop.loc` twice would look like a bug.
    expect(domainSuggestions('shop', 'loc')).toEqual([
      'shop.loc',
      'shop.test',
      'shop.localhost',
      'shop.dev',
    ]);
    expect(domainSuggestions('shop', 'stackvo.loc')[0]).toBe('shop.stackvo.loc');
  });

  it('offers nothing before there is a name to build on', () => {
    expect(domainSuggestions('', 'loc')).toEqual([]);
  });
});

describe('domainAdvice', () => {
  // `.dev` is a real Google-owned TLD, preloaded into every browser's HSTS
  // list. A plain-HTTP project there does not warn, it refuses — so the form
  // has to say so before the project exists, not after.
  it('flags an HSTS-preloaded TLD only while HTTPS is off', () => {
    expect(domainAdvice('shop.dev', 'stackvo.loc', false)).toBe('https');
    expect(domainAdvice('shop.dev', 'stackvo.loc', true)).toBe('certificate');
  });

  it('flags a domain the wildcard certificate cannot cover', () => {
    expect(domainAdvice('shop.test', 'stackvo.loc', true)).toBe('certificate');
    expect(domainAdvice('shop.stackvo.loc', 'stackvo.loc', true)).toBe(null);
  });

  it('says nothing about an empty domain', () => {
    expect(domainAdvice('', 'stackvo.loc', true)).toBe(null);
  });
});

describe('humaniseField', () => {
  it('reads a shouted identifier as a sentence', () => {
    expect(humaniseField('BOOTSTRAP_SERVERS')).toBe('Bootstrap servers');
    expect(humaniseField('PORT')).toBe('Port');
  });

  it('survives the empty case rather than producing a stray capital', () => {
    expect(humaniseField('')).toBe('');
    expect(humaniseField(null)).toBe('');
  });
});

describe('parentDomain', () => {
  // The real set from a working checkout: three siblings, one three-label
  // domain that is alone, and four plain ones.
  const SUFFIX = 'stackvo.loc';

  it('files a subdomain under its parent', () => {
    expect(parentDomain('parser.ajans.loc', SUFFIX)).toBe('ajans.loc');
    expect(parentDomain('tracking.ajans.loc', SUFFIX)).toBe('ajans.loc');
  });

  it('does not file a second-level domain under its TLD', () => {
    // `l00kout.loc` grouped under `loc` would put every project in one bucket
    // named after the TLD, which says nothing about any of them.
    expect(parentDomain('l00kout.loc', SUFFIX)).toBe(null);
    expect(parentDomain('vue-builder.loc', SUFFIX)).toBe(null);
  });

  it('refuses the workspace suffix, which every project shares', () => {
    // Grouping on it produces one group holding everything: no grouping, plus
    // a row. This is the case a workspace that follows its own suffix hits.
    expect(parentDomain('shop.stackvo.loc', SUFFIX)).toBe(null);
    expect(parentDomain('parser.ajans.stackvo.loc', SUFFIX)).toBe('ajans.stackvo.loc');
  });

  it('survives a missing or malformed domain', () => {
    expect(parentDomain(undefined, SUFFIX)).toBe(null);
    expect(parentDomain('', SUFFIX)).toBe(null);
    expect(parentDomain('shop..loc', SUFFIX)).toBe(null);
  });
});

describe('grouping the projects table', () => {
  /** The key each row hands v-data-table, mirroring what the view computes. */
  function groupKeys(domains, suffix) {
    const counts = new Map();
    for (const d of domains) {
      const parent = parentDomain(d, suffix);
      if (parent) counts.set(parent, (counts.get(parent) ?? 0) + 1);
    }
    return domains.map((d) => {
      const parent = parentDomain(d, suffix);
      return parent && counts.get(parent) > 1 ? parent : null;
    });
  }

  it('leaves a project with no siblings ungrouped', () => {
    // null is the table's own passthrough: it skips the header for a
    // null-valued group and always flattens its rows. Handing each standalone
    // project its own key instead made a group of one — and groups start
    // closed, so with the header suppressed nothing was left to open it and
    // the row disappeared from the page entirely.
    const domains = [
      'api.oxoeashop.loc',
      'l00kout.loc',
      'parser.ajans.loc',
      'tracking.ajans.loc',
      'vue-builder.loc',
    ];
    expect(groupKeys(domains, 'stackvo.loc')).toEqual([null, null, 'ajans.loc', 'ajans.loc', null]);
  });

  it('shows every project, grouped or not', () => {
    const domains = ['a.shared.loc', 'b.shared.loc', 'alone.loc', 'x.solo.loc'];
    const keys = groupKeys(domains, 'stackvo.loc');
    expect(keys.filter((k) => k === null)).toHaveLength(2);
    expect(new Set(keys).size).toBe(2);
  });
});
