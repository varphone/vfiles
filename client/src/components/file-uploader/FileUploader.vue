<template>
  <div class="file-uploader">
    <DropZone :initial-pick="initialPick" @files="addFiles" />

    <p v-if="maxFileSizeLabel" class="upload-limit-hint">
      <IconInfoCircle :size="14" />
      <span>单文件最大 {{ maxFileSizeLabel }}，超出的文件不会开始上传。</span>
    </p>

    <!-- 大批量导入走 FTP 更合适：仅在服务端启用时展示 -->
    <FtpImportHint v-if="ftpEnabled" :target-path="targetPath" />

    <div v-if="queue.length > 0" class="upload-queue-section">
      <!-- 队列概览：总数、各类状态与整体进度，右侧是批量操作 -->
      <div class="upload-queue-header">
        <div class="upload-queue-header-info">
          <p class="upload-queue-title">待上传 {{ queue.length }} 项</p>
          <p class="upload-queue-subtitle">
            <span v-if="summary.active > 0">上传中 {{ summary.active }}</span>
            <span v-if="summary.queued > 0">排队 {{ summary.queued }}</span>
            <span v-if="summary.done > 0">完成 {{ summary.done }}</span>
            <span v-if="summary.failed > 0" class="is-danger-text"
              >失败 {{ summary.failed }}</span
            >
            <span v-if="summary.canceled > 0">取消 {{ summary.canceled }}</span>
          </p>
        </div>

        <div class="upload-queue-header-actions">
          <button
            v-if="summary.failed > 0 || summary.canceled > 0"
            class="vf-ghost-button upload-queue-header-button"
            type="button"
            @click="retryFailed"
          >
            <IconRefresh :size="16" />
            <span>重试失败项</span>
          </button>
          <button
            v-if="summary.done > 0"
            class="vf-ghost-button upload-queue-header-button"
            type="button"
            @click="clearCompleted"
          >
            <IconChecklist :size="16" />
            <span>清除已完成</span>
          </button>
          <button
            v-if="summary.active > 0 || summary.queued > 0"
            class="vf-ghost-button is-danger upload-queue-header-button"
            type="button"
            @click="cancelAll"
          >
            <IconBan :size="16" />
            <span>全部取消</span>
          </button>
        </div>
      </div>

      <ProgressBar
        v-if="overallPercent != null"
        class="upload-queue-overall"
        mode="determinate"
        :value="overallPercent"
        label="整体上传进度"
      />

      <UploadQueue
        :items="queueView"
        @cancel="cancelItem"
        @retry="retryItem"
        @remove="removeItem"
        @update-message="updateItemMessage"
      />

      <p class="upload-queue-hint">
        每个文件的备注都会作为版本历史里的更新消息保存。
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
import { useAppStore } from "../../stores/app.store";
import { useAuthStore } from "../../stores/auth.store";
import { filesService } from "../../services/files.service";
import {
  IconBan,
  IconChecklist,
  IconInfoCircle,
  IconRefresh,
} from "@tabler/icons-vue";
import ProgressBar from "../common/ProgressBar.vue";
import { formatSize } from "../../utils/filePresentation";
import DropZone from "./DropZone.vue";
import FtpImportHint from "./FtpImportHint.vue";
import UploadQueue, { type UploadQueueItemView } from "./UploadQueue.vue";

const emit = defineEmits<{
  upload: [];
  close: [];
}>();

const appStore = useAppStore();
const auth = useAuthStore();
const ftpEnabled = computed(() => Boolean(auth.features?.ftpEnabled));

const props = defineProps<{
  targetPath: string;
  /** 打开上传对话框时直接弹出的选择器（来自工具栏的上传菜单）。 */
  initialPick?: "files" | "directory" | null;
}>();

type UploadStatus = "queued" | "uploading" | "done" | "error" | "canceled";
type UploadItem = {
  id: number;
  file: File;
  message: string;
  status: UploadStatus;
  percent: number | null;
  error?: string;
  abort?: AbortController;
  relativePath: string;
};

const queue = ref<UploadItem[]>([]);
let nextId = 1;
const uploading = computed(() =>
  queue.value.some((x) => x.status === "uploading"),
);
const hasQueued = computed(() =>
  queue.value.some((x) => x.status === "queued"),
);

/** 队列概览：工具栏用它显示「上传中 x/y」，无需打开对话框。 */
const summary = computed(() => ({
  total: queue.value.length,
  done: queue.value.filter((x) => x.status === "done").length,
  failed: queue.value.filter((x) => x.status === "error").length,
  active: queue.value.filter((x) => x.status === "uploading").length,
  queued: queue.value.filter((x) => x.status === "queued").length,
  canceled: queue.value.filter((x) => x.status === "canceled").length,
}));

const queueView = computed<UploadQueueItemView[]>(() =>
  queue.value.map((x) => ({
    id: x.id,
    file: x.file,
    message: x.message,
    editable: x.status === "queued",
    status: x.status,
    percent: x.percent,
    error: x.error,
    relativePath: x.relativePath,
  })),
);

function defaultUploadMessage(file: File): string {
  return `上传 ${file.name}`;
}

/** 服务端单文件上限（字节）；0 表示未知。 */
const maxFileSizeBytes = computed(() =>
  Number(auth.features?.maxFileSizeBytes ?? 0),
);
const maxFileSizeLabel = computed(() =>
  maxFileSizeBytes.value > 0 ? formatSize(maxFileSizeBytes.value) : "",
);

/** 超过上限的文件不入队，直接以失败态展示原因（与主流网盘的前置校验一致）。 */
function isOversized(file: File): boolean {
  return maxFileSizeBytes.value > 0 && file.size > maxFileSizeBytes.value;
}

