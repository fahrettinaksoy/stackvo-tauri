import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { readFileSync } from 'node:fs';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import en from '@/i18n/locales/en.js';

/**
 * The runtime cell, which named the wrong language for six of the eight.
 *
 * It was a two-way branch — `runtime === 'node'` drew Node and *everything
 * else* drew PHP — written when there were two runtimes and never revisited
 * when there were eight. A Go project appeared in the table as "PHP N/A" under
 * an elephant, and so did Python, Ruby, Rust, Bun and Deno.
 *
 * Nothing failed. 853 tests passed over it, because a mount test that asserts
 * "the table lists shop" is satisfied by a row that describes shop wrongly, and
 * no assertion anywhere read this cell. That is the shape of the bug worth
 * pinning: not a crash, a confident wrong answer.
 *
 * So the table is checked against the *backend's* list of runtimes rather than
 * against a list retyped here. `IMPLEMENTED_RUNTIMES` in `commands.rs` is what
 * decides which runtimes exist; a ninth one added there should fail this test
 * rather than quietly render as PHP.
 */

const vuetify = createVuetify({ components, directives });

const replies = {};

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy(
    {},
    {
      get: (_t, name) => () => {
        const reply = replies[name];
        return Promise.resolve(reply);
      },
    }
  ),
}));

vi.mock('@/lib/events', async (importOriginal) => ({
  ...(await importOriginal()),
  listenAll: async () => () => {},
  listen: async () => () => {},
}));

const Projects = (await import('@/views/Projects.vue')).default;

/** How each runtime carries its version, which is not one field. */
const versionBlock = (runtime, version) => {
  if (runtime === 'php') return { php: { version } };
  if (runtime === 'node') return { node: { version } };
  return { lang: { version } };
};

const project = (runtime, version, over = {}) => ({
  name: runtime,
  path: `/w/projects/${runtime}`,
  domain: `${runtime}.loc`,
  domainConfigured: true,
  runtime,
  server: 'nginx',
  built: true,
  running: false,
  manifestValid: true,
  generatedStale: false,
  containerName: `stackvo-${runtime}`,
  manifest: { server: 'nginx', errors: [], ...versionBlock(runtime, version) },
  ...over,
});

async function render() {
  const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } });
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(
    {
      components: { Page: Projects },
      template: '<v-app><Page /></v-app>',
    },
    {
      attachTo: host,
      global: {
        plugins: [
          createPinia(),
          createRouter({
            history: createMemoryHistory(),
            routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }],
          }),
          vuetify,
          i18n,
        ],
      },
    }
  );

  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

beforeEach(() => {
  setActivePinia(createPinia());
  for (const key of Object.keys(replies)) delete replies[key];
  replies.projectsList = [];
  replies.projectAdoptable = [];
  replies.importsScan = [];
});

afterEach(() => {
  document.body.innerHTML = '';
});

/** What the backend says it can build. Read, not retyped. */
const IMPLEMENTED = ['php', 'node', 'python', 'go', 'ruby', 'rust', 'bun', 'deno'];

const NAMES = {
  php: 'PHP',
  node: 'Node',
  python: 'Python',
  go: 'Go',
  ruby: 'Ruby',
  rust: 'Rust',
  bun: 'Bun',
  deno: 'Deno',
};

describe('the runtime column', () => {
  it('is checked against every runtime the backend builds', async () => {
    const rust = readFileSync('src-tauri/src/commands.rs', 'utf8');
    // `\s*` across the `=`: the declaration wraps onto its own line, and a
    // regex that only matched the one-line form would report the constant as
    // "moved or renamed" the first time somebody ran the formatter.
    const declared = /const IMPLEMENTED_RUNTIMES: \[&str; \d+\]\s*=\s*\[([^\]]+)\]/.exec(rust);

    expect(declared, 'IMPLEMENTED_RUNTIMES moved or was renamed').not.toBeNull();
    const names = [...declared[1].matchAll(/"([a-z]+)"/g)].map((m) => m[1]);
    expect(names.sort(), 'a runtime exists that this test does not cover').toEqual(
      [...IMPLEMENTED].sort()
    );
  });

  it.each(IMPLEMENTED)('names a %s project as %s rather than as PHP', async (runtime) => {
    replies.projectsList = [project(runtime, '1.2')];
    const wrapper = await render();

    const row = wrapper.get('tbody tr');
    expect(row.text()).toContain(`${NAMES[runtime]} 1.2`);

    if (runtime !== 'php') {
      expect(row.text(), 'a non-PHP project is still being called PHP').not.toContain('PHP');
    }
  });

  /**
   * The server rides in this cell now, and only where it means something:
   * `manifest::read` warns that `server` is ignored on anything but PHP, so a
   * Go row printing "nginx" would be repeating a value the backend discards.
   */
  it('shows the server on PHP and on nothing else', async () => {
    replies.projectsList = [project('php', '8.3')];
    const php = await render();
    expect(php.get('tbody tr').text()).toContain('nginx');
    php.unmount();
    document.body.innerHTML = '';

    replies.projectsList = [project('go', '1.23')];
    const go = await render();
    expect(go.get('tbody tr').text(), 'the server is ignored for Go').not.toContain('nginx');
  });

  /**
   * Three states, and the middle one is why this is not a tick.
   *
   * A directory that was never versioned, a repository somebody ran `git init`
   * in, and a clone are three different answers to "where did this come from",
   * and a boolean folds the first two together. The backend keeps them apart —
   * `git: null` against `git: { remote: null }` — so the column has to.
   */
  it('tells a clone from a local repository from neither', async () => {
    replies.projectsList = [
      project('php', '8.3', { name: 'cloned', git: { remote: 'git@example.com:a/b.git' } }),
      project('php', '8.3', { name: 'inited', domain: 'inited.loc', git: { remote: null } }),
      project('php', '8.3', { name: 'plain', domain: 'plain.loc', git: null }),
    ];
    const wrapper = await render();

    const rows = wrapper.findAll('tbody tr');
    expect(rows).toHaveLength(3);

    // The remote is the button's accessible name, because the cell shows a
    // glyph and a URL is the widest thing that could go in a table.
    const labels = wrapper
      .findAll('tbody button')
      .map((b) => b.attributes('aria-label'))
      .filter(Boolean);
    expect(labels).toContain('git@example.com:a/b.git');
    expect(labels).toContain(en.projectsView.repoLocal);
  });

  /** A version the manifest does not carry is absent, not "N/A". */
  it('says the runtime alone when there is no version', async () => {
    replies.projectsList = [project('go', undefined, { manifest: { errors: [] } })];
    const wrapper = await render();

    const row = wrapper.get('tbody tr');
    expect(row.text()).toContain('Go');
    expect(row.text()).not.toContain('N/A');
  });
});
