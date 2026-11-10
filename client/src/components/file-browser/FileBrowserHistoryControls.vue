<template>
  <div
    class="file-history"
    :class="{ 'is-error': error }"
    role="group"
    aria-label="版本切换"
  >
    <button
      class="vf-icon-button file-history-button"
      :disabled="loading"
      title="快退到最早快照"
      aria-label="快退到最早快照"
      @click="emit('fast-back')"
    >
      <IconChevronsLeft :size="16" />
    </button>
    <button
      class="vf-icon-button file-history-button"
      :disabled="loading"
      title="后退到更早快照"
      aria-label="后退到更早快照"
      @click="emit('step-back')"
    >
      <IconChevronLeft :size="16" />
    </button>
    <span class="file-history-label" :title="title">{{ stateLabel }}</span>
    <button
      class="vf-icon-button file-history-button"
      :disabled="loading || !hasCommit"
      title="前进到较新快照"
      aria-label="前进到较新快照"
      @click="emit('step-forward')"
    >
      <IconChevronRight :size="16" />
    </button>
    <button
      class="vf-icon-button file-history-button"
      :disabled="loading || !hasCommit"
      title="回到当前版本"
      aria-label="回到当前版本"
      @click="emit('fast-forward')"
    >
      <IconChevronsRight :size="16" />
    </button>
  </div>
</template>

<script setup lang="ts">
import {
  IconChevronLeft,
  IconChevronRight,
  IconChevronsLeft,
  IconChevronsRight,
} from "@tabler/icons-vue";

defineProps<{
  stateLabel: string;
  loading: boolean;
  error: boolean;
  title?: string;
  hasCommit: boolean;
}>();

const emit = defineEmits<{
  (e: "fast-back"): void;
  (e: "step-back"): void;
  (e: "step-forward"): void;
  (e: "fast-forward"): void;
}>();
</script>

<style scoped>
/* 版本切换：工具栏右组的紧凑按钮组（8px 外圆角，与视图切换组一致）。 */
.file-history {
  display: inline-flex;
  align-items: center;
  gap: 0;
  flex: 0 0 auto;
  padding: 0;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface-2);
  overflow: hidden;
}

.file-history .vf-icon-button {
  width: 30px;
  height: 34px;
  border-radius: 0;
}

.file-history .file-history-label {
  height: 34px;
  display: inline-flex;
  align-items: center;
}

.file-history.is-error {
  border-color: var(--vf-danger);
}

.file-history-label {
  min-width: 5.5rem;
  max-width: 9rem;
  padding: 0 var(--vf-space-2);
  text-align: center;
  font-size: var(--vf-text-sm);
  font-weight: 600;
  color: var(--vf-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 窄桌面（≤1280px）：版本胶囊先让位，保证搜索/视图/排序单行不被挤断。 */
@media screen and (max-width: 1280px) {
  .file-history {
    display: none;
  }
}
</style>
