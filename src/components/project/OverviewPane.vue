<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { useCopyTick } from '@/composables/useCopyTick';

/**
 * What this project *is*: its domain, its runtime, and the addresses it answers
 * on — the first thing the Configuration section shows.
 *
 * Read-only, and everything comes from the project the view already loaded. The
 * two things it cannot do itself are opening the settings sheet and the hosts
 * repair dialog, both of which are the view's own dialogs, so it asks.
 */
const props = defineProps({
  project: { type: Object, default: null },
  details: { type: Object, default: null },
});

const emit = defineEmits(['settings', 'fixHosts']);

const { t } = useI18n();
const { copied, copy } = useCopyTick();

const manifest = computed(() => props.project?.manifest);

// Both schemes, because which one answers is the whole question a user has
// when a certificate is missing: `http://` works and `https://` does not.
const httpUrl = computed(() => (props.project?.domain ? `http://${props.project.domain}` : null));
const httpsUrl = computed(() => (props.project?.domain ? `https://${props.project.domain}` : null));
</script>

<template>
  <v-card variant="flat" class="pane">
    <div class="d-flex align-center ga-2 mb-4">
      <div class="section-head">
        <v-icon size="18" class="mr-2">mdi-folder-cog</v-icon>{{ t('projectDetail.configuration') }}
      </div>
      <v-spacer />
      <!-- Every value read below is a field in stackvo.json, so the way
           to change one belongs beside them rather than only in the raw
           JSON pane further down the rail. -->
      <v-btn
        size="small"
        variant="tonal"
        prepend-icon="mdi-tune-variant"
        @click="emit('settings')"
        >{{ t('projectSettings.open') }}</v-btn
      >
    </div>

    <v-row>
      <v-col cols="12" md="4">
        <div class="field">
          <span class="field-key">{{ t('projectsView.colDomain') }}</span>
          <a
            v-if="httpUrl"
            class="field-link"
            @click="project.domainConfigured && api.openInBrowser(httpsUrl)"
          >
            {{ project.domain }}
          </a>
          <span v-else class="field-val">—</span>
        </div>
        <div v-if="manifest?.php" class="field">
          <span class="field-key">{{ t('newProject.phpVersion') }}</span>
          <span class="field-val">PHP {{ manifest.php.version }}</span>
        </div>
        <div v-if="manifest?.node" class="field">
          <span class="field-key">{{ t('newProject.nodeVersion') }}</span>
          <span class="field-val">Node {{ manifest.node.version }}</span>
        </div>
        <div class="field">
          <span class="field-key">{{ t('projectDetail.containerPath') }}</span>
          <code class="field-mono">/var/www/html</code>
          <v-btn
            icon
            :aria-label="t('a11y.copy')"
            size="x-small"
            variant="text"
            @click="copy('/var/www/html', 'cpath')"
          >
            <v-icon>{{ copied === 'cpath' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
            <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
          </v-btn>
        </div>
        <div class="field">
          <span class="field-key">{{ t('projectDetail.accessHttp') }}</span>
          <a
            v-if="httpUrl"
            class="field-link"
            @click="project.domainConfigured && api.openInBrowser(httpUrl)"
            >{{ httpUrl }}</a
          >
          <span v-else class="field-val">—</span>
        </div>
      </v-col>

      <v-col cols="12" md="4">
        <div class="field">
          <span class="field-key">{{ t('projectDetail.sslStatus') }}</span>
          <span class="field-val text-success">
            <v-icon size="14" color="success">mdi-lock</v-icon>
            {{ t('projectDetail.sslEnabled') }}
          </span>
        </div>
        <div class="field">
          <span class="field-key">{{ t('projectsView.colServer') }}</span>
          <span class="field-val">{{ manifest?.server || '—' }}</span>
        </div>
        <div class="field">
          <span class="field-key">{{ t('projectDetail.hostPath') }}</span>
          <code class="field-mono">{{ project.path }}</code>
          <v-btn
            icon
            :aria-label="t('a11y.copy')"
            size="x-small"
            variant="text"
            @click="copy(project.path, 'hpath')"
          >
            <v-icon>{{ copied === 'hpath' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
            <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
          </v-btn>
        </div>
      </v-col>

      <v-col cols="12" md="4">
        <div class="field">
          <span class="field-key">{{ t('projectDetail.type') }}</span>
          <span class="field-val">
            <v-icon size="14">mdi-cog</v-icon> {{ t('projectsView.default') }}
          </span>
        </div>
        <div v-if="manifest?.documentRoot" class="field">
          <span class="field-key">{{ t('newProject.documentRoot') }}</span>
          <code class="field-mono">{{ manifest.documentRoot }}</code>
          <v-btn
            icon
            :aria-label="t('a11y.copy')"
            size="x-small"
            variant="text"
            @click="copy(manifest.documentRoot, 'droot')"
          >
            <v-icon>{{ copied === 'droot' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
            <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
          </v-btn>
        </div>
        <div class="field">
          <span class="field-key">{{ t('projectDetail.accessHttps') }}</span>
          <a
            v-if="httpsUrl"
            class="field-link"
            @click="project.domainConfigured && api.openInBrowser(httpsUrl)"
            >{{ httpsUrl }}</a
          >
          <span v-else class="field-val">—</span>
        </div>
      </v-col>
    </v-row>

    <!-- The domain is unreachable without a hosts entry, and the fix is
         one elevated write away — so offer it here rather than only
         reporting the problem. -->
    <v-alert
      v-if="project.domain && !project.domainConfigured"
      type="warning"
      variant="tonal"
      class="mt-2"
    >
      <div class="d-flex align-center ga-2">
        <span class="text-caption">{{ t('projects.domainMissingHint') }}</span>
        <v-spacer />
        <v-btn size="x-small" variant="tonal" @click="emit('fixHosts')">{{ t('hosts.fix') }}</v-btn>
      </div>
    </v-alert>

    <template v-if="manifest?.php?.extensions?.length">
      <div class="section-head mt-8 mb-3">
        <v-icon size="18" class="mr-2">mdi-puzzle</v-icon>{{ t('projectDetail.phpExtensions') }}
        <v-chip size="x-small" class="ml-2">{{ manifest.php.extensions.length }}</v-chip>
      </div>
      <div class="d-flex flex-wrap ga-2">
        <v-chip
          v-for="ext in manifest.php.extensions"
          :key="ext"
          size="small"
          label
          variant="tonal"
        >
          {{ ext }}
        </v-chip>
      </div>
    </template>

    <!-- Contract violations, shown rather than swallowed: the Bash
         generator skips such projects without a word. -->
    <template v-if="manifest?.errors?.length || manifest?.warnings?.length">
      <div class="section-head mt-8 mb-3">
        <v-icon size="18" class="mr-2">mdi-file-alert</v-icon>{{ t('projects.problems') }}
      </div>
      <v-alert v-if="manifest.errors.length" type="error" variant="tonal" class="mb-2">
        <div v-for="(i, k) in manifest.errors" :key="k" class="text-caption">
          <strong>{{ i.code }}</strong> {{ i.path }} — {{ i.message }}
        </div>
      </v-alert>
      <v-alert v-if="manifest.warnings.length" type="warning" variant="tonal">
        <div v-for="(i, k) in manifest.warnings" :key="k" class="text-caption">
          <strong>{{ i.code }}</strong> {{ i.path }} — {{ i.message }}
        </div>
      </v-alert>
    </template>
  </v-card>
</template>
