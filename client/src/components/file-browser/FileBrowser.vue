<template>
  <div class="file-browser">
    <div class="box">
      <Breadcrumb :breadcrumbs="breadcrumbs" @navigate="navigateTo" />
      
      <div class="level is-mobile mb-4">
        <div class="level-left">
          <div class="level-item">
            <button class="button is-primary" @click="showUploader = true">
              <IconUpload :size="20" class="mr-2" />
              <span>上传文件</span>
            </button>
          </div>
        </div>
        <div class="level-right">
          <div class="level-item">
            <button class="button is-light" @click="refresh" :disabled="loading">
              <IconRefresh :size="20" />
            </button>
          </div>
        </div>
      </div>

      <div v-if="loading" class="has-text-centered py-6">
        <div class="spinner mb-3"></div>
        <p class="has-text-grey">加载中...</p>
      </div>

      <div v-else-if="error" class="notification is-danger is-light">
        <IconAlertCircle :size="20" class="mr-2" />
        {{ error }}
      </div>

      <div v-else-if="files.length === 0" class="has-text-centered py-6">
        <IconFolderOpen :size="64" class="has-text-grey-light mb-3" />
        <p class="has-text-grey">此文件夹为空</p>
      </div>

      <div v-else class="file-list">
        <FileItem
          v-for="file in files"
          :key="file.path"
          :file="file"
          @click="handleFileClick"
          @download="handleDownload"
          @delete="handleDelete"
          @view-history="handleViewHistory"
        />
      </div>
    </div>

    <!-- 上传对话框 -->
    <Modal :show="showUploader" title="上传文件" @close="showUploader = false">
      <FileUploader
        @upload="handleUpload"
        @close="showUploader = false"
      />
    </Modal>

    <!-- 历史记录对话框 -->
    <Modal
      :show="showHistory"
      :title="`文件历史: ${selectedFile?.name}`"
      @close="showHistory = false"
    >
      <VersionHistory v-if="selectedFile" :file-path="selectedFile.path" />
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { storeToRefs } from 'pinia';
import {
  IconUpload,
  IconRefresh,
  IconFolderOpen,
  IconAlertCircle,
} from '@tabler/icons-vue';
import { useFilesStore } from '../../stores/files.store';
import { useAppStore } from '../../stores/app.store';
import { filesService } from '../../services/files.service';
import Breadcrumb from './Breadcrumb.vue';
import FileItem from './FileItem.vue';
import FileUploader from '../file-uploader/FileUploader.vue';
import VersionHistory from '../version-history/VersionHistory.vue';
import Modal from '../common/Modal.vue';
import type { FileInfo } from '../../types';

const filesStore = useFilesStore();
const appStore = useAppStore();
const { files, breadcrumbs, loading, error } = storeToRefs(filesStore);

const showUploader = ref(false);
const showHistory = ref(false);
const selectedFile = ref<FileInfo | null>(null);

onMounted(() => {
  filesStore.loadFiles();
});

function navigateTo(path: string) {
  filesStore.navigateTo(path);
}

function refresh() {
  filesStore.loadFiles(filesStore.currentPath);
}

function handleFileClick(file: FileInfo) {
  if (file.type === 'directory') {
    navigateTo(file.path);
  }
}

function handleDownload(file: FileInfo) {
  filesService.downloadFile(file.path);
  appStore.success('开始下载文件');
}

async function handleDelete(file: FileInfo) {
  try {
    await filesStore.deleteFile(file.path);
    appStore.success('文件删除成功');
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '删除失败');
  }
}

function handleViewHistory(file: FileInfo) {
  selectedFile.value = file;
  showHistory.value = true;
}

async function handleUpload() {
  showUploader.value = false;
  appStore.success('文件上传成功');
  await refresh();
}
</script>

<style scoped>
.file-browser {
  max-width: 1200px;
  margin: 0 auto;
  padding: 1rem;
}

.spinner {
  width: 40px;
  height: 40px;
  border: 3px solid rgba(0, 0, 0, 0.1);
  border-top-color: #3273dc;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin: 0 auto;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media screen and (max-width: 768px) {
  .file-browser {
    padding: 0.5rem;
  }

  .level {
    flex-direction: column;
    align-items: stretch !important;
  }

  .level-left,
  .level-right {
    width: 100%;
  }

  .level-item {
    margin-bottom: 0.5rem;
  }

  .button {
    width: 100%;
    justify-content: center;
  }
}
</style>
