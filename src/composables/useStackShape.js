import { computed, ref } from 'vue';
import { api } from '@/lib/ipc';
import { HTTPS_ONLY_SUFFIXES } from '@/lib/manifest';

/**
 * How the stack is addressed: the domain suffix, the network, and the two
 * things that have to agree with them — `/etc/hosts` and the proxy.
 *
 * Lifted out of `Settings.vue` with the Domain pane. The validation here is not
 * decoration: `DEFAULT_TLD_SUFFIX` is interpolated into every generated routing
 * label and into what the certificate has to cover, and none of those places
 * check it again. A suffix with a space in it produces a compose file that
 * parses and a router nothing matches.
 */

/** The TLDs offered in the picker. Anything else is still accepted. */
export const TLD_CHOICES = ['loc', 'test', 'localhost', 'dev'];

/**
 * One label of a hostname: alphanumerics, dots and hyphens, not starting or
 * ending with one.
 */
const PART = /^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$/;

/** Docker network names are laxer — upper case and underscores are legal. */
const NETWORK = /^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/;

/**
 * Split the suffix where people actually think about it.
 *
 * `stackvo.loc` is a namespace and a TLD, and only the second half is what
 * someone means by "can I use .dev instead". Split on the **last** dot:
 * everything before it is the label, which may itself contain dots.
 *
 * Exported because it is the piece worth testing on its own — the two-field
 * form is built entirely out of it and rejoins its halves on every keystroke.
 */
export function splitSuffix(value) {
  const text = String(value ?? '').trim();
  const at = text.lastIndexOf('.');
  return at === -1
    ? { label: '', tld: text }
    : { label: text.slice(0, at), tld: text.slice(at + 1) };
}

/** The inverse: drop an empty half rather than leaving a leading or trailing dot. */
export function joinSuffix(label, tld) {
  return [String(label ?? '').trim(), String(tld ?? '').trim()].filter(Boolean).join('.');
}

export function useStackShape(envEditor, t) {
  const { effective, edit, boolOf } = envEditor;

  const suffix = computed(() => effective('DEFAULT_TLD_SUFFIX'));
  const suffixLabel = computed(() => splitSuffix(suffix.value).label);
  const suffixTld = computed(() => splitSuffix(suffix.value).tld);

  const setSuffix = (label, tld) => edit('DEFAULT_TLD_SUFFIX', joinSuffix(label, tld));

  // Vuetify's rule shape: `true` when valid, a message when not.
  const suffixRules = [
    (v) => !!String(v ?? '').trim() || t('settings.shape.suffixRequired'),
    (v) => PART.test(String(v ?? '').trim()) || t('settings.shape.suffixInvalid'),
  ];
  const suffixLabelRules = [
    // The label half may legitimately be empty — `loc` on its own is a suffix.
    (v) =>
      !String(v ?? '').trim() || PART.test(String(v).trim()) || t('settings.shape.suffixInvalid'),
  ];
  const suffixTldRules = [
    (v) => !!String(v ?? '').trim() || t('settings.shape.suffixRequired'),
    (v) => PART.test(String(v ?? '').trim()) || t('settings.shape.suffixInvalid'),
  ];
  const networkRules = [
    (v) => !!String(v ?? '').trim() || t('settings.shape.networkRequired'),
    (v) => NETWORK.test(String(v ?? '').trim()) || t('settings.shape.networkInvalid'),
  ];

  /**
   * Choosing an HSTS-preloaded TLD for the whole stack with HTTPS off breaks
   * every address at once, not just one project's — the browser refuses plain
   * HTTP to `.dev` before any request is made.
   */
  const suffixNeedsHttps = computed(
    () => HTTPS_ONLY_SUFFIXES.includes(suffixTld.value.toLowerCase()) && !boolOf('SSL_ENABLE')
  );

  /** Both keys, checked with the rules the fields use — one answer for the save button. */
  const valid = computed(
    () =>
      suffixRules.every((r) => r(suffix.value) === true) &&
      networkRules.every((r) => r(effective('DOCKER_DEFAULT_NETWORK')) === true)
  );

  return {
    suffix,
    suffixLabel,
    suffixTld,
    setSuffix,
    suffixRules,
    suffixLabelRules,
    suffixTldRules,
    networkRules,
    suffixNeedsHttps,
    valid,
  };
}

/**
 * The hosts file, as one list rather than one broken domain at a time.
 *
 * Every domain here reaches the browser by name through the proxy, so every one
 * of them needs a line in `/etc/hosts` — and the app only ever offered to add
 * them from whichever page happened to notice one missing. A deleted project's
 * line had no route at all: it points at 127.0.0.1 forever and nothing was
 * looking for it.
 */
export function useHostsOverview() {
  const hosts = ref(null);
  const fixing = ref(false);
  const error = ref(null);

  const missing = computed(() => (hosts.value?.entries ?? []).filter((e) => !e.configured));
  const stale = computed(() => hosts.value?.stale ?? []);
  const needsWork = computed(() => missing.value.length > 0 || stale.value.length > 0);

  async function load() {
    hosts.value = await api.hostsOverview().catch(() => null);
  }

  /**
   * Both directions in one elevation prompt: asking twice for one tidy-up is
   * how people stop half way.
   */
  async function fix() {
    fixing.value = true;
    error.value = null;
    try {
      await api.hostsApply(
        missing.value.map((e) => e.domain),
        stale.value
      );
      await load();
    } catch (e) {
      error.value = e;
    } finally {
      fixing.value = false;
    }
  }

  return { hosts, fixing, error, missing, stale, needsWork, load, fix };
}

/**
 * The proxy, which the app never named.
 *
 * Traefik is not in the service catalog and should not be — it is not a thing
 * you switch on, it is how every project and admin UI is reached at all. But
 * that left the one container the whole stack depends on with no presence in
 * the app: no version, no state, and no route to its own dashboard, which the
 * generator has been writing a router for the entire time.
 */
export function useProxy(tld) {
  const proxy = ref(null);

  const dashboard = computed(() => (tld.value ? `https://traefik.${tld.value}/dashboard/` : null));

  const ports = computed(() =>
    (proxy.value?.ports ?? []).map((p) => p.host ?? p.container).join(', ')
  );

  async function load() {
    // Its own container name, not a catalog id: `container_inspect` prefixes
    // `stackvo-` itself, and Traefik has no catalog entry to look up.
    proxy.value = await api.containerInspect('traefik').catch(() => null);
  }

  return { proxy, dashboard, ports, load };
}
