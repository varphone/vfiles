<template>
  <nav class="breadcrumb has-succeeds-separator" aria-label="breadcrumbs">
    <ul>
      <li
        v-for="(crumb, index) in breadcrumbs"
        :key="crumb.path"
        :class="{ 'is-active': index === breadcrumbs.length - 1 }"
      >
        <a @click.prevent="navigate(crumb.path)" :href="`#${crumb.path}`">
          <IconFolderOpen v-if="index === 0" :size="16" class="mr-1" />
          {{ crumb.name }}
        </a>
      </li>
    </ul>
  </nav>
</template>

<script setup lang="ts">
import { IconFolderOpen } from "@tabler/icons-vue";

defineProps<{
  breadcrumbs: Array<{ name: string; path: string }>;
}>();

const emit = defineEmits<{
  navigate: [path: string];
}>();

function navigate(path: string) {
  emit("navigate", path);
}
</script>

<style scoped>
.breadcrumb {
  background: transparent;
  padding: 0.75rem 0;
}

.breadcrumb a {
  display: flex;
  align-items: center;
  padding: 0.5rem;
  border-radius: 4px;
  transition: background-color 0.2s;
}

.breadcrumb a:hover {
  background-color: rgba(0, 0, 0, 0.05);
}

.breadcrumb li.is-active a {
  color: #363636;
  font-weight: 600;
  cursor: default;
}

.breadcrumb li.is-active a:hover {
  background-color: transparent;
}

@media screen and (max-width: 768px) {
  .breadcrumb ul {
    flex-wrap: nowrap;
    overflow-x: auto;
  }
}
</style>
