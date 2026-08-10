import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import SecretsPane from '@/components/settings/SecretsPane.vue';

/**
 * The pane that changes where a password lives.
 *
 * Two of these are about what it *says* rather than what it does, and they are
 * the ones worth having: a keystore feature is read as "the secret left the
 * disk", and this one leaves it in `generated/docker-compose.dynamic.yml`. A
 * pane that quietly stopped saying so would be the difference between a partial
 * win and a false claim, and nothing else in the build would notice.
 */

const api = vi.hoisted(() => ({
  secretsStatus: vi.fn(),
  secretMove: vi.fn(),
  secretRestore: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api }));

const vuetify = createVuetify({ components, directives });

const mountPane = () => mount(SecretsPane, { global: { plugins: [vuetify, i18n] } });

const STATUS = {
  available: true,
  keys: [
    { key: 'SERVICE_MYSQL_ROOT_PASSWORD', moved: false, resolvable: true, set: true },
    { key: 'SERVICE_GRAFANA_ADMIN_PASSWORD', moved: true, resolvable: true, set: true },
    { key: 'SERVICE_POSTGRES_PASSWORD', moved: false, resolvable: false, set: false },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
  api.secretsStatus.mockResolvedValue(STATUS);
  api.secretMove.mockResolvedValue(undefined);
  api.secretRestore.mockResolvedValue(undefined);
});

describe('what the pane says before it offers a button', () => {
  it('names the file the password is still written into', async () => {
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('generated/docker-compose.dynamic.yml');
    expect(wrapper.text()).toContain('does not take it off the disk');
  });

  it('warns that the command-line tool cannot read a moved credential', async () => {
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('stackvo.sh');
  });
});

describe('the rows', () => {
  it('shows a key with no value nowhere, because there is nothing to move', async () => {
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('SERVICE_MYSQL_ROOT_PASSWORD');
    expect(wrapper.text()).not.toContain('SERVICE_POSTGRES_PASSWORD');
  });

  it('offers Move for a key in the file and Restore for one in the keystore', async () => {
    const wrapper = mountPane();
    await flushPromises();

    const labels = wrapper.findAll('.v-list-item button').map((b) => b.text());
    expect(labels).toEqual(['Move', 'Restore']);
  });

  it('moves the key it was asked about and re-reads afterwards', async () => {
    const wrapper = mountPane();
    await flushPromises();

    await wrapper.findAll('.v-list-item button')[0].trigger('click');
    await flushPromises();

    expect(api.secretMove).toHaveBeenCalledWith('SERVICE_MYSQL_ROOT_PASSWORD');
    expect(api.secretRestore).not.toHaveBeenCalled();
    expect(api.secretsStatus).toHaveBeenCalledTimes(2);
  });

  /**
   * A failed move may still have written the keystore entry, so the row's old
   * state is not something to keep showing — it would be a claim about the disk
   * that nobody checked.
   */
  it('re-reads even when the move failed', async () => {
    api.secretMove.mockRejectedValue(new Error('keychain is locked'));
    const wrapper = mountPane();
    await flushPromises();

    await wrapper.findAll('.v-list-item button')[0].trigger('click');
    await flushPromises();

    expect(api.secretsStatus).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain('keychain is locked');
  });
});

describe('a machine with no keystore', () => {
  it('says so and disables every button rather than offering one that cannot work', async () => {
    api.secretsStatus.mockResolvedValue({ ...STATUS, available: false });
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('no keystore');
    const buttons = wrapper.findAll('.v-list-item button');
    expect(buttons.length).toBeGreaterThan(0);
    expect(buttons.every((b) => b.attributes('disabled') !== undefined)).toBe(true);
  });
});

describe('a reference the keystore will not answer', () => {
  /** Generation is blocked while this is true, so the row has to be loud. */
  it('is reported by name', async () => {
    api.secretsStatus.mockResolvedValue({
      available: true,
      keys: [{ key: 'SERVICE_MYSQL_ROOT_PASSWORD', moved: true, resolvable: false, set: true }],
    });
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('did not answer');
    expect(wrapper.text()).toContain('SERVICE_MYSQL_ROOT_PASSWORD');
  });
});
