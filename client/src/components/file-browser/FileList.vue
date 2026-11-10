<template>
  <div v-if="desktop" class="table-container">
    <table ref="tableRef" class="table is-fullwidth is-hoverable is-narrow file-list-table">
      <!-- 内容自适应（table-layout: auto）：数据列随内容定宽，名称列吸收剩余空间 -->
      <colgroup>
        <col class="file-list-col-check" />
        <col v-for="column in columns" :key="column.field" />
        <col class="file-list-col-rowend" />
      </colgroup>
      <thead>
        <tr>
          <th scope="col" class="is-narrow">
            <!-- 表头全选常驻：行首复选框默认隐藏，全选入口必须始终可见/可达 -->
            <label class="file-list-select-all" @click.stop>
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
            :aria-sort="ariaSortFor(column.field)"
          >
            <button
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
          <!-- 行尾：⋯ 菜单 + 行操作图标（列头保持空白，与 Nextcloud 一致） -->
          <th
            scope="col"
            class="is-narrow has-text-right"
            :class="{ 'file-list-actions-header': showActionColumn }"
            aria-label="操作"
          ></th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-if="padTop > 0"
          class="file-list-spacer"
          aria-hidden="true"
        >
          <td :colspan="columnSpan" :style="{ height: `${padTop}px` }"></td>
        </tr>
        <FileItem
          v-for="file in windowFiles"
          :key="file.path"
          v-memo="[
            file,
            highlight,
            selectMode,
            flashPath === file.path,
            selectedPaths.has(file.path),
            !selectMode && activePath === file.path,
            sharedPaths.has(file.path),
            publicSharedPaths.has(file.path),
            expandedPath === file.path,
            renamingPath === file.path,
            showActionColumn,
            showLocation,
            canWrite,
            canShare,
          ]"
          :file="file"
          :highlight="highlight"
          :select-mode="selectMode"
          :flash="flashPath === file.path"
          :selected="selectedPaths.has(file.path)"
          :active="!selectMode && activePath === file.path"
          :shared="sharedPaths.has(file.path)"
          :shared-public="publicSharedPaths.has(file.path)"
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
        <tr
          v-if="padBottom > 0"
          class="file-list-spacer"
          aria-hidden="true"
        >
          <td :colspan="columnSpan" :style="{ height: `${padBottom}px` }"></td>
        </tr>
      </tbody>
    </table>

    <!-- 表尾汇总：与数据行同一列宽，不属于表格行（无悬停态） -->
    <div v-if="files.length > 0" class="file-list-summary">
      <span class="file-list-summary-counts">
        {{ fileCount }} 个文件 · {{ directoryCount }} 个文件夹
      </span>
      <span class="file-list-summary-size">{{ totalSizeLabel }}</span>
    </div>
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
      :shared-public="publicSharedPaths.has(file.path)"
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
import { computed, nextTick, onBeforeUnmount, onMounted, ref, toRefs, watch } from "vue";
import { IconChevronDown, IconChevronUp } from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import {
  SORT_FIELD_LABELS,
  type SortDirection,
  type SortField,
} from "../../utils/fileSort";
import { formatSize } from "../../utils/filePresentation";
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
    /** 是否显示行尾操作图标组（详情面板可见时可关掉，只保留 ⋯ 菜单）。 */
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
    /**
     * 公开链接分享路径集合（可选）。
     *
     * `sharedPaths` 只说明「有分享」，命中这里时名称列改用链接图标；
     * 未提供时按内部分享（人物图标）展示。
     */
    publicSharedPaths?: Set<string>;
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
    publicSharedPaths: () => new Set<string>(),
  },
);

