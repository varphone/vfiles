<template>
  <section class="sidebar-overview" aria-label="工作区概览">
    <div v-if="loading && !overview" class="sidebar-overview-block">
      <p class="sidebar-overview-title">存储用量</p>
      <SkeletonList
        variant="lines"
        :rows="5"
        row-height="50px"
        label="加载工作区概览"
      />
    </div>

    <div v-else-if="error" class="sidebar-overview-block">
      <p class="sidebar-overview-title">存储用量</p>
      <p class="sidebar-overview-error">
        {{ error }}
        <button class="sidebar-overview-retry" type="button" @click="load">
          重试
        </button>
      </p>
    </div>

    <div v-else-if="overview" class="sidebar-overview-block">
      <p class="sidebar-overview-title">存储用量</p>
      <p class="sidebar-overview-usage">
        <span class="sidebar-overview-size">{{ formattedSize }}</span>
        <span class="sidebar-overview-counts">
          {{ overview.file_count }} 文件 · {{ overview.directory_count }} 目录
        </span>
      </p>

      <!-- 分类占比条：与主流网盘一致，先给整体构成，再列出各类占用 -->
      <div
        class="storage-bar"
        role="img"
        :aria-label="storageBarLabel"
        :title="storageBarLabel"
      >
        <span
          v-for="segment in storageSegments"
          :key="segment.category"
          class="storage-bar-segment"
          :class="`is-${segment.category}`"
          :style="{ width: `${segment.percent}%` }"
        ></span>
        <span
          v-if="storageSegments.length === 0"
          class="storage-bar-segment is-empty"
        ></span>
      </div>

      <ul v-if="storageSegments.length > 0" class="storage-legend">
        <li
          v-for="segment in storageSegments"
          :key="segment.category"
          class="storage-legend-item"
          :title="`${segment.label} · ${segment.count} 个文件`"
        >
          <span
            class="storage-legend-dot"
            :class="`is-${segment.category}`"
            aria-hidden="true"
          ></span>
          <span class="storage-legend-label">{{ segment.label }}</span>
          <span class="storage-legend-size">{{ segment.sizeLabel }}</span>
        </li>
      </ul>
    </div>

    <div v-if="favoritesTotal > 0 || favoritesLoading" class="sidebar-overview-block">
      <p class="sidebar-overview-title">收藏</p>
      <p v-if="favoritesTotal > 0" class="sidebar-favorites-page">
        第 {{ favoritesOffset + 1 }}–{{ favoritesOffset + favorites.length }} 项，共 {{ favoritesTotal }} 项
      </p>
      <ul class="sidebar-recent">
        <li v-for="item in favorites" :key="item.path">
          <div class="sidebar-favorite-row">
            <button
              class="sidebar-recent-item"
              type="button"
              :title="item.path"
              @click="emit('open-favorite', item)"
            >
              <FileTypeIcon :file="favoriteAsFileInfo(item)" :size="16" />
              <span class="sidebar-recent-name">{{ item.name }}</span>
            </button>
            <button
              class="sidebar-favorite-remove"
              type="button"
              :aria-label="`取消收藏 ${item.name}`"
              title="取消收藏"
              @click="removeFavorite(item)"
            >
              <IconStarFilled :size="14" />
            </button>
          </div>
        </li>
      </ul>
      <div v-if="favoritesTotal > FAVORITES_PAGE_SIZE" class="sidebar-favorites-pagination">
        <button
          type="button"
          :disabled="favoritesOffset === 0 || favoritesLoading"
          @click="changeFavoritesPage(-1)"
        >
          上一页
        </button>
        <button
          type="button"
          :disabled="!favoritesHasMore || favoritesLoading"
          @click="changeFavoritesPage(1)"
        >
          下一页
        </button>
      </div>
    </div>

    <div
      v-if="overview && overview.recent_files.length > 0"
      class="sidebar-overview-block"
    >
      <p class="sidebar-overview-title">最近更新</p>
      <ul class="sidebar-recent">
        <li v-for="file in overview.recent_files" :key="file.path">
          <button
            class="sidebar-recent-item"
            type="button"
            :title="file.path"
            @click="emit('open-file', file)"
          >
            <FileTypeIcon :file="recentAsFileInfo(file)" :size="16" />
            <span class="sidebar-recent-name">{{ file.name }}</span>
            <span class="sidebar-recent-time">{{
              formatRelativeDate(file.updated_at)
            }}</span>
          </button>
        </li>
      </ul>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { filesService } from "../../services/files.service";
