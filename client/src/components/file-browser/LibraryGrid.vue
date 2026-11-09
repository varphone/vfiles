<template>
  <section class="library-home" aria-labelledby="library-home-title">
    <header class="library-home-header">
      <div>
        <h2 id="library-home-title" class="library-home-title">我的资料库</h2>
        <p class="library-home-subtitle">{{ directories.length }} 个目录</p>
      </div>
    </header>

    <EmptyState
      v-if="directories.length === 0"
      :icon="IconFolderOpen"
      title="还没有目录"
      :hint="
        canWrite
          ? '在根目录新建文件夹后，它会作为资料库显示在这里'
          : '当前命名空间还没有可浏览的目录'
      "
    />

    <div v-else class="library-grid">
      <article
        v-for="directory in directories"
        :key="directory.path"
        class="library-card"
        :class="{ 'is-menu-open': openMenuPath === directory.path }"
        :data-vfiles-path="directory.path"
        tabindex="0"
        :aria-label="`打开资料库 ${directory.name}`"
        @click="handleCardClick(directory)"
        @keydown="handleCardKeydown($event, directory)"
        @contextmenu.prevent.stop="handleCardContextMenu($event, directory)"
      >
        <div class="library-card-cover" aria-hidden="true">
          <IconFolder :size="52" :stroke-width="1.25" />
        </div>

        <div class="library-card-body">
          <input
            v-if="renamingPath === directory.path"
            :ref="registerRenameInput"
            v-model="renameDraft"
            class="input is-small library-card-rename"
            type="text"
            :aria-label="`重命名 ${directory.name}`"
            @click.stop
            @dblclick.stop
            @keydown.enter.prevent.stop="commitRename(directory)"
            @keydown.esc.prevent.stop="cancelRename"
            @keydown.stop
            @blur="onRenameBlur(directory)"
          />
          <h3 v-else class="library-card-name" :title="directory.name">
            {{ directory.name }}
          </h3>
          <p class="library-card-description">文件夹</p>
          <p
            v-if="updatedLabel(directory)"
            class="library-card-updated"
            :title="updatedTitle(directory)"
          >
            <IconClock :size="12" aria-hidden="true" />
            <span>{{ updatedLabel(directory) }}</span>
          </p>
        </div>

        <div class="library-card-menu" @click.stop @keydown.stop>
          <button
            class="library-card-menu-trigger"
            type="button"
            :aria-label="`${directory.name} 的操作`"
            aria-haspopup="true"
            :aria-expanded="
              openMenuPath === directory.path ? 'true' : 'false'
            "
            @click.stop="toggleMenu(directory)"
          >
            <IconDots :size="16" />
          </button>

          <div
            v-if="openMenuPath === directory.path"
            class="library-card-menu-panel"
            role="menu"
          >
            <button
              class="library-card-menu-item"
              type="button"
              role="menuitem"
              @click="runAction('open', directory)"
            >
              <IconFolderOpen :size="16" />
              <span>打开</span>
            </button>
            <button
              v-if="canWrite"
              class="library-card-menu-item"
              type="button"
              role="menuitem"
              @click="runAction('rename', directory)"
            >
              <IconPencil :size="16" />
              <span>重命名</span>
            </button>
            <button
              class="library-card-menu-item"
              type="button"
              role="menuitem"
              @click="runAction('details', directory)"
            >
              <IconInfoCircle :size="16" />
              <span>详细信息</span>
            </button>
            <button
              v-if="canShare"
              class="library-card-menu-item"
              type="button"
              role="menuitem"
              @click="runAction('share', directory)"
            >
              <IconShare :size="16" />
              <span>分享</span>
            </button>
            <button
              v-if="canWrite"
              class="library-card-menu-item is-danger"
              type="button"
              role="menuitem"
              @click="runAction('delete', directory)"
            >
              <IconTrash :size="16" />
              <span>删除</span>
            </button>
          </div>
        </div>
      </article>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import {
  IconClock,
  IconDots,
  IconFolder,
  IconFolderOpen,
  IconInfoCircle,
  IconPencil,
  IconShare,
  IconTrash,
} from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import {
  formatDate,
  formatRelativeDate,
} from "../../utils/filePresentation";
import { useDocumentEventListener } from "../../composables/useDocumentEventListener";
import EmptyState from "../common/EmptyState.vue";

