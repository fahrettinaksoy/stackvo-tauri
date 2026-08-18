<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';
import { useInventoryStore } from '@/stores/inventory';
import { useOperationsStore } from '@/stores/operations';
import { useFavourites } from '@/composables/useFavourites';
import { useHostsPrompt } from '@/composables/useHostsPrompt';
import { useAppStore } from '@/stores/app';
import { parentDomain, runtimeLook } from '@/lib/manifest';
import { bytes } from '@/lib/format';
import { listenAll, REFRESH_TRIGGERS } from '@/lib/events';
import { api, asList } from '@/lib/ipc';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import HostsDialog from '@/components/HostsDialog.vue';

const { t } = useI18n();
const router = useRouter();
const inventory = useInventoryStore();
const ops = useOperationsStore();
const app = useAppStore();

const search = ref('');
const actionError = ref(null);

const hostsFixFor = ref(null);

// A build that finishes on a name this machine cannot resolve asks about it
// here, rather than leaving the warning icon on the row to be noticed later.
// The same review dialog the icon opens — nothing is written unelevated and
// nothing is written unread.
useHostsPrompt((domain) => (hostsFixFor.value = domain));

const deleteTarget = ref(null);
const deleteFiles = ref(false);

/*
 * There is no `staleManifests` set any more, and that is the point.
 *
 * The badge used to be an accumulator: every `manifest:changed` the watcher
 * emitted added a name, and only a successful regenerate took one out. The
 * watcher cannot tell whose write it saw, so creating a project — the app
 * writing `stackvo.json`, then regenerating from it — added the name and
 * nothing removed it. The badge appeared on every new project and stayed.
 *
 * It reads `item.generatedStale` now, which the backend measures from the
 * manifest's timestamp against the generated output's. A watcher event only
 * triggers a reload; the answer comes from the files.
 */

/**
 * Which of these projects are worktrees, by name (N).
 *
 * Read once for the page rather than joined into `projects_list`: a worktree is
 * a project and the list has no business carrying a second identity for it, but
 * a row that does not say `branch of shop` leaves two unrelated-looking entries
 * where there is one application on two branches.
 *
 * Failure is silence. This is a label; a workspace whose worktree file cannot
 * be read should still show its projects.
 */
const worktrees = ref(new Map());
async function loadWorktrees() {
  try {
    worktrees.value = new Map(asList(await api.worktreeList()).map((w) => [w.name, w]));
  } catch {
    worktrees.value = new Map();
  }
}

/**
 * Rows grouped by parent domain, but only where a parent means something.
 *
 * The rule itself lives in `manifest.js` so it can be tested on its own; what
 * is left here is the counting. A parent with a single project keeps its own
 * domain as the key, so Vuetify makes a group of one and the header slot skips
 * it — the row then reads exactly as it did before grouping existed.
 */
const rows = computed(() => {
  const counts = new Map();
  for (const p of inventory.projects) {
    const parent = parentDomain(p.domain, app.tld);
    if (parent) counts.set(parent, (counts.get(parent) ?? 0) + 1);
  }
  return inventory.projects.map((p) => {
    const parent = parentDomain(p.domain, app.tld);
    return {
      ...p,
      worktree: worktrees.value.get(p.name) ?? null,
      // `null` when it stands alone, which is the table's own escape hatch: a
      // group with a null value has its header skipped and its rows always
      // flattened. Giving each one its own key instead made a group of one —
      // and groups start closed, so the header was suppressed, nothing was
      // left to open it, and five projects vanished from the page.
      parentDomain: parent && counts.get(parent) > 1 ? parent : null,
      // A field rather than a lookup in the cell: the table sorts on it, and a
      // sort key that is not on the row is a column that does not sort.
      favourite: favourites.isFavourite(p.name),
      // Flattened for the same reason. `git` is an object or null, and sorting
      // twenty rows by an object sorts them by nothing — the three states have
      // to be one comparable value for the column header to do anything.
      repo: p.git ? (p.git.remote ? 2 : 1) : 0,
    };
  });
});

/**
 * Groups start expanded.
 *
 * The table keeps that state internally — `opened` is a ref inside its own
 * composable, with no prop to seed it and nothing exposed on the component to
 * reach it. The only handle is `toggleGroup`, handed to this slot, so opening
 * has to be asked for from here.
 *
 * Deferred and remembered, for two reasons. Toggling during render mutates
 * state the same render reads, which Vue rightly complains about; and a group
 * the user collapsed must stay collapsed, so this fires once per group and
 * never again — `seen` is a plain Set rather than a ref because nothing should
 * re-render when it changes.
 */
const seen = new Set();
function openByDefault(item, isGroupOpen, toggleGroup) {
  const open = isGroupOpen(item);
  if (!open && !seen.has(item.id)) {
    seen.add(item.id);
    nextTick(() => toggleGroup(item));
  }
  // Returned so the binding that calls this reflects real state rather than
  // being an attribute that is always empty — an attribute that says nothing
  // is a worse home for a side effect than one that says something.
  return open;
}

const groupBy = [{ key: 'parentDomain', order: 'asc' }];

/**
 * The two narrowings the search box cannot express.
 *
 * Search matches text, and the two questions actually asked of this page are
 * not about text: "what is running right now" and "the handful I work on".
 * Typing a name answers neither, and sorting only moves them to the top of a
 * list you still have to read past.
 *
 * Filters rather than modes: both are visible while they are on and both clear
 * from the empty state, so neither can become a thing somebody is stuck in
 * wondering where their projects went. That is the same rule the inventory
 * store follows in refusing to hide broken projects.
 */
const statusFilter = ref('all');
const favouritesOnly = ref(false);

const STATUS_FILTERS = computed(() => [
  { value: 'all', label: t('projectsView.filter.all') },
  { value: 'running', label: t('projectsView.filter.running') },
  { value: 'stopped', label: t('projectsView.filter.stopped') },
  { value: 'unbuilt', label: t('projectsView.filter.unbuilt') },
]);

/**
 * Stopped means built and not running.
 *
 * A project with no image is not stopped, it has never started — and putting
 * the two together would make "stopped" the answer for a fresh checkout, which
 * is the one case where the next step is different.
 */
const visibleRows = computed(() =>
  rows.value.filter((row) => {
    if (favouritesOnly.value && !row.favourite) return false;
    if (statusFilter.value === 'running') return row.running;
    if (statusFilter.value === 'stopped') return row.built && !row.running;
    if (statusFilter.value === 'unbuilt') return !row.built;
    return true;
  })
);

/**
 * How many narrowings are on, for the badge on the funnel.
 *
 * The search term is not one of them: it is typed into a box that shows it
 * back, so it never needs a second indicator — while the two behind the menu
 * are invisible the moment the menu closes, which is what the count is for.
 */
const activeFilters = computed(
  () => (statusFilter.value !== 'all' ? 1 : 0) + (favouritesOnly.value ? 1 : 0)
);

/** Is anything narrowing the list — so an empty table is a filter, not a void? */
const narrowed = computed(() => !!search.value || activeFilters.value > 0);

function clearFilters() {
  search.value = '';
  statusFilter.value = 'all';
  favouritesOnly.value = false;
}

/**
 * Ordered by domain, inside a group and out.
 *
 * The table sorts by the group key before anything else, so without this the
 * rows arrived in whatever order the inventory returned them — which is the
 * order Docker happened to answer in, and changes between refreshes.
 */
/**
 * Starred projects first, then by domain (M-1).
 *
 * A sort rather than a filter: nothing is hidden, so this cannot become a mode
 * somebody gets stuck in — the same reason the inventory store refuses to hide
 * broken projects.
 */
const favourites = useFavourites();
const sortBy = [
  { key: 'favourite', order: 'desc' },
  { key: 'domain', order: 'asc' },
];

