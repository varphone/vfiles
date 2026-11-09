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
          v-if="canWrite"
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
          v-if="canTransfer"
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
          v-if="canWrite"
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
      <!-- 侧栏头部 + Tab：整体吸顶，长内容滚动时仍可切换视图与关闭 -->
      <div class="details-top">
        <header class="details-head">
          <div class="details-head-top">
            <h2 class="details-head-title" :title="item.name">
              {{ item.name }}
            </h2>
            <div class="details-head-actions">
              <button
                ref="menuButton"
                class="vf-icon-button details-menu"
                type="button"
                :aria-label="`${item.name} 的操作`"
                aria-haspopup="true"
                title="更多操作"
                @click="openRowMenu"
              >
                <IconDots :size="18" />
              </button>
              <button
                class="vf-icon-button details-close"
                type="button"
                title="关闭详细信息"
                aria-label="关闭详细信息"
                @click="emit('close')"
              >
                <IconX :size="18" />
              </button>
            </div>
          </div>
          <p class="details-head-meta">
            <span class="details-head-size">{{ metaSizeLabel }}</span>
            <span class="details-head-sep" aria-hidden="true">•</span>
            <span class="details-head-date" :title="metaDateTitle">
              {{ metaDateLabel }}
            </span>
            <template v-if="ownerName">
              <span class="details-head-sep" aria-hidden="true">•</span>
              <span class="details-head-owner">
                <IconUser :size="14" aria-hidden="true" />
                <span class="details-head-owner-name">{{ ownerName }}</span>
              </span>
            </template>
          </p>
        </header>

        <!-- Tab：图标在上、文字在下的四项等宽切换（Nextcloud 信息侧栏） -->
        <div class="details-tabs" role="tablist" aria-label="条目详情">
          <button
            v-for="tab in tabs"
            :id="`details-tab-${tab.key}`"
            :key="tab.key"
            class="details-tab"
            :class="{ 'is-active': activeTab === tab.key }"
            type="button"
            role="tab"
            :aria-selected="activeTab === tab.key ? 'true' : 'false'"
            :aria-controls="`details-panel-${tab.key}`"
            @click="activeTab = tab.key"
          >
            <component
              :is="tab.icon"
              :size="20"
              class="details-tab-icon"
              aria-hidden="true"
            />
            <span class="details-tab-label">{{ tab.label }}</span>
          </button>
        </div>
      </div>

      <div
        v-if="activeTab === 'details'"
        id="details-panel-details"
        class="details-tab-content"
        role="tabpanel"
        aria-labelledby="details-tab-details"
      >
        <FileDetailsContent :file="item" :preview-url="previewUrl">
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
            <button
              v-if="canShare"
              class="vf-ghost-button"
              @click="emit('share', item)"
            >
              <IconShare :size="16" />
              <span>分享</span>
            </button>
            <button class="vf-ghost-button" @click="emit('view-history', item)">
              <IconHistory :size="16" />
              <span>历史版本</span>
            </button>
            <button
              v-if="canWrite"
              class="vf-ghost-button"
              @click="emit('rename', item)"
            >
              <IconPencil :size="16" />
              <span>重命名</span>
            </button>
            <button
              v-if="canWrite"
              class="vf-ghost-button"
              @click="emit('move', item)"
            >
              <IconArrowsDiff :size="16" />
              <span>移动</span>
            </button>
            <button
              v-if="canTransfer"
              class="vf-ghost-button"
              @click="emit('transfer', item)"
            >
              <IconUserShare :size="16" />
              <span>转移所有权</span>
            </button>
            <button
              v-if="canWrite"
              class="vf-ghost-button is-danger details-danger"
              @click="emit('delete', item)"
            >
              <IconTrash :size="16" />
              <span>删除</span>
            </button>
          </template>
        </FileDetailsContent>
      </div>

      <div
        v-else-if="activeTab === 'shares'"
        id="details-panel-shares"
        class="details-tab-content"
        role="tabpanel"
        aria-labelledby="details-tab-shares"
      >
        <FileShareTab
          :file="item"
          :can-share="canShare"
          @create-share="emit('share', item)"
        />
      </div>

      <div
        v-else-if="activeTab === 'activity'"
        id="details-panel-activity"
        class="details-tab-content"
        role="tabpanel"
        aria-labelledby="details-tab-activity"
      >
        <FileActivityTab :file="item" />
      </div>

      <div
        v-else
        id="details-panel-versions"
        class="details-tab-content details-version-panel"
        role="tabpanel"
        aria-labelledby="details-tab-versions"
      >
        <VersionHistory :file-path="item.path" :can-write="canWrite" />
      </div>
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
import { computed, ref, type Component } from "vue";
import {
  IconArrowsDiff,
  IconBolt,
  IconChecklist,
  IconDots,
  IconDownload,
  IconEye,
  IconFolderOpen,
  IconHistory,
  IconInfoCircle,
  IconPencil,
  IconShare,
  IconTrash,
  IconUser,
  IconUserShare,
  IconX,
} from "@tabler/icons-vue";
import { getActivePinia } from "pinia";
import type { FileInfo } from "../../types";
import { filesService } from "../../services/files.service";
import { useAuthStore } from "../../stores/auth.store";
import {
  formatDate,
  formatRelativeDate,
  formatSize,
  isImageFile,
} from "../../utils/filePresentation";
import EmptyState from "../common/EmptyState.vue";
import VersionHistory from "../version-history/VersionHistory.vue";
import FileActivityTab from "./FileActivityTab.vue";
import FileDetailsContent from "./FileDetailsContent.vue";
import FileShareTab from "./FileShareTab.vue";
import FileTypeIcon from "./FileTypeIcon.vue";