/**
 * 命名空间根目录的 Seafile 式「我的资料库」首页。
 *
 * 只接收根目录的一级子目录，不发起任何请求；卡片菜单复用 FileBrowser
 * 现有的打开/重命名/删除/分享/详细信息动作（由父组件处理事件）。
 */
const props = withDefaults(
  defineProps<{
    directories: FileInfo[];
    /** 内联重命名中的目录路径（由 FileBrowser 统一维护） */
    renamingPath?: string;
    canWrite?: boolean;
    canShare?: boolean;
  }>(),
  {
    renamingPath: "",
    canWrite: true,
    canShare: true,
  },
);

const emit = defineEmits<{
  (e: "open", directory: FileInfo): void;
  (e: "rename", directory: FileInfo): void;
  (e: "rename-commit", directory: FileInfo, name: string): void;
  (e: "rename-cancel", directory: FileInfo): void;
  (e: "delete", directory: FileInfo): void;
  (e: "share", directory: FileInfo): void;
  (e: "details", directory: FileInfo): void;
  (
    e: "context-menu",
    payload: { file: FileInfo; x: number; y: number },
  ): void;
}>();

type LibraryAction = "open" | "rename" | "details" | "share" | "delete";

const openMenuPath = ref("");
const renameDraft = ref("");
const renameInputRef = ref<HTMLInputElement | null>(null);
let renameSettled = false;

function registerRenameInput(element: unknown) {
  renameInputRef.value = element instanceof HTMLInputElement ? element : null;
}

const renamingDirectory = computed(
  () =>
    props.directories.find((item) => item.path === props.renamingPath) ?? null,
);

function selectBaseName(input: HTMLInputElement) {
  const name = input.value;
  const dot = name.lastIndexOf(".");
  const end = dot > 0 ? dot : name.length;
  input.setSelectionRange(0, end);
}

watch(
  () => props.renamingPath,
  (path) => {
    if (!path) return;
    renameSettled = false;
    renameDraft.value = renamingDirectory.value?.name ?? "";
    void nextTick().then(() => {
      const input = renameInputRef.value;
      if (!input) return;
      input.focus();
      selectBaseName(input);
    });
  },
  { immediate: true },
);

function toggleMenu(directory: FileInfo) {
  openMenuPath.value =
    openMenuPath.value === directory.path ? "" : directory.path;
}

function closeMenu() {
  openMenuPath.value = "";
}

function runAction(action: LibraryAction, directory: FileInfo) {
  closeMenu();
  switch (action) {
    case "open":
      emit("open", directory);
      break;
    case "rename":
      emit("rename", directory);
      break;
    case "details":
      emit("details", directory);
      break;
    case "share":
      emit("share", directory);
      break;
    case "delete":
      emit("delete", directory);
      break;
  }
}

function handleCardClick(directory: FileInfo) {
  if (openMenuPath.value) {
    closeMenu();
    return;
  }
  if (props.renamingPath === directory.path) return;
  emit("open", directory);
}

function handleCardKeydown(event: KeyboardEvent, directory: FileInfo) {
  if (event.target !== event.currentTarget) return;
  if (event.key !== "Enter" && event.key !== " ") return;
  event.preventDefault();
  emit("open", directory);
}

function handleCardContextMenu(event: MouseEvent, directory: FileInfo) {
  closeMenu();
  emit("context-menu", {
    file: directory,
    x: event.clientX,
    y: event.clientY,
  });
}

function updatedAt(directory: FileInfo): string {
  return directory.updated_at || "";
}

function updatedLabel(directory: FileInfo): string {
  const raw = updatedAt(directory);
  return raw ? formatRelativeDate(raw) : "";
}

function updatedTitle(directory: FileInfo): string {
  const raw = updatedAt(directory);
  return raw ? formatDate(raw) : "";
}

function commitRename(directory: FileInfo) {
  renameSettled = true;
  emit("rename-commit", directory, renameDraft.value.trim());
}

