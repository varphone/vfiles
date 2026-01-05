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

      <div v-if="downloadQueue.length" class="box mb-4">
        <div class="level is-mobile">
          <div class="level-left">
            <div class="level-item">
              <div>
                <p class="heading">下载队列</p>
                <p class="title is-6">
                  {{ downloadQueue.length }} 项
                  <span v-if="downloading" class="tag is-info is-light ml-2">下载中</span>
                  <span v-if="queueCollapsed && activeDownload" class="tag is-light ml-2 is-size-7">
                    {{ activeDownload.filename }}
                    <span v-if="activeDownloadPercent != null"> · {{ activeDownloadPercent }}%</span>
                  </span>
                </p>
              </div>
            </div>
          </div>
          <div class="level-right">
            <div class="level-item">
              <div class="buttons">
                <button class="button is-small is-light" @click="toggleQueuePanel" :disabled="!downloadQueue.length">
                  {{ queueCollapsed ? '展开' : '最小化' }}
                </button>
                <button class="button is-small is-light" @click="clearFinished" :disabled="downloading && downloadQueue.length === 1">
                  清空已完成
                </button>
                <button class="button is-small is-danger is-light" @click="cancelAll" :disabled="!downloadQueue.length">
                  全部取消
                </button>
              </div>
            </div>
          </div>
        </div>

        <div v-if="!queueCollapsed" class="content">
          <div v-for="item in downloadQueue" :key="item.id" class="download-item">
            <div class="is-flex is-justify-content-space-between is-align-items-center">
              <div class="mr-2" style="min-width: 0">
                <strong class="is-size-7">{{ item.filename }}</strong>
                <span class="tag is-light ml-2 is-size-7">{{ item.kind === 'folder' ? 'ZIP' : '文件' }}</span>
                <span v-if="item.status === 'queued'" class="tag is-light ml-2 is-size-7">排队中</span>
                <span v-else-if="item.status === 'downloading'" class="tag is-info is-light ml-2 is-size-7">下载中</span>
                <span v-else-if="item.status === 'done'" class="tag is-success is-light ml-2 is-size-7">完成</span>
                <span v-else-if="item.status === 'canceled'" class="tag is-warning is-light ml-2 is-size-7">已取消</span>
                <span v-else-if="item.status === 'error'" class="tag is-danger is-light ml-2 is-size-7">失败</span>
              </div>

              <div class="buttons is-right">
                <button
                  v-if="item.status === 'queued' || item.status === 'downloading'"
                  class="button is-small is-light"
                  @click="cancelItem(item.id)"
                >
                  取消
                </button>
                <button
                  v-else
                  class="button is-small is-light"
                  @click="removeItem(item.id)"
                >
                  移除
                </button>
              </div>
            </div>

            <progress
              v-if="item.status === 'downloading' && item.progress && item.progress.total"
              class="progress is-small is-info mt-2"
              :value="item.progress.loaded"
              :max="item.progress.total"
            ></progress>
            <progress
              v-else-if="item.status === 'downloading'"
              class="progress is-small is-info mt-2"
              max="100"
            ></progress>

            <p v-if="item.error" class="has-text-danger is-size-7 mt-1">
              {{ item.error }}
            </p>
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
        <FileList
          v-if="searchResults.length"
          :files="searchResults"
          :highlight="searchQuery"
          :select-mode="batchMode"
          :selected-paths="selectedPaths"
          @click="handleSearchItemClick"
          @download="handleDownload"
          @delete="handleDelete"
          @view-history="handleViewHistory"
          @toggle-select="toggleSelect"
        />
      </div>

      <div v-else class="file-list">
        <FileList
          :files="files"
          :select-mode="batchMode"
          :selected-paths="selectedPaths"
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

    <!-- 预览对话框（当前版本） -->
    <Modal
      :show="preview.open"
      :title="`预览: ${previewFilename}`"
      @close="closePreview"
    >
      <div v-if="preview.loading" class="has-text-centered py-6">
        <div class="spinner mb-3"></div>
        <p class="has-text-grey">加载预览中...</p>
      </div>

      <div v-else-if="preview.error" class="notification is-danger is-light">
        {{ preview.error }}
      </div>

      <div v-else>
        <figure v-if="preview.kind === 'image'" class="image">
          <img :src="preview.objectUrl" :alt="previewFilename" />
        </figure>

        <div v-else-if="preview.kind === 'pdf'" class="preview-frame">
          <iframe :src="preview.objectUrl" title="PDF 预览" class="preview-iframe" />
        </div>

        <div v-else-if="preview.kind === 'video'" class="preview-media">
          <video :src="preview.objectUrl" controls class="preview-video" />
        </div>

        <div v-else-if="preview.kind === 'audio'" class="preview-media">
          <audio :src="preview.objectUrl" controls class="preview-audio" />
        </div>

        <div v-else-if="preview.kind === 'markdown'" class="content markdown-body" v-html="preview.html"></div>

        <div v-else-if="preview.kind === 'code'" class="content">
          <pre class="preview-code hljs"><code v-html="preview.html"></code></pre>
        </div>

        <div v-else-if="preview.kind === 'text'" class="content">
          <pre class="preview-text">{{ preview.text }}</pre>
        </div>

        <div v-else class="notification is-warning is-light">
          暂不支持该文件类型的在线预览，请使用下载。
        </div>
      </div>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from 'vue';
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
import FileList from './FileList.vue';
import FileUploader from '../file-uploader/FileUploader.vue';
import VersionHistory from '../version-history/VersionHistory.vue';
import Modal from '../common/Modal.vue';
import type { FileInfo } from '../../types';
import { marked } from 'marked';
import hljs from 'highlight.js';

