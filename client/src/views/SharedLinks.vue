<template>
  <div class="shares-page">
    <div class="shares-card vf-page-card">
      <header class="shares-header">
        <div class="shares-header-titles">
          <h1 class="shares-title vf-page-title">我的分享</h1>
          <p class="shares-subtitle vf-page-subtitle">
            共 {{ shares.length }} 个链接
            <template v-if="activeShares.length !== shares.length">
              · 有效 {{ activeShares.length }} 个
            </template>
          </p>
        </div>

        <div class="shares-header-actions">
          <button
            v-if="expiredShares.length > 0"
            class="vf-ghost-button is-danger"
            type="button"
            :disabled="clearingExpired || loading"
            @click="clearExpired"
          >
            <IconTrash :size="15" />
            <span>
              {{
                clearingExpired
                  ? `清理中 ${clearedCount}/${expiredShares.length}`
                  : `清理过期链接（${expiredShares.length}）`
              }}
            </span>
          </button>
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
            <IconRefresh :size="15" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <SkeletonList
        v-else-if="loading && shares.length === 0"
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
            <IconFolderOpen :size="15" />
            <span>去选择文件</span>
          </button>
        </template>
      </EmptyState>

      <div v-if="shares.length > 0" class="shares-filters">
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

      <EmptyState
        v-if="shares.length > 0 && visibleShares.length === 0"
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

      <ul v-else-if="shares.length > 0" class="shares-list">
        <li
          v-for="share in visibleShares"
          :key="share.id"
          class="shares-row"
          :class="{ 'is-expired': isExpired(share) }"
        >
          <span class="shares-icon" aria-hidden="true">
            <FileTypeIcon :file="iconSource(share)" :size="20" />
          </span>

          <div class="shares-row-main">
            <div class="shares-row-titles">
              <span class="shares-name" :title="share.entry_name">
                {{ share.entry_name }}
              </span>
              <span class="shares-path" :title="share.entry_path">
                /{{ share.entry_path }}
              </span>
            </div>

            <div class="shares-row-meta">
              <span
                class="shares-badge vf-status-pill"
                :class="statusClass(share)"
              >
                {{ statusLabel(share) }}
              </span>
              <span>访问 {{ share.access_count }} 次</span>
              <span v-if="share.last_accessed_at">
                · 最近 {{ formatRelativeDate(share.last_accessed_at) }}
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

          <div class="shares-row-actions">
            <button
              class="vf-ghost-button shares-action"
              type="button"
              @click="copyLink(share)"
            >
              <IconCheck v-if="copiedCode === share.code" :size="15" />
              <IconCopy v-else :size="15" />
              <span>{{ copiedCode === share.code ? "已复制" : "复制" }}</span>
            </button>
            <button
              class="vf-ghost-button shares-action"
              type="button"
              @click="openLink(share)"
            >
              <IconExternalLink :size="15" />
              <span>打开</span>
            </button>
            <button
              class="vf-ghost-button is-danger shares-action"
              type="button"
              :disabled="disablingCode === share.code"
              @click="stopSharing(share)"
            >
              <IconTrash :size="15" />
              <span>{{
                disablingCode === share.code ? "停止中…" : "停止"
              }}</span>
            </button>
          </div>
        </li>
      </ul>
    </div>
  </div>
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
import SkeletonList from "../components/common/SkeletonList.vue";
import FileTypeIcon from "../components/file-browser/FileTypeIcon.vue";
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
 * 数据来自 `GET /api/share/shares`（服务端已附带条目名称/路径/类型），
 * 支持复制、打开与停止分享。
 */
const auth = useAuthStore();
const app = useAppStore();
const router = useRouter();

const shares = ref<ShareLink[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const copiedCode = ref("");
const disablingCode = ref("");

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
  if (isExpired(share)) return "已过期";
  if (!share.expires_at) return "长期有效";
  return `有效期至 ${formatRelativeDate(share.expires_at)}`;
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
    shares.value = shares.value.filter((item) => item.code !== share.code);
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
  try {
    shares.value = await filesService.listShares();
  } catch (e) {
    shares.value = [];
    error.value = e instanceof Error ? e.message : "加载失败";
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void reload();
});
</script>

<style scoped>
.shares-page {
  padding: 1.25rem 1rem 3rem;
}

.shares-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.8rem;
  flex-wrap: wrap;
  padding-bottom: 0.8rem;
  border-bottom: 1px solid var(--vf-border-weak);
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
  margin: 0.75rem 0 0;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  font-size: 0.78rem;
}

/* 状态筛选 chips：全部 / 有效 / 已过期 / 已被访问 */
.shares-filters {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  flex-wrap: wrap;
  padding: 0.65rem 0 0;
}

.shares-filter {
  gap: 0.3rem;
  min-height: 1.75rem;
  padding: 0 0.55rem;
  font-size: 0.78rem;
}

.shares-filter-count {
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
}

.shares-filter.is-active .shares-filter-count {
  color: currentColor;
  opacity: 0.75;
}

.shares-list {
  display: flex;
  flex-direction: column;
  margin: 0.6rem 0 0;
  padding: 0;
  list-style: none;
}

.shares-row {
  display: flex;
  align-items: flex-start;
  gap: 0.6rem;
  padding: 0.65rem 0.5rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.shares-row:last-child {
  border-bottom: none;
}

.shares-row:hover {
  background: var(--vf-surface-hover);
}

.shares-row.is-expired .shares-name {
  color: var(--vf-text-muted);
}

.shares-icon {
  display: inline-flex;
  flex: 0 0 auto;
  margin-top: 0.1rem;
  color: var(--vf-accent-text);
}

.shares-row.is-expired .shares-icon {
  color: var(--vf-text-subtle);
}

.shares-row-main {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  flex: 1 1 auto;
  min-width: 0;
}

.shares-row-titles {
  display: flex;
  align-items: baseline;
  gap: 0.4rem;
  min-width: 0;
}

.shares-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.86rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.shares-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--vf-text-subtle);
  font-size: 0.74rem;
}

.shares-row-meta {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
  color: var(--vf-text-muted);
  font-size: 0.75rem;
}

.shares-badge.is-active {
  background: var(--vf-success-soft);
  color: var(--vf-success-text);
}

.shares-badge.is-expired {
  background: var(--vf-surface-sunken);
  color: var(--vf-text-subtle);
}

.shares-link-row {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  max-width: 30rem;
}

.shares-link-input {
  font-size: 0.78rem;
}

.shares-row-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex: 0 0 auto;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.shares-action {
  min-height: 1.8rem;
  padding: 0 0.5rem;
  font-size: 0.78rem;
  white-space: nowrap;
}

.is-spinning {
  animation: shares-spin 0.9s linear infinite;
}

@keyframes shares-spin {
  to {
    transform: rotate(360deg);
  }
}

@media screen and (max-width: 760px) {
  .shares-page {
    padding: 0.75rem 0.5rem 2rem;
  }

  .shares-row {
    flex-wrap: wrap;
  }

  .shares-row-actions {
    width: 100%;
    justify-content: flex-start;
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
