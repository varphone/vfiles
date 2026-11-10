<template>
  <div class="file-toolbar-heading">
    <Breadcrumb
      v-if="currentPath || isMobile"
      :breadcrumbs="breadcrumbs"
      :directories="directories"
      @navigate="emit('navigate', $event)"
      @drop="emit('drop', $event)"
    />
    <FileBrowserViewMenu
      v-else
      :open="open"
      :show-shares="showShares"
      @update:open="emit('update:open', $event)"
    />
  </div>
</template>

<script setup lang="ts">
import type { FileInfo } from "../../types";
import Breadcrumb from "./Breadcrumb.vue";
import FileBrowserViewMenu from "./FileBrowserViewMenu.vue";

defineProps<{
  isMobile: boolean;
  currentPath: string;
  breadcrumbs: Array<{ name: string; path: string }>;
  directories: FileInfo[];
  showShares: boolean;
  open: boolean;
}>();

const emit = defineEmits<{
  (e: "navigate", path: string): void;
  (e: "drop", path: string): void;
  (e: "update:open", value: boolean): void;
}>();
</script>

<style scoped>
.file-toolbar-heading {
  display: flex;
  align-items: center;
  min-width: 0;
  flex: 0 1 auto;
}
</style>
