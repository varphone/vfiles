<template>
  <!-- Desktop: <tr> root so table layout enforces column alignment natively -->
  <tr
    v-if="desktop"
    class="desktop-file-row"
    :data-vfiles-path="file.path"
    :class="{ 'drop-target': dragOver, 'is-row-selected': selected }"
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
    <td class="is-narrow">
      <div class="is-flex is-align-items-center">
        <!-- 复选框：批量模式下常驻，其余情况 hover/选中时出现，方便像网盘一样直接勾选 -->
        <label
          v-if="selectMode || selected"
          class="mr-2"
          @click.stop
          @dblclick.stop
        >
          <input
            type="checkbox"
            :checked="selected"
            @change="toggleSelected"
            aria-label="选择"
          />
        </label>
        <label v-else class="mr-2 desktop-row-check" @click.stop @dblclick.stop>
          <input
            type="checkbox"
            :checked="selected"
            @change="toggleSelected"
            :aria-label="`选择 ${file.name}`"
            tabindex="-1"
          />
        </label>

        <span class="icon mr-2">
          <FileTypeIcon :file="file" :size="18" :stroke-width="1.7" />
        </span>
      </div>
    </td>

    <td class="desktop-name-cell">
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
      <a
        v-else-if="isNameLink"
        href="#"
        class="desktop-name-text desktop-name-link"
        :class="nameTextClass"
        :title="file.name"
        @click.stop.prevent="activateNameLink"
      >
        <template v-for="(seg, i) in nameSegments" :key="i">
          <mark v-if="seg.match" class="has-background-warning-light">{{
            seg.text
          }}</mark>
          <span v-else>{{ seg.text }}</span>
        </template>
      </a>
      <span
        v-else-if="!renaming"
        class="desktop-name-text"
        :class="nameTextClass"
        :title="file.name"
      >
        <template v-for="(seg, i) in nameSegments" :key="i">
          <mark v-if="seg.match" class="has-background-warning-light">{{
            seg.text
          }}</mark>
          <span v-else>{{ seg.text }}</span>
        </template>
      </span>

      <span
        v-if="showLocation && locationLabel"
        class="desktop-name-location"
        :title="locationLabel"
      >
        <IconFolder :size="12" />
        <span class="desktop-name-location-text">{{ locationLabel }}</span>
      </span>

      <!-- 行内操作入口：hover / 选中 / 触屏常驻，菜单与右键菜单一致 -->
      <button
        ref="rowMenuButton"
        class="vf-icon-button desktop-row-menu"
        type="button"
        :aria-label="`${file.name} 的操作`"
        :aria-haspopup="true"
        :aria-expanded="menuOpen ? 'true' : 'false'"
        title="更多操作"
        @click.stop.prevent="openRowMenu"
        @dblclick.stop
      >
        <IconDots :size="16" />
      </button>
    </td>

    <td class="is-narrow desktop-file-date" :title="desktopFileDateTitle">
      {{ desktopFileDateLabel }}
    </td>

    <td class="is-narrow">{{ desktopFileKindLabel }}</td>

    <td class="is-narrow has-text-right">{{ desktopFileSizeLabel }}</td>

    <td
      v-if="showActionColumn !== false"
      class="is-narrow has-text-right"
      @click.stop
    >
      <div
        class="buttons has-addons are-small is-right mb-0 desktop-action-buttons"
      >
        <template v-if="file.kind === 'directory'">
          <button
            class="button is-ghost"
            @click="createDirectory"
            title="在此目录下新建子目录"
            aria-label="在此目录下新建子目录"
          >
            <span class="icon is-small"><IconFolderPlus :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="renameEntry"
            title="重命名目录"
            aria-label="重命名目录"
          >
            <span class="icon is-small"><IconPencil :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="moveEntry"
            title="移动目录"
            aria-label="移动目录"
          >
            <span class="icon is-small"><IconArrowsDiff :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="download"
            title="下载目录"
            aria-label="下载目录"
          >
            <span class="icon is-small"><IconDownload :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="share"
            title="分享目录"
            aria-label="分享目录"
          >
            <span class="icon is-small"><IconShare :size="16" /></span>
          </button>
          <button
            class="button is-ghost is-danger"
            @click="confirmDelete"
            title="删除目录"
            aria-label="删除目录"
          >
            <span class="icon is-small"><IconTrash :size="16" /></span>
          </button>
        </template>
        <template v-else>
          <button
            class="button is-ghost"
            @click="preview"
            title="预览文件"
            aria-label="预览文件"
          >
            <span class="icon is-small"><IconEye :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="viewHistory"
            title="查看历史"
            aria-label="查看历史"
          >
            <span class="icon is-small"><IconHistory :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="renameEntry"
            title="重命名文件"
            aria-label="重命名文件"
          >
            <span class="icon is-small"><IconPencil :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="moveEntry"
            title="移动文件"
            aria-label="移动文件"
          >
            <span class="icon is-small"><IconArrowsDiff :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="download"
            title="下载文件"
            aria-label="下载文件"
          >
            <span class="icon is-small"><IconDownload :size="16" /></span>
          </button>
          <button
            class="button is-ghost"
            @click="share"
            title="分享文件"
            aria-label="分享文件"
          >
            <span class="icon is-small"><IconShare :size="16" /></span>
          </button>
          <button
            class="button is-ghost is-danger"
            @click="confirmDelete"
            title="删除文件"
            aria-label="删除文件"
          >
            <span class="icon is-small"><IconTrash :size="16" /></span>
          </button>
        </template>
      </div>
    </td>
  </tr>

  <!-- Mobile: <div> card, unchanged -->
  <div
    v-else
    class="file-item box"
    :class="{
      'has-background-light': selected,
      'is-expanded': showActions,
      'drop-target': dragOver,
    }"
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
    <div class="media">
      <div class="media-left">
        <figure class="image is-48x48">
          <div class="file-icon">
            <FileTypeIcon :file="file" :size="32" :stroke-width="1.5" />
          </div>
        </figure>
      </div>
      <div class="media-content">
        <div class="content">
          <input
            v-if="renaming"
            :ref="registerRenameInput"
            v-model="renameDraft"
            class="input is-small rename-input"
            type="text"
            :aria-label="`重命名 ${file.name}`"
            @click.stop
            @touchstart.stop
            @keydown.enter.prevent.stop="commitRename"
            @keydown.esc.prevent.stop="cancelRename"
            @keydown.stop
            @blur="onRenameBlur"
          />
          <p v-else class="file-name" :class="nameTextClass" :title="file.name">
            <template v-for="(seg, i) in nameSegments" :key="i"
              ><mark v-if="seg.match" class="has-background-warning-light">{{
                seg.text
              }}</mark
              ><span v-else>{{ seg.text }}</span></template
            >
          </p>
          <p class="file-info">
            <span v-if="file.kind === 'file'" class="file-info-size">
              {{ formatSize(file.size_bytes || 0) }}
            </span>
            <span
              v-if="file.kind === 'file'"
              class="file-info-sep"
              aria-hidden="true"
              >·</span
            >
            <span class="file-info-date" :title="formatDate(file.created_at)">
              {{ formatRelativeDate(file.created_at) }}
            </span>
          </p>
          <p v-if="file.lastCommit" class="file-commit">
            <span class="file-commit-message">
              {{ file.lastCommit.message }}
            </span>
          </p>

          <div
            v-if="file.kind === 'file' && file.matches && file.matches.length"
            class="search-matches"
          >
            <p
              v-for="m in file.matches"
              :key="m.line"
              class="is-size-7 has-text-grey"
            >
              <span class="has-text-grey-light mr-2">{{ m.line }}:</span>
              <template v-for="(seg, i) in splitHighlight(m.text)" :key="i">
                <mark v-if="seg.match" class="has-background-warning-light">{{
                  seg.text
                }}</mark>
                <span v-else>{{ seg.text }}</span>
              </template>
            </p>
          </div>
        </div>
      </div>
      <div v-if="selectMode" class="media-right">
        <div class="is-flex is-align-items-center">
          <input
            type="checkbox"
            :checked="selected"
            @click.stop
            @change="toggleSelected"
            aria-label="选择"
          />
        </div>
      </div>
    </div>

    <!-- 浮动操作栏 -->
    <Transition name="slide-up">
      <div v-if="showActions && !selectMode" class="file-actions" @click.stop>
        <div class="actions-bar">
          <button
            v-if="file.kind === 'directory'"
            class="action-btn"
            @click="openFolder"
            title="打开"
          >
            <IconFolderOpen :size="20" />
            <span>打开</span>
          </button>
          <button
            v-if="file.kind === 'file'"
            class="action-btn"
            @click="preview"
            title="预览"
          >
            <IconEye :size="20" />
            <span>预览</span>
          </button>
          <button
            v-if="file.kind === 'file'"
            class="action-btn"
            @click="viewHistory"
            title="历史"
          >
            <IconHistory :size="20" />
            <span>历史</span>
          </button>
          <button class="action-btn" @click="moveEntry" title="移动">
            <IconArrowsDiff :size="20" />
            <span>移动</span>
          </button>
          <button class="action-btn" @click="download" title="下载">
            <IconDownload :size="20" />
            <span>下载</span>
          </button>
          <button class="action-btn" @click="share" title="分享">
            <IconShare :size="20" />
            <span>分享</span>
          </button>
          <button
            class="action-btn is-danger"
            @click="confirmDelete"
            title="删除"
          >
            <IconTrash :size="20" />
            <span>删除</span>
          </button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { confirmDialog } from "../../composables/dialog";
