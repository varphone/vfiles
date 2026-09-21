<template>
  <aside class="desktop-details" aria-label="详细信息">
    <!-- 多选：展示汇总与批量操作（主流网盘的选择摘要），单选才展示条目详情 -->
    <div v-if="isMultiSelection" class="details-selection">
      <header class="details-selection-head">
        <div class="details-selection-titles">
          <p class="details-selection-title">
            已选择 {{ selection.length }} 项
          </p>
          <p class="details-selection-meta">
            {{ selectionSummary }}
          </p>
        </div>
        <button
          class="vf-icon-button"
          type="button"
          title="关闭详细信息"
          aria-label="关闭详细信息"
          @click="emit('close')"
        >
          <IconX :size="16" />
        </button>
      </header>

      <ul class="details-selection-list">
        <li
          v-for="file in selectionPreview"
          :key="file.path"
          class="details-selection-row"
        >
          <FileTypeIcon :file="file" :size="16" />
          <span class="details-selection-name" :title="file.name">
            {{ file.name }}
          </span>
        </li>
        <li v-if="hiddenSelectionCount > 0" class="details-selection-more">
          还有 {{ hiddenSelectionCount }} 项
        </li>
      </ul>

      <div class="details-selection-actions">
        <button
          class="vf-ghost-button is-primary details-primary"
          type="button"
          @click="emit('download-selection')"
        >
          <IconDownload :size="16" />
          <span>下载全部</span>
        </button>
        <button
          class="vf-ghost-button"
          type="button"
          @click="emit('move-selection')"
        >
          <IconArrowsDiff :size="16" />
          <span>移动</span>
        </button>
        <button
          class="vf-ghost-button"
          type="button"
          title="全选当前视图"
          @click="emit('select-all')"
        >
          <IconChecklist :size="16" />
          <span>全选</span>
        </button>
        <button
          class="vf-ghost-button details-selection-wide"
          type="button"
          @click="emit('transfer-selection')"
        >
          <IconUserShare :size="16" />
          <span>转移所有权</span>
        </button>
        <button
          class="vf-ghost-button details-selection-wide"
          type="button"
          @click="emit('clear-selection')"
        >
          <span>清空选择</span>
        </button>
        <button
          class="vf-ghost-button is-danger details-danger"
          type="button"
          @click="emit('delete-selection')"
        >
          <IconTrash :size="16" />
          <span>删除全部</span>
        </button>
      </div>
    </div>

    <template v-else-if="item">
      <FileDetailsContent
        :file="item"
        :preview-url="previewUrl"
        show-close
        @close="emit('close')"
      >
        <template #actions>
          <button
            v-if="item.kind === 'file'"
            class="vf-ghost-button is-primary details-primary"
            @click="emit('preview', item)"
          >
            <IconEye :size="16" />
            <span>预览</span>
          </button>
          <button
            v-else
            class="vf-ghost-button is-primary details-primary"
            @click="emit('open-folder', item)"
          >
            <IconFolderOpen :size="16" />
            <span>打开</span>
          </button>
          <button class="vf-ghost-button" @click="emit('download', item)">
            <IconDownload :size="16" />
            <span>下载</span>
          </button>
          <button class="vf-ghost-button" @click="emit('share', item)">
            <IconShare :size="16" />
            <span>分享</span>
          </button>
          <button class="vf-ghost-button" @click="emit('view-history', item)">
            <IconHistory :size="16" />
            <span>历史版本</span>
          </button>
          <button class="vf-ghost-button" @click="emit('rename', item)">
            <IconPencil :size="16" />
            <span>重命名</span>
          </button>
          <button class="vf-ghost-button" @click="emit('move', item)">
            <IconArrowsDiff :size="16" />
            <span>移动</span>
          </button>
          <button class="vf-ghost-button" @click="emit('transfer', item)">
            <IconUserShare :size="16" />
            <span>转移所有权</span>
          </button>
          <button
            class="vf-ghost-button is-danger details-danger"
            @click="emit('delete', item)"
          >
            <IconTrash :size="16" />
            <span>删除</span>
          </button>
        </template>
      </FileDetailsContent>
    </template>

    <EmptyState
      v-else
      :icon="IconInfoCircle"
      compact
      title="未选择任何条目"
      hint="选中文件或文件夹后，这里会显示详细信息"
    />
  </aside>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  IconArrowsDiff,
  IconChecklist,
  IconDownload,
  IconEye,
  IconFolderOpen,
  IconHistory,
  IconInfoCircle,
  IconPencil,
  IconShare,
  IconTrash,
  IconUserShare,
  IconX,
} from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import { filesService } from "../../services/files.service";
import { formatSize, isImageFile } from "../../utils/filePresentation";
import EmptyState from "../common/EmptyState.vue";
import FileDetailsContent from "./FileDetailsContent.vue";
import FileTypeIcon from "./FileTypeIcon.vue";

