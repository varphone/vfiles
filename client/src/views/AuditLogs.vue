<template>
  <div class="audit-page">
    <div class="audit-card vf-page-card">
      <header class="audit-header">
        <div class="audit-header-titles">
          <h1 class="audit-title vf-page-title">审计日志</h1>
          <p class="audit-subtitle vf-page-subtitle">
            共 {{ total }} 条记录
            <template v-if="items.length < total">
              · 当前显示 {{ items.length }} 条
            </template>
            <template v-if="rangeLabel"> · {{ rangeLabel }}</template>
          </p>
        </div>

        <div class="audit-header-actions">
          <a
            class="vf-ghost-button"
            :href="exportHref"
            :class="{ 'is-disabled': total === 0 }"
            :aria-disabled="total === 0 ? 'true' : 'false'"
            download
            title="按当前筛选条件导出 CSV（只读导出）"
          >
            <IconDownload :size="16" />
            <span>导出 CSV</span>
          </a>
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh :size="16" :class="{ 'is-spinning': loading }" />
            <span>刷新</span>
          </button>
          <button class="vf-ghost-button" type="button" @click="goFiles">
            <IconFolderOpen :size="16" />
            <span>返回文件</span>
          </button>
        </div>
      </header>

      <!-- 只读声明：审计日志不可修改、不可删除（数据库层亦强制） -->
      <p class="audit-readonly">
        <IconLock :size="14" />
        <span>
          审计日志为只读记录：不能修改或删除，用于登录、上传、下载等操作的事后追溯。
        </span>
      </p>

      <div v-if="summary.total > 0" class="audit-summary">
        <div class="audit-summary-totals">
          <span class="audit-summary-total">
            共 <strong>{{ summary.total }}</strong> 条
          </span>
          <button
            v-if="summary.failures > 0"
            class="audit-summary-failures"
            type="button"
            :title="'只看失败记录'"
            @click="showFailuresOnly"
          >
            失败 <strong>{{ summary.failures }}</strong> 条
          </button>
        </div>

        <div v-if="summary.users.length > 0" class="audit-summary-group">
          <span class="audit-summary-label">用户</span>
          <button
            v-for="item in summary.users"
            :key="item.key"
            class="vf-ghost-button audit-summary-chip"
            type="button"
            :title="`只看「${item.key}」的记录`"
            @click="filterByUser(item.key)"
          >
            <span>{{ item.key }}</span>
            <span class="audit-summary-count">{{ item.count }}</span>
          </button>
        </div>

        <div v-if="summary.actions.length > 0" class="audit-summary-group">
          <span class="audit-summary-label">动作</span>
          <button
            v-for="item in summary.actions"
            :key="item.key"
            class="vf-ghost-button audit-summary-chip"
            type="button"
            :title="`只看「${actionLabel(item.key)}」`"
            @click="filterByAction(item.key)"
          >
            <span>{{ actionLabel(item.key) }}</span>
            <span class="audit-summary-count">{{ item.count }}</span>
          </button>
        </div>
      </div>

      <div class="audit-filters">
        <div class="audit-search">
          <IconSearch :size="16" class="audit-search-icon" />
          <input
            v-model.trim="keyword"
            class="input is-small audit-search-input"
            type="search"
            placeholder="搜索用户名或 IP"
            aria-label="搜索用户名或 IP"
            @keyup.enter="applyFilters"
          />
        </div>

        <div class="select is-small">
          <select
            v-model="action"
            aria-label="按动作筛选"
            @change="applyFilters"
          >
            <option value="">全部动作</option>
            <option v-for="item in actions" :key="item" :value="item">
              {{ actionLabel(item) }}
            </option>
          </select>
        </div>

        <div class="select is-small">
          <select
            v-model="result"
            aria-label="按结果筛选"
            @change="applyFilters"
          >
            <option value="">全部结果</option>
            <option value="success">成功</option>
            <option value="failure">失败</option>
          </select>
        </div>

        <div class="select is-small">
          <select
            v-model="range"
            aria-label="按时间范围筛选"
            @change="onRangeChange"
          >
            <option value="all">全部时间</option>
            <option value="today">今天</option>
            <option value="7d">近 7 天</option>
            <option value="30d">近 30 天</option>
            <option value="custom">自定义…</option>
          </select>
        </div>

        <template v-if="range === 'custom'">
          <label class="audit-date">
            <span>从</span>
            <input
              v-model="customSince"
              class="input is-small audit-date-input"
              type="date"
              aria-label="开始日期"
              @change="applyFilters"
            />
          </label>
          <label class="audit-date">
            <span>到</span>
            <input
              v-model="customUntil"
              class="input is-small audit-date-input"
              type="date"
              aria-label="结束日期"
              @change="applyFilters"
            />
          </label>
        </template>

        <button
          class="vf-ghost-button audit-apply"
          type="button"
          @click="applyFilters"
        >
          <IconFilter :size="16" />
          <span>筛选</span>
        </button>
        <button
          v-if="hasFilters"
          class="vf-ghost-button audit-apply"
          type="button"
          @click="clearFilters"
        >
          <span>清除筛选</span>
        </button>
      </div>

      <EmptyState
        v-if="error"
        :icon="IconAlertCircle"
        tone="error"
        title="加载失败"
        :hint="error"
      >
        <template #actions>
          <button class="vf-ghost-button is-primary" @click="reload">
            <IconRefresh :size="16" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <SkeletonList
        v-else-if="loading && items.length === 0"
        :rows="4"
        label="加载审计日志"
      />

      <EmptyState
        v-else-if="items.length === 0"
        :icon="IconHistory"
        title="暂无审计记录"
        :hint="
          hasFilters
            ? '当前筛选条件下没有记录，可清除筛选后再看'
            : '登录、上传、下载等操作会自动记录在这里'
        "
      >
        <template v-if="hasFilters" #actions>
          <button class="vf-ghost-button" @click="clearFilters">
            <span>清除筛选</span>
          </button>
        </template>
      </EmptyState>

      <div v-else class="audit-table-wrap">
        <table class="table is-fullwidth is-hoverable audit-table">
          <thead>
            <tr>
              <th class="is-narrow">时间</th>
              <th class="is-narrow">用户</th>
              <th class="is-narrow">动作</th>
              <th>对象</th>
              <th class="is-narrow">结果</th>
              <th class="is-narrow">IP</th>
              <th class="is-narrow">设备</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="entry in items" :key="entry.id">
              <td class="is-narrow audit-time" :title="entry.created_at">
                {{ formatAuditTime(entry.created_at) }}
              </td>
              <td class="is-narrow">
                <span class="audit-user">{{ entry.username || "匿名" }}</span>
              </td>
              <td class="is-narrow">
                <span class="audit-action vf-status-pill">{{
                  actionLabel(entry.action)
                }}</span>
              </td>
              <td class="audit-target">
                <span class="audit-target-text" :title="entry.target || ''">
                  {{ entry.target || "—" }}
                </span>
                <span v-if="entry.detail" class="audit-detail">
                  {{ entry.detail }}
                </span>
              </td>
              <td class="is-narrow">
                <span
                  class="audit-result"
                  :class="
                    entry.result === 'success' ? 'is-success' : 'is-failure'
                  "
                >
                  <span class="audit-result-dot" aria-hidden="true"></span>
                  <span>{{
                    entry.result === "success" ? "成功" : "失败"
                  }}</span>
                </span>
              </td>
              <td class="is-narrow audit-ip">{{ entry.ip || "—" }}</td>
              <td
                class="is-narrow audit-device"
                :title="entry.user_agent || ''"
              >
                {{ entry.device || "—" }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <footer v-if="total > pageSize" class="audit-pager">
        <button
          class="vf-ghost-button"
          type="button"
          :disabled="offset === 0 || loading"
          @click="goPrevPage"
        >
          上一页
        </button>
        <span class="audit-pager-info">
          第 {{ pageNumber }} / {{ pageCount }} 页
        </span>
        <button
          class="vf-ghost-button"
          type="button"
          :disabled="offset + pageSize >= total || loading"
          @click="goNextPage"
        >
          下一页
        </button>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  IconAlertCircle,
  IconDownload,
  IconFilter,
  IconFolderOpen,
  IconHistory,
  IconLock,
  IconRefresh,
  IconSearch,
} from "@tabler/icons-vue";
import EmptyState from "../components/common/EmptyState.vue";
import SkeletonList from "../components/common/SkeletonList.vue";
import { filesService } from "../services/files.service";
import { formatRelativeDate } from "../utils/filePresentation";
import type { AuditLogEntry, AuditLogSummary } from "../types";

