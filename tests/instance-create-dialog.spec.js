import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import InstanceCreateDialog from '@/components/InstanceCreateDialog.vue';

/**
 * The form that exists for one failure.
 *
 * An image reads `MYSQL_ROOT_PASSWORD` while its data directory is empty and
 * never again, so the only moment a password can be set is before the first
 * boot. The app's only route was create-with-defaults and then edit — which
 * writes the value, regenerates the compose file, genuinely recreates the
 * container, reports success, and leaves the database on `root`. Every step
 * works and the outcome is wrong.
 */

const api = vi.hoisted(() => ({ instancePlan: vi.fn() }));

vi.mock('@/lib/ipc', () => ({ api, asList: (v) => (Array.isArray(v) ? v : []) }));

const vuetify = createVuetify({ components, directives });

const PLAN = {
  id: 'mysql-8-0',
  refused: null,
  settings: [
    {
      key: 'ROOT_PASSWORD',
      kind: 'secret',
      // Not masked, unlike every other reading of a secret in the app: there
      // is no instance and no keystore entry, so this is the manifest's
      // published default, sitting in a JSON file anybody with the package can
      // read. Masking it would be theatre.
      value: 'root',
      secret: true,
      isDefault: true,
      defaultValue: 'root',
      required: false,
      options: [],
      label: {},
    },
    {
      key: 'DATABASE',
      kind: 'string',
      value: 'stackvo',
      secret: false,
      isDefault: true,
      defaultValue: 'stackvo',
      required: false,
      options: [],
      label: {},
    },
  ],
  ports: [{ name: 'main', container: 3306, host: 3306, protocol: 'tcp' }],
};

function mountDialog() {
  return mount(
    {
      components: { InstanceCreateDialog },
      template: `
        <v-app>
          <InstanceCreateDialog
            :target="{ service: 'mysql', version: '8.0' }"
            :model-value="true" />
        </v-app>`,
    },
    { attachTo: document.body, global: { plugins: [vuetify, i18n] } }
  );
}

const buttons = () => [...document.body.querySelectorAll('button')];
const button = (text) => buttons().find((b) => b.textContent.trim() === text);

describe('the create dialog', () => {
  let wrapper;

  beforeEach(() => {
    vi.clearAllMocks();
    api.instancePlan.mockResolvedValue(PLAN);
  });

  afterEach(() => wrapper?.unmount());

  it('asks what would happen without creating anything', async () => {
    wrapper = mountDialog();
    await flushPromises();

    expect(api.instancePlan).toHaveBeenCalledWith('mysql', '8.0');
    // The id it would take, and the sentence that says why the fields matter.
    expect(document.body.textContent).toContain('mysql-8-0');
    expect(document.body.textContent).toContain('initialising an empty data directory');
  });

  it('shows the package defaults, unmasked, and offers them for editing', async () => {
    wrapper = mountDialog();
    await flushPromises();

    const values = wrapper
      .findAllComponents(components.VTextField)
      .map((f) => f.props('modelValue'));
    expect(values).toContain('root');
    expect(values).toContain('stackvo');
    expect(values).toContain('3306');
  });

  it('sends only what was changed', async () => {
    wrapper = mountDialog();
    await flushPromises();

    const password = wrapper
      .findAllComponents(components.VTextField)
      .find((f) => f.props('modelValue') === 'root');
    await password.vm.$emit('update:modelValue', 'a-real-password');
    await flushPromises();

    button('Add instance').click();
    await flushPromises();

    // Only the changed key. A patch repeating every default would write them
    // all into instances.json, which then stops tracking the package: a later
    // version that changes a default would be overridden by a value nobody
    // chose.
    const [payload] = wrapper.findComponent(InstanceCreateDialog).emitted('create')[0];
    expect(payload).toEqual({
      service: 'mysql',
      version: '8.0',
      settings: { ROOT_PASSWORD: 'a-real-password' },
      ports: null,
    });
  });

  it('sends a port the user chose, as a number', async () => {
    wrapper = mountDialog();
    await flushPromises();

    const port = wrapper
      .findAllComponents(components.VTextField)
      .find((f) => f.props('label') === 'main');
    await port.vm.$emit('update:modelValue', '3307');
    await flushPromises();

    button('Add instance').click();
    await flushPromises();

    const [payload] = wrapper.findComponent(InstanceCreateDialog).emitted('create')[0];
    expect(payload.ports).toEqual({ main: 3307 });
    expect(payload.settings).toBeNull();
  });

  it('creates with the package’s own defaults when nothing is touched', async () => {
    wrapper = mountDialog();
    await flushPromises();

    button('Add instance').click();
    await flushPromises();

    // Two nulls, which is exactly what the `+` button did before this dialog
    // existed — the form adds a choice, it does not add an obligation.
    const [payload] = wrapper.findComponent(InstanceCreateDialog).emitted('create')[0];
    expect(payload.settings).toBeNull();
    expect(payload.ports).toBeNull();
  });

  it('says why it cannot create a second instance, rather than failing to open', async () => {
    api.instancePlan.mockResolvedValue({
      ...PLAN,
      refused: 'phpmyadmin declares that only one version may run at a time',
    });

    wrapper = mountDialog();
    await flushPromises();

    expect(document.body.textContent).toContain('only one version may run at a time');
    expect(button('Add instance').disabled).toBe(true);
  });

  it('names the port the allocator could not find a number for', async () => {
    api.instancePlan.mockResolvedValue({
      ...PLAN,
      ports: [{ name: 'main', container: 3306, host: null, protocol: 'tcp' }],
    });

    wrapper = mountDialog();
    await flushPromises();

    // The empty field is the message, so it gets one: the plan answers null
    // per port rather than failing to open, and the user types a number.
    expect(document.body.textContent).toContain('No free port could be found for main');
    expect(button('Add instance').disabled).toBe(true);
  });

  it('will not create while a required setting is empty', async () => {
    api.instancePlan.mockResolvedValue({
      ...PLAN,
      settings: [
        {
          key: 'MASTER_KEY',
          kind: 'string',
          value: '',
          secret: false,
          isDefault: true,
          defaultValue: null,
          required: false,
          options: [],
          label: {},
        },
      ].map((row) => ({ ...row, required: true })),
    });

    wrapper = mountDialog();
    await flushPromises();

    expect(button('Add instance').disabled).toBe(true);
    expect(document.body.textContent).toContain('Required and empty: MASTER_KEY');
  });
});
