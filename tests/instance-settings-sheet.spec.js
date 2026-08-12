import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import InstanceSettingsSheet from '@/components/InstanceSettingsSheet.vue';

/**
 * What an instance is configured with, edited from its manifest.
 *
 * This replaced a sheet that edited `SERVICE_<ID>_*` keys in `.env`, and the
 * difference is not cosmetic: `SERVICE_MYSQL_DATABASE` names a service two
 * versions of can be running, and there is no answer to which one it meant. The
 * settings here belong to `mysql-8-0` and are written to `instances.json`.
 *
 * The version is gone from the form for the same reason. It was a key in `.env`
 * with a catalog of tags behind it; it is the instance's identity now, and
 * changing it means creating another instance rather than rewriting this one.
 * What survives from that work is the control: a row carrying options gets a
 * combobox, because a manifest listing the values it knows about must not make
 * the one it did not think of unreachable from the app that is supposed to be
 * how you set it.
 */

const api = vi.hoisted(() => ({
  instanceSettings: vi.fn(),
  instanceApplySettings: vi.fn(),
  instanceReveal: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api, asList: (v) => (Array.isArray(v) ? v : []) }));

const vuetify = createVuetify({ components, directives });

/** A setting whose manifest enumerates what it knows about. */
const LISTED_ROW = {
  key: 'LOG_LEVEL',
  kind: 'string',
  value: 'warn',
  secret: false,
  isDefault: true,
  required: false,
  options: ['debug', 'info', 'warn', 'error'],
  label: {},
};

const PLAIN_ROW = {
  key: 'DATABASE',
  kind: 'string',
  value: 'stackvo',
  secret: false,
  isDefault: true,
  required: false,
  options: [],
  label: {},
};

const SECRET_ROW = {
  key: 'ROOT_PASSWORD',
  kind: 'secret',
  value: '••••••••',
  secret: true,
  isDefault: true,
  required: false,
  options: [],
  label: {},
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
      components: { InstanceSettingsSheet },
      template: `
        <v-app>
          <InstanceSettingsSheet :instance="{ id: 'mysql-8-0' }" :model-value="true" />
        </v-app>`,
    },
    { attachTo: document.body, global: { plugins: [vuetify, i18n] } }
  );
}

const buttons = () => [...document.body.querySelectorAll('button')];
const button = (text) => buttons().find((b) => b.textContent.trim() === text);
const labels = () => [...document.body.querySelectorAll('label')].map((l) => l.textContent.trim());

