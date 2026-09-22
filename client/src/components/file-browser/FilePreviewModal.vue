<template>
  <Modal :show="show" title="预览" mobile-compact @close="emit('close')">
    <div
      class="preview-shell"
      :class="{ 'is-fullscreen': fullscreen }"
      tabindex="-1"
      @keydown="onKeydown"
    >
      <!-- 顶部工具条：左侧文件信息，右侧操作（与主流网盘的预览器一致） -->
      <div class="preview-toolbar" role="toolbar" aria-label="预览操作">
        <div class="preview-toolbar-info">
          <FileTypeIcon
            v-if="file"
            :file="file"
            :size="18"
            class="preview-toolbar-icon"
          />
          <span class="preview-toolbar-name" :title="filename">
            {{ filename }}
          </span>
          <span v-if="sizeLabel" class="preview-toolbar-meta">
            {{ sizeLabel }}
          </span>
        </div>

        <div class="preview-toolbar-actions">
          <button
            v-if="canCopy"
            class="vf-ghost-button preview-action"
            type="button"
            :title="copyState === 'failed' ? '复制失败' : '复制文件内容'"
            @click="emit('copy')"
          >
            <IconCopy :size="15" />
            <span>{{
              copyState === "done"
                ? "已复制"
                : copyState === "failed"
                  ? "复制失败"
                  : "复制内容"
            }}</span>
          </button>

          <button
            v-if="file"
            class="vf-ghost-button preview-action"
            type="button"
            title="打开所在文件夹"
            @click="emit('reveal')"
          >
            <IconFolderOpen :size="15" />
            <span>所在文件夹</span>
          </button>

          <button
            v-if="file"
            class="vf-ghost-button preview-action"
            type="button"
            title="下载"
            @click="emit('download')"
          >
            <IconDownload :size="15" />
            <span>下载</span>
          </button>

          <button
            v-if="isImage"
            class="vf-icon-button preview-action-icon"
            type="button"
            :title="fullscreen ? '退出全屏' : '全屏'"
            :aria-label="fullscreen ? '退出全屏' : '全屏'"
            @click="toggleFullscreen"
          >
            <IconArrowsMaximize :size="16" />
          </button>
        </div>
      </div>

      <!-- 预览区 -->
      <div class="preview-viewer">
        <SkeletonList
          v-if="preview.loading"
          :variant="isImage ? 'rows' : 'lines'"
          :rows="isImage ? 3 : 10"
          :label="`加载预览 ${filename}`"
        />

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
              @click="emit('retry', preview.path)"
            >
              <IconRefresh :size="16" />
              <span>重试</span>
            </button>
          </template>
        </EmptyState>

        <template v-else>
          <!-- 上一张 / 下一张：悬浮在预览区左右两侧（主流灯箱交互） -->
          <button
            v-if="total > 1"
            class="preview-arrow is-prev"
            type="button"
            :disabled="!canGoPrev"
            title="上一张（←）"
            aria-label="上一张"
            @click="emit('prev')"
          >
            <IconChevronLeft :size="22" />
          </button>
          <button
            v-if="total > 1"
            class="preview-arrow is-next"
            type="button"
            :disabled="!canGoNext"
            title="下一张（→）"
            aria-label="下一张"
            @click="emit('next')"
          >
            <IconChevronRight :size="22" />
          </button>

          <figure v-if="isImage" class="preview-image-stage">
            <img
              :src="preview.objectUrl"
              :alt="filename"
              class="preview-image"
              :class="{ 'is-grabbing': dragging }"
              :style="imageStyle"
              draggable="false"
              @wheel.prevent="onWheel"
              @mousedown="startDrag"
            />
          </figure>

          <div v-else-if="preview.kind === 'pdf'" class="preview-frame">
            <iframe
              :src="preview.objectUrl"
              class="preview-iframe"
              :title="filename"
            ></iframe>
          </div>

          <figure v-else-if="preview.kind === 'video'" class="preview-media">
            <video :src="preview.objectUrl" controls class="preview-video" />
          </figure>

          <div v-else-if="preview.kind === 'audio'" class="preview-media">
            <audio :src="preview.objectUrl" controls class="preview-audio" />
          </div>

          <div
            v-else-if="preview.kind === 'markdown'"
            class="content markdown-body preview-doc"
            v-html="preview.html"
          ></div>

          <div v-else-if="preview.kind === 'code'" class="content preview-doc">
            <pre
              class="preview-code hljs"
            ><code v-html="preview.html"></code></pre>
          </div>

          <div v-else-if="preview.kind === 'text'" class="content preview-doc">
            <pre class="preview-text">{{ preview.text }}</pre>
          </div>

          <EmptyState
            v-else
            :icon="IconFileOff"
            compact
            title="暂不支持在线预览"
            hint="该类型无法在浏览器中打开，请下载后查看"
          >
            <template #actions>
              <button
                class="vf-ghost-button is-primary"
                @click="emit('download')"
              >
                <IconDownload :size="16" />
                <span>下载文件</span>
              </button>
            </template>
          </EmptyState>
        </template>
      </div>

      <!-- 底部状态条：位置与快捷键提示 -->
      <div v-if="!preview.loading && !preview.error" class="preview-status">
        <span v-if="total > 1" class="preview-position">
          {{ Math.max(position, 1) }} / {{ total }}
        </span>
        <span v-else class="preview-position">单张</span>
        <span class="preview-hint">
          <template v-if="total > 1"> <kbd>←</kbd><kbd>→</kbd> 切换 </template>
          <template v-if="isImage">
            <kbd>+</kbd><kbd>-</kbd> 缩放 · <kbd>0</kbd> 适应 ·
            <kbd>R</kbd> 旋转
          </template>
          <kbd>Esc</kbd> 关闭
        </span>
        <span v-if="isImage" class="preview-zoom">{{ zoomPercent }}%</span>
      </div>

      <!-- 图片缩放控制：仅图片类型出现 -->
      <div
        v-if="isImage && !preview.loading && !preview.error"
        class="preview-zoom-bar"
      >
        <button
          class="vf-icon-button"
          type="button"
          title="缩小"
          aria-label="缩小"
          :disabled="zoom <= MIN_ZOOM"
          @click="zoomBy(-1)"
        >
          <IconZoomOut :size="16" />
        </button>
        <button
          class="vf-icon-button"
          type="button"
          title="适应窗口"
          aria-label="适应窗口"
          @click="fitToWindow"
        >
          <IconZoomReset :size="16" />
        </button>
        <button
          class="vf-icon-button"
          type="button"
          title="放大"
          aria-label="放大"
          :disabled="zoom >= MAX_ZOOM"
          @click="zoomBy(1)"
        >
          <IconZoomIn :size="16" />
        </button>
        <button
          class="vf-icon-button"
          type="button"
          title="向左旋转"
          aria-label="向左旋转"
          @click="rotateBy(-90)"
        >
          <IconRotate :size="16" />
        </button>
      </div>
    </div>
  </Modal>