function addFiles(files: File[]) {
  const oversizeError = `文件过大，已超过上限（最大 ${maxFileSizeLabel.value}）`;
  const added: UploadItem[] = files.map((f) => {
    const oversized = isOversized(f);
    return {
      id: nextId++,
      file: f,
      message: defaultUploadMessage(f),
      status: oversized ? "error" : "queued",
      percent: null,
      error: oversized ? oversizeError : undefined,
      relativePath: (f as any).webkitRelativePath || "",
    };
  });
  queue.value = [...queue.value, ...added];
}

function updateItemMessage(id: number, message: string) {
  const item = queue.value.find((x) => x.id === id);
  if (!item || item.status !== "queued") return;
  item.message = message;
}

/** 整体进度：只统计已开始上传的条目（排队项按 0 计），无上传中或无文件时返回 null。 */
const overallPercent = computed(() => {
  const active = queue.value.filter(
    (item) => item.status === "uploading" || item.status === "done",
  );
  if (active.length === 0) return null;
  const total = active.reduce(
    (sum, item) => sum + (item.status === "done" ? 100 : (item.percent ?? 0)),
    0,
  );
  return Math.round(total / active.length);
});

/** 重试单条：回到排队状态并（在没有其它上传时）立即重新开始。 */
function retryItem(id: number) {
  const item = queue.value.find((x) => x.id === id);
  if (!item) return;
  if (item.status !== "error" && item.status !== "canceled") return;
  if (isOversized(item.file)) {
    // 上限问题重试也不会成功，保持失败态
    item.status = "error";
    item.error = `文件过大，已超过上限（最大 ${maxFileSizeLabel.value}）`;
    return;
  }
  item.status = "queued";
  item.error = undefined;
  item.percent = null;
  if (!uploading.value) void startUpload();
}

function retryFailed() {
  for (const item of queue.value) {
    if (item.status === "error" || item.status === "canceled") {
      item.status = "queued";
      item.error = undefined;
      item.percent = null;
    }
  }
  if (!uploading.value) void startUpload();
}

function clearCompleted() {
  queue.value = queue.value.filter((item) => item.status !== "done");
}

function removeItem(id: number) {
  const item = queue.value.find((x) => x.id === id);
  if (!item) return;
  if (item.status === "uploading") return;
  queue.value = queue.value.filter((x) => x.id !== id);
}

function cancelItem(id: number) {
  const item = queue.value.find((x) => x.id === id);
  if (!item) return;

  if (item.status === "queued") {
    item.status = "canceled";
    return;
  }
  if (item.status === "uploading") {
    item.abort?.abort();
  }
}

function cancelAll() {
  for (const item of queue.value) {
    if (item.status === "queued") item.status = "canceled";
    if (item.status === "uploading") item.abort?.abort();
  }
}

async function startUpload() {
  if (!hasQueued.value) return;
  if (uploading.value) return;

  // 顺序上传（保持行为简单可控）
  while (true) {
    const next = queue.value.find((x) => x.status === "queued");
    if (!next) break;

    const abort = new AbortController();
    next.abort = abort;
    next.status = "uploading";
    next.percent = 0;
    next.error = undefined;

    try {
      const message = next.message.trim() || defaultUploadMessage(next.file);
      await filesService.uploadFile(next.file, props.targetPath, message, {
        signal: abort.signal,
        onProgress: ({ loaded, total }) => {
          if (!total) {
            next.percent = null;
            return;
          }
          next.percent = Math.min(100, Math.floor((loaded / total) * 100));
        },
        relativePath: next.relativePath,
      });
      next.status = "done";
      next.abort = undefined;
      next.percent = 100;
    } catch (err: any) {
      const isAbort = abort.signal.aborted || err?.name === "CanceledError";
      next.status = isAbort ? "canceled" : "error";
      next.error = isAbort
        ? undefined
        : err instanceof Error
          ? err.message
          : "上传失败";
      next.abort = undefined;
    }
  }

  // 有成功项就通知上层刷新并关闭（失败/超限项留在队列里，可再次重试）
  const hasError = queue.value.some((x) => x.status === "error");
  const hasSuccess = queue.value.some((x) => x.status === "done");
  if (hasSuccess) {
    queue.value = queue.value.filter((x) => x.status !== "done");
    emit("upload");
  }
  if (hasError) {
    appStore.error("部分文件上传失败，请检查列表");
  }
}

// 暴露给父组件使用
defineExpose({
  uploading,
  hasQueued,
  summary,
  /** 供整窗拖放把文件直接加入队列。 */
  addFiles,
  hasFiles: computed(() => queue.value.length > 0),
  startUpload,
  cancelAll,
});
</script>

<style scoped>
.upload-limit-hint {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin: 0.6rem 0 0;
  color: var(--vf-text-subtle);
  font-size: 0.8rem;
}

.upload-queue-section {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  margin-top: 1rem;
}

.upload-queue-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.6rem;
  flex-wrap: wrap;
}

.upload-queue-title {
  margin: 0;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.upload-queue-subtitle {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin: 0.1rem 0 0;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.upload-queue-subtitle .is-danger-text {
  color: var(--vf-danger-text);
}

.upload-queue-header-actions {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  flex-wrap: wrap;
}

.upload-queue-header-button {
  min-height: 1.85rem;
  padding: 0 0.55rem;
  font-size: 0.8rem;
}

.upload-queue-overall {
  margin: 0;
}

.upload-queue-hint {
  margin: 0;
  color: var(--vf-text-subtle);
  font-size: 0.8rem;
}
</style>
