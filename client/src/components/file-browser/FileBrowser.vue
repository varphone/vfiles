<template>
  <div class="file-browser">
    <div class="box">
      <Breadcrumb :breadcrumbs="breadcrumbs" @navigate="navigateTo" />

      <div class="field has-addons mt-3">
        <div class="control is-expanded">
          <input
            v-model="searchQuery"
            class="input"
            type="text"
            :placeholder="searchMode === 'content' ? '搜索文件内容...' : '搜索文件名...'"
            list="vfiles-search-history"
            @keyup.enter="runSearch"
          />
          <datalist id="vfiles-search-history">
            <option v-for="item in searchHistory" :key="item" :value="item" />
          </datalist>
        </div>
        <div class="control">
          <button
            class="button is-link"
            :class="{ 'is-loading': searchLoading }"
            :disabled="searchLoading"
            @click="runSearch"
          >
            <IconSearch :size="20" />
          </button>
        </div>
        <div class="control">
          <button class="button" :disabled="searchLoading" @click="clearSearch">
            清空
          </button>
        </div>
      </div>

      <div class="field mt-2">
        <label class="checkbox">
          <input type="checkbox" v-model="searchContent" :disabled="searchLoading" />
          全文
        </label>
      </div>

      <div class="field is-grouped is-grouped-multiline mt-2">
        <div class="control">
          <div class="select is-small">
            <select v-model="searchType" :disabled="searchLoading">
              <option value="all">全部</option>
              <option value="file">仅文件</option>
              <option value="directory">仅文件夹</option>
            </select>
          </div>
        </div>

        <div class="control">
          <label class="checkbox">
            <input type="checkbox" v-model="searchScopeCurrent" :disabled="searchLoading" />
            仅当前目录
          </label>
        </div>
      </div>

      <div v-if="searchError" class="notification is-danger is-light">
        <IconAlertCircle :size="20" class="mr-2" />
        {{ searchError }}
      </div>
      
      <div class="level is-mobile mb-4">
        <div class="level-left">
          <div class="level-item">
            <button class="button is-primary" @click="showUploader = true">
              <IconUpload :size="20" class="mr-2" />
              <span>上传文件</span>
            </button>
          </div>

          <div class="level-item">
            <button class="button is-light" @click="toggleBatchMode" :disabled="loading || searchLoading">
              {{ batchMode ? '退出批量' : '批量' }}
            </button>
          </div>

          <div v-if="batchMode" class="level-item">
            <div class="buttons">
              <button class="button is-light" @click="selectAllVisible" :disabled="loading || searchLoading">
                全选
              </button>
              <button class="button is-light" @click="clearSelection" :disabled="loading || searchLoading">
                取消选择
              </button>
              <button class="button is-info" @click="batchDownload" :disabled="selectedCount === 0 || searchLoading">
                批量下载（{{ selectedCount }}）
              </button>
              <button class="button is-danger" @click="batchDelete" :disabled="selectedCount === 0 || searchLoading">
                批量删除（{{ selectedCount }}）
              </button>
              <button class="button is-link" @click="batchMove" :disabled="selectedCount === 0 || searchLoading">
                移动
              </button>
              <button class="button is-warning" @click="renameSelected" :disabled="selectedCount !== 1 || searchLoading">
                重命名
              </button>
            </div>
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

      <div v-else-if="!searchActive && files.length === 0" class="has-text-centered py-6">
        <IconFolderOpen :size="64" class="has-text-grey-light mb-3" />
        <p class="has-text-grey">此文件夹为空</p>
      </div>

      <div v-else-if="searchActive" class="file-list">
        <p class="has-text-grey is-size-7 mb-2">
          搜索结果：{{ searchResults.length }} 项（{{ searchMode === 'content' ? '内容' : '文件名' }}）
        </p>
        <div v-if="searchResults.length === 0" class="has-text-centered py-6">
          <p class="has-text-grey">没有找到匹配的文件</p>
        </div>
        <FileItem
          v-for="file in searchResults"
          :key="file.path"
          :file="file"
          :highlight="searchQuery"
          :select-mode="batchMode"
          :selected="isSelected(file.path)"
          @click="handleSearchItemClick"
          @download="handleDownload"
          @delete="handleDelete"
          @view-history="handleViewHistory"
          @toggle-select="toggleSelect"
        />
      </div>

      <div v-else class="file-list">
        <FileItem
          v-for="file in files"
          :key="file.path"
          :file="file"
          :select-mode="batchMode"
          :selected="isSelected(file.path)"
          @click="handleFileClick"
          @download="handleDownload"
          @delete="handleDelete"
          @view-history="handleViewHistory"
          @toggle-select="toggleSelect"
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
import { ref, onMounted, computed } from 'vue';
import { storeToRefs } from 'pinia';
import {
  IconUpload,
  IconRefresh,
  IconFolderOpen,
  IconAlertCircle,
  IconSearch,
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

const searchQuery = ref('');
const searchResults = ref<FileInfo[]>([]);
const searchLoading = ref(false);
const searchError = ref<string | null>(null);
const searchActive = ref(false);
const searchContent = ref(false);

const batchMode = ref(false);
const selectedPaths = ref<Set<string>>(new Set());

const selectedCount = computed(() => selectedPaths.value.size);

const searchType = ref<'all' | 'file' | 'directory'>('all');
const searchScopeCurrent = ref(false);

const searchMode = computed(() => (searchContent.value ? 'content' : 'name'));

const SEARCH_HISTORY_KEY = 'vfiles.searchHistory';
const searchHistory = ref<string[]>([]);

function loadSearchHistory() {
  try {
    const raw = localStorage.getItem(SEARCH_HISTORY_KEY);
    if (!raw) return;
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      searchHistory.value = parsed.filter((x) => typeof x === 'string').slice(0, 10);
    }
  } catch {
    // ignore
  }
}

