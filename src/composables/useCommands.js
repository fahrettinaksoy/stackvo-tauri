import { computed } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { useTheme } from 'vuetify';
import { useAppStore } from '@/stores/app';
import { useInventoryStore } from '@/stores/inventory';
import { useOperationsStore } from '@/stores/operations';
import { useAppearanceStore } from '@/stores/appearance';
import { setLocale } from '@/i18n';
import { api } from '@/lib/ipc';

/**
 * Everything this window can be asked to do, as one list.
 *
 * A-2. Until this existed there was exactly one way into any action: know which
 * page it lives on, get to that page, and find the control. That is fine for
 * the second week and hostile for the first hour, and it is the reason eight of
 * the ten tools this one is measured against ship a keyboard route of some kind.
 *
 * ## Built at open, not registered by components
 *
 * The obvious shape is a registry a page pushes its commands into on mount. It
 * is also wrong here: a command registered by a page outlives the mount that
 * can run it, so navigating away leaves an entry whose handler closes over a
 * component that is gone. Nothing in that failure is visible — the palette
 * lists it, the click does nothing.
 *
 * Reading the stores at open time inverts that. Every entry is derived from
 * state that is true right now, and a command that should not exist simply is
 * not built.
 *
 * ## Nothing is offered that would fail
 *
 * A stopped project has no Stop, an unbuilt one has no Restart, and a project
 * whose domain has no hosts entry has no "open site" — the same rules the rail
 * menu already applies, because a palette that lists an action and then errors
 * is worse than one that omits it. `disabled` is kept for the one case where
 * absence would read as a missing feature rather than a state: the stack-wide
 * actions when the engine is down.
 */
