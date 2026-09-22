<template>
  <div
    class="file-card"
    :data-vfiles-path="file.path"
    :class="{
      'file-card--selected': selected,
      'file-card--active': active,
      'file-card--grid': true,
      'drop-target': dragOver,
      'is-dragging': dragging,
    }"
    :style="{ '--file-card-thumb-size': `${thumbnailSize}px` }"
    :title="file.name"
    draggable
    @click="handleClick"
    @dblclick="handleActivate"
    @contextmenu.prevent="handleContextMenu"
    @touchstart="handleLongPressStart"
    @touchmove="handleLongPressCancel"
    @touchend="handleLongPressCancel"
    @touchcancel="handleLongPressCancel"
    @dragstart="handleDragStart"
    @dragend="handleDragEnd"
    @dragover.prevent="handleDragOver"
    @dragleave="handleDragLeave"
    @drop.prevent="handleDrop"
  >
    <div class="file-card-thumb">
      <!-- 缩略图未就绪时先铺骨架，加载完成再淡入，避免大目录滚动时闪白 -->
      <span
        v-if="thumbnailUrl && !thumbLoaded && !thumbFailed"
        class="skeleton-block file-card-thumb-placeholder"
        aria-hidden="true"
      ></span>
      <img
        v-if="thumbnailUrl"
        ref="thumbImageRef"
        class="file-card-thumb-image"
        :class="{ 'is-loaded': thumbLoaded }"
        :src="thumbnailUrl"
        :alt="file.name"
        loading="lazy"
        decoding="async"
        fetchpriority="low"
        @load="thumbLoaded = true"
        @error="thumbFailed = true"
      />
      <span v-else class="icon file-card-thumb-icon">
        <component :is="icon" :size="iconSize" :stroke-width="1.4" />
      </span>

      <label v-if="selectMode" class="file-card-check" @click.stop>
        <input
          type="checkbox"
          :checked="selected"
          :aria-label="`选择 ${file.name}`"
          @change="emit('toggle-select', file)"
        />
      </label>
    </div>

    <div class="file-card-menu" @click.stop>
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
          v-if="file.kind === 'directory'"
          class="file-card-menu-item"
          role="menuitem"
          @click="runAndClose(emitCreateDirectory)"
        >
          <IconFolderPlus :size="16" />
          <span>在此新建子目录</span>
        </button>
        <button
          v-if="file.kind === 'directory'"
          class="file-card-menu-item"
          role="menuitem"
          @click="runAndClose(() => emit('open-folder', file))"
        >
          <IconFolderOpen :size="16" />
          <span>打开</span>
        </button>
        <button
          v-if="file.kind === 'file'"
          class="file-card-menu-item"
          role="menuitem"
          @click="runAndClose(() => emit('preview', file))"
        >
          <IconEye :size="16" />
          <span>预览</span>
        </button>
        <button
          v-if="file.kind === 'file'"
          class="file-card-menu-item"
          role="menuitem"
          @click="runAndClose(() => emit('view-history', file))"
        >
          <IconHistory :size="16" />
          <span>历史版本</span>
        </button>
        <button
          class="file-card-menu-item"
          role="menuitem"
          @click="runAndClose(() => emit('rename', file))"
        >
          <IconPencil :size="16" />
          <span>重命名</span>
        </button>
        <button
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
          class="file-card-menu-item is-danger"
          role="menuitem"
          @click="runAndClose(confirmDelete)"
        >
          <IconTrash :size="16" />
          <span>删除</span>
        </button>
      </div>
    </div>

    <div class="file-card-body">
      <input
        v-if="renaming"
        :ref="registerRenameInput"
        v-model="renameDraft"
        class="input is-small rename-input"
        type="text"
        :aria-label="`重命名 ${file.name}`"
        @click.stop
        @dblclick.stop
        @keydown.enter.prevent.stop="commitRename"
        @keydown.esc.prevent.stop="cancelRename"
        @keydown.stop
        @blur="onRenameBlur"
      />
      <div
        v-else
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

      <!-- 拖放目标提示：右上角标注将移入的目录名（位于 v-if/v-else 名称链之外） -->
      <span v-if="dragOver" class="desktop-drop-hint" aria-hidden="true">
        移动到「{{ file.name }}」
      </span>
      <div
        v-if="showLocation && locationLabel"
        class="file-card-location"
        :title="locationLabel"
      >
        <IconFolder :size="11" />
        <span class="file-card-location-text">{{ locationLabel }}</span>
      </div>
      <div class="file-card-meta" :title="dateTitle">
        <span v-if="file.kind === 'file'">{{ sizeLabel }}</span>
        <span class="file-card-meta-sep" v-if="file.kind === 'file'">·</span>
        <span>{{ dateLabel }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import {
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
  formatRelativeDate,
  formatSize,
  isImageFile,
  splitByNeedle,
} from "../../utils/filePresentation";
import type { FileInfo } from "../../types";

const props = withDefaults(
  defineProps<{
    file: FileInfo;
    /** 是否处于内联重命名状态 */
    renaming?: boolean;
    commit?: string;
    highlight?: string;
    selectMode?: boolean;
    selected?: boolean;
    active?: boolean;
    thumbnailSize?: number;
    /** 搜索结果中显示所在目录 */
    showLocation?: boolean;
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

/** 条目所在目录（搜索结果用）：`/` 表示根目录。 */
const locationLabel = computed(() => {
  const parts = (props.file.path || "").split("/");
  parts.pop();
  return parts.length ? `/${parts.join("/")}` : "/";
});

const emit = defineEmits<{
  click: [file: FileInfo];
  download: [file: FileInfo];
  rename: [file: FileInfo];
  renameCommit: [file: FileInfo, name: string];
  renameCancel: [file: FileInfo];
  move: [file: FileInfo];
  delete: [file: FileInfo];
  "view-history": [file: FileInfo];
  "toggle-select": [file: FileInfo];
  share: [file: FileInfo];
  preview: [file: FileInfo];
  "open-folder": [file: FileInfo];
  "create-directory": [file: FileInfo];
  "modifier-select": [
    payload: { file: FileInfo; shift: boolean; meta: boolean },
  ];
  "context-menu": [payload: { file: FileInfo; x: number; y: number }];
  "drag-start": [file: FileInfo];
  "drag-end": [];
  "drop-on-folder": [targetDir: string];
}>();

/** 内联重命名：与列表行一致，打开时聚焦并选中主文件名。 */
const renameDraft = ref("");
// 提交/取消后输入框会被移除，浏览器会补发一次 blur；用它避免重复提交
let renameSettled = false;
const renameInputRef = ref<HTMLInputElement | null>(null);

function registerRenameInput(element: unknown) {
  renameInputRef.value = element instanceof HTMLInputElement ? element : null;
}

function selectBaseName(input: HTMLInputElement) {
  const dot = props.file.name.lastIndexOf(".");
  const end = dot > 0 ? dot : props.file.name.length;
  input.setSelectionRange(0, end);
}

watch(
  () => props.renaming,
  (renaming) => {
    if (!renaming) return;
    renameSettled = false;
    renameDraft.value = props.file.name;
    void nextTick().then(() => {
      const input = renameInputRef.value;
      if (!input) return;
      input.focus();
      selectBaseName(input);
    });
  },
  { immediate: true },
);

function commitRename() {
  renameSettled = true;
  emit("renameCommit", props.file, renameDraft.value.trim());
}

function cancelRename() {
  renameSettled = true;
  emit("renameCancel", props.file);
}

/** 失焦即提交（名称有变化时），与主流文件管理器一致。 */
function onRenameBlur() {
  if (renameSettled) return;
  const name = renameDraft.value.trim();
  if (name && name !== props.file.name) {
    emit("renameCommit", props.file, name);
    return;
  }
  emit("renameCancel", props.file);
}

const menuOpen = ref(false);
const thumbFailed = ref(false);
const thumbLoaded = ref(false);

const dragOver = ref(false);
/** 拖起态：源卡片半透明（同列表行） */
const dragging = ref(false);

const isDirectoryEntry = computed(() => props.file.kind === "directory");

const iconSize = computed(() => Math.round(props.thumbnailSize * 0.32));

const icon = computed(() => {
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
  const thumbImageRef = ref<HTMLImageElement | null>(null);

  // 命中缓存时 load 事件可能在挂载前就结束，这里补一次检查，避免缩略图一直透明
  watch(
    [thumbImageRef, thumbnailUrl],
    () => {
      const image = thumbImageRef.value;
      if (image?.complete && image.naturalWidth > 0) thumbLoaded.value = true;
    },
    { immediate: true, flush: "post" },
  );
  if (thumbFailed.value) return "";
  if (!isImageFile(props.file)) return "";
  return filesService.thumbnailUrl(props.file.path, {
    commit: props.commit,
    // 请求 2x 尺寸以适配高分屏；服务端会按 blob + size 缓存。
    size: Math.min(512, Math.round(props.thumbnailSize * 2)),
  });
});

const sizeLabel = computed(() => formatSize(props.file.size_bytes));
const dateLabel = computed(() =>
  formatRelativeDate(props.file.updated_at || props.file.created_at),
);
const dateTitle = computed(() =>
  formatDate(props.file.updated_at || props.file.created_at),
);
const nameSegments = computed(() =>
  splitByNeedle(props.file.name ?? "", props.highlight ?? ""),
);

function handleClick(event?: MouseEvent) {
  // 长按已打开菜单，忽略随之而来的 click
  if (longPressFired) {
    longPressFired = false;
    return;
  }

  const shift = event?.shiftKey ?? false;
  const meta = (event?.ctrlKey || event?.metaKey) ?? false;
  if (shift || meta) {
    emit("modifier-select", { file: props.file, shift, meta });
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

function handleContextMenu(event: MouseEvent) {
  emit("context-menu", {
    file: props.file,
    x: event.clientX,
    y: event.clientY,
  });
}

// 移动端长按：以触摸点坐标打开上下文菜单（iOS 不会触发 contextmenu）。
const LONG_PRESS_MS = 500;
let longPressTimer: ReturnType<typeof setTimeout> | null = null;
let longPressFired = false;

function clearLongPress() {
  if (longPressTimer !== null) {
    clearTimeout(longPressTimer);
    longPressTimer = null;
  }
}

function handleLongPressStart(event: TouchEvent) {
  const touch = event.touches[0];
  if (!touch) return;

  longPressFired = false;
  const x = touch.clientX;
  const y = touch.clientY;
  clearLongPress();
  longPressTimer = setTimeout(() => {
    longPressTimer = null;
    longPressFired = true;
    emit("context-menu", { file: props.file, x, y });
  }, LONG_PRESS_MS);
}

function handleLongPressCancel() {
  clearLongPress();
}

onBeforeUnmount(clearLongPress);

/** 拖拽：条目可拖动，目录可作为放置目标。 */
function handleDragStart(event: DragEvent) {
  event.dataTransfer?.setData("text/plain", props.file.path);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  dragging.value = true;
  emit("drag-start", props.file);
}

function handleDragEnd() {
  dragOver.value = false;
  dragging.value = false;
  emit("drag-end");
}

function handleDragOver(event: DragEvent) {
  if (props.file.kind !== "directory") return;
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  dragOver.value = true;
}

function handleDragLeave() {
  dragOver.value = false;
}

function handleDrop() {
  dragOver.value = false;
  if (props.file.kind !== "directory") return;
  emit("drop-on-folder", props.file.path);
}

function handleActivate() {
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
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface);
  cursor: pointer;
  overflow: visible;
  transition:
    box-shadow 0.15s var(--vf-motion-standard),
    border-color 0.15s var(--vf-motion-standard),
    transform 0.15s var(--vf-motion-standard),
    opacity 0.15s var(--vf-motion-standard);
}

.file-card:hover {
  border-color: var(--vf-border);
  box-shadow: var(--vf-shadow-card);
  transform: translateY(-1px);
}

.file-card--active {
  border-color: var(--vf-accent);
  box-shadow: 0 0 0 2px var(--vf-focus-ring);
}

.file-card:active {
  background: var(--vf-accent-soft-strong);
}

/* 拖起态：源卡片半透明（同列表行） */
.file-card.is-dragging {
  opacity: 0.55;
}

.file-card--selected {
  border-color: var(--vf-accent);
  background: var(--vf-accent-soft-strong);
}

.file-card-thumb {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  height: var(--file-card-thumb-size, 144px);
  border-bottom: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius) var(--vf-radius) 0 0;
  background: var(--vf-surface-sunken);
  overflow: hidden;
}

.file-card-thumb-placeholder {
  position: absolute;
  inset: 0;
  border-radius: 0;
  background: var(--vf-skeleton-base);
}

.file-card-thumb-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
  /* 加载完成后淡入（配合骨架占位） */
  opacity: 0;
  transition: opacity 0.18s var(--vf-motion-standard);
  object-position: center;
}

.file-card-thumb-image.is-loaded {
  opacity: 1;
}

/* 关闭动画（系统设置）时直接显示 */
@media (prefers-reduced-motion: reduce) {
  .file-card-thumb-image {
    transition: none;
  }
}

.file-card-thumb-icon {
  color: var(--vf-text-muted);
  width: auto;
  height: auto;
}

.file-card-check {
  position: absolute;
  top: 6px;
  left: 6px;
  z-index: 2;
  display: inline-flex;
  padding: 4px;
  border-radius: 6px;
  /* 半透明令牌须配毛玻璃：勾选块浮在缩略图（含照片）上 */
  background: var(--vf-surface-translucent);
  backdrop-filter: blur(8px);
  box-shadow: var(--vf-shadow-sm);
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
  /* 悬浮于缩略图之上：半透明须配毛玻璃 */
  background: var(--vf-surface-translucent);
  backdrop-filter: blur(8px);
  color: var(--vf-text);
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.12s var(--vf-motion-standard);
  box-shadow: var(--vf-shadow-sm);
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
  border: 1px solid var(--vf-border);
  border-radius: 8px;
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-menu);
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
  color: var(--vf-text);
  font-size: 0.8rem;
  text-align: left;
  cursor: pointer;
}

.file-card-menu-item:hover {
  background: var(--vf-surface-sunken);
}

.file-card-menu-item.is-danger {
  color: var(--vf-danger-text);
}

.file-card-menu-item.is-danger:hover {
  background: var(--vf-danger-soft);
}

.file-card-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px 10px;
  min-width: 0;
}

/* 内联重命名输入框 */
.rename-input {
  width: 100%;
  height: 1.9rem;
  font-size: 0.8rem;
}

/* 搜索结果：卡片上标出所在目录 */
.file-card-location {
  display: flex;
  align-items: center;
  gap: 0.15rem;
  max-width: 100%;
  color: var(--vf-text-subtle);
  font-size: 0.7rem;
}

.file-card-location-text {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-card-name {
  font-size: 0.82rem;
  line-height: 1.3;
  color: var(--vf-text);
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
  color: var(--vf-text-subtle);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
