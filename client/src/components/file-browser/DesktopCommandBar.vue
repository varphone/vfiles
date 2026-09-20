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
        v-if="uploadIndicator"
        class="vf-ghost-button upload-indicator"
        type="button"
        :title="uploadIndicator.title"
        @click="emit('upload')"
      >
        <span class="upload-indicator-dot" aria-hidden="true"></span>
        <span>{{ uploadIndicator.label }}</span>
      </button>

      <button
        class="vf-primary-button desktop-primary-action"
        @click="emit('upload')"
      >
        <IconUpload :size="16" />
        <span>上传</span>
      </button>
    </div>

    <div v-if="searchError" class="notification is-danger is-light">
      <IconAlertCircle :size="18" class="mr-2" />
      {{ searchError }}
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  IconAlertCircle,
  IconArrowLeft,
  IconChecklist,
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

const emit = defineEmits<{
  (e: "go-up"): void;
  (e: "refresh"): void;
  (e: "toggle-details"): void;
  (e: "toggle-batch"): void;
  (e: "upload"): void;
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
  animation: upload-indicator-pulse 1.2s ease-in-out infinite;
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