const filesStore = useFilesStore();
const appStore = useAppStore();
const { files, breadcrumbs, loading, error } = storeToRefs(filesStore);

const showUploader = ref(false);
const showHistory = ref(false);
const selectedFile = ref<FileInfo | null>(null);

type PreviewKind = 'text' | 'image' | 'markdown' | 'code' | 'pdf' | 'video' | 'audio' | 'unsupported';
const preview = ref({
  open: false,
  loading: false,
  error: null as string | null,
  path: '',
  kind: 'text' as PreviewKind,
  text: '',
  html: '',
  objectUrl: '',
});

const previewFilename = computed(() => preview.value.path.split('/').pop() || 'file');

function getExtension(p: string): string {
  const name = p.split('/').pop() || '';
  const idx = name.lastIndexOf('.');
  if (idx <= 0 || idx === name.length - 1) return '';
  return name.slice(idx + 1).toLowerCase();
}

function detectPreviewKind(filePath: string): PreviewKind {
  const ext = getExtension(filePath);
  const imageExts = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg']);
  if (imageExts.has(ext)) return 'image';

  if (ext === 'pdf') return 'pdf';

  const videoExts = new Set(['mp4', 'webm', 'ogg', 'mov', 'm4v']);
  if (videoExts.has(ext)) return 'video';

  const audioExts = new Set(['mp3', 'wav', 'ogg', 'm4a', 'aac', 'flac']);
  if (audioExts.has(ext)) return 'audio';

  const mdExts = new Set(['md', 'markdown']);
  if (mdExts.has(ext)) return 'markdown';

  const codeExts = new Set([
    'js',
    'ts',
    'jsx',
    'tsx',
    'vue',
    'json',
    'css',
    'scss',
    'html',
    'xml',
    'yml',
    'yaml',
    'csv',
    'log',
    'sh',
    'py',
    'java',
    'c',
    'cpp',
    'go',
    'rs',
  ]);
  if (codeExts.has(ext)) return 'code';

  const textExts = new Set(['txt', 'log']);
  if (textExts.has(ext) || ext === '') return 'text';

  return 'unsupported';
}

