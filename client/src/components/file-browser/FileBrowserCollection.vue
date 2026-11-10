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
</script>
