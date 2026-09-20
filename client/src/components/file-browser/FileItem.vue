<template>
  <!-- Desktop: <tr> root so table layout enforces column alignment natively -->
  <tr
    v-if="desktop"
    class="desktop-file-row"
    @click="handleClick"
    @dblclick="handleActivate"
    @contextmenu.prevent="handleContextMenu"
  >
    <td class="is-narrow">
      <div class="is-flex is-align-items-center">
        <label v-if="selectMode && !isNavigationShortcut" class="mr-2" @click.stop>
          <input
            type="checkbox"
            :checked="selected"
            @change="toggleSelected"
            aria-label="选择"
          />
        </label>

        <span class="icon mr-2">
          <component :is="icon" :size="18" :stroke-width="1.7" />
        </span>
      </div>
    </td>

    <td>
      <a
        v-if="isNameLink"
        href="#"
        class="desktop-name-text desktop-name-link has-text-link"
        :class="nameTextClass"
        :title="file.name"
        @click.stop.prevent="activateNameLink"
      >
        <template v-for="(seg, i) in nameSegments" :key="i">
          <mark v-if="seg.match" class="has-background-warning-light">{{ seg.text }}</mark>
          <span v-else>{{ seg.text }}</span>
        </template>
      </a>
      <span
        v-else
        class="desktop-name-text"
        :class="nameTextClass"
        :title="file.name"
      >
        <template v-for="(seg, i) in nameSegments" :key="i">
          <mark v-if="seg.match" class="has-background-warning-light">{{ seg.text }}</mark>
          <span v-else>{{ seg.text }}</span>
        </template>
      </span>
    </td>

    <td class="is-narrow">{{ desktopFileDateLabel }}</td>

    <td class="is-narrow">{{ desktopFileKindLabel }}</td>

    <td class="is-narrow has-text-right">{{ desktopFileSizeLabel }}</td>

    <td class="is-narrow has-text-right" @click.stop>
      <div v-if="!isParentShortcut" class="buttons has-addons are-small is-right mb-0 desktop-action-buttons">
        <template v-if="isSelfShortcut">
          <button class="button is-ghost" @click="createDirectory" title="在当前目录下新建子目录" aria-label="在当前目录下新建子目录">
            <span class="icon is-small"><IconFolderPlus :size="16" /></span>
          </button>
        </template>
        <template v-else-if="file.kind === 'directory'">
          <button class="button is-ghost" @click="createDirectory" title="在此目录下新建子目录" aria-label="在此目录下新建子目录">
            <span class="icon is-small"><IconFolderPlus :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="renameEntry" title="重命名目录" aria-label="重命名目录">
            <span class="icon is-small"><IconPencil :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="moveEntry" title="移动目录" aria-label="移动目录">
            <span class="icon is-small"><IconArrowsDiff :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="download" title="下载目录" aria-label="下载目录">
            <span class="icon is-small"><IconDownload :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="share" title="分享目录" aria-label="分享目录">
            <span class="icon is-small"><IconShare :size="16" /></span>
          </button>
          <button class="button is-ghost is-danger" @click="confirmDelete" title="删除目录" aria-label="删除目录">
            <span class="icon is-small"><IconTrash :size="16" /></span>
          </button>
        </template>
        <template v-else>
          <button class="button is-ghost" @click="preview" title="预览文件" aria-label="预览文件">
            <span class="icon is-small"><IconEye :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="viewHistory" title="查看历史" aria-label="查看历史">
            <span class="icon is-small"><IconHistory :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="renameEntry" title="重命名文件" aria-label="重命名文件">
            <span class="icon is-small"><IconPencil :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="moveEntry" title="移动文件" aria-label="移动文件">
            <span class="icon is-small"><IconArrowsDiff :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="download" title="下载文件" aria-label="下载文件">
            <span class="icon is-small"><IconDownload :size="16" /></span>
          </button>
          <button class="button is-ghost" @click="share" title="分享文件" aria-label="分享文件">
            <span class="icon is-small"><IconShare :size="16" /></span>
          </button>
          <button class="button is-ghost is-danger" @click="confirmDelete" title="删除文件" aria-label="删除文件">
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
      'file-item--shortcut-parent': isParentShortcut,
      'file-item--shortcut-self': isSelfShortcut,
    }"
    @click="handleClick"
    @dblclick="handleActivate"
    @contextmenu.prevent="handleContextMenu"
  >
    <div class="media">
      <div class="media-left">
          <figure class="image is-48x48">
            <div class="file-icon">
              <component :is="icon" :size="32" :stroke-width="1.5" />
            </div>
          </figure>
        </div>
        <div class="media-content">
          <div class="content">
            <p class="file-name" :class="nameTextClass" :title="file.name">
              <template v-for="(seg, i) in nameSegments" :key="i"
                ><mark v-if="seg.match" class="has-background-warning-light">{{
                  seg.text
                }}</mark
                ><span v-else>{{ seg.text }}</span></template
              >
            </p>
            <p class="file-info">
              <template v-if="isNavigationShortcut">
                <span class="tag is-light mr-2">
                  {{ desktopFileKindLabel }}
                </span>
                <span class="has-text-grey-light is-size-7">
                  {{ desktopSubtitle }}
                </span>
              </template>
              <template v-else>
                <span v-if="file.kind === 'file'" class="tag is-light mr-2">
                  {{ formatSize(file.size_bytes || 0) }}
                </span>
                <span class="has-text-grey-light is-size-7">
                  {{ formatDate(file.created_at) }}
                </span>
              </template>
            </p>
            <p v-if="file.lastCommit" class="file-commit">
              <span class="tag is-info is-light">
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
        <div v-if="selectMode && !isNavigationShortcut" class="media-right">
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
      <div
        v-if="showActions && !selectMode && !isParentShortcut"
        class="file-actions"
        @click.stop
      >
        <div class="actions-bar">
          <button
            v-if="isSelfShortcut"
            class="action-btn"
            @click="createDirectory"
            title="新建目录"
          >
            <IconFolderPlus :size="20" />
            <span>新建目录</span>
          </button>
          <button
            v-else-if="file.kind === 'directory'"
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
          <button
            v-if="!isSelfShortcut"
            class="action-btn"
            @click="moveEntry"
            title="移动"
          >
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
import { computed } from "vue";
import { confirmDialog } from "../../composables/dialog";
import {
  IconArrowLeft,
  IconArrowsDiff,
  IconFolder,
  IconFolderPlus,
  IconFolderOpen,
  IconFile,
  IconFileText,
  IconFileCode,
  IconPhoto,
  IconFileZip,
  IconHistory,
  IconPencil,
  IconDownload,
  IconTrash,
  IconShare,
  IconEye,
} from "@tabler/icons-vue";
import type { FileInfo } from "../../types";

