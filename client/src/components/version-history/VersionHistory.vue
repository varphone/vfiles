<template>
  <div class="version-history">
    <div v-if="diff.open" class="box mb-4">
      <div class="level is-mobile">
        <div class="level-left">
          <div class="level-item">
            <div>
              <p class="heading">对比视图（文本）</p>
              <p class="title is-6">
                <code class="is-size-7">{{ diff.hash.substring(0, 8) }}</code>
                <span class="ml-2">{{ previewView.filename }}</span>
              </p>
            </div>
          </div>
        </div>
        <div class="level-right">
          <div class="level-item">
            <button class="button is-small" @click="closeDiff">关闭</button>
          </div>
        </div>
      </div>

      <div v-if="diff.loading" class="has-text-centered py-5">
        <div class="spinner mb-3"></div>
        <p class="has-text-grey">加载 diff 中...</p>
      </div>

      <div v-else-if="diff.error" class="notification is-warning is-light">
        {{ diff.error }}
      </div>

      <div v-else class="content">
        <pre class="diff-text">{{ diff.text }}</pre>
      </div>
    </div>

    <div v-if="preview.open" class="box mb-4">
      <div class="level is-mobile">
        <div class="level-left">
          <div class="level-item">
            <div>
              <p class="heading">预览版本</p>
              <p class="title is-6">
                <code class="is-size-7">{{ previewView.hashShort }}</code>
                <span class="ml-2">{{ previewView.filename }}</span>
              </p>
            </div>
          </div>
        </div>
        <div class="level-right">
          <div class="level-item">
            <div class="buttons">
              <button
                class="button is-small is-success is-light"
                @click="downloadVersion(preview.hash)"
                title="下载此版本"
              >
                <IconDownload :size="18" />
              </button>
              <button class="button is-small" @click="closePreview" title="关闭预览">
                关闭
              </button>
            </div>
          </div>
        </div>
      </div>

      <div v-if="preview.loading" class="has-text-centered py-5">
        <div class="spinner mb-3"></div>
        <p class="has-text-grey">加载预览中...</p>
      </div>

      <div v-else-if="preview.error" class="notification is-danger is-light">
        {{ preview.error }}
      </div>

      <div v-else>
        <figure v-if="preview.kind === 'image'" class="image">
          <img :src="preview.objectUrl" :alt="previewView.filename" />
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
    </div>

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
                      class="button is-small is-link is-light"
                      @click="viewDiff(commit.hash, commit.parent?.[0])"
                      title="对比此版本（文本）"
                    >
                      <IconArrowsDiff :size="18" />
                    </button>
                    <button
                      class="button is-small is-warning is-light"
                      @click="restoreVersion(commit.hash)"
                      :disabled="commit.hash === history.currentVersion || restoringHash === commit.hash"
                      title="恢复到此版本（会生成新提交）"
                    >
                      <IconRestore :size="18" />
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
import { ref, onMounted, onBeforeUnmount, computed } from 'vue';
import {
  IconHistory,
  IconEye,
  IconDownload,
  IconRestore,
  IconArrowsDiff,
} from '@tabler/icons-vue';
import { filesService } from '../../services/files.service';
import { useAppStore } from '../../stores/app.store';
import type { FileHistory } from '../../types';
import { marked } from 'marked';
import hljs from 'highlight.js';

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
const restoringHash = ref<string | null>(null);

const diff = ref({
  open: false,
  loading: false,
  error: null as string | null,
  hash: '',
  parent: '' as string | undefined,
  text: '',
});

type PreviewKind = 'text' | 'image' | 'markdown' | 'code' | 'pdf' | 'video' | 'audio' | 'unsupported';
const preview = ref({
  open: false,
  loading: false,
  error: null as string | null,
  hash: '',
  kind: 'text' as PreviewKind,
  text: '',
  html: '',
  objectUrl: '',
  mime: '',
});

const previewFilename = computed(() => props.filePath.split('/').pop() || 'file');
const previewHashShort = computed(() => (preview.value.hash ? preview.value.hash.substring(0, 8) : ''));

const previewView = computed(() => {
  return {
    filename: previewFilename.value,
    hash: preview.value.hash,
    hashShort: previewHashShort.value,
  };
});

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

function getParentDir(filePath: string): string {
  const idx = filePath.lastIndexOf('/');
  if (idx <= 0) return '';
  return filePath.slice(0, idx);
}

function getExtension(p: string): string {
  const name = p.split('/').pop() || '';
  const idx = name.lastIndexOf('.');
  if (idx <= 0 || idx === name.length - 1) return '';
  return name.slice(idx + 1).toLowerCase();
}