function guessMimeByExt(filePath: string): string {
  const ext = getExtension(filePath);
  if (ext === 'pdf') return 'application/pdf';
  if (ext === 'svg') return 'image/svg+xml';
  if (ext === 'png') return 'image/png';
  if (ext === 'jpg' || ext === 'jpeg') return 'image/jpeg';
  if (ext === 'gif') return 'image/gif';
  if (ext === 'webp') return 'image/webp';
  if (ext === 'bmp') return 'image/bmp';

  if (ext === 'mp4' || ext === 'm4v') return 'video/mp4';
  if (ext === 'webm') return 'video/webm';
  if (ext === 'mov') return 'video/quicktime';
  if (ext === 'ogg') return 'application/ogg';

  if (ext === 'mp3') return 'audio/mpeg';
  if (ext === 'wav') return 'audio/wav';
  if (ext === 'm4a') return 'audio/mp4';
  if (ext === 'aac') return 'audio/aac';
  if (ext === 'flac') return 'audio/flac';

  return 'application/octet-stream';
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function safeLinkHref(href: string | null | undefined): string {
  const raw = (href || '').trim();
  if (!raw) return '#';
  if (raw.startsWith('#')) return raw;
  if (raw.startsWith('/')) return raw;
  if (/^https?:\/\//i.test(raw)) return raw;
  if (/^mailto:/i.test(raw)) return raw;
  return '#';
}

const mdRenderer: any = {
  // 防止 Markdown 原始 HTML 直接注入（marked 新版会传 token 对象）
  html(token: any) {
    const html = typeof token === 'string' ? token : token?.text ?? token?.raw ?? '';
    return escapeHtml(String(html));
  },
  link(tokenOrHref: any, title?: any, text?: any) {
    // 兼容：新版传 token 对象；旧版传 (href, title, text)
    const href = tokenOrHref && typeof tokenOrHref === 'object' ? tokenOrHref.href : tokenOrHref;
    const linkTitle = tokenOrHref && typeof tokenOrHref === 'object' ? tokenOrHref.title : title;
    const linkText = tokenOrHref && typeof tokenOrHref === 'object' ? tokenOrHref.text : text;

    const safeHref = safeLinkHref(href);
    const t = linkTitle ? ` title="${escapeHtml(String(linkTitle))}"` : '';
    const inner = typeof linkText === 'string' ? (marked.parseInline(linkText) as string) : '';
    return `<a href="${escapeHtml(safeHref)}"${t} target="_blank" rel="noopener noreferrer">${inner}</a>`;
  },
};

marked.use({
  renderer: mdRenderer,
  gfm: true,
  breaks: true,
});

function closePreview() {
  if (preview.value.objectUrl) URL.revokeObjectURL(preview.value.objectUrl);
  preview.value = {
    open: false,
    loading: false,
    error: null,
    path: '',
    kind: 'text',
    text: '',
    html: '',
    objectUrl: '',
  };
}

async function openPreview(filePath: string) {
  closePreview();
  preview.value.open = true;
  preview.value.loading = true;
  preview.value.path = filePath;
  preview.value.kind = detectPreviewKind(filePath);

  try {
    if (preview.value.kind === 'unsupported') {
      preview.value.loading = false;
      return;
    }

    const blob = await filesService.getFileContent(filePath);

    if (preview.value.kind === 'image' || preview.value.kind === 'pdf' || preview.value.kind === 'video' || preview.value.kind === 'audio') {
      const typed = new Blob([await blob.arrayBuffer()], { type: guessMimeByExt(filePath) });
      preview.value.objectUrl = URL.createObjectURL(typed);
    } else {
      const text = await blob.text();
      if (preview.value.kind === 'markdown') {
        preview.value.html = marked.parse(text) as string;
      } else if (preview.value.kind === 'code') {
        const highlighted = hljs.highlightAuto(text);
        preview.value.html = highlighted.value;
      } else {
        preview.value.text = text;
      }
    }
  } catch (err) {
    preview.value.error = err instanceof Error ? err.message : '预览失败';
  } finally {
    preview.value.loading = false;
  }
}

const searchQuery = ref('');
const searchResults = ref<FileInfo[]>([]);
const searchLoading = ref(false);
const searchError = ref<string | null>(null);
const searchActive = ref(false);
const searchContent = ref(false);

const queueCollapsed = ref(false);
const activeDownload = computed(() => downloadQueue.value.find((x) => x.status === 'downloading'));
const activeDownloadPercent = computed(() => {
  const a = activeDownload.value;
  if (!a?.progress?.total) return null;
  if (a.progress.total <= 0) return null;
  return Math.min(100, Math.floor((a.progress.loaded / a.progress.total) * 100));
});

type DownloadQueueStatus = 'queued' | 'downloading' | 'done' | 'error' | 'canceled';
type DownloadQueueKind = 'file' | 'folder';
type DownloadQueueItem = {
  id: number;
  kind: DownloadQueueKind;
  path: string;
  filename: string;
  status: DownloadQueueStatus;
  progress?: { loaded: number; total?: number };
  error?: string;
  abort?: AbortController;
};

const downloadQueue = ref<DownloadQueueItem[]>([]);
let nextDownloadId = 1;
const downloading = computed(() => downloadQueue.value.some((x) => x.status === 'downloading'));

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

onBeforeUnmount(() => {
  closePreview();
});

function navigateTo(path: string) {
  filesStore.navigateTo(path);
}

function refresh() {
  filesStore.loadFiles(filesStore.currentPath);
}

function enqueueDownload(kind: DownloadQueueKind, path: string) {
  const wasEmpty = downloadQueue.value.length === 0;
  const filename =
    kind === 'folder'
      ? `${path.split('/').filter(Boolean).pop() || 'root'}.zip`
      : path.split('/').pop() || 'download';

  downloadQueue.value = [
    ...downloadQueue.value,
    {
      id: nextDownloadId++,
      kind,
      path,
      filename,
      status: 'queued',
    },
  ];

  // 第一次出现队列时默认展开
  if (wasEmpty) queueCollapsed.value = false;

  // 自动启动队列
  void processQueue();
}

function toggleQueuePanel() {
  queueCollapsed.value = !queueCollapsed.value;
}

async function processQueue() {
  if (downloading.value) return;

  const next = downloadQueue.value.find((x) => x.status === 'queued');
  if (!next) return;

  const abort = new AbortController();
  downloadQueue.value = downloadQueue.value.map((x) =>
    x.id === next.id ? { ...x, status: 'downloading', progress: { loaded: 0 }, abort } : x
  );

  try {
    const onProgress = (p: { loaded: number; total?: number }) => {
      downloadQueue.value = downloadQueue.value.map((x) => (x.id === next.id ? { ...x, progress: p } : x));
    };

    const result =
      next.kind === 'folder'
        ? await filesService.fetchFolderDownload(next.path, { signal: abort.signal, onProgress })
        : await filesService.fetchFileDownload(next.path, undefined, { signal: abort.signal, onProgress });

    filesService.saveDownloadedBlob(result.blob, result.filename);
    downloadQueue.value = downloadQueue.value.map((x) => (x.id === next.id ? { ...x, status: 'done', abort: undefined } : x));
  } catch (err: any) {
    const isAbort = err?.name === 'AbortError';
    downloadQueue.value = downloadQueue.value.map((x) =>
      x.id === next.id
        ? {
            ...x,
            status: isAbort ? 'canceled' : 'error',
            error: isAbort ? undefined : err instanceof Error ? err.message : '下载失败',
            abort: undefined,
          }
        : x
    );
  } finally {
    // 继续下一个
    void processQueue();
  }
}

function cancelItem(id: number) {
  const item = downloadQueue.value.find((x) => x.id === id);
  if (!item) return;

  if (item.status === 'queued') {
    downloadQueue.value = downloadQueue.value.map((x) => (x.id === id ? { ...x, status: 'canceled' } : x));
    return;
  }

  if (item.status === 'downloading') {
    item.abort?.abort();
  }
}

function cancelAll() {
  for (const item of downloadQueue.value) {
    if (item.status === 'queued') {
      downloadQueue.value = downloadQueue.value.map((x) => (x.id === item.id ? { ...x, status: 'canceled' } : x));
    } else if (item.status === 'downloading') {
      item.abort?.abort();
    }
  }
}

function clearFinished() {
  downloadQueue.value = downloadQueue.value.filter((x) => x.status === 'queued' || x.status === 'downloading');
}

function removeItem(id: number) {
  downloadQueue.value = downloadQueue.value.filter((x) => x.id !== id);
}

function handleFileClick(file: FileInfo) {
  if (file.type === 'directory') {
    navigateTo(file.path);
    return;
  }

  openPreview(file.path);
}

function handleSearchItemClick(file: FileInfo) {
  if (file.type === 'directory') {
    clearSearch();
    navigateTo(file.path);
    return;
  }

  openPreview(file.path);
}

function handleDownload(file: FileInfo) {
  enqueueDownload(file.type === 'directory' ? 'folder' : 'file', file.path);
  appStore.success('已加入下载队列');
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

  if (items.length > 20) {
    const ok = confirm(`将开始下载 ${items.length} 个文件，可能会被浏览器拦截弹窗。继续吗？`);
    if (!ok) return;
  }

  if (items.length > 20) {
    const ok = confirm(`将加入 ${items.length} 项到下载队列，可能会触发多次保存。继续吗？`);
    if (!ok) return;
  }

  for (const f of items) {
    enqueueDownload(f.type === 'directory' ? 'folder' : 'file', f.path);
  }
  appStore.success(`已加入 ${items.length} 项到下载队列`);
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
  const targetDir = raw.trim().replace(/\\/g, '/').replace(/^\/+/, '').replace(/\/+$/, '');

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

.preview-text {
  max-height: 60vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
}

.preview-frame {
  height: 70vh;
}

.preview-iframe {
  width: 100%;
  height: 100%;
  border: 0;
}

.preview-media {
  max-height: 70vh;
}

.preview-video {
  width: 100%;
  max-height: 70vh;
}

.preview-audio {
  width: 100%;
}

.markdown-body :deep(pre) {
  max-height: 60vh;
  overflow: auto;
}

.preview-code {
  max-height: 60vh;
  overflow: auto;
  white-space: pre;
}

.hljs :deep(.hljs-comment),
.hljs :deep(.hljs-quote) {
  opacity: 0.7;
}

.hljs :deep(.hljs-keyword),
.hljs :deep(.hljs-selector-tag),
.hljs :deep(.hljs-title) {
  font-weight: 600;
}

.hljs :deep(.hljs-string) {
  font-style: italic;
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
