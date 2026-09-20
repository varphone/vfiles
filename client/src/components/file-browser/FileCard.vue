<template>
  <div
    class="file-card"
    :class="{
      'file-card--selected': selected,
      'file-card--active': active,
      'file-card--shortcut': isNavigationShortcut,
      'file-card--grid': true,
    }"
    :style="{ '--file-card-thumb-size': `${thumbnailSize}px` }"
    :title="file.name"
    @click="handleClick"
    @dblclick="handleActivate"
  >
    <div class="file-card-thumb">
      <img
        v-if="thumbnailUrl"
        class="file-card-thumb-image"
        :src="thumbnailUrl"
        :alt="file.name"
        loading="lazy"
        decoding="async"
        @error="thumbFailed = true"
      />
      <span v-else class="icon file-card-thumb-icon">
        <component :is="icon" :size="iconSize" :stroke-width="1.4" />
      </span>

      <label
        v-if="selectMode && !isNavigationShortcut"
        class="file-card-check"
        @click.stop
      >
        <input
          type="checkbox"
          :checked="selected"
          :aria-label="`选择 ${file.name}`"
          @change="emit('toggle-select', file)"
        />
      </label>

      <div v-if="!isNavigationShortcut" class="file-card-menu" @click.stop>
        <button
          class="file-card-menu-trigger"
          type="button"
          :aria-expanded="menuOpen ? 'true' : 'false'"
          :aria-label="`${file.name} 的操作`"
          @click="toggleMenu"
        >
          <IconDots :size="16" />
        </button>

        <div v-if="menuOpen" class="file-card-menu-panel" role="menu">
          <button
            v-if="isSelfShortcut || file.kind === 'directory'"
            class="file-card-menu-item"
            role="menuitem"
            @click="runAndClose(emitCreateDirectory)"
          >
            <IconFolderPlus :size="16" />
            <span>{{ isSelfShortcut ? "新建子目录" : "在此新建子目录" }}</span>
          </button>
          <button
            v-if="!isSelfShortcut && file.kind === 'directory'"
            class="file-card-menu-item"
            role="menuitem"
            @click="runAndClose(() => emit('open-folder', file))"
          >
            <IconFolderOpen :size="16" />
            <span>打开</span>
          </button>
          <button
            v-if="!isSelfShortcut && file.kind === 'file'"
            class="file-card-menu-item"
            role="menuitem"
            @click="runAndClose(() => emit('preview', file))"
          >
            <IconEye :size="16" />
            <span>预览</span>
          </button>
          <button
            v-if="!isSelfShortcut && file.kind === 'file'"
            class="file-card-menu-item"
            role="menuitem"
            @click="runAndClose(() => emit('view-history', file))"
          >
            <IconHistory :size="16" />
            <span>历史版本</span>
          </button>
          <button
            v-if="!isSelfShortcut"
            class="file-card-menu-item"
            role="menuitem"
            @click="runAndClose(() => emit('rename', file))"
          >
            <IconPencil :size="16" />
            <span>重命名</span>
          </button>
          <button
            v-if="!isSelfShortcut"
            class="file-card-menu-item"
            role="menuitem"
            @click="runAndClose(() => emit('move', file))"
          >
            <IconArrowsDiff :size="16" />
            <span>移动</span>
          </button>
          <button
            class="file-card-menu-item"
            role="menuitem"
            @click="runAndClose(() => emit('download', file))"
          >
            <IconDownload :size="16" />
            <span>下载</span>
          </button>
          <button
            class="file-card-menu-item"
            role="menuitem"
            @click="runAndClose(() => emit('share', file))"
          >
            <IconShare :size="16" />
            <span>分享</span>
          </button>
          <button
            v-if="!isSelfShortcut"
            class="file-card-menu-item is-danger"
            role="menuitem"
            @click="runAndClose(confirmDelete)"
          >
            <IconTrash :size="16" />
            <span>删除</span>
          </button>
        </div>
      </div>
    </div>

    <div class="file-card-body">
      <div
        class="file-card-name"
        :class="{ 'has-text-weight-bold': isDirectoryEntry }"
      >
        <template v-for="(seg, i) in nameSegments" :key="i">
          <mark v-if="seg.match" class="has-background-warning-light">{{
            seg.text
          }}</mark>
          <span v-else>{{ seg.text }}</span>
        </template>
      </div>
      <div class="file-card-meta">
        <span v-if="isParentShortcut">父目录</span>
        <span v-else-if="isSelfShortcut">当前目录</span>
        <template v-else>
          <span v-if="file.kind === 'file'">{{ sizeLabel }}</span>
          <span class="file-card-meta-sep" v-if="file.kind === 'file'">·</span>
          <span>{{ dateLabel }}</span>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import {
  IconArrowLeft,
  IconArrowsDiff,
  IconDots,
  IconDownload,
  IconEye,
  IconFile,
  IconFileCode,
  IconFileText,
  IconFileZip,
  IconFolder,
  IconFolderOpen,
  IconFolderPlus,
  IconHistory,
  IconMusic,
  IconPdf,
  IconPencil,
  IconPhoto,
  IconShare,
  IconTrash,
  IconVideo,
} from "@tabler/icons-vue";
import { confirmDialog } from "../../composables/dialog";
import { filesService } from "../../services/files.service";
import {
  fileIconKind,
  formatDate,
  formatSize,
  isImageFile,
  splitByNeedle,
} from "../../utils/filePresentation";
import type { FileInfo } from "../../types";