</template>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import {
  IconAlertCircle,
  IconArrowsMaximize,
  IconChevronLeft,
  IconChevronRight,
  IconCopy,
  IconDownload,
  IconFileOff,
  IconFolderOpen,
  IconRefresh,
  IconRotate,
  IconZoomIn,
  IconZoomOut,
  IconZoomReset,
} from "@tabler/icons-vue";
import Modal from "../common/Modal.vue";
import EmptyState from "../common/EmptyState.vue";
import SkeletonList from "../common/SkeletonList.vue";
import FileTypeIcon from "./FileTypeIcon.vue";
import { formatSize } from "../../utils/filePresentation";
import type { PreviewState } from "../../composables/useFilePreview";
import type { FileInfo } from "../../types";

/**
 * 预览弹窗（灯箱）。
 *
 * 预览状态、导航与复制逻辑都留在 `useFilePreview` 与父组件；这里额外负责
 * 图片的缩放/旋转、全屏与快捷键——这些只影响呈现，不必上抛给父组件。
 */
const props = withDefaults(
  defineProps<{
    show: boolean;
    filename: string;
    preview: PreviewState;
    canGoPrev: boolean;
    canGoNext: boolean;
    position: number;
    total: number;
    canCopy: boolean;
    copyState: "idle" | "done" | "failed";
    /** 当前预览的条目（用于图标与下载/定位入口）。 */
    file?: FileInfo | null;
  }>(),
  { file: null },
);

