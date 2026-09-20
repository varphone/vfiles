<template>
  <div class="desktop-batch-strip">
    <div class="desktop-batch-meta">已选 {{ selectedCount }} 项</div>
    <div class="desktop-batch-actions">
      <button class="vf-ghost-button" @click="emit('select-all')">
        全选当前视图
      </button>
      <button class="vf-ghost-button" @click="emit('clear-selection')">
        清空选择
      </button>
      <button
        class="vf-ghost-button"
        :disabled="selectedCount === 0"
        @click="emit('download')"
      >
        下载
      </button>
      <button
        class="vf-ghost-button"
        :disabled="selectedCount === 0"
        @click="emit('move')"
      >
        移动
      </button>
      <button
        class="vf-ghost-button"
        :disabled="selectedCount !== 1"
        @click="emit('rename')"
      >
        重命名
      </button>
      <button
        class="vf-ghost-button is-danger"
        :disabled="selectedCount === 0"
        @click="emit('delete')"
      >
        删除
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 批量选择操作条。
 *
 * 只呈现操作入口，批量逻辑（选择集合、下载/删除/移动/重命名）仍在
 * `useFileSelection` 与父组件里，避免这里重复状态。
 */
defineProps<{ selectedCount: number }>();

const emit = defineEmits<{
  (e: "select-all"): void;
  (e: "clear-selection"): void;
  (e: "download"): void;
  (e: "move"): void;
  (e: "rename"): void;
  (e: "delete"): void;
}>();
</script>

<style scoped>
/* 批量操作条：贴住内容区顶部，横向铺满 */
.desktop-batch-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
  padding: 0.55rem 1.1rem;
  background: var(--vf-accent-soft);
  border-bottom: 1px solid var(--vf-border-weak);
}

.desktop-batch-meta {
  color: var(--vf-accent-text);
  font-weight: 700;
}

.desktop-batch-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 0.4rem;
}

.desktop-batch-actions .vf-ghost-button {
  min-height: 1.9rem;
}
</style>