import {
  IconArrowsDiff,
  IconFolder,
  IconFolderPlus,
  IconFolderOpen,
  IconHistory,
  IconPencil,
  IconDownload,
  IconTrash,
  IconShare,
  IconEye,
  IconDots,
} from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import FileTypeIcon from "./FileTypeIcon.vue";
import {
  fileKindLabel,
  formatDate,
  formatRelativeDate,
  formatSize,
} from "../../utils/filePresentation";

const props = defineProps<{
  file: FileInfo;
  highlight?: string;
  selectMode?: boolean;
  selected?: boolean;
  expanded?: boolean;
  desktop?: boolean;
  /** 是否处于内联重命名状态 */
  renaming?: boolean;
  /** 是否显示「操作」列（详情面板可见时由父组件关闭） */
  showActionColumn?: boolean;
  /** 搜索结果中显示所在目录 */
  showLocation?: boolean;
}>();

/** 条目所在目录（搜索结果用）：`/` 表示根目录。 */
const locationLabel = computed(() => {
  const parts = (props.file.path || "").split("/");
  parts.pop();
  return parts.length ? `/${parts.join("/")}` : "/";
});

const emit = defineEmits<{
  click: [file: FileInfo];
  download: [file: FileInfo];
  delete: [file: FileInfo];
  rename: [file: FileInfo];
  renameCommit: [file: FileInfo, name: string];
  renameCancel: [file: FileInfo];
  move: [file: FileInfo];
  viewHistory: [file: FileInfo];
  toggleSelect: [file: FileInfo];
  share: [file: FileInfo];
  preview: [file: FileInfo];
  openFolder: [file: FileInfo];
  createDirectory: [file: FileInfo];
  collapse: [];
  modifierSelect: [payload: { file: FileInfo; shift: boolean; meta: boolean }];
  contextMenu: [payload: { file: FileInfo; x: number; y: number }];
  dragStart: [file: FileInfo];
  dragEnd: [];
  dropOnFolder: [targetDir: string];
}>();

