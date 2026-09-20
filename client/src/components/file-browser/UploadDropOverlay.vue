<template>
  <div
    v-if="visible"
    class="upload-drop-overlay"
    role="status"
    aria-live="polite"
  >
    <div class="upload-drop-card">
      <span class="upload-drop-icon">
        <IconCloudUpload :size="40" :stroke-width="1.4" />
      </span>
      <p class="upload-drop-title">松开即可上传</p>
      <p class="upload-drop-hint">
        文件会上传到
        <strong>{{ targetLabel }}</strong>
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { IconCloudUpload } from "@tabler/icons-vue";

/**
 * 整窗拖放提示浮层。
 *
 * 只在拖动外部文件进入窗口时显示，松手后由 `useWindowFileDrop` 触发上传队列。
 */
defineProps<{
  visible: boolean;
  /** 目标目录的展示名称（根目录显示为「根目录」）。 */
  targetLabel: string;
}>();
</script>

<style scoped>
.upload-drop-overlay {
  position: fixed;
  inset: 0;
  z-index: 3000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.5rem;
  background: color-mix(in srgb, var(--vf-canvas) 72%, transparent);
  backdrop-filter: blur(2px);
  pointer-events: none;
}

.upload-drop-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.4rem;
  width: min(28rem, 100%);
  padding: 2.5rem 1.5rem;
  border: 2px dashed var(--vf-accent);
  border-radius: var(--vf-radius-lg);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-md);
  text-align: center;
}

.upload-drop-icon {
  color: var(--vf-accent);
}

.upload-drop-title {
  font-size: 1.05rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.upload-drop-hint {
  font-size: 0.84rem;
  color: var(--vf-text-muted);
}

.upload-drop-hint strong {
  color: var(--vf-text);
}
</style>
