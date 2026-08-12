<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { open } from '@tauri-apps/plugin-dialog';
import { useMarket } from '@/composables/useMarket';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

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

const { t } = useI18n();
const market = useMarket();

/**
 * Which category tab is open.
 *
 * A local ref rather than composable state: it is where you happen to be
 * looking, not something about the catalogue, and a page that remembered it
 * across a source change would open on a category that source may not have.
 */
const category = ref(null);

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
 * Keep the selection on something that exists.
 *
 * The groups are rebuilt whenever the catalogue is — a refresh, a different
 * source, the first fetch of all — and a `v-model` pointing at a category that
 * is no longer there leaves the strip with nothing active and the window blank.
 * That is also how the first selection is made: at setup there are no groups,
 * and the first load is what fills them.
 *
 * `immediate` earns nothing today and is kept for the case it describes: a
 * catalogue already in hand when this mounts. Measured rather than assumed —
 * turning it off breaks no test, and removing the watch breaks two.
 */
watch(
  () => market.grouped.value,
  (groups) => {
    if (!groups.length) return;
    if (!groups.some((g) => g.category === category.value)) {
      category.value = groups[0].category;
    }
  },
  { immediate: true }
);

onMounted(market.load);

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
 * `admin-uis` → "Admin UIs", from the list Settings already uses.
 *
 * The keys there are camelCase and the category on a package is the directory
 * name, so the two are bridged here rather than by renaming one of them: the
 * directory name is in the published index and in every installed package's
 * path, and the locale key is in two locale files.
 */
const categoryLabel = (category) => {
  const key = category.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const label = t(`serviceSettings.categories.${key}`);
  // vue-i18n returns the key itself when it is missing. A category the repo
  // adds before the locales do should read as its own name, not as a path.
  return label === `serviceSettings.categories.${key}` ? category : label;
};

const supportColour = (support) =>
  ({ supported: 'success', deprecated: 'warning', eol: 'error' })[support] ?? 'default';
</script>

