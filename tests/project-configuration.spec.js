import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Configuration section's two extracted panes: the manifest editor and the
 * Dockerfile preview.
 *
 * The manifest pane is deliberately *not* the owner of the file. The same text
 * is re-read from disk whenever the Xdebug pane rewrites it, so a pane holding
 * its own copy would keep showing the stale one — hence a `v-model` and a view
 * that saves. That is the property worth a test, because it is the kind of
 * thing a later "simplification" removes.
 */

globalThis.visualViewport = undefined;

const replies = {};
const calls = [];

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy(
    {},
    {
      get:
        (_t, name) =>
        (...args) => {
          calls.push([String(name), ...args]);
          const reply = replies[name];
          return typeof reply === 'function' ? reply(...args) : Promise.resolve(reply);
        },
    }
  ),
}));

const { useDockerfilePreview } = await import('@/composables/useDockerfilePreview');
const { i18n } = await import('@/i18n');
const ManifestPane = (await import('@/components/project/ManifestPane.vue')).default;
const DockerfilePane = (await import('@/components/project/DockerfilePane.vue')).default;
const LocalOverridePane = (await import('@/components/project/LocalOverridePane.vue')).default;
const HooksPane = (await import('@/components/project/HooksPane.vue')).default;

const vuetify = createVuetify({ components, directives });
const ref = (value) => ({ value });

const DOCKERFILE = 'FROM php:8.3-fpm-alpine\nRUN docker-php-ext-install pdo_mysql\nCOPY . /app\n';

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.projectDockerfilePreview = {
    dockerfile: DOCKERFILE,
    matchesGenerated: true,
    differences: [],
  };
});

describe('the manifest editor', () => {
  const open = (props, listeners = {}) =>
    mount(
      {
        components: { ManifestPane },
        template: '<v-app><ManifestPane v-bind="$attrs" v-on="$attrs.listeners || {}" /></v-app>',
      },
      {
        attrs: { name: 'shop', ...props, ...listeners },
        global: { plugins: [createPinia(), vuetify, i18n] },
      }
    );

  it('shows the text it was given rather than fetching one', async () => {
    const wrapper = open({ modelValue: '{\n  "runtime": "php"\n}' });

    expect(calls, 'the view owns the manifest, not this pane').toEqual([]);
    expect(wrapper.find('textarea').element.value).toContain('"runtime": "php"');
  });

  /**
   * Two separate signals on one keystroke: the new text, and the fact that it
   * now differs from disk. The view needs both — it re-reads the file when the
   * Xdebug pane rewrites it, and must not clobber an unsaved edit silently.
   */
  it('reports the edit and the fact that there is one', async () => {
    const onUpdate = vi.fn();
    const onDirty = vi.fn();
    const wrapper = open({
      modelValue: '{}',
      'onUpdate:modelValue': onUpdate,
      onDirty,
    });

    await wrapper.find('textarea').setValue('{ "runtime": "node" }');

    expect(onUpdate).toHaveBeenCalledWith('{ "runtime": "node" }');
    expect(onDirty).toHaveBeenCalled();
  });

  /** Saving an unchanged file is a write for no reason. */
  it('cannot be saved until something has changed', async () => {
    const clean = open({ modelValue: '{}', dirty: false });
    const save = clean.findAll('button').find((b) => b.text() === i18n.global.t('detail.save'));
    expect(save.attributes('disabled')).toBeDefined();

    const dirty = open({ modelValue: '{}', dirty: true });
    expect(
      dirty
        .findAll('button')
        .find((b) => b.text() === i18n.global.t('detail.save'))
        .attributes('disabled')
    ).toBeUndefined();
  });

  it('asks the page to save rather than writing the file itself', async () => {
    const onSave = vi.fn();
    const wrapper = open({ modelValue: '{}', dirty: true, onSave });

    await wrapper
      .findAll('button')
      .find((b) => b.text() === i18n.global.t('detail.save'))
      .trigger('click');

    expect(onSave).toHaveBeenCalled();
    expect(calls.some(([n]) => n === 'projectManifestWrite')).toBe(false);
  });
});

/**
 * Both panes on this tab fold shut, and what stays outside the fold is the
 * point of the design.
 *
 * The configuration tab was several screens tall for one reason: a manifest
 * editor fixed at 24 rows and a Dockerfile preview that printed all ~120
 * generated lines. Neither is what the tab is for, so both start closed — but
 * closing a pane must not take its controls with it. Saving a draft and the
 * chip that says whether the generated file on disk is still current both have
 * to be readable while the body is shut.
 */
