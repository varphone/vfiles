<template>
  <div class="version-history">
    <div class="history-summary-row">
      <p class="history-summary">
        <span class="history-summary-strong">{{ history.totalCommits }}</span>
        个版本
        <template v-if="currentShort">
          <span class="history-summary-sep">·</span>
          当前
          <code class="history-summary-hash" :title="history.currentVersion">
            {{ currentShort }}
          </code>
        </template>
      </p>
      <button
        v-if="detailOpen"
        class="vf-ghost-button history-detail-close"
        type="button"
        @click="closeDetail"
      >
        收起详情
      </button>
    </div>

    <div class="history-layout" :class="{ 'has-detail': detailOpen }">
      <section class="history-list-pane" aria-label="版本列表">
        <SkeletonList
          v-if="loading"
          :rows="5"
          row-height="111px"
          label="加载历史记录"
        />

        <EmptyState
          v-else-if="error"
          :icon="IconAlertCircle"
          tone="error"
          compact
          title="加载历史记录失败"
          :hint="error"
        >
          <template #actions>
            <button class="vf-ghost-button is-primary" @click="loadHistory">
              <IconRefresh :size="16" />
              <span>重试</span>
            </button>
          </template>
        </EmptyState>

        <EmptyState
          v-else-if="history.commits.length === 0"
          :icon="IconHistory"
          compact
          title="暂无历史记录"
          hint="文件每次上传新版本后，都会在这里留下记录"
        />

        <template v-else>
          <p class="history-retention">
            <IconInfoCircle :size="14" />
            <span>
              历史版本会长期保留，可随时预览、对比、下载或恢复；删除该文件会同时删除这些历史。
            </span>
          </p>

          <CommitList
            :commits="history.commits"
            :total-commits="history.totalCommits"
            :current-version="history.currentVersion"
            :restoring-hash="restoringHash"
            :format-date="formatDate"
            :selected-hash="selectedHash"
            @view-version="viewVersion"
            @view-diff="viewDiff"
            @restore-version="restoreVersion"
            @download-version="downloadVersion"
          />

          <div
            v-if="nextCursor"
            class="history-more"
          >
            <button
              class="vf-ghost-button"
              type="button"
              :disabled="loading"
              @click="loadMore"
            >
              加载更多（还有
              {{ history.totalCommits - history.commits.length }} 个）
            </button>
          </div>
        </template>
      </section>

      <aside class="history-detail-pane" aria-label="版本详情">
        <!-- 文本对比 -->
        <template v-if="diff.open">
          <header class="history-detail-head">
            <div class="history-detail-titles">
              <p class="history-detail-label">版本对比</p>
              <p class="history-detail-title">
                <code :title="diff.hash">{{ diff.hash.substring(0, 8) }}</code>
                <span class="history-detail-name">{{
                  previewView.filename
                }}</span>
              </p>
            </div>
            <button class="vf-ghost-button" type="button" @click="closeDiff">
              关闭
            </button>
          </header>

          <div class="history-detail-body">
            <SkeletonList
              v-if="diff.loading"
              variant="lines"
              :rows="8"
              label="加载版本对比"
            />

            <p v-else-if="diff.error" class="history-inline-note is-warning">
              <IconAlertTriangle :size="14" />
              <span>{{ diff.error }}</span>
            </p>

            <div v-else class="diff-view">
              <div class="diff-toolbar">
                <button
                  type="button"
                  class="vf-ghost-button"
                  :class="{ 'is-active': diffView === 'unified' }"
                  title="统一视图（U）"
                  @click="diffView = 'unified'"
                >
                  统一
                </button>
                <button
                  type="button"
                  class="vf-ghost-button"
                  :class="{ 'is-active': diffView === 'split' }"
                  title="并排视图（S）"
                  @click="diffView = 'split'"
                >
                  并排
                </button>
                <button
                  type="button"
                  class="vf-ghost-button"
                  title="恢复到此版本（生成新提交）"
                  @click="restoreVersion(diff.hash)"
                >
                  恢复此版本
                </button>
              </div>
              <div
                v-show="diffView === 'unified'"
                class="diff-block"
                role="group"
                aria-label="版本对比"
              >
                <div
                  v-for="(line, i) in diffLines"
                  :key="i"
                  class="diff-line"
                  :class="`is-${line.kind}`"
                >
                  <span class="diff-no" aria-hidden="true">{{
                    line.oldNo ?? ""
                  }}</span>
                  <span class="diff-no" aria-hidden="true">{{
                    line.newNo ?? ""
                  }}</span>
                  <span class="diff-sign" aria-hidden="true">{{
                    line.sign
                  }}</span>
                  <span class="diff-code">
                    <span
                      v-for="(seg, si) in segments(line)"
                      :key="si"
                      :class="seg.mark ? 'diff-word' : undefined"
                      >{{ seg.text }}</span
                    >
                  </span>
                </div>
              </div>
              <div
                v-show="diffView === 'split'"
                class="diff-split"
                role="group"
                aria-label="版本对比（并排）"
              >
                <div
                  v-for="(row, ri) in diffSplitRows"
                  :key="ri"
                  class="diff-split-row"
                >
                  <span class="diff-no" aria-hidden="true">{{
                    row.left?.oldNo ?? ""
                  }}</span>
                  <span
                    class="diff-cell"
                    :class="row.left ? `is-${row.left.kind}` : 'is-empty'"
                  >
                    <span
                      v-for="(seg, si) in segments(row.left)"
                      :key="si"
                      :class="seg.mark ? 'diff-word' : undefined"
                      >{{ seg.text }}</span
                    >
                  </span>
                  <span class="diff-no" aria-hidden="true">{{
                    row.right?.newNo ?? ""
                  }}</span>
                  <span
                    class="diff-cell"
                    :class="row.right ? `is-${row.right.kind}` : 'is-empty'"
                  >
                    <span
                      v-for="(seg, si) in segments(row.right)"
                      :key="si"
                      :class="seg.mark ? 'diff-word' : undefined"
                      >{{ seg.text }}</span
                    >
                  </span>
                </div>
              </div>
            </div>
          </div>
        </template>

        <!-- 版本预览 -->
        <template v-else-if="preview.open">
          <header class="history-detail-head">
            <div class="history-detail-titles">
              <p class="history-detail-label">版本预览</p>
              <p class="history-detail-title">
                <code :title="preview.hash">{{ previewView.hashShort }}</code>
                <span class="history-detail-name">{{
                  previewView.filename
                }}</span>
              </p>
            </div>
            <div class="history-detail-actions">
              <button
                class="vf-ghost-button"
                type="button"
                @click="downloadVersion(preview.hash)"
              >
                <IconDownload :size="16" />
                <span>下载此版本</span>
              </button>
              <button
                class="vf-ghost-button"
                type="button"
                @click="closePreview"
              >
                关闭
              </button>
            </div>
          </header>

          <div class="history-detail-body">
            <div v-if="preview.loading" class="history-state">
              <div class="spinner mb-3"></div>
              <p class="history-state-text">加载预览中...</p>
            </div>

            <EmptyState
              v-else-if="preview.error"
              :icon="IconAlertCircle"
              tone="error"
              compact
              title="预览失败"
              :hint="preview.error"
            >
              <template #actions>
                <button
                  class="vf-ghost-button is-primary"
                  @click="viewVersion(preview.hash)"
                >
                  重试
                </button>
              </template>
            </EmptyState>

            <template v-else>
              <figure v-if="preview.kind === 'image'" class="image">
                <img
                  :src="preview.objectUrl"
                  :alt="previewView.filename"
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
                <video
                  :src="preview.objectUrl"
                  controls
                  class="preview-video"
                />
              </div>

              <div v-else-if="preview.kind === 'audio'" class="preview-media">
                <audio
                  :src="preview.objectUrl"
                  controls
                  class="preview-audio"
                />
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

              <EmptyState
                v-else
                :icon="IconFileOff"
                compact
                title="暂不支持在线预览"
                hint="该类型无法在浏览器中打开，请下载此版本查看"
              >
                <template #actions>
                  <button
                    class="vf-ghost-button is-primary"
                    @click="downloadVersion(preview.hash)"
                  >
                    <IconDownload :size="16" />
                    <span>下载此版本</span>
                  </button>
                </template>
              </EmptyState>
            </template>
          </div>
        </template>

        <!-- 空态：提示如何查看某个版本 -->
        <EmptyState
          v-else
          :icon="IconHistory"
          compact
          title="查看某个版本"
          hint="在左侧版本上选择「预览」或「对比」，内容会显示在这里"
        />
      </aside>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onBeforeUnmount, computed, watch } from "vue";