/**
 * 桌面端右侧「详细信息」面板（Nextcloud 信息侧栏结构）。
 *
 * 只负责展示与事件上抛：当前条目、是否显示由父组件决定，避免这里再持有
 * 列表/选择状态。单选时头部固定展示文件名与元信息，下面按
 * 「详细信息 / 分享 / 活动 / 版本」四个 Tab 切换视图。
 */
const props = withDefaults(
  defineProps<{
    item?: FileInfo;
    /** 浏览版本（用于按版本取缩略图）。 */
    commit?: string;
    /** 批量选择中的条目；多于 1 项时展示选择摘要而不是单条详情。 */
    selection?: FileInfo[];
    /** 只读会话隐藏重命名/移动/删除与版本恢复。 */
    canWrite?: boolean;
    /** 受限会话隐藏分享。 */
    canShare?: boolean;
    /** 受限会话隐藏所有权转移。 */
    canTransfer?: boolean;
  }>(),
  {
    item: undefined,
    commit: undefined,
    selection: () => [],
    canWrite: true,
    canShare: true,
    canTransfer: true,
  },
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
  /** 头部「⋯」：与列表行菜单同源的上下文菜单请求。 */
  (e: "context-menu", payload: { file: FileInfo; x: number; y: number }): void;
}>();

type DetailsTabKey = "details" | "shares" | "activity" | "versions";

const tabs: Array<{ key: DetailsTabKey; label: string; icon: Component }> = [
  { key: "details", label: "详细信息", icon: IconInfoCircle },
  { key: "shares", label: "分享", icon: IconShare },
  { key: "activity", label: "活动", icon: IconBolt },
  { key: "versions", label: "版本", icon: IconHistory },
];

/** 当前 Tab；切换条目时保留，避免每次选文件都跳回第一页。 */
const activeTab = ref<DetailsTabKey>("details");

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

/** 当前会话用户名（用于头部的所有者胶囊）；无 Pinia 的嵌入/测试场景下省略。 */
const pinia = getActivePinia();
const authStore = pinia ? useAuthStore(pinia) : null;
const ownerName = computed(() => authStore?.user?.username ?? "");

const metaSizeLabel = computed(() => {
  const item = props.item;
  if (!item) return "";
  if (item.kind === "directory") return "—";
  return formatSize(item.size_bytes || 0);
});

const metaDateLabel = computed(() => {
  const item = props.item;
  if (!item) return "";
  return formatRelativeDate(item.updated_at || item.created_at);
});

const metaDateTitle = computed(() => {
  const item = props.item;
  if (!item) return "";
  return formatDate(item.updated_at || item.created_at);
});

/** 头部「⋯」：在按钮下方打开与列表行一致的上下文菜单。 */
const menuButton = ref<HTMLButtonElement | null>(null);

function openRowMenu() {
  const item = props.item;
  if (!item) return;
  const rect = menuButton.value?.getBoundingClientRect();
  emit("context-menu", {
    file: item,
    x: rect ? rect.left : 0,
    y: rect ? rect.bottom + 4 : 0,
  });
}
</script>

