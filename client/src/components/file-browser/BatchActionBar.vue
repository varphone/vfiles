<template>
  <div class="desktop-batch-strip" role="region" aria-label="批量操作">
    <div class="desktop-batch-meta">已选择 {{ selectedCount }} 项</div>
    <div class="desktop-batch-actions">
      <button class="vf-ghost-button" @click="emit('select-all')">
        全选当前视图
      </button>
      <button
        class="vf-ghost-button"
        :disabled="selectedCount === 0"
        @click="emit('download')"
      >
        下载
      </button>
      <button
        v-if="canWrite"
        class="vf-ghost-button"
        :disabled="selectedCount === 0"
        @click="emit('move')"
      >
        移动
      </button>
      <button
        v-if="canTransfer"
        class="vf-ghost-button"
        :disabled="selectedCount === 0"
        title="把所选条目（含版本历史）交给另一个用户"
        @click="emit('transfer')"
      >
        转移所有权
      </button>
      <button
        v-if="canWrite"
        class="vf-ghost-button"
        :disabled="selectedCount !== 1"
        @click="emit('rename')"
      >
        重命名
      </button>
      <button
        v-if="canWrite"
        class="vf-ghost-button is-danger"
        :disabled="selectedCount === 0"
        @click="emit('delete')"
      >
        删除
      </button>
      <button
        class="vf-ghost-button desktop-batch-clear"
        @click="emit('clear-selection')"
      >
        清空
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
withDefaults(
  defineProps<{
    selectedCount: number;
    /** 只读会话隐藏移动/重命名/删除。 */
    canWrite?: boolean;
    /** 受限会话隐藏所有权转移。 */
    canTransfer?: boolean;
  }>(),
  { canWrite: true, canTransfer: true },
);

const emit = defineEmits<{
  (e: "select-all"): void;
  (e: "clear-selection"): void;
  (e: "download"): void;
  (e: "move"): void;
  (e: "transfer"): void;
  (e: "rename"): void;
  (e: "delete"): void;
}>();
</script>

<style scoped>
/* 批量操作条：Nextcloud 式中性浮条——白底、细边框、3px 圆角。
   与上方工具栏和下方列表留出间距，避免贴在一起像是工具栏没画完 */
.desktop-batch-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  flex-wrap: wrap;
  /* 位于列表列内部：与列表内容左右对齐，并在列表列内吸顶 */
  margin: 0.4rem 0 0.6rem;
  padding: 0.45rem 0.75rem;
  /* 吸顶：长列表滚动时批量操作始终可达（对齐主流网盘的选择工具条）。
     用实底而不是半透明色，避免滚动时透出下层文字。 */
  position: sticky;
  top: calc(
    var(--bulma-navbar-height, 3.25rem) + env(safe-area-inset-top) + 0.4rem
  );
  z-index: 6;
  background: var(--vf-surface);
  border: 1px solid var(--vf-border-weak);
  border-radius: 3px;
  box-shadow: var(--vf-shadow-popover);
}

.desktop-batch-meta {
  color: var(--vf-text-strong);
  font-size: 0.875rem;
  font-weight: 600;
}

.desktop-batch-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 0.25rem;
}

.desktop-batch-actions .vf-ghost-button {
  min-height: var(--vf-control-height-sm);
  padding: 0 0.6rem;
  font-size: 0.8125rem;
}

.desktop-batch-clear {
  color: var(--vf-text-muted);
}
</style>
