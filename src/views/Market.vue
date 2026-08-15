<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '@/lib/ipc';
import { bytes } from '@/lib/format';
import { useMarket } from '@/composables/useMarket';
import PackageAuthorDialog from '@/components/PackageAuthorDialog.vue';
import { useInventoryStore } from '@/stores/inventory';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import SettingsGroup from '@/components/SettingsGroup.vue';
import InstanceCreateDialog from '@/components/InstanceCreateDialog.vue';
import InstanceSettingsSheet from '@/components/InstanceSettingsSheet.vue';
import ServiceDetailSheet from '@/components/ServiceDetailSheet.vue';

/**
 * Where services come from.
 *
 * The Services page shows what is running. This shows what could be: the
 * catalogue a source publishes, which versions of each are on this machine, and
 * which of those an instance is using.
 *
 * The two panes are in that order on purpose. Installing a package and creating
 * an instance are different acts — the first puts files on disk, the second
 * decides that this workspace runs that version — and a page that merged them
 * would make "I want to try MySQL 9.4 alongside 8.0" indistinguishable from
 * "replace my database".
 */

const { t, locale } = useI18n();
const market = useMarket();

/**
 * C-1. Beside the source picker, because both answer "where do packages come
 * from" — one points at somebody else's, one writes your own.
 */
const authorOpen = ref(false);

/**
 * The Services page's rows, so the detail sheet can be opened from here too.
 *
 * Not a second source of truth: in a workspace on the market model
 * `services_list` walks the same instance table `instance_list` does, and the
 * two agree on `id` because both take it from the row. So this is a lookup, not
 * a copy — and it is a lookup rather than a widened `InstanceRow` because the
 * sheet wants what an instance *is doing* (running, ports as published,
 * credentials resolved) and this table is about what it *is*.
 */
const inventory = useInventoryStore();

/**
 * The catalogue as one tree: category → service → version.
 *
 * This replaced a vertical tab rail over a set of expansion panels, which was
 * two collapsing mechanisms stacked on one another — a category you switched to
 * and a service you opened, each with its own idea of where you were. One tree
 * has one, and the depth it adds is the depth the data already had.
 *
 * The nodes carry `kind` and the raw `entry`/`version` rather than only a
 * title, because the slots below render three different rows and the buttons on
 * the leaf need the pair the catalogue is keyed by. `id` is the value the tree
 * opens and closes on, so it has to be unique across all three depths — a
 * version number alone is not: two services can publish `8.0`.
 */
const catalogueTree = computed(() =>
  market.grouped.value.map((group) => ({
    id: `category:${group.category}`,
    kind: 'category',
    title: categoryLabel(group.category),
    group,
    children: group.packages.map((entry) => ({
      id: `service:${entry.service}`,
      kind: 'service',
      title: entry.name?.en ?? entry.service,
      entry,
      children: entry.versions.map((version) => ({
        id: `version:${entry.service}@${version.version}`,
        kind: 'version',
        title: version.version,
        entry,
        version,
      })),
    })),
  }))
);

/**
 * The instance whose settings are open.
 *
 * Here rather than on the Services page: what a service is configured with is a
 * property of the instance, and the instance is created here. The Services page
 * answers "is it running"; this one answers "which version, and set up how".
 */
const settingsInstance = ref(null);
const settingsSheet = ref(false);

function openSettings(instance) {
  settingsInstance.value = instance;
  settingsSheet.value = true;
}

/**
 * The version about to become an instance, and the dialog that asks about it.
 *
 * The `+` button used to create straight away with the package's defaults —
 * which for a database means `root`/`root`, set at first boot and unreachable
 * afterwards by any form. Asking first is not a confirmation step; it is the
 * only moment those values can be chosen.
 */
const createTarget = ref(null);
const createDialog = ref(false);

function askToCreate(service, version) {
  createTarget.value = { service, version };
  createDialog.value = true;
}

const doCreate = ({ service, version, settings, ports }) =>
  market.create(service, version, settings, ports);

/**
 * The row the detail sheet is reading, in the shape that sheet expects.
 *
 * Held as the resolved service rather than as an id: the sheet reads
 * `service.running` and `service.ports` on every render, and a lookup that
 * re-ran as the inventory reloaded would swap the object underneath an open
 * panel — which is how a sheet ends up showing one service's logs under
 * another's title.
 */
const detailTarget = ref(null);

const serviceFor = (instance) => inventory.services.find((s) => s.id === instance.id) ?? null;

/**
 * Is the container up, and what is it reached at.
 *
 * Both come from the services list rather than from the instance row, because
 * neither is a property of the instance: `enabled` says it *should* be running
 * and these say whether it *is* — the difference a restart loop lives in. The
 * row answers null while that list is still loading, which reads as "no button
 * yet" rather than as "stopped".
 */
const runningOf = (instance) => serviceFor(instance)?.running ?? false;
const domainOf = (instance) => serviceFor(instance)?.url ?? null;

/**
 * The health verdict for a running instance, or null.
 *
 * Null in three cases that are all the same thing here — the container is
 * stopped, its image declares no healthcheck, or the services list has not
 * answered yet — and the row shows nothing beside the switch in all of them.
 * A container that is up and failing its own check is the one case worth an
 * extra glyph, and until now the table said "ON" and stopped there.
 */
const HEALTH_DOT = {
  healthy: { color: 'success', icon: 'mdi-heart-pulse' },
  unhealthy: { color: 'error', icon: 'mdi-heart-broken' },
  starting: { color: 'warning', icon: 'mdi-timer-sand' },
};

/**
 * Instance id → its health dot, computed once per services reload rather than
 * per cell: the row needs the colour, the glyph and the tooltip, and three
 * lookups through `serviceFor` to draw one icon is three linear scans of the
 * services array for every row on the page.
 */
