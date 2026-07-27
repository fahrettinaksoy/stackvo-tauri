<script setup>
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';

/**
 * Asked once per close, unless the user says not to ask again.
 *
 * In the front end rather than a native dialog because it carries "remember
 * this", and a remembered answer is the same preference the Settings page
 * edits — one control rather than two that can disagree about the current
 * value.
 */
const { t } = useI18n();

const open = defineModel({ type: Boolean, default: false });
const remember = ref(false);
const busy = ref(null);

const CHOICES = [
  { action: 'tray', icon: 'mdi-tray-arrow-down', label: 'close.tray', hint: 'close.trayHint' },
  { action: 'quit', icon: 'mdi-window-close', label: 'close.quit', hint: 'close.quitHint' },
  {
    action: 'stopAndQuit',
    icon: 'mdi-stop-circle-outline',
    label: 'close.stopAndQuit',
    hint: 'close.stopAndQuitHint',
    color: 'error',
  },
];

async function choose(action) {
  busy.value = action;
  try {
    await api.windowCloseAction(action, remember.value);
  } finally {
    // Only reached if the window survived — 'tray' hides it, the other two
    // exit and this component goes with the process.
    busy.value = null;
    open.value = false;
  }
}
</script>

<template>
  <v-dialog v-model="open" max-width="480" persistent>
    <v-card>
      <v-card-item>
        <v-card-title class="text-body-1">{{ t('close.title') }}</v-card-title>
        <v-card-subtitle>{{ t('close.subtitle') }}</v-card-subtitle>
      </v-card-item>

      <v-list density="comfortable" class="py-0">
        <v-list-item
          v-for="choice in CHOICES"
          :key="choice.action"
          :prepend-icon="choice.icon"
          :disabled="!!busy"
          :base-color="choice.color"
          @click="choose(choice.action)"
        >
          <v-list-item-title>{{ t(choice.label) }}</v-list-item-title>
          <v-list-item-subtitle class="text-caption">{{ t(choice.hint) }}</v-list-item-subtitle>
          <template #append>
            <v-progress-circular v-if="busy === choice.action" size="18" width="2" indeterminate />
          </template>
        </v-list-item>
      </v-list>

      <v-divider />

      <v-card-actions>
        <v-checkbox
          v-model="remember"
          :label="t('close.remember')"
          density="compact"
          hide-details
          :disabled="!!busy"
        />
        <v-spacer />
        <v-btn variant="text" :disabled="!!busy" @click="open = false">
          {{ t('app.cancel') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
