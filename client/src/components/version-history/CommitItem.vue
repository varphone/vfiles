<template>
  <div class="timeline-item">
    <div class="timeline-marker" :class="{ 'is-primary': index === 0 }"></div>
    <div class="timeline-content">
      <div class="box commit-item-box">
        <CommitDetails
          :commit="commit"
          :isLatest="index === 0"
          :formatDate="formatDate"
        />

        <div class="commit-footer">
          <div class="commit-hash-row">
            <span class="commit-hash-label">版本</span>
            <code class="is-size-7">{{ commit.hash.substring(0, 8) }}</code>
          </div>

          <div class="buttons are-small commit-actions">
            <button
              class="button is-info is-light"
              @click="emit('view-version', commit.hash)"
              title="查看此版本"
            >
              <IconEye :size="18" />
            </button>
            <button
              class="button is-link is-light"
              @click="emit('view-diff', commit.hash, commit.parent?.[0])"
              title="对比此版本（文本）"
            >
              <IconArrowsDiff :size="18" />
            </button>
            <button
              class="button is-warning is-light"
              @click="emit('restore-version', commit.hash)"
              :disabled="
                commit.hash === currentVersion ||
                restoringHash === commit.hash
              "
              title="恢复到此版本（会生成新提交）"
            >
              <IconRestore :size="18" />
            </button>
            <button
              class="button is-success is-light"
              @click="emit('download-version', commit.hash)"
              title="下载此版本"
            >
              <IconDownload :size="18" />
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  IconEye,
  IconDownload,
  IconRestore,
  IconArrowsDiff,
} from "@tabler/icons-vue";
import type { CommitInfo } from "../../types";
import CommitDetails from "./CommitDetails.vue";

defineProps<{
  commit: CommitInfo;
  index: number;
  currentVersion: string;
  restoringHash: string | null;
  formatDate: (date: string) => string;
}>();

const emit = defineEmits<{
  (e: "view-version", hash: string): void;
  (e: "view-diff", hash: string, parent?: string): void;
  (e: "restore-version", hash: string): void;
  (e: "download-version", hash: string): void;
}>();
</script>

<style scoped>
.commit-item-box {
  padding-bottom: 0.95rem;
}

.commit-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.9rem;
  margin-top: 0.85rem;
  padding-top: 0.75rem;
  border-top: 1px solid rgba(214, 223, 235, 0.9);
}

.commit-hash-row {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  min-width: 0;
  color: #5d6c7f;
}

.commit-hash-label {
  font-size: 0.75rem;
  color: #7a8899;
}

.commit-actions {
  margin-bottom: 0;
  flex-wrap: nowrap;
  justify-content: flex-end;
}

.commit-actions :deep(.button) {
  margin-bottom: 0;
}

@media screen and (max-width: 768px) {
  .commit-footer {
    gap: 0.65rem;
  }

  .commit-actions {
    gap: 0.35rem;
  }
}
</style>
