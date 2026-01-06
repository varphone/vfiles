<template>
  <div
    class="file-browser"
    @touchstart="onTouchStart"
    @touchmove="onTouchMove"
    @touchend="onTouchEnd"
  >
    <div class="box file-browser-box">
      <div class="breadcrumb-bar">
        <div class="breadcrumb-left">
          <div
            ref="pathMenuRef"
            class="dropdown breadcrumb-path-dropdown"
            :class="{ 'is-active': pathMenuOpen }"
          >
            <div class="dropdown-trigger">
              <button
                type="button"
                class="button is-ghost breadcrumb-path-button"
                :title="currentPathLabel"
                @click="togglePathMenu"
              >
                <IconFolder :size="18" class="mr-2" />
                <span class="breadcrumb-title">{{ currentPathLabel }}</span>
              </button>
            </div>
            <div class="dropdown-menu" role="menu">
              <div class="dropdown-content breadcrumb-path-menu">
                <a
                  v-if="parentPath != null"
                  class="dropdown-item"
                  href="#"
                  @click.prevent="navigateToPathAndClose(parentPath)"
                >
                  父目录
                </a>
                <div v-else class="dropdown-item is-disabled">已是根目录</div>

                <hr class="dropdown-divider" />

                <div
                  v-if="childDirs.length === 0"
                  class="dropdown-item is-disabled"
                >
                  无子目录
                </div>
                <a
                  v-for="dir in childDirs"
                  :key="dir.path"
                  class="dropdown-item"
                  href="#"
                  @click.prevent="navigateToPathAndClose(dir.path)"
                >
                  {{ dir.name }}
                </a>
              </div>
            </div>
          </div>
        </div>
        <div class="breadcrumb-actions">
          <div class="buttons are-small mb-0">
            <button
              class="button is-light breadcrumb-icon-button"
              :disabled="!currentPath"
              @click="goBack"
              title="上一级"
            >
              <IconArrowLeft :size="18" />
            </button>
            <button
              class="button is-light breadcrumb-icon-button"
              @click="openDirManager"
              title="更多"
            >
              <IconDotsVertical :size="18" />
            </button>
          </div>
        </div>
      </div>

      <div class="file-browser-toolbar">
        <div
          v-if="isMobile && pullIndicatorVisible"
          class="has-text-centered is-size-7 has-text-grey mb-2"
        >
          <span v-if="pullRefreshing">刷新中...</span>
          <span v-else-if="pullReady">释放刷新</span>
          <span v-else>下拉刷新</span>
        </div>

        <template v-if="isMobile">
          <div class="field has-addons mt-3">
            <div class="control is-expanded">
              <input
                v-model="searchQuery"
                class="input"
                type="text"
                :placeholder="
                  searchMode === 'content' ? '搜索文件内容...' : '搜索文件名...'
                "
                list="vfiles-search-history"
                @keyup.enter="runSearch"
              />
              <datalist id="vfiles-search-history">
                <option
                  v-for="item in searchHistory"
                  :key="item"
                  :value="item"
                />
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
              <button
                class="button"
                :disabled="searchLoading"
                @click="clearSearch"
              >
                清空
              </button>
            </div>
          </div>

          <div class="field mt-2">
            <label class="checkbox">
              <input
                type="checkbox"
                v-model="searchContent"
                :disabled="searchLoading"
              />
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
                <input
                  type="checkbox"
                  v-model="searchScopeCurrent"
                  :disabled="searchLoading"
                />
                仅当前目录
              </label>
            </div>
          </div>

          <div v-if="searchError" class="notification is-danger is-light">
            <IconAlertCircle :size="20" class="mr-2" />
            {{ searchError }}
          </div>
        </template>
      </div>

      <div v-if="downloadQueue.length" class="box mb-4">
        <div class="level is-mobile">
          <div class="level-left">
            <div class="level-item">
              <div>
                <p class="heading">下载队列</p>
                <p class="title is-6">
                  {{ downloadQueue.length }} 项
                  <span v-if="downloading" class="tag is-info is-light ml-2"
                    >下载中</span
                  >
                  <span
                    v-if="queueCollapsed && activeDownload"
                    class="tag is-light ml-2 is-size-7"
                  >
                    {{ activeDownload.filename }}
                    <span v-if="activeDownloadPercent != null">
                      · {{ activeDownloadPercent }}%</span
                    >
                  </span>
                </p>
              </div>
            </div>
          </div>
          <div class="level-right">
            <div class="level-item">
              <div class="buttons">
                <button
                  class="button is-small is-light"
                  @click="toggleQueuePanel"
                  :disabled="!downloadQueue.length"
                >
                  {{ queueCollapsed ? "展开" : "最小化" }}
                </button>
                <button
                  class="button is-small is-light"
                  @click="clearFinished"
                  :disabled="downloading && downloadQueue.length === 1"
                >
                  清空已完成
                </button>
                <button
                  class="button is-small is-danger is-light"
                  @click="cancelAll"
                  :disabled="!downloadQueue.length"
                >
                  全部取消
                </button>
              </div>
            </div>
          </div>
        </div>

        <div v-if="!queueCollapsed" class="content">
          <div
            v-for="item in downloadQueue"
            :key="item.id"
            class="download-item"
          >
            <div
              class="is-flex is-justify-content-space-between is-align-items-center"
            >
              <div class="mr-2" style="min-width: 0">
                <strong class="is-size-7">{{ item.filename }}</strong>
                <span class="tag is-light ml-2 is-size-7">{{
                  item.kind === "folder" ? "ZIP" : "文件"
                }}</span>
                <span
                  v-if="item.status === 'queued'"
                  class="tag is-light ml-2 is-size-7"
                  >排队中</span
                >
                <span
                  v-else-if="item.status === 'downloading'"
                  class="tag is-info is-light ml-2 is-size-7"
                  >下载中
                  <template v-if="item.progress?.total">
                    {{ formatProgress(item.progress.loaded, item.progress.total) }}
                  </template>
                </span>
                <span
                  v-else-if="item.status === 'done'"
                  class="tag is-success is-light ml-2 is-size-7"
                  >完成</span
                >
                <span
                  v-else-if="item.status === 'canceled'"
                  class="tag is-warning is-light ml-2 is-size-7"
                  >已取消</span
                >
                <span
                  v-else-if="item.status === 'error'"
                  class="tag is-danger is-light ml-2 is-size-7"
                  >失败</span
                >
              </div>

              <div class="buttons is-right">
                <button
                  v-if="
                    item.status === 'queued' || item.status === 'downloading'
                  "
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
              v-if="
                item.status === 'downloading' &&
                item.progress &&
                item.progress.total
              "
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

      <div
        v-else-if="!searchActive && files.length === 0"
        class="has-text-centered py-6"
      >
        <IconFolderOpen :size="64" class="has-text-grey-light mb-3" />
        <p class="has-text-grey">此文件夹为空</p>
      </div>

      <div v-else-if="searchActive" class="file-list">
        <p class="has-text-grey is-size-7 mb-2">
          搜索结果：{{ searchResults.length }} 项（{{
            searchMode === "content" ? "内容" : "文件名"
          }}）
        </p>
        <div v-if="searchResults.length === 0" class="has-text-centered py-6">
          <p class="has-text-grey">没有找到匹配的文件</p>
        </div>
        <FileList
          v-if="visibleSearchResults.length"
          :files="visibleSearchResults"
          :highlight="searchQuery"
          :select-mode="batchMode"
          :selected-paths="selectedPaths"
          @click="handleSearchItemClick"
          @download="handleDownload"
          @delete="handleDelete"
          @view-history="handleViewHistory"
          @toggle-select="toggleSelect"
        />

        <div
          v-if="isMobile && hasMore"
          ref="loadMoreSentinel"
          class="has-text-centered has-text-grey is-size-7 py-2"
        >
          继续下滑加载更多...
        </div>
      </div>

      <div v-else class="file-list">
        <FileList
          :files="visibleFiles"
          :select-mode="batchMode"
          :selected-paths="selectedPaths"
          @click="handleFileClick"
          @download="handleDownload"
          @delete="handleDelete"
          @view-history="handleViewHistory"
          @toggle-select="toggleSelect"
        />

        <div
          v-if="isMobile && hasMore"
          ref="loadMoreSentinel"
          class="has-text-centered has-text-grey is-size-7 py-2"
        >
          继续下滑加载更多...
        </div>
      </div>
    </div>

    <!-- 上传对话框 -->
    <Modal
      :show="showUploader"
      title="上传文件"
      :mobile-compact="true"
      @close="showUploader = false"
    >
      <FileUploader
        :target-path="filesStore.currentPath"
        @upload="handleUpload"
        @close="showUploader = false"
      />
    </Modal>

    <!-- 目录管理对话框 -->
    <Modal
      :show="dirManagerOpen"
      title="目录管理"
      :mobile-compact="true"
      @close="dirManagerOpen = false"
    >
      <div class="content">
        <h3 class="title is-6">添加子目录</h3>
        <div class="field has-addons">
          <div class="control is-expanded">
            <input
              v-model="newDirName"
              class="input"
              type="text"
              placeholder="目录名"
            />
          </div>
          <div class="control">
            <button
              class="button is-primary"
              :disabled="!newDirName.trim() || dirOpBusy"
              :class="{ 'is-loading': dirOpLoading === 'create' }"
              @click="createSubDir"
            >
              添加
            </button>
          </div>
        </div>

        <hr />

        <h3 class="title is-6">重命名当前目录</h3>
        <p v-if="!currentPath" class="has-text-grey is-size-7">
          根目录不可重命名
        </p>
        <div class="field has-addons">
          <div class="control is-expanded">
            <input
              v-model="renameDirName"
              class="input"
              type="text"
              placeholder="新目录名"
              :disabled="!currentPath"
            />
          </div>
          <div class="control">
            <button
              class="button is-warning"
              :disabled="!currentPath || !renameDirName.trim() || dirOpBusy"
              :class="{ 'is-loading': dirOpLoading === 'rename' }"
              @click="renameCurrentDir"
            >
              重命名
            </button>
          </div>
        </div>

        <hr />

        <h3 class="title is-6">删除当前目录</h3>
        <p v-if="!currentPath" class="has-text-grey is-size-7">
          根目录不可删除
        </p>
        <button
          class="button is-danger"
          :disabled="!currentPath || dirOpBusy"
          :class="{ 'is-loading': dirOpLoading === 'delete' }"
          @click="deleteCurrentDir"
        >
          删除当前目录
        </button>
      </div>
    </Modal>

    <!-- 历史记录对话框 -->
    <Modal
      :show="showHistory"
      :title="`文件历史: ${selectedFile?.name}`"
      :mobile-compact="true"
      @close="showHistory = false"
    >
      <VersionHistory v-if="selectedFile" :file-path="selectedFile.path" />
    </Modal>

    <!-- 预览对话框（当前版本） -->
    <Modal
      :show="preview.open"
      :title="`预览: ${previewFilename}`"
      :mobile-compact="true"
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
          <img
            :src="preview.objectUrl"
            :alt="previewFilename"
            loading="lazy"
            decoding="async"
          />
        </figure>

        <div v-else-if="preview.kind === 'pdf'" class="preview-frame">
          <iframe
            :src="preview.objectUrl"
            title="PDF 预览"
            class="preview-iframe"
          />
        </div>

        <div v-else-if="preview.kind === 'video'" class="preview-media">
          <video :src="preview.objectUrl" controls class="preview-video" />
        </div>

        <div v-else-if="preview.kind === 'audio'" class="preview-media">
          <audio :src="preview.objectUrl" controls class="preview-audio" />
        </div>

        <div
          v-else-if="preview.kind === 'markdown'"
          class="content markdown-body"
          v-html="preview.html"
        ></div>

        <div v-else-if="preview.kind === 'code'" class="content">
          <pre
            class="preview-code hljs"
          ><code v-html="preview.html"></code></pre>
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
import {
  ref,
  onMounted,
  onBeforeUnmount,
  computed,
  watch,
  nextTick,
} from "vue";
import { storeToRefs } from "pinia";
import {
  IconFolderOpen,
  IconFolder,
  IconAlertCircle,
  IconSearch,
  IconArrowLeft,
  IconDotsVertical,
} from "@tabler/icons-vue";
import { useFilesStore } from "../../stores/files.store";
import { useAppStore } from "../../stores/app.store";
import { filesService } from "../../services/files.service";
import FileList from "./FileList.vue";
import FileUploader from "../file-uploader/FileUploader.vue";
import VersionHistory from "../version-history/VersionHistory.vue";
import Modal from "../common/Modal.vue";
import type { FileInfo } from "../../types";