<style scoped>
.desktop-details {
  display: flex;
  flex-direction: column;
  /* Nextcloud 详情侧栏宽度；与外壳栅格轨道共用同一 token */
  width: var(--vf-details-width, 350px);
  max-width: 100%;
  min-width: 0;
  padding: 0;
  border-left: 1px solid var(--vf-border-weak);
  background: var(--vf-surface);
  /* 与列表一致：面板自身吸顶并内部滚动，长列表滚动时信息与操作始终可见。
     top 取 0：面板就在内容区顶部；带上顶栏高度会在面板上方留出空白
     （sticky 约束按最近的滚动容器计算）。 */
  position: sticky;
  top: 0;
  align-self: start;
  /* 高度受内容区（grid 行）限制，而不是视口：
     用视口高度会让面板越过内容区压住底部状态栏，挡住状态栏右侧的操作 */
  min-height: 100%;
  max-height: 100%;
  overflow-y: auto;
}

/* 头部 + Tab：整体吸顶，随面板滚动时保持可见 */
.details-top {
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--vf-surface);
}

/* 头部：文件名 + 行菜单/关闭 + 一行元信息 */
.details-head {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-2);
  /* 标题区与内容区共用 16px 左右基线；底部留 12px 接 Tab 行 */
  padding: var(--vf-space-4) var(--vf-page-padding) var(--vf-space-3);
  background: var(--vf-surface);
}

.details-head-top {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
  min-width: 0;
}

/* 标题：17px 加粗，最多两行后截断 */
.details-head-title {
  display: -webkit-box;
  flex: 1 1 auto;
  min-width: 0;
  margin: 0;
  overflow: hidden;
  color: var(--vf-text-strong);
  font-size: 17px;
  font-weight: 700;
  line-height: 1.3;
  overflow-wrap: anywhere;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  line-clamp: 2;
}

.details-head-actions {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: var(--vf-space-1);
}

.details-close,
.details-menu {
  flex: 0 0 auto;
}

/* 元信息：大小 • 相对修改时间 • 用户名胶囊 */
.details-head-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--vf-space-2);
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 13px;
  line-height: 1.4;
}

.details-head-sep {
  color: var(--vf-text-subtle);
}

.details-head-owner {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-space-1);
  padding: var(--vf-space-1) var(--vf-space-2);
  border-radius: 999px;
  background: var(--vf-surface-muted);
  color: var(--vf-text);
  font-size: 13px;
  line-height: 1.3;
}

.details-head-owner-name {
  max-width: 12rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Tab：图标在上、文字在下，四项等宽；行高与表头一致（44px） */
.details-tabs {
  display: flex;
  align-items: stretch;
  height: var(--vf-table-head-height);
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface);
}

.details-tab {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  min-width: 0;
  padding: var(--vf-space-1);
  border: none;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  background: transparent;
  color: var(--vf-text-muted);
  font-size: 0.75rem;
  font-weight: 500;
  line-height: 1.2;
  white-space: nowrap;
  cursor: pointer;
}

.details-tab:hover {
  color: var(--vf-text-strong);
}

.details-tab.is-active {
  color: var(--vf-accent-text);
  border-bottom-color: var(--vf-accent);
  font-weight: 600;
}

.details-tab.is-active .details-tab-icon {
  color: var(--vf-accent);
}

.details-tab-icon {
  flex: 0 0 auto;
  color: inherit;
}

.details-tab:focus-visible {
  outline: none;
  box-shadow: 0 0 0 2px var(--vf-focus-ring);
  border-radius: 3px;
}

.details-tab-content {
  display: flex;
  flex-direction: column;
  min-width: 0;
  padding: var(--vf-page-padding);
}

/* 版本 Tab 复用版本历史组件：侧栏只有一列，覆盖其宽屏双列布局 */
.details-version-panel :deep(.history-layout) {
  grid-template-columns: minmax(0, 1fr);
}

.details-version-panel :deep(.history-list-pane) {
  max-height: none;
}

.details-version-panel :deep(.history-detail-pane) {
  max-height: none;
}

/* 多选摘要 */
.details-selection {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-3);
  padding: var(--vf-space-4);
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
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.details-selection-meta {
  margin: 0.15rem 0 0;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.details-selection-list {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-1);
  margin: 0;
  padding: var(--vf-space-2);
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-control);
  background: var(--vf-surface-sunken);
  list-style: none;
}

.details-selection-row {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-width: 0;
  color: var(--vf-text);
  font-size: 0.875rem;
}

.details-selection-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.details-selection-more {
  color: var(--vf-text-subtle);
  font-size: 0.8rem;
}

.details-selection-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--vf-space-2);
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