<template>
  <PageLayout
    top-icon="mdi-storefront-outline"
    :top-title="t('marketView.title')"
    :top-subtitle="t('marketView.subtitle')"
    :bar-title="t('marketView.catalogue')"
  >
    <template #bar-append>
      <!-- A menu rather than a picker, because a source is an address or a
           folder and this offered only the second. -->
      <v-menu v-model="sourceOpen" :close-on-content-click="false" location="bottom end">
        <template #activator="{ props }">
          <v-btn
            v-bind="props"
            variant="text"
            prepend-icon="mdi-swap-horizontal"
            :loading="market.loading.value"
          >
            {{ t('marketView.chooseSource') }}
          </v-btn>
        </template>
        <v-card min-width="420" class="pa-4">
          <div class="text-subtitle-2 mb-2">{{ t('marketView.sourceTitle') }}</div>
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
        <!-- What the catalogue is, and how much of it to believe. `signed` is
           reported rather than assumed: nothing verifies a signature yet, and
           a page that implied otherwise would be the wrong kind of quiet. -->
        <v-alert
          v-if="market.status.value"
          :type="market.status.value.signed ? 'success' : 'info'"
          variant="tonal"
          density="compact"
          class="mb-4"
        >
          {{
            t('marketView.sourceLine', {
              location: market.status.value.sourceLocation ?? '—',
              packages: market.status.value.packages,
              installed: market.status.value.installed,
            })
          }}
          <span v-if="!market.status.value.signed"> · {{ t('marketView.unsigned') }}</span>
        </v-alert>

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
          <section class="market-col">
            <div class="d-flex align-center mb-2">
              <h3 class="text-subtitle-1">{{ t('marketView.available') }}</h3>
              <v-spacer />
              <v-switch
                v-model="market.showOlder.value"
                :label="t('marketView.showOlder')"
                density="compact"
                hide-details
                color="primary"
              />
            </div>

            <!-- Why an unsupported version is published at all, said once and
                 where the switch is, rather than left for somebody to wonder
                 about. A version is withheld from the list, never from the
                 index: a workspace whose `.env` names it has to be able to
                 migrate, and an index that can drop a version is one where
                 somebody's running service loses its source (ADR 0014). -->
            <div class="text-caption text-medium-emphasis mb-3">
              {{ t('marketView.eolWhy') }}
            </div>

            <!-- Categories as a vertical rail beside their contents.
                 Horizontal was the first attempt and it does not fit: eight
                 category names in a column that is half the page either scroll
                 behind arrows — hiding the very thing the tabs were added to
                 make visible — or wrap to three rows, which costs more height
                 than the headings they replaced. Down the side, all eight are
                 legible at once and the list keeps the width. -->
            <div class="catalogue-tabs">
              <v-tabs
                v-model="category"
                direction="vertical"
                density="compact"
                class="category-rail"
              >
                <v-tab
                  v-for="group in market.grouped.value"
                  :key="group.category"
                  :value="group.category"
                  class="justify-start"
                >
                  <span class="text-truncate">{{ categoryLabel(group.category) }}</span>
                  <v-spacer />
                  <v-chip size="x-small" variant="tonal" class="ml-2">
                    {{ group.packages.length }}
                  </v-chip>
                </v-tab>
              </v-tabs>

              <v-tabs-window v-model="category" class="category-body">
                <!-- `eager`, for the reason the panels below are: a category that
                   has never been opened renders nothing, and a page a browser
                   cannot find text in is one you have to already know your way
                   around. The tab still hides it visually — that is what a tab
                   is — but the document carries all twenty-five services. -->
                <v-tabs-window-item
                  v-for="group in market.grouped.value"
                  :key="group.category"
                  :value="group.category"
                  eager
                >
                  <div class="text-caption text-disabled mb-2">
                    {{ t('marketView.serviceCount', { n: group.packages.length }) }}
                    <template v-if="group.hidden">
                      · {{ t('marketView.hiddenCount', { n: group.hidden }) }}
                    </template>
                  </div>

                  <v-expansion-panels variant="accordion">
                    <v-expansion-panel v-for="entry in group.packages" :key="entry.service">
                      <v-expansion-panel-title>
                        <div class="d-flex align-center ga-3 flex-grow-1">
                          <span class="font-weight-medium">{{
                            entry.name?.en ?? entry.service
                          }}</span>
                          <v-chip
                            v-if="entry.multiple"
                            size="x-small"
                            variant="tonal"
                            color="primary"
                          >
                            {{ t('marketView.multiVersion') }}
                          </v-chip>
                          <v-spacer />
                          <span class="text-caption text-medium-emphasis">
                            {{ t('marketView.versionCount', { n: entry.versions.length }) }}
                            <template v-if="entry.hidden">
                              · {{ t('marketView.hiddenCount', { n: entry.hidden }) }}
                            </template>
                          </span>
                        </div>
                      </v-expansion-panel-title>

                      <!-- `eager`, so the versions exist in the document before the panel
               is opened. A collapsed panel that renders nothing is a page a
               screen reader cannot survey and a browser cannot find text in —
               and with twenty-five services, finding is how anybody uses it. -->
                      <v-expansion-panel-text eager>
                        <v-table density="compact">
                          <tbody>
                            <tr v-for="version in entry.versions" :key="version.version">
                              <td class="font-mono">{{ version.version }}</td>
                              <td>
                                <v-chip v-if="version.recommended" size="x-small" color="primary">
                                  {{ t('marketView.recommended') }}
                                </v-chip>
                              </td>
                              <td>
                                <v-chip
                                  size="x-small"
                                  :color="supportColour(version.support)"
                                  variant="tonal"
                                >
                                  {{ t(`marketView.support.${version.support}`) }}
                                </v-chip>
                              </td>
                              <td class="text-right">
                                <v-btn
                                  v-if="!version.installed"
                                  size="small"
                                  variant="tonal"
                                  :loading="
                                    market.working.value === `${entry.service}@${version.version}`
                                  "
                                  @click="market.install(entry.service, version.version)"
                                >
                                  {{ t('marketView.install') }}
                                </v-btn>
                                <template v-else>
                                  <v-btn
                                    size="small"
                                    variant="tonal"
                                    color="primary"
                                    class="mr-2"
                                    :loading="
                                      market.working.value === `${entry.service}@${version.version}`
                                    "
                                    @click="market.create(entry.service, version.version)"
                                  >
                                    {{ t('marketView.addInstance') }}
                                  </v-btn>
                                  <!-- Refused in Rust while an instance names it; disabled
                           here so the refusal is visible before the click. -->
                                  <v-btn
                                    size="small"
                                    variant="text"
                                    :disabled="version.inUse"
                                    :title="version.inUse ? t('marketView.inUse') : undefined"
                                    @click="market.uninstall(entry.service, version.version)"
                                  >
                                    {{ t('marketView.uninstall') }}
                                  </v-btn>
                                </template>
                              </td>
                            </tr>
                          </tbody>
                        </v-table>
                      </v-expansion-panel-text>
                    </v-expansion-panel>
                  </v-expansion-panels>
                </v-tabs-window-item>
              </v-tabs-window>
            </div>
          </section>

          <section class="market-col">
            <div class="d-flex align-center mb-2" style="min-height: 40px">
              <h3 class="text-subtitle-1">{{ t('marketView.instances') }}</h3>
              <v-chip v-if="market.anyInstalled.value" size="x-small" variant="tonal" class="ml-2">
                {{ market.instances.value.length }}
              </v-chip>
            </div>

            <v-empty-state
              v-if="!market.anyInstalled.value"
              icon="mdi-cube-outline"
              :title="t('marketView.noInstances')"
              :text="t('marketView.noInstancesBody')"
            />

            <v-table v-else density="compact">
              <thead>
                <tr>
                  <th>{{ t('marketView.colInstance') }}</th>
                  <th>{{ t('marketView.colContainer') }}</th>
                  <th>{{ t('marketView.colPorts') }}</th>
                  <th>{{ t('marketView.colEnabled') }}</th>
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
                  <td class="font-mono text-caption">
                    {{ Object.values(instance.ports ?? {}).join(', ') || '—' }}
                  </td>
                  <td>
                    <!-- On and off, which is not installed and removed. Neither
                   deletes anything: the volume outlives both, and the word on
                   the destructive button is Remove. -->
                    <v-switch
                      :model-value="instance.enabled"
                      :loading="market.working.value === instance.id"
                      :disabled="!instance.packagePresent"
                      density="compact"
                      hide-details
                      color="primary"
                      @update:model-value="
                        instance.enabled ? market.disable(instance.id) : market.enable(instance.id)
                      "
                    />
                  </td>
                  <td class="text-right">
                    <v-btn
                      v-if="instance.enabled"
                      size="small"
                      variant="text"
                      :loading="market.working.value === instance.id"
                      @click="market.restart(instance.id)"
                    >
                      {{ t('marketView.restart') }}
                    </v-btn>
                    <v-btn
                      v-if="!instance.primary"
                      size="small"
                      variant="text"
                      :loading="market.working.value === instance.id"
                      @click="market.promote(instance.id)"
                    >
                      {{ t('marketView.makePrimary') }}
                    </v-btn>
                    <v-btn
                      size="small"
                      variant="text"
                      :loading="market.working.value === instance.id"
                      @click="market.remove(instance.id)"
                    >
                      {{ t('marketView.removeInstance') }}
                    </v-btn>
                  </td>
                </tr>
              </tbody>
            </v-table>
          </section>
        </div>
      </template>
    </div>
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
.market-columns {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 24px;
  align-items: start;
}

.market-col {
  min-width: 0;
}

/* Below this the two columns are narrower than the tables inside them, and a
   catalogue you have to scroll horizontally is worse than one under the other. */
@media (max-width: 1280px) {
  .market-columns {
    grid-template-columns: minmax(0, 1fr);
  }
}

/* The category rail and what it selects, side by side. */
.catalogue-tabs {
  display: flex;
  gap: 12px;
  align-items: start;
}

/* Wide enough for the longest category name — "Developer tools" — and fixed, so
   the list beside it does not move when the selection does. */
.category-rail {
  flex: 0 0 168px;
}

/* `min-width: 0` again, for the third time on this page and the same reason:
   a flex item's floor is its content, and a version table is wider than its
   share. Without it the rail is squeezed instead of the table scrolling. */
.category-body {
  flex: 1 1 auto;
  min-width: 0;
}

/* Left-aligned rather than centred: these are a list of names read down the
   side, and centring turns a scannable column into a ragged one. */
.category-rail :deep(.v-tab) {
  justify-content: flex-start;
  text-transform: none;
  letter-spacing: normal;
  min-height: 36px;
}

.font-mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  overflow-wrap: anywhere;
}
</style>