function escapeHtml(input: string): string {
  return input
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function safeLinkHref(href: string | null | undefined): string {
  const raw = (href || '').trim();
  if (!raw) return '#';
  // 允许相对路径、锚点、http(s)、mailto
  if (raw.startsWith('#')) return raw;
  if (raw.startsWith('/')) return raw;
  if (/^https?:\/\//i.test(raw)) return raw;
  if (/^mailto:/i.test(raw)) return raw;
  return '#';
}

marked.use({
  // 防止 Markdown 原始 HTML 直接注入
  renderer: {
    html(html) {
      return escapeHtml(html);
    },
    link(href, title, text) {
      const safeHref = safeLinkHref(href);
      const t = title ? ` title="${escapeHtml(title)}"` : '';
      return `<a href="${escapeHtml(safeHref)}"${t} target="_blank" rel="noopener noreferrer">${text}</a>`;
    },
  },
  gfm: true,
  breaks: true,
});

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

  const textExts = new Set([
    'txt',
    'log',
  ]);
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

function closePreview() {
  if (preview.value.objectUrl) URL.revokeObjectURL(preview.value.objectUrl);
  preview.value = {
    open: false,
    loading: false,
    error: null,
    hash: '',
    kind: 'text',
    text: '',
    html: '',
    objectUrl: '',
    mime: '',
  };
}

function closeDiff() {
  diff.value = {
    open: false,
    loading: false,
    error: null,
    hash: '',
    parent: undefined,
    text: '',
  };
}

onBeforeUnmount(() => {
  closePreview();
  closeDiff();
});

async function viewVersion(hash: string) {
  // 打开预览并加载内容
  closePreview();
  preview.value.open = true;
  preview.value.loading = true;
  preview.value.hash = hash;
  preview.value.kind = detectPreviewKind(props.filePath);

  try {
    if (preview.value.kind === 'unsupported') {
      preview.value.loading = false;
      return;
    }

    const blob = await filesService.getFileContent(props.filePath, hash);

    if (preview.value.kind === 'image' || preview.value.kind === 'pdf') {
      const typed = new Blob([await blob.arrayBuffer()], { type: guessMimeByExt(props.filePath) });
      preview.value.objectUrl = URL.createObjectURL(typed);
    } else if (preview.value.kind === 'video' || preview.value.kind === 'audio') {
      const typed = new Blob([await blob.arrayBuffer()], { type: guessMimeByExt(props.filePath) });
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

async function viewDiff(hash: string, parent?: string) {
  closePreview();
  closeDiff();

  const kind = detectPreviewKind(props.filePath);
  if (kind !== 'text' && kind !== 'markdown' && kind !== 'code') {
    diff.value.open = true;
    diff.value.error = '仅支持文本文件的对比视图，请使用下载。';
    return;
  }

  diff.value.open = true;
  diff.value.loading = true;
  diff.value.hash = hash;
  diff.value.parent = parent;

  try {
    diff.value.text = await filesService.getFileDiff(props.filePath, hash, parent);
    if (!diff.value.text.trim()) {
      diff.value.text = '(无差异输出)';
    }
  } catch (err) {
    diff.value.error = err instanceof Error ? err.message : '获取 diff 失败';
  } finally {
    diff.value.loading = false;
  }
}

async function restoreVersion(hash: string) {
  if (hash === history.value.currentVersion) return;
  if (restoringHash.value) return;

  const short = hash.substring(0, 8);
  const ok = window.confirm(`确定要恢复到版本 ${short} 吗？这会生成一个新的提交。`);
  if (!ok) return;

  restoringHash.value = hash;
  try {
    const blob = await filesService.getFileContent(props.filePath, hash);
    const filename = props.filePath.split('/').pop() || 'file';
    const file = new File([blob], filename, { type: blob.type || 'application/octet-stream' });
    const dir = getParentDir(props.filePath);

    await filesService.uploadFile(file, dir, `恢复到版本 ${short}`);
    appStore.success('已恢复并生成新版本');
    await loadHistory();
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '恢复失败');
  } finally {
    restoringHash.value = null;
  }
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

.preview-text {
  max-height: 45vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
}

.preview-frame {
  height: 60vh;
}

.preview-iframe {
  width: 100%;
  height: 100%;
  border: 0;
}

.preview-media {
  max-height: 60vh;
}

.preview-video {
  width: 100%;
  max-height: 60vh;
}

.preview-audio {
  width: 100%;
}

.markdown-body :deep(pre) {
  max-height: 45vh;
  overflow: auto;
}

.preview-code {
  max-height: 45vh;
  overflow: auto;
  white-space: pre;
}

/* highlight.js：不引入主题色，仅做轻量层次 */
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

.diff-text {
  max-height: 45vh;
  overflow: auto;
  white-space: pre;
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