describe('the panes that fold shut', () => {
  const toggleOf = (wrapper) => wrapper.get('.pane-toggle');

  it('starts closed, on both of them', async () => {
    const manifest = mount(
      {
        components: { ManifestPane },
        template: '<v-app><ManifestPane name="shop" model-value="{}" /></v-app>',
      },
      { global: { plugins: [createPinia(), vuetify, i18n] } }
    );
    const dockerfile = mount(
      {
        components: { DockerfilePane },
        template: '<v-app><DockerfilePane name="shop" /></v-app>',
      },
      { global: { plugins: [vuetify, i18n] } }
    );
    await flushPromises();

    for (const wrapper of [manifest, dockerfile]) {
      expect(toggleOf(wrapper).attributes('aria-expanded')).toBe('false');
      expect(wrapper.get('.pane-body').attributes('style')).toContain('display: none');
    }
  });

  it('opens on the heading, which is the control', async () => {
    const wrapper = mount(
      {
        components: { ManifestPane },
        template: '<v-app><ManifestPane name="shop" model-value="{}" /></v-app>',
      },
      { global: { plugins: [createPinia(), vuetify, i18n] } }
    );

    await toggleOf(wrapper).trigger('click');

    expect(toggleOf(wrapper).attributes('aria-expanded')).toBe('true');
    expect(wrapper.get('.pane-body').attributes('style') ?? '').not.toContain('display: none');
  });

  /** The body is named, so the heading can say what it opens. */
  it('points the heading at the body it controls', async () => {
    const wrapper = mount(
      {
        components: { ManifestPane },
        template: '<v-app><ManifestPane name="shop" model-value="{}" /></v-app>',
      },
      { global: { plugins: [createPinia(), vuetify, i18n] } }
    );

    const controls = toggleOf(wrapper).attributes('aria-controls');
    expect(controls).toBeTruthy();
    expect(wrapper.get('.pane-body').attributes('id')).toBe(controls);
  });

  /**
   * A save button that disappears with the editor is a save you have to go
   * looking for — and the draft survives folding, because the text belongs to
   * the view rather than to the pane.
   */
  it('keeps the manifest actions out of the fold', async () => {
    const onSave = vi.fn();
    const wrapper = mount(
      {
        components: { ManifestPane },
        template:
          '<v-app><ManifestPane name="shop" model-value="{}" :dirty="true" @save="onSave" /></v-app>',
        setup: () => ({ onSave }),
      },
      { global: { plugins: [createPinia(), vuetify, i18n] } }
    );

    const save = wrapper.findAll('button').find((b) => b.text() === i18n.global.t('detail.save'));
    expect(save.element.closest('.pane-body'), 'the save button folded away too').toBeNull();

    await save.trigger('click');
    expect(onSave).toHaveBeenCalled();
  });

  /** A closed pane that hides its own warning is a warning nobody sees. */
  it('keeps the bash-agreement verdict out of the fold', async () => {
    replies.projectDockerfilePreview = {
      dockerfile: DOCKERFILE,
      matchesGenerated: false,
      differences: [],
    };

    const wrapper = mount(
      {
        components: { DockerfilePane },
        template: '<v-app><DockerfilePane name="shop" /></v-app>',
      },
      { global: { plugins: [vuetify, i18n] } }
    );
    await vi.waitFor(() =>
      expect(wrapper.text()).toContain(i18n.global.t('detail.generatedStale'))
    );

    const chip = wrapper
      .findAll('.v-chip')
      .find((c) => c.text() === i18n.global.t('detail.generatedStale'));
    expect(chip.element.closest('.pane-body'), 'the verdict folded away with the file').toBeNull();
  });
});