describe('the instance settings sheet', () => {
  let wrapper;

  beforeEach(() => {
    vi.clearAllMocks();
    api.instanceSettings.mockResolvedValue([LISTED_ROW, PLAIN_ROW]);
    api.instanceApplySettings.mockResolvedValue('op-1');
  });

  afterEach(() => wrapper?.unmount());

  it('asks the instance for its settings, not a service name', async () => {
    wrapper = mountSheet();
    await flushPromises();

    expect(api.instanceSettings).toHaveBeenCalledWith('mysql-8-0');
  });

  it('offers the list where the manifest has one and a plain field elsewhere', async () => {
    wrapper = mountSheet();
    await flushPromises();

    // Asserted by component rather than by counting inputs: a combobox *is* a
    // text field with a menu attached, so "there are two inputs" would pass
    // whether or not the listed row ever got its options.
    const comboboxes = wrapper.findAllComponents(components.VCombobox);
    expect(comboboxes).toHaveLength(1);
    expect(comboboxes[0].props('items')).toEqual(LISTED_ROW.options);
    expect(comboboxes[0].props('modelValue')).toBe('warn');

    // The shared vocabulary translates `DATABASE`; the rest keeps the term its
    // own documentation uses, in sentence case.
    expect(labels()).toContain('Database');
    expect(labels()).toContain('Log level');
  });

  it('sends the touched key and nothing else', async () => {
    wrapper = mountSheet();
    await flushPromises();

    await wrapper.findComponent(components.VCombobox).vm.$emit('update:modelValue', 'debug');
    await flushPromises();

    // Applying is behind a confirmation, because it recreates a container.
    expect(button('Apply and rebuild').disabled).toBe(false);
    button('Apply and rebuild').click();
    await flushPromises();

    // The confirmation names what is about to be written, as `was → is`. One
    // row: editing one field must not drag the fields nobody touched along
    // with it.
    const changes = [...document.body.querySelectorAll('.v-dialog .change')];
    expect(changes).toHaveLength(1);
    expect(changes[0].textContent).toContain('LOG_LEVEL');
    // Both halves, because a key on its own does not say what is happening to
    // it, and this dialog is the last look before a container is rebuilt.
    expect(changes[0].textContent).toContain('warn');
    expect(changes[0].textContent).toContain('debug');

    button('Apply').click();
    await flushPromises();

    // Null rather than an empty map for the ports: the command reads "no port
    // patch" from the absence, and an empty object would be a patch that
    // changes nothing arriving on every apply.
    expect(api.instanceApplySettings).toHaveBeenCalledWith(
      'mysql-8-0',
      { LOG_LEVEL: 'debug' },
      null
    );
  });

  it('keeps a value the manifest does not list', async () => {
    wrapper = mountSheet();
    await flushPromises();

    const combobox = wrapper.findComponent(components.VCombobox);
    // What somebody with a level the package author did not enumerate types. A
    // select would have thrown it away; surviving is the whole reason for the
    // component choice.
    await combobox.vm.$emit('update:modelValue', 'trace');
    await flushPromises();

    expect(combobox.props('modelValue')).toBe('trace');
    expect(button('Apply and rebuild').disabled).toBe(false);
  });

  it('is a plain text field when the manifest enumerates nothing', async () => {
    // Which is every setting in every package shipped today: `options` is
    // carried through the boundary so a manifest that grows one needs no change
    // here, and until then the form is text fields.
    api.instanceSettings.mockResolvedValue([PLAIN_ROW]);

    wrapper = mountSheet();
    await flushPromises();

    expect(wrapper.findAllComponents(components.VCombobox)).toHaveLength(0);
    expect(labels()).toContain('Database');
  });

  it('leaves a secret masked until it is asked for', async () => {
    api.instanceSettings.mockResolvedValue([LISTED_ROW, SECRET_ROW]);

    wrapper = mountSheet();
    await flushPromises();

    const fields = [...document.body.querySelectorAll('input')].map((i) => i.value);
    expect(fields).toContain('••••••••');
    // Rendering the form must not go near the keystore. The reveal is a
    // separate act, and it is the one the user takes.
    expect(api.instanceReveal).not.toHaveBeenCalled();
  });

  /**
   * A setting the package says the service cannot start without.
   *
   * No package in the catalogue marks one today, which is exactly why this is
   * a test rather than a thing somebody would notice: the flag crosses the
   * boundary, the form ignored it, and the first package to use it would have
   * shipped a field that looks optional, empties without complaint, and takes
   * the container down on apply.
   */
  const REQUIRED_ROW = {
    key: 'MASTER_KEY',
    kind: 'string',
    value: 'seed-key',
    secret: false,
    isDefault: false,
    required: true,
    options: [],
    label: {},
  };

  it('marks a required setting and refuses to apply it empty', async () => {
    api.instanceSettings.mockResolvedValue([REQUIRED_ROW, PLAIN_ROW]);

    wrapper = mountSheet();
    await flushPromises();

    // The asterisk is the whole of what says "this one is not optional" — the
    // fields carry `hide-details`, so there is no message line to put it in.
    expect(labels()).toContain('Master key *');

    const field = wrapper
      .findAllComponents(components.VTextField)
      .find((f) => f.props('modelValue') === 'seed-key');
    await field.vm.$emit('update:modelValue', '   ');
    await flushPromises();

    // Trimmed: a required field holding one space passes every check that asks
    // whether it is empty and none of the ones the service makes of it.
    expect(field.props('error')).toBe(true);
    expect(button('Apply and rebuild').disabled).toBe(true);
    expect(document.body.textContent).toContain('Required and empty: MASTER_KEY');

    // And it comes back the moment there is something to save.
    await field.vm.$emit('update:modelValue', 'another-key');
    await flushPromises();
    expect(button('Apply and rebuild').disabled).toBe(false);
  });

  it('never calls a masked secret empty', async () => {
    // Its value is eight bullets whether the keystore holds a password or has
    // never been written, so the form cannot answer "is it empty" without
    // revealing it — and revealing secrets in order to validate them would
    // defeat the mask. Unrevealed, it is left alone.
    api.instanceSettings.mockResolvedValue([{ ...SECRET_ROW, required: true }]);

    wrapper = mountSheet();
    await flushPromises();

    expect(document.body.textContent).not.toContain('Required and empty');
    expect(api.instanceReveal).not.toHaveBeenCalled();
  });

  /**
   * The sharpest thing wrong with this form, and the screen was silent about
   * it: `MYSQL_ROOT_PASSWORD` is read by the entrypoint only while the data
   * directory is empty. Every step of applying it to a database that has data
   * succeeds — written, regenerated, container genuinely recreated — and the
   * password in the database does not move.
   */
  it('warns that a first-boot credential may not reach an instance with data', async () => {
    api.instanceSettings.mockResolvedValue([SECRET_ROW, PLAIN_ROW]);

    wrapper = mountSheet();
    await flushPromises();

    // Revealed first, because saving the mask is refused — which is the same
    // route a user takes to change one.
    api.instanceReveal.mockResolvedValue('root');
    const eye = [...document.body.querySelectorAll('button')].find(
      (b) => b.getAttribute('aria-label') === 'Reveal'
    );
    eye.click();
    await flushPromises();

    const field = wrapper
      .findAllComponents(components.VTextField)
      .find((f) => f.props('modelValue') === 'root');
    await field.vm.$emit('update:modelValue', 'a-new-password');
    await flushPromises();

    button('Apply and rebuild').click();
    await flushPromises();

    const text = document.body.textContent;
    expect(text).toContain('may not take effect');
    // Named, not general: a caveat about "some settings" is one nobody can act
    // on, and it would be shown over every edit.
    expect(text).toContain('ROOT_PASSWORD');
  });

  it('says nothing of the sort about an ordinary setting', async () => {
    wrapper = mountSheet();
    await flushPromises();

    await wrapper.findComponent(components.VCombobox).vm.$emit('update:modelValue', 'debug');
    await flushPromises();
    button('Apply and rebuild').click();
    await flushPromises();

    // A log level is read on every start. A warning here would be the kind
    // that teaches people to click through the one that matters.
    expect(document.body.textContent).not.toContain('may not take effect');
  });

  /**
   * The manifest's `type` was arriving and being ignored. The form asked one
   * question — does this row carry options — so a `bool` was a text box
   * somebody was expected to type the word `true` into. No package in the
   * catalogue uses these types yet, which is why nothing had gone wrong and
   * why the first one to use them would have.
   */
  describe('the control follows the declared type', () => {
    it('gives a boolean a switch, and still sends a string', async () => {
      api.instanceSettings.mockResolvedValue([
        {
          key: 'TLS',
          kind: 'bool',
          value: 'false',
          secret: false,
          isDefault: true,
          required: false,
          options: [],
          label: {},
        },
      ]);

      wrapper = mountSheet();
      await flushPromises();

      const toggle = wrapper.findComponent(components.VSwitch);
      expect(toggle.props('modelValue')).toBe(false);

      await toggle.vm.$emit('update:modelValue', true);
      await flushPromises();
      button('Apply and rebuild').click();
      await flushPromises();
      button('Apply').click();
      await flushPromises();

      // A string on the wire either way: that is what a compose file
      // interpolates and what instances.json holds. Only the control changed.
      expect(api.instanceApplySettings).toHaveBeenCalledWith('mysql-8-0', { TLS: 'true' }, null);
    });

    it('gives an integer a numeric field', async () => {
      api.instanceSettings.mockResolvedValue([
        {
          key: 'MAX_CONNECTIONS',
          kind: 'int',
          value: '100',
          secret: false,
          isDefault: true,
          required: false,
          options: [],
          label: {},
        },
      ]);

      wrapper = mountSheet();
      await flushPromises();

      expect(wrapper.findComponent(components.VTextField).props('type')).toBe('number');
    });

    it('offers the instances that answer a reference, as a combobox', async () => {
      // Rust fills `options` for an instanceRef with the instances on this
      // machine that provide the capability, so the form needs to know nothing
      // about the type: a row that carries options already renders a combobox.
      api.instanceSettings.mockResolvedValue([
        {
          key: 'BACKEND',
          kind: 'instanceRef',
          value: 'mysql-8-0',
          secret: false,
          isDefault: false,
          required: false,
          options: ['mysql-8-0', 'mariadb-11-4'],
          label: {},
        },
      ]);

      wrapper = mountSheet();
      await flushPromises();

      const combobox = wrapper.findComponent(components.VCombobox);
      expect(combobox.props('items')).toEqual(['mysql-8-0', 'mariadb-11-4']);
      expect(combobox.props('modelValue')).toBe('mysql-8-0');
    });
  });

  it('offers the package default back once a field has moved off it', async () => {
    api.instanceSettings.mockResolvedValue([{ ...PLAIN_ROW, defaultValue: 'stackvo' }]);

    wrapper = mountSheet();
    await flushPromises();

    const reset = () =>
      [...document.body.querySelectorAll('button')].find((b) =>
        b.getAttribute('aria-label')?.startsWith('Put back the package default')
      );

    // Nothing to put back while the field is already on it — a control that is
    // always there and does nothing half the time is one nobody trusts.
    expect(reset()).toBeUndefined();

    const field = wrapper.findComponent(components.VTextField);
    await field.vm.$emit('update:modelValue', 'shop');
    await flushPromises();

    reset().click();
    await flushPromises();

    expect(field.props('modelValue')).toBe('stackvo');
    // Back where it started is not a change, so there is nothing to apply.
    expect(button('Apply and rebuild').disabled).toBe(true);
  });

  it('does not hand the form a secret’s default', async () => {
    // It would cross the boundary unasked and sit in a field this same sheet
    // takes care to mask. A password is put back by revealing and typing.
    api.instanceSettings.mockResolvedValue([SECRET_ROW]);

    wrapper = mountSheet();
    await flushPromises();

    const reset = [...document.body.querySelectorAll('button')].find((b) =>
      b.getAttribute('aria-label')?.startsWith('Put back the package default')
    );
    expect(reset).toBeUndefined();
  });

  /**
   * Until this existed there was no way to change an allocated port at all:
   * `instance_create` picks one, moves on when the preferred number is taken,
   * and nothing afterwards could move it — so a user whose 3306 had gone
   * elsewhere had to edit `instances.json` by hand. That is a regression on the
   * `.env` model, where the port was a line in a file the app already edited.
   */
  describe('host ports', () => {
    /** With ports, which the instance row already carries. */
    const withPorts = () =>
      mount(
        {
          components: { InstanceSettingsSheet },
          template: `
            <v-app>
              <InstanceSettingsSheet
                :instance="{ id: 'minio-1', ports: { main: 9000, console: 9001 } }"
                :model-value="true" />
            </v-app>`,
        },
        { attachTo: document.body, global: { plugins: [vuetify, i18n] } }
      );

    it('sends only the port that moved, as a number', async () => {
      wrapper = withPorts();
      await flushPromises();

      const field = wrapper
        .findAllComponents(components.VTextField)
        .find((f) => f.props('label') === 'console');
      await field.vm.$emit('update:modelValue', '9500');
      await flushPromises();

      button('Apply and rebuild').click();
      await flushPromises();
      button('Apply').click();
      await flushPromises();

      // A number, not the string the field holds: the command takes a u16 map
      // and `"9500"` is a type error at the boundary rather than a port. And
      // only the handle that moved — `main` was not touched.
      expect(api.instanceApplySettings).toHaveBeenCalledWith('minio-1', {}, { console: 9500 });
    });

    it('names the port in the confirmation, beside the settings', async () => {
      wrapper = withPorts();
      await flushPromises();

      const field = wrapper
        .findAllComponents(components.VTextField)
        .find((f) => f.props('label') === 'main');
      await field.vm.$emit('update:modelValue', '9500');
      await flushPromises();
      button('Apply and rebuild').click();
      await flushPromises();

      const changes = [...document.body.querySelectorAll('.v-dialog .change')];
      expect(changes).toHaveLength(1);
      expect(changes[0].textContent).toContain('port main');
      expect(changes[0].textContent).toContain('9000');
      expect(changes[0].textContent).toContain('9500');
    });

    it('refuses a number that is not a port at all', async () => {
      wrapper = withPorts();
      await flushPromises();

      const field = wrapper
        .findAllComponents(components.VTextField)
        .find((f) => f.props('label') === 'main');
      await field.vm.$emit('update:modelValue', '70000');
      await flushPromises();

      // Range only. Whether a number is *free* is a question about this
      // machine and about the instance table, and both answers live in Rust —
      // a second opinion here would go stale between the guess and the write.
      expect(field.props('error')).toBe(true);
      expect(button('Apply and rebuild').disabled).toBe(true);
    });
  });

  it('prefers the label the package itself gives a setting', async () => {
    // The manifest is the only source that knows what its own setting means, so
    // it wins over both the shared vocabulary and the humanised key.
    api.instanceSettings.mockResolvedValue([
      { ...PLAIN_ROW, label: { en: 'Schema to create on first boot' } },
    ]);

    wrapper = mountSheet();
    await flushPromises();

    expect(labels()).toContain('Schema to create on first boot');
    expect(labels()).not.toContain('Database');
  });
});
