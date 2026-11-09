<template>
  <div v-if="desktop" class="table-container">
    <table class="table is-fullwidth is-hoverable is-narrow file-list-table">
      <!-- 内容自适应（table-layout: auto）：数据列随内容定宽，名称列吸收剩余空间 -->
      <colgroup>
        <col class="file-list-col-check" />
        <col v-for="column in columns" :key="column.field" />
        <col v-if="showActionColumn" class="file-list-col-actions" />
      </colgroup>
      <thead>
        <tr>
          <th scope="col" class="is-narrow">
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
            scope="col"
            v-for="column in columns"
            :key="column.field"
            :class="[column.narrow ? 'is-narrow' : '', column.align || '']"
            :aria-sort="
              column.sortable === false ? 'none' : ariaSortFor(column.field)
            "
          >
            <!-- 「共享」列只展示徽标，不参与排序：静态列头 -->
            <span
              v-if="column.sortable === false"
              class="file-list-static-label"
              >{{ column.label }}</span
            >
            <button
              v-else
              class="file-list-sort"
              type="button"
              :title="`按${column.label}排序`"
              @click="emit('sort-change', column.field as SortField)"
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
          </th>
          <th
            scope="col"
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
          :flash="flashPath === file.path"
          :selected="
            selectedPaths.has(file.path) ||
            (!selectMode && activePath === file.path)
          "
          :shared="sharedPaths.has(file.path)"
          :expanded="expandedPath === file.path"
          :desktop="true"
          @click="emit('click', file)"
          @download="emit('download', file)"
          @rename="emit('rename', file)"
          :renaming="renamingPath === file.path"
          :show-action-column="showActionColumn"
          :show-location="showLocation"
          :can-write="canWrite"
          :can-share="canShare"
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
      :flash="flashPath === file.path"
      :selected="selectedPaths.has(file.path)"
      :shared="sharedPaths.has(file.path)"
      :expanded="expandedPath === file.path"
      :can-write="canWrite"
      :can-share="canShare"
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
import { IconChevronDown, IconChevronUp } from "@tabler/icons-vue";
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
    /** 落子回执路径（父层持态 ✓ 跨 remount） */
    flashPath?: string;
    /** 搜索结果里显示条目所在目录（主流网盘搜索结果的必要信息）。 */
    showLocation?: boolean;
    /** 列宽（像素）；缺省时使用默认宽度。 */
    highlight?: string;
    selectMode: boolean;
    selectedPaths: Set<string>;
    expandedPath?: string;
    activePath?: string;
    desktop?: boolean;
    sortField?: SortField;
    sortDirection?: SortDirection;
    /** 只读会话隐藏重命名/移动/删除/新建子目录。 */
    canWrite?: boolean;
    /** 受限会话隐藏分享。 */
    canShare?: boolean;
    /** 已分享路径集合（由 FileBrowser 按目录加载一次后传入）。 */
    sharedPaths?: Set<string>;
  }>(),
  {
    showActionColumn: true,
    flashPath: "",
    showLocation: false,
    renamingPath: "",
    highlight: "",
    expandedPath: "",
    activePath: "",
    desktop: false,
    sortField: "name",
    sortDirection: "asc",
    canWrite: true,
    canShare: true,
    sharedPaths: () => new Set<string>(),
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
  field: SortField | "shared";
  label: string;
  narrow?: boolean;
  align?: string;
  /** 「共享」列是状态列，不参与排序。 */
  sortable?: boolean;
}

// Nextcloud 列序：名称 / 共享 / 大小 / 修改时间（复选框与操作列在模板里）。
// 类型不再单列（仍可通过排序菜单按类型排序），把宽度留给共享状态。
const columns: SortColumn[] = [
  { field: "name", label: SORT_FIELD_LABELS.name },
  {
    field: "shared",
    label: "共享",
    narrow: true,
    sortable: false,
  },
  {
    field: "size",
    label: SORT_FIELD_LABELS.size,
    narrow: true,
    align: "has-text-right",
  },
  { field: "modified", label: SORT_FIELD_LABELS.modified, narrow: true },
];

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

function ariaSortFor(
  field: SortField | "shared",
): "ascending" | "descending" | "none" {
  if (field === "shared") return "none";
  if (sortField.value !== field) return "none";
  return sortDirection.value === "asc" ? "ascending" : "descending";
}
</script>

<style scoped>
/* 主流网盘风格：无斑马纹、细分隔线、粘性表头 */
.file-list-table {
  /* 内容自适应（用户反馈定稿）：数据列随内容定宽（操作列内容完整显示），
     名称列由 auto 布局自动吸收剩余空间。 */
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

/* 非排序列头（共享）：与可排序列头同字号/字重，但不显示交互暗示 */
.file-list-static-label {
  display: inline-flex;
  align-items: center;
  white-space: nowrap;
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

.file-list-select-all {
  display: inline-flex;
}
</style>
