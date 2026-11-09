<template>
  <div ref="rootRef" class="nc-new-menu" :class="{ 'is-open': open }">
    <button
      class="nc-new-button"
      type="button"
      aria-haspopup="menu"
      :aria-expanded="open ? 'true' : 'false'"
      aria-label="新建"
      title="新建文件或文件夹"
      @click.stop="toggle"
    >
      <IconPlus :size="16" aria-hidden="true" />
      <span class="nc-new-label">新建</span>
      <IconChevronDown :size="14" aria-hidden="true" />
    </button>

    <div v-if="open" class="nc-new-dropdown" role="menu" aria-label="新建">
      <button
        class="nc-new-item"
        type="button"
        role="menuitem"
        @click="choose('files')"
      >
        <IconFileUpload :size="16" aria-hidden="true" />
        <span>上传文件</span>
      </button>
      <button
        class="nc-new-item"
        type="button"
        role="menuitem"
        @click="choose('directory')"
      >
        <IconFolderUp :size="16" aria-hidden="true" />
        <span>上传文件夹</span>
      </button>
      <button
        class="nc-new-item"
        type="button"
        role="menuitem"
        @click="choose('folder')"
      >
        <IconFolderPlus :size="16" aria-hidden="true" />
        <span>新建文件夹</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import {
  IconChevronDown,
  IconFileUpload,
  IconFolderPlus,
  IconFolderUp,
  IconPlus,
} from "@tabler/icons-vue";

/**
 * Nextcloud 风格的「+ 新建」分裂入口：一个主按钮 + 下拉菜单。
 *
 * 只负责展示与上报，上传/建档流程仍由 FileBrowser 持有：
 * 前两项复用 openUploader（FileUploader 对话框），第三项交给
 * appStore.requestCreateDirectory() 的既有建目录流程。
 */
const emit = defineEmits<{
  (e: "upload-files"): void;
  (e: "upload-folder"): void;
  (e: "create-folder"): void;
}>();

const open = ref(false);
const rootRef = ref<HTMLElement | null>(null);

function toggle() {
  open.value = !open.value;
}

function choose(kind: "files" | "directory" | "folder") {
  open.value = false;
  if (kind === "folder") {
    emit("create-folder");
    return;
  }
  if (kind === "directory") {
    emit("upload-folder");
    return;
  }
  emit("upload-files");
}

/** 点击菜单外部或按 Esc 收起。 */
function onDocumentPointer(event: MouseEvent | TouchEvent) {
  if (!open.value) return;
  const target = event.target as Node | null;
  if (target && rootRef.value?.contains(target)) return;
  open.value = false;
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && open.value) open.value = false;
}

onMounted(() => {
  document.addEventListener("click", onDocumentPointer, true);
  document.addEventListener("touchstart", onDocumentPointer, true);
  document.addEventListener("keydown", onDocumentKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentPointer, true);
  document.removeEventListener("touchstart", onDocumentPointer, true);
  document.removeEventListener("keydown", onDocumentKeydown);
});
</script>

<style scoped>
.nc-new-menu {
  position: relative;
  flex: 0 0 auto;
}

/* 主按钮：Nextcloud 的实心主色 + 3px 圆角，与工具栏「上传」保持同一视觉层级 */
.nc-new-button {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  min-height: var(--vf-control-height);
  padding: 0 0.7rem;
  border: 1px solid var(--vf-accent);
  border-radius: 3px;
  background: var(--vf-accent);
  color: var(--vf-on-accent);
  font-size: 0.875rem;
  font-weight: 500;
  line-height: 1.25;
  white-space: nowrap;
  cursor: pointer;
  transition: background-color 0.12s var(--vf-motion-standard);
}

.nc-new-button:hover,
.nc-new-button:focus-visible {
  background: var(--vf-accent-strong);
  border-color: var(--vf-accent-strong);
  color: var(--vf-on-accent);
}

.nc-new-button:focus-visible {
  outline: none;
  box-shadow: 0 0 0 3px var(--vf-focus-ring);
}

.nc-new-label {
  font-weight: 500;
}

/* 下拉：白底、3px 圆角、菜单阴影；绝对定位在按钮下方，不挤压面包屑 */
.nc-new-dropdown {
  position: absolute;
  top: calc(100% + 0.3rem);
  left: 0;
  z-index: 30;
  display: flex;
  flex-direction: column;
  min-width: 11rem;
  padding: 0.25rem;
  border: 1px solid var(--vf-border);
  border-radius: 3px;
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-menu);
}

.nc-new-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.45rem 0.6rem;
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--vf-text);
  font-size: 0.875rem;
  text-align: left;
  cursor: pointer;
}

.nc-new-item:hover,
.nc-new-item:focus-visible {
  background: var(--vf-row-hover);
  color: var(--vf-text-strong);
}

.nc-new-item:focus-visible {
  outline: none;
  box-shadow: 0 0 0 2px var(--vf-focus-ring);
}
</style>
