import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import AgentsPane from '@/components/settings/AgentsPane.vue';

/**
 * The pane that edits another application's configuration file.
 *
 * The tests that matter here are the ones about what it refuses to do. Writing
 * the entry is settled in Rust, where `agents.rs` drives the merge with real
 * client files; what only exists in this component is the decision about *when
 * a button is offered* — and every one of those decisions protects something
 * that cannot be undone from the app: a registration pointing at a binary that
 * is not there, a write flag nobody asked for, an edit to a file with comments
 * in it.
 */

const api = vi.hoisted(() => ({
  agentsStatus: vi.fn(),
  agentsInstall: vi.fn(),
  agentsRemove: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api }));

const vuetify = createVuetify({ components, directives });

const mountPane = () => mount(AgentsPane, { global: { plugins: [vuetify, i18n] } });

const client = (over = {}) => ({
  id: 'cursor',
  label: 'Cursor',
  path: '/Users/x/.cursor/mcp.json',
  present: true,
  exists: true,
  parseable: true,
  command: null,
  current: false,
  ...over,
});

const STATUS = {
  binary: '/opt/stackvo/stackvo-mcp',
  source: 'build',
  root: '/Users/x/.stackvo',
  clients: [
    client(),
    client({ id: 'claude-code', label: 'Claude Code', path: '/Users/x/.claude.json' }),
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
  api.agentsStatus.mockResolvedValue(STATUS);
  api.agentsInstall.mockResolvedValue('/Users/x/.cursor/mcp.json');
  api.agentsRemove.mockResolvedValue('/Users/x/.cursor/mcp.json');
});

describe('what it refuses to offer', () => {
  it('withholds every register button when the server binary was not found', async () => {
    api.agentsStatus.mockResolvedValue({ ...STATUS, binary: null, source: null });
    const wrapper = mountPane();
    await flushPromises();

    // The instruction, not just a disabled control: "it will not work" without
    // "here is the command" is the shape of a bug report.
    expect(wrapper.text()).toContain('cargo build --release --bin stackvo-mcp');
    const buttons = wrapper.findAllComponents({ name: 'VBtn' });
    const register = buttons.filter((b) => b.text() === 'Register');
    expect(register).not.toHaveLength(0);
    expect(register.every((b) => b.props('disabled'))).toBe(true);
  });

  it('offers a copyable block instead of a button for a file it cannot parse', async () => {
    api.agentsStatus.mockResolvedValue({
      ...STATUS,
      clients: [client({ id: 'vscode', label: 'VS Code', parseable: false })],
    });
    const wrapper = mountPane();
    await flushPromises();

    const labels = wrapper.findAllComponents({ name: 'VBtn' }).map((b) => b.text());
    expect(labels).toContain('Copy block');
    expect(labels).not.toContain('Register');
    expect(wrapper.text()).toContain('cannot be edited safely');
  });

  it('names the file on every row, because that is where a refusal sends you', async () => {
    const wrapper = mountPane();
    await flushPromises();

    expect(wrapper.text()).toContain('/Users/x/.cursor/mcp.json');
    expect(wrapper.text()).toContain('/Users/x/.claude.json');
  });
});

describe('the write flag', () => {
  it('is off until it is switched on, and is what gets sent', async () => {
    const wrapper = mountPane();
    await flushPromises();

    await wrapper
      .findAllComponents({ name: 'VBtn' })
      .find((b) => b.text() === 'Register')
      .trigger('click');
    await flushPromises();
    expect(api.agentsInstall).toHaveBeenLastCalledWith('cursor', false);

    await wrapper.findComponent({ name: 'VSwitch' }).setValue(true);
    await wrapper
      .findAllComponents({ name: 'VBtn' })
      .find((b) => b.text() === 'Register')
      .trigger('click');
    await flushPromises();
    expect(api.agentsInstall).toHaveBeenLastCalledWith('cursor', true);
  });

  it('says what it grants, by name', async () => {
    const wrapper = mountPane();
    await flushPromises();

    // Not "allows writes" — the specific one people would not expect.
    expect(wrapper.text()).toContain('stack_down');
    expect(wrapper.text()).toContain('stopping the whole stack');
  });
});

describe('rows already registered', () => {
  it('offers Remove for a current registration and Update for a stale one', async () => {
    api.agentsStatus.mockResolvedValue({
      ...STATUS,
      clients: [
        client({ command: '/opt/stackvo/stackvo-mcp', current: true }),
        client({
          id: 'claude-code',
          label: 'Claude Code',
          path: '/Users/x/.claude.json',
          command: '/old/checkout/stackvo-mcp',
          current: false,
        }),
      ],
    });
    const wrapper = mountPane();
    await flushPromises();

    const labels = wrapper.findAllComponents({ name: 'VBtn' }).map((b) => b.text());
    expect(labels).toContain('Remove');
    expect(labels).toContain('Update');
    expect(wrapper.text()).toContain('pointing at another copy');
  });

  it('re-reads the status after a failure, and still shows the error', async () => {
    api.agentsInstall.mockRejectedValue({ code: 'IO_ERROR', message: 'no' });
    const wrapper = mountPane();
    await flushPromises();

    await wrapper
      .findAllComponents({ name: 'VBtn' })
      .find((b) => b.text() === 'Register')
      .trigger('click');
    await flushPromises();

    // Both halves: the list is not left describing a file nobody re-read, and
    // the message survives the re-read that clears it.
    expect(api.agentsStatus).toHaveBeenCalledTimes(2);
    expect(wrapper.findComponent({ name: 'ErrorAlert' }).exists()).toBe(true);
  });
});

describe('the block it tells you to paste', () => {
  it('is what the installer would have written', async () => {
    const wrapper = mountPane();
    await flushPromises();

    const parsed = JSON.parse(wrapper.vm.snippet(client()));
    expect(parsed.mcpServers.stackvo.command).toBe('/opt/stackvo/stackvo-mcp');
    expect(parsed.mcpServers.stackvo.env.STACKVO_ROOT).toBe('/Users/x/.stackvo');
    expect(parsed.mcpServers.stackvo.args).toBeUndefined();

    // VS Code names the map differently and needs the transport, exactly as
    // `agents.rs` writes it — the two shapes are stated in two languages and
    // this is the half that would drift silently.
    const vscode = JSON.parse(wrapper.vm.snippet(client({ id: 'vscode' })));
    expect(vscode.servers.stackvo.type).toBe('stdio');
    expect(vscode.mcpServers).toBeUndefined();
  });
});
