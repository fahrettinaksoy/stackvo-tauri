<script setup>
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useAppStore } from '@/stores/app';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The screen the app opens on when something it depends on is missing.
 *
 * It replaces a gate that asked only "where is the checkout?", which was one of
 * six answers the app needs and the only one it bothered to ask for. The rest —
 * a running daemon, a compose plugin new enough for profiles, the shared
 * network, a projects directory, a shell — were discovered later, one failed
 * button at a time, each with a message about itself rather than about what to
 * do.
 *
 * Every row states what is wrong, what the machine actually said, and what to
 * do about it; the ones the app can settle itself carry a button that does.
 */
const { t } = useI18n();
const app = useAppStore();

const busy = ref(null);
const rechecking = ref(false);

const ICONS = {
  workspace: 'mdi-folder-search-outline',
  engine: 'mdi-docker',
  compose: 'mdi-layers-outline',
  network: 'mdi-lan',
  projects: 'mdi-folder-multiple-outline',
  bash: 'mdi-console',
};

const STATE = {
  ok: { icon: 'mdi-check-circle', color: 'success' },
  warn: { icon: 'mdi-alert', color: 'warning' },
  fail: { icon: 'mdi-close-circle', color: 'error' },
  unknown: { icon: 'mdi-help-circle-outline', color: 'grey' },
};

const requirements = computed(() => app.preflight?.requirements ?? []);
const os = computed(() => app.preflight?.os ?? 'linux');
const blocking = computed(() => requirements.value.filter((r) => r.state === 'fail'));

/**
 * A requirement that cannot be tested is not a requirement that failed — the
 * network cannot be looked for while the daemon is down. Say so rather than
 * offering a fix for it.
 */
const isBlocked = (requirement) => requirement.state === 'unknown';

async function fix(id) {
  busy.value = id;
  try {
    await app.fixRequirement(id);
  } finally {
    busy.value = null;
  }
}

async function recheck() {
  rechecking.value = true;
  try {
    await app.checkRequirements();
  } finally {
    rechecking.value = false;
  }
}
</script>

<template>
  <v-container class="pa-6">
    <v-card max-width="720" class="mx-auto mt-6">
      <v-card-item>
        <template #prepend>
          <v-icon size="32" color="warning">mdi-clipboard-alert-outline</v-icon>
        </template>
        <v-card-title>{{ t('preflight.title') }}</v-card-title>
        <v-card-subtitle>
          {{ t('preflight.subtitle', { count: blocking.length }) }}
        </v-card-subtitle>
        <template #append>
          <v-btn
            icon
            variant="text"
            :loading="rechecking"
            :aria-label="t('preflight.recheck')"
            @click="recheck"
          >
            <v-icon>mdi-refresh</v-icon>
            <v-tooltip activator="parent">{{ t('preflight.recheck') }}</v-tooltip>
          </v-btn>
        </template>
      </v-card-item>

      <v-card-text>
        <ErrorAlert :error="app.error" type="error" class="mb-4" />

        <div v-for="r in requirements" :key="r.id" class="requirement">
          <v-icon :color="STATE[r.state].color" size="20" class="mt-1">
            {{ STATE[r.state].icon }}
          </v-icon>

          <div class="min-w-0">
            <div class="d-flex align-center ga-2">
              <v-icon size="16" class="text-medium-emphasis">{{ ICONS[r.id] }}</v-icon>
              <span class="text-body-2 font-weight-medium">{{ t(`preflight.${r.id}`) }}</span>
            </div>

            <!-- The machine's own words: a version, a path, the daemon's error. -->
            <div v-if="r.detail" class="text-caption text-medium-emphasis detail">
              {{ r.detail }}
            </div>

            <!-- Instructions only where they are needed, and only the ones that
                 apply to this operating system. -->
            <div v-if="r.state === 'fail'" class="text-caption mt-1">
              {{ t(`preflight.${r.id}Hint.${os}`) }}
            </div>
            <div v-else-if="isBlocked(r)" class="text-caption text-medium-emphasis mt-1">
              {{ t('preflight.blocked') }}
            </div>
          </div>

          <v-spacer />

          <v-btn
            v-if="r.state === 'fail' && r.fixable"
            size="small"
            color="primary"
            variant="flat"
            :loading="busy === r.id"
            @click="fix(r.id)"
          >
            {{ t(`preflight.${r.id}Action`) }}
          </v-btn>
        </div>
      </v-card-text>

      <v-divider />

      <v-card-actions>
        <v-btn
          variant="text"
          prepend-icon="mdi-book-open-variant"
          @click="openUrl('https://stackvo.github.io/stackvo')"
        >
          {{ t('app.documentation') }}
        </v-btn>
        <v-spacer />
        <v-btn variant="text" :loading="rechecking" @click="recheck">
          {{ t('preflight.recheck') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-container>
</template>

<style scoped>
.requirement {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 10px 0;
}

.requirement + .requirement {
  border-top: thin solid rgba(var(--v-border-color), calc(var(--v-border-opacity) / 2));
}

.detail {
  word-break: break-all;
}

.min-w-0 {
  min-width: 0;
}
</style>
