<template>
  <div>
    <p class="heading">{{ formatDate(commit.date) }}</p>
    <div class="commit-message-row">
      <span
        v-if="messageMeta.tagLabel"
        class="tag is-light is-rounded commit-message-tag"
        :class="messageMeta.tagTone"
      >
        {{ messageMeta.tagLabel }}
      </span>
      <p class="title is-6 commit-message-title">{{ messageMeta.title }}</p>
    </div>
    <p v-if="messageMeta.detail" class="commit-message-detail">
      {{ messageMeta.detail }}
    </p>
    <p class="subtitle is-7 has-text-grey commit-author-row">
      <span class="commit-author-label">作者</span>
      <span>{{ commit.author.name }}</span>
      <span v-if="isLatest" class="tag is-primary is-light ml-2">最新版本</span>
    </p>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { CommitInfo } from "../../types";
import { describeCommitMessage } from "./commit-message";

const props = defineProps<{
  commit: CommitInfo;
  isLatest: boolean;
  formatDate: (date: string) => string;
}>();

const messageMeta = computed(() => describeCommitMessage(props.commit));
</script>

<style scoped>
.commit-message-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.commit-message-tag {
  flex-shrink: 0;
}

.commit-message-title {
  margin-bottom: 0;
  word-break: break-word;
}

.commit-message-detail {
  margin-top: 0.25rem;
  margin-bottom: 0.35rem;
  color: #6b7280;
  font-size: 0.82rem;
}

.commit-author-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex-wrap: wrap;
}

.commit-author-label {
  color: #9ca3af;
  font-size: 0.74rem;
  letter-spacing: 0.04em;
}
</style>