const props = defineProps<{
  file: FileInfo;
  highlight?: string;
  selectMode?: boolean;
  selected?: boolean;
  expanded?: boolean;
  desktop?: boolean;
}>();

const emit = defineEmits<{
  click: [file: FileInfo];
  download: [file: FileInfo];
  delete: [file: FileInfo];
  rename: [file: FileInfo];
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
}>();

const showActions = computed(() => props.expanded);
const uiRole = computed(
  () => (props.file as FileInfo & { uiRole?: "self" | "parent" }).uiRole,
);
const isNavigationShortcut = computed(
  () => uiRole.value === "self" || uiRole.value === "parent",
);
const isDirectoryEntry = computed(() => props.file.kind === "directory");
const isNameLink = computed(
  () => isNavigationShortcut.value || isDirectoryEntry.value,
);
const isSelfShortcut = computed(() => uiRole.value === "self");
const isParentShortcut = computed(
  () => uiRole.value === "parent",
);
const nameTextClass = computed(() => ({
  "has-text-weight-bold": isDirectoryEntry.value,
}));

const desktopFileDateLabel = computed(() => {
  if (isNavigationShortcut.value) return "--";
  return formatDate(props.file.updated_at || props.file.created_at);
});

const desktopFileSizeLabel = computed(() => {
  if (isNavigationShortcut.value) return "--";
  if (props.file.kind === "directory") return "--";
  return formatSize(props.file.size_bytes || 0);
});

