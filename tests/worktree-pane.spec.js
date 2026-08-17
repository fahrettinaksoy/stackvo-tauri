import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import WorktreePane from '@/components/project/WorktreePane.vue';

/**
 * N — a branch with an environment of its own.
 *
 * The pane has two roles and the tests are split the same way, because the
 * whole risk of one component doing both is that it draws the wrong one: a
 * project that *has* worktrees showing the env editor for a worktree it is not,
 * or a worktree offering to create one of itself.
 *
 * The rest is about not deciding anything the boundary has already decided.
 * Every derived string — the project name, the hostname, the database name —
 * comes back from `worktreePlan`, and every refusal is its sentence. A pane
 * that assembled either would be a preview showing one thing while the command
 * created another, which is exactly the failure the plan exists to prevent.
 */

const api = vi.hoisted(() => ({
  worktreeSupport: vi.fn(),
  worktreePlan: vi.fn(),
  worktreeCreate: vi.fn(),
  worktreeRemove: vi.fn(),
  worktreeEnvSet: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api, asList: (v) => (Array.isArray(v) ? v : []) }));

const vuetify = createVuetify({ components, directives });

/** `router-link` is the only thing the pane needs from the router. */
const RouterLink = { props: ['to'], template: '<a :href="to"><slot /></a>' };

const mountPane = (name = 'shop') =>
  mount(WorktreePane, {
    props: { name },
    global: { plugins: [vuetify, i18n], stubs: { RouterLink } },
  });

/** A project that can have worktrees, and has one. */
const PARENT = {
  gitAvailable: true,
  repository: true,
  linked: false,
  domain: 'shop.loc',
  currentBranch: 'main',
  branches: [
    { name: 'main', checkedOut: true, current: true },
    { name: 'feature/x', checkedOut: false, current: false },
  ],
  instances: [{ id: 'mysql-9-4', service: 'mysql', kind: 'mysql', running: true }],
  worktrees: [
    {
      name: 'shop-feature-x',
      parent: 'shop',
      branch: 'feature/x',
      domain: 'feature-x.shop.loc',
      path: '/code/shop-feature-x',
      database: { instance: 'mysql-9-4', name: 'stackvo_feature_x' },
      env: {},
      createdAt: '2026-01-01T00:00:00Z',
      exists: true,
      dirty: false,
      orphaned: false,
    },
  ],
};

/** The same repository, seen from inside the worktree. */
const SELF = {
  gitAvailable: true,
  repository: true,
  linked: true,
  record: {
    name: 'shop-feature-x',
    parent: 'shop',
    branch: 'feature/x',
    domain: 'feature-x.shop.loc',
    path: '/code/shop-feature-x',
    database: { instance: 'mysql-9-4', name: 'stackvo_feature_x', seededFrom: 'stackvo' },
    env: { APP_ENV: 'branch' },
    createdAt: '2026-01-01T00:00:00Z',
  },
  effectiveEnv: {
    APP_ENV: 'branch',
    APP_URL: 'https://feature-x.shop.loc',
    DB_DATABASE: 'stackvo_feature_x',
    DB_PASSWORD: '••••••••',
  },
  domain: 'feature-x.shop.loc',
  branches: [],
  instances: [],
  worktrees: [],
};

const PLAN = {
  parent: 'shop',
  branch: 'feature/x',
  newBranch: false,
  name: 'shop-feature-x',
  path: '/code/shop-feature-x',
  domain: 'feature-x.shop.loc',
  database: null,
  warnings: [],
  possible: true,
};

beforeEach(() => {
  vi.clearAllMocks();
  api.worktreeSupport.mockResolvedValue(PARENT);
  api.worktreePlan.mockResolvedValue(PLAN);
  api.worktreeCreate.mockResolvedValue('op-1');
  api.worktreeRemove.mockResolvedValue('op-2');
});

describe('a project that has worktrees', () => {
  it('lists each one with the branch it holds and the name it answers on', async () => {
    const wrapper = mountPane();
    await flushPromises();

    const rows = wrapper.findAll('[data-test="worktree-row"]');
    expect(rows).toHaveLength(1);
    expect(rows[0].text()).toContain('feature/x');
    expect(rows[0].text()).toContain('feature-x.shop.loc');
    // The database is named on the row: "which one is this branch writing to"
    // is the question somebody asks before running a migration.
    expect(rows[0].text()).toContain('stackvo_feature_x');
  });

  it('draws no environment editor, because this project is not a worktree', async () => {
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.find('[data-test="wt-env-row"]').exists()).toBe(false);
  });

  /**
   * The reason comes from the boundary. Assembling it here would mean two
   * implementations of "is this allowed" — and the one on screen would be the
   * one that never runs.
   */
  it('says why it cannot, in the words the boundary used', async () => {
    api.worktreeSupport.mockResolvedValue({
      ...PARENT,
      worktrees: [],
      reason: 'shop is not a git repository, so there are no branches to give an environment to.',
    });

    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.find('[data-test="worktree-reason"]').text()).toContain('not a git repository');
    // And no way to start something that would be refused.
    expect(wrapper.text()).not.toContain('New worktree');
  });
});