describe('the Dockerfile preview', () => {
  it('renders as soon as it is mounted', async () => {
    const wrapper = mount(
      {
        components: { DockerfilePane },
        template: '<v-app><DockerfilePane name="shop" /></v-app>',
      },
      { global: { plugins: [vuetify, i18n] } }
    );

    await vi.waitFor(() => expect(wrapper.text()).toContain('php:8.3-fpm-alpine'));
    expect(calls[0]).toEqual(['projectDockerfilePreview', 'shop', false]);
  });

  it('numbers the file by line', async () => {
    const d = useDockerfilePreview(ref('shop'));
    await d.load();

    expect(d.lines.value).toHaveLength(DOCKERFILE.split('\n').length);
    expect(d.lines.value[0]).toBe('FROM php:8.3-fpm-alpine');
  });

  it('has no lines to number before anything is rendered', () => {
    const d = useDockerfilePreview(ref('shop'));
    expect(d.lines.value).toEqual([]);
  });

  /**
   * `strict` is a different question about the same project, and the flag is
   * what carries it.
   */
  it('asks for the strict rendering by flag, not by a second call', async () => {
    const d = useDockerfilePreview(ref('shop'));
    await d.load('strict');

    expect(calls.at(-1)).toEqual(['projectDockerfilePreview', 'shop', true]);
    expect(d.mode.value).toBe('strict');
  });

  /**
   * Leaving the previous render up while the other mode is fetched shows one
   * mode's file under the other mode's heading.
   */
  it('clears the old rendering before fetching the other mode', async () => {
    let settle;
    const d = useDockerfilePreview(ref('shop'));
    await d.load('compat');
    expect(d.preview.value).toBeTruthy();

    replies.projectDockerfilePreview = () => new Promise((resolve) => (settle = resolve));
    const done = d.load('strict');
    expect(d.preview.value, 'the compat file was still on screen under the strict heading').toBe(
      null
    );

    settle({ dockerfile: 'FROM node:22-alpine\n', matchesGenerated: false, differences: ['ext'] });
    await done;
    expect(d.lines.value[0]).toBe('FROM node:22-alpine');
  });

  it('reports a failed render and stops loading', async () => {
    replies.projectDockerfilePreview = () =>
      Promise.reject({ code: 'MANIFEST_INVALID', message: 'bad runtime' });

    const d = useDockerfilePreview(ref('shop'));
    expect(await d.load()).toBe(null);
    expect(d.error.value.code).toBe('MANIFEST_INVALID');
    expect(d.loading.value).toBe(false);
    expect(d.lines.value).toEqual([]);
  });
});

/**
 * `stackvo.local.json`, the pane below the manifest editor (B-2).
 *
 * Unlike the manifest pane this one *does* own its file — nothing else in the
 * app writes it — so it fetches, and the three things worth asserting are the
 * three that are silent when wrong: which fields are actually in force, that a
 * refused key is named rather than dropped, and that git's three answers are
 * three states rather than a boolean with a default.
 */
describe('the machine-local override pane', () => {
  const open = () =>
    mount(
      {
        components: { LocalOverridePane },
        template: '<v-app><LocalOverridePane name="shop" /></v-app>',
      },
      { global: { plugins: [createPinia(), vuetify, i18n] } }
    );

  const state = (over = {}) => ({
    text: '{\n  "php": { "version": "8.3" }\n}\n',
    exists: true,
    applied: ['php.version'],
    refused: [],
    ignored: true,
    ...over,
  });

  /**
   * The hazard this pane exists to answer is a value in force that nobody
   * remembers setting, so the fields are named rather than summarised.
   */
  it('names the fields in force rather than saying that some are', async () => {
    replies.projectLocalRead = state();
    const wrapper = open();
    await flushPromises();

    expect(wrapper.text()).toContain('php.version');
    expect(wrapper.find('textarea').element.value).toContain('8.3');
  });

  it('names a refused key instead of dropping it', async () => {
    replies.projectLocalRead = state({ refused: ['runtime'] });
    const wrapper = open();
    await flushPromises();

    expect(wrapper.text()).toContain('runtime');
  });

  /**
   * Three states, and only one is a warning: a directory that is not a git
   * repository has nothing to leak into anybody's clone.
   */
  it('warns only when git says it would commit the file', async () => {
    replies.projectLocalRead = state({ ignored: false });
    let wrapper = open();
    await flushPromises();
    expect(
      wrapper.findAll('.v-alert--variant-tonal').some((a) => a.text().includes('.gitignore'))
    ).toBe(true);

    replies.projectLocalRead = state({ ignored: null });
    wrapper = open();
    await flushPromises();
    expect(wrapper.text()).not.toContain('.gitignore');
  });

  /** Removing is saving nothing — one command, so there is one state to be in. */
  it('removes the file by saving empty text', async () => {
    replies.projectLocalRead = state();
    replies.projectLocalWrite = state({ text: '', exists: false, applied: [] });
    const wrapper = open();
    await flushPromises();

    const remove = wrapper
      .findAll('button')
      .find((b) => b.text() === i18n.global.t('local.remove'));
    await remove.trigger('click');
    await flushPromises();

    expect(calls.filter((c) => c[0] === 'projectLocalWrite')).toEqual([
      ['projectLocalWrite', 'shop', ''],
    ]);
  });
});