const healthDots = computed(() => {
  const out = {};
  for (const service of inventory.services) {
    const dot = service.running ? HEALTH_DOT[service.health] : null;
    if (dot) out[service.id] = { ...dot, label: t(`servicesView.health.${service.health}`) };
  }
  return out;
});

/**
 * "5 services · 11 end-of-life", for the row that holds them.
 *
 * Built here rather than in the template because it is the button's accessible
 * name as well as its tooltip, and a sentence assembled twice in two places is
 * a sentence that will come apart in one of them. A tooltip is the only place
 * this text now lives on screen — so the `aria-label` carrying the same words
 * is not a nicety, it is the reader who never hovers.
 */
function countLabel(item) {
  const parts =
    item.kind === 'category'
      ? [t('marketView.serviceCount', { n: item.group.packages.length }), item.group.hidden]
      : [t('marketView.versionCount', { n: item.entry.versions.length }), item.entry.hidden];
  return parts[1] ? `${parts[0]} · ${t('marketView.hiddenCount', { n: parts[1] })}` : parts[0];
}

/** The glyph for each of the three depths. */
const NODE_ICON = {
  category: 'mdi-folder-outline',
  service: 'mdi-cube-outline',
  version: 'mdi-tag-outline',
};

/**
 * Is a missing package the *only* thing stopping the handover?
 *
 * Then the button is the whole sentence and the prose above it was the same
 * fact in another register. Anything else — a service the catalogue has never
 * heard of, a port that cannot be found — has no button, so it keeps its
 * explanation.
 */
const onlyMissingPackages = computed(() => {
  const blockers = market.handover.value?.blockers ?? [];
  const missing = market.handoverMissing.value;
  return (
    blockers.length > 0 &&
    missing.length > 0 &&
    blockers.every((b) => b.kind === 'versionNotInstalled') &&
    missing.every((m) => m.installable)
  );
});

/**
 * Which categories are open.
 *
 * The first one, and only on the first catalogue that arrives. A tree that
 * opened nothing is a column of eight words with the catalogue behind them;
 * one that opened everything is the stacked headings the rail replaced, on
 * twenty-five services. So: the way in is open, the rest is a click.
 *
 * Not re-applied on later loads — a refresh or a change of source rebuilds the
 * groups, and re-opening the first category there would close whatever the
 * reader had opened and move them somewhere else.
 */
const opened = ref([]);
let openedSeeded = false;

watch(
  () => market.grouped.value,
  (groups) => {
    if (openedSeeded || !groups.length) return;
    openedSeeded = true;
    opened.value = [`category:${groups[0].category}`];
  },
  { immediate: true }
);

onMounted(market.load);

/**
 * Re-read the services list whenever the instance table is answered.
 *
 * Every action on a row — the switch, restart, remove — reloads the instance
 * table and replaces the array, so this fires on each of them and on the first
 * load. Hanging it here rather than on each action is what keeps the two in
 * step: the detail sheet reads `running`, and a switch that turned an instance
 * on while this list still said off would open a panel describing a container
 * that stopped being stopped several seconds ago.
 */
watch(market.instances, () => inventory.loadServices(), { immediate: true });

/**
 * A source is an address or a folder, and this used to offer only the folder.
 *
 * The button opened a directory picker and nothing else, so on a machine whose
 * catalogue lives on the network there was no way to say so from this page —
 * the only field that took a URL was the first-run gate, which is seen once and
 * can be skipped. Somebody with the repository address had to find the gate
 * again or edit a file.
 *
 * Both, then, in the order people reach for them: the field is here, the picker
 * is beside it, and the whole setting — with a test that fetches nothing —
 * lives in Settings under the catalogue section.
 */
const address = ref('');
const sourceOpen = ref(false);

async function chooseFolder() {
  const chosen = await open({ directory: true, multiple: false });
  if (typeof chosen === 'string') {
    sourceOpen.value = false;
    await market.refresh(chosen);
  }
}

async function useAddress() {
  if (!address.value) return;
  sourceOpen.value = false;
  await market.refresh(address.value);
}

defineExpose({ market });

/**
 * `admin-uis` → "Admin UIs".
 *
 * The locale keys are camelCase and the category on a package is the directory
 * name, so the two are bridged here rather than by renaming one of them: the
 * directory name is in the published index and in every installed package's
 * path, and the locale key is in two locale files.
 */
const categoryLabel = (category) => {
  const key = category.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const label = t(`serviceCategories.${key}`);
  // vue-i18n returns the key itself when it is missing. A category the repo
  // adds before the locales do should read as its own name, not as a path.
  return label === `serviceCategories.${key}` ? category : label;
};

/**
 * The package's one-line summary in this locale, or null.
 *
 * Null for every package in the official catalogue today — none of the
 * twenty-five fills the field in — so the line is absent rather than empty,
 * and appears on its own the day a source starts publishing them.
 */
const summaryOf = (entry) => entry.summary?.[locale.value] ?? entry.summary?.en ?? null;

const supportColour = (support) =>
  ({ supported: 'success', deprecated: 'warning', eol: 'error' })[support] ?? 'default';
</script>