/**
 * 审计日志（管理员）：只读列表 + 筛选 + 分页。
 *
 * 服务端只提供查询接口，表中的记录由触发器保护，无法修改或删除。
 */
const router = useRouter();

const pageSize = 50;

const items = ref<AuditLogEntry[]>([]);
const emptySummary = (): AuditLogSummary => ({
  total: 0,
  failures: 0,
  users: [],
  actions: [],
});
const summary = ref<AuditLogSummary>(emptySummary());
const total = ref(0);
const actions = ref<string[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);

const keyword = ref("");
const action = ref("");
const result = ref<"" | "success" | "failure">("");
const offset = ref(0);

/** 时间范围预设（主流审计界面常见选项）。 */
type RangePreset = "all" | "today" | "7d" | "30d" | "custom";
const range = ref<RangePreset>("all");
const customSince = ref("");
const customUntil = ref("");

/** 范围起止：按本地时区计算；`until` 取次日 0 点，保证包含结束当天。 */
function rangeBounds(): { since?: string; until?: string } {
  const startOfDay = (date: Date) =>
    new Date(date.getFullYear(), date.getMonth(), date.getDate());

  if (range.value === "today") {
    return { since: startOfDay(new Date()).toISOString() };
  }
  if (range.value === "7d" || range.value === "30d") {
    const days = range.value === "7d" ? 7 : 30;
    const from = startOfDay(new Date());
    from.setDate(from.getDate() - (days - 1));
    return { since: from.toISOString() };
  }
  if (range.value === "custom") {
    const since = customSince.value
      ? new Date(`${customSince.value}T00:00:00`).toISOString()
      : undefined;
    let until: string | undefined;
    if (customUntil.value) {
      const end = new Date(`${customUntil.value}T00:00:00`);
      end.setDate(end.getDate() + 1);
      until = end.toISOString();
    }
    return { since, until };
  }
  return {};
}