import type {
  FavoriteEntry,
  RecentFile,
  StorageCategory,
  WorkspaceOverview,
} from "../../types";
import { IconStarFilled } from "@tabler/icons-vue";
import { formatRelativeDate, formatSize } from "../../utils/filePresentation";
import SkeletonList from "../common/SkeletonList.vue";
import FileTypeIcon from "./FileTypeIcon.vue";

/**
 * 侧栏底部的工作区概览：存储用量与最近更新的文件。
 *
 * 数据来自 `GET /api/files/overview`（服务端 SQL 聚合，不拉全量条目）。
 * 文件发生变更后由父组件递增 `refreshKey` 触发重新加载。
 */
const props = withDefaults(
  defineProps<{
    /** 每次数据变更后递增，用于触发刷新。 */
    refreshKey?: number;
  }>(),
  { refreshKey: 0 },
);

const emit = defineEmits<{
  (e: "open-file", file: RecentFile): void;
  (e: "open-favorite", entry: FavoriteEntry): void;
  /// 收藏增删后通知父组件，便于同步右键菜单里的星标状态
  (e: "favorite-changed", path: string, isFavorite: boolean): void;
}>();

const FAVORITES_PAGE_SIZE = 50;
const overview = ref<WorkspaceOverview | null>(null);
const favorites = ref<FavoriteEntry[]>([]);
const favoritesTotal = ref(0);
const favoritesOffset = ref(0);
const favoritesHasMore = ref(false);
const favoritesLoading = ref(false);
const loading = ref(false);
const error = ref("");

const formattedSize = computed(() =>
  formatSize(overview.value?.total_bytes ?? 0),
);

/** 分类展示名与顺序（与主流网盘的用量分组一致）。 */
const CATEGORY_LABELS: { key: StorageCategory; label: string }[] = [
  { key: "document", label: "文档" },
  { key: "image", label: "图片" },
  { key: "video", label: "视频" },
  { key: "audio", label: "音频" },
  { key: "other", label: "其它" },
];

/**
 * 占比条分段：宽度按字节占比计算，忽略空分类。
 *
 * 最小宽度 2% 保证极小分类仍然可见（否则 1 字节的文件在几 GB 里会消失）。
 */
const storageSegments = computed(() => {
  const categories = overview.value?.categories ?? [];
  const total = categories.reduce((sum, item) => sum + item.bytes, 0);
  if (total <= 0) return [];

  return CATEGORY_LABELS.map(({ key, label }) => {
    const usage = categories.find((item) => item.category === key);
    if (!usage || usage.bytes <= 0) return null;
    return {
      category: key,
      label,
      bytes: usage.bytes,
      count: usage.file_count,
      sizeLabel: formatSize(usage.bytes),
      percent: Math.max(2, (usage.bytes / total) * 100),
    };
  })
    .filter((segment): segment is NonNullable<typeof segment> =>
      Boolean(segment),
    )
    .sort((left, right) => right.bytes - left.bytes);
});

/** 无障碍与悬停提示：把构成说清楚。 */
const storageBarLabel = computed(() => {
  const segments = storageSegments.value;
  if (segments.length === 0) return "暂无文件占用";
  const parts = segments.map(
    (segment) => `${segment.label} ${segment.sizeLabel}`,
  );
  return `存储构成：${parts.join("，")}`;
});

/** 最近文件只需要图标与类型，这里补一个最小可用的 FileInfo。 */
function recentAsFileInfo(file: RecentFile) {
  return {
    id: file.path,
    name: file.name,
    path: file.path,
    kind: "file" as const,
    mime_type: file.mime_type ?? undefined,
    created_at: file.updated_at,
    updated_at: file.updated_at,
  };
}