<template>
  <PageLayout
    top-icon="mdi-storefront-outline"
    :top-title="t('marketView.title')"
    :top-subtitle="t('marketView.subtitle')"
    hide-bar
  >
    <!-- Beside the page title rather than on a bar of its own. The bar carried
         one word and one button, and that word was the page's own name said
         twice — so the row cost a strip of chrome and bought nothing, and the
         two cards under it started a row lower than they had to. -->
    <template #top-append>
      <v-btn
        variant="text"
        prepend-icon="mdi-package-variant-plus"
        class="mr-2"
        @click="authorOpen = true"
      >
        {{ t('authoring.title') }}
      </v-btn>

      <!-- A menu rather than a picker, because a source is an address or a
           folder and this offered only the second. -->
      <v-menu v-model="sourceOpen" :close-on-content-click="false" location="bottom end">
        <template #activator="{ props }">
          <v-btn
            v-bind="props"
            variant="tonal"
            prepend-icon="mdi-swap-horizontal"
            class="mr-5"
            :loading="market.loading.value"
          >
            {{ t('marketView.chooseSource') }}
          </v-btn>
        </template>
        <v-card min-width="420" class="pa-4">
          <div class="text-subtitle-2 mb-2">{{ t('marketView.sourceTitle') }}</div>
          <!-- Which source is in force, and how much of it to believe. It was
               a permanent line above the catalogue; it belongs here, where
               somebody is already asking the question. `signed` is reported
               rather than assumed: nothing verifies a signature yet, and a page
               that implied otherwise would be the wrong kind of quiet. -->
          <div v-if="market.status.value" class="text-caption mb-3">
            <span class="font-mono">{{ market.status.value.sourceLocation ?? '—' }}</span>
            <div :class="market.status.value.signed ? 'text-success' : 'text-medium-emphasis'">
              {{
                t('marketView.sourceCounts', {
                  packages: market.status.value.packages,
                  installed: market.status.value.installed,
                })
              }}
              <template v-if="!market.status.value.signed">
                · {{ t('marketView.unsigned') }}
              </template>
            </div>
          </div>
          <v-text-field
            v-model="address"
            :label="t('catalogueSettings.address')"
            :hint="t('catalogueSettings.addressHint')"
            persistent-hint
            density="compact"
            variant="outlined"
            class="mb-3"
            @keyup.enter="useAddress"
          />
          <div class="d-flex ga-2">
            <v-btn variant="text" prepend-icon="mdi-folder-search-outline" @click="chooseFolder">
              {{ t('catalogueSettings.pickFolder') }}
            </v-btn>
            <v-spacer />
            <v-btn color="primary" variant="flat" :disabled="!address" @click="useAddress">
              {{ t('catalogueSettings.use') }}
            </v-btn>
          </div>
          <!-- The whole setting, with a test that fetches into a scratch
               directory and keeps nothing, lives in Settings. -->
          <div class="text-caption text-medium-emphasis mt-3">
            {{ t('marketView.sourceInSettings') }}
          </div>
        </v-card>
      </v-menu>
    </template>

    <!-- The page's own scroll region, and it had none.
         `PageLayout`'s body is `display: flex; flex-direction: column` with
         `overflow: hidden`, so a page that does not bound itself does not
         overflow — its children **shrink**. That is what was on screen: the
         source line squeezed to a blue sliver a few pixels tall, the catalogue
         running under the bottom edge, and no scrollbar anywhere to reach it.
         The table pages avoid it by handing the data table `height="100%"`
         inside a `min-height: 0` wrapper; this one is expansion panels and a
         table, so it scrolls as one column. -->
    <div class="market-scroll">
      <ErrorAlert :error="market.error.value" class="mb-4" />

      <!-- Never fetched is not the same as empty, and ADR 0011 makes the first
           one the state a fresh install is genuinely in: nothing is embedded, so
           "no services found" would be a lie about why the list is blank. -->
      <v-empty-state
        v-if="!market.fetched.value && !market.loading.value"
        icon="mdi-package-variant-closed"
        :title="t('marketView.noCatalogue')"
        :text="t('marketView.noCatalogueBody')"
      >
        <template #actions>
          <v-btn color="primary" prepend-icon="mdi-swap-horizontal" @click="sourceOpen = true">
            {{ t('marketView.chooseSource') }}
          </v-btn>
        </template>
      </v-empty-state>

      <template v-else>
        <!-- The `.env` → instances.json handover.
           Shown before the catalogue rather than under it, because on a
           workspace that has not migrated the instance list below is empty
           for a reason that has nothing to do with what is installed. What it
           would do is spelled out first: the version a moving tag resolves to
           and the volume that gets adopted are the two facts a person needs
           *before* agreeing, not in a log afterwards. -->
        <v-alert
          v-if="market.handoverPending.value || market.handoverBlocked.value"
          :type="market.handoverBlocked.value ? 'warning' : 'info'"
          variant="tonal"
          class="mb-4"
        >
          <div class="text-subtitle-2 mb-1">{{ t('marketView.handoverTitle') }}</div>

          <!-- One statement of the problem, not two.
               When the only thing standing in the way is a package that is not
               here — which is the ordinary case, and the whole of it on a
               workspace that has never opened the Market — the long refusal and
               the list of what to install said the same fact twice, in two
               registers, one above the other. The refusal is worth reading when
               it is about something a button cannot fix; when it is about a
               missing package the button *is* the sentence. -->
          <template v-if="onlyMissingPackages">
            <div class="text-body-2 mb-2">
              {{ t('marketView.handoverMissing', { n: market.handoverMissing.value.length }) }}
            </div>
            <div class="text-caption font-mono mb-2">
              {{ market.handoverMissing.value.map((m) => `${m.service}@${m.version}`).join(', ') }}
            </div>
            <v-btn
              color="primary"
              variant="flat"
              size="small"
              prepend-icon="mdi-download-outline"
              :loading="market.working.value === 'handover'"
              @click="market.installMissing"
            >
              {{ t('marketView.handoverInstallAll') }}
            </v-btn>
          </template>

          <template v-else>
            <div class="text-body-2 mb-2">
              {{
                market.handoverBlocked.value
                  ? t('marketView.handoverBlocked')
                  : t('marketView.handoverBody', {
                      n: market.handover.value?.instances.length ?? 0,
                    })
              }}
            </div>

            <ul class="text-caption mb-2">
              <li
                v-for="row in market.handover.value?.blockers ?? []"
                :key="row.kind + row.subject"
              >
                {{
                  t(`marketView.handoverNote.${row.kind}`, {
                    subject: row.subject,
                    detail: row.detail,
                  })
                }}
              </li>
              <li v-for="row in market.handover.value?.notes ?? []" :key="row.kind + row.subject">
                {{
                  t(`marketView.handoverNote.${row.kind}`, {
                    subject: row.subject,
                    detail: row.detail,
                  })
                }}
              </li>
            </ul>

            <!-- A package that is not in the catalogue either. No button can
                 answer it — the source is wrong, or `.env` names a version that
                 was never published — so it keeps its own sentence rather than
                 being folded into the generic refusal above. -->
            <ul class="text-caption mb-2">
              <li
                v-for="m in market.handoverMissing.value.filter((m) => !m.installable)"
                :key="`${m.service}@${m.version}`"
              >
                {{
                  t('marketView.handoverNotInCatalogue', {
                    subject: `${m.service}@${m.version}`,
                  })
                }}
              </li>
            </ul>

            <v-btn
              v-if="market.handoverMissing.value.some((m) => m.installable)"
              color="primary"
              variant="tonal"
              size="small"
              class="mb-2"
              prepend-icon="mdi-download-outline"
              :loading="market.working.value === 'handover'"
              @click="market.installMissing"
            >
              {{ t('marketView.handoverInstallAll') }}
            </v-btn>

            <!-- Only beside the button that does it.
                 It used to show while the migration was blocked, where it is an
                 answer to a question nobody has yet — and it named two files,
                 which is how the app protects itself rather than anything the
                 person at the keyboard does. The mechanics stay, in the title,
                 for whoever is actually undoing one. -->
            <div
              v-if="market.handoverPending.value"
              class="text-caption text-medium-emphasis mb-2"
              :title="t('marketView.handoverRevertHow')"
            >
              {{ t('marketView.handoverRevert') }}
            </div>

            <v-btn
              v-if="market.handoverPending.value"
              color="primary"
              variant="flat"
              size="small"
              prepend-icon="mdi-database-arrow-right-outline"
              :loading="market.working.value === 'handover'"
              @click="market.migrate"
            >
              {{ t('marketView.handoverApply') }}
            </v-btn>
          </template>
        </v-alert>

        <!-- Two columns: what could be installed, and what is.
             They were stacked, so on a catalogue of twenty-five services the
             instance table — the half about *this* machine — was a scroll away
             below the fold, and an empty one read as if the page had ended.
             Side by side, installing something and seeing it appear are one
             glance apart. Below `lg` they stack again: two 300px columns are
             worse than one readable one. -->
        <div class="market-columns">
          <!-- The same card the settings panes are built from, because these
               are the same thing: a titled group of related controls. They were
               a bare `h3` over loose content, which on a page that also carries
               an alert and a source line left nothing saying where one list
               ended and the next began. Each scrolls inside its own card rather
               than growing the page — the catalogue is twenty-five services and
               the instance table is however many somebody has made. -->
          <SettingsGroup
            class="market-col"
            icon="mdi-store-outline"
            :title="t('marketView.available')"
            :description="t('marketView.availableDesc')"
          >
            <!-- In the body rather than in the header's append slot. This
                 column is a quarter of the window and the header already
                 carries a title and a description; the switch's label was
                 being squeezed to one letter per line beside them. A row of
                 its own costs nothing here and reads at any width. -->
            <!-- The catalogue had no search: twenty-five services and a
                 hundred versions behind eight collapsed categories, and
                 finding Valkey meant knowing it is filed under `cache`. It
                 matches keywords and capabilities as well as names, because
                 that is what the index publishes them for — MySQL is meant to
                 be findable by typing `database`, and by typing `mariadb`. -->
            <v-text-field
              v-model="market.query.value"
              :label="t('marketView.search')"
              prepend-inner-icon="mdi-magnify"
              density="compact"
              variant="outlined"
              clearable
              hide-details
              class="mb-2"
            />

            <div class="d-flex align-center mb-2">
              <v-switch
                v-model="market.showOlder.value"
                :label="t('marketView.showOlder')"
                density="compact"
                hide-details
                color="primary"
              />
              <!-- Why an unsupported version is published at all, beside the
                   switch that reveals them. It was four lines of prose under
                   the heading, which is a paragraph everybody reads once and
                   then scrolls past forever — and in a quarter-width column it
                   cost more height than the catalogue it explained. On the
                   button, and on its accessible name, so it is a sentence you
                   ask for rather than one you step over. -->
              <v-btn
                icon
                size="small"
                variant="text"
                class="ml-1"
                :aria-label="t('marketView.eolWhy')"
              >
                <v-icon size="small">mdi-information-outline</v-icon>
                <v-tooltip activator="parent" location="bottom" max-width="420">
                  {{ t('marketView.eolWhy') }}
                </v-tooltip>
              </v-btn>
            </div>

            <!-- The catalogue's own shape, as one tree: a category holds
                 services, a service holds versions. It replaced a vertical tab
                 rail over expansion panels — two collapsing mechanisms stacked,
                 each with its own idea of where you were.

                 The `eager` those panels carried is not needed here and is not
                 missing: `VTreeview` writes every descendant into the document
                 and collapses it visually, so all twenty-five services are
                 still findable by a browser and surveyable by a screen reader
                 with the tree shut. Measured, not read about.

                 `prepend-gap="0"` is what makes the indent lines visible, and
                 it is not a spacing choice. The md3 blueprint this app runs
                 sets `VList: { prependGap: 16 }`; `VList` turns any prepend gap
                 into `--v-list-group-prepend: 0px`, `--prepend-width` reads
                 that, and the lines are a grid of
                 `repeat(var(--v-indent-parts), var(--prepend-width))` — so
                 every column is zero pixels wide. The vertical trunk survives,
                 because a 1px left border still paints on a zero-width box; the
                 elbows are `width: 100%` of that box and vanish. One line down
                 the left and nothing else, which is a plausible-looking tree
                 and is why it took a while to see. Rendered both ways in a real
                 browser before this was written. -->
            <div class="pane-scroll">
              <v-treeview
                v-model:opened="opened"
                :items="catalogueTree"
                item-value="id"
                item-title="title"
                density="compact"
                :indent-lines="true"
                :prepend-gap="0"
                open-on-click
                bg-color="transparent"
                class="catalogue-tree"
              >
                <!-- A glyph per depth, as Vuetify's own treeview has. It is not
                   decoration in a column this narrow: it is what tells a
                   category from a service from a version at a glance, now that
                   the words that used to do that have moved into tooltips. -->
                <template #prepend="{ item }">
                  <v-icon size="small" class="mr-2">{{ NODE_ICON[item.kind] }}</v-icon>
                </template>

                <template #title="{ item }">
                  <!-- Three rows out of one slot, because the tree is three
                     depths of one thing and splitting them into components
                     would put the version number and the category label in
                     different files for no reason a reader would find. -->
                  <template v-if="item.kind === 'version'">
                    <span class="font-mono">{{ item.version.version }}</span>
                    <v-chip
                      v-if="item.version.recommended"
                      size="x-small"
                      color="primary"
                      class="ml-2"
                    >
                      {{ t('marketView.recommended') }}
                    </v-chip>
                    <!-- With the date when there is one. "End of life" alone
                         reads the same for a version that stopped being
                         patched two years ago and one that stops next month,
                         and the index has been carrying `eolDate` all along. -->
                    <v-chip
                      size="x-small"
                      variant="tonal"
                      class="ml-2"
                      :color="supportColour(item.version.support)"
                    >
                      {{ t(`marketView.support.${item.version.support}`) }}
                      <template v-if="item.version.eolDate">
                        · {{ item.version.eolDate }}
                      </template>
                    </v-chip>
                    <!-- What it costs to fetch, before deciding to. Also
                         already on the wire and never shown. -->
                    <span
                      v-if="item.version.sizeBytes && !item.version.installed"
                      class="text-caption text-medium-emphasis ml-2"
                    >
                      {{ bytes(item.version.sizeBytes) }}
                    </span>
                  </template>
                  <template v-else-if="item.kind === 'service'">
                    <span class="font-weight-medium">{{ item.title }}</span>
                    <!-- Was a chip reading "Runs more than one version", which in
                       a quarter-width column pushed the service name onto two
                       lines and hyphenated it — "Apache Cassan-dra". The fact
                       is worth one glyph and a hover; the name is worth the
                       room. -->
                    <v-btn
                      v-if="item.entry.multiple"
                      icon
                      size="x-small"
                      variant="text"
                      class="ml-1"
                      :aria-label="t('marketView.multiVersion')"
                      @click.stop
                    >
                      <v-icon size="small">mdi-layers-triple-outline</v-icon>
                      <v-tooltip activator="parent" location="top">
                        {{ t('marketView.multiVersion') }}
                      </v-tooltip>
                    </v-btn>
                  </template>
                  <span v-else>{{ item.title }}</span>

                  <!-- The package's own sentence about why you would install
                       it here. It has been crossing the boundary since the
                       catalogue was ported and had nowhere to land. Under the
                       name rather than beside it: the column is a quarter of
                       the window and a name plus a sentence on one line is a
                       hyphenated name. -->
                  <div
                    v-if="item.kind === 'service' && summaryOf(item.entry)"
                    class="text-caption text-medium-emphasis summary"
                  >
                    {{ summaryOf(item.entry) }}
                  </div>

                  <!-- Beside the name it counts, not at the far end of the row.
                     In the append slot it sat against the right edge with the
                     whole width between it and the thing it was counting, and
                     on a narrow column that gap is most of the row. `.stop`
                     because the row opens on click and a count is not a reason
                     to open anything. -->
                  <v-btn
                    v-if="item.kind !== 'version'"
                    icon
                    size="x-small"
                    variant="text"
                    class="ml-1"
                    :aria-label="countLabel(item)"
                    @click.stop
                  >
                    <v-icon size="small">mdi-information-outline</v-icon>
                    <v-tooltip activator="parent" location="top">
                      {{ countLabel(item) }}
                    </v-tooltip>
                  </v-btn>
                </template>

                <!-- Only a version has anything to act on; a category and a
                   service carry their count beside the name instead. -->
                <template #append="{ item }">
                  <!-- Icons with the label in a tooltip. `icon` is a bare flag
                     and the glyph goes in the slot: Vuetify reads
                     `icon="mdi-…"` only while the slot is empty, and a tooltip
                     *is* slot content — the prop form renders a blank button
                     and nothing complains. `button-icons.spec.js` holds that
                     rule.

                     `.stop` on every one of them: the row opens on click, and
                     a click that installed a package *and* toggled the node it
                     was on would be one act reported as two. -->
                  <template v-if="item.kind === 'version'">
                    <v-btn
                      v-if="!item.version.installed"
                      icon
                      size="small"
                      variant="tonal"
                      :aria-label="t('marketView.install')"
                      :loading="market.working.value === item.id.slice('version:'.length)"
                      @click.stop="market.install(item.entry.service, item.version.version)"
                    >
                      <v-icon size="small">mdi-download-outline</v-icon>
                      <v-tooltip activator="parent" location="top">
                        {{ t('marketView.install') }}
                      </v-tooltip>
                    </v-btn>
                    <template v-else>
                      <v-btn
                        icon
                        size="small"
                        variant="tonal"
                        color="primary"
                        class="mr-2"
                        :aria-label="t('marketView.addInstance')"
                        :loading="market.working.value === item.id.slice('version:'.length)"
                        @click.stop="askToCreate(item.entry.service, item.version.version)"
                      >
                        <v-icon size="small">mdi-plus</v-icon>
                        <v-tooltip activator="parent" location="top">
                          {{ t('marketView.addInstance') }}
                        </v-tooltip>
                      </v-btn>
                      <!-- Refused in Rust while an instance names it; disabled
                         here so the refusal is visible before the click. The
                         tooltip hangs off the span rather than the button: a
                         disabled button emits no pointer events, so
                         `activator="parent"` on it would go quiet in the one
                         state whose sentence is the one worth reading. -->
                      <span>
                        <v-btn
                          icon
                          size="small"
                          variant="text"
                          :disabled="item.version.inUse"
                          :aria-label="t('marketView.uninstall')"
                          @click.stop="market.uninstall(item.entry.service, item.version.version)"
                        >
                          <v-icon size="small">mdi-delete-outline</v-icon>
                        </v-btn>
                        <v-tooltip activator="parent" location="top">
                          {{
                            item.version.inUse ? t('marketView.inUse') : t('marketView.uninstall')
                          }}
                        </v-tooltip>
                      </span>
                    </template>
                  </template>
                </template>
              </v-treeview>
            </div>
          </SettingsGroup>

          <SettingsGroup
            class="market-col"
            icon="mdi-cube-outline"
            :title="t('marketView.instances')"
            :description="t('marketView.instancesDesc')"
          >
            <template #append>
              <v-chip v-if="market.anyInstalled.value" size="small" variant="tonal">
                {{ market.instances.value.length }}
              </v-chip>
            </template>

            <v-empty-state
              v-if="!market.anyInstalled.value"
              icon="mdi-cube-outline"
              :title="t('marketView.noInstances')"
              :text="t('marketView.noInstancesBody')"
            />

            <!-- Eight columns in half a window. Scrolling sideways inside the
                 panel is the honest answer: the alternative is Vuetify's table
                 shrinking every cell until the container names and the domains
                 are ellipses, which loses exactly the two things somebody came
                 to this row to read.

                 `hover` because eight columns is a long way for an eye to
                 travel and the row is the only thing tying the container name
                 at one end to the Remove button at the other.

                 No `fixed-header`, and that is the whole reason the heading row
                 can be transparent. A sticky header needs something opaque
                 behind it or the rows slide visibly under it — so it is one or
                 the other, and this is the one that was asked for. -->
            <v-table v-else hover density="compact" class="instances-table">
              <thead>
                <tr>
                  <th>{{ t('marketView.colInstance') }}</th>
                  <th>{{ t('marketView.colContainer') }}</th>
                  <th>{{ t('marketView.colPorts') }}</th>
                  <th class="text-center">{{ t('marketView.colStopStart') }}</th>
                  <th class="text-center">{{ t('marketView.colRestart') }}</th>
                  <th class="text-center">{{ t('marketView.colOpen') }}</th>
                  <th class="text-center">{{ t('marketView.colStatus') }}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="instance in market.instances.value" :key="instance.id">
                  <td>
                    <span class="font-mono">{{ instance.id }}</span>
                    <!-- The one that answers to the pre-package name, so every
                   project's DB_HOST=stackvo-mysql still reaches something. -->
                    <v-chip v-if="instance.primary" size="x-small" color="primary" class="ml-2">
                      {{ t('marketView.primary') }}
                    </v-chip>
                    <v-chip
                      v-if="!instance.packagePresent"
                      size="x-small"
                      color="error"
                      class="ml-2"
                    >
                      {{ t('marketView.packageMissing') }}
                    </v-chip>
                  </td>
                  <td class="font-mono text-caption">{{ instance.container }}</td>
                  <!-- With the handle, not just the number. `Object.values`
                       threw away the half that says what a port is for, so
                       MinIO read as "9000, 9001" and which of them is the
                       console was a question the table could not answer. -->
                  <td class="font-mono text-caption">
                    <template v-if="Object.keys(instance.ports ?? {}).length">
                      <!-- Along the row rather than stacked down it. A service
                           with two ports — RabbitMQ's broker and its management
                           UI — was the one row in the table twice the height of
                           the others, which reads as the row being special
                           rather than as it having one more number. -->
                      <span v-for="(port, handle, i) in instance.ports" :key="handle">
                        <span v-if="i" class="text-disabled"> · </span>{{ handle }}: {{ port }}
                      </span>
                    </template>
                    <template v-else>—</template>
                  </td>
                  <!-- Stop and start the container, which is a different act
                       from the On/Off beside it: this one leaves the instance
                       enabled and the compose file alone. Start only offers
                       itself for an enabled instance — starting a disabled one
                       would bring up a container the next generate writes out
                       again. -->
                  <td class="text-center">
                    <v-btn
                      v-if="runningOf(instance)"
                      block
                      size="small"
                      color="error"
                      variant="tonal"
                      :aria-label="t('marketView.stop')"
                      :loading="market.working.value === instance.id"
                      @click="market.stop(instance.id)"
                    >
                      <v-icon>mdi-stop</v-icon>
                      <v-tooltip activator="parent" location="top">
                        {{ t('marketView.stop') }}
                      </v-tooltip>
                    </v-btn>
                    <v-btn
                      v-else-if="instance.enabled"
                      block
                      size="small"
                      color="success"
                      variant="tonal"
                      :aria-label="t('marketView.start')"
                      :loading="market.working.value === instance.id"
                      @click="market.start(instance.id)"
                    >
                      <v-icon>mdi-play</v-icon>
                      <v-tooltip activator="parent" location="top">
                        {{ t('marketView.start') }}
                      </v-tooltip>
                    </v-btn>
                  </td>

                  <td class="text-center">
                    <v-btn
                      v-if="runningOf(instance)"
                      block
                      size="small"
                      color="warning"
                      variant="tonal"
                      :aria-label="t('marketView.restart')"
                      :loading="market.working.value === instance.id"
                      @click="market.restart(instance.id)"
                    >
                      <v-icon>mdi-restart</v-icon>
                      <v-tooltip activator="parent" location="top">
                        {{ t('marketView.restart') }}
                      </v-tooltip>
                    </v-btn>
                  </td>

                  <!-- Only for an instance that has a domain *and* is running:
                       a link to a container that is not there is a browser tab
                       showing Traefik's 404. -->
                  <td class="text-center">
                    <v-btn
                      v-if="domainOf(instance) && runningOf(instance)"
                      block
                      size="small"
                      color="primary"
                      variant="tonal"
                      :aria-label="t('marketView.colOpen')"
                      @click="api.openInBrowser(`https://${domainOf(instance)}`)"
                    >
                      <v-icon>mdi-open-in-new</v-icon>
                      <v-tooltip activator="parent" location="top">
                        {{ domainOf(instance) }}
                      </v-tooltip>
                    </v-btn>
                  </td>

                  <!-- On and off, which is not installed and removed. Neither
                       deletes anything: the volume outlives both, and the word
                       on the destructive button is Remove. That is also why
                       this asks nothing before switching off, where the
                       Services page opens a dialog: the command it calls there
                       deletes the volume and the image, and this one does not
                       (ADR 0012). -->
                  <td class="text-center text-no-wrap">
                    <!-- Beside the switch rather than instead of it: ON/OFF is
                         a decision the user made and health is what the
                         container is doing about it, and a healthy service
                         somebody has switched off is a real state. Only ever
                         shown for a running container — see `healthDots`.

                         The tooltip hangs off the span, not off the icon.
                         `v-icon` reads its `icon` prop only while its default
                         slot is empty, exactly as `v-btn` does, so a tooltip
                         placed inside it would render an invisible glyph with
                         a working tooltip — the failure `button-icons.spec.js`
                         exists for, in the one component it does not check. -->
                    <span v-if="healthDots[instance.id]">
                      <v-icon
                        size="small"
                        class="mr-1"
                        :color="healthDots[instance.id].color"
                        :icon="healthDots[instance.id].icon"
                      />
                      <v-tooltip activator="parent" location="top">
                        {{ healthDots[instance.id].label }}
                      </v-tooltip>
                    </span>
                    <v-btn
                      v-if="instance.enabled"
                      size="small"
                      color="success"
                      variant="tonal"
                      :loading="market.working.value === instance.id"
                      @click="market.disable(instance.id)"
                    >
                      <v-icon start>mdi-check-circle</v-icon>{{ t('marketView.enabled') }}
                    </v-btn>
                    <v-btn
                      v-else
                      size="small"
                      color="grey"
                      variant="tonal"
                      :disabled="!instance.packagePresent"
                      :loading="market.working.value === instance.id"
                      @click="market.enable(instance.id)"
                    >
                      <v-icon start>mdi-power</v-icon>{{ t('marketView.disabled') }}
                    </v-btn>
                  </td>
                  <!-- Icons with the label in a tooltip. Four labelled buttons
                       wrapped onto three lines in a five-column table; as
                       glyphs they are one row. `icon` is a bare flag and the
                       glyph goes in the slot — see the note on the version
                       table above for why the prop form would render blank. -->
                  <td class="text-right text-no-wrap">
                    <!-- The same sheet the Services page opens, on the same
                   row: connection string, ports, credentials, dumps and logs.
                   Disabled until the services list has answered, because the
                   sheet reads a row from it and there is nothing to open
                   before then. -->
                    <span>
                      <v-btn
                        icon
                        size="small"
                        variant="text"
                        :disabled="!serviceFor(instance)"
                        :aria-label="t('marketView.detail')"
                        @click="detailTarget = serviceFor(instance)"
                      >
                        <v-icon>mdi-information-outline</v-icon>
                      </v-btn>
                      <v-tooltip activator="parent" location="top">
                        {{ t('marketView.detail') }}
                      </v-tooltip>
                    </span>
                    <!-- The settings are the manifest's, so the button is only
                   enabled while the manifest is: a package whose files have
                   gone has nothing to declare, and an empty form would read as
                   a service with nothing to configure. The span carries the
                   tooltip because a disabled button emits no pointer events. -->
                    <span>
                      <v-btn
                        icon
                        size="small"
                        variant="text"
                        :disabled="!instance.packagePresent"
                        :aria-label="t('marketView.instanceSettings')"
                        @click="openSettings(instance)"
                      >
                        <v-icon>mdi-cog-outline</v-icon>
                      </v-btn>
                      <v-tooltip activator="parent" location="top">
                        {{
                          instance.packagePresent
                            ? t('marketView.instanceSettings')
                            : t('marketView.packageMissing')
                        }}
                      </v-tooltip>
                    </span>
                    <v-btn
                      v-if="!instance.primary"
                      icon
                      size="small"
                      variant="text"
                      :aria-label="t('marketView.makePrimary')"
                      :loading="market.working.value === instance.id"
                      @click="market.promote(instance.id)"
                    >
                      <v-icon>mdi-star-outline</v-icon>
                      <v-tooltip activator="parent" location="top">
                        {{ t('marketView.makePrimary') }}
                      </v-tooltip>
                    </v-btn>
                    <v-btn
                      icon
                      size="small"
                      variant="text"
                      :aria-label="t('marketView.removeInstance')"
                      :loading="market.working.value === instance.id"
                      @click="market.remove(instance.id)"
                    >
                      <v-icon>mdi-delete-outline</v-icon>
                      <v-tooltip activator="parent" location="top">
                        {{ t('marketView.removeInstance') }}
                      </v-tooltip>
                    </v-btn>
                  </td>
                </tr>
              </tbody>
            </v-table>
          </SettingsGroup>
        </div>
      </template>
    </div>

    <!-- Reloaded on apply rather than patched in place: applying rewrites the
         table and recreates the container, so the ports and the enabled flag
         the row shows are both answers this page has just made stale. -->
    <InstanceSettingsSheet
      v-model="settingsSheet"
      :instance="settingsInstance"
      @applied="market.load()"
    />

    <!-- Asked before an instance exists, because that is when a first-boot
         password can still be set. -->
    <InstanceCreateDialog v-model="createDialog" :target="createTarget" @create="doCreate" />

    <!-- One sheet for whichever row is open; `service` is what it reads. -->
    <ServiceDetailSheet
      :service="detailTarget"
      :model-value="!!detailTarget"
      @update:model-value="detailTarget = $event ? detailTarget : null"
    />

    <PackageAuthorDialog v-model="authorOpen" />
  </PageLayout>