describe('the form', () => {
  const open = async () => {
    const wrapper = mountPane();
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((b) => b.text().includes('New worktree'))
      .trigger('click');
    await flushPromises();
    return wrapper;
  };

  it('shows the name and hostname the backend derived, not one of its own', async () => {
    const wrapper = await open();
    wrapper.vm.form.branch = 'feature/x';
    await flushPromises();

    expect(api.worktreePlan).toHaveBeenCalledWith('shop', 'feature/x', expect.any(Object));
    expect(wrapper.text()).toContain('shop-feature-x');
    expect(wrapper.text()).toContain('feature-x.shop.loc');
  });

  /**
   * A refusal is not an error. It belongs beside the field that caused it, and
   * the button it refuses has to be the one that is disabled.
   */
  it('refuses inline and disables Create rather than raising an error', async () => {
    api.worktreePlan.mockResolvedValue({
      ...PLAN,
      possible: false,
      refused: '"feature/x" is already checked out in another worktree.',
    });

    const wrapper = await open();
    wrapper.vm.form.branch = 'feature/x';
    await flushPromises();

    expect(wrapper.find('[data-test="worktree-refused"]').text()).toContain('already checked out');
    const create = wrapper.findAll('button').find((b) => b.text().trim() === 'Create');
    expect(create.attributes('disabled')).toBeDefined();
  });

  /** What proceeds anyway is still said out loud. */
  it('shows a warning without blocking the button', async () => {
    api.worktreePlan.mockResolvedValue({
      ...PLAN,
      warnings: ['shop also answers on *.shop.loc, so feature-x.shop.loc matches two routes.'],
    });

    const wrapper = await open();
    wrapper.vm.form.branch = 'feature/x';
    await flushPromises();

    expect(wrapper.find('[data-test="worktree-warning"]').text()).toContain('two routes');
    const create = wrapper.findAll('button').find((b) => b.text().trim() === 'Create');
    expect(create.attributes('disabled')).toBeUndefined();
  });

  it('passes the database choice through as the backend named it', async () => {
    const wrapper = await open();
    wrapper.vm.form.branch = 'feature/x';
    wrapper.vm.form.database = 'copy';
    wrapper.vm.form.instance = 'mysql-9-4';
    await flushPromises();

    await wrapper
      .findAll('button')
      .find((b) => b.text().trim() === 'Create')
      .trigger('click');
    await flushPromises();

    expect(api.worktreeCreate).toHaveBeenCalledWith('shop', 'feature/x', {
      newBranch: false,
      name: null,
      database: 'copy',
      instance: 'mysql-9-4',
    });
  });
});

describe('a project that is a worktree', () => {
  beforeEach(() => api.worktreeSupport.mockResolvedValue(SELF));

  it('says what it is a branch of, and where its data came from', async () => {
    const wrapper = mountPane('shop-feature-x');
    await flushPromises();

    expect(wrapper.text()).toContain('feature/x');
    expect(wrapper.text()).toContain('feature-x.shop.loc');
    expect(wrapper.text()).toContain('stackvo_feature_x');
    // Copied or empty is the question asked three weeks later.
    expect(wrapper.text()).toContain('stackvo');
    // And no way to create a worktree of a worktree.
    expect(wrapper.text()).not.toContain('New worktree');
  });

  it('edits its own variables rather than the project settings file', async () => {
    api.worktreeEnvSet.mockResolvedValue({
      env: { APP_ENV: 'branch', EXTRA: 'yes' },
      effective: { ...SELF.effectiveEnv, EXTRA: 'yes' },
    });

    const wrapper = mountPane('shop-feature-x');
    await flushPromises();

    expect(wrapper.findAll('[data-test="wt-env-row"]')).toHaveLength(1);
    wrapper.vm.rows.push({ key: 'EXTRA', value: 'yes' });
    await flushPromises();

    await wrapper
      .findAll('button')
      .find((b) => b.text().trim() === 'Save')
      .trigger('click');
    await flushPromises();

    expect(api.worktreeEnvSet).toHaveBeenCalledWith('shop-feature-x', {
      APP_ENV: 'branch',
      EXTRA: 'yes',
    });
  });

  /**
   * The derived half is shown and is not editable. Putting `DB_PASSWORD` in a
   * text field would be offering to edit a copy of a value that is read live on
   * every render — the edit would appear to work and change nothing.
   */
  it('shows what it was given separately from what was typed, with the secret masked', async () => {
    const wrapper = mountPane('shop-feature-x');
    await flushPromises();

    const derived = wrapper.findAll('[data-test="wt-derived"]').map((d) => d.text());
    expect(derived.join(' ')).toContain('APP_URL');
    expect(derived.join(' ')).toContain('DB_DATABASE');
    // The typed one is in the editor above, not repeated here.
    expect(derived.join(' ')).not.toContain('APP_ENV');
    // Never the real password, whatever the record says.
    expect(wrapper.text()).toContain('••••••••');
  });

  /**
   * Each destructive option is its own switch, off by default. One button that
   * did all of it would make the branch and the database casualties of pressing
   * the only thing on offer.
   */
  it('asks about the branch and the database separately, and defaults both to off', async () => {
    const wrapper = mountPane('shop-feature-x');
    await flushPromises();

    await wrapper
      .findAll('button')
      .find((b) => b.text().trim() === 'Remove')
      .trigger('click');
    await flushPromises();

    expect(wrapper.vm.removal).toEqual({
      force: false,
      dropDatabase: false,
      deleteBranch: false,
    });

    await wrapper.vm.confirmRemove();
    expect(api.worktreeRemove).toHaveBeenCalledWith('shop-feature-x', {
      force: false,
      dropDatabase: false,
      deleteBranch: false,
    });
  });

  /** Removing the project the page is about means leaving the page. */
  it('reports its own removal so the page can navigate away', async () => {
    const wrapper = mountPane('shop-feature-x');
    await flushPromises();

    await wrapper
      .findAll('button')
      .find((b) => b.text().trim() === 'Remove')
      .trigger('click');
    await flushPromises();
    await wrapper.vm.confirmRemove();
    await flushPromises();

    expect(wrapper.emitted('removed')).toBeTruthy();
    expect(wrapper.emitted('changed')).toBeFalsy();
  });
});
