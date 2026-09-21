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
/* 批量操作条：独立圆角面板，与上方工具栏和下方列表都留出间距，
   避免贴在一起像是工具栏没画完 */
.desktop-batch-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
  /* 位于列表列内部：与列表内容左右对齐，并在列表列内吸顶 */
  margin: 0.4rem 0 0.6rem;
  padding: 0.5rem 0.75rem;
  /* 吸顶：长列表滚动时批量操作始终可达（对齐主流网盘的选择工具条）。
     用「实底 + 叠加强调色」而不是半透明色，避免滚动时透出下层文字。 */
  position: sticky;
  top: calc(
    var(--bulma-navbar-height, 3.25rem) + env(safe-area-inset-top) + 0.4rem
  );
  z-index: 6;
  background-color: var(--vf-surface);
  background-image: linear-gradient(
    var(--vf-accent-soft),
    var(--vf-accent-soft)
  );
  border: 1px solid var(--vf-accent-soft-strong);
  border-radius: var(--vf-radius);
  box-shadow: var(--vf-shadow-menu);
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