const desktopFileKindLabel = computed(() => {
  if (isParentShortcut.value) return "父目录";
  if (isSelfShortcut.value) return "当前目录";
  if (props.file.kind === "directory") return "文件夹";

  const ext = getExtension(props.file.name);
  if (props.file.mime_type?.startsWith("image/")) return "图像文件";
  if (props.file.mime_type?.startsWith("video/")) return "视频文件";
  if (props.file.mime_type?.startsWith("audio/")) return "音频文件";
  if (ext === "pdf") return "PDF 文档";
  if (["txt", "md", "log"].includes(ext)) return "文本文档";
  if (["zip", "rar", "7z", "tar", "gz"].includes(ext)) return "压缩文件";
  if (ext) return `${ext.toUpperCase()} 文件`;
  return "文件";
});

const desktopSubtitle = computed(() => {
  if (isParentShortcut.value) {
    return "进入上一层目录";
  }

  if (isSelfShortcut.value) {
    return "当前目录快捷入口，可直接新建子目录";
  }

  if (props.file.lastCommit?.message) {
    return props.file.lastCommit.message;
  }

  if (props.file.kind === "directory") {
    return props.file.path || "根目录";
  }

  return props.file.mime_type || "双击打开预览";
});

const icon = computed(() => {
  if (isParentShortcut.value) return IconArrowLeft;
  if (props.file.kind === "directory") return IconFolder;

  const ext = props.file.name.split(".").pop()?.toLowerCase();

  if (["jpg", "jpeg", "png", "gif", "svg", "webp"].includes(ext || "")) {
    return IconPhoto;
  }
  if (
    [
      "js",
      "ts",
      "jsx",
      "tsx",
      "vue",
      "py",
      "java",
      "cpp",
      "c",
      "go",
      "rs",
    ].includes(ext || "")
  ) {
    return IconFileCode;
  }
  if (["txt", "md", "log"].includes(ext || "")) {
    return IconFileText;
  }
  if (["zip", "tar", "gz", "rar", "7z"].includes(ext || "")) {
    return IconFileZip;
  }

  return IconFile;
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

function getExtension(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase();
  return ext || "";
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatDate(date: string): string {
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) {
    return date || "--";
  }

  return parsed.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function handleClick(event?: MouseEvent) {
  if (isNavigationShortcut.value) {
    emit("openFolder", props.file);
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
  if (isNavigationShortcut.value) return;
  emit("contextMenu", { file: props.file, x: event.clientX, y: event.clientY });
}

function handleActivate() {
  if (isNavigationShortcut.value) {
    emit("openFolder", props.file);
    return;
  }
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
.file-item {
  cursor: pointer;
  transition: all 0.2s;
  margin-bottom: 0.75rem;
  content-visibility: auto;
  contain-intrinsic-size: 96px;
  padding-left: 0.75rem;
  padding-right: 0.75rem;
  position: relative;
  overflow: hidden;
}

.file-item:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}

.desktop-name-text {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desktop-name-link {
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 0.12em;
}

.desktop-name-link:hover,
.desktop-name-link:focus-visible {
  text-decoration-thickness: 2px;
}

.desktop-file-row > td {
  vertical-align: middle;
}

.desktop-action-buttons {
  flex-wrap: nowrap;
}

.file-item--shortcut-self {
  background: rgba(38, 132, 101, 0.08);
  box-shadow: inset 0 0 0 1px rgba(38, 132, 101, 0.12);
}

.file-item--shortcut-parent {
  background: rgba(186, 120, 18, 0.08);
  box-shadow: inset 0 0 0 1px rgba(186, 120, 18, 0.12);
}

.file-item--shortcut-self .file-icon {
  color: #1d7d62;
  background: transparent;
}

.file-item--shortcut-parent .file-icon {
  color: #9a650b;
  background: transparent;
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
  background: rgba(255, 255, 255, 0.95);
  backdrop-filter: blur(8px);
  border-top: 1px solid rgba(0, 0, 0, 0.05);
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
  color: #4a4a4a;
  cursor: pointer;
  border-radius: 6px;
  transition: all 0.15s;
  font-size: 0.7rem;
  min-width: 3rem;
}

.action-btn:hover {
  background: rgba(0, 0, 0, 0.05);
  color: #3273dc;
}

.action-btn:active {
  transform: scale(0.95);
}

.action-btn.is-danger:hover {
  background: rgba(255, 56, 96, 0.1);
  color: #ff3860;
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



/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .file-actions {
    background: rgba(30, 30, 30, 0.95);
    border-top-color: rgba(255, 255, 255, 0.1);
  }

  .action-btn {
    color: #f5f5f5;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }
}
</style>
