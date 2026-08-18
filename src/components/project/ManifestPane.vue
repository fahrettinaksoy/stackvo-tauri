<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useOperationsStore } from '@/stores/operations';
import CollapsiblePane from '@/components/project/CollapsiblePane.vue';

/**
 * The project's `stackvo.json`, as text.
 *
 * A controlled editor rather than an owner of the file: the view reads the
 * manifest (the Xdebug pane rewrites it too) and the view saves it, because
 * saving reloads the whole page. This pane holds the draft and says when it
 * differs.
 */
const props = defineProps({
  name: { type: String, required: true },
  modelValue: { type: String, default: '' },
  dirty: { type: Boolean, default: false },
  saving: { type: Boolean, default: false },
  project: { type: Object, default: null },
});

const emit = defineEmits(['update:modelValue', 'dirty', 'save', 'bringUp']);

const { t } = useI18n();
const ops = useOperationsStore();

/**
 * Every keystroke is both the draft and the dirty signal. The view owns both,
 * because the same text is re-read from disk when the Xdebug pane rewrites the
 * manifest — a pane holding its own copy would keep showing the stale one.
 */
const text = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value),
});
</script>

<template>
  <!-- Folded away by default. Twenty-four rows of JSON is the tallest thing on
       the configuration tab and the least often read: everything it holds is
       shown as fields in the pane above, and this is where you come when you
       want to see the file itself. -->
  <CollapsiblePane>
    <template #title>
      <span class="section-head">
        <v-icon size="18" class="mr-2">mdi-code-json</v-icon>{{ t('detail.manifest') }}
      </span>
    </template>

    <template #meta>
      <span class="text-caption text-medium-emphasis">{{ t('detail.manifestHint') }}</span>
    </template>

    <!-- Outside the fold. Saving a draft and bringing the stack up are things
         you do to the project, not to the view of the file — and a save button
         that disappears when the editor is shut is a save you have to go
         looking for. -->
    <template #actions>
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-play-box-outline"
        :loading="ops.isBusy(name)"
        @click="emit('bringUp')"
        >{{ t('detail.bringUp') }}</v-btn
      >
      <v-btn
        size="small"
        color="primary"
        variant="flat"
        :disabled="!dirty"
        :loading="saving"
        @click="emit('save')"
        >{{ t('detail.save') }}</v-btn
      >
    </template>

    <!-- Named: the heading sits above it as a `div`, so Vuetify emitted an
         `aria-labelledby` pointing at a label element that does not exist —
         a screen reader reached a 24-row editor announced as nothing at all. -->
    <v-textarea
      v-model="text"
      :aria-label="t('detail.manifest')"
      variant="outlined"
      rows="24"
      class="mono-input"
      hide-details
      @update:model-value="emit('dirty')"
    />
  </CollapsiblePane>
</template>