function cancelRename() {
  renameSettled = true;
  if (renamingDirectory.value) {
    emit("rename-cancel", renamingDirectory.value);
  }
}

function onRenameBlur(directory: FileInfo) {
  if (renameSettled) return;
  const name = renameDraft.value.trim();
  if (name && name !== directory.name) {
    emit("rename-commit", directory, name);
    return;
  }
  emit("rename-cancel", directory);
}

function onDocumentPointerDown(event: Event) {
  if (!openMenuPath.value) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest(".library-card-menu")) return;
  closeMenu();
}

useDocumentEventListener("pointerdown", onDocumentPointerDown);
useDocumentEventListener("keydown", (event) => {
  if ((event as KeyboardEvent).key === "Escape") closeMenu();
});
</script>

<style scoped>
.library-home {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.35rem 1.1rem 2rem;
  min-height: 100%;
  box-sizing: border-box;
}

.library-home-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 1rem;
}

.library-home-title {
  margin: 0;
  color: var(--vf-text-strong);
  font-size: 1.05rem;
  font-weight: 600;
  letter-spacing: 0.01em;
}

.library-home-subtitle {
  margin: 0.2rem 0 0;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.library-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 1rem;
  align-content: start;
}

.library-card {
  position: relative;
  min-width: 0;
  border: 1px solid var(--vf-border-weak);
  border-radius: 8px;
  background: var(--vf-surface);
  cursor: pointer;
  transition:
    transform 0.14s var(--vf-motion-standard),
    border-color 0.14s var(--vf-motion-standard),
    box-shadow 0.14s var(--vf-motion-standard);
}

.library-card:hover {
  transform: translateY(-2px);
  border-color: var(--vf-accent-line);
  box-shadow: var(--vf-shadow-md);
}

.library-card:focus-visible {
  outline: 2px solid var(--vf-focus-ring);
  outline-offset: 2px;
  border-color: var(--vf-accent-line);
}

.library-card.is-menu-open {
  z-index: 5;
  border-color: var(--vf-accent-line);
  box-shadow: var(--vf-shadow-md);
}

.library-card-cover {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 92px;
  border-radius: 7px 7px 0 0;
  background: var(--vf-accent-soft);
  color: var(--vf-accent);
}

.library-card-body {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  padding: 0.7rem 0.85rem 0.85rem;
  min-width: 0;
}

.library-card-name {
  margin: 0;
  overflow: hidden;
  color: var(--vf-text-strong);
  font-size: 0.9rem;
  font-weight: 600;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.library-card-description {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 0.78rem;
}

.library-card-updated {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  margin: 0.35rem 0 0;
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
}

.library-card-rename {
  width: 100%;
  height: 1.9rem;
  font-size: 0.82rem;
}

.library-card-menu {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 2;
}

.library-card-menu-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 27px;
  height: 27px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: var(--vf-surface-translucent);
  backdrop-filter: blur(8px);
  color: var(--vf-text);
  cursor: pointer;
  opacity: 0.7;
  transition:
    opacity 0.12s var(--vf-motion-standard),
    background-color 0.12s var(--vf-motion-standard);
}

.library-card:hover .library-card-menu-trigger,
.library-card-menu-trigger:focus-visible,
.library-card.is-menu-open .library-card-menu-trigger {
  opacity: 1;
  background: var(--vf-surface);
}

@media (hover: none) {
  .library-card-menu-trigger {
    opacity: 1;
  }
}

.library-card-menu-panel {
  position: absolute;
  top: 32px;
  right: 0;
  z-index: 30;
  min-width: 158px;
  padding: 4px;
  border: 1px solid var(--vf-border);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-menu);
}

.library-card-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.8rem;
  text-align: left;
  cursor: pointer;
}

.library-card-menu-item:hover {
  background: var(--vf-surface-sunken);
}

.library-card-menu-item.is-danger {
  color: var(--vf-danger-text);
}

.library-card-menu-item.is-danger:hover {
  background: var(--vf-danger-soft);
}

@media screen and (max-width: 1023px) {
  .library-home {
    padding: 1rem 0.9rem 1.5rem;
  }

  .library-grid {
    gap: 0.75rem;
  }

  .library-card-cover {
    height: 76px;
  }
}
</style>