export function useCommands() {
  const router = useRouter();
  const { t, locale } = useI18n();
  const theme = useTheme();
  const app = useAppStore();
  const inventory = useInventoryStore();
  const ops = useOperationsStore();
  const appearance = useAppearanceStore();

  /** The same seven destinations the navigation rail draws, from one source. */
  const NAV = [
    { to: '/', icon: 'mdi-view-dashboard-outline', label: 'nav.dashboard' },
    { to: '/projects', icon: 'mdi-folder-multiple-outline', label: 'nav.projects' },
    { to: '/market', icon: 'mdi-storefront-outline', label: 'nav.market' },
    { to: '/logs', icon: 'mdi-text-box-multiple-outline', label: 'nav.logs' },
    { to: '/dumps', icon: 'mdi-bug-outline', label: 'nav.dumps' },
    { to: '/mail', icon: 'mdi-email-outline', label: 'nav.mail' },
    { to: '/settings', icon: 'mdi-cog-outline', label: 'nav.settings' },
  ];

  const LANGUAGES = [
    { value: 'tr', title: 'Türkçe' },
    { value: 'en', title: 'English' },
  ];

  const commands = computed(() => {
    const list = [];

    for (const item of NAV) {
      list.push({
        id: `go:${item.to}`,
        section: t('palette.sections.navigate'),
        label: t(item.label),
        icon: item.icon,
        run: () => router.push(item.to),
      });
    }

    for (const project of inventory.projects) {
      const busy = ops.isBusy(project.name);
      const where = project.domain || project.name;

      list.push({
        id: `project:open:${project.name}`,
        section: t('palette.sections.projects'),
        label: project.name,
        hint: project.domain || undefined,
        icon: project.runtime === 'node' ? 'mdi-nodejs' : 'mdi-language-php',
        run: () => router.push(`/projects/${project.name}`),
      });

      if (!project.built) {
        list.push({
          id: `project:build:${project.name}`,
          section: t('palette.sections.projects'),
          label: t('palette.project.build', { name: where }),
          icon: 'mdi-hammer-wrench',
          disabled: busy || !app.engineUp,
          run: () => api.projectBuild(project.name),
        });
      } else if (project.running) {
        list.push({
          id: `project:stop:${project.name}`,
          section: t('palette.sections.projects'),
          label: t('palette.project.stop', { name: where }),
          icon: 'mdi-stop',
          disabled: busy,
          run: () => api.projectStop(project.name),
        });
        list.push({
          id: `project:restart:${project.name}`,
          section: t('palette.sections.projects'),
          label: t('palette.project.restart', { name: where }),
          icon: 'mdi-restart',
          disabled: busy,
          run: () => api.projectRestart(project.name),
        });
      } else {
        list.push({
          id: `project:start:${project.name}`,
          section: t('palette.sections.projects'),
          label: t('palette.project.start', { name: where }),
          icon: 'mdi-play',
          disabled: busy,
          run: () => api.projectStart(project.name),
        });
      }

      // Only when the name actually resolves — see the rail menu's comment.
      if (project.domain && project.running && project.domainConfigured) {
        list.push({
          id: `project:site:${project.name}`,
          section: t('palette.sections.projects'),
          label: t('palette.project.site', { domain: project.domain }),
          icon: 'mdi-open-in-new',
          run: () => api.openInBrowser(`https://${project.domain}`),
        });
      }
    }

    // Disabled rather than absent: "start everything" not being in the list at
    // all would read as a missing feature, where a greyed row with the engine
    // reported down beside it reads as the state it is.
    const engineDown = !app.engineUp;
    list.push(
      {
        id: 'stack:start',
        section: t('palette.sections.stack'),
        label: t('quickActions.startAll'),
        icon: 'mdi-play-circle-outline',
        disabled: engineDown,
        run: () => api.containersStartAll(),
      },
      {
        id: 'stack:stop',
        section: t('palette.sections.stack'),
        label: t('quickActions.stopAll'),
        icon: 'mdi-stop-circle-outline',
        disabled: engineDown,
        run: () => api.containersStopAll(),
      },
      {
        id: 'stack:restart',
        section: t('palette.sections.stack'),
        label: t('quickActions.restart'),
        icon: 'mdi-restart',
        disabled: engineDown,
        run: () => api.containersRestartAll(),
      }
    );

    list.push({
      id: 'app:new-project',
      section: t('palette.sections.app'),
      label: t('newProject.title'),
      icon: 'mdi-plus',
      disabled: !app.hasWorkspace,
      run: () => {
        app.newProjectOpen = true;
      },
    });

    list.push({
      id: 'app:theme',
      section: t('palette.sections.app'),
      label: t('app.toggleTheme'),
      icon: theme.global.current.value.dark ? 'mdi-weather-sunny' : 'mdi-weather-night',
      run: () => appearance.toggleTheme(theme.global.current.value.dark),
    });

    for (const language of LANGUAGES) {
      if (language.value === locale.value) continue;
      list.push({
        id: `app:locale:${language.value}`,
        section: t('palette.sections.app'),
        label: `${t('app.language')}: ${language.title}`,
        icon: 'mdi-translate',
        run: () => setLocale(language.value),
      });
    }

    return list;
  });

  return { commands };
}

/**
 * The matcher: case-insensitive substring, ranked, and deliberately not fuzzy.
 *
 * A subsequence matcher — the kind that makes `sts` find `SeTtingS` — sounds
 * better and behaves worse on a list this shape: `sts` would also hit "Start
 * all containers" and "Stop all containers", and every one of the results a
 * user did not want would then be sorted into place by a score nobody can
 * predict. Substring is a rule that can be held in the head: what you typed
 * appears, in that order, somewhere in the row.
 *
 * Ranking is by *where* the hit lands, not by how many characters agreed. A
 * label that starts with the query is what the user meant; a label containing
 * it is second; a match that is only in the section name or the hint is last,
 * because the row's own words did not agree at all.
 */
export function matchCommands(commands, query) {
  const needle = query.trim().toLowerCase();
  if (!needle) return commands;

  const scored = [];
  for (const command of commands) {
    const label = command.label.toLowerCase();
    const at = label.indexOf(needle);

    let rank;
    if (at === 0) rank = 0;
    else if (at > 0) rank = 1;
    else if (`${command.section} ${command.hint ?? ''}`.toLowerCase().includes(needle)) rank = 2;
    else continue;

    scored.push({ command, rank, at: at < 0 ? Number.MAX_SAFE_INTEGER : at });
  }

  // Stable within a rank: `sort` is stable in every engine this runs on, so
  // equal rows keep the order `commands` built them in — which is the order the
  // rest of the app shows them in, and the only order a reader has a model of.
  scored.sort((a, b) => a.rank - b.rank || a.at - b.at);
  return scored.map((entry) => entry.command);
}