let cachedMarked: any | null = null;
let cachedHljs: any | null = null;

const filesStore = useFilesStore();
const appStore = useAppStore();
const { files, loading, error, currentPath, browseCommit } =
  storeToRefs(filesStore);

const currentPathLabel = computed(() => {
  return currentPath.value ? `/${currentPath.value}` : "根目录";
});

const isMobile = ref(false);

function updateIsMobile() {
  isMobile.value = window.matchMedia("(max-width: 768px)").matches;
}

const showUploader = ref(false);
const showHistory = ref(false);
const selectedFile = ref<FileInfo | null>(null);

type PreviewKind =
  | "text"
  | "image"
  | "markdown"
  | "code"
  | "pdf"
  | "video"
  | "audio"
  | "unsupported";
const preview = ref({
  open: false,
  loading: false,
  error: null as string | null,
  path: "",
  kind: "text" as PreviewKind,
  text: "",
  html: "",
  objectUrl: "",
});

const previewFilename = computed(
  () => preview.value.path.split("/").pop() || "file",
);

function getExtension(p: string): string {
  const name = p.split("/").pop() || "";
  const idx = name.lastIndexOf(".");
  if (idx <= 0 || idx === name.length - 1) return "";
  return name.slice(idx + 1).toLowerCase();
}

