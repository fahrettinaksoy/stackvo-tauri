import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import ServiceSettingsSheet from '@/components/ServiceSettingsSheet.vue';

/**
 * The version field, and why it is a combobox rather than a select.
 *
 * Setting a service's image tag was always possible — `VERSION` reached the
 * sheet as a text field like every other key — but only if you already knew
 * that mongo publishes `8.0` and not `8`, and that Grafana stopped tagging
 * `10.4`. The catalog answers that. What these tests hold in place is that it
 * answers it without taking the old capability away: the list is short by
 * design, a registry is not, and a closed menu would make an unlisted tag
 * unreachable from the app that is supposed to be how you set it.
 */

const api = vi.hoisted(() => ({
  serviceSettings: vi.fn(),
  serviceApplySettings: vi.fn(),
  envReveal: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api, asList: (v) => (Array.isArray(v) ? v : []) }));

const vuetify = createVuetify({ components, directives });

const VERSION_ROW = {
  key: 'VERSION',
  envKey: 'SERVICE_MONGO_VERSION',
  value: '8.0',
  secret: false,
  isDefault: true,
  options: ['8.0', '8.3', '8.2', '7.0', '6.0', '5.0'],
};

const PORT_ROW = {
  key: 'HOST_PORT',
  envKey: 'SERVICE_MONGO_HOST_PORT',
  value: '27017',
  secret: false,
  isDefault: true,
  options: [],
};

/**
 * Mounted into the document rather than detached, and with the teleports left
 * alone.
 *
 * The sheet is a navigation drawer and the confirmation is an overlay, so both
 * leave the wrapper's subtree on purpose. Stubbing teleport keeps the drawer in
 * place but replaces the overlay's contents with the stub itself, which reads
 * as "the confirmation lists no keys" — true of the stub and of nothing else.
 * So the DOM is queried through `document` and the component tree through the
 * wrapper, which `findComponent` walks regardless of where anything rendered.
 */
function mountSheet() {
  return mount(
    {
      components: { ServiceSettingsSheet },
      template: `
        <v-app>
          <ServiceSettingsSheet :service="{ id: 'mongo' }" :model-value="true" />
        </v-app>`,
    },
    { attachTo: document.body, global: { plugins: [vuetify, i18n] } }
  );
}

const buttons = () => [...document.body.querySelectorAll('button')];
const button = (text) => buttons().find((b) => b.textContent.trim() === text);
const labels = () => [...document.body.querySelectorAll('label')].map((l) => l.textContent.trim());

describe('the service version catalog', () => {
  let wrapper;

  beforeEach(() => {
    vi.clearAllMocks();
    api.serviceSettings.mockResolvedValue([VERSION_ROW, PORT_ROW]);
    api.serviceApplySettings.mockResolvedValue('op-1');
  });

  afterEach(() => wrapper?.unmount());

  it('offers the catalog on the version row and a plain field on the others', async () => {
    wrapper = mountSheet();
    await flushPromises();

    // Asserted by component rather than by counting inputs: a combobox *is* a
    // text field with a menu attached, so "there are two inputs" would pass
    // whether or not the version row ever got its list.
    const comboboxes = wrapper.findAllComponents(components.VCombobox);
    expect(comboboxes).toHaveLength(1);
    expect(comboboxes[0].props('items')).toEqual(VERSION_ROW.options);
    expect(comboboxes[0].props('modelValue')).toBe('8.0');

    expect(labels()).toContain('Version');
    expect(labels()).toContain('Host port');
  });

  it('sends the picked version and nothing else', async () => {
    wrapper = mountSheet();
    await flushPromises();

    await wrapper.findComponent(components.VCombobox).vm.$emit('update:modelValue', '7.0');
    await flushPromises();

    // Applying is behind a confirmation, because it recreates a container.
    expect(button('Apply and rebuild').disabled).toBe(false);
    button('Apply and rebuild').click();
    await flushPromises();

    // The confirmation names the keys about to be written. One key: picking a
    // version must not drag the rows nobody touched along with it.
    const keys = [...document.body.querySelectorAll('.v-dialog .v-chip')].map((c) =>
      c.textContent.trim()
    );
    expect(keys).toEqual(['SERVICE_MONGO_VERSION']);

    button('Apply').click();
    await flushPromises();

    expect(api.serviceApplySettings).toHaveBeenCalledWith('mongo', {
      SERVICE_MONGO_VERSION: '7.0',
    });
  });

  it('keeps a tag the catalog does not list', async () => {
    wrapper = mountSheet();
    await flushPromises();

    const combobox = wrapper.findComponent(components.VCombobox);
    // What somebody pinning a patch release types. A select would have thrown
    // it away; surviving is the entire reason for the component choice.
    await combobox.vm.$emit('update:modelValue', '8.0.28');
    await flushPromises();

    expect(combobox.props('modelValue')).toBe('8.0.28');
    expect(button('Apply and rebuild').disabled).toBe(false);
  });

  it('goes back to being a text field when the catalog is empty', async () => {
    // A workspace that set `SERVICE_MONGO_VERSIONS=` has asked for the plain
    // field back, and the sheet has to give it rather than render an empty menu.
    api.serviceSettings.mockResolvedValue([{ ...VERSION_ROW, options: [] }, PORT_ROW]);

    wrapper = mountSheet();
    await flushPromises();

    expect(wrapper.findAllComponents(components.VCombobox)).toHaveLength(0);
    expect(labels()).toContain('Version');
  });

  it('leaves a secret masked whether or not its row has options', async () => {
    // The reveal path is orthogonal to the catalog and has to stay that way:
    // the version row grew a menu, and the password row beside it must not have
    // become readable on the way.
    api.serviceSettings.mockResolvedValue([
      VERSION_ROW,
      {
        key: 'ROOT_PASSWORD',
        envKey: 'SERVICE_MONGO_ROOT_PASSWORD',
        value: '••••••••',
        secret: true,
        isDefault: true,
        options: [],
      },
    ]);

    wrapper = mountSheet();
    await flushPromises();

    const fields = [...document.body.querySelectorAll('input')].map((i) => i.value);
    expect(fields).toContain('••••••••');
    expect(api.envReveal).not.toHaveBeenCalled();
  });
});