function saveSearchHistory(next: string[]) {
  searchHistory.value = next;
  try {
    localStorage.setItem(SEARCH_HISTORY_KEY, JSON.stringify(next));
  } catch {
    // ignore
  }
}

function pushSearchHistory(term: string) {
  const value = term.trim();
  if (!value) return;
  const normalized = value;

  const withoutDup = searchHistory.value.filter((x) => x.toLowerCase() !== normalized.toLowerCase());
  saveSearchHistory([normalized, ...withoutDup].slice(0, 10));
}

onMounted(() => {
  filesStore.loadFiles();
  loadSearchHistory();
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

function handleSearchItemClick(file: FileInfo) {
  if (file.type === 'directory') {
    clearSearch();
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

async function runSearch() {
  return await doSearch(true);
}

async function doSearch(pushHistoryEnabled: boolean) {
  const q = searchQuery.value.trim();
  searchError.value = null;

  if (!q) {
    clearSearch();
    return;
  }

  searchLoading.value = true;
  searchActive.value = true;

  if (pushHistoryEnabled) {
    pushSearchHistory(q);
  }

  try {
    const scopePath = searchScopeCurrent.value ? filesStore.currentPath : '';
    searchResults.value = await filesService.searchFiles(q, searchMode.value, {
      type: searchType.value,
      path: scopePath,
    });
  } catch (err) {
    searchError.value = err instanceof Error ? err.message : '搜索失败';
    searchResults.value = [];
  } finally {
    searchLoading.value = false;
  }
}

function clearSearch() {
  searchQuery.value = '';
  searchResults.value = [];
  searchError.value = null;
  searchActive.value = false;
}

function toggleBatchMode() {
  batchMode.value = !batchMode.value;
  if (!batchMode.value) {
    clearSelection();
  }
}

function isSelected(path: string): boolean {
  return selectedPaths.value.has(path);
}

function toggleSelect(file: FileInfo) {
  const next = new Set(selectedPaths.value);
  if (next.has(file.path)) {
    next.delete(file.path);
  } else {
    next.add(file.path);
  }
  selectedPaths.value = next;
}

function clearSelection() {
  selectedPaths.value = new Set();
}

function selectAllVisible() {
  const list = searchActive.value ? searchResults.value : files.value;
  const next = new Set(selectedPaths.value);
  for (const f of list) {
    next.add(f.path);
  }
  selectedPaths.value = next;
}

function getSelectedItems(): FileInfo[] {
  const list = searchActive.value ? searchResults.value : files.value;
  const map = new Map(list.map((f) => [f.path, f] as const));
  const items: FileInfo[] = [];
  for (const p of selectedPaths.value) {
    const it = map.get(p);
    if (it) items.push(it);
  }
  return items;
}

async function batchDownload() {
  const items = getSelectedItems();
  if (items.length === 0) return;

  const dirs = items.filter((x) => x.type === 'directory');
  if (dirs.length) {
    appStore.error('批量下载暂不支持文件夹');
    return;
  }

  if (items.length > 20) {
    const ok = confirm(`将开始下载 ${items.length} 个文件，可能会被浏览器拦截弹窗。继续吗？`);
    if (!ok) return;
  }

  for (const f of items) {
    filesService.downloadFile(f.path);
  }
  appStore.success(`开始下载 ${items.length} 个文件`);
}

async function batchDelete() {
  const items = getSelectedItems();
  if (items.length === 0) return;

  const ok = confirm(`确定要删除 ${items.length} 项吗？此操作会生成一次或多次提交。`);
  if (!ok) return;

  try {
    for (const f of items) {
      await filesService.deleteFile(f.path, '批量删除');
    }
    appStore.success('批量删除完成');
    clearSelection();
    await refresh();
    if (searchActive.value) {
      await doSearch(false);
    }
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '批量删除失败');
  }
}

async function batchMove() {
  const items = getSelectedItems();
  if (items.length === 0) return;

  const raw = prompt('输入目标目录（相对路径，留空表示根目录）', filesStore.currentPath || '');
  if (raw == null) return;
  const targetDir = raw.trim().replaceAll('\\', '/').replace(/^\/+/, '').replace(/\/+$/, '');

  try {
    for (const f of items) {
      const to = targetDir ? `${targetDir}/${f.name}` : f.name;
      await filesService.movePath(f.path, to, '批量移动');
    }
    appStore.success('批量移动完成');
    clearSelection();
    await refresh();
    if (searchActive.value) {
      await doSearch(false);
    }
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '批量移动失败');
  }
}

async function renameSelected() {
  const items = getSelectedItems();
  if (items.length !== 1) return;
  const f = items[0];

  const raw = prompt('输入新名称（仅名称，不含路径分隔符）', f.name);
  if (raw == null) return;
  const name = raw.trim();
  if (!name || name === '.' || name === '..' || name.includes('/') || name.includes('\\')) {
    appStore.error('非法名称');
    return;
  }

  const parts = f.path.split('/');
  parts.pop();
  const dir = parts.join('/');
  const to = dir ? `${dir}/${name}` : name;

  try {
    await filesService.movePath(f.path, to, `重命名: ${f.name} -> ${name}`);
    appStore.success('重命名成功');
    clearSelection();
    await refresh();
    if (searchActive.value) {
      await doSearch(false);
    }
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '重命名失败');
  }
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