function detectPreviewKind(filePath: string): PreviewKind {
  const ext = getExtension(filePath);
  const imageExts = new Set([
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "bmp",
    "svg",
  ]);
  if (imageExts.has(ext)) return "image";

  if (ext === "pdf") return "pdf";

  const videoExts = new Set(["mp4", "webm", "ogg", "mov", "m4v"]);
  if (videoExts.has(ext)) return "video";

  const audioExts = new Set(["mp3", "wav", "ogg", "m4a", "aac", "flac"]);
  if (audioExts.has(ext)) return "audio";

  const mdExts = new Set(["md", "markdown"]);
  if (mdExts.has(ext)) return "markdown";

  const codeExts = new Set([
    "js",
    "ts",
    "jsx",
    "tsx",
    "vue",
    "json",
    "css",
    "scss",
    "html",
    "xml",
    "yml",
    "yaml",
    "csv",
    "log",
    "sh",
    "py",
    "java",
    "c",
    "cpp",
    "go",
    "rs",
  ]);
  if (codeExts.has(ext)) return "code";

  const textExts = new Set(["txt", "log"]);
  if (textExts.has(ext) || ext === "") return "text";

  return "unsupported";
}

function guessMimeByExt(filePath: string): string {
  const ext = getExtension(filePath);
  if (ext === "pdf") return "application/pdf";
  if (ext === "svg") return "image/svg+xml";
  if (ext === "png") return "image/png";
  if (ext === "jpg" || ext === "jpeg") return "image/jpeg";
  if (ext === "gif") return "image/gif";
  if (ext === "webp") return "image/webp";
  if (ext === "bmp") return "image/bmp";

  if (ext === "mp4" || ext === "m4v") return "video/mp4";
  if (ext === "webm") return "video/webm";
  if (ext === "mov") return "video/quicktime";
  if (ext === "ogg") return "application/ogg";

  if (ext === "mp3") return "audio/mpeg";
  if (ext === "wav") return "audio/wav";
  if (ext === "m4a") return "audio/mp4";
  if (ext === "aac") return "audio/aac";
  if (ext === "flac") return "audio/flac";

  return "application/octet-stream";
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function safeLinkHref(href: string | null | undefined): string {
  const raw = (href || "").trim();
  if (!raw) return "#";
  if (raw.startsWith("#")) return raw;
  if (raw.startsWith("/")) return raw;
  if (/^https?:\/\//i.test(raw)) return raw;
  if (/^mailto:/i.test(raw)) return raw;
  return "#";
}

function safeImageSrc(src: string | null | undefined): string {
  const raw = (src || "").trim();
  if (!raw) return "";
  if (/^https?:\/\//i.test(raw)) return raw;
  if (/^data:image\//i.test(raw)) return raw;
  if (raw.startsWith("/")) return raw;
  return "";
}

async function getMarked() {
  if (cachedMarked) return cachedMarked;
  const mod: any = await import("marked");
  const markedApi = mod?.marked ?? mod;

  const mdRenderer: any = {
    html(token: any) {
      const html =
        typeof token === "string" ? token : (token?.text ?? token?.raw ?? "");
      return escapeHtml(String(html));
    },
    link(tokenOrHref: any, title?: any, text?: any) {
      const href =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.href
          : tokenOrHref;
      const linkTitle =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.title
          : title;
      const linkText =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.text
          : text;

      const safeHref = safeLinkHref(href);
      const t = linkTitle ? ` title="${escapeHtml(String(linkTitle))}"` : "";
      const inner =
        typeof linkText === "string"
          ? (markedApi.parseInline(linkText) as string)
          : "";
      return `<a href="${escapeHtml(safeHref)}"${t} target="_blank" rel="noopener noreferrer">${inner}</a>`;
    },
    image(tokenOrHref: any, title?: any, text?: any) {
      // 兼容：新版传 token 对象；旧版传 (href, title, text)
      const href =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.href
          : tokenOrHref;
      const imgTitle =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.title
          : title;
      const altText =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.text
          : text;

      const safeSrc = safeImageSrc(href);
      if (!safeSrc) return "";

      const t = imgTitle ? ` title="${escapeHtml(String(imgTitle))}"` : "";
      const alt = altText ? escapeHtml(String(altText)) : "";
      return `<img src="${escapeHtml(safeSrc)}" alt="${alt}" loading="lazy" decoding="async"${t} />`;
    },
  };

  markedApi.use({
    renderer: mdRenderer,
    gfm: true,
    breaks: true,
  });

  cachedMarked = markedApi;
  return markedApi;
}

async function getHljs() {
  if (cachedHljs) return cachedHljs;
  const mod: any = await import("highlight.js");
  cachedHljs = mod?.default ?? mod;
  return cachedHljs;
}

function closePreview() {
  if (preview.value.objectUrl) URL.revokeObjectURL(preview.value.objectUrl);
  preview.value = {
    open: false,
    loading: false,
    error: null,
    path: "",
    kind: "text",
    text: "",
    html: "",
    objectUrl: "",
  };
}

async function openPreview(filePath: string) {
  closePreview();
  preview.value.open = true;
  preview.value.loading = true;
  preview.value.path = filePath;
  preview.value.kind = detectPreviewKind(filePath);

  try {
    if (preview.value.kind === "unsupported") {
      preview.value.loading = false;
      return;
    }

    const blob = await filesService.getFileContent(
      filePath,
      browseCommit.value,
    );

    if (
      preview.value.kind === "image" ||
      preview.value.kind === "pdf" ||
      preview.value.kind === "video" ||
      preview.value.kind === "audio"
    ) {
      const typed = new Blob([await blob.arrayBuffer()], {
        type: guessMimeByExt(filePath),
      });
      preview.value.objectUrl = URL.createObjectURL(typed);
    } else {
      const text = await blob.text();
      if (preview.value.kind === "markdown") {
        const markedApi = await getMarked();
        preview.value.html = markedApi.parse(text) as string;
      } else if (preview.value.kind === "code") {
        const hljsApi = await getHljs();
        const highlighted = hljsApi.highlightAuto(text);
        preview.value.html = highlighted.value;
      } else {
        preview.value.text = text;
      }
    }
  } catch (err) {
    preview.value.error = err instanceof Error ? err.message : "预览失败";
  } finally {
    preview.value.loading = false;
  }
}

const searchQuery = ref("");
const searchResults = ref<FileInfo[]>([]);
const searchLoading = ref(false);
const searchError = ref<string | null>(null);
const searchActive = ref(false);
const searchContent = ref(false);

const queueCollapsed = ref(false);
const activeDownload = computed(() =>
  downloadQueue.value.find((x) => x.status === "downloading"),
);
const activeDownloadPercent = computed(() => {
  const a = activeDownload.value;
  if (!a?.progress?.total) return null;
  if (a.progress.total <= 0) return null;
  return Math.min(
    100,
    Math.floor((a.progress.loaded / a.progress.total) * 100),
  );
});

type DownloadQueueStatus =
  | "queued"
  | "downloading"
  | "done"
  | "error"
  | "canceled";
type DownloadQueueKind = "file" | "folder";
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
const downloading = computed(() =>
  downloadQueue.value.some((x) => x.status === "downloading"),
);

const batchMode = ref(false);
const selectedPaths = ref<Set<string>>(new Set());

const selectedCount = computed(() => selectedPaths.value.size);

const searchType = ref<"all" | "file" | "directory">("all");
const searchScopeCurrent = ref(false);

const searchMode = computed(() => (searchContent.value ? "content" : "name"));

const SEARCH_HISTORY_KEY = "vfiles.searchHistory";
const searchHistory = ref<string[]>([]);

function loadSearchHistory() {
  try {
    const raw = localStorage.getItem(SEARCH_HISTORY_KEY);
    if (!raw) return;
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      searchHistory.value = parsed
        .filter((x) => typeof x === "string")
        .slice(0, 10);
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

  const withoutDup = searchHistory.value.filter(
    (x) => x.toLowerCase() !== normalized.toLowerCase(),
  );
  saveSearchHistory([normalized, ...withoutDup].slice(0, 10));
}

onMounted(() => {
  filesStore.loadFiles();
  loadSearchHistory();

  const onDocPointer = (e: MouseEvent | TouchEvent) => {
    if (!pathMenuOpen.value) return;
    const el = pathMenuRef.value;
    const target = e.target as Node | null;
    if (!el || !target) return;
    if (!el.contains(target)) closePathMenu();
  };

  const onDocKeydown = (e: KeyboardEvent) => {
    if (!pathMenuOpen.value) return;
    if (e.key === "Escape") closePathMenu();
  };

  document.addEventListener("click", onDocPointer, true);
  document.addEventListener("touchstart", onDocPointer, true);
  document.addEventListener("keydown", onDocKeydown);
  onBeforeUnmount(() => {
    document.removeEventListener("click", onDocPointer, true);
    document.removeEventListener("touchstart", onDocPointer, true);
    document.removeEventListener("keydown", onDocKeydown);
  });

  updateIsMobile();
  try {
    const mql = window.matchMedia("(max-width: 768px)");
    const handler = () => updateIsMobile();
    if ("addEventListener" in mql) {
      mql.addEventListener("change", handler);
      onBeforeUnmount(() => mql.removeEventListener("change", handler));
    } else {
      // @ts-expect-error older Safari
      mql.addListener(handler);
      // @ts-expect-error older Safari
      onBeforeUnmount(() => mql.removeListener(handler));
    }
  } catch {
    // ignore
  }
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

function goBack() {
  filesStore.goBack();
}

function goRoot() {
  navigateTo("");
}

const pathMenuOpen = ref(false);
const pathMenuRef = ref<HTMLElement | null>(null);

const parentPath = computed<string | null>(() => {
  const cur = currentPath.value || "";
  const parts = cur.split("/").filter(Boolean);
  if (parts.length === 0) return null;
  parts.pop();
  return parts.join("/");
});

const childDirs = computed(() => {
  return files.value
    .filter((f) => f.type === "directory")
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name, "zh-Hans-CN"));
});

function closePathMenu() {
  pathMenuOpen.value = false;
}

function togglePathMenu() {
  pathMenuOpen.value = !pathMenuOpen.value;
}

function navigateToPathAndClose(path: string) {
  closePathMenu();
  navigateTo(path);
}

const dirManagerOpen = ref(false);
const dirOpLoading = ref<null | "create" | "rename" | "delete">(null);
const dirOpBusy = computed(() => dirOpLoading.value !== null);
const newDirName = ref("");
const renameDirName = ref("");

const currentDirName = computed(() => {
  if (!currentPath.value) return "";
  const parts = currentPath.value.split("/").filter(Boolean);
  return parts[parts.length - 1] || "";
});

watch(
  () => currentPath.value,
  () => {
    renameDirName.value = currentDirName.value;
  },
  { immediate: true },
);

function openDirManager() {
  dirManagerOpen.value = true;
  newDirName.value = "";
  renameDirName.value = currentDirName.value;
}

function isSafeDirName(name: string): boolean {
  const n = name.trim();
  if (!n) return false;
  if (n === "." || n === "..") return false;
  if (n.includes("/") || n.includes("\\")) return false;
  return true;
}

async function createSubDir() {
  const name = newDirName.value.trim();
  if (!isSafeDirName(name)) {
    appStore.error("非法目录名");
    return;
  }

  const dirPath = currentPath.value ? `${currentPath.value}/${name}` : name;
  dirOpLoading.value = "create";
  try {
    await filesService.createDirectory(dirPath, `创建目录: ${dirPath}`);
    appStore.success("目录创建成功");
    newDirName.value = "";
    refresh();
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "目录创建失败");
  } finally {
    if (dirOpLoading.value === "create") dirOpLoading.value = null;
  }
}

async function renameCurrentDir() {
  if (!currentPath.value) return;
  const name = renameDirName.value.trim();
  if (!isSafeDirName(name)) {
    appStore.error("非法目录名");
    return;
  }
  if (name === currentDirName.value) {
    appStore.error("目录名未变化");
    return;
  }

  const parts = currentPath.value.split("/").filter(Boolean);
  parts.pop();
  const parent = parts.join("/");
  const to = parent ? `${parent}/${name}` : name;

  dirOpLoading.value = "rename";
  try {
    await filesService.movePath(
      currentPath.value,
      to,
      `重命名目录: ${currentPath.value} -> ${to}`,
    );
    appStore.success("重命名成功");
    dirManagerOpen.value = false;
    navigateTo(to);
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "重命名失败");
  } finally {
    if (dirOpLoading.value === "rename") dirOpLoading.value = null;
  }
}

async function deleteCurrentDir() {
  if (!currentPath.value) return;

  const expected = currentDirName.value;
  const typed = prompt(
    `危险操作：删除目录 /${currentPath.value}\n\n此操作会删除其下全部内容，并生成提交。\n请输入目录名“${expected}”以确认：`,
    "",
  );
  if (typed == null) return;
  if (typed.trim() !== expected) {
    appStore.error("确认失败：目录名不匹配");
    return;
  }

  const parts = currentPath.value.split("/").filter(Boolean);
  parts.pop();
  const parent = parts.join("/");

  dirOpLoading.value = "delete";
  try {
    await filesService.deleteFile(
      currentPath.value,
      `删除目录: ${currentPath.value}`,
    );
    appStore.success("目录删除成功");
    dirManagerOpen.value = false;
    navigateTo(parent);
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "删除失败");
  } finally {
    if (dirOpLoading.value === "delete") dirOpLoading.value = null;
  }
}

// 4.2: 移动端无限滚动（分批渲染）
const MOBILE_INITIAL_COUNT = 40;
const MOBILE_CHUNK_COUNT = 30;
const mobileVisibleCount = ref(MOBILE_INITIAL_COUNT);
const loadMoreSentinel = ref<HTMLElement | null>(null);
let loadMoreObserver: IntersectionObserver | null = null;

const activeList = computed(() =>
  searchActive.value ? searchResults.value : files.value,
);
const hasMore = computed(
  () => isMobile.value && mobileVisibleCount.value < activeList.value.length,
);
const visibleFiles = computed(() => {
  if (!isMobile.value) return files.value;
  return files.value.slice(0, mobileVisibleCount.value);
});
const visibleSearchResults = computed(() => {
  if (!isMobile.value) return searchResults.value;
  return searchResults.value.slice(0, mobileVisibleCount.value);
});

function bumpVisibleCount() {
  const total = activeList.value.length;
  mobileVisibleCount.value = Math.min(
    total,
    mobileVisibleCount.value + MOBILE_CHUNK_COUNT,
  );
}

function resetVisibleCount() {
  mobileVisibleCount.value = MOBILE_INITIAL_COUNT;
}

function setupLoadMoreObserver() {
  if (loadMoreObserver) {
    loadMoreObserver.disconnect();
    loadMoreObserver = null;
  }

  if (!isMobile.value) return;
  if (!("IntersectionObserver" in window)) return;
  if (!loadMoreSentinel.value) return;

  loadMoreObserver = new IntersectionObserver(
    (entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      if (!hasMore.value) return;
      bumpVisibleCount();
    },
    { root: null, threshold: 0.1 },
  );

  loadMoreObserver.observe(loadMoreSentinel.value);
}

watch(
  [
    () => filesStore.currentPath,
    searchActive,
    searchQuery,
    () => searchResults.value.length,
  ],
  () => {
    resetVisibleCount();
    void nextTick().then(() => setupLoadMoreObserver());
  },
);

watch([isMobile, loadMoreSentinel], () => {
  void nextTick().then(() => setupLoadMoreObserver());
});

onBeforeUnmount(() => {
  if (loadMoreObserver) {
    loadMoreObserver.disconnect();
    loadMoreObserver = null;
  }
});

// 4.2: 下拉刷新 + 手势（边缘右滑返回）
const pullDistance = ref(0);
const pullReady = ref(false);
const pullRefreshing = ref(false);

const pullIndicatorVisible = computed(
  () => pullRefreshing.value || pullDistance.value > 10,
);

const touchStart = ref({ x: 0, y: 0, t: 0 });
const touchMode = ref<"none" | "pull" | "swipe">("none");

const anyModalOpen = computed(() => {
  return (
    showUploader.value ||
    showHistory.value ||
    preview.value.open ||
    pathMenuOpen.value
  );
});

function onTouchStart(e: TouchEvent) {
  if (!isMobile.value) return;
  if (anyModalOpen.value) return;
  const t = e.touches[0];
  if (!t) return;
  touchStart.value = { x: t.clientX, y: t.clientY, t: Date.now() };
  touchMode.value = "none";
}

function onTouchMove(e: TouchEvent) {
  if (!isMobile.value) return;
  if (anyModalOpen.value) return;
  const t = e.touches[0];
  if (!t) return;

  const dx = t.clientX - touchStart.value.x;
  const dy = t.clientY - touchStart.value.y;

  if (touchMode.value === "none") {
    if (Math.abs(dx) > 12 && Math.abs(dx) > Math.abs(dy)) {
      touchMode.value = "swipe";
    } else if (dy > 8 && Math.abs(dy) > Math.abs(dx) && window.scrollY <= 0) {
      touchMode.value = "pull";
    }
  }

  if (
    touchMode.value === "pull" &&
    window.scrollY <= 0 &&
    !pullRefreshing.value
  ) {
    const next = Math.min(90, Math.max(0, dy));
    pullDistance.value = next;
    pullReady.value = next >= 60;
  }
}

async function onTouchEnd(e: TouchEvent) {
  if (!isMobile.value) return;
  if (anyModalOpen.value) return;

  const changed = e.changedTouches[0];
  if (!changed) {
    pullDistance.value = 0;
    pullReady.value = false;
    touchMode.value = "none";
    return;
  }

  const dx = changed.clientX - touchStart.value.x;
  const dy = changed.clientY - touchStart.value.y;

  if (touchMode.value === "swipe") {
    const fromEdge = touchStart.value.x <= 24;
    const horizontal = dx > 80 && Math.abs(dy) < 60;
    if (fromEdge && horizontal) {
      goBack();
    }
  }

  if (touchMode.value === "pull" && pullReady.value && !pullRefreshing.value) {
    pullRefreshing.value = true;
    try {
      await Promise.resolve(refresh());
      appStore.success("已刷新");
    } finally {
      pullRefreshing.value = false;
    }
  }

  pullDistance.value = 0;
  pullReady.value = false;
  touchMode.value = "none";
}

function enqueueDownload(kind: DownloadQueueKind, path: string) {
  const wasEmpty = downloadQueue.value.length === 0;
  const filename =
    kind === "folder"
      ? `${path.split("/").filter(Boolean).pop() || "root"}.zip`
      : path.split("/").pop() || "download";

  downloadQueue.value = [
    ...downloadQueue.value,
    {
      id: nextDownloadId++,
      kind,
      path,
      filename,
      status: "queued",
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

  const next = downloadQueue.value.find((x) => x.status === "queued");
  if (!next) return;

  const abort = new AbortController();
  downloadQueue.value = downloadQueue.value.map((x) =>
    x.id === next.id
      ? { ...x, status: "downloading", progress: { loaded: 0 }, abort }
      : x,
  );

  try {
    const onProgress = (p: { loaded: number; total?: number }) => {
      downloadQueue.value = downloadQueue.value.map((x) =>
        x.id === next.id ? { ...x, progress: p } : x,
      );
    };

    const commit = browseCommit.value;
    const result =
      next.kind === "folder"
        ? await filesService.fetchFolderDownload(next.path, commit, {
            signal: abort.signal,
            onProgress,
          })
        : await filesService.fetchFileDownload(next.path, commit, {
            signal: abort.signal,
            onProgress,
          });

    filesService.saveDownloadedBlob(result.blob, result.filename);
    downloadQueue.value = downloadQueue.value.map((x) =>
      x.id === next.id ? { ...x, status: "done", abort: undefined } : x,
    );
  } catch (err: any) {
    const isAbort = err?.name === "AbortError";
    downloadQueue.value = downloadQueue.value.map((x) =>
      x.id === next.id
        ? {
            ...x,
            status: isAbort ? "canceled" : "error",
            error: isAbort
              ? undefined
              : err instanceof Error
                ? err.message
                : "下载失败",
            abort: undefined,
          }
        : x,
    );
  } finally {
    // 继续下一个
    void processQueue();
  }
}

function formatProgress(loaded: number, total: number): string {
  const percent = Math.floor((loaded / total) * 100);
  const formatSize = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
  };
  return ` ${percent}% (${formatSize(loaded)}/${formatSize(total)})`;
}

function cancelItem(id: number) {
  const item = downloadQueue.value.find((x) => x.id === id);
  if (!item) return;

  if (item.status === "queued") {
    downloadQueue.value = downloadQueue.value.map((x) =>
      x.id === id ? { ...x, status: "canceled" } : x,
    );
    return;
  }

  if (item.status === "downloading") {
    item.abort?.abort();
  }
}

function cancelAll() {
  for (const item of downloadQueue.value) {
    if (item.status === "queued") {
      downloadQueue.value = downloadQueue.value.map((x) =>
        x.id === item.id ? { ...x, status: "canceled" } : x,
      );
    } else if (item.status === "downloading") {
      item.abort?.abort();
    }
  }
}

function clearFinished() {
  downloadQueue.value = downloadQueue.value.filter(
    (x) => x.status === "queued" || x.status === "downloading",
  );
}

function removeItem(id: number) {
  downloadQueue.value = downloadQueue.value.filter((x) => x.id !== id);
}

function handleFileClick(file: FileInfo) {
  if (file.type === "directory") {
    navigateTo(file.path);
    return;
  }

  openPreview(file.path);
}

function handleSearchItemClick(file: FileInfo) {
  if (file.type === "directory") {
    clearSearch();
    navigateTo(file.path);
    return;
  }

  openPreview(file.path);
}

function handleDownload(file: FileInfo) {
  if (file.type === "directory") {
    // 文件夹下载：使用队列方式（ZIP 流式生成，无法预知大小）
    enqueueDownload("folder", file.path);
    appStore.success("已加入下载队列");
  } else {
    // 单文件下载：使用浏览器原生下载（支持原生进度显示）
    filesService.downloadFile(file.path, browseCommit.value);
  }
}

async function handleDelete(file: FileInfo) {
  try {
    await filesStore.deleteFile(file.path);
    appStore.success("文件删除成功");
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "删除失败");
  }
}

function handleViewHistory(file: FileInfo) {
  selectedFile.value = file;
  showHistory.value = true;
}

async function handleUpload() {
  showUploader.value = false;
  appStore.success("文件上传成功");
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
    const scopePath = searchScopeCurrent.value ? filesStore.currentPath : "";
    searchResults.value = await filesService.searchFiles(q, searchMode.value, {
      type: searchType.value,
      path: scopePath,
    });
  } catch (err) {
    searchError.value = err instanceof Error ? err.message : "搜索失败";
    searchResults.value = [];
  } finally {
    searchLoading.value = false;
  }
}

function clearSearch() {
  searchQuery.value = "";
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

defineExpose({
  openUploader: () => {
    showUploader.value = true;
  },
  refresh,
  goBack,
  goRoot,
  toggleBatchMode,
  setSearchQuery: (q: string) => {
    searchQuery.value = q;
  },
  runSearch,
  clearSearch,
  batchMode,
  selectedCount,
  selectAllVisible,
  clearSelection,
  batchDownload,
  batchDelete,
  batchMove,
  renameSelected,
  searchLoading,
});

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

  // 分离文件和文件夹
  const files = items.filter((f) => f.type !== "directory");
  const folders = items.filter((f) => f.type === "directory");

  if (files.length > 10) {
    const ok = confirm(
      `将开始下载 ${files.length} 个文件，可能会被浏览器拦截弹窗。继续吗？`,
    );
    if (!ok) return;
  }

  // 单文件使用浏览器原生下载
  for (const f of files) {
    filesService.downloadFile(f.path, browseCommit.value);
  }

  // 文件夹使用队列下载
  for (const f of folders) {
    enqueueDownload("folder", f.path);
  }

  if (files.length > 0 && folders.length > 0) {
    appStore.success(
      `已开始下载 ${files.length} 个文件，${folders.length} 个文件夹已加入队列`,
    );
  } else if (files.length > 0) {
    appStore.success(`已开始下载 ${files.length} 个文件`);
  } else if (folders.length > 0) {
    appStore.success(`已加入 ${folders.length} 个文件夹到下载队列`);
  }
}

async function batchDelete() {
  const items = getSelectedItems();
  if (items.length === 0) return;

  const ok = confirm(
    `确定要删除 ${items.length} 项吗？此操作会生成一次或多次提交。`,
  );
  if (!ok) return;

  try {
    for (const f of items) {
      await filesService.deleteFile(f.path, "批量删除");
    }
    appStore.success("批量删除完成");
    clearSelection();
    await refresh();
    if (searchActive.value) {
      await doSearch(false);
    }
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "批量删除失败");
  }
}

