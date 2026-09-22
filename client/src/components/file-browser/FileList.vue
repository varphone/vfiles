<template>
  <div v-if="desktop" class="table-container">
    <table
      class="table is-fullwidth is-hoverable is-narrow file-list-table"
      :class="{ 'is-resizing': resizingColumn !== null }"
    >
      <!-- 列宽由 store 持久化，拖拽时只改这一处 -->
      <colgroup>
        <col class="file-list-col-check" />
        <col
          v-for="column in columns"
          :key="column.field"
          :style="{ width: `${columnWidth(column.field)}px` }"
        />
        <col v-if="showActionColumn" class="file-list-col-actions" />
      </colgroup>
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
              <IconChevronUp
                v-if="sortField === column.field && sortDirection === 'asc'"
                :size="14"
                class="file-list-sort-icon"
                aria-hidden="true"
              />
              <IconChevronDown
                v-else-if="sortField === column.field"
                :size="14"
                class="file-list-sort-icon"
                aria-hidden="true"
              />
            </button>

            <!-- 列宽拖拽手柄：双击回到默认宽度，键盘 ←/→ 微调 -->
            <span
              class="file-list-resizer"
              :class="{ 'is-active': resizingColumn === column.field }"
              role="separator"
              tabindex="0"
              :aria-label="`调整「${column.label}」列宽`"
              :aria-orientation="'vertical'"
              :title="`拖动调整「${column.label}」列宽（双击恢复默认）`"
              @mousedown.stop.prevent="startResize(column.field, $event)"
              @touchstart.stop.prevent="startResize(column.field, $event)"
              @dblclick.stop.prevent="resetWidth(column.field)"
              @keydown.left.prevent="nudgeWidth(column.field, -16)"
              @keydown.right.prevent="nudgeWidth(column.field, 16)"
              @keydown.home.prevent="resetWidth(column.field)"
            >
              <span class="file-list-resizer-line" aria-hidden="true"></span>
            </span>
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
          :show-location="showLocation"
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
import { computed, onBeforeUnmount, ref, toRefs } from "vue";
import { IconChevronDown, IconChevronUp } from "@tabler/icons-vue";
import {
  DEFAULT_COLUMN_WIDTHS,
  type ColumnWidthKey,
} from "../../stores/fileView.store";
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
  (e: "resize-column", field: ColumnWidthKey, width: number): void;
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
    /** 搜索结果里显示条目所在目录（主流网盘搜索结果的必要信息）。 */
    showLocation?: boolean;
    /** 列宽（像素）；缺省时使用默认宽度。 */
    columnWidths?: Record<ColumnWidthKey, number>;
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
    showLocation: false,
    columnWidths: undefined,
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
  showLocation,
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

/** 只有可排序的列参与拖拽（与 store 的 ColumnWidthKey 一致）。 */
function isResizable(
  field: SortField | null | undefined,
): field is ColumnWidthKey {
  return (
    field === "name" ||
    field === "modified" ||
    field === "type" ||
    field === "size"
  );
}

function columnWidth(field: SortField | null | undefined): number {
  if (!isResizable(field)) return 0;
  return props.columnWidths?.[field] ?? DEFAULT_COLUMN_WIDTHS[field];
}

function resetWidth(field: SortField | null | undefined) {
  if (!isResizable(field)) return;
  emit("resize-column", field, DEFAULT_COLUMN_WIDTHS[field]);
}

function nudgeWidth(field: SortField | null | undefined, delta: number) {
  if (!isResizable(field)) return;
  emit("resize-column", field, columnWidth(field) + delta);
}

const resizingColumn = ref<SortField | null>(null);
let resizeStartX = 0;
let resizeStartWidth = 0;

function startResize(
  field: SortField | null | undefined,
  event: MouseEvent | TouchEvent,
) {
  if (!isResizable(field)) return;
  resizingColumn.value = field;
  resizeStartX = "touches" in event ? event.touches[0].clientX : event.clientX;
  resizeStartWidth = columnWidth(field);

  if ("touches" in event) {
    window.addEventListener("touchmove", onResizeMove, { passive: false });
    window.addEventListener("touchend", stopResize, { once: true });
  } else {
    window.addEventListener("mousemove", onResizeMove);
    window.addEventListener("mouseup", stopResize, { once: true });
  }
}

function onResizeMove(event: MouseEvent | TouchEvent) {
  const field = resizingColumn.value;
  if (!isResizable(field)) return;
  if ("touches" in event) event.preventDefault();
  const clientX =
    "touches" in event
      ? event.touches[0]?.clientX
      : (event as MouseEvent).clientX;
  if (typeof clientX !== "number") return;
  emit("resize-column", field, resizeStartWidth + (clientX - resizeStartX));
}

function stopResize() {
  resizingColumn.value = null;
  window.removeEventListener("mousemove", onResizeMove);
  window.removeEventListener("touchmove", onResizeMove);
}

onBeforeUnmount(stopResize);

/** 快捷项（`.`/`..`）不参与“全选”，与批量操作的范围保持一致。 */
const selectableFiles = computed(() => files.value.filter(Boolean));

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

/* 桌面端由 .desktop-list-shell 统一滚动：Bulma 的 .table-container 默认
   overflow-y: hidden 会把长列表裁掉且无法滚动 */
@media screen and (min-width: 1024px) {
  .table-container {
    overflow: visible;
  }
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
  flex: 0 0 auto;
  color: var(--vf-accent);
}

/* 表头：悬停时给出可拖拽/可排序的反馈 */
.file-list-table thead th {
  position: relative;
  user-select: none;
}

.file-list-table thead th:hover {
  /* 必须用不透明底：surface-hover 是半透明 tint（行悬停叠色用），
     换给 sticky 表头会让滚动内容透出（用户反馈的"表头悬停变透明"）。 */
  background: var(--vf-surface-sunken);
}

.file-list-table thead th:hover .file-list-sort {
  color: var(--vf-text-strong);
}

/* 列宽拖拽手柄：贴住列右边界，平时只显示细线 */
.file-list-resizer {
  position: absolute;
  top: 0;
  right: 0;
  z-index: 3;
  display: flex;
  align-items: center;
  /* 手柄整体留在本列内：跨到下一列会被相邻 th 抢走命中区域（点击/双击失效） */
  justify-content: flex-end;
  width: 9px;
  height: 100%;
  cursor: col-resize;
  touch-action: none;
}

.file-list-resizer-line {
  width: 1px;
  height: 60%;
  background: var(--vf-border);
  transition: background 0.12s ease;
}

.file-list-resizer:hover .file-list-resizer-line,
.file-list-resizer:focus-visible .file-list-resizer-line,
.file-list-resizer.is-active .file-list-resizer-line {
  width: 2px;
  background: var(--vf-accent);
}

.file-list-table.is-resizing {
  cursor: col-resize;
}

.file-list-select-all {
  display: inline-flex;
}
</style>
