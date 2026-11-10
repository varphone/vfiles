<template>
  <FilesLayout title="最近">
    <template #sidebar>
      <ViewsSidebar @search="goToSearch" />
    </template>

    <div class="recent-view">
      <PageHeader title="最近" description="最近修改的文件">
        <template #actions>
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh
              :size="16"
              :class="{ 'is-spinning': loading }"
              aria-hidden="true"
            />
            <span>刷新</span>
          </button>
        </template>
      </PageHeader>

      <EmptyState
        v-if="error"
        :icon="IconAlertCircle"
        tone="error"
        title="加载失败"
        :hint="error"
      >
        <template #actions>
          <button
            class="vf-ghost-button is-primary"
            type="button"
            @click="reload"
          >
            <IconRefresh :size="16" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <SkeletonList
        v-else-if="loading && files.length === 0"
        :rows="6"
        label="加载最近文件"
      />

      <EmptyState
        v-else-if="files.length === 0"
        :icon="IconClock"
        title="还没有最近文件"
        hint="上传或修改文件后，会按修改时间显示在这里"
      />

      <section v-else class="recent-panel" aria-label="最近修改的文件">
        <div class="recent-table-wrap">
          <table class="table is-fullwidth is-hoverable recent-table">
            <thead>
              <tr>
                <th scope="col">名称</th>
                <th scope="col" class="recent-col-size">大小</th>
                <th scope="col" class="recent-col-time">修改时间</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="file in files" :key="file.path">
                <td>
                  <div class="recent-name-cell">
                    <FileTypeIcon :file="file" :size="20" />
                    <button
                      class="recent-name"
                      type="button"
                      :title="file.path"
                      @click="openFile(file)"
                    >
                      {{ file.name }}
                    </button>
                    <button
                      class="recent-download"
                      type="button"
                      :aria-label="`下载 ${file.name}`"
                      title="下载"
                      @click="downloadFile(file)"
                    >
                      <IconDownload :size="16" />
                    </button>
                  </div>
                </td>
                <td class="recent-col-size recent-meta">
                  {{ formatSize(file.size_bytes) }}
                </td>
                <td
                  class="recent-col-time recent-meta"
                  :title="formatDate(file.updated_at)"
                >
                  {{ formatRelativeDate(file.updated_at) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </div>
  </FilesLayout>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  IconAlertCircle,
  IconClock,
  IconDownload,
  IconRefresh,
} from "@tabler/icons-vue";
import FilesLayout from "../../layouts/FilesLayout.vue";
import ViewsSidebar from "../../components/files/ViewsSidebar.vue";
import EmptyState from "../../components/common/EmptyState.vue";
import SkeletonList from "../../components/common/SkeletonList.vue";
import FileTypeIcon from "../../components/file-browser/FileTypeIcon.vue";
import PageHeader from "../../ui/PageHeader.vue";
import { filesService } from "../../services/files.service";
import {
  formatDate,
  formatRelativeDate,
  formatSize,
} from "../../utils/filePresentation";
import { parentDirectoryPath } from "../../utils/filePaths";
import type { FileInfo } from "../../types";

/** 侧栏搜索：其它视图没有列表内搜索，统一跳到文件页并带上关键字。 */
function goToSearch(query: string) {
  const value = query.trim();
  if (!value) return;
  void router.push({ path: "/", query: { q: value } });
}

/**
 * 「最近」视图：按修改时间倒序列出最近变更的文件。
 *
 * 预览上下文目前绑定在文件浏览器内部，独立视图按「文件名 → 所在目录 +
 * 行内下载」的降级方案实现：点击名称跳回目录（`query.path`），下载交给
 * 服务端下载接口，避免把整套预览状态搬出 FileBrowser。
 */
const RECENT_LIMIT = 50;

const router = useRouter();
const files = ref<FileInfo[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);

async function reload() {
  loading.value = true;
  error.value = null;
  try {
    files.value = await filesService.getRecentFiles(RECENT_LIMIT);
  } catch (e) {
    files.value = [];
    error.value = e instanceof Error ? e.message : "加载最近文件失败";
  } finally {
    loading.value = false;
  }
}

/** 回到文件所在目录：路径约定与收藏视图一致（`/?path=目录`）。 */
function openFile(file: FileInfo) {
  void router.push({
    path: "/",
    query: { path: parentDirectoryPath(file.path) },
  });
}

function downloadFile(file: FileInfo) {
  filesService.downloadFile(file.path);
}

onMounted(() => {
  void reload();
});
</script>

<style scoped>
/* 页面在圆角内容面板内部滚动：面板本身 overflow: hidden。 */
.recent-view {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  gap: var(--vf-section-gap);
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
}

/* 列表面板：与 Nextcloud 文件区一致，卡片 8px 圆角、行高 50px。 */
.recent-panel {
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
  overflow: hidden;
}

.recent-table-wrap {
  overflow-x: auto;
}

.recent-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
}





.recent-col-size {
  width: 108px;
}

.recent-col-time {
  width: 180px;
}

.recent-name-cell {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-width: 0;
}

.recent-name {
  min-width: 0;
  overflow: hidden;
  padding: 0;
  border: none;
  background: none;
  color: var(--vf-text);
  font-size: var(--vf-text-base);
  text-align: left;
  white-space: nowrap;
  text-overflow: ellipsis;
  cursor: pointer;
}

.recent-name:hover,
.recent-name:focus-visible {
  color: var(--vf-accent-text);
  text-decoration: underline;
}

.recent-download {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: var(--vf-control-height-sm);
  height: var(--vf-control-height-sm);
  margin-left: auto;
  border: none;
  border-radius: var(--vf-radius-control);
  background: transparent;
  color: var(--vf-text-subtle);
  cursor: pointer;
}

.recent-download:hover,
.recent-download:focus-visible {
  background: var(--vf-row-hover);
  color: var(--vf-accent-text);
}

.recent-meta {
  overflow: hidden;
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
  white-space: nowrap;
  text-overflow: ellipsis;
}

.is-spinning {
  animation: recent-spin 0.9s linear infinite;
}

@keyframes recent-spin {
  to {
    transform: rotate(360deg);
  }
}

@media screen and (max-width: 600px) {
  .recent-col-size {
    display: none;
  }

  .recent-col-time {
    width: 104px;
  }

  .recent-table th,
  .recent-table td {
    padding: 0 var(--vf-space-3);
  }
}

/* reduced-motion：循环动画降级为静态指示（M3: static loading indicators），
   本文件动画族 recent-spin 0.9s */
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
</style>