/**
 * 桌面端右侧「详细信息」面板。
 *
 * 只负责展示与事件上抛：当前条目、是否显示由父组件决定，避免这里再持有
 * 列表/选择状态。
 */
const props = withDefaults(
  defineProps<{
    item?: FileInfo;
    /** 浏览版本（用于按版本取缩略图）。 */
    commit?: string;
    /** 批量选择中的条目；多于 1 项时展示选择摘要而不是单条详情。 */
    selection?: FileInfo[];
  }>(),
  { selection: () => [] },
);

const emit = defineEmits<{
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
  (e: "download", file: FileInfo): void;
  (e: "share", file: FileInfo): void;
  (e: "view-history", file: FileInfo): void;
  (e: "rename", file: FileInfo): void;
  (e: "move", file: FileInfo): void;
  (e: "transfer", file: FileInfo): void;
  (e: "delete", file: FileInfo): void;
  (e: "close"): void;
  (e: "download-selection"): void;
  (e: "transfer-selection"): void;
  (e: "move-selection"): void;
  (e: "delete-selection"): void;
  (e: "select-all"): void;
  (e: "clear-selection"): void;
}>();

const isMultiSelection = computed(() => props.selection.length > 1);

/** 选择摘要：条目数 + 文件大小合计 + 目录数量。 */
const selectionSummary = computed(() => {
  const files = props.selection.filter((file) => file.kind === "file");
  const directories = props.selection.length - files.length;
  const totalBytes = files.reduce(
    (sum, file) => sum + (file.size_bytes ?? 0),
    0,
  );
  const parts = [`共 ${formatSize(totalBytes)}`];
  if (directories > 0) parts.push(`${directories} 个目录`);
  return parts.join(" · ");
});

const selectionPreview = computed(() => props.selection.slice(0, 5));
const hiddenSelectionCount = computed(() =>
  Math.max(0, props.selection.length - selectionPreview.value.length),
);

/** 图片文件才请求缩略图；其它类型由内容组件回退到类型图标。 */
const previewUrl = computed(() => {
  const item = props.item;
  if (!item || item.kind !== "file" || !isImageFile(item)) return undefined;
  return filesService.thumbnailUrl(item.path, {
    commit: props.commit,
    size: 320,
  });
});
</script>

<style scoped>
.desktop-details {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.1rem;
  border-left: 1px solid var(--vf-border-weak);
  background: var(--vf-surface);
  min-width: 0;
  /* 与列表一致：面板自身吸顶并内部滚动，长列表滚动时信息与操作始终可见 */
  position: sticky;
  top: calc(
    var(--bulma-navbar-height, 3.25rem) + env(safe-area-inset-top) + 0.6rem
  );
  align-self: start;
  /* 高度受内容区（grid 行）限制，而不是视口：
     用视口高度会让面板越过内容区压住底部状态栏，挡住状态栏右侧的操作 */
  max-height: 100%;
  overflow-y: auto;
}

/* 多选摘要 */
.details-selection {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 1rem 1.1rem;
}

.details-selection-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.5rem;
}

.details-selection-titles {
  min-width: 0;
}

.details-selection-title {
  margin: 0;
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.details-selection-meta {
  margin: 0.15rem 0 0;
  color: var(--vf-text-muted);
  font-size: 0.78rem;
}

.details-selection-list {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  margin: 0;
  padding: 0.4rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
  list-style: none;
}

.details-selection-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  min-width: 0;
  color: var(--vf-text);
  font-size: 0.82rem;
}

.details-selection-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.details-selection-more {
  color: var(--vf-text-subtle);
  font-size: 0.76rem;
}

.details-selection-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.4rem;
}

.details-selection-actions .details-primary,
.details-selection-actions .details-danger,
.details-selection-actions .details-selection-wide {
  grid-column: span 2;
}

.details-selection-actions .vf-ghost-button {
  justify-content: center;
}

.desktop-details :deep(.empty-state) {
  margin: auto;
}

/* 主操作（预览/打开）与危险操作横跨两列，形成清晰的主次 */
.desktop-details :deep(.details-primary),
.desktop-details :deep(.details-danger) {
  grid-column: span 2;
}
</style>
