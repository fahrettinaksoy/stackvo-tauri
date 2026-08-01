/**
 * The manifest form, in one place.
 *
 * Creating a project and editing one are the same set of fields over the same
 * contract, so the conversion between `stackvo.json` and the form lives here
 * rather than once per drawer. That matters more than the usual don't-repeat
 * argument: the manifest has write rules a form can break silently (the 50
 * extension cap, `extensions` last, one runtime block), and a second copy of
 * this logic is a second chance to get one of them wrong.
 *
 * Two shapes are involved and they are not the same:
 *   - the **manifest** Rust hands back, camelCase (`documentRoot`);
 *   - the **spec** every writing command takes, which is the file itself and
 *     therefore snake_case (`document_root`).
 * Everything here converts in one direction or the other, explicitly.
 */

/** Compare dotted versions. Returns -1, 0 or 1; missing parts count as zero. */
export function compareVersions(a, b) {
  const pa = String(a || '0')
    .split('.')
    .map(Number);
  const pb = String(b || '0')
    .split('.')
    .map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d) return d > 0 ? 1 : -1;
  }
  return 0;
}

/**
 * Can this extension build against this PHP version?
 *
 * Answered from the catalog rather than by the Docker build, which would
 * otherwise discover it several minutes in.
 */
export function isIncompatible(extension, phpVersion) {
  if (!phpVersion) return false;
  const { removedIn, minPhp } = extension;
  if (removedIn && compareVersions(phpVersion, removedIn) >= 0) return true;
  if (minPhp && compareVersions(phpVersion, minPhp) < 0) return true;
  return false;
}

/**
 * How many extensions the manifest can carry.
 *
 * A Bash parser limit, not a preference: the extractor greps 50 lines past the
 * `"extensions"` marker, so entry 51 onward is dropped without a word (C-04).
 * Read from the catalog so the number has one source, with the contract's own
 * value as the fallback.
 */
export function extensionLimit(catalog) {
  return catalog?.maxExtensions ?? 50;
}

/** Would this form write more extensions than the parser can read back? */
export function overExtensionLimit(form, catalog) {
  return form.extensions.length > extensionLimit(catalog);
}

/** The four runtimes that share one config shape (mirror of LANG_RUNTIMES). */
export const LANG_RUNTIMES = ['python', 'go', 'ruby', 'rust'];

/**
 * Each lang runtime's ecosystem defaults — the same values the Rust side
 * (`manifest::lang_defaults`) applies when a field is omitted, repeated here
 * so the form shows what will actually run instead of empty inputs.
 */
export const LANG_DEFAULTS = {
  python: {
    version: '3.13',
    install: 'pip install --no-cache-dir -r requirements.txt',
    build: '',
    start: 'python main.py',
    port: 8000,
  },
  go: {
    version: '1.23',
    install: '',
    build: 'go build -o /app/server .',
    start: '/app/server',
    port: 8080,
  },
  ruby: {
    version: '3.3',
    install: 'bundle install',
    build: '',
    start: 'bundle exec ruby app.rb',
    port: 4567,
  },
  rust: {
    version: '1',
    install: '',
    build: 'cargo build --release',
    start: 'cargo run --release',
    port: 8080,
  },
};

/** A form with the contract's own defaults in it. */
export function blankForm() {
  return {
    name: '',
    domain: '',
    runtime: 'php',
    server: 'nginx',
    documentRoot: 'public',
    phpVersion: '',
    extensions: [],
    nodeVersion: '',
    install: 'npm install',
    build: '',
    start: 'npm run dev -- --host 0.0.0.0 --port 3000',
    port: 3000,
    // One block of lang fields, reused by whichever lang runtime is chosen;
    // switching runtime re-seeds them from LANG_DEFAULTS.
    langVersion: '',
    langInstall: '',
    langBuild: '',
    langStart: '',
    langPort: 8080,
  };
}

/**
 * Load an existing manifest into the form.
 *
 * The runtime the user is *not* on keeps the blank defaults, so switching
 * runtime in the form lands on something valid instead of empty required
 * fields. A project's own values always win over those defaults.
 */
export function formFromManifest(manifest) {
  const form = blankForm();
  if (!manifest) return form;

  form.name = manifest.name ?? '';
  form.domain = manifest.domain ?? '';
  form.runtime = ['node', ...LANG_RUNTIMES].includes(manifest.runtime) ? manifest.runtime : 'php';

  if (manifest.server) form.server = manifest.server;
  if (manifest.documentRoot) form.documentRoot = manifest.documentRoot;

  if (manifest.php) {
    form.phpVersion = manifest.php.version ?? '';
    form.extensions = [...(manifest.php.extensions ?? [])];
  }

  if (manifest.lang) {
    form.langVersion = manifest.lang.version ?? '';
    form.langInstall = manifest.lang.install ?? '';
    form.langBuild = manifest.lang.build ?? '';
    form.langStart = manifest.lang.start ?? '';
    form.langPort = manifest.lang.port ?? 8080;
  }

  if (manifest.node) {
    form.nodeVersion = manifest.node.version ?? '';
    if (manifest.node.install) form.install = manifest.node.install;
    // `build` is optional in the contract, and absent is a meaningful state —
    // it must come back as empty, not as the placeholder command.
    form.build = manifest.node.build ?? '';
    if (manifest.node.start) form.start = manifest.node.start;
    if (manifest.node.port) form.port = manifest.node.port;
  }

  return form;
}