const props = withDefaults(
  defineProps<{
    file: FileInfo;
    commit?: string;
    highlight?: string;
    selectMode?: boolean;
    selected?: boolean;
    active?: boolean;
    thumbnailSize?: number;
  }>(),
  {
    commit: undefined,
    highlight: "",
    selectMode: false,
    selected: false,
    active: false,
    thumbnailSize: 144,
  },
);

const emit = defineEmits<{
  click: [file: FileInfo];
  download: [file: FileInfo];
  rename: [file: FileInfo];
  move: [file: FileInfo];
  delete: [file: FileInfo];
  "view-history": [file: FileInfo];
  "toggle-select": [file: FileInfo];
  share: [file: FileInfo];
  preview: [file: FileInfo];
  "open-folder": [file: FileInfo];
  "create-directory": [file: FileInfo];
}>();

const menuOpen = ref(false);
const thumbFailed = ref(false);

const uiRole = computed(
  () => (props.file as FileInfo & { uiRole?: "self" | "parent" }).uiRole,
);
const isSelfShortcut = computed(() => uiRole.value === "self");
const isParentShortcut = computed(() => uiRole.value === "parent");
const isNavigationShortcut = computed(
  () => isSelfShortcut.value || isParentShortcut.value,
);
const isDirectoryEntry = computed(() => props.file.kind === "directory");

const iconSize = computed(() => Math.round(props.thumbnailSize * 0.32));

const icon = computed(() => {
  if (isParentShortcut.value) return IconArrowLeft;
  switch (fileIconKind(props.file)) {
    case "folder":
      return IconFolder;
    case "image":
      return IconPhoto;
    case "video":
      return IconVideo;
    case "audio":
      return IconMusic;
    case "archive":
      return IconFileZip;
    case "code":
      return IconFileCode;
    case "text":
      return IconFileText;
    case "pdf":
      return IconPdf;
    default:
      return IconFile;
  }
});

const thumbnailUrl = computed(() => {
  if (isNavigationShortcut.value || thumbFailed.value) return "";
  if (!isImageFile(props.file)) return "";
  return filesService.thumbnailUrl(props.file.path, {
    commit: props.commit,
    // 请求 2x 尺寸以适配高分屏；服务端会按 blob + size 缓存。
    size: Math.min(512, Math.round(props.thumbnailSize * 2)),
  });
});

const sizeLabel = computed(() => formatSize(props.file.size_bytes));
const dateLabel = computed(() =>
  formatDate(props.file.updated_at || props.file.created_at),
);
const nameSegments = computed(() =>
  splitByNeedle(props.file.name ?? "", props.highlight ?? ""),
);

