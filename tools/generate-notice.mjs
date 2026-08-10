#!/usr/bin/env node
/**
 * Write `NOTICE.md`: the third-party licence notice that ships with the app.
 *
 * ## Why this file exists at all
 *
 * Every permissive licence in this dependency graph — MIT, BSD, ISC, Apache-2.0
 * — carries the same obligation: distribute the copyright notice and the
 * licence text along with the software. A repository that merely *contains* MIT
 * dependencies has not met it; the notice has to reach whoever received the
 * binary. So `NOTICE.md` is compiled into the executable (`src-tauri/src/
 * licences.rs`) and readable from the About window, rather than being a file in
 * a source tree the user never sees.
 *
 * The readiness review listed this under "genuinely enterprise and entirely
 * missing", next to the observation that the SBOM was already being produced
 * and nothing was being shown to anyone. This is the shown half.
 *
 * ## What is counted as shipped
 *
 * **Rust:** every crate reachable from the root package through *normal*
 * dependency edges. Build-dependencies and dev-dependencies are excluded
 * because their code is not in the binary — `cargo metadata` reports the edge
 * kind, so this is read rather than guessed.
 *
 * Resolution is deliberately **not** filtered by platform. One notice covers
 * every bundle the release workflow produces, so `windows-sys` appears in the
 * macOS build's notice too. The alternative is four notices that differ, and a
 * user who cannot tell which one applies to the binary in front of them.
 *
 * **npm:** every non-dev entry in `package-lock.json`. Vite bundles a subset of
 * these, and which subset depends on what the code imports — an honest
 * superset is better than a precise number that is only true until the next
 * import. The lock file is read rather than `node_modules`, so the inventory
 * does not depend on which optional packages this platform installed.
 *
 * ## What is exact and what is best-effort
 *
 * The **inventory** — names, versions, licence expressions — is exact,
 * deterministic and gated: `npm run notice:check` fails when it drifts from the
 * manifests, and that is the failure that matters, because it is the one that
 * means a dependency arrived without its notice.
 *
 * The **licence texts and copyright lines** are read out of the local package
 * sources, and a machine that has not downloaded a given crate's source cannot
 * produce them. Those are reported as missing rather than silently dropped, and
 * they are not part of the drift comparison — a gate that fails because a
 * developer has not built for Windows would be turned off within a week.
 *
 * Usage:
 *   node tools/generate-notice.mjs            write NOTICE.md
 *   node tools/generate-notice.mjs --check    fail if the inventory has drifted
 */

import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const NOTICE = join(ROOT, 'NOTICE.md');

/** Files that hold a licence, in the shapes both ecosystems use. */
const LICENCE_FILE = /^(licen[cs]e|copying|notice)([-._].*)?(\.(md|txt))?$/i;

/**
 * Which SPDX identifier a licence file is the text of.
 *
 * Dual-licensed crates ship `LICENSE-MIT` and `LICENSE-APACHE` side by side, so
 * the file name answers this for most of them; the body answers it for the
 * rest. Order matters — "Apache" appears inside the LLVM exception, so the
 * exact identifiers are tried before the loose body match.
 */
function identify(fileName, body) {
  const name = fileName.toUpperCase();
  if (name.includes('APACHE')) return 'Apache-2.0';
  if (name.includes('MIT')) return 'MIT';
  if (name.includes('ZLIB')) return 'Zlib';
  if (name.includes('BSD')) return 'BSD-3-Clause';
  if (name.includes('ISC')) return 'ISC';
  if (name.includes('MPL')) return 'MPL-2.0';
  if (name.includes('UNLICENSE')) return 'Unlicense';
  if (name.includes('BOOST') || name.includes('BSL')) return 'BSL-1.0';
  if (name.includes('CC0')) return 'CC0-1.0';

  const text = body.slice(0, 2000);
  if (/Apache License\s*\n?\s*Version 2\.0/i.test(text)) return 'Apache-2.0';
  if (/Mozilla Public License Version 2\.0/i.test(text)) return 'MPL-2.0';
  if (/Boost Software License/i.test(text)) return 'BSL-1.0';
  if (/CC0 1\.0 Universal/i.test(text)) return 'CC0-1.0';
  if (/Redistribution and use in source and binary forms/i.test(text)) {
    return /Neither the name/i.test(text) ? 'BSD-3-Clause' : 'BSD-2-Clause';
  }
  if (/Permission to use, copy, modify, and\/or distribute/i.test(text)) return 'ISC';
  if (/Permission is hereby granted, free of charge/i.test(text)) return 'MIT';
  if (/This is free and unencumbered software released into the public domain/i.test(text)) {
    return 'Unlicense';
  }
  return null;
}

