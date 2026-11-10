<template>
  <FilesLayout title="收藏">
    <template #sidebar>
      <ViewsSidebar @search="goToSearch" />
    </template>

    <div class="favorites-view">
      <PageHeader title="收藏" description="你标记为收藏的文件与文件夹">
        <template #actions>
          <span v-if="total > 0" class="favorites-total">
            共 {{ total }} 项
          </span>
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh
              :size="16"
              :class="{ 'is-spinning': loading }"
              aria-hidden="true"
            />
            <span>刷新</span>
          </button>
        </template>
      </PageHeader>

      <EmptyState
        v-if="error"
        :icon="IconAlertCircle"
        tone="error"
        title="加载失败"
        :hint="error"
      >
        <template #actions>
          <button
            class="vf-ghost-button is-primary"
            type="button"
            @click="reload"
          >
            <IconRefresh :size="16" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <SkeletonList
        v-else-if="loading && items.length === 0"
        :rows="5"
        label="加载收藏"
      />

      <EmptyState
        v-else-if="items.length === 0"
        :icon="IconStar"
        title="还没有收藏"
        hint="在文件上右键，选择「加入收藏」即可在这里看到"
      />

      <section v-else class="favorites-panel" aria-label="收藏列表">
        <div class="favorites-table-wrap">
          <table class="favorites-table">
            <thead>
              <tr>
                <th scope="col">名称</th>
                <th scope="col" class="favorites-col-location">位置</th>
                <th scope="col" class="favorites-col-actions">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in items" :key="item.path">
                <td>
                  <div class="favorites-name-cell">
                    <FileTypeIcon :file="item" :size="20" />
                    <button
                      class="favorites-name"
                      type="button"
                      :title="item.path"
                      @click="openEntry(item)"
                    >
                      {{ item.name }}
                    </button>
                  </div>
                </td>
                <td
                  class="favorites-col-location favorites-location"
                  :title="item.path"
                >
                  {{ locationLabel(item.path) }}
                </td>
                <td class="favorites-col-actions">
                  <button
                    class="vf-ghost-button is-danger favorites-remove"
                    type="button"
                    :disabled="removingPath !== ''"
                    :aria-label="`取消收藏 ${item.name}`"
                    @click="remove(item)"
                  >
                    <IconStarFilled :size="15" aria-hidden="true" />
                    <span>{{
                      removingPath === item.path ? "取消中…" : "取消收藏"
                    }}</span>
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <footer v-if="hasMore" class="favorites-more">
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loadingMore"
            @click="loadMore"
          >
            <IconRefresh
              :size="16"
              :class="{ 'is-spinning': loadingMore }"
              aria-hidden="true"
            />
            <span>{{ loadingMore ? "加载中…" : "加载更多" }}</span>
          </button>
        </footer>
      </section>
    </div>
  </FilesLayout>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  IconAlertCircle,
  IconRefresh,
  IconStar,
  IconStarFilled,
} from "@tabler/icons-vue";
import FilesLayout from "../../layouts/FilesLayout.vue";
import ViewsSidebar from "../../components/files/ViewsSidebar.vue";
import EmptyState from "../../components/common/EmptyState.vue";
import SkeletonList from "../../components/common/SkeletonList.vue";
import FileTypeIcon from "../../components/file-browser/FileTypeIcon.vue";
import PageHeader from "../../ui/PageHeader.vue";
import { filesService } from "../../services/files.service";
import { useListResource } from "../../composables/useListResource";
import { useAppStore } from "../../stores/app.store";
import { parentDirectoryPath } from "../../utils/filePaths";
import type { FavoriteEntry } from "../../types";

/** 侧栏搜索：其它视图没有列表内搜索，统一跳到文件页并带上关键字。 */
function goToSearch(query: string) {
  const value = query.trim();
  if (!value) return;
  void router.push({ path: "/", query: { q: value } });
}

/**
 * 「收藏」视图：分页读取收藏条目，支持就地取消收藏。
 *
 * 点击条目回到文件列表（`/?path=所在目录`）；当前 Home 尚未消费该参数，
 * 先按约定带上，后续接入目录定位时无需改本页。
 */
const PAGE_SIZE = 50;

const router = useRouter();
const appStore = useAppStore();

const removingPath = ref("");