const emit = defineEmits<{
  (e: "close"): void;
  (e: "retry", path: string): void;
  (e: "prev"): void;
  (e: "next"): void;
  (e: "copy"): void;
  (e: "download"): void;
  (e: "reveal"): void;
}>();

const MIN_ZOOM = 25;
const MAX_ZOOM = 400;
const ZOOM_STEP = 25;

const zoom = ref(100);
const rotation = ref(0);
const fullscreen = ref(false);
const dragging = ref(false);
const offset = ref({ x: 0, y: 0 });
const dragOrigin = ref({ x: 0, y: 0 });
const shellRef = ref<HTMLElement | null>(null);

const isImage = computed(() => props.preview.kind === "image");
const zoomPercent = computed(() => Math.round(zoom.value));
const sizeLabel = computed(() =>
  typeof props.file?.size_bytes === "number"
    ? formatSize(props.file.size_bytes)
    : "",
);

const imageStyle = computed(() => ({
  transform: `translate(${offset.value.x}px, ${offset.value.y}px) rotate(${rotation.value}deg) scale(${zoom.value / 100})`,
  cursor: zoom.value > 100 ? (dragging.value ? "grabbing" : "grab") : "auto",
}));

// 换图（上一张/下一张/重新打开）时复位缩放与旋转
watch(
  () => props.preview.path,
  () => {
    zoom.value = 100;
    rotation.value = 0;
    offset.value = { x: 0, y: 0 };
  },
);

function zoomBy(steps: number) {
  const next = zoom.value + steps * ZOOM_STEP;
  zoom.value = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, next));
  if (zoom.value <= 100) offset.value = { x: 0, y: 0 };
}

function fitToWindow() {
  zoom.value = 100;
  rotation.value = 0;
  offset.value = { x: 0, y: 0 };
}

function rotateBy(degrees: number) {
  rotation.value = (rotation.value + degrees + 360) % 360;
}

function onWheel(event: WheelEvent) {
  if (!isImage.value) return;
  zoomBy(event.deltaY < 0 ? 1 : -1);
}

function startDrag(event: MouseEvent) {
  if (zoom.value <= 100) return;
  dragging.value = true;
  dragOrigin.value = {
    x: event.clientX - offset.value.x,
    y: event.clientY - offset.value.y,
  };
  window.addEventListener("mousemove", onDragMove);
  window.addEventListener("mouseup", stopDrag, { once: true });
}

function onDragMove(event: MouseEvent) {
  if (!dragging.value) return;
  offset.value = {
    x: event.clientX - dragOrigin.value.x,
    y: event.clientY - dragOrigin.value.y,
  };
}

function stopDrag() {
  dragging.value = false;
  window.removeEventListener("mousemove", onDragMove);
}

async function toggleFullscreen() {
  const element = shellRef.value;
  if (!element) return;

  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else {
      await element.requestFullscreen();
    }
  } catch {
    // 浏览器拒绝时保持原状（例如 iframe 内或用户手势缺失）
  }
}

function onFullscreenChange() {
  fullscreen.value = Boolean(document.fullscreenElement);
}

/** 图片类快捷键：+/- 缩放、0 适应、R 旋转；其余交给父级/Modal 处理。 */
function onKeydown(event: KeyboardEvent) {
  if (!isImage.value) return;
  const key = event.key.toLowerCase();
  if (key === "+" || key === "=") {
    zoomBy(1);
  } else if (key === "-") {
    zoomBy(-1);
  } else if (key === "0") {
    fitToWindow();
  } else if (key === "r") {
    rotateBy(90);
  } else {
    return;
  }
  event.preventDefault();
}

