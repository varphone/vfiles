<template>
  <nav class="views-sidebar" aria-label="视图">
    <ul class="views-sidebar-list">
      <li v-for="item in items" :key="item.key">
        <RouterLink
          class="views-sidebar-item"
          :class="{ 'is-active': isActive(item.to) }"
          :to="item.to"
          :aria-current="isActive(item.to) ? 'page' : undefined"
        >
          <component :is="item.icon" :size="18" :stroke-width="1.7" />
          <span class="views-sidebar-label">{{ item.label }}</span>
        </RouterLink>
      </li>
    </ul>

    <!-- 存储用量：受限会话看不到工作区统计，整块隐藏。 -->
    <section
      v-if="showStorage"
      class="views-sidebar-storage"
      aria-label="存储用量"
    >
      <p class="views-sidebar-storage-title">存储用量</p>
      <template v-if="storage">
        <p class="views-sidebar-storage-size">
          {{ formatSize(storage.total_bytes) }}
        </p>
        <p class="views-sidebar-storage-meta">
          {{ storage.file_count }} 文件 · {{ storage.directory_count }} 目录
        </p>
      </template>
      <p v-else class="views-sidebar-storage-meta">
        {{ storageError ? "用量暂不可用" : "加载中…" }}
      </p>
    </section>
  </nav>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, type Component } from "vue";
import { useRoute } from "vue-router";
import {
  IconActivity,
  IconClock,
  IconFolder,
  IconShare,
  IconStar,
} from "@tabler/icons-vue";
import { filesService } from "../../services/files.service";
import { useAuthStore } from "../../stores/auth.store";
import { formatSize } from "../../utils/filePresentation";
import type { WorkspaceOverview } from "../../types";

/**
 * Nextcloud 式视图侧栏：全部文件 / 最近 / 收藏 / 分享 / 活动。
 *
 * 每行 40px（与 50px 的文件行区分），选中态浅蓝底 + 蓝字；
 * 底部固定「存储用量」，数据取 `filesService.getOverview()`。
 */
type ViewItem = {
  key: string;
  to: string;
  label: string;
  icon: Component;
};

const auth = useAuthStore();
const route = useRoute();

const storage = ref<WorkspaceOverview | null>(null);
const storageError = ref(false);

/** 受限会话不展示工作区统计（与账号级入口同策略）。 */
const showStorage = computed(() => !auth.isRestricted);

const items = computed<ViewItem[]>(() => {
  const entries: ViewItem[] = [
    { key: "all", to: "/", label: "全部文件", icon: IconFolder },
    { key: "recent", to: "/recent", label: "最近", icon: IconClock },
    { key: "favorites", to: "/favorites", label: "收藏", icon: IconStar },
  ];

  // 分享视图依赖分享功能且不适用于受限会话（路由守卫同样会回首页）。
  if (!auth.isRestricted && auth.features?.shareEnabled !== false) {
    entries.push({
      key: "shares",
      to: "/shares",
      label: "分享",
      icon: IconShare,
    });
  }

  entries.push({
    key: "activity",
    to: "/activity",
    label: "活动",
    icon: IconActivity,
  });
  return entries;
});

/** 「全部文件」只在根视图高亮，其余按前缀匹配子路径。 */
function isActive(to: string): boolean {
  if (to === "/") return route.path === "/";
  return route.path === to || route.path.startsWith(`${to}/`);
}

async function loadOverview() {
  if (!showStorage.value) return;
  storageError.value = false;
  try {
    storage.value = await filesService.getOverview();
  } catch {
    storageError.value = true;
  }
}

onMounted(() => {
  void loadOverview();
});
</script>

<style scoped>
.views-sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  padding: 0.5rem 0.4rem 0.4rem;
}

.views-sidebar-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  margin: 0;
  padding: 0;
  list-style: none;
}

/* 视图行：40px 高、图标 + 文字、选中浅蓝底。 */
.views-sidebar-item {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  height: 40px;
  padding: 0 0.65rem;
  border-radius: 3px;
  color: var(--vf-text);
  font-size: 0.875rem;
  text-decoration: none;
  white-space: nowrap;
}

.views-sidebar-item:hover,
.views-sidebar-item:focus-visible {
  background: var(--vf-row-hover);
  color: var(--vf-text-strong);
}

.views-sidebar-item.is-active {
  background: var(--vf-row-selected);
  color: var(--vf-accent-text);
  font-weight: 500;
}

.views-sidebar-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.views-sidebar-storage {
  margin-top: auto;
  padding: 0.85rem 0.65rem 0.5rem;
  border-top: 1px solid var(--vf-border-weak);
}

.views-sidebar-storage-title {
  margin: 0 0 0.45rem;
  color: var(--vf-text-muted);
  font-size: 0.75rem;
  font-weight: 600;
  letter-spacing: 0.02em;
}

.views-sidebar-storage-size {
  margin: 0;
  color: var(--vf-text-strong);
  font-size: 1.05rem;
  font-weight: 650;
}

.views-sidebar-storage-meta {
  margin: 0.15rem 0 0;
  color: var(--vf-text-muted);
  font-size: 0.75rem;
}
</style>