const dragOver = ref(false);
const rowMenuButton = ref<HTMLButtonElement | null>(null);
/** 本行的菜单是否已打开（仅用于 aria-expanded）。 */
const menuOpen = ref(false);

const showActions = computed(() => props.expanded);
const isDirectoryEntry = computed(() => props.file.kind === "directory");
/** 内联重命名：父组件通过 `renaming` 打开，输入框自动聚焦并选中主文件名。 */
const renameDraft = ref("");
// 提交/取消后输入框会被移除，浏览器会补发一次 blur；用它避免重复提交
let renameSettled = false;
const renameInputRef = ref<HTMLInputElement | null>(null);

function registerRenameInput(element: unknown) {
  renameInputRef.value = element instanceof HTMLInputElement ? element : null;
}

/** 选中主文件名（保留扩展名），与资源管理器一致。 */
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

/**
 * 打开本行的操作菜单。
 *
 * 坐标取按钮左下角：菜单会出现在「⋯」正下方，和 Google Drive / OneDrive 的
 * 行内菜单位置一致；父组件复用同一个 ContextMenu 组件渲染。
 */
function openRowMenu() {
  const rect = rowMenuButton.value?.getBoundingClientRect();
  const x = rect ? rect.left : 0;
  const y = rect ? rect.bottom + 4 : 0;
  menuOpen.value = true;
  emit("contextMenu", { file: props.file, x, y });
  // 菜单是受控组件，父组件关闭后这里同步复位
  window.setTimeout(() => {
    menuOpen.value = false;
  }, 300);
}

