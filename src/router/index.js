import { createRouter, createWebHashHistory } from 'vue-router';

// Hash history, not web history. In a packaged Tauri app the frontend is served
// from a custom protocol with no server-side rewrite, so a path-based route
// would 404 on reload. The URL is never user-visible here anyway.
const routes = [
  { path: '/', name: 'Dashboard', component: () => import('@/views/Dashboard.vue') },
  { path: '/projects', name: 'Projects', component: () => import('@/views/Projects.vue') },
  // A page, not a dialog: the detail view carries three sections of its own and
  // deserves a URL you can return to.
  {
    path: '/projects/:name',
    name: 'ProjectDetail',
    component: () => import('@/views/ProjectDetail.vue'),
    props: true,
  },
  { path: '/services', name: 'Services', component: () => import('@/views/Services.vue') },
  { path: '/settings', name: 'Settings', component: () => import('@/views/Settings.vue') },
];

export default createRouter({
  history: createWebHashHistory(),
  routes,
});