function handleClick() {
  if (isNavigationShortcut.value) {
    emit("open-folder", props.file);
    return;
  }
  if (props.selectMode) {
    emit("toggle-select", props.file);
    return;
  }
  if (props.file.kind === "directory") {
    emit("open-folder", props.file);
    return;
  }
  emit("click", props.file);
}

function handleActivate() {
  if (isNavigationShortcut.value) {
    emit("open-folder", props.file);
    return;
  }
  if (props.selectMode) return;
  if (props.file.kind === "directory") {
    emit("open-folder", props.file);
    return;
  }
  emit("preview", props.file);
}

function toggleMenu() {
  menuOpen.value = !menuOpen.value;
  if (menuOpen.value) document.addEventListener("click", closeMenu, true);
  else document.removeEventListener("click", closeMenu, true);
}

function closeMenu() {
  menuOpen.value = false;
  document.removeEventListener("click", closeMenu, true);
}

function runAndClose(action: () => void) {
  closeMenu();
  action();
}

function emitCreateDirectory() {
  emit("create-directory", props.file);
}

async function confirmDelete() {
  const ok = await confirmDialog({
    title: "删除确认",
    message: `确定要删除 ${props.file.name} 吗？`,
    confirmText: "删除",
    danger: true,
  });
  if (ok) emit("delete", props.file);
}

onBeforeUnmount(() => {
  document.removeEventListener("click", closeMenu, true);
});
</script>

<style scoped>
.file-card {
  position: relative;
  display: flex;
  flex-direction: column;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  background: #fff;
  cursor: pointer;
  overflow: visible;
  transition:
    box-shadow 0.15s ease,
    border-color 0.15s ease,
    transform 0.15s ease;
}

.file-card:hover {
  border-color: #b5b5b5;
  box-shadow: 0 6px 16px rgba(10, 10, 10, 0.1);
}

.file-card--active {
  border-color: #485fc7;
  box-shadow: 0 0 0 2px rgba(72, 95, 199, 0.2);
}

.file-card--selected {
  border-color: #485fc7;
  background: #f5f7ff;
}

.file-card-thumb {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  height: var(--file-card-thumb-size, 144px);
  border-bottom: 1px solid #f0f0f0;
  border-radius: 10px 10px 0 0;
  background: #fafafa;
  overflow: hidden;
}

.file-card-thumb-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.file-card-thumb-icon {
  color: #7a7a7a;
  width: auto;
  height: auto;
}

.file-card--shortcut .file-card-thumb {
  background: #f3f6ff;
}

.file-card-check {
  position: absolute;
  top: 6px;
  left: 6px;
  z-index: 2;
  display: inline-flex;
  padding: 4px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.9);
  box-shadow: 0 1px 3px rgba(10, 10, 10, 0.15);
}

.file-card-menu {
  position: absolute;
  top: 6px;
  right: 6px;
  z-index: 2;
}

.file-card-menu-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.9);
  color: #4a4a4a;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.12s ease;
  box-shadow: 0 1px 3px rgba(10, 10, 10, 0.15);
}

.file-card:hover .file-card-menu-trigger,
.file-card-menu-trigger:focus-visible {
  opacity: 1;
}

@media (hover: none) {
  .file-card-menu-trigger {
    opacity: 1;
  }
}

.file-card-menu-panel {
  position: absolute;
  top: 30px;
  right: 0;
  min-width: 160px;
  padding: 4px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  background: #fff;
  box-shadow: 0 8px 24px rgba(10, 10, 10, 0.16);
  z-index: 30;
}

.file-card-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: #363636;
  font-size: 0.8rem;
  text-align: left;
  cursor: pointer;
}

.file-card-menu-item:hover {
  background: #f5f5f5;
}

.file-card-menu-item.is-danger {
  color: #cc0f35;
}

.file-card-menu-item.is-danger:hover {
  background: #feecf0;
}

.file-card-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px 10px;
  min-width: 0;
}

.file-card-name {
  font-size: 0.82rem;
  line-height: 1.3;
  color: #363636;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-card-name mark {
  padding: 0 1px;
}

.file-card-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 0.7rem;
  color: #8a8a8a;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
