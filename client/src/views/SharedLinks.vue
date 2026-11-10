<template>
  <SettingsLayout
    title="我的分享"
    description="管理公开分享链接，查看访问次数与过期时间。"
    active="shares"
  >
    <template #actions>
      <div class="shares-header-actions">
          <button
            v-if="expiredShares.length > 0"
            class="vf-ghost-button is-danger"
            type="button"
            :disabled="clearingExpired || loading"
            @click="clearExpired"
          >
            <IconTrash :size="16" />
            <span>
              {{
                clearingExpired
                  ? `清理中 ${clearedCount}/${expiredShares.length}`
                  : `清理已加载的过期链接（${expiredShares.length}）`
              }}
            </span>
          </button>
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh :size="16" :class="{ 'is-spinning': loading }" />
            <span>刷新</span>
          </button>
        </div>
    </template>

    <div class="settings-content">
      <section class="settings-table-panel" aria-label="分享链接列表">

      <p v-if="featureDisabled" class="shares-note">
        <IconInfoCircle :size="14" />
        <span>当前服务未启用分享功能，无法管理分享链接。</span>
      </p>

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
        v-else-if="loading && shares.length === 0"
        class="settings-table-state"
        :rows="3"
        label="加载分享链接"
      />

      <EmptyState
        v-else-if="shares.length === 0"
        :icon="IconLink"
        title="还没有分享链接"
        hint="在文件上右键或打开详情面板，选择「分享」即可生成链接"
      >
        <template #actions>
          <button class="vf-ghost-button is-primary" @click="goFiles">
            <IconFolderOpen :size="16" />
            <span>去选择文件</span>
          </button>
        </template>
      </EmptyState>

      <template v-if="shares.length > 0">
        <div class="shares-filters">
          <button
            v-for="chip in filterChips"
            :key="chip.key"
            class="vf-ghost-button shares-filter"
            :class="{ 'is-active': statusFilter === chip.key }"
            type="button"
            :aria-pressed="statusFilter === chip.key ? 'true' : 'false'"
            @click="statusFilter = chip.key"
          >
            <component :is="chip.icon" :size="14" />
            <span>{{ chip.label }}</span>
            <span class="shares-filter-count">{{ chip.count }}</span>
          </button>
        </div>

        <p v-if="hasMore" class="shares-page-hint">
          状态筛选和过期链接清理仅作用于已加载的分享链接。
        </p>

        <EmptyState
          v-if="visibleShares.length === 0"
          :icon="IconFilter"
          :title="emptyFilterTitle"
          hint="换一个筛选条件即可看到其它链接"
        >
          <template #actions>
            <button class="vf-ghost-button" @click="statusFilter = 'all'">
              <span>查看全部</span>
            </button>
          </template>
        </EmptyState>

        <div v-else-if="isCompact" class="shares-cards">
          <article
            v-for="share in visibleShares"
            :key="share.id"
            class="shares-card"
            :class="{ 'is-expired': isExpired(share) }"
          >
            <header class="shares-card-head">
              <span class="shares-icon" aria-hidden="true">
                <FileTypeIcon :file="iconSource(share)" :size="20" />
              </span>
              <div class="shares-card-titles">
                <span class="shares-name" :title="share.entry_name">
                  {{ share.entry_name }}
                </span>
                <span class="shares-path" :title="share.entry_path">
                  /{{ share.entry_path }}
                </span>
              </div>
              <Badge
                class="shares-badge"
                :class="statusClass(share)"
                :tone="isExpired(share) ? 'muted' : 'success'"
                size="sm"
              >
                {{ statusLabel(share) }}
              </Badge>
            </header>

            <dl class="shares-card-meta">
              <div>
                <dt>类型</dt>
                <dd>{{ kindLabel(share) }}</dd>
              </div>
              <div>
                <dt>访问次数</dt>
                <dd>访问 {{ share.access_count }} 次</dd>
              </div>
              <div>
                <dt>创建时间</dt>
                <dd>{{ formatRelativeDate(share.created_at) }}</dd>
              </div>
              <div>
                <dt>过期时间</dt>
                <dd>{{ expiryLabel(share) }}</dd>
              </div>
            </dl>

            <div class="shares-link-row">
              <input
                class="input is-small shares-link-input"
                type="text"
                :value="shareUrl(share)"
                readonly
                :aria-label="`${share.entry_name} 的分享链接`"
                @focus="selectLink"
              />
            </div>

            <div class="shares-card-actions">
              <button
                class="vf-ghost-button shares-action"
                type="button"
                @click="copyLink(share)"
              >
                <IconCheck v-if="copiedCode === share.code" :size="16" />
                <IconCopy v-else :size="16" />
                <span>{{ copiedCode === share.code ? "已复制" : "复制" }}</span>
              </button>
              <button
                class="vf-ghost-button shares-action"
                type="button"
                @click="openLink(share)"
              >
                <IconExternalLink :size="16" />
                <span>打开</span>
              </button>
              <button
                class="vf-ghost-button is-danger shares-action"
                type="button"
                :disabled="disablingCode === share.code"
                @click="stopSharing(share)"
              >
                <IconTrash :size="16" />
                <span>{{ disablingCode === share.code ? "停止中…" : "停止" }}</span>
              </button>
            </div>
          </article>
        </div>

        <div v-else class="shares-table-wrap">
          <table class="shares-table">
            <thead>
              <tr>
                <th scope="col" class="shares-col-file">文件</th>
                <th scope="col" class="shares-col-kind">类型</th>
                <th scope="col" class="shares-col-status">状态</th>
                <th scope="col" class="shares-col-visits">访问次数</th>
                <th scope="col" class="shares-col-created">创建时间</th>
                <th scope="col" class="shares-col-expiry">过期时间</th>
                <th scope="col" class="shares-col-actions">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="share in visibleShares"
                :key="share.id"
                class="shares-row"
                :class="{ 'is-expired': isExpired(share) }"
              >
                <td class="shares-file-cell">
                  <div class="shares-file">
                    <span class="shares-icon" aria-hidden="true">
                      <FileTypeIcon :file="iconSource(share)" :size="20" />
                    </span>
                    <div class="shares-file-text">
                      <div class="shares-file-titles">
                        <span class="shares-name" :title="share.entry_name">
                          {{ share.entry_name }}
                        </span>
                        <span class="shares-path" :title="share.entry_path">
                          /{{ share.entry_path }}
                        </span>
                      </div>
                      <div class="shares-link-row">
                        <input
                          class="input is-small shares-link-input"
                          type="text"
                          :value="shareUrl(share)"
                          readonly
                          :aria-label="`${share.entry_name} 的分享链接`"
                          @focus="selectLink"
                        />
                      </div>
                    </div>
                  </div>
                </td>
                <td class="shares-muted">{{ kindLabel(share) }}</td>
                <td>
                  <Badge
                    class="shares-badge"
                    :class="statusClass(share)"
                    :tone="isExpired(share) ? 'muted' : 'success'"
                    size="sm"
                  >
                    {{ statusLabel(share) }}
                  </Badge>
                </td>
                <td class="shares-muted shares-count">
                  访问 {{ share.access_count }} 次
                </td>
                <td class="shares-muted">
                  {{ formatRelativeDate(share.created_at) }}
                </td>
                <td class="shares-muted">{{ expiryLabel(share) }}</td>
                <td class="shares-actions">
                  <div class="shares-actions-inner">
                    <button
                      class="vf-ghost-button shares-action"
                      type="button"
                      @click="copyLink(share)"
                    >
                      <IconCheck v-if="copiedCode === share.code" :size="16" />
                      <IconCopy v-else :size="16" />
                      <span>{{
                        copiedCode === share.code ? "已复制" : "复制"
                      }}</span>
                    </button>
                    <button
                      class="vf-ghost-button shares-action"
                      type="button"
                      @click="openLink(share)"
                    >
                      <IconExternalLink :size="16" />
                      <span>打开</span>
                    </button>
                    <button
                      class="vf-ghost-button is-danger shares-action"
                      type="button"
                      :disabled="disablingCode === share.code"
                      @click="stopSharing(share)"
                    >
                      <IconTrash :size="16" />
                      <span>{{
                        disablingCode === share.code ? "停止中…" : "停止"
                      }}</span>
                    </button>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>

      <div v-if="hasMore" class="shares-load-more">
        <button
          class="vf-ghost-button"
          type="button"
          :disabled="loadingMore || loading"
          @click="loadMore"
        >
          <IconRefresh :size="16" :class="{ 'is-spinning': loadingMore }" />
          <span>{{
            loadingMore
              ? "加载中…"
              : `加载更多（${shares.length}/${shareTotal}）`
          }}</span>
        </button>
      </div>
      </section>
    </div>
  </SettingsLayout>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  IconAlertCircle,
  IconCheck,
  IconCircleCheck,
  IconClockOff,
  IconEye,
  IconFilter,
  IconCopy,
  IconExternalLink,
  IconFolderOpen,
  IconInfoCircle,
  IconLink,
  IconRefresh,
  IconTrash,
} from "@tabler/icons-vue";
import EmptyState from "../components/common/EmptyState.vue";
import SettingsLayout from "../layouts/SettingsLayout.vue";
import SkeletonList from "../components/common/SkeletonList.vue";
import FileTypeIcon from "../components/file-browser/FileTypeIcon.vue";
import Badge from "../ui/Badge.vue";
import { useMediaQuery } from "../composables/useMediaQuery";
import { filesService } from "../services/files.service";
import { copyText } from "../utils/clipboard";
import { formatRelativeDate } from "../utils/filePresentation";
import type { FileIconSource } from "../utils/filePresentation";
import { useAuthStore } from "../stores/auth.store";
import { confirmDialog } from "../composables/dialog";
import { useAppStore } from "../stores/app.store";
import type { ShareLink } from "../types";