</template>

<style scoped>
/* Takes the room the card has and scrolls inside it, rather than letting the
   flex column distribute a fixed height across children that each wanted more.
   `min-height: 0` is the half that is easy to leave out and is the half that
   matters: without it a flex item's floor is its content, so the container
   cannot be smaller than the list and the page grows instead of scrolling. */
.market-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  /* A long address in the source line, or a package name in a narrow window,
     must not widen the page — the horizontal overflow was the other half of
     what was on screen, with the toggle's label cut off at the right edge. */
  overflow-x: hidden;
  padding: 16px;
}

/* Alerts, headings and panels keep their natural height. They are flex items of
   the column above and every one of them was being compressed to fit. */
.market-scroll > * {
  flex: 0 0 auto;
}

/* The catalogue and this machine, side by side.
   `minmax(0, …)` on both tracks rather than `1fr` alone: a grid item's floor is
   its content, so a long container name or a wide table would push the column
   past its share and the page would scroll sideways — the failure this page
   already had once. */
/* A quarter and three quarters. The catalogue is a tree of names and reads fine
   narrow; the instance table is eight columns and is where the work happens. */
.market-columns {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 3fr);
  gap: 24px;
  align-items: start;
}

.market-col {
  min-width: 0;
}

/* The instance table is wider than half a window, so it scrolls inside its own
   panel rather than pushing the page sideways. `min-width` on the table is what
   makes the scroll happen at all: without it the table takes the container's
   width and squeezes the cells instead. */
