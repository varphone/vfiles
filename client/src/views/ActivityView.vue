<template>
  <FilesLayout title="活动">
    <template #sidebar>
      <ViewsSidebar />
    </template>

    <div class="activity-view">
      <PageHeader title="活动" description="仅显示你的操作记录">
        <template #actions>
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

      <div class="activity-toolbar">
        <div class="activity-filters" role="group" aria-label="活动筛选">
          <button
            v-for="chip in filterChips"
            :key="chip.value"
            class="activity-filter"
            :class="{ 'is-active': filter === chip.value }"
            type="button"
            :aria-pressed="filter === chip.value ? 'true' : 'false'"
            @click="setFilter(chip.value)"
          >
            {{ chip.label }}
          </button>
        </div>
        <span v-if="total > 0" class="activity-total">共 {{ total }} 条</span>
      </div>

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
        label="加载活动记录"
      />

      <EmptyState
        v-else-if="items.length === 0"
        :icon="IconActivity"
        title="还没有活动记录"
        hint="上传、下载、移动文件等操作会记录在这里"
      />

      <div v-else class="activity-groups">
        <section
          v-for="group in groups"
          :key="group.key"
          class="activity-group"
          :aria-label="group.label"
        >
          <header class="activity-group-head">
            <h2 class="activity-day">{{ group.label }}</h2>
            <span class="activity-day-count">{{ group.items.length }} 条</span>
          </header>

          <ul class="activity-list">
            <li
              v-for="entry in group.items"
              :key="entry.id"
              class="activity-item"
            >
              <Avatar
                class="activity-avatar"
                :name="entry.username || '系统'"
                size="sm"
              />
              <p class="activity-text">
                <span class="activity-user">{{
                  entry.username || "系统"
                }}</span>
                <span class="activity-action">{{
                  actionLabel(entry.action)
                }}</span>
                <span
                  v-if="entry.target"
                  class="activity-target"
                  :title="entry.target"
                >
                  {{ entry.target }}
                </span>
                <span
                  v-if="entry.result === 'failure'"
                  class="activity-failure"
                >
                  <span class="activity-failure-dot" aria-hidden="true"></span>
                  失败
                </span>
              </p>
              <time
                class="activity-time"
                :datetime="entry.created_at"
                :title="formatDate(entry.created_at)"
              >
                {{ formatRelativeDate(entry.created_at) }}
              </time>
            </li>
          </ul>
        </section>

        <div v-if="hasMore" class="activity-more">
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
        </div>
      </div>
    </div>
  </FilesLayout>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { IconActivity, IconAlertCircle, IconRefresh } from "@tabler/icons-vue";
import FilesLayout from "../layouts/FilesLayout.vue";
import ViewsSidebar from "../components/files/ViewsSidebar.vue";
import EmptyState from "../components/common/EmptyState.vue";
import SkeletonList from "../components/common/SkeletonList.vue";
import Avatar from "../ui/Avatar.vue";
import PageHeader from "../ui/PageHeader.vue";
import { filesService } from "../services/files.service";
import { useAppStore } from "../stores/app.store";
import { formatDate, formatRelativeDate } from "../utils/filePresentation";
import type { AuditLogEntry } from "../types";

/**
 * 「活动」视图：按天分组展示当前用户的操作记录。
 *
 * 服务端 `/api/activity` 只返回本人记录，因此页面不提供「他人活动」入口；
 * 顶部 chips 在「全部 / 文件变更」之间切换，列表按天分组并支持加载更多。
 */
type ActivityFilter = "all" | "files";
type ActivityGroup = {
  key: string;
  label: string;
  items: AuditLogEntry[];
};

const PAGE_SIZE = 30;

/** 活动流里的动作名（未收录的动作原样展示，避免新动作变成空白）。 */
const ACTION_LABELS: Record<string, string> = {
  "file.upload": "上传",
  "file.download": "下载",
  "file.delete": "删除",
  "file.move": "移动",
  "file.rename": "重命名",
  "directory.create": "新建目录",
  "share.create": "创建分享",
  "share.disable": "停用分享",
  "login.success": "登录",
};

