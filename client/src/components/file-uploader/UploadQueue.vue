<template>
  <div class="selected-files">
    <div v-for="item in items" :key="item.id" class="selected-file box">
      <div class="level is-mobile">
        <div class="level-left">
          <div class="level-item" style="min-width: 0">
            <IconFile :size="20" class="mr-2" />
            <span class="is-size-7" style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">{{ item.file.name }}</span>
            <span v-if="item.status === 'queued'" class="tag is-light ml-2 is-size-7">排队中</span>
            <span v-else-if="item.status === 'uploading'" class="tag is-info is-light ml-2 is-size-7">上传中</span>
            <span v-else-if="item.status === 'done'" class="tag is-success is-light ml-2 is-size-7">完成</span>
            <span v-else-if="item.status === 'canceled'" class="tag is-warning is-light ml-2 is-size-7">已取消</span>
            <span v-else-if="item.status === 'error'" class="tag is-danger is-light ml-2 is-size-7">失败</span>
          </div>
        </div>
        <div class="level-right">
          <div class="level-item">
            <button
              v-if="item.status === 'queued' || item.status === 'uploading'"
              class="button is-small is-light"
              type="button"
              @click="emit('cancel', item.id)"
            >
              取消
            </button>
            <button v-else class="delete" type="button" @click="emit('remove', item.id)"></button>
          </div>
        </div>
      </div>

      <UploadProgress
        v-if="item.status === 'uploading'"
        :mode="item.percent != null ? 'determinate' : 'indeterminate'"
        :value="item.percent ?? 0"
      />

      <p v-if="item.error" class="has-text-danger is-size-7 mt-1">{{ item.error }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { IconFile } from '@tabler/icons-vue';
import UploadProgress from './UploadProgress.vue';

export type UploadQueueItemView = {
  id: number;
  file: File;
  status: 'queued' | 'uploading' | 'done' | 'error' | 'canceled';
  percent?: number | null;
  error?: string;
};

defineProps<{
  items: UploadQueueItemView[];
}>();

const emit = defineEmits<{
  (e: 'cancel', id: number): void;
  (e: 'remove', id: number): void;
}>();
</script>

<style scoped>
.selected-files {
  max-height: 300px;
  overflow-y: auto;
}

.selected-file {
  padding: 0.75rem;
  margin-bottom: 0.5rem;
}

.selected-file:last-child {
  margin-bottom: 0;
}
</style>
