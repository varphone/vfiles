<template>
  <div class="audit-page">
    <div class="audit-card">
      <header class="audit-header">
        <div class="audit-header-titles">
          <h1 class="audit-title">审计日志</h1>
          <p class="audit-subtitle">
            共 {{ total }} 条记录
            <template v-if="items.length < total">
              · 当前显示 {{ items.length }} 条
            </template>
          </p>
        </div>

        <div class="audit-header-actions">
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh :size="15" :class="{ 'is-spinning': loading }" />
            <span>刷新</span>
          </button>
          <button class="vf-ghost-button" type="button" @click="goFiles">
            <IconFolderOpen :size="15" />
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

      <div class="audit-filters">
        <div class="audit-search">
          <IconSearch :size="15" class="audit-search-icon" />
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

        <button
          class="vf-ghost-button audit-apply"
          type="button"
          @click="applyFilters"
        >
          <IconFilter :size="15" />
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
            <IconRefresh :size="15" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <div v-else-if="loading && items.length === 0" class="audit-skeleton">
        <div v-for="row in 4" :key="row" class="audit-skeleton-row"></div>
      </div>

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
                <span class="audit-action">{{
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
  IconFilter,
  IconFolderOpen,
  IconHistory,
  IconLock,
  IconRefresh,
  IconSearch,
} from "@tabler/icons-vue";
import EmptyState from "../components/common/EmptyState.vue";
import { filesService } from "../services/files.service";
import { formatRelativeDate } from "../utils/filePresentation";
import type { AuditLogEntry } from "../types";

/**
 * 审计日志（管理员）：只读列表 + 筛选 + 分页。
 *
 * 服务端只提供查询接口，表中的记录由触发器保护，无法修改或删除。
 */
const router = useRouter();

const pageSize = 50;

const items = ref<AuditLogEntry[]>([]);
const total = ref(0);
const actions = ref<string[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);

const keyword = ref("");
const action = ref("");
const result = ref<"" | "success" | "failure">("");
const offset = ref(0);

const hasFilters = computed(() =>
  Boolean(keyword.value || action.value || result.value),
);
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
};

function actionLabel(value: string): string {
  return ACTION_LABELS[value] ?? value;
}

function formatAuditTime(value: string): string {
  if (!value) return "—";
  return formatRelativeDate(value);
}

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
    const page = await filesService.listAuditLogs({
      keyword: keyword.value,
      action: action.value,
      result: result.value,
      limit: pageSize,
      offset: offset.value,
    });
    items.value = page.items;
    total.value = page.total;
  } catch (e) {
    items.value = [];
    total.value = 0;
    error.value = e instanceof Error ? e.message : "加载失败";
  } finally {
    loading.value = false;
  }
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

.audit-card {
  max-width: 1200px;
  margin: 0 auto;
  padding: 1.1rem 1.2rem 1.3rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-lg);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
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

.audit-title {
  margin: 0;
  font-size: 1.15rem;
  font-weight: 700;
  color: var(--vf-text-strong);
}

.audit-subtitle {
  margin: 0.15rem 0 0;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
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

.audit-apply {
  min-height: 1.9rem;
  padding: 0 0.55rem;
  font-size: 0.78rem;
}

.audit-skeleton {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 1rem 0;
}

.audit-skeleton-row {
  height: 2.2rem;
  border-radius: var(--vf-radius-sm);
  background: linear-gradient(
    90deg,
    var(--vf-skeleton-base) 0%,
    var(--vf-skeleton-shine) 50%,
    var(--vf-skeleton-base) 100%
  );
  background-size: 200% 100%;
  animation: audit-shimmer 1.2s ease-in-out infinite;
}

@keyframes audit-shimmer {
  to {
    background-position: -200% 0;
  }
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
  display: inline-block;
  padding: 0.1rem 0.4rem;
  border-radius: 999px;
  background: var(--vf-surface-sunken);
  color: var(--vf-text);
  font-size: 0.74rem;
  white-space: nowrap;
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

@media screen and (max-width: 760px) {
  .audit-page {
    padding: 0.75rem 0.5rem 2rem;
  }

  .audit-search-input {
    width: 100%;
  }
}
</style>