const {
  files,
  showActionColumn,
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

// Nextcloud 列序：名称 / 大小 / 修改时间（复选框与行尾操作列在模板里）。
// 「共享」不再单列：已共享标记内联在名称列里（图标 + Shared）。
const columns: SortColumn[] = [
  { field: "name", label: SORT_FIELD_LABELS.name },
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

/** 导航快捷项（`.` / `..`）不是当前目录里的真实条目，不计入表尾汇总。 */
function isShortcut(file: FileInfo): boolean {
  return Boolean((file as FileInfo & { uiRole?: string }).uiRole);
}

const listedFiles = computed(() =>
  files.value.filter((file) => file && !isShortcut(file)),
);
const fileCount = computed(
  () => listedFiles.value.filter((file) => file.kind === "file").length,
);

const directoryCount = computed(
  () => listedFiles.value.filter((file) => file.kind === "directory").length,
);

/** 当前目录所有已加载文件大小之和（目录本身不计入）。 */
const totalBytes = computed(() =>
  listedFiles.value.reduce(
    (sum, file) => (file.kind === "file" ? sum + (file.size_bytes ?? 0) : sum),
    0,
  ),
);

const totalSizeLabel = computed(() => formatSize(totalBytes.value));

function ariaSortFor(field: SortField): "ascending" | "descending" | "none" {
  if (sortField.value !== field) return "none";
  return sortDirection.value === "asc" ? "ascending" : "descending";
}

/* ------------------------------------------------------------------
 * 桌面长列表窗口化
 *
 * 40 条以内全部渲染（首屏/普通目录）；超过阈值且容器有真实布局时，
 * 只渲染视口附近的窗口，用上下两个占位行撑出完整滚动高度。
 * jsdom 的 clientHeight 为 0，自动退化为全量渲染，组件测试不受影响。
 * ------------------------------------------------------------------ */
const VIRTUAL_MIN_ROWS = 60;
const OVERSCAN = 8;
const HEADER_HEIGHT = 44;

const tableRef = ref<HTMLTableElement | null>(null);
const scrollHost = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);
const rowHeight = ref(51);
const virtualEnabled = ref(false);
const pinnedIndex = ref<number | null>(null);
/** 桌面列表在部分布局下由页面滚动（此时容器高度等于内容高度）。 */
const scrollMode = ref<"host" | "window">("host");

const columnSpan = computed(() => columns.length + 2);
const rowCount = computed(() => files.value.length);
const viewportRows = computed(() =>
  Math.max(1, Math.ceil(viewportHeight.value / rowHeight.value)),
);

function updateScrollPosition() {
  const host = scrollHost.value;
  const table = tableRef.value;
  if (!host || !table) return;
  if (scrollMode.value === "host") {
    scrollTop.value = host.scrollTop;
  } else {
    // 页面滚动：表格顶部越过视口顶部的距离就是“已滚过”的内容高度。
    scrollTop.value = Math.max(0, -table.getBoundingClientRect().top);
  }
}

function measure() {
  const host = scrollHost.value;
  const table = tableRef.value;
  if (!host || !table) return;
  const internallyScrollable =
    host.clientHeight > 0 && host.scrollHeight > host.clientHeight + 1;
  scrollMode.value = internallyScrollable ? "host" : "window";
  viewportHeight.value = internallyScrollable
    ? host.clientHeight
    : window.innerHeight;
  const row = host.querySelector<HTMLElement>("tr.desktop-file-row");
  const measured = row?.getBoundingClientRect().height ?? 0;
  if (measured > 0) rowHeight.value = measured;
  // clientHeight 为 0（jsdom）时保持全量渲染。
  virtualEnabled.value =
    host.clientHeight > 0 && rowCount.value > VIRTUAL_MIN_ROWS;
  updateScrollPosition();
}

const windowStart = computed(() => {
  if (!virtualEnabled.value) return 0;
  const base = Math.min(
    Math.max(
      0,
      Math.floor(Math.max(0, scrollTop.value - HEADER_HEIGHT) / rowHeight.value) -
        OVERSCAN,
    ),
    Math.max(0, rowCount.value - 1),
  );
  const pinned = pinnedIndex.value;
  if (pinned !== null && (pinned < base || pinned >= base + viewportRows.value)) {
    return Math.max(0, Math.min(pinned - OVERSCAN, rowCount.value - 1));
  }
  return base;
});

const windowEnd = computed(() => {
  if (!virtualEnabled.value) return rowCount.value;
  return Math.min(
    rowCount.value,
    windowStart.value + viewportRows.value + OVERSCAN * 2,
  );
});

const windowFiles = computed(() =>
  files.value.slice(windowStart.value, windowEnd.value),
);
const padTop = computed(() =>
  virtualEnabled.value ? windowStart.value * rowHeight.value : 0,
);
const padBottom = computed(() =>
  virtualEnabled.value
    ? (rowCount.value - windowEnd.value) * rowHeight.value
    : 0,
);

let scrollFrame = 0;
let resizeObserver: ResizeObserver | null = null;
let classObserver: MutationObserver | null = null;

function onScroll() {
  if (scrollFrame) return;
  scrollFrame = requestAnimationFrame(() => {
    scrollFrame = 0;
    const host = scrollHost.value;
    if (!host) return;
    updateScrollPosition();
    pinnedIndex.value = null;
  });
}

onMounted(() => {
  const host = tableRef.value?.closest<HTMLElement>(".desktop-list-shell");
  if (!host) return;
  scrollHost.value = host;
  measure();
  host.addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("resize", measure);
  if (typeof ResizeObserver !== "undefined") {
    resizeObserver = new ResizeObserver(measure);
    resizeObserver.observe(host);
  }
  classObserver = new MutationObserver(measure);
  classObserver.observe(host, { attributes: true, attributeFilter: ["class"] });
});

onBeforeUnmount(() => {
  scrollHost.value?.removeEventListener("scroll", onScroll);
  window.removeEventListener("scroll", onScroll);
  window.removeEventListener("resize", measure);
  resizeObserver?.disconnect();
  classObserver?.disconnect();
  if (scrollFrame) cancelAnimationFrame(scrollFrame);
});

watch(
  () => files.value.length,
  () => {
    pinnedIndex.value = null;
    void nextTick(measure);
  },
);

// 键盘把活动行移出窗口时，先把它拉回渲染范围，再由父组件 scrollIntoView 定位。
watch(
  () => activePath.value,
  (path) => {
    if (!virtualEnabled.value) return;
    const index = files.value.findIndex((file) => file.path === path);
    if (index < 0) return;
    if (index < windowStart.value || index >= windowEnd.value) {
      pinnedIndex.value = index;
    }
  },
);
</script>

<style scoped>
/* Nextcloud 列表：无斑马纹、细分隔线、粘性表头 */
.file-list-table {
  /* 内容自适应：数据列随内容定宽，名称列由 auto 布局自动吸收剩余空间。 */
  background: transparent;
  margin-bottom: 0;
}

/* 表头：44px 高、灰色 13px */
.file-list-table thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  height: var(--vf-table-head-height);
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
  font-weight: 600;
  letter-spacing: 0.02em;
  white-space: nowrap;
  user-select: none;
  vertical-align: middle;
  line-height: 1.35;
}