function onRangeChange() {
  if (range.value === "custom" && !customSince.value) {
    // 默认填最近 7 天，少一次手工输入
    const from = new Date();
    from.setDate(from.getDate() - 6);
    customSince.value = from.toISOString().slice(0, 10);
  }
  applyFilters();
}

const hasFilters = computed(
  () =>
    Boolean(keyword.value || action.value || result.value) ||
    range.value !== "all",
);
const rangeLabel = computed(() => {
  switch (range.value) {
    case "today":
      return "今天";
    case "7d":
      return "近 7 天";
    case "30d":
      return "近 30 天";
    case "custom":
      return `${customSince.value || "最早"} ~ ${customUntil.value || "现在"}`;
    default:
      return "";
  }
});

const pageNumber = computed(() => Math.floor(offset.value / pageSize) + 1);
const pageCount = computed(() =>
  Math.max(1, Math.ceil(total.value / pageSize)),
);

const ACTION_LABELS: Record<string, string> = {
  "login.success": "登录成功",
  "login.failure": "登录失败",
  "auth.logout": "退出登录",
  "file.upload": "上传",
  "file.download": "下载",
  "file.delete": "删除",
  "file.move": "移动",
  "file.rename": "重命名",
  "file.restore": "恢复版本",
  "directory.create": "新建文件夹",
  "share.create": "创建分享",
  "share.disable": "停止分享",
  "share.download": "分享下载",
  "user.create": "创建用户",
  "user.update": "更新用户",
  "user.sessions_revoke": "强制下线",
  "audit.export": "导出审计日志",
};