.instances-table {
  overflow-x: auto;
}
.instances-table :deep(table) {
  min-width: 720px;
}

/* One row, one line — the table already has somewhere for the width to go.
   `stackvo-mongo-express-1-0-2` broke across two lines, the instance beside it
   dropped its chip onto a second, and three column headings wrapped as well,
   so rows stood at three different heights and the eye lost the row it was
   reading. The panel scrolls sideways by design; making the cells take the
   width they need is what gives it something to scroll. */
.instances-table :deep(th),
.instances-table :deep(td) {
  white-space: nowrap;
}

/* Each card keeps its own scroll rather than growing the page. A viewport
   fraction while the columns are stacked, because there the page scrolls and a
   pane with no ceiling would push the other one off the bottom; in the
   two-column layout below it is replaced by "whatever the row leaves". */
.pane-scroll {
  max-height: min(62vh, 720px);
  overflow-y: auto;
}

/* The table scrolls through its own wrapper rather than through a box around
   it, so the horizontal and vertical scroll are the same box. */
.instances-table {
  display: flex;
  flex-direction: column;
  min-height: 0;
  /* Vuetify paints `.v-table` with `rgb(var(--v-theme-surface))`, which inside
     a card that has its own tint reads as a lighter panel floating in it.
     `VTable` has no `bg-color` prop — that is `VList`, which is why the tree
     beside it can ask in markup and this cannot. */
  background: transparent;
}
.instances-table :deep(.v-table__wrapper) {
  min-height: 0;
  overflow: auto;
}
/* Including the heading row. It scrolls with the rows, so there is nothing for
   it to hide and nothing it needs to be opaque for. */
