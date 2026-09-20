<template>
  <aside class="desktop-details" aria-label="详细信息">
    <template v-if="item">
      <FileDetailsContent :file="item">
        <template #actions>
          <button
            v-if="item.kind === 'file'"
            class="vf-ghost-button"
            @click="emit('preview', item)"
          >
            <IconEye :size="16" />
            <span>预览</span>
          </button>
          <button
            v-else
            class="vf-ghost-button"
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
            class="vf-ghost-button is-danger"
            @click="emit('delete', item)"
          >
            <IconTrash :size="16" />
            <span>删除</span>
          </button>
        </template>
      </FileDetailsContent>
    </template>

    <div v-else class="desktop-details-empty">
      <IconInfoCircle :size="28" />
      <p>选中文件或文件夹后，这里会显示详细信息</p>
    </div>
  </aside>
</template>

<script setup lang="ts">
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
import FileDetailsContent from "./FileDetailsContent.vue";

/**
 * 桌面端右侧「详细信息」面板。
 *
 * 只负责展示与事件上抛：当前条目、是否显示由父组件决定，避免这里再持有
 * 列表/选择状态。
 */
defineProps<{
  item?: FileInfo & { uiRole?: "self" | "parent" };
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
}>();
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
}

.desktop-details-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
  margin: auto;
  padding: 1rem;
  text-align: center;
  color: var(--vf-text-subtle);
  font-size: 0.82rem;
}
</style>
