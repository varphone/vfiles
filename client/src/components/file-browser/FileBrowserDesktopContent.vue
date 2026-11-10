<template>
  <FileSkeleton
    v-if="loading"
    :variant="viewMode === 'grid' ? 'grid' : 'list'"
    :thumbnail-size="thumbnailSize"
  />

  <EmptyState
    v-else-if="error"
    :icon="IconAlertCircle"
    tone="error"
    title="加载失败"
    :hint="error"
  >
    <template #actions>
      <button
        class="vf-ghost-button"
        :class="{ 'is-loading': loading }"
        :disabled="loading"
        @click="emit('retry')"
      >
        <IconRefresh :size="16" />
        <span>重试</span>
      </button>
    </template>
  </EmptyState>

  <LibraryGrid
    v-else-if="showLibraryCards"
    :directories="libraryDirectories"
    :renaming-path="renamingPath"
    :can-write="canWrite"
    :can-share="canShare"
    @open="emit('open-folder', $event)"
    @rename="emit('rename', $event)"
    @rename-commit="(target: FileInfo, name: string) => emit('rename-commit', target, name)"
    @rename-cancel="emit('rename-cancel', $event)"
    @delete="emit('delete', $event)"
    @share="emit('share', $event)"
    @details="emit('open-details', $event)"
    @context-menu="emit('context-menu', $event)"
  />

  <EmptyState
    v-else-if="!searchActive && files.length === 0"
    :icon="IconFolderOpen"
    title="此文件夹为空"
    :hint="
      canWrite
        ? '拖拽文件到这里，或使用下面的按钮上传'
        : '只读访问：当前目录没有可浏览的条目'
    "
  >
    <template v-if="canWrite" #actions>
      <button
        class="vf-ghost-button is-primary"
        @click="emit('upload-files')"
      >
        <IconUpload :size="16" />
        <span>上传文件</span>
      </button>
      <button class="vf-ghost-button" @click="emit('create-folder')">
        <IconFolderPlus :size="16" />
        <span>新建文件夹</span>
      </button>
    </template>
  </EmptyState>

  <EmptyState
    v-else-if="searchActive && searchResults.length === 0"
    :icon="IconSearch"
    title="没有找到匹配的文件"
    :hint="
      searchMode === 'content'
        ? '换个关键字，或改用文件名搜索（内容搜索只匹配文件内容）'
        : '换个关键字，或检查是否限制了搜索范围'
    "
  >
    <template #actions>
      <button
        v-if="searchMode === 'content'"
        class="vf-ghost-button is-primary"
        @click="emit('switch-to-name-search')"
      >
        <IconFileSearch :size="16" />
        <span>改为文件名搜索</span>
      </button>
      <button class="vf-ghost-button" @click="emit('clear-search')">
        <span>清空搜索</span>
      </button>
    </template>
  </EmptyState>

  <template v-else>
    <SearchResultToolbar
      v-if="searchActive"
      class="desktop-list-meta"
      :results="searchResults"
      :active="resultFilter"
      :query="searchQuery"
      :visible-count="desktopItems.length"
      @update:active="emit('update:filter', $event)"
    />

    <FileBrowserCollection
      :files="desktopItems"
      :view-mode="viewMode"
      :can-write="canWrite"
      :can-share="canShare"
      :shared-paths="sharedPaths"
      :highlight="searchActive ? searchQuery : ''"
      :commit="commit"
      :select-mode="batchMode"
      :selected-paths="selectedPaths"
      :flash-path="flashPath"
      :expanded-path="expandedPath"
      :active-path="activePath"
      :desktop="true"
      :show-action-column="!detailsVisible"
      :show-location="searchActive"
      :sort-field="sortField"
      :sort-direction="sortDirection"
      :thumbnail-size="thumbnailSize"
      :renaming-path="renamingPath"
      @click="emit('click', $event)"
      @download="emit('download', $event)"
      @rename="emit('rename', $event)"
      @rename-commit="(target: FileInfo, name: string) => emit('rename-commit', target, name)"
      @rename-cancel="emit('rename-cancel', $event)"
      @move="emit('move', $event)"
      @delete="emit('delete', $event)"
      @view-history="emit('view-history', $event)"
      @toggle-select="emit('toggle-select', $event)"
      @modifier-select="emit('modifier-select', $event)"
      @context-menu="emit('context-menu', $event)"
      @drag-start="emit('drag-start', $event)"
      @drag-end="emit('drag-end')"
      @drop-on-folder="emit('drop-on-folder', $event)"
      @toggle-select-all="emit('toggle-select-all')"
      @sort-change="emit('sort-change', $event)"
      @share="emit('share', $event)"
      @preview="emit('preview', $event)"
      @open-folder="emit('open-folder', $event)"
      @create-directory="emit('create-directory', $event)"
    />
    <slot name="load-more" />
  </template>
</template>

<script setup lang="ts">
import {
  IconAlertCircle,
  IconFileSearch,
  IconFolderOpen,
  IconFolderPlus,
  IconRefresh,
  IconSearch,
  IconUpload,
} from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import type { SortDirection, SortField } from "../../utils/fileSort";
import type { SearchResultFilter } from "../../composables/useFileSearch";
import EmptyState from "../common/EmptyState.vue";
import FileBrowserCollection from "./FileBrowserCollection.vue";
import FileSkeleton from "./FileSkeleton.vue";
import LibraryGrid from "./LibraryGrid.vue";
import SearchResultToolbar from "./SearchResultToolbar.vue";

defineProps<{
  loading: boolean;
  error: string | null;
  searchActive: boolean;
  searchMode: "name" | "content";
  searchResults: FileInfo[];
  desktopItems: FileInfo[];
  files: FileInfo[];
  showLibraryCards: boolean;
  libraryDirectories: FileInfo[];
  canWrite: boolean;
  canShare: boolean;
  viewMode: "list" | "grid";
  batchMode: boolean;
  selectedPaths: Set<string>;
  flashPath: string;
  expandedPath: string;
  activePath: string;
  detailsVisible: boolean;
  sharedPaths: Set<string>;
  sortField: SortField;
  sortDirection: SortDirection;
  thumbnailSize: number;
  renamingPath: string;
  commit?: string;
  resultFilter: SearchResultFilter;
  searchQuery: string;
}>();

const emit = defineEmits<{
  (e: "retry"): void;
  (e: "upload-files"): void;
  (e: "create-folder"): void;
  (e: "switch-to-name-search"): void;
  (e: "clear-search"): void;
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
  (e: "open-details", file: FileInfo): void;
  (e: "update:filter", value: SearchResultFilter): void;
}>();
</script>
