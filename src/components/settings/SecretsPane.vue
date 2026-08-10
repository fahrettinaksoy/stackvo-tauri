<script setup>
import { ref, computed, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Where each credential lives, and the one button that changes it.
 *
 * The pane exists because the decision has a cost the user has to be told
 * before they make it: `stackvo.sh` reads `.env` line by line and would take
 * `keychain:…` for the password itself. So this is a row per key with a switch,
 * not a "secure my passwords" button — twelve silent decisions is what a sweep
 * would be.
 *
 * It also says what the move does **not** do. The password is still rendered
 * into `generated/docker-compose.dynamic.yml`, as it always has been, and a
 * keystore feature is normally read as meaning otherwise. ADR 0010 carries the
 * reasoning; this pane carries the sentence.
 */
const { t } = useI18n();

const status = ref({ available: false, keys: [] });
const error = ref(null);
const busy = ref(null);
const loading = ref(false);

const moved = computed(() => status.value.keys.filter((k) => k.moved));
const broken = computed(() => moved.value.filter((k) => !k.resolvable));

/** Keys with no value are not offered: there is nothing to move. */
const rows = computed(() => status.value.keys.filter((k) => k.set));

async function load() {
  loading.value = true;
  error.value = null;
  try {
    status.value = await api.secretsStatus();
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

async function toggle(row) {
  busy.value = row.key;
  error.value = null;
  try {
    if (row.moved) await api.secretRestore(row.key);
    else await api.secretMove(row.key);
    await load();
  } catch (e) {
    // Re-read first, then report. A failed move may still have written the
    // keystore entry, so a row showing the old state would be a claim about the
    // disk that nobody checked — but `load()` clears `error` on the way in, so
    // setting it before the re-read wipes the only thing that told the user
    // anything happened. The test that caught this asserted the message was on
    // screen; the pane was showing a fresh, silent, apparently-fine list.
    await load();
    error.value = e;
  } finally {
    busy.value = null;
  }
}

onMounted(load);
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <SettingsGroup
    icon="mdi-key-chain-variant"
    :title="t('settings.secrets.title')"
    :description="t('settings.secrets.description')"
  >
    <!-- Said before the switch, not after it. -->
    <v-alert type="info" variant="tonal" density="comfortable" class="mb-4">
      <div class="text-body-2">{{ t('settings.secrets.whatItDoes') }}</div>
      <div class="text-caption mt-2 text-medium-emphasis">
        {{ t('settings.secrets.stillGenerated') }}
      </div>
      <div class="text-caption mt-1 text-medium-emphasis">
        {{ t('settings.secrets.cliCannotRead') }}
      </div>
    </v-alert>

    <v-alert
      v-if="!status.available"
      type="warning"
      variant="tonal"
      density="comfortable"
      class="mb-4"
      :text="t('settings.secrets.noKeystore')"
    />

    <v-alert v-if="broken.length" type="error" variant="tonal" density="comfortable" class="mb-4">
      <div class="text-body-2">{{ t('settings.secrets.unresolvable') }}</div>
      <div class="text-caption mt-1">{{ broken.map((k) => k.key).join(', ') }}</div>
    </v-alert>

    <v-progress-linear v-if="loading" indeterminate class="mb-2" />

    <div v-if="!rows.length && !loading" class="text-body-2 text-medium-emphasis">
      {{ t('settings.secrets.none') }}
    </div>

    <v-list v-else density="compact" class="bg-transparent">
      <v-list-item v-for="row in rows" :key="row.key" class="px-0">
        <template #prepend>
          <v-icon
            :icon="row.moved ? 'mdi-key-chain-variant' : 'mdi-file-document-outline'"
            :color="row.moved ? (row.resolvable ? 'success' : 'error') : undefined"
            class="mr-3"
          />
        </template>

        <v-list-item-title class="text-body-2 font-monospace">{{ row.key }}</v-list-item-title>
        <v-list-item-subtitle class="text-caption">
          {{ row.moved ? t('settings.secrets.inKeystore') : t('settings.secrets.inEnvFile') }}
        </v-list-item-subtitle>

        <template #append>
          <v-btn
            size="small"
            variant="tonal"
            :color="row.moved ? undefined : 'primary'"
            :loading="busy === row.key"
            :disabled="!status.available || (busy !== null && busy !== row.key)"
            @click="toggle(row)"
          >
            {{ row.moved ? t('settings.secrets.restore') : t('settings.secrets.move') }}
          </v-btn>
        </template>
      </v-list-item>
    </v-list>
  </SettingsGroup>
</template>