import { formatDate } from "../../utils/filePresentation";
import {
  IconAlertCircle,
  IconAlertTriangle,
  IconDownload,
  IconFileOff,
  IconHistory,
  IconInfoCircle,
  IconRefresh,
} from "@tabler/icons-vue";
import EmptyState from "../common/EmptyState.vue";
import SkeletonList from "../common/SkeletonList.vue";
import { filesService } from "../../services/files.service";
import { useAppStore } from "../../stores/app.store";
import { confirmDialog } from "../../composables/dialog";
import type { FileHistory } from "../../types";
import { loadHighlight } from "../../utils/highlight";
import CommitList from "./CommitList.vue";

let cachedMarked: any | null = null;
let cachedHljs: any | null = null;

const props = defineProps<{
  filePath: string;
}>();

const appStore = useAppStore();

const history = ref<FileHistory>({
  commits: [],
  currentVersion: "",
  totalCommits: 0,
});
const loading = ref(false);
const error = ref<string | null>(null);
const DEFAULT_LIMIT = 20;
const nextCursor = ref<string | null>(null);
const restoringHash = ref<string | null>(null);

let historyRequestId = 0;

const diff = ref({
  open: false,
  loading: false,
  error: null as string | null,
  hash: "",
  parent: "" as string | undefined,
  text: "",
});

