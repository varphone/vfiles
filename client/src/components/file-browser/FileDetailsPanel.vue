<template>
  <aside class="desktop-details" aria-label="详细信息">
    <template v-if="item">
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
  IconDownload,
  IconEye,
  IconFolderOpen,
  IconHistory,
  IconInfoCircle,
  IconPencil,
  IconShare,
  IconTrash,
} from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import { filesService } from "../../services/files.service";
import { isImageFile } from "../../utils/filePresentation";
import EmptyState from "../common/EmptyState.vue";
import FileDetailsContent from "./FileDetailsContent.vue";

/**
 * 桌面端右侧「详细信息」面板。
 *
 * 只负责展示与事件上抛：当前条目、是否显示由父组件决定，避免这里再持有
 * 列表/选择状态。
 */
const props = defineProps<{
  item?: FileInfo;
  /** 浏览版本（用于按版本取缩略图）。 */
  commit?: string;
}>();

const emit = defineEmits<{
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
  (e: "download", file: FileInfo): void;
  (e: "share", file: FileInfo): void;
  (e: "view-history", file: FileInfo): void;
  (e: "rename", file: FileInfo): void;
  (e: "move", file: FileInfo): void;
  (e: "delete", file: FileInfo): void;
  (e: "close"): void;
}>();

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
  max-height: calc(100vh - var(--bulma-navbar-height, 3.25rem) - 2rem);
  overflow-y: auto;
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
