<template>
  <div class="upload-queue">
    <ul class="upload-queue-list">
      <li
        v-for="item in items"
        :key="item.id"
        class="upload-queue-row"
        :class="`is-${item.status}`"
      >
        <div class="upload-queue-main">
          <span class="upload-queue-icon" aria-hidden="true">
            <FileTypeIcon :file="iconSource(item)" :size="18" />
          </span>

          <div class="upload-queue-titles">
            <span class="upload-queue-name" :title="item.file.name">
              {{ item.file.name }}
            </span>
            <span class="upload-queue-meta">
              <span v-if="directoryLabel(item)" class="upload-queue-dir">
                {{ directoryLabel(item) }}
              </span>
              <span>{{ formatSize(item.file.size) }}</span>
              <span class="upload-queue-status">{{ statusLabel(item) }}</span>
            </span>
          </div>

          <div class="upload-queue-actions">
            <button
              v-if="item.status === 'queued' || item.status === 'uploading'"
              class="vf-ghost-button upload-queue-button"
              type="button"
              :title="item.status === 'uploading' ? '取消上传' : '从队列移除'"
              @click="emit('cancel', item.id)"
            >
              <IconX :size="16" />
              <span>取消</span>
            </button>
            <template v-else>
              <button
                v-if="item.status === 'error' || item.status === 'canceled'"
                class="vf-ghost-button is-primary upload-queue-button"
                type="button"
                title="重新加入队列"
                @click="emit('retry', item.id)"
              >
                <IconRefresh :size="16" />
                <span>重试</span>
              </button>
              <button
                class="vf-icon-button"
                type="button"
                title="从列表移除"
                aria-label="从列表移除"
                @click="emit('remove', item.id)"
              >
                <IconTrash :size="16" />
              </button>
            </template>
          </div>
        </div>

        <!-- 只有真正在上传的行才显示进度条，完成/失败用状态文字表达 -->
        <ProgressBar
          v-if="item.status === 'uploading'"
          :mode="item.percent != null ? 'determinate' : 'indeterminate'"
          :value="item.percent ?? 0"
          :label="`上传 ${item.file.name}`"
        />

        <input
          v-if="item.editable && item.status === 'queued'"
          :value="item.message"
          class="input is-small upload-queue-message"
          type="text"
          maxlength="200"
          placeholder="版本备注（可选，会显示在版本历史里）"
          :aria-label="`${item.file.name} 的版本备注`"
          @input="onMessageInput(item.id, $event)"
        />

        <p v-if="item.error" class="upload-queue-error">
          <IconAlertCircle :size="14" />
          <span>{{ item.error }}</span>
        </p>
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import {
  IconAlertCircle,
  IconRefresh,
  IconTrash,
  IconX,
} from "@tabler/icons-vue";
import ProgressBar from "../common/ProgressBar.vue";
import FileTypeIcon from "../file-browser/FileTypeIcon.vue";
import { formatSize, type FileIconSource } from "../../utils/filePresentation";

export type UploadQueueItemView = {
  id: number;
  file: File;
  message: string;
  editable: boolean;
  status: "queued" | "uploading" | "done" | "error" | "canceled";
  percent?: number | null;
  error?: string;
  relativePath?: string;
};

defineProps<{
  items: UploadQueueItemView[];
}>();

const emit = defineEmits<{
  (e: "cancel", id: number): void;
  (e: "retry", id: number): void;
  (e: "remove", id: number): void;
  (e: "update-message", id: number, message: string): void;
}>();

/** 用文件名推断图标（目录上传仍按文件本身展示）。 */
function iconSource(item: UploadQueueItemView): FileIconSource {
  return { name: item.file.name, kind: "file" };
}

function directoryLabel(item: UploadQueueItemView): string {
  const relative = item.relativePath ?? "";
  if (!relative) return "";
  const parts = relative.split("/");
  parts.pop();
  return parts.join("/");
}

function statusLabel(item: UploadQueueItemView): string {
  switch (item.status) {
    case "uploading":
      return item.percent != null ? `上传中 ${item.percent}%` : "上传中";
    case "done":
      return "已完成";
    case "error":
      return "上传失败";
    case "canceled":
      return "已取消";
    default:
      return "排队中";
  }
}

function onMessageInput(id: number, event: Event) {
  const target = event.target;
  if (!(target instanceof HTMLInputElement)) return;
  emit("update-message", id, target.value);
}
</script>

<style scoped>
.upload-queue {
  min-width: 0;
}

.upload-queue-list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  max-height: 20rem;
  overflow-y: auto;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  list-style: none;
}

/* 紧凑行：文件信息 + 状态 + 操作，悬浮时高亮（主流上传面板） */
.upload-queue-row {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  padding: 0.5rem 0.6rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.upload-queue-row:last-child {
  border-bottom: none;
}

.upload-queue-row:hover {
  background: var(--vf-surface-hover);
}

.upload-queue-main {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
}

.upload-queue-icon {
  display: inline-flex;
  flex: 0 0 auto;
  color: var(--vf-accent-text);
}

.upload-queue-row.is-error .upload-queue-icon,
.upload-queue-row.is-canceled .upload-queue-icon {
  color: var(--vf-text-subtle);
}

.upload-queue-titles {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1 1 auto;
}

.upload-queue-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.875rem;
  color: var(--vf-text-strong);
}

.upload-queue-row.is-done .upload-queue-name {
  color: var(--vf-text-muted);
}

.upload-queue-meta {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  min-width: 0;
  overflow: hidden;
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
  white-space: nowrap;
}

.upload-queue-dir {
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 12rem;
}

.upload-queue-status {
  flex: 0 0 auto;
}

.upload-queue-row.is-done .upload-queue-status {
  color: var(--vf-success-text);
}

.upload-queue-row.is-error .upload-queue-status {
  color: var(--vf-danger-text);
}

.upload-queue-row.is-uploading .upload-queue-status {
  color: var(--vf-accent);
}

.upload-queue-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex: 0 0 auto;
}

.upload-queue-button {
  min-height: 1.75rem;
  padding: 0 0.5rem;
  font-size: 0.8rem;
}

.upload-queue-message {
  font-size: 0.8rem;
}

.upload-queue-error {
  display: flex;
  align-items: flex-start;
  gap: 0.3rem;
  margin: 0;
  color: var(--vf-danger-text);
  font-size: 0.8rem;
  line-height: 1.4;
}
</style>
