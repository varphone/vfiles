<template>
  <div class="desktop-command-bar">
    <div class="desktop-command-group">
      <button
        class="vf-icon-button"
        :disabled="!canGoUp"
        title="上一级"
        aria-label="上一级"
        @click="emit('go-up')"
      >
        <IconArrowLeft :size="18" />
      </button>
      <button
        class="vf-icon-button"
        title="刷新"
        aria-label="刷新"
        @click="emit('refresh')"
      >
        <IconRefresh :size="18" />
      </button>
      <ViewOptions />
      <SortMenu />
      <button
        class="vf-icon-button"
        :class="{ 'is-active': detailsVisible }"
        :title="detailsVisible ? '隐藏详细信息' : '显示详细信息'"
        :aria-label="detailsVisible ? '隐藏详细信息' : '显示详细信息'"
        :aria-pressed="detailsVisible ? 'true' : 'false'"
        @click="emit('toggle-details')"
      >
        <IconLayoutSidebarRight :size="18" />
      </button>
      <button
        class="vf-icon-button"
        :class="{ 'is-active': batchMode }"
        :title="batchMode ? '退出批量选择' : '批量选择'"
        :aria-label="batchMode ? '退出批量选择' : '批量选择'"
        :aria-pressed="batchMode ? 'true' : 'false'"
        @click="emit('toggle-batch')"
      >
        <IconChecklist :size="18" />
      </button>

      <BrowserSearchBox
        :model-value="searchQuery"
        :open="searchOpen"
        :content="searchContent"
        :type="searchType"
        :scope-current="searchScopeCurrent"
        :loading="searchLoading"
        :active="searchActive"
        :content-enabled="searchContentEnabled"
        :filters-active="searchFiltersActive"
        :history="searchHistory"
        :register-input="registerInput"
        @update:model-value="emit('update:searchQuery', $event)"
        @update:open="emit('update:searchOpen', $event)"
        @update:content="emit('update:searchContent', $event)"
        @update:type="emit('update:searchType', $event)"
        @update:scope-current="emit('update:searchScopeCurrent', $event)"
        @search="emit('search')"
        @clear="emit('clear')"
      />

      <button
        class="vf-ghost-button desktop-create-folder"
        type="button"
        title="新建文件夹"
        aria-label="新建文件夹"
        @click="emit('create-folder')"
      >
        <IconFolderPlus :size="16" />
        <span>新建文件夹</span>
      </button>

      <button
        v-if="uploadIndicator"
        class="vf-ghost-button upload-indicator"
        type="button"
        :title="uploadIndicator.title"
        @click="emit('upload-files')"
      >
        <span class="upload-indicator-dot" aria-hidden="true"></span>
        <span>{{ uploadIndicator.label }}</span>
      </button>

      <div
        ref="uploadMenuRef"
        class="dropdown is-right desktop-upload-split"
        :class="{ 'is-active': uploadMenuOpen }"
      >
        <div class="desktop-upload-group">
          <button
            class="vf-primary-button desktop-primary-action"
            type="button"
            title="上传文件"
            @click="emit('upload-files')"
          >
            <IconUpload :size="16" />
            <span>上传</span>
          </button>
          <button
            class="vf-primary-button desktop-upload-caret"
            type="button"
            :aria-expanded="uploadMenuOpen ? 'true' : 'false'"
            aria-haspopup="true"
            aria-label="更多上传方式"
            title="更多上传方式"
            @click="toggleUploadMenu"
          >
            <IconChevronDown :size="14" />
          </button>
        </div>

        <div class="dropdown-menu desktop-upload-menu" role="menu">
          <div class="dropdown-content">
            <button
              class="dropdown-item desktop-upload-item"
              type="button"
              role="menuitem"
              @click="chooseUpload('files')"
            >
              <IconFileUpload :size="16" />
              <span>上传文件</span>
            </button>
            <button
              class="dropdown-item desktop-upload-item"
              type="button"
              role="menuitem"
              @click="chooseUpload('directory')"
            >
              <IconFolderUp :size="16" />
              <span>上传文件夹</span>
            </button>
            <hr class="dropdown-divider" />
            <button
              class="dropdown-item desktop-upload-item"
              type="button"
              role="menuitem"
              @click="chooseCreateFolder"
            >
              <IconFolderPlus :size="16" />
              <span>新建文件夹</span>
            </button>
          </div>
        </div>
      </div>
    </div>

    <div v-if="searchError" class="notification is-danger is-light">
      <IconAlertCircle :size="18" class="mr-2" />
      {{ searchError }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import {
  IconAlertCircle,
  IconArrowLeft,
  IconChecklist,
  IconChevronDown,
  IconFileUpload,
  IconFolderPlus,
  IconFolderUp,
  IconLayoutSidebarRight,
  IconRefresh,
  IconUpload,
} from "@tabler/icons-vue";
import BrowserSearchBox, { type SearchType } from "./BrowserSearchBox.vue";
import SortMenu from "./SortMenu.vue";
import ViewOptions from "./ViewOptions.vue";

/**
 * 桌面工具栏动作组：导航、视图/排序、详情与批量开关、搜索框、上传入口。
 *
 * 只做展示与上报，状态仍由 FileBrowser 持有（搜索分页、批量选择、上传队列都在那里）。
 */
withDefaults(
  defineProps<{
    canGoUp?: boolean;
    detailsVisible?: boolean;
    batchMode?: boolean;
    searchQuery: string;
    searchOpen: boolean;
    searchContent: boolean;
    searchType: SearchType;
    searchScopeCurrent: boolean;
    searchLoading?: boolean;
    searchActive?: boolean;
    searchContentEnabled?: boolean;
    searchFiltersActive?: boolean;
    searchHistory?: string[];
    searchError?: string | null;
    uploadIndicator?: { label: string; title: string } | null;
    registerInput?: (el: Element | { $el?: Element } | null) => void;
  }>(),
  {
    canGoUp: false,
    detailsVisible: false,
    batchMode: false,
    searchLoading: false,
    searchActive: false,
    searchContentEnabled: false,
    searchFiltersActive: false,
    searchHistory: () => [],
    searchError: null,
    uploadIndicator: null,
    registerInput: undefined,
  },
);

const uploadMenuOpen = ref(false);
const uploadMenuRef = ref<HTMLElement | null>(null);

function toggleUploadMenu() {
  uploadMenuOpen.value = !uploadMenuOpen.value;
}

/** 选择上传方式：主按钮与菜单项都走同一事件，由父组件决定是否自动弹出选择器。 */
function chooseUpload(kind: "files" | "directory") {
  uploadMenuOpen.value = false;
  if (kind === "directory") {
    emit("upload-folder");
  } else {
    emit("upload-files");
  }
}

function chooseCreateFolder() {
  uploadMenuOpen.value = false;
  emit("create-folder");
}

/** 点击菜单外部或按 Esc 收起 */
function onDocumentPointer(event: MouseEvent | TouchEvent) {
  if (!uploadMenuOpen.value) return;
  const target = event.target as Node | null;
  if (target && uploadMenuRef.value?.contains(target)) return;
  uploadMenuOpen.value = false;
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && uploadMenuOpen.value) {
    uploadMenuOpen.value = false;
  }
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

const emit = defineEmits<{
  (e: "go-up"): void;
  (e: "refresh"): void;
  (e: "toggle-details"): void;
  (e: "toggle-batch"): void;
  (e: "upload-files"): void;
  (e: "upload-folder"): void;
  (e: "create-folder"): void;
  (e: "search"): void;
  (e: "clear"): void;
  (e: "update:searchQuery", value: string): void;
  (e: "update:searchOpen", value: boolean): void;
  (e: "update:searchContent", value: boolean): void;
  (e: "update:searchType", value: string): void;
  (e: "update:searchScopeCurrent", value: boolean): void;
}>();
</script>

<style scoped>
.desktop-command-bar {
  display: block;
}

/* 动作组排成一行：左侧导航/视图/排序/详情/批量，搜索框自适应，上传按钮固定右侧 */
.desktop-command-group {
  min-width: 0;
  display: flex;
  flex-wrap: nowrap;
  align-items: center;
  gap: 0.3rem;
}

.desktop-command-group > :deep(.desktop-search-box) {
  flex: 1 1 auto;
  min-width: 0;
  margin-left: auto;
}

.desktop-primary-action {
  margin-left: 0.25rem;
  flex: 0 0 auto;
}

/* 上传：分裂按钮（主按钮 + 下拉箭头），下拉里包含「上传文件夹」等次级方式 */
.desktop-upload-split {
  flex: 0 0 auto;
}

.desktop-upload-group {
  display: inline-flex;
  align-items: stretch;
}

.desktop-upload-group .desktop-primary-action {
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
}

.desktop-upload-caret {
  margin-left: 1px;
  padding: 0 0.4rem;
  min-height: 2rem;
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
}

.desktop-upload-menu {
  /* 只保留布局：框体（背景/边框/圆角/阴影）由内层 .dropdown-content 的
     全局规则提供。此前包装层再画一套，造成双层边框、双阴影与 6/10px 圆角混用。 */
  min-width: 11rem;
  margin-top: 0.35rem;
}

.desktop-upload-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.45rem 0.55rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.84rem;
  text-align: left;
  cursor: pointer;
}

.desktop-upload-item:hover {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

/* 新建文件夹：文字按钮，放在上传左侧（搜索框自适应收缩，工具栏保持单行） */
.desktop-create-folder {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.8rem;
}

/* 上传进度胶囊：与「上传」按钮并排，显示队列进度 */
.upload-indicator {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.78rem;
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
