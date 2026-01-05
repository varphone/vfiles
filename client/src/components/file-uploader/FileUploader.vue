<template>
  <div class="file-uploader">
    <div
      class="drop-zone"
      :class="{ 'is-active': isDragging }"
      @drop.prevent="handleDrop"
      @dragover.prevent="isDragging = true"
      @dragleave.prevent="isDragging = false"
    >
      <input
        type="file"
        ref="fileInput"
        @change="handleFileSelect"
        multiple
        style="display: none"
      />

      <div class="drop-zone-content">
        <IconCloudUpload :size="64" class="has-text-grey-light mb-3" />
        <p class="is-size-5 mb-2">拖拽文件到此处</p>
        <p class="has-text-grey mb-4">或</p>
        <button class="button is-primary" @click="selectFiles">
          <IconFile :size="20" class="mr-2" />
          选择文件
        </button>
      </div>
    </div>

    <div v-if="selectedFiles.length > 0" class="mt-4">
      <p class="is-size-6 has-text-weight-semibold mb-3">
        已选择 {{ selectedFiles.length }} 个文件
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

      <div class="selected-files">
        <div
          v-for="(file, index) in selectedFiles"
          :key="index"
          class="selected-file box"
        >
          <div class="level is-mobile">
            <div class="level-left">
              <div class="level-item">
                <IconFile :size="20" class="mr-2" />
                <span>{{ file.name }}</span>
              </div>
            </div>
            <div class="level-right">
              <div class="level-item">
                <button
                  class="delete"
                  @click="removeFile(index)"
                ></button>
              </div>
            </div>
          </div>
          <progress
            v-if="uploading"
            class="progress is-primary is-small"
            :value="progress"
            max="100"
          >
            {{ progress }}%
          </progress>
        </div>
      </div>

      <div class="buttons mt-4">
        <button
          class="button is-primary"
          @click="upload"
          :disabled="uploading"
          :class="{ 'is-loading': uploading }"
        >
          <IconUpload :size="20" class="mr-2" />
          上传
        </button>
        <button
          class="button is-light"
          @click="cancel"
          :disabled="uploading"
        >
          取消
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import {
  IconCloudUpload,
  IconFile,
  IconUpload,
} from '@tabler/icons-vue';
import { useFilesStore } from '../../stores/files.store';
import { useAppStore } from '../../stores/app.store';

const emit = defineEmits<{
  upload: [];
  close: [];
}>();

const filesStore = useFilesStore();
const appStore = useAppStore();

const fileInput = ref<HTMLInputElement>();
const selectedFiles = ref<File[]>([]);
const commitMessage = ref('上传文件');
const isDragging = ref(false);
const uploading = ref(false);
const progress = ref(0);

function selectFiles() {
  fileInput.value?.click();
}

function handleFileSelect(event: Event) {
  const target = event.target as HTMLInputElement;
  if (target.files) {
    selectedFiles.value.push(...Array.from(target.files));
  }
}

function handleDrop(event: DragEvent) {
  isDragging.value = false;
  if (event.dataTransfer?.files) {
    selectedFiles.value.push(...Array.from(event.dataTransfer.files));
  }
}

function removeFile(index: number) {
  selectedFiles.value.splice(index, 1);
}

async function upload() {
  if (selectedFiles.value.length === 0) return;

  uploading.value = true;
  progress.value = 0;

  try {
    const total = selectedFiles.value.length;
    
    for (let i = 0; i < total; i++) {
      const file = selectedFiles.value[i];
      await filesStore.uploadFile(file, commitMessage.value);
      progress.value = Math.round(((i + 1) / total) * 100);
    }

    emit('upload');
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '上传失败');
  } finally {
    uploading.value = false;
    selectedFiles.value = [];
    commitMessage.value = '上传文件';
  }
}

function cancel() {
  selectedFiles.value = [];
  commitMessage.value = '上传文件';
  emit('close');
}
</script>

<style scoped>
.drop-zone {
  border: 2px dashed #dbdbdb;
  border-radius: 8px;
  padding: 3rem 2rem;
  text-align: center;
  transition: all 0.3s;
  background: #fafafa;
}

.drop-zone.is-active {
  border-color: #3273dc;
  background: #eff5ff;
}

.drop-zone-content {
  pointer-events: none;
}

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

@media screen and (max-width: 768px) {
  .drop-zone {
    padding: 2rem 1rem;
  }

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
