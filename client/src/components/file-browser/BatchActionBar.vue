<template>
  <div class="desktop-batch-strip" role="region" aria-label="批量操作">
    <div class="desktop-batch-meta">已选择 {{ selectedCount }} 项</div>
    <div class="desktop-batch-actions">
      <button
        class="vf-ghost-button is-small"
        type="button"
        @click="emit('select-all')"
      >
        <IconChecklist :size="16" aria-hidden="true" />
        <span>全选当前视图</span>
      </button>
      <button
        class="vf-ghost-button is-small"
        type="button"
        :disabled="selectedCount === 0"
        @click="emit('download')"
      >
        <IconDownload :size="16" aria-hidden="true" />
        <span>下载</span>
      </button>
      <button
        v-if="canWrite"
        class="vf-ghost-button is-small"
        type="button"
        :disabled="selectedCount === 0"
        @click="emit('move')"
      >
        <IconArrowsDiff :size="16" aria-hidden="true" />
        <span>移动</span>
      </button>
      <button
        v-if="canTransfer"
        class="vf-ghost-button is-small"
        type="button"
        :disabled="selectedCount === 0"
        title="把所选条目（含版本历史）交给另一个用户"
        @click="emit('transfer')"
      >
        <IconUserShare :size="16" aria-hidden="true" />
        <span>转移所有权</span>
      </button>
      <button
        v-if="canWrite"
        class="vf-ghost-button is-small"
        type="button"
        :disabled="selectedCount !== 1"
        @click="emit('rename')"
      >
        <IconPencil :size="16" aria-hidden="true" />
        <span>重命名</span>
      </button>
      <button
        v-if="canWrite"
        class="vf-ghost-button is-small is-danger"
        type="button"
        :disabled="selectedCount === 0"
        @click="emit('delete')"
      >
        <IconTrash :size="16" aria-hidden="true" />
        <span>删除</span>
      </button>
      <button
        class="vf-ghost-button is-small desktop-batch-clear"
        type="button"
        @click="emit('clear-selection')"
      >
        <IconX :size="16" aria-hidden="true" />
        <span>清空</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  IconArrowsDiff,
  IconChecklist,
  IconDownload,
  IconPencil,
  IconTrash,
  IconUserShare,
  IconX,
} from "@tabler/icons-vue";

/**
 * 批量选择操作条。
 *
 * 只呈现操作入口，批量逻辑（选择集合、下载/删除/移动/重命名）仍在
 * `useFileSelection` 与父组件里，避免这里重复状态。
 * 样式对齐 Nextcloud：浅蓝底、44px 高、左侧选择计数 + 右侧小号文字图标按钮。
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
/* 批量操作条：Nextcloud 式浅蓝选择条（满宽、无圆角、底部一条分隔线） */
.desktop-batch-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  height: 44px;
  /* 位于列表列内部：与列表内容左右对齐，并在列表列内吸顶。
     top 取 0：选择条本来就贴在列表列顶部；带上顶栏高度会把条向下推、
     压住表头/首行（实测 sticky 约束按最近的滚动容器计算）。 */
  margin: 0;
  padding: 0 16px;
  position: sticky;
  top: 0;
  z-index: 6;
  background: var(--vf-row-selected);
  border: none;
  border-bottom: 1px solid var(--vf-border-weak);
  border-radius: 0;
  box-shadow: none;
}

.desktop-batch-meta {
  flex: 0 0 auto;
  color: var(--vf-text);
  font-size: var(--vf-text-base);
  font-weight: 600;
  white-space: nowrap;
}

.desktop-batch-actions {
  display: flex;
  flex: 0 1 auto;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}

.desktop-batch-actions::-webkit-scrollbar {
  display: none;
}

.desktop-batch-actions .vf-ghost-button {
  flex: 0 0 auto;
}

.desktop-batch-clear {
  color: var(--vf-text-muted);
}
</style>