/** 收藏条目的最小 FileInfo（只为取图标）。 */
function favoriteAsFileInfo(item: FavoriteEntry) {
  return {
    id: item.path,
    name: item.name,
    path: item.path,
    kind: item.kind,
    created_at: "",
  };
}

async function removeFavorite(item: FavoriteEntry) {
  try {
    await filesService.removeFavorite(item.path);
    favorites.value = favorites.value.filter((favorite) => favorite.path !== item.path);
    favoritesTotal.value = Math.max(0, favoritesTotal.value - 1);
    if (favoritesTotal.value === 0) {
      favoritesOffset.value = 0;
    } else {
      const lastOffset =
        Math.floor((favoritesTotal.value - 1) / FAVORITES_PAGE_SIZE) *
        FAVORITES_PAGE_SIZE;
      favoritesOffset.value = Math.min(favoritesOffset.value, lastOffset);
    }
    favoritesHasMore.value =
      favoritesOffset.value + favorites.value.length < favoritesTotal.value;
    emit("favorite-changed", item.path, false);
  } catch {
    // 取消失败时保持原样，下一次刷新会恢复
  }
}

async function fetchFavoritesPage(offset: number) {
  let page = await filesService.getFavorites(FAVORITES_PAGE_SIZE, offset);
  if (page.total > 0 && offset >= page.total) {
    const lastOffset =
      Math.floor((page.total - 1) / FAVORITES_PAGE_SIZE) * FAVORITES_PAGE_SIZE;
    page = await filesService.getFavorites(FAVORITES_PAGE_SIZE, lastOffset);
  }
  return page;
}

async function changeFavoritesPage(direction: -1 | 1) {
  const offset = Math.max(
    0,
    favoritesOffset.value + direction * FAVORITES_PAGE_SIZE,
  );
  favoritesLoading.value = true;
  try {
    const page = await fetchFavoritesPage(offset);
    favorites.value = page.items;
    favoritesTotal.value = page.total;
    favoritesOffset.value = page.offset;
    favoritesHasMore.value = page.has_more;
  } catch {
    // 页面切换失败时保留当前页，后续刷新可再次加载。
  } finally {
    favoritesLoading.value = false;
  }
}

async function load() {
  loading.value = true;
  error.value = "";

  // 两个接口各自独立降级：收藏接口失败（例如服务端缺少 favorites 表）时
  // 存储用量与最近更新仍应正常显示，反之亦然。
  const [overviewResult, favoritesResult] = await Promise.allSettled([
    filesService.getOverview(),
    fetchFavoritesPage(favoritesOffset.value),
  ]);

  if (overviewResult.status === "fulfilled") {
    overview.value = overviewResult.value;
  } else {
    // 概览是辅助信息：失败时只在本区域提示，不影响文件列表
    const reason = overviewResult.reason;
    error.value = reason instanceof Error ? reason.message : "加载概览失败";
  }

  if (favoritesResult.status === "fulfilled") {
    favorites.value = favoritesResult.value.items;
    favoritesTotal.value = favoritesResult.value.total;
    favoritesOffset.value = favoritesResult.value.offset;
    favoritesHasMore.value = favoritesResult.value.has_more;
  } else {
    favorites.value = [];
    favoritesTotal.value = 0;
    favoritesOffset.value = 0;
    favoritesHasMore.value = false;
  }

  loading.value = false;
}

onMounted(load);

watch(
  () => props.refreshKey,
  () => {
    void load();
  },
);
</script>

<style scoped>
.sidebar-overview {
  flex: 0 0 auto;
  /* 最高只占「侧栏高度 - 目录树最小高度(10rem)」：
     矮窗口下不再把树挤没或顶出侧栏，超出时概览自身滚动。 */
  min-height: 0;
  max-height: calc(100% - 10rem);
  overflow-y: auto;
  border-top: 1px solid var(--vf-border-weak);
  padding: 0.6rem 0.75rem 0.75rem;
}

.sidebar-overview-block + .sidebar-overview-block {
  margin-top: 0.75rem;
}

