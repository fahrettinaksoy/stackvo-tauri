import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
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

const vuetify = createVuetify({ components, directives });
const ref = (value) => ({ value });

const DOCKERFILE = 'FROM php:8.3-fpm-alpine\nRUN docker-php-ext-install pdo_mysql\nCOPY . /app\n';

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.projectDockerfilePreview = { dockerfile: DOCKERFILE, matches: true, differences: [] };
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
    const save = clean
      .findAll('button')
      .find((b) => b.text() === i18n.global.t('detail.save'));
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

    settle({ dockerfile: 'FROM node:22-alpine\n', matches: false, differences: ['ext'] });
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