const headers = computed(() => [
  // Named even though the column shows a star and no text. An empty `<th>` is
  // a column a screen reader announces as nothing while reading every cell
  // under it — axe's `empty-table-header`. `.visually-hidden` keeps the header
  // row looking exactly as it did.
  {
    title: t('projectsView.colFavourite'),
    key: 'favourite',
    sortable: false,
    align: 'center',
    width: 44,
  },
  { title: t('projectsView.colDomain'), key: 'domain', sortable: true, align: 'start' },
  // The server used to be a column of its own. It is meaningful for exactly one
  // runtime out of eight — `manifest::read` warns that `server` is ignored on
  // anything but PHP — so seven rows in eight paid a column's width for an
  // em dash. It rides along in this cell instead, where a PHP project reads
  // "PHP 8.3 · nginx" and nothing else has to explain a blank.
  { title: t('projectsView.colRuntime'), key: 'runtime', sortable: true, align: 'start' },
  // Where the code came from. Sortable on purpose: "which of these did I clone
  // and which did I start here" is the question it answers, and answering it
  // by eye down a column of twenty is what sorting is for.
  {
    title: t('projectsView.colRepo'),
    key: 'repo',
    sortable: true,
    align: 'center',
    width: 72,
  },
  {
    title: t('projectsView.colConfiguration'),
    key: 'configuration',
    sortable: false,
    align: 'center',
    width: 120,
  },
  {
    title: t('projectsView.colStopStart'),
    key: 'control',
    sortable: false,
    align: 'center',
    width: 100,
  },
  {
    title: t('projectsView.colRestart'),
    key: 'restart',
    sortable: false,
    align: 'center',
    width: 100,
  },
  {
    title: t('projectsView.colTerminal'),
    key: 'terminal',
    sortable: false,
    align: 'center',
    width: 100,
  },
  { title: t('projectsView.colOpen'), key: 'open', sortable: false, align: 'center', width: 100 },
  {
    title: t('projectsView.colDetail'),
    key: 'detail',
    sortable: false,
    align: 'center',
    width: 100,
  },
  // Everything the row can do, said in words.
  //
  // Not a replacement for the columns before it — they are one click and this
  // is two, and the fast path is worth keeping for the acts done constantly.
  // What it adds is a name for each of them: a row of glyphs is a row of things
  // you learn by pressing them.
  //
  // Two acts are *only* here, and both were columns until they were not:
  //
  // - **Delete.** A destructive act one press away, in a table where the row
  //   above and the row below look the same, is a row somebody removes while
  //   aiming at the one under it. Behind a menu it costs a deliberate second
  //   press, which is the whole of what it needed.
  // - **Rebuild.** It shared a hammer with Build in the column beside it and
  //   meant something else — regenerate, build the image, recreate the
  //   container, against Build's first image. Two identical glyphs meaning two
  //   things is the case a name fixes and an icon cannot.
  //
  // It is also where the acts that never had a column live — applying a
  // changed manifest, fixing a hosts entry, opening the folder or the editor —
  // which until now were either a small icon beside the domain you had to know
  // was clickable, or only on the detail page.
  {
    title: t('projectsView.colMore'),
    key: 'more',
    sortable: false,
    align: 'center',
    width: 56,
  },
]);

/**
 * The terminal the user chose in Settings, on this container.
 *
 * There used to be a second, in-app terminal in a dialog. Two terminals with
 * different behaviour for the same button is a coin toss for the reader, and
 * the external one is the one with scrollback, tabs and a profile.
 */
async function openTerminal(project) {
  actionError.value = null;
  try {
    await api.terminalOpenExternal({ kind: 'container', name: project.containerName });
  } catch (e) {
    actionError.value = e;
  }
}

/**
 * Run a row action with the row marked busy.
 *
 * On success the flag is left for the event stream to clear, which is what
 * keeps the row honest when something else acts on the project — another
 * window, the watcher, a container that stopped on its own.
 *
 * That only works if `fn` runs a command whose events carry **the project's
 * own name** as their subject. Start, stop, restart, build and delete all do.
 * A command that reports under something else has to clear the flag itself —
 * see `regenerate`.
 */
async function act(project, fn) {
  actionError.value = null;
  ops.markBusy(project.name, true);
  try {
    await fn(project.name);
  } catch (e) {
    actionError.value = e;
    ops.markBusy(project.name, false);
  }
}

async function confirmDelete() {
  const project = deleteTarget.value;
  deleteTarget.value = null;
  await act(project, (n) => api.projectDelete(n, deleteFiles.value));
  deleteFiles.value = false;
}

/**
 * Re-render the generated tree after a manifest changed underneath us.
 *
 * Not `act`, and the reason is the subject. `generate_run` reports under the
 * SCOPE it was handed — every one of its events carries `"projects"` — so the
 * flag `act` sets under the project's name has nothing coming to clear it. The
 * regenerate finished, said so, and the row's button spun for ever.
 *
 * Cleared in `finally` here instead, which is sound because `generate_run`
 * awaits the whole render before it resolves: the promise settling means the
 * files are written, not that the work was accepted.
 *
 * The badge needs no clearing. It is `item.generatedStale`, measured from the
 * files, so reloading the list after a successful render is what turns it off
 * — and a render that failed leaves it on, which is correct and used not to
 * be: `act` swallows the error, so the marker came off either way.
 */
async function regenerate(project) {
  actionError.value = null;
  ops.markBusy(project.name, true);
  try {
    await api.generateRun('projects');
    await inventory.loadProjects();
  } catch (e) {
    actionError.value = e;
  } finally {
    ops.markBusy(project.name, false);
  }
}

/**
 * The stale-manifest badge does whichever half is actually outstanding.
 *
 * It used to regenerate, always, and that was the weaker of the two acts: a
 * project with an image already built keeps running the old one, so the files
 * on disk agree with `stackvo.json`, the badge goes out, and the container is
 * still the thing it was. The badge said "changed — regenerate to apply it",
 * and regenerating did not apply it.
 *
 * So: built means rebuild — regenerate, build the image, recreate the container
 * — and unbuilt means regenerate, because there is nothing to rebuild yet and
 * pulling a base image is not what a badge click should start.
 */
function applyChange(project) {
  if (!project.built) return regenerate(project);
  return act(project, (n) => api.projectBuild(n));
}

/**
 * What the runtime cell says: a glyph, a name and a version — and, for PHP, the
 * server as well.
 *
 * This cell used to be a two-way branch: `runtime === 'node'` drew Node and
 * *everything else* drew PHP. The app builds eight runtimes, so a Go project
 * appeared in the table as "PHP N/A" under an elephant, and so did Ruby, Rust,
 * Python, Bun and Deno. It was not a rounding error in the label — the row was
 * naming the wrong language.
 *
 * The version lives in a different block per family, which is what makes a
 * single expression impossible here: `php.version`, `node.version`, and one
 * `lang` block shared by the six `LANG_RUNTIMES`, keyed in the file by the
 * runtime's own name.
 */
function runtimeOf(item) {
  const look = runtimeLook(item.runtime);

  const version =
    item.manifest[item.runtime]?.version ??
    item.manifest.lang?.version ??
    item.manifest.php?.version;

  return {
    icon: look.icon,
    label: version ? `${look.label} ${version}` : look.label,
    // Ignored everywhere else — `manifest::read` says so in a warning — so it
    // is shown where it means something and nowhere else.
    server: item.runtime === 'php' ? item.manifest.server || null : null,
  };
}

/**
 * Hand a project's directory to something outside the app.
 *
 * Reported rather than swallowed: "open in editor" fails on a machine with no
 * editor configured, and a menu item that does nothing at all and says nothing
 * at all is indistinguishable from one that worked silently.
 */
async function reveal(fn, project) {
  actionError.value = null;
  try {
    await fn(project.path);
  } catch (e) {
    actionError.value = e;
  }
}

/**
 * Everything a row can do, in words, for the overflow menu at the end of it.
 *
 * The conditions are the columns' own and are read from the same item, because
 * a menu that offers a restart on a stopped container — or hides a rebuild the
 * column beside it is showing — is a menu that has to be kept in step by hand.
 * Written once here, both the row and the menu answer from it.
 *
 * `divider: true` entries are rendered as rules rather than items. The
 * destructive one is below a rule and last, which is the only placement that
 * survives somebody opening this menu quickly.
 *
 * One thing on the detail page's toolbar is deliberately not here: the quick
 * commands — artisan, composer, npm. They are read out of each project's own
 * files, so offering them on a row would mean fetching a command catalogue per
 * project for a table nobody has opened a menu on yet, and they would arrive as
 * a menu inside a menu. They stay where the project is already loaded.
 */
