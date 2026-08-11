<script setup>
import { onMounted } from 'vue';
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

onMounted(market.load);

/** Ask for a directory: an offline bundle, or a checkout of the packages repo. */
async function chooseSource() {
  const chosen = await open({ directory: true, multiple: false });
  if (typeof chosen === 'string') await market.refresh(chosen);
}

defineExpose({ market });

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
      <v-btn
        variant="text"
        prepend-icon="mdi-folder-search-outline"
        :loading="market.loading.value"
        @click="chooseSource"
      >
        {{ t('marketView.chooseSource') }}
      </v-btn>
    </template>

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
        <v-btn color="primary" prepend-icon="mdi-folder-search-outline" @click="chooseSource">
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

      <v-expansion-panels variant="accordion" class="mb-6">
        <v-expansion-panel v-for="entry in market.visible.value" :key="entry.service">
          <v-expansion-panel-title>
            <div class="d-flex align-center ga-3 flex-grow-1">
              <span class="font-weight-medium">{{ entry.name?.en ?? entry.service }}</span>
              <v-chip size="x-small" variant="tonal">{{ entry.category }}</v-chip>
              <v-chip v-if="entry.multiple" size="x-small" variant="tonal" color="primary">
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
                    <v-chip size="x-small" :color="supportColour(version.support)" variant="tonal">
                      {{ t(`marketView.support.${version.support}`) }}
                    </v-chip>
                  </td>
                  <td class="text-right">
                    <v-btn
                      v-if="!version.installed"
                      size="small"
                      variant="tonal"
                      :loading="market.working.value === `${entry.service}@${version.version}`"
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
                        :loading="market.working.value === `${entry.service}@${version.version}`"
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

      <h3 class="text-subtitle-1 mb-2">{{ t('marketView.instances') }}</h3>

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
              <v-chip v-if="!instance.packagePresent" size="x-small" color="error" class="ml-2">
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

      <!-- Said once, plainly, rather than implied by a missing button: an
           instance is a decision recorded, and nothing renders it yet. -->
    </template>
  </PageLayout>
</template>

<style scoped>
.font-mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}
</style>
