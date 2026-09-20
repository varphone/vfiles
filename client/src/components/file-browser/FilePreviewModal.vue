<template>
  <Modal
    :show="show"
    :title="`预览: ${filename}`"
    :mobile-compact="true"
    @close="emit('close')"
  >
    <div v-if="preview.loading" class="has-text-centered py-6">
      <div class="spinner mb-3"></div>
      <p class="has-text-grey">加载预览中...</p>
    </div>

    <div v-else-if="preview.error" class="notification is-danger is-light">
      {{ preview.error }}
      <div class="mt-2">
        <button
          class="button is-small is-danger is-light"
          @click="emit('retry', preview.path)"
        >
          <IconRefresh :size="16" class="mr-1" />
          重试
        </button>
      </div>
    </div>

    <div v-else>
      <figure v-if="preview.kind === 'image'" class="image">
        <img :src="preview.objectUrl" :alt="filename" />
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
        class="content markdown-body"
        v-html="preview.html"
      ></div>

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

    <div v-if="total > 1 || canCopy" class="preview-nav">
      <button
        v-if="canCopy"
        class="button is-small is-light preview-copy"
        :title="copyState === 'failed' ? '复制失败' : '复制文件内容'"
        @click="emit('copy')"
      >
        <IconCopy :size="16" />
        <span>{{
          copyState === "done"
            ? "已复制"
            : copyState === "failed"
              ? "复制失败"
              : "复制"
        }}</span>
      </button>

      <template v-if="total > 1">
        <button
          class="button is-small is-light"
          :disabled="!canGoPrev"
          title="上一张（←）"
          @click="emit('prev')"
        >
          <IconChevronLeft :size="16" />
          <span>上一张</span>
        </button>
        <span class="preview-position">
          {{ Math.max(position, 1) }} / {{ total }}
        </span>
        <button
          class="button is-small is-light"
          :disabled="!canGoNext"
          title="下一张（→）"
          @click="emit('next')"
        >
          <span>下一张</span>
          <IconChevronRight :size="16" />
        </button>
      </template>
    </div>
  </Modal>
</template>

<script setup lang="ts">
import {
  IconChevronLeft,
  IconChevronRight,
  IconCopy,
  IconRefresh,
} from "@tabler/icons-vue";
import Modal from "../common/Modal.vue";
import type { PreviewState } from "../../composables/useFilePreview";

/**
 * 预览弹窗。
 *
 * 预览状态、导航与复制逻辑都留在 `useFilePreview` 与父组件，这里只渲染并上抛
 * 交互，便于单独调整预览的呈现（缩略图、代码配色等）。
 */
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
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "retry", path: string): void;
  (e: "prev"): void;
  (e: "next"): void;
  (e: "copy"): void;
}>();
</script>

<style scoped>
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

.preview-text {
  max-height: 60vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
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

.preview-copy {
  margin-right: auto;
}

.preview-nav {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  margin-top: 0.85rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--vf-border-weak);
}

.preview-position {
  min-width: 4.5rem;
  text-align: center;
  font-size: 0.8rem;
  color: var(--vf-text-muted);
}
</style>