/** Every SPDX identifier named in an expression like `MIT OR Apache-2.0`. */
function identifiers(expression) {
  return [
    ...new Set(
      expression
        .replace(/[()]/g, ' ')
        // `MIT/Apache-2.0` is not SPDX but crates.io is full of it.
        .split(/\s+(?:OR|AND|WITH)\s+|\//i)
        .map((part) => part.trim())
        .filter(Boolean)
    ),
  ];
}

/** The licence files a package directory holds, as `{ name, body }`. */
function licenceFiles(dir) {
  if (!dir || !existsSync(dir)) return [];
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return [];
  }
  return entries
    .filter((e) => e.isFile() && LICENCE_FILE.test(e.name))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((e) => {
      try {
        return { name: e.name, body: readFileSync(join(dir, e.name), 'utf8') };
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

/**
 * The copyright lines in a licence text.
 *
 * This is the part of a permissive licence that is actually per-package: the
 * permission text is boilerplate, the holder is not, and it is the holder the
 * licence requires be carried.
 */
function copyrights(body) {
  return [
    ...new Set(
      body
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => /^copyright\b/i.test(line) && /\d{4}|©/.test(line))
        .map((line) => line.replace(/\s+/g, ' '))
    ),
  ];
}

/** The Rust crates that end up in the binary. */
function rustPackages() {
  const raw = execFileSync(
    'cargo',
    ['metadata', '--format-version', '1', '--manifest-path', join(ROOT, 'src-tauri/Cargo.toml')],
    { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 }
  );
  const meta = JSON.parse(raw);

  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));

  const shipped = new Set();
  const stack = [meta.resolve.root];
  while (stack.length) {
    const id = stack.pop();
    if (shipped.has(id)) continue;
    shipped.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      // `kind: null` is a normal dependency. "dev" and "build" are code that
      // never reaches a user.
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  shipped.delete(meta.resolve.root);

  return [...shipped]
    .map((id) => {
      const pkg = byId.get(id);
      return {
        name: pkg.name,
        version: pkg.version,
        licence: pkg.license ?? (pkg.license_file ? `see ${pkg.license_file}` : 'unstated'),
        repository: pkg.repository ?? '',
        dir: pkg.manifest_path ? dirname(pkg.manifest_path) : null,
      };
    })
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

/**
 * The npm packages that ship — walked, not filtered.
 *
 * The obvious implementation reads `package-lock.json` and keeps every entry
 * without a `dev` flag. It is wrong here, and visibly so: it produced 107
 * packages including all twenty-eight `@esbuild/*` and all twenty-four
 * `@rollup/rollup-*` platform binaries, which are optional dependencies of a
 * bundler that is itself a dev dependency. npm marks those entries `optional`
 * and leaves `dev` unset, so a flag test cannot tell "optional because
 * production wants it on this platform" from "optional because a build tool
 * does".
 *
 * So the graph is walked from `package.json`'s own `dependencies`, exactly as
 * the Rust side is walked from the root package, using node's own resolution
 * order: nearest `node_modules` first, then up the path.
 */
function npmPackages() {
  const lock = JSON.parse(readFileSync(join(ROOT, 'package-lock.json'), 'utf8'));
  const entries = lock.packages;

  /**
   * Where `name`, required from `fromPath`, actually resolves in the tree.
   *
   * Nearest `node_modules` first, then each enclosing one, ending at the root's
   * — which is where npm hoists almost everything, and where the first version
   * of this function forgot to look: `node_modules/vue` has no
   * `/node_modules/` left to strip, so it gave up before trying the top level
   * and reported thirteen shipped packages instead of thirty-one. A resolver
   * that stops early does not fail, it just answers less.
   */
  const resolveFrom = (fromPath, name) => {
    let scope = fromPath;
    for (;;) {
      const candidate = scope ? `${scope}/node_modules/${name}` : `node_modules/${name}`;
      if (entries[candidate]) return candidate;
      if (scope === '') return null;
      const up = scope.lastIndexOf('/node_modules/');
      scope = up === -1 ? '' : scope.slice(0, up);
    }
  };

  const shipped = new Set();
  const stack = [''];
  while (stack.length) {
    const path = stack.pop();
    const entry = entries[path];
    if (!entry) continue;

    // The root contributes its `dependencies` only: its `devDependencies` are
    // the build tooling, which is the whole distinction being drawn. Below the
    // root, optional dependencies are followed as well — `@parcel/watcher`'s
    // platform binaries are optional and do ship — while `devDependencies` are
    // ignored, because npm never installs a dependency's own dev tree.
    //
    // **Peer dependencies are not followed.** A peer is a requirement on the
    // *host*, and the host is this repository, whose dependencies are already
    // the starting point of this walk. Following them put the entire build
    // toolchain in the notice: `vuetify` peer-depends on `vite-plugin-vuetify`,
    // which depends on `vite`, which depends on `esbuild` — and twenty-eight
    // `@esbuild/*` platform binaries appeared in a list of what ships to users.
    const required =
      path === ''
        ? (entry.dependencies ?? {})
        : { ...entry.dependencies, ...entry.optionalDependencies };

    for (const name of Object.keys(required ?? {})) {
      const at = resolveFrom(path, name);
      if (!at || shipped.has(at)) continue;
      shipped.add(at);
      stack.push(at);
    }
  }

  return [...shipped]
    .map((path) => {
      const entry = entries[path];
      // The last `node_modules/` segment is the package name, scope included.
      const name = path.slice(path.lastIndexOf('node_modules/') + 'node_modules/'.length);
      return {
        name,
        version: entry.version ?? '',
        licence: entry.license ?? 'unstated',
        repository: entry.resolved ?? '',
        dir: join(ROOT, path),
      };
    })
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

/**
 * Is this package's row in the notice?
 *
 * Matched on the rendered row rather than on a `name@version` string, which
 * the first version of this did — and scoped npm packages broke it silently:
 * `@babel/parser@7.29.7` has two `@`, the split took the wrong one, and the
 * check reported twenty-six missing packages against a notice that listed every
 * one of them. A gate that cries wolf is a gate that gets ignored, so it
 * compares the exact text it wrote.
 */
function isListed(notice, pkg) {
  return notice.includes(`| ${pkg.name} | ${pkg.version} |`);
}

function summarise(packages) {
  const counts = new Map();
  for (const p of packages) counts.set(p.licence, (counts.get(p.licence) ?? 0) + 1);
  return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}

function table(packages) {
  const rows = packages.map(
    (p) => `| ${p.name} | ${p.version} | ${p.licence.replace(/\|/g, '\\|')} |`
  );
  return ['| Package | Version | Licence |', '| --- | --- | --- |', ...rows].join('\n');
}

function render({ rust, npm, generatedFrom }) {
  const all = [...rust, ...npm];

  // One representative text per identifier, plus who it was taken from.
  const texts = new Map();
  const holders = new Map();
  const noFiles = [];

  for (const pkg of all) {
    const files = licenceFiles(pkg.dir);
    if (files.length === 0) {
      noFiles.push(`${pkg.name}@${pkg.version}`);
      continue;
    }

    const declared = identifiers(pkg.licence);
    for (const file of files) {
      const id = identify(file.name, file.body) ?? (declared.length === 1 ? declared[0] : null);
      if (!id) continue;
      if (!texts.has(id)) {
        texts.set(id, { from: `${pkg.name}@${pkg.version} (${file.name})`, body: file.body.trim() });
      }
      for (const line of copyrights(file.body)) {
        if (!holders.has(line)) holders.set(line, new Set());
        holders.get(line).add(pkg.name);
      }
    }
  }

  const declaredIds = [...new Set(all.flatMap((p) => identifiers(p.licence)))].sort();
  const missingTexts = declaredIds.filter((id) => !texts.has(id));

  const out = [];
  out.push('# Third-party notices');
  out.push('');
  out.push(
    'StackVo Desktop is MIT licensed and is built on the work below. Every',
    'licence here requires that its notice travel with the software, so this',
    'file is compiled into the application and readable from **About →',
    'Third-party licences** — a notice that stays in a source repository has not',
    'reached the person who received the binary.'
  );
  out.push('');
  out.push('> **Generated — do not edit by hand.**  ');
  out.push('> `node tools/generate-notice.mjs`, from `src-tauri/Cargo.lock` and  ');
  out.push('> `package-lock.json`. `npm run notice:check` fails the build when the  ');
  out.push('> inventory below no longer matches those manifests.');
  out.push('');
  out.push(
    'The Rust inventory is resolved for **all platforms at once**, so a crate',
    'used only by the Windows build is listed in every build\'s notice. One',
    'notice that is a superset beats four that differ and cannot be told apart.',
    'Build-time and test-only dependencies are excluded: their code is not in',
    'the binary.'
  );
  out.push('');
  out.push(`Counted from ${generatedFrom}.`);
  out.push('');

  out.push('## Summary');
  out.push('');
  out.push('| Licence | Rust crates | npm packages |');
  out.push('| --- | ---: | ---: |');
  const rustCounts = new Map(summarise(rust));
  const npmCounts = new Map(summarise(npm));
  for (const licence of [...new Set([...rustCounts.keys(), ...npmCounts.keys()])].sort()) {
    out.push(
      `| ${licence.replace(/\|/g, '\\|')} | ${rustCounts.get(licence) ?? ''} | ${npmCounts.get(licence) ?? ''} |`
    );
  }
  out.push(`| **Total** | **${rust.length}** | **${npm.length}** |`);
  out.push('');

  out.push(`## Rust crates (${rust.length})`);
  out.push('');
  out.push(table(rust));
  out.push('');

  out.push(`## npm packages (${npm.length})`);
  out.push('');
  out.push(table(npm));
  out.push('');

  out.push('## Copyright holders');
  out.push('');
  out.push(
    'Collected from the licence files in the packages above. This is the part',
    'of a permissive licence that is not boilerplate, and the part it requires',
    'be carried.'
  );
  out.push('');
  for (const line of [...holders.keys()].sort()) {
    out.push(`- ${line}`);
  }
  out.push('');

  out.push('## Licence texts');
  out.push('');
  out.push(
    'One copy of each licence, taken verbatim from a package that ships it —',
    'not from a template, so the text here is the text the dependency actually',
    'distributed.'
  );
  out.push('');
  for (const id of [...texts.keys()].sort()) {
    const { from, body } = texts.get(id);
    out.push(`### ${id}`);
    out.push('');
    out.push(`_As distributed by ${from}._`);
    out.push('');
    out.push('```');
    out.push(body);
    out.push('```');
    out.push('');
  }

  if (missingTexts.length || noFiles.length) {
    out.push('## What could not be read');
    out.push('');
    out.push(
      'Reported rather than omitted. A notice that silently drops what it could',
      'not find is a notice nobody can check.'
    );
    out.push('');
    if (missingTexts.length) {
      out.push(
        `**No local text for ${missingTexts.length} declared licence${missingTexts.length === 1 ? '' : 's'}:** ` +
          missingTexts.join(', ') +
          '. The identifier is declared by a package whose source is not on the machine that generated this file; the licence still applies in full.'
      );
      out.push('');
    }
    if (noFiles.length) {
      out.push(
        `**No licence file found in ${noFiles.length} package${noFiles.length === 1 ? '' : 's'}**, ` +
          'usually because the source has not been downloaded for this platform. ' +
          'Their declared licences are in the tables above:'
      );
      out.push('');
      out.push(noFiles.map((n) => `\`${n}\``).join(', '));
      out.push('');
    }
  }

  return out.join('\n').replace(/\n{3,}/g, '\n\n') + '\n';
}

function main() {
  const check = process.argv.includes('--check');

  const rust = rustPackages();
  const npm = npmPackages();
  const generatedFrom = `${rust.length} Rust crates and ${npm.length} npm packages`;

  if (check) {
    if (!existsSync(NOTICE)) {
      console.error('NOTICE.md does not exist. Run: npm run notice');
      process.exit(1);
    }
    const current = readFileSync(NOTICE, 'utf8');
    const shipped = [...rust, ...npm];

    // Only the inventory. Licence *texts* depend on which sources this machine
    // has downloaded, and a gate that fails because a developer has not built
    // for Windows is a gate that gets switched off.
    const missing = shipped
      .filter((pkg) => !isListed(current, pkg))
      .map((pkg) => `${pkg.name}@${pkg.version} (${pkg.licence})`);

    if (missing.length) {
      console.error(
        `NOTICE.md is missing ${missing.length} shipped package${missing.length === 1 ? '' : 's'}:\n  ` +
          missing.slice(0, 20).join('\n  ') +
          (missing.length > 20 ? `\n  … and ${missing.length - 20} more` : '') +
          '\n\nA dependency arrived without its licence notice. Run: npm run notice'
      );
      process.exit(1);
    }

    // A comparison that matched nothing would also report nothing missing.
    if (shipped.length < 100) {
      console.error(
        `only ${shipped.length} shipped packages were resolved — the walk has ` +
          'stopped finding the graph, and a check with no inputs passes anything'
      );
      process.exit(1);
    }

    console.log(`NOTICE.md covers all ${shipped.length} shipped packages.`);
    return;
  }

  writeFileSync(NOTICE, render({ rust, npm, generatedFrom }));
  console.log(`NOTICE.md written: ${generatedFrom}.`);
}

main();
