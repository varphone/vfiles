<template>
  <section class="details-tab-panel" aria-label="分享">
    <!-- 内部分享：现有分享列表与创建入口 -->
    <section class="details-share-block">
      <header class="details-share-block-head">
        <h3 class="details-share-block-title">内部分享</h3>
        <button
          class="details-share-info"
          type="button"
          title="与站内用户共享该条目"
          aria-label="内部分享说明"
        >
          <IconInfoCircle :size="16" aria-hidden="true" />
        </button>
      </header>
      <p class="details-tab-hint">
        通过链接把该条目分享给站内用户；链接可在「我的分享」里统一管理。
      </p>

      <button
        v-if="canShare"
        class="vf-ghost-button is-primary details-share-create"
        type="button"
        @click="emit('create-share')"
      >
        <IconShare :size="16" aria-hidden="true" />
        <span>创建分享</span>
      </button>
      <p v-else class="details-share-note">当前会话没有分享权限。</p>

      <SkeletonList
        v-if="loading"
        variant="lines"
        :rows="3"
        label="加载分享链接"
      />

      <EmptyState
        v-else-if="error"
        :icon="IconAlertCircle"
        tone="error"
        compact
        title="加载分享链接失败"
        :hint="error"
      >
        <template #actions>
          <button class="vf-ghost-button is-primary" @click="load">
            <IconRefresh :size="16" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <ul v-else-if="shares.length > 0" class="details-share-list">
        <li v-for="share in shares" :key="share.id" class="details-share-item">
          <span class="details-share-icon" aria-hidden="true">
            <IconLink :size="15" />
          </span>
          <div class="details-share-body">
            <p class="details-share-url" :title="shareUrl(share)">
              {{ shareUrl(share) }}
            </p>
            <p class="details-share-meta">
              <span>{{ expiryLabel(share) }}</span>
              <span aria-hidden="true">·</span>
              <span>访问 {{ share.access_count }} 次</span>
            </p>
          </div>
        </li>
      </ul>

      <EmptyState
        v-else
        :icon="IconLink"
        compact
        title="还没有分享链接"
        :hint="
          canShare
            ? '点击「创建分享」生成一个公开链接'
            : '当前会话不能创建新的分享链接'
        "
      />

      <p v-if="truncated" class="details-share-note">
        分享链接较多，仅列出已加载的部分。
      </p>
    </section>

    <!-- 外部分享：公开链接入口 -->
    <section class="details-share-block">
      <header class="details-share-block-head">
        <h3 class="details-share-block-title">外部分享</h3>
        <button
          class="details-share-info"
          type="button"
          title="通过公开链接分享该条目"
          aria-label="外部分享说明"
        >
          <IconInfoCircle :size="16" aria-hidden="true" />
        </button>
      </header>
      <p class="details-tab-hint">
        生成公开链接后，任何拿到链接的人都能访问该条目；链接同样在「我的分享」里管理。
      </p>
      <button
        v-if="canShare"
        class="vf-ghost-button details-share-create"
        type="button"
        @click="emit('create-share')"
      >
        <IconLink :size="16" aria-hidden="true" />
        <span>创建公开链接</span>
      </button>
      <p v-else class="details-share-note">当前会话不能创建公开链接。</p>
    </section>
  </section>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import {
  IconAlertCircle,
  IconInfoCircle,
  IconLink,
  IconRefresh,
  IconShare,
} from "@tabler/icons-vue";
import type { FileInfo, ShareLink } from "../../types";
import { filesService } from "../../services/files.service";
import { formatDate } from "../../utils/filePresentation";
import EmptyState from "../common/EmptyState.vue";
import SkeletonList from "../common/SkeletonList.vue";

/**
 * 详情侧栏「分享」Tab：按 Nextcloud 分成「内部分享 / 外部分享」两个区块。
 *
 * 内部分享沿用原有分享列表与创建入口；外部分享提供公开链接入口。
 * 现有分享列表接口按页返回全部条目，这里按 `entry_path` 过滤；
 * 最多翻 5 页（500 条），避免为单条详情把整个分享库都拉下来。
 */
const props = withDefaults(
  defineProps<{
    file: FileInfo;
    /** 受限会话隐藏「创建分享」。 */
    canShare?: boolean;
  }>(),
  { canShare: true },
);

const emit = defineEmits<{
  (e: "create-share"): void;
}>();

const PAGE_SIZE = 100;
const MAX_PAGES = 5;

const shares = ref<ShareLink[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const truncated = ref(false);

let requestId = 0;

function normalizePath(path: string | null | undefined): string {
  return String(path ?? "").replace(/^\/+/, "");
}

function shareUrl(share: ShareLink): string {
  if (typeof window === "undefined") return `/s/${share.code}`;
  return new URL(`/s/${share.code}`, window.location.origin).toString();
}

function expiryLabel(share: ShareLink): string {
  if (!share.expires_at) return "永久有效";
  const date = new Date(share.expires_at);
  if (Number.isNaN(date.getTime())) return "永久有效";
  if (date.getTime() <= Date.now()) return "已过期";
  return `${formatDate(share.expires_at)} 到期`;
}

async function load() {
  const id = ++requestId;
  loading.value = true;
  error.value = null;
  truncated.value = false;

  const target = normalizePath(props.file.path);
  const found: ShareLink[] = [];
  let offset = 0;
  let more = false;

  try {
    for (let page = 0; page < MAX_PAGES; page += 1) {
      const result = await filesService.listSharesPage({
        limit: PAGE_SIZE,
        offset,
      });
      if (id !== requestId) return;
      found.push(
        ...result.items.filter(
          (share) => normalizePath(share.entry_path) === target,
        ),
      );
      offset += result.items.length;
      more = result.has_more && result.items.length > 0;
      if (!more) break;
    }

    if (id !== requestId) return;
    truncated.value = more;
    shares.value = found.sort(
      (a, b) =>
        new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
    );
  } catch (err) {
    if (id !== requestId) return;
    error.value = err instanceof Error ? err.message : "加载失败";
  } finally {
    if (id === requestId) loading.value = false;
  }
}

watch(
  () => props.file.path,
  () => {
    shares.value = [];
    void load();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  requestId += 1;
});
</script>

<style scoped>
.details-tab-panel {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

/* 区块：标题 + 右侧说明按钮，区块之间用细分隔线 */
.details-share-block {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  min-width: 0;
}

.details-share-block + .details-share-block {
  padding-top: 16px;
  border-top: 1px solid var(--vf-border-weak);
}

.details-share-block-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.details-share-block-title {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 13px;
  font-weight: 600;
}

.details-share-info {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--vf-text-subtle);
  cursor: pointer;
}

.details-share-info:hover,
.details-share-info:focus-visible {
  color: var(--vf-text-strong);
}

.details-tab-hint {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  line-height: 1.5;
}

.details-share-create {
  align-self: flex-start;
}

.details-share-note {
  margin: 0;
  color: var(--vf-text-subtle);
  font-size: 0.78rem;
  line-height: 1.5;
}

.details-share-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin: 0;
  padding: 0;
  list-style: none;
}

.details-share-item {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  padding: 0.5rem 0.55rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: 3px;
  background: var(--vf-surface-sunken);
}

.details-share-icon {
  display: inline-flex;
  flex: 0 0 auto;
  color: var(--vf-accent-text);
}

.details-share-body {
  min-width: 0;
}

.details-share-url {
  margin: 0;
  color: var(--vf-text);
  font-size: 0.78rem;
  line-height: 1.4;
  overflow-wrap: anywhere;
}

.details-share-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.25rem;
  margin: 0.15rem 0 0;
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
}
</style>
