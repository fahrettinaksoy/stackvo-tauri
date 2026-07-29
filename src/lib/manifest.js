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
  form.runtime = manifest.runtime === 'node' ? 'node' : 'php';

  if (manifest.server) form.server = manifest.server;
  if (manifest.documentRoot) form.documentRoot = manifest.documentRoot;

  if (manifest.php) {
    form.phpVersion = manifest.php.version ?? '';
    form.extensions = [...(manifest.php.extensions ?? [])];
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
export function formToSpec(form) {
  const spec = {
    name: form.name,
    domain: form.domain || `${form.name}.loc`,
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
