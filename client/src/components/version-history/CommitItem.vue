<template>
  <li
    class="history-row"
    :class="{ 'is-current': isCurrent, 'is-selected': selected }"
  >
    <span class="history-rail" aria-hidden="true">
      <span class="history-dot"></span>
    </span>

    <div class="history-card">
      <div class="history-head">
        <span class="history-version">{{ versionLabel }}</span>
        <span v-if="messageMeta.tagLabel" class="history-tag">
          {{ messageMeta.tagLabel }}
        </span>
        <p class="history-title" :title="messageMeta.title">
          {{ messageMeta.title }}
        </p>
      </div>

      <p v-if="messageMeta.detail" class="history-detail">
        {{ messageMeta.detail }}
      </p>

      <p class="history-meta">
        <span class="history-meta-item" :title="absoluteDate">
          {{ relativeDate }}
        </span>
        <span class="history-meta-sep" aria-hidden="true">·</span>
        <span class="history-meta-item">{{
          commit.author.name || "未知"
        }}</span>
        <span class="history-meta-sep" aria-hidden="true">·</span>
        <code class="history-hash" :title="commit.hash">{{ shortHash }}</code>
      </p>

      <div class="history-actions">
        <button
          class="vf-ghost-button history-action"
          type="button"
          @click="emit('view-version', commit.hash)"
        >
          <IconEye :size="15" />
          <span>预览</span>
        </button>
        <button
          class="vf-ghost-button history-action"
          type="button"
          @click="emit('view-diff', commit.hash, commit.parent?.[0])"
        >
          <IconArrowsDiff :size="15" />
          <span>对比</span>
        </button>
        <button
          class="vf-ghost-button history-action"
          type="button"
          @click="emit('download-version', commit.hash)"
        >
          <IconDownload :size="15" />
          <span>下载</span>
        </button>
        <button
          v-if="!isCurrent"
          class="vf-ghost-button history-action is-accent"
          type="button"
          :disabled="restoringHash === commit.hash"
          @click="emit('restore-version', commit.hash)"
        >
          <IconRestore :size="15" />
          <span>{{ restoringHash === commit.hash ? "恢复中…" : "恢复" }}</span>
        </button>
        <span v-else class="history-current-hint">这是当前版本</span>
      </div>
    </div>
  </li>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  IconArrowsDiff,
  IconDownload,
  IconEye,
  IconRestore,
} from "@tabler/icons-vue";
import type { CommitInfo } from "../../types";
import { formatRelativeDate } from "../../utils/filePresentation";
import { describeCommitMessage } from "./commit-message";

const props = defineProps<{
  commit: CommitInfo;
  /** 在列表中的位置（0 = 最新）。 */
  index: number;
  /** 该文件的总版本数，用于显示「第 N 版」。 */
  totalCommits: number;
  currentVersion: string;
  restoringHash: string | null;
  /** 绝对时间格式化（悬停提示用）。 */
  formatDate: (date: string) => string;
  selected?: boolean;
}>();

const emit = defineEmits<{
  (e: "view-version", hash: string): void;
  (e: "view-diff", hash: string, parent?: string): void;
  (e: "restore-version", hash: string): void;
  (e: "download-version", hash: string): void;
}>();

const isCurrent = computed(() => props.commit.hash === props.currentVersion);
const messageMeta = computed(() => describeCommitMessage(props.commit));
const shortHash = computed(() => props.commit.hash.substring(0, 8));
const absoluteDate = computed(() => props.formatDate(props.commit.date));
const relativeDate = computed(() => formatRelativeDate(props.commit.date));
const versionLabel = computed(() => {
  if (isCurrent.value) return "当前版本";
  // 列表按时间倒序，最新一条是「第 totalCommits 版」
  const number = Math.max(1, props.totalCommits - props.index);
  return `第 ${number} 版`;
});
</script>

<style scoped>
.history-row {
  display: grid;
  grid-template-columns: 1.25rem minmax(0, 1fr);
  gap: 0.35rem;
}

/* 时间线轨道：贯穿整列，最后一行自然收尾 */
.history-rail {
  position: relative;
  display: flex;
  justify-content: center;
}

.history-rail::before {
  content: "";
  position: absolute;
  top: 0;
  bottom: -0.75rem;
  width: 2px;
  background: var(--vf-border-weak);
}

.history-row:last-child .history-rail::before {
  bottom: 0;
}

.history-dot {
  position: relative;
  width: 0.6rem;
  height: 0.6rem;
  margin-top: 0.85rem;
  border: 2px solid var(--vf-border);
  border-radius: 50%;
  background: var(--vf-surface);
}

.history-row.is-current .history-dot {
  border-color: var(--vf-accent);
  background: var(--vf-accent);
  box-shadow: 0 0 0 3px var(--vf-accent-soft);
}

.history-card {
  min-width: 0;
  margin-bottom: 0.5rem;
  padding: 0.6rem 0.7rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface);
  transition:
    border-color 0.15s var(--vf-motion-standard),
    background 0.15s var(--vf-motion-standard);
}

.history-row:hover .history-card,
.history-row.is-selected .history-card {
  border-color: var(--vf-border);
  background: var(--vf-surface-hover);
}

.history-row.is-current .history-card {
  border-color: var(--vf-accent-soft-strong);
  background: var(--vf-accent-soft);
}

.history-head {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex-wrap: wrap;
}

.history-version {
  flex: 0 0 auto;
  padding: 0.05rem 0.4rem;
  border-radius: 999px;
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  font-size: 0.72rem;
  font-weight: 600;
}

.history-row.is-current .history-version {
  background: var(--vf-accent);
  color: var(--vf-on-accent);
}

.history-tag {
  flex: 0 0 auto;
  padding: 0.05rem 0.35rem;
  border-radius: 4px;
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  font-size: 0.7rem;
}

.history-title {
  flex: 1 1 auto;
  min-width: 0;
  margin: 0;
  color: var(--vf-text-strong);
  font-size: 0.86rem;
  font-weight: 600;
  line-height: 1.3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.history-detail {
  margin: 0.25rem 0 0;
  color: var(--vf-text-muted);
  font-size: 0.78rem;
  line-height: 1.4;
}

.history-meta {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
  margin: 0.3rem 0 0;
  color: var(--vf-text-subtle);
  font-size: 0.74rem;
}

.history-meta-sep {
  color: var(--vf-border);
}

.history-hash {
  font-size: 0.72rem;
  color: var(--vf-text-subtle);
}

.history-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-wrap: wrap;
  margin-top: 0.45rem;
}

.history-action {
  gap: 0.25rem;
  min-height: 1.75rem;
  padding: 0 0.45rem;
  font-size: 0.76rem;
}

.history-action.is-accent {
  color: var(--vf-accent-text);
  background: var(--vf-accent-soft);
}

.history-action.is-accent:hover:not(:disabled) {
  background: var(--vf-accent-soft-strong);
  color: var(--vf-accent-text);
}

.history-current-hint {
  color: var(--vf-success-text);
  font-size: 0.74rem;
}
</style>