function rowActions(item) {
  const busy = ops.isBusy(item.name);
  const out = [];

  // The control column's three states, which are one button that means a
  // different thing in each. Named, they stop being one button.
  if (!item.built) {
    out.push({
      key: 'build',
      icon: 'mdi-hammer-wrench',
      title: t('projectsView.menu.build'),
      disabled: busy || !app.engineUp || !item.manifestValid,
      run: () => act(item, (n) => api.projectBuild(n)),
    });
  } else if (item.running) {
    out.push({
      key: 'stop',
      icon: 'mdi-stop',
      title: t('projectsView.menu.stop'),
      disabled: busy,
      run: () => act(item, api.projectStop),
    });
  } else {
    out.push({
      key: 'start',
      icon: 'mdi-play',
      title: t('projectsView.menu.start'),
      disabled: busy || !app.engineUp,
      run: () => act(item, api.projectStart),
    });
  }

  if (item.running) {
    out.push({
      key: 'restart',
      icon: 'mdi-restart',
      title: t('projectsView.menu.restart'),
      disabled: busy,
      run: () => act(item, api.projectRestart),
    });
  }

  if (item.built) {
    out.push({
      key: 'rebuild',
      icon: 'mdi-hammer-wrench',
      title: t('projectsView.rebuild'),
      disabled: busy || !app.engineUp || !item.manifestValid,
      run: () => act(item, (n) => api.projectBuild(n)),
    });
  }

  // Never had a column. It was a small blue icon beside the domain that you had
  // to know was clickable, which is not a way to offer the act that makes a
  // changed manifest take effect.
  if (item.generatedStale) {
    out.push({
      key: 'apply',
      icon: 'mdi-sync-alert',
      title: t('projectsView.menu.apply'),
      disabled: busy,
      run: () => applyChange(item),
    });
  }

  out.push({ key: 'sep-open', divider: true });

  if (item.domain && item.running && item.domainConfigured) {
    out.push({
      key: 'open',
      icon: 'mdi-open-in-new',
      title: t('projectsView.colOpen'),
      run: () => api.openInBrowser(`https://${item.domain}`),
    });
  }

  // Same condition as the hosts warning beside the domain, and the same fix.
  if (item.domain && !item.domainConfigured) {
    out.push({
      key: 'hosts',
      icon: 'mdi-alert-circle',
      colour: 'warning',
      title: t('projectsView.menu.fixHosts'),
      run: () => (hostsFixFor.value = item.domain),
    });
  }

  // The two the detail page's toolbar had and this row did not. Both act on
  // `path`, which the row has been carrying since `projects_list` first
  // answered — there was nothing to fetch, only nothing offering them.
  out.push({
    key: 'editor',
    icon: 'mdi-code-tags',
    title: t('detail.openInEditor'),
    run: () => reveal(api.openInEditor, item),
  });

  out.push({
    key: 'folder',
    icon: 'mdi-folder-open',
    title: t('detail.openFolder'),
    run: () => reveal(api.openFolder, item),
  });

  if (item.running) {
    out.push({
      key: 'terminal',
      icon: 'mdi-console',
      title: t('detail.externalTerminal'),
      run: () => openTerminal(item),
    });
  }

  out.push({
    key: 'detail',
    icon: 'mdi-open-in-app',
    title: t('projectsView.colDetail'),
    run: () => router.push(`/projects/${item.name}`),
  });

  out.push({ key: 'sep-delete', divider: true });
  out.push({
    key: 'delete',
    icon: 'mdi-delete',
    colour: 'error',
    title: t('projectsView.colDelete'),
    disabled: busy,
    run: () => (deleteTarget.value = item),
  });

  return out;
}

/**
 * Folders under `projects/` with no `stackvo.json`.
 *
 * Behind the overflow menu now, with a count on the button. They used to sit
 * as a strip above the table, and the reason for that was real — they are
 * invisible everywhere else in the app, which is exactly why they accumulate;
 * on the checkout this was first written against there were eleven of them,
 * three of which were Laravel applications.
 *
 * What paid for the move is that the strip cost every reader vertical space on
 * every visit for a job most people do once. The count is the half worth
 * keeping: a badge still says "there are eleven", so the thing that must not
 * become invisible does not, and the panels themselves are one click away
 * rather than permanently above the table.
 */
const adoptable = ref([]);
const adopting = ref(null);

async function loadAdoptable() {
  try {
    adoptable.value = asList(await api.projectAdoptable());
  } catch {
    // A missing workspace is already reported by the requirements gate.
    adoptable.value = [];
  }
}

/**
 * Sites belonging to XAMPP or Laragon.
 *
 * Beside the adoptable folders rather than in a wizard of its own: both
 * questions are "there is code on this machine StackVo is not running", so
 * they share one panel and one entry point.
 */
const installs = ref([]);
const importing = ref(null);
const importMove = ref(false);

/**
 * The panel both of them live in.
 *
 * A dialog rather than a menu full of expansion panels: adopting is a decision
 * per folder, made against evidence — the detected runtime, the files it was
 * read from, the size of what would be copied — and a menu that has to stay
 * open while you read three lines per row and then flip a destructive switch
 * is a menu being used as a window.
 */
const unmanagedOpen = ref(false);

/**
 * What the button's badge counts.
 *
 * Sites already taken are left out: they are listed so you can see the import
 * worked, not because there is anything left to do about them, and counting
 * them would leave the badge showing a number that no amount of work clears.
 */
const unmanagedCount = computed(
  () =>
    adoptable.value.length +
    installs.value.reduce((n, i) => n + i.sites.filter((s) => !s.taken).length, 0)
);

/**
 * An installation somewhere the well-known paths do not look.
 *
 * The defaults are installer defaults and people move things — and without
 * this the answer for somebody with XAMPP on a second drive is "StackVo says I
 * do not have XAMPP", which is worse than no scan at all.
 */
/**
 * The tools a person can point at (L).
 *
 * All five, and two of them are only reachable this way: Valet is a composer
 * package on PATH and Sail is one inside each project, so neither has an
 * installation directory for the scan to find. The list used to be the two that
 * were written first — which meant somebody whose MAMP is not in
 * /Applications had no way to say so.
 */
const IMPORT_SOURCES = ['xampp', 'laragon', 'mamp', 'valet', 'sail'];

/**
 * Only the installations that turned something up.
 *
 * A scan records the tool it found even when there is nothing under it, and a
 * heading reading "0 sites from laragon" answers a question nobody asked. Used
 * for the dialog's emptiness too: an install with no sites is not content.
 */
const importedInstalls = computed(() => installs.value.filter((i) => i.sites.length));

async function pickInstall(source) {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({ directory: true, multiple: false });
  if (!path) return;

  actionError.value = null;
  try {
    const found = await api.importsScanAt(source, path);
    if (!found) {
      actionError.value = { code: 'NOT_FOUND', message: t('imports.notThere', { source }) };
      return;
    }
    // Replaces a previous answer for the same path rather than stacking, so
    // pointing at the same folder twice does not list its sites twice.
    installs.value = [...installs.value.filter((i) => i.path !== found.path), found];
    // The scan can be started from the menu, where the panel that holds the
    // result is not on screen. Finding sites and showing nothing reads as the
    // folder having been rejected.
    unmanagedOpen.value = true;
  } catch (e) {
    actionError.value = e;
  }
}

async function loadImports() {
  try {
    installs.value = asList(await api.importsScan());
  } catch {
    // Nothing installed is the ordinary case, not a failure to report.
    installs.value = [];
  }
}

/**
 * Copy the site in, then adopt it exactly as any other folder is adopted.
 *
 * Two calls rather than one command that does both, and deliberately: adoption
 * already validates the manifest, applies the name rules and asks the schema
 * whether the result is legal. An importer with its own manifest writer would
 * be a second set of those rules to keep in step.
 */
async function importSite(install, site) {
  importing.value = site.path;
  actionError.value = null;
  try {
    await api.importsTake(site.path, site.name, importMove.value);
    // The domain only when the other tool actually said one. Laragon writes a
    // vhost per site; XAMPP serves by path, so there is nothing to carry and
    // adoption falls back to the suffix like every other project.
    await api.projectAdopt(site.name, null, site.domain ? { domain: site.domain } : undefined);
    await Promise.all([inventory.loadProjects(), loadAdoptable(), loadImports()]);
  } catch (e) {
    actionError.value = e;
  } finally {
    importing.value = null;
  }
}

async function adopt(folder) {
  adopting.value = folder.name;
  actionError.value = null;
  try {
    // No spec: the Rust side re-detects and validates against the same schema
    // project_create uses. Passing the detection back would let a stale reading
    // from before an edit be the thing that gets written.
    await api.projectAdopt(folder.name);
    await Promise.all([inventory.loadProjects(), loadAdoptable()]);
  } catch (e) {
    actionError.value = e;
  } finally {
    adopting.value = null;
  }
}