/**
 * 「我的分享」：集中管理自己创建的分享链接。
 *
 * 数据来自 `GET /api/share/shares/page`（服务端已附带条目名称/路径/类型），
 * 支持复制、打开与停止分享。
 */
const auth = useAuthStore();
const app = useAppStore();
const router = useRouter();
const isCompact = useMediaQuery("(max-width: 768px)");

const shares = ref<ShareLink[]>([]);
const shareTotal = ref(0);
const hasMore = ref(false);
const loading = ref(false);
const loadingMore = ref(false);
const error = ref<string | null>(null);
const copiedCode = ref("");
const disablingCode = ref("");
const SHARE_PAGE_SIZE = 100;
let nextShareOffset = 0;

const featureDisabled = computed(
  () => auth.initialized && auth.features?.shareEnabled === false,
);

/** 未过期（或没有过期时间）的分享算有效。 */
function isExpired(share: ShareLink): boolean {
  if (!share.expires_at) return false;
  const expires = new Date(share.expires_at).getTime();
  return Number.isFinite(expires) && expires <= Date.now();
}

const activeShares = computed(() =>
  shares.value.filter((share) => !isExpired(share)),
);

const expiredShares = computed(() =>
  shares.value.filter((share) => isExpired(share)),
);

const visitedShares = computed(() =>
  shares.value.filter((share) => share.access_count > 0),
);

