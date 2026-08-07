<script setup>
import { useI18n } from 'vue-i18n';
import { bytes } from '@/lib/format';
import { useCopyTick } from '@/composables/useCopyTick';

/**
 * What Docker reports about the container behind this project.
 *
 * A read-only pane — every field comes from the view's already-loaded
 * `details`, so it takes them as props rather than inspecting again. There is
 * nothing here to load and nothing to save; that is why it has no composable.
 */
defineProps({
  project: { type: Object, default: null },
  details: { type: Object, default: null },
  running: { type: Boolean, default: false },
});

const { t } = useI18n();
// The three copy buttons here showed no feedback at all, while the identical
// one on the Tunnel pane ticks. Same helper, same tick — pressing a button
// that gives no sign it worked reads as a button that did not.
const { copied, copy } = useCopyTick();
</script>

<template>
  <v-card variant="flat" class="pane">
    <div class="section-head mb-4">
      <v-icon size="18" class="mr-2">mdi-docker</v-icon>{{ t('projectDetail.container') }}
    </div>

    <div v-if="!details" class="text-caption text-medium-emphasis py-8 text-center">
      {{ t('projects.notBuilt') }}
    </div>

    <template v-else>
      <v-row>
        <v-col cols="12" md="4">
          <div class="field">
            <span class="field-key">{{ t('projectDetail.name') }}</span>
            <code class="field-mono">{{ details.name }}</code>
            <v-btn
              icon
              :aria-label="t('a11y.copy')"
              size="x-small"
              variant="text"
              @click="copy(details.name, 'cname')"
            >
              <v-icon>{{ copied === 'cname' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
              <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
            </v-btn>
          </div>
          <div class="field">
            <span class="field-key">{{ t('projectDetail.uptime') }}</span>
            <span class="field-val">{{
              details.startedAt ? new Date(details.startedAt).toLocaleString() : '—'
            }}</span>
          </div>
          <div class="field">
            <span class="field-key">{{ t('projectDetail.restartPolicy') }}</span>
            <span class="field-val">{{ details.restartPolicy || '—' }}</span>
          </div>
          <div class="field">
            <span class="field-key">{{ t('projectDetail.dnsHosts') }}</span>
            <span
              class="field-val"
              :class="project.domainConfigured ? 'text-success' : 'text-warning'"
            >
              <v-icon size="14">{{
                project.domainConfigured ? 'mdi-check-circle' : 'mdi-alert-circle'
              }}</v-icon>
              {{
                project.domainConfigured
                  ? t('projectDetail.configured')
                  : t('projectsView.noDnsRecord')
              }}
            </span>
          </div>
        </v-col>

        <v-col cols="12" md="4">
          <div class="field">
            <span class="field-key">{{ t('detail.state') }}</span>
            <span class="field-val" :class="details.running ? 'text-success' : ''">
              <v-icon size="10">mdi-circle</v-icon> {{ details.state }}
            </span>
          </div>
          <div class="field">
            <span class="field-key">{{ t('projectDetail.created') }}</span>
            <span class="field-val">{{
              details.created ? new Date(details.created).toLocaleString() : '—'
            }}</span>
          </div>
          <div class="field">
            <span class="field-key">{{ t('projectDetail.containerId') }}</span>
            <code class="field-mono">{{ details.id?.slice(0, 12) }}</code>
            <v-btn
              icon
              :aria-label="t('a11y.copy')"
              size="x-small"
              variant="text"
              @click="copy(details.id, 'cid')"
            >
              <v-icon>{{ copied === 'cid' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
              <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
            </v-btn>
          </div>
        </v-col>

        <v-col cols="12" md="4">
          <div class="field">
            <span class="field-key">{{ t('detail.image') }}</span>
            <code class="field-mono">{{ details.image }}</code>
            <v-btn
              icon
              :aria-label="t('a11y.copy')"
              size="x-small"
              variant="text"
              @click="copy(details.image, 'img')"
            >
              <v-icon>{{ copied === 'img' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
              <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
            </v-btn>
          </div>
          <div class="field">
            <span class="field-key">{{ t('projectDetail.restartCount') }}</span>
            <span class="field-val">{{ details.restartCount }}</span>
          </div>
          <div class="field">
            <span class="field-key">{{ t('projectDetail.imageSize') }}</span>
            <span class="field-val">{{ details.imageSize ? bytes(details.imageSize) : '—' }}</span>
          </div>
        </v-col>
      </v-row>

      <div class="section-head mt-8 mb-3">
        <v-icon size="18" class="mr-2">mdi-lan</v-icon>{{ t('stats.network') }}
      </div>

      <v-row>
        <v-col cols="12" md="4">
          <div class="field">
            <span class="field-key">{{ t('projectDetail.gateway') }}</span>
            <span class="field-val">{{ details.gateway || '—' }}</span>
          </div>
          <div class="field">
            <span class="field-key">{{ t('projectDetail.portMappings') }}</span>
            <span v-if="!details.ports.length" class="field-val">—</span>
            <span v-else class="field-val">
              <template v-for="p in details.ports" :key="p.container">
                <code class="field-mono">{{ p.container }}/{{ p.protocol }}</code>
                <span v-if="p.host" class="text-success ml-1">→ {{ p.host }}</span>
                <span v-else class="text-warning ml-1">{{ t('projectDetail.notPublished') }}</span>
              </template>
            </span>
          </div>
        </v-col>
        <v-col cols="12" md="4">
          <div class="field">
            <span class="field-key">{{ t('stats.network') }}</span>
            <span class="field-val">{{ details.networks.join(', ') || '—' }}</span>
          </div>
        </v-col>
      </v-row>
    </template>
  </v-card>
</template>