/**
 * The hooks pane (B-3).
 *
 * The whole point of this screen is that somebody can read the commands before
 * they run on their machine, so the assertions are about exactly that: the
 * commands are printed in full, host steps are marked as such, and the approval
 * carries back the digest the plan arrived with rather than only a project name.
 */
describe('the hooks pane', () => {
  const open = () =>
    mount(
      {
        components: { HooksPane },
        template: '<v-app><HooksPane name="shop" /></v-app>',
      },
      { global: { plugins: [createPinia(), vuetify, i18n] } }
    );

  const plan = (steps, digest = 'abc123') => [
    { event: 'post-build', steps: [] },
    { event: 'post-start', steps, digest },
    { event: 'pre-stop', steps: [] },
  ];

  it('prints each command in full rather than summarising them', async () => {
    replies.projectHooksPlan = plan([
      { kind: 'exec', command: 'php artisan migrate --force' },
      { kind: 'host', command: 'say up', blocked: 'needs-consent' },
    ]);
    const wrapper = open();
    await flushPromises();

    expect(wrapper.text()).toContain('php artisan migrate --force');
    expect(wrapper.text()).toContain('say up');
  });

  /** Where a step runs is the whole of its risk, so it is on the row. */
  it('marks which steps would run on this machine', async () => {
    replies.projectHooksPlan = plan([
      { kind: 'exec', command: 'a' },
      { kind: 'host', command: 'b', blocked: 'needs-consent' },
    ]);
    const wrapper = open();
    await flushPromises();

    expect(wrapper.text()).toContain(i18n.global.t('hooks.onThisMachine'));
    expect(wrapper.text()).toContain(i18n.global.t('hooks.inContainer'));
  });

  /**
   * The receipt property. Approving sends back the digest that was on screen,
   * so a manifest that changed in between is refused by the backend.
   */
  it('sends the digest the plan arrived with when approving', async () => {
    replies.projectHooksPlan = plan([{ kind: 'host', command: 'b', blocked: 'needs-consent' }]);
    replies.projectHooksApprove = plan([{ kind: 'host', command: 'b' }]);
    const wrapper = open();
    await flushPromises();

    const approve = wrapper
      .findAll('button')
      .find((b) => b.text() === i18n.global.t('hooks.approve'));
    await approve.trigger('click');
    await flushPromises();

    expect(calls.filter((c) => c[0] === 'projectHooksApprove')).toEqual([
      ['projectHooksApprove', 'shop', 'abc123'],
    ]);
    // And once approved the offer becomes a withdrawal, not a second approval.
    expect(wrapper.text()).toContain(i18n.global.t('hooks.revoke'));
  });

  /** Container-only hooks are never gated, so there is nothing to approve. */
  it('offers no approval when nothing would run on this machine', async () => {
    replies.projectHooksPlan = plan([{ kind: 'exec', command: 'a' }], undefined);
    const wrapper = open();
    await flushPromises();

    expect(wrapper.text()).not.toContain(i18n.global.t('hooks.approve'));
    expect(wrapper.text()).toContain('a');
  });

  /**
   * A policy block is an administrator's decision and not a question the person
   * here can answer, so it replaces the button rather than sitting beside it.
   */
  it('explains a policy block instead of offering an approval it cannot honour', async () => {
    replies.projectHooksPlan = plan([{ kind: 'host', command: 'b', blocked: 'policy-host' }]);
    const wrapper = open();
    await flushPromises();

    expect(wrapper.text()).toContain(i18n.global.t('hooks.policyHost'));
    expect(wrapper.text()).not.toContain(i18n.global.t('hooks.approve'));
  });

  /** A project with no hooks gets no pane, not an empty one. */
  it('renders nothing at all when the project declares no hooks', async () => {
    replies.projectHooksPlan = plan([], undefined);
    const wrapper = open();
    await flushPromises();

    expect(wrapper.text()).not.toContain(i18n.global.t('hooks.title'));
  });
});