// 列表状态统一由 useListResource 管理（竞态防护 + 按路径去重）。
const resource = useListResource<FavoriteEntry>({
  pageSize: PAGE_SIZE,
  key: (item) => item.path,
  fetchPage: ({ limit, offset }) => filesService.getFavorites(limit, offset),
});
const items = resource.items;
const loading = resource.loading;
const loadingMore = resource.loadingMore;
const error = resource.error;
const hasMore = computed(() => resource.hasMore());
const total = resource.total;

async function reload() {
  await resource.reload();
}

async function loadMore() {
  await resource.loadMore();
}

/** 取消收藏成功后立即从列表移除，并同步总数与「加载更多」状态。 */
async function remove(item: FavoriteEntry) {
  if (removingPath.value !== "") return;

  removingPath.value = item.path;
  try {
    await filesService.removeFavorite(item.path);
    items.value = items.value.filter((entry: FavoriteEntry) => entry.path !== item.path);
    total.value = Math.max(0, total.value - 1);
    // 当前页被清空但服务端还有数据时，补一页避免停留在空列表上。
    if (items.value.length === 0 && resource.hasMore()) {
      await loadMore();
    }
  } catch (e) {
    appStore.error(e instanceof Error ? e.message : "取消收藏失败");
  } finally {
    removingPath.value = "";
  }
}

/** 跳回条目所在目录，目录通过 `query.path` 约定传递。 */
function openEntry(item: FavoriteEntry) {
  void router.push({
    path: "/",
    query: { path: parentDirectoryPath(item.path) },
  });
}

function locationLabel(path: string): string {
  const parent = parentDirectoryPath(path);
  return parent ? `/${parent}` : "/";
}

onMounted(() => {
  void reload();
});
</script>

<style scoped>
/* 页面在圆角内容面板内部滚动：面板本身 overflow: hidden。 */
.favorites-view {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  gap: var(--vf-section-gap);
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
}

.favorites-total {
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  white-space: nowrap;
}

/* 列表面板：卡片 8px 圆角、行高 50px，与文件列表的密度一致。 */
.favorites-panel {
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
  overflow: hidden;
}

.favorites-table-wrap {
  overflow-x: auto;
}

.favorites-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
}

.favorites-table th {
  height: var(--vf-table-head-height);
  padding: 0 var(--vf-cell-padding-x);
  background: var(--vf-surface-2);
  border-bottom: 1px solid var(--vf-border-weak);
  color: var(--vf-text-muted);
  font-size: 0.8125rem;
  font-weight: 600;
  text-align: left;
  white-space: nowrap;
  /* 与其它表格一致：垂直居中 + 固定行高（见 d0dc882） */
  vertical-align: middle;
  line-height: 1.35;
}

.favorites-table td {
  height: var(--vf-row-height);
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  color: var(--vf-text);
  font-size: 0.875rem;
}

.favorites-table tbody tr:last-child td {
  border-bottom: 0;
}

.favorites-table tbody tr:hover {
  background: var(--vf-row-hover);
}

.favorites-col-location {
  width: 30%;
}

.favorites-col-actions {
  width: 148px;
}

.favorites-table th.favorites-col-actions,
.favorites-table td.favorites-col-actions {
  text-align: right;
}

.favorites-name-cell {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-width: 0;
}

.favorites-name {
  min-width: 0;
  overflow: hidden;
  padding: 0;
  border: none;
  background: none;
  color: var(--vf-text);
  font-size: 0.875rem;
  text-align: left;
  white-space: nowrap;
  text-overflow: ellipsis;
  cursor: pointer;
}

.favorites-name:hover,
.favorites-name:focus-visible {
  color: var(--vf-accent-text);
  text-decoration: underline;
}

.favorites-location {
  overflow: hidden;
  color: var(--vf-text-muted);
  font-size: 0.8125rem;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.favorites-remove {
  gap: var(--vf-space-1);
  padding: 0 var(--vf-space-2);
  font-size: 0.8125rem;
}

.favorites-more {
  display: flex;
  justify-content: center;
  padding: var(--vf-space-2);
  border-top: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
}

.is-spinning {
  animation: favorites-spin 0.9s linear infinite;
}

@keyframes favorites-spin {
  to {
    transform: rotate(360deg);
  }
}

@media screen and (max-width: 600px) {
  .favorites-col-location {
    display: none;
  }

  .favorites-col-actions {
    width: 44px;
  }

  .favorites-remove span {
    display: none;
  }

  .favorites-table th,
  .favorites-table td {
    padding: 0 var(--vf-space-3);
  }
}

/* reduced-motion：循环动画降级为静态指示（M3: static loading indicators），
   本文件动画族 favorites-spin 0.9s */
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
