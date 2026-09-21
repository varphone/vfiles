<template>
  <div class="search-toolbar" role="group" aria-label="搜索结果筛选">
    <p class="search-toolbar-summary">{{ summary }}</p>
    <div v-if="buckets.length > 1" class="search-toolbar-chips">
      <button
        class="vf-ghost-button search-chip"
        :class="{ 'is-active': active === 'all' }"
        type="button"
        :aria-pressed="active === 'all' ? 'true' : 'false'"
        @click="emit('update:active', 'all')"
      >
        <span>全部</span>
        <span class="search-chip-count">{{ total }}</span>
      </button>
      <button
        v-for="bucket in buckets"
        :key="bucket.key"
        class="vf-ghost-button search-chip"
        :class="{ 'is-active': active === bucket.key }"
        type="button"
        :aria-pressed="active === bucket.key ? 'true' : 'false'"
        @click="emit('update:active', bucket.key)"
      >
        <component :is="bucket.icon" :size="14" />
        <span>{{ bucket.label }}</span>
        <span class="search-chip-count">{{ bucket.count }}</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  IconFile,
  IconFileText,
  IconFolder,
  IconMovie,
  IconMusic,
  IconPhoto,
} from "@tabler/icons-vue";
import type { SearchResultFilter } from "../../composables/useFileSearch";
import { fileIconKind } from "../../utils/filePresentation";
import type { FileInfo } from "../../types";

/**
 * 搜索结果工具条：结果摘要 + 类型筛选 chips。
 *
 * 分类沿用文件图标的判定（`fileIconKind`），保证列表里的图标与筛选口径一致。
 */
const props = defineProps<{
  results: FileInfo[];
  active: SearchResultFilter;
  query: string;
  /** 当前实际展示数量（可能受分页影响）。 */
  visibleCount: number;
}>();

const emit = defineEmits<{
  (e: "update:active", filter: SearchResultFilter): void;
}>();

const BUCKETS: {
  key: Exclude<SearchResultFilter, "all">;
  label: string;
  icon: unknown;
  kinds: string[];
}[] = [
  { key: "folder", label: "文件夹", icon: IconFolder, kinds: ["folder"] },
  {
    key: "document",
    label: "文档",
    icon: IconFileText,
    kinds: ["text", "code", "pdf"],
  },
  { key: "image", label: "图片", icon: IconPhoto, kinds: ["image"] },
  { key: "video", label: "视频", icon: IconMovie, kinds: ["video"] },
  { key: "audio", label: "音频", icon: IconMusic, kinds: ["audio"] },
  { key: "other", label: "其它", icon: IconFile, kinds: ["archive", "file"] },
];

const total = computed(() => props.results.length);

const buckets = computed(() =>
  BUCKETS.map((bucket) => ({
    ...bucket,
    count: props.results.filter((file) =>
      bucket.kinds.includes(fileIconKind(file)),
    ).length,
  })).filter((bucket) => bucket.count > 0),
);

const summary = computed(() => {
  const totalLabel = `找到 ${total.value} 项`;
  if (props.active === "all") {
    const more =
      props.visibleCount < total.value
        ? ` · 已显示 ${props.visibleCount} 项`
        : "";
    return `${totalLabel}${more}`;
  }
  const bucket = BUCKETS.find((item) => item.key === props.active);
  return `${totalLabel} · ${bucket?.label ?? "筛选"} ${props.visibleCount} 项`;
});
</script>

<style scoped>
.search-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.6rem;
  flex-wrap: wrap;
  padding: 0 0 0.5rem;
}

.search-toolbar-summary {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.search-toolbar-chips {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-wrap: wrap;
}

/* 窄屏：筛选 chip 单行横向滚动，避免换行挤压结果列表 */
@media screen and (max-width: 700px) {
  .search-toolbar {
    flex-direction: column;
    align-items: stretch;
    gap: 0.35rem;
  }

  .search-toolbar-chips {
    flex-wrap: nowrap;
    overflow-x: auto;
    padding-bottom: 0.15rem;
    scrollbar-width: none;
    -webkit-overflow-scrolling: touch;
  }

  .search-toolbar-chips::-webkit-scrollbar {
    display: none;
  }

  .search-toolbar-chips .search-chip {
    flex: 0 0 auto;
  }
}

.search-chip {
  gap: 0.3rem;
  min-height: 1.75rem;
  padding: 0 0.5rem;
  font-size: 0.78rem;
}

.search-chip-count {
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
}

.search-chip.is-active .search-chip-count {
  color: currentColor;
  opacity: 0.75;
}
</style>