const filterChips: Array<{ value: ActivityFilter; label: string }> = [
  { value: "all", label: "全部" },
  { value: "files", label: "文件变更" },
];

const appStore = useAppStore();

const filter = ref<ActivityFilter>("all");
const items = ref<AuditLogEntry[]>([]);
const total = ref(0);
const loading = ref(false);
const loadingMore = ref(false);
const error = ref<string | null>(null);

const hasMore = computed(() => items.value.length < total.value);

function actionLabel(action: string): string {
  return ACTION_LABELS[action] ?? action;
}

function entryTime(entry: AuditLogEntry): number {
  const parsed = new Date(entry.created_at).getTime();
  return Number.isNaN(parsed) ? Number.NEGATIVE_INFINITY : parsed;
}

function startOfDay(value: Date): Date {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate());
}

function dateKey(value: Date): string {
  return `${value.getFullYear()}-${value.getMonth() + 1}-${value.getDate()}`;
}

/** 组标题：今天 / 昨天 / `YYYY年M月D日`（本地时区）。 */
function dayLabel(value: Date, now: Date): string {
  const target = startOfDay(value).getTime();
  const today = startOfDay(now);
  if (target === today.getTime()) return "今天";

  const yesterday = new Date(
    today.getFullYear(),
    today.getMonth(),
    today.getDate() - 1,
  );
  if (target === yesterday.getTime()) return "昨天";

  return `${value.getFullYear()}年${value.getMonth() + 1}月${value.getDate()}日`;
}

/** 按天分组：先按时间倒序，再按首次出现的日期聚组，保证最新的一天在最前。 */
const groups = computed<ActivityGroup[]>(() => {
  const now = new Date();
  const sorted = [...items.value].sort(
    (left, right) => entryTime(right) - entryTime(left),
  );
  const grouped = new Map<string, ActivityGroup>();

  for (const entry of sorted) {
    const parsed = new Date(entry.created_at);
    const valid = !Number.isNaN(parsed.getTime());
    const key = valid ? dateKey(parsed) : "unknown";
    let group = grouped.get(key);
    if (!group) {
      group = {
        key,
        label: valid ? dayLabel(parsed, now) : "时间未知",
        items: [],
      };
      grouped.set(key, group);
    }
    group.items.push(entry);
  }

  return [...grouped.values()];
});

/** 重新拉取第一页；切换筛选和刷新都走这里。 */
async function reload() {
  loading.value = true;
  loadingMore.value = false;
  error.value = null;
  items.value = [];
  total.value = 0;

  try {
    const page = await filesService.getActivity({
      filter: filter.value,
      limit: PAGE_SIZE,
      offset: 0,
    });
    items.value = page.items;
    total.value = page.total;
  } catch (e) {
    error.value = e instanceof Error ? e.message : "加载活动记录失败";
  } finally {
    loading.value = false;
  }
}

async function loadMore() {
  if (!hasMore.value || loading.value || loadingMore.value) return;

  loadingMore.value = true;
  try {
    const page = await filesService.getActivity({
      filter: filter.value,
      limit: PAGE_SIZE,
      offset: items.value.length,
    });
    // 分页边界可能重复返回同一条记录，按 id 去重。
    const known = new Set(items.value.map((entry) => entry.id));
    items.value = [
      ...items.value,
      ...page.items.filter((entry) => !known.has(entry.id)),
    ];
    total.value = page.total;
  } catch (e) {
    appStore.error(e instanceof Error ? e.message : "加载更多活动失败");
  } finally {
    loadingMore.value = false;
  }
}

function setFilter(next: ActivityFilter) {
  if (filter.value === next) return;
  filter.value = next;
  void reload();
}

onMounted(() => {
  void reload();
});
</script>

