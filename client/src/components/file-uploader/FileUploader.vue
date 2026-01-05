<template>
  <div class="file-uploader">
    <DropZone @files="addFiles" />

    <div v-if="queue.length > 0" class="mt-4">
      <p class="is-size-6 has-text-weight-semibold mb-3">
        已选择 {{ queue.length }} 个文件
      </p>

      <div class="field">
        <label class="label">提交信息</label>
        <div class="control">
          <input
            v-model="commitMessage"
            class="input"
            type="text"
            placeholder="输入提交信息..."
          />
        </div>
      </div>

      <UploadQueue :items="queueView" @cancel="cancelItem" @remove="removeItem" />

      <div class="buttons mt-4">
        <button
          class="button is-primary"
          @click="startUpload"
          :disabled="uploading || !hasQueued"
          :class="{ 'is-loading': uploading }"
        >
          <IconUpload :size="20" class="mr-2" />
          上传
        </button>
        <button
          class="button is-light"
          type="button"
          @click="cancelAll"
          :disabled="queue.length === 0"
        >
          取消
        </button>
        <button class="button is-light" type="button" @click="close" :disabled="uploading">
          关闭
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import {
  IconUpload,
} from '@tabler/icons-vue';
import { useAppStore } from '../../stores/app.store';
import { filesService } from '../../services/files.service';
import DropZone from './DropZone.vue';
import UploadQueue, { type UploadQueueItemView } from './UploadQueue.vue';

const emit = defineEmits<{
  upload: [];
  close: [];
}>();

const appStore = useAppStore();

const props = defineProps<{
  targetPath: string;
}>();

type UploadStatus = 'queued' | 'uploading' | 'done' | 'error' | 'canceled';
type UploadItem = {
  id: number;
  file: File;
  status: UploadStatus;
  percent: number | null;
  error?: string;
  abort?: AbortController;
};

const queue = ref<UploadItem[]>([]);
let nextId = 1;
const uploading = computed(() => queue.value.some((x) => x.status === 'uploading'));
const hasQueued = computed(() => queue.value.some((x) => x.status === 'queued'));
const commitMessage = ref('上传文件');

const queueView = computed<UploadQueueItemView[]>(() =>
  queue.value.map((x) => ({
    id: x.id,
    file: x.file,
    status: x.status,
    percent: x.percent,
    error: x.error,
  }))
);

function addFiles(files: File[]) {
  const added: UploadItem[] = files.map((f) => ({
    id: nextId++,
    file: f,
    status: 'queued',
    percent: null,
  }));
  queue.value = [...queue.value, ...added];
}

function removeItem(id: number) {
  const item = queue.value.find((x) => x.id === id);
  if (!item) return;
  if (item.status === 'uploading') return;
  queue.value = queue.value.filter((x) => x.id !== id);
}

function cancelItem(id: number) {
  const item = queue.value.find((x) => x.id === id);
  if (!item) return;

  if (item.status === 'queued') {
    item.status = 'canceled';
    return;
  }
  if (item.status === 'uploading') {
    item.abort?.abort();
  }
}

function cancelAll() {
  for (const item of queue.value) {
    if (item.status === 'queued') item.status = 'canceled';
    if (item.status === 'uploading') item.abort?.abort();
  }
}

function close() {
  emit('close');
}

async function startUpload() {
  if (!hasQueued.value) return;
  if (uploading.value) return;

  // 顺序上传（保持行为简单可控）
  while (true) {
    const next = queue.value.find((x) => x.status === 'queued');
    if (!next) break;

    const abort = new AbortController();
    next.abort = abort;
    next.status = 'uploading';
    next.percent = 0;
    next.error = undefined;

    try {
      await filesService.uploadFile(next.file, props.targetPath, commitMessage.value, {
        signal: abort.signal,
        onProgress: ({ loaded, total }) => {
          if (!total) {
            next.percent = null;
            return;
          }
          next.percent = Math.min(100, Math.floor((loaded / total) * 100));
        },
      });
      next.status = 'done';
      next.abort = undefined;
      next.percent = 100;
    } catch (err: any) {
      const isAbort = err?.name === 'CanceledError' || err?.name === 'AbortError';
      next.status = isAbort ? 'canceled' : 'error';
      next.error = isAbort ? undefined : err instanceof Error ? err.message : '上传失败';
      next.abort = undefined;
    }
  }

  // 仅在全部成功（无 error）时触发上层刷新并关闭
  const hasError = queue.value.some((x) => x.status === 'error');
  const hasSuccess = queue.value.some((x) => x.status === 'done');
  if (hasSuccess && !hasError) {
    emit('upload');
    queue.value = [];
    commitMessage.value = '上传文件';
  } else if (hasError) {
    appStore.error('部分文件上传失败，请检查列表');
  }
}
</script>

<style scoped>
@media screen and (max-width: 768px) {
  .buttons {
    display: flex;
    flex-direction: column;
  }

  .button {
    width: 100%;
    margin-bottom: 0.5rem;
  }
}
</style>
