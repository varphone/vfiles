<template>
  <div class="file-view-toggle" role="group" aria-label="显示方式">
    <button
      class="vf-icon-button"
      :class="{ 'is-active': viewMode === 'list' }"
      type="button"
      title="列表视图"
      aria-label="列表视图"
      :aria-pressed="viewMode === 'list' ? 'true' : 'false'"
      @click="fileView.setMode('list')"
    >
      <IconList :size="18" />
    </button>
    <button
      class="vf-icon-button"
      :class="{ 'is-active': viewMode === 'grid' }"
      type="button"
      title="网格视图"
      aria-label="网格视图"
      :aria-pressed="viewMode === 'grid' ? 'true' : 'false'"
      @click="fileView.setMode('grid')"
    >
      <IconLayoutGrid :size="18" />
    </button>
  </div>

  <SortMenu />

  <button
    class="vf-icon-button file-toolbar-details"
    :class="{ 'is-active': detailsVisible }"
    :title="detailsVisible ? '隐藏详细信息' : '显示详细信息'"
    :aria-label="detailsVisible ? '隐藏详细信息' : '显示详细信息'"
    :aria-pressed="detailsVisible ? 'true' : 'false'"
    @click="fileView.toggleDetails()"
  >
    <IconLayoutSidebarRight :size="18" />
  </button>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  IconLayoutGrid,
  IconLayoutSidebarRight,
  IconList,
} from "@tabler/icons-vue";
import { useFileViewStore } from "../../stores/fileView.store";
import SortMenu from "./SortMenu.vue";

const fileView = useFileViewStore();
const viewMode = computed(() => fileView.mode);
const detailsVisible = computed(() => fileView.detailsVisible);
</script>

<style scoped>
.file-view-toggle {
  display: inline-flex;
  align-items: center;
  gap: 1px;
  flex: 0 0 auto;
  padding: 2px;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface-2);
}

.file-view-toggle .vf-icon-button {
  width: var(--vf-control-height-sm);
  height: var(--vf-control-height-sm);
  border-radius: var(--vf-radius-control);
}

.file-toolbar-details {
  flex: 0 0 auto;
}
</style>
