<template>
  <div class="version-history">
    <div v-if="loading" class="has-text-centered py-6">
      <div class="spinner mb-3"></div>
      <p class="has-text-grey">加载历史记录中...</p>
    </div>

    <div v-else-if="error" class="notification is-danger is-light">
      {{ error }}
    </div>

    <div v-else-if="history.commits.length === 0" class="has-text-centered py-6">
      <IconHistory :size="64" class="has-text-grey-light mb-3" />
      <p class="has-text-grey">暂无历史记录</p>
    </div>

    <div v-else class="timeline">
      <div
        v-for="(commit, index) in history.commits"
        :key="commit.hash"
        class="timeline-item"
      >
        <div class="timeline-marker" :class="{ 'is-primary': index === 0 }"></div>
        <div class="timeline-content">
          <div class="box">
            <div class="level is-mobile">
              <div class="level-left">
                <div class="level-item">
                  <div>
                    <p class="heading">{{ formatDate(commit.date) }}</p>
                    <p class="title is-6">{{ commit.message }}</p>
                    <p class="subtitle is-7 has-text-grey">
                      {{ commit.author.name }}
                      <span v-if="index === 0" class="tag is-primary is-light ml-2">
                        最新版本
                      </span>
                    </p>
                  </div>
                </div>
              </div>
              <div class="level-right">
                <div class="level-item">
                  <div class="buttons">
                    <button
                      class="button is-small is-info is-light"
                      @click="viewVersion(commit.hash)"
                      title="查看此版本"
                    >
                      <IconEye :size="18" />
                    </button>
                    <button
                      class="button is-small is-success is-light"
                      @click="downloadVersion(commit.hash)"
                      title="下载此版本"
                    >
                      <IconDownload :size="18" />
                    </button>
                  </div>
                </div>
              </div>
            </div>
            <div class="commit-hash">
              <code class="is-size-7">{{ commit.hash.substring(0, 8) }}</code>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="history.totalCommits > history.commits.length" class="has-text-centered mt-4">
      <button class="button is-light" @click="loadMore">
        加载更多
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import {
  IconHistory,
  IconEye,
  IconDownload,
} from '@tabler/icons-vue';
import { filesService } from '../../services/files.service';
import { useAppStore } from '../../stores/app.store';
import type { FileHistory } from '../../types';

const props = defineProps<{
  filePath: string;
}>();

const appStore = useAppStore();

const history = ref<FileHistory>({
  commits: [],
  currentVersion: '',
  totalCommits: 0,
});
const loading = ref(false);
const error = ref<string | null>(null);
const limit = ref(20);

onMounted(() => {
  loadHistory();
});

async function loadHistory() {
  loading.value = true;
  error.value = null;

  try {
    history.value = await filesService.getFileHistory(props.filePath, limit.value);
  } catch (err) {
    error.value = err instanceof Error ? err.message : '加载失败';
  } finally {
    loading.value = false;
  }
}

function formatDate(date: string): string {
  return new Date(date).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function viewVersion(hash: string) {
  // TODO: 实现版本预览功能
  appStore.info(`查看版本: ${hash.substring(0, 8)}`);
}

function downloadVersion(hash: string) {
  filesService.downloadFile(props.filePath, hash);
  appStore.success('开始下载历史版本');
}

function loadMore() {
  limit.value += 20;
  loadHistory();
}
</script>

<style scoped>
.version-history {
  max-height: 70vh;
  overflow-y: auto;
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

.timeline {
  padding: 1rem 0;
}

.timeline-item {
  position: relative;
  padding-left: 2rem;
  padding-bottom: 2rem;
}

.timeline-item::before {
  content: '';
  position: absolute;
  left: 0.5rem;
  top: 1.5rem;
  bottom: 0;
  width: 2px;
  background: #dbdbdb;
}

.timeline-item:last-child::before {
  display: none;
}

.timeline-marker {
  position: absolute;
  left: 0;
  top: 0.5rem;
  width: 1.25rem;
  height: 1.25rem;
  border: 2px solid #dbdbdb;
  border-radius: 50%;
  background: white;
  z-index: 1;
}

.timeline-marker.is-primary {
  border-color: #3273dc;
  background: #3273dc;
}

.timeline-content {
  margin-left: 1rem;
}

.commit-hash {
  margin-top: 0.5rem;
}

.buttons {
  display: flex;
  gap: 0.25rem;
}

@media screen and (max-width: 768px) {
  .timeline-item {
    padding-left: 1.5rem;
  }

  .level {
    flex-direction: column;
    align-items: flex-start !important;
  }

  .level-right {
    margin-top: 0.5rem;
  }

  .buttons {
    flex-direction: row;
  }
}
</style>
