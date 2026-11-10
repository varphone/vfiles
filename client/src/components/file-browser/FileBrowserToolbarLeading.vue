<template>
  <button
    v-if="showSidebarToggle"
    class="vf-icon-button file-toolbar-sidebar-toggle"
    type="button"
    title="切换视图栏"
    :aria-label="sidebarVisible ? '收起视图栏' : '展开视图栏'"
    :aria-expanded="sidebarVisible ? 'true' : 'false'"
    aria-controls="files-view-sidebar"
    @click="emit('toggle-sidebar')"
  >
    <IconMenu2 :size="18" />
  </button>

  <div v-if="showWrite" class="file-toolbar-write">
    <NewMenuButton
      @upload-files="emit('upload-files')"
      @upload-folder="emit('upload-folder')"
      @create-folder="emit('create-folder')"
    />
    <button
      v-if="upload"
      class="vf-ghost-button upload-indicator file-toolbar-upload"
      type="button"
      :title="upload.title"
      @click="emit('open-uploader')"
    >
      <span class="upload-indicator-dot" aria-hidden="true"></span>
      <span>{{ upload.label }}</span>
    </button>
  </div>

  <span v-if="isRestricted" class="scope-badge" :title="scopeTitle">
    <IconLock :size="13" />
    <span>{{ scopeText }}</span>
  </span>
</template>

<script setup lang="ts">
import { IconLock, IconMenu2 } from "@tabler/icons-vue";
import NewMenuButton from "./NewMenuButton.vue";

defineProps<{
  showSidebarToggle: boolean;
  sidebarVisible: boolean;
  showWrite: boolean;
  upload: { label: string; title: string } | null;
  isRestricted: boolean;
  scopeText: string;
  scopeTitle: string;
}>();

const emit = defineEmits<{
  (e: "toggle-sidebar"): void;
  (e: "upload-files"): void;
  (e: "upload-folder"): void;
  (e: "create-folder"): void;
  (e: "open-uploader"): void;
}>();
</script>

<style scoped>
.file-toolbar-sidebar-toggle {
  flex: 0 0 auto;
}

.file-toolbar-write {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  flex: 0 0 auto;
}

.file-toolbar-upload {
  flex: 0 0 auto;
}

/* 受限会话的轻量范围徽标：让用户始终知道当前会话能看/能写哪里 */
.scope-badge {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-badge-gap);
  flex: 0 0 auto;
  max-width: 100%;
  padding: var(--vf-badge-padding-y) var(--vf-badge-padding-x);
  border: 1px solid var(--vf-border);
  border-radius: var(--vf-radius-pill);
  background: var(--vf-surface-2);
  color: var(--vf-text-muted);
  font-size: var(--vf-badge-font-size);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 上传进度胶囊：与「上传」按钮并排，显示队列进度 */
.upload-indicator {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: var(--vf-text-sm);
  color: var(--vf-text-muted);
}

.upload-indicator-dot {
  width: 0.45rem;
  height: 0.45rem;
  border-radius: 50%;
  background: var(--vf-accent);
  animation: upload-indicator-pulse 1.2s var(--vf-motion-standard) infinite;
}

@keyframes upload-indicator-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .upload-indicator-dot {
    animation: none;
  }
}
</style>
