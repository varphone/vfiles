<template>
  <section class="sidebar-overview" aria-label="工作区概览">
    <div v-if="loading && !overview" class="sidebar-overview-block">
      <p class="sidebar-overview-title">存储用量</p>
      <div class="sidebar-skeleton-line"></div>
      <div class="sidebar-skeleton-line is-short"></div>
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
    </div>

    <div v-if="favorites.length > 0" class="sidebar-overview-block">
      <p class="sidebar-overview-title">收藏</p>
      <ul class="sidebar-recent">
        <li v-for="item in favorites" :key="item.path">
          <div class="sidebar-favorite-row">
            <button
              class="sidebar-recent-item"
              type="button"
              :title="item.path"
              @click="emit('open-favorite', item)"
            >
              <FileTypeIcon :file="favoriteAsFileInfo(item)" :size="15" />
              <span class="sidebar-recent-name">{{ item.name }}</span>
            </button>
            <button
              class="sidebar-favorite-remove"
              type="button"
              :aria-label="`取消收藏 ${item.name}`"
              title="取消收藏"
              @click="removeFavorite(item)"
            >
              <IconStarFilled :size="13" />
            </button>
          </div>
        </li>
      </ul>
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
            <FileTypeIcon :file="recentAsFileInfo(file)" :size="15" />
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
import type { FavoriteEntry, RecentFile, WorkspaceOverview } from "../../types";
import { IconStarFilled } from "@tabler/icons-vue";
import { formatRelativeDate, formatSize } from "../../utils/filePresentation";
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
  (e: "favorites-changed", entries: FavoriteEntry[]): void;
}>();

const overview = ref<WorkspaceOverview | null>(null);
const favorites = ref<FavoriteEntry[]>([]);
const loading = ref(false);
const error = ref("");

const formattedSize = computed(() =>
  formatSize(overview.value?.total_bytes ?? 0),
);

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
    favorites.value = await filesService.removeFavorite(item.path);
    emit("favorites-changed", favorites.value);
  } catch {
    // 取消失败时保持原样，下一次刷新会恢复
  }
}

async function load() {
  loading.value = true;
  error.value = "";
  try {
    const [nextOverview, nextFavorites] = await Promise.all([
      filesService.getOverview(),
      filesService.getFavorites(),
    ]);
    overview.value = nextOverview;
    favorites.value = nextFavorites;
    emit("favorites-changed", nextFavorites);
  } catch (err) {
    // 概览是辅助信息：失败时只在本区域提示，不影响文件列表
    error.value = err instanceof Error ? err.message : "加载概览失败";
  } finally {
    loading.value = false;
  }
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
  border-top: 1px solid var(--vf-border-weak);
  padding: 0.6rem 0.75rem 0.75rem;
}

.sidebar-overview-block + .sidebar-overview-block {
  margin-top: 0.75rem;
}

.sidebar-overview-title {
  font-size: 0.72rem;
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
  font-size: 0.76rem;
  color: var(--vf-text-muted);
}

.sidebar-overview-error {
  font-size: 0.76rem;
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
  font-size: 0.78rem;
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
  font-size: 0.7rem;
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

.sidebar-skeleton-line {
  height: 0.7rem;
  margin-bottom: 0.35rem;
  border-radius: 4px;
  background: var(--vf-skeleton-base);
}

.sidebar-skeleton-line.is-short {
  width: 60%;
}
</style>
