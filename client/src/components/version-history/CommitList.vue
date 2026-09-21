<template>
  <ol class="history-list">
    <CommitItem
      v-for="(commit, index) in commits"
      :key="commit.hash"
      :commit="commit"
      :index="index"
      :total-commits="totalCommits"
      :current-version="currentVersion"
      :restoring-hash="restoringHash"
      :format-date="formatDate"
      :selected="commit.hash === selectedHash"
      @view-version="(hash) => emit('view-version', hash)"
      @view-diff="(hash, parent) => emit('view-diff', hash, parent)"
      @restore-version="(hash) => emit('restore-version', hash)"
      @download-version="(hash) => emit('download-version', hash)"
    />
  </ol>
</template>

<script setup lang="ts">
import type { CommitInfo } from "../../types";
import CommitItem from "./CommitItem.vue";

withDefaults(
  defineProps<{
    commits: CommitInfo[];
    /** 该文件的总版本数（用于「第 N 版」）。 */
    totalCommits: number;
    currentVersion: string;
    restoringHash: string | null;
    formatDate: (date: string) => string;
    /** 详情面板当前展示的版本，用于高亮。 */
    selectedHash?: string;
  }>(),
  { selectedHash: "" },
);

const emit = defineEmits<{
  (e: "view-version", hash: string): void;
  (e: "view-diff", hash: string, parent?: string): void;
  (e: "restore-version", hash: string): void;
  (e: "download-version", hash: string): void;
}>();
</script>

<style scoped>
.history-list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
}
</style>