async function batchMove() {
  const items = getSelectedItems();
  if (items.length === 0) return;

  const raw = prompt(
    "输入目标目录（相对路径，留空表示根目录）",
    filesStore.currentPath || "",
  );
  if (raw == null) return;
  const targetDir = raw
    .trim()
    .replace(/\\/g, "/")
    .replace(/^\/+/, "")
    .replace(/\/+$/, "");

  try {
    for (const f of items) {
      const to = targetDir ? `${targetDir}/${f.name}` : f.name;
      await filesService.movePath(f.path, to, "批量移动");
    }
    appStore.success("批量移动完成");
    clearSelection();
    await refresh();
    if (searchActive.value) {
      await doSearch(false);
    }
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "批量移动失败");
  }
}

async function renameSelected() {
  const items = getSelectedItems();
  if (items.length !== 1) return;
  const f = items[0];

  const raw = prompt("输入新名称（仅名称，不含路径分隔符）", f.name);
  if (raw == null) return;
  const name = raw.trim();
  if (
    !name ||
    name === "." ||
    name === ".." ||
    name.includes("/") ||
    name.includes("\\")
  ) {
    appStore.error("非法名称");
    return;
  }

  const parts = f.path.split("/");
  parts.pop();
  const dir = parts.join("/");
  const to = dir ? `${dir}/${name}` : name;

  try {
    await filesService.movePath(f.path, to, `重命名: ${f.name} -> ${name}`);
    appStore.success("重命名成功");
    clearSelection();
    await refresh();
    if (searchActive.value) {
      await doSearch(false);
    }
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "重命名失败");
  }
}
</script>

<style scoped>
.file-browser {
  max-width: 1200px;
  margin: 0 auto;
  padding: 1rem;
}

.breadcrumb-bar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.75rem;
}

.breadcrumb-left {
  flex: 1;
  min-width: 0;
}

.breadcrumb-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.breadcrumb-actions {
  flex: 0 0 auto;
}

.breadcrumb-path-dropdown {
  min-width: 0;
}

.breadcrumb-path-button {
  padding: 0;
  min-width: 0;
  height: auto;
}

.breadcrumb-path-button :deep(.icon) {
  flex: 0 0 auto;
}

.breadcrumb-path-menu {
  max-height: 50vh;
  overflow: auto;
}

.breadcrumb-icon-button {
  width: 2.25rem;
  min-width: 2.25rem;
  height: 2.25rem;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
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
    padding: 0;
  }

  .file-browser-box {
    padding: 0.5rem;
  }

  .file-browser-toolbar {
    display: none;
  }

  .breadcrumb-title {
    max-width: 70vw;
  }

  .breadcrumb-actions .buttons {
    flex-wrap: nowrap;
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

  .level .button {
    width: 100%;
    justify-content: center;
  }
}
</style>
