import './styles/global.css';
import { createApp } from 'vue';
import { createPinia } from 'pinia';
import router from './router';
import vuetify from './plugins/vuetify';
import { i18n } from './i18n';
import App from './App.vue';

const app = createApp(App);

// Uncaught component errors go to the console with a trace. A user-facing
// snackbar lands with the feedback composable in Phase 2, alongside the
// mutating commands that can actually fail mid-action.
app.config.errorHandler = (err, _instance, info) => {
  console.error('[Vue error]', err, info);
};

app.use(createPinia());
app.use(router);
app.use(i18n);
app.use(vuetify);

app.mount('#app');