/**
 * Reading a folder's own `docker-compose.yml` before adopting it.
 *
 * Detection reads the code and gets runtime, framework and document root. The
 * compose file records what its author decided — the PHP version, the domain,
 * the extensions, and the backing services, which no marker file states at all.
 * Adopting without it produces a project that builds and then cannot reach its
 * database.
 *
 * Reviewed before applied: the diff covers a manifest *and* somebody's `.env`,
 * which is more than an adoption has ever written in one go.
 */
const migration = ref(null);
const migrationFor = ref('');
const migrationBusy = ref(false);

async function scanCompose(folder) {
  migrationBusy.value = true;
  actionError.value = null;
  migrationFor.value = folder.name;
  try {
    migration.value = await api.migrateScan(folder.name);
  } catch (e) {
    migration.value = null;
    migrationFor.value = '';
    actionError.value = e;
  } finally {
    migrationBusy.value = false;
  }
}

async function applyMigration() {
  migrationBusy.value = true;
  actionError.value = null;
  try {
    await api.migrateApply(migrationFor.value);
    migration.value = null;
    migrationFor.value = '';
    await Promise.all([inventory.loadProjects(), loadAdoptable()]);
  } catch (e) {
    actionError.value = e;
  } finally {
    migrationBusy.value = false;
  }
}

function closeMigration() {
  migration.value = null;
  migrationFor.value = '';
}

/** The conclusions worth a row. Anything the file did not state is left out
 *  rather than shown as a blank — an empty cell reads as "it said nothing"
 *  only if you already know the row was optional. */
const migrationFields = computed(() => {
  const m = migration.value?.migration;
  if (!m) return {};
  const rows = {
    runtime: m.runtime,
    server: m.server,
    phpVersion: m.phpVersion,
    nodeVersion: m.nodeVersion,
    documentRoot: m.documentRoot,
    domain: m.domain,
    extensions: m.extensions.length ? m.extensions.join(', ') : null,
  };
  return Object.fromEntries(Object.entries(rows).filter(([, v]) => v));
});

let teardown = null;

onMounted(async () => {
  favourites.load();
  inventory.loadProjects();
  loadAdoptable();
  loadImports();
  loadWorktrees();

  const offRefresh = await listenAll(REFRESH_TRIGGERS, () => {
    inventory.loadProjects();
    // A worktree arriving or leaving is a project arriving or leaving, and it
    // travels on the same events — so the badge is refreshed by the same nudge
    // rather than by a second subscription that could drift out of step.
    loadWorktrees();
  });

  // The watcher reports a manifest change; it does not regenerate. Rebuilding a
  // container under someone who is mid-edit is worse than the staleness.
  //
  // The event is a nudge to look again, not the answer: whether the project is
  // actually behind its generated output is `generatedStale`, which the reload
  // brings back with the row.
  const offManifest = await listenAll(['manifest:changed'], () => inventory.loadProjects());

  const offHosts = await listenAll(['hosts:changed'], () => inventory.loadProjects());

  teardown = () => {
    offRefresh();
    offManifest();
    offHosts();
  };
});

onUnmounted(() => teardown?.());
</script>