onMounted(() => {
  document.addEventListener("fullscreenchange", onFullscreenChange);
  void nextTick(() => shellRef.value?.focus());
});

onBeforeUnmount(() => {
  document.removeEventListener("fullscreenchange", onFullscreenChange);
  stopDrag();
});
</script>

<style scoped>
.preview-shell {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  min-width: 0;
  outline: none;
}

.preview-shell.is-fullscreen {
  padding: 0.75rem;
  background: var(--vf-surface);
}

/* 顶部工具条 */
.preview-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  flex-wrap: wrap;
  padding-bottom: 0.55rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.preview-toolbar-info {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  min-width: 0;
}

.preview-toolbar-icon {
  flex: 0 0 auto;
  color: var(--vf-accent-text);
}

.preview-toolbar-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.86rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.preview-toolbar-meta {
  flex: 0 0 auto;
  color: var(--vf-text-subtle);
  font-size: 0.76rem;
}

.preview-toolbar-actions {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  flex-wrap: wrap;
}

.preview-action {
  min-height: 1.9rem;
  padding: 0 0.55rem;
  font-size: 0.79rem;
}

.preview-action-icon {
  flex: 0 0 auto;
}

/* 预览区：相对定位以便放置悬浮箭头 */
.preview-viewer {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 18rem;
  padding: 0 2.4rem;
}

.preview-image-stage {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  max-height: 62vh;
  overflow: hidden;
}

.preview-image {
  max-width: 100%;
  max-height: 62vh;
  transition: transform 0.12s var(--vf-motion-standard);
  user-select: none;
}

.preview-frame {
  width: 100%;
  height: 70vh;
}

.preview-iframe {
  width: 100%;
  height: 100%;
  border: 0;
}

.preview-media {
  width: 100%;
  max-height: 70vh;
}

.preview-video {
  width: 100%;
  max-height: 70vh;
}

.preview-audio {
  width: 100%;
}

.preview-doc {
  width: 100%;
  max-height: 62vh;
  overflow: auto;
}

.preview-text {
  border-radius: var(--vf-radius-sm);
  max-height: 62vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
}

.markdown-body :deep(pre) {
  max-height: 56vh;
  overflow: auto;
}

.preview-code {
  max-height: 62vh;
  overflow: auto;
  white-space: pre;
}

/* 悬浮的上一张/下一张 */
.preview-arrow {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.4rem;
  height: 2.4rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: 50%;
  /* 半透明令牌须配毛玻璃（同 .file-actions 语言）：箭头浮在图片上，
     否则就是"透底"而不是"玻璃"。 */
  background: var(--vf-surface-translucent-strong);
  backdrop-filter: blur(8px);
  color: var(--vf-text);
  cursor: pointer;
  z-index: 2;
}

.preview-arrow:hover:not(:disabled) {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.preview-arrow:disabled {
  opacity: 0.35;
  cursor: default;
}

.preview-arrow.is-prev {
  left: 0;
}

.preview-arrow.is-next {
  right: 0;
}

/* 底部状态条 + 缩放控制 */
.preview-status {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  flex-wrap: wrap;
  padding-top: 0.55rem;
  border-top: 1px solid var(--vf-border-weak);
  color: var(--vf-text-muted);
  font-size: 0.76rem;
}

.preview-position {
  min-width: 3rem;
  color: var(--vf-text-strong);
  font-weight: 600;
}

.preview-hint {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  flex-wrap: wrap;
  color: var(--vf-text-subtle);
}

.preview-hint kbd {
  padding: 0 0.25rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-xs);
  background: var(--vf-surface-sunken);
  font-family: inherit;
  font-size: 0.7rem;
}

.preview-zoom {
  min-width: 3rem;
  text-align: right;
  color: var(--vf-text-muted);
}

.preview-zoom-bar {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.3rem;
}

.spinner {
  width: 40px;
  height: 40px;
  border: 3px solid var(--vf-skeleton-base);
  border-top-color: var(--vf-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin: 0 auto;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
