import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import RequirementsPane from '@/components/project/RequirementsPane.vue';

/**
 * The pane that turns a committed declaration into a running stack.
 *
 * Everything asserted here is about keeping two things apart that the backend
 * deliberately returns separately: what the repository *declares*, which a
 * colleague agreed to, and what this app *guessed* from the project's `.env`.
 * A pane that quietly enabled a guess, or wrote one into `stackvo.json` on the
 * same click as the rest, would undo the whole reason the two lists exist.
 */

const api = vi.hoisted(() => ({
  projectRequirements: vi.fn(),
  projectRequirementsApply: vi.fn(),
  projectRequirementsDeclare: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api }));

const vuetify = createVuetify({ components, directives });

const mountPane = () =>
  mount(RequirementsPane, {
    props: { name: 'shop' },
    global: { plugins: [vuetify, i18n] },
  });

const STATE = {
  declared: [
    { id: 'mysql', known: true, enabled: true },
    { id: 'redis', known: true, enabled: false },
    { id: 'postgress', known: false, enabled: false },
  ],
  suggested: [{ service: 'meilisearch', key: 'SCOUT_DRIVER' }],
  plan: { changes: [{ key: 'SERVICE_REDIS_ENABLE', to: 'true' }], needsRegenerate: true },
};

beforeEach(() => {
  vi.clearAllMocks();
  api.projectRequirements.mockResolvedValue(STATE);
  api.projectRequirementsApply.mockResolvedValue(STATE.plan);
  api.projectRequirementsDeclare.mockResolvedValue({});
});

describe('the two lists', () => {
  it('says which came from the repository and which is a guess', async () => {
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('Declared in stackvo.json');
    expect(wrapper.text()).toContain('Suggested by this project’s own .env');
    // And names the key the guess came from, so it can be checked.
    expect(wrapper.text()).toContain('from SCOUT_DRIVER');
  });

  it('warns before writing a guess into a file colleagues read', async () => {
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('read as a decision');
  });
});

describe('enabling', () => {
  it('offers only the services that are declared and not on', async () => {
    const wrapper = mountPane();
    await flushPromises();

    // One of the three: mysql is already on, postgress has no template.
    expect(wrapper.text()).toContain('Enable 1 service(s)');
  });

  it('starts what was declared, and never the suggestion', async () => {
    const wrapper = mountPane();
    await flushPromises();

    await wrapper
      .findAllComponents({ name: 'VBtn' })
      .find((b) => b.text().startsWith('Enable'))
      .trigger('click');
    await flushPromises();

    expect(api.projectRequirementsApply).toHaveBeenCalledWith('shop');
    // The profiles handed up are the declared ones that exist — not the
    // unknown id, which would be a compose profile matching nothing, and not
    // meilisearch, which nobody has agreed to.
    expect(wrapper.emitted('apply')[0][0]).toEqual(['mysql', 'redis']);
  });
});

describe('writing the declaration', () => {
  it('keeps what was already declared and adds the ticked suggestions', async () => {
    const wrapper = mountPane();
    await flushPromises();

    await wrapper
      .findAllComponents({ name: 'VBtn' })
      .find((b) => b.text().includes('stackvo.json'))
      .trigger('click');
    await flushPromises();

    expect(api.projectRequirementsDeclare).toHaveBeenCalledWith('shop', [
      'mysql',
      'redis',
      'postgress',
      'meilisearch',
    ]);
  });

  it('does not offer the button when nothing is ticked', async () => {
    api.projectRequirements.mockResolvedValue({ ...STATE, suggested: [] });
    const wrapper = mountPane();
    await flushPromises();

    const write = wrapper
      .findAllComponents({ name: 'VBtn' })
      .find((b) => b.text().includes('stackvo.json'));
    expect(write).toBeUndefined();
  });
});

describe('a name with no template', () => {
  it('is shown as unacted-on rather than dropped', async () => {
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('postgress');
    expect(wrapper.text()).toContain('No template for this service');
    expect(wrapper.text()).toContain('left in the file rather than removed');
  });
});

describe('a project that declares nothing', () => {
  it('says so instead of showing an empty list', async () => {
    api.projectRequirements.mockResolvedValue({ declared: [], suggested: [], plan: null });
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('declares no services');
  });
});
