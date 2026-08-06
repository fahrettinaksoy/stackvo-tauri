import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

/**
 * What the inventory store does with a reply it did not expect.
 *
 * The IPC boundary is untyped. `src/lib/ipc.js` is hand-written, nothing checks
 * that a Rust command still returns the shape the frontend believes it returns,
 * and until `tauri-specta` generates both halves nothing will. So the boundary
 * has to be treated as untrusted at the point of assignment.
 *
 * This is not hypothetical. Before the fix these tests guard, `loadProjects`
 * assigned the reply straight into `projects.value`, and a `null` — a command
 * marked `deferred`, an `Option` that came back `None`, a renamed field — made
 * every computed read `.filter` off `null`. In a desktop app that is not a
 * missing list; the render throws and the window goes blank. It surfaced as
 * four unhandled rejections in the app-shell suite that nobody had chased,
 * because the tests still passed around them.
 *
 * The failure the boundary produces must be a visible empty state, never a
 * dead window.
 */

const replies = {};
vi.mock('@/lib/ipc', () => ({
  // The real guard, not a stub — see views-render.spec.js.
  asList: (value) => (Array.isArray(value) ? value : []),
  // A function reply is invoked, so a test can make the call reject rather than
  // resolve; anything else is handed back as-is. Storing a rejected promise in
  // the table instead would fire an unhandled rejection before the test that
  // wants it ever runs.
  api: new Proxy(
    {},
    {
      get: (_t, name) => () => {
        const reply = replies[name];
        return typeof reply === 'function' ? reply() : Promise.resolve(reply);
      },
    }
  ),
}));

const { useInventoryStore } = await import('@/stores/inventory');

beforeEach(() => {
  setActivePinia(createPinia());
  for (const key of Object.keys(replies)) delete replies[key];
});

describe('the inventory store against a boundary that misbehaves', () => {
  /** Every reply a command is capable of producing that is not a list. */
  const notLists = [
    ['null', null],
    ['undefined', undefined],
    ['an object', { projects: [] }],
    ['a string', 'nope'],
    ['a number', 0],
  ];

  for (const [label, reply] of notLists) {
    it(`survives ${label} from projects_list`, async () => {
      replies.projectsList = reply;
      const store = useInventoryStore();

      await store.loadProjects();

      expect(store.projects).toEqual([]);
      // The computeds are what actually threw; reading them is the assertion.
      expect(store.invalidProjects).toEqual([]);
      expect(store.runningProjects).toEqual([]);
      expect(store.unreachableDomains).toEqual([]);
    });

    it(`survives ${label} from services_list`, async () => {
      replies.servicesList = reply;
      const store = useInventoryStore();

      await store.loadServices();

      expect(store.services).toEqual([]);
      expect(store.enabledServices).toEqual([]);
      expect(store.runningServices).toEqual([]);
      expect(store.brokenDependencies).toEqual([]);
      expect(store.servicesByCategory).toEqual({});
    });
  }

  /**
   * A well-formed list must still be passed through untouched — a guard that
   * quietly empties good data is worse than the crash it replaced.
   */
  it('does not interfere with a well-formed reply', async () => {
    replies.projectsList = [
      {
        name: 'shop',
        manifestValid: true,
        running: true,
        domain: 'shop.loc',
        domainConfigured: false,
      },
      { name: 'blog', manifestValid: false, running: false, domain: null, domainConfigured: false },
    ];
    const store = useInventoryStore();

    await store.loadProjects();

    expect(store.projects).toHaveLength(2);
    expect(store.runningProjects.map((p) => p.name)).toEqual(['shop']);
    expect(store.invalidProjects.map((p) => p.name)).toEqual(['blog']);
    expect(store.unreachableDomains.map((p) => p.name)).toEqual(['shop']);
  });

  /**
   * One field, not the whole list. A service missing `unmetDependencies` is a
   * service with none — not a reason for the services page to throw.
   */
  it('treats a service with no dependency field as having no unmet ones', async () => {
    replies.servicesList = [
      { id: 'mysql', enabled: true, category: 'database' },
      { id: 'redis', enabled: true, category: 'cache', unmetDependencies: ['mysql'] },
    ];
    const store = useInventoryStore();

    await store.loadServices();

    expect(store.brokenDependencies.map((s) => s.id)).toEqual(['redis']);
  });

  /**
   * A rejection is a different failure from a bad shape, and it already had an
   * answer — the error ref. That must keep working, so the UI can tell "we were
   * told nothing" from "there is nothing".
   */
  it('keeps reporting a real failure through the error ref', async () => {
    const boom = new Error('engine unreachable');
    replies.projectsList = () => Promise.reject(boom);

    const store = useInventoryStore();
    await store.loadProjects();

    expect(store.projects).toEqual([]);
    expect(store.projectsError).toBe(boom);
    expect(store.loadingProjects).toBe(false);
  });
});
