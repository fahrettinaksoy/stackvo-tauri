<script setup>
import { computed, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { useSharedEnvEditor } from '@/composables/useEnvEditor';
import {
  useStackShape,
  useHostsOverview,
  useProxy,
  TLD_CHOICES,
} from '@/composables/useStackShape';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * How the stack is addressed: the domain suffix, the hosts file, the proxy and
 * the network.
 *
 * The fourth pane out of `Settings.vue` under §14.16, and the first that needed
 * the shared `.env` editor — six panes write one file through one diff, so it
 * is injected rather than constructed here. `useSharedEnvEditor` falls back to
 * its own instance when nothing provided one, which is what lets this be
 * mounted alone in a test.
 *
 * The logic came out one round earlier (§26) and is in `useStackShape`; this is
 * the markup that was left behind.
 */
const { t } = useI18n();
const app = useAppStore();

/**
 * `regenerating` is the parent's operation, not this pane's.
 *
 * The regenerate that applies a routing change runs through the shared stack
 * action and reports in the operation console, so its busy state belongs to the
 * view that owns it. The pane only draws the button.
 */
defineProps({
  regenerating: { type: Boolean, default: false },
});

const env = useSharedEnvEditor();
const {
  effective,
  edit,
  isDefault,
  resetToDefault,
  boolOf,
  setBool,
  dirty,
  changedCount,
  saving,
  saved,
  routingChanged,
  suffixChanged,
} = env;

const {
  suffixLabel,
  suffixTld,
  setSuffix,
  suffixLabelRules,
  suffixTldRules,
  networkRules,
  suffixNeedsHttps,
  valid: shapeValid,
} = useStackShape(env, t);

const {
  hosts,
  fixing: hostsFixing,
  error: hostsError,
  missing: hostsMissing,
  load: loadHosts,
  fix: fixHosts,
} = useHostsOverview();

const { proxy, dashboard: proxyDashboard, load: loadProxy } = useProxy(computed(() => app.tld));

/**
 * Saving and regenerating are the parent's, not this pane's.
 *
 * A save writes `.env` — which five other panes also do — and the regenerate
 * that applies a routing change reports through the shared operation console.
 * Both belong to whoever owns the editor.
 */
const emit = defineEmits(['save', 'regenerate']);

const save = () => emit('save');
const regenerateAfterChange = () => emit('regenerate');

onMounted(() => {
  loadHosts();
  loadProxy();
});
</script>

<template>
  <ErrorAlert v-if="hostsError" :error="hostsError" class="mb-4" />

  <SettingsGroup
    icon="mdi-web"
    :title="t('settings.shape.addressTitle')"
    :description="t('settings.shape.addressDesc')"
  >
    <template #append>
      <v-btn
        v-if="dirty"
        size="small"
        variant="tonal"
        color="primary"
        prepend-icon="mdi-content-save-outline"
        :loading="saving"
        :disabled="!shapeValid"
        @click="save"
      >
        {{ t('settings.save', { count: changedCount }) }}
      </v-btn>
      <v-chip v-else-if="saved" color="success" size="small">
        {{ t('settings.saved') }}
      </v-chip>
    </template>

    <!-- Two fields for one key. The suffix is a namespace and a TLD
         glued together, and only the TLD is the part people mean
         when they ask to swap .loc for .dev — as one input the two
         are indistinguishable from a raw .env row. -->
    <v-row dense align="start">
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="suffixLabel"
          :label="t('settings.shape.suffixLabel')"
          :hint="t('settings.shape.suffixLabelHint')"
          :rules="suffixLabelRules"
          prepend-inner-icon="mdi-tag-outline"
          persistent-hint
          density="comfortable"
          variant="outlined"
          @update:model-value="(v) => setSuffix(v, suffixTld)"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-combobox
          :model-value="suffixTld"
          :items="TLD_CHOICES"
          :label="t('settings.shape.suffixTld')"
          :hint="t('settings.shape.suffixTldHint')"
          :rules="suffixTldRules"
          prepend-inner-icon="mdi-web"
          persistent-hint
          density="comfortable"
          variant="outlined"
          @update:model-value="(v) => setSuffix(suffixLabel, v ?? '')"
        />
      </v-col>
    </v-row>

    <!-- What the two fields actually produce. The suffix is never
         seen on its own: it is always something dot this. -->
    <div class="d-flex align-center ga-2 mt-2 flex-wrap">
      <span class="text-caption text-medium-emphasis">
        {{ t('settings.shape.preview') }}
      </span>
      <v-chip size="small" variant="tonal" prepend-icon="mdi-folder-outline">
        shop.{{ effective('DEFAULT_TLD_SUFFIX') }}
      </v-chip>
      <v-chip size="small" variant="tonal" prepend-icon="mdi-database-outline">
        phpmyadmin.{{ effective('DEFAULT_TLD_SUFFIX') }}
      </v-chip>
      <v-btn
        v-if="!isDefault('DEFAULT_TLD_SUFFIX')"
        size="x-small"
        variant="text"
        prepend-icon="mdi-restore"
        @click="resetToDefault('DEFAULT_TLD_SUFFIX')"
      >
        {{ t('settings.shape.reset') }}
      </v-btn>
    </div>

    <v-alert
      v-if="suffixNeedsHttps"
      type="warning"
      variant="tonal"
      density="comfortable"
      class="mt-3"
      :text="t('settings.shape.suffixHsts')"
    />
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-file-document-outline"
    :title="t('settings.shape.hostsTitle')"
    :description="t('settings.shape.hostsDesc')"
  >
    <template #append>
      <v-btn
        v-if="hostsMissing.length || hosts?.stale.length"
        size="small"
        variant="tonal"
        color="primary"
        prepend-icon="mdi-wrench-outline"
        :loading="hostsFixing"
        @click="fixHosts"
      >
        {{ t('settings.shape.hostsFix') }}
      </v-btn>
      <v-chip v-else size="small" color="success" variant="tonal">
        {{ t('settings.shape.hostsOk') }}
      </v-chip>
    </template>

    <div v-if="!hosts" class="text-caption text-medium-emphasis">
      {{ t('app.loading') }}
    </div>

    <template v-else>
      <div v-for="entry in hosts.entries" :key="entry.domain" class="d-flex align-center ga-2 py-1">
        <v-icon
          size="small"
          :color="entry.configured ? 'success' : 'warning'"
          :icon="entry.configured ? 'mdi-check-circle' : 'mdi-alert-circle'"
        />
        <span class="text-caption break">{{ entry.domain }}</span>
        <!-- Whose line it is decides whether this app may remove it. -->
        <v-chip v-if="!entry.managedByStackvo && entry.configured" size="x-small" variant="tonal">
          {{ t('settings.shape.hostsManual') }}
        </v-chip>
      </div>

      <template v-if="hosts.stale.length">
        <v-divider class="my-3" />
        <div class="text-caption text-medium-emphasis mb-1">
          {{ t('settings.shape.hostsStale') }}
        </div>
        <v-chip v-for="d in hosts.stale" :key="d" size="x-small" variant="tonal" class="mr-1 mb-1">
          {{ d }}
        </v-chip>
      </template>
    </template>
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-transit-connection-variant"
    :title="t('settings.shape.proxyTitle')"
    :description="t('settings.shape.proxyDesc')"
  >
    <template #append>
      <v-chip size="small" :color="proxy?.running ? 'success' : 'error'">
        {{ proxy?.running ? t('engine.running') : t('engine.down') }}
      </v-chip>
    </template>

    <div class="d-flex justify-space-between py-1 ga-4">
      <span class="text-caption text-medium-emphasis">{{ t('about.docker') }}</span>
      <span class="text-caption text-right break">{{ proxy?.image || '—' }}</span>
    </div>
    <div class="d-flex justify-space-between py-1 ga-4">
      <span class="text-caption text-medium-emphasis">
        {{ t('settings.shape.proxyPorts') }}
      </span>
      <span class="text-caption text-right break">
        {{ (proxy?.ports ?? []).map((p) => p.host ?? p.container).join(', ') || '—' }}
      </span>
    </div>

    <!-- The dashboard needs a hosts entry like any other domain, and
         until recently nothing offered one — which is why it is worth
         a button rather than a sentence telling you the address. -->
    <v-btn
      v-if="proxyDashboard"
      size="small"
      variant="tonal"
      prepend-icon="mdi-view-dashboard-outline"
      class="mt-3"
      :disabled="!proxy?.running"
      @click="api.openInBrowser(proxyDashboard)"
    >
      {{ t('settings.shape.proxyDashboard') }}
    </v-btn>
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-lan"
    :title="t('settings.shape.networkTitle')"
    :description="t('settings.shape.networkGroupDesc')"
  >
    <v-row dense>
      <v-col cols="12" md="6">
        <v-text-field
          :model-value="effective('DOCKER_DEFAULT_NETWORK')"
          :label="t('settings.shape.network')"
          :hint="t('settings.shape.networkHint')"
          :rules="networkRules"
          prepend-inner-icon="mdi-lan"
          persistent-hint
          density="comfortable"
          variant="outlined"
          @update:model-value="(v) => edit('DOCKER_DEFAULT_NETWORK', v)"
        >
          <template #append-inner>
            <v-tooltip
              v-if="!isDefault('DOCKER_DEFAULT_NETWORK')"
              :text="t('settings.shape.reset')"
              location="top"
            >
              <template #activator="{ props: tip }">
                <v-btn
                  v-bind="tip"
                  size="x-small"
                  variant="text"
                  icon="mdi-restore"
                  :aria-label="t('settings.shape.reset')"
                  @click="resetToDefault('DOCKER_DEFAULT_NETWORK')"
                />
              </template>
            </v-tooltip>
          </template>
        </v-text-field>
      </v-col>
    </v-row>

    <v-divider class="my-3" />

    <v-switch
      :model-value="boolOf('SSL_ENABLE')"
      :label="t('settings.shape.ssl')"
      :messages="t('settings.shape.sslHint')"
      color="primary"
      density="comfortable"
      hide-details="auto"
      @update:model-value="(v) => setBool('SSL_ENABLE', v)"
    />

    <!-- The generator already reports this — `traefik_routing_warning`
         returns it and it lands in the generate report — but the
         report is not where the decision is made. Every router the
         generator writes targets the `websecure` entry point, and
         that entry point is only written when this is on, so turning
         it off produces a pair of files that disagree and a stack
         where nothing resolves. Said beside the switch that causes
         it. -->
    <v-alert
      v-if="!boolOf('SSL_ENABLE')"
      type="warning"
      variant="tonal"
      density="comfortable"
      class="mt-3"
      :text="t('settings.shape.sslOffBreaksRouting')"
    />

    <!-- Saved, but not yet true of the running stack. The routing
         labels are baked into generated files, so until those are
         rewritten the old suffix is still what Traefik matches on. -->
    <v-alert v-if="routingChanged" type="info" variant="tonal" density="comfortable" class="mt-4">
      <div class="text-body-2">{{ t('settings.shape.thenRegenerate') }}</div>
      <div v-if="suffixChanged" class="text-caption text-medium-emphasis mt-1">
        {{ t('settings.shape.thenCertificates') }}
      </div>
      <template #append>
        <v-btn
          size="small"
          variant="tonal"
          prepend-icon="mdi-cog-sync-outline"
          :loading="regenerating"
          @click="regenerateAfterChange"
        >
          {{ t('settings.shape.regenerate') }}
        </v-btn>
      </template>
    </v-alert>

    <!-- Redirecting to a scheme that is switched off is a dead end,
         so the dependent control cannot be left on by itself. -->
    <v-switch
      :model-value="boolOf('SSL_ENABLE') && boolOf('REDIRECT_TO_HTTPS')"
      :disabled="!boolOf('SSL_ENABLE')"
      :label="t('settings.shape.redirect')"
      :messages="
        boolOf('SSL_ENABLE')
          ? t('settings.shape.redirectHint')
          : t('settings.shape.redirectBlocked')
      "
      color="primary"
      density="comfortable"
      hide-details="auto"
      @update:model-value="(v) => setBool('REDIRECT_TO_HTTPS', v)"
    />
  </SettingsGroup>
</template>