<style scoped>
.activity-view {
  display: flex;
  flex-direction: column;
  gap: var(--vf-section-gap);
  min-width: 0;
}

/* 分组卡片之间同样是 16px 区块间距（原先靠 .activity-group + .activity-group 的 0.9rem 外边距） */
.activity-groups {
  display: flex;
  flex-direction: column;
  gap: var(--vf-section-gap);
  min-width: 0;
}

.activity-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--vf-space-4);
  flex-wrap: wrap;
}

.activity-filters {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-space-2);
  flex-wrap: wrap;
}

/* 分段 chips：3px 圆角、选中浅蓝底，与视图侧栏的选中态一致。 */
.activity-filter {
  height: var(--vf-control-height);
  padding: 0 var(--vf-space-3);
  border: 1px solid var(--vf-border-soft);
  border-radius: var(--vf-radius-control);
  background: var(--vf-surface);
  color: var(--vf-text-muted);
  font-size: 0.8125rem;
  font-weight: 500;
  cursor: pointer;
  transition:
    background-color 0.15s var(--vf-motion-standard),
    border-color 0.15s var(--vf-motion-standard),
    color 0.15s var(--vf-motion-standard);
}

.activity-filter:hover,
.activity-filter:focus-visible {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.activity-filter.is-active {
  border-color: var(--vf-accent-line);
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
  font-weight: 600;
}

.activity-total {
  color: var(--vf-text-subtle);
  font-size: 0.78rem;
}

/* 每天一张卡片：卡片 8px 圆角、行高 50px。 */
.activity-group {
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
  overflow: hidden;
}

.activity-group-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--vf-space-2);
  height: var(--vf-table-head-height);
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-sunken);
}

.activity-day {
  margin: 0;
  color: var(--vf-text-strong);
  font-size: 0.8125rem;
  font-weight: 650;
}

.activity-day-count {
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
}

.activity-list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.activity-item {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-height: var(--vf-row-height);
  padding: var(--vf-space-1) var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
}

.activity-item:last-child {
  border-bottom: 0;
}

.activity-item:hover {
  background: var(--vf-row-hover);
}

.activity-avatar {
  flex: none;
}

.activity-text {
  display: flex;
  align-items: baseline;
  gap: var(--vf-space-1);
  flex: 1 1 auto;
  min-width: 0;
  margin: 0;
  color: var(--vf-text);
  font-size: 0.875rem;
  line-height: 1.45;
}

.activity-user {
  flex: none;
  color: var(--vf-text-strong);
  font-weight: 600;
}

.activity-action {
  flex: none;
  color: var(--vf-text-muted);
}

.activity-target {
  min-width: 0;
  overflow: hidden;
  color: var(--vf-text-subtle);
  white-space: nowrap;
  text-overflow: ellipsis;
}

/* 失败标记：红点 + 红字，保持小尺寸不抢主信息。 */
.activity-failure {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-space-1);
  flex: none;
  color: var(--vf-danger-text);
  font-size: 0.72rem;
  font-weight: 600;
}

.activity-failure-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--vf-danger);
}

.activity-time {
  flex: none;
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
  white-space: nowrap;
}

.activity-more {
  display: flex;
  justify-content: center;
  padding: var(--vf-space-2) 0;
}

.is-spinning {
  animation: activity-spin 0.9s linear infinite;
}

@keyframes activity-spin {
  to {
    transform: rotate(360deg);
  }
}

@media screen and (max-width: 600px) {
  .activity-item {
    flex-wrap: wrap;
    row-gap: var(--vf-space-1);
    padding: var(--vf-space-2) var(--vf-space-3);
  }

  .activity-time {
    width: 100%;
    margin-left: calc(28px + var(--vf-space-2));
    text-align: left;
  }

  .activity-group-head {
    padding: 0 var(--vf-space-3);
  }
}

/* reduced-motion：循环动画降级为静态指示（M3: static loading indicators），
   本文件动画族 activity-spin 0.9s */
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
