import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { defineComponent, h } from 'vue';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import { useEnvEditor, provideEnvEditor } from '@/composables/useEnvEditor';
import ManagedBadge from '@/components/settings/ManagedBadge.vue';
import PolicyNotice from '@/components/settings/PolicyNotice.vue';

/**
 * The managed-machine half of the settings surface.
 *
 * Every assertion here is about a machine almost nobody runs — a fleet laptop
 * with an administrator's policy file on it. That is exactly why it is tested:
 * the developer writing the next Settings pane has no policy file, so a badge
 * that renders nothing and a locked field that saves anyway both look correct
 * on their machine.
 */

const api = vi.hoisted(() => ({
  envGet: vi.fn(),
  envDefaults: vi.fn(),
  policyStatus: vi.fn(),
  envSet: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api }));

const vuetify = createVuetify({ components, directives });

/** Mount a component under one editor, the way `Settings.vue` provides it. */
function withEditor(component, editor, props = {}) {
  return mount(
    defineComponent({
      setup() {
        provideEnvEditor(editor);
        return () => h(component, props);
      },
    }),
    { global: { plugins: [vuetify, i18n] } }
  );
}

/** `ManagedBadge` takes a key; `PolicyNotice` takes nothing. */
const badgeFor = (editor, envKey = 'DEFAULT_TLD_SUFFIX') =>
  withEditor(ManagedBadge, editor, { envKey });

const MANAGED = {
  active: true,
  source: '/etc/stackvo/policy.json',
  managed: ['DEFAULT_TLD_SUFFIX', 'SERVER_TYPE'],
  locked: ['DEFAULT_TLD_SUFFIX'],
  registryPrefix: 'registry.corp.example/proxy',
  error: null,
};

beforeEach(() => {
  vi.clearAllMocks();
  // Vuetify teleports tooltip content to an overlay outside the wrapper, so
  // one test's overlay would otherwise still be in `document.body` when the
  // next one looks — and an assertion satisfied by a previous test is an
  // assertion satisfied by nothing.
  document.body.innerHTML = '';
  api.envGet.mockResolvedValue({});
  api.envDefaults.mockResolvedValue({});
  api.policyStatus.mockResolvedValue({
    active: false,
    source: null,
    managed: [],
    locked: [],
    error: null,
  });
});

describe('the editor and an administrator', () => {
  it('starts unmanaged, which is what nearly every machine is', async () => {
    const editor = useEnvEditor();
    await editor.loadDefaults();

    expect(editor.policy.value.active).toBe(false);
    expect(editor.isManaged('DEFAULT_TLD_SUFFIX')).toBe(false);
    expect(editor.isLocked('DEFAULT_TLD_SUFFIX')).toBe(false);
  });

  it('separates "the policy sets it" from "and you may not change it"', async () => {
    api.policyStatus.mockResolvedValue(MANAGED);
    const editor = useEnvEditor();
    await editor.loadDefaults();

    expect(editor.isManaged('SERVER_TYPE')).toBe(true);
    expect(editor.isLocked('SERVER_TYPE')).toBe(false);
    expect(editor.isLocked('DEFAULT_TLD_SUFFIX')).toBe(true);
  });

  /**
   * A `policy_status` that throws must not take the pane with it.
   *
   * The command reads a file that is usually absent; on a build where it went
   * wrong, the settings page failing to open would be a far worse outcome than
   * a missing badge.
   */
  it('survives a policy_status that fails', async () => {
    api.policyStatus.mockRejectedValue(new Error('nope'));
    const editor = useEnvEditor();
    await editor.loadDefaults();

    expect(editor.policy.value.active).toBe(false);
    expect(editor.isLocked('DEFAULT_TLD_SUFFIX')).toBe(false);
  });
});

describe('the managed badge', () => {
  it('renders nothing on an unmanaged machine', async () => {
    const editor = useEnvEditor();
    await editor.loadDefaults();

    expect(badgeFor(editor).text()).toBe('');
  });

  it('names the policy file, because that is the only thing to act on', async () => {
    api.policyStatus.mockResolvedValue(MANAGED);
    const editor = useEnvEditor();
    await editor.loadDefaults();

    const wrapper = badgeFor(editor);
    expect(wrapper.text()).toContain('Locked');
    // The tooltip body renders into an overlay attached to the document, not
    // into the wrapper — so the path is asserted where it actually lands.
    expect(document.body.textContent).toContain('/etc/stackvo/policy.json');
  });
});

describe('the policy notice', () => {
  it('says nothing at all when nobody is managing this machine', async () => {
    const editor = useEnvEditor();
    await editor.loadDefaults();

    const wrapper = withEditor(PolicyNotice, editor);
    expect(wrapper.text()).toBe('');
  });

  /**
   * The sentence this whole feature would be dishonest without.
   */
  it('states that it is not a security boundary', async () => {
    api.policyStatus.mockResolvedValue(MANAGED);
    const editor = useEnvEditor();
    await editor.loadDefaults();

    const wrapper = withEditor(PolicyNotice, editor);
    expect(wrapper.text()).toContain('not a security boundary');
    expect(wrapper.text()).toContain('STACKVO_POLICY_FILE');
  });

  /**
   * The failure case, which matters more than the working one.
   *
   * A policy that could not be parsed applies nothing and the app behaves as
   * though unmanaged. The administrator who pushed it has no way to find that
   * out, and this machine is the only place it is visible.
   */
  it('shows a policy that failed to parse, rather than swallowing it', async () => {
    api.policyStatus.mockResolvedValue({
      ...MANAGED,
      managed: [],
      locked: [],
      registryPrefix: null,
      error: 'is not valid JSON: expected `,` at line 4 column 3',
    });
    const editor = useEnvEditor();
    await editor.loadDefaults();

    const wrapper = withEditor(PolicyNotice, editor);
    expect(wrapper.text()).toContain('did not fully apply');
    expect(wrapper.text()).toContain('expected `,` at line 4 column 3');
  });

  it('names the registry every image is pulled through', async () => {
    api.policyStatus.mockResolvedValue(MANAGED);
    const editor = useEnvEditor();
    await editor.loadDefaults();

    expect(withEditor(PolicyNotice, editor).text()).toContain('registry.corp.example/proxy');
  });
});
