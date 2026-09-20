<template>
  <div v-if="desktop" class="table-container">
    <table class="table is-fullwidth is-hoverable is-narrow file-list-table">
      <thead>
        <tr>
          <th class="is-narrow">
            <label v-if="selectMode" class="file-list-select-all" @click.stop>
              <input
                type="checkbox"
                :checked="allSelected"
                :indeterminate.prop="someSelected && !allSelected"
                aria-label="全选当前视图"
                @change="emit('toggle-select-all')"
              />
            </label>
          </th>
          <th
            v-for="column in columns"
            :key="column.field"
            :class="[column.narrow ? 'is-narrow' : '', column.align || '']"
            :aria-sort="ariaSortFor(column.field)"
          >
            <button
              class="file-list-sort"
              type="button"
              :title="`按${column.label}排序`"
              @click="emit('sort-change', column.field)"
            >
              <span>{{ column.label }}</span>
              <span
                v-if="sortField === column.field"
                class="file-list-sort-icon"
                aria-hidden="true"
                >{{ sortDirection === "asc" ? "▲" : "▼" }}</span
              >
            </button>
          </th>
          <th
            v-if="showActionColumn"
            class="is-narrow has-text-right file-list-actions-header"
          >
            操作
          </th>
        </tr>
      </thead>
      <tbody>
        <FileItem
          v-for="file in files"
          :key="file.path"
          :file="file"
          :highlight="highlight"
          :select-mode="selectMode"
          :selected="
            selectedPaths.has(file.path) ||
            (!selectMode && activePath === file.path)
          "
          :expanded="expandedPath === file.path"
          :desktop="true"
          @click="emit('click', file)"
          @download="emit('download', file)"
          @rename="emit('rename', file)"
          :renaming="renamingPath === file.path"
          :show-action-column="showActionColumn"
          @rename-commit="(target, name) => emit('rename-commit', target, name)"
          @rename-cancel="(target) => emit('rename-cancel', target)"
          @move="emit('move', file)"
          @delete="emit('delete', file)"
          @view-history="emit('view-history', file)"
          @toggle-select="emit('toggle-select', file)"
          @modifier-select="emit('modifier-select', $event)"
          @context-menu="emit('context-menu', $event)"
          @drag-start="emit('drag-start', $event)"
          @drag-end="emit('drag-end')"
          @drop-on-folder="emit('drop-on-folder', $event)"
          @share="emit('share', file)"
          @preview="emit('preview', file)"
          @open-folder="emit('open-folder', file)"
          @create-directory="emit('create-directory', file)"
        />
      </tbody>
    </table>
  </div>

  <template v-else>
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
      @rename="emit('rename', file)"
      :renaming="renamingPath === file.path"
      @rename-commit="(target, name) => emit('rename-commit', target, name)"
      @rename-cancel="(target) => emit('rename-cancel', target)"
      @move="emit('move', file)"
      @delete="emit('delete', file)"
      @view-history="emit('view-history', file)"
      @toggle-select="emit('toggle-select', file)"
      @modifier-select="emit('modifier-select', $event)"
      @context-menu="emit('context-menu', $event)"
      @drag-start="emit('drag-start', $event)"
      @drag-end="emit('drag-end')"
      @drop-on-folder="emit('drop-on-folder', $event)"
      @share="emit('share', file)"
      @preview="emit('preview', file)"
      @open-folder="emit('open-folder', file)"
      @create-directory="emit('create-directory', file)"
    />
  </template>
</template>

<script setup lang="ts">
import { computed, toRefs } from "vue";
import type { FileInfo } from "../../types";
import {
  SORT_FIELD_LABELS,
  type SortDirection,
  type SortField,
} from "../../utils/fileSort";
import FileItem from "./FileItem.vue";

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
  (e: "share", file: FileInfo): void;
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
  (e: "create-directory", file: FileInfo): void;
  (e: "sort-change", field: SortField): void;
  (e: "toggle-select-all"): void;
  (
    e: "modifier-select",
    payload: { file: FileInfo; shift: boolean; meta: boolean },
  ): void;
  (e: "context-menu", payload: { file: FileInfo; x: number; y: number }): void;
  (e: "drag-start", file: FileInfo): void;
  (e: "drag-end"): void;
  (e: "drop-on-folder", targetDir: string): void;
}>();

const props = withDefaults(
  defineProps<{
    files: FileInfo[];
    renamingPath?: string;
    /** 是否显示「操作」列；详情面板可见时可关掉，避免与面板里的操作重复。 */
    showActionColumn?: boolean;
    highlight?: string;
    selectMode: boolean;
    selectedPaths: Set<string>;
    expandedPath?: string;
    activePath?: string;
    desktop?: boolean;
    sortField?: SortField;
    sortDirection?: SortDirection;
  }>(),
  {
    showActionColumn: true,
    renamingPath: "",
    highlight: "",
    expandedPath: "",
    activePath: "",
    desktop: false,
    sortField: "name",
    sortDirection: "asc",
  },
);

const {
  files,
  highlight,
  selectMode,
  selectedPaths,
  expandedPath,
  activePath,
  sortField,
  sortDirection,
} = toRefs(props);

interface SortColumn {
  field: SortField;
  label: string;
  narrow?: boolean;
  align?: string;
}

// 列头文案复用共享的排序字段标签，避免同一字段在表头与排序菜单里叫法不同
const columns: SortColumn[] = [
  { field: "name", label: SORT_FIELD_LABELS.name },
  { field: "modified", label: SORT_FIELD_LABELS.modified, narrow: true },
  { field: "type", label: SORT_FIELD_LABELS.type, narrow: true },
  {
    field: "size",
    label: SORT_FIELD_LABELS.size,
    narrow: true,
    align: "has-text-right",
  },
];

/** 快捷项（`.`/`..`）不参与“全选”，与批量操作的范围保持一致。 */
const selectableFiles = computed(() =>
  files.value.filter(Boolean),
);

const allSelected = computed(
  () =>
    selectableFiles.value.length > 0 &&
    selectableFiles.value.every((file) => selectedPaths.value.has(file.path)),
);

const someSelected = computed(() =>
  selectableFiles.value.some((file) => selectedPaths.value.has(file.path)),
);

function ariaSortFor(field: SortField): "ascending" | "descending" | "none" {
  if (sortField.value !== field) return "none";
  return sortDirection.value === "asc" ? "ascending" : "descending";
}
</script>

<style scoped>
/* 主流网盘风格：无斑马纹、细分隔线、粘性表头 */
.file-list-table {
  background: transparent;
  margin-bottom: 0;
}

.file-list-table thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  padding: 0.6rem 0.75rem;
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface);
  color: var(--vf-text-muted);
  font-size: 0.75rem;
  font-weight: 600;
  letter-spacing: 0.02em;
  white-space: nowrap;
}

.file-list-table tbody td {
  padding: 0 0.75rem;
  border-bottom: 1px solid var(--vf-border-weak);
  vertical-align: middle;
}

.file-list-table tbody tr:last-child td {
  border-bottom: none;
}

.file-list-sort {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}

.file-list-sort:hover {
  color: var(--vf-accent);
}

.file-list-sort-icon {
  font-size: 0.65rem;
  line-height: 1;
}

.file-list-select-all {
  display: inline-flex;
}
</style>
