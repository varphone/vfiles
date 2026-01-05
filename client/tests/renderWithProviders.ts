import { render } from '@testing-library/vue';
import { createRouter, createMemoryHistory } from 'vue-router';
import { createPinia, setActivePinia } from 'pinia';

export function renderWithProviders(component: any, options: any = {}) {
  const pinia = createPinia();
  setActivePinia(pinia);

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [],
  });

  return render(component, {
    global: {
      plugins: [pinia, router],
    },
    ...options,
  });
}