.sidebar-overview-title {
  font-size: 0.75rem;
  letter-spacing: 0.03em;
  text-transform: uppercase;
  color: var(--vf-text-subtle);
  margin-bottom: 0.35rem;
}

.sidebar-overview-usage {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
}

.sidebar-overview-size {
  font-size: 1rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.sidebar-overview-counts {
  font-size: 0.8rem;
  color: var(--vf-text-muted);
}

/* 分类占比条：细分段 + 圆角，与主流网盘的「存储空间」条一致 */
.storage-bar {
  display: flex;
  gap: 2px;
  height: 0.4rem;
  margin-top: 0.55rem;
  border-radius: var(--vf-radius-pill);
  overflow: hidden;
  background: var(--vf-surface-sunken);
}

.storage-bar-segment {
  height: 100%;
  border-radius: var(--vf-radius-pill);
  transition: width 0.2s var(--vf-motion-standard);
}

.storage-bar-segment.is-empty {
  width: 100%;
  border-radius: var(--vf-radius-pill);
  background: var(--vf-border-weak);
}

.storage-bar-segment.is-document,
.storage-legend-dot.is-document {
  background: var(--vf-chart-document);
}

.storage-bar-segment.is-image,
.storage-legend-dot.is-image {
  background: var(--vf-chart-image);
}

.storage-bar-segment.is-video,
.storage-legend-dot.is-video {
  background: var(--vf-chart-video);
}

.storage-bar-segment.is-audio,
.storage-legend-dot.is-audio {
  background: var(--vf-chart-audio);
}

.storage-bar-segment.is-other,
.storage-legend-dot.is-other {
  background: var(--vf-chart-other);
}

.storage-legend {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  margin: 0.5rem 0 0;
  list-style: none;
}

.storage-legend-item {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.75rem;
  color: var(--vf-text-muted);
}

.storage-legend-dot {
  flex: 0 0 auto;
  width: 0.5rem;
  height: 0.5rem;
  border-radius: var(--vf-radius-xs);
}

.storage-legend-label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.storage-legend-size {
  flex: 0 0 auto;
  color: var(--vf-text-subtle);
}

.sidebar-overview-error {
  font-size: 0.8rem;
  color: var(--vf-text-muted);
}

.sidebar-overview-retry {
  border: none;
  background: transparent;
  padding: 0;
  color: var(--vf-accent);
  cursor: pointer;
}

.sidebar-recent {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  margin: 0;
}

.sidebar-recent-item {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  width: 100%;
  padding: 0.2rem 0.35rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.8rem;
  text-align: left;
  cursor: pointer;
}

.sidebar-recent-item:hover {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.sidebar-recent-name {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sidebar-recent-time {
  flex: 0 0 auto;
  font-size: 0.75rem;
  color: var(--vf-text-subtle);
}

.sidebar-favorite-row {
  display: flex;
  align-items: center;
  gap: 0.15rem;
}

.sidebar-favorite-row .sidebar-recent-item {
  flex: 1 1 auto;
}

.sidebar-favorites-page {
  margin: 0 0 0.35rem;
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.sidebar-favorites-pagination {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
  margin-top: 0.4rem;
}

.sidebar-favorites-pagination button {
  padding: 0.2rem 0.45rem;
  border: 1px solid var(--vf-border);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface);
  color: var(--vf-text);
  font-size: 0.75rem;
  cursor: pointer;
}

.sidebar-favorites-pagination button:disabled {
  opacity: 0.5;
  cursor: default;
}

.sidebar-favorite-remove {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.5rem;
  height: 1.5rem;
  padding: 0;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-warning-text);
  cursor: pointer;
}

.sidebar-favorite-remove:hover {
  background: var(--vf-surface-hover);
}

/* reduced-motion：transform/width/all 过渡含位移或布局动画，降级为瞬时
   （色/透明/阴影类淡入不在此列 = 无位移风险）。 */
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
</style>

/* 非颜色区分（r153 ✓ WCAG 1.4.1 同 r152 式） */
.sidebar-overview-retry:hover,
.sidebar-overview-retry:focus-visible {
  text-decoration: underline;
}