interface DiffLine {
  kind: "meta" | "hunk" | "add" | "del" | "ctx";
  sign: string;
  text: string;
  oldNo?: number;
  newNo?: number;
  /** 词级强调：差异段 [起, 止) 字符区间（改行对经公共前后缀推导） */
  emph?: [number, number];
}

/** unified diff 解析为结构行（± 色带 + 符号槽 + 旧/新双列行号 + hunk/meta 弱化）。 */
const diffLines = computed<DiffLine[]>(() => {
  const raw = diff.value.text ?? "";
  const lines = raw
    .split("\n")
    .filter(
      (line: string, i: number, arr: string[]) =>
        !(i === arr.length - 1 && line === ""),
    );
  let oldNo = 0;
  let newNo = 0;
  const out = lines.map((line: string): DiffLine => {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
    if (hunk) {
      oldNo = Number(hunk[1]);
      newNo = Number(hunk[2]);
      return { kind: "hunk", sign: "", text: line };
    }
    if (line.startsWith("+++") || line.startsWith("---"))
      return { kind: "meta", sign: "", text: line };
    if (line.startsWith("+"))
      return {
        kind: "add",
        sign: "+",
        text: line.slice(1),
        newNo: newNo++,
      };
    if (line.startsWith("-"))
      return {
        kind: "del",
        sign: "−",
        text: line.slice(1),
        oldNo: oldNo++,
      };
    return {
      kind: "ctx",
      sign: " ",
      text: line.startsWith(" ") ? line.slice(1) : line,
      oldNo: oldNo++,
      newNo: newNo++,
    };
  });

  // 词级强调：相邻 −/＋ 组（典型"改行"）按序配对，标注公共前后缀之外的差异段。
  for (let i = 0; i < out.length; i++) {
    if (out[i].kind !== "del") continue;
    const dels: number[] = [];
    const adds: number[] = [];
    let j = i;
    while (j < out.length && out[j].kind === "del") dels.push(j++);
    while (j < out.length && out[j].kind === "add") adds.push(j++);
    const pairs = Math.min(dels.length, adds.length);
    for (let t = 0; t < pairs; t++) {
      const a = out[dels[t]].text;
      const b = out[adds[t]].text;
      let pre = 0;
      while (pre < a.length && pre < b.length && a[pre] === b[pre]) pre++;
      let suf = 0;
      while (
        suf < a.length - pre &&
        suf < b.length - pre &&
        a[a.length - 1 - suf] === b[b.length - 1 - suf]
      )
        suf++;
      const aEnd = a.length - suf;
      const bEnd = b.length - suf;
      if (aEnd > pre || bEnd > pre) {
        out[dels[t]].emph = [pre, Math.max(aEnd, pre)];
        out[adds[t]].emph = [pre, Math.max(bEnd, pre)];
      }
    }
    i = j - 1;
  }
  return out;
});