function commitRename() {
  renameSettled = true;
  emit("renameCommit", props.file, renameDraft.value.trim());
}

function cancelRename() {
  renameSettled = true;
  emit("renameCancel", props.file);
}

/** 失焦即提交（名称变化时），与主流文件管理器一致。 */
function onRenameBlur() {
  if (renameSettled) return;
  const name = renameDraft.value.trim();
  if (name && name !== props.file.name) {
    emit("renameCommit", props.file, name);
    return;
  }
  emit("renameCancel", props.file);
}

const isNameLink = computed(() => isDirectoryEntry.value);
const nameTextClass = computed(() => ({
  // 文件夹名称略重，颜色仍走正文色，避免整列都是链接蓝
  "is-folder-name": isDirectoryEntry.value,
}));

const desktopFileDateLabel = computed(() => {
  return formatRelativeDate(props.file.updated_at || props.file.created_at);
});

/** 悬停显示精确时间，作为相对时间的补充 */
const desktopFileDateTitle = computed(() => {
  return formatDate(props.file.updated_at || props.file.created_at);
});

const desktopFileSizeLabel = computed(() => {
  if (props.file.kind === "directory") return "--";
  return formatSize(props.file.size_bytes || 0);
});

const desktopFileKindLabel = computed(() => {
  // 统一走共享实现，避免各处对「代码 / 视频」的判定不一致
  return fileKindLabel(props.file);
});

type NameSegment = { text: string; match: boolean };

function splitByNeedle(text: string, needleRaw: string): NameSegment[] {
  const hay = text ?? "";
  const needle = (needleRaw ?? "").trim().toLowerCase();
  if (!needle) return [{ text: hay, match: false }];

  const hayLower = hay.toLowerCase();
  const segments: NameSegment[] = [];
  let start = 0;

  while (start < hay.length) {
    const idx = hayLower.indexOf(needle, start);
    if (idx === -1) {
      segments.push({ text: hay.slice(start), match: false });
      break;
    }

    if (idx > start) {
      segments.push({ text: hay.slice(start, idx), match: false });
    }
    segments.push({ text: hay.slice(idx, idx + needle.length), match: true });
    start = idx + needle.length;
  }

  return segments.length ? segments : [{ text: hay, match: false }];
}

const nameSegments = computed<NameSegment[]>(() => {
  return splitByNeedle(props.file.name ?? "", props.highlight ?? "");
});

function splitHighlight(text: string): NameSegment[] {
  return splitByNeedle(text, props.highlight ?? "");
}

function handleClick(event?: MouseEvent) {
  // 长按已打开菜单，忽略随之而来的 click
  if (longPressFired) {
    longPressFired = false;
    return;
  }

  const shift = event?.shiftKey ?? false;
  const meta = (event?.ctrlKey || event?.metaKey) ?? false;
  // Shift/Ctrl(Cmd) 点击进入范围/加选，交由父组件统一维护选择状态
  if (shift || meta) {
    emit("modifierSelect", { file: props.file, shift, meta });
    return;
  }

  if (props.selectMode) {
    emit("toggleSelect", props.file);
    return;
  }
  emit("click", props.file);
}