<template>
  <PageLayout
    top-icon="mdi-folder-multiple"
    :top-title="t('projectsView.title')"
    :top-subtitle="t('projectsView.subtitle')"
    :bar-title="t('projectsView.list')"
  >
    <template #bar-append>
      <div class="d-flex ga-2 align-center">
        <v-chip size="large" variant="tonal" color="success">
          {{ inventory.runningProjects.length }} / {{ inventory.projects.length }}
          {{ t('projectsView.running') }}
        </v-chip>
        <v-btn
          icon
          variant="tonal"
          size="small"
          elevation="0"
          :aria-label="t('newProject.title')"
          :disabled="!app.hasWorkspace"
          @click="app.newProjectOpen = true"
        >
          <v-icon>mdi-plus</v-icon>
          <v-tooltip activator="parent" location="bottom">{{ t('newProject.title') }}</v-tooltip>
        </v-btn>
        <v-btn
          icon
          variant="tonal"
          size="small"
          elevation="0"
          :aria-label="t('app.refresh')"
          :loading="inventory.loadingProjects"
          @click="inventory.loadProjects()"
        >
          <v-icon>mdi-refresh</v-icon>
          <v-tooltip activator="parent" location="bottom">{{ t('app.refresh') }}</v-tooltip>
        </v-btn>

        <!-- The things you do to the workspace rather than to a project. Both
             of them answer "there is code on this machine StackVo is not
             running", which is a question about the folder the table is a view
             of — so it belongs beside the table's own controls. -->
        <!-- Bounded. A menu sizes itself to its widest child, and a subtitle is
             a sentence — unbounded, the first version ran the full width of the
             window because one row explained itself in a line of prose. -->
        <v-menu location="bottom end" min-width="280" max-width="360">
          <template #activator="{ props: menu }">
            <v-btn
              v-bind="menu"
              icon
              variant="tonal"
              size="small"
              elevation="0"
              class="mr-2"
              :aria-label="t('unmanaged.title')"
            >
              <!-- The number, not a dot: "eleven folders are sitting there
                   undeclared" is the whole of what the strip above the table
                   used to say at a glance, and a dot says only "something". -->
              <v-badge
                :model-value="!!unmanagedCount"
                :content="unmanagedCount"
                color="primary"
                offset-x="-2"
                offset-y="-2"
              >
                <v-icon>mdi-dots-vertical</v-icon>
              </v-badge>
              <v-tooltip activator="parent" location="bottom">
                {{ t('unmanaged.title') }}
              </v-tooltip>
            </v-btn>
          </template>

          <v-list density="compact" class="more-menu">
            <v-list-subheader>{{ t('unmanaged.title') }}</v-list-subheader>

            <!-- Every item says what it does under its name. A menu where one
                 row is explained and the next two are not reads as the first
                 one needing an excuse.

                 The count is folders and sites together, as the badge is:
                 "{n} folders" would read 0 on a machine whose only unmanaged
                 code is a XAMPP installation. -->
            <v-list-item
              prepend-icon="mdi-folder-search-outline"
              :title="t('unmanaged.review')"
              :subtitle="
                unmanagedCount
                  ? t('unmanaged.waiting', { n: unmanagedCount })
                  : t('unmanaged.nothing')
              "
              @click="unmanagedOpen = true"
            />

            <v-divider class="my-1" />

            <!-- The scans that ran on load only looked where the installers
                 put things. This is for the second drive. -->
            <v-list-item
              v-for="source in IMPORT_SOURCES"
              :key="source"
              prepend-icon="mdi-folder-open-outline"
              :title="t('imports.pick', { tool: source })"
              :subtitle="t('unmanaged.pickExplain')"
              @click="pickInstall(source)"
            />
          </v-list>
        </v-menu>
      </div>
    </template>

    <ErrorAlert
      :error="actionError || inventory.projectsError"
      type="error"
      closable
      class="ma-2"
      @close="actionError = null"
    />

    <!-- Unmanaged code ---------------------------------------------------- -->
    <!-- Real code sitting in projects/ with no stackvo.json, and sites
         belonging to XAMPP or Laragon. Both are invisible everywhere else in
         the app, which is why they accumulate; the badge on the overflow
         button is what keeps them from being invisible here too. -->
    <!-- Fixed, both ways. Two accordions that each grew and shrank as they were
         opened made a window that changed size while it was being read; the
         dialog now takes one shape and its body scrolls inside it. Vuetify caps
         both against the viewport, so a short screen still gets a whole
         dialog. -->
    <v-dialog v-model="unmanagedOpen" width="820" height="560" scrollable>
      <v-card>
        <v-card-title class="d-flex align-center ga-2">
          <v-icon size="20">mdi-folder-search-outline</v-icon>
          {{ t('unmanaged.title') }}
        </v-card-title>
        <!-- Wraps. Vuetify clips a card subtitle to one line, and this one is a
             sentence: it shipped as "…ve XAMPP ya da Laragon'a …", which ends
             before it has said the second of the two things it is there to
             name. -->
        <v-card-subtitle class="text-caption pb-2 dialog-lede">
          {{ t('unmanaged.explain') }}
        </v-card-subtitle>

        <v-divider />

        <v-card-text class="pa-0">
          <!-- The same error as the page's, and deliberately the same ref: an
               import that fails while this is open would otherwise report it
               behind the dialog. -->
          <ErrorAlert
            :error="actionError"
            type="error"
            closable
            class="ma-2"
            @close="actionError = null"
          />

          <!-- Said rather than left as an empty dialog: "we looked and there
               is nothing" and "this has not run" look identical when blank.

               The "show me the XAMPP folder" actions are not repeated here:
               they sit in the overflow menu that opened this dialog, and a
               control does not become a second control by being shown twice. -->
          <div
            v-if="!unmanagedCount && !importedInstalls.length"
            class="pa-4 text-caption text-medium-emphasis"
          >
            {{ t('unmanaged.none') }}
          </div>

          <!-- Sites belonging to another tool. Same shape as the adoptable
               section below it, because it answers the same question from
               further away. -->
          <div v-if="importedInstalls.length" class="adopt-section">
            <!-- The destructive choice is a switch above the lists, off, and
                 says what it does. A per-row "move" button would be a delete
                 somebody reaches for while aiming at the row below.

                 Once, not once per tool: it is a single setting, and a copy of
                 it under every heading would read as a per-tool choice that it
                 has never been. -->
            <v-switch
              v-model="importMove"
              color="warning"
              density="compact"
              hide-details
              :label="t('imports.move')"
            />
            <div class="text-caption text-medium-emphasis mb-3">
              {{ importMove ? t('imports.moveOn') : t('imports.moveOff') }}
            </div>

            <div v-for="install in importedInstalls" :key="install.path" class="adopt-group">
              <div class="section-head d-flex align-center ga-1">
                <v-icon size="small">mdi-import</v-icon>
                {{ t('imports.found', { tool: install.source, n: install.sites.length }) }}
              </div>
              <div class="text-caption text-medium-emphasis mb-2">
                {{ t('imports.explain', { path: install.path }) }}
              </div>

              <v-table density="compact" hover fixed-header class="adopt-table">
                <thead>
                  <tr>
                    <th>{{ t('imports.colSite') }}</th>
                    <th>{{ t('imports.colDetected') }}</th>
                    <th>{{ t('imports.colDomain') }}</th>
                    <th>{{ t('imports.colSize') }}</th>
                    <th class="text-end">{{ t('imports.colAction') }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="site in install.sites" :key="site.path">
                    <td class="adopt-name">{{ site.name }}</td>

                    <td>
                      <div class="cell-chips">
                        <v-chip
                          v-if="site.detected.framework"
                          size="x-small"
                          color="success"
                          variant="tonal"
                        >
                          {{ site.detected.framework }}
                        </v-chip>
                        <v-chip v-else size="x-small" variant="tonal">
                          {{ site.detected.runtime }}
                        </v-chip>

                        <!-- Only Sail says what it needs: its compose file names
                             the services, mapped here onto this app's own
                             catalogue. The other tools do not state it, and
                             inventing it would be an import that describes
                             something nobody wrote. -->
                        <v-chip
                          v-for="service in site.services || []"
                          :key="service"
                          size="x-small"
                          color="info"
                          variant="tonal"
                          :title="t('imports.serviceHint')"
                        >
                          {{ service }}
                        </v-chip>
                      </div>
                    </td>

                    <td class="adopt-evidence">{{ site.domain || '—' }}</td>

                    <td class="adopt-evidence">
                      {{
                        site.partial
                          ? t('imports.sizeAtLeast', { size: bytes(site.bytes) })
                          : bytes(site.bytes)
                      }}
                    </td>

                    <td class="text-end">
                      <span v-if="site.taken" class="text-caption text-medium-emphasis">
                        {{ t('imports.taken') }}
                      </span>
                      <v-btn
                        v-else
                        size="x-small"
                        variant="tonal"
                        color="primary"
                        prepend-icon="mdi-import"
                        :loading="importing === site.path"
                        :disabled="!!importing || !!adopting"
                        @click="importSite(install, site)"
                      >
                        {{ t('imports.take') }}
                      </v-btn>
                    </td>
                  </tr>
                </tbody>
              </v-table>
            </div>
          </div>

          <v-divider v-if="importedInstalls.length && adoptable.length" />

          <!-- No longer behind a header you have to ask for by name. There is
               one list here and it is the reason the dialog is usually opened;
               the card scrolls, so nothing below it is out of reach. -->
          <div v-if="adoptable.length" class="adopt-section">
            <div class="section-head d-flex align-center ga-1">
              <v-icon size="small">mdi-folder-search-outline</v-icon>
              {{ t('adopt.found', { n: adoptable.length }) }}
            </div>

            <!-- The directory itself, named, the way the tool sections above
                 name theirs. There is no fixed `projects/` to refer to: the
                 tree is wherever the workspace gate was pointed, so the text
                 that claimed one was describing a layout this app dropped.

                 `projectsDir`, which is what Settings shows as the working
                 directory and what the scan actually reads. `root` is the app's
                 own state directory — `~/.stackvo` — and naming it here told
                 the reader to go looking in the wrong place. -->
            <div v-if="app.workspace?.projectsDir" class="text-caption text-medium-emphasis mb-2">
              {{ t('adopt.where', { path: app.workspace.projectsDir }) }}
            </div>

            <v-table density="compact" hover fixed-header class="adopt-table">
              <thead>
                <tr>
                  <th>{{ t('adopt.colFolder') }}</th>
                  <th>{{ t('adopt.colDetected') }}</th>
                  <th>{{ t('adopt.colEvidence') }}</th>
                  <th class="text-end">{{ t('adopt.colAction') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="folder in adoptable" :key="folder.name">
                  <td class="adopt-name">{{ folder.name }}</td>

                  <td>
                    <v-chip
                      v-if="folder.detected.framework"
                      size="x-small"
                      color="success"
                      variant="tonal"
                    >
                      {{ folder.detected.framework }}
                    </v-chip>
                    <v-chip v-else size="x-small" variant="tonal">
                      {{ folder.detected.runtime }}
                    </v-chip>
                  </td>

                  <!-- The files the guess came from. A document root inferred
                       wrongly builds, starts and serves a 404 with no error
                       anywhere. -->
                  <td class="adopt-evidence">
                    {{
                      folder.detected.evidence.length
                        ? t('adopt.from', { files: folder.detected.evidence.join(', ') })
                        : t('adopt.noEvidence')
                    }}
                  </td>

                  <td class="text-end text-no-wrap">
                    <!-- Offered only when the folder has one. It is the better
                         route when it exists: a compose file states the PHP
                         version, the domain and the services, none of which any
                         marker file does. -->
                    <v-btn
                      v-if="folder.composeFile"
                      size="x-small"
                      variant="tonal"
                      color="primary"
                      prepend-icon="mdi-file-import-outline"
                      class="mr-1"
                      :loading="migrationBusy && migrationFor === folder.name"
                      :disabled="!!adopting || migrationBusy"
                      @click="scanCompose(folder)"
                    >
                      {{ t('migrate.read') }}
                    </v-btn>

                    <v-btn
                      size="x-small"
                      variant="tonal"
                      :loading="adopting === folder.name"
                      :disabled="!!adopting || !folder.hasFiles || migrationBusy"
                      @click="adopt(folder)"
                    >
                      {{ t('adopt.action') }}
                    </v-btn>
                  </td>
                </tr>
              </tbody>
            </v-table>
          </div>
        </v-card-text>

        <v-divider />

        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="unmanagedOpen = false">{{ t('app.close') }}</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- The compose review. A dialog rather than an inline expansion: it is a
         decision about two files at once — a manifest and the shared .env —
         and it deserves the whole of the reader's attention. -->
    <v-dialog :model-value="!!migration" max-width="760" @update:model-value="closeMigration">
      <v-card v-if="migration">
        <v-card-title class="d-flex align-center ga-2">
          <v-icon size="20">mdi-file-import-outline</v-icon>
          {{ t('migrate.title', { name: migrationFor }) }}
        </v-card-title>
        <v-card-subtitle class="text-caption pb-2">
          {{ migration.migration.source }}
        </v-card-subtitle>

        <v-divider />

        <v-card-text>
          <div class="section-head mb-2">{{ t('migrate.project') }}</div>
          <v-table density="compact">
            <tbody>
              <tr v-for="(value, key) in migrationFields" :key="key">
                <td class="text-medium-emphasis">{{ t(`migrate.field.${key}`) }}</td>
                <td class="mono">{{ value }}</td>
              </tr>
            </tbody>
          </v-table>

          <!-- The half no marker file can state, and the reason this exists. -->
          <template v-if="migration.env.changes.length">
            <div class="section-head mt-5 mb-2">{{ t('migrate.services') }}</div>
            <v-table density="compact">
              <tbody>
                <tr v-for="change in migration.env.changes" :key="change.key">
                  <td>{{ change.subject }}</td>
                  <td class="mono text-medium-emphasis">{{ change.from ?? '—' }}</td>
                  <td class="mono">{{ change.to }}</td>
                </tr>
              </tbody>
            </v-table>
          </template>
          <div
            v-else-if="migration.migration.services.length"
            class="text-caption text-medium-emphasis mt-4"
          >
            {{ t('migrate.servicesAlready') }}
          </div>

          <!-- Named, not dropped: silently ignoring the one service the project
               actually needs looks finished and is not. -->
          <v-alert
            v-if="migration.migration.unmapped.length"
            type="warning"
            variant="tonal"
            class="mt-4"
          >
            <div class="text-caption font-weight-medium mb-1">{{ t('migrate.unmapped') }}</div>
            <div
              v-for="entry in migration.migration.unmapped"
              :key="entry"
              class="text-caption mono"
            >
              {{ entry }}
            </div>
          </v-alert>

          <v-alert v-if="migration.alreadyManaged" type="info" variant="tonal" class="mt-4">
            <div class="text-caption">{{ t('migrate.alreadyManaged') }}</div>
          </v-alert>

          <v-expansion-panels variant="accordion" class="mt-4">
            <v-expansion-panel :title="t('migrate.evidence')">
              <v-expansion-panel-text>
                <div
                  v-for="line in migration.migration.evidence"
                  :key="line"
                  class="text-caption mono"
                >
                  {{ line }}
                </div>
              </v-expansion-panel-text>
            </v-expansion-panel>
            <v-expansion-panel :title="t('migrate.manifest')">
              <v-expansion-panel-text>
                <pre class="migrate-json">{{ JSON.stringify(migration.spec, null, 2) }}</pre>
              </v-expansion-panel-text>
            </v-expansion-panel>
          </v-expansion-panels>
        </v-card-text>

        <v-divider />

        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" :disabled="migrationBusy" @click="closeMigration">
            {{ t('app.cancel') }}
          </v-btn>
          <v-btn color="primary" variant="flat" :loading="migrationBusy" @click="applyMigration">
            {{ t('migrate.apply') }}
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- One control, not a row of them. Four status buttons and a star beside
         the search box was a second toolbar competing with the page's own, and
         the widths of four translated words decided how much room was left to
         type in. They are a menu on the end of the field now.
         The field still says what is on: the funnel takes the accent colour
         and carries a count while anything is narrowing the list, because a
         filter you cannot see is a filter you get stuck in. -->
    <v-text-field
      v-model="search"
      prepend-inner-icon="mdi-magnify"
      :label="t('projectsView.searchPlaceholder')"
      class="rounded-0 search-field"
      variant="filled"
      rounded="0"
      single-line
      hide-details
      clearable
    >
      <template #append-inner>
        <v-menu location="bottom end" min-width="240" :close-on-content-click="false">
          <template #activator="{ props: menu }">
            <v-btn
              v-bind="menu"
              icon
              size="small"
              variant="text"
              class="filter-btn"
              :color="activeFilters ? 'primary' : undefined"
              :aria-label="t('projectsView.filter.title')"
              @click.stop
            >
              <v-badge :model-value="!!activeFilters" :content="activeFilters" color="primary">
                <v-icon size="small">
                  {{ activeFilters ? 'mdi-filter' : 'mdi-filter-outline' }}
                </v-icon>
              </v-badge>
              <v-tooltip activator="parent" location="bottom">
                {{ t('projectsView.filter.title') }}
              </v-tooltip>
            </v-btn>
          </template>

          <v-list density="compact">
            <v-list-subheader>{{ t('projectsView.filter.status') }}</v-list-subheader>
            <!-- Checkmarks rather than a radio group: the state has to be
                 readable at a glance in a menu that stays open, and a list item
                 that merely looks selected reads as hover. -->
            <v-list-item
              v-for="f in STATUS_FILTERS"
              :key="f.value"
              :active="statusFilter === f.value"
              :prepend-icon="statusFilter === f.value ? 'mdi-check' : 'mdi-blank'"
              :title="f.label"
              @click="statusFilter = f.value"
            />

            <v-divider class="my-1" />

            <!-- Not a fifth status. It narrows *with* the status rather than
                 instead of it, and "my starred projects that are running" is
                 the pair somebody actually wants. -->
            <v-list-item
              :active="favouritesOnly"
              :prepend-icon="favouritesOnly ? 'mdi-star' : 'mdi-star-outline'"
              :base-color="favouritesOnly ? 'warning' : undefined"
              :title="t('projectsView.filter.favourites')"
              @click="favouritesOnly = !favouritesOnly"
            />

            <template v-if="activeFilters">
              <v-divider class="my-1" />
              <v-list-item
                prepend-icon="mdi-filter-off-outline"
                :title="t('projectsView.filter.clear')"
                @click="clearFilters"
              />
            </template>
          </v-list>
        </v-menu>
      </template>
    </v-text-field>

    <div class="table-wrap">
      <v-data-table
        :headers="headers"
        :items="visibleRows"
        :group-by="groupBy"
        :sort-by="sortBy"
        :search="search"
        :loading="inventory.loadingProjects"
        items-per-page="-1"
        class="elevation-0"
        fixed-header
        hover
        item-value="name"
        striped="even"
        hide-default-footer
        height="100%"
        density="compact"
      >
        <!-- Standalone projects never reach this slot: their group value is
             null and the table skips the header for those outright. The guard
             is here anyway so a future key change cannot quietly reintroduce a
             one-row group with a heading over it. -->
        <template #group-header="{ item, columns, toggleGroup, isGroupOpen }">
          <tr
            v-if="item.items.length > 1"
            class="group-row"
            :data-open="openByDefault(item, isGroupOpen, toggleGroup)"
          >
            <td :colspan="columns.length">
              <div class="d-flex align-center ga-2">
                <v-btn
                  icon
                  size="x-small"
                  variant="text"
                  :aria-label="item.value"
                  @click="toggleGroup(item)"
                >
                  <v-icon>{{
                    isGroupOpen(item) ? 'mdi-chevron-down' : 'mdi-chevron-right'
                  }}</v-icon>
                  <v-tooltip activator="parent" location="top">{{ item.value }}</v-tooltip>
                </v-btn>
                <v-icon size="small" icon="mdi-sitemap-outline" />
                <span class="font-weight-medium">{{ item.value }}</span>
                <v-chip size="x-small" variant="tonal">{{ item.items.length }}</v-chip>
              </div>
            </td>
          </tr>
        </template>

        <!-- The column shows a star and no words, and an empty `<th>` is a
             column a screen reader announces as nothing while reading every
             cell under it (axe's `empty-table-header`). The name is rendered
             and hidden rather than left out, so the header row looks exactly as
             it did. -->
        <template #header.favourite>
          <span class="visually-hidden">{{ t('projectsView.colFavourite') }}</span>
        </template>

        <template #item.favourite="{ item }">
          <v-btn
            icon
            size="x-small"
            variant="text"
            data-test="favourite"
            :aria-label="
              item.favourite
                ? t('projectsView.aria.unfavourite', { name: item.name })
                : t('projectsView.aria.favourite', { name: item.name })
            "
            @click="favourites.toggle(item.name)"
          >
            <v-icon :color="item.favourite ? 'warning' : undefined" size="small">
              {{ item.favourite ? 'mdi-star' : 'mdi-star-outline' }}
            </v-icon>
            <v-tooltip activator="parent" location="top">
              {{
                item.favourite
                  ? t('projectsView.aria.unfavourite', { name: item.name })
                  : t('projectsView.aria.favourite', { name: item.name })
              }}
            </v-tooltip>
          </v-btn>
        </template>

        <template #item.domain="{ item }">
          <div v-if="item.domain" class="d-flex align-center ga-2">
            <!-- A button, not an anchor. It carries no href — it calls a
                 command — and an anchor without one takes no focus and is
                 announced as text, so the whole table was unreachable by
                 keyboard. Found by the first run of the browser suite. -->
            <button
              type="button"
              class="domain-link"
              :disabled="!item.domainConfigured"
              @click="api.openInBrowser(`https://${item.domain}`)"
            >
              {{ item.domain }}
            </button>

            <!-- N. Without this the list shows `shop` and `shop-feature-x` as
                 two unrelated siblings, which is exactly the confusion
                 worktrees would otherwise introduce. -->
            <v-chip
              v-if="item.worktree"
              size="x-small"
              variant="tonal"
              label
              prepend-icon="mdi-source-branch"
              data-test="worktree-badge"
            >
              {{ t('projectsView.worktreeOf', { parent: item.worktree.parent }) }}
            </v-chip>

            <!-- A domain with no hosts entry cannot resolve. The web UI could
                 detect this; here the icon is also the fix. -->
            <v-tooltip v-if="!item.domainConfigured" location="top">
              <template #activator="{ props }">
                <v-icon
                  v-bind="props"
                  color="warning"
                  size="small"
                  :aria-label="t('projectsView.aria.fixHosts', { name: item.domain })"
                  :aria-hidden="false"
                  @click.stop="hostsFixFor = item.domain"
                  >mdi-alert-circle</v-icon
                >
              </template>
              <div class="text-caption">
                <strong>{{ t('projectsView.noDnsRecord') }}</strong
                ><br />
                {{ t('projectsView.addToHosts') }}<br />
                <code>127.0.0.1 {{ item.domain }}</code>
              </div>
            </v-tooltip>

            <!-- Contract violations, shown rather than swallowed: the render
                 skips such projects without a word. -->
            <v-tooltip v-if="!item.manifestValid" location="top">
              <template #activator="{ props }">
                <v-icon v-bind="props" color="error" size="small">mdi-file-alert</v-icon>
              </template>
              <div class="text-caption">
                <strong>{{ t('projects.invalidManifest') }}</strong
                ><br />
                <span v-for="(issue, i) in item.manifest.errors" :key="i">
                  {{ issue.code }} {{ issue.path }} — {{ issue.message }}<br />
                </span>
              </div>
            </v-tooltip>

            <v-tooltip v-if="item.generatedStale" location="top">
              <template #activator="{ props }">
                <v-icon v-bind="props" color="info" size="small" @click.stop="applyChange(item)"
                  >mdi-sync-alert</v-icon
                >
              </template>
              <span class="text-caption">
                {{
                  item.built ? t('projects.manifestChangedBuilt') : t('projects.manifestChanged')
                }}
              </span>
            </v-tooltip>
          </div>
          <span v-else class="text-medium-emphasis">—</span>
        </template>

        <template #item.runtime="{ item }">
          <v-icon start>{{ runtimeOf(item).icon }}</v-icon>
          {{ runtimeOf(item).label }}
          <!-- PHP's server, on the runtime it belongs to. Faint and after a
               separator, because it is a property of the runtime rather than a
               second thing the row is about. -->
          <span v-if="runtimeOf(item).server" class="text-medium-emphasis">
            · {{ runtimeOf(item).server }}
          </span>
        </template>

        <!-- Three states, and the middle one is the reason this is not a tick.
             A directory that was never versioned, a repository with local
             history and no upstream, and a clone: "did this come from
             somewhere" has three honest answers and a boolean would fold the
             first two together.

             The remote is in the tooltip rather than the cell. It is a URL —
             the widest thing that could go in a table — and it is read once,
             when somebody wants to know *which* repository. -->
        <template #item.repo="{ item }">
          <span v-if="!item.git" class="text-disabled">—</span>
          <v-btn
            v-else
            icon
            size="x-small"
            variant="text"
            :color="item.git.remote ? 'primary' : undefined"
            :aria-label="item.git.remote || t('projectsView.repoLocal')"
            @click.stop
          >
            <v-icon size="small">{{ item.git.remote ? 'mdi-source-branch' : 'mdi-git' }}</v-icon>
            <v-tooltip activator="parent" location="top">
              {{ item.git.remote || t('projectsView.repoLocal') }}
            </v-tooltip>
          </v-btn>
        </template>

        <template #item.configuration>
          <v-chip size="small" variant="tonal" color="grey" class="w-100">
            <v-icon start size="small">mdi-cog-outline</v-icon>{{ t('projectsView.default') }}
          </v-chip>
        </template>

        <template #item.control="{ item }">
          <v-btn
            v-if="!item.built"
            block
            size="small"
            color="info"
            variant="tonal"
            :loading="ops.isBusy(item.name)"
            :disabled="!app.engineUp || !item.manifestValid"
            :aria-label="t('projectsView.aria.build', { name: item.name })"
            @click="act(item, (n) => api.projectBuild(n))"
          >
            <v-icon>mdi-hammer-wrench</v-icon>
            <v-tooltip activator="parent" location="top">{{
              t('projectsView.menu.build')
            }}</v-tooltip>
          </v-btn>
          <v-btn
            v-else-if="item.running"
            block
            size="small"
            color="error"
            variant="tonal"
            :loading="ops.isBusy(item.name)"
            :aria-label="t('projectsView.aria.stop', { name: item.name })"
            @click="act(item, api.projectStop)"
          >
            <v-icon>mdi-stop</v-icon>
            <v-tooltip activator="parent" location="top">{{
              t('projectsView.menu.stop')
            }}</v-tooltip>
          </v-btn>
          <v-btn
            v-else
            block
            size="small"
            color="success"
            variant="tonal"
            :loading="ops.isBusy(item.name)"
            :disabled="!app.engineUp"
            :aria-label="t('projectsView.aria.start', { name: item.name })"
            @click="act(item, api.projectStart)"
          >
            <v-icon>mdi-play</v-icon>
            <v-tooltip activator="parent" location="top">{{
              t('projectsView.menu.start')
            }}</v-tooltip>
          </v-btn>
        </template>

        <template #item.restart="{ item }">
          <v-btn
            v-if="item.running"
            block
            size="small"
            color="warning"
            variant="tonal"
            :loading="ops.isBusy(item.name)"
            :aria-label="t('projectsView.aria.restart', { name: item.name })"
            @click="act(item, api.projectRestart)"
          >
            <v-icon>mdi-restart</v-icon>
            <v-tooltip activator="parent" location="top">{{
              t('projectsView.menu.restart')
            }}</v-tooltip>
          </v-btn>
        </template>

        <template #item.terminal="{ item }">
          <v-btn
            v-if="item.running"
            block
            size="small"
            color="info"
            variant="tonal"
            :aria-label="t('detail.externalTerminal')"
            @click="openTerminal(item)"
          >
            <v-icon>mdi-console</v-icon>
            <v-tooltip activator="parent">{{ t('detail.externalTerminal') }}</v-tooltip>
          </v-btn>
        </template>

        <template #item.open="{ item }">
          <!-- Only when the domain resolves; otherwise the browser shows an
               error page and the user has no idea why. -->
          <v-btn
            v-if="item.domain && item.running && item.domainConfigured"
            block
            size="small"
            color="primary"
            variant="tonal"
            :aria-label="t('projectsView.aria.open', { name: item.name })"
            @click="api.openInBrowser(`https://${item.domain}`)"
          >
            <v-icon>mdi-open-in-new</v-icon>
            <v-tooltip activator="parent" location="top">{{ item.domain }}</v-tooltip>
          </v-btn>
        </template>

        <template #item.detail="{ item }">
          <v-btn
            block
            size="small"
            color="info"
            variant="tonal"
            :aria-label="t('projectsView.aria.detail', { name: item.name })"
            @click="router.push(`/projects/${item.name}`)"
          >
            <v-icon>mdi-open-in-app</v-icon>
            <v-tooltip activator="parent" location="top">{{
              t('projectsView.colDetail')
            }}</v-tooltip>
          </v-btn>
        </template>

        <!-- The same acts as the columns, named, plus the ones that have no
             column of their own.
             Built from `rowActions(item)` rather than written out as fifteen
             `v-list-item`s with `v-if` on each: the conditions are the columns'
             own — a rebuild needs an image, a restart needs a running
             container — and duplicating them in a second place is how the menu
             and the row start disagreeing about what a project can do. -->
        <template #item.more="{ item }">
          <v-menu location="bottom end" min-width="240">
            <template #activator="{ props: menu }">
              <!-- Sized to its neighbours rather than to Vuetify's idea of an
                   icon button. `.v-btn--icon` adds 12px to the height at
                   default density, so this one circle stood taller than the
                   nine rectangles beside it and made the row look misaligned
                   at its own end. -->
              <v-btn
                v-bind="menu"
                icon
                size="small"
                variant="tonal"
                class="row-more"
                :aria-label="t('projectsView.aria.more', { name: item.name })"
              >
                <v-icon>mdi-dots-vertical</v-icon>
                <v-tooltip activator="parent" location="top">
                  {{ t('projectsView.colMore') }}
                </v-tooltip>
              </v-btn>
            </template>

            <v-list density="compact">
              <template v-for="action in rowActions(item)" :key="action.key">
                <v-divider v-if="action.divider" class="my-1" />
                <v-list-item
                  v-else
                  :prepend-icon="action.icon"
                  :title="action.title"
                  :base-color="action.colour"
                  :disabled="action.disabled"
                  @click="action.run()"
                />
              </template>
            </v-list>
          </v-menu>
        </template>

        <!-- Two empty states, not one.
             "Nothing here yet" and "nothing matched what you typed" are
             different situations with different next moves, and a single
             centred sentence answered neither: on a first run it named the
             problem and offered nothing, and after a typo it implied the
             projects were gone. Each now carries the action that resolves it. -->
        <template #no-data>
          <!-- A filter can empty this table as easily as a typo can, and the
               two look identical from here. The button clears both, so an empty
               table is never a place somebody is stuck in. -->
          <v-empty-state
            v-if="narrowed"
            icon="mdi-magnify-close"
            :title="t('projects.noMatch')"
            :text="
              search ? t('projects.noMatchText', { term: search }) : t('projects.noMatchFilter')
            "
          >
            <template #actions>
              <v-btn variant="tonal" prepend-icon="mdi-close" @click="clearFilters">
                {{ t('projects.clearSearch') }}
              </v-btn>
            </template>
          </v-empty-state>

          <v-empty-state
            v-else
            icon="mdi-folder-plus-outline"
            :title="t('projects.empty')"
            :text="t('projects.emptyText')"
          >
            <template #actions>
              <v-btn
                color="primary"
                variant="flat"
                prepend-icon="mdi-plus"
                :disabled="!app.hasWorkspace"
                @click="app.newProjectOpen = true"
              >
                {{ t('newProject.title') }}
              </v-btn>
            </template>
          </v-empty-state>
        </template>

        <template #bottom />
      </v-data-table>
    </div>

    <!-- Deleting source code needs an explicit opt-in, not a default. -->
    <v-dialog
      :model-value="!!deleteTarget"
      max-width="480"
      @update:model-value="deleteTarget = null"
    >
      <v-card v-if="deleteTarget">
        <v-card-item>
          <template #prepend><v-icon color="error">mdi-delete-alert-outline</v-icon></template>
          <v-card-title class="text-body-1">
            {{ t('newProject.deleteTitle', { name: deleteTarget.name }) }}
          </v-card-title>
        </v-card-item>
        <v-card-text>
          <p class="text-body-2 mb-1">{{ t('newProject.deleteBody') }}</p>
          <!-- The rest of what goes, named. A dialog that says only "your
               source files stay" reads as a promise that nothing else moves. -->
          <p class="text-caption text-medium-emphasis mb-3">{{ t('newProject.deleteAlso') }}</p>
          <v-checkbox
            v-model="deleteFiles"
            :label="t('newProject.deleteFiles')"
            hide-details
            color="error"
          />
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="deleteTarget = null">{{ t('hosts.cancel') }}</v-btn>
          <v-btn color="error" variant="flat" @click="confirmDelete">
            {{ t('newProject.delete') }}
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <HostsDialog
      v-if="hostsFixFor"
      :add="[hostsFixFor]"
      :model-value="!!hostsFixFor"
      @update:model-value="hostsFixFor = $event ? hostsFixFor : null"
      @applied="inventory.loadProjects()"
    />
  </PageLayout>
</template>

<style scoped>
/* The overflow button, at the height of the action buttons it sits beside.
   Derived from the same variable they are, so a density change moves all of
   them together instead of moving nine and leaving one. */
.row-more.v-btn--icon {
  width: var(--v-btn-height);
  height: var(--v-btn-height);
}

/* The subtitles wrap instead of being clipped.
   Against a bounded menu the stock rule turns every explanation into its first
   four words and an ellipsis, which is a sentence that costs a line and says
   nothing. `white-space` is not what does it and setting that alone was the
   first, failed attempt: the truncation is `-webkit-line-clamp: 1` from
   `.v-list-item--one-line`, and a clamp only applies to `display:
   -webkit-box`. Both have to go, so the paragraph is a paragraph. */
.more-menu :deep(.v-list-item-subtitle) {
  display: block;
  -webkit-line-clamp: unset;
  white-space: normal;
  line-height: 1.35;
  padding-block: 2px;
}

/* A dialog's opening sentence is a sentence, not a label. Vuetify's subtitle
   is `white-space: nowrap` with an ellipsis, which cuts explanations mid-clause
   in a card 820px wide. */
.dialog-lede {
  white-space: normal;
  overflow: visible;
  text-overflow: clip;
  line-height: 1.4;
}

/* The two lists in the unmanaged dialog, each given the card's own padding so
   the rows line up with the title above them. Nothing bounds their height any
   more: the dialog is a fixed size and its body is what scrolls. */
.adopt-section {
  padding: 12px 16px;
}

/* One tool's sites under one heading. Spaced from the next tool rather than
   ruled off it — a rule per tool in a list of five is a table. */
.adopt-group + .adopt-group {
  margin-top: 12px;
}

/* A heading, not a row of data: it carries the parent domain and a count, and
   should read as the label above the rows rather than as one of them. */
.group-row td {
  background: rgba(var(--v-theme-on-surface), 0.04);
}

/* Names a block: the four in the migration review, and each tool's sites in the
   unmanaged dialog. */
.section-head {
  font-size: 13px;
  font-weight: 600;
  opacity: 0.82;
}

/* A version, a domain or a container path — places where 8.0 and 8.O differ. */
.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}

/* The proposed manifest, scrolling in its own box rather than stretching the
   dialog to the height of whatever the compose file turned out to imply. */
.migrate-json {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11.5px;
  line-height: 1.5;
  max-height: 260px;
  overflow: auto;
  margin: 0;
}

.domain-link {
  /* The reset a button needs to read as the text it replaced. */
  appearance: none;
  background: none;
  border: 0;
  padding: 0;
  font: inherit;
  text-align: inherit;

  color: inherit;
  cursor: pointer;
  text-decoration: none;
}

.domain-link:disabled {
  cursor: default;
}

.domain-link:hover {
  text-decoration: underline;
}

/* The search field keeps its natural height; the table fills the rest. */
.search-field {
  flex: 0 0 auto;
}

.table-wrap {
  flex: 1 1 auto;
  min-height: 0;
}

.table-wrap :deep(.v-table) {
  height: 100%;
}

/* Column labels are short phrases, and wrapping them onto a second line makes
   the header band twice the height of a row for no gain. They stay on one line
   and take the width they need. */
.table-wrap :deep(thead th) {
  white-space: nowrap;
}

.table-wrap :deep(.v-data-table-header__content) {
  flex-wrap: nowrap;
}

/* Each list scrolls inside itself, under a header that stays put. The dialog is
   a fixed size and holds two of these; without a bound of their own, a workspace
   with twenty stray folders pushes the second list past the bottom of the card
   and you scroll the card to find out there was a second one at all. */
.adopt-table :deep(.v-table__wrapper) {
  max-height: 260px;
}

.adopt-table :deep(th) {
  font-size: 12px;
  white-space: nowrap;
}

/* Rows are read across, not down: a name, a guess, what the guess came from.
   Compact density puts them at 32px, and this stops a long cell from making one
   row twice the height of its neighbours. */
.adopt-table :deep(td) {
  white-space: nowrap;
}

/* The runtime, the framework and whatever services a compose file named — on
   one line, in a cell that does not stretch to hold them. */
.cell-chips {
  display: flex;
  align-items: center;
  gap: 4px;
}

.adopt-name {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 13px;
  font-weight: 600;
}

/* The evidence is the part that lets someone check the guess, so it truncates
   rather than wrapping the row into two lines per folder. */
.adopt-evidence {
  font-size: 12px;
  opacity: 0.7;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 280px;
}
</style>
