<script setup>
import { computed, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useCertificates } from '@/composables/useCertificates';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The Certificates pane, as a component rather than 210 lines of `Settings.vue`.
 *
 * It is the first slice of the §14.16 split, chosen because it is the pane with
 * a *shape mirror* test — `tests/certificates-pane.spec.js` rebuilt a copy of
 * this markup and then read the real file as text to check the copy still
 * matched. That was the only way to test a pane trapped inside a 3,433-line
 * view, and its cost is in the review: behaviour verified in the copy, not in
 * the product, with a `toContain` string match holding the two together.
 *
 * Mounting this is now ordinary, so `tests/certificates-pane.spec.js` is gone
 * and `tests/settings-certificates.spec.js` mounts the real thing — including
 * the tooltip that shipped not working, which is why that file exists at all.
 *
 * State lives in `useCertificates` because the settings rail badges a stale
 * certificate and has to know before you open this pane.
 */
const { t, locale } = useI18n();

const { certs, plan, error, busy, notReloaded, load, reissue, trustInTerminal, expiry } =
  useCertificates();

const expiryLabel = computed(() => expiry(locale.value));

// Read on mount rather than on opening the pane: the rail badge is derived from
// the same state, and a badge that only appears once you have navigated to the
// thing it points at is decoration.
onMounted(load);
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <SettingsGroup
    icon="mdi-certificate-outline"
    :title="t('certs.title')"
    :description="t('certs.subtitle')"
  >
    <template #append>
      <v-btn
        size="x-small"
        variant="text"
        icon="mdi-refresh"
        :aria-label="t('app.refresh')"
        :loading="busy"
        @click="load"
      />
    </template>

    <!-- SSL off is a choice, not a fault: without it the generator
         emits no `websecure` entry point and nothing below applies. -->
    <v-alert v-if="certs && !certs.sslEnabled" type="info" variant="tonal" class="mb-3">
      <div class="text-caption">{{ t('certs.sslOff') }}</div>
    </v-alert>

    <template v-else-if="certs">
      <div class="d-flex align-center ga-2 mb-3 flex-wrap">
        <v-chip size="small" :color="certs.stale ? 'warning' : 'success'">
          {{ certs.stale ? t('certs.stale') : t('certs.current') }}
        </v-chip>
        <v-chip
          size="small"
          :color="
            certs.caTrusted === true ? 'success' : certs.caTrusted === false ? 'warning' : undefined
          "
        >
          {{
            certs.caTrusted === true
              ? t('certs.caTrusted')
              : certs.caTrusted === false
                ? t('certs.caUntrusted')
                : t('certs.caUnknown')
          }}
        </v-chip>
        <span v-if="expiryLabel" class="text-caption text-medium-emphasis">
          {{
            certs.expired
              ? t('certs.expiredOn', { date: expiryLabel })
              : t('certs.expiresOn', {
                  date: expiryLabel,
                  days: certs.daysRemaining,
                })
          }}
        </span>
      </div>

      <!-- mkcert is the whole mechanism; without it nothing here can
           be repaired, so it is said plainly rather than left for the
           reissue button to fail on. -->
      <v-alert v-if="!certs.mkcertAvailable" type="warning" variant="tonal" class="mb-3">
        <div class="text-caption">{{ t('certs.noMkcert') }}</div>
      </v-alert>

      <v-alert v-if="certs.error" type="error" variant="tonal" class="mb-3">
        <div class="text-caption">{{ certs.error }}</div>
      </v-alert>

      <!-- The point of the pane: which domains the file on disk does
           not vouch for. -->
      <template v-if="certs.missing.length">
        <div class="text-caption text-medium-emphasis mb-1">
          {{ t('certs.missing') }}
        </div>
        <div class="mb-3">
          <v-chip
            v-for="d in certs.missing"
            :key="d"
            size="x-small"
            color="warning"
            class="mr-1 mb-1"
          >
            {{ d }}
          </v-chip>
        </div>
      </template>

      <template v-if="plan?.remove?.length">
        <div class="text-caption text-medium-emphasis mb-1">
          {{ t('certs.dropping') }}
        </div>
        <div class="mb-3">
          <v-chip
            v-for="d in plan.remove"
            :key="d"
            size="x-small"
            variant="outlined"
            class="mr-1 mb-1"
          >
            {{ d }}
          </v-chip>
        </div>
      </template>

      <template v-if="certs.rejected.length">
        <div class="text-caption text-error mb-1">{{ t('certs.rejected') }}</div>
        <div class="mb-3">
          <v-chip
            v-for="d in certs.rejected"
            :key="d"
            size="x-small"
            color="error"
            class="mr-1 mb-1"
          >
            {{ d }}
          </v-chip>
        </div>
      </template>

      <div class="text-caption text-medium-emphasis mb-1">
        {{ t('certs.covered', { n: certs.covered.length }) }}
      </div>
      <div class="mb-3">
        <v-chip v-for="d in certs.covered" :key="d" size="x-small" class="mr-1 mb-1">
          {{ d }}
        </v-chip>
      </div>

      <v-btn
        size="small"
        variant="tonal"
        block
        prepend-icon="mdi-autorenew"
        :loading="busy"
        :disabled="!certs.mkcertAvailable"
        @click="reissue"
      >
        {{ t('certs.reissue') }}
      </v-btn>

      <!-- Trusting the CA is a separate button because it is a
           separate thing, and on macOS it is the only one this app
           cannot do for itself: `sudo` needs a terminal, root through
           AppleScript is refused, and the user-domain write exits 0
           and changes nothing. So it opens a terminal — which is
           honest, and works. -->
      <v-btn
        v-if="certs.caTrusted !== true"
        size="small"
        variant="tonal"
        color="warning"
        block
        class="mt-2"
        prepend-icon="mdi-console"
        :disabled="!certs.mkcertAvailable"
        @click="trustInTerminal"
      >
        {{ t('certs.trustInTerminal') }}
      </v-btn>
      <div v-if="certs.caTrusted !== true" class="text-caption text-medium-emphasis mt-2">
        {{ t('certs.trustInTerminalHint') }}
      </div>

      <!-- The certificate is on disk and the browser is still getting
           the old one. Silence here is what made this bug survive:
           the reissue reports success either way. -->
      <v-alert v-if="notReloaded" type="warning" variant="tonal" class="mt-3">
        <div class="text-caption">{{ t('certs.notReloaded') }}</div>
      </v-alert>

      <!-- Both paths, each said to be what it is.
           They were reported as "the certificate is in two places",
           three times, because only one of them was ever shown and
           the other was found by looking. They are two different
           files with two different jobs, and the reason they are not
           in one directory is the line below the second one. -->
      <div class="mt-3">
        <div v-if="certs.certPath" class="text-caption text-medium-emphasis">
          <strong>{{ t('certs.leafLabel') }}</strong> · {{ certs.certPath }}
        </div>
        <div v-if="certs.caPath" class="text-caption text-medium-emphasis mt-1">
          <strong>{{ t('certs.caLabel') }}</strong> · {{ certs.caPath }}
          <!-- The reason they are not one directory is worth having
               and is not worth three lines of a settings pane. It was
               three lines here, and read as a lecture attached to two
               file paths.
               The `#activator` slot rather than `activator="parent"`:
               the first version nested the tooltip inside `v-icon`
               alongside the icon's own name, so the slot held two
               things and the hover reached neither. This is the shape
               every other tooltip in this app already uses. -->
          <v-tooltip :text="t('certs.whySeparate')" location="top" max-width="420">
            <template #activator="{ props }">
              <v-icon
                v-bind="props"
                size="14"
                class="ml-1 why-separate"
                icon="mdi-information-outline"
              />
            </template>
          </v-tooltip>
        </div>
      </div>
    </template>
  </SettingsGroup>
</template>

<style scoped>
/* The icon is a hover target and nothing else, so it says so. Moved here with
   the pane: a scoped style does not follow an element into another component,
   and leaving it in `Settings.vue` dropped the cursor without breaking a
   thing that any test or lint could see. */
.why-separate {
  cursor: help;
}
</style>