function handleContextMenu(event: MouseEvent) {
  emit("contextMenu", { file: props.file, x: event.clientX, y: event.clientY });
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
  if (props.desktop) return;
  const touch = event.touches[0];
  if (!touch) return;

  longPressFired = false;
  const x = touch.clientX;
  const y = touch.clientY;
  clearLongPress();
  longPressTimer = setTimeout(() => {
    longPressTimer = null;
    longPressFired = true;
    emit("contextMenu", { file: props.file, x, y });
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
  emit("dragStart", props.file);
}

function handleDragEnd() {
  dragOver.value = false;
  emit("dragEnd");
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
  emit("dropOnFolder", props.file.path);
}

function handleActivate() {
  if (props.selectMode) return;
  if (props.file.kind === "directory") {
    emit("openFolder", props.file);
    return;
  }
  emit("preview", props.file);
}

function activateNameLink() {
  if (!isNameLink.value) return;
  emit("openFolder", props.file);
}

function toggleSelected() {
  emit("toggleSelect", props.file);
}

function openFolder() {
  emit("openFolder", props.file);
}

function createDirectory() {
  emit("createDirectory", props.file);
}

function preview() {
  emit("preview", props.file);
}

function download() {
  emit("download", props.file);
}

async function confirmDelete() {
  const ok = await confirmDialog({
    title: "删除确认",
    message: `确定要删除 ${props.file.name} 吗？`,
    confirmText: "删除",
    danger: true,
  });
  if (ok) {
    emit("delete", props.file);
  }
}

function renameEntry() {
  emit("rename", props.file);
}

function moveEntry() {
  emit("move", props.file);
}

function viewHistory() {
  emit("viewHistory", props.file);
}

function share() {
  emit("share", props.file);
}
</script>

<style scoped>
/* 移动端列表：扁平行 + 细分隔线（主流网盘移动端不用卡片阴影） */
.file-item {
  cursor: pointer;
  transition: background-color 0.15s ease;
  margin-bottom: 0 !important;
  border-radius: 0 !important;
  box-shadow: none !important;
  border-bottom: 1px solid var(--vf-border-weak);
  content-visibility: auto;
  contain-intrinsic-size: 96px;
  padding: 0.75rem 0.9rem;
  position: relative;
  overflow: hidden;
}

.file-item:hover,
.file-item:active {
  background: var(--vf-surface-hover);
}

/* 文件名用正文色（不再整列蓝色链接），hover 时才提示可点击 */
.is-folder-name {
  font-weight: 600;
}

/* 内联重命名输入框：占满名称列，字号与正文一致 */
.rename-input {
  width: 100%;
  max-width: 22rem;
  height: 1.9rem;
  font-size: 0.84rem;
}

.desktop-name-text {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.875rem;
  color: var(--vf-text);
}

.desktop-name-link {
  cursor: pointer;
  text-decoration: none;
}

.desktop-name-link:hover,
.desktop-name-link:focus-visible {
  color: var(--vf-accent-text);
  text-decoration: underline;
  text-underline-offset: 0.15em;
}

/* 行高与 hover/选中态对齐主流云盘 */
.desktop-file-row {
  height: 3rem;
}

.desktop-file-row > td {
  vertical-align: middle;
}

/* 次要信息用低对比度颜色，让名称成为视觉焦点 */
.desktop-file-row > td.is-narrow,
.desktop-file-date {
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

/* M3 状态层：悬停 8%（accent-soft）、选中 16%（accent-soft-strong），
   两态相差一倍，肉眼可分；悬停/选中的行面用 8px 圆角（Drive 列表项语言）。 */
.desktop-file-row:hover > td {
  background: var(--vf-accent-soft);
}

.desktop-file-row.is-row-selected > td {
  background: var(--vf-accent-soft-strong);
}

.desktop-file-row:hover > td:first-child {
  border-top-left-radius: var(--vf-radius-sm);
  border-bottom-left-radius: var(--vf-radius-sm);
}

.desktop-file-row:hover > td:last-child {
  border-top-right-radius: var(--vf-radius-sm);
  border-bottom-right-radius: var(--vf-radius-sm);
}

.desktop-file-row.is-row-selected > td:first-child {
  border-top-left-radius: var(--vf-radius-sm);
  border-bottom-left-radius: var(--vf-radius-sm);
}

.desktop-file-row.is-row-selected > td:last-child {
  border-top-right-radius: var(--vf-radius-sm);
  border-bottom-right-radius: var(--vf-radius-sm);
}

/* 名称单元格：为尾部「⋯」预留空间，并作为其定位参考 */
.desktop-name-cell {
  position: relative;
  padding-right: 2.1rem !important;
}

/* 搜索结果：名称下方显示所在目录，帮助区分同名文件 */
.desktop-name-location {
  display: flex;
  align-items: center;
  gap: 0.2rem;
  max-width: 100%;
  margin-top: 0.1rem;
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
  line-height: 1.3;
}

.desktop-name-location-text {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 行内操作入口「⋯」：默认隐藏，hover / 聚焦 / 选中 / 触屏时出现。
   即使隐藏了「操作」列，用户也能在行内直接打开下载/重命名/移动等菜单。 */
.desktop-row-menu {
  position: absolute;
  top: 50%;
  right: 0.35rem;
  transform: translateY(-50%);
  width: 1.75rem;
  height: 1.75rem;
  opacity: 0;
  transition: opacity 0.12s ease;
}

.desktop-file-row:hover .desktop-row-menu,
.desktop-file-row:focus-within .desktop-row-menu,
.desktop-file-row.is-row-selected .desktop-row-menu {
  opacity: 1;
}

/* hover 才出现的复选框：批量模式下由 FileItem 直接渲染常驻版本 */
.desktop-row-check {
  opacity: 0;
  transition: opacity 0.12s ease;
}

.desktop-file-row:hover .desktop-row-check,
.desktop-file-row:focus-within .desktop-row-check,
.desktop-file-row.is-row-selected .desktop-row-check {
  opacity: 1;
}

@media (hover: none) {
  .desktop-row-menu,
  .desktop-row-check {
    opacity: 1;
  }
}

/* 行内操作默认隐藏，hover / 键盘聚焦时出现，减少视觉噪音 */
.desktop-action-buttons {
  flex-wrap: nowrap;
  justify-content: flex-end;
  opacity: 0;
  transition: opacity 0.12s ease;
}

.desktop-file-row:hover .desktop-action-buttons,
.desktop-file-row:focus-within .desktop-action-buttons,
.desktop-file-row.is-row-selected .desktop-action-buttons {
  opacity: 1;
}

@media (hover: none) {
  .desktop-action-buttons {
    opacity: 1;
  }
}

.file-item.is-expanded {
  padding-bottom: 3.5rem;
}

.file-item :deep(.media-left) {
  margin-right: 0.5rem;
}

.file-item :deep(.media-right) {
  margin-left: 0.5rem;
}

.file-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: 8px;
  background: transparent;
  color: inherit;
}

.file-name {
  padding-left: 0 !important;
  padding-right: 0 !important;
  margin-left: 0 !important;
  margin-right: 0 !important;
  margin-bottom: 0 !important;
  word-break: break-word;
}

/* 移动端行内元信息：弱化文本而不是胶囊标签（与桌面列表一致） */
.file-info-size,
.file-info-date,
.file-info-sep {
  color: var(--vf-text-subtle);
  font-size: 0.76rem;
}

.file-info-sep {
  margin: 0 0.25rem;
}

.file-commit-message {
  display: inline-block;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--vf-text-muted);
  font-size: 0.74rem;
}

.file-info {
  padding-left: 0 !important;
  padding-right: 0 !important;
  margin-left: 0 !important;
  margin-right: 0 !important;
  margin-bottom: 0.25rem !important;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
}

.file-commit {
  padding-left: 0 !important;
  padding-right: 0 !important;
  margin-left: 0 !important;
  margin-right: 0 !important;
  margin-top: 0.25rem;
}

.file-commit .tag {
  font-size: 0.75rem;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.search-matches {
  margin-top: 0.5rem;
}

/* 浮动操作栏 */
.file-actions {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: var(--vf-surface-translucent-strong);
  backdrop-filter: blur(8px);
  border-top: 1px solid var(--vf-border-weak);
  padding: 0.5rem 0.75rem;
}

.actions-bar {
  display: flex;
  justify-content: space-around;
  align-items: center;
  gap: 0.25rem;
}

.action-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.125rem;
  padding: 0.25rem 0.5rem;
  border: none;
  background: transparent;
  color: var(--vf-text);
  cursor: pointer;
  border-radius: 6px;
  transition: all 0.15s;
  font-size: 0.7rem;
  min-width: 3rem;
}

.action-btn:hover {
  background: var(--vf-surface-hover);
  color: var(--vf-accent);
}

.action-btn:active {
  transform: scale(0.95);
}

.action-btn.is-danger:hover {
  background: var(--vf-danger-soft);
  color: var(--vf-danger);
}

.action-btn span {
  line-height: 1;
}

/* 动画 */
.slide-up-enter-active,
.slide-up-leave-active {
  transition: all 0.2s ease;
}

.slide-up-enter-from,
.slide-up-leave-to {
  opacity: 0;
  transform: translateY(100%);
}

@media screen and (max-width: 1023px) {
  .file-item {
    padding-left: 0.5rem;
    padding-right: 0.5rem;
  }

  .file-item :deep(.media-left) {
    margin-right: 0.35rem;
  }

  .file-item :deep(.media-right) {
    margin-left: 0.35rem;
  }

  .media-left .image {
    width: 40px;
    height: 40px;
  }

  .file-icon {
    width: 40px;
    height: 40px;
  }

  .action-btn {
    min-width: 2.5rem;
    padding: 0.25rem 0.25rem;
  }
}
</style>
