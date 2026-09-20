<template>
  <div
    class="file-grid"
    :style="{ '--file-card-min': `${Math.round(thumbnailSize * 1.35)}px` }"
  >
    <FileCard
      v-for="file in cards"
      :key="file.path"
      :file="file"
      :commit="commit"
      :highlight="highlight"
      :select-mode="selectMode"
      :selected="
        selectedPaths.has(file.path) ||
        (!selectMode && activePath === file.path)
      "
      :active="activePath === file.path"
      :thumbnail-size="thumbnailSize"
      @click="emit('click', file)"
      @download="emit('download', file)"
      @rename="emit('rename', file)"
      @move="emit('move', file)"
      @delete="emit('delete', file)"
      @view-history="emit('view-history', file)"
      @toggle-select="emit('toggle-select', file)"
      @modifier-select="emit('modifier-select', $event)"
      @context-menu="emit('context-menu', $event)"
      @share="emit('share', file)"
      @preview="emit('preview', file)"
      @open-folder="emit('open-folder', file)"
      @create-directory="emit('create-directory', file)"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, toRefs } from "vue";
import type { FileInfo } from "../../types";
import FileCard from "./FileCard.vue";

const emit = defineEmits<{
  (e: "click", file: FileInfo): void;
  (e: "download", file: FileInfo): void;
  (e: "rename", file: FileInfo): void;
  (e: "move", file: FileInfo): void;
  (e: "delete", file: FileInfo): void;
  (e: "view-history", file: FileInfo): void;
  (e: "toggle-select", file: FileInfo): void;
  (e: "share", file: FileInfo): void;
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
  (e: "create-directory", file: FileInfo): void;
  (
    e: "modifier-select",
    payload: { file: FileInfo; shift: boolean; meta: boolean },
  ): void;
  (e: "context-menu", payload: { file: FileInfo; x: number; y: number }): void;
}>();

const props = withDefaults(
  defineProps<{
    files: FileInfo[];
    highlight?: string;
    commit?: string;
    selectMode: boolean;
    selectedPaths: Set<string>;
    activePath?: string;
    thumbnailSize?: number;
  }>(),
  {
    highlight: "",
    commit: undefined,
    activePath: "",
    thumbnailSize: 144,
  },
);

const {
  files,
  highlight,
  commit,
  selectMode,
  selectedPaths,
  activePath,
  thumbnailSize,
} = toRefs(props);

/**
 * 网格视图通过面包屑/工具栏完成“返回上一级”，
 * 因此过滤掉列表视图专用的“.”“..”快捷项，与主流云盘保持一致。
 */
const cards = computed(() =>
  files.value.filter(
    (file) => !(file as FileInfo & { uiRole?: string }).uiRole,
  ),
);
</script>

<style scoped>
.file-grid {
  display: grid;
  grid-template-columns: repeat(
    auto-fill,
    minmax(var(--file-card-min, 190px), 1fr)
  );
  gap: 14px;
  padding: 4px 2px 12px;
}
</style>