function actionLabel(value: string): string {
  return ACTION_LABELS[value] ?? value;
}

function formatAuditTime(value: string): string {
  if (!value) return "—";
  return formatRelativeDate(value);
}

/** 导出链接：带上当前筛选条件，服务端按条件导出（最多 1 万条）。 */
const exportHref = computed(() => {
  const params = new URLSearchParams();
  if (keyword.value) params.set("keyword", keyword.value);
  if (action.value) params.set("action", action.value);
  if (result.value) params.set("result", result.value);
  const bounds = rangeBounds();
  if (bounds.since) params.set("since", bounds.since);
  if (bounds.until) params.set("until", bounds.until);
  const query = params.toString();
  return `/api/audit/logs.csv${query ? `?${query}` : ""}`;
});

function goFiles() {
  router.push({ path: "/" });
}

function applyFilters() {
  offset.value = 0;
  void reload();
}

function clearFilters() {
  keyword.value = "";
  action.value = "";
  result.value = "";
  range.value = "all";
  customSince.value = "";
  customUntil.value = "";
  summary.value = emptySummary();
  applyFilters();
}

function goPrevPage() {
  offset.value = Math.max(0, offset.value - pageSize);
  void reload();
}

function goNextPage() {
  offset.value += pageSize;
  void reload();
}

async function reload() {
  loading.value = true;
  error.value = null;
  try {
    const bounds = rangeBounds();
    const page = await filesService.listAuditLogs({
      keyword: keyword.value,
      action: action.value,
      result: result.value,
      since: bounds.since,
      until: bounds.until,
      limit: pageSize,
      offset: offset.value,
    });
    items.value = page.items;
    total.value = page.total;
    void loadSummary();
  } catch (e) {
    items.value = [];
    total.value = 0;
    error.value = e instanceof Error ? e.message : "加载失败";
  } finally {
    loading.value = false;
  }
}

/** 概览与列表一起刷新（同一组筛选条件）。 */
async function loadSummary() {
  try {
    summary.value = await filesService.getAuditSummary({
      keyword: keyword.value,
      action: action.value,
      result: result.value,
      ...rangeBounds(),
    });
  } catch {
    // 概览失败不影响主列表
    summary.value = emptySummary();
  }
}

/** 点击概览里的用户 / 动作即筛选。 */
function filterByUser(key: string) {
  keyword.value = key === "(匿名)" ? "" : key;
  offset.value = 0;
  void reload();
}

function filterByAction(key: string) {
  action.value = key;
  offset.value = 0;
  void reload();
}

function showFailuresOnly() {
  result.value = "failure";
  offset.value = 0;
  void reload();
}

async function loadActions() {
  try {
    actions.value = await filesService.listAuditActions();
  } catch {
    // 动作下拉失败不影响主列表
    actions.value = [];
  }
}

onMounted(() => {
  void reload();
  void loadActions();
});
</script>

<style scoped>
.audit-page {
  padding: 1.25rem 1rem 3rem;
}

.audit-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.8rem;
  flex-wrap: wrap;
  padding-bottom: 0.8rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.audit-header-actions {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
}

.audit-readonly {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin: 0.75rem 0 0;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  font-size: 0.78rem;
}

/* 概览条：总量/失败 + Top 用户/动作，点击即筛选 */
.audit-summary {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
  padding: 0.6rem 0.65rem;
  margin-top: 0.7rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
  font-size: 0.78rem;
}

.audit-summary-totals {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  color: var(--vf-text-muted);
}

.audit-summary-total strong {
  color: var(--vf-text-strong);
}