const diffView = ref<"unified" | "split">("unified");

/** 词级分段（unified/并排共用）：有强调段时切 3 段并滤空。 */
function segments(line: DiffLine | undefined) {
  if (!line) return [] as { text: string; mark: boolean }[];
  if (line.emph && line.emph[1] > line.emph[0]) {
    return [
      { text: line.text.slice(0, line.emph[0]), mark: false },
      { text: line.text.slice(line.emph[0], line.emph[1]), mark: true },
      { text: line.text.slice(line.emph[1]), mark: false },
    ].filter((seg) => seg.text.length > 0);
  }
  return [{ text: line.text || " ", mark: false }];
}

/** 并排视图行：ctx/hunk/meta 双侧共显，−/＋ 组按序左右配对。 */
const diffSplitRows = computed(() => {
  const out: { left?: DiffLine; right?: DiffLine }[] = [];
  const lines = diffLines.value;
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.kind === "ctx" || line.kind === "meta" || line.kind === "hunk") {
      out.push({ left: line, right: line });
      i++;
      continue;
    }
    const dels: DiffLine[] = [];
    const adds: DiffLine[] = [];
    while (i < lines.length && lines[i].kind === "del") dels.push(lines[i++]);
    while (i < lines.length && lines[i].kind === "add") adds.push(lines[i++]);
    const n = Math.max(dels.length, adds.length);
    for (let t = 0; t < n; t++) out.push({ left: dels[t], right: adds[t] });
  }
  return out;
});

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
  hash: "",
  kind: "text" as PreviewKind,
  text: "",
  html: "",
  objectUrl: "",
  mime: "",
});

const previewFilename = computed(
  () => props.filePath.split("/").pop() || "file",
);
const previewHashShort = computed(() =>
  preview.value.hash ? preview.value.hash.substring(0, 8) : "",
);

const previewView = computed(() => {
  return {
    filename: previewFilename.value,
    hash: preview.value.hash,
    hashShort: previewHashShort.value,
  };
});

const currentShort = computed(() =>
  history.value.currentVersion
    ? history.value.currentVersion.substring(0, 8)
    : "",
);

/** 详情面板当前展示的版本（用于列表高亮）。 */
const selectedHash = computed(() =>
  diff.value.open
    ? diff.value.hash
    : preview.value.open
      ? preview.value.hash
      : "",
);
/** 对比视图键（r65）：U/S 切换 统一/并排（输入态不抢键 ✓ 层级纪律 ✓） */
function onDiffViewKey(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  if (
    t &&
    (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)
  ) {
    return;
  }
  if (!diff.value.open) return;
  if (e.key === "u" || e.key === "U") diffView.value = "unified";
  if (e.key === "s" || e.key === "S") diffView.value = "split";
}
watch(
  () => diff.value.open,
  (open) => {
    if (open) window.addEventListener("keydown", onDiffViewKey);
    else window.removeEventListener("keydown", onDiffViewKey);
  },
);
onBeforeUnmount(() => window.removeEventListener("keydown", onDiffViewKey));

