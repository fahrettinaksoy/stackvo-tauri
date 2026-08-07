<script setup>
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useInventoryStore } from '@/stores/inventory';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ServiceSettingsSheet from '@/components/ServiceSettingsSheet.vue';

/**
 * Every shared service, grouped by the category the catalog already assigns.
 *
 * Tenth pane out of `Settings.vue` under §14.16. It edits nothing itself — the
 * per-service settings live in `ServiceSettingsSheet` — so this is the index
 * that opens it, and the one place the grouping is decided.
 */
const { t } = useI18n();
const inventory = useInventoryStore();

const serviceTab = ref('all');
const sheetService = ref(null);
const sheetOpen = ref(false);

const serviceCategories = computed(() => {
  const seen = [...new Set((inventory.services ?? []).map((s) => s.category).filter(Boolean))];
  return ['all', ...seen.sort()];
});

const servicesInTab = computed(() => {
  const list = [...(inventory.services ?? [])].sort((a, b) => a.id.localeCompare(b.id));
  return serviceTab.value === 'all' ? list : list.filter((s) => s.category === serviceTab.value);
});

/**
 * A category's name in the reader's language.
 *
 * Spelled out rather than assembled from the slug, because the i18n check only
 * sees literal `t('…')` calls — a key built by interpolation reads as
 * unreachable there and as a missing translation here. The default returns the
 * slug itself, so a category added to the contract upstream shows its own name
 * instead of a blank chip.
 */
function categoryLabel(category) {
  switch (category) {
    case 'all':
      return t('serviceSettings.all');
    case 'databases':
      return t('serviceSettings.categories.databases');
    case 'cache':
      return t('serviceSettings.categories.cache');
    case 'queue':
      return t('serviceSettings.categories.queue');
    case 'search':
      return t('serviceSettings.categories.search');
    case 'monitoring':
      return t('serviceSettings.categories.monitoring');
    case 'devtools':
      return t('serviceSettings.categories.devtools');
    case 'adminUis':
      return t('serviceSettings.categories.adminUis');
    default:
      return category;
  }
}

function openService(service) {
  sheetService.value = service;
  sheetOpen.value = true;
}
</script>

<template>
  <SettingsGroup
    icon="mdi-cube-outline"
    :title="t('serviceSettings.pick')"
    :description="t('serviceSettings.desc')"
  >
    <div class="d-flex flex-column flex-md-row ga-4 align-start">
      <div class="services-main">
        <v-list
          v-if="servicesInTab.length"
          density="comfortable"
          class="pa-0 service-list"
          bg-color="transparent"
        >
          <v-list-item
            v-for="s in servicesInTab"
            :key="s.id"
            :title="s.id"
            :subtitle="
              s.version ? `${categoryLabel(s.category)} · ${s.version}` : categoryLabel(s.category)
            "
            rounded="lg"
            class="mb-1"
            @click="openService(s)"
          >
            <template #prepend>
              <v-avatar
                rounded="lg"
                size="32"
                :color="s.running ? 'success' : s.enabled ? 'warning' : 'surface-variant'"
              >
                <v-icon size="18" icon="mdi-cube-outline" />
              </v-avatar>
            </template>
            <template #append>
              <v-chip v-if="!s.enabled" size="x-small" variant="tonal" class="mr-2">
                {{ t('serviceSettings.off') }}
              </v-chip>
              <v-icon icon="mdi-chevron-right" />
            </template>
          </v-list-item>
        </v-list>

        <v-alert
          v-else
          type="info"
          variant="tonal"
          density="comfortable"
          :text="t('serviceSettings.empty')"
        />
      </div>

      <!-- The filter reads as a sidebar to the thing it filters, so
             it sits beside the list rather than above it. Sized to the
             labels rather than to a share of the row: they are seven
             short words, and a grid fraction gave them a third of the
             pane to sit in. -->
      <v-tabs
        v-model="serviceTab"
        :direction="$vuetify.display.mdAndUp ? 'vertical' : 'horizontal'"
        density="compact"
        bg-color="transparent"
        class="service-tabs"
      >
        <v-tab v-for="c in serviceCategories" :key="c" :value="c">
          {{ categoryLabel(c) }}
        </v-tab>
      </v-tabs>
    </div>
  </SettingsGroup>

  <!-- ---- doctor ----------------------------------------------------- -->
  <!-- Diagnosis next to its repair. The findings here used to surface
       one failed compose up at a time, each as an error about itself:
       "address already in use" with no word on by what. -->
  <!-- ---- workspace and stack control ------------------------------- -->

  <ServiceSettingsSheet
    v-model="sheetOpen"
    :service="sheetService"
    @applied="inventory.loadServices()"
  />
</template>
