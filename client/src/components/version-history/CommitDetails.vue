<template>
  <div>
    <p class="heading">{{ formatDate(commit.date) }}</p>
    <p class="title is-6">{{ commit.message }}</p>
    <p class="subtitle is-7 has-text-grey">
      {{ commit.author.name }}
      <span v-if="isLatest" class="tag is-primary is-light ml-2">最新版本</span>
    </p>
  </div>
</template>

<script setup lang="ts">
import type { CommitInfo } from "../../types";

defineProps<{
  commit: CommitInfo;
  isLatest: boolean;
  formatDate: (date: string) => string;
}>();
</script>