const detailOpen = computed(() => diff.value.open || preview.value.open);

/** 移动端/窄屏：收起详情面板，回到纯列表。 */
function closeDetail() {
  closeDiff();
  closePreview();
}

watch(
  () => props.filePath,
  () => {
    // filePath 变化时重置所有本地状态，避免复用组件导致历史/预览/对比残留
    historyRequestId++;
    nextCursor.value = null;
    history.value = { commits: [], currentVersion: "", totalCommits: 0 };
    loading.value = false;
    error.value = null;
    restoringHash.value = null;
    closePreview();
    closeDiff();
    void loadHistory();
  },
  { immediate: true },
);

async function loadHistory(cursor?: string) {
  const reqId = ++historyRequestId;
  loading.value = true;
  error.value = null;

  try {
    const data = await filesService.getFileHistory(
      props.filePath,
      DEFAULT_LIMIT,
      cursor,
    );
    if (reqId !== historyRequestId) return;
    history.value = cursor
      ? {
          ...data,
          commits: [...history.value.commits, ...data.commits],
        }
      : data;
    nextCursor.value = data.nextCursor ?? null;
  } catch (err) {
    if (reqId !== historyRequestId) return;
    error.value = err instanceof Error ? err.message : "加载失败";
  } finally {
    if (reqId !== historyRequestId) return;
    loading.value = false;
  }
}

