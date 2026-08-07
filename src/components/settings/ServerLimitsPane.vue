<script setup>
import { computed, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useSharedEnvEditor } from '@/composables/useEnvEditor';
import { useVersionChoices } from '@/composables/useCatalog';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ServerDirectivesPane from '@/components/settings/ServerDirectivesPane.vue';

/**
 * The request limits every generated server config carries, and which servers
 * can take extra directives at all.
 *
 * Ninth pane out of `Settings.vue` under §14.16. The directives editor below it
 * came out first (§24) because it owned its own file; this half is the one that
 * drives the shared `.env` editor, which is why it had to wait for
 * `useEnvEditor`.
 */
const { t } = useI18n();

const env = useSharedEnvEditor();
const { effective, edit, onOff, setOnOff, dirty, changedCount, saving, saved } = env;
const { servers, loadCatalog } = useVersionChoices(env);

const emit = defineEmits(['save', 'directives-saved']);
const save = () => emit('save');

const NGINX_FIELDS = [
  { key: 'SERVER_MAX_BODY_SIZE', kind: 'size', icon: 'mdi-upload' },
  { key: 'SERVER_CLIENT_BODY_TIMEOUT', kind: 'seconds', icon: 'mdi-timer-sand' },
  { key: 'SERVER_KEEPALIVE_TIMEOUT', kind: 'seconds', icon: 'mdi-lan-connect' },
  { key: 'SERVER_FASTCGI_CONNECT_TIMEOUT', kind: 'seconds', icon: 'mdi-transit-connection' },
  { key: 'SERVER_FASTCGI_SEND_TIMEOUT', kind: 'seconds', icon: 'mdi-upload-network' },
  { key: 'SERVER_FASTCGI_TIMEOUT', kind: 'seconds', icon: 'mdi-timer-outline' },
];
const NGINX_SWITCHES = [
  { key: 'SERVER_TCP_NODELAY', on: 'on', off: 'off' },
  { key: 'SERVER_GZIP', on: 'on', off: 'off' },
];

const gzipOn = computed(() => onOff('SERVER_GZIP'));

/**
 * Which servers have a generated config file, and so can take directives.
 *
 * FrankenPHP was `false` here and it was simply wrong: it writes a `Caddyfile`
 * exactly as caddy does. Sitting greyed out beside Apache and Swoole — whose
 * exclusion the note underneath explains — made an oversight look like a
 * decision somebody had made.
 *
 * Not the same question as the request limits above, which reach nginx and
 * caddy only; the note says which is which rather than this map pretending one
 * flag answers both.
 */
const SERVER_SUPPORT = {
  nginx: true,
  caddy: true,
  frankenphp: true,
  apache: false,
  swoole: false,
};

// Vuetify's rule shape: `true` when valid, a message when not. Both accept an
// empty value — a cleared field falls back to the shipped default rather than
// being a validation failure.
const sizeRules = [
  (v) =>
    !String(v ?? '').trim() ||
    /^\d+[kKmMgG]?$/.test(String(v).trim()) ||
    t('settings.servers.sizeInvalid'),
];
const secondsRules = [
  (v) =>
    !String(v ?? '').trim() ||
    /^\d+$/.test(String(v).trim()) ||
    t('settings.servers.secondsInvalid'),
];

onMounted(loadCatalog);
</script>

<template>
  <SettingsGroup
    icon="mdi-web-box"
    :title="t('settings.servers.limits')"
    :description="t('settings.servers.limitsDesc')"
  >
    <template #append>
      <v-btn
        v-if="dirty"
        size="small"
        variant="tonal"
        color="primary"
        prepend-icon="mdi-content-save-outline"
        :loading="saving"
        @click="save"
      >
        {{ t('settings.save', { count: changedCount }) }}
      </v-btn>
      <v-chip v-else-if="saved" color="success" size="small">
        {{ t('settings.saved') }}
      </v-chip>
    </template>

    <v-row dense>
      <v-col v-for="f in NGINX_FIELDS" :key="f.key" cols="12" sm="6" md="4">
        <v-text-field
          :model-value="effective(f.key)"
          :label="t(`settings.servers.field.${f.key}`)"
          :rules="f.kind === 'size' ? sizeRules : secondsRules"
          :suffix="f.kind === 'seconds' ? 's' : undefined"
          :prepend-inner-icon="f.icon"
          density="comfortable"
          variant="outlined"
          hide-details="auto"
          @update:model-value="(v) => edit(f.key, v)"
        />
      </v-col>
    </v-row>

    <v-divider class="my-4" />

    <div class="d-flex ga-6 flex-wrap">
      <v-switch
        v-for="sw in NGINX_SWITCHES"
        :key="sw.key"
        :model-value="onOff(sw.key)"
        :label="t(`settings.servers.field.${sw.key}`)"
        color="primary"
        density="comfortable"
        hide-details
        @update:model-value="(v) => setOnOff(sw.key, v)"
      />
    </div>

    <!-- Only meaningful once compression is on, so it appears with
           it rather than sitting greyed out asking to be understood. -->
    <v-row v-if="gzipOn" dense class="mt-2">
      <v-col cols="12" sm="4">
        <v-text-field
          :model-value="effective('SERVER_GZIP_COMP_LEVEL')"
          :label="t('settings.servers.field.SERVER_GZIP_COMP_LEVEL')"
          type="number"
          min="1"
          max="9"
          density="comfortable"
          variant="outlined"
          hide-details="auto"
          @update:model-value="(v) => edit('SERVER_GZIP_COMP_LEVEL', v)"
        />
      </v-col>
      <v-col cols="12" sm="8">
        <v-text-field
          :model-value="effective('SERVER_GZIP_TYPES')"
          :label="t('settings.servers.field.SERVER_GZIP_TYPES')"
          :hint="t('settings.servers.gzipTypesHint')"
          persistent-hint
          density="comfortable"
          variant="outlined"
          @update:model-value="(v) => edit('SERVER_GZIP_TYPES', v)"
        />
      </v-col>
    </v-row>

    <!-- The half people find last. An upload dies at whichever limit
           is lowest, and PHP's are per project — raising one here and
           not the other is the failure this note exists to prevent. -->
    <v-alert
      type="info"
      variant="tonal"
      density="comfortable"
      class="mt-3"
      :text="t('settings.servers.phpNote')"
    />
  </SettingsGroup>

  <!-- The directives editor reports its own save upward; the "regenerate to
       apply" notice belongs to whoever owns the shared editor. -->
  <ServerDirectivesPane @saved="(keys) => emit('directives-saved', keys)" />

  <SettingsGroup
    icon="mdi-server-network"
    :title="t('settings.servers.applies')"
    :description="t('settings.servers.appliesDesc')"
  >
    <div class="d-flex ga-2 flex-wrap">
      <v-chip
        v-for="srv in servers"
        :key="srv"
        size="small"
        variant="tonal"
        :prepend-icon="SERVER_SUPPORT[srv] ? 'mdi-check' : 'mdi-minus'"
        :color="SERVER_SUPPORT[srv] ? 'primary' : undefined"
      >
        {{ srv }}
      </v-chip>
    </div>
    <div class="text-caption text-medium-emphasis mt-2">
      {{ t('settings.servers.supportNote') }}
    </div>
  </SettingsGroup>

  <!-- ---- services --------------------------------------------------- -->
</template>
