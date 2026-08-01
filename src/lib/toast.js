/**
 * The global snackbar queue.
 *
 * One reactive array, rendered once in App.vue by `v-snackbar-queue`, pushed
 * to from anywhere. A module rather than a Pinia store: there is no derived
 * state, no persistence and no devtools story here — it is a mailbox.
 *
 * The queue exists because operations stopped announcing themselves through
 * the console alone: a successful generate does not need 16 lines of `wrote …`
 * held open on screen, it needs one green sentence that goes away by itself.
 * The console remains the place failures keep their evidence.
 */
import { ref } from 'vue';

export const toasts = ref([]);

export function toast(text, color = undefined, timeout = 4000) {
  toasts.value.push({ text, color, timeout });
}

export const toastSuccess = (text) => toast(text, 'success');
export const toastError = (text) => toast(text, 'error', 6000);
