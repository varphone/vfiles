<template>
  <aside class="desktop-details" aria-label="详细信息">
    <template v-if="item">
      <div class="desktop-details-header">
        <span class="desktop-details-icon">
          <FileTypeIcon :file="item" :size="36" :stroke-width="1.3" />
        </span>
        <div class="desktop-details-titles">
          <p class="desktop-details-name" :title="item.name">
            {{ item.name }}
          </p>
          <p class="desktop-details-path" :title="item.path">
            {{ item.path ? `/${item.path}` : "/" }}
          </p>
        </div>
      </div>

      <dl class="desktop-details-grid">
        <div>
          <dt>类型</dt>
          <dd>{{ fileKindLabel(item) }}</dd>
        </div>
        <div>
          <dt>大小</dt>
          <dd>
            {{
              item.kind === "directory"
                ? "--"
                : formatSize(item.size_bytes || 0)
            }}
          </dd>
        </div>
        <div>
          <dt>修改时间</dt>
          <dd :title="modifiedAbsolute">{{ modifiedRelative }}</dd>
        </div>
        <div>
          <dt>创建时间</dt>
          <dd>{{ formatDate(item.created_at) }}</dd>
        </div>
        <div v-if="item.lastCommit?.message">
          <dt>最近提交</dt>
          <dd>{{ item.lastCommit.message }}</dd>
        </div>
      </dl>

      <div class="desktop-details-actions">
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
        <button class="vf-ghost-button is-danger" @click="emit('delete', item)">
          <IconTrash :size="16" />
          <span>删除</span>
        </button>
      </div>
    </template>

    <div v-else class="desktop-details-empty">
      <IconInfoCircle :size="28" />
      <p>选中文件或文件夹后，这里会显示详细信息</p>
    </div>
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
import {
  fileKindLabel,
  formatDate,
  formatRelativeDate,
  formatSize,
} from "../../utils/filePresentation";
import FileTypeIcon from "./FileTypeIcon.vue";

/**
 * 桌面端右侧「详细信息」面板。
 *
 * 只负责展示与事件上抛：当前条目、是否显示由父组件决定，避免这里再持有
 * 列表/选择状态。
 */
const props = defineProps<{
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

const modified = computed(() =>
  props.item ? props.item.updated_at || props.item.created_at : undefined,
);
const modifiedRelative = computed(() => formatRelativeDate(modified.value));
const modifiedAbsolute = computed(() => formatDate(modified.value));
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

.desktop-details-header {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  min-width: 0;
}

.desktop-details-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 3rem;
  height: 3rem;
  border-radius: var(--vf-radius);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
}

.desktop-details-titles {
  min-width: 0;
}

.desktop-details-name {
  font-size: 0.92rem;
  font-weight: 600;
  color: var(--vf-text-strong);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desktop-details-path {
  font-size: 0.76rem;
  color: var(--vf-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desktop-details-grid {
  display: grid;
  gap: 0.6rem;
  margin: 0;
}

.desktop-details-grid dt {
  font-size: 0.72rem;
  color: var(--vf-text-subtle);
}

.desktop-details-grid dd {
  margin: 0.1rem 0 0;
  font-size: 0.84rem;
  color: var(--vf-text);
  word-break: break-word;
}

.desktop-details-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.3rem;
}

.desktop-details-actions .vf-ghost-button {
  justify-content: flex-start;
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
