import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { api, asList } from '@/lib/ipc';

/**
 * Projects and services.
 *
 * Note what is NOT here: a filter that hides broken projects. The Bash
 * generator skips a manifest with a missing domain and moves on, so the project
 * simply disappears. `projects_list` returns it with `manifestValid: false` and
 * the reasons attached, and the UI shows it.
 */
export const useInventoryStore = defineStore('inventory', () => {
  const projects = ref([]);
  const services = ref([]);
  const loadingProjects = ref(false);
  const loadingServices = ref(false);
  const projectsError = ref(null);
  const servicesError = ref(null);

  const invalidProjects = computed(() => projects.value.filter((p) => !p.manifestValid));
  const runningProjects = computed(() => projects.value.filter((p) => p.running));
  const enabledServices = computed(() => services.value.filter((s) => s.enabled));
  const runningServices = computed(() => services.value.filter((s) => s.running));

  /** Projects whose domain has no /etc/hosts entry — unreachable in a browser. */
  const unreachableDomains = computed(() =>
    projects.value.filter((p) => p.domain && !p.domainConfigured)
  );

  /** Services enabled but missing a dependency they need to actually work. */
  const brokenDependencies = computed(() =>
    // `?.` for the same reason `asList` exists: the field is read off whatever
    // the boundary handed back, and a service without it would throw here
    // rather than simply not be broken.
    services.value.filter((s) => s.enabled && s.unmetDependencies?.length > 0)
  );

  const servicesByCategory = computed(() => {
    const groups = {};
    for (const service of services.value) {
      (groups[service.category] ||= []).push(service);
    }
    return groups;
  });

  async function loadProjects() {
    loadingProjects.value = true;
    projectsError.value = null;
    try {
      projects.value = asList(await api.projectsList());
    } catch (e) {
      projectsError.value = e;
      projects.value = [];
    } finally {
      loadingProjects.value = false;
    }
  }

  async function loadServices() {
    loadingServices.value = true;
    servicesError.value = null;
    try {
      services.value = asList(await api.servicesList());
    } catch (e) {
      servicesError.value = e;
      services.value = [];
    } finally {
      loadingServices.value = false;
    }
  }

  async function loadAll() {
    await Promise.all([loadProjects(), loadServices()]);
  }

  return {
    projects,
    services,
    loadingProjects,
    loadingServices,
    projectsError,
    servicesError,
    invalidProjects,
    runningProjects,
    enabledServices,
    runningServices,
    unreachableDomains,
    brokenDependencies,
    servicesByCategory,
    loadProjects,
    loadServices,
    loadAll,
  };
});
