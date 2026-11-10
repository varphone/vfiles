<template>
  <div
    ref="gridRef"
    class="file-grid"
    :class="{ 'is-density-compact': view.density === 'compact' }"
    :style="[
      { '--file-card-min': `${cardMinWidth(thumbnailSize)}px` },
      virtualGridStyle,
    ]"
  >
    <FileCard
      v-for="file in windowCards"
      :key="file.path"
      v-memo="[
        file,
        commit,
        highlight,
        selectMode,
        selectedPaths.has(file.path),
        activePath === file.path,
        showLocation,
        thumbnailSize,
        canWrite,
        canShare,
        renamingPath === file.path,
      ]"
      :file="file"
      :commit="commit"
      :highlight="highlight"
      :select-mode="selectMode"
      :selected="selectedPaths.has(file.path)"
      :active="activePath === file.path"
      :show-location="showLocation"
      :thumbnail-size="thumbnailSize"
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
  </div>
</template>

<script setup lang="ts">
import { useFileViewStore } from "../../stores/fileView.store";
import { computed, ref, toRefs, watch } from "vue";
import type { FileInfo } from "../../types";
import FileCard from "./FileCard.vue";
import { useVirtualWindow } from "../../composables/useVirtualWindow";

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
    highlight?: string;
    commit?: string;
    selectMode: boolean;
    selectedPaths: Set<string>;
    activePath?: string;
    thumbnailSize?: number;
    /** 搜索结果中显示所在目录 */
    showLocation?: boolean;
    /** 只读会话隐藏重命名/移动/删除/新建子目录。 */
    canWrite?: boolean;
    /** 受限会话隐藏分享。 */
    canShare?: boolean;
  }>(),
  {
    renamingPath: "",
    highlight: "",
    commit: undefined,
    activePath: "",
    thumbnailSize: 144,
    canWrite: true,
    canShare: true,
  },
);

const {
  files,
  highlight,
  commit,
  selectMode,
  selectedPaths,
  activePath,
  thumbnailSize,
} = toRefs(props);

/**
 * 网格视图通过面包屑/工具栏完成“返回上一级”，
 * 因此过滤掉列表视图专用的“.”“..”快捷项，与主流云盘保持一致。
 */
const cards = computed(() =>
  files.value.filter(
    (file) => !(file as FileInfo & { uiRole?: string }).uiRole,
  ),
);

/* ------------------------------------------------------------------
 * 网格窗口化：按「行」保留视口附近的卡片，用上下 padding 撑出完整高度。
 * 列数随容器宽度变化，因此每次测量都重新读取 grid-template-columns。
 * jsdom 没有布局，enabled 为 false，测试仍渲染全部卡片。
 * ------------------------------------------------------------------ */
const gridRef = ref<HTMLDivElement | null>(null);
const itemsPerRow = ref(1);

function measureColumns(): number {
  const el = gridRef.value;
  if (!el) return itemsPerRow.value;
  const columns = getComputedStyle(el)
    .gridTemplateColumns.split(" ")
    .filter(Boolean).length;
  itemsPerRow.value = Math.max(1, columns);
  return itemsPerRow.value;
}

function measureRowHeight(): number {
  const el = gridRef.value;
  if (!el) return 0;
  const columns = measureColumns();
  const cards = el.querySelectorAll<HTMLElement>(".file-card");
  if (cards.length === 0) return 0;
  const first = cards[0].getBoundingClientRect();
  const nextRow = cards[columns]?.getBoundingClientRect();
  if (nextRow && nextRow.top > first.top) {
    return nextRow.top - first.top;
  }
  const gap = Number.parseFloat(getComputedStyle(el).rowGap) || 0;
  return first.height + gap;
}

const {
  start,
  end,
  padTop,
  padBottom,
  pin,
} = useVirtualWindow({
  anchor: gridRef,
  itemCount: computed(() => cards.value.length),
  itemsPerRow,
  measureRowHeight,
  threshold: 60,
  overscan: 2,
});

const windowCards = computed(() => cards.value.slice(start.value, end.value));
const virtualGridStyle = computed(() => ({
  paddingTop: `${padTop.value}px`,
  paddingBottom: `${padBottom.value}px`,
}));

watch(
  () => props.activePath,
  (path) => {
    const index = cards.value.findIndex((file) => file.path === path);
    if (index >= 0) pin(index);
  },
);
import { cardMinWidth } from "../../stores/fileView.store";

const view = useFileViewStore();
</script>

<style scoped>
.file-grid {
  display: grid;
  grid-template-columns: repeat(
    auto-fill,
    minmax(var(--file-card-min, 190px), 1fr)
  );
  gap: var(--vf-section-gap);
  padding: var(--vf-page-padding);
}

.file-grid.is-density-compact {
  gap: var(--vf-space-2);
  padding: var(--vf-space-2);
}
</style>
