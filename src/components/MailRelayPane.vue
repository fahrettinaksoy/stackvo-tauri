<script setup>
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Where a released message is sent through (M-2).
 *
 * On the Mail page rather than in Settings, because it is only ever configured
 * for a reason that happens here: somebody has a message they need a real
 * person to see, presses Release, and is told no relay is set up. This is the
 * next thing they need, one panel away.
 *
 * ## The password field is write-only
 *
 * It goes to the OS keystore and comes back as a boolean. Left empty, the
 * stored one is untouched; cleared with the button, it is removed. There is no
 * command in this app that reads a stored credential back and this pane is not
 * going to be the first to want one.
 */
const { t } = useI18n();

const relay = ref(null);
const password = ref('');
const recipients = ref('');
const open = ref(false);
const busy = ref(false);
const error = ref(null);

async function load() {
  try {
    relay.value = await api.mailRelayGet();
    recipients.value = (relay.value.allowedRecipients ?? []).join(', ');
  } catch (e) {
    relay.value = null;
  }
}

async function save() {
  busy.value = true;
  error.value = null;
  try {
    relay.value = await api.mailRelaySet(
      {
        enabled: relay.value.enabled,
        host: relay.value.host,
        port: Number(relay.value.port) || 0,
        username: relay.value.username,
        security: relay.value.security,
        from: relay.value.from,
        allowedRecipients: recipients.value
          .split(',')
          .map((a) => a.trim())
          .filter(Boolean),
      },
      // Empty means "leave the stored one alone"; the remove button sends ''.
      password.value ? password.value : null
    );
    password.value = '';
    recipients.value = (relay.value.allowedRecipients ?? []).join(', ');
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <v-card v-if="relay" variant="flat" class="pa-3 mb-3">
    <div class="d-flex align-center ga-2">
      <v-icon size="18" :color="relay.enabled ? 'success' : 'grey'">mdi-send-outline</v-icon>
      <span class="text-subtitle-2">{{ t('mail.relayTitle') }}</span>
      <span class="text-caption text-medium-emphasis">
        {{ relay.enabled ? relay.host : t('mail.relayOff') }}
      </span>
      <v-spacer />
      <v-btn size="small" variant="text" @click="open = !open">
        {{ open ? t('app.close') : t('mail.relayConfigure') }}
      </v-btn>
    </div>

    <div v-if="open" class="mt-3">
      <p class="text-caption text-medium-emphasis mb-3">{{ t('mail.relayExplain') }}</p>
      <ErrorAlert v-if="error" :error="error" class="mb-3" />

      <!-- Said where it matters rather than discovered when a password
           silently does not persist. -->
      <v-alert v-if="!relay.keystore" type="warning" variant="tonal" class="mb-3">
        <div class="text-caption">{{ t('mail.relayNoKeystore') }}</div>
      </v-alert>

      <v-switch
        v-model="relay.enabled"
        color="primary"
        density="compact"
        hide-details
        class="mb-2"
        :label="t('mail.relayEnable')"
      />

      <div class="d-flex ga-2 flex-wrap">
        <v-text-field
          v-model="relay.host"
          :label="t('mail.relayHost')"
          density="compact"
          variant="outlined"
          hide-details
          style="min-width: 220px"
        />
        <v-text-field
          v-model="relay.port"
          :label="t('mail.relayPort')"
          type="number"
          density="compact"
          variant="outlined"
          hide-details
          style="max-width: 110px"
        />
        <v-select
          v-model="relay.security"
          :items="[
            { value: 'starttls', title: 'STARTTLS (587)' },
            { value: 'tls', title: 'TLS (465)' },
            { value: 'none', title: t('mail.relayNoTls') },
          ]"
          :label="t('mail.relaySecurity')"
          density="compact"
          variant="outlined"
          hide-details
          style="max-width: 200px"
        />
      </div>

      <div class="d-flex ga-2 flex-wrap mt-2">
        <v-text-field
          v-model="relay.username"
          :label="t('mail.relayUsername')"
          density="compact"
          variant="outlined"
          hide-details
          autocomplete="off"
          style="min-width: 220px"
        />
        <v-text-field
          v-model="password"
          :label="relay.hasPassword ? t('mail.relayPasswordSet') : t('mail.relayPassword')"
          type="password"
          density="compact"
          variant="outlined"
          hide-details
          autocomplete="off"
          style="min-width: 200px"
        />
      </div>

      <v-text-field
        v-model="relay.from"
        :label="t('mail.relayFrom')"
        :hint="t('mail.relayFromHint')"
        persistent-hint
        density="compact"
        variant="outlined"
        class="mt-2"
      />

      <!-- Empty means anywhere, which is Mailpit's own default and one typo
           away from a real customer. Offered rather than assumed. -->
      <v-text-field
        v-model="recipients"
        :label="t('mail.relayAllowed')"
        :hint="t('mail.relayAllowedHint')"
        persistent-hint
        density="compact"
        variant="outlined"
        class="mt-2"
      />

      <div class="d-flex ga-2 mt-3">
        <v-btn size="small" color="primary" variant="tonal" :loading="busy" @click="save">
          {{ t('site.save') }}
        </v-btn>
        <v-btn
          v-if="relay.hasPassword"
          size="small"
          variant="text"
          :loading="busy"
          @click="
            password = '';
            api.mailRelaySet(relay, '').then(load);
          "
        >
          {{ t('mail.relayForget') }}
        </v-btn>
      </div>

      <p class="text-caption text-medium-emphasis mt-3 mb-0">{{ t('mail.relayRestart') }}</p>
    </div>
  </v-card>
</template>