.audit-summary-failures {
  padding: 0;
  border: none;
  background: none;
  color: var(--vf-danger-text);
  font-size: 0.78rem;
  cursor: pointer;
}

.audit-summary-failures:hover {
  text-decoration: underline;
}

.audit-summary-group {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-wrap: wrap;
  min-width: 0;
}

.audit-summary-label {
  color: var(--vf-text-subtle);
  font-size: 0.74rem;
}

.audit-summary-chip {
  gap: 0.3rem;
  min-height: 1.6rem;
  padding: 0 0.45rem;
  font-size: 0.75rem;
}

.audit-summary-count {
  color: var(--vf-text-subtle);
  font-size: 0.7rem;
}

.audit-filters {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
  padding: 0.7rem 0 0.2rem;
}

.audit-search {
  position: relative;
  display: flex;
  align-items: center;
}

.audit-search-icon {
  position: absolute;
  left: 0.5rem;
  color: var(--vf-text-subtle);
  pointer-events: none;
}

.audit-search-input {
  width: min(16rem, 60vw);
  padding-left: 1.75rem;
  font-size: 0.8rem;
}

/* 导出按钮与其它按钮一致，禁用时降低不透明度 */
.audit-header-actions .vf-ghost-button {
  min-height: 1.9rem;
  padding: 0 0.55rem;
  font-size: 0.8rem;
  text-decoration: none;
}

.audit-header-actions .vf-ghost-button.is-disabled {
  opacity: 0.5;
  pointer-events: none;
}

.audit-date {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  color: var(--vf-text-muted);
  font-size: 0.78rem;
}

.audit-date-input {
  width: 8.5rem;
  font-size: 0.78rem;
}

.audit-apply {
  min-height: 1.9rem;
  padding: 0 0.55rem;
  font-size: 0.78rem;
}

.audit-table-wrap {
  margin-top: 0.5rem;
  overflow-x: auto;
}

.audit-table {
  font-size: 0.82rem;
}

.audit-table thead th {
  border-bottom: 1px solid var(--vf-border-weak);
  color: var(--vf-text-muted);
  font-size: 0.74rem;
  font-weight: 600;
  white-space: nowrap;
}

.audit-table tbody tr:hover {
  background: var(--vf-surface-hover);
}

.audit-time {
  color: var(--vf-text-muted);
  white-space: nowrap;
}

.audit-user {
  color: var(--vf-text-strong);
  font-weight: 600;
}

.audit-action {
  background: var(--vf-surface-sunken);
  color: var(--vf-text);
}

.audit-target {
  max-width: 22rem;
}

.audit-target-text {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--vf-text);
}

.audit-detail {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
}

.audit-result {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  white-space: nowrap;
}

.audit-result-dot {
  width: 0.45rem;
  height: 0.45rem;
  border-radius: 50%;
}

.audit-result.is-success {
  color: var(--vf-success-text);
}

.audit-result.is-success .audit-result-dot {
  background: var(--vf-success-text);
}

.audit-result.is-failure {
  color: var(--vf-danger-text);
}

.audit-result.is-failure .audit-result-dot {
  background: var(--vf-danger-text);
}

.audit-ip {
  color: var(--vf-text-muted);
  white-space: nowrap;
}

.audit-device {
  color: var(--vf-text-muted);
  white-space: nowrap;
}

.audit-pager {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  padding-top: 0.8rem;
  border-top: 1px solid var(--vf-border-weak);
}

.audit-pager-info {
  color: var(--vf-text-muted);
  font-size: 0.78rem;
}

.is-spinning {
  animation: audit-spin 0.9s linear infinite;
}

@keyframes audit-spin {
  to {
    transform: rotate(360deg);
  }
}

@media screen and (max-width: 768px) {
  .audit-page {
    padding: 0.75rem 0.5rem 2rem;
  }

  .audit-search-input {
    width: 100%;
  }
}

/* reduced-motion：循环动画降级为静态指示（M3: static loading indicators），
   本文件动画族 audit-spin 0.9s */
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
