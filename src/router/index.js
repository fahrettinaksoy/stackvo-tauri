import { createRouter, createWebHashHistory } from 'vue-router';

/**
 * A lazy route whose chunk cannot be fetched, recovered rather than swallowed.
 *
 * Vue Router aborts a navigation whose async component rejects, and it does so
 * silently: the sidebar item never becomes active, the old page stays, and
 * nothing appears on screen. A click that does literally nothing is the worst
 * possible failure mode — it reads as a dead button rather than a broken load.
 *
 * The chunk really can go missing. In dev, Vite discovers a new dependency the
 * first time a view is transformed, re-bundles, and invalidates the URLs the
 * in-flight import was using — the request fails with "Importing a module
 * script failed", and because a failed dynamic import stays failed for the life
 * of the document, every later click on that item fails too. Vite's own answer
 * to this is a full reload, so take it: one reload puts the page back on the
 * fresh dependency graph and the route loads.
 *
 * Rate-limited rather than once-per-session: a genuinely broken build must not
 * reload in a loop, but a second, unrelated failure an hour later still
 * deserves its own recovery.
 */
const RELOAD_KEY = 'stackvo.chunk-reload-at';
const RELOAD_COOLDOWN_MS = 10_000;

function view(loader) {
  return async () => {
    try {
      return await loader();
    } catch (error) {
      const last = Number(sessionStorage.getItem(RELOAD_KEY)) || 0;
      if (Date.now() - last < RELOAD_COOLDOWN_MS) throw error;

      sessionStorage.setItem(RELOAD_KEY, String(Date.now()));
      console.warn('[router] chunk failed to load, reloading once', error);
      location.reload();
      // The document is being replaced; resolving would mount a view into it.
      return new Promise(() => {});
    }
  };
}

// Hash history, not web history. In a packaged Tauri app the frontend is served
// from a custom protocol with no server-side rewrite, so a path-based route
// would 404 on reload. The URL is never user-visible here anyway.
const routes = [
  { path: '/', name: 'Dashboard', component: view(() => import('@/views/Dashboard.vue')) },
  { path: '/projects', name: 'Projects', component: view(() => import('@/views/Projects.vue')) },
  // A page, not a dialog: the detail view carries three sections of its own and
  // deserves a URL you can return to.
  {
    path: '/projects/:name',
    name: 'ProjectDetail',
    component: view(() => import('@/views/ProjectDetail.vue')),
    props: true,
  },
  { path: '/services', name: 'Services', component: view(() => import('@/views/Services.vue')) },
  // Where services come from, as opposed to what is running. Beside Services
  // because the two are read together and a user moving between them is asking
  // one question: what have I got, and what could I have.
  { path: '/market', name: 'Market', component: view(() => import('@/views/Market.vue')) },
  // Belongs to no project, which is the point: it is where you look before you
  // know which project to open.
  { path: '/logs', name: 'Logs', component: view(() => import('@/views/Logs.vue')) },
  // Beside the log page and for the same reason: you look here before you know
  // which project to open, and it is somewhere you leave open while working.
  { path: '/dumps', name: 'Dumps', component: view(() => import('@/views/Dumps.vue')) },
  // The inbox as a destination, not a service-sheet tab: "my app just sent a
  // mail — show me" names a place, the way Herd's Mail page does.
  { path: '/mail', name: 'Mail', component: view(() => import('@/views/Mail.vue')) },
  { path: '/settings', name: 'Settings', component: view(() => import('@/views/Settings.vue')) },
  // Its own window, opened from the menu bar. App.vue renders it without the
  // shell — an about box with a sidebar and an app bar is not an about box.
  { path: '/about', name: 'About', component: view(() => import('@/views/About.vue')) },
];

export default createRouter({
  history: createWebHashHistory(),
  routes,
});
