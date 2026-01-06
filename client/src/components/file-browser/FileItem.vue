<template>
  <div
    class="file-item box"
    :class="{ 'has-background-light': selected, 'is-expanded': showActions }"
    @click="handleClick"
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
          <p class="file-name">
            <strong
              ><template v-for="(seg, i) in nameSegments" :key="i"
                ><mark v-if="seg.match" class="has-background-warning-light">{{
                  seg.text
                }}</mark
                ><span v-else>{{ seg.text }}</span></template
              ></strong
            >
          </p>
          <p class="file-info">
            <span v-if="file.type === 'file'" class="tag is-light mr-2">
              {{ formatSize(file.size) }}
            </span>
            <span class="has-text-grey-light is-size-7">
              {{ formatDate(file.mtime) }}
            </span>
          </p>
          <p v-if="file.lastCommit" class="file-commit">
            <span class="tag is-info is-light">
              {{ file.lastCommit.message }}
            </span>
          </p>

          <div
            v-if="file.type === 'file' && file.matches && file.matches.length"
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
            v-if="file.type === 'directory'"
            class="action-btn"
            @click="openFolder"
            title="打开"
          >
            <IconFolderOpen :size="20" />
            <span>打开</span>
          </button>
          <button
            v-if="file.type === 'file'"
            class="action-btn"
            @click="preview"
            title="预览"
          >
            <IconEye :size="20" />
            <span>预览</span>
          </button>
          <button
            v-if="file.type === 'file'"
            class="action-btn"
            @click="viewHistory"
            title="历史"
          >
            <IconHistory :size="20" />
            <span>历史</span>
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
import {
  IconFolder,
  IconFolderOpen,
  IconFile,
  IconFileText,
  IconFileCode,
  IconPhoto,
  IconFileZip,
  IconHistory,
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
}>();

const emit = defineEmits<{
  click: [file: FileInfo];
  download: [file: FileInfo];
  delete: [file: FileInfo];
  viewHistory: [file: FileInfo];
  toggleSelect: [file: FileInfo];
  share: [file: FileInfo];
  preview: [file: FileInfo];
  openFolder: [file: FileInfo];
  collapse: [];
}>();

const showActions = computed(() => props.expanded);

const icon = computed(() => {
  if (props.file.type === "directory") return IconFolder;

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

function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatDate(date: string): string {
  return new Date(date).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function handleClick() {
  if (props.selectMode) {
    emit("toggleSelect", props.file);
    return;
  }
  // 点击切换展开/收起操作栏
  emit("click", props.file);
}

function toggleSelected() {
  emit("toggleSelect", props.file);
}

function openFolder() {
  emit("openFolder", props.file);
}

function preview() {
  emit("preview", props.file);
}

function download() {
  emit("download", props.file);
}

function confirmDelete() {
  if (confirm(`确定要删除 ${props.file.name} 吗？`)) {
    emit("delete", props.file);
  }
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

@media screen and (max-width: 768px) {
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
