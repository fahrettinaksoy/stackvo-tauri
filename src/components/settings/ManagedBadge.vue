<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useSharedEnvEditor } from '@/composables/useEnvEditor';

/**
 * "This field is not yours."
 *
 * A disabled input with no explanation is the thing this component exists to
 * prevent. Somebody who cannot type in the domain suffix and is told nothing
 * concludes the app is broken and files a bug; somebody who is told the value
 * comes from a policy file, and shown which one, has an action available even
 * though the action is to go and ask a colleague.
 *
 * Two states rather than one, because they are different situations:
 *
 *   * **managed** — a policy set this value, and the user may still change it.
 *     Worth showing so an unexpected default is explained, not worth
 *     preventing.
 *   * **locked** — set *and* held. The field is disabled and a save would be
 *     refused by the back end anyway, with `FORBIDDEN`.
 *
 * Renders nothing at all on an unmanaged machine, which is nearly every one.
 */
const props = defineProps({
  /** The `.env` key this badge speaks for. */
  envKey: { type: String, required: true },
});

const { t } = useI18n();
const { policy, isManaged, isLocked } = useSharedEnvEditor();

const locked = computed(() => isLocked(props.envKey));
const shown = computed(() => locked.value || isManaged(props.envKey));

/**
 * The path, not "a policy file".
 *
 * Naming it is the whole difference between a message somebody can act on and
 * one they can only be annoyed by. `source` is null only in the case where
 * nothing is managed, in which case this component has already rendered
 * nothing.
 */
const source = computed(() => policy.value.source ?? '');
</script>

<template>
  <v-tooltip v-if="shown" location="top">
    <template #activator="{ props: tip }">
      <v-chip
        v-bind="tip"
        size="x-small"
        variant="tonal"
        :color="locked ? 'warning' : 'info'"
        :prepend-icon="locked ? 'mdi-lock-outline' : 'mdi-domain'"
      >
        {{ locked ? t('settings.policy.locked') : t('settings.policy.managed') }}
      </v-chip>
    </template>
    <div class="text-caption">
      {{ locked ? t('settings.policy.lockedHint') : t('settings.policy.managedHint') }}
    </div>
    <div class="text-caption font-weight-medium">{{ source }}</div>
  </v-tooltip>
</template>
