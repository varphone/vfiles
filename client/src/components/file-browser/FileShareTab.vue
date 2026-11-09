<template>
  <section class="details-tab-panel" aria-label="分享">
    <div class="details-share-head">
      <p class="details-tab-hint">
        通过链接把该条目分享给其他人；链接可在「我的分享」里统一管理。
      </p>
      <button
        v-if="canShare"
        class="vf-ghost-button is-primary details-share-create"
        type="button"
        @click="emit('create-share')"
      >
        <IconPlus :size="16" />
        <span>创建分享</span>
      </button>
      <p v-else class="details-share-note">当前会话没有分享权限。</p>
    </div>

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

    <EmptyState
      v-else-if="shares.length === 0"
      :icon="IconLink"
      compact
      title="还没有分享链接"
      :hint="
        canShare
          ? '点击「创建分享」生成一个公开链接'
          : '当前会话不能创建新的分享链接'
      "
    />

    <ul v-else class="details-share-list">
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

    <p v-if="truncated" class="details-share-note">
      分享链接较多，仅列出已加载的部分。
    </p>
  </section>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import {
  IconAlertCircle,
  IconLink,
  IconPlus,
  IconRefresh,
} from "@tabler/icons-vue";
import type { FileInfo, ShareLink } from "../../types";
import { filesService } from "../../services/files.service";
import { formatDate } from "../../utils/filePresentation";
import EmptyState from "../common/EmptyState.vue";
import SkeletonList from "../common/SkeletonList.vue";

/**
 * 详情侧栏「分享」Tab：列出该条目已生成的公开链接。
 *
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
  gap: 0.75rem;
}

.details-share-head {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
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
