import { onMounted, onUnmounted } from 'vue';
import { listenAll } from '@/lib/events';
import { api } from '@/lib/ipc';

/**
 * Offer the hosts entry as soon as a build has produced something to visit.
 *
 * `project_build` does not touch `/etc/hosts` and should not: that write is the
 * one place in the app that asks for an administrator password, and the rule
 * everywhere else is that the diff is shown and accepted *before* the prompt
 * appears. A build that raised an auth dialog somewhere in the middle of
 * `docker compose build` would break that order, so the write stayed a separate
 * act the user had to go and find — from a row menu, the detail page, the
 * doctor, or the Domains pane.
 *
 * The finding was the cost of that: a project can build, come up, and answer on
 * a name the machine does not resolve, with nothing on screen at the moment it
 * finishes to say so. What this restores is the *asking*, not the writing — the
 * same review dialog, opened at the one moment the answer is relevant, still
 * refusing to write a byte until somebody has read the diff and pressed apply.
 *
 * ## Three things it will not interrupt for
 *
 * `build:success` is the name of the finished event, not a claim: `run_operation`
 * emits it with `success: false` when the run failed, so the flag is what is
 * read rather than the event's name.
 *
 * The DNS responder (E-1) answers for the whole suffix, which is what makes a
 * per-project hosts line unnecessary in the first place — and wildcards, which
 * the hosts file cannot express at all. Where it is listening *and* the machine
 * is actually asking it, there is nothing to fix and nothing to offer. A
 * responder that is configured but down is the broken state, and that one is
 * worth offering the hosts line for.
 *
 * The project is re-read here rather than taken from whatever list the page is
 * holding. The row's `domainConfigured` is refreshed by the same event, so
 * reading it would be a race against a refetch — and the answer decides whether
 * a modal appears over somebody's work.
 *
 * @param {(domain: string, project: string) => void} open — show the review
 *   dialog for this domain. Called only when a hosts line is genuinely missing.
 */
export function useHostsPrompt(open) {
  let teardown = null;
  let stopped = false;

  async function consider(payload) {
    if (!payload || payload.success === false) return;

    const name = payload.subject ?? payload.project;
    if (!name) return;

    const dns = await api.dnsStatus().catch(() => null);
    if (dns?.listening && dns?.configured) return;

    const project = await api.projectGet(name).catch(() => null);
    if (!project?.domain || project.domainConfigured) return;

    if (!stopped) open(project.domain, name);
  }

  onMounted(async () => {
    const off = await listenAll(['build:success'], (_event, payload) => {
      // Deliberately not awaited: the listener is a notification, and holding
      // it open across two IPC round trips would queue the next build's event
      // behind this one's lookups.
      consider(payload);
    });

    // Unmounting while the subscription was still resolving used to leave it
    // attached with nothing left to call it back.
    if (stopped) off();
    else teardown = off;
  });

  onUnmounted(() => {
    stopped = true;
    teardown?.();
    teardown = null;
  });
}
