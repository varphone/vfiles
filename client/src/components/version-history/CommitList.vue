<template>
  <Timeline>
    <CommitItem
      v-for="(commit, index) in commits"
      :key="commit.hash"
      :commit="commit"
      :index="index"
      :currentVersion="currentVersion"
      :restoringHash="restoringHash"
      :formatDate="formatDate"
      @view-version="(hash) => emit('view-version', hash)"
      @view-diff="(hash, parent) => emit('view-diff', hash, parent)"
      @restore-version="(hash) => emit('restore-version', hash)"
      @download-version="(hash) => emit('download-version', hash)"
    />
  </Timeline>
</template>

<script setup lang="ts">
import type { CommitInfo } from '../../types';
import Timeline from './Timeline.vue';
import CommitItem from './CommitItem.vue';

defineProps<{
  commits: CommitInfo[];
  currentVersion: string;
  restoringHash: string | null;
  formatDate: (date: string) => string;
}>();

const emit = defineEmits<{
  (e: 'view-version', hash: string): void;
  (e: 'view-diff', hash: string, parent?: string): void;
  (e: 'restore-version', hash: string): void;
  (e: 'download-version', hash: string): void;
}>();
</script>
