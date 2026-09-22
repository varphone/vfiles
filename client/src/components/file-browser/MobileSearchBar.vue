<template>
  <div class="mobile-search-toolbar">
    <div
      v-if="pullIndicatorVisible"
      class="has-text-centered is-size-7 vf-text-muted mb-2"
    >
      <span v-if="pullRefreshing">刷新中...</span>
      <span v-else-if="pullReady">释放刷新</span>
      <span v-else>下拉刷新</span>
    </div>

    <div class="mobile-search-row">
      <div class="control is-expanded">
        <input
          :value="query"
          class="input is-small mobile-search-input"
          type="search"
          :placeholder="content ? '搜索文件内容...' : '搜索文件名...'"
          list="vfiles-search-history"
          @input="
            emit('update:query', ($event.target as HTMLInputElement).value)
          "
          @keyup.enter="emit('search')"
        />
        <datalist id="vfiles-search-history">
          <option v-for="item in history" :key="item" :value="item" />
        </datalist>
      </div>
      <ViewOptions />
      <SortMenu />
      <button
        class="vf-icon-button"
        :class="{ 'is-active': filtersOpen }"
        title="搜索筛选"
        aria-label="搜索筛选"
        :aria-expanded="filtersOpen ? 'true' : 'false'"
        @click="emit('update:filtersOpen', !filtersOpen)"
      >
        <IconAdjustmentsHorizontal :size="18" />
      </button>
      <button
        class="vf-icon-button"
        :class="{ 'is-active': active, 'is-loading': loading }"
        :disabled="loading"
        title="搜索"
        aria-label="搜索"
        @click="emit('search')"
      >
        <IconSearch :size="18" />
      </button>
      <button
        v-if="active || query"
        class="vf-icon-button"
        title="清空搜索"
        aria-label="清空搜索"
        :disabled="loading"
        @click="emit('clear')"
      >
        <IconX :size="18" />
      </button>
    </div>

    <div v-if="filtersOpen" class="mobile-search-filters">
      <label class="checkbox mobile-filter-item">
        <input
          type="checkbox"
          :checked="content"
          :disabled="loading || !contentEnabled"
          @change="
            emit('update:content', ($event.target as HTMLInputElement).checked)
          "
        />
        全文搜索
      </label>
      <p v-if="!contentEnabled" class="help is-warning mb-0">
        内容搜索功能未启用
      </p>

      <div class="select is-small">
        <select
          :value="type"
          :disabled="loading"
          aria-label="搜索类型"
          @change="
            emit('update:type', ($event.target as HTMLSelectElement).value)
          "
        >
          <option value="all">全部</option>
          <option value="file">仅文件</option>
          <option value="directory">仅文件夹</option>
        </select>
      </div>

      <label class="checkbox mobile-filter-item">
        <input
          type="checkbox"
          :checked="scopeCurrent"
          :disabled="loading"
          @change="
            emit(
              'update:scopeCurrent',
              ($event.target as HTMLInputElement).checked,
            )
          "
        />
        仅当前目录
      </label>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  IconAdjustmentsHorizontal,
  IconSearch,
  IconX,
} from "@tabler/icons-vue";
import SortMenu from "./SortMenu.vue";
import ViewOptions from "./ViewOptions.vue";

/**
 * 移动端工具栏：下拉刷新提示 + 搜索行 + 高级筛选。
 *
 * 只负责展示与上报交互，搜索状态仍由 FileBrowser 持有（结果、分页、过期响应
 * 处理都在那边），避免把整套搜索状态再往上提一层。
 */
withDefaults(
  defineProps<{
    query: string;
    loading?: boolean;
    active?: boolean;
    content: boolean;
    contentEnabled?: boolean;
    type: string;
    scopeCurrent: boolean;
    filtersOpen: boolean;
    history?: string[];
    pullIndicatorVisible?: boolean;
    pullRefreshing?: boolean;
    pullReady?: boolean;
  }>(),
  {
    loading: false,
    active: false,
    contentEnabled: false,
    history: () => [],
    pullIndicatorVisible: false,
    pullRefreshing: false,
    pullReady: false,
  },
);

const emit = defineEmits<{
  (e: "update:query", value: string): void;
  (e: "update:content", value: boolean): void;
  (e: "update:type", value: string): void;
  (e: "update:scopeCurrent", value: boolean): void;
  (e: "update:filtersOpen", value: boolean): void;
  (e: "search"): void;
  (e: "clear"): void;
}>();
</script>

<style scoped>
/* 移动端搜索栏吸顶：滚动结果时搜索框与筛选始终可达（主流移动端行为） */
.mobile-search-toolbar {
  position: sticky;
  top: calc(var(--bulma-navbar-height, 3.25rem) + env(safe-area-inset-top));
  z-index: 20;
  padding: 0.5rem 0.4rem;
  margin: 0 -0.4rem;
  background: var(--vf-surface);
  border-bottom: 1px solid var(--vf-border-weak);
}

.mobile-search-row {
  /* 弹层（视图/排序）的定位容器：面板在行宽内居中，避免被外层裁切 */
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.3rem;
}

/* 搜索行里的视图/排序下拉需要浮在列表之上（子组件元素需要 :deep() 才匹配得到） */
.mobile-search-row :deep(.dropdown-menu) {
  z-index: 30;
}

.mobile-search-row :deep(.dropdown-menu .vf-ghost-button) {
  min-height: 1.75rem;
}

.mobile-search-input {
  border-radius: var(--vf-radius-pill);
  border-color: transparent;
  background: var(--vf-surface-sunken);
}

.mobile-search-input:focus {
  border-color: var(--vf-accent);
  background: var(--vf-surface);
}

.mobile-search-filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.6rem;
  margin-top: 0.5rem;
  padding: 0.6rem;
  border-radius: var(--vf-radius);
  background: var(--vf-surface-sunken);
  font-size: 0.82rem;
}

.mobile-filter-item {
  margin-bottom: 0 !important;
}
</style>