/** 列表筛选：全部 / 有效 / 已过期 / 已被访问。 */
type StatusFilter = "all" | "active" | "expired" | "visited";
const statusFilter = ref<StatusFilter>("all");

const filterChips = computed(() => [
  {
    key: "all" as const,
    label: "全部",
    icon: IconLink,
    count: shares.value.length,
  },
  {
    key: "active" as const,
    label: "有效",
    icon: IconCircleCheck,
    count: activeShares.value.length,
  },
  {
    key: "expired" as const,
    label: "已过期",
    icon: IconClockOff,
    count: expiredShares.value.length,
  },
  {
    key: "visited" as const,
    label: "已被访问",
    icon: IconEye,
    count: visitedShares.value.length,
  },
]);

const visibleShares = computed(() => {
  switch (statusFilter.value) {
    case "active":
      return activeShares.value;
    case "expired":
      return expiredShares.value;
    case "visited":
      return visitedShares.value;
    default:
      return shares.value;
  }
});

const emptyFilterTitle = computed(() => {
  switch (statusFilter.value) {
    case "active":
      return "没有有效的分享链接";
    case "expired":
      return "没有已过期的链接";
    case "visited":
      return "还没有被访问过的链接";
    default:
      return "没有匹配的分享链接";
  }
});

/** 批量停止所有已过期链接（仍会逐个调用接口，并显示进度）。 */
const clearingExpired = ref(false);
const clearedCount = ref(0);

async function clearExpired() {
  const targets = expiredShares.value;
  if (targets.length === 0 || clearingExpired.value) return;

  const ok = await confirmDialog({
    title: "清理过期链接",
    message: `确定要停止 ${targets.length} 个已过期的分享链接吗？\n（过期链接已无法访问，停止后从列表移除）`,
    confirmText: "全部停止",
    danger: true,
  });
  if (!ok) return;

  clearingExpired.value = true;
  clearedCount.value = 0;
  let failed = 0;

  for (const share of targets) {
    try {
      await filesService.disableShare(share.code);
      shares.value = shares.value.filter((item) => item.code !== share.code);
      clearedCount.value += 1;
    } catch {
      failed += 1;
    }
  }

  clearingExpired.value = false;
  if (clearedCount.value > 0) await reload();
  if (failed > 0) {
    app.error(`有 ${failed} 个链接未能停止，请重试`);
  } else {
    app.success(`已清理 ${clearedCount.value} 个过期链接`);
  }
}

