<template>
  <section class="details-tab-panel" aria-label="活动">
    <p class="details-tab-hint">该条目的版本与提交记录。</p>

    <SkeletonList
      v-if="loading && commits.length === 0"
      variant="lines"
      :rows="4"
      label="加载活动"
    />

    <EmptyState
      v-else-if="error"
      :icon="IconAlertCircle"
      tone="error"
      compact
      title="加载活动失败"
      :hint="error"
    >
      <template #actions>
        <button class="vf-ghost-button is-primary" @click="load()">
          <IconRefresh :size="16" />
          <span>重试</span>
        </button>
      </template>
    </EmptyState>

    <EmptyState
      v-else-if="commits.length === 0"
      :icon="IconHistory"
      compact
      title="暂无活动"
      hint="文件更新后，提交记录会显示在这里"
    />

    <template v-else>
      <ol class="details-activity-list">
        <li
          v-for="commit in commits"
          :key="commit.hash"
          class="details-activity-item"
        >
          <span class="details-activity-dot" aria-hidden="true"></span>
          <div class="details-activity-body">
            <p class="details-activity-message">
              {{ commitText(commit) }}
            </p>
            <p class="details-activity-meta">
              <code class="details-activity-hash" :title="commit.hash">
                {{ shortHash(commit.hash) }}
              </code>
              <span aria-hidden="true">·</span>
              <span :title="formatDate(commit.date)">{{
                formatRelativeDate(commit.date)
              }}</span>
              <template v-if="commit.author?.name">
                <span aria-hidden="true">·</span>
                <span>{{ commit.author.name }}</span>
              </template>
            </p>
          </div>
        </li>
      </ol>

      <button
        v-if="nextCursor"
        class="vf-ghost-button details-activity-more"
        type="button"
        :disabled="loading"
        @click="load(nextCursor)"
      >
        <span>{{ loading ? "加载中..." : "加载更多" }}</span>
      </button>
    </template>
  </section>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { IconAlertCircle, IconHistory, IconRefresh } from "@tabler/icons-vue";
import type { CommitInfo, FileInfo } from "../../types";
import { filesService } from "../../services/files.service";
import { formatDate, formatRelativeDate } from "../../utils/filePresentation";
import EmptyState from "../common/EmptyState.vue";
import SkeletonList from "../common/SkeletonList.vue";

/**
 * 详情侧栏「活动」Tab：把文件的版本历史作为活动流展示。
 *
 * 只取第一页（`withTotal: false`），避免为了侧栏再统计一次整段历史；
 * 需要更多时用游标继续加载，交互与版本对话框一致。
 */
const props = defineProps<{ file: FileInfo }>();

const PAGE_SIZE = 20;
const commits = ref<CommitInfo[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const nextCursor = ref<string | null>(null);

let requestId = 0;
let controller: AbortController | null = null;

function shortHash(hash: string): string {
  return (hash || "").substring(0, 8) || "--------";
}

function commitText(commit: CommitInfo): string {
  return commit.message?.trim() || "更新文件";
}

async function load(cursor?: string) {
  controller?.abort();
  const current = new AbortController();
  controller = current;
  const id = ++requestId;

  if (!cursor) commits.value = [];
  loading.value = true;
  error.value = null;

  try {
    const data = await filesService.getFileHistory(
      props.file.path,
      PAGE_SIZE,
      cursor,
      { withTotal: false, signal: current.signal },
    );
    if (id !== requestId) return;
    commits.value = cursor ? [...commits.value, ...data.commits] : data.commits;
    nextCursor.value = data.nextCursor ?? null;
  } catch (err) {
    if (id !== requestId) return;
    if (err instanceof Error && err.name === "AbortError") return;
    error.value = err instanceof Error ? err.message : "加载失败";
  } finally {
    if (id === requestId) loading.value = false;
  }
}

watch(
  () => props.file.path,
  () => {
    nextCursor.value = null;
    void load();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  requestId += 1;
  controller?.abort();
});
</script>

<style scoped>
.details-tab-panel {
  display: flex;
  flex-direction: column;
  gap: var(--vf-section-gap);
}

.details-tab-hint {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 0.8125rem;
  line-height: 1.5;
}

.details-activity-list {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.details-activity-item {
  display: flex;
  align-items: flex-start;
  gap: var(--vf-space-2);
  min-width: 0;
}

.details-activity-dot {
  flex: 0 0 auto;
  width: 7px;
  height: 7px;
  margin-top: var(--vf-space-1);
  border: 2px solid var(--vf-accent-soft-strong);
  border-radius: 50%;
  background: var(--vf-accent);
}

.details-activity-body {
  min-width: 0;
}

.details-activity-message {
  margin: 0;
  color: var(--vf-text);
  font-size: 0.84rem;
  line-height: 1.45;
  word-break: break-word;
}

.details-activity-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.25rem;
  margin: 0.15rem 0 0;
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.details-activity-hash {
  padding: 0 0.25rem;
  border-radius: 3px;
  background: var(--vf-surface-2);
  color: var(--vf-text-muted);
  font-size: 0.72rem;
}

.details-activity-more {
  align-self: flex-start;
}
</style>
