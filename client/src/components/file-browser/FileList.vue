<template>
  <FileItem
    v-for="file in files"
    :key="file.path"
    :file="file"
    :highlight="highlight"
    :select-mode="selectMode"
    :selected="selectedPaths.has(file.path)"
    :expanded="expandedPath === file.path"
    @click="emit('click', file)"
    @download="emit('download', file)"
    @delete="emit('delete', file)"
    @view-history="emit('view-history', file)"
    @toggle-select="emit('toggle-select', file)"
    @share="emit('share', file)"
    @preview="emit('preview', file)"
    @open-folder="emit('open-folder', file)"
  />
</template>

<script setup lang="ts">
import type { FileInfo } from "../../types";
import FileItem from "./FileItem.vue";

withDefaults(
  defineProps<{
    files: FileInfo[];
    highlight?: string;
    selectMode: boolean;
    selectedPaths: Set<string>;
    expandedPath?: string;
  }>(),
  {
    highlight: "",
    expandedPath: "",
  },
);

const emit = defineEmits<{
  (e: "click", file: FileInfo): void;
  (e: "download", file: FileInfo): void;
  (e: "delete", file: FileInfo): void;
  (e: "view-history", file: FileInfo): void;
  (e: "toggle-select", file: FileInfo): void;
  (e: "share", file: FileInfo): void;
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
}>();
</script>