.instances-table :deep(thead th) {
  background: transparent;
}

/* Side by side, the two cards are exactly as tall as the space under the
   source line and each scrolls inside itself — so the page never scrolls and
   the two headers stay level. Stacked, none of this applies: there the page is
   the scroll and a card pinned to the viewport would hide the other one. */
@media (min-width: 1281px) {
  .market-scroll {
    overflow-y: hidden;
    display: flex;
    flex-direction: column;
  }
  .market-columns {
    flex: 1 1 auto;
    min-height: 0;
    /* Overrides the `start` above, which is what a grid does by default here
       and is why the two cards were different heights: each took its content's
       height instead of the row's. */
    align-items: stretch;
  }
  .market-col {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  /* The card's own body, so the scroll box below is a flex child of it rather
     than of the card — a scroll region whose parent has no height of its own
     grows instead of scrolling. */
  .market-col :deep(.group-body) {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }
  .pane-scroll,
  .instances-table {
    flex: 1 1 auto;
    min-height: 0;
    max-height: none;
  }
  /* The scroll box is the only thing in the column that grows.
     Vuetify's `.v-input` is `flex: 1 1 auto`, which is right in the row it was
     written for and means "grow taller" here — so with the tree collapsed the
     search field split the leftover height with it and stood at twice its own
     size, a 40px input in an 80px box with the label adrift in the middle of
     it. It is a control, not a pane: it keeps the height it asks for. */
  .market-col :deep(.group-body) > .v-input {
    flex: 0 0 auto;
  }
}

/* Below this the two columns are narrower than the tables inside them, and a
   catalogue you have to scroll horizontally is worse than one under the other. */
@media (max-width: 1280px) {
  .market-columns {
    grid-template-columns: minmax(0, 1fr);
  }
}

/* A quarter of the window, and a service name plus its chips is wider than
   that. Wrapping rather than truncating: the version number and the support
   chip are the two things a leaf exists to say, and an ellipsis takes the
   second one away. */
.catalogue-tree :deep(.v-list-item-title) {
  white-space: normal;
}

/* The package's own sentence, under the name. Two lines and then an ellipsis:
   the summary is a hint about why you would install this, and a paragraph of it
   in a quarter-width column would push the versions below off the pane. */
.summary {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  line-height: 1.3;
}

/* The count sits at the end of a row that already carries a name and a chip,
   so it gets the smallest share and gives it back when the row needs it. */
.catalogue-tree :deep(.v-list-item__append) {
  flex: 0 1 auto;
  min-width: 0;
}

.font-mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  overflow-wrap: anywhere;
}
</style>
