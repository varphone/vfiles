<template>
  <div v-if="desktop" class="table-container">
    <table class="table is-fullwidth is-hoverable is-narrow is-striped">
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
          <th class="is-narrow has-text-right">操作</th>
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
import type { SortDirection, SortField } from "../../utils/fileSort";
import FileItem from "./FileItem.vue";

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

const columns: SortColumn[] = [
  { field: "name", label: "名称" },
  { field: "modified", label: "修改日期", narrow: true },
  { field: "type", label: "类型", narrow: true },
  { field: "size", label: "大小", narrow: true, align: "has-text-right" },
];

/** 快捷项（`.`/`..`）不参与“全选”，与批量操作的范围保持一致。 */
const selectableFiles = computed(() =>
  files.value.filter(
    (file) => !(file as FileInfo & { uiRole?: string }).uiRole,
  ),
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
