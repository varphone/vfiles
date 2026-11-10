<template>
  <FileGrid
    v-if="viewMode === 'grid'"
    :files="files"
    :commit="commit"
    :highlight="highlight"
    :select-mode="selectMode"
    :selected-paths="selectedPaths"
    :active-path="activePath"
    :show-location="showLocation"
    :thumbnail-size="thumbnailSize"
    :can-write="canWrite"
    :can-share="canShare"
    :renaming-path="renamingPath"
    v-on="listeners"
  />
  <FileList
    v-else
    :files="files"
    :desktop="desktop"
    :highlight="highlight"
    :select-mode="selectMode"
    :selected-paths="selectedPaths"
    :active-path="activePath"
    :flash-path="flashPath"
    :expanded-path="expandedPath"
    :renaming-path="renamingPath"
    :show-action-column="showActionColumn"
    :show-location="showLocation"
    :can-write="canWrite"
    :can-share="canShare"
    :shared-paths="sharedPaths"
    :sort-field="sortField"
    :sort-direction="sortDirection"
    v-on="listeners"
  />
</template>

<script setup lang="ts">
import type { FileInfo } from "../../types";
import type { SortDirection, SortField } from "../../utils/fileSort";
import FileList from "./FileList.vue";
import FileGrid from "./FileGrid.vue";

/**
 * 列表/网格的统一入口：只负责按视图选择组件并转交渲染 props。
 *
 * 事件不在这里声明——监听器作为 fallthrough attrs 落到 FileList/FileGrid 的
 * 同名 emit 上，避免在四个调用点重复 20+ 条事件接线。
 */
withDefaults(
  defineProps<{
    files: FileInfo[];
    viewMode: "list" | "grid";
    selectMode: boolean;
    selectedPaths: Set<string>;
    canWrite?: boolean;
    canShare?: boolean;
    desktop?: boolean;
    activePath?: string;
    highlight?: string;
    commit?: string;
    renamingPath?: string;
    flashPath?: string;
    expandedPath?: string;
    showActionColumn?: boolean;
    showLocation?: boolean;
    thumbnailSize?: number;
    sharedPaths?: Set<string>;
    sortField?: SortField;
    sortDirection?: SortDirection;
  }>(),
  {
    canWrite: true,
    canShare: true,
    desktop: false,
    activePath: "",
    highlight: "",
    commit: undefined,
    renamingPath: "",
    flashPath: "",
    expandedPath: "",
    showActionColumn: false,
    showLocation: false,
    thumbnailSize: 144,
    sharedPaths: () => new Set<string>(),
    sortField: "name",
    sortDirection: "asc",
  },
);

const emit = defineEmits<{
  (e: "click", file: FileInfo): void;
  (e: "download", file: FileInfo): void;
  (e: "rename", file: FileInfo): void;
  (e: "rename-commit", file: FileInfo, name: string): void;
  (e: "rename-cancel", file: FileInfo): void;
  (e: "move", file: FileInfo): void;
  (e: "delete", file: FileInfo): void;
  (e: "view-history", file: FileInfo): void;
  (e: "toggle-select", file: FileInfo): void;
  (
    e: "modifier-select",
    payload: { file: FileInfo; shift: boolean; meta: boolean },
  ): void;
  (e: "context-menu", payload: { file: FileInfo; x: number; y: number }): void;
  (e: "drag-start", file: FileInfo): void;
  (e: "drag-end"): void;
  (e: "drop-on-folder", targetDir: string): void;
  (e: "toggle-select-all"): void;
  (e: "sort-change", field: SortField): void;
  (e: "share", file: FileInfo): void;
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
  (e: "create-directory", file: FileInfo): void;
}>();

const listeners = {
  click: (file: FileInfo) => emit("click", file),
  download: (file: FileInfo) => emit("download", file),
  rename: (file: FileInfo) => emit("rename", file),
  "rename-commit": (file: FileInfo, name: string) =>
    emit("rename-commit", file, name),
  "rename-cancel": (file: FileInfo) => emit("rename-cancel", file),
  move: (file: FileInfo) => emit("move", file),
  delete: (file: FileInfo) => emit("delete", file),
  "view-history": (file: FileInfo) => emit("view-history", file),
  "toggle-select": (file: FileInfo) => emit("toggle-select", file),
  "modifier-select": (payload: {
    file: FileInfo;
    shift: boolean;
    meta: boolean;
  }) => emit("modifier-select", payload),
  "context-menu": (payload: { file: FileInfo; x: number; y: number }) =>
    emit("context-menu", payload),
  "drag-start": (file: FileInfo) => emit("drag-start", file),
  "drag-end": () => emit("drag-end"),
  "drop-on-folder": (targetDir: string) => emit("drop-on-folder", targetDir),
  "toggle-select-all": () => emit("toggle-select-all"),
  "sort-change": (field: SortField) => emit("sort-change", field),
  share: (file: FileInfo) => emit("share", file),
  preview: (file: FileInfo) => emit("preview", file),
  "open-folder": (file: FileInfo) => emit("open-folder", file),
  "create-directory": (file: FileInfo) => emit("create-directory", file),
};
</script>