function iconSource(share: ShareLink): FileIconSource {
  return { name: share.entry_name, kind: share.entry_kind };
}

function shareUrl(share: ShareLink): string {
  if (typeof window === "undefined") return `/s/${share.code}`;
  return new URL(`/s/${share.code}`, window.location.origin).toString();
}

function statusLabel(share: ShareLink): string {
  return isExpired(share) ? "已过期" : "有效";
}

function expiryLabel(share: ShareLink): string {
  if (!share.expires_at) return "长期有效";
  return formatRelativeDate(share.expires_at);
}

function kindLabel(share: ShareLink): string {
  return share.entry_kind === "directory" ? "文件夹" : "文件";
}

function statusClass(share: ShareLink): string {
  return isExpired(share) ? "is-expired" : "is-active";
}

function selectLink(event: FocusEvent) {
  (event.target as HTMLInputElement | null)?.select();
}

async function copyLink(share: ShareLink) {
  const ok = await copyText(shareUrl(share));
  if (!ok) {
    app.error("复制失败，请手动选择链接复制");
    return;
  }
  copiedCode.value = share.code;
  window.setTimeout(() => {
    if (copiedCode.value === share.code) copiedCode.value = "";
  }, 2000);
}

function openLink(share: ShareLink) {
  window.open(shareUrl(share), "_blank", "noopener,noreferrer");
}

async function stopSharing(share: ShareLink) {
  const ok = await confirmDialog({
    title: "停止分享",
    message: `确定要停止分享「${share.entry_name}」吗？\n（链接会立即失效）`,
    confirmText: "停止分享",
    danger: true,
  });
  if (!ok) return;

  disablingCode.value = share.code;
  try {
    await filesService.disableShare(share.code);
    await reload();
    app.success("已停止分享");
  } catch (e) {
    app.error(e instanceof Error ? e.message : "停止分享失败");
  } finally {
    disablingCode.value = "";
  }
}

function goFiles() {
  router.push({ path: "/" });
}

async function reload() {
  loading.value = true;
  error.value = null;
  shares.value = [];
  shareTotal.value = 0;
  hasMore.value = false;
  nextShareOffset = 0;
  try {
    const page = await filesService.listSharesPage({
      limit: SHARE_PAGE_SIZE,
      offset: 0,
    });
    shares.value = page.items;
    shareTotal.value = page.total;
    nextShareOffset = page.offset + page.items.length;
    hasMore.value = page.has_more;
  } catch (e) {
    shares.value = [];
    error.value = e instanceof Error ? e.message : "加载失败";
  } finally {
    loading.value = false;
  }
}

async function loadMore() {
  if (!hasMore.value || loadingMore.value || loading.value) return;

  loadingMore.value = true;
  try {
    const page = await filesService.listSharesPage({
      limit: SHARE_PAGE_SIZE,
      offset: nextShareOffset,
    });
    const knownIds = new Set(shares.value.map((share) => share.id));
    shares.value = [
      ...shares.value,
      ...page.items.filter((share) => !knownIds.has(share.id)),
    ];
    shareTotal.value = page.total;
    nextShareOffset = page.offset + page.items.length;
    hasMore.value = page.has_more;
  } catch (e) {
    app.error(e instanceof Error ? e.message : "加载更多分享链接失败");
  } finally {
    loadingMore.value = false;
  }
}

onMounted(() => {
  void reload();
});
</script>

<style scoped>
.settings-content > .settings-table-panel {
  padding: 0;
  overflow: hidden;
}