/**
 * Build the manifest. The payload IS the manifest — nothing is reassembled
 * on the Rust side, which is precisely what made the web UI's Node path broken.
 *
 * Only the chosen runtime's block is emitted. Sending both is not a tidiness
 * question: the Bash parser reads the first `version` key it finds without
 * disambiguating, so two blocks silently corrupt the generated Dockerfile
 * (W-02).
 */
export function formToSpec(form, tld) {
  // The suffix is passed in rather than assumed. It used to be the literal
  // `.loc`, which is not what the stack is configured with: routing labels,
  // certificates and the services list all use DEFAULT_TLD_SUFFIX, so a
  // project created without a typed domain got an address that nothing served.
  // An unknown suffix leaves the domain empty, which the schema rejects — a
  // visible failure rather than a project at the wrong hostname.
  const suffix = String(tld ?? '').trim();
  const spec = {
    name: form.name,
    domain: form.domain || (suffix ? `${form.name}.${suffix}` : ''),
    runtime: form.runtime,
  };

  if (form.runtime === 'node') {
    spec.node = {
      version: form.nodeVersion,
      install: form.install,
      start: form.start,
      port: Number(form.port),
    };
    if (form.build) spec.node.build = form.build;
  } else if (LANG_RUNTIMES.includes(form.runtime)) {
    const block = {
      version: form.langVersion,
      start: form.langStart,
      port: Number(form.langPort),
    };
    if (form.langInstall) block.install = form.langInstall;
    if (form.langBuild) block.build = form.langBuild;
    spec[form.runtime] = block;
  } else {
    spec.server = form.server;
    spec.document_root = form.documentRoot;
    spec.php = { version: form.phpVersion, extensions: [...form.extensions] };
  }

  return spec;
}

/**
 * Has anything actually changed?
 *
 * Compared as specs rather than field by field, so a difference that does not
 * survive into the file — a `build` command typed and cleared again, a port
 * held as a string in one place and a number in the other — does not light up
 * a Save button that would write a byte-identical file.
 */
export function specsDiffer(a, b) {
  return JSON.stringify(a) !== JSON.stringify(b);
}

/**
 * Suffixes offered when picking a project's domain, best first.
 *
 * `test` and `localhost` are reserved for exactly this by RFC 6761, so no
 * registry can ever sell them out from under a local setup. `loc` is the
 * convention StackVo shipped with: not reserved, but unallocated, so it works.
 * `dev` is here because people ask for it, with the catch below.
 */
export const LOCAL_SUFFIXES = ['test', 'localhost', 'loc', 'dev'];

/**
 * TLDs on the HSTS preload list, where every browser forces HTTPS.
 *
 * There is no click-through on these — a plain-HTTP `.dev` site is not a
 * warning, it is a refusal. Offering the suffix without saying so would hand
 * someone a project that cannot be opened and no clue why.
 */
export const HTTPS_ONLY_SUFFIXES = ['dev', 'app', 'page', 'new', 'foo'];

/** `name.configured` first, then the alternatives, with no duplicates. */
export function domainSuggestions(name, configured) {
  const base = String(name ?? '').trim();
  if (!base) return [];
  const suffixes = [String(configured ?? '').trim(), ...LOCAL_SUFFIXES].filter(Boolean);
  return [...new Set(suffixes.map((s) => `${base}.${s}`))];
}

/**
 * What the chosen domain will cost, or null when it costs nothing.
 *
 * `https` — an HSTS-preloaded TLD: it will not load over plain HTTP at all.
 * `certificate` — outside the configured suffix, so the wildcard does not
 * cover it and certificates have to be reissued before HTTPS works.
 */
export function domainAdvice(domain, configured, sslEnabled) {
  const value = String(domain ?? '')
    .trim()
    .toLowerCase();
  if (!value) return null;

  const tld = value.split('.').pop();
  if (HTTPS_ONLY_SUFFIXES.includes(tld) && !sslEnabled) return 'https';

  const suffix = String(configured ?? '')
    .trim()
    .toLowerCase();
  if (suffix && !value.endsWith(`.${suffix}`)) return 'certificate';
  return null;
}

/**
 * A `.env` field name as a sentence rather than a shout.
 *
 * The fallback for settings with no translation of their own — the ones whose
 * names are the terms their own documentation uses, where a translation would
 * be a phrase nobody can search for. `BOOTSTRAP_SERVERS` becomes
 * `Bootstrap servers`, which is the same word and easier to read.
 */
export function humaniseField(field) {
  const text = String(field ?? '').trim();
  if (!text) return '';
  return text.charAt(0).toUpperCase() + text.slice(1).toLowerCase().replace(/_/g, ' ');
}

/**
 * The domain a project should be filed under, or null when it stands alone.
 *
 * `parser.ajans.loc` belongs with `tracking.ajans.loc`; `l00kout.loc` does not
 * belong to `loc`. Two rules, and both are load-bearing:
 *
 *  * a parent needs two labels of its own, so a second-level domain is never
 *    filed under its TLD;
 *  * it must not be the workspace's own suffix — every project shares that by
 *    construction, so grouping on it yields one group holding everything,
 *    which is no grouping plus a row.
 */
export function parentDomain(domain, suffix) {
  const parts = String(domain ?? '')
    .split('.')
    .filter(Boolean);
  if (parts.length < 3) return null;
  const parent = parts.slice(1).join('.');
  return parent === String(suffix ?? '').trim() ? null : parent;
}