function getExtension(p: string): string {
  const name = p.split("/").pop() || "";
  const idx = name.lastIndexOf(".");
  if (idx <= 0 || idx === name.length - 1) return "";
  return name.slice(idx + 1).toLowerCase();
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
  // 允许相对路径、锚点、http(s)、mailto
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
  cachedHljs = await loadHighlight();
  return cachedHljs;
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

function closePreview() {
  if (preview.value.objectUrl) URL.revokeObjectURL(preview.value.objectUrl);
  preview.value = {
    open: false,
    loading: false,
    error: null,
    hash: "",
    kind: "text",
    text: "",
    html: "",
    objectUrl: "",
    mime: "",
  };
}

function closeDiff() {
  diff.value = {
    open: false,
    loading: false,
    error: null,
    hash: "",
    parent: undefined,
    text: "",
  };
}

onBeforeUnmount(() => {
  closePreview();
  closeDiff();
});

async function viewVersion(hash: string) {
  // 打开预览并加载内容；预览与对比共用右侧面板，需先关掉对比
  closePreview();
  closeDiff();
  preview.value.open = true;
  preview.value.loading = true;
  preview.value.hash = hash;
  preview.value.kind = detectPreviewKind(props.filePath);

  try {
    if (preview.value.kind === "unsupported") {
      preview.value.loading = false;
      return;
    }

    const blob = await filesService.getFileContent(props.filePath, hash);

    if (preview.value.kind === "image" || preview.value.kind === "pdf") {
      const typed = new Blob([await blob.arrayBuffer()], {
        type: guessMimeByExt(props.filePath),
      });
      preview.value.objectUrl = URL.createObjectURL(typed);
    } else if (
      preview.value.kind === "video" ||
      preview.value.kind === "audio"
    ) {
      const typed = new Blob([await blob.arrayBuffer()], {
        type: guessMimeByExt(props.filePath),
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

async function viewDiff(hash: string, parent?: string) {
  closePreview();
  closeDiff();

  const kind = detectPreviewKind(props.filePath);
  if (kind !== "text" && kind !== "markdown" && kind !== "code") {
    diff.value.open = true;
    diff.value.error = "仅支持文本文件的对比视图，请使用下载。";
    return;
  }

  diff.value.open = true;
  diff.value.loading = true;
  diff.value.hash = hash;
  diff.value.parent = parent;

  try {
    diff.value.text = await filesService.getFileDiff(
      props.filePath,
      hash,
      parent,
    );
    if (!diff.value.text.trim()) {
      diff.value.text = "(无差异输出)";
    }
  } catch (err) {
    diff.value.error = err instanceof Error ? err.message : "获取 diff 失败";
  } finally {
    diff.value.loading = false;
  }
}

async function restoreVersion(hash: string) {
  if (hash === history.value.currentVersion) return;
  if (restoringHash.value) return;

  const short = hash.substring(0, 8);
  const ok = await confirmDialog({
    title: "恢复历史版本",
    message: `确定要恢复到版本 ${short} 吗？这会生成一个新的提交。`,
    confirmText: "恢复",
  });
  if (!ok) return;

  restoringHash.value = hash;
  try {
    await filesService.restoreFileVersion(props.filePath, hash, "恢复历史版本");
    appStore.success("已恢复并生成新版本");
    await loadHistory();
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "恢复失败");
  } finally {
    restoringHash.value = null;
  }
}

function downloadVersion(hash: string) {
  filesService.downloadFile(props.filePath, hash);
  appStore.success("开始下载历史版本");
}

function loadMore() {
  if (nextCursor.value) void loadHistory(nextCursor.value);
}
</script>

<style scoped>
.version-history {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  min-width: min(920px, 100%);
}

.history-summary-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.history-summary {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.history-summary-strong {
  color: var(--vf-text-strong);
  font-weight: 600;
}

.history-summary-sep {
  margin: 0 0.3rem;
  color: var(--vf-border);
}

.history-summary-hash {
  font-size: 0.8rem;
  color: var(--vf-text-muted);
}

/* 宽屏两栏：左列表右详情；窄屏单栏，详情置顶 */
.history-layout {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 0.85rem;
}

@media screen and (min-width: 900px) {
  .history-layout {
    /* 主流版本浏览器比例：提交列表是导航（≈33%），详情是内容（≈67%）。
       原 1.1fr/1fr（52/48）列表过宽、详情局促。 */
    grid-template-columns: minmax(16rem, 0.5fr) minmax(0, 1fr);
  }
}

.history-list-pane {
  min-width: 0;
  max-height: min(64vh, 560px);
  overflow-y: auto;
  padding-right: 0.2rem;
}

.history-detail-pane {
  /* 长 diff 独立滚动（主流 diff 浏览器行为）：原 visible 会撑高对话框 */
  overflow-y: auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  min-width: 0;
  max-height: min(64vh, 560px);
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface);
}

/* 移动端：打开详情时把详情放到列表上方，避免用户来回滚动 */
@media screen and (max-width: 899px) {
  .history-layout.has-detail .history-detail-pane {
    order: -1;
  }

  .history-list-pane,
  .history-detail-pane {
    max-height: none;
  }
}

.history-detail-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.6rem;
  flex-wrap: wrap;
  padding: 0.6rem 0.7rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.history-detail-titles {
  min-width: 0;
}

.history-detail-label {
  margin: 0;
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.history-detail-title {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin: 0.15rem 0 0;
  color: var(--vf-text-strong);
  font-size: 0.875rem;
  font-weight: 600;
}

.history-detail-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.history-detail-actions {
  display: flex;
  align-items: center;
  gap: 0.3rem;
}

.history-detail-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  padding: 0.7rem;
}

.history-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2rem 1rem;
  text-align: center;
}

/* 保留说明：一行提示，不抢列表焦点 */
.history-retention {
  display: flex;
  align-items: flex-start;
  gap: 0.35rem;
  margin: 0 0 0.6rem;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  line-height: 1.5;
}

/* diff 等次级提示：黄色行内条 */
.history-inline-note {
  display: flex;
  align-items: flex-start;
  gap: 0.35rem;
  margin: 0;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-warning-line);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-warning-soft);
  color: var(--vf-text);
  font-size: 0.8rem;
  line-height: 1.5;
}

.history-inline-note.is-warning {
  border-color: var(--vf-warning-line);
}

.history-state-text {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 0.875rem;
}

.history-more {
  display: flex;
  justify-content: center;
  padding: 0.5rem 0 0.2rem;
}

.diff-toolbar {
  display: flex;
  gap: 4px;
  justify-content: flex-end;
  padding-bottom: 0.4rem;
}

.diff-split {
  display: flex;
  flex-direction: column;
  max-height: 56vh;
  overflow: auto;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.8rem;
}

.diff-split-row {
  display: flex;
  align-items: stretch;
}

.diff-cell {
  flex: 1 1 50%;
  min-width: 0;
  padding: 0.1rem 0.5rem;
  white-space: pre-wrap;
  word-break: break-word;
  border-left: 1px solid var(--vf-border-weak);
}

.diff-cell.is-empty {
  background: var(--vf-surface-sunken);
}

.diff-cell.is-add {
  background: var(--vf-success-soft);
  color: var(--vf-success-text);
}

.diff-cell.is-del {
  background: var(--vf-danger-soft);
  color: var(--vf-danger-text);
}

.diff-cell.is-hunk,
.diff-cell.is-meta {
  color: var(--vf-text-muted);
  font-size: 0.75rem;
}

.diff-block {
  display: flex;
  flex-direction: column;
  max-height: 56vh;
  overflow: auto;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.8rem;
}

.diff-line {
  display: flex;
  gap: 0.5rem;
  padding: 0.1rem 0.5rem;
  white-space: pre-wrap;
  word-break: break-word;
}

.diff-no {
  flex: 0 0 2.2rem;
  text-align: right;
  color: var(--vf-text-subtle);
  user-select: none;
  font-variant-numeric: tabular-nums;
}

.diff-sign {
  flex: 0 0 1.1rem;
  text-align: center;
  user-select: none;
  font-weight: 600;
}

/* ± 色带用 round-7 双主题 AA 配对（soft 底 + text 字） */
.diff-line.is-add {
  background: var(--vf-success-soft);
  color: var(--vf-success-text);
}

.diff-line.is-del {
  background: var(--vf-danger-soft);
  color: var(--vf-danger-text);
}

.diff-word {
  /* GitHub 式行内强调：彩底 + text-strong 深字（行色带已承载 ± 语义）。
     彩底×彩字会双双掉档（实测浅 4.38 / 深 3.18 ✗），换深字后 ≥4.5 ✓。 */
  font-weight: 600;
  border-radius: var(--vf-radius-xs);
  color: var(--vf-text-strong);
  /* 叠 12% 黑膜：深色主题词底偏亮（白字 4.41 差一线），压暗后达标；
     浅色主题 7.8 → ~6.5 仍在 AA 上 ✓ 双向安全。 */
  background-image: linear-gradient(rgba(0, 0, 0, 0.12), rgba(0, 0, 0, 0.12));
}

.is-add .diff-word {
  background: var(--vf-success-line);
}

.is-del .diff-word {
  background: var(--vf-danger-line);
}

.diff-line.is-hunk {
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  font-size: 0.75rem;
}

.diff-line.is-meta {
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.preview-code,
.preview-text {
  margin: 0;
  font-size: 0.8rem;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

.preview-frame {
  position: relative;
  width: 100%;
  height: min(52vh, 460px);
}

.preview-iframe {
  width: 100%;
  height: 100%;
  border: none;
  border-radius: var(--vf-radius-sm);
}

.preview-video {
  width: 100%;
  max-height: min(52vh, 460px);
  border-radius: var(--vf-radius-sm);
}

.preview-audio {
  width: 100%;
}
</style>