.settings-table-panel {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.shares-header-actions {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
}

.shares-note {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin: 0;
  padding: var(--vf-space-3) var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.settings-table-panel > .settings-table-state {
  padding: var(--vf-space-4);
}

/* 状态筛选 chips：全部 / 有效 / 已过期 / 已被访问 */
.shares-filters {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  flex-wrap: wrap;
  padding: 0.75rem 1rem 0.35rem;
}

.shares-page-hint {
  margin: 0;
  padding: 0 1rem 0.5rem;
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.shares-filter {
  gap: 0.3rem;
  min-height: 1.75rem;
  padding: 0 0.55rem;
  font-size: 0.8rem;
}

.shares-filter-count {
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.shares-filter.is-active .shares-filter-count {
  color: currentColor;
  opacity: 0.75;
}

.shares-table-wrap {
  width: 100%;
  overflow-x: auto;
}

.shares-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 0.875rem;
}

.shares-table th {
  height: var(--vf-table-head-height);
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
  color: var(--vf-text-muted);
  font-size: 0.8125rem;
  font-weight: 600;
  text-align: left;
  white-space: nowrap;
}

.shares-table td {
  height: var(--vf-row-height);
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  vertical-align: middle;
  overflow: hidden;
  text-overflow: ellipsis;
}

.shares-table th:first-child,
.shares-table td:first-child {
  padding-left: var(--vf-cell-padding-x);
}

.shares-table th:last-child,
.shares-table td:last-child {
  padding-right: var(--vf-cell-padding-x);
}

.shares-table tbody tr:last-child td {
  border-bottom: none;
}

.shares-table tbody tr:hover {
  background: var(--vf-row-hover);
}

.shares-col-file {
  width: 30%;
}

.shares-col-kind {
  width: 7%;
}

.shares-col-status {
  width: 8%;
}

.shares-col-visits {
  width: 8%;
}

.shares-col-created {
  width: 10%;
}

.shares-col-expiry {
  width: 9%;
}

.shares-col-actions {
  width: 28%;
  text-align: right;
}

.shares-row.is-expired .shares-name {
  color: var(--vf-text-muted);
}

.shares-row.is-expired .shares-icon {
  color: var(--vf-text-subtle);
}

.shares-file {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  min-width: 0;
}

.shares-file-text {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-width: 0;
}

.shares-file-titles {
  display: flex;
  align-items: baseline;
  gap: 0.4rem;
  min-width: 0;
}

.shares-file-titles .shares-name {
  flex: 0 1 auto;
  min-width: 0;
}

.shares-file-titles .shares-path {
  flex: 1 1 auto;
  min-width: 0;
}

.shares-icon {
  display: inline-flex;
  flex: 0 0 auto;
  color: var(--vf-accent-text);
}

.shares-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--vf-text);
}

.shares-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.shares-badge {
  flex: 0 0 auto;
}

.shares-muted {
  color: var(--vf-text-muted);
  white-space: nowrap;
}

.shares-count {
  font-variant-numeric: tabular-nums;
}

.shares-link-row {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  min-width: 0;
  margin-top: 0.2rem;
}

.shares-link-input {
  width: 100%;
  min-width: 0;
  height: 1.55rem;
  padding-top: 0;
  padding-bottom: 0;
  font-size: 0.72rem;
}

.shares-actions {
  text-align: right;
  white-space: nowrap;
}

.shares-actions-inner {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 0.15rem;
  flex-wrap: wrap;
}

.shares-action {
  min-height: 1.9rem;
  padding: 0 0.45rem;
  font-size: 0.78rem;
  white-space: nowrap;
}

.shares-load-more {
  display: flex;
  justify-content: center;
  padding: 0.7rem 1rem;
  border-top: 1px solid var(--vf-border-weak);
}

/* 窄屏：每行一张卡，表格列不横向溢出 */
.shares-cards {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  padding: 0.75rem;
}

.shares-card {
  padding: 0.75rem 0.8rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface);
}

.shares-card-head {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  min-width: 0;
}

.shares-card-titles {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-width: 0;
}

.shares-card-meta {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(7.5rem, 1fr));
  gap: 0.5rem 0.9rem;
  margin: 0.6rem 0 0.55rem;
}

.shares-card-meta > div {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  min-width: 0;
}

.shares-card-meta dt {
  color: var(--vf-text-subtle);
  font-size: 0.7rem;
}

.shares-card-meta dd {
  margin: 0;
  color: var(--vf-text);
  font-size: 0.8rem;
  word-break: break-word;
}

.shares-card .shares-link-row {
  margin: 0 0 0.55rem;
}

.shares-card-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
}

.is-spinning {
  animation: shares-spin 0.9s linear infinite;
}

@keyframes shares-spin {
  to {
    transform: rotate(360deg);
  }
}

/* reduced-motion：循环动画降级为静态指示（M3: static loading indicators），
   本文件动画族 shares-spin 0.9s */
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