/* 单元格由 FileItem 渲染：用 :deep 才能作用到子组件的 <td>
   （与表尾汇总的 16px 内边距对齐） */
.file-list-table :deep(tbody td) {
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  vertical-align: middle;
}

.file-list-table :deep(tbody tr:last-child td) {
  border-bottom: none;
}

/* 行尾列（⋯ + 行操作图标）贴右侧，不参与名称列宽度竞争 */
.file-list-col-rowend {
  width: 1%;
}

/* 窗口化占位行：撑高滚动区，不参与悬停/分隔线 */
.file-list-spacer td {
  padding: 0;
  border: 0;
  line-height: 0;
}

/* 表尾汇总：不属于数据行，无悬停态；与表头同高、同 16px 基线 */
.file-list-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--vf-space-3);
  min-height: var(--vf-table-head-height);
  padding: 0 var(--vf-cell-padding-x);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
}

.file-list-summary-counts,
.file-list-summary-size {
  white-space: nowrap;
}

/* 桌面端由 .desktop-list-shell 统一滚动：Bulma 的 .table-container 默认
   overflow-y: hidden 会把长列表裁掉且无法滚动 */
@media screen and (min-width: 1024px) {
  .table-container {
    overflow: visible;
  }
}

/* 表头：悬停时给出可排序的反馈 */
.file-list-table thead th:hover {
  /* 必须用不透明底：行悬停令牌是不透明中性灰，sticky 表头不会透出滚动内容。 */
  background: var(--vf-row-hover);
}

.file-list-table thead th:hover .file-list-sort {
  color: var(--vf-text);
}

.file-list-sort {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-space-1);
  /* 44px 表头全高可点（13+18+13），负外边距保持行高 */
  padding: 13px 0;
  margin: -13px 0;
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

.file-list-select-all {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  /* 与行内复选框同一命中区口径（25px）。 */
  padding: 6px;
  margin: -6px;
}
</style>
