<template>
  <FileSkeleton
    v-if="loading"
    :variant="viewMode === 'grid' ? 'grid' : 'list'"
    :thumbnail-size="thumbnailSize"
  />

  <div v-else-if="error" class="notification is-danger is-light">
    <IconAlertCircle :size="20" class="mr-2" />
    {{ error }}
    <div class="mt-2">
      <button
        class="button is-small is-danger is-light"
        :class="{ 'is-loading': loading }"
        :disabled="loading"
        @click="emit('retry')"
      >
        <IconRefresh :size="16" class="mr-1" />
        重试
      </button>
    </div>
  </div>

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
      canWrite ? '点击右下角按钮上传文件' : '只读访问：当前目录没有可浏览的条目'
    "
  />

  <div v-else-if="searchActive" class="file-list">
    <SearchResultToolbar
      :results="searchResults"
      :active="resultFilter"
      :query="searchQuery"
      :visible-count="visibleSearchResults.length"
      @update:active="emit('update:filter', $event)"
    />
    <EmptyState
      v-if="searchResults.length === 0"
      :icon="IconSearch"
      compact
      title="没有找到匹配的文件"
      hint="换个关键字，或清空搜索条件"
    >
      <template #actions>
        <button class="vf-ghost-button" @click="emit('clear-search')">
          <span>清空搜索</span>
        </button>
      </template>
    </EmptyState>
    <FileBrowserCollection
      v-if="visibleSearchResults.length"
      :files="visibleSearchResults"
      :view-mode="viewMode"
      :can-write="canWrite"
      :can-share="canShare"
      :highlight="searchQuery"
      :commit="commit"
      :select-mode="batchMode"
      :selected-paths="selectedPaths"
      :flash-path="flashPath"
      :expanded-path="expandedPath"
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
      @share="emit('share', $event)"
      @preview="emit('preview', $event)"
      @open-folder="emit('open-folder', $event)"
      @create-directory="emit('create-directory', $event)"
    />
    <slot name="load-more" />
  </div>

  <div v-else class="file-list">
    <FileBrowserCollection
      :files="visibleFiles"
      :view-mode="viewMode"
      :can-write="canWrite"
      :can-share="canShare"
      :commit="commit"
      :select-mode="batchMode"
      :selected-paths="selectedPaths"
      :flash-path="flashPath"
      :expanded-path="expandedPath"
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
      @share="emit('share', $event)"
      @preview="emit('preview', $event)"
      @open-folder="emit('open-folder', $event)"
      @create-directory="emit('create-directory', $event)"
    />
    <slot name="load-more" />
  </div>
</template>

<script setup lang="ts">
import { IconAlertCircle, IconFolderOpen, IconRefresh, IconSearch } from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import EmptyState from "../common/EmptyState.vue";
import FileBrowserCollection from "./FileBrowserCollection.vue";
import FileSkeleton from "./FileSkeleton.vue";
import LibraryGrid from "./LibraryGrid.vue";
import SearchResultToolbar from "./SearchResultToolbar.vue";
import type { SearchResultFilter } from "../../composables/useFileSearch";

defineProps<{
  loading: boolean;
  error: string | null;
  searchActive: boolean;
  showLibraryCards: boolean;
  libraryDirectories: FileInfo[];
  files: FileInfo[];
  visibleFiles: FileInfo[];
  searchResults: FileInfo[];
  visibleSearchResults: FileInfo[];
  searchQuery: string;
  resultFilter: SearchResultFilter;
  canWrite: boolean;
  canShare: boolean;
  viewMode: "list" | "grid";
  batchMode: boolean;
  selectedPaths: Set<string>;
  flashPath: string;
  expandedPath: string;
  thumbnailSize: number;
  renamingPath: string;
  commit?: string;
}>();

const emit = defineEmits<{
  (e: "retry"): void;
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
  (e: "share", file: FileInfo): void;
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
  (e: "create-directory", file: FileInfo): void;
  (e: "open-details", file: FileInfo): void;
  (e: "update:filter", value: SearchResultFilter): void;
  (e: "clear-search"): void;
}>();
</script>
